"""Tests for the independent structure-template auditor.

The tests never import the Rust product. They load the auditor by path, feed it
package fixtures and hand-built roots, and assert that it accepts honest input
and rejects every drift it claims to catch.
"""

from __future__ import annotations

import copy
import importlib.util
import io
import json
import shutil
import tempfile
import contextlib
import unittest
from pathlib import Path

from symlink_test_support import create_symlink_or_skip

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = ROOT / "fixtures" / "structure-template-v1"
AUDITOR = ROOT / "scripts" / "verify_template_structure.py"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_template_structure", AUDITOR)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def sha256(data: bytes) -> str:
    return V.sha256(data)


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, ensure_ascii=False, indent=2)
    if trailing_newline:
        text += "\n"
    path.write_text(text, encoding="utf-8", newline="\n")


def write_canonical_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(V.canonical(value) + "\n", encoding="utf-8", newline="\n")


def fake_identity(index: int):
    return {"platform": 1, "device": 2, "inode": index}


def package_declaration(name: str = "project.json"):
    return V.load_package(PACKAGE, name)


def replay_or_fail(declaration):
    code = V.structured_failure(declaration)
    if code:
        raise AssertionError(f"unexpected rejection: {code}")
    return V.replay(declaration)


def build_root(directory: Path, package: Path = PACKAGE, project_name: str = "project.json") -> Path:
    root = directory / "root"
    control = root / V.CONTROL
    control.mkdir(parents=True)
    project_bytes = (package / project_name).read_bytes()
    (control / V.PROJECT).write_bytes(project_bytes)
    declaration = V.load_package(package, project_name)
    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "project_sha256": sha256(project_bytes),
        "templates": len(declaration["templates"]),
        "topics": len(declaration["topics"]),
        "goals": sum(len(topic["goals"]) for topic in declaration["topics"]),
        "concepts": len(declaration["concepts"]),
        "atoms": len(declaration["atoms"]),
    }
    inputs_bytes = V.canonical(inputs).encode("utf-8")
    (control / V.INPUTS).write_bytes(inputs_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "root": str(root.resolve()),
        "root_identity": fake_identity(1),
        "parent_identity": fake_identity(2),
        "control_identity": fake_identity(3),
        "manifest_identity": fake_identity(4),
        "inputs_identity": fake_identity(5),
        "project_sha256": sha256(project_bytes),
        "inputs_sha256": sha256(inputs_bytes),
        "created_at": "2026-01-01T00:00:00Z",
    }
    (control / V.MANIFEST).write_bytes(V.canonical(manifest).encode("utf-8"))
    return root


def report_markdown(replay_value) -> str:
    return V.render_markdown(replay_value)


def build_output(directory: Path, replay_value) -> Path:
    output = directory / "out"
    output.mkdir()
    plan_bytes = (V.canonical(replay_value["plan"]) + "\n").encode("utf-8")
    report_bytes = (V.canonical(replay_value["report"]) + "\n").encode("utf-8")
    markdown_bytes = report_markdown(replay_value).encode("utf-8")
    (output / V.PLAN).write_bytes(plan_bytes)
    (output / V.REPORT).write_bytes(report_bytes)
    (output / V.MARKDOWN).write_bytes(markdown_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay_value["plan"]["case_id"],
        "source_root": str(directory.resolve()),
        "source_inputs_sha256": "0" * 64,
        "plan_sha256": sha256(plan_bytes),
        "report_sha256": sha256(report_bytes),
        "markdown_sha256": sha256(markdown_bytes),
        "verdict": replay_value["report"]["verdict"],
        "counts": replay_value["report"]["counts"],
    }
    write_canonical_json(output / V.OUTPUT_MANIFEST, manifest)
    return output


def audit(root: Path | None, output: Path | None, package: Path = PACKAGE):
    report = V.Report()
    if package is not None:
        declaration = V.load_package(package)
        replay_value = V.replay(declaration)
        V.check_expected(package, replay_value, report)
        V.check_frozen(package, report)
    if root is not None:
        V.check_layout(root, report)
        manifest = V.check_manifest(root, report)
        if manifest is not None:
            V.check_inputs(root, report, manifest)
        if output is not None:
            V.check_output(output, replay_value, report)
    return report


def failed(report) -> set[str]:
    return {name for name, ok, _detail in report.checks if not ok}


def show(report) -> None:
    for name, ok, detail in report.checks:
        if not ok:
            raise AssertionError(f"{name}: {detail}")


class Sandbox:
    def __enter__(self) -> Path:
        self.path = Path(tempfile.mkdtemp(prefix="tpl-audit-"))
        return self.path

    def __exit__(self, *_exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)


class FrozenPackageTests(unittest.TestCase):
    def test_the_frozen_manifest_covers_every_member(self):
        report = V.Report()
        V.check_frozen(PACKAGE, report)
        show(report)
        self.assertGreater(len(report.checks), 10)

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            (package / "cases.json").write_text("{}\n", encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-digest:cases.json", failed(report))

    def test_an_extra_member_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}\n", encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-members", failed(report))

    def test_refreezing_is_byte_stable(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            before = (package / V.FROZEN).read_bytes()
            with contextlib.redirect_stdout(io.StringIO()):
                V.freeze(package, V.Report())
            self.assertEqual(before, (package / V.FROZEN).read_bytes())


class CleanRootTests(unittest.TestCase):
    def test_a_clean_root_passes(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            report = V.Report()
            V.check_layout(root, report)
            manifest = V.check_manifest(root, report)
            V.check_inputs(root, report, manifest)
            show(report)
            self.assertGreater(len(report.checks), 10)

    def test_root_and_output_pass_together(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            report = audit(root, output)
            show(report)
            self.assertGreater(len(report.checks), 60)

    def test_the_control_directory_has_exactly_three_entries(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            names = sorted(item.name for item in (root / V.CONTROL).iterdir())
            self.assertEqual(names, list(V.CONTROL_ENTRIES))


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_level_entry_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "notes.md").write_text("x", encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_layout(root, report)
            self.assertIn("layout-root-entries", failed(report))

    def test_an_extra_control_entry_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / V.CONTROL / "extra.json").write_text("{}", encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_layout(root, report)
            self.assertIn("layout-control-entries", failed(report))

    def test_a_symlink_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            create_symlink_or_skip(root / "link.md", root / V.CONTROL / V.PROJECT)
            report = V.Report()
            V.check_layout(root, report)
            self.assertIn("layout-no-links", failed(report))


class ManifestTests(unittest.TestCase):
    def test_an_unknown_field_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            manifest["extra"] = 1
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-strict", failed(report))

    def test_a_pretty_manifest_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            write_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-strict", failed(report))

    def test_a_moved_root_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            manifest["root"] = "/somewhere/else"
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-root", failed(report))

    def test_a_changed_project_digest_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            manifest["project_sha256"] = "0" * 64
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-digest:project.json", failed(report))

    def test_a_changed_inputs_digest_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            manifest["inputs_sha256"] = "1" * 64
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-digest:inputs.json", failed(report))

    def test_a_missing_identity_field_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            del manifest["control_identity"]["inode"]
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertTrue(any(name.startswith("manifest-identity:control_identity") for name in failed(report)))


class InputsTests(unittest.TestCase):
    def test_a_drifted_case_id_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            inputs = json.loads((root / V.CONTROL / V.INPUTS).read_text(encoding="utf-8"))
            inputs["case_id"] = "other"
            (root / V.CONTROL / V.INPUTS).write_bytes(V.canonical(inputs).encode("utf-8"))
            report = V.Report()
            manifest = V.check_manifest(root, report)
            V.check_inputs(root, report, manifest)
            self.assertIn("manifest-digest:inputs.json", failed(report))

    def test_a_drifted_policy_is_refused(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            inputs = json.loads((root / V.CONTROL / V.INPUTS).read_text(encoding="utf-8"))
            inputs["policy"] = "other"
            (root / V.CONTROL / V.INPUTS).write_bytes(V.canonical(inputs).encode("utf-8"))
            report = V.Report()
            manifest = json.loads((root / V.CONTROL / V.MANIFEST).read_text(encoding="utf-8"))
            manifest["inputs_sha256"] = sha256((root / V.CONTROL / V.INPUTS).read_bytes())
            write_canonical_json(root / V.CONTROL / V.MANIFEST, manifest)
            V.check_inputs(root, report, manifest)
            self.assertIn("inputs-policy", failed(report))


class ReplayRuleTests(unittest.TestCase):
    def test_the_prior_counts_are_reproduced(self):
        replay_value = replay_or_fail(package_declaration())
        expected = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        self.assertEqual(replay_value["report"]["counts"], expected["counts"])
        self.assertEqual(replay_value["report"]["verdict"], expected["verdict"])

    def test_every_concept_has_one_explanation(self):
        replay_value = replay_or_fail(package_declaration())
        self.assertEqual(len(replay_value["plan"]["explanations"]), 4)
        concepts = [entry["concept_id"] for entry in replay_value["plan"]["explanations"]]
        self.assertEqual(len(set(concepts)), len(concepts))

    def test_references_never_target_their_own_note(self):
        replay_value = replay_or_fail(package_declaration())
        for goal in replay_value["plan"]["goals"]:
            for reference in goal["references"]:
                self.assertNotEqual(goal["path"], reference["target_note"])

    def test_an_empty_section_is_refused(self):
        declaration = package_declaration("project-empty-section.json")
        self.assertEqual(V.structured_failure(declaration), "TEMPLATE_EMPTY_SECTION")

    def test_a_section_out_of_order_is_refused(self):
        declaration = package_declaration("project-section-order.json")
        self.assertEqual(V.structured_failure(declaration), "TEMPLATE_SECTION_ORDER")

    def test_a_special_section_without_a_reason_is_refused(self):
        declaration = package_declaration("project-special.json")
        self.assertEqual(V.structured_failure(declaration), "TEMPLATE_UNREASONED")

    def test_a_title_outside_the_pattern_needs_a_reason(self):
        declaration = package_declaration("project-title.json")
        self.assertEqual(V.structured_failure(declaration), "TEMPLATE_UNREASONED")

    def test_an_inconsistent_shape_becomes_a_finding(self):
        declaration = package_declaration("project-inconsistent.json")
        self.assertEqual(V.structured_failure(declaration), "")
        replay_value = V.replay(declaration)
        kinds = {finding["kind"] for finding in replay_value["report"]["findings"]}
        self.assertIn("inconsistent_structure", kinds)

    def test_a_second_explanation_becomes_a_finding(self):
        declaration = package_declaration("project-duplicate.json")
        replay_value = V.replay(declaration)
        kinds = {finding["kind"] for finding in replay_value["report"]["findings"]}
        self.assertIn("duplicate_explanation", kinds)

    def test_omissions_are_counted_and_visible(self):
        replay_value = replay_or_fail(package_declaration())
        omitted = [item for item in replay_value["report"]["deviations"] if item["kind"] == "section_omitted"]
        self.assertEqual(len(omitted), 10)
        self.assertTrue(all("#" in item["subject"] for item in omitted))

    def test_a_concept_without_explanation_is_a_finding(self):
        declaration = package_declaration()
        for atom in declaration["atoms"]:
            if atom["atom_id"] == "atom-glycolysis-position":
                atom["role"] = "reference"
        replay_value = V.replay(declaration)
        kinds = {finding["kind"] for finding in replay_value["report"]["findings"]}
        self.assertIn("concept_unexplained", kinds)

    def test_a_reference_inside_the_explanation_note_is_refused(self):
        declaration = package_declaration()
        for atom in declaration["atoms"]:
            if atom["atom_id"] == "atom-glycolysis-net":
                atom["goal_id"] = "goal-glycolysis-main"
                atom["section"] = "定位"
        self.assertEqual(V.structured_failure(declaration), "TEMPLATE_ARGUMENT")


class ExpectedTests(unittest.TestCase):
    def test_every_prior_count_is_checked(self):
        report = V.Report()
        V.check_expected(PACKAGE, replay_or_fail(package_declaration()), report)
        show(report)
        expected = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        names = {name for name, _ok, _detail in report.checks}
        for key in expected["counts"]:
            self.assertIn(f"expected-count:{key}", names)

    def test_a_drifted_prior_count_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["counts"]["notes"] += 1
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_expected(package, replay_or_fail(package_declaration()), report)
            self.assertIn("expected-count:notes", failed(report))

    def test_a_drifted_goal_path_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["goals"][0]["path"] = "代谢/别的/01-x.md"
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_expected(package, replay_or_fail(package_declaration()), report)
            self.assertIn("expected-goal:goal-glycolysis-main", failed(report))

    def test_a_drifted_explanation_note_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["explanations"][0]["note"] = "代谢/别的/01-x.md"
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_expected(package, replay_or_fail(package_declaration()), report)
            self.assertIn("expected-explanation:concept-glycolysis", failed(report))

    def test_a_drifted_deviation_count_is_detected(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["deviations"]["section_omitted"] = 9
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_expected(package, replay_or_fail(package_declaration()), report)
            self.assertIn("expected-deviation:section_omitted", failed(report))

    def test_a_variant_that_stops_being_refused_is_detected(self):
        expected = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            shutil.copyfile(package / "project.json", package / "project-empty-section.json")
            report = V.Report()
            V.check_variants(package, expected, report)
            self.assertIn("variant-empty-section:code", failed(report))


class OutputTests(unittest.TestCase):
    def test_an_extra_output_entry_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            (output / "extra.md").write_text("x", encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-entries", failed(report))

    def test_a_missing_output_file_is_reported(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            (output / V.MARKDOWN).unlink()
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-complete", failed(report))

    def test_a_drifted_plan_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            plan = json.loads((output / V.PLAN).read_text(encoding="utf-8"))
            plan["goals"][0]["path"] = "elsewhere.md"
            write_canonical_json(output / V.PLAN, plan)
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-plan", failed(report))

    def test_a_drifted_report_verdict_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            report_value = json.loads((output / V.REPORT).read_text(encoding="utf-8"))
            report_value["verdict"] = "findings"
            write_canonical_json(output / V.REPORT, report_value)
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-report", failed(report))

    def test_an_output_manifest_with_a_timestamp_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            manifest = json.loads((output / V.OUTPUT_MANIFEST).read_text(encoding="utf-8"))
            manifest["created_at"] = "2026-01-01T00:00:00Z"
            write_canonical_json(output / V.OUTPUT_MANIFEST, manifest)
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-strict", failed(report))

    def test_a_drifted_output_digest_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            manifest = json.loads((output / V.OUTPUT_MANIFEST).read_text(encoding="utf-8"))
            manifest["plan_sha256"] = "0" * 64
            write_canonical_json(output / V.OUTPUT_MANIFEST, manifest)
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-manifest-digest:plan.json", failed(report))

    def test_a_missing_markdown_section_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            text = (output / V.MARKDOWN).read_text(encoding="utf-8")
            (output / V.MARKDOWN).write_text(text.replace("## 缺口", "## 别的"), encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("markdown-section:## 缺口", failed(report))

    def test_a_missing_boundary_statement_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            text = (output / V.MARKDOWN).read_text(encoding="utf-8")
            (output / V.MARKDOWN).write_text(text.replace("不判断内容质量与科学正确性", "略"), encoding="utf-8", newline="\n")
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("markdown-boundary", failed(report))

    def test_a_pretty_output_manifest_is_refused(self):
        with Sandbox() as sandbox:
            replay_value = replay_or_fail(package_declaration())
            output = build_output(sandbox, replay_value)
            manifest = json.loads((output / V.OUTPUT_MANIFEST).read_text(encoding="utf-8"))
            write_json(output / V.OUTPUT_MANIFEST, manifest)
            report = V.Report()
            V.check_output(output, replay_value, report)
            self.assertIn("output-strict", failed(report))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, arguments):
        output = io.StringIO()
        errors = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            code = V.main(arguments)
        return code, output.getvalue(), errors.getvalue()

    def test_package_mode_exits_zero(self):
        code, out, _err = self.run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, out)

    def test_a_tampered_root_exits_one(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / V.CONTROL / V.PROJECT).write_text("{}", encoding="utf-8", newline="\n")
            code, out, _err = self.run_cli([str(root)])
            self.assertEqual(code, 1)
            self.assertIn("manifest-digest:project.json", out)

    def test_no_arguments_is_a_usage_error(self):
        code, _out, err = self.run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给", err)

    def test_freeze_without_a_package_is_a_usage_error(self):
        code, _out, err = self.run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", err)

    def test_json_output_is_structured(self):
        code, out, _err = self.run_cli(["--package", str(PACKAGE), "--json"])
        self.assertEqual(code, 0)
        value = json.loads(out)
        self.assertEqual(value["failures"], 0)
        self.assertGreater(value["total"], 10)

    def test_refreezing_keeps_the_bytes(self):
        with Sandbox() as sandbox:
            package = sandbox / "pkg"
            shutil.copytree(PACKAGE, package)
            before = (package / V.FROZEN).read_bytes()
            code, out, _err = self.run_cli(["--package", str(package), "--freeze"])
            self.assertEqual(code, 0, out)
            self.assertEqual(before, (package / V.FROZEN).read_bytes())


if __name__ == "__main__":
    unittest.main()
