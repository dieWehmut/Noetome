"""Independent ordinary-CLI audit against a real local HTTP fake server.

No live service, real credential, model invocation or paid API call is allowed.
Author gold is loaded before the first request. Source SQL/files and saved
request/response/report bytes are inspected independently of the Rust validators.
"""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from validate_extraction_gateway import inventory, sha, sql_snapshot

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "fixtures/extraction-gateway-v1"
TEST_KEY = "noetome-test-public-credential-never-real"
ESCAPED_KEY = "".join("\\u%04x" % ord(ch) for ch in TEST_KEY)


def safe_text(text: str) -> str:
    return text.replace(TEST_KEY, "<omitted synthetic credential>").replace(
        ESCAPED_KEY, "<omitted escaped synthetic credential>"
    )


def save(path: Path, value: object) -> None:
    text = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
    assert TEST_KEY not in text and ESCAPED_KEY not in text, "audit must not archive credential echoes"
    path.write_text(text, encoding="utf-8")


def encoded(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


class LocalFake:
    def __init__(self, payload: dict, *, candidate_change=None, wire_change=None,
                 raw=None, status=200, delay_headers=0.0, delay_body=0.0,
                 truncated=False, on_request=None, duplicate_candidate=False,
                 duplicate_wire=False, trailing=False):
        self.payload = payload
        self.candidate_change = candidate_change
        self.wire_change = wire_change
        self.raw = raw
        self.status = status
        self.delay_headers = delay_headers
        self.delay_body = delay_body
        self.truncated = truncated
        self.on_request = on_request
        self.duplicate_candidate = duplicate_candidate
        self.duplicate_wire = duplicate_wire
        self.trailing = trailing
        self.receipts = []
        self.errors = []
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                    assert 0 < length <= 256 * 1024, length
                    body = self.rfile.read(length)
                    request = json.loads(body)
                    assert self.path == "/v1/responses", self.path
                    assert self.headers.get("Authorization") == "Bearer " + TEST_KEY
                    input_data = json.loads(request["input"])
                    fake.receipts.append({
                        "method": "POST", "path": self.path,
                        "authorization_matches_artificial_test_key": True,
                        "body_sha256": sha(body), "body_length": len(body),
                        "request": request,
                    })
                    if fake.on_request:
                        fake.on_request()
                    candidate = copy.deepcopy(fake.payload)
                    candidate.pop("producer")
                    candidate["request_id"] = input_data["request_id"]
                    candidate["binding"] = input_data["binding"]
                    if fake.candidate_change:
                        fake.candidate_change(candidate)
                    text = encoded(candidate).decode()
                    if fake.duplicate_candidate:
                        text = text.replace('"schema_version":1', '"schema_version":1,"schema_version":1', 1)
                    wire = {
                        "id": "resp_noetome_test", "object": "response",
                        "model": "noetome-test-model", "status": "completed",
                        "error": None, "incomplete_details": None, "service_tier": "default",
                        "output": [{
                            "id": "msg_noetome_test", "type": "message", "role": "assistant",
                            "status": "completed", "content": [{
                                "type": "output_text", "text": text,
                                "annotations": [], "logprobs": [],
                            }],
                        }],
                        "usage": {
                            "input_tokens": 100,
                            "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                            "output_tokens": 600, "output_tokens_details": {"reasoning_tokens": 0},
                            "total_tokens": 700,
                        },
                    }
                    if fake.wire_change:
                        fake.wire_change(wire)
                    response = encoded(wire) if fake.raw is None else fake.raw
                    if fake.duplicate_wire:
                        response = response.replace(b'"status":"completed"', b'"status":"completed","status":"completed"', 1)
                    if fake.trailing:
                        response += b" {}"
                    fake.receipts[-1].update(
                        response_sha256=sha(response), response_length=len(response),
                        advertised_status=fake.status,
                    )
                    if fake.delay_headers:
                        time.sleep(fake.delay_headers)
                    self.send_response(fake.status)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(response) + (17 if fake.truncated else 0)))
                    self.send_header("Connection", "close")
                    self.send_header("x-request-id", "req_noetome_test")
                    if fake.status == 302:
                        self.send_header("Location", fake.endpoint)
                    if fake.status in (429, 503):
                        self.send_header("Retry-After", "1")
                    self.end_headers()
                    if fake.delay_body:
                        time.sleep(fake.delay_body)
                    self.wfile.write(response)
                    self.wfile.flush()
                    self.close_connection = True
                except (BrokenPipeError, ConnectionResetError, OSError):
                    # Cancellation/timeout deliberately closes the client.
                    pass
                except BaseException as exc:
                    fake.errors.append(safe_text(repr(exc)))

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}/v1/responses"
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        assert not self.thread.is_alive(), "local accept loop did not stop"


class Audit:
    def __init__(self, binary: Path, root: Path):
        self.binary = binary.resolve()
        self.root = root.resolve()
        self.root.mkdir()  # must be new; never adopt/delete an existing run
        self.gold = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))
        self.payload = json.loads((FIXTURE / "response.json").read_text(encoding="utf-8"))
        self.calls = []
        self.rows = []
        self.project = self.root / "project 中文"
        self.call(["project", "init", str(self.project)], expected=0)
        imported = self.call(["source", "import", str(self.project), str(FIXTURE / "source.txt")], expected=0)
        self.imported = json.loads(imported.stdout)
        self.import_id = self.imported["import_id"]
        self.before = inventory(self.project)
        self.sql_before = sql_snapshot(self.project)
        self.injected_source_files = []

    def call(self, args: list[str], expected=None, *, credential=TEST_KEY):
        child_env = os.environ.copy()  # normal inheritance; never log or discover secret values
        child_env["NOETOME_TEST_CREDENTIAL"] = credential
        run = subprocess.run([str(self.binary), *args], capture_output=True, env=child_env, timeout=65)
        stdout, stderr = run.stdout.decode("utf-8"), run.stderr.decode("utf-8")
        self.calls.append({
            "argv": args, "exit_code": run.returncode,
            "stdout": safe_text(stdout), "stderr": safe_text(stderr),
        })
        assert TEST_KEY not in stdout + stderr and ESCAPED_KEY not in stdout + stderr, "credential leaked to terminal"
        if expected is not None:
            assert run.returncode == expected, (args, run.returncode, safe_text(stderr), safe_text(stdout[:1000]))
        return subprocess.CompletedProcess(run.args, run.returncode, stdout, stderr)

    @staticmethod
    def config(endpoint: str) -> dict:
        return {
            "schema_version": 1, "provider": "openai_responses", "transport_mode": "local_http_test",
            "endpoint": endpoint, "model": "noetome-test-model",
            "credential_env": "NOETOME_TEST_CREDENTIAL", "required_input": "text",
            "capabilities": {"text": True, "structured_output": True, "vision": False},
            "selection": [{"kind": "text", "ordinal": 0}],
            "limits": {"max_units": 1, "max_input_bytes": 4096, "max_response_bytes": 65536,
                       "max_output_tokens": 2048, "timeout_ms": 3000},
            "budget": {"max_requests": 1, "max_total_tokens": 65536, "max_cost_usd": None},
        }

    @staticmethod
    def reports(output: Path, expected: int):
        path = output / "validation.json"
        if not path.exists():
            return None
        report = json.loads(path.read_text(encoding="utf-8"))
        markdown = (output / "validation.md").read_text(encoding="utf-8")
        assert "```json\n" in markdown
        snapshot = markdown.split("```json\n", 1)[1].rsplit("\n```", 1)[0]
        assert json.loads(snapshot) == report, "JSON/Markdown audit facts differ"
        assert report["producer"] == "api" and report["transport_mode"] == "local_http_test"
        assert report["published"] is False and report["knowledge_status"] == "pending"
        assert report["scientific_quality"] == "not_run"
        assert report["usage"]["real_model_calls"] == 0
        assert report["usage"]["price_usd"] is None and report["usage"]["model_tokenizer"] is None
        for name, fingerprint in report["artifacts"].items():
            target = (output / name).resolve()
            assert target.is_relative_to(output.resolve()) and target.is_file()
            raw = target.read_bytes()
            assert fingerprint == {"sha256": sha(raw), "byte_length": len(raw)}
        assert report["validated_atoms"] == (1 if expected == 0 else 0)
        if expected == 0:
            assert report["status"] == "candidates_validated"
        else:
            assert report["status"] == "failed"
        return report

    def gold_and_body(self, output: Path, receipt: dict, *, project=None, imported=None, ordinal=0) -> None:
        project = self.project if project is None else project
        imported = self.imported if imported is None else imported
        body = receipt["request"]
        assert body["model"] == "noetome-test-model"
        assert body["store"] is False and body["stream"] is False and body["background"] is False
        assert body["tools"] == [] and body["tool_choice"] == "none"
        assert body["max_output_tokens"] == 2048 and body["service_tier"] == "default"
        assert body["truncation"] == "disabled"
        assert body["text"]["format"]["type"] == "json_schema"
        assert body["text"]["format"]["strict"] is True

        def schema(node):
            if isinstance(node, dict):
                if node.get("type") == "object" or (
                    isinstance(node.get("type"), list) and "object" in node["type"]
                ):
                    assert node["additionalProperties"] is False
                    assert set(node["required"]) == set(node["properties"])
                for value in node.values():
                    schema(value)
            elif isinstance(node, list):
                for value in node:
                    schema(value)
        schema(body["text"]["format"]["schema"])
        wire_bytes = (output / "request-body.json").read_bytes()
        assert sha(wire_bytes) == receipt["body_sha256"] and len(wire_bytes) == receipt["body_length"]
        assert json.loads(wire_bytes) == body
        input_data = json.loads(body["input"])
        version = imported["version"]
        binding = input_data["binding"]
        for field in ("version_id", "source_id", "content_sha256", "normalized_sha256"):
            assert binding[field] == version[field]
        assert binding["import_id"] == imported["import_id"] and binding["parse_id"] is None
        normalized = (project / version["normalized_snapshot"]).read_bytes()
        assert sha(normalized) == binding["normalized_sha256"]
        span = imported["spans"][ordinal]
        unit = input_data["units"][0]
        assert unit["locator"]["span_id"] == span["span_id"]
        assert unit["text"] == normalized[span["start_byte"]:span["end_byte"]].decode()
        assert unit["locator"]["resource_sha256"] == sha(normalized)
        assert unit["start_byte"] == span["start_byte"] and unit["end_byte"] == span["end_byte"]
        anchor = input_data["evidence_anchors"][0]
        assert anchor["excerpt_start_byte"] == unit["start_byte"]
        assert anchor["excerpt_end_byte"] == unit["end_byte"]
        assert anchor["quote"] == unit["text"] and anchor["excerpt_sha256"] == sha(unit["text"].encode())
        candidates = json.loads((output / "candidates.json").read_text(encoding="utf-8"))["payload"]
        assert candidates["request_id"] == input_data["request_id"] and candidates["binding"] == binding
        assert candidates["concepts"][0]["canonical_name"] == self.gold["concept_name"]
        atom, measurement = candidates["atoms"][0]["atom"], candidates["atoms"][0]["measurement"]
        assert atom["content"] == self.gold["atom_content"]
        assert atom["required"] is False and atom["verification"] == "unverified"
        assert measurement["value"] == self.gold["value"] and measurement["unit"] == self.gold["unit"]
        assert measurement["conditions"] == self.gold["conditions"]
        evidence = candidates["evidence"][0]
        excerpt = normalized[evidence["excerpt_start_byte"]:evidence["excerpt_end_byte"]]
        assert evidence["quote"] == self.gold["quote"] == excerpt.decode()
        assert evidence["excerpt_sha256"] == sha(excerpt)

    def case(self, name: str, *, expected=3, code=None, calls=1, config_change=None,
             config_raw=None, server=None, cancel_before=False, cancel_during=False,
             output_existing=False, output_in_source=False, source_injection=False,
             output_injection=False, credential=TEST_KEY, check_gold=False,
             metadata=None, omitted=False, extra_args=(), project=None, import_id=None,
             gold_context=None):
        case_root = self.root / f"case-{name}"
        case_root.mkdir()
        output = (self.project / f"forbidden-{name}") if output_in_source else case_root / "new output"
        if output_existing:
            output.mkdir()
            (output / "human.txt").write_bytes(b"existing human content")
        old_output = inventory(output) if output.exists() else None
        signal = case_root / "cancel.signal"
        if cancel_before:
            signal.write_bytes(b"stop")
        project = self.project if project is None else project
        import_id = self.import_id if import_id is None else import_id
        source_before = inventory(project)
        source_root_mtime_before = project.stat().st_mtime_ns
        sql_before = sql_snapshot(project)
        scene = {}

        def action():
            if cancel_during:
                signal.write_bytes(b"stop")
            if source_injection:
                target = project / f"human-{name}.txt"
                target.write_bytes(b"new human source scene")
                scene["source_file"] = target.name
                self.injected_source_files.append(target.name)
            if output_injection:
                target = output / "human-late.txt"
                target.write_bytes(b"new human output scene")
                scene["output_file"] = target.name

        server_options = {} if server is None else dict(server)
        if cancel_during or source_injection or output_injection:
            server_options["on_request"] = action
        with LocalFake(self.payload, **server_options) as fake:
            config = self.config(fake.endpoint)
            if config_change:
                config_change(config)
            config_file = case_root / "api-config.json"
            if config_raw is None:
                save(config_file, config)
            else:
                config_file.write_bytes(config_raw(config))
            config_before = config_file.read_bytes()
            config_mtime = config_file.stat().st_mtime_ns
            args = ["extract", "api", str(project), import_id, str(config_file), str(output)]
            if cancel_before or cancel_during:
                args += ["--cancel-file", str(signal)]
            args += list(extra_args)
            started = time.monotonic()
            run = self.call(args, expected, credential=credential)
            elapsed = time.monotonic() - started
        assert not fake.errors, (name, fake.errors)
        assert len(fake.receipts) == calls, (name, calls, fake.receipts)
        assert config_file.read_bytes() == config_before and config_file.stat().st_mtime_ns == config_mtime
        sql_after = sql_snapshot(project)
        source_after = inventory(project)
        source_root_mtime_after = project.stat().st_mtime_ns
        assert sql_after == sql_before, "API changed source SQL"
        expected_source = dict(source_before)
        if source_injection:
            assert (project / scene["source_file"]).read_bytes() == b"new human source scene"
            source_without_scene = dict(source_after)
            source_without_scene.pop(scene["source_file"])
            assert source_without_scene == source_before, "registered source changed during injected scene"
        else:
            assert source_after == expected_source, "source bytes/mtime/inventory changed"
            assert source_root_mtime_after == source_root_mtime_before, "source root mtime changed"
        if old_output is not None:
            assert inventory(output) == old_output, "existing output changed"
        if output_injection:
            assert (output / scene["output_file"]).read_bytes() == b"new human output scene"
        if code:
            assert code in run.stdout + run.stderr, (name, code, safe_text(run.stdout[:1600]), safe_text(run.stderr))
        if expected != 0:
            assert not (output / "candidates.json").exists(), name
        report = self.reports(output, expected)
        if report is not None:
            assert json.loads(run.stdout) == report
            assert report["usage"]["network_attempts"] == calls
            if metadata is not None:
                assert (report["usage"]["model_usage"] is not None) == metadata, name
            if omitted:
                assert not (output / "response.bin").exists(), "credential echo response was persisted"
                assert "omitted_credential_echo" in json.dumps(report), name
            if check_gold:
                self.gold_and_body(
                    output, fake.receipts[0], project=project,
                    imported=None if gold_context is None else gold_context[0],
                    ordinal=0 if gold_context is None else gold_context[1],
                )
                assert (output / "response.bin").read_bytes()
            if calls and (output / "response.bin").exists():
                actual = (output / "response.bin").read_bytes()
                if report.get("response_complete") is True:
                    assert sha(actual) == fake.receipts[0]["response_sha256"]
                    assert len(actual) == fake.receipts[0]["response_length"]
        for path in output.rglob("*") if output.exists() else []:
            if path.is_file():
                raw = path.read_bytes()
                assert TEST_KEY.encode() not in raw and ESCAPED_KEY.encode() not in raw
        if cancel_during or server_options.get("delay_headers") or server_options.get("delay_body"):
            assert elapsed < 2.5, (name, elapsed)  # bounded smoke margin, not a performance benchmark
        row = {
            "name": name, "expected_exit": expected, "observed_exit": run.returncode,
            "local_post_count": len(fake.receipts), "elapsed_seconds": round(elapsed, 3),
            "config_sha256_before": sha(config_before), "config_sha256_after": sha(config_file.read_bytes()),
            "config_mtime_before": config_mtime, "config_mtime_after": config_file.stat().st_mtime_ns,
            "source_inventory_sha256_before": sha(encoded(source_before)),
            "source_inventory_sha256_after": sha(encoded(source_after)),
            "source_root_mtime_before": source_root_mtime_before,
            "source_root_mtime_after": source_root_mtime_after,
            "source_sql_sha256_before": sha(encoded(sql_before)), "source_sql_sha256_after": sha(encoded(sql_after)),
            "controlled_scene": scene, "server_receipts": fake.receipts, "passed": True,
        }
        self.rows.append(row)
        return output

    def nonzero_source(self):
        prefix = "本段仅为工程前言。\n\n"
        quote = self.gold["quote"]
        source = self.root / "nonzero-source.txt"
        source.write_bytes((prefix + quote).encode())
        project = self.root / "nonzero project"
        self.call(["project", "init", str(project)], expected=0)
        imported_run = self.call(["source", "import", str(project), str(source)], expected=0)
        imported = json.loads(imported_run.stdout)
        start = len(prefix.encode())
        assert imported["spans"][1]["start_byte"] == start

        def absolute_evidence(candidate):
            candidate["evidence"][0].update(
                excerpt_start_byte=start, excerpt_end_byte=start + len(quote.encode())
            )
        self.case(
            "nonzero-source-span", expected=0, metadata=True, check_gold=True,
            config_change=lambda c: c.update(selection=[{"kind": "text", "ordinal": 1}]),
            server={"candidate_change": absolute_evidence}, project=project,
            import_id=imported["import_id"], gold_context=(imported, 1),
        )

    def request_secret(self):
        source = self.root / "public-synthetic-credential-source.txt"
        source.write_bytes(("Only a public artificial marker: " + TEST_KEY).encode())
        project = self.root / "credential source project"
        self.call(["project", "init", str(project)], expected=0)
        imported = json.loads(self.call(
            ["source", "import", str(project), str(source)], expected=0
        ).stdout)
        self.case(
            "credential-in-request", code="EXTRACT_API_CREDENTIAL_ECHO", calls=0,
            project=project, import_id=imported["import_id"],
        )

    def matrix(self):
        self.case("valid", expected=0, check_gold=True, metadata=True)
        self.case("replay", expected=0, check_gold=True, metadata=True)
        self.nonzero_source()
        for status, code in [
            (400, "EXTRACT_API_HTTP_STATUS"), (401, "EXTRACT_API_HTTP_AUTH"),
            (403, "EXTRACT_API_HTTP_AUTH"), (429, "EXTRACT_API_HTTP_RATE_LIMIT"),
            (500, "EXTRACT_API_HTTP_SERVER"), (503, "EXTRACT_API_HTTP_SERVER"),
            (302, "EXTRACT_API_HTTP_STATUS"),
        ]:
            self.case(f"http-{status}", code=code, server={"status": status}, metadata=False)
        self.case("truncated-http", code="EXTRACT_API_TRUNCATED", server={"truncated": True}, metadata=False)
        self.case("empty-auth", code="EXTRACT_API_HTTP_AUTH", server={"status": 401, "raw": b""}, metadata=False)
        self.case("empty-rate-limit", code="EXTRACT_API_HTTP_RATE_LIMIT", server={"status": 429, "raw": b""}, metadata=False)
        self.case("empty-body", code="EXTRACT_API_RESPONSE", server={"raw": b""}, metadata=False)
        self.case("bad-json", code="EXTRACT_API_RESPONSE", server={"raw": b"not JSON"}, metadata=False)
        self.case("duplicate-wire", code="EXTRACT_API_RESPONSE", server={"duplicate_wire": True}, metadata=False)
        self.case("trailing-wire", code="EXTRACT_API_RESPONSE", server={"trailing": True}, metadata=False)
        self.case("duplicate-candidate", code="EXTRACT_API_CANDIDATES", server={"duplicate_candidate": True}, metadata=True)
        self.case("candidate-extra", code="EXTRACT_API_CANDIDATES",
                  server={"candidate_change": lambda p: p.update(extra=True)}, metadata=True)
        self.case("candidate-version", code="EXTRACT_API_CANDIDATES",
                  server={"candidate_change": lambda p: p.update(schema_version=2)}, metadata=True)
        self.case("missing-nullable", code="EXTRACT_API_CANDIDATES",
                  server={"candidate_change": lambda p: p["atoms"][0].pop("measurement")}, metadata=True)
        self.case("bad-output-id", code="EXTRACT_API_RESPONSE", metadata=True,
                  server={"wire_change": lambda w: w["output"][0].update(id=None)})
        self.case("bad-output-summary", code="EXTRACT_API_RESPONSE", metadata=True,
                  server={"wire_change": lambda w: w["output"][0].update(summary="bad")})
        self.case("bad-output-annotations", code="EXTRACT_API_RESPONSE", metadata=True,
                  server={"wire_change": lambda w: w["output"][0]["content"][0].update(annotations={})})
        self.case("wrong-binding", code="EXTRACT_SOURCE_BINDING",
                  server={"candidate_change": lambda p: p["binding"].update(version_id=str(uuid.uuid4()))}, metadata=True)
        self.case("wrong-quote", code="EXTRACT_EVIDENCE_QUOTE",
                  server={"candidate_change": lambda p: p["evidence"][0].update(quote="wrong quote")}, metadata=True)
        self.case("chinese-boundary", code="EXTRACT_EVIDENCE_RANGE",
                  server={"candidate_change": lambda p: p["evidence"][0].update(excerpt_start_byte=1)}, metadata=True)
        self.case("wrong-sha", code="EXTRACT_EVIDENCE_SHA",
                  server={"candidate_change": lambda p: p["evidence"][0].update(excerpt_sha256="0" * 64)}, metadata=True)
        self.case("claimed-review", code="EXTRACT_VERIFICATION",
                  server={"candidate_change": lambda p: p["atoms"][0]["atom"].update(required=True)}, metadata=True)
        self.case("missing-condition", code="EXTRACT_QUALIFIERS",
                  server={"candidate_change": lambda p: p["atoms"][0]["atom"]["qualifiers"].remove("温度 20 ℃")}, metadata=True)
        self.case("refusal", code="EXTRACT_API_REFUSAL", metadata=True,
                  server={"wire_change": lambda w: w["output"][0].update(content=[{"type": "refusal", "refusal": "local fake refusal"}])})
        self.case("incomplete", code="EXTRACT_API_INCOMPLETE", metadata=True,
                  server={"wire_change": lambda w: w.update(status="incomplete", incomplete_details={"reason": "max_output_tokens"})})
        self.case("extra-text", code="EXTRACT_API_OUTPUT", metadata=True,
                  server={"wire_change": lambda w: w["output"][0]["content"].append({"type": "output_text", "text": "{}"})})
        self.case("tool-output", code="EXTRACT_API_OUTPUT", metadata=True,
                  server={"wire_change": lambda w: w["output"].append({"type": "function_call", "name": "forbidden"})})
        self.case("reasoning-metadata", expected=0, check_gold=True, metadata=True,
                  server={"wire_change": lambda w: w["output"].insert(0, {"id": "rs_noetome_test", "type": "reasoning", "summary": []})})
        self.case("future-metadata", expected=0, check_gold=True, metadata=True,
                  server={"wire_change": lambda w: w.update(future_metadata={"safe": True})})
        self.case("missing-usage", code="EXTRACT_API_USAGE", metadata=False,
                  server={"wire_change": lambda w: w.pop("usage")})
        self.case("usage-sum", code="EXTRACT_API_USAGE", metadata=False,
                  server={"wire_change": lambda w: w["usage"].update(total_tokens=701)})
        self.case("usage-negative", code="EXTRACT_API_USAGE", metadata=False,
                  server={"wire_change": lambda w: w["usage"].update(input_tokens=-1)})
        self.case("usage-fraction", code="EXTRACT_API_USAGE", metadata=False,
                  server={"wire_change": lambda w: w["usage"].update(output_tokens=600.5)})
        self.case("usage-detail", code="EXTRACT_API_USAGE", metadata=False,
                  server={"wire_change": lambda w: w["usage"]["input_tokens_details"].update(cache_write_tokens=-1)})
        self.case("usage-no-details", expected=0, metadata=True,
                  server={"wire_change": lambda w: (w["usage"].pop("input_tokens_details"), w["usage"].pop("output_tokens_details"))})
        self.case("usage-over-budget", code="EXTRACT_BUDGET_STOP", metadata=True,
                  server={"wire_change": lambda w: w["usage"].update(input_tokens=1000000, total_tokens=1000600)})
        self.case("output-over-budget", code="EXTRACT_BUDGET_STOP", metadata=True,
                  config_change=lambda c: c["limits"].update(max_output_tokens=16),
                  server={"wire_change": lambda w: w["usage"].update(output_tokens=17, total_tokens=117)})
        self.case("zero-requests", code="EXTRACT_BUDGET_STOP", calls=0,
                  config_change=lambda c: c["budget"].update(max_requests=0), metadata=False)
        self.case("reserve-stop", code="EXTRACT_BUDGET_STOP", calls=0,
                  config_change=lambda c: c["budget"].update(max_total_tokens=2048), metadata=False)
        self.case("unknown-price", code="EXTRACT_PRICE_UNKNOWN", calls=0,
                  config_change=lambda c: c["budget"].update(max_cost_usd="0.01"), metadata=False)
        for capability in ("text", "structured_output"):
            self.case(f"missing-{capability}", code="EXTRACT_API_CAPABILITY", calls=0,
                      config_change=lambda c, key=capability: c["capabilities"].update({key: False}), metadata=False)
        self.case("vision-request", code="EXTRACT_API_CAPABILITY", calls=0,
                  config_change=lambda c: c.update(required_input="vision"), metadata=False)
        self.case("input-limit", code="EXTRACT_INPUT_LIMIT", calls=0,
                  config_change=lambda c: c["limits"].update(max_input_bytes=4))
        self.case("response-limit", code="EXTRACT_RESPONSE_LIMIT",
                  config_change=lambda c: c["limits"].update(max_response_bytes=128), metadata=False)
        self.case("timeout-headers", code="EXTRACT_TIMEOUT",
                  config_change=lambda c: c["limits"].update(timeout_ms=200),
                  server={"delay_headers": 1.0}, metadata=False)
        self.case("timeout-body", code="EXTRACT_TIMEOUT",
                  config_change=lambda c: c["limits"].update(timeout_ms=200),
                  server={"delay_body": 1.0}, metadata=False)
        self.case("cancel-running", code="EXTRACT_CANCELLED", cancel_during=True,
                  server={"delay_headers": 1.0}, metadata=False)
        self.case("cancel-existing", code="EXTRACT_CANCELLED", cancel_before=True, calls=0)
        for name, change in [
            ("unknown-config-field", lambda c: c.update(secret="refused extra property")),
            ("unknown-provider", lambda c: c.update(provider="not_implemented")),
            ("unknown-mode", lambda c: c.update(transport_mode="guessed_compatible")),
            ("unknown-version", lambda c: c.update(schema_version=2)),
            ("invalid-output-minimum", lambda c: c["limits"].update(max_output_tokens=1)),
            ("requests-over-one", lambda c: c["budget"].update(max_requests=2)),
            ("credential-ref", lambda c: c.update(credential_env="invalid-name")),
        ]:
            self.case(name, expected=2, code="EXTRACT_API_CONFIG", calls=0, config_change=change)
        for name, change in [
            ("named-loopback", lambda c: c.update(endpoint=c["endpoint"].replace("127.0.0.1", "localhost"))),
            ("non-loopback", lambda c: c.update(endpoint="http://192.0.2.1:1/v1/responses")),
            ("query", lambda c: c.update(endpoint=c["endpoint"] + "?credential=forbidden")),
            ("userinfo", lambda c: c.update(endpoint=c["endpoint"].replace("http://", "http://name@"))),
            ("fragment", lambda c: c.update(endpoint=c["endpoint"] + "#fragment")),
            ("live-other-host", lambda c: c.update(transport_mode="live", endpoint="https://example.invalid/v1/responses")),
        ]:
            self.case(name, expected=2, code="EXTRACT_API_ENDPOINT", calls=0, config_change=change)
        self.case("duplicate-config", expected=2, code="EXTRACT_API_CONFIG", calls=0,
                  config_raw=lambda c: encoded(c).replace(b'"schema_version":1', b'"schema_version":1,"schema_version":1', 1))
        self.case("missing-config-nullable", expected=2, code="EXTRACT_API_CONFIG", calls=0,
                  config_change=lambda c: c["budget"].pop("max_cost_usd"))
        self.case("oversize-config", expected=2, code="EXTRACT_API_CONFIG", calls=0,
                  config_raw=lambda c: encoded(c) + b" " * 65536)
        self.case("missing-credential", expected=2, code="EXTRACT_API_CREDENTIAL", calls=0,
                  config_change=lambda c: c.update(credential_env="NOETOME_TEST_ABSENT_" + uuid.uuid4().hex.upper()))
        self.case("real-key-to-local-refused", expected=2, code="EXTRACT_API_CREDENTIAL", calls=0,
                  credential="public-invalid-credential-without-required-test-prefix")
        self.request_secret()
        self.case("echo-plain", code="EXTRACT_API_CREDENTIAL_ECHO", omitted=True,
                  server={"wire_change": lambda w: w.update(debug=TEST_KEY)})
        echo = ('{"error":{"message":"' + ESCAPED_KEY + '"}}').encode()
        self.case("echo-escaped", code="EXTRACT_API_CREDENTIAL_ECHO", omitted=True,
                  server={"raw": echo, "status": 401})
        self.case("echo-escaped-malformed", code="EXTRACT_API_CREDENTIAL_ECHO", omitted=True,
                  server={"raw": echo[:-1] + b"]", "status": 500})
        def nested_echo(wire):
            text = wire["output"][0]["content"][0]["text"]
            replacement = '"canonical_name":"' + ESCAPED_KEY + '"'
            wire["output"][0]["content"][0]["text"] = text.replace(
                '"canonical_name":"样例 A 速度"', replacement, 1
            )
        self.case("echo-nested-json", code="EXTRACT_API_CREDENTIAL_ECHO", omitted=True,
                  server={"wire_change": nested_echo})
        self.case("existing-output", expected=2, code="EXTRACT_OUTPUT_EXISTS", calls=0, output_existing=True)
        self.case("output-in-source", expected=2, code="EXTRACT_OUTPUT_OVERLAP", calls=0, output_in_source=True)
        self.case("extra-cli-argument", expected=2, code="CLI_INVALID_ARGUMENT", calls=0, extra_args=("--unknown",))
        self.case("source-human-scene", code="EXTRACT_SOURCE_CHANGED", source_injection=True)
        self.case("output-human-scene", code="EXTRACT_OUTPUT_CHANGED", output_injection=True)

    def finish(self):
        assert sql_snapshot(self.project) == self.sql_before
        after = inventory(self.project)
        registered_after = dict(after)
        for name in self.injected_source_files:
            registered_after.pop(name)
        assert registered_after == self.before, "registered source changed"
        save(self.root / "audit.json", {
            "schema_version": 1, "producer": "local_http_test", "real_model_calls": 0, "paid_calls": 0,
            "binary_sha256": sha(self.binary.read_bytes()),
            "author_gold_sha256": sha((FIXTURE / "expected.json").read_bytes()),
            "checks": self.rows, "calls": self.calls,
            "source_inventory_before": self.before, "source_inventory_after": after,
            "controlled_source_scene_files": self.injected_source_files,
            "source_sql_before": self.sql_before, "source_sql_after": sql_snapshot(self.project),
            "scientific_quality": "not_run",
        })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--red", action="store_true")
    args = parser.parse_args()
    audit = Audit(args.binary, args.root)
    try:
        audit.matrix()
        assert not args.red, "RED baseline unexpectedly implements the API contract"
        audit.finish()
        print(f"{len(audit.rows)} independent CLI/local HTTP checks passed; no real model or paid calls")
    except BaseException as exc:
        save(audit.root / "failure-evidence.json", {
            "expected_red": args.red, "error": safe_text(str(exc)),
            "checks": audit.rows, "calls": audit.calls,
            "source_inventory": inventory(audit.project),
            "source_sql": sql_snapshot(audit.project), "real_model_calls": 0,
        })
        if (args.red and audit.calls[-1]["argv"][:2] == ["extract", "api"]
                and audit.calls[-1]["exit_code"] == 2
                and "CLI_INVALID_ARGUMENT" in audit.calls[-1]["stderr"]):
            print("RED observed: ordinary CLI has no extract api; author gold predates implementation")
            return
        raise


if __name__ == "__main__":
    main()
