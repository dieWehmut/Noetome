"""Bounded stdlib Responses transport; stdin credentials, deterministic offline decode."""
import hashlib
import http.client
import json
import math
import os
import re
import socket
import ssl
import struct
import sys
import time
import unicodedata
import uuid
from pathlib import Path

ENDPOINT = "https://ai.1343263.xyz/v1/responses"
PROVIDER = {"producer": "model", "provider_id": "ai-1343263-responses",
            "adapter_version": "responses-stdlib-v1", "requested_model": "deepseek-flash"}
PROTOCOL = "noetome-model-single-v1"
LIMITS = {"max_input_bytes": 8192, "max_response_bytes": 524288,
          "max_output_tokens": 1024, "timeout_ms": 30000}
U64_MAX = (1 << 64) - 1
FRAME_MAGIC = b"NOETOME-CREDENTIAL-STDIN\0"
FRAME_MAX = len(FRAME_MAGIC) + 12 + 4096 + 65536
JSON_ESCAPE_FRAGMENT = re.compile(r'\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4})')
SURROGATE_PAIR = re.compile(r'[\ud800-\udbff][\udc00-\udfff]')
ESCAPE_SCAN_DEPTH = 8


class Invalid(ValueError):
    """Only fixed local codes; never propagate supplied text or a transport exception."""


def fail(code="RESPONSES_INVALID"):
    raise Invalid(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def named(value, required, optional=()):
    if type(value) is not dict or not set(required) <= value.keys() or value.keys() - set(required) - set(optional):
        fail()
    return value


def integer(value, minimum=0, maximum=U64_MAX):
    if type(value) is not int or not minimum <= value <= maximum:
        fail()
    return value


def string(value, maximum, nonempty=True, identity=False):
    if type(value) is not str:
        fail()
    try:
        length = len(value.encode("utf-8"))
    except UnicodeError:
        fail()
    if length > maximum or "\0" in value or (nonempty and not value.strip()):
        fail()
    if identity and any(unicodedata.category(char) == "Cc" for char in value):
        fail()
    return value


def valid_uuid(value):
    string(value, 36, identity=True)
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        fail()
    if parsed.version != 4 or parsed.variant != uuid.RFC_4122 or str(parsed) != value:
        fail()


def valid_hash(value):
    if type(value) is not str or len(value) != 64 or any(char not in "0123456789abcdef" for char in value):
        fail()


def check_strings(value, credential=b""):
    if type(value) is str:
        string(value, 524288, nonempty=False)
        if credential:
            scan_escaped_string(value, credential.decode("utf-8"))
    elif type(value) is dict:
        for key, child in value.items():
            check_strings(key, credential)
            check_strings(child, credential)
    elif type(value) is list:
        for child in value:
            check_strings(child, credential)
    elif type(value) is tuple:
        for child in value:
            check_strings(child, credential)


def scan_escaped_string(value, secret):
    """Scan every string, including scalars, prefixes and incomplete JSON tokens.

    Decode only valid JSON escape fragments; unrelated natural backslashes remain
    ordinary text. Each pass shrinks the bounded string. An unresolved eighth
    layer is rejected instead of allowing a deeper representation to be saved.
    """
    current = value
    for unused in range(ESCAPE_SCAN_DEPTH):
        if secret in current:
            fail("RESPONSES_CREDENTIAL_ECHO")
        decoded = JSON_ESCAPE_FRAGMENT.sub(lambda match: json.loads('"' + match.group() + '"'), current)
        decoded = SURROGATE_PAIR.sub(
            lambda match: chr(0x10000 + ((ord(match.group()[0]) - 0xD800) << 10) + ord(match.group()[1]) - 0xDC00),
            decoded,
        )
        if decoded == current:
            return
        current = decoded
    if secret in current:
        fail("RESPONSES_CREDENTIAL_ECHO")
    if JSON_ESCAPE_FRAGMENT.search(current):
        fail("RESPONSES_CREDENTIAL_SCAN_DEPTH")


def strict_json(raw, maximum, credential=b""):
    if type(raw) is not bytes or not 0 < len(raw) <= maximum:
        fail()
    if credential and credential in raw:
        fail("RESPONSES_CREDENTIAL_ECHO")

    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                fail()
            result[key] = value
        return result

    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=pairs,
                           parse_float=finite_float, parse_constant=lambda unused: fail())
    except (UnicodeError, json.JSONDecodeError, RecursionError):
        fail()
    try:
        check_strings(value, credential)
    except RecursionError:
        fail()
    return value


def finite_float(token):
    value = float(token)
    if not math.isfinite(value):
        fail()
    return value


def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")


def validate_request_wire(wire, credential=b""):
    request = named(strict_json(wire, 65536, credential),
                    ("schema_version", "protocol", "request_id", "provider", "source", "policy"))
    integer(request["schema_version"], 1, 1)
    if request["protocol"] != PROTOCOL:
        fail()
    valid_uuid(request["request_id"])
    named(request["provider"], PROVIDER)
    if request["provider"] != PROVIDER:
        fail()
    policy = named(request["policy"], tuple(LIMITS) + ("max_calls", "retries", "search_enabled", "tools_enabled"))
    for field, maximum in LIMITS.items():
        integer(policy[field], 1, maximum)
    integer(policy["max_calls"], 1, 1)
    integer(policy["retries"], 0, 0)
    if policy["search_enabled"] is not False or policy["tools_enabled"] is not False:
        fail()
    source = named(request["source"], ("project_id", "source_id", "version_id", "revision", "raw_sha256",
                                       "normalized_sha256", "normalization_version", "parser_version", "units"))
    for field in ("project_id", "source_id", "version_id"):
        valid_uuid(source[field])
    integer(source["revision"], 1)
    valid_hash(source["raw_sha256"])
    valid_hash(source["normalized_sha256"])
    if source["normalization_version"] != "utf8-newlines-v1" or source["parser_version"] != "blocks-v1":
        fail()
    units = source["units"]
    if type(units) is not list or not units:
        fail()
    end, spans, pieces = 0, set(), []
    for index, unit in enumerate(units):
        named(unit, ("kind", "span_id", "ordinal", "start_byte", "end_byte", "text_sha256", "text"))
        if unit["kind"] != "text_span":
            fail()
        valid_uuid(unit["span_id"])
        if unit["span_id"] in spans:
            fail()
        spans.add(unit["span_id"])
        integer(unit["ordinal"], index, index)
        integer(unit["start_byte"], end, end)
        text = string(unit["text"], policy["max_input_bytes"], nonempty=False).encode("utf-8")
        if not text:
            fail()
        end += len(text)
        integer(unit["end_byte"], end, end)
        valid_hash(unit["text_sha256"])
        if unit["text_sha256"] != sha(text) or end > policy["max_input_bytes"]:
            fail()
        pieces.append(text)
    if source["normalized_sha256"] != sha(b"".join(pieces)):
        fail()
    return request


def build_http_body(wire):
    request = validate_request_wire(wire)
    assets = Path(__file__).resolve().parent
    schema = strict_json(read_bounded(assets / "schema.json", 8192), 8192)
    instruction = string(read_bounded(assets / "instructions.txt", 8192).decode("utf-8"), 8192)
    body = {"model": PROVIDER["requested_model"], "instructions": instruction,
            "input": encode({"units": [{"ordinal": unit["ordinal"], "text": unit["text"]}
                                        for unit in request["source"]["units"]]}).decode("utf-8"),
            "max_output_tokens": request["policy"]["max_output_tokens"], "store": False,
            "background": False, "stream": False, "truncation": "disabled", "parallel_tool_calls": False,
            "tools": [], "tool_choice": "none",
            "text": {"format": {"type": "json_schema", "name": "noetome_candidates", "strict": True, "schema": schema}}}
    raw = encode(body)
    if len(raw) > 65536:
        fail("RESPONSES_REQUEST_LIMIT")
    return raw


OBSERVATION_KEYS = ("schema_version", "protocol", "request_sha256", "http_request_sha256", "endpoint",
                    "http_state", "http_status", "response_id", "actual_model", "failure",
                    "raw_response_available", "canonical_response_available", "usage_input_tokens",
                    "usage_output_tokens", "usage_total_tokens")


def observation(wire, body):
    return {"schema_version": 1, "protocol": "noetome-responses-observation-v1", "request_sha256": sha(wire),
            "http_request_sha256": sha(body), "endpoint": ENDPOINT, "http_state": "not_sent", "http_status": None,
            "response_id": None, "actual_model": None, "failure": None, "raw_response_available": False,
            "canonical_response_available": False, "usage_input_tokens": None, "usage_output_tokens": None,
            "usage_total_tokens": None}


def nullable_string(value, maximum=256):
    if value is not None:
        string(value, maximum, identity=True)


def validate_usage(value):
    if value is None:
        return (None, None, None)
    named(value, ("input_tokens", "output_tokens", "total_tokens"),
          ("input_tokens_details", "output_tokens_details"))
    counts = []
    for field in ("input_tokens", "output_tokens", "total_tokens"):
        count = value[field]
        if count is not None:
            integer(count)
        counts.append(count)
    if all(count is not None for count in counts) and counts[0] + counts[1] != counts[2]:
        fail()
    for field, counter, upper in (("input_tokens_details", "cached_tokens", counts[0]),
                                 ("output_tokens_details", "reasoning_tokens", counts[1])):
        if field in value and value[field] is not None:
            named(value[field], (counter,))
            integer(value[field][counter])
            if upper is not None and value[field][counter] > upper:
                fail()
    return tuple(counts)


def validate_response(value):
    """Strict supported Responses envelope, including documented echoed settings."""
    required = ("id", "object", "created_at", "status", "model", "output", "usage", "error", "incomplete_details")
    optional = ("background", "billing", "completed_at", "conversation", "instructions", "max_output_tokens",
                "max_tool_calls", "metadata", "parallel_tool_calls", "previous_response_id", "prompt",
                "prompt_cache_key", "prompt_cache_retention", "reasoning", "safety_identifier", "service_tier", "store",
                "temperature", "text", "tool_choice", "tools", "top_logprobs", "top_p", "truncation", "user")
    named(value, required, optional)
    if value["object"] != "response" or value["status"] not in ("completed", "failed", "incomplete", "in_progress", "queued", "cancelled"):
        fail()
    string(value["id"], 256, identity=True)
    string(value["model"], 256, identity=True)
    integer(value["created_at"])
    for field in ("background", "parallel_tool_calls", "store"):
        if field in value and type(value[field]) is not bool:
            fail()
    for field in ("completed_at", "max_output_tokens", "max_tool_calls", "top_logprobs"):
        if field in value and value[field] is not None:
            integer(value[field])
    for field in ("previous_response_id", "prompt_cache_key", "prompt_cache_retention", "safety_identifier", "service_tier", "truncation", "user"):
        if field in value:
            nullable_string(value[field])
    # Temperature and top_p are the only documented fractional response fields.
    for field in ("temperature", "top_p"):
        if field in value and value[field] is not None:
            if type(value[field]) not in (int, float) or not 0 <= value[field] <= 2:
                fail()
    if "instructions" in value and value["instructions"] is not None:
        string(value["instructions"], 65536, nonempty=False)
    if "metadata" in value:
        if type(value["metadata"]) is not dict or len(value["metadata"]) > 16:
            fail()
        for key, child in value["metadata"].items():
            string(key, 64)
            string(child, 512, nonempty=False)
    if "billing" in value:
        named(value["billing"], ("payer",))
        if value["billing"]["payer"] not in ("developer", "openai"):
            fail()
    if "conversation" in value and value["conversation"] is not None:
        named(value["conversation"], ("id",))
        string(value["conversation"]["id"], 256, identity=True)
    # No prompt template is used by this adapter. A returned non-null template is not this capability.
    if value.get("prompt") is not None:
        fail()
    if "reasoning" in value and value["reasoning"] is not None:
        named(value["reasoning"], (), ("effort", "summary", "generate_summary"))
        for child in value["reasoning"].values():
            nullable_string(child)
    if "text" in value:
        named(value["text"], ("format",), ("verbosity",))
        form = named(value["text"]["format"], ("type",), ("name", "strict", "schema", "description"))
        if form["type"] not in ("text", "json_object", "json_schema"):
            fail()
        for field in ("name", "description"):
            if field in form:
                string(form[field], 4096, nonempty=False)
        if "strict" in form and type(form["strict"]) is not bool:
            fail()
        if "schema" in form and type(form["schema"]) is not dict:
            fail()
        if "verbosity" in value["text"]:
            nullable_string(value["text"]["verbosity"])
    if "tools" in value and value["tools"] != []:
        fail("RESPONSES_CAPABILITY")
    if "tool_choice" in value and value["tool_choice"] not in ("none", "auto"):
        fail("RESPONSES_CAPABILITY")
    if value["error"] is not None:
        named(value["error"], ("code", "message"))
        string(value["error"]["code"], 256, identity=True)
        string(value["error"]["message"], 65536, nonempty=False)
    if value["incomplete_details"] is not None:
        named(value["incomplete_details"], ("reason",))
        string(value["incomplete_details"]["reason"], 256, identity=True)
    validate_usage(value["usage"])
    if type(value["output"]) is not list:
        fail()
    for item in value["output"]:
        if type(item) is dict and item.get("type") == "reasoning":
            named(item, ("id", "type", "summary"), ("content", "encrypted_content", "status"))
            string(item["id"], 256, identity=True)
            if type(item["summary"]) is not list:
                fail()
            for summary in item["summary"]:
                named(summary, ("type", "text"))
                if summary["type"] != "summary_text":
                    fail()
                string(summary["text"], 524288, nonempty=False)
            if "content" in item and item["content"] is not None:
                if type(item["content"]) is not list:
                    fail()
                for content in item["content"]:
                    named(content, ("type", "text"))
                    if content["type"] != "reasoning_text":
                        fail()
                    string(content["text"], 524288, nonempty=False)
            if "encrypted_content" in item:
                nullable_string(item["encrypted_content"], 524288)
            if "status" in item and item["status"] not in ("completed", "incomplete", "in_progress"):
                fail()
            continue
        if type(item) is not dict or item.get("type") != "message":
            fail("RESPONSES_CAPABILITY")
        named(item, ("id", "type", "role", "status", "content"))
        string(item["id"], 256, identity=True)
        if item["role"] != "assistant" or item["status"] not in ("completed", "incomplete", "in_progress"):
            fail()
        if type(item["content"]) is not list:
            fail()
        for part in item["content"]:
            if type(part) is not dict or part.get("type") != "output_text":
                fail("RESPONSES_CAPABILITY")
            named(part, ("type", "text", "annotations"), ("logprobs",))
            string(part["text"], 524288, nonempty=False)
            if part["annotations"] != [] or part.get("logprobs", []) != []:
                fail("RESPONSES_CAPABILITY")
    return value


def canonical_outcome(wire, request, response):
    if response["status"] != "completed" or response["error"] is not None or response["incomplete_details"] is not None:
        fail("RESPONSES_CAPABILITY")
    messages = [item for item in response["output"] if item["type"] == "message"]
    if len(messages) != 1:
        fail("RESPONSES_CAPABILITY")
    message = messages[0]
    if message["status"] != "completed" or len(message["content"]) != 1:
        fail("RESPONSES_CAPABILITY")
    inner = named(strict_json(message["content"][0]["text"].encode("utf-8"), 524288), ("candidates",))
    proposals = inner["candidates"]
    if type(proposals) is not list or not 1 <= len(proposals) <= 256:
        fail()
    candidates = []
    units = {unit["ordinal"]: unit for unit in request["source"]["units"]}
    for index, proposal in enumerate(proposals):
        named(proposal, ("unit_ordinal", "statement", "qualifiers"))
        integer(proposal["unit_ordinal"])
        if proposal["unit_ordinal"] not in units:
            fail()
        statement = string(proposal["statement"], 4096)
        qualifiers = proposal["qualifiers"]
        if type(qualifiers) is not list or len(qualifiers) > 32:
            fail()
        for qualifier in qualifiers:
            string(qualifier, 256)
        unit = units[proposal["unit_ordinal"]]
        text = unit["text"]
        start = text.find(statement)
        if start < 0 or text.find(statement, start + 1) >= 0:
            fail()
        start_byte = len(text[:start].encode("utf-8"))
        statement_bytes = statement.encode("utf-8")
        candidates.append({"ordinal": index, "statement": statement, "qualifiers": qualifiers,
                           "evidence": [{"span_id": unit["span_id"], "excerpt_start_byte": start_byte,
                                         "excerpt_end_byte": start_byte + len(statement_bytes),
                                         "excerpt_sha256": sha(statement_bytes)}]})
    input_tokens, output_tokens, unused_total = validate_usage(response["usage"])
    if output_tokens is not None and output_tokens > request["policy"]["max_output_tokens"]:
        fail("RESPONSES_CAPABILITY")
    result = {"schema_version": 1, "protocol": PROTOCOL, "request_id": request["request_id"],
              "request_sha256": sha(wire), "provider": request["provider"], "actual_model": response["model"],
              "status": "complete", "candidates": candidates,
              "usage": {"provenance": "provider_reported", "input_tokens": input_tokens, "output_tokens": output_tokens,
                        "cost": {"status": "unknown", "amount_microunits": None, "currency": None, "basis": "not_available"}},
              "failure": None}
    if len(encode(result)) > request["policy"]["max_response_bytes"]:
        fail("RESPONSES_RESPONSE_LIMIT")
    return result


def status_failure(status):
    if status in (401, 403):
        return "authentication"
    if status == 429:
        return "rate_limit"
    if status in (408, 504):
        return "timeout"
    if status in (400, 404, 405, 415, 422):
        return "capability_insufficient"
    return "remote_unknown"


def decode_http_response(wire, raw, http_status, credential=b""):
    request = validate_request_wire(wire, credential)
    body = build_http_body(wire)
    obs = observation(wire, body)
    integer(http_status, 100, 599)
    obs["http_state"], obs["http_status"] = "sent_once", http_status
    if len(raw) > request["policy"]["max_response_bytes"]:
        obs["failure"] = "response_limit"
        return None, obs
    try:
        parsed = strict_json(raw, request["policy"]["max_response_bytes"], credential)
        if not 200 <= http_status < 300 and type(parsed) is dict and set(parsed) == {"error"}:
            error = named(parsed["error"], ("message",), ("type", "param", "code"))
            string(error["message"], 65536, nonempty=False)
            for field in ("type", "param", "code"):
                if field in error:
                    nullable_string(error[field])
            obs["raw_response_available"] = True
            obs["failure"] = status_failure(http_status)
            return None, obs
        response = validate_response(parsed)
    except Invalid:
        obs["failure"] = status_failure(http_status) if not 200 <= http_status < 300 else "invalid_response"
        return None, obs
    obs["raw_response_available"] = True
    obs["response_id"], obs["actual_model"] = response["id"], response["model"]
    obs["usage_input_tokens"], obs["usage_output_tokens"], obs["usage_total_tokens"] = validate_usage(response["usage"])
    if not 200 <= http_status < 300:
        obs["failure"] = status_failure(http_status)
        return None, obs
    try:
        outcome = canonical_outcome(wire, request, response)
    except Invalid as error:
        obs["failure"] = {"RESPONSES_CAPABILITY": "capability_insufficient",
                          "RESPONSES_RESPONSE_LIMIT": "response_limit"}.get(str(error), "invalid_response")
        return None, obs
    obs["canonical_response_available"] = True
    return outcome, obs


def run_live(frame, connection_factory=None):
    """Single-request live entry. Offline tests may inject only the connection boundary."""
    credential, wire = decode_frame(frame)
    request = validate_request_wire(wire, credential)
    body = build_http_body(wire)
    check_strings(strict_json(body, 65536), credential)
    if credential in body:
        fail("RESPONSES_CREDENTIAL_ECHO")
    obs = observation(wire, body)
    check_strings({"observation": obs, "states": ["not_sent", "sent_once", "remote_unknown"],
                   "failures": ["authentication", "rate_limit", "timeout", "capability_insufficient",
                                "remote_unknown", "invalid_response", "response_limit"]}, credential)
    strict_json(encode(obs), 65536, credential)
    for filename in ("http-request.json", "http-observation.json", "http-response.bin"):
        if os.path.lexists(filename):
            fail("RESPONSES_OUTPUT_EXISTS")
    save_fixed("http-request.json", body)
    deadline = time.monotonic() + request["policy"]["timeout_ms"] / 1000
    connection, outcome, raw = None, None, None
    try:
        factory = connection_factory if connection_factory is not None else http.client.HTTPSConnection
        connection = factory(host="ai.1343263.xyz", port=443, timeout=remaining(deadline), context=ssl.create_default_context())
        connection.connect()
        network_socket = connection.sock
        # Set the uncertainty state BEFORE initiating a send; a raised send can follow partial delivery.
        obs["http_state"] = "remote_unknown"
        network_socket.settimeout(remaining(deadline))
        connection.request("POST", "/v1/responses", body=body,
                           headers={"Authorization": b"Bearer " + credential, "Content-Type": "application/json",
                                    "Accept": "application/json", "Accept-Encoding": "identity"})
        network_socket.settimeout(remaining(deadline))
        response = connection.getresponse()
        integer(response.status, 100, 599)
        obs["http_status"] = response.status
        chunks, length = [], 0
        maximum = request["policy"]["max_response_bytes"]
        while True:
            network_socket.settimeout(remaining(deadline))
            chunk = response.read1(min(16384, maximum + 1 - length))
            length += len(chunk)
            if length > maximum:
                obs["failure"] = "response_limit"
                break
            if not chunk:
                if response.length not in (None, 0):
                    raise http.client.IncompleteRead(b"")
                raw = b"".join(chunks)
                outcome, obs = decode_http_response(wire, raw, response.status, credential)
                break
            chunks.append(chunk)
            # read1 closes its file at an exhausted Content-Length. Decode now,
            # before a further socket timeout update on that closed descriptor.
            if response.length == 0:
                raw = b"".join(chunks)
                outcome, obs = decode_http_response(wire, raw, response.status, credential)
                break
    except (TimeoutError, socket.timeout):
        obs["failure"] = "timeout"
    except Exception:
        obs["failure"] = "remote_unknown"
    finally:
        if connection is not None:
            try:
                connection.close()
            except Exception:
                pass
    strict_json(encode(obs), 65536, credential)
    if obs["raw_response_available"]:
        save_fixed("http-response.bin", raw)
    save_fixed("http-observation.json", encode(obs))
    return outcome


def remaining(deadline):
    interval = deadline - time.monotonic()
    if interval <= 0:
        raise TimeoutError()
    return interval


def decode_frame(frame):
    if type(frame) is not bytes or not len(FRAME_MAGIC) + 12 <= len(frame) <= FRAME_MAX or not frame.startswith(FRAME_MAGIC):
        fail("RESPONSES_FRAME_INVALID")
    version, secret_length, wire_length = struct.unpack_from("<III", frame, len(FRAME_MAGIC))
    header_length = len(FRAME_MAGIC) + 12
    if version != 1 or not 1 <= secret_length <= 4096 or not 1 <= wire_length <= 65536 or len(frame) != header_length + secret_length + wire_length:
        fail("RESPONSES_FRAME_INVALID")
    secret = frame[header_length:header_length + secret_length]
    if any(char in secret for char in (0, 10, 13)):
        fail("RESPONSES_FRAME_INVALID")
    try:
        secret.decode("utf-8")
    except UnicodeError:
        fail("RESPONSES_FRAME_INVALID")
    return secret, frame[header_length + secret_length:]


def save_fixed(filename, raw):
    if filename not in ("http-request.json", "http-observation.json", "http-response.bin"):
        fail()
    # Core provides and guards a new working directory. These exclusive fixed files cannot overwrite an existing artifact.
    with open(filename, "xb") as handle:
        handle.write(raw)
        handle.flush()
        os.fsync(handle.fileno())


def read_bounded(filename, maximum):
    with open(filename, "rb") as handle:
        raw = handle.read(maximum + 1)
    if not 0 < len(raw) <= maximum:
        fail()
    return raw


def main():
    try:
        if len(sys.argv) == 3 and sys.argv[1] == "--offline-body":
            wire = read_bounded(sys.argv[2], 65536)
            sys.stdout.buffer.write(build_http_body(wire))
            return 0
        if len(sys.argv) == 5 and sys.argv[1] == "--offline-decode":
            status = sys.argv[4]
            if len(status) != 3 or not status.isascii() or not status.isdecimal() or status[0] == "0":
                return 2
            outcome, unused_obs = decode_http_response(read_bounded(sys.argv[2], 65536), read_bounded(sys.argv[3], 524288), int(status))
            if outcome is None:
                return 2
            sys.stdout.buffer.write(encode(outcome))
            return 0
        if len(sys.argv) == 1:
            frame = sys.stdin.buffer.read(FRAME_MAX + 1)
            outcome = run_live(frame)
            if outcome is None:
                return 2
            sys.stdout.buffer.write(encode(outcome))
            return 0
    except (Exception, KeyboardInterrupt):
        pass
    return 2


if __name__ == "__main__":
    sys.exit(main())
