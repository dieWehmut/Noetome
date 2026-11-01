"""Verify the real CLI with self-authored text in a new, explicitly chosen root.

This is an independent read-only verifier of persisted SQLite and snapshot data,
not a text-parser implementation or a scientific knowledge quality test.
"""

import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import uuid


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(root):
    result = {}
    for path in [root, *sorted(root.rglob("*"))]:
        stat = path.stat()
        result[path.relative_to(root).as_posix()] = {
            "kind": "directory" if path.is_dir() else "file",
            "mtime_ns": stat.st_mtime_ns,
            "size": stat.st_size if path.is_file() else None,
            "sha256": sha256(path.read_bytes()) if path.is_file() else None,
        }
    return result


def require_uuid4(value):
    parsed = uuid.UUID(value)
    assert parsed.version == 4 and str(parsed) == value, value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--test-root", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--distribution-record", type=Path)
    options = parser.parse_args()
    cli = options.cli.resolve(strict=True)
    root = options.test_root.absolute()
    assert root.parent.is_dir(), "Choose an existing parent for the new test root"
    assert not root.exists(), "The verifier never reuses an existing test root"
    assert not options.evidence.exists(), "Choose a new evidence file"
    root.mkdir()
    sentinel = root / "外部人工哨兵.md"
    sentinel.write_bytes(b"self-authored sentinel; preserve these bytes\n")
    sentinel_before = (sentinel.read_bytes(), sentinel.stat().st_mtime_ns)
    project = root / "学习 项目 % # &"
    source = root / "自造 来源.md"
    original = b"\xef\xbb\xbf# Demo\r\n\r\nFirst \xce\xb1\rSecond\r\n"
    normalized = b"# Demo\n\nFirst \xce\xb1\nSecond\n"
    expected_ranges = [
        ("heading", 0, 7, 1, 1),
        ("formatting", 7, 8, 2, 2),
        ("text", 8, 24, 3, 4),
    ]
    assert len(normalized) == 24
    source.write_bytes(original)
    calls = []

    def call(*arguments, expected=0, diagnostic=None, report=False):
        argv = [str(cli), *map(str, arguments)]
        output = subprocess.run(argv, capture_output=True, timeout=30, cwd=root)
        item = {
            "argv": argv,
            "exit_code": output.returncode,
            "stdout": output.stdout.decode("utf-8"),
            "stderr": output.stderr.decode("utf-8"),
        }
        calls.append(item)
        assert output.returncode == expected, item
        if diagnostic:
            assert diagnostic in item["stderr"], item
        else:
            assert not item["stderr"], item
        return json.loads(item["stdout"]) if report else item

    def check_counts(report):
        assert report["schema_version"] == 1
        assert report["knowledge_status"] == "pending"
        assert report["span_count"] == len(report["spans"])
        assert report["pending_extraction_count"] == len(report["spans"])
        require_uuid4(report["import_id"])
        if report["source_id"]:
            require_uuid4(report["source_id"])
        if report["version_id"]:
            require_uuid4(report["version_id"])

    call("--version")
    call("source", "--help")
    call("project", "init", project)
    initial_config = json.loads((project / "project.json").read_bytes())
    assert initial_config["schema_version"] == 1
    first = call("source", "import", project, source, report=True)
    check_counts(first)
    assert first["project_id"] == initial_config["project_id"]
    assert first["status"] == "ready" and not first["reused"]
    version = first["version"]
    assert version["revision"] == 1 and version["byte_length"] == len(original)
    assert version["content_sha256"] == sha256(original)
    assert version["normalized_sha256"] == sha256(normalized)
    assert version["normalization_version"] == "utf8-newlines-v1"
    raw_path = project / version["raw_snapshot"]
    normalized_path = project / version["normalized_snapshot"]
    assert raw_path.read_bytes() == original
    assert normalized_path.read_bytes() == normalized
    old_snapshots = snapshot(raw_path.parent)
    assert len(first["spans"]) == len(expected_ranges)
    for ordinal, (span, expected) in enumerate(zip(first["spans"], expected_ranges)):
        require_uuid4(span["span_id"])
        actual = tuple(span[key] for key in (
            "kind", "start_byte", "end_byte", "start_line", "end_line"
        ))
        assert actual == expected, (actual, expected)
        assert span["ordinal"] == ordinal
        assert span["source_version_id"] == first["version_id"]
        assert span["parser_version"] == "blocks-v1"
        assert span["parse_status"] == "parsed"
        assert span["text_sha256"] == sha256(normalized[expected[1]:expected[2]])

    repeated = call("source", "import", project, source, report=True)
    renamed_source = root / "重命名后的 来源.txt"
    source.rename(renamed_source)
    renamed = call("source", "import", project, renamed_source, report=True)
    for item in (repeated, renamed):
        check_counts(item)
        assert item["reused"] and item["source_id"] == first["source_id"]
        assert item["version_id"] == first["version_id"]
        assert item["spans"] == first["spans"]
        assert item["import_id"] != first["import_id"]

    changed_raw = original + b"Third\r\n"
    changed_normalized = normalized + b"Third\n"
    renamed_source.write_bytes(changed_raw)
    changed = call("source", "import", project, renamed_source, report=True)
    check_counts(changed)
    assert changed["source_id"] == first["source_id"]
    assert changed["version_id"] != first["version_id"] and not changed["reused"]
    assert changed["version"]["revision"] == 2
    assert (project / changed["version"]["raw_snapshot"]).read_bytes() == changed_raw
    assert (project / changed["version"]["normalized_snapshot"]).read_bytes() == changed_normalized
    assert snapshot(raw_path.parent) == old_snapshots

    invalid_source = root / "无效 UTF8.txt"
    invalid_bytes = b"\xff\x00not-utf8"
    invalid_source.write_bytes(invalid_bytes)
    invalid = call("source", "import", project, invalid_source, expected=3,
                   diagnostic="SOURCE_INVALID_UTF8", report=True)
    check_counts(invalid)
    failed_version = invalid["version"]
    assert invalid["status"] == failed_version["ingest_status"] == "failed"
    assert failed_version["content_sha256"] == sha256(invalid_bytes)
    assert failed_version["byte_length"] == len(invalid_bytes)
    assert failed_version["normalized_sha256"] is None
    assert failed_version["normalization_version"] is None
    assert failed_version["normalized_snapshot"] is None
    failed_raw = project / failed_version["raw_snapshot"]
    assert failed_raw.read_bytes() == invalid_bytes
    assert {path.name for path in failed_raw.parent.iterdir()} == {"raw.bin"}
    missing = call("source", "import", project, root / "未存在的 来源.txt", expected=3,
                   diagnostic="SOURCE_READ_ERROR", report=True)
    check_counts(missing)
    assert missing["status"] == "failed" and missing["version"] is None
    assert missing["source_id"] is None and missing["version_id"] is None
    reports = [first, repeated, renamed, changed, invalid, missing]
    assert len({item["import_id"] for item in reports}) == 6

    config = json.loads((project / "project.json").read_bytes())
    assert config == {**initial_config, "schema_version": 2}
    before = snapshot(project)
    database = sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True)
    try:
        tables = sorted(row[0] for row in database.execute(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT GLOB 'sqlite_*'"
        ))
        assert tables == ["project_metadata", "source_imports", "source_spans", "source_versions", "sources"]
        assert database.execute("PRAGMA user_version").fetchone()[0] == 2
        assert not database.execute("PRAGMA foreign_key_check").fetchall()
        metadata = database.execute("SELECT * FROM project_metadata").fetchone()
        assert metadata == (1, config["project_id"], 2, config["created_at"], "initialized")
        sql_counts = {table: database.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                      for table in tables}
        assert sql_counts == {"project_metadata": 1, "sources": 2, "source_versions": 3,
                              "source_spans": 6, "source_imports": 6}, sql_counts
        import_counts = database.execute(
            "SELECT import_id, span_count, pending_extraction_count, knowledge_status FROM source_imports"
        ).fetchall()
        expected_counts = {item["import_id"]: (item["span_count"], item["pending_extraction_count"], "pending")
                           for item in reports}
        assert {row[0]: tuple(row[1:]) for row in import_counts} == expected_counts
        persisted = database.execute(
            "SELECT span_id, ordinal, kind, start_byte, end_byte, start_line, end_line, text_sha256 "
            "FROM source_spans WHERE source_version_id=? ORDER BY ordinal", (first["version_id"],)
        ).fetchall()
        assert persisted == [tuple(span[key] for key in (
            "span_id", "ordinal", "kind", "start_byte", "end_byte", "start_line", "end_line", "text_sha256"
        )) for span in first["spans"]]
    finally:
        database.close()
    assert snapshot(project) == before
    for item in reports:
        assert call("source", "inspect", project, item["import_id"], report=True) == item
    assert snapshot(project) == before
    unknown = call("source", "inspect", project, uuid.uuid4(), expected=2,
                   diagnostic="SOURCE_NOT_FOUND")
    assert not unknown["stdout"] and snapshot(project) == before

    moved = root / "搬移后的 项目 % # &"
    project.rename(moved)
    renamed_source.unlink()
    invalid_source.unlink()
    moved_before = snapshot(moved)
    project_inspect = call("project", "inspect", moved)
    assert config["project_id"] in project_inspect["stdout"]
    for item in reports:
        assert call("source", "inspect", moved, item["import_id"], report=True) == item
    assert snapshot(moved) == moved_before == before
    assert (sentinel.read_bytes(), sentinel.stat().st_mtime_ns) == sentinel_before
    distribution = None
    if options.distribution_record:
        distribution = json.loads(options.distribution_record.read_bytes())
        assert distribution["SHA256"] == distribution["SourceSHA256"] == sha256(cli.read_bytes())
        assert distribution["IntegrityRid"] >= 8192
        assert Path(distribution["Path"]).resolve() == cli
    evidence = {
        "result": "passed", "platform": sys.platform, "python": sys.version.split()[0],
        "python_verifier_sqlite": sqlite3.sqlite_version,
        "verifier_note": "Python SQLite only reads persisted files; it is not the bundled Rust SQLite engine.",
        "test_root": str(root), "project_path": str(moved), "cli_path": str(cli),
        "cli_sha256": sha256(cli.read_bytes()), "distribution": distribution,
        "original_sha256": sha256(original), "normalized_sha256": sha256(normalized),
        "normalized_byte_length": len(normalized), "expected_span_ranges": expected_ranges,
        "project_config": config, "sql_counts": sql_counts, "sql_import_counts": import_counts,
        "reports": reports, "calls": calls, "project_files": moved_before,
        "unchanged_old_snapshots": old_snapshots, "sentinel_unchanged": True,
        "inspect_unchanged_files_and_mtimes": True,
        "moved_cached_source_inspect_without_original_files": True,
        "scope": "Self-authored MD/TXT source registration only; no PDF/model/IR/Vault or knowledge quality claim.",
    }
    with options.evidence.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(evidence, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({"result": "passed", "evidence": str(options.evidence),
                      "test_root": str(root), "project_id": config["project_id"],
                      "sql_counts": sql_counts, "cli_sha256": evidence["cli_sha256"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
