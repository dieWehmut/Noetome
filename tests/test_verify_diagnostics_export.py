"""Tests for scripts/verify_diagnostics_export.py.

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

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_diagnostics_export.py"
PACKAGE = ROOT / "fixtures" / "diagnostics-export-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_diagnostics_export", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    text = json.dumps(value, ensure_ascii=False, indent=2)
    path.write_text(text + ("\n" if trailing_newline else ""), encoding="utf-8")


def write_canonical(path: Path, value) -> None:
    path.write_text(V.canonical(value) + "\n", encoding="utf-8")


def fake_identity(index: int) -> dict:
    return {"platform": "unix", "device": 2096, "inode": 3_561_700 + index}


def package_declaration() -> dict:
    return read_json(PACKAGE / "project.json")


def replay_or_fail(declaration: dict):
    try:
        return V.replay(declaration)
    except V.ExportError as error:  # pragma: no cover - a broken fixture
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
        "selected_categories": value["counts"]["selected_categories"],
        "config": value["counts"]["config"],
        "fingerprints": value["counts"]["fingerprints"],
    }


def build_root(directory: Path, declaration: dict | None = None) -> Path:
    """Build a diagnostics root shaped exactly like the product's."""
    declaration = declaration or package_declaration()
    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    project_bytes = (PACKAGE / "project.json").read_bytes()
    (control / "project.json").write_bytes(project_bytes)
    inputs = inputs_payload(declaration)
    write_canonical(control / "inputs.json", inputs)
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
    export_bytes = (V.canonical(value["export"]) + "\n").encode("utf-8")
    report_bytes = (V.canonical(value["report"]) + "\n").encode("utf-8")
    markdown_bytes = V.render_markdown(value).encode("utf-8")
    (output / "export.json").write_bytes(export_bytes)
    (output / "report.json").write_bytes(report_bytes)
    (output / "report.md").write_bytes(markdown_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "source_root": str((directory / "root").resolve()),
        "source_inputs_sha256": V.sha256((V.canonical(inputs_payload(declaration)) + "\n").encode("utf-8")),
        "export_sha256": V.sha256(export_bytes),
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
            except V.ExportError as error:
                report.record("replay", False, f"{error.code}：{error.detail}")
        V.check_inputs(report, root, value, declaration)
    V.check_package(report, package, value)
    if value is None and package is not None and (package / "project.json").is_file():
        try:
            value = V.replay_package(package)
            report.record("package-replay", True)
        except V.ExportError as error:
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
        self.path = Path(tempfile.mkdtemp(prefix="diagnostics-audit-"))
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
        self.assertEqual(frozen["workflow_id"], "diagnostics-export-v1")

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            target = package / "project.json"
            target.write_text(target.read_text(encoding="utf-8") + " ", encoding="utf-8")
            report = audit(package=package)
            self.assertIn("frozen-digest:project.json", failed(report))

    def test_an_extra_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}", encoding="utf-8")
            report = audit(package=package)
            self.assertIn("frozen-members", failed(report))

    def test_refreezing_is_byte_stable(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            before = (package / "frozen-sha256.json").read_bytes()
            code, _ = run_cli(["--package", str(package), "--freeze"])
            self.assertEqual(code, 0)
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
            output, _ = build_output(directory)
            self.assertEqual(
                sorted(item.name for item in output.iterdir()),
                ["export.json", "manifest.json", "report.json", "report.md"],
            )


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_entry_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "notes").mkdir()
            report = audit(root=root)
            self.assertIn("layout-root-entries", failed(report))

    def test_an_extra_control_entry_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "extra.json").write_text("{}", encoding="utf-8")
            report = audit(root=root)
            self.assertIn("layout-control-entries", failed(report))

    def test_a_symlink_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "link.json").symlink_to(root / "control" / "project.json")
            report = audit(root=root)
            self.assertIn("layout-no-symlinks", failed(report))


class ManifestTests(unittest.TestCase):
    def test_an_unknown_field_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            value = read_json(path)
            value["extra"] = 1
            write_canonical(path, value)
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
            value = read_json(path)
            value["root"] = "/somewhere/else"
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("manifest-root", failed(report))

    def test_a_drifted_producer_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            value = read_json(path)
            value["producer"] = "noetome-journal"
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("manifest-producer", failed(report))

    def test_a_drifted_project_digest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            value = read_json(path)
            value["project_sha256"] = "0" * 64
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("manifest-project-digest", failed(report))

    def test_a_string_identity_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            value = read_json(path)
            value["root_identity"] = "unix:2096:3561797"
            write_canonical(path, value)
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
            value = read_json(path)
            value["case_id"] = "other"
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("inputs-case", failed(report))

    def test_a_drifted_entry_count_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            value = read_json(path)
            value["entries"] = 9
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("inputs-count:entries", failed(report))

    def test_a_drifted_project_digest_is_detected(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            value = read_json(path)
            value["project_sha256"] = "f" * 64
            write_canonical(path, value)
            report = audit(root=root)
            self.assertIn("inputs-project-digest", failed(report))


class ReplayRuleTests(unittest.TestCase):
    def test_the_counts_follow_the_selection(self):
        value = replay_or_fail(package_declaration())
        self.assertEqual(value["counts"]["categories"], 3)
        self.assertEqual(value["counts"]["selected_categories"], 2)
        self.assertEqual(value["counts"]["excluded_categories"], 1)
        self.assertEqual(value["counts"]["entries"], 4)
        self.assertEqual(value["counts"]["selected_entries"], 3)
        self.assertEqual(value["counts"]["exported_bytes"], 700)
        self.assertEqual(value["counts"]["excluded_bytes"], 9000)

    def test_an_unselected_category_carries_no_entries(self):
        value = replay_or_fail(package_declaration())
        self.assertNotIn("src-1", [entry["entry_id"] for entry in value["entries"]])
        category = [item for item in value["categories"] if item["category_id"] == "source-body"][0]
        self.assertFalse(category["included"])
        self.assertEqual(category["exported_bytes"], 0)
        self.assertEqual(category["bytes"], 9000)
        self.assertEqual(category["preview"], "糖酵解发生在细胞质中，净产生两分子 ATP。")

    def test_a_sensitive_category_needs_recorded_consent(self):
        declaration = package_declaration()
        declaration["selection"].append("source-body")
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_CONSENT")

    def test_consent_lets_the_sensitive_category_through(self):
        declaration = package_declaration()
        declaration["selection"].append("source-body")
        declaration["consent"]["sensitive_approved"] = True
        value = replay_or_fail(declaration)
        self.assertEqual(value["counts"]["sensitive_included"], 1)
        self.assertEqual(value["counts"]["exported_bytes"], 9700)
        self.assertIn("src-1", [entry["entry_id"] for entry in value["entries"]])

    def test_consent_alone_does_not_pull_in_an_unselected_category(self):
        declaration = package_declaration()
        declaration["consent"]["sensitive_approved"] = True
        value = replay_or_fail(declaration)
        self.assertEqual(value["counts"]["sensitive_included"], 0)
        self.assertEqual(value["counts"]["exported_bytes"], 700)

    def test_an_absolute_path_anywhere_is_refused(self):
        declaration = package_declaration()
        declaration["categories"][0]["entries"][0]["preview"] = "见 /home/lyinn/x 下的样例。"
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ABSOLUTE_PATH")

    def test_a_drive_letter_is_refused(self):
        declaration = package_declaration()
        declaration["config"][0]["value"] = "D:\\Noetome\\model.json"
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ABSOLUTE_PATH")

    def test_a_relative_path_is_accepted(self):
        declaration = package_declaration()
        declaration["config"][0]["value"] = "configs/model.json"
        value = replay_or_fail(declaration)
        self.assertEqual(value["counts"]["config"], 3)

    def test_a_credential_shaped_string_is_refused(self):
        declaration = package_declaration()
        declaration["categories"][1]["entries"][0]["preview"] = "password=hunter2"
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_SECRET_MARKER")

    def test_an_unknown_selection_is_refused(self):
        declaration = package_declaration()
        declaration["selection"].append("network-capture")
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ARGUMENT")

    def test_an_empty_category_is_refused(self):
        declaration = package_declaration()
        declaration["categories"][1]["entries"] = []
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ARGUMENT")
        self.assertIn("空类别", caught.exception.detail)

    def test_a_preview_over_the_limit_is_refused(self):
        declaration = package_declaration()
        declaration["limits"]["max_preview"] = 5
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ARGUMENT")

    def test_a_fingerprint_that_is_not_a_digest_is_refused(self):
        declaration = package_declaration()
        declaration["fingerprints"][0]["digest"] = "not-a-digest"
        with self.assertRaises(V.ExportError) as caught:
            V.replay(declaration)
        self.assertEqual(caught.exception.code, "DIAGNOSTICS_ARGUMENT")

    def test_every_entry_carries_its_category_and_preview(self):
        value = replay_or_fail(package_declaration())
        for entry in value["entries"]:
            self.assertEqual(set(entry), {"entry_id", "category_id", "label", "bytes", "preview"})
            self.assertTrue(entry["preview"])


class VariantTests(unittest.TestCase):
    def test_every_declared_variant_is_refused_with_its_code(self):
        cases = read_json(PACKAGE / "cases.json")
        for variant in cases["variants"]:
            declaration = read_json(PACKAGE / variant["project"])
            with self.assertRaises(V.ExportError) as caught:
                V.replay(declaration)
            self.assertEqual(caught.exception.code, variant["expect"]["code"], variant["project"])

    def test_the_prior_rejections_match_the_case_list(self):
        cases = read_json(PACKAGE / "cases.json")
        expected = read_json(PACKAGE / "expected.json")
        declared = [
            {
                "project": variant["project"],
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
            path = package / "expected.json"
            value = read_json(path)
            value["rejections"][0]["code"] = "DIAGNOSTICS_ARGUMENT"
            write_json(path, value)
            report = audit(package=package)
            self.assertIn("expected-rejections", failed(report))


class ExpectedTests(unittest.TestCase):
    def test_the_prior_counts_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        self.assertEqual(set(expected["counts"]), V.COUNT_FIELDS)
        for key, want in expected["counts"].items():
            self.assertEqual(value["counts"][key], want, key)

    def test_the_prior_categories_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        keys = ("category_id", "sensitive", "included", "entries", "selected_entries", "bytes", "exported_bytes")
        self.assertEqual([{key: item[key] for key in keys} for item in value["categories"]], expected["categories"])

    def test_the_prior_entries_match(self):
        value = replay_or_fail(package_declaration())
        expected = read_json(PACKAGE / "expected.json")
        keys = ("entry_id", "category_id", "bytes", "preview")
        self.assertEqual([{key: item[key] for key in keys} for item in value["entries"]], expected["entries"])

    def test_a_drifted_prior_count_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            value = read_json(path)
            value["counts"]["exported_bytes"] += 1
            write_json(path, value)
            report = audit(package=package)
            self.assertIn("expected-count:exported_bytes", failed(report))

    def test_a_drifted_prior_category_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            value = read_json(path)
            value["categories"][2]["included"] = True
            write_json(path, value)
            report = audit(package=package)
            self.assertIn("expected-categories", failed(report))

    def test_a_drifted_prior_entry_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            value = read_json(path)
            value["entries"][0]["bytes"] = 121
            write_json(path, value)
            report = audit(package=package)
            self.assertIn("expected-entries", failed(report))

    def test_a_prior_with_a_timestamp_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            value = read_json(path)
            value["determinism"]["manifest_has_created_at"] = True
            write_json(path, value)
            report = audit(package=package)
            self.assertIn("expected-determinism", failed(report))


class OutputTests(unittest.TestCase):
    def test_a_missing_output_file_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "report.md").unlink()
            report = audit(root=root, output=output)
            self.assertIn("output-complete", failed(report))

    def test_an_extra_output_file_is_detected(self):
        with Sandbox() as directory:
            output, _ = build_output(directory)
            (output / "extra.json").write_text("{}", encoding="utf-8")
            report = audit(output=output)
            self.assertIn("output-entries", failed(report))

    def test_a_timestamped_output_manifest_is_detected(self):
        with Sandbox() as directory:
            output, value = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["created_at"] = "1794100000"
            write_canonical(path, manifest)
            report = audit(output=output, )
            self.assertIn("output-manifest-strict", failed(report))

    def test_a_drifted_output_digest_is_detected(self):
        with Sandbox() as directory:
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["export_sha256"] = "0" * 64
            write_canonical(path, manifest)
            report = audit(output=output)
            self.assertIn("output-manifest-digest:export.json", failed(report))

    def test_a_drifted_output_count_is_detected(self):
        with Sandbox() as directory:
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["counts"]["entries"] += 1
            write_canonical(path, manifest)
            report = audit(output=output, package=PACKAGE)
            self.assertIn("output-counts", failed(report))

    def test_a_drifted_export_is_detected(self):
        with Sandbox() as directory:
            output, value = build_output(directory)
            path = output / "export.json"
            exported = read_json(path)
            exported["entries"] = exported["entries"][:1]
            write_canonical(path, exported)
            report = audit(output=output, package=PACKAGE)
            self.assertIn("output-export", failed(report))

    def test_a_drifted_report_is_detected(self):
        with Sandbox() as directory:
            output, _ = build_output(directory)
            path = output / "report.json"
            result = read_json(path)
            result["counts"]["sensitive_included"] = 1
            write_canonical(path, result)
            report = audit(output=output, package=PACKAGE)
            self.assertIn("output-report", failed(report))

    def test_a_drifted_markdown_is_detected(self):
        with Sandbox() as directory:
            output, _ = build_output(directory)
            path = output / "report.md"
            path.write_text(path.read_text(encoding="utf-8") + "多一行\n", encoding="utf-8")
            report = audit(output=output, package=PACKAGE)
            self.assertIn("output-markdown", failed(report))


class CommandLineTests(unittest.TestCase):
    def test_package_mode_succeeds(self):
        code, text = run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, text)
        self.assertIn("失败 0", text)

    def test_root_and_output_succeed(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            code, text = run_cli([str(root), "--output", str(output), "--package", str(PACKAGE)])
            self.assertEqual(code, 0, text)

    def test_no_arguments_is_a_usage_error(self):
        code, text = run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给一个诊断根或 --package", text)

    def test_a_tampered_root_exits_one(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            value = read_json(path)
            value["policy"] = "other"
            write_canonical(path, value)
            code, text = run_cli([str(root)])
            self.assertEqual(code, 1)
            self.assertIn("manifest-policy", text)

    def test_freeze_without_package_is_a_usage_error(self):
        code, text = run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", text)

    def test_json_output_is_machine_readable(self):
        code, text = run_cli(["--package", str(PACKAGE), "--json"])
        self.assertEqual(code, 0)
        payload = json.loads(text)
        self.assertEqual(payload["failed"], 0)
        self.assertGreater(payload["total"], 40)


if __name__ == "__main__":
    unittest.main()
