"""Tests for the independent revision-feedback auditor (B08-T03)."""

from __future__ import annotations

import importlib.util
import json
import shutil
import sqlite3
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_revision_feedback.py"
PACKAGE = ROOT / "fixtures" / "revision-feedback-v1"
FIXED_AT = "2026-11-07T00:00:00Z"

CASES_FIELDS = (
    "schema_version",
    "producer",
    "case_id",
    "policy",
    "description",
    "sequence",
    "expected",
    "boundaries",
)


def load_auditor(module_name: str = "verify_revision_feedback"):
    spec = importlib.util.spec_from_file_location(module_name, SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def cases() -> dict:
    return json.loads((PACKAGE / "cases.json").read_text("utf-8"))


def build_root(directory: Path) -> Path:
    """Build a revision root with the auditor's own replay, without the product binary."""
    replay = V.Replay(PACKAGE)
    replay.run(cases()["sequence"])

    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    render_dir = root / "render"
    render_dir.mkdir()
    (control / "run.lock").write_bytes(b"")

    connection = sqlite3.connect(control / "revision.sqlite")
    connection.execute(f"PRAGMA application_id = {V.APPLICATION_ID}")
    connection.execute(f"PRAGMA user_version = {V.SCHEMA_VERSION}")
    connection.execute("PRAGMA journal_mode = DELETE")
    for definition in V.REQUIRED_TABLES.values():
        connection.execute(definition)

    metadata = {
        "policy": V.POLICY,
        "producer": V.PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay.author["case_id"],
        "author": replay.author,
        "corpus": replay.corpus,
        "author_manifest_sha256": V.sha256((PACKAGE / "author.json").read_bytes()),
        "corpus_manifest_sha256": V.sha256((PACKAGE / "corpus-manifest.json").read_bytes()),
        "created_at": FIXED_AT,
    }
    connection.execute(
        "INSERT INTO metadata(singleton, schema_version, metadata_json) VALUES (1, ?, ?)",
        (V.SCHEMA_VERSION, V.canonical(metadata)),
    )

    for position, gap_id in enumerate(replay.gap_order):
        record = dict(replay.gaps[gap_id])
        record["created_at"] = FIXED_AT
        record["updated_at"] = FIXED_AT
        connection.execute(
            "INSERT INTO gaps(gap_id, position, status, payload_json) VALUES (?, ?, ?, ?)",
            (gap_id, position, record["status"], V.canonical(record)),
        )

    for sequence, record in enumerate(replay.revisions):
        payload = dict(record)
        payload["at"] = FIXED_AT
        connection.execute(
            "INSERT INTO revisions(revision_id, sequence, gap_id, payload_json) VALUES (?, ?, ?, ?)",
            (payload["revision_id"], sequence + 1, payload["gap_id"], V.canonical(payload)),
        )

    final_renders = replay.history[-1]["renders"]
    for position, representation in enumerate(replay.representations):
        rid = representation["representation_id"]
        text = final_renders[rid]
        connection.execute(
            "INSERT INTO representations(representation_id, position, atom_ids_json, "
            "render_sha256, render) VALUES (?, ?, ?, ?, ?)",
            (
                rid,
                position,
                json.dumps(representation["atom_ids"], ensure_ascii=False),
                V.sha256(text.encode("utf-8")),
                text,
            ),
        )
        (render_dir / f"{rid}.md").write_text(text, encoding="utf-8")

    for record in replay.versions:
        payload = {
            "version": record["version"],
            "atoms_sha256": record["atoms_sha256"],
            "atoms": record["atoms"],
            "at": FIXED_AT,
        }
        connection.execute(
            "INSERT INTO ir_versions(version, atoms_sha256, payload_json, at) VALUES (?, ?, ?, ?)",
            (payload["version"], payload["atoms_sha256"], V.canonical(payload), payload["at"]),
        )
    connection.commit()
    connection.close()

    owner = {
        "policy": V.POLICY,
        "producer": V.PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": metadata["case_id"],
        "root": str(root.resolve()),
        "root_identity": {"platform": "unix", "device": 1, "inode": 1},
        "parent_identity": {"platform": "unix", "device": 1, "inode": 2},
        "control_identity": {"platform": "unix", "device": 1, "inode": 3},
        "owner_identity": {"platform": "unix", "device": 1, "inode": 4},
        "database_identity": {"platform": "unix", "device": 1, "inode": 5},
        "lock_identity": {"platform": "unix", "device": 1, "inode": 6},
        "author_manifest_sha256": metadata["author_manifest_sha256"],
        "corpus_manifest_sha256": metadata["corpus_manifest_sha256"],
        "created_at": metadata["created_at"],
    }
    (control / "owner.json").write_bytes(V.canonical(owner).encode("utf-8"))
    return root


def audit(root: Path, package: Path = PACKAGE):
    return V.audit(root, package)


def failed(report) -> set[str]:
    return {item["name"] for item in report.items if not item["ok"] and not item["warn"]}


class FrozenPackageTests(unittest.TestCase):
    def test_frozen_manifest_matches_members(self) -> None:
        report = V.Report()
        V.check_frozen(PACKAGE, report)
        self.assertEqual([], report.failures)

    def test_tampered_corpus_byte_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "package"
            shutil.copytree(PACKAGE, package)
            target = package / "corpus" / "chapter-pdc.md"
            target.write_bytes(target.read_bytes() + b"\n")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-bytes", failed(report))

    def test_extra_member_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "package"
            shutil.copytree(PACKAGE, package)
            (package / "unexpected.md").write_text("x", encoding="utf-8")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-file-set", failed(report))


class ReplayTests(unittest.TestCase):
    def test_replay_matches_frozen_expectations(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.run(cases()["sequence"])
        expected = cases()["expected"]
        observed = [
            {
                "revision_id": item["revision_id"],
                "sequence": index + 1,
                "gap_id": item["gap_id"],
                "ir_version": item["ir_version"],
                "recompiled": item["recompiled"],
                "unchanged": item["unchanged"],
                "evidence": len(item["evidence"]),
            }
            for index, item in enumerate(replay.revisions)
        ]
        self.assertEqual(observed, expected["revisions"])

    def test_replay_gap_states_match_expectations(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.run(cases()["sequence"])
        observed = [
            {
                "gap_id": replay.gaps[gap_id]["gap_id"],
                "status": replay.gaps[gap_id]["status"],
                "severity": replay.gaps[gap_id]["severity"],
                "revisions": replay.gaps[gap_id]["revisions"],
            }
            for gap_id in replay.gap_order
        ]
        self.assertEqual(observed, cases()["expected"]["gaps"])

    def test_replay_refusals_match_contract(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.run(cases()["sequence"])
        observed = [
            {"round": item["round"], "exit": item["exit"], "code": item["code"],
             "revision_id": item["revision_id"]}
            for item in replay.refusals
        ]
        self.assertEqual(observed, cases()["expected"]["refusals"])

    def test_unchanged_representations_keep_their_bytes(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.run(cases()["sequence"])
        for index, record in enumerate(replay.revisions):
            before = replay.history[index]["renders"]
            after = replay.history[index + 1]["renders"]
            for rid in record["unchanged"]:
                self.assertEqual(before[rid], after[rid], f"{record['revision_id']}:{rid}")

    def test_overturn_without_new_evidence_is_refused(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.adopt()
        replay.gap("gaps/gap-net-account.json")
        replay.revise("revisions/rev-net-1.json")
        replay.revise("revisions/rev-net-2.json")
        with self.assertRaises(V.Refusal) as caught:
            replay.revise("revisions/rev-net-3.json")
        self.assertEqual("REVISION_NO_NEW_EVIDENCE", caught.exception.code)
        self.assertEqual(3, caught.exception.exit_code)

    def test_escalated_gap_locks_automated_revisions(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.adopt()
        replay.gap("gaps/gap-net-account.json")
        replay.revise("revisions/rev-net-1.json")
        replay.revise("revisions/rev-net-2.json")
        replay.revise("revisions/rev-net-4.json")
        with self.assertRaises(V.Refusal) as caught:
            replay.revise("revisions/rev-net-5.json")
        self.assertEqual("REVISION_LIMIT_ESCALATED", caught.exception.code)

    def test_noop_revision_is_refused(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.adopt()
        replay.gap("gaps/gap-pdc-nadh.json")
        replay.revise("revisions/rev-pdc-1.json")
        with self.assertRaises(V.Refusal) as caught:
            replay.revise("revisions/rev-pdc-noop.json")
        self.assertEqual("REVISION_ARGUMENT", caught.exception.code)
        self.assertEqual(2, caught.exception.exit_code)

    def test_revision_without_sources_is_marked_pending(self) -> None:
        replay = V.Replay(PACKAGE)
        replay.adopt()
        replay.gap("gaps/gap-pdc-nadh.json")
        replay.revise("revisions/rev-pdc-1.json")
        text = replay.history[-1]["renders"]["rep-energy"]
        self.assertIn("【待核验】", text)
        self.assertIn("pending_verification", text)


class RootTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.mkdtemp(prefix="noetome-revision-audit-")
        self.addCleanup(shutil.rmtree, self.directory, True)
        self.root = build_root(Path(self.directory))

    def test_clean_root_passes_every_check(self) -> None:
        report = audit(self.root)
        self.assertEqual([], report.failures)
        self.assertGreaterEqual(len(report.items), 40)

    def test_tampered_render_bytes_are_detected(self) -> None:
        path = self.root / "render" / "rep-narrative.md"
        path.write_bytes(path.read_bytes().replace("4 步".encode(), "5 步".encode()))
        self.assertIn("replay-renders", failed(audit(self.root)))

    def test_extra_table_is_detected(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        connection.execute("CREATE TABLE extra(x INTEGER)")
        connection.commit()
        connection.close()
        self.assertIn("db-table-set", failed(audit(self.root)))

    def test_relaxed_table_definition_is_detected(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        connection.executescript(
            "ALTER TABLE gaps RENAME TO gaps_old;"
            "CREATE TABLE gaps(gap_id TEXT PRIMARY KEY,position INTEGER NOT NULL,"
            "status TEXT NOT NULL,payload_json TEXT NOT NULL);"
            "INSERT INTO gaps SELECT gap_id, position, status, payload_json FROM gaps_old;"
            "DROP TABLE gaps_old;"
        )
        connection.commit()
        connection.close()
        self.assertIn("db-table-gaps", failed(audit(self.root)))

    def test_gap_status_column_drift_is_detected(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        connection.execute("UPDATE gaps SET status = 'resolved' WHERE gap_id = 'gap-net-account'")
        connection.commit()
        connection.close()
        self.assertIn("db-gaps-columns", failed(audit(self.root)))

    def test_ir_payload_rewrite_is_detected(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        (payload,) = connection.execute(
            "SELECT payload_json FROM ir_versions WHERE version = 3"
        ).fetchone()
        record = json.loads(payload)
        record["atoms"][0]["value"] = "99"
        connection.execute(
            "UPDATE ir_versions SET payload_json = ? WHERE version = 3",
            (V.canonical(record),),
        )
        connection.commit()
        connection.close()
        self.assertIn("db-ir-atoms-sha256", failed(audit(self.root)))

    def test_non_canonical_owner_is_detected(self) -> None:
        path = self.root / "control" / "owner.json"
        raw = path.read_bytes()
        path.write_bytes(raw[:1] + b" " + raw[1:])
        self.assertIn("owner-strict", failed(audit(self.root)))

    def test_wal_journal_mode_is_detected(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        connection.execute("PRAGMA journal_mode = WAL")
        connection.commit()
        connection.close()
        self.assertIn("db-journal-mode", failed(audit(self.root)))

    def test_corpus_byte_drift_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "package"
            shutil.copytree(PACKAGE, package)
            target = package / "corpus" / "chapter-pdc.md"
            target.write_bytes(target.read_bytes() + b"x")
            report = audit(self.root, package)
            self.assertIn("metadata-corpus-bytes", failed(report))

    def test_extra_top_level_entry_is_detected(self) -> None:
        (self.root / "notes").mkdir()
        self.assertIn("layout-root", failed(audit(self.root)))

    def test_pending_marking_requires_the_marker_in_renders(self) -> None:
        connection = sqlite3.connect(self.root / "control" / "revision.sqlite")
        rows = connection.execute(
            "SELECT representation_id, render FROM representations"
        ).fetchall()
        for rid, text in rows:
            stripped = text.replace("【待核验】", "")
            connection.execute(
                "UPDATE representations SET render = ?, render_sha256 = ? "
                "WHERE representation_id = ?",
                (stripped, V.sha256(stripped.encode("utf-8")), rid),
            )
        connection.commit()
        connection.close()
        self.assertIn("replay-renders", failed(audit(self.root)))


if __name__ == "__main__":
    unittest.main()
