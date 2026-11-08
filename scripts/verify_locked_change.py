#!/usr/bin/env python3
"""Read-only auditor for the locked-change planner.

Independent re-implementation: it never imports the Rust product and never runs
the CLI to decide anything.

Usage: <root> [--output DIR] [--package DIR] [--json] [--freeze]
Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "locked-change-v1"
APP_PRODUCER = "noetome-change"
OUTPUT_PRODUCER = "locked_change_output"
PLAN_VERSION = 1
SCHEMA_VERSION = 1

LOCK_DIRECTORY = "directory"
LOCK_FILE = "file"
LOCK_SECTION = "section"
LOCK_ORDER = "order"
LOCK_CANONICAL = "canonical"
LOCKS = (LOCK_DIRECTORY, LOCK_FILE, LOCK_SECTION, LOCK_ORDER, LOCK_CANONICAL)

FINDING_LOCK = "lock_conflict"
FINDING_MISSED = "missed_dependency"
FINDING_UNEXPECTED = "unexpected_touch"

REFACTOR_MOVE = "move"
REFACTOR_SPLIT = "split"
REFACTOR_MERGE = "merge"

CODE_ARGUMENT = "CHANGE_ARGUMENT"
CODE_INTEGRITY = "CHANGE_INTEGRITY"
CODE_UNSAFE = "CHANGE_UNSAFE_REFACTOR"

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
PROJECT = "control/project.json"
CHANGES = "changes.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("inputs.json", "manifest.json", "project.json")
OUTPUT_ENTRIES = ("changes.json", "manifest.json", "report.json", "report.md")

PROJECT_FIELDS = (
    "policy", "schema_version", "producer", "case_id", "description",
    "notes", "changes", "locks", "refactors", "limits",
)
MANIFEST_FIELDS = (
    "policy", "producer", "schema_version", "case_id", "root", "root_identity",
    "parent_identity", "control_identity", "manifest_identity", "inputs_identity",
    "project_sha256", "inputs_sha256", "created_at",
)
INPUTS_FIELDS = (
    "policy", "schema_version", "case_id", "project_sha256",
    "notes", "changes", "locks", "refactors", "unchanged",
)
CHANGE_FILE_FIELDS = (
    "policy", "plan_version", "case_id", "applied", "candidates", "refactors", "unchanged",
)
REPORT_FIELDS = (
    "policy", "schema_version", "case_id", "description", "verdict", "counts",
    "findings", "model_calls", "network_requests",
)
COUNT_FIELDS = (
    "notes", "changes", "locks", "applied", "candidates",
    "refactors", "reference_changes", "unchanged", "findings",
    "model_calls", "network_requests",
)
OUTPUT_MANIFEST_FIELDS = (
    "policy", "producer", "schema_version", "case_id", "source_root",
    "source_inputs_sha256", "changes_sha256", "report_sha256", "markdown_sha256",
    "verdict", "counts",
)


class StrictError(ValueError):
    pass


class Report:
    def __init__(self) -> None:
        self.checks = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append({"name": name, "ok": bool(ok), "detail": detail})

    @property
    def failures(self):
        return [item for item in self.checks if not item["ok"]]

    def dump(self, as_json: bool) -> None:
        if as_json:
            print(json.dumps(
                {"checks": self.checks, "failures": len(self.failures)},
                ensure_ascii=False, indent=2,
            ))
            return
        for item in self.failures:
            print(f"[失败] {item['name']}：{item['detail']}")
        print(f"合计 {len(self.checks)} 项检查：失败 {len(self.failures)}、提示 0")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    try:
        return path.read_bytes()
    except OSError as error:
        raise StrictError(f"{path} 无法读取：{error}") from error


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(path: Path, fields, label: str, canonical_required: bool = True):
    text = read_bytes(path).decode("utf-8")
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise StrictError(f"{label} 不是合法 JSON：{error}") from error
    if not isinstance(value, dict):
        raise StrictError(f"{label} 必须是对象。")
    if set(value) != set(fields):
        raise StrictError(f"{label} 字段不符：多 {sorted(set(value) - set(fields))}、少 {sorted(set(fields) - set(value))}")
    if canonical_required:
        encoded = canonical(value)
        if text not in (encoded, encoded + "\n"):
            raise StrictError(f"{label} 不是规范编码。")
    return value


def under(path: str, area: str) -> bool:
    return path == area or path.startswith(area + "/")


def same_shape(shapes) -> bool:
    return len({tuple(shape) for shape in shapes}) == 1


def locality(declaration, change, touched):
    findings = []
    declared = set(change["notes"])
    for note in declaration["notes"]:
        shares = bool(set(note["concepts"]) & touched)
        listed = note["note_id"] in declared
        if shares and not listed:
            findings.append({
                "kind": FINDING_MISSED, "subject": change["change_id"],
                "detail": f"{note['note_id']} 引用了被触动的概念，但没有出现在这次变更的受影响清单里。",
            })
        if listed and not shares and note["topic_id"] != change["topic_id"]:
            findings.append({
                "kind": FINDING_UNEXPECTED, "subject": change["change_id"],
                "detail": f"{note['note_id']} 既不共享任何概念，也不属于主题 {change['topic_id']}：它不该出现在受影响清单里。",
            })
    return findings


def lock_conflicts(declaration, note, proposed_path, proposed_sections):
    conflicts = []
    for lock in declaration["locks"]:
        kind = lock["kind"]
        hit = None
        if kind == LOCK_FILE and lock.get("path") == note["path"] and proposed_path != note["path"]:
            hit = f"文件锁 {lock['lock_id']}：{note['path']} 不允许改名。"
        elif kind == LOCK_DIRECTORY and lock.get("path") and under(note["path"], lock["path"]) and not under(proposed_path, lock["path"]):
            hit = f"目录锁 {lock['lock_id']}：{note['path']} 必须留在 {lock['path']} 里。"
        elif kind == LOCK_SECTION and lock.get("note_id") == note["note_id"]:
            section = lock.get("section")
            current = note["sections"].index(section) if section in note["sections"] else -1
            wanted = proposed_sections.index(section) if section in proposed_sections else -1
            if current != wanted:
                hit = f"章节锁 {lock['lock_id']}：章节「{section}」必须停在第 {current} 个位置。"
        elif kind == LOCK_ORDER and lock.get("note_id") == note["note_id"] and lock.get("position") != note["position"]:
            hit = f"顺序锁 {lock['lock_id']}：位置必须是 {lock['position']}。"
        elif kind == LOCK_CANONICAL and lock.get("concept_id") in note["concepts"] and proposed_path != note["path"]:
            hit = f"规范位置锁 {lock['lock_id']}：{lock['concept_id']} 的完整解释必须留在 {note['path']}。"
        if hit:
            conflicts.append({"lock_id": lock["lock_id"], "kind": kind, "detail": hit})
    return conflicts


class PlanError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def check_refactor(declaration, refactor, notes):
    note = notes.get(refactor["note_id"])
    if note is None:
        raise PlanError(CODE_ARGUMENT, f"重构 {refactor['refactor_id']} 指向未知笔记。")
    if not refactor.get("references"):
        raise PlanError(CODE_ARGUMENT, f"重构 {refactor['refactor_id']} 没有任何引用变化。")
    targets = refactor["targets"]
    if not targets:
        raise PlanError(CODE_ARGUMENT, f"重构 {refactor['refactor_id']} 没有目标。")
    if refactor["kind"] == REFACTOR_SPLIT:
        covered = {}
        for atom in refactor.get("atoms", []):
            if atom["note_id"] not in targets:
                raise PlanError(CODE_UNSAFE, f"拆分目标 {atom['note_id']} 不在目标清单里：原子必须完整划分到目标。")
            covered.setdefault(atom["note_id"], 0)
            covered[atom["note_id"]] += 1
        empty = [target for target in targets if not covered.get(target)]
        if empty:
            raise PlanError(CODE_UNSAFE, f"拆分的目标 {empty[0]} 没有分到任何原子：空壳不是拆分。")
        seen = {}
        for atom in refactor.get("atoms", []):
            if atom["atom_id"] in seen:
                raise PlanError(CODE_UNSAFE, f"原子 {atom['atom_id']} 被分到两个目标。")
            seen[atom["atom_id"]] = True
    for target in targets:
        if target == note["path"]:
            continue
        for lock in declaration["locks"]:
            kind = lock["kind"]
            blocked = (
                (kind == LOCK_FILE and lock.get("path") == note["path"])
                or (kind == LOCK_DIRECTORY and lock.get("path") and under(note["path"], lock["path"]) and not under(target, lock["path"]))
                or (kind == LOCK_CANONICAL and lock.get("concept_id") in note["concepts"])
            )
            if blocked:
                raise PlanError(
                    CODE_UNSAFE,
                    f"重构 {refactor['refactor_id']} 想把 {note['path']} 移到 {target}，但锁定 {lock['lock_id']} 不允许；已保留旧位置，未生成任何变更。",
                )


def replay(declaration):
    notes = {note["note_id"]: note for note in declaration["notes"]}
    applied, candidates, findings = [], [], []
    changed = set()
    refactors = []

    for change in declaration["changes"]:
        note = notes.get(change["notes"][0]) if change["notes"] else None
        touched = set(change["concepts"])
        findings.extend(locality(declaration, change, touched))
        if note is None:
            continue
        if note["note_id"] in changed:
            continue
        proposed_path = change.get("proposed_path") or note["path"]
        proposed_sections = change.get("proposed_sections") or note["sections"]
        conflicts = lock_conflicts(declaration, note, proposed_path, proposed_sections)
        if conflicts:
            candidates.append({
                "change_id": change["change_id"], "note_id": note["note_id"],
                "current_path": note["path"], "proposed_path": proposed_path,
                "conflicts": conflicts,
            })
            findings.append({
                "kind": FINDING_LOCK, "subject": change["change_id"],
                "detail": f"{note['note_id']} 的提案与 {len(conflicts)} 条锁冲突，已转候选并保留原结构。",
            })
        else:
            applied.append({
                "change_id": change["change_id"], "note_id": note["note_id"],
                "path": proposed_path, "reason": change["kind"],
            })
        changed.add(note["note_id"])

    for refactor in declaration["refactors"]:
        check_refactor(declaration, refactor, notes)
        refactors.append({
            "refactor_id": refactor["refactor_id"], "kind": refactor["kind"],
            "note_id": refactor["note_id"], "targets": list(refactor["targets"]),
            "references": len(refactor.get("references", [])), "atoms": len(refactor.get("atoms", [])),
        })
        changed.add(refactor["note_id"])

    unchanged = [
        {
            "note_id": note["note_id"], "path": note["path"],
            "fingerprint": note["fingerprint"],
        }
        for note in declaration["notes"]
        if note["note_id"] not in changed
    ]
    counts = {
        "notes": len(declaration["notes"]),
        "changes": len(declaration["changes"]),
        "locks": len(declaration["locks"]),
        "applied": len(applied),
        "candidates": len(candidates),
        "refactors": len(refactors),
        "reference_changes": sum(item["references"] for item in refactors),
        "unchanged": len(unchanged),
        "findings": len(findings),
        "model_calls": 0,
        "network_requests": 0,
    }
    return {
        "applied": applied, "candidates": candidates, "refactors": refactors,
        "unchanged": unchanged, "findings": findings, "counts": counts,
        "verdict": "pass" if not findings else "findings",
    }


def load_package(package: Path, name: str = "project.json"):
    return strict_load(package / name, PROJECT_FIELDS, f"{name} 声明", canonical_required=False)


def structured_failure(declaration) -> str:
    try:
        replay(declaration)
    except PlanError as error:
        return error.code
    return ""


def check_layout(report, root: Path):
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control-present", False, "缺少 control/")
        return
    names = sorted(item.name for item in control.iterdir())
    report.record("layout-control-entries", names == list(CONTROL_ENTRIES), f"control 条目 {names}")
    links = [str(path) for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-symlinks", not links, f"符号链接 {links}")


def check_manifest(report, root: Path):
    path = root / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 manifest.json")
        return None
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "清单")
    except StrictError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-policy", manifest["policy"] == POLICY, manifest["policy"])
    report.record("manifest-producer", manifest["producer"] == APP_PRODUCER, manifest["producer"])
    report.record("manifest-schema", manifest["schema_version"] == SCHEMA_VERSION, str(manifest["schema_version"]))
    report.record("manifest-root", Path(manifest["root"]).resolve() == root.resolve(), manifest["root"])
    for key, relative in (("project_sha256", PROJECT), ("inputs_sha256", INPUTS)):
        actual = sha256(read_bytes(root / relative))
        report.record(f"manifest-digest:{key}", actual == manifest[key], f"{actual} != {manifest[key]}")
    return manifest


def check_inputs(report, root: Path, declaration):
    try:
        inputs = strict_load(root / INPUTS, INPUTS_FIELDS, "inputs.json")
    except StrictError as error:
        report.record("inputs-strict", False, str(error))
        return
    report.record("inputs-strict", True)
    report.record("inputs-policy", inputs["policy"] == POLICY, inputs["policy"])
    report.record("inputs-case", inputs["case_id"] == declaration["case_id"], inputs["case_id"])
    for key in ("notes", "changes", "locks", "refactors"):
        report.record(f"inputs-count:{key}", inputs[key] == len(declaration[key]), f"{inputs[key]} != {len(declaration[key])}")
    expected = replay(declaration)["unchanged"]
    report.record("inputs-unchanged", inputs["unchanged"] == expected, "冻结的未受影响快照与重算不一致")


def check_frozen(report, package: Path):
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, "缺少 frozen-sha256.json")
        return
    frozen = json.loads(path.read_text(encoding="utf-8"))
    actual = {}
    for item in sorted(package.rglob("*")):
        if item.is_file() and item.name != FROZEN:
            actual[item.relative_to(package).as_posix()] = {
                "sha256": sha256(item.read_bytes()), "byte_length": item.stat().st_size,
            }
    report.record("frozen-members", sorted(actual) == sorted(frozen["files"]), f"{sorted(actual)}")
    for name, value in actual.items():
        report.record(f"frozen-digest:{name}", frozen["files"].get(name) == value, name)


def check_expected(report, package: Path, declaration):
    expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
    result = replay(declaration)
    report.record("expected-case", expected["case_id"] == declaration["case_id"], expected["case_id"])
    report.record("expected-counts", result["counts"] == expected["counts"], f"{result['counts']} != {expected['counts']}")
    report.record("expected-verdict", result["verdict"] == expected["verdict"], result["verdict"])
    report.record("expected-applied", result["applied"] == expected["applied"], "已应用清单不符")
    report.record("expected-candidates", result["candidates"] == expected["candidates"], "候选清单不符")
    report.record("expected-unchanged", result["unchanged"] == expected["unchanged"], "未受影响清单不符")
    report.record(
        "expected-refactors",
        [(item["refactor_id"], item["kind"], item["note_id"], item["targets"], item["references"], item["atoms"]) for item in result["refactors"]]
        == [(item["refactor_id"], item["kind"], item["note_id"], item["targets"], item["references"], item["atoms"]) for item in expected["refactors"]],
        "重构清单不符",
    )
    report.record(
        "expected-findings",
        [item["kind"] for item in result["findings"]] == [item["kind"] for item in expected["findings"]],
        f"{[item['kind'] for item in result['findings']]}",
    )
    for item in expected["variants"]:
        variant = load_package(package, item["project"])
        if item["kind"] == "rejection":
            code = structured_failure(variant)
            report.record(f"variant:{item['project']}", code == item["code"], code)
        else:
            variant_result = replay(variant)
            ok = variant_result["counts"]["findings"] == item["findings"]
            if "applied" in item:
                ok = ok and variant_result["counts"]["applied"] == item["applied"]
            if "candidates" in item:
                ok = ok and variant_result["counts"]["candidates"] == item["candidates"]
            if "kinds" in item:
                ok = ok and [finding["kind"] for finding in variant_result["findings"]] == item["kinds"]
            report.record(f"variant:{item['project']}", ok, f"{variant_result['counts']}")


def check_output(report, output: Path, declaration):
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"输出条目 {entries}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    report.record("output-complete", not missing, f"缺少 {missing}")
    if missing:
        return
    try:
        changes = strict_load(output / CHANGES, CHANGE_FILE_FIELDS, "changes.json")
        result = strict_load(output / REPORT, REPORT_FIELDS, "report.json")
        manifest = strict_load(output / OUTPUT_MANIFEST, OUTPUT_MANIFEST_FIELDS, "输出清单")
    except StrictError as error:
        report.record("output-strict", False, str(error))
        return
    report.record("output-strict", True)
    expected = replay(declaration)
    report.record("output-counts", result["counts"] == expected["counts"], f"{result['counts']}")
    report.record("output-verdict", result["verdict"] == expected["verdict"], result["verdict"])
    report.record("output-applied", changes["applied"] == expected["applied"], "已应用清单不符")
    report.record("output-candidates", changes["candidates"] == expected["candidates"], "候选清单不符")
    report.record("output-unchanged", changes["unchanged"] == expected["unchanged"], "未受影响清单不符")
    report.record(
        "output-findings",
        [(item["kind"], item["subject"]) for item in result["findings"]]
        == [(item["kind"], item["subject"]) for item in expected["findings"]],
        "缺口清单不符",
    )
    report.record("output-manifest-counts", manifest["counts"] == expected["counts"], "输出清单计数不符")
    report.record("output-manifest-verdict", manifest["verdict"] == expected["verdict"], manifest["verdict"])
    for key, relative in (("changes_sha256", CHANGES), ("report_sha256", REPORT), ("markdown_sha256", MARKDOWN)):
        report.record(f"output-digest:{key}", manifest[key] == sha256(read_bytes(output / relative)), key)
    markdown = (output / MARKDOWN).read_text(encoding="utf-8")
    for heading in ("## 变更规模", "## 已应用的变更", "## 锁定冲突", "## 重构与引用变化", "## 未受影响的笔记", "## 缺口", "## 边界"):
        report.record(f"markdown-section:{heading}", heading in markdown, heading)
    report.record("markdown-zero-calls", "模型调用与网络请求 0 次" in markdown, "缺少零调用声明")


def freeze(package: Path):
    files = {}
    for item in sorted(package.rglob("*")):
        if item.is_file() and item.name != FROZEN:
            files[item.relative_to(package).as_posix()] = {
                "sha256": sha256(item.read_bytes()), "byte_length": item.stat().st_size,
            }
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"已冻结 {len(files)} 个成员")


def main(argv) -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("root", nargs="?")
    parser.add_argument("--output")
    parser.add_argument("--package")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args(argv)

    package = Path(args.package) if args.package else None
    if args.freeze:
        if package is None:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        freeze(package)
        return 0

    report = Report()
    root = Path(args.root) if args.root else None
    if root is None and package is None:
        print("至少要给一个变更根或 --package。", file=sys.stderr)
        return 2
    if root is not None and not root.is_dir():
        print(f"{root} 不是目录。", file=sys.stderr)
        return 2

    declaration = None
    if package is not None:
        try:
            declaration = load_package(package)
        except StrictError as error:
            report.record("package-declaration", False, str(error))
        if declaration is not None:
            check_frozen(report, package)
            check_expected(report, package, declaration)

    if root is not None:
        check_layout(report, root)
        manifest = check_manifest(report, root)
        if declaration is None and (root / PROJECT).is_file():
            try:
                declaration = strict_load(root / PROJECT, PROJECT_FIELDS, "project.json", canonical_required=False)
            except StrictError as error:
                report.record("root-declaration", False, str(error))
        if declaration is not None:
            check_inputs(report, root, declaration)
        if args.output:
            output = Path(args.output)
            if not output.is_dir():
                print(f"{output} 不是目录。", file=sys.stderr)
                return 2
            if declaration is not None:
                check_output(report, output, declaration)

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
