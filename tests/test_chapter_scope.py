"""Opt-in real CLI scope regression with authored corpus and synthetic budget.

No PDF parser, HTTP server, model request, scientific evaluation or real account is
used. Select NOETOME_TEST_COMPILE_BINARY explicitly; default discovery skips this
blackbox instead of building or choosing a binary implicitly.
"""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest
import uuid


def save_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf8")


class ScopeCliFixture:
    def __init__(self, binary, root):
        self.binary = Path(binary).resolve()
        self.root = root
        self.project = root / "project"
        self.env = os.environ.copy()
        for name in ("NOETOME_TANGMAO_API_KEY", "NOETOME_API_KEY", "NOETOME_OCR_PROFILE"):
            self.env.pop(name, None)
        # Project initialization emits text; read its durable JSON metadata.
        self.call("project", "init", self.project, expect_json=False)
        initialized = json.loads((self.project / "project.json").read_text(encoding="utf8"))
        corpus = self.project / ".noetome-corpus"
        corpus.mkdir()
        self.source_id = str(uuid.uuid4())
        source = corpus / self.source_id
        source.mkdir()
        # Deliberately not a PDF: these tests do not claim ingestion coverage.
        self.original = source / "raw.pdf"
        self.original.write_bytes(b"Authored immutable chapter selection fixture, not a parsed PDF.")
        self.original_bytes = self.original.read_bytes()
        self.source_sha = hashlib.sha256(self.original_bytes).hexdigest()
        db = sqlite3.connect(corpus / "corpus.sqlite")
        try:
            db.executescript("""
                CREATE TABLE metadata(singleton INTEGER PRIMARY KEY, project_id TEXT, schema_version INTEGER);
                CREATE TABLE sources(source_id TEXT PRIMARY KEY, display_name TEXT, selected_path TEXT,
                    content_sha256 TEXT, byte_length INTEGER, status TEXT, page_count INTEGER,
                    created_at TEXT, pause_requested INTEGER, error_code TEXT, error_message TEXT,
                    preflight_json TEXT);
                CREATE TABLE pages(source_id TEXT, page_number INTEGER, status TEXT, page_json TEXT,
                    page_sha256 TEXT, PRIMARY KEY(source_id,page_number));
                PRAGMA application_id=1313817424;
                PRAGMA user_version=1;
            """)
            db.execute("INSERT INTO metadata VALUES(1,?,1)", (initialized["project_id"],))
            db.execute("INSERT INTO sources VALUES(?,?,?,?,?,'needs_review',8,?,0,NULL,NULL,NULL)",
                       (self.source_id, "authored-chapter.pdf", "explicit-synthetic-selection",
                        self.source_sha, len(self.original_bytes), "2026-11-20T00:00:00Z"))
            for number in range(1, 9):
                page = {"schema_version": 1, "source_id": self.source_id, "page_number": number,
                        "status": "text_ready", "text": f"Authored page {number} café.",
                        "width_pt": 612.0, "height_pt": 792.0, "geometry": {}, "regions": [], "issues": []}
                raw = json.dumps(page, ensure_ascii=False, separators=(",", ":"))
                db.execute("INSERT INTO pages VALUES(?,?,?,?,?)", (self.source_id, number,
                           "text_ready", raw, hashlib.sha256(raw.encode("utf8")).hexdigest()))
            db.commit()
        finally:
            db.close()
        self.budget_root = root / "synthetic-external-budget"
        self.budget_root.mkdir()
        self.ledger = self.budget_root / "authored.usage.json"
        save_json(self.ledger, {"schema_version": 1, "approved_total_tokens": 1_000_000,
                  "accounted_tokens": 100, "pending_reserved_tokens": 0, "remaining_tokens": 999_900,
                  "model_attempts": 1, "records": [{"invocation_id": "authored-history",
                  "reservation_tokens": 2000, "confirmed_total_tokens": 100, "network_attempts": 1,
                  "status": "settled_valid_usage_text"}], "updated_at": "2026-11-20T00:00:00Z"})
        self.baseline = self.ledger.read_bytes()
        self.selection_file = root / "selection.json"
        self.config_file = root / "config.json"
        self.scope_file = root / "scope.json"
        self.config = {"schema_version": 1, "endpoint": "http://127.0.0.1:1/v1/chat/completions",
                       "model": "noetome-test-chapter-scope", "credential_env": "NOETOME_TEST_CHAPTER_SCOPE_KEY",
                       "transport_mode": "local_http_test", "usage_ledger": str(self.ledger),
                       "max_chunk_bytes": 1024, "max_output_tokens": 512, "request_timeout_ms": 1000}
        save_json(self.selection_file, [self.source_id])
        save_json(self.config_file, self.config)
        save_json(self.scope_file, self.scope([(7, 7), (3, 4)]))

    def call(self, *args, expect_ok=True, expect_json=True):
        result = subprocess.run([str(self.binary), *map(str, args)], env=self.env,
                                capture_output=True, timeout=20)
        if not expect_ok:
            return result
        if result.returncode:
            raise AssertionError(f"CLI exit {result.returncode}: {result.stderr.decode('utf8', 'replace')[:1600]}")
        if not expect_json:
            return result.stdout.decode("utf8")
        try:
            return json.loads(result.stdout)
        except (ValueError, UnicodeError) as error:
            raise AssertionError(
                f"CLI expected JSON stdout for {args[:2]!r}; exit {result.returncode}: "
                f"stdout={result.stdout.decode('utf8', 'replace')[:800]!r}; "
                f"stderr={result.stderr.decode('utf8', 'replace')[:800]!r}"
            ) from error

    def scope(self, ranges):
        return {"schema_version": 1, "sources": [{"source_id": self.source_id,
                "page_ranges": [{"start_page": start, "end_page": end} for start, end in ranges]}]}

    def create(self, *extra, expect_ok=True):
        return self.call("compile", "create", self.project, self.selection_file, self.config_file,
                         *extra, expect_ok=expect_ok)

    def database_inputs(self, run):
        db = sqlite3.connect(self.project / ".noetome-compile" / "compile.sqlite")
        try:
            return db.execute("SELECT r.config_json,r.config_sha256,s.scope_json,s.scope_sha256 "
                              "FROM runs r JOIN sources s ON s.run_id=r.run_id WHERE r.run_id=?", (run,)).fetchone()
        finally:
            db.close()


@unittest.skipUnless(os.environ.get("NOETOME_TEST_COMPILE_BINARY"),
                     "Select explicit NOETOME_TEST_COMPILE_BINARY for synthetic scope CLI regression")
class ChapterScopeCliTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="noetome-chapter-scope-cli-")
        self.addCleanup(temporary.cleanup)
        self.fixture = ScopeCliFixture(os.environ["NOETOME_TEST_COMPILE_BINARY"], Path(temporary.name))

    def assert_no_migration_or_run(self):
        fixture = self.fixture
        self.assertFalse((fixture.project / ".noetome-compile").exists())
        self.assertEqual([path.name for path in fixture.budget_root.iterdir()], [fixture.ledger.name])
        self.assertEqual(fixture.ledger.read_bytes(), fixture.baseline)
        self.assertEqual(fixture.original.read_bytes(), fixture.original_bytes)

    def test_noncontiguous_scope_is_persisted_and_later_input_changes_require_new_run(self):
        fixture = self.fixture
        created = fixture.create("--scope", fixture.scope_file)
        self.assertEqual((created["physical_pages"], created["selected_pages"], created["out_of_scope_pages"]), (8, 3, 5))
        self.assertEqual((created["prepared_pages"], created["unprepared_pages"], created["unresolved_pages"]), (0, 3, 3))
        self.assertEqual((created["failed_pages"], created["pending_ocr_pages"], created["units_total"], created["attempts"]), (0, 0, 0, 0))
        self.assertEqual(created["request_strategy_version"], 1)
        self.assertEqual(created["sources"][0]["source_sha256"], fixture.source_sha)
        self.assertEqual(created["sources"][0]["physical_pages"], 8)
        self.assertEqual(created["sources"][0]["page_ranges"], [{"start_page": 3, "end_page": 4}, {"start_page": 7, "end_page": 7}])
        self.assertEqual(created["budget"]["accounted_tokens"], 100)
        self.assertEqual(created["budget"]["history_count"], 1)
        run = created["run_id"]
        saved = fixture.database_inputs(run)
        self.assertEqual(json.loads(saved[0])["request_strategy_version"], 1)
        pages = fixture.call("compile", "query", fixture.project, run, "pages", 0, 20)
        self.assertEqual(pages["total"], 8)
        self.assertEqual([page["page_number"] for page in pages["items"] if page["in_scope"]], [3, 4, 7])
        for page in pages["items"]:
            self.assertEqual(page["units_total"], 0)
            self.assertEqual(page["status"], "unprepared" if page["in_scope"] else "out_of_scope")
        save_json(fixture.scope_file, fixture.scope([(1, 8)]))
        paused = fixture.call("compile", "control", fixture.project, run, "pause")
        self.assertEqual(paused["status"], "paused")
        reopened = fixture.call("compile", "status", fixture.project, run)
        self.assertEqual((reopened["selected_pages"], reopened["out_of_scope_pages"]), (3, 5))
        self.assertEqual(fixture.database_inputs(run), saved)
        changed = fixture.create("--scope", fixture.scope_file)
        self.assertNotEqual(changed["run_id"], run)
        self.assertEqual((changed["selected_pages"], changed["out_of_scope_pages"]), (8, 0))
        self.assertEqual(fixture.database_inputs(run), saved)
        self.assertEqual(fixture.ledger.read_bytes(), fixture.baseline)
        self.assertEqual(fixture.original.read_bytes(), fixture.original_bytes)

    def test_invalid_scope_matrix_rejects_before_budget_migration(self):
        fixture = self.fixture
        invalid = [fixture.scope(ranges) for ranges in ([], [(0, 2)], [(4, 3)], [(8, 9)],
                   [(3, 4), (3, 4)], [(2, 5), (4, 6)], [(2, 4), (4, 6)])]
        invalid.extend([{"schema_version": 1, "sources": []}, {**fixture.scope([(2, 3)]), "schema_version": 2},
                        {"schema_version": 1, "sources": [{"source_id": str(uuid.uuid4()),
                        "page_ranges": [{"start_page": 2, "end_page": 3}]}]}])
        for number, value in enumerate(invalid):
            with self.subTest(case=number):
                save_json(fixture.scope_file, value)
                rejected = fixture.create("--scope", fixture.scope_file, expect_ok=False)
                self.assertNotEqual(rejected.returncode, 0)
                self.assertIn("COMPILE_SCOPE", rejected.stderr.decode("utf8", "replace"))
                self.assert_no_migration_or_run()
        other = str(uuid.uuid4())
        save_json(fixture.selection_file, [fixture.source_id, other])
        duplicated = fixture.scope([(2, 2)])
        duplicated["sources"].append(duplicated["sources"][0].copy())
        save_json(fixture.scope_file, duplicated)
        self.assertNotEqual(fixture.create("--scope", fixture.scope_file, expect_ok=False).returncode, 0)
        self.assert_no_migration_or_run()

    def test_scope_flag_and_json_errors_have_no_compile_or_budget_side_effects(self):
        fixture = self.fixture
        for suffix in [("--scope",), ("--unknown", fixture.scope_file),
                       ("--scope", fixture.scope_file, "--scope", fixture.scope_file),
                       ("--scope", fixture.scope_file, "unexpected")]:
            with self.subTest(arguments=tuple(map(str, suffix))):
                self.assertNotEqual(fixture.create(*suffix, expect_ok=False).returncode, 0)
                self.assert_no_migration_or_run()
        for raw in ('{"schema_version":1,', 'null', '[]', '{"schema_version":1,"sources":[],"unexpected":true}'):
            with self.subTest(json=raw):
                fixture.scope_file.write_text(raw, encoding="utf8")
                self.assertNotEqual(fixture.create("--scope", fixture.scope_file, expect_ok=False).returncode, 0)
                self.assert_no_migration_or_run()

    def test_omitted_scope_keeps_whole_book_entry_and_explicit_v1_for_new_run(self):
        fixture = self.fixture
        original_config = fixture.config_file.read_bytes()
        created = fixture.create()
        self.assertEqual((created["physical_pages"], created["selected_pages"], created["out_of_scope_pages"]), (8, 8, 0))
        self.assertEqual(created["unprepared_pages"], 8)
        self.assertEqual(created["sources"][0]["scope_schema_version"], 1)
        self.assertEqual(created["sources"][0]["page_ranges"], [{"start_page": 1, "end_page": 8}])
        self.assertEqual(created["request_strategy_version"], 1)
        saved_config, config_hash, _, _ = fixture.database_inputs(created["run_id"])
        self.assertEqual(json.loads(saved_config)["request_strategy_version"], 1)
        self.assertEqual(hashlib.sha256(saved_config.encode("utf8")).hexdigest(), config_hash)
        self.assertEqual(fixture.config_file.read_bytes(), original_config)
        self.assertEqual(fixture.ledger.read_bytes(), fixture.baseline)
        self.assertEqual(created["attempts"], 0)

    def test_unknown_request_strategies_fail_before_budget_migration(self):
        fixture = self.fixture
        for version in (0, 2, 4294967295):
            with self.subTest(version=version):
                save_json(fixture.config_file, {**fixture.config, "request_strategy_version": version})
                rejected = fixture.create("--scope", fixture.scope_file, expect_ok=False)
                self.assertNotEqual(rejected.returncode, 0)
                self.assertIn("COMPILE_REQUEST_STRATEGY", rejected.stderr.decode("utf8", "replace"))
                self.assert_no_migration_or_run()


if __name__ == "__main__":
    unittest.main()
