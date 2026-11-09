#!/usr/bin/env python3
"""Independent read-only audit of a revision-feedback root produced by B08-T03.

This auditor never imports the Rust implementation and never calls the CLI to
decide anything. It re-derives the expected IR version chain, gap transitions
and rendered bytes from the frozen author package, then compares them with the
SQLite ledger and the files on disk.

Usage:
    python3 scripts/verify_revision_feedback.py <root> \
        [--package fixtures/revision-feedback-v1] [--json] [--freeze]
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sqlite3
import sys
from pathlib import Path

POLICY = "revision-feedback-v1"
CORPUS_POLICY = "revision-corpus-v1"
PRODUCER = "revision-feedback"
AUTHOR_PRODUCER = "fixture_author"
SCHEMA_VERSION = 1
APPLICATION_ID = 0x4E525652

CONTROL = "control"
OWNER = "owner.json"
DATABASE = "revision.sqlite"
LOCK = "run.lock"
RENDER = "render"

MAX_TEXT = 600
MAX_ID = 48
MAX_ATOMS = 128
MAX_REPRESENTATIONS = 16
MAX_EVIDENCE = 8
MAX_UNIT = 32
MAX_REVISIONS_LIMIT = 8
MAX_ENTRIES = 64

KIND_TABLE = "table"
KIND_PARAGRAPH = "paragraph"
KIND_TRACEABILITY = "traceability"
KINDS = (KIND_TABLE, KIND_PARAGRAPH, KIND_TRACEABILITY)

VERIFIED = "verified"
PENDING = "pending_verification"

GAP_OPEN = "open"
GAP_RESOLVED = "resolved"
GAP_PENDING = "pending_verification"
GAP_ESCALATED = "escalated"

SEVERITY_NORMAL = "normal"
SEVERITY_MANUAL = "manual_judgment_required"

REQUIRED_TABLES = {
    "metadata": (
        "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),"
        "schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL)"
    ),
    "gaps": (
        "CREATE TABLE gaps(gap_id TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),"
        "status TEXT NOT NULL,payload_json TEXT NOT NULL)"
    ),
    "revisions": (
        "CREATE TABLE revisions(revision_id TEXT PRIMARY KEY,sequence INTEGER NOT NULL CHECK(sequence>0),"
        "gap_id TEXT NOT NULL,payload_json TEXT NOT NULL)"
    ),
    "representations": (
        "CREATE TABLE representations(representation_id TEXT PRIMARY KEY,"
        "position INTEGER NOT NULL CHECK(position>=0),atom_ids_json TEXT NOT NULL,"
        "render_sha256 TEXT NOT NULL,render TEXT NOT NULL)"
    ),
    "ir_versions": (
        "CREATE TABLE ir_versions(version INTEGER PRIMARY KEY CHECK(version>=1),"
        "atoms_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,at TEXT NOT NULL)"
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
    "corpus_manifest_sha256",
    "created_at",
)

METADATA_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "author",
    "corpus",
    "author_manifest_sha256",
    "corpus_manifest_sha256",
    "created_at",
)

AUTHOR_FIELDS = (
    "policy",
    "schema_version",
    "producer",
    "case_id",
    "corpus",
    "limits",
    "atoms",
    "representations",
)

ATOM_FIELDS = ("atom_id", "subject", "property", "value", "unit", "evidence")
EVIDENCE_FIELDS = ("source", "locator")
REPRESENTATION_FIELDS = ("representation_id", "kind", "title", "atom_ids")
LIMITS_FIELDS = ("max_revisions",)
CORPUS_FIELDS = ("policy", "schema_version", "case_id", "documents")
DOCUMENT_FIELDS = ("path", "sha256", "byte_length")

GAP_INPUT_FIELDS = (
    "gap_id",
    "representation_id",
    "atom_id",
    "reason",
    "source",
    "impact",
    "note",
)

REVISION_INPUT_FIELDS = (
    "revision_id",
    "gap_id",
    "atom_id",
    "value",
    "unit",
    "evidence",
    "reason",
)

GAP_RECORD_FIELDS = (
    "gap_id",
    "representation_id",
    "atom_id",
    "reason",
    "source",
    "impact",
    "note",
    "status",
    "severity",
    "revisions",
    "created_at",
    "updated_at",
)

REVISION_RECORD_FIELDS = (
    "revision_id",
    "gap_id",
    "atom_id",
    "value",
    "unit",
    "evidence",
    "reason",
    "ir_version",
    "recompiled",
    "unchanged",
    "at",
)

IR_VERSION_FIELDS = ("version", "atoms_sha256", "atoms", "at")

HEX64 = re.compile(r"^[0-9a-f]{64}$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9._+-]+$")


class Refusal(Exception):
    """A deterministic refusal raised by the independent replay."""

    def __init__(self, code: str, exit_code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.exit_code = exit_code
        self.message = message


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.warnings: list[str] = []
        self.items: list[dict] = []

    def record(self, name: str, ok: bool, detail: str = "", *, warn: bool = False) -> None:
        self.items.append({"name": name, "ok": ok, "detail": detail, "warn": warn})
        if not ok:
            if warn:
                self.warnings.append(f"{name}: {detail}")
            else:
                self.failures.append(f"{name}: {detail}")

    def fail(self, name: str, detail: str) -> None:
        self.record(name, False, detail)

    def ok(self, name: str, detail: str = "") -> None:
        self.record(name, True, detail)

    def warn(self, name: str, detail: str) -> None:
        self.record(name, False, detail, warn=True)

    def dump(self) -> dict:
        return {
            "checks": len(self.items),
            "failures": len(self.failures),
            "warnings": len(self.warnings),
            "failure_items": self.failures,
            "warning_items": self.warnings,
            "items": self.items,
        }


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def collapse(text: str) -> str:
    return " ".join(text.split())


def strict_load(payload, fields, label: str, *, canonical_required: bool = True):
    """Parse UTF-8 JSON, require the exact field set, and re-check canonical form."""
    if isinstance(payload, bytes):
        try:
            text = payload.decode("utf-8")
        except UnicodeDecodeError as error:
            raise Refusal(f"{label}-utf8", 3, f"{label} 不是 UTF-8：{error}")
    else:
        text = payload
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise Refusal(f"{label}-json", 3, f"{label} 不是严格 JSON：{error}")
    if not isinstance(value, dict):
        raise Refusal(f"{label}-object", 3, f"{label} 必须是 JSON 对象")
    keys = set(value)
    expected = set(fields)
    if keys != expected:
        raise Refusal(
            f"{label}-fields",
            3,
            f"{label} 字段不符：多余 {sorted(keys - expected)}，缺少 {sorted(expected - keys)}",
        )
    if canonical_required and canonical(value) != text:
        raise Refusal(f"{label}-canonical", 3, f"{label} 非规范编码、重复键或未知字段")
    return value


# --------------------------------------------------------------------------- #
# Deterministic renderer: an independent reimplementation of the contract.
# --------------------------------------------------------------------------- #


def evidence_key(item: dict) -> str:
    return f"{item['source']}{item['locator']}"


def evidence_keys(items) -> set[str]:
    return {evidence_key(item) for item in items}


def verification(atom: dict) -> str:
    return PENDING if not atom["evidence"] else VERIFIED


def render(representation: dict, atoms: dict[str, dict]) -> str:
    ordered = []
    for atom_id in representation["atom_ids"]:
        if atom_id not in atoms:
            raise Refusal(
                "render-missing-atom",
                3,
                f"表示 {representation['representation_id']} 引用的原子 {atom_id} 不在当前 IR",
            )
        ordered.append(atoms[atom_id])
    lines: list[str] = []
    kind = representation["kind"]
    if kind == KIND_TABLE:
        lines.append(f"## {representation['title']}")
        lines.append("")
        lines.append("| 对象 | 属性 | 值 | 单位 | 证据 | 状态 |")
        lines.append("| --- | --- | --- | --- | --- | --- |")
        for atom in ordered:
            marker = "【待核验】" if verification(atom) == PENDING else ""
            unit = atom["unit"] if atom["unit"] else "-"
            lines.append(
                f"| {atom['subject']} | {atom['property']} | {marker}{atom['value']} "
                f"| {unit} | {len(atom['evidence'])} | {verification(atom)} |"
            )
    elif kind == KIND_PARAGRAPH:
        lines.append(f"# {representation['title']}")
        lines.append("")
        for atom in ordered:
            marker = "【待核验】" if verification(atom) == PENDING else ""
            unit = f" {atom['unit']}" if atom["unit"] else ""
            lines.append(
                f"- {atom['subject']} 的 {atom['property']} 是 {marker}{atom['value']}{unit}"
                f"（证据 {len(atom['evidence'])} 条；状态 {verification(atom)}）"
            )
    elif kind == KIND_TRACEABILITY:
        lines.append(f"# {representation['title']}")
        lines.append("")
        for atom in ordered:
            if not atom["evidence"]:
                lines.append(f"- {atom['atom_id']} ← 无来源，待核验")
            else:
                for evidence in atom["evidence"]:
                    lines.append(f"- {atom['atom_id']} ← {evidence['source']}{evidence['locator']}")
    else:
        raise Refusal("render-kind", 2, f"表示类型必须是 {', '.join(KINDS)} 之一（实际 {kind}）")
    return "\n".join(lines) + "\n"


def dependents(representations, atom_id: str) -> set[str]:
    return {
        representation["representation_id"]
        for representation in representations
        if atom_id in representation["atom_ids"]
    }


def atoms_map(atoms) -> dict[str, dict]:
    return {atom["atom_id"]: atom for atom in atoms}


# --------------------------------------------------------------------------- #
# Independent replay of FR-14 semantics.
# --------------------------------------------------------------------------- #


class Replay:
    """Replay the frozen case sequence without touching the product."""

    def __init__(self, package: Path) -> None:
        self.package = package
        author = strict_load(
            read_bytes(package / "author.json"), AUTHOR_FIELDS, "author", canonical_required=False
        )
        self.author = author
        self.corpus = strict_load(
            read_bytes(package / author["corpus"]["manifest"]),
            CORPUS_FIELDS,
            "corpus-manifest",
            canonical_required=False,
        )
        self.limits = author["limits"]
        self.atoms = atoms_map(author["atoms"])
        self.representations = author["representations"]
        self.gaps: dict[str, dict] = {}
        self.gap_order: list[str] = []
        self.revisions: list[dict] = []
        self.versions: list[dict] = []
        self.renders: dict[str, str] = {}
        self.refusals: list[dict] = []
        self.history: list[dict] = []

    # -- helpers ---------------------------------------------------------- #

    def known_paths(self) -> set[str]:
        return {document["path"] for document in self.corpus["documents"]}

    def render_all(self) -> dict[str, str]:
        return {
            representation["representation_id"]: render(representation, self.atoms)
            for representation in self.representations
        }

    def _record_version(self) -> None:
        ordered = [self.atoms[key] for key in sorted(self.atoms)]
        self.renders = self.render_all()
        self.versions.append(
            {
                "version": len(self.versions) + 1,
                "atoms_sha256": sha256(canonical(ordered).encode("utf-8")),
                "atoms": ordered,
            }
        )
        self.history.append(
            {"version": len(self.versions), "renders": dict(self.renders)}
        )

    # -- verbs ------------------------------------------------------------ #

    def adopt(self) -> None:
        if self.versions:
            raise Refusal("replay-adopt-twice", 2, "adopt 只能一次")
        self._record_version()

    def gap(self, name: str) -> None:
        payload = strict_load(
            read_bytes(self.package / name), GAP_INPUT_FIELDS, f"gap:{name}", canonical_required=False
        )
        if not payload["source"]:
            raise Refusal("gap-no-source", 2, "缺口必须带来源")
        positions = {item["representation_id"]: item for item in self.representations}
        if payload["representation_id"] not in positions:
            raise Refusal("gap-unknown-representation", 2, "缺口发现位置不在作者清单里")
        if payload["atom_id"] not in self.atoms:
            raise Refusal("gap-unknown-atom", 2, "缺口涉及的原子不在作者清单里")
        if payload["atom_id"] not in positions[payload["representation_id"]]["atom_ids"]:
            raise Refusal("gap-not-in-representation", 2, "缺口发现位置不引用该原子")
        actual = dependents(self.representations, payload["atom_id"])
        if set(payload["impact"]) != actual:
            raise Refusal("gap-impact", 2, "缺口影响范围必须恰好等于引用该原子的表示")
        if payload["representation_id"] not in actual:
            raise Refusal("gap-position", 2, "缺口发现位置必须包含在影响范围里")
        if payload["gap_id"] in self.gaps:
            raise Refusal("gap-duplicate", 2, "缺口编号必须唯一")
        if len(self.gaps) >= MAX_ENTRIES:
            raise Refusal("gap-limit", 2, "缺口数已达上限")
        for item in payload["source"]:
            if item["source"] not in self.known_paths():
                raise Refusal("gap-source", 2, "缺口来源不在冻结语料清单内")
        self.gaps[payload["gap_id"]] = {
            "gap_id": payload["gap_id"],
            "representation_id": payload["representation_id"],
            "atom_id": payload["atom_id"],
            "reason": payload["reason"],
            "source": payload["source"],
            "impact": payload["impact"],
            "note": payload["note"],
            "status": GAP_OPEN,
            "severity": SEVERITY_NORMAL,
            "revisions": 0,
        }
        self.gap_order.append(payload["gap_id"])

    def revise(self, name: str) -> dict:
        payload = strict_load(
            read_bytes(self.package / name),
            REVISION_INPUT_FIELDS,
            f"revision:{name}",
            canonical_required=False,
        )
        gap = self.gaps.get(payload["gap_id"])
        if gap is None:
            raise Refusal("revision-unknown-gap", 2, "缺口不存在")
        if gap["status"] == GAP_ESCALATED:
            raise Refusal(
                "REVISION_LIMIT_ESCALATED",
                3,
                "缺口已升级为人工判断；不再接受自动修订",
            )
        if gap["atom_id"] != payload["atom_id"]:
            raise Refusal("revision-atom-mismatch", 2, "修订指向的原子与缺口不一致")
        if any(item["revision_id"] == payload["revision_id"] for item in self.revisions):
            raise Refusal("revision-duplicate", 2, "修订编号必须唯一")

        current = self.atoms[payload["atom_id"]]
        previous_values = {
            item["value"]
            for item in self.revisions
            if item["atom_id"] == payload["atom_id"]
        }
        new_keys = evidence_keys(payload["evidence"])
        history_keys: set[str] = set()
        for item in self.revisions:
            if item["gap_id"] == gap["gap_id"]:
                history_keys |= evidence_keys(item["evidence"])
        overturn = payload["value"] in previous_values and payload["value"] != current["value"]
        if overturn and not (new_keys - history_keys):
            raise Refusal(
                "REVISION_NO_NEW_EVIDENCE",
                3,
                "要改回先前版本已核验过的值，但没有带来任何新来源",
            )
        for item in payload["evidence"]:
            if item["source"] not in self.known_paths():
                raise Refusal("revision-source", 2, "修订证据来源不在冻结语料清单内")
        if (
            payload["value"] == current["value"]
            and payload["unit"] == current["unit"]
            and new_keys == evidence_keys(current["evidence"])
        ):
            raise Refusal("REVISION_ARGUMENT", 2, "修订没有改变任何内容")

        affected = dependents(self.representations, payload["atom_id"])
        if not affected:
            raise Refusal("revision-orphan-atom", 3, "原子没有被任何表示引用")
        revised = dict(current)
        revised["value"] = payload["value"]
        revised["unit"] = payload["unit"]
        revised["evidence"] = payload["evidence"]
        self.atoms[payload["atom_id"]] = revised

        previous = dict(self.renders)
        now = self.render_all()
        recompiled = []
        unchanged = []
        changed: dict[str, str] = {}
        for representation in self.representations:
            rid = representation["representation_id"]
            if rid in affected:
                if now[rid] != previous[rid]:
                    changed[rid] = now[rid]
                recompiled.append(rid)
            elif now[rid] != previous[rid]:
                raise Refusal(
                    "revision-unaffected-changed", 3, "未引用该原子的表示重编译后字节却变了"
                )
            else:
                unchanged.append(rid)

        gap["revisions"] += 1
        gap["status"] = GAP_PENDING if verification(revised) == PENDING else GAP_RESOLVED
        over_limit = gap["revisions"] > self.limits["max_revisions"]
        if over_limit:
            gap["status"] = GAP_ESCALATED
            gap["severity"] = SEVERITY_MANUAL

        record = {
            "revision_id": payload["revision_id"],
            "gap_id": gap["gap_id"],
            "atom_id": payload["atom_id"],
            "value": payload["value"],
            "unit": payload["unit"],
            "evidence": payload["evidence"],
            "reason": payload["reason"],
            "ir_version": len(self.versions) + 1,
            "recompiled": recompiled,
            "unchanged": unchanged,
        }
        self.revisions.append(record)
        self.renders = now
        self._record_version()
        return record

    def run(self, sequence) -> None:
        for step in sequence:
            action = step["action"]
            if action == "adopt":
                self.adopt()
            elif action == "gap":
                self.gap(step["input"])
            elif action == "revise":
                try:
                    record = self.revise(step["input"])
                except Refusal as refusal:
                    self.refusals.append(
                        {
                            "round": step["round"],
                            "revision_id": step.get("revision_id")
                            or json.loads((self.package / step["input"]).read_text("utf-8"))[
                                "revision_id"
                            ],
                            "code": refusal.code,
                            "exit": refusal.exit_code,
                            "message": refusal.message,
                        }
                    )
                    continue
                self.history[-1]["revision"] = record["revision_id"]
            else:
                raise Refusal("replay-action", 2, f"未知步骤动作 {action}")


def replay_package(package: Path) -> Replay:
    cases = strict_load(
        read_bytes(package / "cases.json"),
        ("schema_version", "producer", "case_id", "policy", "description", "sequence", "expected", "boundaries"),
        "cases",
        canonical_required=False,
    )
    replay = Replay(package)
    replay.run(cases["sequence"])
    return replay


# --------------------------------------------------------------------------- #
# Checks against the real root.
# --------------------------------------------------------------------------- #


def check_layout(root: Path, report: Report) -> None:
    top = {entry.name for entry in root.iterdir()}
    if top != {CONTROL, RENDER}:
        report.fail("layout-root", f"顶层只允许 {CONTROL}/ 与 {RENDER}/，实际 {sorted(top)}")
    else:
        report.ok("layout-root")
    control = root / CONTROL
    if not control.is_dir():
        report.fail("layout-control-dir", f"{CONTROL}/ 必须是目录")
        return
    names = {entry.name for entry in control.iterdir()}
    if names != {OWNER, DATABASE, LOCK}:
        report.fail("layout-control", f"{CONTROL}/ 只允许三个文件，实际 {sorted(names)}")
    else:
        report.ok("layout-control")
    for path in sorted(control.iterdir()):
        if path.is_symlink() or not path.is_file():
            report.fail("layout-control-ordinary", f"{path.name} 必须是普通文件")
            return
    report.ok("layout-control-ordinary")
    render_dir = root / RENDER
    if not render_dir.is_dir():
        report.fail("layout-render-dir", f"{RENDER}/ 必须是目录")
        return
    for path in sorted(render_dir.iterdir()):
        if path.is_symlink() or not path.is_file():
            report.fail("layout-render-ordinary", f"{path.name} 必须是普通文件")
            return
    report.ok("layout-render-ordinary")


def check_owner(root: Path, package: Path, report: Report) -> dict | None:
    path = root / CONTROL / OWNER
    try:
        owner = strict_load(read_bytes(path), OWNER_FIELDS, "owner")
    except (Refusal, OSError) as error:
        report.fail("owner-strict", str(error))
        return None
    report.ok("owner-strict")
    checks = {
        "owner-policy": owner["policy"] == POLICY,
        "owner-producer": owner["producer"] == PRODUCER,
        "owner-schema": owner["schema_version"] == SCHEMA_VERSION,
        "owner-root": owner["root"] == os.path.realpath(root),
    }
    for name, ok in checks.items():
        if ok:
            report.ok(name)
        else:
            report.fail(name, f"{name} 不符：{owner}")
    author_bytes = read_bytes(package / "author.json")
    if owner["author_manifest_sha256"] == sha256(author_bytes):
        report.ok("owner-author-sha256")
    else:
        report.fail("owner-author-sha256", "作者清单摘要不是 author.json 的实际 SHA256")
    manifest_bytes = read_bytes(package / "corpus-manifest.json")
    if owner["corpus_manifest_sha256"] == sha256(manifest_bytes):
        report.ok("owner-corpus-sha256")
    else:
        report.fail("owner-corpus-sha256", "语料清单摘要不是 corpus-manifest.json 的实际 SHA256")
    return owner


def open_database(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def check_database(root: Path, report: Report) -> sqlite3.Connection | None:
    path = root / CONTROL / DATABASE
    try:
        connection = open_database(path)
    except sqlite3.Error as error:
        report.fail("db-open", f"无法只读打开账本：{error}")
        return None
    try:
        application_id = connection.execute("PRAGMA application_id").fetchone()[0]
        user_version = connection.execute("PRAGMA user_version").fetchone()[0]
        journal_mode = connection.execute("PRAGMA journal_mode").fetchone()[0]
        if application_id == APPLICATION_ID:
            report.ok("db-application-id")
        else:
            report.fail("db-application-id", f"application_id 应为 {APPLICATION_ID}，实际 {application_id}")
        if user_version == SCHEMA_VERSION:
            report.ok("db-user-version")
        else:
            report.fail("db-user-version", f"user_version 应为 {SCHEMA_VERSION}，实际 {user_version}")
        if str(journal_mode).lower() == "delete":
            report.ok("db-journal-mode")
        else:
            report.fail("db-journal-mode", f"journal_mode 应为 delete，实际 {journal_mode}")
        rows = connection.execute(
            "SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
        ).fetchall()
        tables = {name: " ".join((sql or "").split()) for name, sql in rows}
        if set(tables) == set(REQUIRED_TABLES):
            report.ok("db-table-set")
        else:
            report.fail("db-table-set", f"表集合不符：{sorted(tables)}")
        for name, definition in REQUIRED_TABLES.items():
            actual = tables.get(name)
            if actual == " ".join(definition.split()):
                report.ok(f"db-table-{name}")
            else:
                report.fail(f"db-table-{name}", f"{name} 表定义不是冻结的那一份：{actual}")
        return connection
    except sqlite3.Error as error:
        report.fail("db-read", f"读取账本失败：{error}")
        connection.close()
        return None


def read_metadata(connection: sqlite3.Connection, report: Report) -> dict | None:
    rows = connection.execute("SELECT singleton, schema_version, metadata_json FROM metadata").fetchall()
    if len(rows) != 1:
        report.fail("db-metadata-rows", f"metadata 必须恰好一行，实际 {len(rows)}")
        return None
    singleton, schema_version, payload = rows[0]
    if singleton != 1 or schema_version != SCHEMA_VERSION:
        report.fail("db-metadata-columns", "metadata 的 singleton/schema_version 不符")
        return None
    report.ok("db-metadata-rows")
    try:
        metadata = strict_load(payload.encode("utf-8"), METADATA_FIELDS, "metadata")
    except Refusal as error:
        report.fail("db-metadata-strict", str(error))
        return None
    report.ok("db-metadata-strict")
    return metadata


def read_gaps(connection: sqlite3.Connection, report: Report) -> list[dict] | None:
    rows = connection.execute(
        "SELECT gap_id, position, status, payload_json FROM gaps ORDER BY position"
    ).fetchall()
    gaps = []
    for index, (gap_id, position, status, payload) in enumerate(rows):
        if position != index:
            report.fail("db-gaps-position", f"缺口位置不连续：{gap_id}")
            return None
        try:
            record = strict_load(payload.encode("utf-8"), GAP_RECORD_FIELDS, f"gap-record:{gap_id}")
        except Refusal as error:
            report.fail("db-gaps-strict", str(error))
            return None
        if record["gap_id"] != gap_id or record["status"] != status:
            report.fail("db-gaps-columns", f"缺口 {gap_id} 的列与记录不一致")
            return None
        gaps.append(record)
    report.ok("db-gaps-position")
    report.ok("db-gaps-strict")
    return gaps


def read_revisions(connection: sqlite3.Connection, report: Report) -> list[dict] | None:
    rows = connection.execute(
        "SELECT revision_id, sequence, gap_id, payload_json FROM revisions ORDER BY sequence"
    ).fetchall()
    revisions = []
    for index, (revision_id, sequence, gap_id, payload) in enumerate(rows):
        if sequence != index + 1:
            report.fail("db-revisions-sequence", f"修订序号不连续：{revision_id}")
            return None
        try:
            record = strict_load(
                payload.encode("utf-8"), REVISION_RECORD_FIELDS, f"revision-record:{revision_id}"
            )
        except Refusal as error:
            report.fail("db-revisions-strict", str(error))
            return None
        if record["revision_id"] != revision_id or record["gap_id"] != gap_id:
            report.fail("db-revisions-columns", f"修订 {revision_id} 的列与记录不一致")
            return None
        revisions.append(record)
    report.ok("db-revisions-sequence")
    report.ok("db-revisions-strict")
    return revisions


def read_representations(connection: sqlite3.Connection, report: Report) -> dict | None:
    rows = connection.execute(
        "SELECT representation_id, position, atom_ids_json, render_sha256, render "
        "FROM representations ORDER BY position"
    ).fetchall()
    result = {}
    for index, (rid, position, atom_ids_json, digest, text) in enumerate(rows):
        if position != index:
            report.fail("db-representations-position", f"表示位置不连续：{rid}")
            return None
        atom_ids = json.loads(atom_ids_json)
        if not isinstance(atom_ids, list):
            report.fail("db-representations-atoms", f"表示 {rid} 的原子列表不是数组")
            return None
        if sha256(text.encode("utf-8")) != digest:
            report.fail("db-representations-digest", f"表示 {rid} 的渲染字节与摘要不符")
            return None
        result[rid] = {"atom_ids": atom_ids, "render_sha256": digest, "render": text}
    report.ok("db-representations-position")
    report.ok("db-representations-digest")
    return result


def read_versions(connection: sqlite3.Connection, report: Report) -> list[dict] | None:
    rows = connection.execute(
        "SELECT version, atoms_sha256, payload_json, at FROM ir_versions ORDER BY version"
    ).fetchall()
    versions = []
    for index, (version, digest, payload, at) in enumerate(rows):
        if version != index + 1:
            report.fail("db-ir-sequence", f"IR 版本不连续：v{version}")
            return None
        try:
            record = strict_load(payload.encode("utf-8"), IR_VERSION_FIELDS, f"ir-version:{version}")
        except Refusal as error:
            report.fail("db-ir-strict", str(error))
            return None
        if record["version"] != version or record["atoms_sha256"] != digest or record["at"] != at:
            report.fail("db-ir-columns", f"IR v{version} 的列与记录不一致")
            return None
        atoms = record["atoms"]
        if sha256(canonical(atoms).encode("utf-8")) != digest:
            report.fail("db-ir-atoms-sha256", f"IR v{version} 的原子摘要不匹配记录内容")
            return None
        versions.append(record)
    report.ok("db-ir-sequence")
    report.ok("db-ir-strict")
    report.ok("db-ir-atoms-sha256")
    return versions


def check_corpus(package: Path, metadata: dict, report: Report) -> None:
    corpus = metadata["corpus"]
    if corpus["policy"] != CORPUS_POLICY or corpus["schema_version"] != SCHEMA_VERSION:
        report.fail("metadata-corpus-policy", "语料清单 policy/schema_version 不符")
        return
    report.ok("metadata-corpus-policy")
    root = package / metadata["author"]["corpus"]["root"]
    for document in corpus["documents"]:
        path = root / document["path"]
        try:
            data = read_bytes(path)
        except OSError:
            report.fail("metadata-corpus-bytes", f"冻结语料缺少 {document['path']}")
            return
        if sha256(data) != document["sha256"] or len(data) != document["byte_length"]:
            report.fail("metadata-corpus-bytes", f"冻结语料 {document['path']} 的字节与清单不符")
            return
    report.ok("metadata-corpus-bytes")


def check_replay(
    root: Path,
    package: Path,
    report: Report,
    versions: list[dict],
    revisions: list[dict],
    gaps: list[dict],
    renders: dict,
) -> None:
    cases = strict_load(
        read_bytes(package / "cases.json"),
        ("schema_version", "producer", "case_id", "policy", "description", "sequence", "expected", "boundaries"),
        "cases",
        canonical_required=False,
    )
    replay = Replay(package)
    replay.run(cases["sequence"])
    expected = cases["expected"]

    if len(versions) != len(replay.versions):
        report.fail(
            "replay-ir-count",
            f"IR 版本数不符：账本 {len(versions)}，独立重放 {len(replay.versions)}",
        )
    else:
        report.ok("replay-ir-count")
    if [item["version"] for item in versions] == [item["version"] for item in replay.versions]:
        report.ok("replay-ir-numbers")
    else:
        report.fail("replay-ir-numbers", "IR 版本编号序列不符")

    per_revision = [
        {
            "revision_id": item["revision_id"],
            "sequence": index + 1,
            "gap_id": item["gap_id"],
            "ir_version": item["ir_version"],
            "recompiled": item["recompiled"],
            "unchanged": item["unchanged"],
            "evidence": len(item["evidence"]),
        }
        for index, item in enumerate(revisions)
    ]
    if per_revision == expected["revisions"]:
        report.ok("replay-revisions")
    else:
        report.fail("replay-revisions", f"修订记录不符：{per_revision}")

    observed_gaps = [
        {
            "gap_id": item["gap_id"],
            "status": item["status"],
            "severity": item["severity"],
            "revisions": item["revisions"],
        }
        for item in gaps
    ]
    if observed_gaps == expected["gaps"]:
        report.ok("replay-gaps")
    else:
        report.fail("replay-gaps", f"缺口状态不符：{observed_gaps}")

    live = atoms_map(versions[-1]["atoms"])
    counts = {VERIFIED: 0, PENDING: 0}
    for atom in live.values():
        counts[verification(atom)] += 1
    if counts == expected["verification"]:
        report.ok("replay-verification")
    else:
        report.fail("replay-verification", f"核验计数不符：{counts}")

    expected_renders = {
        representation["representation_id"]: render(representation, live)
        for representation in replay.representations
    }
    mismatched = []
    for rid, text in expected_renders.items():
        ledger = renders.get(rid)
        if ledger is None or ledger["render"] != text:
            mismatched.append(rid)
            continue
        path = root / RENDER / f"{rid}.md"
        try:
            on_disk = read_bytes(path)
        except OSError:
            mismatched.append(f"{rid}(missing)")
            continue
        if on_disk != text.encode("utf-8"):
            mismatched.append(f"{rid}(disk)")
    if mismatched:
        report.fail("replay-renders", f"渲染字节与独立复算不一致：{mismatched}")
    else:
        report.ok("replay-renders")

    # Unaffected representations must keep their bytes across each revision.
    drift = []
    for index, record in enumerate(revisions):
        before = replay.history[index]["renders"]
        after = replay.history[index + 1]["renders"]
        for rid in record["unchanged"]:
            if before.get(rid) != after.get(rid):
                drift.append(f"{record['revision_id']}:{rid}")
        for rid in record["recompiled"]:
            if rid not in before:
                drift.append(f"{record['revision_id']}:{rid}(unknown)")
    if drift:
        report.fail("replay-unchanged-bytes", f"未受影响表示的字节发生了变化：{drift}")
    else:
        report.ok("replay-unchanged-bytes")

    observed_refusals = [
        {
            "round": item["round"],
            "exit": item["exit"],
            "code": item["code"],
            "revision_id": item["revision_id"],
        }
        for item in replay.refusals
    ]
    if observed_refusals == expected["refusals"]:
        report.ok("replay-refusals")
    else:
        report.fail("replay-refusals", f"拒绝序列不符：{observed_refusals}")

    if replay.versions == []:
        report.fail("replay-versions-empty", "独立重放没有产生任何 IR 版本")
    else:
        report.ok("replay-versions-empty")


def check_pending_marking(report: Report, versions: list[dict], renders: dict) -> None:
    if not versions or not renders:
        return
    live = atoms_map(versions[-1]["atoms"])
    pending = [atom for atom in live.values() if verification(atom) == PENDING]
    if not pending:
        report.warn("pending-marking", "最新 IR 里没有待核验原子；本包预期至少一个")
        return
    hit = False
    for record in renders.values():
        text = record["render"]
        if "【待核验】" in text and "pending_verification" in text:
            hit = True
    if hit:
        report.ok("pending-marking")
    else:
        report.fail("pending-marking", "无来源补充没有在渲染里标为【待核验】")


def check_no_network(package: Path, report: Report) -> None:
    cases = strict_load(
        read_bytes(package / "cases.json"),
        ("schema_version", "producer", "case_id", "policy", "description", "sequence", "expected", "boundaries"),
        "cases",
        canonical_required=False,
    )
    boundaries = cases["boundaries"]
    if boundaries["uses_network"] or boundaries["uses_model"]:
        report.fail("no-network-declared", "冻结包声明了网络或模型调用")
    else:
        report.ok("no-network-declared")
    if boundaries["treats_research_candidate_as_gap"]:
        report.fail("gap-candidate-separation", "冻结包把研究候选当成缺口")
    else:
        report.ok("gap-candidate-separation")


def check_frozen(package: Path, report: Report) -> None:
    path = package / "frozen-sha256.json"
    if not path.exists():
        report.fail("frozen-missing", "缺少 frozen-sha256.json；先运行 --freeze 生成")
        return
    manifest = strict_load(
        read_bytes(path),
        ("schema_version", "producer", "workflow_id", "case_count", "files"),
        "frozen",
        canonical_required=False,
    )
    actual = set()
    for current, dirs, files in os.walk(package):
        dirs[:] = sorted(item for item in dirs if not item.startswith("."))
        for name in sorted(files):
            full = Path(current) / name
            relative = full.relative_to(package).as_posix()
            if relative == "frozen-sha256.json":
                continue
            actual.add(relative)
    declared = set(manifest["files"])
    if actual == declared:
        report.ok("frozen-file-set")
    else:
        report.fail(
            "frozen-file-set",
            f"冻结清单与实际成员不符：多出 {sorted(actual - declared)}，缺少 {sorted(declared - actual)}",
        )
    bad = []
    for relative, record in manifest["files"].items():
        try:
            data = read_bytes(package / relative)
        except OSError:
            bad.append(f"{relative}(missing)")
            continue
        if sha256(data) != record["sha256"] or len(data) != record["byte_length"]:
            bad.append(relative)
    if bad:
        report.fail("frozen-bytes", f"冻结成员字节被改动：{bad}")
    else:
        report.ok("frozen-bytes")
    if manifest["case_count"] == len(
        [step for step in json.loads((package / "cases.json").read_text("utf-8"))["sequence"]]
    ):
        report.ok("frozen-case-count")
    else:
        report.fail("frozen-case-count", "case_count 与 cases.json 的步骤数不符")


def freeze(package: Path) -> None:
    files = {}
    for current, dirs, names in os.walk(package):
        dirs[:] = sorted(item for item in dirs if not item.startswith("."))
        for name in sorted(names):
            full = Path(current) / name
            relative = full.relative_to(package).as_posix()
            if relative == "frozen-sha256.json":
                continue
            data = full.read_bytes()
            files[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    cases = json.loads((package / "cases.json").read_text("utf-8"))
    manifest = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": "revision-feedback-v1",
        "case_count": len(cases["sequence"]),
        "files": files,
    }
    (package / "frozen-sha256.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"已冻结 {len(files)} 个成员（case_count={manifest['case_count']}）")


def audit(root: Path, package: Path) -> Report:
    report = Report()
    if not root.exists():
        report.fail("root-exists", f"{root} 不存在")
        return report
    check_layout(root, report)
    owner = check_owner(root, package, report)
    connection = check_database(root, report)
    if connection is None:
        return report
    try:
        metadata = read_metadata(connection, report)
        if metadata is not None:
            if owner is not None:
                consistent = (
                    metadata["case_id"] == owner["case_id"]
                    and metadata["created_at"] == owner["created_at"]
                    and metadata["author_manifest_sha256"] == owner["author_manifest_sha256"]
                    and metadata["corpus_manifest_sha256"] == owner["corpus_manifest_sha256"]
                )
                if consistent:
                    report.ok("db-metadata-owner")
                else:
                    report.fail("db-metadata-owner", "metadata 与 owner.json 不一致")
            check_corpus(package, metadata, report)
        versions = read_versions(connection, report)
        revisions = read_revisions(connection, report)
        gaps = read_gaps(connection, report)
        renders = read_representations(connection, report)
        check_no_network(package, report)
        if versions and revisions and gaps and renders:
            check_replay(root, package, report, versions, revisions, gaps, renders)
            check_pending_marking(report, versions, renders)
        check_frozen(package, report)
    finally:
        connection.close()
    return report


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    package = Path("fixtures/revision-feedback-v1")
    as_json = False
    do_freeze = False
    positionals: list[str] = []
    index = 0
    while index < len(argv):
        argument = argv[index]
        if argument == "--package":
            package = Path(argv[index + 1])
            index += 2
        elif argument == "--json":
            as_json = True
            index += 1
        elif argument == "--freeze":
            do_freeze = True
            index += 1
        elif argument.startswith("-"):
            print(f"未知参数：{argument}", file=sys.stderr)
            return 2
        else:
            positionals.append(argument)
            index += 1
    if not package.is_dir():
        print(f"作者包不存在：{package}", file=sys.stderr)
        return 2
    if do_freeze:
        # 冻结只需要作者包：冻结包本身不依赖任何账本根。
        freeze(package)
        return 0
    if len(positionals) != 1:
        print("需要一个修订根：verify_revision_feedback.py <修订根> [--package P] [--json]", file=sys.stderr)
        return 2
    root = Path(positionals[0])
    report = audit(root, package)
    if as_json:
        print(json.dumps(report.dump(), ensure_ascii=False, indent=2))
    else:
        for item in report.items:
            if not item["ok"]:
                print(f"[{'提示' if item['warn'] else '失败'}] {item['name']}：{item['detail']}")
        print(
            f"合计 {len(report.items)} 项检查：失败 {len(report.failures)}、提示 {len(report.warnings)}"
        )
    return 1 if report.failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
