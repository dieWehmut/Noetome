"""Offline only: synthetic data and credential, never the selected remote API."""
import hashlib
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import socket
import http.client
import http.server
import threading
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "sidecars" / "responses_api" / "adapter.py"
spec = importlib.util.spec_from_file_location("noetome_responses_adapter", ADAPTER)
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


def wire_request():
    text = "灯箱的输入电压为 12 V，仅用于直流供电。\n"
    raw = text.encode("utf-8")
    return json.dumps({
        "schema_version": 1, "protocol": "noetome-model-single-v1",
        "request_id": "10000000-0000-4000-8000-000000000001",
        "provider": {"producer": "model", "provider_id": "ai-1343263-responses",
                     "adapter_version": "responses-stdlib-v1", "requested_model": "deepseek-flash"},
        "source": {
            "project_id": "10000000-0000-4000-8000-000000000002",
            "source_id": "10000000-0000-4000-8000-000000000003",
            "version_id": "10000000-0000-4000-8000-000000000004", "revision": 1,
            "raw_sha256": hashlib.sha256(raw).hexdigest(),
            "normalized_sha256": hashlib.sha256(raw).hexdigest(),
            "normalization_version": "utf8-newlines-v1", "parser_version": "blocks-v1",
            "units": [{"kind": "text_span", "span_id": "10000000-0000-4000-8000-000000000005",
                       "ordinal": 0, "start_byte": 0, "end_byte": len(raw),
                       "text_sha256": hashlib.sha256(raw).hexdigest(), "text": text}]},
        "policy": {"max_input_bytes": 8192, "max_response_bytes": 524288,
                   "max_output_tokens": 1024, "timeout_ms": 30000, "max_calls": 1,
                   "retries": 0, "search_enabled": False, "tools_enabled": False}
    }, ensure_ascii=False, indent=2).encode("utf-8")


def provider_response():
    return {"id": "resp_synthetic_frozen", "object": "response", "created_at": 1,
            "status": "completed", "model": "deepseek-flash-returned-synthetic",
            "output": [{"id": "msg_synthetic", "type": "message", "role": "assistant", "status": "completed",
                        "content": [{"type": "output_text", "annotations": [], "logprobs": [],
                                     "text": json.dumps({"candidates": [{"unit_ordinal": 0,
                                         "statement": "灯箱的输入电压为 12 V，仅用于直流供电。", "qualifiers": ["仅用于直流供电"]}]}, ensure_ascii=False)}]}],
            "usage": {"input_tokens": 100, "output_tokens": 30, "total_tokens": 130,
                      "input_tokens_details": {"cached_tokens": 0},
                      "output_tokens_details": {"reasoning_tokens": 0}},
            "error": None, "incomplete_details": None}


def decode_cli(raw, wire=None, status="200"):
    with tempfile.TemporaryDirectory() as directory:
        request_path = Path(directory) / "request.json"
        response_path = Path(directory) / "http-response.bin"
        request_path.write_bytes(wire if wire is not None else wire_request())
        response_path.write_bytes(raw)
        before = {p.name: p.read_bytes() for p in Path(directory).iterdir()}
        run = subprocess.run([sys.executable, "-B", str(ADAPTER), "--offline-decode",
                              str(request_path), str(response_path), status],
                             cwd=directory, capture_output=True, timeout=5)
        after = {p.name: p.read_bytes() for p in Path(directory).iterdir()}
        if before != after:
            raise AssertionError("offline decode mutated files")
        return run


def response_raw(response=None):
    return json.dumps(response if response is not None else provider_response(),
                      ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def credential_frame(wire=None, secret=b"synthetic-private-key-ONLY-TEST"):
    import struct
    wire = wire if wire is not None else wire_request()
    return b"NOETOME-CREDENTIAL-STDIN\0" + struct.pack("<III", 1, len(secret), len(wire)) + secret + wire


class MockResponse:
    def __init__(self, raw, status=200, read_error=None):
        self.raw, self.status, self.read_error = raw, status, read_error
        self.length = len(raw)

    def read1(self, maximum):
        if self.read_error:
            raise self.read_error
        chunk, self.raw = self.raw[:maximum], self.raw[maximum:]
        self.length -= len(chunk)
        return chunk


class MockConnection:
    """Boundary double records the actual outgoing request; no remote contact."""
    def __init__(self, raw=None, status=200, connect_error=None, send_error=None, read_error=None):
        self.response = MockResponse(raw if raw is not None else response_raw(), status, read_error)
        self.connect_error, self.send_error = connect_error, send_error
        self.requests, self.events = [], []
        self.sock = self

    def connect(self):
        self.events.append("connect")
        if self.connect_error:
            raise self.connect_error

    def request(self, method, path, body=None, headers=None):
        self.events.append("request")
        self.requests.append((method, path, body, headers))
        if self.send_error:
            raise self.send_error

    def getresponse(self):
        self.events.append("response")
        return self.response

    def settimeout(self, timeout):
        if not 0 < timeout <= 30:
            raise AssertionError("transport exceeded timeout policy")

    def close(self):
        self.events.append("close")


def live_test(frame=None, connection=None):
    import os
    connection = connection if connection is not None else MockConnection()
    with tempfile.TemporaryDirectory() as directory:
        old = os.getcwd()
        try:
            os.chdir(directory)
            outcome = adapter.run_live(frame if frame is not None else credential_frame(),
                                       connection_factory=lambda **unused: connection)
            files = {p.name: p.read_bytes() for p in Path(directory).iterdir()}
        finally:
            os.chdir(old)
    return outcome, files, connection


class OfflineBodyBehaviorTests(unittest.TestCase):
    def test_full_8192_byte_input_is_sent_without_truncation_and_8193_is_rejected(self):
        request = json.loads(wire_request())
        for size in (8192, 8193):
            text = "x" * size
            unit = request["source"]["units"][0]
            unit.update(text=text, end_byte=size, text_sha256=hashlib.sha256(text.encode()).hexdigest())
            request["source"]["normalized_sha256"] = unit["text_sha256"]
            wire = json.dumps(request).encode()
            if size == 8192:
                body = json.loads(adapter.build_http_body(wire))
                self.assertEqual(json.loads(body["input"])["units"][0]["text"], text)
            else:
                with self.assertRaises(adapter.Invalid):
                    adapter.build_http_body(wire)

    def test_offline_body_contains_full_source_once_and_disables_tools(self):
        with tempfile.TemporaryDirectory() as directory:
            request_file = Path(directory) / "request.json"
            request_file.write_bytes(wire_request())
            run = subprocess.run([sys.executable, "-B", str(ADAPTER), "--offline-body", str(request_file)],
                                 cwd=directory, capture_output=True, timeout=5)
            self.assertEqual(run.returncode, 0, "offline body generation must succeed")
            self.assertEqual(run.stderr, b"")
            body = json.loads(run.stdout)
            self.assertEqual(body["model"], "deepseek-flash")
            self.assertEqual(body["tools"], [])
            self.assertEqual(body["tool_choice"], "none")
            self.assertEqual(body["max_output_tokens"], 1024)
            self.assertEqual(body["text"]["format"]["type"], "json_schema")
            self.assertTrue(body["text"]["format"]["strict"])
            source_text = json.loads(body["input"])["units"][0]["text"]
            self.assertEqual(source_text, "灯箱的输入电压为 12 V，仅用于直流供电。\n")
            self.assertEqual(json.dumps(body, ensure_ascii=False).count("灯箱的输入电压"), 1)
            self.assertEqual(list(Path(directory).iterdir()), [request_file])

    def test_credential_frame_is_not_treated_as_offline_domain_request(self):
        run = decode_cli(response_raw(), wire=b"NOETOME-CREDENTIAL-STDIN\0" + wire_request())
        self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))


class OfflineDecodeBehaviorTests(unittest.TestCase):
    def test_reasoning_item_is_scanned_and_ignored_while_message_forms_candidate(self):
        response = provider_response()
        response["output"].insert(0, {"id": "rs_synthetic", "type": "reasoning", "status": "completed",
                                     "summary": [{"type": "summary_text", "text": "synthetic summary"}],
                                     "content": [{"type": "reasoning_text", "text": "synthetic reasoning"}],
                                     "encrypted_content": None})
        run = decode_cli(response_raw(response))
        self.assertEqual((run.returncode, run.stderr), (0, b""))
        self.assertEqual(len(json.loads(run.stdout)["candidates"]), 1)
        response["output"][0]["unrecognized"] = "x"
        run = decode_cli(response_raw(response))
        self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))

    def test_formatting_units_are_in_input_and_unicode_evidence_uses_local_byte_offsets(self):
        request = json.loads(wire_request())
        statement = "灯箱的输入电压为 12 V，仅用于直流供电。"
        texts = ["\n\n", "说明。\n" + statement + "\n", "\n"]
        units, offset = [], 0
        for ordinal, text in enumerate(texts):
            raw = text.encode("utf-8")
            units.append({"kind": "text_span", "span_id": "10000000-0000-4000-8000-%012d" % (ordinal + 5),
                          "ordinal": ordinal, "start_byte": offset, "end_byte": offset + len(raw),
                          "text_sha256": hashlib.sha256(raw).hexdigest(), "text": text})
            offset += len(raw)
        request["source"]["units"] = units
        request["source"]["normalized_sha256"] = hashlib.sha256("".join(texts).encode()).hexdigest()
        wire = json.dumps(request, ensure_ascii=False).encode()
        body = json.loads(adapter.build_http_body(wire))
        self.assertEqual([u["text"] for u in json.loads(body["input"])["units"]], texts)
        response = provider_response()
        response["output"][0]["content"][0]["text"] = json.dumps({"candidates": [{"unit_ordinal": 1, "statement": statement, "qualifiers": []}]}, ensure_ascii=False)
        run = decode_cli(response_raw(response), wire=wire)
        self.assertEqual(run.returncode, 0)
        evidence = json.loads(run.stdout)["candidates"][0]["evidence"][0]
        self.assertEqual((evidence["excerpt_start_byte"], evidence["excerpt_end_byte"]), (10, 66))
        self.assertEqual(evidence["span_id"], "10000000-0000-4000-8000-000000000006")

    def test_unknown_usage_is_null_and_ambiguous_evidence_cannot_be_canonical(self):
        response = provider_response()
        response["usage"] = None
        run = decode_cli(response_raw(response))
        self.assertEqual(run.returncode, 0)
        self.assertIsNone(json.loads(run.stdout)["usage"]["input_tokens"])
        self.assertIsNone(json.loads(run.stdout)["usage"]["output_tokens"])
        request = json.loads(wire_request())
        unit = request["source"]["units"][0]
        unit["text"] *= 2
        unit["end_byte"] *= 2
        unit["text_sha256"] = hashlib.sha256(unit["text"].encode()).hexdigest()
        request["source"]["normalized_sha256"] = unit["text_sha256"]
        run = decode_cli(response_raw(), wire=json.dumps(request, ensure_ascii=False).encode())
        self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))

    def test_frozen_synthetic_protocol_fixture_decodes_exactly(self):
        fixture = ROOT / "tests" / "fixtures" / "responses-api-synthetic-v1"
        run = decode_cli((fixture / "http-response.json").read_bytes(), wire=(fixture / "request.json").read_bytes())
        self.assertEqual((run.returncode, run.stderr), (0, b""))
        self.assertEqual(run.stdout, (fixture / "canonical-response.json").read_bytes())
        self.assertEqual(adapter.build_http_body((fixture / "request.json").read_bytes()), (fixture / "http-request.json").read_bytes())

    def test_completed_response_has_original_wire_sha_actual_model_and_local_evidence(self):
        wire = wire_request()
        run = decode_cli(response_raw(), wire=wire)
        self.assertEqual(run.returncode, 0, "strict valid response must decode")
        self.assertEqual(run.stderr, b"")
        outcome = json.loads(run.stdout)
        self.assertEqual(outcome["request_sha256"], hashlib.sha256(wire).hexdigest())
        self.assertEqual(outcome["actual_model"], "deepseek-flash-returned-synthetic")
        self.assertEqual(outcome["status"], "complete")
        candidate = outcome["candidates"][0]
        self.assertEqual(candidate["ordinal"], 0)
        self.assertEqual(candidate["statement"], "灯箱的输入电压为 12 V，仅用于直流供电。")
        self.assertEqual(candidate["evidence"], [{"span_id": "10000000-0000-4000-8000-000000000005",
                          "excerpt_start_byte": 0, "excerpt_end_byte": 56,
                          "excerpt_sha256": "33e4ae0ee3597f873b7ee97cd68aae9d10ec3b6cde87cc90ca9e6647ec17cdff"}])
        self.assertEqual(outcome["usage"], {"provenance": "provider_reported", "input_tokens": 100,
                         "output_tokens": 30, "cost": {"status": "unknown", "amount_microunits": None,
                         "currency": None, "basis": "not_available"}})
        self.assertIsNone(outcome["failure"])

    def test_strict_response_rejects_unknown_duplicates_wrong_types_and_bad_unicode(self):
        mutations = []
        original = provider_response()
        for path, value in [(("unrecognized",), "x"), (("usage", "input_tokens"), True),
                            (("usage", "output_tokens"), 30.0), (("usage", "total_tokens"), -1),
                            (("usage", "total_tokens"), 131), (("model",), "\ud800"),
                            (("status",), "in_progress"), (("status",), "incomplete"),
                            (("usage", "output_tokens"), 1025), (("output",), []),
                            (("output", 0, "content", 0, "type"), "refusal"),
                            (("output", 0, "type"), "function_call")]:
            changed = copy.deepcopy(original)
            target = changed
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            mutations.append(json.dumps(changed, ensure_ascii=True).encode())
        mutations += [b'{"id":"a","id":"b"}', b'[]', b'{"model":NaN}', b'\xff',
                      response_raw()[:-1], response_raw() + b'{}']
        for raw in mutations:
            with self.subTest(case=mutations.index(raw)):
                run = decode_cli(raw)
                self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))

    def test_candidate_rejects_invented_span_ambiguous_excerpt_bad_shape_and_empty(self):
        values = [{"candidates": []}, {"candidates": [[0, "anything", []]]},
                  {"candidates": [{"unit_ordinal": True, "statement": "灯箱", "qualifiers": []}]},
                  {"candidates": [{"unit_ordinal": 0.0, "statement": "灯箱", "qualifiers": []}]},
                  {"candidates": [{"unit_ordinal": 99, "statement": "灯箱", "qualifiers": []}]},
                  {"candidates": [{"unit_ordinal": 0, "statement": "invented", "qualifiers": []}]},
                  {"candidates": [{"unit_ordinal": 0, "statement": "", "qualifiers": []}]},
                  {"candidates": [{"unit_ordinal": 0, "statement": "灯箱", "qualifiers": [], "span_id": "a"}]}]
        rawtexts = [json.dumps(value, ensure_ascii=False) for value in values]
        rawtexts += ['{"candidates":[],"candidates":[]}', '{"candidates":[{"unit_ordinal":0,"unit_ordinal":1,"statement":"灯箱","qualifiers":[]}]}']
        for text in rawtexts:
            response = provider_response()
            response["output"][0]["content"][0]["text"] = text
            run = decode_cli(response_raw(response))
            self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))


class SingleHttpBehaviorTests(unittest.TestCase):
    def test_safe_unknown_missing_or_wrong_type_envelope_retains_raw_for_diagnostics(self):
        responses = []
        unknown = provider_response()
        unknown["unsupported_vendor_field"] = {"diagnostic": "synthetic only"}
        responses.append((unknown, 200))
        missing = provider_response()
        del missing["model"]
        responses.append((missing, 200))
        wrong_type = provider_response()
        wrong_type["usage"]["input_tokens"] = True
        responses.append((wrong_type, 200))
        responses.append((unknown, 401))
        for index, (response, status) in enumerate(responses):
            with self.subTest(case=index):
                raw = json.dumps(response, ensure_ascii=False, indent=2).encode("utf-8")
                outcome, files, connection = live_test(connection=MockConnection(raw=raw, status=status))
                self.assertIsNone(outcome)
                self.assertIn("http-response.bin", files, "strict safe JSON must remain inspectable after envelope rejection")
                self.assertEqual(files["http-response.bin"], raw)
                self.assertEqual(len(connection.requests), 1)
                obs = json.loads(files["http-observation.json"])
                failure = "invalid_response" if status == 200 else "authentication"
                self.assertEqual((obs["http_state"], obs["http_status"], obs["failure"]), ("sent_once", status, failure))
                self.assertTrue(obs["raw_response_available"])
                self.assertFalse(obs["canonical_response_available"])
                for field in ("response_id", "actual_model", "usage_input_tokens", "usage_output_tokens", "usage_total_tokens"):
                    self.assertIsNone(obs[field])
                run = decode_cli(raw, status=str(status))
                self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))

    def test_diagnostic_raw_retention_does_not_accept_echo_duplicates_or_malformed_json(self):
        unknown_echo = provider_response()
        unknown_echo["unsupported_vendor_field"] = "synthetic-private-key-ONLY-TEST"
        raw_cases = [response_raw(unknown_echo), response_raw().replace(b'"id":', b'"id":"duplicate","id":', 1),
                     response_raw()[:-1], b"\xff", b'{"future":1e999}']
        for raw in raw_cases:
            with self.subTest(case=raw_cases.index(raw)):
                outcome, files, connection = live_test(connection=MockConnection(raw=raw))
                self.assertIsNone(outcome)
                self.assertNotIn("http-response.bin", files)
                self.assertEqual(len(connection.requests), 1)
                obs = json.loads(files["http-observation.json"])
                self.assertFalse(obs["raw_response_available"])
                self.assertFalse(obs["canonical_response_available"])
                for field in ("response_id", "actual_model", "usage_input_tokens", "usage_output_tokens", "usage_total_tokens"):
                    self.assertIsNone(obs[field])

    def test_json_scalar_prefix_codefence_and_unclosed_escaped_credential_are_never_saved(self):
        secret = b"SYNTHETIC_REVIEW_KEY_20261102"
        escaped = "".join("\\u%04x" % c for c in secret)
        variants = {
            "scalar": '"' + escaped + '"',
            "nested": json.dumps('"' + escaped + '"'),
            "unterminated_object": '{"value":"' + escaped,
            "unterminated_scalar": '"' + escaped,
            "codefence": chr(96) * 3 + 'json\n{"value":"' + escaped + '"}\n' + chr(96) * 3,
            "prefixed_object": 'Here is the JSON: {"value":"' + escaped + '"}',
        }
        for kind, text in variants.items():
            with self.subTest(kind=kind):
                response = provider_response()
                response["output"][0]["content"][0]["text"] = text
                outcome, files, unused = live_test(frame=credential_frame(secret=secret),
                                                  connection=MockConnection(raw=response_raw(response)))
                self.assertIsNone(outcome)
                self.assertNotIn("http-response.bin", files, "decodable secret must not enter raw persistence")
                obs = json.loads(files["http-observation.json"])
                self.assertFalse(obs["raw_response_available"])
                self.assertIsNone(obs["actual_model"])
                self.assertIsNone(obs["usage_total_tokens"])

    def test_escape_scan_depth_is_bounded_and_natural_backslashes_remain_readable(self):
        secret = b"SYNTHETIC_REVIEW_KEY_20261102"
        escaped = "".join("\\u%04x" % c for c in secret)
        text = '"' + escaped + '"'
        for unused in range(10):
            text = json.dumps(text)
        response = provider_response()
        response["output"][0]["content"][0]["text"] = text
        outcome, files, unused = live_test(frame=credential_frame(secret=secret),
                                          connection=MockConnection(raw=response_raw(response)))
        self.assertIsNone(outcome)
        self.assertNotIn("http-response.bin", files)
        response = provider_response()
        response["output"].insert(0, {"id": "rs_natural", "type": "reasoning",
                                     "summary": [{"type": "summary_text",
                                                  "text": r'He said \"hi\"; C:\notes\file; \alpha is natural text.'}]})
        outcome, files, unused = live_test(frame=credential_frame(secret=secret),
                                          connection=MockConnection(raw=response_raw(response)))
        self.assertIsNotNone(outcome)
        self.assertIn("http-response.bin", files)

    def test_malformed_inner_json_closed_string_credential_escape_suppresses_raw(self):
        secret = b"synthetic-private-key-ONLY-TEST"
        escaped = "".join("\\u%04x" % c for c in secret)
        response = provider_response()
        response["output"][0]["content"][0]["text"] = '{"candidates":[{"unit_ordinal":0,"statement":"' + escaped + '","qualifiers":[]}'
        outcome, files, unused = live_test(connection=MockConnection(raw=response_raw(response)))
        self.assertIsNone(outcome)
        self.assertNotIn("http-response.bin", files)
        self.assertIsNone(json.loads(files["http-observation.json"])["usage_total_tokens"])

    def test_nonfinite_json_number_cannot_hide_in_echoed_schema(self):
        response = provider_response()
        response["text"] = {"format": {"type": "json_schema", "name": "noetome_candidates", "strict": True,
                                        "schema": {"mal": "NUMBER_SENTINEL"}}}
        raw = response_raw(response).replace(b'"NUMBER_SENTINEL"', b"1e999")
        outcome, files, unused = live_test(connection=MockConnection(raw=raw))
        self.assertIsNone(outcome)
        self.assertNotIn("http-response.bin", files)
        obs = json.loads(files["http-observation.json"])
        self.assertEqual(obs["failure"], "invalid_response")

    def test_credential_colliding_with_fixed_observation_fields_is_rejected_before_send(self):
        for secret in (b"https://ai.1343263.xyz", b"noetome-responses-observation-v1", b"sent_once"):
            connection = MockConnection()
            with self.assertRaises(adapter.Invalid):
                live_test(frame=credential_frame(secret=secret), connection=connection)
            self.assertEqual(connection.events, [])

    def test_response_limit_keeps_unknown_state_and_does_not_persist_partial_raw(self):
        outcome, files, connection = live_test(connection=MockConnection(raw=b"x" * 524289))
        self.assertIsNone(outcome)
        obs = json.loads(files["http-observation.json"])
        self.assertEqual((obs["http_state"], obs["failure"]), ("remote_unknown", "response_limit"))
        self.assertNotIn("http-response.bin", files)
        self.assertIsNone(obs["usage_total_tokens"])
        self.assertEqual(len(connection.requests), 1)

    def test_existing_fixed_artifact_denies_invocation_before_network(self):
        import os
        with tempfile.TemporaryDirectory() as directory:
            old = os.getcwd()
            try:
                os.chdir(directory)
                sentinel = Path("http-response.bin")
                sentinel.write_bytes(b"keep")
                connection = MockConnection()
                with self.assertRaises(adapter.Invalid):
                    adapter.run_live(credential_frame(), connection_factory=lambda **unused: connection)
                self.assertEqual(connection.events, [])
                self.assertEqual(sentinel.read_bytes(), b"keep")
                self.assertEqual([p.name for p in Path(directory).iterdir()], ["http-response.bin"])
            finally:
                os.chdir(old)

    def test_cli_bad_frame_and_unknown_flags_have_empty_stderr_and_no_files(self):
        import os
        with tempfile.TemporaryDirectory() as directory:
            env = dict(os.environ, OPENAI_API_KEY="synthetic-ENV-never-selected", HTTPS_PROXY="http://127.0.0.1:1")
            for args in ([], ["--endpoint", "http://127.0.0.1:1"], ["--offline-decode", "x", "y", "200"]):
                run = subprocess.run([sys.executable, "-B", str(ADAPTER)] + args, input=b"bad frame", env=env,
                                     cwd=directory, capture_output=True, timeout=5)
                self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))
                self.assertEqual(list(Path(directory).iterdir()), [])

    def test_real_stdlib_http_boundary_handles_connection_close_response_without_second_post(self):
        captured = []

        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                captured.append((self.command, self.path, body))
                raw = response_raw()
                self.send_response(200)
                self.send_header("Content-Length", str(len(raw)))
                self.send_header("Content-Type", "application/json")
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(raw)

            def log_message(self, format, *args):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        import os
        with tempfile.TemporaryDirectory() as directory:
            old = os.getcwd()
            try:
                os.chdir(directory)
                outcome = adapter.run_live(credential_frame(), connection_factory=lambda **unused:
                                           http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=5))
                self.assertIsNotNone(outcome, "complete close-delimited connection still has readable response")
                obs = json.loads(Path("http-observation.json").read_bytes())
                self.assertEqual(obs["http_state"], "sent_once")
                self.assertEqual(len(captured), 1)
                self.assertEqual(captured[0][:2], ("POST", "/v1/responses"))
                self.assertEqual(captured[0][2], Path("http-request.json").read_bytes())
            finally:
                os.chdir(old)
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)

    def test_nested_json_unicode_credential_echo_is_never_saved(self):
        secret = b"synthetic-private-key-ONLY-TEST"
        escaped = "".join("\\u%04x" % c for c in secret)
        response = provider_response()
        response["output"][0]["content"][0]["text"] = '{"candidates":[{"unit_ordinal":0,"statement":"' + escaped + '","qualifiers":[]}]}'
        outcome, files, unused = live_test(connection=MockConnection(raw=response_raw(response)))
        self.assertIsNone(outcome)
        self.assertNotIn("http-response.bin", files, "nested JSON decoding must not hide credential echo")
        self.assertIsNone(json.loads(files["http-observation.json"])["usage_total_tokens"])

    def test_complete_live_transport_connects_first_posts_once_and_saves_safe_observation(self):
        outcome, files, connection = live_test()
        self.assertIsNotNone(outcome, "successful single POST must return canonical outcome")
        self.assertEqual(connection.events, ["connect", "request", "response", "close"])
        self.assertEqual(len(connection.requests), 1)
        method, path, body, headers = connection.requests[0]
        self.assertEqual((method, path), ("POST", "/v1/responses"))
        self.assertEqual(headers["Authorization"], b"Bearer synthetic-private-key-ONLY-TEST")
        self.assertEqual(files["http-request.json"], body)
        self.assertEqual(set(files), {"http-request.json", "http-observation.json", "http-response.bin"})
        obs = json.loads(files["http-observation.json"])
        self.assertEqual(set(obs), {"schema_version", "protocol", "request_sha256", "http_request_sha256", "endpoint",
                         "http_state", "http_status", "response_id", "actual_model", "failure", "raw_response_available",
                         "canonical_response_available", "usage_input_tokens", "usage_output_tokens", "usage_total_tokens"})
        self.assertEqual(obs["http_state"], "sent_once")
        self.assertEqual(obs["http_status"], 200)
        self.assertTrue(obs["raw_response_available"])
        self.assertTrue(obs["canonical_response_available"])
        self.assertEqual(obs["usage_total_tokens"], 130)
        self.assertIsNone(obs["failure"])
        for raw in files.values():
            self.assertNotIn(b"synthetic-private-key-ONLY-TEST", raw)

    def test_connect_send_and_read_failure_states_do_not_retry_or_save_remote_text(self):
        for stage, state in (("connect_error", "not_sent"), ("send_error", "remote_unknown"), ("read_error", "remote_unknown")):
            with self.subTest(stage=stage):
                connection = MockConnection(**{stage: socket.timeout("synthetic-private-key-ONLY-TEST")})
                outcome, files, connection = live_test(connection=connection)
                self.assertIsNone(outcome)
                self.assertIn("http-observation.json", files)
                obs = json.loads(files["http-observation.json"])
                self.assertEqual(obs["http_state"], state)
                self.assertEqual(obs["failure"], "timeout")
                self.assertFalse(obs["raw_response_available"])
                self.assertIsNone(obs["usage_total_tokens"])
                self.assertLessEqual(len(connection.requests), 1)
                for raw in files.values():
                    self.assertNotIn(b"synthetic-private-key-ONLY-TEST", raw)

    def test_redirect_is_not_followed_and_actual_usage_remains_unknown(self):
        connection = MockConnection(raw=b'{"error":{"message":"moved","type":"invalid_request_error","param":null,"code":null}}', status=302)
        outcome, files, connection = live_test(connection=connection)
        self.assertIsNone(outcome)
        self.assertEqual(len(connection.requests), 1)
        obs = json.loads(files["http-observation.json"])
        self.assertEqual((obs["http_state"], obs["http_status"], obs["failure"]), ("sent_once", 302, "remote_unknown"))
        self.assertIsNone(obs["usage_total_tokens"])

    def test_incomplete_response_retains_actual_model_and_usage_without_canonical(self):
        response = provider_response()
        response["status"] = "incomplete"
        response["incomplete_details"] = {"reason": "max_output_tokens"}
        outcome, files, unused = live_test(connection=MockConnection(raw=response_raw(response)))
        self.assertIsNone(outcome)
        obs = json.loads(files["http-observation.json"])
        self.assertEqual(obs["actual_model"], "deepseek-flash-returned-synthetic")
        self.assertEqual(obs["usage_output_tokens"], 30)
        self.assertEqual(obs["failure"], "capability_insufficient")
        self.assertFalse(obs["canonical_response_available"])
        self.assertIn("http-response.bin", files)

    def test_credential_echo_in_raw_decoded_value_or_key_suppresses_entire_response(self):
        secret = "synthetic-private-key-ONLY-TEST"
        for place in ("value", "key", "escaped"):
            response = provider_response()
            response["metadata"] = ({secret: "x"} if place == "key" else {"x": secret})
            raw = response_raw(response)
            if place == "escaped":
                escaped = "".join("\\u%04x" % ord(c) for c in secret)
                raw = raw.replace(secret.encode(), escaped.encode())
            outcome, files, unused = live_test(connection=MockConnection(raw=raw))
            self.assertIsNone(outcome)
            self.assertNotIn("http-response.bin", files)
            obs = json.loads(files["http-observation.json"])
            self.assertFalse(obs["raw_response_available"])
            self.assertIsNone(obs["response_id"])
            self.assertIsNone(obs["usage_total_tokens"])
            for saved in files.values():
                self.assertNotIn(secret.encode(), saved)

    def test_bad_frame_zero_requests_and_no_output_files(self):
        frames = [b"", credential_frame()[:-1], credential_frame() + b"x",
                  credential_frame(secret=b"bad\nkey"), credential_frame(secret=b"\xff"),
                  credential_frame(secret=b""), credential_frame(secret=b"x" * 4097)]
        for frame in frames:
            connection = MockConnection()
            with self.assertRaises(adapter.Invalid):
                live_test(frame=frame, connection=connection)
            self.assertEqual(connection.events, [])

    def test_non_success_status_does_not_create_canonical_outcome(self):
        for status in ("401", "403", "429", "500", "302", "True", "200.0", "099"):
            run = decode_cli(response_raw(), status=status)
            self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))

    def test_request_unknown_duplicate_bad_policy_or_source_hash_fails_offline(self):
        mutations = []
        request = json.loads(wire_request())
        for path, value in [(("unrecognized",), 1), (("schema_version",), True),
                            (("provider", "requested_model"), "other"), (("policy", "retries"), 1),
                            (("policy", "tools_enabled"), True), (("source", "normalized_sha256"), "0" * 64),
                            (("source", "units", 0, "start_byte"), 1)]:
            changed = copy.deepcopy(request)
            target = changed
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = value
            mutations.append(json.dumps(changed, ensure_ascii=False).encode())
        mutations.append(wire_request().replace(b'"schema_version": 1', b'"schema_version": 1,"schema_version":1'))
        for wire in mutations:
            run = decode_cli(response_raw(), wire=wire)
            self.assertEqual((run.returncode, run.stdout, run.stderr), (2, b"", b""))


if __name__ == "__main__":
    unittest.main()
