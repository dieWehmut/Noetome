"""Independent ordinary-CLI checks for the explicitly fake B04-T01 gateway.

Author gold is read before any extraction. This script never accepts a report's
own read-only or scientific-quality claim as evidence of source preservation.
"""
from __future__ import annotations

import argparse
import copy
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time


REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "fixtures/extraction-gateway-v1"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def inventory(root: Path) -> dict:
    result = {}
    for path in sorted(root.rglob("*")):
        assert not path.is_symlink(), path
        stat = path.stat()
        result[path.relative_to(root).as_posix()] = {
            "kind": "directory" if path.is_dir() else "file",
            "mtime_ns": stat.st_mtime_ns,
            "length": stat.st_size if path.is_file() else None,
            "sha256": sha(path.read_bytes()) if path.is_file() else None,
        }
    return result


def sql_snapshot(project: Path) -> list:
    with closing(sqlite3.connect(f"{(project / 'project.sqlite').as_uri()}?mode=ro&immutable=1", uri=True)) as db:
        return list(db.iterdump())


class Audit:
    def __init__(self, binary: Path, root: Path):
        self.binary = binary.resolve()
        self.root = root.resolve()
        self.root.mkdir()  # new directory only; never adopt or delete an old run
        self.gold = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))
        self.manifest = json.loads((FIXTURE / "fake.json").read_text(encoding="utf-8"))
        self.payload = json.loads((FIXTURE / "response.json").read_text(encoding="utf-8"))
        self.rows = []
        self.calls = []
        self.project = self.root / "project 中文"
        self.call(["project", "init", str(self.project)], expected=0)
        imported = self.call(["source", "import", str(self.project), str(FIXTURE / "source.txt")], expected=0)
        self.imported = json.loads(imported.stdout)
        self.import_id = self.imported["import_id"]
        self.before = inventory(self.project)
        self.sql_before = sql_snapshot(self.project)

    def call(self, args: list[str], expected: int | None = None) -> subprocess.CompletedProcess:
        run = subprocess.run([str(self.binary), *args], capture_output=True, timeout=40)
        stdout, stderr = run.stdout.decode("utf-8"), run.stderr.decode("utf-8")
        self.calls.append({"argv": args, "exit_code": run.returncode, "stdout": stdout, "stderr": stderr})
        if expected is not None:
            assert run.returncode == expected, (args, run.returncode, stderr, stdout[:1600])
        return subprocess.CompletedProcess(run.args, run.returncode, stdout, stderr)

    def case(self, name: str, manifest=None, payload=None, raw=None) -> Path:
        root = self.root / f"case-{name}"
        root.mkdir()
        save(root / "fake.json", self.manifest if manifest is None else manifest)
        if raw is None:
            save(root / "response.json", self.payload if payload is None else payload)
        else:
            (root / "response.json").write_bytes(raw)
        return root

    def assert_source(self) -> None:
        assert inventory(self.project) == self.before, "source file bytes/mtime/inventory changed"
        assert sql_snapshot(self.project) == self.sql_before, "source SQL changed"

    def extract(self, name: str, *, expected=3, code=None, manifest=None, payload=None, raw=None):
        case = self.case(name, manifest, payload, raw)
        case_before = inventory(case)
        output = self.root / f"out-{name}"
        run = self.call(["extract", "fake", str(self.project), self.import_id, str(case), str(output)], expected)
        assert inventory(case) == case_before, "fake input changed"
        self.assert_source()
        if code:
            assert code in run.stdout + run.stderr, (name, code, run.stdout[:1400], run.stderr)
        if expected != 0:
            assert not (output / "candidates.json").exists(), name
        if (output / "validation.json").exists():
            report = json.loads((output / "validation.json").read_text(encoding="utf-8"))
            markdown = (output / "validation.md").read_text(encoding="utf-8")
            assert "```json\n" in markdown, "Markdown needs the same auditable facts as JSON"
            snapshot = markdown.split("```json\n", 1)[1].rsplit("\n```", 1)[0]
            assert json.loads(snapshot) == report, "JSON/Markdown validation facts differ"
            for filename, fingerprint in report["artifacts"].items():
                actual = (output / filename).read_bytes()
                assert fingerprint == {"sha256": sha(actual), "byte_length": len(actual)}
            if expected != 0:
                assert report["published"] is False and report["knowledge_status"] == "pending"
        self.rows.append({"name": name, "expected_exit": expected, "observed_exit": run.returncode, "passed": True})
        return output

    def nominal(self):
        output = self.extract("valid", expected=0)
        request = json.loads((output / "request.json").read_text(encoding="utf-8"))
        result = json.loads((output / "candidates.json").read_text(encoding="utf-8"))
        report = json.loads((output / "validation.json").read_text(encoding="utf-8"))
        version = self.imported["version"]
        assert request["binding"]["version_id"] == version["version_id"]
        assert request["binding"]["source_id"] == version["source_id"]
        assert request["binding"]["normalized_sha256"] == version["normalized_sha256"]
        assert request["binding"]["parse_id"] is None
        unit = request["units"][0]
        source = (self.project / version["normalized_snapshot"]).read_bytes()
        span = self.imported["spans"][0]
        assert unit["locator"]["span_id"] == span["span_id"]
        assert unit["text"] == source[span["start_byte"]:span["end_byte"]].decode()
        assert unit["locator"]["resource_sha256"] == sha(source)
        payload = result["payload"]
        assert payload["concepts"][0]["canonical_name"] == self.gold["concept_name"]
        candidate = payload["atoms"][0]
        atom, measurement = candidate["atom"], candidate["measurement"]
        assert atom["content"] == self.gold["atom_content"]
        assert atom["verification"] == "unverified" and atom["required"] is False
        assert measurement["value"] == self.gold["value"] and measurement["unit"] == self.gold["unit"]
        assert measurement["conditions"] == self.gold["conditions"]
        evidence = payload["evidence"][0]
        actual = source[evidence["excerpt_start_byte"]:evidence["excerpt_end_byte"]]
        assert evidence["quote"] == self.gold["quote"] == actual.decode()
        assert evidence["excerpt_sha256"] == sha(actual)
        assert report["status"] == "candidates_validated"
        assert report["published"] is False and report["knowledge_status"] == "pending"
        assert report["scientific_quality"] == "not_run" and report["producer"] == "fake"
        assert report["usage"]["price_usd"] is None and report["usage"]["model_usage"] is None
        assert report["usage"]["paid_calls"] == 0 and report["usage"]["network_calls"] == 0
        replay = self.extract("replay", expected=0)
        repeated = json.loads((replay / "candidates.json").read_text(encoding="utf-8"))
        assert repeated["payload"] == result["payload"] and repeated["request_id"] != result["request_id"]
        return request

    def negatives(self, request):
        for name, raw in [("non-json", b"not json"), ("trailing-json", b"{} {}"), ("empty", b""),
                          ("duplicate-field", b'{"schema_version":1,"schema_version":1,"producer":"fake","concepts":[],"evidence":[],"atoms":[]}')]:
            self.extract(name, raw=raw)
        mutations = {
            "extra-field": lambda p: p.update(extra=True),
            "missing-field": lambda p: p.pop("producer"),
            "unknown-schema": lambda p: p.update(schema_version=2),
            "utf8-boundary": lambda p: p["evidence"][0].update(excerpt_start_byte=1),
            "quote-mismatch": lambda p: p["evidence"][0].update(quote="different"),
            "excerpt-sha": lambda p: p["evidence"][0].update(excerpt_sha256="0" * 64),
            "unknown-unit": lambda p: p["evidence"][0].update(unit_id="unit-99"),
            "unknown-concept": lambda p: p["atoms"][0]["atom"].update(concept_ids=["missing-concept"]),
            "required-fake": lambda p: p["atoms"][0]["atom"].update(required=True),
            "reviewed-fake": lambda p: p["atoms"][0]["atom"].update(verification="fixture_reviewed"),
            "missing-measurement": lambda p: p["atoms"][0].pop("measurement"),
            "unknown-value-zero": lambda p: p["atoms"][0]["measurement"].update(value_status="unknown", value="0"),
            "dropped-unit": lambda p: p["atoms"][0]["atom"].update(qualifiers=self.gold["conditions"]),
            "no-support": lambda p: p["atoms"][0]["atom"]["evidence_links"][0].update(relation="refutes"),
        }
        for name, mutate in mutations.items():
            payload = copy.deepcopy(self.payload)
            mutate(payload)
            self.extract(name, payload=payload)
        for name, path, value in [
            ("no-text-capability", ("capabilities", "text"), False),
            ("no-json-capability", ("capabilities", "structured_output"), False),
            ("input-limit", ("limits", "max_input_bytes"), 1),
            ("response-limit", ("limits", "max_response_bytes"), 1),
            ("output-token-limit", ("limits", "max_output_tokens"), 1),
            ("budget-stop", ("budget", "max_requests"), 0),
            ("token-reservation", ("budget", "max_total_tokens"), 1),
            ("price-unknown", ("budget", "max_cost_usd"), "1.00"),
        ]:
            manifest = copy.deepcopy(self.manifest)
            manifest[path[0]][path[1]] = value
            self.extract(name, manifest=manifest)
        for name, field, value in [("unknown-version", "version_id", "00000000-0000-4000-8000-000000000000"),
                                   ("unknown-parse", "parse_id", "00000000-0000-4000-8000-000000000000"),
                                   ("wrong-resource-binding", "normalized_sha256", "0" * 64)]:
            manifest = copy.deepcopy(self.manifest)
            manifest["binding_override"] = dict(request["binding"], **{field: value})
            self.extract(name, manifest=manifest)
        manifest = copy.deepcopy(self.manifest)
        manifest["delay_ms"] = 200
        manifest["limits"]["timeout_ms"] = 15
        self.extract("timeout", manifest=manifest, code="EXTRACT_TIMEOUT")

        for name, mutate in {
            "config-missing-nullable": lambda m: m.pop("binding_override"),
            "config-extra-field": lambda m: m.update(extra=True),
            "config-unknown-schema": lambda m: m.update(schema_version=2),
            "config-response-traversal": lambda m: m.update(response_file="../case-valid/response.json"),
            "config-zero-limit": lambda m: m["limits"].update(max_units=0),
            "config-many-requests": lambda m: m["budget"].update(max_requests=2),
            "unknown-span": lambda m: m.update(selection=[{"kind": "text", "ordinal": 999}]),
        }.items():
            manifest = copy.deepcopy(self.manifest)
            mutate(manifest)
            self.extract(name, manifest=manifest, expected=2)

    def cancellation_and_paths(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["delay_ms"] = 1000
        manifest["limits"]["timeout_ms"] = 5000
        case = self.case("runtime-cancel", manifest=manifest)
        output = self.root / "out-runtime-cancel"
        signal = self.root / "stop.signal"
        argv = ["extract", "fake", str(self.project), self.import_id, str(case), str(output), "--cancel-file", str(signal)]
        process = subprocess.Popen([str(self.binary), *argv], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        until = time.monotonic() + 8
        while not (output / "request.json").exists() and process.poll() is None and time.monotonic() < until:
            time.sleep(0.01)
        if not (output / "request.json").exists():
            process.kill()
            stdout, stderr = process.communicate()
            raise AssertionError(("no actual request before cancellation", stdout, stderr))
        signal.write_bytes(b"stop\n")
        stdout, stderr = process.communicate(timeout=10)
        self.calls.append({"argv": argv, "exit_code": process.returncode, "stdout": stdout.decode(), "stderr": stderr.decode()})
        assert process.returncode == 3 and b"EXTRACT_CANCELLED" in stdout + stderr
        assert not (output / "candidates.json").exists()
        report = json.loads((output / "validation.json").read_text(encoding="utf-8"))
        assert report["published"] is False and report["usage"]["paid_calls"] == 0
        self.assert_source()
        self.rows.append({"name": "actual-runtime-cancel-file", "expected_exit": 3, "observed_exit": process.returncode, "passed": True})

        valid_case = self.root / "case-valid"
        sentinel = self.root / "old-output"
        sentinel.mkdir()
        (sentinel / "human.txt").write_bytes(b"human protected bytes")
        protected = inventory(sentinel)
        for name, output in [("old-output", sentinel), ("inside-source", self.project / "new-output"),
                             ("inside-fake", valid_case / "new-output"), ("path-traversal", self.root / "jump" / ".." / "bad")]:
            run = self.call(["extract", "fake", str(self.project), self.import_id, str(valid_case), str(output)], expected=2)
            assert not (output / "candidates.json").exists()
            self.rows.append({"name": name, "expected_exit": 2, "observed_exit": run.returncode, "passed": True})
        assert inventory(sentinel) == protected
        self.assert_source()

    def pdf(self):
        author = json.loads((REPO / "fixtures/pdf-text-v1/expected.json").read_text(encoding="utf-8"))["cases"][0]
        quote = author["pages"][0]["lines"][1]["text"]
        assert quote == "Alpha keeps its unit: 12 mg at room temperature."
        imported = json.loads(self.call(["source", "import", str(self.project), str(REPO / "fixtures/pdf-text-v1/single-page.pdf")], expected=0).stdout)
        parse_root = self.project / imported["pdf"]["snapshot_path"]
        manifest = json.loads((parse_root / "parser/pdf-manifest.json").read_text(encoding="utf-8"))
        page = manifest["pages"][0]
        page_bytes = (parse_root / "parser/pages/000000.txt").read_bytes()
        region = next(r for r in page["regions"] if page_bytes[r["text"]["start_byte"]:r["text"]["end_byte"]].decode() == quote)
        start = region["text"]["start_byte"]
        payload = copy.deepcopy(self.payload)
        payload["concepts"][0]["canonical_name"] = "Alpha mass"
        payload["atoms"][0]["atom"].update(content="Alpha uses 12 mg at room temperature.", qualifiers=["mg", "room temperature"])
        payload["atoms"][0]["measurement"].update(value="12", unit="mg", conditions=["room temperature"])
        payload["evidence"][0].update(quote=quote, excerpt_start_byte=start, excerpt_end_byte=start + len(quote.encode()), excerpt_sha256=sha(quote.encode()))
        config = copy.deepcopy(self.manifest)
        config["selection"] = [{"kind": "pdf", "page_index": 0, "region_id": region["region_id"]}]
        old_import = self.import_id
        self.import_id = imported["import_id"]
        self.before, self.sql_before = inventory(self.project), sql_snapshot(self.project)
        output = self.extract("pdf-valid", expected=0, manifest=config, payload=payload)
        request = json.loads((output / "request.json").read_text(encoding="utf-8"))
        binding, unit = request["binding"], request["units"][0]
        assert binding["normalized_sha256"] is None and binding["parse_id"] == imported["pdf"]["parse_id"]
        assert binding["manifest_sha256"] == sha((parse_root / "parser/pdf-manifest.json").read_bytes())
        assert unit["locator"]["kind"] == "pdf" and unit["locator"]["page_index"] == 0
        assert unit["locator"]["parse_id"] == binding["parse_id"]
        assert unit["locator"]["bbox"] == region["bbox"]
        assert unit["locator"]["resource_sha256"] == sha(page_bytes) and unit["text"] == quote
        bad = copy.deepcopy(config)
        bad["binding_override"] = dict(binding, parse_id="00000000-0000-4000-8000-000000000000")
        self.extract("pdf-unknown-parse", manifest=bad, payload=payload)
        bad = copy.deepcopy(config)
        bad["binding_override"] = dict(binding, manifest_sha256="0" * 64)
        self.extract("pdf-manifest-sha", manifest=bad, payload=payload)
        bad = copy.deepcopy(config)
        bad["selection"][0]["region_id"] = "unknown-region"
        self.extract("pdf-unknown-region", manifest=bad, payload=payload, expected=2)
        for name, file in [("pdf-failed", "damaged.pdf"), ("pdf-no-text", "scanned-page.pdf")]:
            failed = json.loads(self.call(["source", "import", str(self.project), str(REPO / "fixtures/pdf-text-v1" / file)], expected=3).stdout)
            self.import_id = failed["import_id"]
            self.before, self.sql_before = inventory(self.project), sql_snapshot(self.project)
            self.extract(name, manifest=config, payload=payload, expected=2)
        self.import_id = old_import

    def needs_review(self):
        # This source and expectation are fixed before the fake request. An
        # unclosed fence is a real blocks-v1 needs_review Span, not a fake flag.
        body = "```txt\n观测值未知；单位 ms，条件未验证。\n"
        selected = self.root / "needs-review.md"
        selected.write_text(body, encoding="utf-8")
        imported = json.loads(self.call(["source", "import", str(self.project), str(selected)], expected=0).stdout)
        assert imported["spans"][0]["parse_status"] == "needs_review"
        self.before, self.sql_before = inventory(self.project), sql_snapshot(self.project)
        old_import, self.import_id = self.import_id, imported["import_id"]
        payload = copy.deepcopy(self.payload)
        payload["atoms"][0]["atom"].update(content="观测值未知。", value_status="unknown", qualifiers=["ms", "条件未验证"])
        payload["atoms"][0]["measurement"].update(value_status="unknown", value=None, unit="ms", conditions=["条件未验证"])
        payload["evidence"][0].update(quote=body, excerpt_start_byte=0, excerpt_end_byte=len(body.encode()), excerpt_sha256=sha(body.encode()))
        output = self.extract("needs-review-unknown-value", payload=payload, expected=0)
        request = json.loads((output / "request.json").read_text(encoding="utf-8"))
        report = json.loads((output / "validation.json").read_text(encoding="utf-8"))
        assert request["units"][0]["source_status"] == "needs_review"
        assert report["source_statuses"] == ["needs_review"]
        candidate = json.loads((output / "candidates.json").read_text(encoding="utf-8"))["payload"]["atoms"][0]
        assert candidate["atom"]["verification"] == "unverified" and candidate["measurement"]["value"] is None
        self.import_id = old_import

    def interference(self):
        # Each race owns a copied test project; deliberate external changes are
        # audited separately from the normal source-preservation comparisons.
        for target in ["source", "input", "output"]:
            project = self.root / f"race-project-{target}"
            shutil.copytree(self.project, project)
            before = inventory(project)
            manifest = copy.deepcopy(self.manifest)
            manifest["delay_ms"] = 400
            manifest["limits"]["timeout_ms"] = 5000
            case = self.case(f"race-{target}", manifest=manifest)
            output = self.root / f"out-race-{target}"
            argv = ["extract", "fake", str(project), self.import_id, str(case), str(output)]
            process = subprocess.Popen([str(self.binary), *argv], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            until = time.monotonic() + 8
            while not (output / "request.json").exists() and process.poll() is None and time.monotonic() < until:
                time.sleep(0.01)
            assert (output / "request.json").exists(), "race requires a live actual request"
            where = {"source": project, "input": case, "output": output}[target]
            sentinel = where / "external-human-file.txt"
            sentinel.write_bytes(b"external change after request capture")
            stdout, stderr = process.communicate(timeout=10)
            self.calls.append({"argv": argv, "exit_code": process.returncode, "stdout": stdout.decode(), "stderr": stderr.decode()})
            assert process.returncode == 3, (target, stdout, stderr)
            assert not (output / "candidates.json").exists(), target
            assert sentinel.read_bytes() == b"external change after request capture", "unknown file must survive"
            after = inventory(project)
            if target != "source":
                assert after == before, target
            else:
                assert {k: v for k, v in after.items() if k != sentinel.name} == before
            self.rows.append({"name": f"live-{target}-inventory-interference", "expected_exit": 3, "observed_exit": process.returncode, "passed": True})

    def finish(self):
        self.assert_source()
        save(self.root / "evidence.json", {"schema_version": 1, "producer": "independent_cli_audit",
             "fixture_sha256": sha((FIXTURE / "expected.json").read_bytes()),
             "binary_sha256": sha(self.binary.read_bytes()), "checks": self.rows, "calls": self.calls,
             "source_inventory_before": self.before, "source_inventory_after": inventory(self.project),
             "source_sql_before": self.sql_before, "source_sql_after": sql_snapshot(self.project),
             "scientific_quality": "not_run"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--red", action="store_true")
    args = parser.parse_args()
    audit = Audit(args.binary, args.root)
    try:
        request = audit.nominal()
        if args.red:
            raise AssertionError("RED baseline unexpectedly supports the frozen extraction contract")
        audit.negatives(request)
        audit.cancellation_and_paths()
        audit.needs_review()
        audit.pdf()
        audit.interference()
        audit.finish()
        print(f"{len(audit.rows)} independent ordinary-CLI checks passed; fake only; sources unchanged")
    except BaseException as exc:
        save(audit.root / "failure-evidence.json", {"expected_red": args.red, "error": str(exc),
             "calls": audit.calls, "source_inventory": inventory(audit.project)})
        if args.red and audit.calls[-1]["argv"][:2] == ["extract", "fake"] and audit.calls[-1]["exit_code"] == 2:
            print("RED observed: existing ordinary CLI rejects extract fake; author gold predates implementation")
            return
        raise


if __name__ == "__main__":
    main()
