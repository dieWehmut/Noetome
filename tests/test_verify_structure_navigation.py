"""Tests for the independent structure-navigation auditor (B10-T03).

The auditor never imports the product and never runs the CLI to decide anything.
These tests build a structure root and an output root with the auditor's own
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

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_structure_navigation.py"
PACKAGE = ROOT / "fixtures" / "structure-family-v1"
FIXED_AT = "2026-11-07T00:00:00Z"


def load_auditor(module_name: str = "verify_structure_navigation"):
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
    """Build a structure root from the auditor's replay of the frozen package."""
    replay = replay_or_fail(package)
    family = replay["family"]
    case_id = family["case_id"]
    root = directory / "root"
    control = root / "control"
    text_dir = control / "text"
    text_dir.mkdir(parents=True)

    for view in replay["views"]:
        note = view["note"]
        (text_dir / f"{note['note_id']}{V.TEXT_SUFFIX}").write_bytes(
            (package / note["path"]).read_bytes()
        )

    (control / "family.json").write_bytes((package / "family.json").read_bytes())
    (control / "concepts.json").write_bytes((package / "concepts.json").read_bytes())

    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "family": family["family"],
        "notes": [
            {
                "note_id": view["note"]["note_id"],
                "path": view["note"]["path"],
                "title": view["note"]["title"],
                "raw_sha256": view["raw_sha256"],
                "byte_length": view["byte_length"],
                "normalized_sha256": view["raw_sha256"],
                "span_count": 0,
                "heading_count": len(view["text"].split("\n")) and replay_note(replay, view)["heading_count"],
            }
            for view in replay["views"]
        ],
    }
    # 产品记录不带尾换行：`strict` 会重新 canonical 后逐字节比对。
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
        "family_sha256": V.sha256((package / "family.json").read_bytes()),
        "concepts_sha256": V.sha256((package / "concepts.json").read_bytes()),
        "inputs_sha256": V.sha256((control / "inputs.json").read_bytes()),
        "documents": [
            {
                "note_id": view["note"]["note_id"],
                "path": view["note"]["path"],
                "normalized_sha256": view["raw_sha256"],
                "byte_length": view["byte_length"],
            }
            for view in replay["views"]
        ],
        "created_at": FIXED_AT,
    }
    write_json(control / "manifest.json", manifest, trailing_newline=False)
    return root, replay


def replay_note(replay: dict, view: dict) -> dict:
    return next(
        note for note in replay["notes"] if note["note_id"] == view["note"]["note_id"]
    )


def navigation_markdown(replay: dict) -> str:
    family = replay["family"]
    lines = [
        f"# 导航与复习（{family['family']}）",
        "",
        "由确定性规则生成：只用作者包声明的笔记路径、标题与复习提示，不生成外部链接，不猜测考试频次。",
        "",
    ]
    for note in family["notes"]:
        lines += [f"## {note['title']}", "", f"位置：`{note['path']}`", "", "### 同族导航", ""]
        for sibling in family["notes"]:
            if sibling["note_id"] == note["note_id"]:
                continue
            lines.append(f"- [[{sibling['path']}|{sibling['title']}]]")
        lines += ["", "### 章节骨架", ""]
        for verdict in replay_skeleton(replay, note):
            lines.append(f"- {verdict['title']}：{verdict['status']}（{verdict['requirement']}）")
        lines += ["", "### 复习提示", ""]
        lines += [f"- {review}" for review in note["reviews"]]
        lines.append("")
    lines += [
        "## 边界",
        "",
        "- 导航只指向本族已声明笔记，不生成库外假设目标。",
        "- 复习提示来自作者声明，不虚构考试频次或出题概率。",
        "",
    ]
    return "\n".join(lines)


def replay_skeleton(replay: dict, note: dict) -> list[dict]:
    return next(
        item["skeleton"] for item in replay["notes"] if item["note_id"] == note["note_id"]
    )


def report_markdown(replay: dict) -> str:
    counts = replay["counts"]
    lines = [
        f"# 同类结构一致性与导航复习（{replay['family']['case_id']}）",
        "",
        f"主题族：{replay['family']['family']}；骨架 {counts['sections']['skeleton']} 节。",
        "",
        "## 结构一致性",
        "",
        "| 笔记 | 章节 | 就位 | 错位 | 多余 | 缺必填 | 省可选 | 空节 | 一致 |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for note in replay["notes"]:
        lines.append(
            f"| {note['title']} | {len(note['sections'])} | {note['matched']} | "
            f"{note['misordered']} | {note['extra']} | {note['missing_required']} | "
            f"{note['omitted_optional']} | {note['empty']} | "
            f"{'是' if note['consistent'] else '否'} |"
        )
    lines += ["", "## 结构偏离明细", "", "| 笔记 | 节 | 结果 | 位置 |", "| --- | --- | --- | --- |"]
    for note in replay["notes"]:
        for verdict in note["skeleton"]:
            if verdict["status"] == "matched":
                continue
            lines.append(f"| {note['title']} | {verdict['title']} | {verdict['status']} | - |")
        for title in note["extra_titles"]:
            lines.append(f"| {note['title']} | {title} | 多余（骨架未声明） | （不在骨架内） |")
    lines += ["", "## 概念的唯一解释位置", "", "| 概念 | 规范位置 | 引用方 |", "| --- | --- | --- |"]
    for item in replay["placements"]:
        lines.append(
            f"| {item['term']} | {item['canonical'] or '（无本地完整解释）'} | "
            f"{'、'.join(item['referenced_by']) or '（无）'} |"
        )
    lines += [
        "",
        "## 链接",
        "",
        f"- 来源正文链接 {counts['links']['source_links']} 条。",
        f"- 生成链接 {counts['links']['generated_links']} 条：断链 {counts['links']['generated_broken']} 条。",
        "",
        "## 边界",
        "",
        "- 只报告结构偏离与链接缺陷，不自动改写正文、不静默补齐缺失章节。",
        "- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。",
        "",
    ]
    return "\n".join(lines)


def build_output(directory: Path, replay: dict) -> Path:
    """Write the output root a correct build produces."""
    family = replay["family"]
    case_id = family["case_id"]
    root = directory / "root"
    out = directory / "out"
    out.mkdir(parents=True)

    structure = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "family": family["family"],
        "skeleton": family["skeleton"],
        "notes": replay["notes"],
        "counts": replay["counts"]["sections"],
    }
    links = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "source": replay["source_links"],
        "generated": replay["generated"],
        "counts": replay["counts"]["links"],
    }
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "family": family["family"],
        "counts": replay["counts"],
        "concepts": replay["placements"],
        "model_calls": 0,
        "network_requests": 0,
    }
    write_json(out / V.STRUCTURE, structure)
    write_json(out / V.LINKS, links)
    write_json(out / V.REPORT, report)
    (out / V.NAVIGATION).write_bytes(navigation_markdown(replay).encode("utf-8"))
    (out / V.MARKDOWN).write_bytes(report_markdown(replay).encode("utf-8"))
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "source_root": str(root.resolve()),
        "source_inputs_sha256": V.sha256((root / V.INPUTS).read_bytes()),
        "structure_sha256": V.sha256((out / V.STRUCTURE).read_bytes()),
        "links_sha256": V.sha256((out / V.LINKS).read_bytes()),
        "report_sha256": V.sha256((out / V.REPORT).read_bytes()),
        "navigation_sha256": V.sha256((out / V.NAVIGATION).read_bytes()),
        "markdown_sha256": V.sha256((out / V.MARKDOWN).read_bytes()),
        "counts": replay["counts"],
    }
    write_json(out / V.OUTPUT_MANIFEST, manifest)
    return out


def audit(root: Path, output: Path | None = None, package: Path = PACKAGE):
    report = V.Report()
    V.check_layout(root, report)
    V.check_manifest(root, package, report)
    replay = replay_or_fail(package)
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
        self.directory = Path(tempfile.mkdtemp(prefix="structure-audit-"))

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
        target = copy / "笔记" / "糖酵解.md"
        target.write_bytes(target.read_bytes() + b"\n")
        report = V.Report()
        V.check_frozen(copy, report)
        self.assertIn("frozen-笔记/糖酵解.md", failed(report))

    def test_extra_member_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "笔记" / "extra.md").write_bytes(b"# extra\n")
        report = V.Report()
        V.check_frozen(copy, report)
        self.assertIn("frozen-members", failed(report))


class PackagePriorTests(unittest.TestCase):
    def test_expected_priors_match_the_independent_replay(self) -> None:
        report = V.Report()
        V.check_expected(PACKAGE, replay_or_fail(), report)
        self.assertEqual(failed(report), set())

    def test_count_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_text())
        expected["counts"]["sections"]["matched"] += 1
        (copy / "expected.json").write_text(
            json.dumps(expected, ensure_ascii=False), encoding="utf-8", newline="\n"
        )
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-counts", failed(report))

    def test_note_prior_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_text())
        expected["notes"][1]["consistent"] = True
        (copy / "expected.json").write_text(
            json.dumps(expected, ensure_ascii=False), encoding="utf-8", newline="\n"
        )
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-note-note-gluconeogenesis", failed(report))

    def test_source_link_prior_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_text())
        expected["source_links"][0]["line"] += 1
        (copy / "expected.json").write_text(
            json.dumps(expected, ensure_ascii=False), encoding="utf-8", newline="\n"
        )
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-source-links", failed(report))

    def test_generated_link_prior_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_text())
        expected["generated"]["concept"] += 1
        (copy / "expected.json").write_text(
            json.dumps(expected, ensure_ascii=False), encoding="utf-8", newline="\n"
        )
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-generated", failed(report))

    def test_concept_prior_drift_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        expected = json.loads((copy / "expected.json").read_text())
        expected["concepts"][-1]["canonical"] = "note-ppp"
        (copy / "expected.json").write_text(
            json.dumps(expected, ensure_ascii=False), encoding="utf-8", newline="\n"
        )
        report = V.Report()
        V.check_expected(copy, replay_or_fail(), report)
        self.assertIn("expected-concepts", failed(report))


class RejectionTests(unittest.TestCase):
    def test_the_three_variants_are_rejected_with_their_codes(self) -> None:
        report = V.Report()
        V.check_rejections(PACKAGE, report)
        self.assertEqual(failed(report), set())

    def test_a_variant_that_is_not_rejected_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        # 把重复解释变体改回合法作者包：审计器必须发现"该拒的没拒"。
        (copy / "family-duplicate.json").write_bytes((copy / "family.json").read_bytes())
        report = V.Report()
        V.check_rejections(copy, report)
        self.assertIn("rejection-duplicate-explanation", failed(report))

    def test_the_baseline_package_itself_is_not_rejected(self) -> None:
        replay, code, detail = V.replay_package(PACKAGE)
        self.assertIsNotNone(replay, f"{code}：{detail}")


class CleanRootTests(SandboxCase):
    def test_root_and_output_are_all_green(self) -> None:
        report = self.audit()
        self.assertEqual(failed(report), set())
        self.assertGreater(len(report.checks), 60)

    def test_root_alone_is_green(self) -> None:
        report = self.audit(output=False)
        self.assertEqual(failed(report), set())


class LayoutTests(SandboxCase):
    def test_a_top_level_extra_entry_is_detected(self) -> None:
        (self.root / "notes.md").write_bytes(b"# extra\n")
        self.assertIn("root-entries", failed(self.audit()))

    def test_an_extra_control_entry_is_detected(self) -> None:
        (self.root / "control" / "extra.json").write_bytes(b"{}\n")
        self.assertIn("control-entries", failed(self.audit()))

    def test_a_missing_text_directory_is_detected(self) -> None:
        shutil.rmtree(self.root / "control" / "text")
        report = self.audit()
        self.assertIn("text-directory", failed(report))

    def test_a_symlinked_file_is_detected(self) -> None:
        target = self.read_path = self.root / "control" / "family.json"
        link = self.root / "control" / "alias.json"
        link.symlink_to(target)
        self.assertIn("no-symlinks", failed(self.audit()))

    def read_path(self, path: Path) -> bytes:
        return path.read_bytes()


class ManifestTests(SandboxCase):
    def test_an_unknown_field_is_detected(self) -> None:
        manifest = self.read(self.root / "control/manifest.json")
        manifest["extra"] = 1
        self.write(self.root / "control/manifest.json", manifest, trailing_newline=False)
        self.assertIn("manifest-strict", failed(self.audit()))

    def test_a_pretty_printed_record_is_detected(self) -> None:
        manifest = self.read(self.root / "control/manifest.json")
        (self.root / "control/manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8", newline="\n"
        )
        self.assertIn("manifest-strict", failed(self.audit()))

    def test_a_root_drift_is_detected(self) -> None:
        manifest = self.read(self.root / "control/manifest.json")
        manifest["root"] = "/somewhere/else"
        self.write(self.root / "control/manifest.json", manifest, trailing_newline=False)
        self.assertIn("manifest-root", failed(self.audit()))

    def test_a_family_digest_drift_is_detected(self) -> None:
        manifest = self.read(self.root / "control/manifest.json")
        manifest["family_sha256"] = "0" * 64
        self.write(self.root / "control/manifest.json", manifest, trailing_newline=False)
        self.assertIn("manifest-family.json-digest", failed(self.audit()))

    def test_a_body_drift_is_detected(self) -> None:
        body = self.root / "control/text/note-ppp.md"
        body.write_bytes(body.read_bytes() + b"\n")
        report = self.audit()
        self.assertIn("manifest-note-note-ppp", failed(report))

    def test_a_missing_frozen_body_is_detected(self) -> None:
        (self.root / "control/text/note-ppp.md").unlink()
        self.assertIn("manifest-text-set", failed(self.audit()))

    def test_an_extra_frozen_body_is_detected(self) -> None:
        (self.root / "control/text/note-extra.md").write_bytes(b"# extra\n")
        self.assertIn("manifest-text-set", failed(self.audit()))


class InputsTests(SandboxCase):
    def test_a_title_drift_is_detected(self) -> None:
        inputs = self.read(self.root / "control/inputs.json")
        inputs["notes"][0]["title"] = "别的标题"
        self.write(self.root / "control/inputs.json", inputs, trailing_newline=False)
        self.assertIn("inputs-note-note-glycolysis", failed(self.audit()))

    def test_a_body_digest_drift_is_detected(self) -> None:
        inputs = self.read(self.root / "control/inputs.json")
        inputs["notes"][0]["raw_sha256"] = "0" * 64
        self.write(self.root / "control/inputs.json", inputs, trailing_newline=False)
        self.assertIn("inputs-note-note-glycolysis", failed(self.audit()))

    def test_a_case_drift_is_detected(self) -> None:
        inputs = self.read(self.root / "control/inputs.json")
        inputs["case_id"] = "other-case"
        self.write(self.root / "control/inputs.json", inputs, trailing_newline=False)
        self.assertIn("inputs-case", failed(self.audit()))

    def test_a_missing_note_is_detected(self) -> None:
        inputs = self.read(self.root / "control/inputs.json")
        inputs["notes"] = inputs["notes"][:2]
        self.write(self.root / "control/inputs.json", inputs, trailing_newline=False)
        self.assertIn("inputs-note-count", failed(self.audit()))

    def test_a_heading_count_drift_is_detected(self) -> None:
        inputs = self.read(self.root / "control/inputs.json")
        inputs["notes"][0]["heading_count"] = 999
        self.write(self.root / "control/inputs.json", inputs, trailing_newline=False)
        self.assertIn("inputs-note-note-glycolysis", failed(self.audit()))


class OutputTests(SandboxCase):
    def test_a_structure_count_drift_is_detected(self) -> None:
        structure = self.read(self.out / "structure.json")
        structure["counts"]["matched"] += 1
        self.write(self.out / "structure.json", structure)
        self.assertIn("structure-counts", failed(self.audit()))

    def test_a_body_drift_is_detected(self) -> None:
        structure = self.read(self.out / "structure.json")
        structure["notes"][2]["empty"] = 0
        self.write(self.out / "structure.json", structure)
        self.assertIn("structure-notes", failed(self.audit()))

    def test_a_source_link_drift_is_detected(self) -> None:
        links = self.read(self.out / "links.json")
        links["source"][0]["status"] = "target_missing"
        self.write(self.out / "links.json", links)
        self.assertIn("links-source", failed(self.audit()))

    def test_a_broken_generated_link_is_detected(self) -> None:
        links = self.read(self.out / "links.json")
        links["generated"][0]["status"] = "target_missing"
        links["counts"]["generated_broken"] = 1
        self.write(self.out / "links.json", links)
        report = self.audit()
        self.assertIn("links-no-broken", failed(report))
        self.assertIn("links-counts", failed(report))

    def test_a_report_count_drift_is_detected(self) -> None:
        report = self.read(self.out / "report.json")
        report["counts"]["links"]["generated_links"] = 8
        self.write(self.out / "report.json", report)
        self.assertIn("report-counts", failed(self.audit()))

    def test_a_concept_table_drift_is_detected(self) -> None:
        report = self.read(self.out / "report.json")
        report["concepts"][-1]["canonical"] = "note-ppp"
        self.write(self.out / "report.json", report)
        self.assertIn("report-concepts", failed(self.audit()))

    def test_a_nonzero_model_call_is_detected(self) -> None:
        report = self.read(self.out / "report.json")
        report["model_calls"] = 1
        self.write(self.out / "report.json", report)
        self.assertIn("report-zero-calls", failed(self.audit()))

    def test_a_missing_deviation_section_is_detected(self) -> None:
        markdown = (self.out / "report.md").read_text(encoding="utf-8")
        (self.out / "report.md").write_text(
            markdown.replace("## 结构偏离明细", "## 其它"), encoding="utf-8", newline="\n"
        )
        self.assertIn("markdown-结构偏离明细", failed(self.audit()))

    def test_a_surplus_section_missing_from_the_report_is_detected(self) -> None:
        markdown = (self.out / "report.md").read_text(encoding="utf-8")
        (self.out / "report.md").write_text(
            "\n".join(line for line in markdown.split("\n") if "延伸阅读" not in line),
            encoding="utf-8", newline="\n",
        )
        self.assertIn("markdown-extra-note-ppp-延伸阅读", failed(self.audit()))

    def test_a_navigation_without_the_boundary_is_detected(self) -> None:
        navigation = (self.out / "navigation.md").read_text(encoding="utf-8")
        (self.out / "navigation.md").write_text(
            navigation.replace("不虚构考试频次", "重点"), encoding="utf-8", newline="\n"
        )
        self.assertIn("navigation-no-exam-guess", failed(self.audit()))

    def test_a_created_at_in_the_output_manifest_is_detected(self) -> None:
        manifest = self.read(self.out / "manifest.json")
        manifest["created_at"] = FIXED_AT
        self.write(self.out / "manifest.json", manifest)
        report = self.audit()
        self.assertIn("output-manifest-strict", failed(report))

    def test_a_missing_output_file_is_detected(self) -> None:
        (self.out / "navigation.md").unlink()
        report = self.audit()
        self.assertIn("output-entries", failed(report))
        self.assertIn("output-present-navigation.md", failed(report))

    def test_a_tampered_output_digest_is_detected(self) -> None:
        manifest = self.read(self.out / "manifest.json")
        manifest["report_sha256"] = "0" * 64
        self.write(self.out / "manifest.json", manifest)
        self.assertIn("output-manifest-digests", failed(self.audit()))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, *arguments: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *arguments],
            capture_output=True,
            text=True,
            encoding="utf-8",
        )

    def test_package_mode_is_green(self) -> None:
        result = self.run_cli("--package", str(PACKAGE))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("失败 0", result.stdout)

    def test_no_arguments_is_a_usage_error(self) -> None:
        result = self.run_cli()
        self.assertEqual(result.returncode, 2)

    def test_json_output_is_structured(self) -> None:
        result = self.run_cli("--package", str(PACKAGE), "--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["failures"], 0)
        self.assertGreater(payload["checks"], 10)

    def test_freeze_rewrites_the_same_bytes(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        before = (copy / "frozen-sha256.json").read_bytes()
        result = self.run_cli("--package", str(copy), "--freeze")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((copy / "frozen-sha256.json").read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
