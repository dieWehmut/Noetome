"""Tests for scripts/verify_audit_journal.py.

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
SCRIPT = ROOT / "scripts" / "verify_audit_journal.py"
PACKAGE = ROOT / "fixtures" / "journal-audit-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_audit_journal", SCRIPT)
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
    except V.JournalError as error:  # pragma: no cover - a broken fixture
        raise AssertionError(f"重放失败：{error.code}：{error.detail}") from error


def build_root(directory: Path, declaration: dict | None = None) -> Path:
    """Build a journal root shaped exactly like the product's."""
    declaration = declaration or package_declaration()
    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    project_bytes = (PACKAGE / "project.json").read_bytes()
    (control / "project.json").write_bytes(project_bytes)
    value = replay_or_fail(declaration)
    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "project_sha256": V.sha256(project_bytes),
        "tasks": len(declaration["tasks"]),
        "events": len(declaration["events"]),
        "sources": len(declaration["sources"]),
        "objects": len(declaration["objects"]),
        "files": len(declaration["files"]),
    }
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
    journal_bytes = (V.canonical(value["journal"]) + "\n").encode("utf-8")
    report_bytes = (V.canonical(value["report"]) + "\n").encode("utf-8")
    markdown_bytes = V.render_markdown(value).encode("utf-8")
    (output / "journal.json").write_bytes(journal_bytes)
    (output / "report.json").write_bytes(report_bytes)
    (output / "report.md").write_bytes(markdown_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "source_root": str((directory / "root").resolve()),
        "source_inputs_sha256": V.sha256(b""),
        "journal_sha256": V.sha256(journal_bytes),
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
    if root is not None:
        manifest = V.check_manifest(report, root)
        declaration = V.check_project(report, root)
        if declaration is not None:
            V.check_inputs(report, root, declaration)
    value = None
    if declaration is not None:
        try:
            value = V.replay(declaration)
            report.record("replay", True)
        except V.JournalError as error:
            report.record("replay", False, f"{error.code}：{error.detail}")
    V.check_package(report, package, value)
    if value is None and package is not None and (package / "project.json").is_file():
        try:
            value = V.replay_package(package)
            report.record("package-replay", True)
        except V.JournalError as error:
            report.record("package-replay", False, f"{error.code}：{error.detail}")
    V.check_expected(report, package, value)
    V.check_variants(report, package)
    V.check_frozen(report, package)
    V.check_output(report, output, value)
    if manifest is not None and value is not None:
        report.record(
            "manifest-case",
            manifest["case_id"] == value["case_id"],
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
        self.path = Path(tempfile.mkdtemp(prefix="journal-audit-"))
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
        self.assertEqual(frozen["workflow_id"], "audit-journal-v1")

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            target = package / "project.json"
            target.write_text(target.read_text(encoding="utf-8") + " ", encoding="utf-8", newline="\n")
            report = audit(package=package)
            self.assertIn("frozen-digest:project.json", failed(report))

    def test_an_extra_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}", encoding="utf-8", newline="\n")
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
        self.assertGreater(len(report.checks), 30)

    def test_root_alone_passes(self):
        with Sandbox() as directory:
            root = build_root(directory)
            report = audit(root=root)
            self.assertEqual(failed(report), set(), show(report))

    def test_the_control_directory_holds_exactly_three_entries(self):
        with Sandbox() as directory:
            root = build_root(directory)
            self.assertEqual(
                sorted(item.name for item in (root / "control").iterdir()),
                ["inputs.json", "manifest.json", "project.json"],
            )

    def test_root_and_output_and_package_pass_together(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            report = audit(root=root, output=output, package=PACKAGE)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 80)


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_level_entry_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "note.txt").write_text("x", encoding="utf-8", newline="\n")
            self.assertIn("layout-root-entries", failed(audit(root=root)))

    def test_an_extra_control_entry_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "extra.json").write_text("{}", encoding="utf-8", newline="\n")
            self.assertIn("layout-control-entries", failed(audit(root=root)))

    def test_a_symlink_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            create_symlink_or_skip(root / "control" / "link.json", root / "control" / "inputs.json")
            report = audit(root=root)
            self.assertIn("layout-no-symlinks", failed(report))


class ManifestTests(unittest.TestCase):
    def test_an_unknown_field_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["surprise"] = 1
            write_canonical(path, manifest)
            self.assertIn("manifest-strict", failed(audit(root=root)))

    def test_a_pretty_printed_manifest_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            write_json(path, read_json(path))
            self.assertIn("manifest-strict", failed(audit(root=root)))

    def test_a_drifted_root_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["root"] = "/somewhere/else"
            write_canonical(path, manifest)
            self.assertIn("manifest-root", failed(audit(root=root)))

    def test_a_drifted_project_digest_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["project_sha256"] = "0" * 64
            write_canonical(path, manifest)
            self.assertIn("manifest-project-digest", failed(audit(root=root)))

    def test_a_drifted_inputs_digest_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["inputs_sha256"] = "0" * 64
            write_canonical(path, manifest)
            self.assertIn("manifest-inputs-digest", failed(audit(root=root)))

    def test_an_unstructured_identity_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["root_identity"] = "unix:2096:1:0"
            write_canonical(path, manifest)
            self.assertIn("manifest-identity:root_identity", failed(audit(root=root)))


class InputsTests(unittest.TestCase):
    def test_a_drifted_case_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            inputs = read_json(path)
            inputs["case_id"] = "other-case"
            write_canonical(path, inputs)
            self.assertIn("inputs-case", failed(audit(root=root)))

    def test_a_drifted_count_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "inputs.json"
            inputs = read_json(path)
            inputs["events"] = inputs["events"] + 1
            write_canonical(path, inputs)
            report = audit(root=root)
            self.assertIn("inputs-count:events", failed(report))

    def test_a_missing_project_copy_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            (root / "control" / "project.json").unlink()
            self.assertIn("project-present", failed(audit(root=root)))


class ReplayRuleTests(unittest.TestCase):
    def test_events_are_ordered_by_task_then_sequence(self):
        value = replay_or_fail(package_declaration())
        self.assertEqual(
            [event["event_id"] for event in value["journal"]["events"]],
            ["e5", "e6", "e1", "e2", "e3", "e4"],
        )

    def test_reordering_the_declaration_does_not_change_the_journal(self):
        declaration = package_declaration()
        first = replay_or_fail(declaration)
        shuffled = copy.deepcopy(declaration)
        shuffled["events"].reverse()
        shuffled["tasks"].reverse()
        second = replay_or_fail(shuffled)
        self.assertEqual(first["journal"]["events"], second["journal"]["events"])
        self.assertEqual(first["journal"]["tasks"], second["journal"]["tasks"])

    def test_a_followed_failure_is_marked(self):
        value = replay_or_fail(package_declaration())
        followed = {item["event_id"]: item["followed_by_action"] for item in value["journal"]["failures"]}
        self.assertEqual(followed, {"e2": True, "e6": False})

    def test_unresolved_failures_count_only_unfollowed_ones(self):
        value = replay_or_fail(package_declaration())
        self.assertEqual(value["counts"]["located_failures"], 2)
        self.assertEqual(value["counts"]["unresolved_failures"], 1)

    def test_the_outcome_is_the_last_event_kind(self):
        value = replay_or_fail(package_declaration())
        outcomes = {task["task_id"]: task["outcome"] for task in value["journal"]["tasks"]}
        self.assertEqual(outcomes, {"task-compile": "failed", "task-parse": "resolved"})

    def test_every_failure_carries_four_layers(self):
        value = replay_or_fail(package_declaration())
        for failure in value["journal"]["failures"]:
            self.assertTrue(failure["task_id"])
            self.assertTrue(failure["sources"])
            self.assertTrue(failure["objects"])
            self.assertTrue(failure["files"])

    def test_severity_follows_the_kind(self):
        value = replay_or_fail(package_declaration())
        for event in value["journal"]["events"]:
            expected = "error" if event["kind"] == "failed" else "info"
            self.assertEqual(event["severity"], expected)

    def test_a_task_without_sources_is_refused(self):
        declaration = package_declaration()
        declaration["tasks"][0]["sources"] = []
        with self.assertRaises(V.JournalError) as caught:
            V.validate(declaration)
        self.assertEqual(caught.exception.code, "JOURNAL_ARGUMENT")

    def test_a_duplicate_sequence_is_refused(self):
        declaration = package_declaration()
        declaration["events"][2]["at"] = declaration["events"][1]["at"]
        with self.assertRaises(V.JournalError) as caught:
            V.validate(declaration)
        self.assertEqual(caught.exception.code, "JOURNAL_ARGUMENT")

    def test_a_credential_shaped_string_is_refused(self):
        declaration = package_declaration()
        declaration["sources"][0]["description"] = "教材A password=hunter2"
        with self.assertRaises(V.JournalError) as caught:
            V.validate(declaration)
        self.assertEqual(caught.exception.code, "JOURNAL_SECRET_MARKER")

    def test_a_foreign_file_is_refused(self):
        declaration = package_declaration()
        declaration["tasks"][1]["files"] = ["file-glycolysis"]
        with self.assertRaises(V.JournalError) as caught:
            V.validate(declaration)
        self.assertEqual(caught.exception.code, "JOURNAL_ARGUMENT")

    def test_secret_detection_is_case_insensitive(self):
        hits = V.secret_hits([("来源描述", "PASSWORD=hunter2")])
        self.assertTrue(hits)

    def test_the_markdown_names_the_four_layers_and_the_boundary(self):
        value = replay_or_fail(package_declaration())
        markdown = V.render_markdown(value)
        for needle in ("## 失败定位", "## 任务状态", "## 边界", "来源范围", "知识对象", "受影响文件"):
            self.assertIn(needle, markdown)


class ExpectedTests(unittest.TestCase):
    def test_the_prior_matches_the_replay(self):
        report = audit(package=PACKAGE)
        for name in failed(report):
            self.assertNotIn("expected-", name, show(report))

    def test_a_drifted_count_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            expected = read_json(path)
            expected["counts"]["events"] = 7
            write_json(path, expected)
            report = audit(package=package)
            self.assertIn("expected-count:events", failed(report))

    def test_a_drifted_event_order_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            expected = read_json(path)
            expected["event_order"] = list(reversed(expected["event_order"]))
            write_json(path, expected)
            self.assertIn("expected-event-order", failed(audit(package=package)))

    def test_a_drifted_task_row_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            expected = read_json(path)
            expected["tasks"][0]["outcome"] = "none"
            write_json(path, expected)
            self.assertIn("expected-tasks", failed(audit(package=package)))

    def test_a_drifted_failure_flag_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            expected = read_json(path)
            # The prior lists failures in the same task-id order as the journal:
            # e6 (`false`) first, then e2 (`true`). Flipping e2 is the real drift.
            self.assertFalse(expected["failures"][0]["followed_by_action"])
            self.assertTrue(expected["failures"][1]["followed_by_action"])
            expected["failures"][1]["followed_by_action"] = False
            write_json(path, expected)
            self.assertIn("expected-failures", failed(audit(package=package)))

    def test_a_missing_boundary_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            expected = read_json(path)
            expected["boundaries"]["scientific_status"] = "passed"
            write_json(path, expected)
            self.assertIn("expected-boundary", failed(audit(package=package)))


class VariantTests(unittest.TestCase):
    def test_every_declared_variant_is_covered(self):
        cases = read_json(PACKAGE / "cases.json")
        for variant in cases["variants"]:
            if "project" not in variant:
                continue
            declaration = read_json(PACKAGE / variant["project"])
            with self.assertRaises(V.JournalError) as caught:
                V.validate(declaration)
            self.assertEqual(caught.exception.code, variant["expect"]["code"])

    def test_a_variant_that_passes_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            broken = package / "project-no-source.json"
            broken.write_bytes((package / "project.json").read_bytes())
            report = audit(package=package)
            self.assertIn("variant-code:project-no-source.json", failed(report))

    def test_a_missing_variant_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "project-secret.json").unlink()
            report = audit(package=package)
            self.assertIn("variant-present:project-secret.json", failed(report))


class OutputTests(unittest.TestCase):
    def test_an_extra_output_entry_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "extra.json").write_text("{}", encoding="utf-8", newline="\n")
            self.assertIn("output-entries", failed(audit(root=root, output=output)))

    def test_a_missing_output_file_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "report.md").unlink()
            self.assertIn("output-complete", failed(audit(root=root, output=output)))

    def test_a_journal_that_differs_from_the_replay_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, value = build_output(directory)
            journal = read_json(output / "journal.json")
            journal["tasks"][0]["events"] = 0
            write_canonical(output / "journal.json", journal)
            report = audit(root=root, output=output)
            self.assertIn("output-journal", failed(report))

    def test_a_markdown_that_differs_from_the_replay_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            (output / "report.md").write_text("# 别的东西\n", encoding="utf-8", newline="\n")
            self.assertIn("output-markdown", failed(audit(root=root, output=output)))

    def test_an_output_manifest_with_a_timestamp_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["created_at"] = "1794100000"
            write_canonical(path, manifest)
            self.assertIn("output-manifest-strict", failed(audit(root=root, output=output)))

    def test_a_drifted_output_digest_is_reported(self):
        with Sandbox() as directory:
            root = build_root(directory)
            output, _ = build_output(directory)
            path = output / "manifest.json"
            manifest = read_json(path)
            manifest["journal_sha256"] = "0" * 64
            write_canonical(path, manifest)
            self.assertIn("output-manifest-digest:journal.json", failed(audit(root=root, output=output)))


class CommandLineTests(unittest.TestCase):
    def test_package_mode_succeeds(self):
        code, output = run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, output)
        self.assertIn("合计", output)

    def test_no_arguments_is_a_usage_error(self):
        code, _ = run_cli([])
        self.assertEqual(code, 2)

    def test_a_tampered_root_exits_non_zero(self):
        with Sandbox() as directory:
            root = build_root(directory)
            path = root / "control" / "manifest.json"
            manifest = read_json(path)
            manifest["policy"] = "other"
            write_canonical(path, manifest)
            code, output = run_cli([str(root)])
            self.assertEqual(code, 1)
            self.assertIn("manifest-policy", output)

    def test_freeze_without_a_package_is_a_usage_error(self):
        code, output = run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--package", output)

    def test_json_output_is_structured(self):
        code, output = run_cli(["--package", str(PACKAGE), "--json"])
        self.assertEqual(code, 0)
        payload = json.loads(output)
        self.assertEqual(payload["failed"], 0)
        self.assertGreater(payload["total"], 30)


if __name__ == "__main__":
    unittest.main()
