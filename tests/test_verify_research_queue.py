"""B08-T01 独立审计器（scripts/verify_research_queue.py）的对抗用例。

这些用例只检验审计器本身：它必须能在账本被篡改、布局被污染、字段被改写时
报出失败，而不是把伪造的现场当成合格队列。判定语义的重放部分单独覆盖。

样例是本测试自造的工程数据，不代表任何真实研究结果或科学正确性。
"""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_research_queue", ROOT / "scripts/verify_research_queue.py"
)
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/research-queue-v1"


def author_package() -> dict:
    return json.loads((FIXTURE / "author.json").read_bytes().decode("utf-8"))


def cases_package() -> dict:
    return json.loads((FIXTURE / "cases.json").read_bytes().decode("utf-8"))


def build_queue(directory: Path) -> Path:
    """用审计器的自主重放搭一个与控制目录同形的队列根（不调用产品 CLI）。

    这样审计器的用例不依赖产品实现是否可用：它测的是审计器能不能独立
    从冻结作者包与同形磁盘现场得出结论。
    """
    author = author_package()
    cases = cases_package()
    queue = V.replay(str(FIXTURE), author, cases["sequence"])

    root = directory / "queue"
    control = root / "control"
    control.mkdir(parents=True)
    (control / V.LOCK).write_bytes(b"")

    connection = sqlite3.connect(control / V.DATABASE)
    try:
        connection.execute(f"PRAGMA application_id = {V.APPLICATION_ID}")
        connection.execute(f"PRAGMA user_version = {V.SCHEMA_VERSION}")
        connection.execute("PRAGMA journal_mode = DELETE")
        for name, statement in V.REQUIRED_TABLES.items():
            connection.execute(statement)
        metadata = {
            "policy": V.POLICY,
            "producer": V.PRODUCER,
            "schema_version": V.SCHEMA_VERSION,
            "case_id": author["case_id"],
            "author_manifest_sha256": V.sha256((FIXTURE / "author.json").read_bytes()),
            "objective": author["objective"],
            "limits": author["limits"],
            "created_at": "2026-11-07T00:00:00Z",
        }
        connection.execute(
            "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(1,?,?)",
            (V.SCHEMA_VERSION, V.canonical(metadata)),
        )
        ordered = sorted(queue.candidates.items(), key=lambda item: item[0])
        for position, (_key, candidate) in enumerate(ordered):
            connection.execute(
                "INSERT INTO candidates(subject,position,state,payload_json) VALUES(?,?,?,?)",
                (candidate["subject"], position, candidate["state"], V.canonical(candidate)),
            )
        # 一个 adopt 事件 + 每个观测/决定步骤各一条事件，最后一条携带最终计数。
        steps = [step for step in cases["sequence"] if step["step"] in ("observe", "decide")]
        rows = [("adopt", "adopt", {"round": "adopt", "applied": [], "counters": {
            "observations": 0,
            "blocked_repeats": 0,
            "reopenings": 0,
            "merges": 0,
            "decisions": 0,
        }})]
        for index, step in enumerate(steps):
            payload = json.loads(
                (FIXTURE / step["input"]).read_bytes().decode("utf-8")
            )
            last = index == len(steps) - 1
            rows.append(
                (
                    step["step"],
                    payload["round"],
                    {
                        "round": payload["round"],
                        "applied": [],
                        "counters": queue.counters if last else {
                            "observations": 0,
                            "blocked_repeats": 0,
                            "reopenings": 0,
                            "merges": 0,
                            "decisions": 0,
                        },
                    },
                )
            )
        for offset, (kind, round_name, event) in enumerate(rows):
            connection.execute(
                "INSERT INTO events(kind,round,payload_json,at) VALUES(?,?,?,?)",
                (kind, round_name, V.canonical(event), "2026-11-07T00:00:%02dZ" % offset),
            )
        connection.commit()
    finally:
        connection.close()

    owner = {
        "policy": V.POLICY,
        "producer": V.PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": author["case_id"],
        "root": os.path.realpath(root),
        "root_identity": {"platform": "unix", "device": 1, "inode": 1},
        "parent_identity": {"platform": "unix", "device": 1, "inode": 2},
        "control_identity": {"platform": "unix", "device": 1, "inode": 3},
        "owner_identity": {"platform": "unix", "device": 1, "inode": 4},
        "database_identity": {"platform": "unix", "device": 1, "inode": 5},
        "lock_identity": {"platform": "unix", "device": 1, "inode": 6},
        "author_manifest_sha256": metadata["author_manifest_sha256"],
        "objective": author["objective"],
        "limits": author["limits"],
        "created_at": metadata["created_at"],
    }
    (control / V.OWNER).write_bytes(V.canonical(owner).encode("utf-8"))
    return root


def audit(root: Path, package: Path = FIXTURE) -> V.Report:
    report = V.Report()
    frozen = V.check_frozen(str(package), report)
    author = json.loads((package / "author.json").read_bytes().decode("utf-8"))
    cases = json.loads((package / "cases.json").read_bytes().decode("utf-8"))
    expected = dict(cases["expected"])
    expected["sequence_steps"] = [
        sum(1 for step in cases["sequence"] if step["step"] == "observe"),
        sum(1 for step in cases["sequence"] if step["step"] == "decide"),
    ]
    managed = V.check_layout(str(root), report)
    owner = None
    metadata = None
    candidates = []
    events = []
    if managed:
        owner = V.check_owner(str(root), str(package), report)
        metadata, candidates, events = V.check_database(str(root), report)
        if owner and metadata and events:
            V.check_queue(owner, metadata, candidates, events, expected, report)
            V.check_replay(str(package), author, cases, candidates, events, report)
    report.record("frozen-anchor", frozen is not None, "冻结清单存在并逐字节核对")
    return report


def failed_names(report: V.Report) -> list[str]:
    return [check["check"] for check in report.checks if check["status"] == "failed"]


class FrozenPackageTests(unittest.TestCase):
    def test_frozen_manifest_matches_every_member_byte(self):
        report = V.Report()
        frozen = V.check_frozen(str(FIXTURE), report)
        self.assertIsNotNone(frozen)
        self.assertEqual(report.failures, 0, failed_names(report))
        self.assertEqual(frozen["workflow_id"], V.POLICY)
        self.assertEqual(frozen["producer"], "independent_fixture_author_freeze")

    def test_changed_member_byte_breaks_the_frozen_anchor(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "research-queue-v1"
            shutil.copytree(FIXTURE, package)
            target = package / "round-02-decisions.json"
            payload = json.loads(target.read_bytes().decode("utf-8"))
            payload["round"] = "round-02-forged"
            target.write_bytes(json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8"))

            report = V.Report()
            V.check_frozen(str(package), report)
            self.assertGreater(report.failures, 0, "改动成员后冻结锚必须失效")

    def test_added_member_breaks_the_frozen_file_set(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "research-queue-v1"
            shutil.copytree(FIXTURE, package)
            (package / "extra-note.md").write_bytes("自造笔记".encode("utf-8"))

            report = V.Report()
            V.check_frozen(str(package), report)
            self.assertIn("frozen-file-set", failed_names(report))


class LayoutTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = build_queue(Path(self.directory.name))

    def test_independently_built_queue_passes_every_check(self):
        report = audit(self.root)
        self.assertEqual(report.failures, 0, failed_names(report))
        self.assertGreater(len(report.checks), 100, "审计器必须真的逐项检查，而不是只报总数")

    def test_extra_entry_in_control_is_refused(self):
        (self.root / "control" / "notes.txt").write_bytes(b"x")
        report = audit(self.root)
        self.assertIn("layout-control", failed_names(report))

    def test_extra_entry_at_root_is_refused(self):
        (self.root / "vault").mkdir()
        report = audit(self.root)
        self.assertIn("layout-root", failed_names(report))

    def test_database_sidecar_is_not_silently_accepted(self):
        (self.root / "control" / "research.sqlite-journal").write_bytes(b"")
        report = audit(self.root)
        self.assertIn("layout-control", failed_names(report))


class LedgerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = build_queue(Path(self.directory.name))
        self.database = self.root / "control" / V.DATABASE

    def execute(self, statement, parameters=()):
        connection = sqlite3.connect(self.database)
        try:
            connection.execute(statement, parameters)
            connection.commit()
        finally:
            connection.close()

    def test_application_id_tamper_is_detected(self):
        self.execute("PRAGMA application_id = 1")
        report = audit(self.root)
        self.assertIn("db-application-id", failed_names(report))

    def test_user_version_tamper_is_detected(self):
        self.execute("PRAGMA user_version = 2")
        report = audit(self.root)
        self.assertIn("db-user-version", failed_names(report))

    def test_wal_journal_mode_is_detected(self):
        connection = sqlite3.connect(self.database)
        try:
            connection.execute("PRAGMA journal_mode = WAL")
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertIn("db-journal-mode", failed_names(report))

    def test_extra_table_is_detected(self):
        self.execute("CREATE TABLE shadow(x TEXT)")
        report = audit(self.root)
        self.assertIn("db-tables", failed_names(report))

    def test_dropped_check_constraint_is_detected(self):
        # 同名字段、但定义里少了 CHECK：靠逐字比对表定义才能发现。
        self.execute("DROP TABLE candidates")
        self.execute(
            "CREATE TABLE candidates(subject TEXT PRIMARY KEY,position INTEGER NOT NULL,state TEXT NOT NULL,payload_json TEXT NOT NULL)"
        )
        report = audit(self.root)
        self.assertIn("db-table-candidates", failed_names(report))

    def test_single_row_constraint_cannot_be_silently_relaxed(self):
        # 产品用 CHECK(singleton=1) 保证单行；把约束去掉再塞两行是典型篡改。
        # 审计器靠表定义逐字比对发现它，而不是只看行数。
        connection = sqlite3.connect(self.database)
        try:
            raw, = connection.execute("SELECT metadata_json FROM metadata").fetchone()
            connection.execute("DROP TABLE metadata")
            connection.execute(
                "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY,"
                "schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL)"
            )
            for singleton in (1, 2):
                connection.execute(
                    "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(?,?,?)",
                    (singleton, V.SCHEMA_VERSION, raw),
                )
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertIn("db-table-metadata", failed_names(report))
        self.assertIn("db-metadata-rows", failed_names(report))

    def test_metadata_with_extra_field_is_detected(self):
        connection = sqlite3.connect(self.database)
        try:
            raw = connection.execute("SELECT metadata_json FROM metadata").fetchone()[0]
            payload = json.loads(raw)
            payload["unexpected"] = "x"
            connection.execute(
                "UPDATE metadata SET metadata_json=?", (V.canonical(payload),)
            )
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertIn("db-metadata-strict", failed_names(report))

    def test_non_canonical_metadata_is_detected(self):
        connection = sqlite3.connect(self.database)
        try:
            raw = connection.execute("SELECT metadata_json FROM metadata").fetchone()[0]
            connection.execute("UPDATE metadata SET metadata_json=?", (" " + raw,))
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertIn("db-metadata-strict", failed_names(report))

    def test_candidate_subject_key_must_match_its_column(self):
        connection = sqlite3.connect(self.database)
        try:
            subject, payload = connection.execute(
                "SELECT subject,payload_json FROM candidates ORDER BY position LIMIT 1"
            ).fetchone()
            forged = json.loads(payload)
            forged["subject"] = forged["subject"] + "（改写）"
            connection.execute(
                "UPDATE candidates SET payload_json=? WHERE subject=?",
                (V.canonical(forged), subject),
            )
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertTrue(
            any(name.startswith("db-candidate-key:") for name in failed_names(report)),
            failed_names(report),
        )

    def test_candidate_state_column_must_match_payload(self):
        connection = sqlite3.connect(self.database)
        try:
            subject = connection.execute(
                "SELECT subject FROM candidates ORDER BY position LIMIT 1"
            ).fetchone()[0]
            connection.execute(
                "UPDATE candidates SET state='accepted' WHERE subject=?", (subject,)
            )
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertTrue(
            any(name.startswith("db-candidate-key:") for name in failed_names(report))
        )

    def test_broken_position_sequence_is_detected(self):
        connection = sqlite3.connect(self.database)
        try:
            subject = connection.execute(
                "SELECT subject FROM candidates ORDER BY position DESC LIMIT 1"
            ).fetchone()[0]
            connection.execute(
                "UPDATE candidates SET position=99 WHERE subject=?", (subject,)
            )
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertIn("db-candidate-positions", failed_names(report))

    def test_owner_root_must_match_this_directory(self):
        path = self.root / "control" / V.OWNER
        owner = json.loads(path.read_bytes().decode("utf-8"))
        owner["root"] = "/elsewhere"
        path.write_bytes(V.canonical(owner).encode("utf-8"))
        report = audit(self.root)
        self.assertIn("owner-root", failed_names(report))

    def test_owner_author_digest_must_match_frozen_package(self):
        path = self.root / "control" / V.OWNER
        owner = json.loads(path.read_bytes().decode("utf-8"))
        owner["author_manifest_sha256"] = "0" * 64
        path.write_bytes(V.canonical(owner).encode("utf-8"))
        report = audit(self.root)
        self.assertIn("owner-author-bytes", failed_names(report))

    def test_owner_metadata_disagreement_is_detected(self):
        path = self.root / "control" / V.OWNER
        owner = json.loads(path.read_bytes().decode("utf-8"))
        owner["limits"] = dict(owner["limits"])
        owner["limits"]["max_candidates"] = 7
        path.write_bytes(V.canonical(owner).encode("utf-8"))
        report = audit(self.root)
        self.assertIn("owner-metadata-limits", failed_names(report))

    def test_counters_must_match_the_replayed_contract(self):
        connection = sqlite3.connect(self.database)
        try:
            raw = connection.execute("SELECT payload_json FROM events").fetchone()[0]
            payload = json.loads(raw)
            payload["counters"]["blocked_repeats"] = 0
            connection.execute("UPDATE events SET payload_json=?", (V.canonical(payload),))
            connection.commit()
        finally:
            connection.close()
        report = audit(self.root)
        self.assertTrue(
            any("counter" in name or "replay" in name for name in failed_names(report)),
            failed_names(report),
        )


class ReplayTests(unittest.TestCase):
    def test_replay_counts_match_the_frozen_expectations(self):
        author = author_package()
        cases = cases_package()
        queue = V.replay(str(FIXTURE), author, cases["sequence"])
        expected = cases["expected"]
        self.assertEqual(queue.counters, expected["counters"])
        self.assertEqual(queue.must_do, set(expected["must_do"]))
        self.assertEqual(queue.queue_only(), set(expected["queue_only"]))
        for subject, count in expected["evidence"].items():
            self.assertEqual(
                len(queue.candidates[subject]["evidence"]), count, subject
            )

    def test_repeated_topic_merge_and_scope_revision_are_tracked(self):
        author = author_package()
        cases = cases_package()
        queue = V.replay(str(FIXTURE), author, cases["sequence"])
        rejected = queue.candidates["乳酸穿梭与科里循环"]
        self.assertEqual(rejected["state"], "proposed")
        self.assertEqual(len(rejected["rejections"]), 1)
        self.assertEqual(len(rejected["scope_revisions"]), 1)
        merged = queue.candidates["磷酸戊糖途径产物去向"]
        self.assertEqual(merged["state"], "merged")
        self.assertEqual(merged["merged_into"], "脂肪酸β氧化能量账")

    def test_out_of_scope_target_is_never_promoted_by_decisions(self):
        author = author_package()
        cases = cases_package()
        queue = V.replay(str(FIXTURE), author, cases["sequence"])
        for subject in queue.must_do:
            candidate = queue.candidates[subject]
            self.assertNotEqual(candidate["scope_class"], "out_of_scope")

    def test_decision_about_unknown_subject_is_rejected(self):
        author = author_package()
        queue = V.Queue(author)
        with self.assertRaises(ValueError):
            queue.decide(
                {
                    "subject": "从未观测过的主题",
                    "action": "accept",
                    "reason": "凭空接受",
                    "merged_into": None,
                    "scope_note": None,
                }
            )

    def test_merge_into_itself_is_rejected(self):
        author = author_package()
        cases = cases_package()
        queue = V.replay(str(FIXTURE), author, cases["sequence"])
        with self.assertRaises(ValueError):
            queue.decide(
                {
                    "subject": "脂肪酸β氧化能量账",
                    "action": "merge",
                    "reason": "自合并",
                    "merged_into": "脂肪酸β氧化能量账",
                    "scope_note": None,
                }
            )


class StrictLoadTests(unittest.TestCase):
    def test_duplicate_json_keys_are_not_a_record(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            path.write_bytes(b'{"round":"a","round":"b"}')
            with self.assertRaises(ValueError):
                V.strict_load(str(path), ("round",), "record")

    def test_non_canonical_whitespace_is_not_a_record(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            path.write_bytes(b'{"round": "round-01"}')
            with self.assertRaises(ValueError):
                V.strict_load(str(path), ("round",), "record")

    def test_unknown_field_is_not_a_record(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            path.write_bytes(b'{"round":"round-01","extra":1}')
            with self.assertRaises(ValueError):
                V.strict_load(str(path), ("round",), "record")


if __name__ == "__main__":
    unittest.main()
