#!/usr/bin/env python3
"""B08-T02 独立检查器：只读磁盘、SQLite 与冻结语料，独立重放有界检索并核对预算与终止理由。

它不调用 CLI 的任何判定函数，也不 import Rust 侧代码：命中、去重、冲突、预算预留、
单位状态与终止理由的证据支撑全部由本文件从 `plan.json`、`corpus-manifest.json`、
`corpus/` 与 `control/` 下的 owner.json、retrieval.sqlite、run.lock 重新计算，
再与 `cases.json` 的先验预期逐条对照。

用法：
    python3 scripts/verify_retrieval_budget.py <检索根> --package <作者包目录> [--json]
    python3 scripts/verify_retrieval_budget.py --package <作者包目录> --freeze

退出码：0 = 全部硬检查通过；1 = 有硬检查失败；2 = 用法错误。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sqlite3
import sys
from typing import Any

POLICY = "retrieval-plan-v1"
CORPUS_POLICY = "retrieval-corpus-v1"
PRODUCER = "noetome-retrieval-budget"
AUTHOR_PRODUCER = "fixture_author"
SCHEMA_VERSION = 1
APPLICATION_ID = 0x4E525452
MAX_SNIPPET = 160

CONTROL = "control"
OWNER = "owner.json"
DATABASE = "retrieval.sqlite"
LOCK = "run.lock"

SCOPE_AUTHORIZED = "authorized_target"
SCOPE_PREREQUISITE = "required_prerequisite"
SCOPES = (SCOPE_AUTHORIZED, SCOPE_PREREQUISITE)

UNIT_PENDING = "pending"
UNIT_SETTLED = "settled"
UNIT_GAPPED = "gapped"
UNITS = (UNIT_PENDING, UNIT_SETTLED, UNIT_GAPPED)

ATTEMPT_RESERVED = "reserved"
ATTEMPT_SETTLED = "settled"

OUTCOME_EVIDENCE = "evidence"
OUTCOME_NO_HITS = "no_hits"
OUTCOME_DUPLICATES = "duplicates_only"
OUTCOME_EXTERNAL = "requires_external"
OUTCOME_CONFLICT = "conflict"
OUTCOMES = (
    OUTCOME_EVIDENCE,
    OUTCOME_NO_HITS,
    OUTCOME_DUPLICATES,
    OUTCOME_EXTERNAL,
    OUTCOME_CONFLICT,
)

REASON_GOAL_MET = "goal_met"
REASON_NO_NEW_EVIDENCE = "no_reliable_new_evidence"
REASON_MARGINAL_GAIN = "marginal_gain_insufficient"
REASON_BUDGET = "budget_exhausted"
REASON_PAUSED = "user_paused"
REASON_NETWORK = "network_restricted"
REASON_MANUAL = "manual_judgment_required"
REASONS = (
    REASON_GOAL_MET,
    REASON_NO_NEW_EVIDENCE,
    REASON_MARGINAL_GAIN,
    REASON_BUDGET,
    REASON_PAUSED,
    REASON_NETWORK,
    REASON_MANUAL,
)

REQUIRED_TABLES = {
    "metadata": (
        "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),"
        "schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL)"
    ),
    "units": (
        "CREATE TABLE units(query_id TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),"
        "state TEXT NOT NULL,payload_json TEXT NOT NULL)"
    ),
    "attempts": (
        "CREATE TABLE attempts(attempt INTEGER PRIMARY KEY CHECK(attempt>0),"
        "query_id TEXT NOT NULL,state TEXT NOT NULL,payload_json TEXT NOT NULL)"
    ),
    "controls": (
        "CREATE TABLE controls(sequence INTEGER PRIMARY KEY AUTOINCREMENT,"
        "action TEXT NOT NULL CHECK(action IN ('pause','resume')),at TEXT NOT NULL)"
    ),
    "termination": (
        "CREATE TABLE termination(singleton INTEGER PRIMARY KEY CHECK(singleton=1),"
        "reason TEXT NOT NULL,note TEXT NOT NULL,at TEXT NOT NULL)"
    ),
}

OWNER_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "root",
    "root_identity",
    "parent_identity",
    "control_identity",
    "owner_identity",
    "database_identity",
    "lock_identity",
    "plan_manifest_sha256",
    "corpus_manifest_sha256",
    "created_at",
)

METADATA_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "plan",
    "corpus",
    "corpus_root",
    "plan_manifest_sha256",
    "corpus_manifest_sha256",
    "created_at",
)

UNIT_FIELDS = ("query_id", "position", "state", "query")
QUERY_FIELDS = (
    "query_id",
    "subject",
    "terms",
    "scope_class",
    "requires_external",
    "conflict_group",
)
ATTEMPT_FIELDS = (
    "attempt",
    "query_id",
    "reserved_queries",
    "reserved_bytes",
    "reserved_millis",
    "state",
    "settled_bytes",
    "hits",
    "new_hits",
    "outcome",
    "hit_keys",
    "at",
)


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.warnings: list[str] = []
        self.passed = 0

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        if ok:
            self.passed += 1
        else:
            self.failures.append(f"{name}: {detail}" if detail else name)

    def fail(self, name: str, detail: str) -> None:
        self.failures.append(f"{name}: {detail}")

    def ok(self, name: str, detail: str = "") -> None:
        self.passed += 1
        if detail:
            self.warnings.append(f"{name}: {detail}")

    def dump(self) -> dict[str, Any]:
        return {
            "passed": self.passed,
            "failures": self.failures,
            "warnings": self.warnings,
        }


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as handle:
        return handle.read()


def canonical(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def collapse(text: str) -> str:
    return "".join(text.split())


def strict_load(
    payload: bytes,
    fields: tuple[str, ...],
    label: str,
    *,
    canonical_required: bool = True,
) -> dict[str, Any]:
    """严格解析：UTF-8、字段集完全一致、（可选）必须是规范编码。"""
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} 必须是 UTF-8 文本：{error}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"{label} 不是严格 JSON：{error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{label} 必须是 JSON 对象，实际 {type(value).__name__}")
    keys = set(value)
    expected = set(fields)
    if keys != expected:
        extra = sorted(keys - expected)
        missing = sorted(expected - keys)
        raise ValueError(f"{label} 字段集不符：多 {extra}、缺 {missing}")
    if canonical_required and canonical(value) != text:
        raise ValueError(f"{label} 非规范编码、重复键或字段顺序不为规范序")
    return value


# ---------------------------------------------------------------------------
# 独立重放
# ---------------------------------------------------------------------------


def snippet(line: str) -> str:
    text = line.strip()
    if len(text) <= MAX_SNIPPET:
        return text
    return text[:MAX_SNIPPET]


def scan(documents: list[tuple[str, bytes]], terms: list[str], max_hits: int) -> list[dict[str, Any]]:
    """确定性扫描：按清单顺序与行顺序，大小写无关子串匹配，跟踪 # 标题。"""
    hits: list[dict[str, Any]] = []
    for path, raw in documents:
        text = raw.decode("utf-8", errors="replace")
        heading = ""
        offset = 0
        lines = text.split("\n")
        for index, line in enumerate(lines):
            with_newline = line if index == len(lines) - 1 else line + "\n"
            stripped = with_newline.lstrip()
            if stripped.startswith("#"):
                heading = stripped.lstrip("#").strip()
            lower = with_newline.lower()
            for term in terms:
                if term.lower() in lower:
                    hits.append(
                        {
                            "document": path,
                            "heading": heading,
                            "offset": offset,
                            "snippet": snippet(with_newline),
                            "term": term,
                        }
                    )
                    if len(hits) >= max_hits:
                        return hits
            offset += len(with_newline.encode("utf-8"))
    return hits


def hit_key(hit: dict[str, Any]) -> str:
    return f"{hit['document']}#{hit['term']}@{hit['offset']}"


class Budget:
    def __init__(self, limits: dict[str, Any]) -> None:
        self.limits = limits

    def view(self, attempts: list[dict[str, Any]]) -> dict[str, int]:
        queries_used = len(attempts)
        bytes_used = sum(item["reserved_bytes"] for item in attempts)
        millis_used = sum(item["reserved_millis"] for item in attempts)
        limits = self.limits
        return {
            "queries_used": queries_used,
            "bytes_used": bytes_used,
            "millis_used": millis_used,
            "queries_remaining": max(0, limits["max_queries"] - queries_used),
            "bytes_remaining": max(0, limits["max_total_bytes"] - bytes_used),
            "millis_remaining": max(0, limits["max_total_millis"] - millis_used),
        }

    def estimate(self) -> tuple[int, int, int]:
        return (1, self.limits["max_query_bytes"], self.limits["max_query_millis"])

    def admits(self, attempts: list[dict[str, Any]]) -> bool:
        view = self.view(attempts)
        queries, byte_count, millis = self.estimate()
        return (
            view["queries_remaining"] >= queries
            and view["bytes_remaining"] >= byte_count
            and view["millis_remaining"] >= millis
        )


class Replay:
    """独立重放：只按 plan 与冻结语料计算，不看产品账本。"""

    def __init__(self, plan: dict[str, Any], corpus: dict[str, Any], documents: list[tuple[str, bytes]]) -> None:
        self.plan = plan
        self.corpus = corpus
        self.documents = documents
        self.budget = Budget(plan["limits"])
        self.units: list[dict[str, Any]] = []
        for position, query in enumerate(plan["queries"]):
            self.units.append(
                {"query_id": query["query_id"], "position": position, "state": UNIT_PENDING, "query": query}
            )
        self.attempts: list[dict[str, Any]] = []

    def step(self) -> dict[str, Any]:
        reserved = next((item for item in self.attempts if item["state"] == ATTEMPT_RESERVED), None)
        if reserved is not None:
            return self._finish(reserved)
        pending = next((unit for unit in self.units if unit["state"] == UNIT_PENDING), None)
        if pending is None:
            raise StopIteration("no pending query")
        if not self.budget.admits(self.attempts):
            return {"budget_exhausted": True, "query_id": pending["query_id"]}
        queries, byte_count, millis = self.budget.estimate()
        attempt = {
            "attempt": len(self.attempts) + 1,
            "query_id": pending["query_id"],
            "reserved_queries": queries,
            "reserved_bytes": byte_count,
            "reserved_millis": millis,
            "state": ATTEMPT_RESERVED,
            "settled_bytes": None,
            "hits": None,
            "new_hits": 0,
            "outcome": None,
            "hit_keys": [],
            "at": "",
        }
        self.attempts.append(attempt)
        return self._finish(attempt)

    def _finish(self, attempt: dict[str, Any]) -> dict[str, Any]:
        unit = next(item for item in self.units if item["query_id"] == attempt["query_id"])
        query = unit["query"]
        if query.get("requires_external", False):
            hits: list[dict[str, Any]] = []
            settled_bytes = 0
            outcome = OUTCOME_EXTERNAL
        else:
            hits = scan(self.documents, query["terms"], self.plan["limits"]["max_hits"])
            settled_bytes = sum(len(item["snippet"]) for item in hits)
            keys = [hit_key(item) for item in hits]
            previous = {
                key
                for other in self.attempts
                if other["attempt"] != attempt["attempt"]
                and other["state"] == ATTEMPT_SETTLED
                for key in other["hit_keys"]
            }
            fresh = [key for key in keys if key not in previous]
            if not hits:
                outcome = OUTCOME_NO_HITS
            elif not fresh:
                outcome = OUTCOME_DUPLICATES
            else:
                outcome = OUTCOME_EVIDENCE
            if outcome == OUTCOME_EVIDENCE and query.get("conflict_group"):
                for other in self.attempts:
                    if other["attempt"] == attempt["attempt"] or other["state"] != ATTEMPT_SETTLED:
                        continue
                    if other["outcome"] != OUTCOME_EVIDENCE:
                        continue
                    other_unit = next(
                        (item for item in self.units if item["query_id"] == other["query_id"]), None
                    )
                    if other_unit is None:
                        continue
                    if other_unit["query"].get("conflict_group") != query["conflict_group"]:
                        continue
                    if sorted(keys) != sorted(other["hit_keys"]):
                        outcome = OUTCOME_CONFLICT
                        break
            attempt["new_hits"] = len(fresh)

        attempt["state"] = ATTEMPT_SETTLED
        attempt["settled_bytes"] = settled_bytes
        attempt["hits"] = len(hits)
        attempt["outcome"] = outcome
        attempt["hit_keys"] = [hit_key(item) for item in hits]
        unit["state"] = UNIT_GAPPED if outcome == OUTCOME_EXTERNAL else UNIT_SETTLED
        return {
            "budget_exhausted": False,
            "query_id": attempt["query_id"],
            "outcome": outcome,
            "hits": len(hits),
            "new_hits": attempt["new_hits"],
        }

    def run(self) -> list[dict[str, Any]]:
        steps: list[dict[str, Any]] = []
        while True:
            try:
                output = self.step()
            except StopIteration:
                return steps
            steps.append(output)
            if output.get("budget_exhausted"):
                return steps

    def totals(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for attempt in self.attempts:
            if attempt["outcome"] is None:
                continue
            counts[attempt["outcome"]] = counts.get(attempt["outcome"], 0) + 1
        return counts


def replay_package(package: str, plan_name: str) -> dict[str, Any]:
    plan_bytes = read_bytes(os.path.join(package, plan_name))
    plan = strict_load(
        plan_bytes,
        (
            "policy",
            "schema_version",
            "producer",
            "case_id",
            "corpus",
            "external_retrieval",
            "limits",
            "goal_subjects",
            "queries",
        ),
        f"{plan_name}",
        canonical_required=False,
    )
    corpus_root = os.path.join(package, plan["corpus"]["root"])
    manifest_bytes = read_bytes(os.path.join(package, plan["corpus"]["manifest"]))
    corpus = strict_load(
        manifest_bytes,
        ("policy", "schema_version", "case_id", "documents"),
        plan["corpus"]["manifest"],
        canonical_required=False,
    )
    documents: list[tuple[str, bytes]] = []
    for document in corpus["documents"]:
        documents.append((document["path"], read_bytes(os.path.join(corpus_root, document["path"]))))
    return {"plan": plan, "corpus": corpus, "documents": documents}


# ---------------------------------------------------------------------------
# 磁盘与账本检查
# ---------------------------------------------------------------------------


def normalise_plan(plan: dict[str, Any]) -> str:
    """把计划规约成可比较的规范形：缺省字段补默认值，字段顺序固定。

    作者包可以省略 `requires_external` 与 `conflict_group`（前者默认 false，
    后者默认 null），而账本按 serde 序列化后总是写出两者。语义等价即可，
    不要求逐字节相同。
    """
    queries = []
    for query in plan["queries"]:
        queries.append(
            {
                "query_id": query["query_id"],
                "subject": query["subject"],
                "terms": list(query["terms"]),
                "scope_class": query["scope_class"],
                "requires_external": bool(query.get("requires_external", False)),
                "conflict_group": query.get("conflict_group") or None,
            }
        )
    return canonical(
        {
            "policy": plan["policy"],
            "schema_version": plan["schema_version"],
            "producer": plan["producer"],
            "case_id": plan["case_id"],
            "corpus": {"root": plan["corpus"]["root"], "manifest": plan["corpus"]["manifest"]},
            "external_retrieval": plan["external_retrieval"],
            "limits": dict(plan["limits"]),
            "goal_subjects": list(plan["goal_subjects"]),
            "queries": queries,
        }
    )


def check_layout(root: str, report: Report) -> None:
    names = sorted(os.listdir(root))
    report.record("layout-root", names == [CONTROL], f"检索根只允许 control/，实际 {names}")
    control = os.path.join(root, CONTROL)
    if not os.path.isdir(control):
        report.fail("layout-control", "control/ 必须是普通目录")
        return
    entries = sorted(os.listdir(control))
    report.record(
        "layout-control",
        entries == sorted([OWNER, DATABASE, LOCK]),
        f"control/ 只允许三个文件，实际 {entries}",
    )
    for name in (OWNER, DATABASE, LOCK):
        path = os.path.join(control, name)
        if not os.path.exists(path):
            continue
        report.record(f"layout-regular-{name}", os.path.isfile(path) and not os.path.islink(path), f"{name} 必须是普通文件")


def open_database(root: str) -> sqlite3.Connection:
    path = os.path.join(root, CONTROL, DATABASE)
    uri = f"file:{path}?mode=ro"
    connection = sqlite3.connect(uri, uri=True)
    connection.row_factory = sqlite3.Row
    return connection


def check_database(root: str, report: Report) -> sqlite3.Connection | None:
    path = os.path.join(root, CONTROL, DATABASE)
    if not os.path.exists(path):
        report.fail("db-present", "缺少 retrieval.sqlite")
        return None
    try:
        connection = open_database(root)
    except sqlite3.DatabaseError as error:
        report.fail("db-open", f"账本无法只读打开：{error}")
        return None
    application_id = connection.execute("PRAGMA application_id").fetchone()[0]
    report.record("db-application-id", application_id == APPLICATION_ID, f"application_id={application_id:#x}")
    user_version = connection.execute("PRAGMA user_version").fetchone()[0]
    report.record("db-user-version", user_version == SCHEMA_VERSION, f"user_version={user_version}")
    journal_mode = connection.execute("PRAGMA journal_mode").fetchone()[0]
    report.record(
        "db-journal-mode",
        str(journal_mode).lower() == "delete",
        f"journal_mode={journal_mode} 必须是 delete",
    )
    rows = connection.execute(
        "SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name"
    ).fetchall()
    actual = {row["name"]: row["sql"] for row in rows}
    report.record(
        "db-table-set",
        sorted(actual) == sorted(REQUIRED_TABLES),
        f"表集合 {sorted(actual)} 必须等于 {sorted(REQUIRED_TABLES)}",
    )
    for name, definition in REQUIRED_TABLES.items():
        if name not in actual:
            continue
        report.record(
            f"db-table-{name}",
            collapse(actual[name]) == collapse(definition),
            f"{name} 的定义与契约不符",
        )
    return connection


def check_owner(root: str, report: Report, package: str, plan_name: str) -> dict[str, Any] | None:
    path = os.path.join(root, CONTROL, OWNER)
    if not os.path.exists(path):
        report.fail("owner-present", "缺少 control/owner.json")
        return None
    try:
        owner = strict_load(read_bytes(path), OWNER_FIELDS, "owner.json")
    except (OSError, ValueError) as error:
        report.fail("owner-strict", str(error))
        return None
    report.record("owner-policy", owner["policy"] == POLICY, f"policy={owner['policy']}")
    report.record("owner-producer", owner["producer"] == PRODUCER, f"producer={owner['producer']}")
    report.record("owner-schema", owner["schema_version"] == SCHEMA_VERSION, f"schema_version={owner['schema_version']}")
    report.record("owner-root", os.path.realpath(owner["root"]) == os.path.realpath(root), "owner.root 必须等于当前根")
    plan_bytes = read_bytes(os.path.join(package, plan_name))
    report.record(
        "owner-plan-sha256",
        owner["plan_manifest_sha256"] == sha256(plan_bytes),
        "owner 记录的作者清单指纹与 plan.json 实际字节不符",
    )
    return owner


def read_metadata(connection: sqlite3.Connection, report: Report, owner: dict[str, Any] | None) -> dict[str, Any] | None:
    rows = connection.execute("SELECT singleton,schema_version,metadata_json FROM metadata").fetchall()
    report.record("db-metadata-rows", len(rows) == 1, f"metadata 必须只有一行，实际 {len(rows)}")
    if len(rows) != 1:
        return None
    row = rows[0]
    report.record("db-metadata-singleton", row["singleton"] == 1, f"singleton={row['singleton']}")
    report.record(
        "db-metadata-schema",
        row["schema_version"] == SCHEMA_VERSION,
        f"schema_version={row['schema_version']}",
    )
    try:
        metadata = strict_load(row["metadata_json"].encode("utf-8"), METADATA_FIELDS, "metadata_json")
    except ValueError as error:
        report.fail("db-metadata-strict", str(error))
        return None
    if owner is not None:
        for field in ("case_id", "created_at", "plan_manifest_sha256", "corpus_manifest_sha256"):
            report.record(
                f"db-metadata-{field}",
                metadata[field] == owner[field],
                f"metadata 与 owner 的 {field} 不一致",
            )
    return metadata


def read_units(connection: sqlite3.Connection, report: Report) -> list[dict[str, Any]]:
    rows = connection.execute("SELECT position,state,payload_json FROM units ORDER BY position").fetchall()
    positions = [row["position"] for row in rows]
    report.record(
        "db-unit-positions",
        positions == list(range(len(positions))),
        f"position 必须是连续整数，实际 {positions}",
    )
    units: list[dict[str, Any]] = []
    for row in rows:
        try:
            unit = strict_load(row["payload_json"].encode("utf-8"), UNIT_FIELDS, f"unit@{row['position']}")
        except ValueError as error:
            report.fail("db-unit-strict", str(error))
            continue
        report.record(
            f"db-unit-state-{unit['query_id']}",
            row["state"] == unit["state"] and unit["state"] in UNITS,
            f"state 列 {row['state']} 与记录 {unit['state']} 不一致",
        )
        report.record(
            f"db-unit-position-{unit['query_id']}",
            unit["position"] == row["position"],
            f"position 列 {row['position']} 与记录 {unit['position']} 不一致",
        )
        units.append(unit)
    return units


def read_attempts(connection: sqlite3.Connection, report: Report) -> list[dict[str, Any]]:
    rows = connection.execute(
        "SELECT attempt,state,payload_json FROM attempts ORDER BY attempt"
    ).fetchall()
    attempts: list[dict[str, Any]] = []
    for row in rows:
        try:
            attempt = strict_load(row["payload_json"].encode("utf-8"), ATTEMPT_FIELDS, f"attempt@{row['attempt']}")
        except ValueError as error:
            report.fail("db-attempt-strict", str(error))
            continue
        report.record(
            f"db-attempt-number-{attempt['attempt']}",
            attempt["attempt"] == row["attempt"] and attempt["attempt"] > 0,
            f"attempt 列 {row['attempt']} 与记录 {attempt['attempt']} 不一致",
        )
        report.record(
            f"db-attempt-state-{attempt['attempt']}",
            attempt["state"] == row["state"],
            f"state 列 {row['state']} 与记录 {attempt['state']} 不一致",
        )
        attempts.append(attempt)
    return attempts


def check_ledger_consistency(
    units: list[dict[str, Any]],
    attempts: list[dict[str, Any]],
    plan: dict[str, Any],
    report: Report,
) -> None:
    report.record(
        "ledger-unit-count",
        len(units) == len(plan["queries"]),
        f"unit 数 {len(units)} 必须等于计划查询数 {len(plan['queries'])}",
    )
    planned = [query["query_id"] for query in plan["queries"]]
    report.record(
        "ledger-unit-order",
        [unit["query_id"] for unit in units] == planned,
        "unit 顺序必须等于计划顺序",
    )
    settled = {attempt["query_id"] for attempt in attempts if attempt["state"] == ATTEMPT_SETTLED}
    reserved = [attempt["query_id"] for attempt in attempts if attempt["state"] == ATTEMPT_RESERVED]
    report.record("ledger-single-reserved", len(reserved) <= 1, f"最多一条待补结算尝试，实际 {reserved}")
    for unit in units:
        if unit["state"] == UNIT_PENDING:
            report.record(
                f"ledger-pending-not-attempted-{unit['query_id']}",
                unit["query_id"] not in settled,
                "待执行查询不应已有已结算尝试",
            )
        else:
            report.record(
                f"ledger-settled-has-attempt-{unit['query_id']}",
                unit["query_id"] in settled,
                "非待执行查询必须有已结算尝试",
            )
    # 每次尝试只预留一次：同一查询不得出现两条已结算尝试。
    seen: set[str] = set()
    duplicated = False
    for attempt in attempts:
        if attempt["state"] != ATTEMPT_SETTLED:
            continue
        if attempt["query_id"] in seen:
            duplicated = True
        seen.add(attempt["query_id"])
    report.record("ledger-no-double-settle", not duplicated, "同一查询被结算了两次，预算可能被重复计费")


def check_budget(
    attempts: list[dict[str, Any]],
    plan: dict[str, Any],
    report: Report,
) -> dict[str, int]:
    budget = Budget(plan["limits"])
    view = budget.view(attempts)
    limits = plan["limits"]
    report.record(
        "budget-reservation-shape",
        all(
            attempt["reserved_queries"] == 1
            and attempt["reserved_bytes"] == limits["max_query_bytes"]
            and attempt["reserved_millis"] == limits["max_query_millis"]
            for attempt in attempts
        ),
        "每次尝试必须按最坏情况预留：1 次查询 + 单次字节上限 + 单次耗时上限",
    )
    report.record(
        "budget-within-limits",
        view["queries_used"] <= limits["max_queries"]
        and view["bytes_used"] <= limits["max_total_bytes"]
        and view["millis_used"] <= limits["max_total_millis"],
        f"已用预算 {view} 超出上限 {limits}",
    )
    report.record(
        "budget-no-refund",
        view["bytes_used"] == sum(item["reserved_bytes"] for item in attempts),
        "已记额度不得回收：bytes_used 必须等于预留之和",
    )
    return view


def check_termination(
    connection: sqlite3.Connection,
    units: list[dict[str, Any]],
    attempts: list[dict[str, Any]],
    plan: dict[str, Any],
    view: dict[str, int],
    report: Report,
) -> None:
    try:
        rows = connection.execute("SELECT reason,note,at FROM termination").fetchall()
    except sqlite3.DatabaseError as error:
        report.fail("termination-table", f"termination 表不可读：{error}")
        return
    report.record("termination-rows", len(rows) <= 1, f"终止理由最多一条，实际 {len(rows)}")
    if not rows:
        report.ok("termination-absent", "本轮尚未记录终止理由")
        return
    row = rows[0]
    reason = row["reason"]
    report.record("termination-known", reason in REASONS, f"未知终止理由 {reason}")
    report.record("termination-note", 1 <= len(row["note"]) <= 600, "终止理由的 note 必须有 1 到 600 个字符")
    if reason not in REASONS:
        return
    budget = Budget(plan["limits"])
    settled = [item for item in attempts if item["state"] == ATTEMPT_SETTLED]
    outcomes = [item["outcome"] for item in settled]
    goals = set(plan["goal_subjects"])
    covered = {
        next(unit for unit in units if unit["query_id"] == item["query_id"])["query"]["subject"]
        for item in settled
        if item["outcome"] == OUTCOME_EVIDENCE
    }
    pending = [unit for unit in units if unit["state"] == UNIT_PENDING]
    paused = connection.execute(
        "SELECT action FROM controls ORDER BY sequence DESC LIMIT 1"
    ).fetchone()
    paused = bool(paused) and paused["action"] == "pause"

    if reason == REASON_GOAL_MET:
        missing = sorted(goals - covered)
        report.record("termination-goal-covered", not missing, f"这些目标没有已结算证据：{missing}")
        report.record("termination-goal-no-pending", not pending, f"还有 {len(pending)} 条查询未结算")
    elif reason == REASON_NO_NEW_EVIDENCE:
        report.record(
            "termination-no-hits",
            OUTCOME_NO_HITS in outcomes,
            "没有零命中查询就没有『无可靠新证据』的证据",
        )
    elif reason == REASON_MARGINAL_GAIN:
        report.record(
            "termination-duplicates",
            OUTCOME_DUPLICATES in outcomes,
            "没有重复线索查询就没有『边际收益不足』的证据",
        )
    elif reason == REASON_BUDGET:
        report.record("termination-budget-pending", bool(pending), "没有待执行查询，不能记预算不足")
        if pending:
            report.record(
                "termination-budget-exceeded",
                not budget.admits(attempts),
                "剩余预算仍足够执行下一条查询，不能记预算不足",
            )
    elif reason == REASON_PAUSED:
        report.record("termination-paused", paused, "账本里没有生效中的暂停控制")
    elif reason == REASON_NETWORK:
        report.record(
            "termination-network-off",
            plan["external_retrieval"] is False,
            "本轮允许外部检索，没有网络受限证据",
        )
        report.record(
            "termination-network-external-query",
            OUTCOME_EXTERNAL in outcomes,
            "没有需要外部来源的查询，记『网络受限』无依据",
        )
    elif reason == REASON_MANUAL:
        report.record(
            "termination-conflict",
            OUTCOME_CONFLICT in outcomes,
            "本轮没有冲突线索，记『需要人工判断』无依据",
        )


def check_replay(
    units: list[dict[str, Any]],
    attempts: list[dict[str, Any]],
    expected: dict[str, Any],
    report: Report,
) -> None:
    replay = expected.get("replay")
    if not replay:
        report.fail("replay-expected", "cases.json 缺少 replay 先验预期")
        return
    outcomes = [attempt["outcome"] for attempt in attempts]
    report.record(
        "replay-outcomes",
        outcomes == replay["outcomes"],
        f"实际 {outcomes}，先验 {replay['outcomes']}",
    )
    hits = [attempt["hits"] for attempt in attempts]
    report.record("replay-hits", hits == replay["hits"], f"实际 {hits}，先验 {replay['hits']}")
    fresh = [attempt["new_hits"] for attempt in attempts]
    report.record(
        "replay-new-hits",
        fresh == replay["new_hits"],
        f"实际 {fresh}，先验 {replay['new_hits']}",
    )
    states: dict[str, int] = {}
    for unit in units:
        states[unit["state"]] = states.get(unit["state"], 0) + 1
    report.record(
        "replay-unit-states",
        states == replay["unit_states"],
        f"实际 {states}，先验 {replay['unit_states']}",
    )


def check_no_network(root: str, report: Report) -> None:
    """账本不得保存任何网络或模型痕迹：只允许四个顶层条目。"""
    control = os.path.join(root, CONTROL)
    entries = sorted(os.listdir(control))
    report.record(
        "no-network-artifacts",
        entries == sorted([OWNER, DATABASE, LOCK]),
        f"检索根只允许三个控制文件，实际 {entries}",
    )


def check_frozen(package: str, report: Report) -> None:
    path = os.path.join(package, "frozen-sha256.json")
    if not os.path.exists(path):
        report.fail("frozen-present", "缺少 frozen-sha256.json")
        return
    frozen = json.loads(read_bytes(path).decode("utf-8"))
    files = frozen["files"]
    actual: dict[str, str] = {}
    for base, _, names in os.walk(package):
        for name in names:
            full = os.path.join(base, name)
            relative = os.path.relpath(full, package).replace(os.sep, "/")
            if relative == "frozen-sha256.json":
                continue
            actual[relative] = sha256(read_bytes(full))
    report.record(
        "frozen-file-set",
        sorted(actual) == sorted(files),
        f"清单集合与实际不一致：多 {sorted(set(actual) - set(files))}、缺 {sorted(set(files) - set(actual))}",
    )
    for relative, digest in actual.items():
        if relative not in files:
            continue
        report.record(
            f"frozen-sha-{relative}",
            files[relative]["sha256"] == digest,
            f"{relative} 的 SHA256 与冻结清单不符",
        )


def freeze(package: str) -> int:
    files: dict[str, Any] = {}
    for base, _, names in os.walk(package):
        for name in names:
            full = os.path.join(base, name)
            relative = os.path.relpath(full, package).replace(os.sep, "/")
            if relative == "frozen-sha256.json":
                continue
            payload = read_bytes(full)
            files[relative] = {"sha256": sha256(payload), "byte_length": len(payload)}
    cases_path = os.path.join(package, "cases.json")
    case_count = 0
    if os.path.exists(cases_path):
        cases = json.loads(read_bytes(cases_path).decode("utf-8"))
        case_count = len(cases.get("sequence", [])) or 1
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": os.path.basename(os.path.normpath(package)),
        "case_count": case_count,
        "files": dict(sorted(files.items())),
    }
    with open(os.path.join(package, "frozen-sha256.json"), "w", encoding="utf-8", newline="\n") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print(f"已冻结 {len(files)} 个成员（case_count={case_count}）")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="B08-T02 有界检索独立检查器")
    parser.add_argument("root", nargs="?", help="检索根")
    parser.add_argument("--package", required=True, help="冻结作者包目录")
    parser.add_argument("--plan", default="plan.json", help="要重放的检索计划（默认 plan.json）")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出")
    parser.add_argument("--freeze", action="store_true", help="重新生成 frozen-sha256.json")
    args = parser.parse_args(argv)

    package = os.path.abspath(args.package)
    if args.freeze:
        return freeze(package)
    if not args.root:
        parser.error("必须给出检索根（除非使用 --freeze）")
    root = os.path.abspath(args.root)
    if not os.path.isdir(root):
        print(f"检索根不存在：{root}", file=sys.stderr)
        return 2

    report = Report()
    replay_data = replay_package(package, args.plan)
    plan = replay_data["plan"]
    corpus = replay_data["corpus"]

    check_layout(root, report)
    owner = check_owner(root, report, package, args.plan)
    connection = check_database(root, report)
    metadata = read_metadata(connection, report, owner) if connection is not None else None
    if metadata is not None:
        report.record(
            "db-metadata-plan",
            normalise_plan(metadata["plan"]) == normalise_plan(plan),
            "账本里的计划必须与作者包 plan.json 语义一致",
        )
        report.record(
            "db-metadata-corpus",
            canonical(metadata["corpus"]) == canonical(corpus),
            "账本里的语料清单必须与 corpus-manifest.json 完全一致",
        )
        report.record(
            "db-metadata-corpus-root",
            os.path.realpath(metadata["corpus_root"])
            == os.path.realpath(os.path.join(package, plan["corpus"]["root"])),
            "账本记录的语料根必须指向作者包内冻结语料",
        )
    units = read_units(connection, report) if connection is not None else []
    attempts = read_attempts(connection, report) if connection is not None else []
    if connection is not None:
        check_ledger_consistency(units, attempts, plan, report)
        view = check_budget(attempts, plan, report)
        check_termination(connection, units, attempts, plan, view, report)
        connection.close()

    cases = json.loads(read_bytes(os.path.join(package, "cases.json")).decode("utf-8"))
    expected = cases["expected"][args.plan]
    check_replay(units, attempts, expected, report)
    check_no_network(root, report)
    check_frozen(package, report)

    # 独立重放：从零算一遍，确认账本结果与纯判定层一致。
    replay = Replay(plan, corpus, replay_data["documents"])
    steps = replay.run()
    outcomes = [step.get("outcome") for step in steps if not step.get("budget_exhausted")]
    report.record(
        "independent-replay-outcomes",
        outcomes == expected["replay"]["outcomes"],
        f"独立重放 {outcomes}，先验 {expected['replay']['outcomes']}",
    )
    report.record(
        "independent-replay-budget",
        (replay.budget.view(replay.attempts)["bytes_used"], replay.budget.view(replay.attempts)["millis_used"])
        == (expected["budget"]["bytes_used"], expected["budget"]["millis_used"]),
        "独立重放的预算消耗与先验不符",
    )

    if args.json:
        payload = report.dump()
        payload["root"] = root
        payload["package"] = package
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        for warning in report.warnings:
            print(f"提示：{warning}")
        for failure in report.failures:
            print(f"失败：{failure}", file=sys.stderr)
        print(
            f"合计 {report.passed + len(report.failures)} 项检查：失败 {len(report.failures)}、提示 {len(report.warnings)}"
        )
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
