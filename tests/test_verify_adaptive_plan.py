"""Tests for the independent adaptive-plan auditor.

The auditor is loaded as a module and driven by hand-built roots; the product is
never imported and the CLI is never used to decide anything. Every test asserts a
specific check name, so a rename in the auditor fails loudly here.
"""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "fixtures" / "adaptive-plan-v1"


def load_auditor():
    path = ROOT / "scripts" / "verify_adaptive_plan.py"
    spec = importlib.util.spec_from_file_location("verify_adaptive_plan", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    text = canonical(value)
    if trailing_newline:
        text += "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def fake_identity(index: int) -> dict:
    return {"platform": 2, "device": 1000 + index, "inode": 2000 + index}


def package_prior() -> dict:
    return json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))


def package_declaration() -> dict:
    return json.loads((PACKAGE / "project.json").read_text(encoding="utf-8"))


def replay_or_fail(declaration=None) -> dict:
    return V.replay(declaration or package_declaration())


def build_root(directory: Path, declaration=None) -> Path:
    """Build a frozen root shaped exactly like the product's `plan adopt`."""
    declaration = declaration or package_declaration()
    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    project_bytes = json.dumps(declaration, ensure_ascii=False, indent=2).encode("utf-8") + b"\n"
    (control / "project.json").write_bytes(project_bytes)
    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "project_sha256": sha256(project_bytes),
        "topics": len(declaration["topics"]),
        "goals": len(declaration["goals"]),
        "concepts": len(declaration["concepts"]),
        "atoms": len(declaration["atoms"]),
        "chains": len(declaration["chains"]),
    }
    write_json(control / "inputs.json", inputs, trailing_newline=False)
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
        "inputs_sha256": sha256(canonical(inputs).encode("utf-8")),
        "created_at": "2026-11-08T00:00:00.000000000Z",
    }
    write_json(control / "manifest.json", manifest, trailing_newline=False)
    return root


def report_markdown(replay_result: dict) -> str:
    lines = [f"# 教材式自适应规划（{replay_result['case_id']}）", "", "## 教学顺序", ""]
    for index, entry in enumerate(replay_result["teaching_order"], start=1):
        lines.append(f"{index}. {entry['canonical']}")
    lines += ["", "## 阅读目标与位置", "", "## 反应链", "", "## 提醒", "", "## 边界", ""]
    lines.append("- 规划只读已核对的概念关系与必要原子，不生成知识、不写正文。")
    lines.append("- 模型调用与网络请求 0 次；本阶段只做结构规划。")
    return "\n".join(lines) + "\n"


def build_output(directory: Path, replay_result: dict) -> Path:
    output = directory / "out"
    output.mkdir(parents=True)
    plan = {
        "policy": V.POLICY,
        "plan_version": V.PLAN_VERSION,
        "case_id": replay_result["case_id"],
        "folder_root": replay_result["folder_root"],
        "teaching_order": replay_result["teaching_order"],
        "folders": replay_result["folders"],
        "notes": replay_result["notes"],
        "atoms": replay_result["atoms"],
    }
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay_result["case_id"],
        "description": "测试用规划声明。",
        "counts": replay_result["counts"],
        "chains": replay_result["chains"],
        "warnings": [
            {"kind": item["kind"], "subject": item["subject"], "detail": "测试用提醒详情。"}
            for item in replay_result["warnings"]
        ],
        "model_calls": 0,
        "network_requests": 0,
    }
    markdown = report_markdown(replay_result)
    write_json(output / "plan.json", plan)
    write_json(output / "report.json", report)
    (output / "report.md").write_text(markdown, encoding="utf-8")
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay_result["case_id"],
        "source_root": str((directory / "root").resolve()),
        "source_inputs_sha256": "0" * 64,
        "plan_sha256": sha256((output / "plan.json").read_bytes()),
        "report_sha256": sha256((output / "report.json").read_bytes()),
        "markdown_sha256": sha256((output / "report.md").read_bytes()),
        "counts": replay_result["counts"],
    }
    write_json(output / "manifest.json", manifest)
    return output


def failed(report) -> set[str]:
    return {name for name, ok, _detail in report.checks if not ok}


def show(report) -> str:
    return "; ".join(f"{name}={detail}" for name, ok, detail in report.checks if not ok)


def audit(root: Path, output: Path | None, package: Path | None) -> object:
    report = V.Report()
    V.check_layout(root, report)
    manifest = V.check_manifest(root, report)
    declaration = json.loads((root / "control" / "project.json").read_text(encoding="utf-8"))
    V.check_inputs(root, manifest, declaration, report)
    local = V.replay(declaration)
    if output is not None:
        V.check_output(output, local, report)
    if package is not None:
        prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
        V.check_expected(local, declaration, prior, report)
        V.check_variants(package, prior, report)
        V.check_frozen(package, report)
    return report


class Sandbox:
    def __enter__(self) -> Path:
        self.path = Path(tempfile.mkdtemp(prefix="plan-audit-"))
        return self.path

    def __exit__(self, *exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)


class FrozenPackageTests(unittest.TestCase):
    def test_the_recorded_members_cover_every_file(self):
        with Sandbox() as sandbox:
            report = V.Report()
            V.check_frozen(PACKAGE, report)
            self.assertEqual(failed(report), set(), show(report))

    def test_a_tampered_member_is_reported(self):
        with Sandbox() as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            (package / "expected.json").write_text("{}\n", encoding="utf-8")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-digest:expected.json", failed(report))

    def test_an_extra_member_is_reported(self):
        with Sandbox() as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}\n", encoding="utf-8")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-members", failed(report))


class CleanRootTests(unittest.TestCase):
    def test_a_shaped_root_and_output_pass(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            report = audit(root, output, PACKAGE)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 80)

    def test_a_root_alone_passes(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            report = audit(root, None, None)
            self.assertEqual(failed(report), set(), show(report))

    def test_the_control_directory_has_exactly_three_entries(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            self.assertEqual(
                sorted(item.name for item in (root / "control").iterdir()),
                ["inputs.json", "manifest.json", "project.json"],
            )


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_level_entry_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "notes").mkdir()
            report = audit(root, None, None)
            self.assertIn("layout-root-entries", failed(report))

    def test_an_extra_control_entry_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "control" / "plan.json").write_text("{}", encoding="utf-8")
            report = audit(root, None, None)
            self.assertIn("layout-control-entries", failed(report))

    def test_a_symlink_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "control" / "link.json").symlink_to(root / "control" / "inputs.json")
            report = V.Report()
            V.check_layout(root, report)
            self.assertIn("layout-symlinks", failed(report))


class ManifestTests(unittest.TestCase):
    def test_an_unknown_field_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["extra"] = 1
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-strict", failed(report))

    def test_pretty_json_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-strict", failed(report))

    def test_a_moved_root_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["root"] = "/somewhere/else"
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-root", failed(report))

    def test_a_changed_declaration_digest_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "control" / "project.json").write_text("{}\n", encoding="utf-8")
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-project-digest", failed(report))

    def test_a_missing_identity_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            del value["inputs_identity"]
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-strict", failed(report))

    def test_a_wrong_identity_shape_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["root_identity"] = {"platform": 2}
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_manifest(root, report)
            self.assertIn("manifest-identity:root_identity", failed(report))


class InputsTests(unittest.TestCase):
    def test_a_changed_case_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "inputs.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["case_id"] = "other"
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_inputs(root, None, package_declaration(), report)
            self.assertIn("inputs-case", failed(report))

    def test_a_changed_count_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "inputs.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["atoms"] = 99
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_inputs(root, None, package_declaration(), report)
            self.assertIn("inputs-count:atoms", failed(report))

    def test_an_unknown_field_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            path = root / "control" / "inputs.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["extra"] = 0
            write_json(path, value, trailing_newline=False)
            report = V.Report()
            V.check_inputs(root, None, package_declaration(), report)
            self.assertIn("inputs-strict", failed(report))


class ReplayRuleTests(unittest.TestCase):
    def test_the_prior_matches_the_replay(self):
        report = V.Report()
        declaration = package_declaration()
        V.check_expected(replay_or_fail(declaration), declaration, package_prior(), report)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_drifted_count_is_reported(self):
        report = V.Report()
        prior = package_prior()
        prior["counts"]["cross_refs"] = 99
        V.check_expected(replay_or_fail(), package_declaration(), prior, report)
        self.assertIn("expected-count:cross_refs", failed(report))

    def test_a_drifted_note_is_reported(self):
        report = V.Report()
        prior = package_prior()
        prior["notes"][1]["path"] = "代谢/糖酵解/99-other.md"
        V.check_expected(replay_or_fail(), package_declaration(), prior, report)
        self.assertIn("expected-notes", failed(report))

    def test_teaching_order_puts_prerequisites_first(self):
        order = V.teaching_order(package_declaration())
        self.assertLess(order.index("concept-glycolysis"), order.index("concept-gluconeogenesis"))
        self.assertLess(order.index("concept-pyruvate"), order.index("concept-gluconeogenesis"))

    def test_a_prerequisite_cycle_is_refused_by_the_replay(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["relations"].append(
            {"from": "concept-pfk", "to": "concept-glycolysis", "kind": "prerequisite"}
        )
        with self.assertRaises(V.PlanError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "PLAN_PREREQUISITE_CYCLE")

    def test_a_chain_split_without_a_reason_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["chains"][0].pop("split_reason")
        with self.assertRaises(V.PlanError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "PLAN_CHAIN_SPLIT")

    def test_a_declared_reason_is_recorded(self):
        result = replay_or_fail()
        self.assertEqual(result["chains"][0]["split_reason"], "chain_too_long")
        self.assertEqual(result["counts"]["chains_split"], 1)

    def test_the_secondary_goal_links_to_the_primary_note(self):
        result = replay_or_fail()
        energy = next(note for note in result["notes"] if note["goal_id"] == "goal-glycolysis-energy")
        self.assertEqual(energy["links"][0]["target_note"], "代谢/糖酵解/01-goal-glycolysis-main.md")
        self.assertEqual(result["counts"]["cross_refs"], 3)

    def test_atoms_are_reported_in_reading_order(self):
        result = replay_or_fail()
        order = [atom["atom_id"] for atom in result["atoms"]]
        self.assertEqual(order[0], "atom-glycolysis-definition")
        self.assertEqual(order[-1], "atom-gluconeogenesis-compare")

    def test_an_empty_goal_becomes_a_warning(self):
        result = replay_or_fail()
        self.assertEqual(
            result["warnings"],
            [{"kind": "goal_without_atoms", "subject": "topic-gluconeogenesis-goal-gluconeogenesis-energy"}],
        )

    def test_a_concept_outside_the_primary_goal_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        for atom in declaration["atoms"]:
            if atom["concept_id"] == "concept-gluconeogenesis":
                atom["goal_id"] = "goal-gluconeogenesis-compare"
        code, _detail = V.structured_failure(declaration)
        self.assertEqual(code, "PLAN_UNPLACED")

    def test_a_cross_topic_atom_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        for atom in declaration["atoms"]:
            if atom["atom_id"] == "atom-pfk-allosteric":
                atom["goal_id"] = "goal-gluconeogenesis-main"
        code, _detail = V.structured_failure(declaration)
        self.assertEqual(code, "PLAN_ARGUMENT")

    def test_a_non_contiguous_chain_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["atoms"][1]["step"] = 5
        code, _detail = V.structured_failure(declaration)
        self.assertEqual(code, "PLAN_ARGUMENT")


class VariantTests(unittest.TestCase):
    def test_all_four_variants_report_their_code(self):
        report = V.Report()
        V.check_variants(PACKAGE, package_prior(), report)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_wrong_prior_code_is_reported(self):
        prior = package_prior()
        prior["rejections"][0]["code"] = "PLAN_ARGUMENT"
        report = V.Report()
        V.check_variants(PACKAGE, prior, report)
        self.assertIn("variant:project-cycle.json", failed(report))


class OutputTests(unittest.TestCase):
    def test_a_missing_output_file_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            (output / "report.md").unlink()
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("output-present:report.md", failed(report))

    def test_an_extra_output_entry_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            (output / "notes").mkdir()
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("output-entries", failed(report))

    def test_a_changed_plan_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            value = json.loads((output / "plan.json").read_text(encoding="utf-8"))
            value["atoms"][0]["position"] = 9
            write_json(output / "plan.json", value)
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("plan-atoms", failed(report))

    def test_a_changed_count_in_the_report_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            value = json.loads((output / "report.json").read_text(encoding="utf-8"))
            value["counts"]["notes"] = 7
            write_json(output / "report.json", value)
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("report-counts", failed(report))

    def test_a_non_zero_model_call_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            value = json.loads((output / "report.json").read_text(encoding="utf-8"))
            value["model_calls"] = 1
            write_json(output / "report.json", value)
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("report-zero-calls", failed(report))

    def test_a_timestamp_in_the_output_manifest_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            value = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            value["created_at"] = "2026-11-08T00:00:00Z"
            write_json(output / "manifest.json", value)
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("output-manifest-strict", failed(report))

    def test_a_changed_digest_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            value = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            value["plan_sha256"] = "f" * 64
            write_json(output / "manifest.json", value)
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("output-digest:plan.json", failed(report))

    def test_a_missing_markdown_section_is_reported(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            output = build_output(sandbox, replay_or_fail())
            (output / "report.md").write_text("# 空\n", encoding="utf-8")
            report = V.Report()
            V.check_output(output, replay_or_fail(), report)
            self.assertIn("markdown-section:## 教学顺序", failed(report))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, *arguments: str) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            code = V.main(list(arguments))
        return code, stdout.getvalue(), stderr.getvalue()

    def test_the_package_alone_passes(self):
        code, stdout, _stderr = self.run_cli("--package", str(PACKAGE))
        self.assertEqual(code, 0, stdout)
        self.assertIn("失败 0", stdout)

    def test_no_arguments_is_a_usage_error(self):
        code, _stdout, stderr = self.run_cli()
        self.assertEqual(code, 2)
        self.assertIn("规划根", stderr)

    def test_freeze_without_a_package_is_a_usage_error(self):
        code, _stdout, stderr = self.run_cli("--freeze")
        self.assertEqual(code, 2)
        self.assertIn("--package", stderr)

    def test_a_tampered_root_exits_one(self):
        with Sandbox() as sandbox:
            root = build_root(sandbox)
            (root / "control" / "inputs.json").write_text("{}", encoding="utf-8")
            code, stdout, _stderr = self.run_cli(str(root))
            self.assertEqual(code, 1)
            self.assertIn("inputs-strict", stdout)


if __name__ == "__main__":
    unittest.main()
