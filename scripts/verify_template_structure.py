#!/usr/bin/env python3
"""Independent, read-only auditor for `noetome template` (B12-T02).

The auditor never imports the Rust product and never runs the CLI to decide
anything: it replays the documented rules from the declaration itself and
compares what it derives with what the product froze or wrote.

Usage: <root> [--output DIR] [--package DIR] [--json] [--freeze]
Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "structure-template-v1"
APP_PRODUCER = "noetome-template"
OUTPUT_PRODUCER = "structure_template_output"
PLAN_VERSION = 1
SCHEMA_VERSION = 1

CONTROL = "control"
MANIFEST = "manifest.json"
INPUTS = "inputs.json"
PROJECT = "project.json"
PLAN = "plan.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("inputs.json", "manifest.json", "project.json")
OUTPUT_ENTRIES = ("manifest.json", "plan.json", "report.json", "report.md")

ROLE_EXPLANATION = "explanation"
ROLE_REFERENCE = "reference"
ROLE_EMBED = "embed"
ROLES = (ROLE_EXPLANATION, ROLE_REFERENCE, ROLE_EMBED)

KINDS = {
    "overview": "总览",
    "reaction": "反应",
    "regulation": "调控",
    "energy": "能量",
    "comparison": "比较",
}

DEVIATION_OMITTED = "section_omitted"
DEVIATION_SPECIAL = "special_section"
DEVIATION_TITLE = "title_deviation"
DEVIATION_DEPTH = "depth_deviation"

FINDING_INCONSISTENT = "inconsistent_structure"
FINDING_UNEXPLAINED = "concept_unexplained"
FINDING_DUPLICATE = "duplicate_explanation"

PROJECT_FIELDS = (
    "policy",
    "schema_version",
    "producer",
    "case_id",
    "description",
    "folder_root",
    "templates",
    "topics",
    "concepts",
    "atoms",
    "limits",
)
MANIFEST_FIELDS = (
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
)
IDENTITY_FIELDS = ("platform", "device", "inode")
INPUTS_FIELDS = (
    "policy",
    "schema_version",
    "case_id",
    "project_sha256",
    "templates",
    "topics",
    "goals",
    "concepts",
    "atoms",
)
PLAN_FIELDS = ("policy", "plan_version", "case_id", "folder_root", "topics", "goals", "explanations")
TOPIC_FIELDS = ("topic_id", "title", "folder", "template_id", "level", "goals")
GOAL_FIELDS = (
    "goal_id",
    "topic_id",
    "template_id",
    "kind",
    "title",
    "path",
    "navigation_depth",
    "sections",
    "atoms",
    "references",
)
SECTION_FIELDS = ("section", "atoms", "special")
REFERENCE_FIELDS = ("concept_id", "target_note", "role")
EXPLANATION_FIELDS = ("concept_id", "canonical", "note", "references")
REPORT_FIELDS = ("policy", "schema_version", "case_id", "description", "verdict", "counts", "deviations", "findings")
COUNT_FIELDS = (
    "templates",
    "topics",
    "goals",
    "notes",
    "concepts",
    "atoms",
    "explanations",
    "references",
    "embeds",
    "sections",
    "special_sections",
    "omitted_sections",
    "deviations",
    "findings",
    "model_calls",
    "network_requests",
)
DEVIATION_FIELDS = ("kind", "subject", "detail")
OUTPUT_MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "source_root",
    "source_inputs_sha256",
    "plan_sha256",
    "report_sha256",
    "markdown_sha256",
    "verdict",
    "counts",
)


class Report:
    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append((name, bool(ok), detail))

    def failures(self) -> list[tuple[str, bool, str]]:
        return [item for item in self.checks if not item[1]]

    def dump(self, as_json: bool) -> None:
        if as_json:
            print(
                json.dumps(
                    {
                        "checks": [
                            {"name": name, "ok": ok, "detail": detail}
                            for name, ok, detail in self.checks
                        ],
                        "failures": len(self.failures()),
                        "total": len(self.checks),
                    },
                    ensure_ascii=False,
                    indent=2,
                )
            )
            return
        for name, ok, detail in self.checks:
            if not ok:
                print(f"[失败] {name}：{detail}")
        print(f"合计 {len(self.checks)} 项检查：失败 {len(self.failures())}、提示 0")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


class StrictError(ValueError):
    pass


def strict_load(path: Path, fields: tuple[str, ...], label: str, canonical_required: bool = True):
    try:
        text = read_bytes(path).decode("utf-8")
    except FileNotFoundError as error:
        raise StrictError(f"{label} 不存在：{path}") from error
    except UnicodeDecodeError as error:
        raise StrictError(f"{label} 必须是 UTF-8：{error}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise StrictError(f"{label} 不是合法 JSON：{error}") from error
    if not isinstance(value, dict):
        raise StrictError(f"{label} 必须是对象")
    if tuple(sorted(value)) != tuple(sorted(fields)):
        raise StrictError(f"{label} 字段集合不符：{sorted(value)}")
    if canonical_required:
        expected = canonical(value)
        if text not in (expected, expected + "\n"):
            raise StrictError(f"{label} 不是规范编码")
    return value


def text_field(mapping, key: str, limit: int, label: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise StrictError(f"{label}.{key} 必须是非空字符串")
    if len(value) > limit:
        raise StrictError(f"{label}.{key} 太长")
    return value


def identifier(value, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise StrictError(f"{label} 必须是非空字符串")
    if len(value) > 48:
        raise StrictError(f"{label} 太长")
    for char in value:
        if not (char.isascii() and (char.isalnum() or char in "-_")):
            raise StrictError(f"{label} 只能包含 ASCII 字母、数字、- 和 _")
    return value


def relative(value, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise StrictError(f"{label} 必须是非空字符串")
    if value.startswith("/") or ":" in value or ".." in value.split("/"):
        raise StrictError(f"{label} 必须是相对路径")
    return value


def load_package(package: Path, project_name: str = PROJECT):
    declaration = strict_load(package / project_name, PROJECT_FIELDS, "结构声明", canonical_required=False)
    if declaration["policy"] != POLICY:
        raise StrictError("结构声明的 policy 与合同不符")
    if declaration["schema_version"] != SCHEMA_VERSION:
        raise StrictError("结构声明的 schema_version 与合同不符")
    return declaration


class PlanError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def _goals(declaration):
    for topic in declaration["topics"]:
        for goal in topic["goals"]:
            yield topic, goal


def structured_failure(declaration) -> str:
    """Return the first refusal code the documented order would raise."""
    templates = {template["template_id"]: template for template in declaration["templates"]}
    concepts = {concept["concept_id"] for concept in declaration["concepts"]}
    goals = {goal["goal_id"]: goal for _, goal in _goals(declaration)}
    seen = set()
    for topic, goal in _goals(declaration):
        if goal["goal_id"] in seen:
            return "TEMPLATE_ARGUMENT"
        seen.add(goal["goal_id"])
        if goal["kind"] not in KINDS:
            return "TEMPLATE_ARGUMENT"
        template = templates.get(topic["template_id"])
        if template is None:
            return "TEMPLATE_ARGUMENT"
        positions = {section: index for index, section in enumerate(template["sections"])}
        previous = -1
        for section in goal["sections"]:
            if section not in positions:
                if not goal.get("special_reason"):
                    return "TEMPLATE_UNREASONED"
                continue
            position = positions[section]
            if previous >= 0 and position <= previous:
                return "TEMPLATE_SECTION_ORDER"
            previous = position
        if goal["navigation_depth"] != template["navigation_depth"] and not goal.get("depth_reason"):
            return "TEMPLATE_UNREASONED"
        expected_title = f"{topic['title']}：{KINDS[goal['kind']]}"
        if goal["title"] != expected_title and not goal.get("title_reason"):
            return "TEMPLATE_UNREASONED"
    for atom in declaration["atoms"]:
        if atom["concept_id"] not in concepts or atom["goal_id"] not in goals:
            return "TEMPLATE_ARGUMENT"
        if atom["role"] not in ROLES:
            return "TEMPLATE_ARGUMENT"
        goal = goals[atom["goal_id"]]
        if atom["section"] not in goal["sections"]:
            return "TEMPLATE_ARGUMENT"
    for topic, goal in _goals(declaration):
        declared = set(goal["sections"])
        for section in goal["sections"]:
            has = any(
                atom["goal_id"] == goal["goal_id"] and atom["section"] == section
                for atom in declaration["atoms"]
            )
            if not has:
                return "TEMPLATE_EMPTY_SECTION"
        del declared
    # A reference or embed inside the note that carries the explanation is noise.
    for concept in declaration["concepts"]:
        cid = concept["concept_id"]
        owned = [atom for atom in declaration["atoms"] if atom["concept_id"] == cid and atom["role"] == ROLE_EXPLANATION]
        if len(owned) == 1:
            owner = owned[0]["goal_id"]
            for atom in declaration["atoms"]:
                if atom["concept_id"] == cid and atom["role"] != ROLE_EXPLANATION and atom["goal_id"] == owner:
                    return "TEMPLATE_ARGUMENT"
    return ""


def replay(declaration):
    templates = {template["template_id"]: template for template in declaration["templates"]}
    canonical = {concept["concept_id"]: concept["canonical"] for concept in declaration["concepts"]}
    goals_out = []
    deviations = []
    index_by_goal = {}
    for topic in declaration["topics"]:
        for index, goal in enumerate(topic["goals"], start=1):
            index_by_goal[goal["goal_id"]] = index
            template = templates[topic["template_id"]]
            sections = []
            for section in goal["sections"]:
                atoms = [
                    atom["atom_id"]
                    for atom in declaration["atoms"]
                    if atom["goal_id"] == goal["goal_id"] and atom["section"] == section
                ]
                sections.append(
                    {
                        "section": section,
                        "atoms": atoms,
                        "special": section not in template["sections"],
                    }
                )
            for section in template["sections"]:
                if section not in goal["sections"]:
                    deviations.append(
                        {
                            "kind": DEVIATION_OMITTED,
                            "subject": f"{goal['goal_id']}#{section}",
                            "detail": f"阅读目标 {goal['goal_id']} 省略了模板章节 {section}。",
                        }
                    )
            if goal["title"] != f"{topic['title']}：{KINDS[goal['kind']]}":
                deviations.append(
                    {
                        "kind": DEVIATION_TITLE,
                        "subject": goal["goal_id"],
                        "detail": f"阅读目标 {goal['goal_id']} 的标题偏离模板命名。",
                    }
                )
            if goal["navigation_depth"] != template["navigation_depth"]:
                deviations.append(
                    {
                        "kind": DEVIATION_DEPTH,
                        "subject": goal["goal_id"],
                        "detail": f"阅读目标 {goal['goal_id']} 的导航深度偏离模板。",
                    }
                )
            goals_out.append(
                {
                    "goal_id": goal["goal_id"],
                    "topic_id": topic["topic_id"],
                    "template_id": topic["template_id"],
                    "kind": goal["kind"],
                    "title": goal["title"],
                    "path": f"{declaration['folder_root']}/{topic['folder']}/{index:02}-{goal['goal_id']}.md",
                    "navigation_depth": goal["navigation_depth"],
                    "sections": sections,
                    "atoms": [atom["atom_id"] for atom in declaration["atoms"] if atom["goal_id"] == goal["goal_id"]],
                    "references": [],
                }
            )
    by_id = {goal["goal_id"]: goal for goal in goals_out}
    explanations = []
    findings = []
    for concept in declaration["concepts"]:
        cid = concept["concept_id"]
        owned = [atom for atom in declaration["atoms"] if atom["concept_id"] == cid and atom["role"] == ROLE_EXPLANATION]
        if len(owned) == 0:
            findings.append({"kind": FINDING_UNEXPLAINED, "subject": cid, "detail": f"概念 {cid} 没有完整解释。"})
            continue
        if len(owned) > 1:
            findings.append(
                {
                    "kind": FINDING_DUPLICATE,
                    "subject": cid,
                    "detail": f"概念 {cid} 有 {len(owned)} 处完整解释。",
                }
            )
            continue
        owner = by_id[owned[0]["goal_id"]]
        references = [
            atom
            for atom in declaration["atoms"]
            if atom["concept_id"] == cid and atom["role"] != ROLE_EXPLANATION
        ]
        for atom in references:
            goal = by_id[atom["goal_id"]]
            goal["references"].append(
                {
                    "concept_id": cid,
                    "target_note": owner["path"],
                    "role": atom["role"],
                }
            )
        explanations.append(
            {
                "concept_id": cid,
                "canonical": canonical[cid],
                "note": owner["path"],
                "references": len(references),
            }
        )
    shapes = {}
    for goal in goals_out:
        key = (goal["template_id"], goal["navigation_depth"], goal["kind"])
        shape = [section["section"] for section in goal["sections"]]
        shapes.setdefault(key, []).append((goal["goal_id"], shape))
    for key, entries in shapes.items():
        baseline = entries[0][1]
        for goal_id, shape in entries[1:]:
            if shape != baseline:
                findings.append(
                    {
                        "kind": FINDING_INCONSISTENT,
                        "subject": goal_id,
                        "detail": f"同类同层主题 {key[0]}/{key[2]} 的章节序列与 {entries[0][0]} 不同。",
                    }
                )
    counts = {
        "templates": len(declaration["templates"]),
        "topics": len(declaration["topics"]),
        "goals": len(goals_out),
        "notes": len(goals_out),
        "concepts": len(declaration["concepts"]),
        "atoms": len(declaration["atoms"]),
        "explanations": len(explanations),
        "references": sum(1 for atom in declaration["atoms"] if atom["role"] == ROLE_REFERENCE),
        "embeds": sum(1 for atom in declaration["atoms"] if atom["role"] == ROLE_EMBED),
        "sections": sum(len(goal["sections"]) for goal in goals_out),
        "special_sections": sum(1 for deviation in deviations if deviation["kind"] == DEVIATION_SPECIAL),
        "omitted_sections": sum(1 for deviation in deviations if deviation["kind"] == DEVIATION_OMITTED),
        "deviations": len(deviations),
        "findings": len(findings),
        "model_calls": 0,
        "network_requests": 0,
    }
    order = [
        {
            "topic_id": topic["topic_id"],
            "title": topic["title"],
            "folder": topic["folder"],
            "template_id": topic["template_id"],
            "level": topic["level"],
            "goals": [goal["goal_id"] for goal in topic["goals"]],
        }
        for topic in declaration["topics"]
    ]
    plan_file = {
        "policy": POLICY,
        "plan_version": PLAN_VERSION,
        "case_id": declaration["case_id"],
        "folder_root": declaration["folder_root"],
        "topics": order,
        "goals": goals_out,
        "explanations": explanations,
    }
    report = {
        "policy": POLICY,
        "schema_version": SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "verdict": "pass" if not findings else "findings",
        "counts": counts,
        "deviations": deviations,
        "findings": findings,
    }
    return {"plan": plan_file, "report": report}


def render_markdown(replay_value):
    report = replay_value["report"]
    plan = replay_value["plan"]
    counts = report["counts"]
    lines = [f"# 主题阅读目标与结构模板（{plan['case_id']}）", "", "## 规划规模"]
    lines.append(f"- 模板 {counts['templates']} 个、主题 {counts['topics']} 个、阅读目标 {counts['goals']} 个")
    lines.append(f"- 笔记 {counts['notes']} 份、章节 {counts['sections']} 个")
    lines.append(f"- 主解释 {counts['explanations']} 个、引用 {counts['references']} 处、嵌入 {counts['embeds']} 处")
    lines.extend(["", "## 阅读目标与结构", "", "| 阅读目标 | 路径 | 章节 | 原子 | 引用 |", "| --- | --- | --- | --- | --- |"])
    for goal in plan["goals"]:
        sections = "、".join(section["section"] + ("（模板外）" if section["special"] else "") for section in goal["sections"])
        lines.append(f"| {goal['title']} | {goal['path']} | {sections} | {len(goal['atoms'])} | {len(goal['references'])} |")
    lines.extend(["", "## 唯一解释位置", "", "| 概念 | 笔记 | 引用 |", "| --- | --- | --- |"])
    for entry in plan["explanations"]:
        lines.append(f"| {entry['canonical']} | {entry['note']} | {entry['references']} |")
    lines.extend(["", "## 偏差与理由", ""])
    for deviation in report["deviations"]:
        lines.append(f"- {deviation['kind']}（{deviation['subject']}）：{deviation['detail']}")
    if not report["deviations"]:
        lines.append("- 无")
    lines.extend(["", "## 缺口", ""])
    for finding in report["findings"]:
        lines.append(f"- {finding['kind']}（{finding['subject']}）：{finding['detail']}")
    if not report["findings"]:
        lines.append("- 无")
    lines.extend(["", "## 边界", ""])
    lines.append("- 只读结构声明，不生成知识正文，不写项目与 Vault。")
    lines.append("- 每个概念整份规划里恰有一处完整解释，其余位置只能引用或同源嵌入。")
    lines.append("- 模型调用与网络请求 0 次，不判断内容质量与科学正确性。")
    return "\n".join(lines) + "\n"


def check_layout(root: Path, report: Report) -> None:
    if not root.is_dir():
        report.record("root-is-directory", False, f"{root} 不是目录")
        return
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control/")
        return
    report.record("layout-control", True, "控制目录存在")
    names = sorted(item.name for item in control.iterdir())
    report.record("layout-control-entries", names == list(CONTROL_ENTRIES), f"控制目录 {names}")
    links = [path for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-no-links", not links, f"符号链接 {links}")


def check_manifest(root: Path, report: Report):
    path = root / CONTROL / MANIFEST
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "冻结清单")
    except StrictError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True, "")
    report.record("manifest-policy", manifest["policy"] == POLICY, manifest["policy"])
    report.record("manifest-producer", manifest["producer"] == APP_PRODUCER, manifest["producer"])
    report.record("manifest-schema", manifest["schema_version"] == SCHEMA_VERSION, str(manifest["schema_version"]))
    report.record("manifest-root", Path(manifest["root"]).resolve() == root.resolve(), manifest["root"])
    for key in ("root_identity", "parent_identity", "control_identity", "manifest_identity", "inputs_identity"):
        report.record(f"manifest-identity:{key}", tuple(sorted(manifest[key])) == tuple(sorted(IDENTITY_FIELDS)), str(manifest[key]))
    for name, digest_key in ((PROJECT, "project_sha256"), (INPUTS, "inputs_sha256")):
        actual = sha256(read_bytes(root / CONTROL / name))
        report.record(f"manifest-digest:{name}", actual == manifest[digest_key], f"{actual} != {manifest[digest_key]}")
    return manifest


def check_inputs(root: Path, report: Report, manifest) -> None:
    try:
        inputs = strict_load(root / CONTROL / INPUTS, INPUTS_FIELDS, "冻结输入摘要")
    except StrictError as error:
        report.record("inputs-strict", False, str(error))
        return
    report.record("inputs-strict", True, "")
    report.record("inputs-case", inputs["case_id"] == manifest["case_id"], inputs["case_id"])
    report.record("inputs-project", inputs["project_sha256"] == manifest["project_sha256"], inputs["project_sha256"])
    report.record("inputs-policy", inputs["policy"] == POLICY, inputs["policy"])


def check_frozen(package: Path, report: Report) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, "缺少 frozen-sha256.json")
        return
    report.record("frozen-present", True, "")
    frozen = json.loads(path.read_text(encoding="utf-8"))
    actual = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item == path:
            continue
        relative_path = item.relative_to(package).as_posix()
        data = read_bytes(item)
        actual[relative_path] = {"sha256": sha256(data), "byte_length": len(data)}
    report.record("frozen-members", sorted(actual) == sorted(frozen["files"]), f"{sorted(actual)}")
    for name, entry in actual.items():
        recorded = frozen["files"].get(name, {})
        report.record(f"frozen-digest:{name}", recorded == entry, f"{recorded} != {entry}")


def check_expected(package: Path, replay_value, report: Report) -> None:
    expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
    report.record("expected-policy", expected["policy"] == POLICY, expected["policy"])
    for key, value in expected["counts"].items():
        actual = replay_value["report"]["counts"].get(key)
        report.record(f"expected-count:{key}", actual == value, f"{actual} != {value}")
    report.record("expected-verdict", replay_value["report"]["verdict"] == expected["verdict"], replay_value["report"]["verdict"])
    goals = {goal["goal_id"]: goal for goal in replay_value["plan"]["goals"]}
    for prior in expected["goals"]:
        actual = goals.get(prior["goal_id"])
        ok = (
            actual is not None
            and actual["path"] == prior["path"]
            and actual["title"] == prior["title"]
            and [section["section"] for section in actual["sections"]] == prior["sections"]
            and len(actual["atoms"]) == prior["atoms"]
            and len(actual["references"]) == prior["references"]
        )
        report.record(f"expected-goal:{prior['goal_id']}", ok, f"{actual}")
    explanations = {entry["concept_id"]: entry for entry in replay_value["plan"]["explanations"]}
    for prior in expected["explanations"]:
        actual = explanations.get(prior["concept_id"])
        ok = (
            actual is not None
            and actual["note"] == prior["note"]
            and actual["references"] == prior["references"]
        )
        report.record(f"expected-explanation:{prior['concept_id']}", ok, f"{actual}")
    kinds = {}
    for deviation in replay_value["report"]["deviations"]:
        kinds[deviation["kind"]] = kinds.get(deviation["kind"], 0) + 1
    for kind, value in expected["deviations"].items():
        report.record(f"expected-deviation:{kind}", kinds.get(kind, 0) == value, f"{kinds.get(kind, 0)} != {value}")


def check_variants(package: Path, expected, report: Report) -> None:
    for name, prior in expected["variants"].items():
        path = package / f"project-{name}.json"
        try:
            declaration = load_package(package, path.name)
        except StrictError as error:
            report.record(f"variant-{name}:load", False, str(error))
            continue
        code = structured_failure(declaration)
        if "code" in prior:
            report.record(f"variant-{name}:code", code == prior["code"], f"{code} != {prior['code']}")
        else:
            if code:
                report.record(f"variant-{name}:code", False, f"意外拒绝 {code}")
            else:
                replay_value = replay(declaration)
                kinds = {finding["kind"] for finding in replay_value["report"]["findings"]}
                report.record(f"variant-{name}:finding", prior["finding"] in kinds, f"{sorted(kinds)}")


def check_output(output: Path, replay_value, report: Report) -> None:
    if not output.is_dir():
        report.record("output-is-directory", False, f"{output} 不是目录")
        return
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"输出根 {entries}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    report.record("output-complete", not missing, f"缺少 {missing}")
    if missing:
        return
    try:
        plan = strict_load(output / PLAN, PLAN_FIELDS, "规划结果")
        result = strict_load(output / REPORT, REPORT_FIELDS, "规划报告")
        manifest = strict_load(output / OUTPUT_MANIFEST, OUTPUT_MANIFEST_FIELDS, "输出清单")
    except StrictError as error:
        report.record("output-strict", False, str(error))
        return
    report.record("output-strict", True, "")
    report.record("output-plan", plan == replay_value["plan"], "规划结果与重放不一致")

    def project_report(value):
        return {
            "policy": value["policy"],
            "schema_version": value["schema_version"],
            "case_id": value["case_id"],
            "description": value["description"],
            "verdict": value["verdict"],
            "counts": value["counts"],
            "deviations": [(item["kind"], item["subject"]) for item in value["deviations"]],
            "findings": [(item["kind"], item["subject"]) for item in value["findings"]],
        }

    report.record(
        "output-report",
        project_report(result) == project_report(replay_value["report"]),
        "报告与重放不一致（按种类与主体比对，说明文字不复制产品措辞）",
    )
    for goal in plan["goals"]:
        report.record(f"output-goal-fields:{goal['goal_id']}", tuple(sorted(goal)) == tuple(sorted(GOAL_FIELDS)), f"{sorted(goal)}")
        for section in goal["sections"]:
            report.record(
                f"output-section-fields:{goal['goal_id']}",
                tuple(sorted(section)) == tuple(sorted(SECTION_FIELDS)),
                f"{sorted(section)}",
            )
    report.record("output-manifest-producer", manifest["producer"] == OUTPUT_PRODUCER, manifest["producer"])
    report.record("output-manifest-no-created-at", "created_at" not in manifest, "")
    for name, key in ((PLAN, "plan_sha256"), (REPORT, "report_sha256"), (MARKDOWN, "markdown_sha256")):
        actual = sha256(read_bytes(output / name))
        report.record(f"output-manifest-digest:{name}", actual == manifest[key], f"{actual} != {manifest[key]}")
    report.record("output-manifest-verdict", manifest["verdict"] == result["verdict"], manifest["verdict"])
    report.record("output-manifest-counts", manifest["counts"] == result["counts"], "计数不一致")
    markdown = (output / MARKDOWN).read_text(encoding="utf-8")
    for heading in ("## 规划规模", "## 阅读目标与结构", "## 唯一解释位置", "## 偏差与理由", "## 缺口", "## 边界"):
        report.record(f"markdown-section:{heading}", heading in markdown, f"缺少 {heading}")
    report.record("markdown-boundary", "不判断内容质量与科学正确性" in markdown, "")
    report.record("markdown-no-model", "模型调用与网络请求 0 次" in markdown, "")


def freeze(package: Path, report: Report) -> int:
    files = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        relative_path = item.relative_to(package).as_posix()
        data = read_bytes(item)
        files[relative_path] = {"sha256": sha256(data), "byte_length": len(data)}
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("root", nargs="?")
    parser.add_argument("--output")
    parser.add_argument("--package")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args(argv)

    report = Report()
    if args.freeze:
        if not args.package:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        return freeze(Path(args.package), report)

    if args.package is None and args.root is None:
        print("至少要给一个结构根或 --package。", file=sys.stderr)
        return 2

    if args.package is not None:
        package = Path(args.package)
        if not (package / PROJECT).is_file():
            print(f"{package} 不是结构包（缺少 project.json）。", file=sys.stderr)
            return 2
        try:
            declaration = load_package(package)
        except StrictError as error:
            report.record("package", False, str(error))
        else:
            report.record("package", True, "")
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            replay_value = replay(declaration)
            check_expected(package, replay_value, report)
            check_variants(package, expected, report)
            check_frozen(package, report)
            if args.root is not None:
                root = Path(args.root)
                check_layout(root, report)
                manifest = check_manifest(root, report)
                if manifest is not None:
                    check_inputs(root, report, manifest)
                if args.output:
                    check_output(Path(args.output), replay_value, report)

    if args.root is not None and args.package is None:
        root = Path(args.root)
        if not root.is_dir():
            print(f"{root} 不是目录。", file=sys.stderr)
            return 2
        check_layout(root, report)
        manifest = check_manifest(root, report)
        if manifest is not None:
            check_inputs(root, report, manifest)
        try:
            declaration = load_package(root / CONTROL, PROJECT)
        except StrictError as error:
            # 被改坏的结构声明已经由摘要检查点名；这里不该把审计变成 traceback。
            report.record("root-declaration", False, str(error))
        else:
            replay_value = replay(declaration)
            if args.output:
                check_output(Path(args.output), replay_value, report)

    report.dump(args.json)
    return 0 if not report.failures() else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
