"""Tests for the read-only soak auditor.

These tests build a synthetic output root that mirrors what the product writes,
then hand it to the auditor. A test passes when only the check that owns the
mutated field fails: a coarser assertion would hide a check that silently
stopped looking at its own field.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AUDITOR = ROOT / "scripts" / "verify_soak_harness.py"
PACKAGE = ROOT / "fixtures" / "soak-harness-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_soak_harness", AUDITOR)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    text = canonical(value)
    if trailing_newline:
        text += "\n"
    path.write_text(text, encoding="utf-8")


def write_canonical(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    write_json(path, value)


def package_declaration() -> dict:
    return json.loads((PACKAGE / "spec.json").read_text(encoding="utf-8"))


def structure_for(spec: dict) -> dict:
    return {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": V.CASE_ID,
        "spec": dict(spec),
        "enforcement": V.enforcement(spec),
        "coverage": V.coverage(spec),
        "reproduce": V.reproduce(spec),
        "model_calls": 0,
        "network_requests": 0,
    }


def summary_for(spec: dict) -> dict:
    base = V.prior_summary(spec)
    base.update(
        {
            "total_millis": 40,
            "max_work_millis": 8,
            "median_work_millis": 5,
            "first_rss_bytes": 20_000_000,
            "last_rss_bytes": 20_500_000,
            "peak_rss_bytes": 20_800_000,
            "rss_growth_bytes": 500_000,
        }
    )
    if base["scheduled"] == 0:
        base.update(
            {
                "total_millis": 0,
                "max_work_millis": 0,
                "median_work_millis": 0,
                "first_rss_bytes": None,
                "last_rss_bytes": None,
                "peak_rss_bytes": None,
                "rss_growth_bytes": None,
            }
        )
    return base


def rounds_for(spec: dict) -> list[dict]:
    rounds = []
    gate = spec["pause_after_ms"]
    for index in range(1, spec["rounds"] + 1):
        closed = gate is not None and index >= 1 and gate == 0
        rounds.append(
            {
                "round": index,
                "state": V.STATE_REFUSED if closed else V.STATE_SCHEDULED,
                "elapsed_millis": 0 if gate is None else 0,
                "work_millis": 0 if closed else 4,
                "total_millis": 0 if closed else 4,
                "rss_bytes": None if closed else 20_000_000,
                "error_code": None,
            }
        )
    return rounds


def pause_for(spec: dict) -> dict:
    base = V.prior_pause(spec)
    return {
        "after_ms": base["after_ms"],
        "effective_round": base["effective_round"],
        "latency_millis": None if base["after_ms"] is None else 0,
        "bound_millis": V.PAUSE_BOUND_MILLIS,
        "within_bound": base["within_bound"],
        "scheduled_after_pause": 0,
        "note": "暂停闸门只在轮次开始时判断一次，关掉就不再打开。",
    }


def timings_for(spec: dict) -> dict:
    return {
        "policy": V.POLICY,
        "case_id": V.CASE_ID,
        "rounds": rounds_for(spec),
        "summary": summary_for(spec),
        "pause": pause_for(spec),
        "note": "读数来自本次运行，不参与确定性比较。",
    }


def report_markdown(structure: dict) -> str:
    spec = structure["spec"]
    lines = [
        f'# 长时运行 soak（{structure["case_id"]}）',
        "",
        "这一份是**合成**的长时运行观察：工作单元是对一页合成文本做只读分析，"
        "不含任何教材内容，也不代表真实项目负载。",
        "",
        "## 参数",
        "",
        f'- 轮次：{spec["rounds"]}',
        f'- 轮间隔：{spec["interval_ms"]} 毫秒',
        "- 暂停时点："
        + ("不暂停" if spec["pause_after_ms"] is None else f'{spec["pause_after_ms"]} 毫秒'),
        f'- 内存增长上限：{spec["max_rss_growth_bytes"]} 字节',
        "",
        "## 工作单元",
        "",
        structure["enforcement"]["unit"],
        "",
        "## 暂停闸门",
        "",
        f'- 时限：暂停后 {structure["enforcement"]["pause_bound_millis"]} 毫秒内必须停止调度新请求（NFR-13）。',
        "- 判定口径：每一轮开始时记下距起点的毫秒数；暂停时点之后的所有轮次都必须是 refused。",
        "- 可观测计数：闸门关闭后仍被调度的轮数必须为 0，读数在 `timings.json` 的 `pause.scheduled_after_pause`。",
        "",
        "## 覆盖范围与未达标项",
        "",
    ]
    for entry in structure["coverage"]:
        lines.append(f'- **{entry["requirement"]}**：{entry["status"]}——{entry["detail"]}')
    lines.extend(
        [
            "",
            "## 边界",
            "",
            "- 读数写在 `timings.json`：本文件不含本次运行的任何耗时或内存读数，"
            "写进来同一参数两次运行就不再相同；上面出现的毫秒数只有规则常量。",
            "- 合成负载：工作单元只跑文本分析，不写项目、不写 Vault、不联网、不调用模型。",
            f'- 复现命令：`{structure["reproduce"]}`',
        ]
    )
    return "\n".join(lines) + "\n"


def manifest_for(spec: dict, structure: dict) -> dict:
    body = canonical(structure) + "\n"
    return {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": V.CASE_ID,
        "spec": dict(spec),
        "structure_sha256": V.sha256(body.encode("utf-8")),
        "structure_byte_length": len(body.encode("utf-8")),
        "coverage": [
            {"requirement": entry["requirement"], "status": entry["status"]}
            for entry in structure["coverage"]
        ],
    }


def build_root(directory: Path, spec: dict | None = None, *, structure=None,
               timings=None, manifest=None, report=None) -> Path:
    spec = dict(spec or package_declaration()["spec"])
    structure = structure or structure_for(spec)
    timings = timings or timings_for(spec)
    manifest = manifest or manifest_for(spec, structure)
    report = report if report is not None else report_markdown(structure)
    directory.mkdir(parents=True, exist_ok=True)
    write_canonical(directory / "structure.json", structure)
    write_canonical(directory / "timings.json", timings)
    write_canonical(directory / "manifest.json", manifest)
    (directory / "report.md").write_text(report, encoding="utf-8")
    return directory


def audit(root: Path, package: Path | None = None) -> "V.Report":
    report = V.Report()
    V.check_layout(root, report)
    V.check_output(root, report)
    if package is not None:
        V.check_package(package, report)
        V.check_frozen(package, report)
        V.check_package_output(package, root, report)
    return report


def failed(report: "V.Report") -> set[str]:
    return {name for name, ok, _ in report.checks if not ok}


def show(report: "V.Report") -> str:
    return "\n".join(f"[失败] {name}：{detail}" for name, ok, detail in report.checks if not ok)


class Sandbox:
    def __init__(self) -> None:
        self.path = Path(tempfile.mkdtemp(prefix="soak-auditor-"))

    def __enter__(self) -> Path:
        return self.path

    def __exit__(self, *_exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)


class CleanRootTests(unittest.TestCase):
    def test_a_mirrored_root_passes_every_check(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            report = audit(root)
            self.assertTrue(report.checks, "审计器必须真的跑了检查")
            self.assertEqual(failed(report), set(), show(report))
            self.assertGreater(len(report.checks), 40)

    def test_a_paused_root_also_passes(self) -> None:
        with Sandbox() as path:
            spec = dict(package_declaration()["paused_spec"])
            root = build_root(path / "run", spec)
            report = audit(root)
            self.assertEqual(failed(report), set(), show(report))

    def test_the_package_alone_passes(self) -> None:
        report = V.Report()
        V.check_package(PACKAGE, report)
        V.check_frozen(PACKAGE, report)
        self.assertEqual(failed(report), set(), show(report))


class LayoutTests(unittest.TestCase):
    def test_an_extra_entry_is_reported(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "notes.txt").write_text("x", encoding="utf-8")
            report = audit(root)
            self.assertIn("layout-entries", failed(report))

    def test_a_missing_file_is_reported(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "timings.json").unlink()
            report = audit(root)
            self.assertIn("timings-strict", failed(report))

    def test_a_symlink_is_reported(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "extra.json").symlink_to(root / "structure.json")
            report = audit(root)
            self.assertIn("layout-no-symlinks", failed(report))


class StructureTests(unittest.TestCase):
    def test_an_unknown_field_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["extra"] = 1
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-strict", failed(report))

    def test_a_wrong_policy_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["policy"] = "other"
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-policy", failed(report))

    def test_an_unmet_requirement_claimed_as_met_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            structure = structure_for(spec)
            structure["coverage"][0]["status"] = "met"
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-coverage-honest", failed(report))

    def test_a_missing_pause_bound_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["enforcement"]["pause_bound_millis"] = 3_000
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-enforcement", failed(report))

    def test_a_wrong_planned_floor_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["enforcement"]["planned_floor_millis"] = 5
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-planned-floor", failed(report))

    def test_a_wrong_reproduce_command_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["reproduce"] = "noetome soak run --rounds 1"
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-reproduce", failed(report))

    def test_a_model_call_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            structure["model_calls"] = 1
            root = build_root(path / "run", structure=structure)
            report = audit(root)
            self.assertIn("structure-zero-calls", failed(report))


class TimingsTests(unittest.TestCase):
    def test_a_scheduled_round_after_the_gate_is_reported(self) -> None:
        with Sandbox() as path:
            spec = dict(package_declaration()["paused_spec"])
            timings = timings_for(spec)
            timings["rounds"][3]["state"] = V.STATE_SCHEDULED
            timings["summary"]["scheduled"] = 1
            timings["summary"]["units"] = 1
            root = build_root(path / "run", spec, timings=timings)
            report = audit(root)
            self.assertIn("timings-gate-states", failed(report))

    def test_a_late_elapsed_time_is_reported(self) -> None:
        with Sandbox() as path:
            spec = {"rounds": 8, "interval_ms": 10, "pause_after_ms": 20,
                    "max_rss_growth_bytes": V.MAX_RSS_GROWTH_BYTES}
            timings = timings_for(spec)
            timings["pause"] = {
                "after_ms": 20,
                "effective_round": 3,
                "latency_millis": 3,
                "bound_millis": V.PAUSE_BOUND_MILLIS,
                "within_bound": True,
                "scheduled_after_pause": 0,
                "note": "闸门只在轮次开始时判断一次。",
            }
            for index, round in enumerate(timings["rounds"]):
                round["elapsed_millis"] = index * 10
                round["state"] = V.STATE_SCHEDULED if index < 2 else V.STATE_REFUSED
            timings["rounds"][3]["elapsed_millis"] = 5  # 早于暂停时点
            timings["summary"].update({"scheduled": 2, "refused": 6, "units": 2})
            root = build_root(path / "run", spec, timings=timings)
            report = audit(root)
            self.assertIn("timings-gate-states", failed(report))

    def test_a_nonzero_scheduled_after_pause_is_reported(self) -> None:
        with Sandbox() as path:
            spec = dict(package_declaration()["paused_spec"])
            timings = timings_for(spec)
            timings["pause"]["scheduled_after_pause"] = 2
            root = build_root(path / "run", spec, timings=timings)
            report = audit(root)
            self.assertIn("timings-gate-count", failed(report))

    def test_a_negative_growth_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            timings = timings_for(spec)
            timings["summary"]["rss_growth_bytes"] = 1
            root = build_root(path / "run", spec, timings=timings)
            report = audit(root)
            self.assertIn("timings-rss-growth", failed(report))

    def test_missing_readings_with_no_scheduled_rounds_is_fine(self) -> None:
        with Sandbox() as path:
            spec = dict(package_declaration()["paused_spec"])
            root = build_root(path / "run", spec)
            report = audit(root)
            self.assertEqual(failed(report), set(), show(report))

    def test_a_round_with_an_error_code_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            timings = timings_for(spec)
            timings["rounds"][0]["error_code"] = "SOAK_WORK"
            root = build_root(path / "run", spec, timings=timings)
            report = audit(root)
            self.assertIn("timings-no-error-codes", failed(report))


class ManifestTests(unittest.TestCase):
    def test_a_reading_in_the_manifest_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            structure = structure_for(spec)
            manifest = manifest_for(spec, structure)
            manifest["summary"] = summary_for(spec)
            root = build_root(path / "run", spec, structure=structure, manifest=manifest)
            report = audit(root)
            # 清单的字段集合是精确的，所以"把读数塞进清单"会先被 strict 拦下；
            # manifest-no-readings 是第二道闸：万一有人把读数写进字段列表本身。
            self.assertTrue(
                {"manifest-strict", "manifest-no-readings"} & failed(report),
                show(report),
            )

    def test_the_declared_manifest_fields_carry_no_readings(self) -> None:
        for name in ("summary", "pause", "timings_sha256", "rounds"):
            self.assertNotIn(name, V.MANIFEST_FIELDS,
                             f"清单字段列表里不该出现读数结构 {name}")

    def test_a_wrong_producer_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            structure = structure_for(spec)
            manifest = manifest_for(spec, structure)
            manifest["producer"] = "someone_else"
            root = build_root(path / "run", spec, structure=structure, manifest=manifest)
            report = audit(root)
            self.assertIn("manifest-producer", failed(report))

    def test_a_wrong_structure_digest_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            structure = structure_for(spec)
            manifest = manifest_for(spec, structure)
            manifest["structure_sha256"] = "0" * 64
            root = build_root(path / "run", spec, structure=structure, manifest=manifest)
            report = audit(root)
            self.assertIn("manifest-structure-digest-value", failed(report))

    def test_a_missing_manifest_is_reported(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "manifest.json").unlink()
            report = audit(root)
            self.assertIn("manifest-strict", failed(report))


class ReportTests(unittest.TestCase):
    def test_a_reading_in_the_report_is_reported(self) -> None:
        with Sandbox() as path:
            spec = package_declaration()["spec"]
            structure = structure_for(spec)
            timings = timings_for(spec)
            reading = timings["summary"]["median_work_millis"]
            self.assertNotEqual(reading, V.PAUSE_BOUND_MILLIS)
            report_text = report_markdown(structure) + f"\n中位耗时 {reading} 毫秒。\n"
            root = build_root(path / "run", spec, structure=structure, timings=timings,
                              report=report_text)
            report = audit(root)
            self.assertIn("report-no-readings", failed(report))

    def test_the_constant_threshold_is_allowed(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            report = audit(root)
            self.assertNotIn("report-no-readings", failed(report))

    def test_a_missing_boundary_section_is_reported(self) -> None:
        with Sandbox() as path:
            structure = structure_for(package_declaration()["spec"])
            text = report_markdown(structure).replace("## 边界", "## 其它")
            root = build_root(path / "run", structure=structure, report=text)
            report = audit(root)
            self.assertTrue(
                {"report-needle:## 边界", "report-reproduce"} & failed(report),
                show(report),
            )

    def test_a_missing_report_is_reported(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "report.md").unlink()
            report = audit(root)
            self.assertIn("report-present", failed(report))


class PackageTests(unittest.TestCase):
    def test_the_prior_matches_the_declaration(self) -> None:
        report = V.Report()
        V.check_package(PACKAGE, report)
        for name in ("expected-enforcement", "expected-coverage", "expected-no_pause-summary",
                     "expected-paused-summary", "expected-determinism"):
            self.assertNotIn(name, failed(report), show(report))

    def test_a_drifted_prior_summary_is_reported(self) -> None:
        with Sandbox() as path:
            package = path / "package"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["no_pause"]["summary"]["scheduled"] = 3
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_package(package, report)
            self.assertIn("expected-no_pause-summary", failed(report))

    def test_a_claim_that_nfr09_met_is_reported(self) -> None:
        with Sandbox() as path:
            package = path / "package"
            shutil.copytree(PACKAGE, package)
            expected = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            expected["coverage"][0]["status"] = "met"
            write_json(package / "expected.json", expected)
            report = V.Report()
            V.check_package(package, report)
            self.assertIn("expected-coverage", failed(report))

    def test_a_frozen_digest_drift_is_reported(self) -> None:
        with Sandbox() as path:
            package = path / "package"
            shutil.copytree(PACKAGE, package)
            (package / "README.md").write_text("changed", encoding="utf-8")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-digest:README.md", failed(report))

    def test_a_missing_frozen_list_is_reported(self) -> None:
        with Sandbox() as path:
            package = path / "package"
            shutil.copytree(PACKAGE, package)
            (package / "frozen-sha256.json").unlink()
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-present", failed(report))

    def test_a_drifted_case_sequence_is_reported(self) -> None:
        with Sandbox() as path:
            package = path / "package"
            shutil.copytree(PACKAGE, package)
            cases = json.loads((package / "cases.json").read_text(encoding="utf-8"))
            cases["variants"] = cases["variants"][:2]
            write_json(package / "cases.json", cases)
            report = V.Report()
            V.check_package(package, report)
            self.assertIn("cases-variants", failed(report))


class CommandLineTests(unittest.TestCase):
    def run_cli(self, arguments: list[str]) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = V.main(arguments)
        return code, out.getvalue(), err.getvalue()

    def test_package_mode_succeeds(self) -> None:
        code, out, _ = self.run_cli(["--package", str(PACKAGE)])
        self.assertEqual(code, 0, out)
        self.assertIn("失败 0", out)

    def test_root_and_package_together_succeed(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            code, out, _ = self.run_cli([str(root), "--package", str(PACKAGE)])
            self.assertEqual(code, 0, out)
            self.assertIn("失败 0", out)

    def test_no_arguments_is_a_usage_error(self) -> None:
        code, _, err = self.run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给一个输出根或 --package", err)

    def test_freeze_without_a_package_is_a_usage_error(self) -> None:
        code, _, err = self.run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", err)

    def test_a_broken_root_exits_one(self) -> None:
        with Sandbox() as path:
            root = build_root(path / "run")
            (root / "structure.json").write_text("{}", encoding="utf-8")
            code, out, _ = self.run_cli([str(root)])
            self.assertEqual(code, 1)
            self.assertIn("structure-strict", out)

    def test_json_output_is_structured(self) -> None:
        code, out, _ = self.run_cli(["--package", str(PACKAGE), "--json"])
        self.assertEqual(code, 0)
        payload = json.loads(out)
        self.assertEqual(payload["failures"], 0)
        self.assertGreater(payload["total"], 10)


if __name__ == "__main__":
    unittest.main()
