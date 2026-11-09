#!/usr/bin/env python3
"""Read-only auditor for a retention root and the cleanup plan it implies.

This program never imports the Rust product and never runs the CLI to decide
anything: it re-derives the cleanup plan from the frozen declaration on its own,
then compares that derivation with what is on disk.

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

POLICY = "retention-policy-v1"
SCHEMA_VERSION = 1
PLAN_VERSION = 1
APP_PRODUCER = "noetome-retention"
OUTPUT_PRODUCER = "retention_plan_output"
AUTHOR_PRODUCER = "fixture_author"

MAX_CATEGORIES = 8
MAX_ENTRIES = 64
MAX_PUBLISHED = 8
MAX_REQUIRED = 32
MAX_ID = 48
MAX_TEXT = 300
MAX_LABEL = 120
MAX_DAYS = 3650

REASON_EXPIRED = "超过保留期"
REASON_WITHIN_POLICY = "未超过保留期"
REASON_PROTECTED = "已发布版本仍需引用"
ACTION_DELETE = "delete"
ACTION_KEEP = "keep"

CONTROL = "control"
MANIFEST = "manifest.json"
INPUTS = "inputs.json"
PROJECT = "project.json"
PLAN = "plan.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = (CONTROL,)
CONTROL_ENTRIES = tuple(sorted((INPUTS, MANIFEST, PROJECT)))
OUTPUT_ENTRIES = tuple(sorted((MANIFEST, MARKDOWN, PLAN, REPORT)))

PROJECT_FIELDS = {
    "policy",
    "schema_version",
    "producer",
    "case_id",
    "description",
    "categories",
    "entries",
    "published",
    "limits",
}
CATEGORY_SPEC_FIELDS = {"category_id", "label", "keep_days"}
ENTRY_SPEC_FIELDS = {"entry_id", "category_id", "bytes", "age_days", "recovery", "trace"}
PUBLISHED_SPEC_FIELDS = {"release_id", "required_index", "required_traces"}
LIMIT_FIELDS = {"max_categories", "max_entries", "max_published", "max_required"}
MANIFEST_FIELDS = {
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "root",
    "root_identity",
    "parent_identity",
    "control_identity",
    "manifest_identity",
    "inputs_identity",
    "project_sha256",
    "inputs_sha256",
    "created_at",
}
INPUTS_FIELDS = {
    "policy",
    "schema_version",
    "case_id",
    "project_sha256",
    "categories",
    "entries",
    "published",
    "deleted",
    "kept",
    "protected",
}
ACTION_FIELDS_MIN = {
    "entry_id",
    "category_id",
    "bytes",
    "keep_days",
    "age_days",
    "action",
    "reason",
}
REQUIREMENT_FIELDS = {"entry_id", "release_id", "reason"}
RECOVERY_LOSS_FIELDS = {"entry_id", "recovery"}
TRACE_CHANGE_FIELDS = {"trace", "before", "after", "removed"}
PLAN_FIELDS = {
    "policy",
    "plan_version",
    "case_id",
    "actions",
    "protected",
    "recovery_loss",
    "trace_changes",
}
COUNT_FIELDS = {
    "categories",
    "entries",
    "published",
    "deleted",
    "kept",
    "protected",
    "deleted_bytes",
    "kept_bytes",
    "lost_recovery",
    "shortened_traces",
    "model_calls",
    "network_requests",
}
REPORT_FIELDS = {
    "policy",
    "schema_version",
    "case_id",
    "description",
    "counts",
    "model_calls",
    "network_requests",
}
OUTPUT_MANIFEST_FIELDS = {
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "source_root",
    "source_inputs_sha256",
    "plan_sha256",
    "report_sha256",
    "markdown_sha256",
    "counts",
}
CASES_FIELDS = {
    "schema_version",
    "producer",
    "case_id",
    "policy",
    "sequence",
    "variants",
    "expected",
    "boundaries",
}
EXPECTED_FIELDS = {
    "policy",
    "plan_version",
    "case_id",
    "counts",
    "actions",
    "protected",
    "recovery_loss",
    "trace_changes",
    "rejections",
    "determinism",
    "boundaries",
    "derivation",
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


def identifier(value) -> bool:
    if not isinstance(value, str):
        return False
    if not value or len(value) > MAX_ID:
        return False
    return all(char.isascii() and (char.isalnum() or char in "-_.") for char in value)


def text_field(value, limit: int) -> bool:
    if not isinstance(value, str):
        return False
    return bool(value) and len(value) <= limit


class RetentionError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def validate(declaration: dict) -> None:
    """Re-derive the declaration checks, in the same order the product uses."""
    if declaration.get("policy") != POLICY:
        raise RetentionError("RETENTION_ARGUMENT", f"policy={declaration.get('policy')}")
    if declaration.get("schema_version") != SCHEMA_VERSION:
        raise RetentionError("RETENTION_ARGUMENT", "schema_version")
    if declaration.get("producer") != AUTHOR_PRODUCER:
        raise RetentionError("RETENTION_ARGUMENT", f"producer={declaration.get('producer')}")
    if not identifier(declaration.get("case_id")):
        raise RetentionError("RETENTION_ARGUMENT", "case_id")
    if not text_field(declaration.get("description"), MAX_TEXT):
        raise RetentionError("RETENTION_ARGUMENT", "description")

    limits = declaration.get("limits") or {}
    if set(limits) != LIMIT_FIELDS:
        raise RetentionError("RETENTION_ARGUMENT", f"limits 字段 {sorted(limits)}")
    if not 1 <= limits["max_categories"] <= MAX_CATEGORIES:
        raise RetentionError("RETENTION_ARGUMENT", "max_categories")
    if not 1 <= limits["max_entries"] <= MAX_ENTRIES:
        raise RetentionError("RETENTION_ARGUMENT", "max_entries")
    if not 1 <= limits["max_published"] <= MAX_PUBLISHED:
        raise RetentionError("RETENTION_ARGUMENT", "max_published")
    if not 1 <= limits["max_required"] <= MAX_REQUIRED:
        raise RetentionError("RETENTION_ARGUMENT", "max_required")

    categories = declaration.get("categories") or []
    if not categories or len(categories) > limits["max_categories"]:
        raise RetentionError("RETENTION_ARGUMENT", "categories 数量")
    category_ids: set[str] = set()
    labels: set[str] = set()
    for category in categories:
        if set(category) != CATEGORY_SPEC_FIELDS:
            raise RetentionError("RETENTION_ARGUMENT", f"类别字段 {sorted(category)}")
        if not identifier(category.get("category_id")):
            raise RetentionError("RETENTION_ARGUMENT", "category_id")
        if not text_field(category.get("label"), MAX_LABEL):
            raise RetentionError("RETENTION_ARGUMENT", "category.label")
        if not isinstance(category.get("keep_days"), int):
            raise RetentionError("RETENTION_ARGUMENT", "keep_days")
        if category["category_id"] in category_ids:
            raise RetentionError("RETENTION_ARGUMENT", f"类别编号 {category['category_id']} 重复")
        category_ids.add(category["category_id"])
        if category["label"] in labels:
            raise RetentionError("RETENTION_ARGUMENT", f"类别名称 {category['label']} 重复")
        labels.add(category["label"])
        if category["keep_days"] > MAX_DAYS:
            raise RetentionError("RETENTION_ARGUMENT", "keep_days 超过上限")

    entries = declaration.get("entries") or []
    if not entries or len(entries) > limits["max_entries"]:
        raise RetentionError("RETENTION_ARGUMENT", "entries 数量")
    entry_ids: set[str] = set()
    for entry in entries:
        if set(entry) - {"recovery", "trace"} != {"entry_id", "category_id", "bytes", "age_days"}:
            raise RetentionError("RETENTION_ARGUMENT", f"条目字段 {sorted(entry)}")
        if not identifier(entry.get("entry_id")):
            raise RetentionError("RETENTION_ARGUMENT", "entry_id")
        if entry["entry_id"] in entry_ids:
            raise RetentionError("RETENTION_ARGUMENT", f"条目编号 {entry['entry_id']} 重复")
        entry_ids.add(entry["entry_id"])
        if entry.get("category_id") not in category_ids:
            raise RetentionError(
                "RETENTION_ARGUMENT",
                f"条目 {entry['entry_id']} 的类别 {entry.get('category_id')} 未声明",
            )
        if not isinstance(entry.get("bytes"), int) or entry["bytes"] < 0:
            raise RetentionError("RETENTION_ARGUMENT", "bytes")
        if not isinstance(entry.get("age_days"), int) or entry["age_days"] > MAX_DAYS:
            raise RetentionError("RETENTION_ARGUMENT", "age_days")
        for key in ("recovery", "trace"):
            if key in entry and not text_field(entry[key], MAX_TEXT):
                raise RetentionError("RETENTION_ARGUMENT", f"entry.{key}")

    published = declaration.get("published") or []
    if not published or len(published) > limits["max_published"]:
        raise RetentionError("RETENTION_ARGUMENT", "published 数量")
    release_ids: set[str] = set()
    for release in published:
        if set(release) != PUBLISHED_SPEC_FIELDS:
            raise RetentionError("RETENTION_ARGUMENT", f"已发布字段 {sorted(release)}")
        if not identifier(release.get("release_id")):
            raise RetentionError("RETENTION_ARGUMENT", "release_id")
        if release["release_id"] in release_ids:
            raise RetentionError("RETENTION_ARGUMENT", f"已发布版本 {release['release_id']} 重复")
        release_ids.add(release["release_id"])
        required_index = release.get("required_index") or []
        required_traces = release.get("required_traces") or []
        if len(required_index) > limits["max_required"] or len(required_traces) > limits["max_required"]:
            raise RetentionError("RETENTION_ARGUMENT", "必需条目超过上限")
        for required in required_index + required_traces:
            if not identifier(required):
                raise RetentionError("RETENTION_ARGUMENT", "required entry")
            if required not in entry_ids:
                raise RetentionError(
                    "RETENTION_ARGUMENT",
                    f"已发布版本 {release['release_id']} 的必需条目 {required} 不存在",
                )


def protected_map(declaration: dict) -> dict[str, list[dict]]:
    out: dict[str, list[dict]] = {}
    for release in declaration["published"]:
        for entry in release["required_index"]:
            out.setdefault(entry, []).append(
                {
                    "entry_id": entry,
                    "release_id": release["release_id"],
                    "reason": "已发布版本必需的索引条目",
                }
            )
        for entry in release["required_traces"]:
            out.setdefault(entry, []).append(
                {
                    "entry_id": entry,
                    "release_id": release["release_id"],
                    "reason": "已发布版本必需的追溯条目",
                }
            )
    return out


def replay(declaration: dict) -> dict:
    validate(declaration)
    required = protected_map(declaration)
    categories = {item["category_id"]: item for item in declaration["categories"]}

    actions: list[dict] = []
    expired_protected: list[str] = []
    for entry in declaration["entries"]:
        category = categories[entry["category_id"]]
        expired = entry["age_days"] > category["keep_days"]
        releases = required.get(entry["entry_id"])
        release_id = releases[0]["release_id"] if releases else None
        if expired and release_id is not None:
            expired_protected.append(entry["entry_id"])
            continue
        if expired:
            action, reason = ACTION_DELETE, REASON_EXPIRED
        elif release_id is not None:
            action, reason = ACTION_KEEP, REASON_PROTECTED
        else:
            action, reason = ACTION_KEEP, REASON_WITHIN_POLICY
        record = {
            "entry_id": entry["entry_id"],
            "category_id": entry["category_id"],
            "bytes": entry["bytes"],
            "keep_days": category["keep_days"],
            "age_days": entry["age_days"],
            "action": action,
            "reason": reason,
        }
        if release_id is not None:
            record["release_id"] = release_id
        actions.append(record)

    if expired_protected:
        expired_protected.sort()
        raise RetentionError(
            "RETENTION_PROTECTED",
            "会过期：" + "；".join(expired_protected),
        )

    actions.sort(key=lambda item: item["entry_id"])

    deleted_ids = {item["entry_id"] for item in actions if item["action"] == ACTION_DELETE}
    recovery_loss = [
        {"entry_id": entry["entry_id"], "recovery": entry["recovery"]}
        for entry in declaration["entries"]
        if entry["entry_id"] in deleted_ids and entry.get("recovery")
    ]
    recovery_loss.sort(key=lambda item: item["entry_id"])

    traces: dict[str, list[str]] = {}
    trace_order: dict[str, int] = {}
    for entry in declaration["entries"]:
        trace = entry.get("trace")
        if not trace:
            continue
        trace_order[trace] = trace_order.get(trace, 0) + 1
        traces.setdefault(trace, [])
        if entry["entry_id"] in deleted_ids:
            traces[trace].append(entry["entry_id"])

    trace_changes = []
    for trace in sorted(traces):
        removed = sorted(traces[trace])
        if not removed:
            continue
        before = trace_order[trace]
        trace_changes.append(
            {"trace": trace, "before": before, "after": before - len(removed), "removed": removed}
        )

    protected_records = []
    for entry in declaration["entries"]:
        for item in required.get(entry["entry_id"], []):
            protected_records.append(dict(item))
    protected_records.sort(
        key=lambda item: (item["entry_id"], item["release_id"], item["reason"])
    )

    deleted = sum(1 for item in actions if item["action"] == ACTION_DELETE)
    kept = sum(1 for item in actions if item["action"] == ACTION_KEEP)
    counts = {
        "categories": len(declaration["categories"]),
        "entries": len(declaration["entries"]),
        "published": len(declaration["published"]),
        "deleted": deleted,
        "kept": kept,
        "protected": sum(1 for item in actions if item["reason"] == REASON_PROTECTED),
        "deleted_bytes": sum(item["bytes"] for item in actions if item["action"] == ACTION_DELETE),
        "kept_bytes": sum(item["bytes"] for item in actions if item["action"] == ACTION_KEEP),
        "lost_recovery": len(recovery_loss),
        "shortened_traces": len(trace_changes),
        "model_calls": 0,
        "network_requests": 0,
    }
    plan = {
        "policy": POLICY,
        "plan_version": PLAN_VERSION,
        "case_id": declaration["case_id"],
        "actions": actions,
        "protected": protected_records,
        "recovery_loss": recovery_loss,
        "trace_changes": trace_changes,
    }
    report = {
        "policy": POLICY,
        "schema_version": SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "counts": counts,
        "model_calls": 0,
        "network_requests": 0,
    }
    return {"declaration": declaration, "plan": plan, "report": report, "counts": counts}


def render_markdown(value: dict) -> str:
    plan = value["plan"]
    counts = value["counts"]
    out: list[str] = []
    out.append(f"# 保留与清理计划（{plan['case_id']}）")
    out.append("")
    out.append(value["declaration"]["description"])
    out.append("")
    out.append("## 清理规模")
    out.append("")
    out.append(
        f"- 可清理类别 {counts['categories']} 个、存储条目 {counts['entries']} 条、已发布版本 {counts['published']} 个"
    )
    out.append(
        f"- 删除 {counts['deleted']} 条（{counts['deleted_bytes']} 字节）、保留 {counts['kept']} 条（{counts['kept_bytes']} 字节），其中受保护 {counts['protected']} 条"
    )
    out.append("- 模型调用与网络请求 0 次")
    out.append("")
    out.append("## 逐条动作")
    out.append("")
    out.append("| 条目 | 类别 | 字节 | 保留期 | 年龄 | 动作 | 理由 |")
    out.append("| --- | --- | --- | --- | --- | --- | --- |")
    for action in plan["actions"]:
        reason = action["reason"]
        if action.get("release_id"):
            reason = f"{reason}（{action['release_id']}）"
        label = "删除" if action["action"] == ACTION_DELETE else "保留"
        out.append(
            f"| {action['entry_id']} | {action['category_id']} | {action['bytes']} | {action['keep_days']} 天 | {action['age_days']} 天 | {label} | {reason} |"
        )
    out.append("")
    out.append("## 影响说明")
    out.append("")
    if plan["recovery_loss"]:
        for loss in plan["recovery_loss"]:
            out.append(f"- {loss['entry_id']} 被删除后，恢复能力「{loss['recovery']}」随之丧失。")
    else:
        out.append("- 没有恢复能力会因此丧失。")
    if plan["trace_changes"]:
        for change in plan["trace_changes"]:
            out.append(
                f"- 追溯链 {change['trace']} 会从 {change['before']} 条变短到 {change['after']} 条，删除的是 {'、'.join(change['removed'])}。"
            )
    else:
        out.append("- 没有追溯链会因此变短。")
    out.append("")
    out.append("## 受保护条目")
    out.append("")
    if plan["protected"]:
        for item in plan["protected"]:
            out.append(f"- {item['entry_id']}（{item['release_id']}）：{item['reason']}。")
    else:
        out.append("- 本次没有已发布版本必需的条目。")
    out.append("")
    out.append("## 边界")
    out.append("")
    out.append("- 只生成计划：不删除任何文件、不改写任何已发布版本，清理动作由调用方按计划执行。")
    out.append("- 只作用于声明的可清理类别；已发布版本必需的索引与追溯条目一律受保护，命中保护即整条拒绝。")
    out.append("- 同一冻结输入的两次计划必须字节相同（输出清单不含时间戳）。")
    out.append("- 模型调用与网络请求 0 次，不判断内容质量与科学正确性。")
    out.append("")
    return "\n".join(out)


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
        manifest = strict_load(path, MANIFEST_FIELDS, "保留根清单")
    except StrictError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, f"policy={manifest['policy']}")
    report.record(
        "manifest-producer",
        manifest["producer"] == APP_PRODUCER,
        f"producer={manifest['producer']}",
    )
    report.record(
        "manifest-schema",
        manifest["schema_version"] == SCHEMA_VERSION,
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
            "保留声明副本的摘要与清单不符",
        )
    inputs_path = root / CONTROL / INPUTS
    if inputs_path.is_file():
        report.record(
            "manifest-inputs-digest",
            sha256(read_bytes(inputs_path)) == manifest["inputs_sha256"],
            "冻结输入的摘要与清单不符",
        )
    for field in (
        "root_identity",
        "parent_identity",
        "control_identity",
        "manifest_identity",
        "inputs_identity",
    ):
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
        declaration = json.loads(read_bytes(path).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        report.record("project-strict", False, f"保留声明副本不可读：{error}")
        return None
    if not isinstance(declaration, dict):
        report.record("project-strict", False, "保留声明副本必须是 JSON 对象。")
        return None
    report.record(
        "project-fields",
        set(declaration) == PROJECT_FIELDS,
        f"字段 {sorted(declaration)}",
    )
    try:
        validate(declaration)
        report.record("project-validates", True)
    except RetentionError as error:
        report.record("project-validates", False, f"{error.code}：{error.detail}")
    return declaration


def check_inputs(report: Report, root: Path, value: dict | None, declaration: dict | None) -> dict | None:
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
        "inputs-schema",
        inputs["schema_version"] == SCHEMA_VERSION,
        f"schema_version={inputs['schema_version']}",
    )
    if declaration is not None:
        report.record("inputs-case", inputs["case_id"] == declaration["case_id"], "case_id 不一致")
        project_path = root / CONTROL / PROJECT
        report.record(
            "inputs-project-digest",
            sha256(read_bytes(project_path)) == inputs["project_sha256"],
            "冻结输入登记的保留声明摘要与副本不符",
        )
    if value is not None:
        for key in ("categories", "entries", "published", "deleted", "kept", "protected"):
            report.record(
                f"inputs-count:{key}",
                inputs[key] == value["counts"][key],
                f"重放 {value['counts'][key]}，冻结输入 {inputs[key]}",
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
        if not member.is_file() or member.name == FROZEN:
            continue
        relpath = member.relative_to(package).as_posix()
        payload = read_bytes(member)
        actual[relpath] = {"sha256": sha256(payload), "byte_length": len(payload)}
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
    return json.loads((package / PROJECT).read_text(encoding="utf-8"))


def replay_package(package: Path) -> dict:
    return replay(load_package(package))


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
    except RetentionError as error:
        report.record("package-validates", False, f"{error.code}：{error.detail}")
        return
    if value is not None:
        report.record(
            "package-replay",
            replayed["report"] == value["report"],
            "包内声明的重放与冻结根的重放不一致",
        )


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
        "expected-version",
        expected["plan_version"] == PLAN_VERSION,
        f"plan_version={expected['plan_version']}",
    )
    if value is None:
        return
    report.record(
        "expected-case",
        expected["case_id"] == value["report"]["case_id"],
        "先验 case_id 与声明不一致",
    )
    report.record("expected-count-fields", set(expected["counts"]) == COUNT_FIELDS, "先验计数键不完整")
    for key, want in expected["counts"].items():
        report.record(
            f"expected-count:{key}",
            value["counts"][key] == want,
            f"先验 {want}，重放 {value['counts'][key]}",
        )
    report.record(
        "expected-actions",
        value["plan"]["actions"] == expected["actions"],
        "逐条动作与先验不符",
    )
    report.record(
        "expected-protected",
        value["plan"]["protected"] == expected["protected"],
        "受保护条目与先验不符",
    )
    report.record(
        "expected-recovery-loss",
        value["plan"]["recovery_loss"] == expected["recovery_loss"],
        "恢复能力丧失与先验不符",
    )
    report.record(
        "expected-trace-changes",
        value["plan"]["trace_changes"] == expected["trace_changes"],
        "追溯链变化与先验不符",
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
    report.record(
        "expected-no-deletion",
        expected["boundaries"].get("deletes_files") is False,
        "先验必须声明本包不删除任何文件",
    )
    # The rejection list in the prior and the per-variant expectation in the case
    # list are two views of the same promise; they must not drift apart.
    cases_path = package / "cases.json"
    if cases_path.is_file():
        try:
            cases = json.loads(cases_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            report.record("expected-rejections", False, f"cases.json 不可读：{error}")
            return
        declared = [
            {
                "variant": variant.get("project"),
                "code": (variant.get("expect") or {}).get("code"),
                "exit": (variant.get("expect") or {}).get("exit"),
            }
            for variant in cases.get("variants") or []
        ]
        report.record(
            "expected-rejections",
            declared == expected["rejections"],
            "先验的拒绝清单与用例清单不一致",
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
        want = (variant.get("expect") or {}).get("code")
        try:
            declaration = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            report.record(f"variant-code:{name}", False, f"不是合法 JSON：{error}")
            continue
        try:
            replay(declaration)
        except RetentionError as error:
            report.record(
                f"variant-code:{name}",
                error.code == want,
                f"期望 {want}，重放 {error.code}",
            )
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
    for name in (PLAN, REPORT):
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
        "输出清单不得带时间戳，否则两次计划不再相同",
    )
    for name, field in (
        (PLAN, "plan_sha256"),
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
        plan = json.loads((output / PLAN).read_text(encoding="utf-8"))
        report.record("output-plan", plan == value["plan"], "plan.json 与重放不符")
        result = json.loads((output / REPORT).read_text(encoding="utf-8"))
        report.record("output-report", result == value["report"], "report.json 与重放不符")
        markdown = (output / MARKDOWN).read_text(encoding="utf-8")
        report.record(
            "output-markdown",
            markdown == render_markdown(value),
            "report.md 与重放不符",
        )


def freeze(package: Path) -> int:
    if not package.is_dir():
        print(f"{package} 不是目录。", file=sys.stderr)
        return 2
    files = {}
    for member in sorted(package.rglob("*")):
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
    parser = argparse.ArgumentParser(description="保留与清理计划的独立只读审计器")
    parser.add_argument("root", nargs="?", help="已建立的保留根")
    parser.add_argument("--output", help="retention build 的输出根")
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
        print("至少要给一个保留根或 --package。", file=sys.stderr)
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
    value = None
    if root is not None:
        manifest = check_manifest(report, root)
        declaration = check_project(report, root)
        if declaration is not None:
            try:
                value = replay(declaration)
                report.record("replay", True)
            except RetentionError as error:
                report.record("replay", False, f"{error.code}：{error.detail}")
        check_inputs(report, root, value, declaration)

    check_package(report, package, value)
    # In package-only mode there is no root to replay, but the package's own
    # declaration is just as good a subject for the prior comparison.
    if value is None and package is not None and (package / PROJECT).is_file():
        try:
            value = replay_package(package)
            report.record("package-replay", True)
        except RetentionError as error:
            report.record("package-replay", False, f"{error.code}：{error.detail}")
    check_expected(report, package, value)
    check_variants(report, package)
    check_frozen(report, package)
    check_output(report, output, value)

    if manifest is not None and value is not None:
        report.record(
            "manifest-case",
            manifest["case_id"] == value["report"]["case_id"],
            "清单登记的 case_id 与重放不符",
        )

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
