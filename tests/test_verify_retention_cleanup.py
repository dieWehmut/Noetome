"""Tests for scripts/verify_retention_cleanup.py.

The auditor is loaded as a module and driven directly: these tests never run the
`noetome` binary and never import the Rust crate, so a green run here says the
independent derivation agrees with the frozen package — nothing more.
"""

from __future__ import annotations

import contextlib
import copy
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from symlink_test_support import create_symlink_or_skip

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_retention_cleanup.py"
PACKAGE = ROOT / "fixtures" / "retention-cleanup-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_retention_cleanup", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    text = json.dumps(value, ensure_ascii=False, indent=2)
    path.write_text(text + ("\n" if trailing_newline else ""), encoding="utf-8", newline="\n")


def write_canonical(path: Path, value) -> None:
    path.write_text(V.canonical(value) + "\n", encoding="utf-8", newline="\n")


def fake_identity(index: int) -> dict:
    return {"platform": "unix", "device": 2096, "inode": 3_561_700 + index}


def package_declaration() -> dict:
    return read_json(PACKAGE / "project.json")


def replay_or_fail(declaration: dict):
    try:
        return V.replay(declaration)
    except V.RetentionError as error:  # pragma: no cover - a broken fixture
        raise AssertionError(f"重放失败：{error.code}：{error.detail}") from error


def inputs_payload(declaration: dict) -> dict:
    """The frozen-input record the product writes, derived from the declaration."""
    value = replay_or_fail(declaration)
    project_bytes = (PACKAGE / "project.json").read_bytes()
    return {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "project_sha256": V.sha256(project_bytes),
        "categories": value["counts"]["categories"],
        "entries": value["counts"]["entries"],
        "published": value["counts"]["published"],
        "deleted": value["counts"]["deleted"],
        "kept": value["counts"]["kept"],
        "protected": value["counts"]["protected"],
    }


def build_root(directory: Path, declaration: dict | None = None) -> Path:
    """Build a retention root shaped exactly like the product's."""
    declaration = declaration or package_declaration()
    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    project_bytes = (PACKAGE / "project.json").read_bytes()
    (control / "project.json").write_bytes(project_bytes)
    write_canonical(control / "inputs.json", inputs_payload(declaration))
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
        "project_sha256": V.sha256(project_bytes),
        "inputs_sha256": V.sha256((control / "inputs.json").read_bytes()),
        "created_at": "1794100000",
    }
    write_canonical(control / "manifest.json", manifest)
    return root


def build_output(directory: Path, declaration: dict | None = None) -> tuple[Path, dict]:
    declaration = declaration or package_declaration()
    value = replay_or_fail(declaration)
    output = directory / "out"
    output.mkdir(parents=True)
    plan_bytes = (V.canonical(value["plan"]) + "\n").encode("utf-8")
    report_bytes = (V.canonical(value["report"]) + "\n").encode("utf-8")
    markdown_bytes = V.render_markdown(value).encode("utf-8")
    (output / "plan.json").write_bytes(plan_bytes)
    (output / "report.json").write_bytes(report_bytes)
    (output / "report.md").write_bytes(markdown_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "source_root": str((directory / "root").resolve()),
        "source_inputs_sha256": V.sha256((V.canonical(inputs_payload(declaration)) + "\n").encode("utf-8")),
        "plan_sha256": V.sha256(plan_bytes),
        "report_sha256": V.sha256(report_bytes),
        "markdown_sha256": V.sha256(markdown_bytes),
        "counts": value["counts"],
    }
    write_canonical(output / "manifest.json", manifest)
    return output, value


def audit(root=None, output=None, package=None) -> "V.Report":
    report = V.Report()
    V.check_layout(report, root)
    declaration = None
    manifest = None
    value = None
    if root is not None:
        manifest = V.check_manifest(report, root)
        declaration = V.check_project(report, root)
        if declaration is not None:
            try:
                value = V.replay(declaration)
                report.record("replay", True)
            except V.RetentionError as error:
                report.record("replay", False, f"{error.code}：{error.detail}")
        V.check_inputs(report, root, value, declaration)
    V.check_package(report, package, value)
    if value is None and package is not None and (package / "project.json").is_file():
        try:
            value = V.replay_package(package)
            report.record("package-replay", True)
        except V.RetentionError as error:
            report.record("package-replay", False, f"{error.code}：{error.detail}")
    V.check_expected(report, package, value)
    V.check_variants(report, package)
    V.check_frozen(report, package)
    V.check_output(report, output, value)
    if manifest is not None and value is not None:
        report.record(
            "manifest-case",
            manifest["case_id"] == value["report"]["case_id"],
            "清单登记的 case_id 与重放不符",
        )
    return report


def failed(report) -> set[str]:
    return {name for name, ok, _ in report.checks if not ok}


def show(report) -> str:
    return "\n".join(f"{name}：{detail}" for name, ok, detail in report.checks if not ok)


def run_cli(argv: list[str]) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = V.main(argv)
    return code, out.getvalue() + err.getvalue()


class Sandbox:
    def __enter__(self) -> Path:
        self.path = Path(tempfile.mkdtemp(prefix="retention-audit-"))
        return self.path

    def __exit__(self, *exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)


class FrozenPackageTests(unittest.TestCase):
    def test_the_freeze_covers_every_member(self):
        frozen = read_json(PACKAGE / "frozen-sha256.json")
        members = {
            member.relative_to(PACKAGE).as_posix()
            for member in PACKAGE.rglob("*")
            if member.is_file() and member.name != "frozen-sha256.json"
        }
        self.assertEqual(set(frozen["files"]), members)

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            path.write_text(path.read_text(encoding="utf-8") + " ", encoding="utf-8", newline="\n")
            report = audit(package=package)
            self.assertIn("frozen-digest:expected.json", failed(report))

    def test_an_extra_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}\n", encoding="utf-8", newline="\n")
            report = audit(package=package)
            self.assertIn("frozen-members", failed(report))

    def test_refreezing_is_byte_stable(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            before = (package / "frozen-sha256.json").read_bytes()
            self.assertEqual(V.main(["--package", str(package), "--freeze"]), 0)
            self.assertEqual((package / "frozen-sha256.json").read_bytes(), before)


class CleanRootTests(unittest.TestCase):
    def test_the_package_alone_passes(self):
        report = audit(package=PACKAGE)
        self.assertEqual(failed(report), set(), show(report))
        self.assertGreater(len(report.checks), 40)

    def test_root_alone_passes(self):
        with Sandbox() as directory:
            root = build_root(directory)
            report = audit(root=root)
            self.assertEqual(failed(report), set(), show(report))

    def test_root_and_output_together_pass(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 80)

    def test_the_root_holds_only_control(self):
        with Sandbox() as directory:
            root = build_root(directory)
            self.assertEqual(sorted(item.name for item in root.iterdir()), ["control"])
            self.assertEqual(
                sorted(item.name for item in (root / "control").iterdir()),
                ["inputs.json", "manifest.json", "project.json"],
            )

    def test_the_output_holds_exactly_four_files(self):
        with Sandbox() as directory:
            build_root(directory)
            output, _ = build_output(directory)
            self.assertEqual(
                sorted(item.name for item in output.iterdir()),
                ["manifest.json", "plan.json", "report.json", "report.md"],
            )


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_entry_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "note.txt").write_text("note", encoding="utf-8", newline="\n")
            report = audit(root=root)
            self.assertIn("layout-root-entries", failed(report))

    def test_an_extra_control_entry_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "note.txt").write_text("note", encoding="utf-8", newline="\n")
            report = audit(root=root)
            self.assertIn("layout-control-entries", failed(report))

    def test_a_symlink_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            create_symlink_or_skip(root / "control" / "link.json", root / "control" / "inputs.json")
            report = audit(root=root)
            self.assertIn("layout-no-symlinks", failed(report))


class ManifestTests(unittest.TestCase):
    def test_an_unknown_field_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["extra"] = 1
            write_canonical(path, manifest)
            report = audit(root=root)
            self.assertIn("manifest-strict", failed(report))

    def test_pretty_json_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            write_json(path, read_json(path))
            report = audit(root=root)
            self.assertIn("manifest-strict", failed(report))

    def test_a_drifted_root_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["root"] = str(directory / "somewhere-else")
            write_canonical(path, manifest)
            report = audit(root=root)
            self.assertIn("manifest-root", failed(report))

    def test_a_drifted_producer_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["producer"] = "someone-else"
            write_canonical(path, manifest)
            report = audit(root=root)
            self.assertIn("manifest-producer", failed(report))

    def test_a_drifted_project_digest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "project.json"
            path.write_bytes(path.read_bytes() + b" ")
            report = audit(root=root)
            self.assertIn("manifest-project-digest", failed(report))

    def test_a_string_identity_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["root_identity"] = "unix:2096:3561701"
            write_canonical(path, manifest)
            report = audit(root=root)
            self.assertIn("manifest-identity:root_identity", failed(report))

    def test_a_missing_manifest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "manifest.json").unlink()
            report = audit(root=root)
            self.assertIn("manifest-present", failed(report))


class InputsTests(unittest.TestCase):
    def test_a_drifted_case_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            inputs = read_json(path)
            inputs["case_id"] = "another-case"
            write_canonical(path, inputs)
            report = audit(root=root)
            self.assertIn("inputs-case", failed(report))

    def test_a_drifted_entry_count_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            inputs = read_json(path)
            inputs["entries"] = 4
            write_canonical(path, inputs)
            report = audit(root=root)
            self.assertIn("inputs-count:entries", failed(report))

    def test_a_drifted_project_digest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            inputs = read_json(path)
            inputs["project_sha256"] = "0" * 64
            write_canonical(path, inputs)
            report = audit(root=root)
            self.assertIn("inputs-project-digest", failed(report))


class ReplayRuleTests(unittest.TestCase):
    def test_the_counts_follow_the_retention_policy(self):
        value = replay_or_fail(package_declaration())
        counts = value["counts"]
        self.assertEqual(counts["entries"], 5)
        self.assertEqual(counts["deleted"], 2)
        self.assertEqual(counts["kept"], 3)
        self.assertEqual(counts["protected"], 2)
        self.assertEqual(counts["deleted_bytes"], 9400)
        self.assertEqual(counts["kept_bytes"], 2200)
        self.assertEqual(counts["lost_recovery"], 1)
        self.assertEqual(counts["shortened_traces"], 1)
        self.assertEqual(counts["model_calls"], 0)
        self.assertEqual(counts["network_requests"], 0)

    def test_an_expired_entry_is_deleted_and_a_fresh_one_is_kept(self):
        value = replay_or_fail(package_declaration())
        actions = {action["entry_id"]: action for action in value["plan"]["actions"]}
        self.assertEqual(actions["cache-1"]["action"], "delete")
        self.assertEqual(actions["cache-1"]["reason"], "超过保留期")
        self.assertEqual(actions["log-2"]["action"], "keep")
        self.assertEqual(actions["log-2"]["reason"], "未超过保留期")

    def test_a_published_entry_is_kept_even_when_it_is_old(self):
        declaration = copy.deepcopy(package_declaration())
        for entry in declaration["entries"]:
            if entry["entry_id"] == "index-1":
                entry["age_days"] = 29
        value = replay_or_fail(declaration)
        actions = {action["entry_id"]: action for action in value["plan"]["actions"]}
        self.assertEqual(actions["index-1"]["action"], "keep")
        self.assertEqual(actions["index-1"]["reason"], "已发布版本仍需引用")
        self.assertEqual(actions["index-1"]["release_id"], "rel-1")

    def test_a_policy_that_would_expire_a_required_entry_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["categories"][1]["keep_days"] = 10
        with self.assertRaises(V.RetentionError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "RETENTION_PROTECTED")

    def test_a_requirement_must_exist(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["published"][0]["required_index"] = ["index-2"]
        with self.assertRaises(V.RetentionError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "RETENTION_ARGUMENT")

    def test_an_unknown_category_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["entries"][0]["category_id"] = "unknown"
        with self.assertRaises(V.RetentionError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "RETENTION_ARGUMENT")

    def test_a_duplicate_entry_is_refused(self):
        declaration = copy.deepcopy(package_declaration())
        declaration["entries"][1]["entry_id"] = "log-1"
        with self.assertRaises(V.RetentionError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "RETENTION_ARGUMENT")

    def test_recovery_loss_only_covers_deleted_entries_that_had_one(self):
        value = replay_or_fail(package_declaration())
        loss = value["plan"]["recovery_loss"]
        self.assertEqual([item["entry_id"] for item in loss], ["log-1"])

    def test_a_shortened_trace_counts_what_is_left(self):
        value = replay_or_fail(package_declaration())
        changes = value["plan"]["trace_changes"]
        self.assertEqual(len(changes), 1)
        self.assertEqual(changes[0]["trace"], "trace-parse")
        self.assertEqual(changes[0]["before"], 3)
        self.assertEqual(changes[0]["after"], 2)
        self.assertEqual(changes[0]["removed"], ["log-1"])

    def test_actions_are_ordered_by_entry_id(self):
        value = replay_or_fail(package_declaration())
        ids = [action["entry_id"] for action in value["plan"]["actions"]]
        self.assertEqual(ids, sorted(ids))

    def test_the_markdown_names_the_impact_and_the_boundary(self):
        text = V.render_markdown(replay_or_fail(package_declaration()))
        self.assertIn("## 影响说明", text)
        self.assertIn("恢复能力「按日志重放失败任务」随之丧失", text)
        self.assertIn("追溯链 trace-parse 会从 3 条变短到 2 条，删除的是 log-1", text)
        self.assertIn("index-1（rel-1）：已发布版本必需的索引条目。", text)
        self.assertIn("只生成计划：不删除任何文件", text)
        self.assertIn("命中保护即整条拒绝", text)


class VariantTests(unittest.TestCase):
    def test_every_declared_variant_is_refused_with_its_code(self):
        cases = read_json(PACKAGE / "cases.json")
        for variant in cases["variants"]:
            declaration = read_json(PACKAGE / variant["project"])
            with self.assertRaises(V.RetentionError) as caught:
                V.replay(declaration)
            self.assertEqual(
                caught.exception.code,
                variant["expect"]["code"],
                variant["project"],
            )

    def test_the_prior_rejections_match_the_case_list(self):
        cases = read_json(PACKAGE / "cases.json")
        expected = read_json(PACKAGE / "expected.json")
        declared = [
            {
                "variant": variant["project"],
                "code": variant["expect"]["code"],
                "exit": variant["expect"]["exit"],
            }
            for variant in cases["variants"]
        ]
        self.assertEqual(declared, expected["rejections"])

    def test_a_drifted_rejection_code_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            expected = read_json(package / "expected.json")
            expected["rejections"][0]["code"] = "RETENTION_ARGUMENT"
            write_json(package / "expected.json", expected)
            report = audit(package=package)
            self.assertIn("expected-rejections", failed(report))


class ExpectedTests(unittest.TestCase):
    def test_the_prior_counts_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        self.assertEqual(value["counts"], expected["counts"])

    def test_the_prior_actions_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        self.assertEqual(value["plan"]["actions"], expected["actions"])

    def test_the_prior_protected_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        self.assertEqual(value["plan"]["protected"], expected["protected"])

    def test_a_drifted_prior_count_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            expected = read_json(package / "expected.json")
            expected["counts"]["deleted"] = 3
            write_json(package / "expected.json", expected)
            report = audit(package=package)
            self.assertIn("expected-count:deleted", failed(report))

    def test_a_drifted_prior_action_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            expected = read_json(package / "expected.json")
            expected["actions"][0]["reason"] = "超过保留期（另一个理由）"
            write_json(package / "expected.json", expected)
            report = audit(package=package)
            self.assertIn("expected-actions", failed(report))

    def test_a_drifted_prior_trace_change_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            expected = read_json(package / "expected.json")
            expected["trace_changes"][0]["after"] = 1
            write_json(package / "expected.json", expected)
            report = audit(package=package)
            self.assertIn("expected-trace-changes", failed(report))

    def test_a_prior_that_claims_to_delete_files_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            expected = read_json(package / "expected.json")
            expected["boundaries"]["deletes_files"] = True
            write_json(package / "expected.json", expected)
            report = audit(package=package)
            self.assertIn("expected-no-deletion", failed(report))


class OutputTests(unittest.TestCase):
    def test_a_missing_output_file_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "report.md").unlink()
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-complete", failed(report))

    def test_an_extra_output_file_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "extra.json").write_text("{}\n", encoding="utf-8", newline="\n")
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-entries", failed(report))

    def test_a_timestamped_output_manifest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["created_at"] = "1794100000"
            write_canonical(path, manifest)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-manifest-strict", failed(report))

    def test_a_drifted_output_digest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["plan_sha256"] = "0" * 64
            write_canonical(path, manifest)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-manifest-digest:plan.json", failed(report))

    def test_a_drifted_output_count_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["counts"]["deleted"] = 9
            write_canonical(path, manifest)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-counts", failed(report))

    def test_a_drifted_plan_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, value = build_output(directory)
            plan = read_json(output / "plan.json")
            plan["actions"][0]["action"] = "keep"
            write_canonical(output / "plan.json", plan)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-plan", failed(report))

    def test_a_drifted_report_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            report_json = read_json(output / "report.json")
            report_json["counts"]["protected"] = 0
            write_canonical(output / "report.json", report_json)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-report", failed(report))

    def test_a_drifted_markdown_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            text = (output / "report.md").read_text(encoding="utf-8")
            (output / "report.md").write_text(text.replace("## 受保护条目", "## 其它"), encoding="utf-8", newline="\n")
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertIn("output-markdown", failed(report))


class CommandLineTests(unittest.TestCase):
    def test_package_mode_succeeds(self):
        code, output = run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, output)
        self.assertIn("失败 0", output)

    def test_root_and_output_succeed(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output_dir, _ = build_output(directory)
            code, text = run_cli(
                [str(root), "--output", str(output_dir), "--package", str(PACKAGE)]
            )
            self.assertEqual(code, 0, text)

    def test_no_arguments_is_a_usage_error(self):
        code, output = run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给", output)

    def test_a_tampered_root_exits_one(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["producer"] = "someone-else"
            write_canonical(path, manifest)
            code, output = run_cli([str(root)])
            self.assertEqual(code, 1)
            self.assertIn("manifest-producer", output)

    def test_freeze_without_package_is_a_usage_error(self):
        code, output = run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", output)

    def test_json_output_is_machine_readable(self):
        code, output = run_cli(["--package", str(PACKAGE), "--json"])
        self.assertEqual(code, 0, output)
        payload = json.loads(output[output.index("{") :])
        self.assertEqual(payload["failed"], 0)
        self.assertEqual(payload["total"], len(payload["checks"]))


if __name__ == "__main__":
    unittest.main()
