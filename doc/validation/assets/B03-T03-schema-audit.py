"""Replay of the independent frozen-schema review; SQL databases only in memory."""
from pathlib import Path
import hashlib
import json
import re
import sqlite3
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
base = "058a8b353dd41cff7840a2995dedde032a41db9a"
frozen = Path(".artifacts/b03-t03-schema-review")
expected_hashes = {
    "source_schema.rs": "c0edd0fa39891681331256a8eca8804beb4690dc5e9ffb7394c33532dbab8171",
    "source_schema3.sql": "d688730dbe7e649867a0c11d493796b18588f7c2e66d8db038b7b8b984f8c780",
    "source_schema3_tests.rs": "f6a165b74a43cd4350378c5108e75b50d0cb67a88a1ca63b23c46dcc800c4e6f",
    "project.rs": "8d5382097ad9aafe23bbbc3faccaef68bf695df0f9e7fe30b54dbc08edfa15c5",
}
for name, sha in expected_hashes.items():
    assert hashlib.sha256((frozen / name).read_bytes()).hexdigest() == sha, name
source = (frozen / "source_schema.rs").read_text(encoding="utf-8")
project = (frozen / "project.rs").read_text(encoding="utf-8")
schema_pattern = r'(?:pub\(crate\) )?const SCHEMA: &str = "(.*?)";'
schema2 = re.search(schema_pattern, source, re.S).group(1)
schema1 = re.search(schema_pattern, project, re.S).group(1)
schema3 = (frozen / "source_schema3.sql").read_text(encoding="utf-8")


def base_source(name):
    return subprocess.run(
        ["git", "show", base + ":crates/noetome-cli/src/" + name],
        stdout=subprocess.PIPE,
        check=True,
    ).stdout.decode("utf-8")


assert schema2 == re.search(schema_pattern, base_source("source_schema.rs"), re.S).group(1)
assert schema1 == re.search(schema_pattern, base_source("project.rs"), re.S).group(1)
assert "pub const SCHEMA_VERSION: u32 = 1;" in project


def statements(text):
    remainder = ""
    for ch in text:
        remainder += ch
        if ch == ";" and sqlite3.complete_statement(remainder):
            yield remainder
            remainder = ""
    assert not remainder.strip(), remainder


def execute_batch(db, text):
    for sql in statements(text):
        db.execute(sql)


def memory_db(schema):
    db = sqlite3.connect(":memory:", isolation_level=None)
    db.execute("PRAGMA foreign_keys=ON")
    execute_batch(db, schema)
    return db


def shape(db):
    return [
        (name, kind, " ".join(sql.split()))
        for name, kind, sql in db.execute(
            "SELECT name,type,coalesce(sql,'') FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name"
        )
    ]


ref2, ref3 = memory_db(schema2), memory_db(schema3)
columns = {
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
columns = {name: values.split() for name, values in columns.items()}
assert [(name, kind) for name, kind, _ in shape(ref3)] == [(name, "table") for name in sorted(columns)]
for name, expected in columns.items():
    assert [row[1] for row in ref3.execute("PRAGMA table_info(" + name + ")")] == expected, name
preserved = ["sources", "source_spans", "source_imports"]
for name in preserved:
    ddl2 = ref2.execute("SELECT sql FROM sqlite_schema WHERE name=?", (name,)).fetchone()[0]
    ddl3 = ref3.execute("SELECT sql FROM sqlite_schema WHERE name=?", (name,)).fetchone()[0]
    assert " ".join(ddl2.split()) == " ".join(ddl3.split()), name
meta2 = ref2.execute("SELECT sql FROM sqlite_schema WHERE name='project_metadata'").fetchone()[0]
meta3 = ref3.execute("SELECT sql FROM sqlite_schema WHERE name='project_metadata'").fetchone()[0]
assert " ".join(meta2.replace("schema_version = 2", "schema_version = 3").split()) == " ".join(meta3.split())
assert ref3.execute("PRAGMA user_version").fetchone() == (3,)
assert ref3.execute("PRAGMA application_id").fetchone() == (0x4E4F4554,)
fks = {
    name: [(row[0], row[1], row[2], row[3], row[4]) for row in ref3.execute("PRAGMA foreign_key_list(" + name + ")")]
    for name in ["pdf_parses", "pdf_pages", "pdf_regions", "pdf_resources", "pdf_imports"]
}
assert set(tuple(row[2:]) for row in fks["pdf_regions"]) == {
    ("pdf_pages", "parse_id", "parse_id"),
    ("pdf_pages", "physical_page_index", "physical_page_index"),
    ("pdf_resources", "parse_id", "parse_id"),
    ("pdf_resources", "text_resource_id", "resource_id"),
}
assert set(tuple(row[2:]) for row in fks["pdf_imports"]) == {
    ("sources", "source_id", "source_id"),
    ("source_versions", "version_id", "version_id"),
    ("source_versions", "source_id", "source_id"),
    ("pdf_parses", "parse_id", "parse_id"),
    ("pdf_parses", "version_id", "source_version_id"),
}

# Expectations use independent SELECT rowid,* instead of migration SELECTs.
project_id = "5232018c-42e9-4db6-8e2e-58d8e40d9e66"
when = "2026-11-01T00:00:00Z"
legacy = ["project_metadata", "sources", "source_versions", "source_spans", "source_imports"]
legacy_specs = re.findall(r'LegacyTable \{\s*select: "([^"]+)",\s*insert: "([^"]+)",\s*\}', source)
assert len(legacy_specs) == 5
history_src = (frozen / "source_schema3_tests.rs").read_text(encoding="utf-8")
history_sql = re.search(r'fn old_schema_history\(\).*?database\.execute_batch\("(.*?)"\)\.unwrap\(\)', history_src, re.S).group(1)
history_sql = history_sql.replace("\\\\", "\\")
migrations = []
for old_schema, old_version in [(schema1, 1), (schema2, 2)]:
    db = memory_db(old_schema)
    db.execute("INSERT INTO project_metadata VALUES(1,?,?,?,'initialized')", (project_id, old_version, when))
    if old_version == 2:
        execute_batch(db, history_sql)
        before = {t: db.execute("SELECT rowid,* FROM " + t + " ORDER BY rowid").fetchall() for t in legacy}
    else:
        before = {
            "project_metadata": db.execute("SELECT rowid,* FROM project_metadata ORDER BY rowid").fetchall(),
            **{t: [] for t in legacy[1:]},
        }
    expected = {t: [list(row) for row in rows] for t, rows in before.items()}
    expected["project_metadata"][0][3] = 3
    db.execute("BEGIN IMMEDIATE")
    if old_version == 1:
        db.execute("DROP TABLE project_metadata")
        execute_batch(db, schema2)
        db.execute("INSERT INTO project_metadata VALUES(1,?,2,?,'initialized')", (project_id, when))
    captured = [[list(row) for row in db.execute(select).fetchall()] for select, _ in legacy_specs]
    captured[0][0][3] = 3
    for t in ["source_imports", "source_spans", "source_versions", "sources", "project_metadata"]:
        db.execute("DROP TABLE " + t)
    execute_batch(db, schema3)
    for (_, insert), rows in zip(legacy_specs, captured):
        db.executemany(insert, rows)
    assert shape(db) == shape(ref3)
    assert not db.execute("PRAGMA foreign_key_check").fetchall()
    assert db.execute("PRAGMA quick_check").fetchone() == ("ok",)
    db.execute("COMMIT")
    actual = {t: [list(row) for row in db.execute("SELECT rowid,* FROM " + t + " ORDER BY rowid").fetchall()] for t in legacy}
    assert actual == expected, old_version
    assert db.execute("PRAGMA user_version").fetchone() == (3,)
    assert all(db.execute("SELECT count(*) FROM " + t).fetchone() == (0,) for t in columns if t.startswith("pdf_"))
    migrations.append({"from": old_version, "to": 3, "old_rows_preserved": {t: len(expected[t]) for t in legacy}})
    db.close()
print(json.dumps({
    "status": "passed", "scope": "frozen review audit replay; SQL only in memory; no Rust rerun",
    "frozen_hashes_verified": expected_hashes, "sqlite_version": sqlite3.sqlite_version,
    "schema1_ddl_unchanged": True, "schema2_ddl_unchanged": True,
    "schema3_table_count": len(columns), "schema3_ordered_column_counts": {t: len(v) for t, v in columns.items()},
    "legacy_ddl_preserved": preserved, "metadata_only_version_changed": True,
    "composite_pdf_foreign_keys_verified": True, "migration_sql_row_values_and_rowids": migrations,
}, ensure_ascii=False, indent=2))
ref2.close()
ref3.close()
