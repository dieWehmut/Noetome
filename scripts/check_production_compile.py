#!/usr/bin/env python3
"""Real CLI/HTTP blackbox against an explicitly artificial, loopback-only model.

No local model profile or credential file is read. Every source PDF and ledger is
authored in a new temporary directory, retained for evidence unless --cleanup.
"""
from __future__ import annotations

import argparse
import contextlib
import dataclasses
import datetime
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import time

FAKE_ENV = "NOETOME_TEST_COMPILE_BLACKBOX_KEY"
FAKE_KEY = "noetome-test-only-artificial-credential"
FAKE_MODEL = "noetome-test-production-compile"
USAGE = 150
OUTPUT_TOKENS = 256
CHUNK_BYTES = 256
SCENARIOS = ("success", "two_runs", "pause", "kill", "near_limit", "empty", "malformed", "auth", "timeout", "cancel")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf8")


def save(path: Path, value) -> None:
    with path.open("xb") as stream:
        stream.write(json_bytes(value))


def authored_pdf(pages=4, lines=10, blank_last=True) -> bytes:
    """Minimal valid PDF with a WinAnsi accented character and known page tree."""
    count = pages + int(blank_last)
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"", b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"]
    page_ids = []
    for number in range(1, count + 1):
        page_id = len(objects) + 1
        page_ids.append(page_id)
        objects.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {page_id+1} 0 R >>".encode())
        body = bytearray(b"BT /F1 10 Tf 36 756 Td 14 TL\n")
        if number <= pages:
            for line in range(1, lines + 1):
                text = f"Page {number} line {line}: Diffusion follows a concentration gradient. Caf\xe9 is an authored label."
                literal = text.encode("latin1").replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")
                body.extend(b"(" + literal + b") Tj T*\n")
        body.extend(b"ET\n")
        objects.append(f"<< /Length {len(body)} >>\nstream\n".encode() + body + b"endstream")
    objects[1] = f"<< /Type /Pages /Count {count} /Kids [{' '.join(f'{n} 0 R' for n in page_ids)}] >>".encode()
    output = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for ordinal, body in enumerate(objects, 1):
        offsets.append(len(output))
        output.extend(f"{ordinal} 0 obj\n".encode() + body + b"\nendobj\n")
    xref = len(output)
    output.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        output.extend(f"{offset:010d} 00000 n \n".encode())
    output.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
    return bytes(output)


def fake_candidates(body: dict, empty=False) -> dict:
    user = json.loads(body["messages"][1]["content"])
    request = user["request"]
    unit = request["unit"]
    candidates = {"schema_version": 1, "request_id": request["request_id"], "binding": request["binding"], "concepts": [], "evidence": [], "atoms": []}
    if not empty:
        contexts = request["existing_concepts"]
        concept = contexts[0]["concept"] if contexts else {"concept_id": "local_concept", "revision": 1, "canonical_name": "Authored diffusion", "aliases": [], "needs_full_explanation": True}
        candidates.update(concepts=[concept], evidence=[{"evidence_id": "local_evidence", "unit_id": unit["unit_id"], **unit["evidence_anchor"], "quote": unit["text"]}], atoms=[{"atom": {"atom_id": "local_atom", "revision": 1, "kind": "definition", "concept_ids": [concept["concept_id"]], "content": unit["text"], "qualifiers": [], "evidence_links": [{"evidence_id": "local_evidence", "relation": "supports"}], "required": False, "verification": "unverified", "value_status": "not_applicable"}, "measurement": None}])
    return candidates


def chat_response(body: dict, empty=False, malformed=False) -> bytes:
    content = "{ this is not JSON" if malformed else json.dumps(fake_candidates(body, empty), ensure_ascii=False)
    return json.dumps({"id": "chatcmpl-explicit-fake", "object": "chat.completion", "model": FAKE_MODEL, "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": content}}], "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": USAGE}}, ensure_ascii=False).encode("utf8")


class FakeChat:
    def __init__(self, output: Path, mode="good", fault_at=2):
        self.output = output
        output.mkdir()
        self.mode, self.fault_at = mode, fault_at
        self.records = []
        self.condition = threading.Condition()
        self.release = threading.Event()
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                if self.path != "/v1/chat/completions":
                    self.send_error(404)
                    return
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 256 * 1024:
                    self.send_error(413)
                    return
                raw = self.rfile.read(length)
                body = json.loads(raw)
                request = json.loads(body["messages"][1]["content"])["request"]
                with owner.condition:
                    ordinal = len(owner.records) + 1
                    record = {"ordinal": ordinal, "received_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "body_sha256": sha(raw), "body_bytes": len(raw), "request": request, "auth_valid": self.headers.get("Authorization") == "Bearer " + FAKE_KEY}
                    owner.records.append(record)
                    (owner.output / f"request-{ordinal:04d}.bin").write_bytes(raw)
                    save(owner.output / f"request-{ordinal:04d}.json", record)
                    owner.condition.notify_all()
                mode = owner.mode if ordinal == owner.fault_at else "good"
                if mode == "delay":
                    owner.release.wait(30)
                if mode == "auth":
                    status, response = 401, b'{"error":{"type":"authentication_error","message":"explicit fake refusal"}}'
                else:
                    status, response = 200, chat_response(body, empty=mode == "empty", malformed=mode == "malformed")
                (owner.output / f"response-{ordinal:04d}.bin").write_bytes(response)
                try:
                    self.send_response(status)
                    self.send_header("Content-Type", "application/json; charset=utf-8")
                    self.send_header("Content-Length", str(len(response)))
                    self.end_headers()
                    self.wfile.write(response)
                except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                    pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def endpoint(self):
        return f"http://127.0.0.1:{self.server.server_port}/v1/chat/completions"

    def __enter__(self):
        self.thread.start()
        return self

    def wait_requests(self, count, timeout=20):
        with self.condition:
            if not self.condition.wait_for(lambda: len(self.records) >= count, timeout):
                raise AssertionError(f"Fake server expected {count} requests; received {len(self.records)}")

    def __exit__(self, *_):
        self.release.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(5)


@dataclasses.dataclass
class OwnedProcess:
    process: subprocess.Popen
    stdout: Path
    stderr: Path
    command: list[str]


class Blackbox:
    def __init__(self, binary: Path, pdf_python: Path, output: Path, previous_evidence: Path | None = None):
        self.binary, self.pdf_python = binary.resolve(), pdf_python.absolute()
        self.started_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
        self.binary_sha = sha(self.binary.read_bytes())
        self.previous_evidence = None
        if previous_evidence is not None:
            prior = previous_evidence.resolve()
            self.previous_evidence = {"directory": str(prior), "inputs_sha256": sha((prior / "inputs.json").read_bytes()), "summary_sha256": sha((prior / "summary.json").read_bytes()), "relation": "Fresh execution of the same deterministic authored PDF scenarios after preserving the prior RED evidence; source hashes may be compared independently"}
        self.output = output.absolute()
        output.mkdir(parents=True, exist_ok=False)
        self.temp = Path(tempfile.mkdtemp(prefix="noetome-production-compile-blackbox-"))
        self.env = os.environ.copy()
        for name in ("NOETOME_TANGMAO_API_KEY", "NOETOME_API_KEY", "NOETOME_OCR_PROFILE", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
            self.env.pop(name, None)
        self.env.update({FAKE_ENV: FAKE_KEY, "NOETOME_PDF_PYTHON": str(self.pdf_python), "NO_PROXY": "127.0.0.1,localhost", "no_proxy": "127.0.0.1,localhost"})
        self.checks, self.results = [], []
        self.command_number = 0
        self.first_reservation = None
        save(output / "inputs.json", {"scope": "Explicit fake model, authored PDF, temporary artificial external ledger; zero external HTTP", "started_at_utc": self.started_at, "binary": str(self.binary), "binary_sha256": self.binary_sha, "harness_sha256": sha(Path(__file__).read_bytes()), "pdf_python": str(self.pdf_python), "temporary_root": str(self.temp), "scenarios": list(SCENARIOS), "authored_pdf_sha256": {"success_and_near_limit": sha(authored_pdf(4, 10)), "other_scenarios": sha(authored_pdf(2, 8))}, "previous_evidence": self.previous_evidence})

    def check(self, name, condition, details=None):
        self.checks.append({"name": name, "passed": bool(condition), "details": details})
        if not condition:
            raise AssertionError(name + (": " + str(details) if details else ""))

    def call(self, *arguments, expect_ok=True, timeout=120):
        self.command_number += 1
        command = [str(self.binary), *map(str, arguments)]
        process = subprocess.run(command, env=self.env, capture_output=True, timeout=timeout)
        return self.record(command, process.returncode, process.stdout, process.stderr, expect_ok)

    def record(self, command, code, stdout, stderr, expect_ok=True):
        stem = f"command-{self.command_number:04d}"
        (self.output / (stem + ".stdout")).write_bytes(stdout)
        (self.output / (stem + ".stderr")).write_bytes(stderr)
        try:
            value = json.loads(stdout)
        except (ValueError, UnicodeError):
            value = None
        save(self.output / (stem + ".json"), {"command": command, "exit_code": code, "stdout_sha256": sha(stdout), "stderr_sha256": sha(stderr)})
        if expect_ok and code != 0:
            raise AssertionError(f"CLI exit {code}: {' '.join(command[:2])}: {stderr.decode('utf8', 'replace')[:1000]}")
        return value if expect_ok else {"exit_code": code, "value": value, "stderr": stderr.decode("utf8", "replace")}

    def spawn(self, *arguments):
        self.command_number += 1
        command = [str(self.binary), *map(str, arguments)]
        stdout, stderr = self.output / f"command-{self.command_number:04d}.stdout", self.output / f"command-{self.command_number:04d}.stderr"
        with stdout.open("xb") as out, stderr.open("xb") as err:
            process = subprocess.Popen(command, env=self.env, stdout=out, stderr=err)
        return OwnedProcess(process, stdout, stderr, command)

    def finish(self, owned, expect_ok=True, timeout=20):
        code = owned.process.wait(timeout)
        # Commands invoked during a spawned process each receive their own index.
        self.command_number += 1
        return self.record(owned.command, code, owned.stdout.read_bytes(), owned.stderr.read_bytes(), expect_ok)

    def fixture(self, name, server, cap=1_000_000, opening=100, pages=2, lines=8, timeout=3000):
        root = self.temp / name
        root.mkdir()
        original = root / "authored.pdf"
        original.write_bytes(authored_pdf(pages, lines))
        project = root / "project"
        self.call("project", "init", project)
        imported = self.call("corpus", "import", project, original)
        source_id = imported["source_id"]
        parsed = self.call("corpus", "process", project, source_id)
        self.check(name + ".real_pdf_page_count", parsed["page_count"] == pages + 1, parsed)
        ledger = root / "artificial.usage.json"
        baseline = {"schema_version": 1, "approved_total_tokens": cap, "accounted_tokens": opening, "pending_reserved_tokens": 0, "remaining_tokens": cap - opening, "model_attempts": 1, "records": [{"invocation_id": "authored-opening", "reservation_tokens": max(opening, 1), "confirmed_total_tokens": opening, "network_attempts": 1, "status": "settled_valid_usage_text", "reason": "Artificial historical consumption, never a real account"}], "updated_at": "2026-11-16T00:00:00Z"}
        save(ledger, baseline)
        selection, config = root / "selection.json", root / "config.json"
        save(selection, [source_id])
        save(config, {"schema_version": 1, "endpoint": server.endpoint, "model": FAKE_MODEL, "credential_env": FAKE_ENV, "transport_mode": "local_http_test", "usage_ledger": str(ledger), "max_chunk_bytes": CHUNK_BYTES, "max_output_tokens": OUTPUT_TOKENS, "request_timeout_ms": timeout})
        created = self.call("compile", "create", project, selection, config)
        self.check(name + ".opening_preserved", created["budget"]["accounted_tokens"] == opening and created["budget"]["history_count"] == 1)
        return {"root": root, "project": project, "ledger": ledger, "selection": selection, "config": config, "source_id": source_id, "run_id": created["run_id"], "original": original, "original_sha": sha(original.read_bytes()), "baseline_sha": sha(ledger.read_bytes()), "opening": opening}

    def status(self, fixture):
        return self.call("compile", "status", fixture["project"], fixture["run_id"])

    def run_cli(self, fixture):
        result = self.call("compile", "run", fixture["project"], fixture["run_id"], expect_ok=False)
        value = result["value"]
        if result["exit_code"] not in (0, 3) or not isinstance(value, dict) or value.get("run_id") != fixture["run_id"]:
            raise AssertionError("CLI did not return a durable RunSummary: " + str(result))
        return value

    def query(self, fixture, kind, limit=1, search=""):
        offset, all_items = 0, []
        while True:
            args = ["compile", "query", fixture["project"], fixture["run_id"], kind, offset, limit]
            if search:
                args.append(search)
            result = self.call(*args)
            self.check("pagination." + kind, result["offset"] == offset and len(result["items"]) <= limit)
            all_items.extend(result["items"])
            offset += len(result["items"])
            if offset >= result["total"]:
                break
            self.check("pagination.progress", bool(result["items"]))
        return all_items

    def db_rows(self, fixture, table):
        path = fixture["project"] / ".noetome-compile/compile.sqlite"
        with contextlib.closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)) as connection:
            connection.row_factory = sqlite3.Row
            return [dict(row) for row in connection.execute(f"SELECT * FROM {table} WHERE run_id=?", (fixture["run_id"],))]

    def coverage(self, name, fixture, summary):
        path = fixture["project"] / ".noetome-corpus/corpus.sqlite"
        units = self.db_rows(fixture, "units")
        multi, total_bytes, unicode_seen = 0, 0, False
        with contextlib.closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)) as connection:
            pages = list(connection.execute("SELECT page_number,page_json,page_sha256 FROM pages WHERE source_id=? ORDER BY page_number", (fixture["source_id"],)))
            self.check(name + ".all_physical_pages_prepared", len(pages) == summary["physical_pages"] == summary["prepared_pages"])
            for number, raw, page_sha in pages:
                page = json.loads(raw)
                text = page["text"].encode("utf8")
                unicode_seen |= "é" in page["text"]
                pieces = sorted((json.loads(unit["unit_json"]) for unit in units if unit["page_number"] == number), key=lambda unit: unit["start_byte"])
                multi += len(pieces) > 1
                cursor = 0
                for unit in pieces:
                    self.check(name + ".byte_partition", unit["start_byte"] == cursor and unit["end_byte"] > cursor and unit["end_byte"] - cursor <= CHUNK_BYTES)
                    self.check(name + ".exact_utf8_source", text[cursor:unit["end_byte"]] == unit["text"].encode("utf8") and unit["page_sha256"] == page_sha == sha(raw.encode("utf8")))
                    cursor = unit["end_byte"]
                self.check(name + ".no_tail_truncation", cursor == len(text) if text.strip() else not pieces, {"page": number, "covered": cursor, "bytes": len(text)})
                total_bytes += len(text)
        self.check(name + ".multiple_chunks_on_multiple_pages", multi >= 2, multi)
        self.check(name + ".multibyte_source_seen", unicode_seen)
        return {"prepared_pages": len(pages), "units": len(units), "text_bytes": total_bytes, "multi_chunk_pages": multi}

    def identity_snapshot(self, fixture):
        return {kind: self.query(fixture, kind, limit=2) for kind in ("concepts", "atoms", "evidence")}

    def source_unchanged(self, name, fixture):
        self.check(name + ".original_pdf_unchanged", sha(fixture["original"].read_bytes()) == fixture["original_sha"])
        self.check(name + ".original_ledger_bytes_unchanged", sha(fixture["ledger"].read_bytes()) == fixture["baseline_sha"])

    def success(self):
        with FakeChat(self.output / "success-http") as server:
            fixture = self.fixture("success", server, pages=4, lines=10)
            final = self.run_cli(fixture)
            self.check("success.all_units_confirmed", final["units_total"] == final["units_confirmed"] == len(server.records) and final["units_failed"] == 0, final)
            coverage = self.coverage("success", fixture, final)
            self.check("success.never_scientifically_verified", final["scientific_quality"] == "not_run" and final["knowledge_status"] == "unverified")
            first = server.records[0]
            self.first_reservation = first["body_bytes"] + 1024 + OUTPUT_TOKENS
            self.check("success.independent_usage_sum", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE * len(server.records) and final["budget"]["pending_reserved_tokens"] == 0)
            before = self.identity_snapshot(fixture)
            request_count = len(server.records)
            repeated = self.run_cli(fixture)
            self.check("success.no_duplicate_request_on_reopen", len(server.records) == request_count and repeated["ir_revision"] == final["ir_revision"])
            self.check("success.stable_ids_and_full_ir", self.identity_snapshot(fixture) == before)
            listed = self.call("compile", "list", fixture["project"])
            self.check("success.persisted_run_list", len(listed) == 1 and listed[0]["run_id"] == fixture["run_id"])
            evidence = before["evidence"][0]
            resolved = self.call("compile", "evidence", fixture["project"], fixture["run_id"], evidence["evidence_id"])
            self.check("success.evidence_jump_exact_span", resolved["evidence"] == evidence and resolved["page"]["text"].encode("utf8")[evidence["start_byte"]:evidence["end_byte"]] == evidence["quote"].encode("utf8"))
            self.check("success.search", bool(self.query(fixture, "atoms", limit=2, search="Diffusion")))
            refused = self.call("compile", "query", fixture["project"], fixture["run_id"], "atoms", 0, 51, expect_ok=False)
            self.check("success.query_bound_refusal", refused["exit_code"] != 0)
            vault = fixture["root"] / "new-test-vault"
            export = self.call("compile", "export", fixture["project"], fixture["run_id"], vault)
            for row in export["files"]:
                file = vault / row["path"]
                self.check("success.export_hash", file.is_file() and len(file.read_bytes()) == row["fingerprint"]["bytes"] and sha(file.read_bytes()) == row["fingerprint"]["sha256"])
            export_before = {p.relative_to(vault).as_posix(): sha(p.read_bytes()) for p in vault.rglob("*") if p.is_file()}
            manifest = json.loads((vault / "export-manifest.json").read_bytes())
            declared = {row["path"]: row["fingerprint"] for row in manifest["files"]}
            self.check("success.full_manifest_exact_set", len(declared) == len(manifest["files"]) and set(declared) == set(export_before) - {"export-manifest.json"} and "处理范围.json" in declared and manifest["self_included"] is False and manifest["status"] == "complete")
            for relative, pin in declared.items():
                file = vault / relative
                self.check("success.full_manifest_actual_pins", sha(file.read_bytes()) == pin["sha256"] and file.stat().st_size == pin["bytes"])
            page_index = [json.loads(line) for line in (vault / ".noetome-index/pages.ndjson").read_text(encoding="utf8").splitlines()]
            unit_index = [json.loads(line) for line in (vault / ".noetome-index/units.ndjson").read_text(encoding="utf8").splitlines()]
            saved_units = self.db_rows(fixture, "units")
            units_by_id = {json.loads(row["unit_json"])["unit_id"]: json.loads(row["unit_json"]) for row in saved_units}
            self.check("success.export_all_physical_pages_and_blank_page", len(page_index) == final["physical_pages"] and {p["page_number"] for p in page_index} == set(range(1, final["physical_pages"] + 1)) and all(p["source_id"] == fixture["source_id"] for p in page_index) and sum(p["units_total"] for p in page_index) == final["units_total"])
            self.check("success.export_every_unit_once", len(unit_index) == len(units_by_id) == final["units_total"] and {u["unit_id"] for u in unit_index} == set(units_by_id))
            for unit in unit_index:
                saved = units_by_id[unit["unit_id"]]
                self.check("success.export_unit_source_pins_and_offsets", all(unit[k] == saved[k] for k in ("source_id", "source_sha256", "page_number", "page_sha256", "start_byte", "end_byte")) and unit["excerpt_sha256"] == sha(saved["text"].encode("utf8")) and unit["status"] == "confirmed")
            for destination in (vault, fixture["project"] / "bad-vault", fixture["root"]):
                refusal = self.call("compile", "export", fixture["project"], fixture["run_id"], destination, expect_ok=False)
                self.check("success.export_existing_or_source_refused", refusal["exit_code"] != 0)
            self.check("success.failed_export_preserves_vault", export_before == {p.relative_to(vault).as_posix(): sha(p.read_bytes()) for p in vault.rglob("*") if p.is_file()})
            existing_vault = fixture["root"] / "authored-existing-vault"
            existing_vault.mkdir()
            (existing_vault / ".obsidian").mkdir()
            guarded_destination = existing_vault / "new-export"
            refusal = self.call("compile", "export", fixture["project"], fixture["run_id"], guarded_destination, expect_ok=False)
            self.check("success.existing_obsidian_vault_child_refused", refusal["exit_code"] != 0 and not guarded_destination.exists() and {p.name for p in existing_vault.iterdir()} == {".obsidian"})
            self.check("success.export_reports_pending_science", export["scientific_quality"] == "not_run" and not export["raw_pdf_included"] and (vault / "处理范围.json").is_file() and (vault / "source-index.json").is_file())
            self.source_unchanged("success", fixture)
            return {"summary": final, "coverage": coverage, "http_requests": request_count, "first_reservation": self.first_reservation, "export_files": len(export_before)}

    def two_runs(self):
        with FakeChat(self.output / "two-runs-http") as server:
            fixture = self.fixture("two-runs", server)
            first = self.run_cli(fixture)
            snapshot = self.identity_snapshot(fixture)
            next_run = self.call("compile", "create", fixture["project"], fixture["selection"], fixture["config"])
            self.check("two_runs.same_authority_no_reset", next_run["budget"]["authority_id"] == first["budget"]["authority_id"] and next_run["budget"]["accounted_tokens"] == first["budget"]["accounted_tokens"])
            second_fixture = {**fixture, "run_id": next_run["run_id"]}
            final = self.run_cli(second_fixture)
            self.check("two_runs.all_usage_cumulative", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE * len(server.records) and final["budget"]["history_count"] == 1)
            self.check("two_runs.first_ir_unchanged", self.identity_snapshot(fixture) == snapshot)
            self.check("two_runs.list_two", len(self.call("compile", "list", fixture["project"])) == 2)
            self.source_unchanged("two_runs", fixture)
            return {"first_run_id": fixture["run_id"], "second_run_id": final["run_id"], "summary": final, "http_requests": len(server.records)}

    def interruption(self, name):
        with FakeChat(self.output / (name + "-http"), mode="delay") as server:
            fixture = self.fixture(name, server, timeout=30000)
            owned = self.spawn("compile", "run", fixture["project"], fixture["run_id"])
            try:
                server.wait_requests(2)
                before = self.status(fixture)
                existing = self.identity_snapshot(fixture)
                self.check(name + ".first_commit_before_interrupt", before["units_confirmed"] == 1 and before["budget"]["accounted_tokens"] == fixture["opening"] + USAGE)
                pending = before["budget"]["pending_reserved_tokens"]
                self.check(name + ".may_send_held", pending == server.records[1]["body_bytes"] + 1024 + OUTPUT_TOKENS)
                if name == "kill":
                    owned.process.kill()
                    killed = self.finish(owned, expect_ok=False)
                    self.check(name + ".owned_root_confirmed_exit", killed["exit_code"] != 0 and owned.process.poll() is not None)
                else:
                    control_at = time.monotonic()
                    self.call("compile", "control", fixture["project"], fixture["run_id"], name)
                    self.finish(owned, expect_ok=False, timeout=5)
                    stop_seconds = time.monotonic() - control_at
                    self.check(name + ".stop_within_five_seconds_of_control", stop_seconds < 5, {"seconds_including_control_call": stop_seconds, "request_timeout_seconds": 30})
                stopped = self.status(fixture)
                expected = "interrupted" if name == "kill" else "paused" if name == "pause" else "cancelled"
                self.check(name + ".durable_stop_state", stopped["status"] == expected, stopped)
                self.check(name + ".unknown_not_refunded", stopped["budget"]["pending_reserved_tokens"] == pending and stopped["budget"]["accounted_tokens"] == fixture["opening"] + USAGE)
                self.check(name + ".confirmed_ir_unchanged", self.identity_snapshot(fixture) == existing)
            finally:
                if owned.process.poll() is None:
                    owned.process.kill()
                    owned.process.wait(10)
                server.release.set()
                server.mode = "good"
            if name == "cancel":
                refuse = self.call("compile", "run", fixture["project"], fixture["run_id"], expect_ok=False)
                self.check(name + ".cancel_terminal", refuse["exit_code"] != 0 and len(server.records) == 2)
                return {"summary": self.status(fixture), "http_requests": len(server.records)}
            resumed = self.run_cli(fixture)
            self.check(name + ".unknown_requires_explicit_retry", resumed["units_failed"] == 1 and resumed["budget"]["pending_reserved_tokens"] == pending)
            self.check(name + ".no_resend_old_request", len({r["request"]["request_id"] for r in server.records}) == len(server.records))
            self.call("compile", "control", fixture["project"], fixture["run_id"], "retry")
            final = self.run_cli(fixture)
            self.check(name + ".retry_new_attempt_and_old_hold", final["units_failed"] == 0 and final["units_confirmed"] == final["units_total"] and final["budget"]["pending_reserved_tokens"] == pending)
            atoms = self.query(fixture, "atoms", limit=2)
            self.check(name + ".first_ids_retained_after_retry", {a["atom_id"] for a in existing["atoms"]}.issubset({a["atom_id"] for a in atoms}))
            self.check(name + ".known_usage_separate_from_unknown", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE * (len(server.records) - 1))
            attempts = self.db_rows(fixture, "attempts")
            self.check(name + ".attempt_numbers_bounded", max(a["attempt_no"] for a in attempts) == 2 and sum(a["attempt_no"] == 2 for a in attempts) == 1)
            self.source_unchanged(name, fixture)
            return {"summary": final, "retained_unknown_tokens": pending, "http_requests": len(server.records)}

    def near_limit(self):
        if self.first_reservation is None:
            raise AssertionError("Run success first to establish independent request-byte reservation")
        with FakeChat(self.output / "near-limit-http") as server:
            fixture = self.fixture("near-limit", server, cap=100 + self.first_reservation, pages=4, lines=10)
            final = self.run_cli(fixture)
            self.check("near_limit.first_request_only", len(server.records) == 1 and final["units_confirmed"] == 1, final)
            self.check("near_limit.stop_preserves_scope", final["status"] in ("budget_stop", "budget_stopped") and final["prepared_pages"] == final["physical_pages"] and final["units_total"] > 1)
            self.check("near_limit.no_unknown_for_denied_request", final["budget"]["accounted_tokens"] == 100 + USAGE and final["budget"]["pending_reserved_tokens"] == 0)
            self.source_unchanged("near_limit", fixture)
            return {"summary": final, "http_requests": len(server.records), "derived_cap": 100 + self.first_reservation}

    def failure(self, name):
        mode = "delay" if name == "timeout" else name
        with FakeChat(self.output / (name + "-http"), mode=mode) as server:
            fixture = self.fixture(name, server, timeout=400 if name == "timeout" else 3000)
            final = self.run_cli(fixture)
            if name == "empty":
                self.check(name + ".empty_unresolved_not_fatal", final["units_failed"] == 1 and final["units_confirmed"] == final["units_total"] - 1 and len(server.records) == final["units_total"] and final["status"] == "needs_review", final)
                self.check(name + ".empty_valid_usage_charged", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE * len(server.records))
            else:
                self.check(name + ".global_stop_after_bad_second", final["status"] == "failed" and len(server.records) == 2 and final["units_confirmed"] == 1 and final["units_total"] > 2, final)
                self.check(name + ".bad_second_has_issue", final["error"] is not None and final["units_failed"] == 1 and bool(self.query(fixture, "issues", limit=2)))
                if name == "malformed":
                    self.check(name + ".valid_usage_of_bad_response_charged", final["budget"]["accounted_tokens"] == fixture["opening"] + 2 * USAGE and final["budget"]["pending_reserved_tokens"] == 0)
                else:
                    self.check(name + ".unknown_preserved", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE and final["budget"]["pending_reserved_tokens"] > 0)
            initial = final
            if name == "auth":
                pending = final["budget"]["pending_reserved_tokens"]
                before = self.identity_snapshot(fixture)
                blocked = self.run_cli(fixture)
                self.check(name + ".global_error_no_automatic_retry", blocked["status"] == "failed" and len(server.records) == 2 and blocked["budget"]["pending_reserved_tokens"] == pending)
                server.mode = "good"
                self.call("compile", "control", fixture["project"], fixture["run_id"], "retry")
                final = self.run_cli(fixture)
                self.check(name + ".explicit_retry_keeps_unknown_hold", final["units_confirmed"] == final["units_total"] and final["units_failed"] == 0 and final["budget"]["pending_reserved_tokens"] == pending)
                self.check(name + ".explicit_retry_known_usage_only", final["budget"]["accounted_tokens"] == fixture["opening"] + USAGE * (len(server.records) - 1))
                self.check(name + ".original_confirmed_ids_retained", {a["atom_id"] for a in before["atoms"]}.issubset({a["atom_id"] for a in self.query(fixture, "atoms", limit=2)}))
            self.check(name + ".science_not_run", final["scientific_quality"] == "not_run")
            self.source_unchanged(name, fixture)
            server.release.set()
            return {"initial_summary": initial, "summary": final, "http_requests": len(server.records)}

    def run(self, scenarios=SCENARIOS):
        for name in scenarios:
            first_check = len(self.checks)
            try:
                if name in ("pause", "kill", "cancel"):
                    details = self.interruption(name)
                elif name in ("empty", "malformed", "auth", "timeout"):
                    details = self.failure(name)
                else:
                    details = getattr(self, name)()
                outcome = {"scenario": name, "passed": True, "details": details}
            except Exception as error:
                outcome = {"scenario": name, "passed": False, "failure_type": type(error).__name__, "failure": str(error)}
            outcome["checks"] = self.checks[first_check:]
            self.results.append(outcome)
            save(self.output / (name + "-result.json"), outcome)
            print(json.dumps({"scenario": name, "passed": outcome["passed"], "checks": len(outcome["checks"]), "failure": outcome.get("failure")}, ensure_ascii=False), flush=True)
        end_sha = sha(self.binary.read_bytes())
        unchanged = end_sha == self.binary_sha
        global_checks = [{"name": "binary.unchanged_during_matrix", "passed": unchanged}]
        summary = {"schema_version": 1, "producer": "noetome-production-compile-blackbox", "scope": "Authored PDF + real CLI/corpus parser + strict loopback fake Chat; no real model/credential/account", "external_http_attempts": 0, "started_at_utc": self.started_at, "completed_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "binary_sha256": self.binary_sha, "binary_sha256_after": end_sha, "global_checks": global_checks, "previous_evidence": self.previous_evidence, "scenarios": self.results, "passed": sum(r["passed"] for r in self.results), "failed": sum(not r["passed"] for r in self.results), "all_passed": unchanged and all(r["passed"] for r in self.results), "temporary_root": str(self.temp)}
        save(self.output / "summary.json", summary)
        return summary

    def cleanup(self):
        for path in self.temp.rglob("*"):
            if path.is_file() and path.name == "artificial.usage.json":
                path.chmod(0o600)
        shutil.rmtree(self.temp)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--pdf-python", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--scenarios", default=",".join(SCENARIOS), help="Comma separated subset; near_limit requires success first")
    parser.add_argument("--cleanup", action="store_true")
    parser.add_argument("--previous-evidence", type=Path, help="Pin preserved prior inputs/summary without modifying them")
    args = parser.parse_args(argv)
    names = tuple(args.scenarios.split(","))
    if len(set(names)) != len(names) or any(name not in SCENARIOS for name in names):
        parser.error("Select unique known scenarios")
    harness = Blackbox(args.binary, args.pdf_python, args.output, args.previous_evidence)
    result = harness.run(names)
    if args.cleanup:
        harness.cleanup()
    return 0 if result["all_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
