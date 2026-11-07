#!/usr/bin/env python3
"""B08-T01 独立检查器：只读磁盘、SQLite 与冻结作者包，独立重放候选队列并核对拒绝记忆。

它不调用 CLI 的判定函数，也不 import Rust 侧任何代码：候选状态、证据数、计数器、
必做集合与越界集合全部由本文件从 `author.json`、各轮观测/决定文件与 `control/`
下的 owner.json、research.sqlite、queue.lock 重新计算得出，再与 `cases.json`
的先验预期逐条对照。

用法：
    python3 scripts/verify_research_queue.py <队列根> --package <作者包目录> [--json]
    python3 scripts/verify_research_queue.py --package <作者包目录> --freeze

退出码：0 = 全部硬检查通过；1 = 有硬检查失败；2 = 用法错误。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import sys
from typing import Any

POLICY = "research-queue-v1"
PRODUCER = "noetome-research-queue"
SCHEMA_VERSION = 1
APPLICATION_ID = 0x4E525148

CONTROL = "control"
OWNER = "owner.json"
DATABASE = "research.sqlite"
LOCK = "queue.lock"

SCOPE_AUTHORIZED = "authorized_target"
SCOPE_PREREQUISITE = "required_prerequisite"
SCOPE_OUT = "out_of_scope"
SCOPES = (SCOPE_AUTHORIZED, SCOPE_PREREQUISITE, SCOPE_OUT)

STATE_PROPOSED = "proposed"
STATE_ACCEPTED = "accepted"
STATE_REJECTED = "rejected"
STATE_DEFERRED = "deferred"
STATE_MERGED = "merged"
STATES = (STATE_PROPOSED, STATE_ACCEPTED, STATE_REJECTED, STATE_DEFERRED, STATE_MERGED)

ACTION_ACCEPT = "accept"
ACTION_REJECT = "reject"
ACTION_DEFER = "defer"
ACTION_MERGE = "merge"
ACTION_REVISE = "revise_scope"

REQUIRED_TABLES = {
    "metadata": (
        "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),"
        "schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL)"
    ),
    "candidates": (
        "CREATE TABLE candidates(subject TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),"
        "state TEXT NOT NULL,payload_json TEXT NOT NULL)"
    ),
    "events": (
        "CREATE TABLE events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,"
        "round TEXT NOT NULL,payload_json TEXT NOT NULL,at TEXT NOT NULL)"
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
    "author_manifest_sha256",
    "objective",
    "limits",
    "created_at",
)

METADATA_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "author_manifest_sha256",
    "objective",
    "limits",
    "created_at",
)

EVENT_FIELDS = ("round", "applied", "counters")

CANDIDATE_FIELDS = (
    "subject",
    "scope_class",
    "reason",
    "relation",
    "benefit",
    "source_leads",
    "estimated_cost",
    "depends_on",
    "state",
    "scope_note",
    "evidence",
    "rejections",
    "scope_revisions",
    "merged_into",
)

SHA256 = re.compile(r"^[0-9a-f]{64}$")


class Report:
    def __init__(self) -> None:
        self.checks: list[dict[str, Any]] = []
        self.failures = 0
        self.warnings = 0

    def record(self, name: str, ok: bool, detail: str = "", warn_only: bool = False) -> bool:
        if ok:
            self.checks.append({"check": name, "status": "ok", "detail": detail})
        elif warn_only:
            self.warnings += 1
            self.checks.append({"check": name, "status": "warning", "detail": detail})
        else:
            self.failures += 1
            self.checks.append({"check": name, "status": "failed", "detail": detail})
        return ok

    def fail(self, name: str, detail: str) -> None:
        self.record(name, False, detail)

    def ok(self, name: str, detail: str = "") -> None:
        self.record(name, True, detail)

    def warn(self, name: str, detail: str) -> None:
        self.record(name, False, detail, warn_only=True)

    def dump(self) -> dict[str, Any]:
        return {
            "failures": self.failures,
            "warnings": self.warnings,
            "checks": self.checks,
        }


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as handle:
        return handle.read()


def canonical(value: Any) -> str:
    """与 serde_json::to_string 同形的紧凑 JSON：无空格、非 ASCII 原样输出。"""
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(
    source: Any, fields: tuple[str, ...], label: str, *, canonical_required: bool = True
) -> Any:
    raw = read_bytes(source) if isinstance(source, str) else source
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{label}: 不是 UTF-8（{error}）") from None
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"{label}: 不是合法 JSON（{error}）") from None
    if not isinstance(value, dict):
        raise ValueError(f"{label}: 顶层必须是对象")
    missing = [name for name in fields if name not in value]
    extra = [name for name in value if name not in fields]
    if missing or extra:
        raise ValueError(f"{label}: 字段集不符（缺 {missing}、多 {extra}）")
    if canonical_required and canonical(value) != text:
        raise ValueError(f"{label}: 非规范编码、重复键或含多余空白")
    return value


def subject_key(value: str) -> str:
    return " ".join(value.split())


def collapse(values: Any, label: str) -> list[str]:
    if not isinstance(values, list):
        raise ValueError(f"{label}: 必须是数组")
    return [subject_key(str(item)) for item in values]


# ---------------------------------------------------------------------------
# 合同重放：完全依据 FR-13 的文字语义，独立重算队列
# ---------------------------------------------------------------------------


class Queue:
    def __init__(self, author: dict[str, Any]) -> None:
        self.case_id = author["case_id"]
        self.objective = author["objective"]
        self.limits = author["limits"]
        self.must_do = {
            subject_key(goal["subject"])
            for goal in list(self.objective["authorized_targets"])
            + list(self.objective["required_prerequisites"])
        }
        self.candidates: dict[str, dict[str, Any]] = {}
        self.counters = {
            "observations": 0,
            "blocked_repeats": 0,
            "reopenings": 0,
            "merges": 0,
            "decisions": 0,
        }

    def observe(self, round_name: str, observation: dict[str, Any]) -> str:
        key = subject_key(observation["subject"])
        candidate = self.candidates.get(key)
        if candidate is None:
            self.candidates[key] = {
                "subject": key,
                "scope_class": observation["scope_class"],
                "reason": observation["reason"],
                "relation": observation["relation"],
                "benefit": observation["benefit"],
                "source_leads": list(observation["source_leads"]),
                "estimated_cost": observation["estimated_cost"],
                "depends_on": [subject_key(item) for item in observation["depends_on"]],
                "state": STATE_PROPOSED,
                "scope_note": "",
                "evidence": [
                    {
                        "round": round_name,
                        "note": " ".join(observation["reason"].split()),
                        "leads": list(observation["source_leads"]),
                    }
                ],
                "rejections": [],
                "scope_revisions": [],
                "merged_into": None,
            }
            self.counters["observations"] += 1
            return "created"
        if candidate["state"] == STATE_MERGED:
            raise ValueError(f"候选 {key} 已合并，不再接受新观测")
        if candidate["state"] == STATE_REJECTED:
            note = (observation.get("revisit_reason") or "").strip()
            if not note:
                self.counters["blocked_repeats"] += 1
                self.counters["observations"] += 1
                return "blocked_repeat"
            repeats = note in [item["reason"] for item in candidate["rejections"]] or note in [
                item["note"] for item in candidate["evidence"]
            ]
            if repeats:
                self.counters["blocked_repeats"] += 1
                self.counters["observations"] += 1
                return "blocked_repeat"
            candidate["state"] = STATE_PROPOSED
            candidate["evidence"].append(
                {"round": round_name, "note": note, "leads": list(observation["source_leads"])}
            )
            self.counters["reopenings"] += 1
            self.counters["observations"] += 1
            return "reopened"
        note = " ".join(observation["reason"].split())
        seen = any(
            item["round"] == round_name and item["note"] == note for item in candidate["evidence"]
        )
        self.counters["observations"] += 1
        if seen:
            return "duplicate_evidence"
        candidate["evidence"].append(
            {"round": round_name, "note": note, "leads": list(observation["source_leads"])}
        )
        return "merged_evidence"

    def decide(self, decision: dict[str, Any]) -> str:
        key = subject_key(decision["subject"])
        candidate = self.candidates.get(key)
        if candidate is None:
            raise ValueError(f"决定主题 {key} 不在候选队列中")
        action = decision["action"]
        if action == ACTION_ACCEPT:
            if candidate["state"] in (STATE_REJECTED, STATE_MERGED):
                raise ValueError(f"候选 {key} 处于 {candidate['state']}，不能接受")
            candidate["state"] = STATE_ACCEPTED
        elif action == ACTION_REJECT:
            if candidate["state"] == STATE_MERGED:
                raise ValueError(f"候选 {key} 已合并，无需拒绝")
            candidate["state"] = STATE_REJECTED
            candidate["rejections"].append(
                {"reason": " ".join((decision.get("reason") or "").split())}
            )
        elif action == ACTION_DEFER:
            if candidate["state"] == STATE_MERGED:
                raise ValueError(f"候选 {key} 已合并，无需延后")
            candidate["state"] = STATE_DEFERRED
        elif action == ACTION_MERGE:
            target_key = subject_key(str(decision.get("merged_into") or ""))
            target = self.candidates.get(target_key)
            if target is None:
                raise ValueError(f"合并目标 {target_key} 不在候选队列中")
            if target_key == key:
                raise ValueError("merge 不能合并到自身")
            if candidate["state"] == STATE_MERGED:
                raise ValueError(f"候选 {key} 已合并过")
            if target["state"] in (STATE_MERGED, STATE_REJECTED):
                raise ValueError(f"合并目标 {target_key} 处于 {target['state']}")
            for evidence in candidate["evidence"]:
                if not any(
                    item["round"] == evidence["round"] and item["note"] == evidence["note"]
                    for item in target["evidence"]
                ):
                    target["evidence"].append(evidence)
            candidate["state"] = STATE_MERGED
            candidate["merged_into"] = target_key
            self.counters["merges"] += 1
        elif action == ACTION_REVISE:
            note = " ".join((decision.get("scope_note") or "").split())
            if candidate["state"] == STATE_MERGED:
                raise ValueError(f"候选 {key} 已合并，不能改范围")
            if candidate["scope_note"] == note:
                raise ValueError(f"候选 {key} 的范围说明没有变化")
            if len(candidate["scope_revisions"]) >= int(self.limits["max_revisions"]):
                raise ValueError(f"候选 {key} 的范围修订已达上限")
            before = candidate["scope_note"]
            candidate["scope_note"] = note
            candidate["scope_revisions"].append(
                {"reason": " ".join((decision.get("reason") or "").split()), "before": before, "after": note}
            )
        else:
            raise ValueError(f"未知决定动作 {action}")
        self.counters["decisions"] += 1
        return action

    def queue_only(self) -> set[str]:
        return {
            key
            for key, candidate in self.candidates.items()
            if candidate["scope_class"] == SCOPE_OUT and candidate["state"] != STATE_MERGED
        }

    def totals(self) -> dict[str, Any]:
        totals: dict[str, Any] = {"total": len(self.candidates)}
        for state in STATES:
            totals[state] = sum(1 for c in self.candidates.values() if c["state"] == state)
        for scope in SCOPES:
            totals[scope] = sum(1 for c in self.candidates.values() if c["scope_class"] == scope)
        return totals


def replay(package: str, author: dict[str, Any], sequence: list[dict[str, Any]]) -> Queue:
    queue = Queue(author)
    for step in sequence:
        if step["step"] == "adopt":
            continue
        path = os.path.join(package, step["input"])
        payload = json.loads(read_bytes(path).decode("utf-8"))
        if step["step"] == "observe":
            for observation in payload["observations"]:
                queue.observe(payload["round"], observation)
        elif step["step"] == "decide":
            for decision in payload["decisions"]:
                queue.decide(decision)
    return queue


# ---------------------------------------------------------------------------
# 磁盘检查
# ---------------------------------------------------------------------------


def check_layout(root: str, report: Report) -> bool:
    if not os.path.isdir(root):
        report.fail("layout", f"{root} 不是目录")
        return False
    entries = sorted(os.listdir(root))
    ok = report.record(
        "layout-root",
        entries == [CONTROL],
        f"根目录条目 {entries}（只允许 control/）",
    )
    if not ok:
        return False
    control = os.path.join(root, CONTROL)
    control_entries = sorted(os.listdir(control))
    report.record(
        "layout-control",
        control_entries == sorted([OWNER, DATABASE, LOCK]),
        f"control/ 条目 {control_entries}（只允许 owner.json、research.sqlite、queue.lock）",
    )
    for name in (OWNER, DATABASE, LOCK):
        path = os.path.join(control, name)
        report.record(
            f"layout-file-{name}",
            os.path.isfile(path) and not os.path.islink(path),
            f"{name} 必须是普通文件",
        )
    return True


def check_owner(root: str, package: str, report: Report) -> dict[str, Any] | None:
    path = os.path.join(root, CONTROL, OWNER)
    if not os.path.isfile(path):
        report.fail("owner-present", "control/owner.json 缺失")
        return None
    try:
        owner = strict_load(path, OWNER_FIELDS, "owner.json")
    except ValueError as error:
        report.fail("owner-strict", str(error))
        return None
    report.ok("owner-strict", "字段集、编码与规范化均符合要求")
    report.record("owner-policy", owner["policy"] == POLICY, f"policy={owner['policy']}")
    report.record("owner-producer", owner["producer"] == PRODUCER, f"producer={owner['producer']}")
    report.record(
        "owner-schema-version",
        owner["schema_version"] == SCHEMA_VERSION,
        f"schema_version={owner['schema_version']}",
    )
    real_root = os.path.realpath(root)
    report.record("owner-root", owner["root"] == real_root, f"owner.root={owner['root']}")
    report.record(
        "owner-author-sha256",
        bool(SHA256.match(str(owner["author_manifest_sha256"]))),
        f"author_manifest_sha256={owner['author_manifest_sha256']}",
    )
    author_path = os.path.join(package, "author.json")
    if os.path.isfile(author_path):
        actual = sha256(read_bytes(author_path))
        report.record(
            "owner-author-bytes",
            owner["author_manifest_sha256"] == actual,
            f"冻结作者包 SHA256={actual}",
        )
    return owner


def check_database(root: str, report: Report) -> tuple[dict[str, Any] | None, list[dict[str, Any]], list[dict[str, Any]]]:
    path = os.path.join(root, CONTROL, DATABASE)
    if not os.path.isfile(path):
        report.fail("db-present", "control/research.sqlite 缺失")
        return None, [], []
    try:
        connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    except sqlite3.Error as error:
        report.fail("db-open", f"只读打开失败：{error}")
        return None, [], []
    try:
        application_id = connection.execute("PRAGMA application_id").fetchone()[0]
        user_version = connection.execute("PRAGMA user_version").fetchone()[0]
        journal_mode = connection.execute("PRAGMA journal_mode").fetchone()[0]
        report.record("db-application-id", application_id == APPLICATION_ID, f"application_id={application_id:#x}")
        report.record("db-user-version", user_version == SCHEMA_VERSION, f"user_version={user_version}")
        report.record("db-journal-mode", str(journal_mode).lower() == "delete", f"journal_mode={journal_mode}")
        rows = connection.execute("SELECT name,sql FROM sqlite_master WHERE type='table'").fetchall()
        # sqlite_ 前缀是 SQLite 内部表（AUTOINCREMENT 会自动建 sqlite_sequence），不计入契约表集合。
        tables = {name: sql for name, sql in rows if not str(name).startswith("sqlite_")}
        expected = set(REQUIRED_TABLES)
        report.record("db-tables", set(tables) == expected, f"表集合 {sorted(tables)}")
        for name, sql in REQUIRED_TABLES.items():
            compact = "".join(str(tables.get(name, "")).split())
            report.record(
                f"db-table-{name}",
                compact == "".join(sql.split()),
                "定义逐字一致" if compact == "".join(sql.split()) else f"定义不符：{compact}",
            )
        metadata_rows = connection.execute(
            "SELECT singleton,schema_version,metadata_json FROM metadata"
        ).fetchall()
        report.record("db-metadata-rows", len(metadata_rows) == 1, f"metadata 行数={len(metadata_rows)}")
        metadata = None
        if len(metadata_rows) == 1:
            try:
                metadata = strict_load(
                    metadata_rows[0][2].encode("utf-8"), METADATA_FIELDS, "metadata_json"
                )
                report.ok("db-metadata-strict", "metadata_json 严格合规")
            except ValueError as error:
                report.fail("db-metadata-strict", str(error))
        candidates = []
        positions = []
        for subject, position, state, payload in connection.execute(
            "SELECT subject,position,state,payload_json FROM candidates ORDER BY position"
        ):
            try:
                candidate = strict_load(payload.encode("utf-8"), CANDIDATE_FIELDS, f"candidate {subject}")
            except ValueError as error:
                report.fail(f"db-candidate-strict:{subject}", str(error))
                continue
            report.record(
                f"db-candidate-key:{subject}",
                subject == subject_key(candidate["subject"]) and state == candidate["state"],
                f"键与状态一致（position={position}）",
            )
            candidates.append(candidate)
            positions.append(position)
        report.record(
            "db-candidate-positions",
            positions == list(range(len(positions))),
            f"候选序号必须从 0 连续（{len(positions)} 项，实际 {positions[:8]}）",
        )
        events = []
        for kind, round_name, payload, at in connection.execute(
            "SELECT kind,round,payload_json,at FROM events ORDER BY sequence"
        ):
            try:
                event = strict_load(payload.encode("utf-8"), EVENT_FIELDS, f"event {kind}")
            except ValueError as error:
                report.fail(f"db-event-strict:{kind}", str(error))
                continue
            events.append({"kind": kind, "round": round_name, "at": at, **event})
        report.record("db-events-ordered", bool(events), f"事件数={len(events)}")
        return metadata, candidates, events
    finally:
        connection.close()


def check_queue(
    owner: dict[str, Any] | None,
    metadata: dict[str, Any] | None,
    candidates: list[dict[str, Any]],
    events: list[dict[str, Any]],
    expected: dict[str, Any],
    report: Report,
) -> None:
    if owner and metadata:
        report.record(
            "owner-metadata-case",
            owner["case_id"] == metadata["case_id"],
            f"case_id={owner['case_id']}",
        )
        report.record(
            "owner-metadata-objective",
            canonical(owner["objective"]) == canonical(metadata["objective"]),
            "本轮目标一致",
        )
        report.record(
            "owner-metadata-limits",
            canonical(owner["limits"]) == canonical(metadata["limits"]),
            "限额一致",
        )
        report.record(
            "owner-metadata-created-at",
            owner["created_at"] == metadata["created_at"],
            f"created_at={owner['created_at']}",
        )
        report.record(
            "owner-metadata-author",
            owner["author_manifest_sha256"] == metadata["author_manifest_sha256"],
            "作者包指纹一致",
        )

    by_subject = {subject_key(candidate["subject"]): candidate for candidate in candidates}

    if owner:
        objective = owner["objective"]
        must_do = sorted(
            {subject_key(goal["subject"]) for goal in objective["authorized_targets"]}
            | {subject_key(goal["subject"]) for goal in objective["required_prerequisites"]}
        )
        report.record(
            "queue-must-do",
            must_do == sorted(subject_key(item) for item in expected["must_do"]),
            f"必做集合={must_do}",
        )
        queue_only = sorted(
            key
            for key, candidate in by_subject.items()
            if candidate["scope_class"] == SCOPE_OUT and candidate["state"] != STATE_MERGED
        )
        report.record(
            "queue-queue-only",
            queue_only == sorted(subject_key(item) for item in expected["queue_only"]),
            f"越界且未合并={queue_only}",
        )

    for state, subjects in expected["states"].items():
        actual = sorted(key for key, candidate in by_subject.items() if candidate["state"] == state)
        report.record(
            f"state-{state}",
            actual == sorted(subject_key(item) for item in subjects),
            f"{state} 候选={actual}",
        )
    for scope, subjects in expected["scope_classes"].items():
        actual = sorted(key for key, candidate in by_subject.items() if candidate["scope_class"] == scope)
        report.record(
            f"scope-{scope}",
            actual == sorted(subject_key(item) for item in subjects),
            f"{scope} 候选={actual}",
        )
    for subject, count in expected["evidence"].items():
        key = subject_key(subject)
        candidate = by_subject.get(key)
        actual = len(candidate["evidence"]) if candidate else -1
        report.record(f"evidence-{key}", actual == count, f"证据数 {actual}（预期 {count}）")
    for subject, count in expected["rejections"].items():
        key = subject_key(subject)
        candidate = by_subject.get(key)
        actual = len(candidate["rejections"]) if candidate else -1
        report.record(f"rejections-{key}", actual == count, f"拒绝记忆 {actual} 条（预期 {count}）")
    for subject, count in expected["scope_revisions"].items():
        key = subject_key(subject)
        candidate = by_subject.get(key)
        actual = len(candidate["scope_revisions"]) if candidate else -1
        report.record(f"revisions-{key}", actual == count, f"范围修订 {actual} 条（预期 {count}）")
    for subject, target in expected["merged_into"].items():
        key = subject_key(subject)
        candidate = by_subject.get(key)
        actual = candidate["merged_into"] if candidate else None
        report.record(
            f"merged-into-{key}",
            actual == subject_key(target),
            f"merged_into={actual}（预期 {subject_key(target)}）",
        )

    counters = events[-1]["counters"] if events else {}
    for name, count in expected["counters"].items():
        report.record(
            f"counter-{name}",
            counters.get(name) == count,
            f"{name}={counters.get(name)}（预期 {count}）",
        )
    observe_steps, decide_steps = expected["sequence_steps"]
    total_steps = 1 + observe_steps + decide_steps
    report.record(
        "counter-event-count",
        len(events) == total_steps,
        f"事件数={len(events)}（预期 {total_steps}：adopt 1 + 观测 {observe_steps} + 决定 {decide_steps}）",
    )
    zero_retrieval = all(
        event.get("counters", {}).get("retrieval_calls", 0) == 0 for event in events
    )
    report.record(
        "no-network-no-model",
        zero_retrieval and expected.get("retrieval_calls") == 0 and expected.get("model_calls") == 0,
        "全部事件检索调用与模型调用均为 0",
    )


def check_replay(
    package: str, author: dict[str, Any], cases: dict[str, Any], candidates: list[dict[str, Any]], events: list[dict[str, Any]], report: Report
) -> None:
    try:
        queue = replay(package, author, cases["sequence"])
    except (ValueError, OSError, json.JSONDecodeError) as error:
        report.fail("replay", f"按合同重放失败：{error}")
        return
    report.ok("replay", "按 FR-13 语义独立重放全部输入成功")
    disk = {subject_key(candidate["subject"]): candidate for candidate in candidates}
    report.record(
        "replay-candidate-count",
        len(disk) == len(queue.candidates),
        f"磁盘 {len(disk)} 项 / 重放 {len(queue.candidates)} 项",
    )
    for key, candidate in queue.candidates.items():
        stored = disk.get(key)
        if stored is None:
            report.fail(f"replay-missing:{key}", "重放存在但磁盘缺失")
            continue
        for field in ("scope_class", "state", "merged_into"):
            report.record(
                f"replay-{field}:{key}",
                stored[field] == candidate[field],
                f"{field}={stored[field]}（重放 {candidate[field]}）",
            )
        report.record(
            f"replay-evidence:{key}",
            len(stored["evidence"]) == len(candidate["evidence"]),
            f"证据数 {len(stored['evidence'])}（重放 {len(candidate['evidence'])}）",
        )
        report.record(
            f"replay-rejections:{key}",
            len(stored["rejections"]) == len(candidate["rejections"]),
            f"拒绝 {len(stored['rejections'])}（重放 {len(candidate['rejections'])}）",
        )
        report.record(
            f"replay-revisions:{key}",
            len(stored["scope_revisions"]) == len(candidate["scope_revisions"]),
            f"修订 {len(stored['scope_revisions'])}（重放 {len(candidate['scope_revisions'])}）",
        )
    report.record(
        "replay-must-do",
        sorted(queue.must_do) == sorted(subject_key(item) for item in cases["expected"]["must_do"]),
        f"重放必做集合={sorted(queue.must_do)}",
    )
    report.record(
        "replay-queue-only",
        sorted(queue.queue_only()) == sorted(subject_key(item) for item in cases["expected"]["queue_only"]),
        f"重放越界集合={sorted(queue.queue_only())}",
    )
    counters = events[-1]["counters"] if events else {}
    for name, value in queue.counters.items():
        report.record(
            f"replay-counter-{name}",
            counters.get(name) == value,
            f"磁盘 {name}={counters.get(name)}（重放 {value}）",
        )


def check_frozen(package: str, report: Report) -> dict[str, Any] | None:
    path = os.path.join(package, "frozen-sha256.json")
    if not os.path.isfile(path):
        report.fail("frozen-present", "frozen-sha256.json 缺失")
        return None
    try:
        frozen = strict_load(
            path,
            ("schema_version", "producer", "workflow_id", "case_count", "files"),
            "frozen-sha256.json",
            canonical_required=False,
        )
    except ValueError as error:
        report.fail("frozen-strict", str(error))
        return None
    files = frozen["files"]
    actual = {}
    for base, _dirs, names in os.walk(package):
        for name in names:
            full = os.path.join(base, name)
            relative = os.path.relpath(full, package).replace(os.sep, "/")
            if relative == "frozen-sha256.json":
                continue
            data = read_bytes(full)
            actual[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    report.record(
        "frozen-file-set",
        set(files) == set(actual),
        f"冻结清单 {len(files)} 个 / 实际 {len(actual)} 个",
    )
    for relative, stamp in sorted(files.items()):
        truth = actual.get(relative)
        ok = truth is not None and truth["sha256"] == stamp["sha256"] and truth["byte_length"] == stamp["byte_length"]
        report.record(f"frozen-bytes:{relative}", ok, "字节与冻结记录一致" if ok else "字节不符")
    return frozen


def freeze(package: str) -> int:
    files = {}
    for base, _dirs, names in os.walk(package):
        for name in names:
            full = os.path.join(base, name)
            relative = os.path.relpath(full, package).replace(os.sep, "/")
            if relative == "frozen-sha256.json":
                continue
            data = read_bytes(full)
            files[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    cases_path = os.path.join(package, "cases.json")
    case_count = len(json.loads(read_bytes(cases_path).decode("utf-8"))["sequence"])
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": case_count,
        "files": dict(sorted(files.items())),
    }
    target = os.path.join(package, "frozen-sha256.json")
    with open(target, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    print(f"已冻结 {len(files)} 个成员到 {target}（case_count={case_count}）。")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="B08-T01 研究候选队列的独立只读检查器（不调用 CLI 判定）。"
    )
    parser.add_argument("root", nargs="?", help="队列根目录")
    parser.add_argument("--package", required=False, default=None, help="冻结作者包目录（fixtures/research-queue-v1）")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出全部检查结果")
    parser.add_argument("--freeze", action="store_true", help="重新生成 frozen-sha256.json（需人工核对后提交）")
    args = parser.parse_args(argv)

    if args.freeze:
        if not args.package:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        return freeze(os.path.abspath(args.package))

    if not args.root or not args.package:
        print("需要 <队列根> 与 --package。", file=sys.stderr)
        return 2

    root = os.path.abspath(args.root)
    package = os.path.abspath(args.package)
    report = Report()

    if not os.path.isdir(root):
        print(f"拒绝：{root} 不是目录。", file=sys.stderr)
        return 2

    frozen = check_frozen(package, report)
    try:
        author = strict_load(
            os.path.join(package, "author.json"),
            (
                "policy",
                "schema_version",
                "producer",
                "case_id",
                "objective",
                "limits",
                "observations",
                "decisions",
            ),
            "author.json",
            canonical_required=False,
        )
        cases = strict_load(
            os.path.join(package, "cases.json"),
            ("schema_version", "producer", "case_id", "policy", "description", "sequence", "expected", "boundaries"),
            "cases.json",
            canonical_required=False,
        )
    except (ValueError, OSError) as error:
        print(f"拒绝：{error}", file=sys.stderr)
        return 2

    expected = cases["expected"]
    expected = dict(expected)
    expected["sequence_steps"] = [
        sum(1 for step in cases["sequence"] if step["step"] == "observe"),
        sum(1 for step in cases["sequence"] if step["step"] == "decide"),
    ]

    managed = check_layout(root, report)
    owner = None
    metadata = None
    candidates: list[dict[str, Any]] = []
    events: list[dict[str, Any]] = []
    if managed:
        owner = check_owner(root, package, report)
        metadata, candidates, events = check_database(root, report)
        if owner and metadata and events:
            check_queue(owner, metadata, candidates, events, expected, report)
            check_replay(package, author, cases, candidates, events, report)

    report.record("frozen-anchor", frozen is not None, "冻结清单存在并逐字节核对")

    payload = report.dump()
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        for check in payload["checks"]:
            mark = {"ok": "通过", "warning": "提示", "failed": "失败"}[check["status"]]
            line = f"[{mark}] {check['check']}"
            if check["detail"]:
                line += f"：{check['detail']}"
            print(line)
        print()
        print(f"合计 {len(payload['checks'])} 项检查：失败 {payload['failures']}、提示 {payload['warnings']}。")
    return 0 if payload["failures"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
