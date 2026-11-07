"""B08-T02 独立审计器（scripts/verify_retrieval_budget.py）的对抗用例。

这些用例只检验审计器本身：它必须能在账本被篡改、布局被污染、预算被偷偷回收、
终止理由缺乏证据时报出失败，而不是把伪造的现场当成合格检索根。

样例是本测试自造的工程数据，不代表任何真实检索结果或科学正确性。
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
    "verify_retrieval_budget", ROOT / "scripts/verify_retrieval_budget.py"
)
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/retrieval-budget-v1"


def package_plan(name: str = "plan.json") -> dict:
    return json.loads((FIXTURE / name).read_bytes().decode("utf-8"))


def package_cases() -> dict:
    return json.loads((FIXTURE / "cases.json").read_bytes().decode("utf-8"))


def build_root(directory: Path, plan_name: str = "plan.json") -> Path:
    """用审计器的自主重放搭一个与检索根同形的现场（不调用产品 CLI）。"""
    data = V.replay_package(str(FIXTURE), plan_name)
    plan = data["plan"]
    corpus = data["corpus"]
    replay = V.Replay(plan, corpus, data["documents"])
    replay.run()

    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    (control / V.LOCK).write_bytes(b"")

    connection = sqlite3.connect(control / V.DATABASE)
    try:
        connection.execute(f"PRAGMA application_id = {V.APPLICATION_ID}")
        connection.execute(f"PRAGMA user_version = {V.SCHEMA_VERSION}")
        connection.execute("PRAGMA journal_mode = DELETE")
        for statement in V.REQUIRED_TABLES.values():
            connection.execute(statement)
        metadata = {
            "policy": V.POLICY,
            "producer": V.PRODUCER,
            "schema_version": V.SCHEMA_VERSION,
            "case_id": plan["case_id"],
            "plan": plan,
            "corpus": corpus,
            "corpus_root": os.path.realpath(FIXTURE / plan["corpus"]["root"]),
            "plan_manifest_sha256": V.sha256((FIXTURE / plan_name).read_bytes()),
            "corpus_manifest_sha256": V.sha256(
                (FIXTURE / plan["corpus"]["manifest"]).read_bytes()
            ),
            "created_at": "2026-11-07T00:00:00Z",
        }
        connection.execute(
            "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(1,?,?)",
            (V.SCHEMA_VERSION, V.canonical(metadata)),
        )
        for position, unit in enumerate(replay.units):
            connection.execute(
                "INSERT INTO units(query_id,position,state,payload_json) VALUES(?,?,?,?)",
                (unit["query_id"], position, unit["state"], V.canonical(unit)),
            )
        for attempt in replay.attempts:
            record = dict(attempt)
            record["at"] = "2026-11-07T00:00:%02dZ" % record["attempt"]
            connection.execute(
                "INSERT INTO attempts(attempt,query_id,state,payload_json) VALUES(?,?,?,?)",
                (record["attempt"], record["query_id"], record["state"], V.canonical(record)),
            )
        connection.commit()
    finally:
        connection.close()

    owner = {
        "policy": V.POLICY,
        "producer": V.PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": plan["case_id"],
        "root": os.path.realpath(root),
        "root_identity": {"platform": "unix", "device": 1, "inode": 1},
        "parent_identity": {"platform": "unix", "device": 1, "inode": 2},
        "control_identity": {"platform": "unix", "device": 1, "inode": 3},
        "owner_identity": {"platform": "unix", "device": 1, "inode": 4},
        "database_identity": {"platform": "unix", "device": 1, "inode": 5},
        "lock_identity": {"platform": "unix", "device": 1, "inode": 6},
        "plan_manifest_sha256": metadata["plan_manifest_sha256"],
        "corpus_manifest_sha256": metadata["corpus_manifest_sha256"],
        "created_at": metadata["created_at"],
    }
    (control / V.OWNER).write_bytes(V.canonical(owner).encode("utf-8"))
    return root


def audit(root: Path, package: Path = FIXTURE, plan_name: str = "plan.json") -> V.Report:
    """复刻审计器 main() 的检查序列，供用例直接调用。"""
    report = V.Report()
    data = V.replay_package(str(package), plan_name)
    plan = data["plan"]
    corpus = data["corpus"]

    V.check_layout(str(root), report)
    owner = V.check_owner(str(root), report, str(package), plan_name)
    connection = V.check_database(str(root), report)
    metadata = V.read_metadata(connection, report, owner) if connection is not None else None
    if metadata is not None:
        report.record(
            "db-metadata-plan",
            V.normalise_plan(metadata["plan"]) == V.normalise_plan(plan),
            "账本里的计划必须与作者包 plan.json 语义一致",
        )
        report.record(
            "db-metadata-corpus",
            V.canonical(metadata["corpus"]) == V.canonical(corpus),
            "账本里的语料清单必须与 corpus-manifest.json 完全一致",
        )
    units = V.read_units(connection, report) if connection is not None else []
    attempts = V.read_attempts(connection, report) if connection is not None else []
    if connection is not None:
        V.check_ledger_consistency(units, attempts, plan, report)
        view = V.check_budget(attempts, plan, report)
        V.check_termination(connection, units, attempts, plan, view, report)
        connection.close()

    expected = package_cases()["expected"][plan_name]
    V.check_replay(units, attempts, expected, report)
    V.check_no_network(str(root), report)
    V.check_frozen(str(package), report)

    replay = V.Replay(plan, corpus, data["documents"])
    steps = replay.run()
    outcomes = [step.get("outcome") for step in steps if not step.get("budget_exhausted")]
    report.record(
        "independent-replay-outcomes",
        outcomes == expected["replay"]["outcomes"],
        f"独立重放 {outcomes}，先验 {expected['replay']['outcomes']}",
    )
    return report


class FrozenPackageTests(unittest.TestCase):
    def test_frozen_manifest_matches_every_member_byte(self):
        report = V.Report()
        V.check_frozen(str(FIXTURE), report)
        self.assertEqual(report.failures, [], report.failures)

    def test_changed_corpus_byte_breaks_the_frozen_anchor(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "retrieval-budget-v1"
            shutil.copytree(FIXTURE, package)
            target = package / "corpus/chapter-glycolysis.md"
            target.write_bytes(target.read_bytes() + b"\n# tampered\n")
            report = V.Report()
            V.check_frozen(str(package), report)
            self.assertTrue(
                any("frozen-sha-corpus/chapter-glycolysis.md" in failure for failure in report.failures),
                report.failures,
            )

    def test_extra_member_breaks_the_file_set(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "retrieval-budget-v1"
            shutil.copytree(FIXTURE, package)
            (package / "extra.md").write_bytes(b"extra\n")
            report = V.Report()
            V.check_frozen(str(package), report)
            self.assertTrue(
                any("frozen-file-set" in failure for failure in report.failures),
                report.failures,
            )


class CleanRootTests(unittest.TestCase):
    def test_independently_built_root_passes_every_check(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            report = audit(root)
            self.assertEqual(report.failures, [], report.failures)
            self.assertGreater(report.passed, 60)

    def test_tight_plan_root_passes_every_check(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory), "plan-tight.json")
            report = audit(root, plan_name="plan-tight.json")
            self.assertEqual(report.failures, [], report.failures)


class LayooutTests(unittest.TestCase):
    def test_extra_top_level_entry_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            (root / "notes.txt").write_bytes(b"foreign\n")
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("layout-root") for failure in report.failures),
                report.failures,
            )

    def test_extra_control_file_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            (root / "control/extra.json").write_bytes(b"{}\n")
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("layout-control") for failure in report.failures),
                report.failures,
            )


class LedgerTests(unittest.TestCase):
    def mutate(self, sql: str) -> V.Report:
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                connection.executescript(sql)
                connection.commit()
            finally:
                connection.close()
            return audit(root)

    def assert_failed(self, report: V.Report, prefix: str) -> None:
        self.assertTrue(
            any(failure.startswith(prefix) for failure in report.failures),
            report.failures,
        )

    def test_wal_journal_mode_is_detected(self):
        report = self.mutate("PRAGMA journal_mode = WAL")
        self.assert_failed(report, "db-journal-mode")

    def test_user_version_drift_is_detected(self):
        report = self.mutate("PRAGMA user_version = 2")
        self.assert_failed(report, "db-user-version")

    def test_application_id_drift_is_detected(self):
        report = self.mutate("PRAGMA application_id = 1")
        self.assert_failed(report, "db-application-id")

    def test_missing_table_is_detected(self):
        report = self.mutate("DROP TABLE termination")
        self.assert_failed(report, "db-table-set")

    def test_relaxed_check_constraint_is_detected(self):
        report = self.mutate(
            "CREATE TABLE controls_relaxed(sequence INTEGER PRIMARY KEY AUTOINCREMENT,"
            "action TEXT NOT NULL,at TEXT NOT NULL);"
            "DROP TABLE controls;"
            "ALTER TABLE controls_relaxed RENAME TO controls;"
        )
        self.assert_failed(report, "db-table-controls")

    def test_budget_recycling_is_detected(self):
        # 把某次预留的字节数改小：等于把已花额度收回，必须被抓到。
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute(
                    "SELECT attempt,payload_json FROM attempts ORDER BY attempt LIMIT 1"
                ).fetchone()
                attempt = json.loads(row[1])
                attempt["reserved_bytes"] = 1
                connection.execute(
                    "UPDATE attempts SET payload_json = ? WHERE attempt = ?",
                    (V.canonical(attempt), row[0]),
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assert_failed(report, "budget-reservation-shape")

    def test_double_settlement_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute(
                    "SELECT attempt,query_id,state,payload_json FROM attempts ORDER BY attempt LIMIT 1"
                ).fetchone()
                clone = json.loads(row[3])
                clone["attempt"] = 999
                connection.execute(
                    "INSERT INTO attempts(attempt,query_id,state,payload_json) VALUES(?,?,?,?)",
                    (999, row[1], row[2], V.canonical(clone)),
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assertTrue(
                any("ledger-no-double-settle" in failure for failure in report.failures),
                report.failures,
            )

    def test_pending_unit_with_settled_attempt_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute(
                    "SELECT position,payload_json FROM units ORDER BY position LIMIT 1"
                ).fetchone()
                unit = json.loads(row[1])
                unit["state"] = V.UNIT_PENDING
                connection.execute(
                    "UPDATE units SET state = ?, payload_json = ? WHERE position = ?",
                    (V.UNIT_PENDING, V.canonical(unit), row[0]),
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assertTrue(
                any("ledger-pending-not-attempted" in failure for failure in report.failures),
                report.failures,
            )

    def test_unit_state_column_drift_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute(
                    "SELECT query_id FROM units ORDER BY position LIMIT 1"
                ).fetchone()
                connection.execute(
                    "UPDATE units SET state = 'gapped' WHERE query_id = ?", (row[0],)
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("db-unit-state-") for failure in report.failures),
                report.failures,
            )

    def test_non_canonical_metadata_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute("SELECT metadata_json FROM metadata").fetchone()
                spaced = row[0].replace("{", "{ ", 1)
                connection.execute(
                    "UPDATE metadata SET metadata_json = ?", (spaced,)
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assert_failed(report, "db-metadata-strict")

    def test_unknown_metadata_field_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute("SELECT metadata_json FROM metadata").fetchone()
                metadata = json.loads(row[0])
                metadata["unexpected"] = True
                connection.execute(
                    "UPDATE metadata SET metadata_json = ?", (V.canonical(metadata),)
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assert_failed(report, "db-metadata-strict")

    def test_metadata_plan_drift_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                row = connection.execute("SELECT metadata_json FROM metadata").fetchone()
                metadata = json.loads(row[0])
                metadata["plan"]["queries"][0]["terms"] = ["changed"]
                connection.execute(
                    "UPDATE metadata SET metadata_json = ?", (V.canonical(metadata),)
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root)
            self.assert_failed(report, "db-metadata-plan")


class OwnerTests(unittest.TestCase):
    def test_owner_tampering_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            path = root / "control" / V.OWNER
            bytes_value = path.read_bytes()
            path.write_bytes(bytes_value[:1] + b" " + bytes_value[1:])
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("owner-strict") for failure in report.failures),
                report.failures,
            )

    def test_owner_plan_digest_drift_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            path = root / "control" / V.OWNER
            owner = json.loads(path.read_bytes().decode("utf-8"))
            owner["plan_manifest_sha256"] = "0" * 64
            path.write_bytes(V.canonical(owner).encode("utf-8"))
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("owner-plan-sha256") for failure in report.failures),
                report.failures,
            )

    def test_owner_root_drift_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            path = root / "control" / V.OWNER
            owner = json.loads(path.read_bytes().decode("utf-8"))
            owner["root"] = "/elsewhere"
            path.write_bytes(V.canonical(owner).encode("utf-8"))
            report = audit(root)
            self.assertTrue(
                any(failure.startswith("owner-root") for failure in report.failures),
                report.failures,
            )


class TerminationTests(unittest.TestCase):
    def record(self, reason: str, note: str = "说明") -> V.Report:
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory))
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                connection.execute(
                    "INSERT INTO termination(singleton,reason,note,at) VALUES(1,?,?,?)",
                    (reason, note, "2026-11-07T00:00:00Z"),
                )
                connection.commit()
            finally:
                connection.close()
            return audit(root)

    def test_goal_met_without_missing_evidence_passes(self):
        report = self.record(V.REASON_GOAL_MET)
        self.assertEqual(report.failures, [], report.failures)

    def test_unknown_reason_is_detected(self):
        report = self.record("because_i_said_so")
        self.assertTrue(
            any(failure.startswith("termination-known") for failure in report.failures),
            report.failures,
        )

    def test_empty_note_is_detected(self):
        report = self.record(V.REASON_GOAL_MET, note="")
        self.assertTrue(
            any(failure.startswith("termination-note") for failure in report.failures),
            report.failures,
        )

    def test_budget_reason_without_pending_work_is_detected(self):
        # 计划已全部结算，此时记「预算不足」没有依据。
        report = self.record(V.REASON_BUDGET)
        self.assertTrue(
            any(failure.startswith("termination-budget-pending") for failure in report.failures),
            report.failures,
        )

    def test_network_reason_without_external_query_is_detected(self):
        # plan-tight 只跑到第 4 条，尚未出现需要外部来源的查询。
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory), "plan-tight.json")
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                connection.execute(
                    "INSERT INTO termination(singleton,reason,note,at) VALUES(1,?,?,?)",
                    (V.REASON_NETWORK, "没有网络", "2026-11-07T00:00:00Z"),
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root, plan_name="plan-tight.json")
            self.assertTrue(
                any(
                    failure.startswith("termination-network-external-query")
                    for failure in report.failures
                ),
                report.failures,
            )

    def test_paused_reason_without_pause_control_is_detected(self):
        report = self.record(V.REASON_PAUSED)
        self.assertTrue(
            any(failure.startswith("termination-paused") for failure in report.failures),
            report.failures,
        )

    def test_manual_reason_without_conflict_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = build_root(Path(directory), "plan-tight.json")
            connection = sqlite3.connect(root / "control" / V.DATABASE)
            try:
                connection.execute("PRAGMA journal_mode = DELETE")
                connection.execute(
                    "INSERT INTO termination(singleton,reason,note,at) VALUES(1,?,?,?)",
                    (V.REASON_MANUAL, "需要判断", "2026-11-07T00:00:00Z"),
                )
                connection.commit()
            finally:
                connection.close()
            report = audit(root, plan_name="plan-tight.json")
            self.assertTrue(
                any(failure.startswith("termination-conflict") for failure in report.failures),
                report.failures,
            )


class StrictLoadTests(unittest.TestCase):
    def test_unknown_field_is_rejected(self):
        with self.assertRaises(ValueError):
            V.strict_load(b'{"a":1,"b":2}', ("a",), "示例")

    def test_missing_field_is_rejected(self):
        with self.assertRaises(ValueError):
            V.strict_load(b'{"a":1}', ("a", "b"), "示例")

    def test_non_canonical_text_is_rejected_when_required(self):
        with self.assertRaises(ValueError):
            V.strict_load(b'{"a": 1}', ("a",), "示例")

    def test_non_canonical_text_is_accepted_when_optional(self):
        self.assertEqual(
            V.strict_load(b'{"a": 1}', ("a",), "示例", canonical_required=False),
            {"a": 1},
        )


if __name__ == "__main__":
    unittest.main()
