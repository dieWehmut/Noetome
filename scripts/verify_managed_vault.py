#!/usr/bin/env python3
"""B07-T03 独立检查器：只读磁盘与 SQLite，重建三方字节、影响清单、审批绑定与逐项日志。

它不调用 CLI 的判定函数，也不 import Rust 侧任何代码；全部结论都由本文件从
`control/owner.json`、`control/vault.sqlite`、`baseline/`、`vault/`、`releases/`
以及冻结作者包重新计算得出，再与 `cases.json` 的预期逐条对照。

用法：
    python3 scripts/verify_managed_vault.py <托管根> \
        [--package <作者包目录>] [--run-dir <计划/批准文件所在目录>] [--json]

退出码：0 = 全部硬检查通过；1 = 有硬检查失败；2 = 用法错误。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import posixpath
import re
import sqlite3
import sys
from typing import Any, Iterable

POLICY = "managed-vault-update-v1"
SCHEMA_VERSION = 1
APPLICATION_ID = 0x4E56554C
PRODUCER = "noetome-managed-vault"

CONTROL = "control"
BASELINE = "baseline"
VAULT = "vault"
RELEASES = "releases"
OWNER = "owner.json"
DATABASE = "vault.sqlite"
LOCK = "update.lock"
RELEASE_RECORD = "release.json"
RELEASE_CANDIDATE = "candidate"
RELEASE_LOG = "commit.log"
OPERATIONS = "operations.json"

REQUIRED_TABLES = {
    "metadata": (
        "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),"
        "schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL)"
    ),
    "plans": (
        "CREATE TABLE plans(plan_id TEXT PRIMARY KEY,kind TEXT NOT NULL,"
        "plan_sha256 TEXT NOT NULL UNIQUE,payload_json TEXT NOT NULL,created_at TEXT NOT NULL)"
    ),
    "releases": (
        "CREATE TABLE releases(sequence INTEGER PRIMARY KEY CHECK(sequence>0),"
        "release_id TEXT NOT NULL UNIQUE,approval_sha256 TEXT NOT NULL,"
        "status TEXT NOT NULL,entries_json TEXT NOT NULL,at TEXT NOT NULL)"
    ),
}

# 契约点名的五个拒绝码；其余诊断码不属于本检查器的判定对象。
REFUSALS = (
    "path-outside-managed-set",
    "link-would-break",
    "delete-requires-explicit-flag",
    "approval-stale",
    "rollback-conflict",
)

LINK_PATTERN = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")


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


# ---------------------------------------------------------------------------
# 字节与规范化
# ---------------------------------------------------------------------------


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as handle:
        return handle.read()


def canonical(value: Any) -> str:
    """与 serde_json::to_string 同形的紧凑 JSON：无空格、非 ASCII 原样输出。"""
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(path_or_bytes: Any, fields: tuple[str, ...], label: str) -> Any:
    """严格读取：UTF-8、字段集完全一致、且重新序列化后逐字节相同。"""
    raw = read_bytes(path_or_bytes) if isinstance(path_or_bytes, str) else path_or_bytes
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
    if canonical(value) != text:
        raise ValueError(f"{label}: 非规范编码、重复键或多余空白")
    return value


def walk_files(root: str) -> list[str]:
    found: list[str] = []
    for base, directories, files in os.walk(root):
        directories.sort()
        for name in sorted(files):
            relative = os.path.relpath(os.path.join(base, name), root)
            found.append(relative.replace(os.sep, "/"))
    return found


def normalise(relative: str) -> str:
    """把托管相对路径按 `..` 词法折叠，用于链接解析对账。"""
    parts: list[str] = []
    for part in relative.split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            if parts:
                parts.pop()
            continue
        parts.append(part)
    return "/".join(parts)


# ---------------------------------------------------------------------------
# 冻结作者包与独立判定
# ---------------------------------------------------------------------------


def load_package(directory: str) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    author = json.loads(read_bytes(os.path.join(directory, "author.json")))
    cases: dict[str, dict[str, Any]] = {}
    for case in json.loads(read_bytes(os.path.join(directory, "cases.json")))["cases"]:
        cases[case["id"]] = case
    return author, cases


def author_paths(author: dict[str, Any]) -> tuple[set[str], set[str]]:
    notes = {note["path"] for note in author["notes"]}
    resources = {resource["path"] for resource in author["resources"]}
    return notes, resources


def managed_set(author: dict[str, Any]) -> set[str]:
    notes, resources = author_paths(author)
    return notes | resources


def resolve_link(source: str, raw: str) -> str | None:
    """把笔记里的相对链接解析成托管相对路径；带协议或锚点的一律不算托管链接。"""
    target = raw.split("#", 1)[0].split("?", 1)[0]
    if not target or "://" in target:
        return None
    if target.startswith("/"):
        return None
    return normalise(posixpath.join(posixpath.dirname(source), target))


def note_links(source: str, text: str) -> list[str]:
    resolved: list[str] = []
    for raw in LINK_PATTERN.findall(text):
        target = resolve_link(source, raw)
        if target is not None:
            resolved.append(target)
    return resolved


def candidate_files(directory: str) -> dict[str, bytes]:
    """候选目录里除 operations.json 之外的文件，按托管相对路径给出字节。"""
    files: dict[str, bytes] = {}
    for relative in walk_files(directory):
        if relative == OPERATIONS:
            continue
        files[relative] = read_bytes(os.path.join(directory, relative))
    return files


def decide_candidate(
    author: dict[str, Any],
    candidate_dir: str,
    baseline: dict[str, bytes],
    current: dict[str, bytes],
) -> tuple[str, str | None]:
    """从磁盘独立重算一份候选的判定结果，不参考 Rust 侧任何输出。

    检查顺序与契约一致：先看托管集合，再看删除，最后看链接。返回
    ("approvable", None) 或 ("refuse", 拒绝码)。
    """
    operations = json.loads(read_bytes(os.path.join(candidate_dir, OPERATIONS)))
    if operations.get("policy") != POLICY:
        return "refuse", "policy-mismatch"
    notes, resources = author_paths(author)
    managed = notes | resources

    for operation in operations["operations"]:
        kind = operation["kind"]
        if kind not in author["operations"]:
            return "refuse", "operation-not-declared"
        # 契约：重命名的 `to` 是本计划引入的新路径，不属于既有托管集合；
        # 只有来源与既有目标必须在集合内。
        if kind == "rename":
            touched = [operation["from"]]
        else:
            touched = [operation["path"]]
        for path in touched:
            if path not in managed:
                return "refuse", "path-outside-managed-set"

    for operation in operations["operations"]:
        if operation["kind"] == "delete":
            # 契约：计划阶段删除一律拒绝，必须由批准文件显式携带该操作。
            return "refuse", "delete-requires-explicit-flag"

    files = candidate_files(candidate_dir)
    final: dict[str, bytes] = {}
    for path, bytes_ in baseline.items():
        final[path] = current.get(path, bytes_)
    renames: dict[str, str] = {}
    for operation in operations["operations"]:
        kind = operation["kind"]
        if kind in ("edit", "link", "attach"):
            path = operation["path"]
            if path in final:
                final[path] = files[path]
        elif kind == "rename":
            source = operation["from"]
            target = operation["to"]
            renames[source] = target
            if source in final:
                final[target] = final.pop(source)

    for path in list(final):
        if path in resources:
            continue
        for target in note_links(path, final[path].decode("utf-8", "replace")):
            if target not in final:
                return "refuse", "link-would-break"
    return "approvable", None


# ---------------------------------------------------------------------------
# 所有权与基准
# ---------------------------------------------------------------------------


OWNER_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "root",
    "root_identity",
    "parent_identity",
    "control_identity",
    "owner_identity",
    "baseline_identity",
    "vault_identity",
    "lock_identity",
    "database_identity",
    "author_manifest_sha256",
    "baseline_manifest_sha256",
    "created_at",
)

BASELINE_FILE_FIELDS = ("path", "sha256", "byte_length")


def check_layout(root: str, report: Report) -> bool:
    control = os.path.join(root, CONTROL)
    if not os.path.isdir(control):
        report.warn(
            "layout.not-managed",
            f"{root} 里没有 control/：这是陌生 Vault 或纯人工目录，按契约保持只读，什么都不改。",
        )
        return False
    allowed = {CONTROL, BASELINE, VAULT, RELEASES}
    present = {name for name in os.listdir(root)}
    unknown = sorted(present - allowed)
    report.record(
        "layout.root-entries",
        not unknown,
        "托管根只允许 control/、baseline/、vault/、releases/" if unknown else "",
    )
    if unknown:
        report.fail("layout.root-entries", f"托管根出现契约之外的条目：{unknown}")
    for directory in (BASELINE, VAULT):
        report.record(
            f"layout.{directory}",
            os.path.isdir(os.path.join(root, directory)),
            "",
        )
    control_entries = set(os.listdir(control))
    expected = {OWNER, DATABASE, LOCK}
    extra = sorted(control_entries - expected)
    missing = sorted(expected - control_entries)
    if extra or missing:
        report.fail("layout.control-entries", f"control/ 条目不符（多 {extra}、缺 {missing}）")
    else:
        report.ok("layout.control-entries", "control/ 只有 owner.json、vault.sqlite、update.lock")
    return os.path.isfile(os.path.join(control, OWNER))


def check_owner(root: str, package: str | None, report: Report) -> dict[str, Any] | None:
    path = os.path.join(root, CONTROL, OWNER)
    try:
        owner = strict_load(path, OWNER_FIELDS, "control/owner.json")
    except ValueError as error:
        report.fail("owner.strict", str(error))
        return None
    report.ok("owner.strict", "owner.json 严格、规范、无未知字段")

    report.record(
        "owner.policy",
        owner["policy"] == POLICY and owner["schema_version"] == SCHEMA_VERSION,
        f"policy={owner['policy']} schema_version={owner['schema_version']}",
    )
    report.record("owner.producer", owner["producer"] == PRODUCER, f"producer={owner['producer']}")

    recorded = os.path.normpath(owner["root"])
    actual = os.path.normpath(os.path.abspath(root))
    if recorded != actual:
        report.warn(
            "owner.root-path",
            f"owner.json 记录 root={recorded}，本次检查 {actual}；"
            "复制/搬移托管根会让该路径与 inode 身份不再吻合（CLI 侧同样会拒绝），不是内容错误。",
        )
    else:
        report.ok("owner.root-path", "记录路径与检查路径一致")

    for name in (
        "root_identity",
        "parent_identity",
        "control_identity",
        "owner_identity",
        "baseline_identity",
        "vault_identity",
        "lock_identity",
        "database_identity",
    ):
        identity = owner.get(name)
        report.record(
            f"owner.{name}",
            isinstance(identity, dict) and {"platform", "device", "inode"} <= set(identity),
            f"{identity}",
        )
    return owner


def check_baseline(
    root: str,
    owner: dict[str, Any],
    package: str | None,
    report: Report,
    metadata: dict[str, Any] | None,
) -> dict[str, bytes]:
    baseline_root = os.path.join(root, BASELINE)
    baseline: dict[str, bytes] = {}
    for relative in walk_files(baseline_root):
        baseline[relative] = read_bytes(os.path.join(baseline_root, relative))

    files = [
        {"path": path, "sha256": sha256(data), "byte_length": len(data)}
        for path, data in sorted(baseline.items())
    ]
    recomputed = sha256(canonical(files).encode("utf-8"))
    report.record(
        "baseline.manifest",
        recomputed == owner["baseline_manifest_sha256"],
        f"重算 {recomputed} / owner.json {owner['baseline_manifest_sha256']}",
    )

    if package:
        author_bytes = read_bytes(os.path.join(package, "author.json"))
        report.record(
            "baseline.author-manifest",
            sha256(author_bytes) == owner["author_manifest_sha256"],
            f"重算 {sha256(author_bytes)} / owner.json {owner['author_manifest_sha256']}",
        )
        author = json.loads(author_bytes)
        mismatched: list[str] = []
        for note in author["notes"] + author["resources"]:
            declared = note["sha256"]
            actual = sha256(baseline.get(note["path"], b""))
            if actual != declared or len(baseline.get(note["path"], b"")) != note["byte_length"]:
                mismatched.append(f"{note['path']}（声明 {declared[:12]}，baseline 实为 {actual[:12]}）")
        report.record(
            "baseline.matches-author",
            not mismatched,
            "；".join(mismatched),
        )

    if metadata is not None:
        declared = [
            {"path": item["path"], "sha256": item["sha256"], "byte_length": item["byte_length"]}
            for item in metadata["baseline_files"]
        ]
        report.record(
            "baseline.matches-metadata",
            declared == files,
            "metadata.baseline_files 与 baseline/ 实际字节一致"
            if declared == files
            else f"metadata 记录 {len(declared)} 项、磁盘 {len(files)} 项且内容不同",
        )
    return baseline


# ---------------------------------------------------------------------------
# 数据库
# ---------------------------------------------------------------------------


def check_database(root: str, report: Report) -> tuple[dict[str, Any] | None, list[dict], list[dict]]:
    path = os.path.join(root, CONTROL, DATABASE)
    if not os.path.isfile(path):
        report.fail("db.exists", f"{path} 不存在")
        return None, [], []
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        application_id = connection.execute("PRAGMA application_id").fetchone()[0]
        user_version = connection.execute("PRAGMA user_version").fetchone()[0]
        journal_mode = connection.execute("PRAGMA journal_mode").fetchone()[0]
        report.record(
            "db.pragmas",
            application_id == APPLICATION_ID
            and user_version == SCHEMA_VERSION
            and journal_mode.lower() == "delete",
            f"application_id={application_id} user_version={user_version} journal_mode={journal_mode}",
        )

        tables = {
            name: sql
            for kind, name, sql in connection.execute(
                "select type, name, sql from sqlite_master where type='table' and sql is not null"
            )
        }
        report.record(
            "db.tables",
            set(tables) == set(REQUIRED_TABLES),
            f"实际表：{sorted(tables)}",
        )
        for name, expected in REQUIRED_TABLES.items():
            actual = " ".join((tables.get(name) or "").split())
            report.record(
                f"db.ddl.{name}",
                actual == expected,
                "" if actual == expected else f"实际：{actual}",
            )

        rows = connection.execute(
            "select singleton, schema_version, metadata_json from metadata"
        ).fetchall()
        metadata = None
        if len(rows) != 1:
            report.fail("db.metadata-row", f"metadata 应有且只有一行，实际 {len(rows)} 行")
        else:
            singleton, schema_version, metadata_json = rows[0]
            report.record(
                "db.metadata-row",
                singleton == 1 and schema_version == SCHEMA_VERSION,
                f"singleton={singleton} schema_version={schema_version}",
            )
            try:
                metadata = strict_load(
                    metadata_json.encode("utf-8"),
                    (
                        "policy",
                        "producer",
                        "schema_version",
                        "root",
                        "parent_identity",
                        "created_at",
                        "author",
                        "author_manifest_sha256",
                        "author_canonical_sha256",
                        "baseline_manifest_sha256",
                        "baseline_files",
                    ),
                    "metadata.metadata_json",
                )
                report.ok("db.metadata-strict", "metadata_json 严格且规范")
            except ValueError as error:
                report.fail("db.metadata-strict", str(error))

        plans = []
        for plan_id, kind, plan_sha256, payload_json, created_at in connection.execute(
            "select plan_id, kind, plan_sha256, payload_json, created_at from plans"
        ):
            digest = sha256(payload_json.encode("utf-8"))
            report.record(
                f"db.plan[{plan_id[:12]}]",
                digest == plan_id == plan_sha256,
                f"重算 payload SHA256 {digest}",
            )
            plans.append(
                {
                    "plan_id": plan_id,
                    "kind": kind,
                    "plan_sha256": plan_sha256,
                    "payload": json.loads(payload_json),
                    "payload_json": payload_json,
                    "created_at": created_at,
                }
            )

        releases = []
        previous = 0
        for sequence, release_id, approval_sha256, status, entries_json, at in connection.execute(
            "select sequence, release_id, approval_sha256, status, entries_json, at "
            "from releases order by sequence"
        ):
            report.record(
                f"db.release[{sequence}].sequence",
                sequence == previous + 1,
                f"序号 {sequence}，上一行 {previous}",
            )
            previous = sequence
            try:
                entries = json.loads(entries_json)
                report.record(
                    f"db.release[{sequence}].entries-strict",
                    canonical(entries) == entries_json,
                    "",
                )
            except json.JSONDecodeError as error:
                report.fail(f"db.release[{sequence}].entries-strict", str(error))
                continue
            releases.append(
                {
                    "sequence": sequence,
                    "release_id": release_id,
                    "approval_sha256": approval_sha256,
                    "status": status,
                    "entries": entries,
                    "at": at,
                }
            )
        return metadata, plans, releases
    finally:
        connection.close()


# ---------------------------------------------------------------------------
# 发布记录、逐项日志与三方字节
# ---------------------------------------------------------------------------


ENTRY_FIELDS = ("action", "path", "sha256", "byte_length", "previous_sha256")
IMPACT_FIELDS = ("category", "path", "detail")
RELEASE_FIELDS = (
    "policy",
    "schema_version",
    "release_id",
    "approval_sha256",
    "plan_id",
    "plan_sha256",
    "kind",
    "case_id",
    "entries",
    "impacts",
    "source",
    "created_at",
)


def check_releases(
    root: str,
    releases: list[dict],
    plans: list[dict],
    baseline: dict[str, bytes],
    report: Report,
) -> dict[str, bytes]:
    vault_root = os.path.join(root, VAULT)
    current: dict[str, bytes] = {}
    for relative in walk_files(vault_root):
        current[relative] = read_bytes(os.path.join(vault_root, relative))

    covered: dict[str, str] = {}
    releases_root = os.path.join(root, RELEASES)
    if os.path.isdir(releases_root):
        unknown = sorted(set(os.listdir(releases_root)) - {row["release_id"] for row in releases})
        report.record(
            "releases.orphan-directories",
            not unknown,
            f"账本里没有这些 release 目录：{unknown}" if unknown else "",
        )
    else:
        report.record("releases.present", not releases, "没有 releases/ 目录" if not releases else "")
        if releases:
            report.fail("releases.present", "账本有发布行，但 releases/ 目录不存在")

    plan_by_id = {plan["plan_id"]: plan for plan in plans}
    for row in releases:
        release_id = row["release_id"]
        directory = os.path.join(releases_root, release_id)
        label = f"release[{row['sequence']}]"
        report.record(f"{label}.dir", os.path.isdir(directory), release_id)
        if not os.path.isdir(directory):
            continue
        record_path = os.path.join(directory, RELEASE_RECORD)
        try:
            record = strict_load(record_path, RELEASE_FIELDS, f"{label}/release.json")
        except (OSError, ValueError) as error:
            report.fail(f"{label}.record", str(error))
            continue
        report.record(
            f"{label}.record-identity",
            record["release_id"] == release_id
            and record["approval_sha256"] == row["approval_sha256"]
            and record["policy"] == POLICY,
            f"release_id={record['release_id'][:12]} approval={record['approval_sha256'][:12]}",
        )

        plan = plan_by_id.get(record["plan_id"])
        report.record(f"{label}.plan-present", plan is not None, f"plan_id={record['plan_id'][:12]}")
        if plan is not None:
            impacts_sha256 = sha256(canonical(record["impacts"]).encode("utf-8"))
            report.record(
                f"{label}.impacts-binding",
                impacts_sha256 == plan["payload"]["impacts_sha256"],
                f"重算 {impacts_sha256}",
            )
            report.record(
                f"{label}.entries-match-plan",
                record["entries"] == plan["payload"]["entries"],
                "",
            )

        logged: list[dict] = []
        log_path = os.path.join(directory, RELEASE_LOG)
        try:
            text = read_bytes(log_path).decode("utf-8")
            for index, line in enumerate([item for item in text.splitlines() if item.strip()]):
                entry = json.loads(line)
                report.record(
                    f"{label}.log[{index}].canonical",
                    canonical(entry) == line,
                    "" if canonical(entry) == line else "该行不是规范编码",
                )
                report.record(
                    f"{label}.log[{index}].sequence",
                    entry.get("sequence") == row["sequence"] and entry.get("index") == index,
                    f"sequence={entry.get('sequence')} index={entry.get('index')}",
                )
                logged.append(entry)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            report.fail(f"{label}.log", f"commit.log 读取失败：{error}")

        states = row["entries"]
        for position, entry in enumerate(states):
            report.record(
                f"{label}.state[{position}]",
                entry.get("action") in ("write", "delete") and entry.get("state") in ("written", "deleted"),
                f"{entry.get('action')} {entry.get('path')} state={entry.get('state')}",
            )
            covered[entry["path"]] = f"{label}"
        report.record(
            f"{label}.log-covers-entries",
            len(logged) == len(states)
            and all(
                logged[index].get("path") == states[index].get("path")
                and logged[index].get("sha256") == states[index].get("sha256")
                and logged[index].get("action") == states[index].get("action")
                for index in range(min(len(logged), len(states)))
            ),
            f"日志 {len(logged)} 行、账本 {len(states)} 条",
        )
        report.record(
            f"{label}.status",
            (row["status"] == "published") == (len(logged) == len(states) and len(states) > 0),
            f"status={row['status']} 已落盘 {len(logged)}/{len(states)}",
        )

        for entry in states:
            if entry.get("action") != "write":
                continue
            candidate = os.path.join(directory, RELEASE_CANDIDATE, entry["path"])
            if not os.path.isfile(candidate):
                report.fail(f"{label}.candidate[{entry['path']}]", "暂存候选缺失")
                continue
            data = read_bytes(candidate)
            report.record(
                f"{label}.candidate[{entry['path']}]",
                sha256(data) == entry["sha256"] and len(data) == entry["byte_length"],
                f"{len(data)} 字节",
            )

    # 未批准覆盖：vault/ 里任何与基准不同的字节都必须能被某条发布条目解释。
    unexplained = [
        path
        for path, data in sorted(current.items())
        if (path in baseline and baseline[path] != data or path not in baseline)
        and path not in covered
    ]
    report.record(
        "vault.unapproved-overwrite",
        not unexplained,
        f"以下路径与基准不同但没有任何发布条目：{unexplained}" if unexplained else "未批准覆盖 0",
    )

    stray = [path for path in current if path not in baseline and path not in covered]
    report.record(
        "vault.unmanaged-content",
        not stray,
        f"vault/ 里出现托管集合之外的路径：{stray}" if stray else "vault/ 只含托管文件",
    )
    return current


def check_approvals(run_dir: str, plans: list[dict], report: Report) -> None:
    plan_by_id = {plan["plan_id"]: plan for plan in plans}
    seen = 0
    for name in sorted(os.listdir(run_dir)):
        if not name.endswith(".approval.json"):
            continue
        path = os.path.join(run_dir, name)
        try:
            approval = json.loads(read_bytes(path))
        except json.JSONDecodeError as error:
            report.fail(f"approval[{name}]", f"不是合法 JSON：{error}")
            continue
        seen += 1
        plan = plan_by_id.get(approval.get("plan_id"))
        if plan is None:
            report.fail(f"approval[{name}].plan", f"plan_id={approval.get('plan_id')} 不在账本里")
            continue
        report.record(
            f"approval[{name}].plan-binding",
            approval.get("plan_sha256") == plan["plan_sha256"],
            "",
        )
        report.record(
            f"approval[{name}].entries",
            approval.get("entries") == plan["payload"]["entries"],
            "",
        )
        report.record(
            f"approval[{name}].stamps",
            approval.get("stamps") == plan["payload"]["stamps"],
            "",
        )
        report.record(
            f"approval[{name}].impacts",
            approval.get("impacts") == plan["payload"]["impacts"]
            and approval.get("impacts_sha256") == plan["payload"]["impacts_sha256"],
            "",
        )
        report.record(
            f"approval[{name}].vault-untouched",
            approval.get("vault_written") is False,
            f"vault_written={approval.get('vault_written')}",
        )
    report.record("approvals.scanned", True, f"核对了 {seen} 份批准文件")


def check_cases(
    package: str,
    baseline: dict[str, bytes],
    report: Report,
) -> None:
    """八类候选的独立判定。

    `cases.json` 的预期是按 adopt 刚完成、`vault/` 等于基准时写的，所以判定必须在
    同一初始状态下重算；用发布后漂移过的 `vault/` 会得出另一份（同样正确的）结论，
    那不是候选本身的问题。真实 `vault/` 与基准的差异另由 `vault.*` 检查覆盖。
    """
    author, cases = load_package(package)
    initial = dict(baseline)
    for case_id, case in sorted(cases.items()):
        candidate_dir = os.path.join(package, case["candidate"])
        expected = case["expect"]
        decision, refusal = decide_candidate(author, candidate_dir, initial, initial)
        matched = (decision, refusal) == (expected["decision"], expected["refusal"])
        report.record(
            f"cases.{case_id}",
            matched,
            f"独立判定 {decision}/{refusal}，cases.json 预期 {expected['decision']}/{expected['refusal']}",
        )


# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="B07-T03 托管测试集合的独立只读检查器（不调用 CLI 判定）。"
    )
    parser.add_argument("root", help="托管根目录")
    parser.add_argument("--package", default=None, help="冻结作者包目录（fixtures/managed-vault-update-v1）")
    parser.add_argument("--run-dir", default=None, help="含计划/批准文件的目录，用于核对审批绑定")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出全部检查结果")
    args = parser.parse_args(argv)

    root = os.path.abspath(args.root)
    report = Report()

    if not os.path.isdir(root):
        print(f"拒绝：{root} 不是目录。", file=sys.stderr)
        return 2

    managed = check_layout(root, report)
    if not managed:
        payload = report.dump()
        payload["status"] = "not-managed"
        print(json.dumps(payload, ensure_ascii=False, indent=2) if args.json else
              "未接管：没有 control/，按契约保持只读，未做任何写入。")
        return 0

    owner = check_owner(root, args.package, report)
    metadata, plans, releases = check_database(root, report)
    baseline = check_baseline(root, owner, args.package, report, metadata) if owner else {}
    current = check_releases(root, releases, plans, baseline, report)

    if args.package:
        check_cases(args.package, baseline, report)
    if args.run_dir:
        check_approvals(args.run_dir, plans, report)

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
        print(
            f"合计 {len(payload['checks'])} 项检查：失败 {payload['failures']}、提示 {payload['warnings']}。"
        )
    return 0 if payload["failures"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
