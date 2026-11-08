"""Tests for the independent locked-change auditor."""

import copy
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def load_auditor():
    spec = importlib.util.spec_from_file_location(
        "verify_locked_change", ROOT / "scripts" / "verify_locked_change.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def sha256(data):
    import hashlib

    return hashlib.sha256(data).hexdigest()


def identity(index):
    return {"platform": "test", "device": 1, "inode": index}


def replay_or_fail(declaration):
    return V.replay(declaration)


def build_root(directory, declaration):
    root = directory / "root"
    control = root / "control"
    control.mkdir(parents=True)
    project_bytes = json.dumps(declaration, ensure_ascii=False, indent=2).encode() + b"\n"
    (control / "project.json").write_bytes(project_bytes)
    result = replay_or_fail(declaration)
    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "project_sha256": sha256(project_bytes),
        "notes": len(declaration["notes"]),
        "changes": len(declaration["changes"]),
        "locks": len(declaration["locks"]),
        "refactors": len(declaration["refactors"]),
        "unchanged": result["unchanged"],
    }
    inputs_bytes = V.canonical(inputs).encode()
    (control / "inputs.json").write_bytes(inputs_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "root": str(root),
        "root_identity": identity(1),
        "parent_identity": identity(2),
        "control_identity": identity(3),
        "manifest_identity": identity(4),
        "inputs_identity": identity(5),
        "project_sha256": sha256(project_bytes),
        "inputs_sha256": sha256(inputs_bytes),
        "created_at": "1",
    }
    (control / "manifest.json").write_bytes(V.canonical(manifest).encode())
    return root


def report_markdown(declaration):
    result = replay_or_fail(declaration)
    counts = result["counts"]
    return "\n".join(
        [
            f"# 锁定与变更（{declaration['case_id']}）",
            "## 变更规模",
            f"- 笔记 {counts['notes']} 份",
            "## 已应用的变更",
            "## 锁定冲突（转候选）",
            "## 重构与引用变化",
            "## 未受影响的笔记",
            "## 缺口",
            "## 边界",
            "模型调用与网络请求 0 次。",
        ]
    )


def build_output(directory, declaration):
    output = directory / "out"
    output.mkdir()
    result = replay_or_fail(declaration)
    changes = {
        "policy": V.POLICY,
        "plan_version": V.PLAN_VERSION,
        "case_id": declaration["case_id"],
        "applied": result["applied"],
        "candidates": result["candidates"],
        "refactors": result["refactors"],
        "unchanged": result["unchanged"],
    }
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "description": declaration["description"],
        "verdict": result["verdict"],
        "counts": result["counts"],
        "findings": result["findings"],
        "model_calls": 0,
        "network_requests": 0,
    }
    changes_bytes = (V.canonical(changes) + "\n").encode()
    report_bytes = (V.canonical(report) + "\n").encode()
    markdown_bytes = report_markdown(declaration).encode()
    (output / "changes.json").write_bytes(changes_bytes)
    (output / "report.json").write_bytes(report_bytes)
    (output / "report.md").write_bytes(markdown_bytes)
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": declaration["case_id"],
        "source_root": str(directory / "root"),
        "source_inputs_sha256": sha256(b"x"),
        "changes_sha256": sha256(changes_bytes),
        "report_sha256": sha256(report_bytes),
        "markdown_sha256": sha256(markdown_bytes),
        "verdict": result["verdict"],
        "counts": result["counts"],
    }
    (output / "manifest.json").write_bytes(V.canonical(manifest).encode())
    return output


def audit(root, output=None, package=None):
    report = V.Report()
    declaration = V.load_package(Path(package)) if package else None
    if root is not None:
        V.check_layout(report, Path(root))
        V.check_manifest(report, Path(root))
        if declaration is not None:
            V.check_inputs(report, Path(root), declaration)
    if package is not None:
        V.check_frozen(report, Path(package))
        if declaration is not None:
            V.check_expected(report, Path(package), declaration)
    if output is not None and declaration is not None:
        V.check_output(report, Path(output), declaration)
    return report


def failed(report):
    return {item["name"] for item in report.failures}


def show(report):
    return "\n".join(item["name"] for item in report.failures)


class Sandbox:
    def __enter__(self):
        self.path = Path(tempfile.mkdtemp(prefix="locked-change-"))
        return self.path

    def __exit__(self, *exc):
        shutil.rmtree(self.path, ignore_errors=True)


def declaration():
    return json.loads((ROOT / "fixtures" / "locked-change-v1" / "project.json").read_text(encoding="utf-8"))


class PackageTests(unittest.TestCase):
    def test_the_package_matches_its_frozen_list(self):
        package = ROOT / "fixtures" / "locked-change-v1"
        report = audit(None, None, package)
        self.assertEqual(failed(report), set(), show(report))

    def test_the_prior_matches_the_replay(self):
        package = ROOT / "fixtures" / "locked-change-v1"
        report = V.Report()
        V.check_expected(report, package, V.load_package(package))
        self.assertEqual(failed(report), set(), show(report))

    def test_a_drifted_count_is_detected(self):
        expected = json.loads((ROOT / "fixtures" / "locked-change-v1" / "expected.json").read_text(encoding="utf-8"))
        expected["counts"]["applied"] += 1
        with Sandbox() as directory:
            package = directory / "pkg"
            shutil.copytree(ROOT / "fixtures" / "locked-change-v1", package)
            (package / "expected.json").write_text(json.dumps(expected, ensure_ascii=False), encoding="utf-8")
            report = V.Report()
            V.check_expected(report, package, V.load_package(package))
            self.assertIn("expected-counts", failed(report))

    def test_a_drifted_applied_path_is_detected(self):
        expected = json.loads((ROOT / "fixtures" / "locked-change-v1" / "expected.json").read_text(encoding="utf-8"))
        expected["applied"][0]["path"] = "别的/路径.md"
        with Sandbox() as directory:
            package = directory / "pkg"
            shutil.copytree(ROOT / "fixtures" / "locked-change-v1", package)
            (package / "expected.json").write_text(json.dumps(expected, ensure_ascii=False), encoding="utf-8")
            report = V.Report()
            V.check_expected(report, package, V.load_package(package))
            self.assertIn("expected-applied", failed(report))

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "pkg"
            shutil.copytree(ROOT / "fixtures" / "locked-change-v1", package)
            (package / "project.json").write_text("{}", encoding="utf-8")
            report = V.Report()
            V.check_frozen(report, package)
            self.assertTrue(any(item.startswith("frozen-digest") for item in failed(report)))


class ReplayTests(unittest.TestCase):
    def test_a_clean_declaration_applies_every_change(self):
        result = replay_or_fail(declaration())
        self.assertEqual(result["counts"]["applied"], 3)
        self.assertEqual(result["counts"]["candidates"], 0)
        self.assertEqual(result["verdict"], "pass")

    def test_a_file_lock_turns_a_rename_into_a_candidate(self):
        value = declaration()
        value["changes"][1]["proposed_path"] = "代谢/糖酵解/02-磷酸果糖激酶-重写.md"
        result = replay_or_fail(value)
        self.assertEqual(result["counts"]["candidates"], 1)
        self.assertEqual(result["candidates"][0]["note_id"], "n-pfk")
        self.assertEqual(result["findings"][0]["kind"], "lock_conflict")

    def test_a_directory_lock_turns_a_move_into_a_candidate(self):
        value = declaration()
        value["locks"].append({"lock_id": "lock-dir-gng", "kind": "directory", "path": "代谢/糖异生"})
        value["changes"][2]["proposed_path"] = "代谢/糖异生-新/01-糖异生.md"
        result = replay_or_fail(value)
        self.assertEqual(result["candidates"][0]["note_id"], "n-gng")

    def test_a_section_reorder_is_refused_by_its_lock(self):
        value = declaration()
        value["changes"][0]["proposed_sections"] = ["反应", "定位"]
        result = replay_or_fail(value)
        self.assertEqual(result["counts"]["candidates"], 1)

    def test_an_unlocked_move_is_applied(self):
        value = declaration()
        value["locks"] = [lock for lock in value["locks"] if lock["lock_id"] != "lock-file-pfk"]
        value["changes"][1]["proposed_path"] = "代谢/糖酵解/02-磷酸果糖激酶-重写.md"
        result = replay_or_fail(value)
        self.assertEqual(result["counts"]["applied"], 3)
        self.assertEqual(result["applied"][1]["path"], "代谢/糖酵解/02-磷酸果糖激酶-重写.md")

    def test_a_missed_dependency_is_reported(self):
        value = declaration()
        value["changes"].append({
            "change_id": "chg-x", "kind": "source_added", "topic_id": "t-gly",
            "concepts": ["c-pyr"], "notes": ["n-gly"],
        })
        result = replay_or_fail(value)
        self.assertIn("missed_dependency", [item["kind"] for item in result["findings"]])

    def test_an_unexpected_touch_is_reported(self):
        value = declaration()
        value["changes"].append({
            "change_id": "chg-y", "kind": "style_change", "topic_id": "t-tca",
            "concepts": ["c-gly"], "notes": ["n-gly", "n-pyr"],
        })
        result = replay_or_fail(value)
        kinds = [item["kind"] for item in result["findings"]]
        self.assertIn("unexpected_touch", kinds)
        self.assertNotIn("missed_dependency", kinds)

    def test_unchanged_notes_keep_their_fingerprint(self):
        result = replay_or_fail(declaration())
        self.assertEqual(len(result["unchanged"]), 1)
        self.assertEqual(result["unchanged"][0]["note_id"], "n-pyr")
        self.assertEqual(result["unchanged"][0]["fingerprint"], "e" * 64)

    def test_a_canonical_lock_makes_a_move_unsafe(self):
        value = declaration()
        value["locks"].append({"lock_id": "lock-canonical-tca", "kind": "canonical", "concept_id": "c-tca"})
        self.assertEqual(V.structured_failure(value), "CHANGE_UNSAFE_REFACTOR")

    def test_a_split_without_a_partition_is_unsafe(self):
        value = declaration()
        value["refactors"].append({
            "refactor_id": "r-split", "kind": "split", "note_id": "n-gng",
            "targets": ["代谢/糖异生/01a.md", "代谢/糖异生/01b.md"],
            "references": [{"from": "a", "to": "b"}],
            "atoms": [{"atom_id": "a1", "note_id": "代谢/糖异生/01a.md"}],
        })
        self.assertEqual(V.structured_failure(value), "CHANGE_UNSAFE_REFACTOR")

    def test_a_refactor_without_references_is_refused(self):
        value = declaration()
        value["refactors"][0]["references"] = []
        self.assertEqual(V.structured_failure(value), "CHANGE_ARGUMENT")

    def test_the_under_helper_compares_components(self):
        self.assertTrue(V.under("a/b", "a"))
        self.assertFalse(V.under("a2/b", "a"))


class RootTests(unittest.TestCase):
    def test_a_clean_root_passes(self):
        with Sandbox() as directory:
            value = declaration()
            root = build_root(directory, value)
            report = audit(root, None, ROOT / "fixtures" / "locked-change-v1")
            self.assertEqual(failed(report), set(), show(report))

    def test_an_extra_entry_is_refused(self):
        with Sandbox() as directory:
            root = build_root(directory, declaration())
            (root / "extra.txt").write_text("x", encoding="utf-8")
            report = audit(root, None, None)
            self.assertIn("layout-root-entries", failed(report))

    def test_a_stale_inputs_snapshot_is_refused(self):
        with Sandbox() as directory:
            value = declaration()
            root = build_root(directory, value)
            inputs = json.loads((root / "control" / "inputs.json").read_text(encoding="utf-8"))
            inputs["unchanged"] = []
            (root / "control" / "inputs.json").write_bytes(V.canonical(inputs).encode())
            report = audit(root, None, ROOT / "fixtures" / "locked-change-v1")
            self.assertIn("inputs-unchanged", failed(report))


class OutputTests(unittest.TestCase):
    def test_a_clean_output_passes(self):
        with Sandbox() as directory:
            value = declaration()
            root = build_root(directory, value)
            output = build_output(directory, value)
            report = audit(root, output, ROOT / "fixtures" / "locked-change-v1")
            self.assertEqual(failed(report), set(), show(report))

    def test_a_missing_file_is_reported_not_raised(self):
        with Sandbox() as directory:
            value = declaration()
            output = build_output(directory, value)
            (output / "report.md").unlink()
            report = V.Report()
            V.check_output(report, output, value)
            self.assertIn("output-complete", failed(report))

    def test_a_drifted_counts_block_is_detected(self):
        with Sandbox() as directory:
            value = declaration()
            output = build_output(directory, value)
            report_value = json.loads((output / "report.json").read_text(encoding="utf-8"))
            report_value["counts"]["applied"] += 1
            (output / "report.json").write_bytes((V.canonical(report_value) + "\n").encode())
            report = V.Report()
            V.check_output(report, output, value)
            self.assertIn("output-counts", failed(report))

    def test_a_missing_manifest_is_detected(self):
        with Sandbox() as directory:
            value = declaration()
            output = build_output(directory, value)
            (output / "manifest.json").unlink()
            report = V.Report()
            V.check_output(report, output, value)
            self.assertIn("output-complete", failed(report))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, arguments):
        import contextlib
        import io

        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = V.main(arguments)
        return code, out.getvalue(), err.getvalue()

    def test_package_mode_passes(self):
        code, out, _err = self.run_cli(["--package", str(ROOT / "fixtures" / "locked-change-v1")])
        self.assertEqual(code, 0, out)
        self.assertIn("失败 0", out)

    def test_no_arguments_is_a_usage_error(self):
        code, _out, err = self.run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给一个变更根", err)

    def test_freeze_requires_a_package(self):
        code, _out, err = self.run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", err)

    def test_json_mode_reports_structured_failures(self):
        with Sandbox() as directory:
            root = build_root(directory, declaration())
            (root / "extra.txt").write_text("x", encoding="utf-8")
            code, out, _err = self.run_cli([str(root), "--json"])
            self.assertEqual(code, 1)
            payload = json.loads(out)
            self.assertGreaterEqual(payload["failures"], 1)


if __name__ == "__main__":
    unittest.main()
