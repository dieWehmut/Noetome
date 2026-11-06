"""Independent checker tests use hostile receipts and ordinary temporary files."""

import os
import copy
import json
from pathlib import Path
import sys
import tempfile
import sqlite3
from types import SimpleNamespace
import unittest
from contextlib import closing, redirect_stdout
from io import StringIO
from urllib.parse import urlsplit
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS))
import verify_run_checkpoint as audit


class OriginalEvidenceTests(unittest.TestCase):
    def test_duplicate_json_keys_cannot_replace_first_identity(self):
        for value in (b'{"run_id":"original","run_id":"forged"}', b'{"units":[{"state":"confirmed","state":"pending"}]}'):
            with self.subTest(value=value), self.assertRaises(audit.AuditError):
                audit.strict_json(value)

    def test_nonfinite_or_invalid_json_is_not_a_receipt(self):
        for value in (b'{"revision":NaN}', b'{"revision":Infinity}', b'{"state":', b'\xff'):
            with self.subTest(value=value), self.assertRaises(audit.AuditError):
                audit.strict_json(value)

    def test_original_byte_change_is_detected_even_with_preserved_timestamp(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "原文.md"
            path.write_bytes(b"original")
            before = audit.inventory(path)
            info = path.stat()
            path.write_bytes(b"forged!!")
            os.utime(path, ns=(info.st_atime_ns, info.st_mtime_ns))
            self.assertEqual(before["."]["mtime_ns"], audit.inventory(path)["."]["mtime_ns"])
            self.assertNotEqual(before, audit.inventory(path))

    def test_relative_manifest_path_cannot_escape_owner(self):
        for value in ("../outside.md", "/outside.md", "a/../b.md", "a//b.md", "a/./b.md", "C:outside.md", "a\\b.md", ""):
            with self.subTest(value=value), self.assertRaises(audit.AuditError):
                audit.safe_relative(value)
        self.assertEqual(str(audit.safe_relative("笔记/甲容器.md")), "笔记/甲容器.md")

    def test_symlink_ancestor_is_rejected_and_external_original_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "outside"
            outside.mkdir()
            original = outside / "original.md"
            original.write_bytes(b"do not read through substituted ownership")
            before = audit.inventory(outside)
            try:
                (root / "link").symlink_to(outside, target_is_directory=True)
            except (OSError, NotImplementedError):
                self.skipTest("ordinary symlink creation is unavailable")
            with self.assertRaises(audit.AuditError):
                audit.ordinary_path(root / "link" / "original.md")
            self.assertEqual(before, audit.inventory(outside))

    def test_cli_children_only_inherit_selected_runtime_environment(self):
        with patch.object(audit.os, "environ", {"OPENAI_API_KEY": "never-pass", "HTTP_PROXY": "never-pass", "NOETOME_REAL_BUDGET": "never-pass"}):
            environment = audit.clean_environment()
        self.assertNotIn("OPENAI_API_KEY", environment)
        self.assertNotIn("HTTP_PROXY", environment)
        self.assertNotIn("NOETOME_REAL_BUDGET", environment)
        with self.assertRaises(audit.AuditError):
            audit.clean_environment({"OPENAI_API_KEY": "fake-is-still-unexpected"})

    def test_selected_author_expected_is_frozen_before_any_cli_output(self):
        with tempfile.TemporaryDirectory() as directory:
            import shutil
            fixture = Path(directory) / "case"
            shutil.copytree(SCRIPTS.parent / "fixtures/text-compare-v1", fixture)
            before = audit.author_check(fixture)
            self.assertTrue(before)
            expected = fixture / "expected.json"
            expected.write_bytes(expected.read_bytes().replace("5 cm".encode(), "6 cm".encode()))
            with self.assertRaisesRegex(audit.AuditError, "author package changed: expected.json"):
                audit.author_check(fixture)

    def test_normalization_does_not_hide_content_versions_or_scientific_status(self):
        first = {"run_id": "first", "release_id": "a", "started_at": "one", "finished_at": "two",
                 "publication": {"release_id": "a", "commit_table": "publication_commits"},
                 "versions": {"renderer": "frozen-v1"}, "checks": {"scientific_quality": "not_run"},
                 "files": [{"note_id": "author-id", "content_sha256": "original"}]}
        import copy
        second = copy.deepcopy(first)
        second.update(run_id="second", release_id="b", started_at="three", finished_at="four")
        second["publication"]["release_id"] = "b"
        self.assertEqual(audit.normalized_report(first), audit.normalized_report(second))
        for field in ("versions", "checks", "files"):
            changed = copy.deepcopy(second)
            changed[field] = {"synchronised_forgery": True}
            with self.subTest(field=field):
                self.assertNotEqual(audit.normalized_report(first), audit.normalized_report(changed))
        self.assertEqual(first["run_id"], "first")


class CheckpointSemanticTests(unittest.TestCase):
    @staticmethod
    def initial(status="queued"):
        return {"schema_version": 1, "status": status,
                "units": [{"unit_id": name, "dependencies": [] if index == 0 else [audit.UNITS[index - 1]],
                           "state": "pending", "attempts": 0, "output_paths": []} for index, name in enumerate(audit.UNITS)],
                "acknowledged_control": 0, "checks": {}, "manifest": [], "output_snapshot": None,
                "pending_intent": None, "validation_report": None, "diagnostic": None}

    @staticmethod
    def rows(events, run_id="retained-run"):
        rows, previous = [], ""
        for sequence, (event, state) in enumerate(events, 1):
            value = {"schema_version": 1, "sequence": sequence, "event": event, "at": "2026-11-06T12:00:00Z",
                     "run_id": run_id, "state": state}
            raw = json.dumps(value, ensure_ascii=False, indent=2)
            digest = audit.sha(raw.encode())
            rows.append((sequence, previous, raw, digest))
            previous = digest
        return rows

    def valid_prefix(self):
        initial = self.initial()
        started = copy.deepcopy(initial)
        started["status"] = "running"
        started["units"][0].update(state="running", attempts=1)
        confirmed = copy.deepcopy(started)
        confirmed["units"][0]["state"] = "confirmed"
        confirmed["checks"] = {"source_contract": {"status": "passed", "issue_count": 0}}
        confirmed["output_snapshot"] = {"schema_version": 1, "published": False}
        return [("created", initial), ("unit_started", started), ("unit_confirmed", confirmed)]

    def test_chain_hash_uses_actual_indented_payload_bytes(self):
        result = audit.validate_history(self.rows(self.valid_prefix()), "retained-run", [])
        self.assertEqual(result["confirmations"]["source_contract"], 1)
        self.assertEqual(result["state"]["units"][0]["attempts"], 1)

    def test_deleted_or_reordered_checkpoint_is_refused(self):
        rows = self.rows(self.valid_prefix())
        for changed in ([rows[0], rows[2]], [rows[1], rows[0], rows[2]]):
            with self.subTest(changed=changed), self.assertRaises(audit.AuditError):
                audit.validate_history(changed, "retained-run", [])

    def test_synchronised_hash_does_not_authorise_skipped_dependency(self):
        events = self.valid_prefix()
        forged = copy.deepcopy(events[-1][1])
        forged["units"][2].update(state="confirmed", attempts=1)
        events[-1] = ("unit_confirmed", forged)
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", [])


    def test_second_confirmation_cannot_rewrite_confirmed_attempt(self):
        events = self.valid_prefix()
        forged = copy.deepcopy(events[-1][1])
        forged["units"][0]["attempts"] = 2
        events.append(("unit_confirmed", forged))
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", [])

    def test_progress_does_not_schedule_or_increment_attempt(self):
        events = self.valid_prefix()[:2]
        progressed = copy.deepcopy(events[-1][1])
        progressed["checks"] = {"source_contract": {"status": "passed", "issue_count": 0}}
        events.append(("unit_progress", progressed))
        audit.validate_history(self.rows(events), "retained-run", [])
        progressed["units"][0]["attempts"] = 2
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", [])

    def test_cancelled_run_cannot_resume_even_with_valid_chain_hashes(self):
        events = self.valid_prefix()
        cancelled = copy.deepcopy(events[-1][1])
        cancelled.update(status="cancelled", acknowledged_control=1)
        events.append(("cancelled", cancelled))
        resumed = copy.deepcopy(cancelled)
        resumed["status"] = "running"
        events.append(("resumed", resumed))
        controls = [(1, "retained-run", "cancel", "2026-11-06T12:00:00Z")]
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", controls)

    def test_control_event_cannot_rewrite_preserved_content(self):
        events = self.valid_prefix()
        paused = copy.deepcopy(events[-1][1])
        paused.update(status="paused", manifest=[{"relative_path": "forged.md"}])
        events.append(("paused", paused))
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", [])

    def test_unknown_missing_fields_and_boolean_attempts_are_refused(self):
        for change in (lambda state: state.update(extra="unknown"), lambda state: state.pop("diagnostic"),
                       lambda state: state["units"][0].update(attempts=True), lambda state: state.update(status="approved")):
            state = self.initial()
            change(state)
            with self.subTest(state=state), self.assertRaises(audit.AuditError):
                audit.validate_state(state)

    def test_publication_reconcile_cannot_confirm_source_contract(self):
        events = self.valid_prefix()
        events[-1] = ("publication_reconciled", events[-1][1])
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", [])

    def test_cancel_dominates_later_pause_when_both_requests_acknowledged(self):
        events = self.valid_prefix()
        cancelled = copy.deepcopy(events[-1][1])
        cancelled.update(status="cancelled", acknowledged_control=2)
        events.append(("cancelled", cancelled))
        controls = [(1, "retained-run", "cancel", "2026-11-06T12:00:00Z"),
                    (2, "retained-run", "pause", "2026-11-06T12:00:00Z")]
        result = audit.validate_history(self.rows(events), "retained-run", controls)
        self.assertEqual(result["state"]["status"], "cancelled")
        cancelled["status"] = "paused"
        events[-1] = ("paused", cancelled)
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events), "retained-run", controls)

    def test_failed_attempt_requires_explicit_resume_and_remains_in_history(self):
        events = self.valid_prefix()[:2]
        failed = copy.deepcopy(events[-1][1])
        failed.update(status="failed", diagnostic="actual temporary I/O failure")
        events.append(("failed", failed))
        resumed = copy.deepcopy(failed)
        resumed.update(status="queued", diagnostic=None)
        events.append(("resumed", resumed))
        started = copy.deepcopy(resumed)
        started["status"] = "running"
        started["units"][0]["attempts"] = 2
        events.append(("unit_started", started))
        result = audit.validate_history(self.rows(events), "retained-run", [])
        self.assertEqual(result["state"]["units"][0]["attempts"], 2)
        self.assertIn("failed", [checkpoint["event"] for checkpoint in result["checkpoints"]])
        with self.assertRaises(audit.AuditError):
            audit.validate_history(self.rows(events[:3] + events[4:]), "retained-run", [])

    def test_prepared_intent_rejects_unknown_paths_fields_and_invalid_hashes(self):
        good = {"phase": "verified", "manifest": [], "files": {"reports/validation.json": {"sha256": "a" * 64, "byte_length": 4}}}
        for change in (lambda intent: intent.update(extra=True), lambda intent: intent["files"].update({"../outside": {"sha256": "a" * 64, "byte_length": 4}}),
                       lambda intent: intent["files"]["reports/validation.json"].update(sha256="short")):
            state = self.initial("running")
            state["units"][0].update(state="running", attempts=1)
            state["pending_intent"] = copy.deepcopy(good)
            change(state["pending_intent"])
            with self.subTest(state=state), self.assertRaises(audit.AuditError):
                audit.validate_state(state)

    def test_empty_rehashed_confirmations_lack_independent_result_witness(self):
        events = self.valid_prefix()
        events[-1][1]["checks"] = {}
        events[-1][1]["output_snapshot"] = None
        with self.assertRaisesRegex(audit.AuditError, "lacks durable"):
            audit.validate_history(self.rows(events), "retained-run", [])

    def test_created_seed_cannot_claim_cancelled_running_or_prepared_results(self):
        for change in (lambda state: state.update(status="cancelled"), lambda state: state.update(status="running"),
                       lambda state: state.update(pending_intent={"phase": "planned", "manifest": [], "files": {}})):
            state = self.initial()
            change(state)
            with self.subTest(state=state), self.assertRaises(audit.AuditError):
                audit.validate_history(self.rows([("created", state)]), "retained-run", [])

    def test_work_event_cannot_acknowledge_or_skip_actual_pause_request(self):
        events = self.valid_prefix()[:2]
        events[-1][1]["acknowledged_control"] = 1
        controls = [(1, "retained-run", "pause", "2026-11-06T12:00:00Z")]
        with self.assertRaisesRegex(audit.AuditError, "forged control acknowledgement"):
            audit.validate_history(self.rows(events), "retained-run", controls)

    def test_failed_input_can_be_invalidated_without_restarting_work(self):
        events = self.valid_prefix()[:2]
        failed = copy.deepcopy(events[-1][1])
        failed.update(status="failed", diagnostic="temporary I/O error")
        invalidated = copy.deepcopy(failed)
        invalidated.update(status="invalidated", diagnostic="selected input changed")
        events.extend([("failed", failed), ("invalidated", invalidated)])
        result = audit.validate_history(self.rows(events), "retained-run", [])
        self.assertEqual(result["state"]["units"][0]["attempts"], 1)
        self.assertEqual(result["state"]["status"], "invalidated")

    def test_resume_records_queued_before_any_new_work_dispatch(self):
        events = self.valid_prefix()
        paused = copy.deepcopy(events[-1][1])
        paused.update(status="paused", acknowledged_control=1)
        events.append(("paused", paused))
        resumed = copy.deepcopy(paused)
        resumed["status"] = "queued"
        controls = [(1, "retained-run", "pause", "2026-11-06T12:00:00Z")]
        audit.validate_history(self.rows(events + [("resumed", resumed)]), "retained-run", controls)
        resumed["status"] = "running"
        with self.assertRaisesRegex(audit.AuditError, "event and visible status differ"):
            audit.validate_history(self.rows(events + [("resumed", resumed)]), "retained-run", controls)

    def test_utc_receipts_require_rfc3339_and_not_local_or_loose_iso_text(self):
        for value in ("2026-11-06 12:00:00Z", "2026-11-06T12:00:00", "2026-11-06T12:00:00+01:00", "2026-11-06T12:00:00.1234567891Z"):
            with self.subTest(value=value), self.assertRaises(audit.AuditError):
                audit.utc_time(value)
        self.assertTrue(audit.utc_time("2026-11-06T12:00:00.123456789Z"))

    def completed_events(self):
        events = self.valid_prefix()
        for index in range(1, len(audit.UNITS)):
            started = copy.deepcopy(events[-1][1])
            started["units"][index].update(state="running", attempts=1)
            events.append(("unit_started", started))
            confirmed = copy.deepcopy(started)
            confirmed["units"][index]["state"] = "confirmed"
            if index >= 1:
                confirmed["manifest"] = [{"relative_path": "笔记/甲容器.md"}, {"relative_path": "笔记/乙容器.md"}]
            if index >= 4:
                confirmed["validation_report"] = {"run_id": "retained-run"}
            if index == 5:
                confirmed["status"] = "succeeded"
                confirmed["output_snapshot"]["published"] = True
            events.append(("unit_confirmed", confirmed))
        return events

    def test_completed_control_acknowledges_late_cancel_without_rollback_or_repeat(self):
        events = self.completed_events()
        completed = copy.deepcopy(events[-1][1])
        completed.update(acknowledged_control=2, diagnostic="completed before control took effect")
        events.append(("completed_control", completed))
        controls = [(1, "retained-run", "cancel", "2026-11-06T12:00:00Z"),
                    (2, "retained-run", "pause", "2026-11-06T12:00:00Z")]
        result = audit.validate_history(self.rows(events), "retained-run", controls)
        self.assertEqual(result["state"]["status"], "succeeded")
        self.assertEqual(result["confirmations"], dict.fromkeys(audit.UNITS, 1))

    def test_completed_control_requires_actual_terminal_publication_and_new_request(self):
        controls = [(1, "retained-run", "pause", "2026-11-06T12:00:00Z")]
        for events in (self.valid_prefix(), self.completed_events()):
            completed = copy.deepcopy(events[-1][1])
            completed.update(acknowledged_control=1, diagnostic="completed before control took effect")
            changed = events + [("completed_control", completed)]
            if len(events) == 3:
                with self.assertRaises(audit.AuditError):
                    audit.validate_history(self.rows(changed), "retained-run", controls)
            else:
                with self.assertRaises(audit.AuditError):
                    audit.validate_history(self.rows(changed), "retained-run", [])
                with self.assertRaises(audit.AuditError):
                    audit.validate_history(self.rows(changed + [("completed_control", completed)]), "retained-run", controls)

    def test_completed_control_cannot_rewrite_author_facts_attempts_or_result(self):
        for change in (lambda state: state["manifest"][0].update(relative_path="forged.md"),
                       lambda state: state["units"][0].update(attempts=2),
                       lambda state: state.update(diagnostic="cancelled and rolled back"),
                       lambda state: state.update(status="cancelled")):
            events = self.completed_events()
            completed = copy.deepcopy(events[-1][1])
            completed.update(acknowledged_control=1, diagnostic="completed before control took effect")
            change(completed)
            with self.subTest(state=completed), self.assertRaises(audit.AuditError):
                audit.validate_history(self.rows(events + [("completed_control", completed)]), "retained-run",
                                       [(1, "retained-run", "cancel", "2026-11-06T12:00:00Z")])


class RealSQLiteWriterTests(unittest.TestCase):
    def journal(self, cold):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "run.sqlite"
            original_metadata = json.dumps({"owner": "synthetic isolated helper test", "overflow_provenance": "冻结原始中文 metadata 页;" * 4096}, ensure_ascii=False)
            with closing(sqlite3.connect(database)) as db, db:
                db.execute("CREATE TABLE metadata(metadata_json TEXT NOT NULL)")
                db.execute("INSERT INTO metadata VALUES(?)", (original_metadata,))
            recorder = SimpleNamespace(root=root, auxiliary_calls=[])
            before = audit.sha(database.read_bytes())
            receipt = audit.sqlite_writer(recorder, database, "selected-writer", cold=cold)
            self.assertTrue(receipt["alive_before_kill"])
            self.assertNotEqual(receipt["exit_code"], 0)
            self.assertEqual(receipt["old_database_sha256"], before)
            self.assertEqual(receipt["journal_header_hex"], "0000000000000000" if cold else "d9d505f920a163d7")
            self.assertEqual(len(recorder.auxiliary_calls), 1)
            self.assertEqual(receipt["real_model_calls"], 0)
            handshake = (root / "sqlite-writers/selected-writer/handshake.bin").read_bytes()
            self.assertEqual(audit.sha(handshake), receipt["handshake_sha256"])
            self.assertTrue((root / "sqlite-writers/selected-writer/stdout.bin").read_bytes().startswith(handshake))
            if cold:
                self.assertEqual(audit.sha(database.read_bytes()), before)
            else:
                import shutil
                recovered = root / "independent-recovery.sqlite"
                shutil.copyfile(database, recovered)
                shutil.copyfile(Path(str(database) + "-journal"), Path(str(recovered) + "-journal"))
                with closing(sqlite3.connect(recovered)) as db, db:
                    self.assertEqual(db.execute("SELECT metadata_json FROM metadata").fetchone(), (original_metadata,))
                    self.assertEqual(db.execute("SELECT name FROM sqlite_master ORDER BY name").fetchall(), [("metadata",)])
                self.assertEqual(audit.sha(recovered.read_bytes()), before)

    def test_real_spilled_hot_beforeimage_is_created_by_killed_sqlite_writer(self):
        self.journal(cold=False)

    def test_real_unspilled_cold_journal_is_not_a_zeroed_fake_header(self):
        self.journal(cold=True)


class SQLiteNamespaceTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "actual Windows mandatory empty/nonempty range locks require NTFS")
    def test_actual_empty_locked_file_is_proved_and_copied_but_nonempty_lock_is_refused(self):
        import msvcrt
        import shutil
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "中文 空格"
            source.mkdir()
            locked = source / "worker.lock"
            locked.write_bytes(b"")
            before = audit.inventory(source)
            identity = audit.actual_identity(locked)
            with locked.open("r+b") as holder:
                msvcrt.locking(holder.fileno(), msvcrt.LK_NBLCK, 1)
                try:
                    with self.assertRaises(PermissionError):
                        locked.read_bytes()
                    self.assertEqual(audit.inventory(source), before)
                    shutil.copytree(source, root / "original-bytes", copy_function=audit.copy_observed_file)
                    self.assertEqual((root / "original-bytes/worker.lock").read_bytes(), b"")
                    self.assertEqual(audit.actual_identity(locked), identity)
                finally:
                    holder.seek(0)
                    msvcrt.locking(holder.fileno(), msvcrt.LK_UNLCK, 1)
            locked.write_bytes(b"x")
            with locked.open("r+b") as holder:
                msvcrt.locking(holder.fileno(), msvcrt.LK_NBLCK, 1)
                try:
                    with self.assertRaises(PermissionError):
                        audit.inventory(source)
                    with self.assertRaises(PermissionError):
                        audit.copy_observed_file(locked, root / "must-not-copy.lock")
                finally:
                    holder.seek(0)
                    msvcrt.locking(holder.fileno(), msvcrt.LK_UNLCK, 1)
            self.assertEqual(locked.read_bytes(), b"x")

    def test_actual_unicode_space_database_uri_preserves_identity_query_and_readonly(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "中文 空格 #%.sqlite"
            with closing(sqlite3.connect(database)) as db, db:
                db.execute("CREATE TABLE original(value TEXT)")
                db.execute("INSERT INTO original VALUES('author original')")
            before = audit.inventory(root)
            canonical = audit.canonical_selected_path(database)
            self.assertEqual(audit.actual_identity(canonical), audit.actual_identity(database))
            if os.name == "nt":
                self.assertTrue(str(canonical).startswith("\\\\?\\"))
            uri = audit.sqlite_file_uri(canonical, "mode=ro&immutable=1")
            self.assertEqual(urlsplit(uri).netloc, "")
            self.assertEqual(urlsplit(uri).query, "mode=ro&immutable=1")
            with closing(sqlite3.connect(uri, uri=True)) as db:
                self.assertEqual(db.execute("SELECT value FROM original").fetchall(), [("author original",)])
                with self.assertRaises(sqlite3.OperationalError):
                    db.execute("UPDATE original SET value='forged'")
            self.assertEqual(before, audit.inventory(root))

    def test_uri_cannot_turn_readonly_mode_into_write_or_duplicate_mode(self):
        for query in ("mode=rw", "mode=ro&mode=rw", "mode=ro&mode=ro", "immutable=1"):
            with self.subTest(query=query), self.assertRaises(audit.AuditError):
                audit.sqlite_file_uri(Path("unused.sqlite"), query)

    def test_writer_only_accepts_exact_lf_or_crlf_and_no_other_payload(self):
        for line in (b"actual_sqlite_writer_ready\n", b"actual_sqlite_writer_ready\r\n"):
            audit.validate_sqlite_writer_handshake(line)
        for line in (b"actual_sqlite_writer_ready", b"actual_sqlite_writer_ready\r", b"actual_sqlite_writer_ready \n",
                     b"other_sqlite_writer_ready\r\n", b"actual_sqlite_writer_ready\r\r\n"):
            with self.subTest(line=line), self.assertRaises(audit.AuditError):
                audit.validate_sqlite_writer_handshake(line)

    @unittest.skipUnless(os.name == "nt", "actual Windows main/NTFS proof requires Windows")
    def test_main_normal_windows_path_matches_actual_owned_canonical_run(self):
        binary = os.environ.get("NOETOME_TEST_BINARY") or os.environ.get("CLI_BIN")
        if not binary:
            self.skipTest("explicit selected native CLI is required")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "中文 空格"
            root.mkdir()
            pack = SCRIPTS.parent / "fixtures/run-checkpoint-v1"
            def bounded_probe(recorder, author_pack):
                run = recorder.root / "实际 Run"
                value, _ = recorder.command(("run", "start", recorder.fixture, run, "--paused"), label="native ordinary-path main probe")
                audit.verify_envelope(run, recorder.fixture, value, value["run_id"], "paused")
                before = audit.inventory(run)
                status, _ = recorder.command(("run", "status", run), label="native actual readonly canonical status")
                audit.verify_envelope(run, recorder.fixture, status, value["run_id"], "paused")
                self.assertEqual(before, audit.inventory(run))
                self.assertTrue(str(recorder.root).startswith("\\\\?\\"))
                published, _ = recorder.command(("run", "resume", recorder.fixture, run), label="native full six-unit resume verifies legacy ordinary-namespace bridge")
                audit.verify_finished(recorder, author_pack, run, published, value["run_id"])
                return {"status": "passed", "scope": "main canonical selection and three real native CLI operations only"}
            argv = [str(SCRIPTS / "verify_run_checkpoint.py"), "--cli", binary, "--root", str(root / "new-root"),
                    "--author-pack", str(pack), "--fixture-sha256", "240cf6bf13c082c24c93d92ab47a5e76cadb799beb4fe162d2665e9fbc12ccd1", "--public-only"]
            with patch.object(sys, "argv", argv), patch.object(audit, "public_audit", bounded_probe), \
                    patch.object(audit, "refusal_audit", return_value=[]), patch.object(audit, "hot_journal_audit", return_value=[]), \
                    patch.object(audit, "compiler_version_audit", return_value=[]), redirect_stdout(StringIO()):
                audit.main()
            self.assertEqual(audit.read_json(root / "new-root/actual.json")["actual_cli_calls"], 3)


if __name__ == "__main__":
    unittest.main()
