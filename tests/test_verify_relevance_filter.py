"""Tests for the independent relevance-filter auditor (B10-T02).

The auditor never imports the product and never runs the CLI to decide anything.
These tests build a filter root with the auditor's own replay, so a green run
proves the checks read the documented layout instead of mirroring whatever the
product happens to write.
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

from symlink_test_support import create_symlink_or_skip

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_relevance_filter.py"
PACKAGE = ROOT / "fixtures" / "relevance-filter-v1"
FIXED_AT = "2026-11-07T00:00:00Z"


def load_auditor(module_name: str = "verify_relevance_filter"):
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


def build_root(directory: Path) -> tuple[Path, dict]:
    """Build a filter root from the auditor's replay of the frozen package."""
    replay = V.replay_package(PACKAGE)
    author = replay["author"]
    case_id = author["case_id"]
    root = directory / "root"
    control = root / "control"
    text_dir = control / "text"
    text_dir.mkdir(parents=True)

    documents = []
    for document in replay["documents"]:
        data = (PACKAGE / document["path"]).read_bytes()
        (text_dir / f"{document['source_id']}{V.TEXT_SUFFIX}").write_bytes(data)
        documents.append(
            {
                "source_id": document["source_id"],
                "title": document["title"],
                "path": document["path"],
                "raw_sha256": V.sha256(data),
                "byte_length": len(data),
                "normalized_sha256": document["normalized_sha256"],
                "span_count": len(document["spans"]),
                "spans": document["spans"],
            }
        )

    for name in ("audience", "atoms", "review"):
        relative = author[name]
        (control / Path(relative).name).write_bytes((PACKAGE / relative).read_bytes())

    # The product stores the author declaration canonically without a trailing
    # newline, because `strict` compares the bytes with the canonical form.
    write_json(control / "author.json", author, trailing_newline=False)

    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "documents": documents,
    }
    write_json(control / "inputs.json", inputs)

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
        "author_sha256": V.sha256((PACKAGE / "author.json").read_bytes()),
        "audience_sha256": V.sha256((PACKAGE / author["audience"]).read_bytes()),
        "atoms_sha256": V.sha256((PACKAGE / author["atoms"]).read_bytes()),
        "review_sha256": V.sha256((PACKAGE / author["review"]).read_bytes()),
        "inputs_sha256": V.sha256((control / "inputs.json").read_bytes()),
        "documents": [
            {
                "source_id": item["source_id"],
                "normalized_sha256": item["normalized_sha256"],
                "span_count": item["span_count"],
                "byte_length": item["byte_length"],
            }
            for item in documents
        ],
        "created_at": FIXED_AT,
    }
    write_json(control / "manifest.json", manifest, trailing_newline=False)
    return root, replay


def expansion(atom: dict) -> str:
    """Mirror the product's detailed-density sentence."""
    conditions = "、".join(atom["conditions"]) or "无附加条件"
    requires = "、".join(atom["requires"]) or "无"
    statement = atom["statement"].rstrip("。")
    return (
        f"{statement}。适用条件：{conditions}。前置知识：{requires}。"
        f"来源：{atom['source_id']} 第 {atom['line']} 行。"
    )


def density_entries(replay: dict, *, expanded: bool) -> list[dict]:
    return [
        {
            "atom_id": atom["atom_id"],
            "kind": atom["kind"],
            "statement": atom["statement"],
            "conditions": atom["conditions"],
            "requires": atom["requires"],
            "source": {"source_id": atom["source_id"], "line": atom["line"]},
            "expansion": expansion(atom) if expanded else None,
        }
        for atom in replay["kept_atoms"]
    ]


def density_counts(replay: dict) -> dict:
    return {
        "atoms": replay["counts"]["kept_atoms"],
        "conditions": replay["counts"]["conditions"],
    }


def build_output(directory: Path, replay: dict) -> Path:
    """Write the output root a correct build produces."""
    case_id = replay["author"]["case_id"]
    root = directory / "root"
    out = directory / "out"
    out.mkdir(parents=True)

    filter_record = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "level": replay["audience"]["level"],
        "purpose": replay["audience"]["purpose"],
        "counts": replay["counts"],
        "reason_counts": replay["reason_counts"],
        "spans": replay["spans"],
        "atoms": replay["decisions"],
    }
    densities_record = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "unchanged_across_densities": True,
        "minimal": {
            "policy": V.POLICY,
            "schema_version": V.SCHEMA_VERSION,
            "case_id": case_id,
            "density": "minimal",
            "level": replay["audience"]["level"],
            "counts": density_counts(replay),
            "atoms": density_entries(replay, expanded=False),
        },
        "detailed": {
            "policy": V.POLICY,
            "schema_version": V.SCHEMA_VERSION,
            "case_id": case_id,
            "density": "detailed",
            "level": replay["audience"]["level"],
            "counts": density_counts(replay),
            "atoms": density_entries(replay, expanded=True),
        },
    }
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "level": replay["audience"]["level"],
        "counts": replay["counts"],
        "reason_counts": replay["reason_counts"],
        "densities": {"minimal": density_counts(replay), "detailed": density_counts(replay)},
        "unchanged_across_densities": True,
        "model_calls": 0,
        "network_requests": 0,
    }
    counts = replay["counts"]
    lines = [
        f"# 相关性筛选记录（{case_id}）",
        "",
        "## 计数",
        "",
        f"- 保留正文片段 {counts['kept_spans']} 个、排除 {counts['excluded_spans']} 个",
        "",
        "## 排除理由分布",
        "",
        "| 理由 | 条数 |",
        "| --- | --- |",
    ]
    lines += [f"| {reason} | {tally} |" for reason, tally in replay["reason_counts"].items()]
    lines += ["", "## 排除明细", "", "| 来源 | 行 | 理由 | 说明 |", "| --- | --- | --- | --- |"]
    lines += [
        f"| {item['source_id']} | {item['line']} | {item['reason']} | {item['detail']} |"
        for item in replay["excluded_specs"]
    ]
    lines += ["", "## 保留原子与两种密度", "", "| 原子 | 状态 |", "| --- | --- |"]
    lines += [f"| {atom['atom_id']} | {atom['status']} |" for atom in replay["decisions"]]
    lines += [
        "",
        "## 边界",
        "",
        "- 模型调用与网络请求：0 次；本阶段不判断科学正确性。",
        "",
    ]
    markdown = "\n".join(lines)

    write_json(out / V.FILTER, filter_record)
    write_json(out / V.DENSITIES, densities_record)
    write_json(out / V.REPORT, report)
    (out / V.MARKDOWN).write_bytes(markdown.encode("utf-8"))
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": case_id,
        "source_root": str(root.resolve()),
        "source_inputs_sha256": V.sha256((root / V.INPUTS).read_bytes()),
        "filter_sha256": V.sha256((out / V.FILTER).read_bytes()),
        "densities_sha256": V.sha256((out / V.DENSITIES).read_bytes()),
        "report_sha256": V.sha256((out / V.REPORT).read_bytes()),
        "markdown_sha256": V.sha256((out / V.MARKDOWN).read_bytes()),
        "counts": counts,
    }
    write_json(out / V.OUTPUT_MANIFEST, manifest)
    return out


def audit(root: Path, output: Path | None = None, package: Path = PACKAGE):
    report = V.Report()
    V.check_layout(report, root)
    manifest = V.check_manifest(report, root, package)
    inputs = V.check_inputs(report, root, manifest)
    V.check_expected(report, package)
    V.check_frozen(report, package)
    replayed = V.check_replay(report, package, root, manifest, inputs)
    if output is not None and replayed is not None:
        V.check_filter(report, output, replayed)
        densities = V.check_densities(report, output, replayed)
        V.check_report(report, output, replayed, densities)
        V.check_markdown(report, output, replayed)
        V.check_output_manifest(report, output, replayed)
    return report


def failed(report) -> set[str]:
    return {item["check"] for item in report.checks if not item["ok"]}


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="relevance-audit-"))

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
        V.check_frozen(report, PACKAGE)
        self.assertEqual(failed(report), set())

    def test_tampered_member_byte_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        target = copy / "documents" / "basics.md"
        target.write_bytes(target.read_bytes() + b"\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-digests", failed(report))

    def test_extra_member_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "documents" / "extra.md").write_bytes(b"# extra\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-set", failed(report))


class PackagePriorTests(unittest.TestCase):
    def test_expected_priors_match_the_independent_replay(self) -> None:
        report = V.Report()
        V.check_expected(report, PACKAGE)
        self.assertEqual(failed(report), set())

    def mutate_expected(self, mutate) -> set[str]:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        path = copy / "expected.json"
        value = json.loads(path.read_bytes().decode("utf-8"))
        mutate(value)
        write_json(path, value)
        report = V.Report()
        V.check_expected(report, copy)
        return failed(report)

    def test_prior_count_drift_is_detected(self) -> None:
        names = self.mutate_expected(
            lambda value: value["counts"].__setitem__("kept_spans", 99)
        )
        self.assertIn("expected-counts-kept_spans", names)

    def test_prior_exclusion_drift_is_detected(self) -> None:
        names = self.mutate_expected(
            lambda value: value["exclusions"][0].__setitem__("line", 2)
        )
        self.assertIn("expected-exclusions", names)

    def test_prior_atom_drift_is_detected(self) -> None:
        names = self.mutate_expected(lambda value: value["atoms"].pop())
        self.assertIn("expected-atoms", names)

    def test_prior_density_drift_is_detected(self) -> None:
        names = self.mutate_expected(
            lambda value: value["densities"]["minimal"].__setitem__("conditions", 4)
        )
        self.assertIn("expected-densities", names)

    def test_prior_rejection_drift_is_detected(self) -> None:
        names = self.mutate_expected(
            lambda value: value["rejections"][0].__setitem__("code", "RELEVANCE_ARGUMENT")
        )
        self.assertIn("expected-rejections", names)


class CleanRootTests(SandboxCase):
    def test_a_clean_root_and_output_pass(self) -> None:
        report = self.audit()
        self.assertEqual(failed(report), set())
        self.assertGreaterEqual(len(report.checks), 100)

    def test_a_clean_root_alone_passes(self) -> None:
        report = self.audit(output=False)
        self.assertEqual(failed(report), set())


class LayoutTests(SandboxCase):
    def test_an_extra_top_level_entry_is_detected(self) -> None:
        (self.root / "notes.txt").write_bytes(b"hi\n")
        self.assertIn("layout-root-entries", failed(self.audit(output=False)))

    def test_an_extra_control_entry_is_detected(self) -> None:
        (self.root / "control" / "extra.json").write_bytes(b"{}\n")
        self.assertIn("layout-control-entries", failed(self.audit(output=False)))

    def test_a_missing_text_directory_is_detected(self) -> None:
        shutil.rmtree(self.root / "control" / "text")
        self.assertIn("layout-text", failed(self.audit(output=False)))

    def test_a_symlinked_text_file_is_detected(self) -> None:
        target = self.root / "control" / "text" / "doc-basics.md"
        copy = self.root / "control" / "text" / "copy.md"
        copy.write_bytes(target.read_bytes())
        target.unlink()
        create_symlink_or_skip(target, copy.name)
        self.assertIn("layout-no-links", failed(self.audit(output=False)))


class ManifestTests(SandboxCase):
    def mutate(self, mutate) -> set[str]:
        path = self.root / "control" / "manifest.json"
        value = self.read(path)
        mutate(value)
        self.write(path, value, trailing_newline=False)
        return failed(self.audit(output=False))

    def test_an_unknown_field_is_refused(self) -> None:
        self.assertIn("manifest-strict", self.mutate(lambda v: v.__setitem__("extra", 1)))

    def test_pretty_printed_manifest_is_refused(self) -> None:
        path = self.root / "control" / "manifest.json"
        value = self.read(path)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
        self.assertIn("manifest-strict", failed(self.audit(output=False)))

    def test_root_path_drift_is_detected(self) -> None:
        names = self.mutate(lambda v: v.__setitem__("root", "/tmp/elsewhere"))
        self.assertIn("manifest-root", names)

    def test_author_digest_drift_is_detected(self) -> None:
        names = self.mutate(lambda v: v.__setitem__("author_sha256", "0" * 64))
        self.assertIn("manifest-author_sha256", names)

    def test_audience_digest_drift_is_detected(self) -> None:
        names = self.mutate(lambda v: v.__setitem__("audience_sha256", "0" * 64))
        self.assertIn("manifest-audience_sha256", names)

    def test_inputs_digest_drift_is_detected(self) -> None:
        names = self.mutate(lambda v: v.__setitem__("inputs_sha256", "0" * 64))
        self.assertIn("manifest-inputs-sha256", names)

    def test_missing_frozen_text_is_detected(self) -> None:
        (self.root / "control" / "text" / "doc-anecdote.md").unlink()
        names = failed(self.audit(output=False))
        self.assertTrue({"manifest-text-anecdote", "manifest-text-set"} & names, names)

    def test_extra_frozen_text_is_detected(self) -> None:
        (self.root / "control" / "text" / "doc-extra.md").write_bytes(b"# extra\n")
        self.assertIn("manifest-text-set", failed(self.audit(output=False)))

    def test_frozen_body_byte_drift_is_detected(self) -> None:
        path = self.root / "control" / "text" / "doc-basics.md"
        path.write_bytes(path.read_bytes() + b"\n")
        names = failed(self.audit(output=False))
        self.assertTrue({"manifest-text-doc-basics", "inputs-spans-doc-basics"} & names, names)


class InputsTests(SandboxCase):
    def mutate_inputs(self, mutate) -> set[str]:
        path = self.root / "control" / "inputs.json"
        value = self.read(path)
        mutate(value)
        self.write(path, value)
        return failed(self.audit(output=False))

    def test_span_context_drift_is_detected(self) -> None:
        names = self.mutate_inputs(
            lambda v: v["documents"][0]["spans"][0].__setitem__("kind", "text")
        )
        self.assertIn("inputs-spans-doc-basics", names)

    def test_span_offset_drift_is_detected(self) -> None:
        names = self.mutate_inputs(
            lambda v: v["documents"][0]["spans"][0].__setitem__("end_byte", 999)
        )
        self.assertTrue({"inputs-spans-doc-basics", "inputs-coverage-doc-basics"} & names, names)

    def test_a_dropped_document_is_detected(self) -> None:
        names = self.mutate_inputs(lambda v: v["documents"].pop())
        self.assertIn("inputs-document-count", names)

    def test_a_broken_case_id_is_detected(self) -> None:
        names = self.mutate_inputs(lambda v: v.__setitem__("case_id", "other-case"))
        self.assertIn("inputs-case-id", names)


class ReplayTests(SandboxCase):
    def test_a_declaration_that_differs_from_the_package_is_detected(self) -> None:
        path = self.root / "control" / "audience.json"
        path.write_bytes(path.read_bytes() + b"\n")
        self.assertIn("replay-frozen-control/audience.json", failed(self.audit()))

    def test_replay_refuses_an_unregistered_reason(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        author = json.loads((copy / "author.json").read_bytes().decode("utf-8"))
        path = copy / author["review"]
        value = json.loads(path.read_bytes().decode("utf-8"))
        value["reasons"] = [
            item for item in value["reasons"] if item["reason"] != "pure_exercise"
        ]
        write_json(path, value)
        report = V.Report()
        V.check_replay(report, copy, self.root, None, None)
        self.assertIn("replay-package", failed(report))


class OutputFilterTests(SandboxCase):
    def mutate_filter(self, mutate) -> set[str]:
        path = self.out / V.FILTER
        value = self.read(path)
        mutate(value)
        self.write(path, value)
        return failed(self.audit())

    def test_a_filter_span_drift_is_detected(self) -> None:
        names = self.mutate_filter(
            lambda v: v["spans"][0].__setitem__("status", "excluded")
        )
        self.assertIn("filter-spans", names)

    def test_a_filter_atom_drift_is_detected(self) -> None:
        names = self.mutate_filter(
            lambda v: v["atoms"][0].__setitem__("required", not v["atoms"][0]["required"])
        )
        self.assertIn("filter-atoms", names)

    def test_a_missing_filter_is_detected(self) -> None:
        (self.out / V.FILTER).unlink()
        self.assertIn("filter-present", failed(self.audit()))

    def test_a_pretty_printed_filter_is_detected(self) -> None:
        path = self.out / V.FILTER
        value = self.read(path)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
        self.assertIn("filter-strict", failed(self.audit()))


class DensitiesTests(SandboxCase):
    def mutate_densities(self, mutate) -> set[str]:
        path = self.out / V.DENSITIES
        value = self.read(path)
        mutate(value)
        self.write(path, value)
        return failed(self.audit())

    def test_an_expansion_in_the_minimal_density_is_detected(self) -> None:
        names = self.mutate_densities(
            lambda v: v["minimal"]["atoms"][0].__setitem__("expansion", "不该出现")
        )
        self.assertIn("densities-minimal-bare", names)

    def test_a_missing_expansion_in_the_detailed_density_is_detected(self) -> None:
        names = self.mutate_densities(
            lambda v: v["detailed"]["atoms"][0].__setitem__("expansion", None)
        )
        self.assertIn("densities-detailed-expanded", names)

    def test_a_changed_atom_order_is_detected(self) -> None:
        names = self.mutate_densities(lambda v: v["detailed"]["atoms"].reverse())
        self.assertIn("densities-detailed-atom-order", names)

    def test_a_false_unchanged_flag_is_detected(self) -> None:
        names = self.mutate_densities(
            lambda v: v.__setitem__("unchanged_across_densities", False)
        )
        self.assertIn("densities-same-atoms", names)

    def test_a_different_condition_count_is_detected(self) -> None:
        names = self.mutate_densities(
            lambda v: v["detailed"].__setitem__(
                "counts", {"atoms": v["detailed"]["counts"]["atoms"], "conditions": 99}
            )
        )
        self.assertIn("densities-detailed-counts", names)


class ReportAndMarkdownTests(SandboxCase):
    def test_report_count_drift_is_detected(self) -> None:
        path = self.out / V.REPORT
        value = self.read(path)
        value["counts"]["kept_spans"] = 99
        self.write(path, value)
        self.assertIn("report-counts", failed(self.audit()))

    def test_a_zero_call_claim_is_required(self) -> None:
        path = self.out / V.REPORT
        value = self.read(path)
        value["model_calls"] = 3
        self.write(path, value)
        self.assertIn("report-zero-calls", failed(self.audit()))

    def test_a_missing_markdown_section_is_detected(self) -> None:
        path = self.out / V.MARKDOWN
        path.write_text(
            path.read_text(encoding="utf-8").replace("## 排除明细", "## 明细"),
            encoding="utf-8", newline="\n",
        )
        self.assertIn("markdown-sections", failed(self.audit()))

    def test_a_missing_reason_row_is_detected(self) -> None:
        path = self.out / V.MARKDOWN
        kept = [
            line
            for line in path.read_text(encoding="utf-8").splitlines(keepends=True)
            if "decoration" not in line
        ]
        path.write_text("".join(kept), encoding="utf-8", newline="\n")
        self.assertIn("markdown-reasons", failed(self.audit()))


class OutputManifestTests(SandboxCase):
    def mutate_manifest(self, mutate) -> set[str]:
        path = self.out / V.OUTPUT_MANIFEST
        value = self.read(path)
        mutate(value)
        self.write(path, value)
        return failed(self.audit())

    def test_a_timestamp_is_detected(self) -> None:
        names = self.mutate_manifest(lambda v: v.__setitem__("created_at", FIXED_AT))
        self.assertIn("output-manifest-strict", names)

    def test_a_digest_drift_is_detected(self) -> None:
        names = self.mutate_manifest(lambda v: v.__setitem__("report_sha256", "0" * 64))
        self.assertIn("output-manifest-digests", names)

    def test_an_extra_output_entry_is_detected(self) -> None:
        (self.out / "notes.md").write_bytes(b"# extra\n")
        self.assertIn("output-entries", failed(self.audit()))

    def test_a_missing_output_manifest_is_detected(self) -> None:
        (self.out / V.OUTPUT_MANIFEST).unlink()
        self.assertIn("output-manifest-present", failed(self.audit()))


class CommandLineTests(SandboxCase):
    def run_cli(self, *arguments: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *arguments],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_package_only_mode_exits_zero(self) -> None:
        completed = self.run_cli("--package", str(PACKAGE))
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("失败 0", completed.stdout)

    def test_no_arguments_exit_two(self) -> None:
        completed = self.run_cli()
        self.assertEqual(completed.returncode, 2)
        self.assertIn("用法", completed.stderr)

    def test_a_tampered_root_exits_one(self) -> None:
        (self.root / "control" / "text" / "doc-basics.md").write_bytes(b"# x\n")
        completed = self.run_cli(str(self.root))
        self.assertEqual(completed.returncode, 1)
        self.assertIn("失败", completed.stdout)

    def test_json_mode_reports_structured_failures(self) -> None:
        (self.out / V.FILTER).unlink()
        completed = self.run_cli(str(self.root), "--output", str(self.out), "--json")
        self.assertEqual(completed.returncode, 1)
        payload = json.loads(completed.stdout)
        self.assertTrue(payload["failures"] > 0)
        self.assertIn("filter-present", payload["failure_detail"][0])

    def test_freeze_rewrites_the_same_bytes(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        before = (copy / "frozen-sha256.json").read_bytes()
        completed = self.run_cli("--package", str(copy), "--freeze")
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual((copy / "frozen-sha256.json").read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
