#!/usr/bin/env python3
"""Read-only auditor for a diagnostics-export root and its output.

This program never imports the Rust product and never runs the CLI to decide
anything: it re-derives the export from the frozen declaration on its own, then
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

POLICY = "diagnostics-export-v1"
SCHEMA_VERSION = 1
EXPORT_VERSION = 1
APP_PRODUCER = "noetome-diagnostics"
OUTPUT_PRODUCER = "diagnostics_export_output"
AUTHOR_PRODUCER = "fixture_author"

MAX_CATEGORIES = 8
MAX_ENTRIES = 64
MAX_PREVIEW = 240
MAX_ID = 48
MAX_TEXT = 300
MAX_LABEL = 120
MAX_CONFIG = 16
MAX_FINGERPRINTS = 16

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
EXPORT = "export.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = (CONTROL,)
CONTROL_ENTRIES = tuple(sorted((INPUTS, MANIFEST, PROJECT)))
OUTPUT_ENTRIES = tuple(sorted((EXPORT, MANIFEST, MARKDOWN, REPORT)))

PROJECT_FIELDS = {
    "policy",
    "schema_version",
    "producer",
    "case_id",
    "description",
    "config",
    "fingerprints",
    "categories",
    "selection",
    "consent",
    "limits",
}
CONFIG_FIELDS = {"key", "value"}
FINGERPRINT_FIELDS = {"name", "digest"}
ENTRY_SPEC_FIELDS = {"entry_id", "label", "bytes", "preview"}
CATEGORY_SPEC_FIELDS = {"category_id", "label", "sensitive", "entries"}
CONSENT_FIELDS = {"sensitive_approved"}
LIMIT_FIELDS = {"max_categories", "max_entries", "max_preview"}
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
    "selected_categories",
    "config",
    "fingerprints",
}
EXPORT_FIELDS = {
    "policy",
    "export_version",
    "case_id",
    "description",
    "config",
    "fingerprints",
    "entries",
}
ENTRY_RECORD_FIELDS = {"entry_id", "category_id", "label", "bytes", "preview"}
CATEGORY_RECORD_FIELDS = {
    "category_id",
    "label",
    "sensitive",
    "included",
    "entries",
    "selected_entries",
    "bytes",
    "exported_bytes",
    "preview",
}
COUNT_FIELDS = {
    "categories",
    "selected_categories",
    "excluded_categories",
    "sensitive_categories",
    "sensitive_included",
    "entries",
    "selected_entries",
    "exported_bytes",
    "excluded_bytes",
    "config",
    "fingerprints",
    "model_calls",
    "network_requests",
}
REPORT_FIELDS = {
    "policy",
    "schema_version",
    "case_id",
    "description",
    "counts",
    "categories",
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
    "export_sha256",
    "report_sha256",
    "markdown_sha256",
    "counts",
}
CASES_FIELDS = {"schema_version", "producer", "case_id", "policy", "sequence", "variants", "expected", "boundaries"}
EXPECTED_FIELDS = {
    "policy",
    "export_version",
    "case_id",
    "counts",
    "categories",
    "entries",
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


def identifier(value: str, label: str) -> bool:
    if not isinstance(value, str):
        return False
    trimmed = value.strip()
    if not trimmed or len(trimmed) > MAX_ID:
        return False
    return all(char.isascii() and (char.isalnum() or char in "-_") for char in value)


def single(value: str, limit: int) -> bool:
    if not isinstance(value, str):
        return False
    trimmed = value.strip()
    return bool(trimmed) and len(trimmed) <= limit


def is_digest(value: str) -> bool:
    if not isinstance(value, str) or len(value) != 64:
        return False
    return all(char in "0123456789abcdef" for char in value)


def absolute_forms(value: str) -> bool:
    """Any path-looking token counts, not only one that starts the string."""
    chars = list(value)
    boundary = set('（(「[【：:,，;；="\'')

    def starts_token(index: int) -> bool:
        if index == 0:
            return True
        previous = chars[index - 1]
        return previous.isspace() or previous in boundary

    for index, current in enumerate(chars):
        if current in "/\\" and starts_token(index):
            return True
        if index + 2 < len(chars):
            if current.isascii() and current.isalpha() and chars[index + 1] == ":" and chars[index + 2] in "/\\":
                return True
    return False


def secret_hits(values) -> list[str]:
    hits: set[str] = set()
    for label, value in values:
        lowered = value.lower()
        for marker in SECRET_MARKERS:
            if marker in lowered:
                hits.add(f"{label} 里出现疑似凭据字样「{marker}」")
    return sorted(hits)


def strings_of(declaration: dict) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = [
        ("案例编号", declaration.get("case_id", "")),
        ("说明", declaration.get("description", "")),
    ]
    for entry in declaration.get("config") or []:
        out.append((f"配置「{entry.get('key')}」的键", entry.get("key", "")))
        out.append((f"配置「{entry.get('key')}」的值", entry.get("value", "")))
    for fingerprint in declaration.get("fingerprints") or []:
        out.append((f"指纹「{fingerprint.get('name')}」的名称", fingerprint.get("name", "")))
        out.append((f"指纹「{fingerprint.get('name')}」的摘要", fingerprint.get("digest", "")))
    for category in declaration.get("categories") or []:
        out.append((f"范围类别「{category.get('category_id')}」的编号", category.get("category_id", "")))
        out.append((f"范围类别「{category.get('category_id')}」的名称", category.get("label", "")))
        for entry in category.get("entries") or []:
            out.append((f"条目「{entry.get('entry_id')}」的编号", entry.get("entry_id", "")))
            out.append((f"条目「{entry.get('entry_id')}」的名称", entry.get("label", "")))
            out.append((f"条目「{entry.get('entry_id')}」的预览", entry.get("preview", "")))
    return out


class ExportError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def validate(declaration: dict) -> dict:
    """Re-derive the validation order of the product and return the first refusal."""
    if declaration.get("policy") != POLICY:
        raise ExportError("DIAGNOSTICS_ARGUMENT", f"policy 必须是 {POLICY}。")
    if declaration.get("schema_version") != SCHEMA_VERSION:
        raise ExportError("DIAGNOSTICS_ARGUMENT", "schema_version 必须是 1。")
    if declaration.get("producer") != AUTHOR_PRODUCER:
        raise ExportError("DIAGNOSTICS_ARGUMENT", f"producer 必须是 {AUTHOR_PRODUCER}。")
    if not identifier(declaration.get("case_id", ""), "case_id"):
        raise ExportError("DIAGNOSTICS_ARGUMENT", "case_id 不合法。")
    if not single(declaration.get("description", ""), MAX_TEXT):
        raise ExportError("DIAGNOSTICS_ARGUMENT", "description 不合法。")

    limits = declaration.get("limits") or {}
    for key, bound in (
        ("max_categories", MAX_CATEGORIES),
        ("max_entries", MAX_ENTRIES),
        ("max_preview", MAX_PREVIEW),
    ):
        value = limits.get(key)
        if not isinstance(value, int) or not 1 <= value <= bound:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"{key} 必须在 1 到 {bound} 之间。")

    config = declaration.get("config") or []
    if len(config) > MAX_CONFIG:
        raise ExportError("DIAGNOSTICS_ARGUMENT", "配置项超过上限。")
    seen_keys: set[str] = set()
    for entry in config:
        if not identifier(entry.get("key", ""), "配置键"):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "配置键不合法。")
        if not single(entry.get("value", ""), MAX_TEXT):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "配置值不合法。")
        if entry["key"] in seen_keys:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"配置键重复：{entry['key']}。")
        seen_keys.add(entry["key"])

    fingerprints = declaration.get("fingerprints") or []
    if len(fingerprints) > MAX_FINGERPRINTS:
        raise ExportError("DIAGNOSTICS_ARGUMENT", "指纹超过上限。")
    seen_names: set[str] = set()
    for fingerprint in fingerprints:
        if not identifier(fingerprint.get("name", ""), "指纹名称"):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "指纹名称不合法。")
        if not is_digest(fingerprint.get("digest", "")):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "指纹摘要必须是 64 位小写十六进制。")
        if fingerprint["name"] in seen_names:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"指纹名称重复：{fingerprint['name']}。")
        seen_names.add(fingerprint["name"])

    categories = declaration.get("categories") or []
    if not categories:
        raise ExportError("DIAGNOSTICS_ARGUMENT", "至少要声明一个范围类别。")
    if len(categories) > limits["max_categories"]:
        raise ExportError("DIAGNOSTICS_ARGUMENT", "范围类别超过声明上限。")
    seen_categories: set[str] = set()
    seen_entries: set[str] = set()
    for category in categories:
        if not identifier(category.get("category_id", ""), "范围类别编号"):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "范围类别编号不合法。")
        if not single(category.get("label", ""), MAX_LABEL):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "范围类别名称不合法。")
        if category["category_id"] in seen_categories:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"范围类别编号重复：{category['category_id']}。")
        seen_categories.add(category["category_id"])
        entries = category.get("entries") or []
        if not entries:
            raise ExportError(
                "DIAGNOSTICS_ARGUMENT",
                f"范围类别 {category['category_id']} 没有任何条目：空类别会让预览看起来比实际更完整。",
            )
        if len(entries) > limits["max_entries"]:
            raise ExportError("DIAGNOSTICS_ARGUMENT", "条目数超过声明上限。")
        for entry in entries:
            if not identifier(entry.get("entry_id", ""), "条目编号"):
                raise ExportError("DIAGNOSTICS_ARGUMENT", "条目编号不合法。")
            if not single(entry.get("label", ""), MAX_LABEL):
                raise ExportError("DIAGNOSTICS_ARGUMENT", "条目名称不合法。")
            if len(entry.get("preview", "")) > limits["max_preview"]:
                raise ExportError("DIAGNOSTICS_ARGUMENT", "条目预览超过声明上限。")
            if entry["entry_id"] in seen_entries:
                raise ExportError("DIAGNOSTICS_ARGUMENT", f"条目编号重复：{entry['entry_id']}。")
            seen_entries.add(entry["entry_id"])

    seen_selection: set[str] = set()
    for category_id in declaration.get("selection") or []:
        if not identifier(category_id, "勾选的范围类别"):
            raise ExportError("DIAGNOSTICS_ARGUMENT", "勾选的范围类别不合法。")
        if category_id not in seen_categories:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"勾选了不存在的范围类别：{category_id}。")
        if category_id in seen_selection:
            raise ExportError("DIAGNOSTICS_ARGUMENT", f"范围类别被勾选了两次：{category_id}。")
        seen_selection.add(category_id)

    strings = strings_of(declaration)
    absolute = [f"{label}「{value}」" for label, value in strings if absolute_forms(value)]
    if absolute:
        raise ExportError(
            "DIAGNOSTICS_ABSOLUTE_PATH",
            "诊断声明里出现绝对路径：" + "；".join(absolute),
        )

    hits = secret_hits(strings)
    if hits:
        raise ExportError("DIAGNOSTICS_SECRET_MARKER", "；".join(hits))
    return {"ok": True}


def replay(declaration: dict) -> dict:
    """Independently derive the export the way the product documents it."""
    validate(declaration)
    selection = set(declaration.get("selection") or [])

    # Consent is a hard stop before any counting: a selected sensitive category
    # without a recorded approval must not produce an export at all.
    sensitive_selected = [
        category["category_id"]
        for category in declaration["categories"]
        if category["sensitive"] and category["category_id"] in selection
    ]
    if sensitive_selected and not declaration["consent"]["sensitive_approved"]:
        raise ExportError(
            "DIAGNOSTICS_CONSENT",
            "勾选了敏感范围 " + "、".join(sensitive_selected) + "，但没有记录同意。",
        )

    counts = {
        "categories": len(declaration["categories"]),
        "selected_categories": 0,
        "excluded_categories": 0,
        "sensitive_categories": 0,
        "sensitive_included": 0,
        "entries": 0,
        "selected_entries": 0,
        "exported_bytes": 0,
        "excluded_bytes": 0,
        "config": len(declaration["config"]),
        "fingerprints": len(declaration["fingerprints"]),
        "model_calls": 0,
        "network_requests": 0,
    }
    entries: list[dict] = []
    categories: list[dict] = []
    for category in declaration["categories"]:
        included = category["category_id"] in selection
        total_bytes = sum(entry["bytes"] for entry in category["entries"])
        selected_entries = 0
        for entry in category["entries"]:
            counts["entries"] += 1
            if included:
                entries.append(
                    {
                        "entry_id": entry["entry_id"],
                        "category_id": category["category_id"],
                        "label": entry["label"],
                        "bytes": entry["bytes"],
                        "preview": entry["preview"],
                    }
                )
                selected_entries += 1
                counts["selected_entries"] += 1
                counts["exported_bytes"] += entry["bytes"]
            else:
                counts["excluded_bytes"] += entry["bytes"]
        if included:
            counts["selected_categories"] += 1
            if category["sensitive"]:
                counts["sensitive_included"] += 1
        else:
            counts["excluded_categories"] += 1
        if category["sensitive"]:
            counts["sensitive_categories"] += 1
        categories.append(
            {
                "category_id": category["category_id"],
                "label": category["label"],
                "sensitive": category["sensitive"],
                "included": included,
                "entries": len(category["entries"]),
                "selected_entries": selected_entries,
                "bytes": total_bytes,
                "exported_bytes": total_bytes if included else 0,
                "preview": category["entries"][0]["preview"] if category["entries"] else "",
            }
        )

    export = {
        "policy": POLICY,
        "export_version": EXPORT_VERSION,
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "config": declaration["config"],
        "fingerprints": declaration["fingerprints"],
        "entries": entries,
    }
    report = {
        "policy": POLICY,
        "schema_version": SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "counts": counts,
        "categories": categories,
        "model_calls": 0,
        "network_requests": 0,
    }
    return {"export": export, "report": report, "counts": counts, "categories": categories, "entries": entries}


def category_rows(report: dict) -> str:
    out = "| 范围类别 | 名称 | 敏感 | 是否导出 | 条目 | 字节 | 预览 |\n"
    out += "| --- | --- | --- | --- | --- | --- | --- |\n"
    for category in report["categories"]:
        included = "已导出" if category["included"] else "未导出"
        sensitive = "是" if category["sensitive"] else "否"
        preview = category["preview"].replace("|", "\\|").replace("\n", " ")
        out += (
            f"| {category['category_id']} | {category['label']} | {sensitive} | {included} | "
            f"{category['entries']} | {category['bytes']} | {preview} |\n"
        )
    return out


def render_markdown(value: dict) -> str:
    report = value["report"]
    counts = report["counts"]
    out = f"# 诊断导出（{report['case_id']}）\n\n"
    out += f"{report['description']}\n\n"
    out += "## 导出范围\n\n"
    out += (
        f"- 范围类别 {counts['categories']} 个：已导出 {counts['selected_categories']} 个、"
        f"未导出 {counts['excluded_categories']} 个；敏感类别 {counts['sensitive_categories']} 个，"
        f"其中已导出 {counts['sensitive_included']} 个。\n"
    )
    out += (
        f"- 条目 {counts['entries']} 条：已导出 {counts['selected_entries']} 条，"
        f"共 {counts['exported_bytes']} 字节；未导出 {counts['excluded_bytes']} 字节。\n"
    )
    out += (
        f"- 复现信息：配置 {counts['config']} 项、输入指纹 {counts['fingerprints']} 个。\n\n"
    )
    out += "## 类别与预览\n\n"
    out += category_rows(report)
    out += "\n"
    out += "## 未导出的类别\n\n"
    excluded = [category for category in report["categories"] if not category["included"]]
    if not excluded:
        out += "- 没有未导出的类别：本次勾选覆盖了全部范围。\n\n"
    else:
        for category in excluded:
            out += (
                f"- {category['category_id']}（{category['label']}）：未勾选，如实标为未导出；"
                f"这 {category['entries']} 条、{category['bytes']} 字节不在包里。\n"
            )
        out += "\n"
    out += "## 边界\n\n"
    out += "- 只导出声明的类别与条目：没有勾选的内容不会被静默补全。\n"
    out += "- 预览是声明写好的短摘要，不是原文全文；默认不含原文与来源正文。\n"
    out += "- 声明里出现绝对路径或疑似凭据时整包拒绝，拒绝时不写出任何字节。\n"
    out += "- 导出清单不含时间戳：同一冻结声明的两次导出字节相同。\n"
    out += "- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。\n"
    return out


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
        manifest = strict_load(path, MANIFEST_FIELDS, "诊断根清单")
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
            "工程声明副本的摘要与清单不符",
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
    except ExportError as error:
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
            "冻结输入登记的工程声明摘要与副本不符",
        )
    if value is not None:
        for key in ("categories", "entries", "selected_categories", "config", "fingerprints"):
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
    except ExportError as error:
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
        expected["export_version"] == EXPORT_VERSION,
        f"export_version={expected['export_version']}",
    )
    if value is None:
        return
    report.record("expected-case", expected["case_id"] == value["report"]["case_id"], "先验 case_id 与声明不一致")
    report.record("expected-count-fields", set(expected["counts"]) == COUNT_FIELDS, "先验计数键不完整")
    for key, want in expected["counts"].items():
        report.record(
            f"expected-count:{key}",
            value["counts"][key] == want,
            f"先验 {want}，重放 {value['counts'][key]}",
        )
    category_keys = ("category_id", "sensitive", "included", "entries", "selected_entries", "bytes", "exported_bytes")
    report.record(
        "expected-categories",
        [{key: item[key] for key in category_keys} for item in value["categories"]]
        == expected["categories"],
        "类别表与先验不符",
    )
    entry_keys = ("entry_id", "category_id", "bytes", "preview")
    report.record(
        "expected-entries",
        [{key: item[key] for key in entry_keys} for item in value["entries"]]
        == expected["entries"],
        "条目表与先验不符",
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
        "expected-determinism",
        not expected["determinism"].get("manifest_has_created_at", True),
        "先验必须声明输出清单不含时间戳",
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
            {"project": variant.get("project"), "code": (variant.get("expect") or {}).get("code"),
             "exit": (variant.get("expect") or {}).get("exit")}
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
        except ExportError as error:
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
    for name in (EXPORT, REPORT):
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
        "输出清单不得带时间戳，否则两次导出不再相同",
    )
    for name, field in (
        (EXPORT, "export_sha256"),
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
        exported = json.loads((output / EXPORT).read_text(encoding="utf-8"))
        report.record("output-export", exported == value["export"], "export.json 与重放不符")
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
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="诊断导出的独立只读审计器")
    parser.add_argument("root", nargs="?", help="已建立的诊断根")
    parser.add_argument("--output", help="diagnostics build 的输出根")
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
        print("至少要给一个诊断根或 --package。", file=sys.stderr)
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
            except ExportError as error:
                report.record("replay", False, f"{error.code}：{error.detail}")
        check_inputs(report, root, value, declaration)

    check_package(report, package, value)
    # In package-only mode there is no root to replay, but the package's own
    # declaration is just as good a subject for the prior comparison.
    if value is None and package is not None and (package / PROJECT).is_file():
        try:
            value = replay_package(package)
            report.record("package-replay", True)
        except ExportError as error:
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
