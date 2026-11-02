"""Independent ordinary-CLI, read-only SQLite and byte audit for B03-T03.

The PDF oracle is the previously authored fixtures/pdf-text-v1/expected.json.
This script uses only the Python standard library. It imports neither Noetome
nor its parser, validators, schema helpers, renderers, or test support code.
All inputs and intentional corruptions live in a new selected test root.
"""
from __future__ import annotations

import argparse
import base64
from contextlib import closing, contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import sqlite3
import stat
import subprocess
import time
import traceback
import uuid


TABLE_COLUMNS = {
    "project_metadata": "singleton project_id schema_version created_at state",
    "sources": "source_id created_at",
    "source_versions": "version_id source_id revision created_at media_type display_name content_sha256 byte_length normalized_sha256 normalization_version ingest_status raw_snapshot normalized_snapshot error_code error_message",
    "source_spans": "span_id source_version_id ordinal kind start_byte end_byte start_line end_line text_sha256 parser_version parse_status",
    "source_imports": "import_id imported_at selected_path source_id version_id status reused span_count pending_extraction_count knowledge_status error_code error_message",
    "pdf_parses": "parse_id source_version_id created_at cache_key protocol_version requested_parser_json parser_json import_options_version adapter_sha256 status outcome_kind page_count confirmed_page_count parsed_page_count needs_review_page_count failed_page_count region_count snapshot_path outcome_sha256 manifest_sha256 error_code error_message",
    "pdf_pages": "parse_id physical_page_index display_page_number printed_page_label status record_json",
    "pdf_regions": "parse_id region_id physical_page_index locator_kind bbox_left bbox_top bbox_right bbox_bottom text_resource_id text_resource_sha256 start_byte end_byte glyph_indices_json",
    "pdf_resources": "parse_id resource_id relative_path sha256 byte_length kind media_type confidence",
    "pdf_imports": "import_id imported_at selected_path source_id version_id parse_id status reused parse_reused acquisition_status error_code error_message",
}
PDF_REPORT_KEYS = set("schema_version report_kind project_id import_id imported_at selected_path source_id version_id version status reused acquisition_status pdf pending_extraction_count knowledge_status error".split())
PDF_KEYS = set("parse_id reused status requested_parser parser protocol_version import_options_version adapter_sha256 cache_key page_count confirmed_page_count parsed_page_count needs_review_page_count failed_page_count region_count snapshot_path outcome_kind outcome_sha256 manifest_sha256 issues error".split())
PINNED_PARSER = {
    "engine": "pdfplumber", "engine_version": "0.11.9",
    "engine_dependency": "pdfminer.six", "engine_dependency_version": "20251230",
    "adapter_version": "noetome-pdf-text-v1", "options_version": "crop-pt-lines-v1",
    "python_version": "3.12.14",
}
OLD_RAW = "\ufeff# 标题\r\n\r\n正文 α\r第二行\n```rust\r\nlet x = 1;\r\n```\r\n".encode("utf8")
OLD_NORMAL = "# 标题\n\n正文 α\n第二行\n```rust\nlet x = 1;\n```\n".encode("utf8")
OLD_RANGES = [("heading", 0, 9, 1, 1), ("formatting", 9, 10, 2, 2), ("text", 10, 30, 3, 4), ("code", 30, 53, 5, 7)]
SENTINEL = b"Human sentinel outside every selected project.\r\n"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf8")


def write_new(path: Path, data: bytes) -> None:
    with path.open("xb") as output:
        output.write(data)


def is_link(path: Path) -> bool:
    attributes = path.lstat()
    return stat.S_ISLNK(attributes.st_mode) or bool(getattr(attributes, "st_file_attributes", 0) & 0x400)


def ordinary_components(path: Path) -> None:
    for item in [*reversed(path.parents), path]:
        assert not is_link(item), f"Refuse a symlink/junction: {item}"


def fingerprint(path: Path) -> dict:
    data = path.read_bytes()
    return {"sha256": sha(data), "byte_length": len(data)}


def snapshot(root: Path) -> dict:
    result = {}

    def visit(path: Path) -> None:
        assert not is_link(path), f"Snapshot must not follow a symlink/junction: {path}"
        observed = path.stat()
        assert path.is_file() or path.is_dir(), path
        result[path.relative_to(root).as_posix()] = {
            "kind": "file" if path.is_file() else "directory",
            "mtime_ns": observed.st_mtime_ns,
            "readonly": not bool(observed.st_mode & stat.S_IWRITE),
            "fingerprint": fingerprint(path) if path.is_file() else None,
        }
        if path.is_dir():
            for child in sorted(path.iterdir()):
                visit(child)

    visit(root)
    return result


def uuid4(value: str) -> None:
    parsed = uuid.UUID(value)
    assert parsed.version == 4 and str(parsed) == value, value


def digest(value: str) -> None:
    assert isinstance(value, str) and len(value) == 64
    assert all(character in "0123456789abcdef" for character in value), value


def managed(root: Path, relative: str) -> Path:
    assert isinstance(relative, str) and relative
    assert "\\" not in relative and ":" not in relative
    parts = PurePosixPath(relative)
    assert not parts.is_absolute() and all(part not in (".", "..") for part in parts.parts)
    path = root.joinpath(*parts.parts)
    assert path.resolve(strict=True).is_relative_to(root.resolve(strict=True))
    ordinary_components(path)
    return path


@contextmanager
def readonly_database(project: Path):
    with closing(sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro", uri=True)) as connection:
        connection.row_factory = sqlite3.Row
        yield connection


def records(project: Path, sql: str, parameters: tuple = ()) -> list[dict]:
    with readonly_database(project) as database:
        return [dict(row) for row in database.execute(sql, parameters)]


def counts(project: Path) -> dict:
    with readonly_database(project) as database:
        names = {row[0] for row in database.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
        assert names == set(TABLE_COLUMNS), (project, names)
        for table, expected_columns in TABLE_COLUMNS.items():
            actual = [row[1] for row in database.execute(f"PRAGMA table_info({table})")]
            assert actual == expected_columns.split(), (table, actual)
        assert database.execute("PRAGMA user_version").fetchone()[0] == 3
        assert database.execute("PRAGMA application_id").fetchone()[0] == 0x4E4F4554
        assert not list(database.execute("PRAGMA foreign_key_check"))
        metadata = dict(database.execute("SELECT * FROM project_metadata").fetchone())
        config = json.loads((project / "project.json").read_bytes())
        assert config["schema_version"] == metadata["schema_version"] == 3
        assert (config["project_id"], config["created_at"], config["state"]) == (metadata["project_id"], metadata["created_at"], "initialized")
        return {table: database.execute(f"SELECT count(*) FROM {table}").fetchone()[0] for table in TABLE_COLUMNS}


def close_numbers(actual: list, expected: list, tolerance: float) -> None:
    assert len(actual) == len(expected)
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected, tolerance)


@contextmanager
def windows_file_guard(path: Path, share: int):
    import ctypes
    from ctypes import wintypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    create.restype = wintypes.HANDLE
    close = kernel.CloseHandle
    close.argtypes = [wintypes.HANDLE]
    close.restype = wintypes.BOOL
    handle = create(str(path), 0x80000000, share, None, 3, 0x80, None)
    assert handle != ctypes.c_void_p(-1).value, ctypes.WinError(ctypes.get_last_error())
    try:
        yield
    finally:
        assert close(handle), ctypes.WinError(ctypes.get_last_error())


class Verifier:
    def __init__(self, root: Path, binary: Path, repository: Path):
        self.root, self.binary, self.repository = root, binary, repository
        self.fixtures = repository / "fixtures/pdf-text-v1"
        expected_path = self.fixtures / "expected.json"
        expected = json.loads(expected_path.read_bytes())
        assert expected["schema_version"] == 1 and expected["producer"] == "authoring_fixture"
        self.cases = {case["file"]: case for case in expected["cases"]}
        self.runtime = Path(os.environ["NOETOME_PDF_PYTHON"])
        assert self.runtime.is_absolute() and self.runtime.is_file()
        ordinary_components(self.runtime)
        self.sentinel = root / "external-sentinel.bin"
        write_new(self.sentinel, SENTINEL)
        self.sentinel_before = snapshot(self.sentinel)
        self.facts = {
            "schema_version": 1, "task": "B03-T03", "result": "running",
            "started_at_utc": datetime.now(timezone.utc).isoformat(),
            "verifier": "stdlib ordinary CLI + readonly SQLite + frozen authoring/file oracles",
            "platform": platform.platform(), "verifier_python": platform.python_version(),
            "binary": {"path": str(binary), **fingerprint(binary)},
            "runtime": str(self.runtime), "test_root": str(root),
            "authoring_expected": fingerprint(expected_path),
            "verifier_source": fingerprint(Path(__file__).resolve()),
            "not_run": ["arbitrary power-loss recovery", "1000-page throughput", "24-hour stability", "cross-platform execution", "real model extraction", "OCR", "knowledge/scientific correctness", "production Vault write"],
            "calls": [], "checks": [],
        }
        self.import_order: dict[Path, list[str]] = {}

    def assert_sentinel(self) -> None:
        assert snapshot(self.sentinel) == self.sentinel_before

    def call(self, *arguments, expected: int | None = 0, report: bool = True, runtime: bool = True):
        command = [str(self.binary), *map(str, arguments)]
        environment = os.environ.copy()
        if not runtime:
            environment.pop("NOETOME_PDF_PYTHON", None)
        started = time.perf_counter()
        output = subprocess.run(command, cwd=self.root, env=environment, capture_output=True, timeout=60)
        record = {
            "argv": command, "runtime_environment": "pinned" if runtime else "removed",
            "exit_code": output.returncode,
            "wall_ms": round((time.perf_counter() - started) * 1000, 3),
            "stdout": output.stdout.decode("utf8", errors="replace"),
            "stderr": output.stderr.decode("utf8", errors="replace"),
            "stdout_base64": base64.b64encode(output.stdout).decode("ascii"),
            "stderr_base64": base64.b64encode(output.stderr).decode("ascii"),
        }
        self.facts["calls"].append(record)
        if expected is None:
            assert output.returncode != 0 and not output.stdout and output.stderr, record
        else:
            assert output.returncode == expected, record
            if expected == 0:
                assert not output.stderr, record
        self.assert_sentinel()
        return json.loads(output.stdout) if report else record

    def project(self, name: str) -> tuple[Path, dict]:
        project = self.root / name
        self.call("project", "init", project, report=False)
        initial = json.loads((project / "project.json").read_bytes())
        assert initial["schema_version"] == 1
        self.import_order[project] = []
        return project, initial

    def selected(self, name: str, fixture_name: str) -> Path:
        path = self.root / name
        raw = (self.fixtures / fixture_name).read_bytes()
        case = self.cases[fixture_name]
        assert {"sha256": sha(raw), "byte_length": len(raw)} == {"sha256": case["raw_sha256"], "byte_length": case["byte_length"]}
        write_new(path, raw)
        return path

    def import_file(self, project: Path, selected: Path, *, expected: int = 0, source_id: str | None = None, reparse: bool = False, runtime: bool = True) -> dict:
        arguments = ["source", "import", project, selected]
        if source_id:
            arguments.extend(["--source-id", source_id])
        if reparse:
            arguments.append("--reparse")
        result = self.call(*arguments, expected=expected, runtime=runtime)
        uuid4(result["import_id"])
        self.import_order[project].append(result["import_id"])
        return result

    def inspect(self, project: Path, report: dict, expected: int = 0) -> None:
        before = snapshot(project)
        actual = self.call("source", "inspect", project, report["import_id"], expected=expected, runtime=False)
        assert actual == report
        assert snapshot(project) == before, "Inspect changed bytes, mtimes, or side entries"

    def global_order(self, project: Path) -> None:
        actual = records(project, "SELECT rowid,import_id FROM source_imports UNION ALL SELECT rowid,import_id FROM pdf_imports ORDER BY rowid")
        assert actual == [{"rowid": i + 1, "import_id": identity} for i, identity in enumerate(self.import_order[project])], actual

    def acquisition(self, project: Path, report: dict, expected_raw: bytes) -> None:
        assert set(report) == PDF_REPORT_KEYS and set(report["pdf"]) == PDF_KEYS
        assert report["schema_version"] == 2 and report["report_kind"] == "source_pdf_import"
        assert report["acquisition_status"] == "acquired" and report["knowledge_status"] == "pending"
        for field in ("project_id", "import_id", "source_id", "version_id"):
            uuid4(report[field])
        uuid4(report["pdf"]["parse_id"])
        version = report["version"]
        assert version["source_id"] == report["source_id"] and version["version_id"] == report["version_id"]
        assert version["media_type"] == "application/pdf" and version["ingest_status"] == "acquired"
        assert (version["content_sha256"], version["byte_length"]) == (sha(expected_raw), len(expected_raw))
        assert all(version[field] is None for field in ("normalized_sha256", "normalization_version", "normalized_snapshot", "error"))
        assert version["raw_snapshot"] == f".noetome-sources/{report['version_id']}/raw.pdf"
        path = managed(project, version["raw_snapshot"])
        assert path.read_bytes() == expected_raw and not (path.parent / "normalized.txt").exists()
        stored = records(project, "SELECT * FROM source_versions WHERE version_id=?", (report["version_id"],))
        assert len(stored) == 1
        for field in TABLE_COLUMNS["source_versions"].split():
            if field not in ("error_code", "error_message"):
                assert stored[0][field] == version[field], field
        assert stored[0]["error_code"] is None and stored[0]["error_message"] is None
        assert not records(project, "SELECT * FROM source_spans WHERE source_version_id=?", (report["version_id"],))
        counts(project)
        digest(report["pdf"]["cache_key"])
        assert report["pdf"]["adapter_sha256"] == sha((self.repository / "sidecars/pdf_parser/adapter.py").read_bytes())

    def pdf_case(self, project: Path, report: dict, name: str) -> dict:
        case = self.cases[name]
        raw = (self.fixtures / name).read_bytes()
        self.acquisition(project, report, raw)
        pdf = report["pdf"]
        assert report["status"] == ("ready" if case["status"] == "parsed" else case["status"])
        assert pdf["status"] == case["status"] and pdf["outcome_kind"] == "validated"
        assert pdf["requested_parser"] == pdf["parser"] == PINNED_PARSER
        assert pdf["protocol_version"] == 1 and pdf["page_count"] == case["parse_page_count"]
        root = managed(project, pdf["snapshot_path"])
        assert pdf["snapshot_path"] == f".noetome-pdf-parses/{pdf['parse_id']}"
        assert (root / "parser/raw.pdf").read_bytes() == raw
        manifest_path = root / "parser/pdf-manifest.json"
        manifest = json.loads(manifest_path.read_bytes())
        assert pdf["manifest_sha256"] == sha(manifest_path.read_bytes())
        assert manifest["input_fingerprint"] == {"sha256": case["raw_sha256"], "byte_length": case["byte_length"]}
        assert manifest["parser"] == PINNED_PARSER and manifest["status"] == case["status"]
        actual, expected = manifest["pages"], case["pages"]
        assert len(actual) == len(expected) == pdf["confirmed_page_count"]
        stored_pages = records(project, "SELECT * FROM pdf_pages WHERE parse_id=? ORDER BY physical_page_index", (pdf["parse_id"],))
        assert len(stored_pages) == len(expected)
        for page, want, indexed in zip(actual, expected, stored_pages):
            for field in ("physical_page_index", "display_page_number", "printed_page_label", "status"):
                assert page[field] == indexed[field] == want[field], (name, field)
            assert json.loads(indexed["record_json"]) == page
            if want["status"] == "failed":
                assert page["text"] is None and not page["glyphs"] and not page["regions"]
                continue
            geometry = page["geometry"]
            assert (geometry["unit"], geometry["origin"], geometry["y_axis"]) == ("pt", "crop_top_left", "down")
            for field in ("media_box", "crop_box"):
                close_numbers(geometry[field], want[field], 0.000001)
            assert geometry["rotation_degrees"] == want["rotation_degrees"]
            for field in ("width_pt", "height_pt", "user_unit"):
                close_numbers([geometry[field]], [want["geometry"][field]], 0.000001)
            close_numbers(geometry["pdf_to_display"], want["geometry"]["pdf_to_display"], 0.000001)
            assert len(page["glyphs"]) == len(want["glyph_positions"])
            for glyph, expected_glyph in zip(page["glyphs"], want["glyph_positions"]):
                assert glyph["ordinal"] == expected_glyph["ordinal"] and glyph["text"] == expected_glyph["text"]
                close_numbers(glyph["bbox"], expected_glyph["bbox"], expected_glyph["tolerance_pt"])
            if not want["lines"]:
                continue
            text = (root / f"parser/pages/{page['physical_page_index']:06}.txt").read_bytes()
            literal = "".join(line["text"] + "\n" for line in want["lines"]).encode("utf8")
            assert text == literal and page["text"]["sha256"] == sha(text)
            assert len(page["regions"]) == len(want["lines"])
            cursor = 0
            for region, line in zip(page["regions"], want["lines"]):
                assert region["physical_page_index"] == page["physical_page_index"]
                assert region["locator_kind"] == "pdf_page_region"
                reference = region["text"]
                start, end = reference["start_byte"], reference["end_byte"]
                assert start == cursor and text[start:end] == line["text"].encode("utf8") and text[end:end + 1] == b"\n"
                text[:start].decode("utf8"); text[:end].decode("utf8")
                cursor = end + 1
                assert reference["resource_id"] == page["text"]["resource_id"] and reference["resource_sha256"] == sha(text)
                close_numbers(region["bbox"], line["bbox"], line["tolerance_pt"])
            assert cursor == len(text)
        for state, field in (("parsed", "parsed_page_count"), ("needs_review", "needs_review_page_count"), ("failed", "failed_page_count")):
            assert pdf[field] == sum(page["status"] == state for page in expected)
        manifest_regions = [region for page in actual for region in page["regions"]]
        indexed_regions = records(project, "SELECT * FROM pdf_regions WHERE parse_id=?", (pdf["parse_id"],))
        assert len(indexed_regions) == len(manifest_regions) == pdf["region_count"] == report["pending_extraction_count"]
        by_id = {region["region_id"]: region for region in indexed_regions}
        assert set(by_id) == {region["region_id"] for region in manifest_regions}
        for region in manifest_regions:
            indexed = by_id[region["region_id"]]
            assert indexed["physical_page_index"] == region["physical_page_index"] and indexed["locator_kind"] == "pdf_page_region"
            close_numbers([indexed[field] for field in ("bbox_left", "bbox_top", "bbox_right", "bbox_bottom")], region["bbox"], 0)
            for field, reference in (("text_resource_id", "resource_id"), ("text_resource_sha256", "resource_sha256"), ("start_byte", "start_byte"), ("end_byte", "end_byte")):
                assert indexed[field] == region["text"][reference]
            assert json.loads(indexed["glyph_indices_json"]) == region["glyph_indices"]
        self.resources(project, report, root, validated=True)
        outcome = json.loads((root / "parse-result.json").read_bytes())
        assert outcome["input_fingerprint"] == manifest["input_fingerprint"] and outcome["parser"] == PINNED_PARSER
        assert outcome["manifest_sha256"] == pdf["manifest_sha256"]
        assert pdf["outcome_sha256"] == sha((root / "parse-result.json").read_bytes())
        self.import_record(project, report)
        return {"file": name, "status": report["status"], "page_count": pdf["page_count"], "confirmed_pages": len(expected), "regions": len(manifest_regions), "resources": len(records(project, "SELECT * FROM pdf_resources WHERE parse_id=?", (pdf["parse_id"],))), "counts": counts(project)}

    def resources(self, project: Path, report: dict, root: Path, *, validated: bool) -> None:
        resources = records(project, "SELECT * FROM pdf_resources WHERE parse_id=?", (report["pdf"]["parse_id"],))
        names = set()
        for resource in resources:
            path = managed(root, resource["relative_path"])
            assert fingerprint(path) == {"sha256": resource["sha256"], "byte_length": resource["byte_length"]}
            assert resource["relative_path"] not in names
            names.add(resource["relative_path"])
            assert resource["kind"] in ("binary", "utf8")
            if resource["relative_path"].startswith("parser/"):
                confidence = "confirmed" if validated else "unconfirmed"
            else:
                assert resource["relative_path"] in ({"parse-result.json", "parse-result.md"} if validated else {"failure.json"})
                confidence = "diagnostic"
            assert resource["confidence"] == confidence
        files = {name for name, entry in snapshot(root).items() if entry["kind"] == "file"}
        assert names == files, (names, files)

    def import_record(self, project: Path, report: dict) -> None:
        rows = records(project, "SELECT * FROM pdf_imports WHERE import_id=?", (report["import_id"],))
        assert len(rows) == 1
        row = rows[0]
        for field in ("import_id", "imported_at", "selected_path", "source_id", "version_id", "status", "reused", "acquisition_status"):
            assert row[field] == report[field], field
        assert row["parse_id"] == report["pdf"]["parse_id"] and row["parse_reused"] == report["pdf"]["reused"]
        error = report["error"]
        assert (row["error_code"], row["error_message"]) == ((error["code"], error["message"]) if error else (None, None))

    def no_version(self, project: Path, report: dict) -> None:
        assert set(report) == PDF_REPORT_KEYS and set(report["pdf"]) == PDF_KEYS
        assert report["schema_version"] == 2 and report["report_kind"] == "source_pdf_import"
        assert report["status"] == report["acquisition_status"] == "failed" and not report["reused"]
        assert report["knowledge_status"] == "pending" and report["pending_extraction_count"] == 0
        assert all(report[field] is None for field in ("source_id", "version_id", "version"))
        pdf = report["pdf"]
        assert pdf["status"] == "not_attempted" and not pdf["reused"] and not pdf["issues"]
        zero = {"confirmed_page_count", "parsed_page_count", "needs_review_page_count", "failed_page_count", "region_count"}
        for field in PDF_KEYS - {"status", "reused", "issues"}:
            assert pdf[field] == (0 if field in zero else None), field
        actual = counts(project)
        assert actual == {table: 1 if table in ("project_metadata", "pdf_imports") else 0 for table in TABLE_COLUMNS}
        self.import_record(project, report)

    def rejected_inspect(self, project: Path, report: dict, code: str | None = None) -> None:
        before = snapshot(project)
        result = self.call("source", "inspect", project, report["import_id"], expected=None, report=False, runtime=False)
        if code:
            assert code in result["stderr"], result
        assert snapshot(project) == before

    def check(self, label: str, detail: dict) -> None:
        self.global_order(Path(detail["project"])) if "project" in detail and detail.get("coherent", True) else None
        self.assert_sentinel()
        self.facts["checks"].append({"check": label, "result": "passed", **detail})


def legacy_history(verifier: Verifier) -> None:
    project, initial = verifier.project("schema2-history")
    selected = verifier.root / "old-source.MD"
    write_new(selected, OLD_RAW)
    first = verifier.import_file(project, selected)
    assert first["schema_version"] == 1 and first["status"] == "ready"
    version = first["version"]
    assert version["content_sha256"] == sha(OLD_RAW) and version["normalized_sha256"] == sha(OLD_NORMAL)
    assert managed(project, version["raw_snapshot"]).read_bytes() == OLD_RAW
    assert managed(project, version["normalized_snapshot"]).read_bytes() == OLD_NORMAL
    assert len(first["spans"]) == len(OLD_RANGES)
    for ordinal, (span, expected) in enumerate(zip(first["spans"], OLD_RANGES)):
        uuid4(span["span_id"])
        assert tuple(span[field] for field in ("kind", "start_byte", "end_byte", "start_line", "end_line")) == expected
        assert span["ordinal"] == ordinal and span["source_version_id"] == first["version_id"]
        assert span["parser_version"] == "blocks-v1" and span["parse_status"] == "parsed"
        assert span["text_sha256"] == sha(OLD_NORMAL[expected[1]:expected[2]])
    repeated = verifier.import_file(project, selected)
    selected.write_bytes(b"changed\r\n")
    changed = verifier.import_file(project, selected)
    selected.write_bytes(OLD_RAW)
    back = verifier.import_file(project, selected)
    assert back["version_id"] == first["version_id"] and back["spans"] == first["spans"]
    invalid_path = verifier.root / "old-invalid.txt"
    write_new(invalid_path, b"\xff\x00old real raw")
    invalid = verifier.import_file(project, invalid_path, expected=3)
    assert invalid["version"]["content_sha256"] == sha(invalid_path.read_bytes())
    missing = verifier.import_file(project, verifier.root / "old-missing.txt", expected=3)
    assert missing["version"] is None
    reports = [first, repeated, changed, back, invalid, missing]
    old_rows = {table: records(project, f"SELECT rowid,* FROM {table} ORDER BY rowid") for table in ("sources", "source_versions", "source_spans", "source_imports")}
    old_files = snapshot(project / ".noetome-sources")
    assert json.loads((project / "project.json").read_bytes())["schema_version"] == 2
    pdf = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
    detail = verifier.pdf_case(project, pdf, "single-page.pdf")
    config = json.loads((project / "project.json").read_bytes())
    assert config["project_id"] == initial["project_id"] and config["created_at"] == initial["created_at"]
    for table, original in old_rows.items():
        assert records(project, f"SELECT rowid,* FROM {table} ORDER BY rowid")[:len(original)] == original
    current = snapshot(project / ".noetome-sources")
    for path, entry in old_files.items():
        if entry["kind"] == "file":
            assert current[path] == entry, path
    for report in reports:
        verifier.inspect(project, report)  # Legacy failed inspect intentionally stays exit 0.
    after = verifier.root / "after-schema3.txt"
    write_new(after, b"still\r\nplain\rtext")
    text = verifier.import_file(project, after)
    assert text["schema_version"] == 1 and text["version"]["normalization_version"] == "utf8-newlines-v1"
    assert managed(project, text["version"]["normalized_snapshot"]).read_bytes() == b"still\nplain\ntext"
    assert json.loads((project / "project.json").read_bytes())["schema_version"] == 3
    verifier.inspect(project, text)
    verifier.inspect(project, pdf)
    verifier.check("Schema2 history, all old identities/SHA/rows/bytes and Schema3 text semantics", {"project": str(project), "old_imports": len(reports), "old_rows": old_rows, "pdf": detail, "counts": counts(project)})


def identity_history(verifier: Verifier) -> None:
    project, initial = verifier.project("schema1-pdf-identity")
    selected = verifier.selected("page88-file999.pdf", "single-page.pdf")
    raw = selected.read_bytes()
    first = verifier.import_file(project, selected)
    verifier.pdf_case(project, first, "single-page.pdf")
    config = json.loads((project / "project.json").read_bytes())
    assert config["schema_version"] == 3 and config["project_id"] == initial["project_id"] and config["created_at"] == initial["created_at"]
    first_counts = counts(project)
    assert first_counts == {"project_metadata": 1, "sources": 1, "source_versions": 1, "source_spans": 0, "source_imports": 0, "pdf_parses": 1, "pdf_pages": 1, "pdf_regions": 3, "pdf_resources": 5, "pdf_imports": 1}
    old_parse = snapshot(managed(project, first["pdf"]["snapshot_path"]))
    old_raw = snapshot(managed(project, first["version"]["raw_snapshot"]).parent)
    repeat = verifier.import_file(project, selected)
    renamed = verifier.root / "renamed.pdf"
    selected.rename(renamed)
    rename = verifier.import_file(project, renamed)
    for report in (repeat, rename):
        assert report["source_id"] == first["source_id"] and report["version_id"] == first["version_id"] and report["version"] == first["version"]
        assert report["reused"] and report["pdf"]["reused"] and report["pdf"]["parse_id"] == first["pdf"]["parse_id"]
        assert report["import_id"] != first["import_id"]
        verifier.inspect(project, report)
    assert repeat["import_id"] != rename["import_id"]
    assert counts(project) == {**first_counts, "pdf_imports": 3}
    assert snapshot(managed(project, first["pdf"]["snapshot_path"])) == old_parse
    renamed.write_bytes((verifier.fixtures / "multiple-pages.pdf").read_bytes())
    changed = verifier.import_file(project, renamed)
    verifier.pdf_case(project, changed, "multiple-pages.pdf")
    assert changed["source_id"] == first["source_id"] and changed["version_id"] != first["version_id"] and changed["version"]["revision"] == 2
    renamed.write_bytes(raw)
    back = verifier.import_file(project, renamed)
    assert back["version_id"] == first["version_id"] and back["pdf"]["parse_id"] == first["pdf"]["parse_id"]
    fresh = verifier.import_file(project, renamed, reparse=True)
    verifier.pdf_case(project, fresh, "single-page.pdf")
    assert fresh["version_id"] == first["version_id"] and fresh["reused"] and not fresh["pdf"]["reused"]
    assert fresh["pdf"]["parse_id"] != first["pdf"]["parse_id"] and fresh["pdf"]["cache_key"] != first["pdf"]["cache_key"]
    assert fresh["pdf"]["import_options_version"] == "pdf-import-fresh-v1"
    second_fresh = verifier.import_file(project, renamed, reparse=True)
    assert second_fresh["version_id"] == fresh["version_id"] and second_fresh["pdf"]["cache_key"] == fresh["pdf"]["cache_key"]
    assert second_fresh["pdf"]["parse_id"] != fresh["pdf"]["parse_id"] and not second_fresh["pdf"]["reused"]
    assert snapshot(managed(project, first["pdf"]["snapshot_path"])) == old_parse
    assert snapshot(managed(project, first["version"]["raw_snapshot"]).parent) == old_raw
    renamed.rename(verifier.root / "original-moved.pdf")
    for report in (first, changed, back, fresh, second_fresh):
        verifier.inspect(project, report)  # Runtime removed, original no longer at selected_path.
    verifier.check("Schema1 real import, repeat/rename/revision/back and two forced fresh parses", {"project": str(project), "first_import": first, "changed_import": changed, "fresh_import": fresh, "counts": counts(project), "original_moved_runtime_removed_readonly": True})


def explicit_identity(verifier: Verifier) -> None:
    project, _ = verifier.project("explicit-source")
    alpha_path = verifier.selected("alpha.pdf", "single-page.pdf")
    beta_path = verifier.selected("beta.pdf", "multiple-pages.pdf")
    alpha = verifier.import_file(project, alpha_path)
    beta = verifier.import_file(project, beta_path)
    assert alpha["source_id"] != beta["source_id"]
    before = snapshot(managed(project, alpha["pdf"]["snapshot_path"]))
    beta_path.write_bytes(alpha_path.read_bytes())
    shared = verifier.import_file(project, beta_path, source_id=beta["source_id"])
    assert shared["source_id"] == beta["source_id"] and shared["version"]["revision"] == 2
    assert shared["version"]["content_sha256"] == alpha["version"]["content_sha256"]
    assert shared["version_id"] != alpha["version_id"] and shared["pdf"]["parse_id"] != alpha["pdf"]["parse_id"]
    assert not shared["reused"] and not shared["pdf"]["reused"]
    before_counts = counts(project)
    again = verifier.import_file(project, beta_path, source_id=beta["source_id"])
    assert again["version_id"] == shared["version_id"] and again["pdf"]["parse_id"] == shared["pdf"]["parse_id"]
    assert counts(project) == {**before_counts, "pdf_imports": before_counts["pdf_imports"] + 1}
    assert snapshot(managed(project, alpha["pdf"]["snapshot_path"])) == before
    for report in (alpha, beta, shared, again):
        verifier.inspect(project, report)
    verifier.check("Explicit Source isolation for identical raw SHA", {"project": str(project), "alpha_source": alpha["source_id"], "beta_source": beta["source_id"], "shared_sha": shared["version"]["content_sha256"], "counts": counts(project)})


def parser_boundaries(verifier: Verifier) -> None:
    for name in ("damaged.pdf", "encrypted.pdf", "partial-page-failure.pdf", "scanned-page.pdf", "table-page.pdf"):
        project, _ = verifier.project("boundary-" + Path(name).stem)
        first = verifier.import_file(project, verifier.fixtures / name, expected=3)
        detail = verifier.pdf_case(project, first, name)
        before = counts(project)
        old = snapshot(managed(project, first["pdf"]["snapshot_path"]))
        repeated = verifier.import_file(project, verifier.fixtures / name, expected=3)
        assert repeated["version_id"] == first["version_id"] and repeated["reused"]
        assert repeated["import_id"] != first["import_id"]
        if first["status"] == "failed":
            assert repeated["pdf"]["parse_id"] != first["pdf"]["parse_id"] and not repeated["pdf"]["reused"]
            assert counts(project)["pdf_parses"] == before["pdf_parses"] + 1
            assert counts(project)["pdf_pages"] == 2 * before["pdf_pages"]
        else:
            assert repeated["pdf"]["parse_id"] == first["pdf"]["parse_id"] and repeated["pdf"]["reused"]
            assert counts(project) == {**before, "pdf_imports": before["pdf_imports"] + 1}
        verifier.pdf_case(project, repeated, name)
        valid = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
        verifier.pdf_case(project, valid, "single-page.pdf")
        verifier.inspect(project, first, 3)
        verifier.inspect(project, repeated, 3)
        assert snapshot(managed(project, first["pdf"]["snapshot_path"])) == old
        verifier.check("Honest parser boundary/retry: " + name, {"project": str(project), **detail, "after_valid_recovery": counts(project)})


def acquisition_boundaries(verifier: Verifier) -> None:
    modes = ["missing", "too-large"] + (["windows-locked"] if os.name == "nt" else [])
    for mode in modes:
        project, _ = verifier.project("acquisition-" + mode)
        selected = verifier.root / (mode + ".pdf")
        if mode == "too-large":
            with selected.open("xb") as file:
                file.truncate(64 * 1024 * 1024 + 1)
            old = (selected.stat().st_size, selected.stat().st_mtime_ns)
        elif mode == "windows-locked":
            selected = verifier.selected(mode + ".pdf", "single-page.pdf")
        if mode == "windows-locked":
            with windows_file_guard(selected, 0):
                failed = verifier.import_file(project, selected, expected=3)
        else:
            failed = verifier.import_file(project, selected, expected=3)
        verifier.no_version(project, failed)
        if mode == "too-large":
            assert failed["error"]["code"] == "SOURCE_TOO_LARGE"
            assert (selected.stat().st_size, selected.stat().st_mtime_ns) == old
        verifier.inspect(project, failed, 3)
        valid = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
        verifier.pdf_case(project, valid, "single-page.pdf")
        verifier.inspect(project, failed, 3)
        verifier.check("Unread acquisition creates no fake Version: " + mode, {"project": str(project), "failed_import": failed, "counts_after_recovery": counts(project)})
    project, _ = verifier.project("missing-runtime")
    selected = verifier.selected("runtime-input.pdf", "single-page.pdf")
    failed = verifier.import_file(project, selected, expected=3, runtime=False)
    verifier.acquisition(project, failed, selected.read_bytes())
    pdf = failed["pdf"]
    assert failed["status"] == "failed" and failed["error"]["code"] == "PDF_RUNTIME"
    assert pdf["outcome_kind"] == "host_failed" and pdf["parser"] is None and pdf["page_count"] is None
    assert all(pdf[field] == 0 for field in ("confirmed_page_count", "parsed_page_count", "needs_review_page_count", "failed_page_count", "region_count"))
    assert not records(project, "SELECT * FROM pdf_pages") and not records(project, "SELECT * FROM pdf_regions")
    root = managed(project, pdf["snapshot_path"])
    verifier.resources(project, failed, root, validated=False)
    assert pdf["outcome_sha256"] == sha((root / "failure.json").read_bytes())
    old = snapshot(root)
    again = verifier.import_file(project, selected, expected=3, runtime=False)
    verifier.acquisition(project, again, selected.read_bytes())
    assert again["version_id"] == failed["version_id"] and again["import_id"] != failed["import_id"]
    assert again["pdf"]["parse_id"] != failed["pdf"]["parse_id"] and not again["pdf"]["reused"]
    recovered = verifier.import_file(project, selected)
    verifier.pdf_case(project, recovered, "single-page.pdf")
    assert recovered["version_id"] == failed["version_id"] and recovered["reused"] and not recovered["pdf"]["reused"]
    assert recovered["pdf"]["parse_id"] != failed["pdf"]["parse_id"]
    selected.rename(verifier.root / "runtime-original-moved.pdf")
    verifier.inspect(project, failed, 3)
    verifier.inspect(project, again, 3)
    verifier.inspect(project, recovered)
    assert snapshot(root) == old
    verifier.check("Complete acquisition survives missing runtime, valid recovery and moved input", {"project": str(project), "failed_import": failed, "counts": counts(project), "runtime_removed_during_inspect": True})


def sync_manifest(project: Path, report: dict, manifest: dict) -> None:
    """Set up semantic corruption; synchronize every outer checksum/index."""
    root = managed(project, report["pdf"]["snapshot_path"])
    manifest_bytes = json_bytes(manifest)
    digest_value = sha(manifest_bytes)
    (root / "parser/pdf-manifest.json").write_bytes(manifest_bytes)
    outcome_path = root / "parse-result.json"
    outcome = json.loads(outcome_path.read_bytes())
    old_sha, old_bytes = outcome["manifest_sha256"], outcome["resource_bytes"]
    outcome["manifest_sha256"] = digest_value
    by_id = {item["resource_id"]: item for item in manifest["resources"]}
    for item in outcome["sidecar"]["resources"]:
        if item["resource_id"] in by_id:
            item["fingerprint"] = by_id[item["resource_id"]]["fingerprint"]
    resource = next(item for item in outcome["sidecar"]["resources"] if item["resource_id"] == "pdf-manifest")
    resource["fingerprint"] = {"sha256": digest_value, "byte_length": len(manifest_bytes)}
    outcome["resource_bytes"] = sum(item["fingerprint"]["byte_length"] for item in outcome["sidecar"]["resources"])
    outcome_bytes = json_bytes(outcome)
    outcome_path.write_bytes(outcome_bytes)
    markdown_path = root / "parse-result.md"
    markdown = markdown_path.read_text(encoding="utf8").replace(old_sha, digest_value).replace(f"| resource_bytes | {old_bytes} |", f"| resource_bytes | {outcome['resource_bytes']} |")
    markdown_path.write_bytes(markdown.encode("utf8"))
    identity = report["pdf"]["parse_id"]
    with closing(sqlite3.connect(project / "project.sqlite")) as database:
        with database:
            database.execute("UPDATE pdf_parses SET manifest_sha256=?,outcome_sha256=? WHERE parse_id=?", (digest_value, sha(outcome_bytes), identity))
            for name, data in (("parser/pdf-manifest.json", manifest_bytes), ("parse-result.json", outcome_bytes), ("parse-result.md", markdown.encode("utf8"))):
                assert database.execute("UPDATE pdf_resources SET sha256=?,byte_length=? WHERE parse_id=? AND relative_path=?", (sha(data), len(data), identity, name)).rowcount == 1
            for item in manifest["resources"]:
                assert database.execute("UPDATE pdf_resources SET sha256=?,byte_length=? WHERE parse_id=? AND resource_id=?", (item["fingerprint"]["sha256"], item["fingerprint"]["byte_length"], identity, item["resource_id"])).rowcount == 1
            for page in manifest["pages"]:
                database.execute("UPDATE pdf_pages SET record_json=? WHERE parse_id=? AND physical_page_index=?", (json.dumps(page, ensure_ascii=False), identity, page["physical_page_index"]))
                for region in page["regions"]:
                    database.execute("UPDATE pdf_regions SET bbox_left=?,bbox_top=?,bbox_right=?,bbox_bottom=?,start_byte=?,end_byte=?,glyph_indices_json=?,text_resource_sha256=? WHERE parse_id=? AND region_id=?", (*region["bbox"], region["text"]["start_byte"], region["text"]["end_byte"], json.dumps(region["glyph_indices"]), region["text"]["resource_sha256"], identity, region["region_id"]))


def integrity_boundaries(verifier: Verifier) -> None:
    for mutation in ("raw", "resource-index", "page-json", "region-index", "wrong-bbox", "wrong-geometry", "cut-utf8", "changed-body", "unknown-file", "unknown-parse", "unknown-source-version", "unknown-table", "duplicate-rowid", "duplicate-import-id", "schema-mismatch", "unknown-schema"):
        project, _ = verifier.project("integrity-" + mutation)
        old_text = None
        if mutation.startswith("duplicate-"):
            selected_text = verifier.root / (mutation + ".md")
            write_new(selected_text, b"# Earlier text\n")
            old_text = verifier.import_file(project, selected_text)
        report = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
        root = managed(project, report["pdf"]["snapshot_path"])
        identity = report["pdf"]["parse_id"]
        code = None
        if mutation == "raw":
            path = managed(project, report["version"]["raw_snapshot"])
            path.write_bytes(b"!" + path.read_bytes()[1:])
        elif mutation == "unknown-file":
            write_new(root / "parser/human.txt", b"unknown bytes must remain untouched")
        elif mutation in ("unknown-parse", "unknown-source-version"):
            parent = project / (".noetome-pdf-parses" if mutation == "unknown-parse" else ".noetome-sources")
            unknown = parent / str(uuid.uuid4())
            unknown.mkdir()
            write_new(unknown / "human.txt", b"unknown bytes must remain untouched")
        elif mutation in ("schema-mismatch", "unknown-schema"):
            path = project / "project.json"
            config = json.loads(path.read_bytes())
            config["schema_version"] = 2 if mutation == "schema-mismatch" else 99
            path.write_bytes(json_bytes(config))
        elif mutation in ("wrong-bbox", "wrong-geometry", "cut-utf8", "changed-body"):
            manifest = json.loads((root / "parser/pdf-manifest.json").read_bytes())
            if mutation == "wrong-bbox":
                region = manifest["pages"][0]["regions"][0]
                for axis in (0, 2):
                    region["bbox"][axis] += 10
                code = "PDF_LOCATOR"
            elif mutation == "wrong-geometry":
                manifest["pages"][0]["geometry"]["width_pt"] += 10
                code = "PDF_LOCATOR"
            elif mutation == "changed-body":
                path = root / "parser/pages/000000.txt"
                data = b"X" + path.read_bytes()[1:]
                path.write_bytes(data)
                digest_value = sha(data)
                manifest["pages"][0]["text"]["sha256"] = digest_value
                for region in manifest["pages"][0]["regions"]:
                    region["text"]["resource_sha256"] = digest_value
                resource = next(item for item in manifest["resources"] if item["resource_id"] == "page-text-000000")
                resource["fingerprint"]["sha256"] = digest_value
                code = "PDF_TEXT"
            else:
                manifest["pages"][0]["regions"][2]["text"]["start_byte"] += 4
                code = "PDF_TEXT"
            sync_manifest(project, report, manifest)
        else:
            with closing(sqlite3.connect(project / "project.sqlite")) as database:
                with database:
                    if mutation == "resource-index":
                        assert database.execute("UPDATE pdf_resources SET byte_length=byte_length+1 WHERE parse_id=? AND relative_path='parser/pages/000000.txt'", (identity,)).rowcount == 1
                    elif mutation == "page-json":
                        page = json.loads(database.execute("SELECT record_json FROM pdf_pages WHERE parse_id=?", (identity,)).fetchone()[0])
                        page["display_page_number"] = 88
                        database.execute("UPDATE pdf_pages SET record_json=? WHERE parse_id=?", (json.dumps(page), identity))
                    elif mutation == "region-index":
                        database.execute("UPDATE pdf_regions SET bbox_left=bbox_left+1,bbox_right=bbox_right+1 WHERE parse_id=?", (identity,))
                    elif mutation == "unknown-table":
                        database.execute("CREATE TABLE human_notes (note TEXT)")
                    elif mutation == "duplicate-rowid":
                        database.execute("UPDATE pdf_imports SET rowid=(SELECT min(rowid) FROM source_imports) WHERE import_id=?", (report["import_id"],))
                    elif mutation == "duplicate-import-id":
                        database.execute("UPDATE pdf_imports SET import_id=? WHERE import_id=?", (old_text["import_id"], report["import_id"]))
        verifier.rejected_inspect(project, old_text or report, code)
        if mutation.startswith("unknown-"):
            before = snapshot(project)
            verifier.call("source", "import", project, verifier.fixtures / "single-page.pdf", expected=None, report=False)
            assert snapshot(project) == before
        verifier.check("Integrity rejection preserves scene: " + mutation, {"project": str(project), "coherent": False, "semantic_code": code, "inspector_readonly": True})
    if os.name == "nt":
        project, _ = verifier.project("integrity-hardlink")
        report = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
        root = managed(project, report["pdf"]["snapshot_path"])
        path = root / "parser/pages/000000.txt"
        original = fingerprint(path)
        os.link(path, verifier.root / "outside-page-twin.txt")
        verifier.rejected_inspect(project, report)
        assert fingerprint(path) == original
        verifier.check("Actual unchanged-byte Windows hardlink rejection", {"project": str(project), "coherent": False, "fingerprint": original})
        project, _ = verifier.project("integrity-cache-junction")
        report = verifier.import_file(project, verifier.fixtures / "single-page.pdf")
        cache = project / ".noetome-pdf-parses"
        target = verifier.root / "outside-managed-cache"
        cache.rename(target)
        target_before = snapshot(target)
        config_before = fingerprint(project / "project.json")
        database_before = fingerprint(project / "project.sqlite")
        linked = subprocess.run(["cmd", "/c", "mklink", "/J", str(cache), str(target)], capture_output=True, timeout=10)
        assert linked.returncode == 0, linked.stderr
        verifier.call("source", "inspect", project, report["import_id"], expected=None, report=False, runtime=False)
        assert snapshot(target) == target_before
        assert fingerprint(project / "project.json") == config_before
        assert fingerprint(project / "project.sqlite") == database_before
        assert is_link(cache) and cache.resolve(strict=True) == target.resolve(strict=True)
        assert cache.parent.resolve(strict=True).is_relative_to(verifier.root)
        cache.rmdir()  # Remove only this confirmed junction object; never recurse into target.
        assert snapshot(target) == target_before
        verifier.check("Actual Windows cache junction rejection preserves target", {"project": str(project), "coherent": False, "target_files": target_before})
        project, _ = verifier.project("publication-deny-delete")
        selected = verifier.root / "publication-old.md"
        write_new(selected, OLD_RAW)
        old_report = verifier.import_file(project, selected)
        before = snapshot(project)
        with windows_file_guard(project / "project.sqlite", 1):
            verifier.call("source", "import", project, verifier.fixtures / "single-page.pdf", expected=None, report=False)
        for name, item in before.items():
            if item["kind"] == "file":
                assert fingerprint(project / name) == item["fingerprint"]
        assert json.loads((project / "project.json").read_bytes())["schema_version"] == 2
        if (project / ".noetome-writing").exists():
            verifier.rejected_inspect(project, old_report, "SOURCE_WRITE_INCOMPLETE")
        else:
            verifier.inspect(project, old_report)
        verifier.check("Actual Windows database publication lock preserves old bytes", {"project": str(project), "coherent": False, "old_files": before, "recovery_marker": (project / ".noetome-writing").exists()})


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new selected test root; existing parent required")
    parser.add_argument("--binary", type=Path, required=True, help="actual ordinary Noetome CLI")
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve(strict=True)
    ordinary_components(binary)
    assert binary.is_file()
    requested = args.output.absolute()
    assert ".." not in requested.parts and requested.parent.is_dir()
    ordinary_components(requested.parent)
    assert not requested.exists(), "Verifier never adopts an existing root"
    requested.mkdir(exist_ok=False)
    root = requested.resolve(strict=True)
    assert root.parent == requested.parent.resolve(strict=True)
    write_new(root / ".verifier-owner.json", json_bytes({"schema_version": 1, "nonce": str(uuid.uuid4()), "root": str(root)}))
    verifier = Verifier(root, binary, repository)
    try:
        verifier.call("--version", report=False)
        legacy_history(verifier)
        identity_history(verifier)
        explicit_identity(verifier)
        parser_boundaries(verifier)
        acquisition_boundaries(verifier)
        integrity_boundaries(verifier)
        verifier.assert_sentinel()
        verifier.facts["result"] = "passed"
        verifier.facts["completed_at_utc"] = datetime.now(timezone.utc).isoformat()
        verifier.facts["real_cli_calls"] = len(verifier.facts["calls"])
        verifier.facts["checks_passed"] = len(verifier.facts["checks"])
        write_new(root / "verification.json", json_bytes(verifier.facts))
        lines = ["# B03-T03 独立 PDF 来源验收\n\n", "使用普通 Noetome CLI、Python 标准库 SQLite 只读查询及真实文件字节。正文、页和坐标预期来自事先冻结的 authoring fixture；不调用产品校验器生成预期。\n\n", "| 实际验收 | 结果 |\n", "| --- | --- |\n"]
        lines.extend(f"| {item['check']} | passed |\n" for item in verifier.facts["checks"])
        lines.append(f"\n实际 CLI 调用 {len(verifier.facts['calls'])} 次，检查 {len(verifier.facts['checks'])} 项。JSON 保存每次参数、退出码、stdout/stderr、实际耗时和独立 SQL/字节证据。\n\n未运行：" + "、".join(verifier.facts["not_run"]) + "。\n")
        write_new(root / "verification.md", "".join(lines).encode("utf8"))
        print(f"Verified {len(verifier.facts['checks'])} PDF source checks / {len(verifier.facts['calls'])} ordinary CLI calls; evidence: {root / 'verification.json'}")
    except BaseException as error:
        verifier.facts["result"] = "failed"
        verifier.facts["failure"] = repr(error)
        verifier.facts["traceback"] = traceback.format_exc()
        write_new(root / "verification-failure.json", json_bytes(verifier.facts))
        raise


if __name__ == "__main__":
    main()
