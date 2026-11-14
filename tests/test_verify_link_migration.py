"""Tests for the independent link-migration auditor (B11-T01).

The auditor never imports the product and never runs the CLI to decide anything.
These tests build a migration root and an output root with the auditor's own
replay, so a green run proves the checks read the documented layout instead of
mirroring whatever the product happens to write.
"""

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from symlink_test_support import create_symlink_or_skip

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_link_migration.py"
PACKAGE = ROOT / "fixtures" / "link-migration-v1"
FIXED_AT = "2026-11-08T00:00:00Z"


def load_auditor(module_name: str = "verify_link_migration"):
    spec = importlib.util.spec_from_file_location(module_name, SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def fake_identity(index: int) -> dict:
    return {"platform": "unix", "device": 1, "inode": index}


def write_json(path: Path, value: object, trailing_newline: bool = True) -> None:
    payload = V.canonical(value) + ("\n" if trailing_newline else "")
    path.write_bytes(payload.encode("utf-8"))


def replay_or_fail(package: Path = PACKAGE) -> dict:
    replay, code, detail = V.replay_package(package)
    assert replay is not None, f"{code}：{detail}"
    return replay


def build_root(directory: Path, package: Path = PACKAGE) -> tuple[Path, dict]:
    """Build a migration root from the auditor's replay of the frozen package."""
    replay = replay_or_fail(package)
    plan = replay["plan"]
    case_id = plan["case_id"]
    root = directory / "root"
    control = root / "control"
    before_dir = control / "before"
    after_dir = control / "after"
    before_dir.mkdir(parents=True)
    after_dir.mkdir(parents=True)

    views = [
        {"side": V.SIDE_BEFORE, **item} for item in replay["sides"][V.SIDE_BEFORE]
    ] + [{"side": V.SIDE_AFTER, **item} for item in replay["sides"][V.SIDE_AFTER]]
    for view in views:
        folder = before_dir if view["side"] == V.SIDE_BEFORE else after_dir
        (folder / f"{view['note']['note_id']}{V.TEXT_SUFFIX}").write_bytes(view["raw"])

    (control / "plan.json").write_bytes((package / "plan.json").read_bytes())
    (control / "objects.json").write_bytes((package / plan["objects"]).read_bytes())

    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "before_dir": plan["before_dir"],
        "after_dir": plan["after_dir"],
        "notes": [
            {
                "side": view["side"],
                "note_id": view["note"]["note_id"],
                "path": view["note"]["path"],
                "title": view["note"]["title"],
                "counterpart": view["note"].get("counterpart"),
                "change": view["note"].get("change"),
                "raw_sha256": V.sha256(view["raw"]),
                "byte_length": len(view["raw"]),
                "normalized_sha256": V.sha256(view["raw"]),
                "span_count": 0,
            }
            for view in views
        ],
    }
    write_json(control / "inputs.json", inputs, trailing_newline=False)

    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "root": str(root.resolve()),
        "root_identity": fake_identity(1),
        "parent_identity": fake_identity(2),
        "control_identity": fake_identity(3),
        "manifest_identity": fake_identity(4),
        "inputs_identity": fake_identity(5),
        "plan_sha256": V.sha256((control / "plan.json").read_bytes()),
        "objects_sha256": V.sha256((control / "objects.json").read_bytes()),
        "inputs_sha256": V.sha256((control / "inputs.json").read_bytes()),
        "documents": [
            {
                "side": view["side"],
                "note_id": view["note"]["note_id"],
                "path": view["note"]["path"],
                "normalized_sha256": V.sha256(view["raw"]),
                "byte_length": len(view["raw"]),
            }
            for view in views
        ],
        "created_at": FIXED_AT,
    }
    write_json(control / "manifest.json", manifest, trailing_newline=False)
    return root, replay


def report_notes(replay: dict) -> list[dict]:
    """重放「每个 after 笔记有没有守住身份」的核对结论。"""
    plan = replay["plan"]
    by_id = {spec["note_id"]: spec for spec in plan["before"]}
    views = {
        item["note"]["note_id"]: item
        for item in replay["sides"][V.SIDE_AFTER]
    }
    out: list[dict] = []
    for spec in plan["after"]:
        view = views[spec["note_id"]]
        change = spec.get("change")
        bytes_equal = None
        stripped_equal = None
        promotion_satisfied = None
        if change in (V.CHANGE_UNCHANGED, V.CHANGE_LINKS_ONLY, V.CHANGE_PROMOTED):
            before_view = next(
                item
                for item in replay["sides"][V.SIDE_BEFORE]
                if item["note"]["note_id"] == spec.get("counterpart")
            )
            if change == V.CHANGE_UNCHANGED:
                bytes_equal = before_view["text"] == view["text"]
                stripped_equal = V.strip_links(before_view["text"]) == V.strip_links(view["text"])
            elif change == V.CHANGE_LINKS_ONLY:
                bytes_equal = before_view["text"] == view["text"]
                stripped_equal = V.strip_links(before_view["text"]) == V.strip_links(view["text"])
            else:
                bytes_equal = before_view["text"] == view["text"]
                stripped_equal = False
                promotion_satisfied = spec.get("counterpart") in by_id
        out.append(
            {
                "note_id": spec["note_id"],
                "path": spec["path"],
                "side": V.SIDE_AFTER,
                "counterpart": spec.get("counterpart"),
                "change": change,
                "bytes_equal": bytes_equal,
                "stripped_equal": stripped_equal,
                "promotion_satisfied": promotion_satisfied,
            }
        )
    return out


def report_markdown(replay: dict) -> str:
    prior = V.replay_expected(replay)
    counts = prior["counts"]
    lines = [
        f"# 链接迁移报告（{replay['plan']['case_id']}）",
        "",
        replay["plan"]["description"],
        "",
        "## 计数",
        "",
        f"- 笔记：迁移前 {counts['notes_before']} 篇、迁移后 {counts['notes_after']} 篇",
        f"- 引用：迁移前 {counts['links_before']} 条、迁移后 {counts['links_after']} 条；断链 {counts['broken_links']} 条",
        f"- 迁移后规范位置 {counts['canonical_links_after']} 条、跳转位置 {counts['redirect_links_after']} 条",
        f"- 知识对象 {counts['objects']} 个：被引用 {counts['objects_referenced']} 个、未被引用 {counts['objects_unreferenced']} 个、身份违规 {counts['identity_violations']} 个",
        "",
        "## 知识对象身份",
        "",
        "| 对象 | 迁移前位置 | 迁移后位置 | 迁移前引用 | 迁移后引用 |",
        "| --- | --- | --- | --- | --- |",
    ]
    for item in prior["placements"]:
        lines.append(
            f"| {item['object_id']} | {item['before_target']} | {item['after_target']} | "
            f"{item['links_before']} | {item['links_after']} |"
        )
    lines += ["", "## 迁移后引用", "", "| 笔记 | 行 | 链接 | 对象 | 匹配 |", "| --- | --- | --- | --- | --- |"]
    for link in replay["after"]:
        raw = link["raw"].replace("|", "\\|")
        lines.append(
            f"| {link['path']} | {link['line']} | {raw} | {link['object_id']} | {link['matched']} |"
        )
    lines += ["", "## 笔记核对", "", "| 笔记 | 变更 | 逐字节相等 | 剥链接后相等 |", "| --- | --- | --- | --- |"]
    for note in report_notes(replay):
        mark = lambda value: "是" if value else ("否" if value is False else "—")  # noqa: E731
        lines.append(
            f"| {note['path']} | {note['change']} | {mark(note['bytes_equal'])} | "
            f"{mark(note['stripped_equal'])} |"
        )
    lines += [
        "",
        "## 边界",
        "",
        "- 迁移只核对与报告，不改写作者包。",
        "- 链接先解析到知识对象再比较身份，文件名只是位置。",
        "- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。",
        "",
    ]
    return "\n".join(lines)


def build_output(directory: Path, replay: dict) -> Path:
    """Write the output root a correct build produces."""
    prior = V.replay_expected(replay)
    case_id = replay["plan"]["case_id"]
    root = directory / "root"
    out = directory / "out"
    out.mkdir(parents=True)

    links = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "before": replay["before"],
        "after": replay["after"],
        "counts": {key: prior["counts"][key] for key in V.LINK_COUNT_FIELDS},
    }
    placements = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "objects": prior["placements"],
        "counts": {
            "objects": prior["counts"]["objects"],
            "objects_referenced": prior["counts"]["objects_referenced"],
            "objects_unreferenced": prior["counts"]["objects_unreferenced"],
            "identity_violations": prior["counts"]["identity_violations"],
            "redirects": sum(len(item["redirect_targets"]) for item in prior["placements"]),
        },
    }
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "description": replay["plan"]["description"],
        "counts": {key: prior["counts"][key] for key in V.COUNT_FIELDS},
        "notes": report_notes(replay),
        "model_calls": 0,
        "network_requests": 0,
    }
    write_json(out / V.LINKS, links)
    write_json(out / V.PLACEMENTS, placements)
    write_json(out / V.REPORT, report)
    (out / V.MARKDOWN).write_bytes(report_markdown(replay).encode("utf-8"))
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "source_root": str(root.resolve()),
        "source_inputs_sha256": V.sha256((root / V.INPUTS).read_bytes()),
        "links_sha256": V.sha256((out / V.LINKS).read_bytes()),
        "placements_sha256": V.sha256((out / V.PLACEMENTS).read_bytes()),
        "report_sha256": V.sha256((out / V.REPORT).read_bytes()),
        "markdown_sha256": V.sha256((out / V.MARKDOWN).read_bytes()),
        "counts": {key: prior["counts"][key] for key in V.COUNT_FIELDS},
    }
    write_json(out / V.OUTPUT_MANIFEST, manifest)
    return out


def audit(root: Path, output: Path | None = None, package: Path = PACKAGE):
    report = V.Report()
    V.check_layout(root, report)
    V.check_manifest(root, report)
    replay = replay_or_fail(package)
    replay["root"] = str(root.resolve())
    V.check_inputs(root, replay, report)
    V.check_expected(package, replay, report)
    V.check_rejections(package, report)
    V.check_frozen(package, report)
    if output is not None:
        V.check_output(output, replay, report)
    return report


def failed(report) -> set[str]:
    return {item["check"] for item in report.checks if not item["ok"]}


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="link-migration-audit-"))

    def path(self, name: str) -> Path:
        return self.directory / name

    def close(self) -> None:
        shutil.rmtree(self.directory, ignore_errors=True)


class SandboxCase(unittest.TestCase):
    """A fresh sandbox holding one built root and one built output root."""

    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)
        self.root, self.replay = build_root(self.sandbox.directory)
        self.out = build_output(self.sandbox.directory, self.replay)

    def read(self, path: Path) -> dict:
        return json.loads(path.read_bytes().decode("utf-8"))

    def write(self, path: Path, value: object, trailing_newline: bool = True) -> None:
        write_json(path, value, trailing_newline)

    def audit(self, *, output: bool = True, package: Path = PACKAGE):
        return audit(self.root, self.out if output else None, package)


class FrozenPackageTests(unittest.TestCase):
    def test_frozen_manifest_matches_members(self) -> None:
        report = V.Report()
        V.check_frozen(PACKAGE, report)
        self.assertEqual(failed(report), set())

    def test_tampered_member_byte_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        target = copy / "before" / "笔记" / "糖酵解.md"
        target.write_bytes(target.read_bytes() + b"\n")
        report = V.Report()
        V.check_frozen(copy, report)
        self.assertIn("frozen-digest:before/笔记/糖酵解.md", failed(report))

    def test_extra_member_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "before" / "笔记" / "extra.md").write_bytes(b"# extra\n")
        report = V.Report()
        V.check_frozen(copy, report)
        self.assertIn("frozen-members", failed(report))


class PackagePriorTests(unittest.TestCase):
    def test_the_frozen_prior_is_self_consistent(self) -> None:
        expected = json.loads((PACKAGE / "expected.json").read_bytes().decode("utf-8"))
        counts = expected["counts"]
        self.assertEqual(counts["links_before"], len(expected["before_links"]))
        self.assertEqual(counts["links_after"], len(expected["after_links"]))
        self.assertEqual(counts["objects"], len(expected["objects"]))
        self.assertEqual(counts["notes_after"], len(expected["notes"]))
        self.assertEqual(counts["broken_links"], 0)
        self.assertEqual(counts["identity_violations"], 0)
        self.assertEqual(counts["rewritten_targets"], 2)

    def test_package_mode_audit_is_green(self) -> None:
        report = V.Report()
        V.check_expected(PACKAGE, replay_or_fail(), report)
        V.check_rejections(PACKAGE, report)
        V.check_frozen(PACKAGE, report)
        self.assertEqual(failed(report), set())

    def test_a_count_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_bytes().decode("utf-8"))
        expected["counts"]["rewritten_targets"] = 3
        write_json(copy / "expected.json", expected)
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-count:rewritten_targets", failed(report))

    def test_a_broken_link_count_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_bytes().decode("utf-8"))
        expected["counts"]["broken_links"] = 1
        write_json(copy / "expected.json", expected)
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-count:broken_links", failed(report))

    def test_a_moved_target_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_bytes().decode("utf-8"))
        expected["objects"][1]["expected_target"] = "[[笔记/糖酵解.md#核心机制]]"
        write_json(copy / "expected.json", expected)
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-object:obj-glycolysis-mechanism:target", failed(report))

    def test_a_link_record_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_bytes().decode("utf-8"))
        expected["after_links"][3]["match"] = "canonical"
        write_json(copy / "expected.json", expected)
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-links:after:3:match", failed(report))

    def test_a_note_verdict_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_bytes().decode("utf-8"))
        expected["notes"][4]["bytes_equal"] = False
        write_json(copy / "expected.json", expected)
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-notes:4:bytes_equal", failed(report))


class RejectionTests(unittest.TestCase):
    def test_all_four_variants_are_rejected_with_their_own_code(self) -> None:
        report = V.Report()
        V.check_rejections(PACKAGE, report)
        self.assertEqual(failed(report), set())

    def test_the_baseline_package_is_not_rejected(self) -> None:
        plan = json.loads((PACKAGE / "plan.json").read_bytes().decode("utf-8"))
        code, detail = V.replay_plan(PACKAGE, plan, "plan.json")
        self.assertEqual(code, "")
        self.assertEqual(detail, "")

    def test_dropping_the_ambiguous_copy_makes_the_plan_valid_again(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "after" / "附录" / "糖酵解.md").unlink()
        plan = json.loads((copy / "plan.json").read_bytes().decode("utf-8"))
        plan["after"] = [spec for spec in plan["after"] if spec["note_id"] != "a-glycolysis-appendix"]
        write_json(copy / "plan.json", plan)
        code, detail = V.replay_plan(copy, plan, "plan.json")
        self.assertEqual(code, "", detail)

    def test_an_unescaped_alias_is_rejected(self) -> None:
        plan = json.loads((PACKAGE / "plan-unescaped-table.json").read_bytes().decode("utf-8"))
        code, _ = V.replay_plan(PACKAGE, plan, "plan-unescaped-table.json")
        self.assertEqual(code, "MIGRATE_UNESCAPED_ALIAS")

    def test_a_missing_declared_position_is_rejected(self) -> None:
        plan = json.loads((PACKAGE / "plan-unsafe.json").read_bytes().decode("utf-8"))
        code, _ = V.replay_plan(PACKAGE, plan, "plan-unsafe.json")
        self.assertEqual(code, "MIGRATE_OBJECT_MISSING")


class CleanRootTests(SandboxCase):
    def test_a_built_root_and_output_are_green(self) -> None:
        report = self.audit()
        self.assertEqual(failed(report), set())
        self.assertGreater(len(report.checks), 200)

    def test_the_root_alone_is_green(self) -> None:
        report = self.audit(output=False)
        self.assertEqual(failed(report), set())


class LayoutTests(SandboxCase):
    def test_an_extra_entry_at_the_top_level_is_detected(self) -> None:
        (self.root / "note.txt").write_bytes(b"x\n")
        self.assertIn("layout-root-entries", failed(self.audit(output=False)))

    def test_an_extra_entry_in_control_is_detected(self) -> None:
        (self.root / "control" / "extra.json").write_bytes(b"{}\n")
        self.assertIn("layout-control-entries", failed(self.audit(output=False)))

    def test_a_missing_side_directory_is_detected(self) -> None:
        shutil.rmtree(self.root / "control" / "after")
        self.assertIn("layout-after-dir", failed(self.audit(output=False)))

    def test_a_symbolic_link_anywhere_in_the_root_is_detected(self) -> None:
        target = self.root / "control" / "plan-link.json"
        create_symlink_or_skip(target, self.root / "control" / "plan.json")
        self.assertIn("layout-no-links", failed(self.audit(output=False)))


class ManifestTests(SandboxCase):
    def test_an_unknown_field_is_detected(self) -> None:
        path = self.root / V.MANIFEST
        value = self.read(path)
        value["extra"] = 1
        self.write(path, value, trailing_newline=False)
        self.assertIn("manifest-strict", failed(self.audit(output=False)))

    def test_pretty_json_is_detected(self) -> None:
        path = self.root / V.MANIFEST
        value = self.read(path)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2), newline="\n")
        self.assertIn("manifest-strict", failed(self.audit(output=False)))

    def test_a_drifted_root_is_detected(self) -> None:
        path = self.root / V.MANIFEST
        value = self.read(path)
        value["root"] = str(self.sandbox.path("elsewhere"))
        self.write(path, value, trailing_newline=False)
        self.assertIn("manifest-root", failed(self.audit(output=False)))

    def test_a_drifted_plan_digest_is_detected(self) -> None:
        path = self.root / V.MANIFEST
        value = self.read(path)
        value["plan_sha256"] = V.sha256(b"other")
        self.write(path, value, trailing_newline=False)
        self.assertIn("manifest-plan-digest", failed(self.audit(output=False)))

    def test_a_changed_frozen_body_is_detected(self) -> None:
        note = self.root / "control" / "after" / "a-gluconeogenesis.md"
        note.write_bytes(note.read_bytes() + b"\n")
        self.assertIn("manifest-after-digest:a-gluconeogenesis", failed(self.audit(output=False)))

    def test_a_missing_frozen_body_is_detected(self) -> None:
        (self.root / "control" / "before" / "b-ppp.md").unlink()
        self.assertIn("manifest-before-files", failed(self.audit(output=False)))

    def test_an_unknown_frozen_body_is_detected(self) -> None:
        (self.root / "control" / "before" / "b-extra.md").write_bytes(b"# extra\n")
        self.assertIn("manifest-before-unknown:b-extra.md", failed(self.audit(output=False)))


class InputsTests(SandboxCase):
    def test_a_drifted_case_id_is_detected(self) -> None:
        path = self.root / V.INPUTS
        value = self.read(path)
        value["case_id"] = "other-case"
        self.write(path, value, trailing_newline=False)
        self.assertIn("inputs-case", failed(self.audit(output=False)))

    def test_a_drifted_side_directory_is_detected(self) -> None:
        path = self.root / V.INPUTS
        value = self.read(path)
        value["before_dir"] = "elsewhere"
        self.write(path, value, trailing_newline=False)
        self.assertIn("inputs-dirs", failed(self.audit(output=False)))

    def test_a_missing_note_record_is_detected(self) -> None:
        path = self.root / V.INPUTS
        value = self.read(path)
        value["notes"] = value["notes"][:-1]
        self.write(path, value, trailing_newline=False)
        self.assertIn("inputs-notes", failed(self.audit(output=False)))

    def test_an_unknown_field_in_a_note_record_is_detected(self) -> None:
        path = self.root / V.INPUTS
        value = self.read(path)
        value["notes"][0]["extra"] = 1
        self.write(path, value, trailing_newline=False)
        failed_names = failed(self.audit(output=False))
        self.assertTrue(any(name.startswith("inputs-fields:") for name in failed_names), failed_names)

    def test_a_tampered_inputs_digest_is_detected(self) -> None:
        path = self.root / V.INPUTS
        value = self.read(path)
        value["notes"][0]["byte_length"] = 999
        self.write(path, value, trailing_newline=False)
        self.assertIn("manifest-inputs-digest", failed(self.audit(output=False)))


class OutputTests(SandboxCase):
    def test_a_drifted_link_record_is_detected(self) -> None:
        path = self.out / V.LINKS
        value = self.read(path)
        value["after"][0]["line"] = 99
        self.write(path, value)
        self.assertIn("links-after", failed(self.audit()))

    def test_a_drifted_link_count_is_detected(self) -> None:
        path = self.out / V.LINKS
        value = self.read(path)
        value["counts"]["links_after"] = 9
        self.write(path, value)
        self.assertIn("links-counts", failed(self.audit()))

    def test_a_broken_link_is_detected(self) -> None:
        path = self.out / V.LINKS
        value = self.read(path)
        value["after"][0]["status"] = "target_missing"
        value["counts"]["broken_links"] = 1
        self.write(path, value)
        self.assertIn("links-no-broken", failed(self.audit()))

    def test_a_drifted_object_placement_is_detected(self) -> None:
        path = self.out / V.PLACEMENTS
        value = self.read(path)
        value["objects"][0]["after_target"] = "[[笔记/别处.md]]"
        self.write(path, value)
        self.assertIn("placements-objects", failed(self.audit()))

    def test_a_drifted_identity_count_is_detected(self) -> None:
        path = self.out / V.PLACEMENTS
        value = self.read(path)
        value["counts"]["identity_violations"] = 1
        self.write(path, value)
        self.assertIn("placements-counts", failed(self.audit()))

    def test_a_note_verdict_on_the_before_side_is_detected(self) -> None:
        path = self.out / V.REPORT
        value = self.read(path)
        value["notes"][0]["side"] = V.SIDE_BEFORE
        self.write(path, value)
        self.assertIn("report-notes-after-only", failed(self.audit()))

    def test_a_missing_note_verdict_is_detected(self) -> None:
        path = self.out / V.REPORT
        value = self.read(path)
        value["notes"] = value["notes"][:-1]
        self.write(path, value)
        self.assertIn("report-notes-complete", failed(self.audit()))

    def test_a_non_zero_model_call_is_detected(self) -> None:
        path = self.out / V.REPORT
        value = self.read(path)
        value["model_calls"] = 1
        self.write(path, value)
        self.assertIn("report-zero-calls", failed(self.audit()))

    def test_a_missing_markdown_section_is_detected(self) -> None:
        path = self.out / V.MARKDOWN
        lines = path.read_text(encoding="utf-8").split("\n")
        path.write_text("\n".join(line for line in lines if line.strip() != "## 知识对象身份"), encoding="utf-8", newline="\n")
        self.assertIn("markdown-知识对象身份", failed(self.audit()))

    def test_a_missing_markdown_boundary_is_detected(self) -> None:
        path = self.out / V.MARKDOWN
        text = path.read_text(encoding="utf-8")
        path.write_text(text.replace("不判断内容质量与科学正确性", "（略）"), encoding="utf-8", newline="\n")
        self.assertIn("markdown-boundary", failed(self.audit()))

    def test_a_created_at_in_the_output_manifest_is_detected(self) -> None:
        path = self.out / V.OUTPUT_MANIFEST
        value = self.read(path)
        value["created_at"] = FIXED_AT
        self.write(path, value)
        self.assertIn("output-manifest-strict", failed(self.audit()))

    def test_a_missing_output_file_is_detected(self) -> None:
        (self.out / V.MARKDOWN).unlink()
        self.assertIn("output-entries", failed(self.audit()))

    def test_a_drifted_output_digest_is_detected(self) -> None:
        path = self.out / V.OUTPUT_MANIFEST
        value = self.read(path)
        value["report_sha256"] = V.sha256(b"other")
        self.write(path, value)
        self.assertIn("output-manifest-digests", failed(self.audit()))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, *arguments: str):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *arguments],
            capture_output=True,
            text=True,
        )

    def test_package_only_mode_succeeds(self) -> None:
        result = self.run_cli("--package", str(PACKAGE))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_no_arguments_is_rejected(self) -> None:
        result = self.run_cli()
        self.assertEqual(result.returncode, 2)

    def test_a_tampered_package_fails_the_audit(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        target = copy / "before" / "笔记" / "糖酵解.md"
        target.write_bytes(target.read_bytes() + b"\n")
        result = self.run_cli("--package", str(copy))
        self.assertEqual(result.returncode, 1)
        self.assertIn("失败", result.stdout)

    def test_json_output_is_structured(self) -> None:
        result = self.run_cli("--package", str(PACKAGE), "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertIn("checks", payload)

    def test_freeze_rewrites_the_same_bytes(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        before = (copy / V.FROZEN).read_bytes()
        result = self.run_cli("--package", str(copy), "--freeze")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((copy / V.FROZEN).read_bytes(), before)
