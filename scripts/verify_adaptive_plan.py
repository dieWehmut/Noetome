#!/usr/bin/env python3
"""Independent, read-only audit of a Noetome adaptive plan.

This script never imports the Rust product and never runs the CLI to decide
anything: it replays the planning rules from the frozen declaration and compares
its own answer with the recorded plan, the hand-written prior and the on-disk
freeze. A disagreement is a failure, not a warning.

Usage: <root> [--output DIR] [--package DIR] [--json] [--freeze]
Exit codes: 0 all checks passed, 1 failures, 2 usage error
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "adaptive-plan-v1"
APP_PRODUCER = "noetome-plan"
OUTPUT_PRODUCER = "adaptive_plan_output"
PLAN_VERSION = 1
SCHEMA_VERSION = 1

RELATION_PREREQUISITE = "prerequisite"
RELATION_RELATED = "related"
SPLIT_REASONS = ("chain_too_long", "topic_boundary", "user_lock")
WARNING_GOAL_WITHOUT_ATOMS = "goal_without_atoms"

CONTROL = "control"
MANIFEST = "manifest.json"
INPUTS = "inputs.json"
PROJECT = "project.json"
PLAN = "plan.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = ["control"]
CONTROL_ENTRIES = ["inputs.json", "manifest.json", "project.json"]
OUTPUT_ENTRIES = ["manifest.json", "plan.json", "report.json", "report.md"]

PROJECT_FIELDS = (
    "policy", "schema_version", "producer", "case_id", "description", "folder_root",
    "topics", "goals", "concepts", "relations", "chains", "atoms", "limits",
)
MANIFEST_FIELDS = (
    "policy", "producer", "schema_version", "case_id", "root", "root_identity",
    "parent_identity", "control_identity", "manifest_identity", "inputs_identity",
    "project_sha256", "inputs_sha256", "created_at",
)
IDENTITY_FIELDS = ("platform", "device", "inode")
INPUTS_FIELDS = (
    "policy", "schema_version", "case_id", "project_sha256", "topics", "goals",
    "concepts", "atoms", "chains",
)
PLAN_FIELDS = (
    "policy", "plan_version", "case_id", "folder_root", "teaching_order", "folders",
    "notes", "atoms",
)
ORDER_FIELDS = ("concept_id", "canonical")
FOLDER_FIELDS = ("topic_id", "path", "title")
NOTE_FIELDS = ("note_id", "topic_id", "goal_id", "path", "title", "primary", "atom_ids", "links")
LINK_FIELDS = ("concept_id", "target_note", "display_text")
ATOM_FIELDS = ("atom_id", "concept_id", "chain_id", "note_id", "position")
REPORT_FIELDS = (
    "policy", "schema_version", "case_id", "description", "counts", "chains",
    "warnings", "model_calls", "network_requests",
)
COUNT_FIELDS = (
    "topics", "goals", "concepts", "atoms", "atoms_planned", "notes", "folders",
    "chains", "chains_split", "cross_refs",
)
CHAIN_FIELDS = ("chain_id", "label", "notes", "split_reason")
WARNING_FIELDS = ("kind", "subject", "detail")
OUTPUT_MANIFEST_FIELDS = (
    "policy", "producer", "schema_version", "case_id", "source_root",
    "source_inputs_sha256", "plan_sha256", "report_sha256", "markdown_sha256", "counts",
)


class Report:
    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append((name, bool(ok), detail))

    @property
    def failures(self) -> list[tuple[str, bool, str]]:
        return [entry for entry in self.checks if not entry[1]]

    def dump(self, as_json: bool) -> None:
        if as_json:
            payload = {
                "checks": [
                    {"name": name, "ok": ok, "detail": detail}
                    for name, ok, detail in self.checks
                ],
                "failures": len(self.failures),
                "total": len(self.checks),
            }
            json.dump(payload, sys.stdout, ensure_ascii=False, indent=2)
            sys.stdout.write("\n")
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


def strict_load(path: Path, fields: tuple[str, ...], label: str, canonical_required: bool = True):
    try:
        raw = read_bytes(path)
    except OSError as error:
        raise ValueError(f"{label} 不存在：{error}") from error
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{label} 不是 UTF-8：{error}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"{label} 不是合法 JSON：{error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{label} 不是 JSON 对象")
    if set(value) != set(fields):
        missing = sorted(set(fields) - set(value))
        extra = sorted(set(value) - set(fields))
        raise ValueError(f"{label} 字段不符：缺少 {missing}、多出 {extra}")
    if canonical_required:
        expected = canonical(value)
        if text not in (expected, expected + "\n"):
            raise ValueError(f"{label} 不是规范编码（字段顺序或空白被改动）")
    return value


def identifier(value, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} 必须是非空字符串")
    if len(value) > 48:
        raise ValueError(f"{label} 超过 48 个字符")
    if any(ch not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for ch in value):
        raise ValueError(f"{label} 只能使用 ASCII 字母、数字、- 与 _")
    return value


def text_field(value, label: str, limit: int = 600) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} 必须是非空字符串")
    if len(value) > limit:
        raise ValueError(f"{label} 超过 {limit} 个字符")
    return value


def relative(value, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} 必须是相对路径字符串")
    if value.startswith("/") or "\\" in value or ":" in value:
        raise ValueError(f"{label} 必须是相对路径，不能是绝对路径：{value}")
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise ValueError(f"{label} 不能包含空段、. 或 ..：{value}")
    return value


def load_project(package: Path) -> dict:
    return strict_load(package / PROJECT, PROJECT_FIELDS, "规划声明", canonical_required=False)


class PlanError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def teaching_order(declaration: dict) -> list[str]:
    concepts = [entry["concept_id"] for entry in declaration["concepts"]]
    position = {concept: index for index, concept in enumerate(concepts)}
    indegree = {concept: 0 for concept in concepts}
    edges: dict[str, list[str]] = {concept: [] for concept in concepts}
    for relation in declaration["relations"]:
        if relation["kind"] != RELATION_PREREQUISITE:
            continue
        edges[relation["from"]].append(relation["to"])
        indegree[relation["to"]] += 1
    ready = sorted(
        (concept for concept in concepts if indegree[concept] == 0),
        key=lambda concept: position[concept],
    )
    order: list[str] = []
    while ready:
        concept = ready.pop(0)
        order.append(concept)
        for target in edges[concept]:
            indegree[target] -= 1
            if indegree[target] == 0:
                ready.append(target)
        ready.sort(key=lambda entry: position[entry])
    if len(order) != len(concepts):
        left = sorted(set(concepts) - set(order))
        raise PlanError("PLAN_PREREQUISITE_CYCLE", f"成环：{'、'.join(left)}")
    return order


def replay(declaration: dict) -> dict:
    topics = declaration["topics"]
    goals = {goal["goal_id"]: goal for goal in declaration["goals"]}
    concepts = {concept["concept_id"]: concept for concept in declaration["concepts"]}
    order = teaching_order(declaration)
    rank = {concept: index for index, concept in enumerate(order)}

    folders: list[dict] = []
    notes: list[dict] = []
    atoms: list[dict] = []
    cross_refs = 0

    for topic in topics:
        ordered_goals = []
        for goal_index, goal_id in enumerate(topic["goals"]):
            goal = goals[goal_id]
            best = min(
                (
                    rank[atom["concept_id"]]
                    for atom in declaration["atoms"]
                    if atom["goal_id"] == goal_id
                ),
                default=len(order) + 1,
            )
            ordered_goals.append((goal, best, goal_index))
        ordered_goals.sort(key=lambda entry: (entry[1], entry[2]))

        folders.append(
            {
                "topic_id": topic["topic_id"],
                "path": f"{declaration['folder_root']}/{topic['folder']}",
                "title": topic["title"],
            }
        )

        note_paths: dict[str, str] = {}
        for position, (goal, _best, _index) in enumerate(ordered_goals):
            path = (
                f"{declaration['folder_root']}/{topic['folder']}/"
                f"{position + 1:02d}-{goal['goal_id']}.md"
            )
            note_paths[goal["goal_id"]] = path
            notes.append(
                {
                    "note_id": f"{topic['topic_id']}-{goal['goal_id']}",
                    "topic_id": topic["topic_id"],
                    "goal_id": goal["goal_id"],
                    "path": path,
                    "title": goal["title"],
                    "primary": goal["goal_id"] == topic["primary_goal"],
                    "atom_ids": [],
                    "links": [],
                }
            )

        buckets: dict[str, list[dict]] = {}
        for atom in declaration["atoms"]:
            if atom["concept_id"] not in topic["concepts"]:
                continue
            buckets.setdefault(atom["goal_id"], []).append(atom)
        for goal_id, bucket in buckets.items():
            bucket.sort(
                key=lambda atom: (
                    0 if atom.get("chain_id") else 1,
                    atom.get("step") or 0,
                    rank[atom["concept_id"]],
                    [entry["atom_id"] for entry in declaration["atoms"]].index(atom["atom_id"]),
                )
            )
            index = next(
                position
                for position, note in enumerate(notes)
                if note["topic_id"] == topic["topic_id"] and note["goal_id"] == goal_id
            )
            note_id = notes[index]["note_id"]
            for position, atom in enumerate(bucket, start=1):
                notes[index]["atom_ids"].append(atom["atom_id"])
                atoms.append(
                    {
                        "atom_id": atom["atom_id"],
                        "concept_id": atom["concept_id"],
                        "chain_id": atom.get("chain_id"),
                        "note_id": note_id,
                        "position": position,
                    }
                )

        primary_path = note_paths[topic["primary_goal"]]
        seen: set[tuple[int, str]] = set()
        for atom in declaration["atoms"]:
            if atom["concept_id"] not in topic["concepts"]:
                continue
            if atom["goal_id"] == topic["primary_goal"]:
                continue
            index = next(
                (
                    position
                    for position, note in enumerate(notes)
                    if note["topic_id"] == topic["topic_id"] and note["goal_id"] == atom["goal_id"]
                ),
                None,
            )
            if index is None:
                continue
            key = (index, atom["concept_id"])
            if key in seen:
                continue
            seen.add(key)
            notes[index]["links"].append(
                {
                    "concept_id": atom["concept_id"],
                    "target_note": primary_path,
                    "display_text": concepts[atom["concept_id"]]["canonical"],
                }
            )
            cross_refs += 1

    note_index = {note["note_id"]: position for position, note in enumerate(notes)}
    atoms.sort(key=lambda atom: (note_index[atom["note_id"]], atom["position"]))

    chains = []
    split = 0
    for chain in declaration["chains"]:
        members = [atom for atom in atoms if atom["chain_id"] == chain["chain_id"]]
        note_ids = sorted({atom["note_id"] for atom in members})
        if len(note_ids) > 1:
            if chain.get("split_reason") not in SPLIT_REASONS:
                raise PlanError(
                    "PLAN_CHAIN_SPLIT",
                    f"反应链 {chain['chain_id']} 被拆到 {len(note_ids)} 份笔记却没有理由",
                )
            split += 1
        chains.append(
            {
                "chain_id": chain["chain_id"],
                "label": chain["label"],
                "notes": note_ids,
                "split_reason": chain.get("split_reason"),
            }
        )

    warnings = [
        {"kind": WARNING_GOAL_WITHOUT_ATOMS, "subject": note["note_id"]}
        for note in notes
        if not note["atom_ids"]
    ]

    counts = {
        "topics": len(topics),
        "goals": len(declaration["goals"]),
        "concepts": len(declaration["concepts"]),
        "atoms": len(declaration["atoms"]),
        "atoms_planned": len(atoms),
        "notes": len(notes),
        "folders": len(folders),
        "chains": len(declaration["chains"]),
        "chains_split": split,
        "cross_refs": cross_refs,
    }
    if counts["atoms_planned"] != counts["atoms"]:
        raise PlanError("PLAN_UNPLACED", "有原子没有规划位置")

    return {
        "policy": POLICY,
        "plan_version": PLAN_VERSION,
        "case_id": declaration["case_id"],
        "folder_root": declaration["folder_root"],
        "teaching_order": [
            {"concept_id": concept, "canonical": concepts[concept]["canonical"]} for concept in order
        ],
        "folders": folders,
        "notes": notes,
        "atoms": atoms,
        "counts": counts,
        "chains": chains,
        "warnings": warnings,
    }


def structured_failure(declaration: dict) -> tuple[str, str]:
    """Return the first refusal a declaration must trigger, mirroring validation order."""
    if len(declaration["topics"]) == 0:
        return "PLAN_ARGUMENT", "没有主题"
    known_topics = {topic["topic_id"] for topic in declaration["topics"]}
    concept_owner: dict[str, str] = {}
    for topic in declaration["topics"]:
        for concept in topic["concepts"]:
            if concept in concept_owner:
                return "PLAN_ARGUMENT", f"概念 {concept} 出现在两个主题里"
            concept_owner[concept] = topic["topic_id"]
    goals = {goal["goal_id"]: goal for goal in declaration["goals"]}
    for topic in declaration["topics"]:
        if topic["primary_goal"] not in topic["goals"]:
            return "PLAN_ARGUMENT", "主阅读目标不在目标列表里"
    for atom in declaration["atoms"]:
        goal = goals.get(atom["goal_id"])
        if goal is None:
            return "PLAN_ARGUMENT", f"原子 {atom['atom_id']} 指向未知目标"
        owner = concept_owner.get(atom["concept_id"])
        if owner is None:
            return "PLAN_ARGUMENT", f"原子 {atom['atom_id']} 的概念没有主题"
        if goal["topic_id"] != owner or goal["topic_id"] not in known_topics:
            return "PLAN_ARGUMENT", f"原子 {atom['atom_id']} 跨主题"
        if (atom.get("chain_id") is None) != (atom.get("step") is None):
            return "PLAN_ARGUMENT", f"原子 {atom['atom_id']} 的链与步骤必须同时给出"
    for chain in declaration["chains"]:
        steps = sorted(
            atom["step"] for atom in declaration["atoms"] if atom.get("chain_id") == chain["chain_id"]
        )
        if steps and steps != list(range(1, len(steps) + 1)):
            return "PLAN_ARGUMENT", f"反应链 {chain['chain_id']} 的步骤不连续"
    for topic in declaration["topics"]:
        for concept in topic["concepts"]:
            if not any(
                atom["concept_id"] == concept and atom["goal_id"] == topic["primary_goal"]
                for atom in declaration["atoms"]
            ):
                return "PLAN_UNPLACED", f"概念 {concept} 不在主阅读目标里"
    try:
        teaching_order(declaration)
    except PlanError as error:
        return error.code, error.detail
    if any(atom["chain_id"] for atom in declaration["atoms"]):
        try:
            replay(declaration)
        except PlanError as error:
            return error.code, error.detail
    return "", ""


def check_layout(root: Path, report: Report) -> None:
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == ROOT_ENTRIES, f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control 目录")
        return
    report.record("layout-control", True)
    control_entries = sorted(item.name for item in control.iterdir())
    report.record(
        "layout-control-entries",
        control_entries == CONTROL_ENTRIES,
        f"control 条目 {control_entries}",
    )
    links = [str(path) for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-symlinks", not links, f"发现符号链接 {links}")


def check_manifest(root: Path, report: Report) -> dict | None:
    path = root / CONTROL / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少冻结清单")
        return None
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "冻结清单")
    except ValueError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, str(manifest["policy"]))
    report.record("manifest-producer", manifest["producer"] == APP_PRODUCER, str(manifest["producer"]))
    report.record(
        "manifest-schema", manifest["schema_version"] == SCHEMA_VERSION, str(manifest["schema_version"])
    )
    report.record(
        "manifest-root",
        Path(manifest["root"]).resolve() == root.resolve(),
        f"{manifest['root']} != {root}",
    )
    for key in (
        "root_identity", "parent_identity", "control_identity", "manifest_identity", "inputs_identity"
    ):
        value = manifest[key]
        report.record(
            f"manifest-identity:{key}",
            isinstance(value, dict) and set(value) == set(IDENTITY_FIELDS),
            f"{key} 字段 {sorted(value) if isinstance(value, dict) else type(value).__name__}",
        )
    project = root / CONTROL / PROJECT
    report.record(
        "manifest-project-digest",
        project.is_file() and sha256(read_bytes(project)) == manifest["project_sha256"],
        "工程声明摘要不符",
    )
    inputs = root / CONTROL / INPUTS
    report.record(
        "manifest-inputs-digest",
        inputs.is_file() and sha256(read_bytes(inputs)) == manifest["inputs_sha256"],
        "冻结输入摘要不符",
    )
    return manifest


def check_inputs(root: Path, manifest: dict | None, declaration: dict, report: Report) -> None:
    path = root / CONTROL / INPUTS
    try:
        inputs = strict_load(path, INPUTS_FIELDS, "冻结输入")
    except ValueError as error:
        report.record("inputs-strict", False, str(error))
        return
    report.record("inputs-strict", True)
    report.record("inputs-case", inputs["case_id"] == declaration["case_id"], str(inputs["case_id"]))
    report.record("inputs-policy", inputs["policy"] == POLICY, str(inputs["policy"]))
    report.record(
        "inputs-project-digest",
        manifest is None or inputs["project_sha256"] == manifest["project_sha256"],
        "两份账本对工程声明的摘要不一致",
    )
    for field, key in (
        ("topics", "topics"), ("goals", "goals"), ("concepts", "concepts"),
        ("atoms", "atoms"), ("chains", "chains"),
    ):
        want = len(declaration[key])
        report.record(f"inputs-count:{field}", inputs[field] == want, f"{inputs[field]} != {want}")


def check_frozen(package: Path, report: Report) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, f"缺少 {FROZEN}")
        return
    try:
        frozen = json.loads(read_bytes(path).decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        report.record("frozen-strict", False, str(error))
        return
    recorded = frozen.get("files", {})
    actual = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        relation = item.relative_to(package).as_posix()
        actual[relation] = {"sha256": sha256(read_bytes(item)), "byte_length": item.stat().st_size}
    report.record(
        "frozen-members", sorted(recorded) == sorted(actual), "冻结清单与成员集合不一致"
    )
    for name in sorted(actual):
        report.record(
            f"frozen-digest:{name}",
            recorded.get(name) == actual[name],
            f"{name} 的摘要或长度与冻结清单不符",
        )


def check_expected(replay_result: dict, declaration: dict, expected: dict, report: Report) -> None:
    report.record("expected-policy", expected["policy"] == POLICY, str(expected["policy"]))
    report.record(
        "expected-case", expected["case_id"] == declaration["case_id"], str(expected["case_id"])
    )
    for key in COUNT_FIELDS:
        report.record(
            f"expected-count:{key}",
            replay_result["counts"][key] == expected["counts"][key],
            f"{replay_result['counts'][key]} != {expected['counts'][key]}",
        )
    report.record(
        "expected-teaching-order",
        replay_result["teaching_order"] == expected["teaching_order"],
        "教学顺序与先验不符",
    )
    report.record("expected-folders", replay_result["folders"] == expected["folders"], "文件夹与先验不符")
    report.record("expected-notes", replay_result["notes"] == expected["notes"], "笔记与先验不符")
    report.record("expected-atoms", replay_result["atoms"] == expected["atoms"], "原子安置与先验不符")
    report.record("expected-chains", replay_result["chains"] == expected["chains"], "反应链与先验不符")
    projected = [
        {"kind": warning["kind"], "subject": warning["subject"]}
        for warning in replay_result["warnings"]
    ]
    report.record(
        "expected-warnings",
        projected == expected["warnings"],
        f"{projected} != {expected['warnings']}",
    )
    report.record("expected-zero-model", expected["model_calls"] == 0, "先验要求零模型调用")
    report.record("expected-zero-network", expected["network_requests"] == 0, "先验要求零网络请求")


def check_variants(package: Path, expected: dict, report: Report) -> None:
    for entry in expected["rejections"]:
        path = package / entry["variant"]
        if not path.is_file():
            report.record(f"variant:{entry['variant']}", False, "缺少变体声明")
            continue
        try:
            declaration = strict_load(path, PROJECT_FIELDS, entry["variant"], canonical_required=False)
        except ValueError as error:
            report.record(f"variant:{entry['variant']}", False, str(error))
            continue
        code, detail = structured_failure(declaration)
        report.record(
            f"variant:{entry['variant']}",
            code == entry["code"],
            f"期望 {entry['code']}，重放得到 {code or '无'}（{detail}）",
        )


def check_output(output: Path, replay_result: dict, report: Report) -> None:
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == OUTPUT_ENTRIES, f"输出条目 {entries}")
    for name in OUTPUT_ENTRIES:
        report.record(f"output-present:{name}", (output / name).is_file(), f"缺少 {name}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    if missing:
        # A missing file would make every later comparison read bytes that are not
        # there; the presence checks above already name what is absent, so stop
        # instead of turning a missing output into a traceback.
        report.record("output-complete", False, f"缺少 {missing}")
        return
    manifest_path = output / OUTPUT_MANIFEST
    if not manifest_path.is_file():
        report.record("output-manifest-strict", False, "缺少输出清单")
        return
    try:
        manifest = strict_load(manifest_path, OUTPUT_MANIFEST_FIELDS, "输出清单")
    except ValueError as error:
        report.record("output-manifest-strict", False, str(error))
        return
    report.record("output-manifest-strict", True)
    report.record(
        "output-manifest-identity",
        manifest["policy"] == POLICY
        and manifest["producer"] == OUTPUT_PRODUCER
        and manifest["schema_version"] == SCHEMA_VERSION
        and manifest["case_id"] == replay_result["case_id"],
        "输出清单的身份字段不符",
    )
    report.record(
        "output-manifest-created-at",
        "created_at" not in manifest,
        "输出清单不得带时间戳",
    )
    for name, key in (("plan.json", "plan_sha256"), ("report.json", "report_sha256"), ("report.md", "markdown_sha256")):
        target = output / name
        report.record(
            f"output-digest:{name}",
            target.is_file() and sha256(read_bytes(target)) == manifest[key],
            f"{name} 的摘要与输出清单不符",
        )
    report.record(
        "output-manifest-counts",
        manifest["counts"] == replay_result["counts"],
        "输出清单的计数与重放不符",
    )

    try:
        plan = strict_load(output / PLAN, PLAN_FIELDS, "plan.json")
    except ValueError as error:
        report.record("plan-strict", False, str(error))
        return
    report.record("plan-strict", True)
    report.record("plan-identity", plan["case_id"] == replay_result["case_id"] and plan["plan_version"] == PLAN_VERSION, "plan.json 身份字段不符")
    for entry in plan["teaching_order"]:
        report.record("plan-order-fields", set(entry) == set(ORDER_FIELDS), f"教学顺序字段 {sorted(entry)}")
    for folder in plan["folders"]:
        report.record("plan-folder-fields", set(folder) == set(FOLDER_FIELDS), f"文件夹字段 {sorted(folder)}")
    for note in plan["notes"]:
        report.record("plan-note-fields", set(note) == set(NOTE_FIELDS), f"笔记字段 {sorted(note)}")
        for link in note["links"]:
            report.record("plan-link-fields", set(link) == set(LINK_FIELDS), f"链接字段 {sorted(link)}")
    for atom in plan["atoms"]:
        report.record("plan-atom-fields", set(atom) == set(ATOM_FIELDS), f"原子字段 {sorted(atom)}")
    report.record("plan-teaching-order", plan["teaching_order"] == replay_result["teaching_order"], "plan.json 的教学顺序与重放不符")
    report.record("plan-folders", plan["folders"] == replay_result["folders"], "plan.json 的文件夹与重放不符")
    report.record("plan-notes", plan["notes"] == replay_result["notes"], "plan.json 的笔记与重放不符")
    report.record("plan-atoms", plan["atoms"] == replay_result["atoms"], "plan.json 的原子与重放不符")

    try:
        result = strict_load(output / REPORT, REPORT_FIELDS, "report.json")
    except ValueError as error:
        report.record("report-strict", False, str(error))
        return
    report.record("report-strict", True)
    report.record("report-counts", result["counts"] == replay_result["counts"], "report.json 计数与重放不符")
    report.record("report-chains", result["chains"] == replay_result["chains"], "report.json 反应链与重放不符")
    projected = [{"kind": item["kind"], "subject": item["subject"]} for item in result["warnings"]]
    expected_warnings = [{"kind": item["kind"], "subject": item["subject"]} for item in replay_result["warnings"]]
    report.record("report-warnings", projected == expected_warnings, "report.json 提醒与重放不符")
    report.record(
        "report-zero-calls",
        result["model_calls"] == 0 and result["network_requests"] == 0,
        "报告必须记录零模型调用与零网络请求",
    )

    markdown = (output / MARKDOWN).read_text(encoding="utf-8")
    for heading in ("## 教学顺序", "## 阅读目标与位置", "## 反应链", "## 提醒", "## 边界"):
        report.record(f"markdown-section:{heading}", heading in markdown, f"缺少 {heading}")
    report.record(
        "markdown-boundary",
        "不生成知识" in markdown and "模型调用与网络请求 0 次" in markdown,
        "缺少边界声明",
    )


def freeze(package: Path) -> int:
    files = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        relation = item.relative_to(package).as_posix()
        files[relation] = {"sha256": sha256(read_bytes(item)), "byte_length": item.stat().st_size}
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(canonical(payload) + "\n", encoding="utf-8")
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("root", nargs="?", default=None)
    parser.add_argument("--output", default=None)
    parser.add_argument("--package", default=None)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args(argv)

    package = Path(args.package).resolve() if args.package else None
    if args.freeze:
        if package is None:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        return freeze(package)

    report = Report()
    declaration = None
    replay_result = None
    if package is not None:
        if not (package / PROJECT).is_file():
            print(f"{package} 不是规划包（缺少 project.json）。", file=sys.stderr)
            return 2
        declaration = load_project(package)
        expected = strict_load(package / "expected.json", tuple(expected_fields()), "规划期望", canonical_required=False)
        try:
            replay_result = replay(declaration)
        except PlanError as error:
            report.record("replay", False, f"{error.code}：{error.detail}")
            replay_result = None
        if replay_result is not None:
            check_expected(replay_result, declaration, expected, report)
        check_variants(package, expected, report)
        check_frozen(package, report)

    if args.root is None and package is None:
        print("至少要给一个规划根或 --package。", file=sys.stderr)
        return 2

    if args.root is not None:
        root = Path(args.root).resolve()
        if not root.is_dir():
            print(f"{root} 不是目录。", file=sys.stderr)
            return 2
        check_layout(root, report)
        manifest = check_manifest(root, report)
        if declaration is None:
            try:
                declaration = strict_load(root / CONTROL / PROJECT, PROJECT_FIELDS, "工程声明", canonical_required=False)
            except ValueError as error:
                report.record("root-project", False, str(error))
                declaration = None
        if declaration is not None:
            check_inputs(root, manifest, declaration, report)
            try:
                local = replay(declaration)
            except PlanError as error:
                report.record("root-replay", False, f"{error.code}：{error.detail}")
                local = None
            if local is not None:
                report.record("root-replay", True)
                if args.output is None:
                    pass
        if args.output is not None:
            output = Path(args.output).resolve()
            if not output.is_dir():
                print(f"{output} 不是目录。", file=sys.stderr)
                return 2
            if declaration is not None:
                try:
                    local_replay = replay(declaration)
                except PlanError as error:
                    report.record("output-replay", False, f"{error.code}：{error.detail}")
                    local_replay = None
                if local_replay is not None:
                    check_output(output, local_replay, report)

    if args.output is not None and package is None and args.root is None:
        report.record("output-needs-package", False, "只给输出根时无法复算")

    report.dump(args.json)
    return 0 if not report.failures else 1


def expected_fields() -> list[str]:
    return ["policy", "plan_version", "case_id", "teaching_order", "counts", "folders", "notes", "atoms", "chains", "warnings", "rejections", "determinism", "model_calls", "network_requests", "boundaries", "derivation"]


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
