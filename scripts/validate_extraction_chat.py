"""Independent Chat CLI audit using author gold and numeric-loopback HTTP only.

No selected live service, actual key, model or paid call is used. The common
source/config/output checks are shared with the existing Responses audit;
Chat requests, usage and returned content are checked independently here.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from validate_extraction_api import Audit, TEST_KEY, encoded, safe_text, save
from validate_extraction_gateway import sha

STRATEGY = "tangmao-deepseek-chat-positive-revision-v6"


class LocalChat:
    def __init__(self, payload, *, candidate_change=None, wire_change=None,
                 raw=None, status=200, delay_headers=0.0, delay_body=0.0,
                 truncated=False, on_request=None, duplicate_candidate=False,
                 duplicate_wire=False, trailing=False):
        self.receipts, self.errors = [], []
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    assert self.path == "/v1/chat/completions"
                    assert self.headers.get("Authorization") == "Bearer " + TEST_KEY
                    length = int(self.headers.get("Content-Length", "0"))
                    assert 0 < length <= 256 * 1024
                    body = self.rfile.read(length)
                    request = json.loads(body)
                    user = json.loads(request["messages"][1]["content"])
                    input_data = user["request"]
                    fake.receipts.append({"method": "POST", "path": self.path,
                                          "authorization_matches_artificial_test_key": True,
                                          "body_sha256": sha(body), "body_length": len(body),
                                          "request": request})
                    if on_request:
                        on_request()
                    candidate = copy.deepcopy(payload)
                    candidate.pop("producer")
                    candidate["request_id"] = input_data["request_id"]
                    candidate["binding"] = input_data["binding"]
                    if candidate_change:
                        candidate_change(candidate)
                    text = encoded(candidate).decode()
                    if duplicate_candidate:
                        text = text.replace('"schema_version":1', '"schema_version":1,"schema_version":1', 1)
                    wire = {
                        "id": "chatcmpl_noetome_test", "object": "chat.completion",
                        "created": 1, "model": "noetome-test-model",
                        "choices": [{"index": 0, "finish_reason": "stop", "message": {
                            "role": "assistant", "content": text, "reasoning_content": None}}],
                        "usage": {"prompt_tokens": 100, "completion_tokens": 600, "total_tokens": 700,
                                  "prompt_tokens_details": {"cached_tokens": 0},
                                  "completion_tokens_details": {"reasoning_tokens": 0}},
                    }
                    if wire_change:
                        wire_change(wire)
                    response = encoded(wire) if raw is None else raw
                    if duplicate_wire:
                        response = response.replace(b'"object":"chat.completion"', b'"object":"chat.completion","object":"chat.completion"', 1)
                    if trailing:
                        response += b" {}"
                    fake.receipts[-1].update(response_sha256=sha(response), response_length=len(response),
                                             advertised_status=status)
                    if delay_headers:
                        time.sleep(delay_headers)
                    self.send_response(status)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(response) + (17 if truncated else 0)))
                    self.send_header("Connection", "close")
                    self.send_header("x-request-id", "req_noetome_chat_test")
                    self.end_headers()
                    if delay_body:
                        time.sleep(delay_body)
                    self.wfile.write(response)
                    self.wfile.flush()
                    self.close_connection = True
                except (BrokenPipeError, ConnectionResetError, OSError):
                    pass  # deliberate client timeout, cancellation and byte limits
                except BaseException as exc:
                    fake.errors.append(safe_text(repr(exc)))

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}/v1/chat/completions"
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        assert not self.thread.is_alive()


class ChatAudit(Audit):
    def local_fake(self, payload, **options):
        return LocalChat(payload, **options)

    @staticmethod
    def config(endpoint):
        value = Audit.config(endpoint)
        value["provider"] = "openai_chat_completions"
        return value

    def gold_and_body(self, output, receipt, **_):
        body = receipt["request"]
        assert set(body) == {"model", "messages", "thinking", "response_format", "max_tokens", "stream"}
        assert body["model"] == "noetome-test-model" and body["stream"] is False
        assert body["thinking"] == {"type": "disabled"}
        assert body["response_format"] == {"type": "json_object"} and body["max_tokens"] == 2048
        assert [m["role"] for m in body["messages"]] == ["system", "user"]
        user = json.loads(body["messages"][1]["content"])
        assert set(user) == {"task", "response_schema", "request"}
        assert user["task"].startswith(body["messages"][0]["content"]) and "plain JSON object" in user["task"]
        for instruction in ("two top-level keys: atom and measurement", "inside atom", "sibling of atom", "Do not flatten"):
            assert instruction in user["task"]
        for instruction in ("Include measurement.unit verbatim in atom.qualifiers", "Include every measurement.conditions string verbatim in atom.qualifiers"):
            assert instruction in user["task"]
        assert "Every concept.revision and atom.revision must be a positive integer" in user["task"]
        atom_item = user["response_schema"]["properties"]["atoms"]["items"]
        assert set(atom_item["required"]) == set(atom_item["properties"]) == {"atom", "measurement"}
        for revision in (user["response_schema"]["properties"]["concepts"]["items"]["properties"]["revision"],
                         atom_item["properties"]["atom"]["properties"]["revision"]):
            assert revision["type"] == "integer" and revision["minimum"] == 1

        def schema(node):
            if isinstance(node, dict):
                if node.get("type") == "object" or (isinstance(node.get("type"), list) and "object" in node["type"]):
                    assert node["additionalProperties"] is False
                    assert set(node["required"]) == set(node["properties"])
                for value in node.values():
                    schema(value)
            elif isinstance(node, list):
                for value in node:
                    schema(value)
        schema(user["response_schema"])
        raw = (output / "request-body.json").read_bytes()
        assert sha(raw) == receipt["body_sha256"] and len(raw) == receipt["body_length"] and json.loads(raw) == body
        data, version = user["request"], self.imported["version"]
        binding = data["binding"]
        for field in ("version_id", "source_id", "content_sha256", "normalized_sha256"):
            assert binding[field] == version[field]
        assert binding["import_id"] == self.import_id and binding["parse_id"] is None
        normalized = (self.project / version["normalized_snapshot"]).read_bytes()
        assert sha(normalized) == binding["normalized_sha256"]
        span, unit = self.imported["spans"][0], data["units"][0]
        assert unit["locator"]["span_id"] == span["span_id"]
        assert unit["text"] == normalized[span["start_byte"]:span["end_byte"]].decode()
        assert unit["start_byte"] == span["start_byte"] and unit["end_byte"] == span["end_byte"]
        anchor = data["evidence_anchors"][0]
        assert anchor["excerpt_start_byte"] == unit["start_byte"] and anchor["excerpt_end_byte"] == unit["end_byte"]
        assert anchor["quote"] == unit["text"] and anchor["excerpt_sha256"] == sha(unit["text"].encode())
        request = json.loads((output / "request.json").read_text(encoding="utf-8"))
        assert request["binding"] == binding and request["request_id"] == data["request_id"]
        candidates = json.loads((output / "candidates.json").read_text(encoding="utf-8"))["payload"]
        assert candidates["binding"] == binding and candidates["request_id"] == data["request_id"]
        assert candidates["concepts"][0]["canonical_name"] == self.gold["concept_name"]
        atom, measurement = candidates["atoms"][0]["atom"], candidates["atoms"][0]["measurement"]
        assert atom["content"] == self.gold["atom_content"] and atom["required"] is False and atom["verification"] == "unverified"
        assert measurement["value"] == self.gold["value"] and measurement["unit"] == self.gold["unit"]
        assert measurement["conditions"] == self.gold["conditions"]
        evidence = candidates["evidence"][0]
        excerpt = normalized[evidence["excerpt_start_byte"]:evidence["excerpt_end_byte"]]
        assert evidence["quote"] == self.gold["quote"] == excerpt.decode() and evidence["excerpt_sha256"] == sha(excerpt)
        report = json.loads((output / "validation.json").read_text(encoding="utf-8"))
        assert report["provider"] == "openai_chat_completions" and report["request_strategy"] == STRATEGY
        assert report["usage"]["model_usage"]["input_tokens"] == 100
        assert report["usage"]["model_usage"]["output_tokens"] == 600 and report["usage"]["model_usage"]["total_tokens"] == 700
        assert report["usage"]["raw_model_usage"]["prompt_tokens"] == 100
        assert report["usage"]["raw_model_usage"]["completion_tokens"] == 600
        assert report["usage"]["request_utf8_bytes"] == len(raw)
        assert report["usage"]["input_reserved_tokens"] == len(raw) + 1024
        assert report["usage"]["reserved_tokens"] == len(raw) + 1024 + body["max_tokens"]

    def matrix(self):
        self.case("chat-normal", expected=0, check_gold=True, metadata=True)
        self.case("chat-replay", expected=0, check_gold=True, metadata=True)
        self.case("chat-missing-unit-qualifier", code="EXTRACT_QUALIFIERS", metadata=True,
                  server={"candidate_change": lambda p: p["atoms"][0]["atom"]["qualifiers"].remove(p["atoms"][0]["measurement"]["unit"])})
        self.case("chat-zero-concept-revision", code="EXTRACT_KNOWLEDGE", metadata=True,
                  server={"candidate_change": lambda p: p["concepts"][0].update(revision=0)})
        self.case("chat-zero-atom-revision", code="EXTRACT_KNOWLEDGE", metadata=True,
                  server={"candidate_change": lambda p: p["atoms"][0]["atom"].update(revision=0)})
        choice = lambda wire: wire["choices"][0]
        message = lambda wire: choice(wire)["message"]
        self.case("chat-length", code="EXTRACT_API_INCOMPLETE", metadata=True, server={"wire_change": lambda w: choice(w).update(finish_reason="length")})
        self.case("chat-filter", code="EXTRACT_API_REFUSAL", metadata=True, server={"wire_change": lambda w: choice(w).update(finish_reason="content_filter")})
        self.case("chat-refusal", code="EXTRACT_API_REFUSAL", metadata=True, server={"wire_change": lambda w: message(w).update(refusal="refused")})
        self.case("chat-tool", code="EXTRACT_API_OUTPUT", metadata=True, server={"wire_change": lambda w: message(w).update(tool_calls=[{"id": "tool"}])})
        self.case("chat-function", code="EXTRACT_API_OUTPUT", metadata=True, server={"wire_change": lambda w: message(w).update(function_call={"name": "unexpected"})})
        self.case("chat-two-choices", code="EXTRACT_API_OUTPUT", metadata=True, server={"wire_change": lambda w: w["choices"].append(copy.deepcopy(w["choices"][0]))})
        self.case("chat-wrong-index", code="EXTRACT_API_OUTPUT", metadata=True, server={"wire_change": lambda w: choice(w).update(index=1)})
        self.case("chat-wrong-role", code="EXTRACT_API_OUTPUT", metadata=True, server={"wire_change": lambda w: message(w).update(role="user")})
        self.case("chat-fenced", code="EXTRACT_API_CANDIDATES", metadata=True, server={"wire_change": lambda w: message(w).update(content="```json\n" + message(w)["content"] + "\n```")})
        self.case("chat-candidate-extra", code="EXTRACT_API_CANDIDATES", metadata=True, server={"candidate_change": lambda c: c.update(unexpected=1)})
        self.case("chat-candidate-duplicate", code="EXTRACT_API_CANDIDATES", metadata=True, server={"duplicate_candidate": True})
        self.case("chat-wire-duplicate", code="EXTRACT_API_RESPONSE", metadata=False, server={"duplicate_wire": True})
        self.case("chat-wire-trailing", code="EXTRACT_API_RESPONSE", metadata=False, server={"trailing": True})
        self.case("chat-no-usage", code="EXTRACT_API_USAGE", metadata=False, server={"wire_change": lambda w: w.pop("usage")})
        self.case("chat-wrong-total", code="EXTRACT_API_USAGE", metadata=False, server={"wire_change": lambda w: w["usage"].update(total_tokens=701)})
        self.case("chat-boolean-usage", code="EXTRACT_API_USAGE", metadata=False, server={"wire_change": lambda w: w["usage"].update(prompt_tokens=True)})
        self.case("chat-empty-wire", metadata=False, server={"raw": b""})
        self.case("chat-framing-cut", metadata=False, server={"truncated": True})
        self.case("chat-auth", metadata=False, server={"status": 401})
        self.case("chat-rate", metadata=False, server={"status": 429})
        self.case("chat-service", metadata=False, server={"status": 503})
        self.case("chat-zero-budget", calls=0, config_change=lambda c: c["budget"].update(max_requests=0))
        self.case("chat-price-unknown", calls=0, config_change=lambda c: c["budget"].update(max_cost_usd="0.01"))
        self.case("chat-provider-mismatch", expected=2, calls=0, config_change=lambda c: c.update(provider="openai_responses"))
        self.case("chat-local-model-mismatch", expected=2, calls=0, config_change=lambda c: c.update(model="deepseek-flash"))
        def unselected_live(config):
            config.update(transport_mode="live", endpoint="https://api.openai.com/v1/chat/completions", model="deepseek-flash")
            config["budget"]["max_requests"] = 0
        self.case("chat-unselected-live-host", expected=2, calls=0, config_change=unselected_live)
        self.case("chat-content-echo", omitted=True, server={"wire_change": lambda w: message(w).update(content=TEST_KEY)})
        self.case("chat-reasoning-echo", omitted=True, server={"wire_change": lambda w: message(w).update(reasoning_content=TEST_KEY)})
        self.case("chat-timeout", metadata=False, config_change=lambda c: c["limits"].update(timeout_ms=80), server={"delay_headers": 0.2})
        self.case("chat-cancel", calls=0, cancel_before=True)
        self.case("chat-source-human", source_injection=True)
        self.case("chat-output-human", output_injection=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    audit = ChatAudit(args.binary, args.root)
    try:
        audit.matrix()
        audit.finish()
        print(f"{len(audit.rows)} independent Chat CLI/local HTTP checks passed; no real model or paid calls")
    except BaseException as exc:
        save(audit.root / "failure-evidence.json", {"error": safe_text(str(exc)), "checks": audit.rows, "calls": audit.calls, "real_model_calls": 0})
        raise


if __name__ == "__main__":
    main()
