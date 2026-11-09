"""Tests for the independent outline-alignment auditor (B10-T01).

The auditor never imports the product and never runs the CLI. These tests build an
alignment root with the auditor's own replay, so a green run proves the checks read
the documented layout rather than mirroring whatever the product happens to write.
"""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_outline_alignment.py"
PACKAGE = ROOT / "fixtures" / "outline-alignment-v1"
FIXED_AT = "2026-11-07T00:00:00Z"

MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "root",
    "root_identity",
    "parent_identity",
    "control_identity",
    "manifest_identity",
    "inputs_identity",
    "author_manifest_sha256",
    "lexicon_sha256",
    "inputs_sha256",
    "documents",
    "created_at",
)


def load_auditor(module_name: str = "verify_outline_alignment"):
    spec = importlib.util.spec_from_file_location(module_name, SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def fake_identity(index: int) -> dict:
    return {"platform": "unix", "device": 1, "inode": index}


def segment(text: str) -> list[dict]:
    """Tile the whole text with contiguous spans, one line at a time.

    The product groups consecutive body lines into one span; the contract only
    requires contiguous coverage with the right heading count, so a per-line tiling
    is a faithful independent computation.
    """
    spans: list[dict] = []
    offset = 0
    for number, line in enumerate(text.splitlines(keepends=True), start=1):
        start = offset
        end = offset + len(line.encode("utf-8"))
        offset = end
        content = line.rstrip("\n")
        if V.heading_level(content) > 0:
            kind = "heading"
        elif content.strip() == "":
            kind = "formatting"
        else:
            kind = "text"
        spans.append(
            {
                "span_id": f"span-{len(spans) + 1:05d}",
                "kind": kind,
                "start_byte": start,
                "end_byte": end,
                "start_line": number,
                "end_line": number,
                "text_sha256": V.sha256(line.encode("utf-8")),
            }
        )
    return spans


def write_json(path: Path, value: object, trailing_newline: bool = True) -> None:
    payload = V.canonical(value) + ("\n" if trailing_newline else "")
    path.write_bytes(payload.encode("utf-8"))


def build_root(directory: Path, *, output: bool = False) -> tuple[Path, dict]:
    """Build an alignment root (and optionally an output root) from the replay."""
    replay = V.replay_package(PACKAGE)
    author = replay["author"]
    root = directory / "root"
    control = root / "control"
    text_dir = control / "text"
    text_dir.mkdir(parents=True)

    documents = []
    for spec in author["documents"]:
        data = (PACKAGE / spec["path"]).read_bytes()
        (text_dir / f"{spec['source_id']}.md").write_bytes(data)
        spans = segment(data.decode("utf-8"))
        documents.append(
            {
                "source_id": spec["source_id"],
                "title": spec["title"],
                "path": spec["path"],
                "raw_sha256": V.sha256(data),
                "byte_length": len(data),
                "normalized_sha256": V.sha256(data),
                "span_count": len(spans),
                "spans": spans,
            }
        )

    lexicon = {
        "policy": V.CONCEPTS_POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "concepts": replay["concepts"],
    }
    lexicon_path = text_dir / f"{author['case_id']}{V.LEXICON_SUFFIX}"
    write_json(lexicon_path, lexicon)

    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": author["case_id"],
        "documents": documents,
    }
    inputs_path = control / "inputs.json"
    write_json(inputs_path, inputs)

    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": author["case_id"],
        "root": str(root.resolve()),
        "root_identity": fake_identity(1),
        "parent_identity": fake_identity(2),
        "control_identity": fake_identity(3),
        "manifest_identity": fake_identity(4),
        "inputs_identity": fake_identity(5),
        "author_manifest_sha256": V.sha256((PACKAGE / "author.json").read_bytes()),
        "lexicon_sha256": V.sha256(lexicon_path.read_bytes()),
        "inputs_sha256": V.sha256(inputs_path.read_bytes()),
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
    write_json(control / "manifest.json", manifest)
    if not output:
        return root, replay
    return root, replay


def build_output(directory: Path, replay: dict) -> Path:
    """Write a derived output root that the contract says a correct build produces."""
    titles = {
        spec["source_id"]: spec["title"] for spec in replay["author"]["documents"]
    }
    tree = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["author"]["case_id"],
        "documents": [
            {
                "source_id": item["source_id"],
                "title": titles[item["source_id"]],
                "sections": item["sections"],
                "aligned": item["aligned"],
                "unaligned": item["unaligned"],
            }
            for item in replay["trees"]
        ],
    }
    counts = {
        "documents": len(replay["trees"]),
        "sections": sum(len(item["sections"]) for item in replay["trees"]),
        "sections_aligned": sum(item["aligned"] for item in replay["trees"]),
        "sections_unaligned": sum(item["unaligned"] for item in replay["trees"]),
        "concepts": len(replay["inventory"]),
        "occurrences": sum(item["total"] for item in replay["inventory"]),
        "warnings": len(replay["warnings"]),
    }
    tally: dict[str, int] = {}
    for item in replay["warnings"]:
        tally[item["kind"]] = tally.get(item["kind"], 0) + 1
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["author"]["case_id"],
        "counts": counts,
        "warning_counts": dict(sorted(tally.items())),
        "warnings": replay["warnings"],
        "model_calls": 0,
        "network_requests": 0,
    }
    inventory = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["author"]["case_id"],
        "concepts": [
            {
                "concept_id": item["concept_id"],
                "canonical": item["canonical"],
                "aliases": next(
                    concept["aliases"]
                    for concept in replay["concepts"]
                    if concept["concept_id"] == item["concept_id"]
                ),
                "total": item["total"],
                "by_document": item["by_document"],
                "contexts": item["contexts"],
                "first": (
                    {
                        "document": item["first"].split(":")[0],
                        "line": int(item["first"].split(":")[1]),
                    }
                    if item["first"]
                    else None
                ),
                "occurrences": item["occurrences"],
            }
            for item in replay["inventory"]
        ],
    }
    markdown = (
        "# 章节对齐与全局概念清单（%s）\n\n## 章节对齐\n\n## 全局概念清单\n\n"
        "## 对齐警告\n\n## 边界\n\n- 确定性推导，零模型调用与零网络请求。\n"
        % replay["author"]["case_id"]
    )
    out = directory / "out"
    out.mkdir(parents=True)
    write_json(out / V.TREE, tree)
    write_json(out / V.CONCEPTS, inventory)
    write_json(out / V.REPORT, report)
    (out / V.MARKDOWN).write_bytes(markdown.encode("utf-8"))
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["author"]["case_id"],
        "source_root": str(directory.joinpath("root").resolve()),
        "source_inputs_sha256": V.sha256(
            (directory / "root" / V.INPUTS).read_bytes()
        ),
        "tree_sha256": V.sha256((out / V.TREE).read_bytes()),
        "concepts_sha256": V.sha256((out / V.CONCEPTS).read_bytes()),
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
    V.check_inputs(report, root, manifest)
    V.check_expected(report, package)
    V.check_frozen(report, package)
    if output is not None:
        tree = json.loads((output / V.TREE).read_bytes().decode("utf-8"))
        inventory = json.loads((output / V.CONCEPTS).read_bytes().decode("utf-8"))
        result = json.loads((output / V.REPORT).read_bytes().decode("utf-8"))
        V.check_counts(report, tree, inventory, result)
        V.check_output(report, output, tree, inventory, result)
        V.check_replay(report, package, tree, inventory, result)
    return report


def failed_names(report) -> set[str]:
    return {item["check"] for item in report.checks if not item["ok"]}


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="outline-audit-"))

    def path(self, name: str) -> Path:
        return self.directory / name

    def close(self) -> None:
        shutil.rmtree(self.directory, ignore_errors=True)


class FrozenPackageTests(unittest.TestCase):
    def test_frozen_manifest_matches_members(self) -> None:
        report = V.Report()
        V.check_frozen(report, PACKAGE)
        self.assertEqual(failed_names(report), set())

    def test_tampered_member_byte_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        target = copy / "documents" / "glycolysis.md"
        target.write_bytes(target.read_bytes() + b"\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-digests", failed_names(report))

    def test_extra_member_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "documents" / "extra.md").write_bytes(b"# extra\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-set", failed_names(report))


class CleanRootTests(unittest.TestCase):
    def test_clean_root_and_output_pass_every_check(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        root, replay = build_root(sandbox.directory)
        output = build_output(sandbox.directory, replay)
        report = audit(root, output)
        self.assertEqual(failed_names(report), set())
        self.assertGreater(len(report.checks), 60)

    def test_root_without_output_still_checks_the_frozen_inputs(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        root, _ = build_root(sandbox.directory)
        report = audit(root)
        self.assertEqual(failed_names(report), set())


class LayoutTests(unittest.TestCase):
    def test_extra_top_level_entry_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        root, _ = build_root(sandbox.directory)
        (root / "notes.md").write_bytes(b"stray")
        report = audit(root)
        self.assertIn("layout-root-entries", failed_names(report))

    def test_extra_control_entry_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        root, _ = build_root(sandbox.directory)
        (root / "control" / "extra.json").write_bytes(b"{}")
        report = audit(root)
        self.assertIn("layout-control-entries", failed_names(report))

    def test_symlinked_root_is_detected(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        root, _ = build_root(sandbox.directory)
        link = sandbox.path("link")
        link.symlink_to(root, target_is_directory=True)
        report = audit(link)
        self.assertIn("layout-no-links", failed_names(report))


class ManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)
        self.root, _ = build_root(self.sandbox.directory)
        self.manifest_path = self.root / "control" / "manifest.json"
        self.manifest = json.loads(self.manifest_path.read_text("utf-8"))

    def rewrite(self, value: dict, *, canonical: bool = True) -> None:
        if canonical:
            write_json(self.manifest_path, value)
        else:
            self.manifest_path.write_text(
                json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
            )

    def test_unknown_field_is_rejected(self) -> None:
        value = dict(self.manifest)
        value["extra"] = 1
        self.rewrite(value)
        self.assertIn("manifest-strict", failed_names(audit(self.root)))

    def test_pretty_printed_manifest_is_rejected(self) -> None:
        self.rewrite(self.manifest, canonical=False)
        self.assertIn("manifest-strict", failed_names(audit(self.root)))

    def test_root_must_match_the_real_path(self) -> None:
        value = dict(self.manifest)
        value["root"] = str(self.root) + "-elsewhere"
        self.rewrite(value)
        self.assertIn("manifest-root", failed_names(audit(self.root)))

    def test_author_digest_drift_is_detected(self) -> None:
        value = dict(self.manifest)
        value["author_manifest_sha256"] = "0" * 64
        self.rewrite(value)
        self.assertIn("manifest-author-sha256", failed_names(audit(self.root)))

    def test_inputs_digest_drift_is_detected(self) -> None:
        value = dict(self.manifest)
        value["inputs_sha256"] = "0" * 64
        self.rewrite(value)
        self.assertIn("manifest-inputs-sha256", failed_names(audit(self.root)))

    def test_missing_lexicon_is_detected(self) -> None:
        (self.root / "control" / "text" / "outl-glycolysis-v1.lexicon.json").unlink()
        self.assertIn("manifest-lexicon-present", failed_names(audit(self.root)))

    def test_text_digest_drift_is_detected(self) -> None:
        body = self.root / "control" / "text" / "doc-glycolysis.md"
        body.write_bytes(body.read_bytes().replace("糖酵解".encode(), "糖酵解".encode() + b" "))
        failed = failed_names(audit(self.root))
        self.assertIn("manifest-text-doc-glycolysis", failed)
        self.assertIn("inputs-spans-doc-glycolysis", failed)

    def test_extra_frozen_text_is_detected(self) -> None:
        (self.root / "control" / "text" / "stray.md").write_bytes(b"# stray\n")
        self.assertIn("manifest-text-set", failed_names(audit(self.root)))


class InputsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)
        self.root, _ = build_root(self.sandbox.directory)
        self.path = self.root / "control" / "inputs.json"
        self.inputs = json.loads(self.path.read_text("utf-8"))

    def test_case_id_drift_is_detected(self) -> None:
        value = json.loads(json.dumps(self.inputs))
        value["case_id"] = "other-case"
        write_json(self.path, value)
        self.assertIn("inputs-case-id", failed_names(audit(self.root)))

    def test_gap_in_span_coverage_is_detected(self) -> None:
        value = json.loads(json.dumps(self.inputs))
        document = value["documents"][0]
        document["spans"] = document["spans"][1:]
        write_json(self.path, value)
        manifest = json.loads((self.root / "control" / "manifest.json").read_text("utf-8"))
        manifest["inputs_sha256"] = V.sha256(self.path.read_bytes())
        write_json(self.root / "control" / "manifest.json", manifest)
        self.assertIn("inputs-spans-doc-glycolysis", failed_names(audit(self.root)))

    def test_heading_span_count_must_match_the_independent_derivation(self) -> None:
        value = json.loads(json.dumps(self.inputs))
        document = value["documents"][0]
        document["spans"][0]["kind"] = "text"
        write_json(self.path, value)
        manifest = json.loads((self.root / "control" / "manifest.json").read_text("utf-8"))
        manifest["inputs_sha256"] = V.sha256(self.path.read_bytes())
        write_json(self.root / "control" / "manifest.json", manifest)
        self.assertIn("inputs-headings-doc-glycolysis", failed_names(audit(self.root)))


class ExpectedTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)
        self.copy = self.sandbox.path("package")
        shutil.copytree(PACKAGE, self.copy)
        self.expected_path = self.copy / "expected.json"
        self.expected = json.loads(self.expected_path.read_text("utf-8"))

    def test_warning_line_drift_is_detected(self) -> None:
        value = json.loads(json.dumps(self.expected))
        value["warnings"][0]["line"] = 999
        write_json(self.expected_path, value)
        report = V.Report()
        V.check_expected(report, self.copy)
        self.assertIn("expected-warnings", failed_names(report))

    def test_count_drift_is_detected(self) -> None:
        value = json.loads(json.dumps(self.expected))
        value["counts"]["sections"] = 16
        write_json(self.expected_path, value)
        report = V.Report()
        V.check_expected(report, self.copy)
        self.assertIn("expected-counts", failed_names(report))

    def test_concept_index_drift_is_detected(self) -> None:
        value = json.loads(json.dumps(self.expected))
        value["concepts"][0]["total"] = 3
        write_json(self.expected_path, value)
        report = V.Report()
        V.check_expected(report, self.copy)
        self.assertIn("expected-concepts", failed_names(report))


class ReplayRuleTests(unittest.TestCase):
    """The replay must encode the rules, not just copy the product's numbers."""

    def setUp(self) -> None:
        self.replay = V.replay_package(PACKAGE)
        self.concepts = self.replay["concepts"]

    def test_numbering_depth_is_judged_against_the_parent_section(self) -> None:
        text = "# 章\n\n## 1 节\n\n### 1.1 子节\n\n内容\n"
        result = V.replay_document(text, "doc", "标题", [])
        kinds = [item["kind"] for item in result["warnings"]]
        self.assertNotIn("number_depth_mismatch", kinds)

    def test_numbering_depth_mismatch_is_reported_for_the_deep_number(self) -> None:
        text = "# 章\n\n## 1 节\n\n### 1.1.1 过深\n\n内容\n"
        result = V.replay_document(text, "doc", "标题", [])
        self.assertIn(
            "number_depth_mismatch", [item["kind"] for item in result["warnings"]]
        )

    def test_a_section_owns_every_line_until_the_next_heading(self) -> None:
        text = "# 章\n\n## 1 节\n\n正文一\n\n正文二\n\n## 2 节\n\n正文三\n"
        result = V.replay_document(text, "doc", "标题", [])
        spans = [
            (item["start_line"], item["end_line"]) for item in result["sections"]
        ]
        # The chapter heading owns lines 1-2; the body lines belong to their section
        # and the last section owns the trailing text, so no body line is orphaned.
        self.assertEqual(spans, [(1, 2), (3, 8), (9, 11)])

    def test_heading_term_missing_from_the_body_is_reported(self) -> None:
        text = "# 章\n\n## 1 糖酵解净能量收支\n\n本节只写别的。\n"
        result = V.replay_document(text, "doc", "标题", self.concepts)
        warnings = V.replay_package(PACKAGE)["warnings"]
        self.assertIn(
            "heading_concept_missing_from_body", [item["kind"] for item in warnings]
        )
        self.assertTrue(result["sections"])

    def test_caption_and_appendix_occurrences_are_classified(self) -> None:
        text = "# 附录A 术语\n\n图 1 列出 NADPH 的用途\n"
        result = V.replay_document(text, "doc", "标题", self.concepts)
        contexts = {item["context"] for item in result["occurrences"]}
        self.assertEqual(contexts, {"caption"})

    def test_a_concept_only_in_heading_is_not_caption_only(self) -> None:
        text = "# NADPH 专题\n\n正文提到别的。\n"
        replayed = V.replay_package(PACKAGE)
        self.assertNotIn(
            "concept_only_in_caption_or_appendix",
            [
                item["kind"]
                for item in replayed["warnings"]
                if item["document"] == "doc-glycolysis"
            ],
        )
        result = V.replay_document(text, "doc", "标题", self.concepts)
        self.assertEqual({item["context"] for item in result["occurrences"]}, {"heading"})

    def test_document_without_headings_reports_no_headings(self) -> None:
        result = V.replay_document("只有正文。\n", "doc", "标题", [])
        self.assertEqual([item["kind"] for item in result["warnings"]], ["no_headings"])

    def test_replay_matches_the_frozen_expectations(self) -> None:
        report = V.Report()
        V.check_expected(report, PACKAGE)
        self.assertEqual(failed_names(report), set())


class OutputTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)
        self.root, replay = build_root(self.sandbox.directory)
        self.output = build_output(self.sandbox.directory, replay)
        self.report_path = self.output / V.REPORT
        self.result = json.loads(self.report_path.read_text("utf-8"))

    def test_output_manifest_records_digests_and_counts(self) -> None:
        report = audit(self.root, self.output)
        self.assertEqual(failed_names(report), set())

    def test_count_drift_in_report_is_detected(self) -> None:
        value = json.loads(json.dumps(self.result))
        value["counts"]["warnings"] = 10
        write_json(self.report_path, value)
        failed = failed_names(audit(self.root, self.output))
        self.assertIn("counts-derive-from-tables", failed)

    def test_aligned_and_unaligned_must_sum_to_the_section_total(self) -> None:
        tree_path = self.output / V.TREE
        tree = json.loads(tree_path.read_text("utf-8"))
        tree["documents"][0]["aligned"] += 1
        write_json(tree_path, tree)
        report = V.Report()
        V.check_counts(report, tree, json.loads((self.output / V.CONCEPTS).read_text("utf-8")), self.result)
        self.assertIn("counts-derive-from-tables", failed_names(report))

    def test_markdown_without_the_boundary_sections_is_detected(self) -> None:
        (self.output / V.MARKDOWN).write_bytes("# 只有标题\n".encode("utf-8"))
        self.assertIn("output-markdown-sections", failed_names(audit(self.root, self.output)))

    def test_zero_call_claims_are_checked(self) -> None:
        value = json.loads(json.dumps(self.result))
        value["model_calls"] = 1
        write_json(self.report_path, value)
        self.assertIn("output-zero-calls", failed_names(audit(self.root, self.output)))

    def test_output_with_an_extra_file_is_detected(self) -> None:
        (self.output / "extra.json").write_bytes(b"{}")
        self.assertIn("output-entries", failed_names(audit(self.root, self.output)))

    def test_output_manifest_digest_drift_is_detected(self) -> None:
        path = self.output / V.OUTPUT_MANIFEST
        manifest = json.loads(path.read_text("utf-8"))
        manifest["tree_sha256"] = "0" * 64
        write_json(path, manifest)
        self.assertIn(
            "output-manifest-digests", failed_names(audit(self.root, self.output))
        )


class CommandLineTests(unittest.TestCase):
    def test_package_only_mode_reports_success(self) -> None:
        self.assertEqual(V.main(["--package", str(PACKAGE)]), 0)

    def test_missing_arguments_explain_the_usage(self) -> None:
        self.assertEqual(V.main([]), 2)

    def test_freeze_rewrites_the_same_digests(self) -> None:
        sandbox = Sandbox()
        self.addCleanup(sandbox.close)
        copy = sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        before = (copy / "frozen-sha256.json").read_bytes()
        self.assertEqual(V.main(["--package", str(copy), "--freeze"]), 0)
        self.assertEqual((copy / "frozen-sha256.json").read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
