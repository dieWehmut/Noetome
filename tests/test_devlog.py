"""Behavior checks for task transitions, evidence and safe ledger persistence."""

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("devlog", Path(__file__).resolve().parents[1] / "scripts" / "devlog.py")
devlog = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(devlog)


def task(ident, dependencies=(), batch="B00"):
    return {"id": ident, "title": "Task " + ident, "batch": batch, "status": "todo",
            "depends_on": list(dependencies), "requirements": [], "acceptance": ["Observable output exists"], "history": []}


class DevlogTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "dev-log").mkdir()
        self.ledger = self.root / "dev-log" / "tasks.json"
        self.data = {"schema_version": 1, "current_batch": "B00", "tasks": [task("A"), task("B", ["A"]), task("C", batch="B01")]}
        self.ledger.write_text(json.dumps(self.data), encoding="utf-8")
        (self.root / "artifact.txt").write_text("verified output", encoding="utf-8")
        devlog.render(self.root)

    def change(self, action, ident="A", **kwargs):
        return devlog.change_task(self.root, action, ident, "Explicit execution note", **kwargs)

    def complete(self, ident="A"):
        return self.change("complete", ident, validation=["Inspected the expected output"], artifacts=["artifact.txt"])

    def test_dependencies_batch_and_single_active_task(self):
        with self.assertRaises(devlog.DevlogError):
            self.change("start", "B")
        with self.assertRaises(devlog.DevlogError):
            self.change("start", "C")
        self.change("start")
        with self.assertRaises(devlog.DevlogError):
            self.change("start", "B")
        self.complete()
        self.change("start", "B")
        self.assertIn("检查通过", devlog.check(self.root))

    def test_completion_requires_explanation_and_existing_internal_artifact(self):
        self.change("start")
        before = self.ledger.read_bytes()
        for args in ({}, {"validation": ["Checked"]},
                     {"validation": ["Checked"], "artifacts": ["missing.txt"]},
                     {"validation": ["Checked"], "artifacts": ["../outside.txt"]}):
            with self.subTest(args=args), self.assertRaises(devlog.DevlogError):
                self.change("complete", **args)
            self.assertEqual(before, self.ledger.read_bytes())
        self.complete()
        data, _ = devlog.load(self.root)
        event = data["tasks"][0]["history"][-1]
        self.assertEqual(event["artifacts"][0]["sha256"], hashlib.sha256(b"verified output").hexdigest())
        self.assertEqual(event["validation"], ["Inspected the expected output"])

    def test_completion_rejects_self_referencing_ledger_and_views(self):
        self.change("start")
        before = self.ledger.read_bytes()
        for path in ("dev-log/tasks.json", "dev-log/TODO.md", "dev-log/DONE.md", "dev-log/STATUS.md"):
            with self.subTest(path=path), self.assertRaisesRegex(devlog.DevlogError, "不能引用"):
                self.change("complete", validation=["Checked"], artifacts=[path])
            self.assertEqual(before, self.ledger.read_bytes())

    def test_display_time_uses_shanghai_and_crosses_utc_date_boundary(self):
        self.assertEqual(devlog.display_time("2026-10-31T16:05:00Z"),
                         "2026-11-01 00:05:00 +08:00（Asia/Shanghai）")

    def test_block_reopen_and_preserve_completion_history(self):
        self.change("block")
        with self.assertRaises(devlog.DevlogError):
            self.change("start")
        self.change("reopen")
        self.change("start")
        self.complete()
        self.change("reopen")
        data, _ = devlog.load(self.root)
        self.assertEqual([event["action"] for event in data["tasks"][0]["history"]],
                         ["block", "reopen", "start", "complete", "reopen"])
        self.assertIn("Inspected the expected output", (self.root / "dev-log" / "DONE.md").read_text(encoding="utf-8"))
        self.assertEqual(data["tasks"][0]["status"], "todo")

    def test_reopening_dependency_cannot_invalidate_active_consumer(self):
        self.change("start")
        self.complete()
        self.change("start", "B")
        with self.assertRaises(devlog.DevlogError):
            self.change("reopen")

    def test_check_is_read_only_and_detects_stale_view(self):
        before = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in (self.root / "dev-log").iterdir()}
        devlog.check(self.root)
        self.assertEqual(before, {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in before})
        (self.root / "dev-log" / "TODO.md").write_text("stale", encoding="utf-8")
        with self.assertRaises(devlog.DevlogError):
            devlog.check(self.root)

    def test_atomic_replace_failure_preserves_previous_ledger(self):
        before = self.ledger.read_bytes()
        with patch.object(devlog.os, "replace", side_effect=OSError("simulated disk failure")):
            with self.assertRaises(OSError):
                self.change("start")
        self.assertEqual(before, self.ledger.read_bytes())
        self.assertEqual([], list((self.root / "dev-log").glob("*.tmp")))
        devlog.check(self.root)

    def test_view_failure_leaves_valid_committed_ledger_and_can_be_repaired(self):
        original = devlog.atomic_write
        def fail_view(path, content):
            if path.name == "TODO.md":
                raise OSError("simulated view failure")
            return original(path, content)
        with patch.object(devlog, "atomic_write", side_effect=fail_view):
            with self.assertRaisesRegex(devlog.DevlogError, "账本已安全更新"):
                self.change("start")
        data, _ = devlog.load(self.root)
        self.assertEqual(data["tasks"][0]["status"], "in_progress")
        with self.assertRaises(devlog.DevlogError):
            devlog.check(self.root)
        devlog.render(self.root)
        devlog.check(self.root)

    def test_concurrent_writer_is_rejected(self):
        with devlog.writer_lock(self.root):
            with self.assertRaisesRegex(devlog.DevlogError, "另一个"):
                self.change("start")

    def test_invalid_dependencies_and_unrecorded_block_are_rejected(self):
        invalid = copy.deepcopy(self.data)
        invalid["tasks"][0]["depends_on"] = ["B"]
        with self.assertRaisesRegex(devlog.DevlogError, "循环"):
            devlog.validate(invalid)
        invalid = copy.deepcopy(self.data)
        invalid["tasks"][0]["status"] = "blocked"
        with self.assertRaisesRegex(devlog.DevlogError, "历史"):
            devlog.validate(invalid)

    def test_batch_requires_current_round_completed(self):
        with self.assertRaises(devlog.DevlogError):
            devlog.change_batch(self.root, "B01", "Next reviewed round")
        self.change("start")
        self.complete()
        self.change("start", "B")
        self.complete("B")
        devlog.change_batch(self.root, "B01", "Next reviewed round")
        self.change("start", "C")
        data, _ = devlog.load(self.root)
        self.assertEqual(data["current_batch"], "B01")
        self.assertEqual(data["history"][-1]["to_batch"], "B01")


if __name__ == "__main__":
    unittest.main()
