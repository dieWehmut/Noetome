"""Tests for the independent scale-harness auditor.

The auditor is loaded by path so these tests never import the Rust product and
never run the CLI to decide anything.
"""

from __future__ import annotations

import copy
import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUDITOR = ROOT / "scripts" / "verify_scale_harness.py"
PACKAGE = ROOT / "fixtures" / "scale-harness-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_scale_harness", AUDITOR)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
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
    path.write_text(text, encoding="utf-8")


def write_canonical(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(V.canonical(value) + "\n", encoding="utf-8")


def package_spec() -> dict:
    return json.loads((PACKAGE / "spec.json").read_text(encoding="utf-8"))


def package_prior() -> dict:
    return json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))


def replay_or_fail() -> dict:
    return V.replay(package_spec()["spec"])


def report_markdown(structure: dict) -> str:
    counts = structure["counts"]
    limits = structure["limits"]
    lines = [
        "# 规模实测（scale-synthetic-v1）",
        "",
        "这是**合成语料**的实测：页面由参数生成，不含任何真实教材内容，也不证明真实教材的抽取质量。",
        "",
        "## 参数",
        "",
        f'- 页数 {structure["spec"]["pages"]}；每页标题 {structure["spec"]["headings_per_page"]}；'
        f'每页段落 {structure["spec"]["paragraphs_per_page"]}',
        "",
        "## 结构产物",
        "",
        f'- 页面 {counts["pages"]}、字节 {counts["bytes"]}、文本片段 {counts["spans"]}、'
        f'标题 {counts["headings"]}、段落 {counts["paragraphs"]}、概念 {counts["concepts"]}、'
        f'出现 {counts["occurrences"]}、对齐警告 {counts["warnings"]}',
        "",
        "## 实测",
        "",
        "- 耗时与峰值内存写在 `timings.json`，本文件不复制它们：报告是摘要产物，",
        "  必须与结构产物一起保持逐字节确定，任何挂钟读数都会让同一参数两次运行不再相同。",
        "",
        "## 规划上限（未达标项）",
        "",
        f'- 结论：{structure["nfr10_status"]}',
        f'- 上限：topics={limits["max_topics"]}、goals={limits["max_goals"]}、'
        f'concepts={limits["max_concepts"]}、atoms={limits["max_atoms"]}',
        f'- {limits["detail"]}',
        "",
        "## 边界",
        "",
        "- 结构产物只由参数决定，两次同参数运行的字节相同；耗时单独成文件，不参与确定性比较。",
        "- 页面是合成的，不证明真实教材的抽取质量，也不代表科学正确性。",
        "- 不联网、不调用模型、不写项目与 Vault；本阶段不生成知识。",
        "",
    ]
    return "\n".join(lines)


def build_output(directory: Path, structure: dict) -> Path:
    output = directory / "out"
    output.mkdir(parents=True, exist_ok=True)
    structure_text = V.canonical(structure) + "\n"
    write_canonical(output / "structure.json", structure)
    timings = {
        "analyse_millis": 1,
        "align_millis": 2,
        "total_millis": 3,
        "peak_rss_bytes": 1024,
        "note": "耗时与峰值内存来自本次真实运行，不参与确定性比较；结构产物只由参数决定。",
    }
    write_canonical(output / "timings.json", timings)
    (output / "report.md").write_text(report_markdown(structure), encoding="utf-8")
    manifest = {
        "policy": structure["policy"],
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": structure["schema_version"],
        "case_id": structure["case_id"],
        "spec": structure["spec"],
        "structure_sha256": sha256(structure_text.encode("utf-8")),
        "structure_byte_length": len(structure_text.encode("utf-8")),
        "counts": structure["counts"],
        "nfr10_status": structure["nfr10_status"],
    }
    write_canonical(output / "manifest.json", manifest)
    return output


def structure_of(directory: Path) -> dict:
    spec = package_spec()["spec"]
    result = replay_or_fail()
    prior = package_prior()
    return {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": V.CASE_ID,
        "spec": spec,
        "counts": result["counts"],
        "pages": result["pages"],
        "warning_counts": prior["warning_counts"],
        "limits": prior["limits"],
        "nfr10_status": prior["nfr10_status"],
        "model_calls": 0,
        "network_requests": 0,
    }


def audit(root: Path | None, package: Path | None = None) -> "V.Report":
    report = V.Report()
    if root is not None:
        V.check_layout(root, report)
        V.check_output(root, report)
    if package is not None:
        V.check_package(package, report)
        V.check_frozen(package, report)
        if root is not None:
            V.check_package_output(package, root, report)
    return report


def failed(report) -> set[str]:
    return {name for name, _, _ in report.failures}


def show(report) -> str:
    return "\n".join(f"{name}：{detail}" for name, _, detail in report.failures)


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="scale-audit-"))

    def __enter__(self) -> Path:
        return self.directory

    def __exit__(self, *_) -> None:
        shutil.rmtree(self.directory, ignore_errors=True)


class PackageTests(unittest.TestCase):
    def test_the_frozen_package_passes(self):
        report = audit(None, PACKAGE)
        self.assertEqual(failed(report), set(), show(report))
        self.assertGreater(len(report.checks), 15)

    def test_a_changed_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            target = package / "expected.json"
            target.write_text(target.read_text(encoding="utf-8").replace("80", "81", 1),
                              encoding="utf-8")
            report = audit(None, package)
            self.assertIn("frozen-digest:expected.json", failed(report))

    def test_an_extra_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "extra.json").write_text("{}", encoding="utf-8")
            self.assertIn("frozen-members", failed(audit(None, package)))

    def test_a_drifted_prior_count_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            for key in prior["counts"]:
                broken = copy.deepcopy(prior)
                broken["counts"][key] = broken["counts"][key] + 1
                write_json(package / "expected.json", broken)
                report = audit(None, package)
                self.assertIn("expected-counts", failed(report), key)

    def test_a_drifted_prior_page_digest_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["pages"][0]["sha256"] = "0" * 64
            write_json(package / "expected.json", prior)
            self.assertIn("expected-pages", failed(audit(None, package)))

    def test_a_missing_declaration_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            (package / "spec.json").unlink()
            self.assertIn("package-present", failed(audit(None, package)))

    def test_a_prior_claiming_the_limit_was_met_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["nfr10_status"] = "met"
            write_json(package / "expected.json", prior)
            self.assertIn("expected-nfr10", failed(audit(None, package)))

    def test_a_prior_losing_its_synthetic_boundary_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["boundaries"]["synthetic"] = False
            prior["boundaries"]["scientific_status"] = "passed"
            write_json(package / "expected.json", prior)
            self.assertIn("expected-boundary", failed(audit(None, package)))

    def test_a_prior_without_derivation_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["derivation"] = prior["derivation"][:2]
            write_json(package / "expected.json", prior)
            self.assertIn("expected-derivation", failed(audit(None, package)))

    def test_a_case_table_pointing_elsewhere_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            cases = json.loads((package / "cases.json").read_text(encoding="utf-8"))
            cases["expected"] = "other.json"
            write_json(package / "cases.json", cases)
            self.assertIn("package-cases-expected", failed(audit(None, package)))

    def test_a_case_table_with_too_few_variants_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            cases = json.loads((package / "cases.json").read_text(encoding="utf-8"))
            cases["variants"] = cases["variants"][:2]
            write_json(package / "cases.json", cases)
            self.assertIn("package-variants", failed(audit(None, package)))

    def test_freezing_again_keeps_the_bytes(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            before = (package / "frozen-sha256.json").read_bytes()
            V.freeze(package)
            self.assertEqual((package / "frozen-sha256.json").read_bytes(), before)


class CleanRootTests(unittest.TestCase):
    def test_a_correct_output_root_passes(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            report = audit(output)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 20)

    def test_a_correct_root_matches_the_package(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            report = audit(output, PACKAGE)
            self.assertEqual(failed(report), set(), show(report))

    def test_the_output_root_holds_exactly_four_files(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            names = sorted(item.name for item in output.iterdir())
            self.assertEqual(names, sorted(V.OUTPUT_ENTRIES))


class LayoutTests(unittest.TestCase):
    def test_an_extra_entry_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            (output / "extra.txt").write_text("x", encoding="utf-8")
            self.assertIn("layout-entries", failed(audit(output)))

    def test_a_missing_entry_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            (output / "timings.json").unlink()
            report = audit(output)
            self.assertIn("layout-entries", failed(report))
            self.assertIn("timings-strict", failed(report))

    def test_a_symlink_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            (output / "link.json").symlink_to(output / "structure.json")
            self.assertIn("layout-no-symlinks", failed(audit(output)))


class StructureTests(unittest.TestCase):
    def test_an_extra_field_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["extra"] = True
            output = build_output(directory, structure)
            self.assertIn("structure-strict", failed(audit(output)))

    def test_a_bad_policy_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["policy"] = "other"
            self.assertIn("structure-policy", failed(audit(build_output(directory, structure))))

    def test_a_claimed_model_call_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["model_calls"] = 1
            self.assertIn("structure-zero-calls",
                          failed(audit(build_output(directory, structure))))

    def test_claiming_the_limit_was_met_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["nfr10_status"] = "met"
            self.assertIn("structure-nfr10",
                          failed(audit(build_output(directory, structure))))

    def test_a_drifted_count_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["counts"]["spans"] += 1
            self.assertIn("structure-counts",
                          failed(audit(build_output(directory, structure))))

    def test_a_drifted_page_record_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["pages"][0]["span_count"] = 1
            self.assertIn("structure-pages",
                          failed(audit(build_output(directory, structure))))

    def test_a_page_record_with_a_missing_field_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            del structure["pages"][0]["headings"]
            self.assertIn("structure-page-fields:0",
                          failed(audit(build_output(directory, structure))))


class TimingsTests(unittest.TestCase):
    def test_an_inconsistent_total_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            timings = json.loads((output / "timings.json").read_text(encoding="utf-8"))
            timings["total_millis"] = 999
            write_canonical(output / "timings.json", timings)
            self.assertIn("timings-total", failed(audit(output)))

    def test_a_note_without_the_exclusion_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            timings = json.loads((output / "timings.json").read_text(encoding="utf-8"))
            timings["note"] = "耗时。"
            write_canonical(output / "timings.json", timings)
            self.assertIn("timings-note", failed(audit(output)))


class ManifestTests(unittest.TestCase):
    def test_a_manifest_carrying_a_timings_digest_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            output = build_output(directory, structure)
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            manifest["timings_sha256"] = "0" * 64
            write_canonical(output / "manifest.json", manifest)
            self.assertIn("manifest-strict", failed(audit(output)))

    def test_a_drifted_structure_digest_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            manifest["structure_sha256"] = "0" * 64
            write_canonical(output / "manifest.json", manifest)
            self.assertIn("manifest-structure-digest", failed(audit(output)))

    def test_a_drifted_count_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            manifest["counts"]["pages"] += 1
            write_canonical(output / "manifest.json", manifest)
            self.assertIn("manifest-counts", failed(audit(output)))

    def test_a_wrong_producer_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            manifest = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
            manifest["producer"] = "someone-else"
            write_canonical(output / "manifest.json", manifest)
            self.assertIn("manifest-producer", failed(audit(output)))


class ReportTests(unittest.TestCase):
    def test_a_report_losing_the_synthetic_boundary_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            text = (output / "report.md").read_text(encoding="utf-8")
            (output / "report.md").write_text(text.replace("合成语料", "真实语料"),
                                              encoding="utf-8")
            self.assertIn("report-synthetic", failed(audit(output)))

    def test_a_report_hiding_the_unmet_limit_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            text = (output / "report.md").read_text(encoding="utf-8")
            (output / "report.md").write_text(text.replace("未达标项", "已完成"), encoding="utf-8")
            self.assertIn("report-unmet", failed(audit(output)))

    def test_a_report_copying_a_wall_clock_reading_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            text = (output / "report.md").read_text(encoding="utf-8")
            (output / "report.md").write_text(
                text.replace("## 实测", "## 实测\n\n- 分析 306 ms"), encoding="utf-8"
            )
            self.assertIn("report-no-wall-clock", failed(audit(output)))

    def test_a_missing_report_is_detected(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            (output / "report.md").unlink()
            self.assertIn("report-present", failed(audit(output)))


class PackageOutputTests(unittest.TestCase):
    def test_a_run_that_does_not_match_the_prior_is_detected(self):
        with Sandbox() as directory:
            structure = structure_of(directory)
            structure["counts"]["headings"] += 1
            output = build_output(directory, structure)
            self.assertIn("output-expected-counts", failed(audit(output, PACKAGE)))

    def test_a_prior_limit_that_disagrees_with_the_run_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["limits"]["max_atoms"] = 1024
            write_json(package / "expected.json", prior)
            structure = structure_of(directory)
            output = build_output(directory, structure)
            self.assertIn("output-expected-limit:max_atoms", failed(audit(output, package)))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, argv):
        import contextlib
        import io

        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = V.main(argv)
        return code, out.getvalue(), err.getvalue()

    def test_package_only_exits_zero(self):
        code, out, _ = self.run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0)
        self.assertIn("失败 0", out)

    def test_no_arguments_is_a_usage_error(self):
        code, _, err = self.run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给", err)

    def test_freeze_requires_a_package(self):
        code, _, err = self.run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package。", err)

    def test_a_corrupted_root_exits_one_with_json(self):
        with Sandbox() as directory:
            output = build_output(directory, structure_of(directory))
            (output / "structure.json").write_text("{}", encoding="utf-8")
            code, out, _ = self.run_cli([str(output), "--json"])
            self.assertEqual(code, 1)
            payload = json.loads(out)
            names = {check["name"] for check in payload["checks"] if not check["ok"]}
            self.assertIn("structure-strict", names)


if __name__ == "__main__":
    unittest.main()
