#!/usr/bin/env python3
"""Unit tests for scripts/verify_library_bench.py.

The auditor is loaded as a module; nothing here imports the Rust product and
nothing runs the CLI to decide anything. Every fixture root is rebuilt from the
auditor's own replay so a test can change exactly one field and assert that the
check which owns that field is the one that fails.
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
from types import ModuleType

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify_library_bench.py"
PACKAGE = Path(__file__).resolve().parents[1] / "fixtures" / "library-bench-v1"


def load_auditor() -> ModuleType:
    spec = importlib.util.spec_from_file_location("verify_library_bench", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    text = json.dumps(value, ensure_ascii=False, indent=2)
    if trailing_newline:
        text += "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def write_canonical(path: Path, value) -> None:
    """The product writes canonical JSON plus one trailing newline."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(V.canonical(value) + "\n", encoding="utf-8", newline="\n")


def package_run() -> dict:
    spec = json.loads((PACKAGE / "spec.json").read_text(encoding="utf-8"))
    return spec["spec"]


def package_prior() -> dict:
    return json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))


def replay_or_fail(run: dict | None = None) -> dict:
    return V.replay(run or package_run())


def timings_for(run: dict, structure: dict) -> dict:
    """A plausible timings document: three samples per stage, ascending."""
    stages = []
    for index, name in enumerate(V.STAGES):
        samples = [10 + index, 12 + index, 14 + index]
        stages.append(
            {
                "stage": name,
                "samples_millis": samples,
                "p95_millis": V.nearest_rank(samples),
                "median_millis": sorted(samples)[len(samples) // 2],
                "min_millis": min(samples),
                "max_millis": max(samples),
            }
        )
    verdicts = [
        {
            "requirement": "NFR-12",
            "stage": stage["stage"],
            "p95_millis": stage["p95_millis"],
            "threshold_millis": 2000,
            "status": "met" if stage["p95_millis"] <= 2000 else "not_met",
        }
        for stage in stages
    ]
    return {
        "policy": V.POLICY,
        "case_id": V.CASE_ID,
        "protocol": structure["protocol"],
        "stages": stages,
        "threshold_millis": 2000,
        "verdicts": verdicts,
        "memory": {
            "rss_before_bytes": 20_000_000,
            "rss_after_scan_bytes": 21_000_000,
            "rss_after_plan_bytes": 22_000_000,
            "peak_rss_bytes": 22_500_000,
            "note": (
                "内存读数是整个进程的常驻集与峰值（/proc/self/status 的 VmRSS/VmHWM），"
                "包含已建好的语料，不是单个阶段的分配量；平台不支持时峰值写 null。"
            ),
        },
        "note": "所有毫秒数来自本次真实运行，不参与确定性比较；结构产物只由参数决定。",
    }


def structure_for(run: dict) -> dict:
    previous = replay_or_fail(run)
    return {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": V.CASE_ID,
        "spec": dict(run),
        "counts": previous["counts"],
        "corpus_sha256": previous["corpus_sha256"],
        "baseline_sha256": previous["baseline_sha256"],
        "changed_notes": previous["changed_notes"],
        "first_page_sha256": previous["first_page_sha256"],
        "protocol": {
            "samples": run["samples"],
            "warmup": run["warmup"],
            "process_start_included": False,
            "percentile": 95,
            "quantile_method": "nearest_rank",
            "sample_unit": "同一进程内对已建语料重复执行该阶段",
            "note": (
                "样本从预热之后的重复执行里取；进程启动、参数解析与语料生成都不计入阶段耗时。"
            ),
        },
        "threshold_millis": 2000,
        "coverage": [
            {
                "requirement": "NFR-11",
                "status": "partially_measured",
                "detail": (
                    f'合成库确实建到 {run["notes"]} 篇笔记、{run["edits"]} 处人工改动，'
                    "扫描、检测、影响规划与分页都在这个规模上真实跑过；但「不因全量渲染冻结界面」"
                    "属于 GUI 行为，本 harness 没有界面，只证明数据结构与分页产物能算出来、算得快。"
                ),
            },
            {
                "requirement": "NFR-12",
                "status": "partially_measured",
                "detail": (
                    "本轮只测 scan/detect/plan/page 四个阶段在 2 秒门槛下的 P95；"
                    "项目页、暂停命令、搜索与打开问题的端到端反馈没有界面可测，暂停语义属于 B13-T03。"
                ),
            },
        ],
        "model_calls": 0,
        "network_requests": 0,
    }


def report_markdown(structure: dict) -> str:
    """The report the product writes: parameters, protocol, threshold, boundaries."""
    run = structure["spec"]
    counts = structure["counts"]
    lines = [
        f'# 大库规模实测（{structure["case_id"]}）',
        "",
        "语料是**合成的**：笔记由参数生成，人工改动也是生成的。它证明这个规模上的结构与分页能算出来，",
        "不证明真实笔记库的语言质量，也不证明界面不会冻结。",
        "",
        "## 参数",
        "",
        f'- 笔记数 {run["notes"]}、人工改动 {run["edits"]}、样本数 {run["samples"]}、'
        f'预热 {run["warmup"]}',
        f'- 分页大小 {V.PAGE_SIZE} 条/页',
        "",
        "## 语料与阶段",
        "",
        f'- 主题 {counts["topics"]}、概念 {counts["concepts"]}、文本片段 {counts["spans"]}',
        f'- 检测到改动 {counts["changed"]} 处，落在 {counts["pages"]} 页变更清单里',
        f'- 影响范围 {counts["impacts"]} 篇（与改动笔记共享概念或主题的笔记）',
        "",
        "## 测量协议",
        "",
        f'- 每个阶段重复 {structure["protocol"]["samples"]} 次，'
        f'前 {structure["protocol"]["warmup"]} 次作为预热丢弃',
        "- 不含进程启动、参数解析与语料生成：计时只包住阶段本身",
        "- 分位数用 nearest_rank 的 P95：样本排序后取第 ceil(P/100×n) 个",
        "- 阶段定义：scan 逐篇跑文本分析并取摘要；detect 与冻结基线比摘要；"
        "plan 汇总共享概念与主题的笔记；page 渲染变更清单第一页",
        "",
        "## 门槛与判定规则",
        "",
        f'- NFR-12 门槛 {structure["threshold_millis"]} ms，逐个阶段比较 P95 与门槛；'
        "判定结果写在 timings.json，不写进本文件",
        "- 本文件不含本次运行的任何耗时读数：一旦写进来，同一参数两次运行就不再相同；"
        "上面那个门槛是常量，不是读数",
        "",
        "## 覆盖范围与未达标项",
        "",
    ]
    for coverage in structure["coverage"]:
        lines.append(
            f'- {coverage["requirement"]}（{coverage["status"]}）：{coverage["detail"]}'
        )
    lines.extend(
        [
            "",
            "## 边界",
            "",
            "- 不联网、不调用模型、不写项目与 Vault；只写这一个输出根。",
            "- 合成语料不代表真实笔记库；没有真实 PDF 抽取，也没有真实人工编辑。",
            "- 没有界面，因此「不因全量渲染冻结界面」没有被验证。",
            "",
        ]
    )
    return "\n".join(lines)


def build_root(directory: Path, structure: dict | None = None, timings: dict | None = None):
    """Rebuild a clean output root from the auditor's own replay."""
    run = package_run() if structure is None else structure["spec"]
    structure = structure_for(run) if structure is None else structure
    timings = timings_for(run, structure) if timings is None else timings
    directory.mkdir(parents=True, exist_ok=True)
    write_canonical(directory / V.STRUCTURE, structure)
    write_canonical(directory / V.TIMINGS, timings)
    (directory / V.REPORT).write_text(report_markdown(structure), encoding="utf-8", newline="\n")
    write_canonical(directory / V.MANIFEST, manifest_for(directory, structure))
    return structure


def manifest_for(directory: Path, structure: dict) -> dict:
    raw = (directory / V.STRUCTURE).read_bytes()
    return {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": V.CASE_ID,
        "spec": structure["spec"],
        "structure_sha256": V.sha256(raw),
        "structure_byte_length": len(raw),
        "counts": structure["counts"],
        "coverage": structure["coverage"],
    }


def audit(root: Path | None, package: Path | None) -> "V.Report":
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


def failed(report: "V.Report") -> set[str]:
    return {name for name, ok, _ in report.checks if not ok}


def show(report: "V.Report") -> str:
    return "; ".join(f"{name}: {detail}" for name, ok, detail in report.checks if not ok)


def run_cli(arguments: list[str]) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = V.main(arguments)
    return code, out.getvalue(), err.getvalue()


class Sandbox:
    def __init__(self, label: str) -> None:
        self.path = Path(tempfile.mkdtemp(prefix=f"bench-audit-{label}-"))

    def __enter__(self) -> Path:
        return self.path

    def __exit__(self, *_exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)


class CleanRootTests(unittest.TestCase):
    def test_a_clean_root_passes_every_check(self):
        with Sandbox("clean") as sandbox:
            build_root(sandbox / "a")
            report = audit(sandbox / "a", None)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 60)

    def test_a_clean_root_with_the_package_passes(self):
        with Sandbox("clean-package") as sandbox:
            build_root(sandbox / "a")
            report = audit(sandbox / "a", PACKAGE)
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 140)

    def test_the_replay_reproduces_the_frozen_prior(self):
        prior = package_prior()
        result = replay_or_fail()
        self.assertEqual(result["counts"], prior["counts"])
        self.assertEqual(result["corpus_sha256"], prior["corpus_sha256"])
        self.assertEqual(result["baseline_sha256"], prior["baseline_sha256"])
        self.assertEqual(result["first_page_sha256"], prior["first_page_sha256"])
        self.assertEqual(result["protocol"]["samples"], prior["protocol"]["samples"])

    def test_changed_ids_match_the_frozen_prior(self):
        prior = package_prior()
        result = replay_or_fail()
        self.assertEqual(
            [entry["note_id"] for entry in result["changed_notes"]],
            prior["changed_notes"],
        )


class LayoutTests(unittest.TestCase):
    def test_an_extra_entry_is_reported(self):
        with Sandbox("layout") as sandbox:
            build_root(sandbox / "a")
            (sandbox / "a" / "note.txt").write_text("x", encoding="utf-8", newline="\n")
            self.assertIn("layout-entries", failed(audit(sandbox / "a", None)))

    def test_a_missing_entry_is_reported(self):
        with Sandbox("missing") as sandbox:
            build_root(sandbox / "a")
            (sandbox / "a" / V.REPORT).unlink()
            self.assertIn("layout-entries", failed(audit(sandbox / "a", None)))
            self.assertIn("report-present", failed(audit(sandbox / "a", None)))

    def test_a_symlink_is_reported(self):
        with Sandbox("symlink") as sandbox:
            build_root(sandbox / "a")
            (sandbox / "a" / "alias.md").symlink_to(sandbox / "a" / V.REPORT)
            self.assertIn("layout-no-symlinks", failed(audit(sandbox / "a", None)))


class StructureTests(unittest.TestCase):
    def test_a_drifted_count_is_reported(self):
        with Sandbox("counts") as sandbox:
            structure = structure_for(package_run())
            structure["counts"]["spans"] += 1
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-counts", failed(audit(sandbox / "a", None)))

    def test_a_drifted_corpus_digest_is_reported(self):
        with Sandbox("corpus") as sandbox:
            structure = structure_for(package_run())
            structure["corpus_sha256"] = "0" * 64
            build_root(sandbox / "a", structure=structure)
            failures = failed(audit(sandbox / "a", None))
            self.assertIn("structure-corpus", failures)
            self.assertNotIn("structure-counts", failures)

    def test_a_drifted_baseline_digest_is_reported(self):
        with Sandbox("baseline") as sandbox:
            structure = structure_for(package_run())
            structure["baseline_sha256"] = "0" * 64
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-baseline", failed(audit(sandbox / "a", None)))

    def test_a_drifted_first_page_is_reported(self):
        with Sandbox("page") as sandbox:
            structure = structure_for(package_run())
            structure["first_page_sha256"] = "0" * 64
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-first-page", failed(audit(sandbox / "a", None)))

    def test_a_dropped_changed_note_is_reported(self):
        with Sandbox("changed") as sandbox:
            structure = structure_for(package_run())
            structure["changed_notes"] = structure["changed_notes"][:-1]
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-changed", failed(audit(sandbox / "a", None)))

    def test_a_drifted_protocol_is_reported(self):
        with Sandbox("protocol") as sandbox:
            structure = structure_for(package_run())
            structure["protocol"]["process_start_included"] = True
            build_root(sandbox / "a", structure=structure)
            self.assertIn(
                "structure-protocol:process_start_included",
                failed(audit(sandbox / "a", None)),
            )

    def test_a_dishonest_coverage_status_is_reported(self):
        with Sandbox("coverage") as sandbox:
            structure = structure_for(package_run())
            structure["coverage"][0]["status"] = "met"
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-coverage-honest", failed(audit(sandbox / "a", None)))

    def test_a_hardcoded_coverage_detail_is_reported(self):
        with Sandbox("coverage-params") as sandbox:
            structure = structure_for(package_run())
            structure["coverage"][0]["detail"] = structure["coverage"][0]["detail"].replace(
                "300 篇笔记", "3000 篇笔记"
            )
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-coverage-params", failed(audit(sandbox / "a", None)))

    def test_an_unknown_field_is_reported(self):
        with Sandbox("unknown") as sandbox:
            structure = structure_for(package_run())
            structure["extra"] = 1
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-strict", failed(audit(sandbox / "a", None)))

    def test_a_pretty_printed_product_file_is_reported(self):
        with Sandbox("encoding") as sandbox:
            structure = structure_for(package_run())
            build_root(sandbox / "a")
            (sandbox / "a" / V.STRUCTURE).write_text(
                json.dumps(structure, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
            )
            self.assertIn("structure-strict", failed(audit(sandbox / "a", None)))

    def test_a_non_zero_model_call_is_reported(self):
        with Sandbox("model") as sandbox:
            structure = structure_for(package_run())
            structure["model_calls"] = 1
            build_root(sandbox / "a", structure=structure)
            self.assertIn("structure-zero-calls", failed(audit(sandbox / "a", None)))


class TimingsTests(unittest.TestCase):
    def test_a_drifted_p95_is_reported(self):
        with Sandbox("p95") as sandbox:
            structure = structure_for(package_run())
            timings = timings_for(structure["spec"], structure)
            timings["stages"][0]["p95_millis"] = 1
            build_root(sandbox / "a", structure=structure, timings=timings)
            self.assertIn("timings-p95:scan", failed(audit(sandbox / "a", None)))

    def test_a_wrong_verdict_is_reported(self):
        with Sandbox("verdict") as sandbox:
            structure = structure_for(package_run())
            timings = timings_for(structure["spec"], structure)
            timings["stages"][0]["p95_millis"] = 5_000
            timings["verdicts"][0]["p95_millis"] = 5_000
            build_root(sandbox / "a", structure=structure, timings=timings)
            self.assertIn("timings-verdict:scan", failed(audit(sandbox / "a", None)))

    def test_a_wrong_sample_count_is_reported(self):
        with Sandbox("samples") as sandbox:
            structure = structure_for(package_run())
            timings = timings_for(structure["spec"], structure)
            timings["stages"][0]["samples_millis"] = [1, 2]
            build_root(sandbox / "a", structure=structure, timings=timings)
            self.assertIn("timings-samples:scan", failed(audit(sandbox / "a", None)))

    def test_a_missing_stage_is_reported(self):
        with Sandbox("stage") as sandbox:
            structure = structure_for(package_run())
            timings = timings_for(structure["spec"], structure)
            timings["stages"] = timings["stages"][:-1]
            timings["verdicts"] = timings["verdicts"][:-1]
            build_root(sandbox / "a", structure=structure, timings=timings)
            failures = failed(audit(sandbox / "a", None))
            self.assertIn("timings-stages", failures)
            self.assertIn("timings-verdict-count", failures)

    def test_a_timings_note_without_the_boundary_is_reported(self):
        with Sandbox("timings-note") as sandbox:
            structure = structure_for(package_run())
            timings = timings_for(structure["spec"], structure)
            timings["note"] = "所有耗时来自本次运行。"
            build_root(sandbox / "a", structure=structure, timings=timings)
            self.assertIn("timings-note", failed(audit(sandbox / "a", None)))


class ManifestTests(unittest.TestCase):
    def test_the_manifest_must_not_pin_the_clock(self):
        with Sandbox("manifest-clock") as sandbox:
            structure = structure_for(package_run())
            build_root(sandbox / "a", structure=structure)
            path = sandbox / "a" / V.MANIFEST
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["timings_sha256"] = "0" * 64
            write_canonical(path, manifest)
            self.assertIn("manifest-strict", failed(audit(sandbox / "a", None)))

    def test_a_drifted_structure_digest_is_reported(self):
        with Sandbox("manifest-digest") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.MANIFEST
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["structure_sha256"] = "0" * 64
            write_canonical(path, manifest)
            self.assertIn("manifest-structure-digest", failed(audit(sandbox / "a", None)))

    def test_a_drifted_structure_length_is_reported(self):
        with Sandbox("manifest-length") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.MANIFEST
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["structure_byte_length"] += 1
            write_canonical(path, manifest)
            self.assertIn("manifest-structure-length", failed(audit(sandbox / "a", None)))

    def test_a_drifted_manifest_count_is_reported(self):
        with Sandbox("manifest-counts") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.MANIFEST
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["counts"]["spans"] += 1
            write_canonical(path, manifest)
            self.assertIn("manifest-counts", failed(audit(sandbox / "a", None)))

    def test_a_drifted_manifest_coverage_is_reported(self):
        with Sandbox("manifest-coverage") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.MANIFEST
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["coverage"] = manifest["coverage"][:1]
            write_canonical(path, manifest)
            self.assertIn("manifest-coverage", failed(audit(sandbox / "a", None)))


class ReportTests(unittest.TestCase):
    def test_a_missing_boundary_statement_is_reported(self):
        with Sandbox("report") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.REPORT
            text = path.read_text(encoding="utf-8")
            path.write_text(text.replace("不证明真实笔记库", "证明真实笔记库"), encoding="utf-8", newline="\n")
            self.assertIn("report-boundary", failed(audit(sandbox / "a", None)))

    def test_the_gui_gap_must_be_written_down(self):
        with Sandbox("report-gui") as sandbox:
            build_root(sandbox / "a")
            path = sandbox / "a" / V.REPORT
            text = path.read_text(encoding="utf-8")
            path.write_text(
                text.replace("没有界面，因此「不因全量渲染冻结界面」没有被验证。", ""),
                encoding="utf-8", newline="\n",
            )
            self.assertIn("report-gui-gap", failed(audit(sandbox / "a", None)))

    def test_a_stage_reading_in_the_report_is_reported(self):
        with Sandbox("report-clock") as sandbox:
            structure = structure_for(package_run())
            build_root(sandbox / "a", structure=structure)
            path = sandbox / "a" / V.REPORT
            text = path.read_text(encoding="utf-8")
            timings = json.loads((sandbox / "a" / V.TIMINGS).read_text(encoding="utf-8"))
            reading = timings["stages"][0]["p95_millis"]
            self.assertNotEqual(reading, 2000)
            path.write_text(text + f"\n- scan 用了 {reading} ms\n", encoding="utf-8", newline="\n")
            failures = failed(audit(sandbox / "a", None))
            self.assertIn("report-ms-threshold-only", failures)
            self.assertIn("report-no-stage-numbers", failures)

    def test_the_threshold_alone_is_allowed(self):
        with Sandbox("report-threshold") as sandbox:
            build_root(sandbox / "a")
            report = audit(sandbox / "a", None)
            self.assertNotIn("report-ms-threshold-only", failed(report))
            self.assertNotIn("report-no-stage-numbers", failed(report))

    def test_a_missing_coverage_detail_is_reported(self):
        with Sandbox("report-detail") as sandbox:
            structure = structure_for(package_run())
            build_root(sandbox / "a", structure=structure)
            path = sandbox / "a" / V.REPORT
            text = path.read_text(encoding="utf-8")
            path.write_text(
                text.replace("暂停语义属于 B13-T03。", "暂停语义未测。"), encoding="utf-8", newline="\n"
            )
            self.assertIn("report-coverage-detail", failed(audit(sandbox / "a", None)))


class PackageTests(unittest.TestCase):
    def test_the_frozen_package_passes(self):
        report = audit(None, PACKAGE)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_tampered_member_is_reported(self):
        with Sandbox("package") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            path = package / "expected.json"
            path.write_text(path.read_text(encoding="utf-8") + " ", encoding="utf-8", newline="\n")
            self.assertIn("frozen-digest:expected.json", failed(audit(None, package)))

    def test_an_extra_member_is_reported(self):
        with Sandbox("package-extra") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            (package / "notes.md").write_text("x", encoding="utf-8", newline="\n")
            self.assertIn("frozen-members", failed(audit(None, package)))

    def test_refreezing_is_byte_stable(self):
        with Sandbox("package-freeze") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            before = (package / V.FROZEN).read_bytes()
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(V.freeze(package), 0)
            self.assertEqual((package / V.FROZEN).read_bytes(), before)

    def test_a_drifted_prior_count_is_reported(self):
        with Sandbox("prior") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["counts"]["spans"] += 1
            write_json(package / "expected.json", prior)
            with contextlib.redirect_stdout(io.StringIO()):
                V.freeze(package)
            self.assertIn("expected-counts", failed(audit(None, package)))

    def test_a_drifted_prior_digest_is_reported(self):
        with Sandbox("prior-digest") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["corpus_sha256"] = "0" * 64
            write_json(package / "expected.json", prior)
            with contextlib.redirect_stdout(io.StringIO()):
                V.freeze(package)
            self.assertIn("expected-corpus", failed(audit(None, package)))

    def test_a_dishonest_prior_coverage_is_reported(self):
        with Sandbox("prior-coverage") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            prior["coverage"][0]["status"] = "met"
            write_json(package / "expected.json", prior)
            with contextlib.redirect_stdout(io.StringIO()):
                V.freeze(package)
            self.assertIn("expected-coverage-honest", failed(audit(None, package)))

    def test_a_missing_variant_exit_is_reported(self):
        with Sandbox("prior-variant") as sandbox:
            package = sandbox / "package"
            shutil.copytree(PACKAGE, package)
            cases = json.loads((package / "cases.json").read_text(encoding="utf-8"))
            cases["variants"][0]["expect"] = {"code": "BENCH_ARGUMENT"}
            write_json(package / "cases.json", cases)
            with contextlib.redirect_stdout(io.StringIO()):
                V.freeze(package)
            self.assertIn(
                "package-variant-exit:rejected-existing", failed(audit(None, package))
            )


class PackageOutputTests(unittest.TestCase):
    def test_a_run_that_disagrees_with_the_prior_is_reported(self):
        with Sandbox("output-prior") as sandbox:
            run = copy.deepcopy(package_run())
            run["notes"] = 300
            structure = structure_for(package_run())
            structure["counts"]["impacts"] += 5
            build_root(sandbox / "a", structure=structure)
            failures = failed(audit(sandbox / "a", PACKAGE))
            self.assertIn("output-expected-counts", failures)

    def test_a_matching_run_passes_against_the_prior(self):
        with Sandbox("output-match") as sandbox:
            build_root(sandbox / "a")
            report = audit(sandbox / "a", PACKAGE)
            self.assertEqual(failed(report), set(), show(report))


class CommandLineTests(unittest.TestCase):
    def test_package_only_passes(self):
        code, out, _ = run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, out)
        self.assertIn("失败 0", out)

    def test_no_arguments_is_a_usage_error(self):
        code, _, err = run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给一个输出根或 --package", err)

    def test_a_tampered_root_exits_one(self):
        with Sandbox("cli") as sandbox:
            build_root(sandbox / "a")
            (sandbox / "a" / "extra.txt").write_text("x", encoding="utf-8", newline="\n")
            code, out, _ = run_cli([str(sandbox / "a")])
            self.assertEqual(code, 1)
            self.assertIn("layout-entries", out)

    def test_json_output_is_structured(self):
        with Sandbox("cli-json") as sandbox:
            build_root(sandbox / "a")
            code, out, _ = run_cli([str(sandbox / "a"), "--json"])
            self.assertEqual(code, 0)
            payload = json.loads(out)
            self.assertEqual(payload["failures"], 0)
            self.assertEqual(payload["total"], len(payload["checks"]))

    def test_freeze_without_a_package_is_a_usage_error(self):
        code, _, err = run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", err)

    def test_a_missing_root_is_a_usage_error(self):
        with Sandbox("cli-missing") as sandbox:
            code, _, err = run_cli([str(sandbox / "nope")])
            self.assertEqual(code, 2)
            self.assertIn("不是目录", err)


if __name__ == "__main__":
    unittest.main()
