#!/usr/bin/env python3
"""Read-only auditor for the audit-journal root and its output.

This program never imports the Rust product and never runs the CLI to decide
anything: it re-derives the journal from the frozen declaration on its own, then
compares that derivation with what is on disk.

Usage: <root> [--output DIR] [--package DIR] [--json]
       --package DIR [--json] [--freeze]

Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "audit-journal-v1"
SCHEMA_VERSION = 1
JOURNAL_VERSION = 1
APP_PRODUCER = "noetome-journal"
OUTPUT_PRODUCER = "audit_journal_output"
AUTHOR_PRODUCER = "fixture_author"

MAX_TASKS = 16
MAX_EVENTS = 256
MAX_SOURCES = 16
MAX_OBJECTS = 32
MAX_FILES = 64

EVENT_STARTED = "started"
EVENT_FAILED = "failed"
EVENT_RETRIED = "retried"
EVENT_RESOLVED = "resolved"
EVENT_KINDS = (EVENT_STARTED, EVENT_FAILED, EVENT_RETRIED, EVENT_RESOLVED)
SEVERITY_INFO = "info"
SEVERITY_ERROR = "error"

SECRET_MARKERS = (
    "api_key",
    "apikey",
    "password",
    "secret",
    "bearer ",
    "authorization:",
    "token=",
    "-----begin",
)

CONTROL = "control"
MANIFEST = "manifest.json"
INPUTS = "inputs.json"
PROJECT = "project.json"
JOURNAL = "journal.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = (CONTROL,)
CONTROL_ENTRIES = tuple(sorted((INPUTS, MANIFEST, PROJECT)))
OUTPUT_ENTRIES = tuple(sorted((JOURNAL, MANIFEST, MARKDOWN, REPORT)))

PROJECT_FIELDS = {
    "policy", "schema_version", "producer", "case_id", "description",
    "sources", "objects", "files", "tasks", "events", "limits",
}
SOURCE_FIELDS = {"source_id", "locator", "description"}
OBJECT_FIELDS = {"object_id", "source_id", "label"}
FILE_FIELDS = {"file_id", "object_id", "path"}
TASK_FIELDS = {"task_id", "title", "sources", "objects", "files"}
EVENT_FIELDS = {"event_id", "task_id", "at", "kind", "class", "action", "message"}
LIMIT_FIELDS = {"max_tasks", "max_events", "max_sources", "max_objects", "max_files"}

MANIFEST_FIELDS = {
    "policy", "producer", "schema_version", "case_id", "root",
    "root_identity", "parent_identity", "control_identity",
    "manifest_identity", "inputs_identity",
    "project_sha256", "inputs_sha256", "created_at",
}
INPUTS_FIELDS = {
    "policy", "schema_version", "case_id", "project_sha256",
    "tasks", "events", "sources", "objects", "files",
}
OUTPUT_MANIFEST_FIELDS = {
    "policy", "producer", "schema_version", "case_id", "source_root",
    "source_inputs_sha256", "journal_sha256", "report_sha256",
    "markdown_sha256", "counts",
}
JOURNAL_FIELDS = {"policy", "journal_version", "case_id", "tasks", "events", "failures"}
REPORT_FIELDS = {
    "policy", "schema_version", "case_id", "description", "counts",
    "model_calls", "network_requests",
}
COUNT_FIELDS = {
    "tasks", "events", "started", "failed", "retried", "resolved",
    "sources", "objects", "files", "located_failures",
    "unresolved_failures", "model_calls", "network_requests",
}
TASK_RECORD_FIELDS = {
    "task_id", "title", "sources", "objects", "files",
    "events", "failures", "outcome",
}
EVENT_RECORD_FIELDS = {
    "event_id", "task_id", "at", "kind", "severity",
    "class", "action", "message",
}
FAILURE_RECORD_FIELDS = {
    "event_id", "task_id", "task_title", "at", "class", "action", "message",
    "sources", "objects", "files", "followed_by_action",
}
CASES_FIELDS = {
    "schema_version", "producer", "case_id", "policy",
    "sequence", "variants", "expected", "boundaries",
}
EXPECTED_FIELDS = {
    "policy", "journal_version", "case_id", "counts", "event_order",
    "tasks", "failures", "severity", "rejections", "determinism",
    "boundaries", "derivation",
}


class Report:
    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks.append((name, bool(ok), detail))
        return bool(ok)

    @property
    def failures(self) -> list[tuple[str, bool, str]]:
        return [check for check in self.checks if not check[1]]

    def dump(self, as_json: bool) -> None:
        if as_json:
            payload = {
                "checks": [
                    {"name": name, "ok": ok, "detail": detail}
                    for name, ok, detail in self.checks
                ],
                "total": len(self.checks),
                "failed": len(self.failures),
            }
            print(json.dumps(payload, ensure_ascii=False, indent=2))
            return
        for name, ok, detail in self.checks:
            if not ok:
                print(f"[失败] {name}：{detail}")
        print(f"合计 {len(self.checks)} 项检查：失败 {len(self.failures)}、提示 0")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


class StrictError(ValueError):
    pass


def strict_load(path: Path, fields: set[str], label: str, canonical_required: bool = True):
    try:
        raw = read_bytes(path)
    except OSError as error:
        raise StrictError(f"{label} 无法读取：{error}") from error
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise StrictError(f"{label} 不是合法 UTF-8：{error}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise StrictError(f"{label} 不是合法 JSON：{error}") from error
    if not isinstance(value, dict):
        raise StrictError(f"{label} 必须是 JSON 对象。")
    keys = set(value)
    if keys != fields:
        missing = sorted(fields - keys)
        extra = sorted(keys - fields)
        raise StrictError(f"{label} 字段不符：缺少 {missing}，多出 {extra}。")
    if canonical_required:
        expected = canonical(value)
        if text not in (expected, expected + "\n"):
            raise StrictError(f"{label} 不是规范编码。")
    return value


def identifier(value: str, label: str) -> bool:
    return bool(value) and len(value) <= 48 and all(
        character.isascii() and (character.isalnum() or character in "-_") for character in value
    )


def relative(value: str) -> bool:
    if not value or value.startswith("/"):
        return False
    if len(value) > 1 and value[1] == ":":
        return False
    parts = value.split("/")
    return all(part not in ("", ".", "..") for part in parts)


def secret_hits(values) -> list[str]:
    hits: list[str] = []
    for label, text in values:
        lowered = text.lower()
        for marker in SECRET_MARKERS:
            if marker in lowered:
                hits.append(f"{label} 里出现疑似凭据字样「{marker}」")
                break
    return hits


def strings_of(declaration: dict) -> list[tuple[str, str]]:
    values: list[tuple[str, str]] = []
    if declaration.get("description"):
        values.append(("工程描述", declaration["description"]))
    for index, source in enumerate(declaration.get("sources") or []):
        values.append((f"来源描述（{source.get('source_id', index)}）", source.get("description", "")))
        values.append((f"来源定位（{source.get('source_id', index)}）", source.get("locator", "")))
    for index, item in enumerate(declaration.get("objects") or []):
        values.append((f"知识对象（{item.get('object_id', index)}）", item.get("label", "")))
    for index, item in enumerate(declaration.get("files") or []):
        values.append((f"受影响文件（{item.get('file_id', index)}）", item.get("path", "")))
    for index, item in enumerate(declaration.get("tasks") or []):
        values.append((f"任务标题（{item.get('task_id', index)}）", item.get("title", "")))
    for index, item in enumerate(declaration.get("events") or []):
        values.append((f"事件动作（{item.get('event_id', index)}）", item.get("action", "")))
        values.append((f"事件说明（{item.get('event_id', index)}）", item.get("message", "")))
    return values


class JournalError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def validate(declaration: dict) -> dict:
    """Re-derive the validation order of the product and return the first refusal."""
    if declaration.get("policy") != POLICY:
        raise JournalError("JOURNAL_ARGUMENT", f"policy 必须是 {POLICY}。")
    if declaration.get("schema_version") != SCHEMA_VERSION:
        raise JournalError("JOURNAL_ARGUMENT", "schema_version 必须是 1。")
    if declaration.get("producer") != AUTHOR_PRODUCER:
        raise JournalError("JOURNAL_ARGUMENT", f"producer 必须是 {AUTHOR_PRODUCER}。")
    if not identifier(declaration.get("case_id", ""), "case_id"):
        raise JournalError("JOURNAL_ARGUMENT", "case_id 不合法。")
    if not declaration.get("description"):
        raise JournalError("JOURNAL_ARGUMENT", "description 不能为空。")

    limits = declaration.get("limits") or {}
    for key, bound in (
        ("max_tasks", MAX_TASKS),
        ("max_events", MAX_EVENTS),
        ("max_sources", MAX_SOURCES),
        ("max_objects", MAX_OBJECTS),
        ("max_files", MAX_FILES),
    ):
        value = limits.get(key)
        if not isinstance(value, int) or not 1 <= value <= bound:
            raise JournalError("JOURNAL_ARGUMENT", f"{key} 必须在 1 到 {bound} 之间。")

    sources = declaration.get("sources") or []
    if not sources:
        raise JournalError("JOURNAL_ARGUMENT", "至少要声明一个来源。")
    seen: set[str] = set()
    for source in sources:
        source_id = source.get("source_id", "")
        if not identifier(source_id, "source_id"):
            raise JournalError("JOURNAL_ARGUMENT", "source_id 不合法。")
        if source_id in seen:
            raise JournalError("JOURNAL_ARGUMENT", f"来源 {source_id} 重复。")
        seen.add(source_id)
        if not relative(source.get("locator", "")):
            raise JournalError("JOURNAL_ARGUMENT", f"来源 {source_id} 的 locator 必须是相对路径。")

    objects = declaration.get("objects") or []
    if len(objects) > MAX_OBJECTS:
        raise JournalError("JOURNAL_ARGUMENT", "知识对象超过上限。")
    source_ids = {source["source_id"] for source in sources}
    object_ids: set[str] = set()
    for item in objects:
        object_id = item.get("object_id", "")
        if not identifier(object_id, "object_id"):
            raise JournalError("JOURNAL_ARGUMENT", "object_id 不合法。")
        if object_id in object_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"知识对象 {object_id} 重复。")
        object_ids.add(object_id)
        if item.get("source_id") not in source_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"知识对象 {object_id} 的来源未声明。")

    files = declaration.get("files") or []
    if len(files) > MAX_FILES:
        raise JournalError("JOURNAL_ARGUMENT", "受影响文件超过上限。")
    file_ids: set[str] = set()
    for item in files:
        file_id = item.get("file_id", "")
        if not identifier(file_id, "file_id"):
            raise JournalError("JOURNAL_ARGUMENT", "file_id 不合法。")
        if file_id in file_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"文件 {file_id} 重复。")
        file_ids.add(file_id)
        if item.get("object_id") not in object_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"文件 {file_id} 的知识对象未声明。")

    tasks = declaration.get("tasks") or []
    if not tasks or len(tasks) > MAX_TASKS:
        raise JournalError("JOURNAL_ARGUMENT", "任务数量必须在上限之内且至少一个。")
    task_ids: set[str] = set()
    for task in tasks:
        task_id = task.get("task_id", "")
        if not identifier(task_id, "task_id"):
            raise JournalError("JOURNAL_ARGUMENT", "task_id 不合法。")
        if task_id in task_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 重复。")
        task_ids.add(task_id)
        if not task.get("sources"):
            raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 没有声明任何来源范围。")
        if not task.get("objects"):
            raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 没有声明任何知识对象。")
        for value in task["sources"]:
            if value not in source_ids:
                raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 引用了未声明的来源 {value}。")
        for value in task["objects"]:
            if value not in object_ids:
                raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 引用了未声明的对象 {value}。")
        for value in task.get("files") or []:
            if value not in file_ids:
                raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 引用了未声明的文件 {value}。")
            owner = next(item["object_id"] for item in files if item["file_id"] == value)
            if owner not in task["objects"]:
                raise JournalError(
                    "JOURNAL_ARGUMENT",
                    f"任务 {task_id} 声明的文件 {value} 不属于它列出的任何知识对象。",
                )

    events = declaration.get("events") or []
    if len(events) > MAX_EVENTS:
        raise JournalError("JOURNAL_ARGUMENT", "事件数量超过上限。")
    event_ids: set[str] = set()
    sequences: dict[str, set[int]] = {}
    for event in events:
        event_id = event.get("event_id", "")
        if not identifier(event_id, "event_id"):
            raise JournalError("JOURNAL_ARGUMENT", "event_id 不合法。")
        if event_id in event_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"事件 {event_id} 重复。")
        event_ids.add(event_id)
        task_id = event.get("task_id", "")
        if task_id not in task_ids:
            raise JournalError("JOURNAL_ARGUMENT", f"事件 {event_id} 的任务未声明。")
        at = event.get("at")
        if not isinstance(at, int) or at < 1:
            raise JournalError("JOURNAL_ARGUMENT", f"事件 {event_id} 的 at 必须 >= 1。")
        marks = sequences.setdefault(task_id, set())
        if at in marks:
            raise JournalError("JOURNAL_ARGUMENT", f"任务 {task_id} 有两个事件都用 at = {at}。")
        marks.add(at)
        if event.get("kind") not in EVENT_KINDS:
            raise JournalError("JOURNAL_ARGUMENT", f"事件 {event_id} 的 kind 不合法。")
        for key in ("class", "action", "message"):
            if not event.get(key):
                raise JournalError("JOURNAL_ARGUMENT", f"事件 {event_id} 的 {key} 不能为空。")

    hits = secret_hits(strings_of(declaration))
    if hits:
        raise JournalError("JOURNAL_SECRET_MARKER", "；".join(hits))
    return {"ok": True}


def severity_of(kind: str) -> str:
    return SEVERITY_ERROR if kind == EVENT_FAILED else SEVERITY_INFO


def replay(declaration: dict) -> dict:
    """Independently derive the journal the way the product documents it."""
    validate(declaration)
    tasks_by_id = {task["task_id"]: task for task in declaration["tasks"]}

    ordered_events = sorted(
        declaration["events"],
        key=lambda event: (event["task_id"], event["at"], event["event_id"]),
    )
    events = [
        {
            "event_id": event["event_id"],
            "task_id": event["task_id"],
            "at": event["at"],
            "kind": event["kind"],
            "severity": severity_of(event["kind"]),
            "class": event["class"],
            "action": event["action"],
            "message": event["message"],
        }
        for event in ordered_events
    ]

    tasks: list[dict] = []
    failures: list[dict] = []
    for task_id in sorted(tasks_by_id):
        task = tasks_by_id[task_id]
        mine = [event for event in events if event["task_id"] == task_id]
        pending: list[str] = []
        followed: set[str] = set()
        for event in mine:
            if event["kind"] == EVENT_FAILED:
                pending.append(event["event_id"])
            elif event["kind"] in (EVENT_RETRIED, EVENT_RESOLVED):
                for event_id in pending:
                    followed.add(event_id)
                pending.clear()
        for event in mine:
            if event["kind"] != EVENT_FAILED:
                continue
            failures.append(
                {
                    "event_id": event["event_id"],
                    "task_id": task_id,
                    "task_title": task["title"],
                    "at": event["at"],
                    "class": event["class"],
                    "action": event["action"],
                    "message": event["message"],
                    "sources": list(task["sources"]),
                    "objects": list(task["objects"]),
                    "files": list(task.get("files") or []),
                    "followed_by_action": event["event_id"] in followed,
                }
            )
        tasks.append(
            {
                "task_id": task_id,
                "title": task["title"],
                "sources": list(task["sources"]),
                "objects": list(task["objects"]),
                "files": list(task.get("files") or []),
                "events": len(mine),
                "failures": sum(1 for event in mine if event["kind"] == EVENT_FAILED),
                "outcome": mine[-1]["kind"] if mine else "none",
            }
        )

    counts = {
        "tasks": len(tasks),
        "events": len(events),
        "started": sum(1 for event in events if event["kind"] == EVENT_STARTED),
        "failed": sum(1 for event in events if event["kind"] == EVENT_FAILED),
        "retried": sum(1 for event in events if event["kind"] == EVENT_RETRIED),
        "resolved": sum(1 for event in events if event["kind"] == EVENT_RESOLVED),
        "sources": len(declaration["sources"]),
        "objects": len(declaration["objects"]),
        "files": len(declaration["files"]),
        "located_failures": len(failures),
        "unresolved_failures": sum(1 for item in failures if not item["followed_by_action"]),
        "model_calls": 0,
        "network_requests": 0,
    }
    return {
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "journal": {
            "policy": POLICY,
            "journal_version": JOURNAL_VERSION,
            "case_id": declaration["case_id"],
            "tasks": tasks,
            "events": events,
            "failures": failures,
        },
        "report": {
            "policy": POLICY,
            "schema_version": SCHEMA_VERSION,
            "case_id": declaration["case_id"],
            "description": declaration["description"],
            "counts": counts,
            "model_calls": 0,
            "network_requests": 0,
        },
        "counts": counts,
    }


def render_markdown(replay_value: dict) -> str:
    journal = replay_value["journal"]
    counts = replay_value["counts"]
    out: list[str] = []
    out.append(f"# 审计事件日志（{replay_value['case_id']}）\n\n")
    out.append(f"{replay_value['description']}\n\n")
    out.append("## 事件规模\n\n")
    out.append(
        f"- 任务 {counts['tasks']} 个、事件 {counts['events']} 条："
        f"started {counts['started']} / failed {counts['failed']} / "
        f"retried {counts['retried']} / resolved {counts['resolved']}。\n"
    )
    out.append(
        f"- 来源 {counts['sources']} 个、知识对象 {counts['objects']} 个、"
        f"受影响文件 {counts['files']} 个。\n\n"
    )
    out.append("## 失败定位\n\n")
    out.append("| 事件 | 任务 | 分类 | 来源范围 | 知识对象 | 受影响文件 | 后续动作 |\n")
    out.append("| --- | --- | --- | --- | --- | --- | --- |\n")
    for failure in journal["failures"]:
        follow = "已重试或已解决" if failure["followed_by_action"] else "未处理"
        out.append(
            f"| {failure['event_id']} | {failure['task_id']} | {failure['class']} | "
            f"{', '.join(failure['sources'])} | {', '.join(failure['objects'])} | "
            f"{', '.join(failure['files'])} | {follow} |\n"
        )
    out.append("\n## 任务状态\n\n")
    out.append("| 任务 | 标题 | 事件 | 失败 | 当前结果 |\n")
    out.append("| --- | --- | --- | --- | --- |\n")
    for task in journal["tasks"]:
        out.append(
            f"| {task['task_id']} | {task['title']} | {task['events']} | "
            f"{task['failures']} | {task['outcome']} |\n"
        )
    out.append("\n## 边界\n\n")
    out.append("- 事件只追加，不重写、不重排；同一冻结输入两次运行字节相同。\n")
    out.append(
        "- 声明里出现疑似凭据即整包拒绝：先过滤再继续会得到一份看起来完整、"
        "实际缺关键上下文的日志。\n"
    )
    out.append("- 模型调用与网络请求 0 次；本日志只做定位，不判断内容质量与科学正确性。\n")
    return "".join(out)


def check_layout(report: Report, root: Path | None) -> None:
    if root is None:
        return
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control-present", False, "缺少 control/")
        return
    control_entries = sorted(item.name for item in control.iterdir())
    report.record(
        "layout-control-entries",
        control_entries == list(CONTROL_ENTRIES),
        f"control 条目 {control_entries}",
    )
    links = [str(path) for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-no-symlinks", not links, f"符号链接 {links}")


def check_manifest(report: Report, root: Path) -> dict | None:
    path = root / CONTROL / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 control/manifest.json")
        return None
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "审计根清单")
    except StrictError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, f"policy={manifest['policy']}")
    report.record(
        "manifest-producer", manifest["producer"] == APP_PRODUCER, f"producer={manifest['producer']}"
    )
    report.record(
        "manifest-schema", manifest["schema_version"] == SCHEMA_VERSION,
        f"schema_version={manifest['schema_version']}",
    )
    report.record(
        "manifest-root",
        Path(manifest["root"]).resolve() == root.resolve(),
        f"登记根 {manifest['root']} 与实参 {root}",
    )
    project_path = root / CONTROL / PROJECT
    if project_path.is_file():
        report.record(
            "manifest-project-digest",
            sha256(read_bytes(project_path)) == manifest["project_sha256"],
            "工程声明副本的摘要与清单不符",
        )
    inputs_path = root / CONTROL / INPUTS
    if inputs_path.is_file():
        report.record(
            "manifest-inputs-digest",
            sha256(read_bytes(inputs_path)) == manifest["inputs_sha256"],
            "冻结输入的摘要与清单不符",
        )
    for field in ("root_identity", "parent_identity", "control_identity", "manifest_identity", "inputs_identity"):
        value = manifest.get(field)
        report.record(
            f"manifest-identity:{field}",
            isinstance(value, dict) and "platform" in value,
            f"{field} 必须是结构化身份：{value!r}",
        )
    return manifest


def check_project(report: Report, root: Path) -> dict | None:
    path = root / CONTROL / PROJECT
    if not path.is_file():
        report.record("project-present", False, "缺少 control/project.json")
        return None
    try:
        text = read_bytes(path).decode("utf-8")
        declaration = json.loads(text)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        report.record("project-strict", False, f"工程声明副本不可读：{error}")
        return None
    if not isinstance(declaration, dict):
        report.record("project-strict", False, "工程声明副本必须是 JSON 对象。")
        return None
    # The product freezes the author's own bytes, so re-encoding is not required;
    # what must hold is the field set and the content.
    report.record(
        "project-fields",
        set(declaration) == PROJECT_FIELDS,
        f"字段 {sorted(declaration)}",
    )
    try:
        validate(declaration)
        report.record("project-validates", True)
    except JournalError as error:
        report.record("project-validates", False, f"{error.code}：{error.detail}")
    return declaration


def check_inputs(report: Report, root: Path, declaration: dict | None) -> dict | None:
    path = root / CONTROL / INPUTS
    if not path.is_file():
        report.record("inputs-present", False, "缺少 control/inputs.json")
        return None
    try:
        inputs = strict_load(path, INPUTS_FIELDS, "冻结输入")
    except StrictError as error:
        report.record("inputs-strict", False, str(error))
        return None
    report.record("inputs-strict", True)
    report.record("inputs-policy", inputs["policy"] == POLICY, f"policy={inputs['policy']}")
    report.record(
        "inputs-schema", inputs["schema_version"] == SCHEMA_VERSION,
        f"schema_version={inputs['schema_version']}",
    )
    if declaration is not None:
        report.record("inputs-case", inputs["case_id"] == declaration["case_id"], "case_id 不一致")
        expected = {
            "tasks": len(declaration["tasks"]),
            "events": len(declaration["events"]),
            "sources": len(declaration["sources"]),
            "objects": len(declaration["objects"]),
            "files": len(declaration["files"]),
        }
        for key, value in expected.items():
            report.record(
                f"inputs-count:{key}", inputs[key] == value,
                f"声明 {value}，冻结输入 {inputs[key]}",
            )
    return inputs


def check_frozen(report: Report, package: Path | None) -> None:
    if package is None:
        return
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, f"{package} 缺少 {FROZEN}")
        return
    report.record("frozen-present", True)
    try:
        frozen = json.loads(read_bytes(path).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        report.record("frozen-strict", False, f"冻结清单不可读：{error}")
        return
    report.record("frozen-strict", True, "")
    actual = {}
    for member in sorted(package.rglob("*")):
        if not member.is_file():
            continue
        if member.name == FROZEN:
            continue
        relpath = member.relative_to(package).as_posix()
        actual[relpath] = {"sha256": sha256(read_bytes(member)), "byte_length": member.stat().st_size}
    files = frozen.get("files") or {}
    report.record(
        "frozen-members",
        set(files) == set(actual),
        f"清单 {sorted(files)} 与实际 {sorted(actual)}",
    )
    for name in sorted(set(files) & set(actual)):
        report.record(
            f"frozen-digest:{name}",
            files[name] == actual[name],
            f"清单 {files[name]}，实际 {actual[name]}",
        )


def load_package(package: Path) -> dict:
    declaration = json.loads((package / PROJECT).read_text(encoding="utf-8"))
    return declaration


def replay_package(package: Path) -> dict:
    return replay(load_package(package))


def check_expected(report: Report, package: Path | None, value: dict | None) -> None:
    if package is None:
        return
    path = package / "expected.json"
    if not path.is_file():
        report.record("expected-present", False, "缺少 expected.json")
        return
    try:
        expected = strict_load(path, EXPECTED_FIELDS, "先验期望", canonical_required=False)
    except StrictError as error:
        report.record("expected-strict", False, str(error))
        return
    report.record("expected-strict", True)
    report.record("expected-policy", expected["policy"] == POLICY, f"policy={expected['policy']}")
    report.record(
        "expected-case", expected["case_id"] == value["case_id"] if value else False,
        "先验 case_id 与声明不一致",
    )
    if value is None:
        return
    for key, want in expected["counts"].items():
        report.record(
            f"expected-count:{key}",
            value["counts"][key] == want,
            f"先验 {want}，重放 {value['counts'][key]}",
        )
    report.record(
        "expected-event-order",
        [event["event_id"] for event in value["journal"]["events"]] == expected["event_order"],
        "事件顺序与先验不符",
    )
    task_keys = ("task_id", "title", "sources", "objects", "files", "events", "failures", "outcome")
    report.record(
        "expected-tasks",
        [{key: task[key] for key in task_keys} for task in value["journal"]["tasks"]]
        == expected["tasks"],
        "任务表与先验不符",
    )
    failure_keys = (
        "event_id", "task_id", "at", "class", "sources", "objects", "files", "followed_by_action",
    )
    report.record(
        "expected-failures",
        [{key: item[key] for key in failure_keys} for item in value["journal"]["failures"]]
        == expected["failures"],
        "失败表与先验不符",
    )
    report.record(
        "expected-severity",
        all(event["severity"] == expected["severity"][event["kind"]] for event in value["journal"]["events"]),
        "严重级与先验不符",
    )
    report.record(
        "expected-derivation",
        len(expected["derivation"]) >= 6,
        f"先验推导只有 {len(expected['derivation'])} 条",
    )
    report.record(
        "expected-boundary",
        expected["boundaries"].get("scientific_status") == "not_run",
        "先验边界必须含 scientific_status = not_run",
    )


def check_variants(report: Report, package: Path | None) -> None:
    if package is None:
        return
    cases_path = package / "cases.json"
    if not cases_path.is_file():
        report.record("cases-present", False, "缺少 cases.json")
        return
    try:
        cases = strict_load(cases_path, CASES_FIELDS, "用例清单", canonical_required=False)
    except StrictError as error:
        report.record("cases-strict", False, str(error))
        return
    report.record("cases-strict", True)
    variants = cases.get("variants") or []
    report.record("cases-variants", bool(variants), "没有声明任何变体")
    for variant in variants:
        name = variant.get("project")
        if not name:
            continue
        path = package / name
        if not path.is_file():
            report.record(f"variant-present:{name}", False, f"缺少 {name}")
            continue
        try:
            declaration = json.loads(path.read_text(encoding="utf-8"))
            validate(declaration)
        except JournalError as error:
            want = variant.get("expect") or {}
            report.record(
                f"variant-code:{name}",
                error.code == want.get("code"),
                f"期望 {want.get('code')}，重放 {error.code}",
            )
        except json.JSONDecodeError as error:
            report.record(f"variant-code:{name}", False, f"不是合法 JSON：{error}")
        else:
            report.record(f"variant-code:{name}", False, "变体本该被拒绝，但重放通过了")


def check_output(report: Report, output: Path | None, value: dict | None) -> None:
    if output is None:
        return
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"输出条目 {entries}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    report.record("output-complete", not missing, f"缺少 {missing}")
    if missing:
        return
    for name in (JOURNAL, REPORT):
        path = output / name
        try:
            text = read_bytes(path).decode("utf-8")
            parsed = json.loads(text)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            report.record(f"output-strict:{name}", False, f"{name} 不可读：{error}")
            continue
        report.record(
            f"output-strict:{name}",
            text in (canonical(parsed), canonical(parsed) + "\n"),
            f"{name} 不是规范编码",
        )
    try:
        manifest = strict_load(output / OUTPUT_MANIFEST, OUTPUT_MANIFEST_FIELDS, "输出清单")
    except StrictError as error:
        report.record("output-manifest-strict", False, str(error))
        return
    report.record("output-manifest-strict", True)
    report.record(
        "output-manifest-producer",
        manifest["producer"] == OUTPUT_PRODUCER,
        f"producer={manifest['producer']}",
    )
    report.record(
        "output-manifest-no-timestamp",
        "created_at" not in manifest,
        "输出清单不得带时间戳，否则两次运行不再相同",
    )
    for name, field in (
        (JOURNAL, "journal_sha256"),
        (REPORT, "report_sha256"),
        (MARKDOWN, "markdown_sha256"),
    ):
        report.record(
            f"output-manifest-digest:{name}",
            sha256(read_bytes(output / name)) == manifest[field],
            f"{name} 的摘要与输出清单不符",
        )
    if value is not None:
        report.record(
            "output-counts",
            manifest["counts"] == value["counts"],
            "输出清单的计数与重放不符",
        )
        journal = json.loads((output / JOURNAL).read_text(encoding="utf-8"))
        report.record(
            "output-journal",
            journal == value["journal"],
            "journal.json 与重放不符",
        )
        result = json.loads((output / REPORT).read_text(encoding="utf-8"))
        report.record(
            "output-report",
            result == value["report"],
            "report.json 与重放不符",
        )
        markdown = (output / MARKDOWN).read_text(encoding="utf-8")
        report.record(
            "output-markdown",
            markdown == render_markdown(value),
            "report.md 与重放不符",
        )


def check_package(report: Report, package: Path | None, value: dict | None) -> None:
    if package is None:
        return
    if not package.is_dir():
        report.record("package-present", False, f"{package} 不是目录")
        return
    report.record("package-present", True)
    project_path = package / PROJECT
    if not project_path.is_file():
        report.record("package-project", False, "包缺少 project.json")
        return
    try:
        declaration = json.loads(project_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        report.record("package-project", False, f"project.json 不是合法 JSON：{error}")
        return
    report.record("package-project", True)
    report.record(
        "package-policy",
        declaration.get("policy") == POLICY,
        f"policy={declaration.get('policy')}",
    )
    report.record(
        "package-producer",
        declaration.get("producer") == AUTHOR_PRODUCER,
        f"producer={declaration.get('producer')}",
    )
    try:
        replayed = replay(declaration)
        report.record("package-validates", True)
    except JournalError as error:
        report.record("package-validates", False, f"{error.code}：{error.detail}")
        return
    if value is not None:
        report.record(
            "package-replay",
            replayed["journal"] == value["journal"],
            "包内声明的重放与冻结根的重放不一致",
        )


def freeze(package: Path) -> int:
    if not package.is_dir():
        print(f"{package} 不是目录。", file=sys.stderr)
        return 2
    files = {}
    for member in sorted(package.rglob("*"), key=lambda path: path.relative_to(package).parts):
        if not member.is_file() or member.name == FROZEN:
            continue
        relpath = member.relative_to(package).as_posix()
        payload = read_bytes(member)
        files[relpath] = {"sha256": sha256(payload), "byte_length": len(payload)}
    value = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="审计事件日志的独立只读审计器")
    parser.add_argument("root", nargs="?", help="已建立的审计根")
    parser.add_argument("--output", help="journal build 的输出根")
    parser.add_argument("--package", help="冻结夹具目录")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出检查结果")
    parser.add_argument("--freeze", action="store_true", help="重写夹具的 frozen-sha256.json")
    args = parser.parse_args(argv)

    package = Path(args.package).resolve() if args.package else None
    if args.freeze:
        if package is None:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        return freeze(package)

    root = Path(args.root).resolve() if args.root else None
    output = Path(args.output).resolve() if args.output else None
    if root is None and package is None:
        print("至少要给一个审计根或 --package。", file=sys.stderr)
        return 2
    if root is not None and not root.is_dir():
        print(f"{root} 不是目录。", file=sys.stderr)
        return 2
    if output is not None and not output.is_dir():
        print(f"{output} 不是目录。", file=sys.stderr)
        return 2

    report = Report()
    check_layout(report, root)
    declaration = None
    manifest = None
    if root is not None:
        manifest = check_manifest(report, root)
        declaration = check_project(report, root)
        if declaration is not None:
            check_inputs(report, root, declaration)

    value = None
    if declaration is not None:
        try:
            value = replay(declaration)
            report.record("replay", True)
        except JournalError as error:
            report.record("replay", False, f"{error.code}：{error.detail}")

    check_package(report, package, value)
    # In package-only mode there is no root to replay, but the package's own
    # declaration is just as good a subject for the prior comparison.
    if value is None and package is not None and (package / PROJECT).is_file():
        try:
            value = replay_package(package)
            report.record("package-replay", True)
        except JournalError as error:
            report.record("package-replay", False, f"{error.code}：{error.detail}")
    check_expected(report, package, value)
    check_variants(report, package)
    check_frozen(report, package)
    check_output(report, output, value)

    if manifest is not None and value is not None:
        report.record(
            "manifest-case",
            manifest["case_id"] == value["case_id"],
            "清单登记的 case_id 与重放不符",
        )

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
