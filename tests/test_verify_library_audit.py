"""Tests for scripts/verify_library_audit.py.

The auditor under test never imports the Rust product and never runs the CLI to
decide anything, so these tests build roots and outputs from the auditor's own
replay helpers and then break exactly one thing at a time.
"""

from __future__ import annotations

import copy
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from symlink_test_support import create_symlink_or_skip

ROOT = Path(__file__).resolve().parents[1]
AUDITOR = ROOT / "scripts" / "verify_library_audit.py"
FIXTURE = ROOT / "fixtures" / "library-audit-v1"


def load_auditor():
    spec = importlib.util.spec_from_file_location("verify_library_audit", AUDITOR)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


V = load_auditor()


def write_json(path: Path, value, trailing_newline: bool = True) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, ensure_ascii=False, indent=2)
    if trailing_newline:
        text += "\n"
    path.write_text(text, encoding="utf-8")


def write_canonical_json(path: Path, value) -> None:
    """Product JSON files are canonical with exactly one trailing newline."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(V.canonical(value) + "\n", encoding="utf-8")


def fake_identity(index: int) -> dict:
    return {"platform": "linux", "device": index, "inode": 100 + index}


def replay_or_fail(project_name: str = "project.json") -> dict:
    replay, code, detail = V.replay_package(FIXTURE, project_name)
    if replay is None:
        raise AssertionError(f"fixture replay failed: {code} {detail}")
    return replay


def build_root(directory: Path, replay: dict) -> Path:
    root = directory / "root"
    control = root / "control"
    (control / "notes").mkdir(parents=True)
    for note in replay["project"]["notes"]:
        target = control / "notes" / note["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(replay["bodies"][note["note_id"]], encoding="utf-8")
    project_text = json.dumps(replay["project"], ensure_ascii=False, indent=2)
    (control / "project.json").write_text(project_text, encoding="utf-8")
    (control / "gold.json").write_text(
        json.dumps(replay["gold"], ensure_ascii=False, indent=2), encoding="utf-8"
    )
    notes = [
        {
            "note_id": note["note_id"],
            "path": note["path"],
            "title": note["title"],
            "sha256": V.sha256((control / "notes" / note["path"]).read_bytes()),
            "byte_length": (control / "notes" / note["path"]).stat().st_size,
        }
        for note in replay["project"]["notes"]
    ]
    inputs = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["project"]["case_id"],
        "gold_sha256": V.sha256((control / "gold.json").read_bytes()),
        "notes": notes,
    }
    inputs_text = V.canonical(inputs)
    (control / "inputs.json").write_text(inputs_text, encoding="utf-8")
    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["project"]["case_id"],
        "root": str(root.resolve()),
        "root_identity": fake_identity(1),
        "parent_identity": fake_identity(2),
        "control_identity": fake_identity(3),
        "manifest_identity": fake_identity(4),
        "notes_identity": fake_identity(5),
        "inputs_identity": fake_identity(6),
        "project_sha256": V.sha256((control / "project.json").read_bytes()),
        "gold_sha256": V.sha256((control / "gold.json").read_bytes()),
        "inputs_sha256": V.sha256((control / "inputs.json").read_bytes()),
        "created_at": "2026-11-08T00:00:00Z",
    }
    (control / "manifest.json").write_text(V.canonical(manifest), encoding="utf-8")
    return root


def report_markdown(replay: dict) -> str:
    audit = replay["audit"]
    lines = [
        f"# 全库链接与解释审计（{replay['project']['case_id']}）",
        "",
        "## 结论",
        "",
        audit["verdict"],
        "",
        "## 口径",
        "",
        "| 口径 | 分子 | 分母 | 比率 |",
        "| --- | --- | --- | --- |",
        f"| 链接可用 | {audit['availability']['numerator']} | {audit['availability']['denominator']} | {audit['availability']['display']} |",
        f"| 关键原子召回 | {audit['key_recall']['numerator']} | {audit['key_recall']['denominator']} | {audit['key_recall']['display']} |",
        f"| 一般原子召回 | {audit['general_recall']['numerator']} | {audit['general_recall']['denominator']} | {audit['general_recall']['display']} |",
        "",
        "## 链接明细",
        "",
        "## 唯一解释位置",
        "",
        "## 缺口",
        "",
        "## 边界",
        "",
        "本阶段不判断内容质量与科学正确性；模型调用与网络请求 0 次。",
        "",
    ]
    return "\n".join(lines)


def build_output(directory: Path, replay: dict) -> Path:
    output = directory / "out"
    output.mkdir(parents=True)
    audit = replay["audit"]
    write_canonical_json(output / "links.json", {"policy": V.POLICY, "case_id": replay["project"]["case_id"],
                                       "links": audit["links"]})
    write_canonical_json(output / "explanations.json", {
        "policy": V.POLICY,
        "case_id": replay["project"]["case_id"],
        "concepts": audit["concepts"],
        "atoms": audit["atoms"],
    })
    report = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["project"]["case_id"],
        "description": replay["project"]["description"],
        "verdict": audit["verdict"],
        "counts": audit["counts"],
        "availability": audit["availability"],
        "key_recall": audit["key_recall"],
        "general_recall": audit["general_recall"],
        "findings": audit["findings"],
        "model_calls": 0,
        "network_requests": 0,
    }
    write_canonical_json(output / "report.json", report)
    (output / "report.md").write_text(report_markdown(replay), encoding="utf-8")
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["project"]["case_id"],
        "source_root": str((directory / "root").resolve()),
        "source_inputs_sha256": V.sha256((directory / "root" / "control" / "inputs.json").read_bytes()),
        "links_sha256": V.sha256((output / "links.json").read_bytes()),
        "explanations_sha256": V.sha256((output / "explanations.json").read_bytes()),
        "report_sha256": V.sha256((output / "report.json").read_bytes()),
        "markdown_sha256": V.sha256((output / "report.md").read_bytes()),
        "verdict": audit["verdict"],
        "counts": audit["counts"],
    }
    write_canonical_json(output / "manifest.json", manifest)
    return output


def audit(root: Path, output: Path | None = None, package: Path | None = None):
    report = V.Report()
    replay = None
    if package is not None:
        replay, code, detail = V.replay_package(package)
        if replay is None:
            report.record("replay", False, f"{code} {detail}")
        else:
            V.check_frozen(package, report)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            V.check_expected(replay, prior, report)
            V.check_variants(package, prior, report)
    if root is not None:
        V.check_layout(root, report)
        manifest = V.check_manifest(root, report)
        if replay is not None and manifest is not None:
            V.check_project(root, replay, report)
            V.check_inputs(root, replay, manifest, report)
        if output is not None and replay is not None:
            V.check_output(root, output, replay, report)
    return report


def failed(report) -> set[str]:
    return {name for name, ok, _ in report.checks if not ok}


def show(report) -> str:
    return "\n".join(f"{name}: {detail}" for name, ok, detail in report.checks if not ok)


def run_cli(arguments: list[str]) -> tuple[int, str, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        try:
            code = V.main(arguments)
        except SystemExit as exit_code:  # argparse usage errors
            code = int(exit_code.code or 0)
    return code, out.getvalue(), err.getvalue()


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="library-audit-"))

    def __enter__(self) -> Path:
        return self.directory

    def __exit__(self, *_: object) -> None:
        shutil.rmtree(self.directory, ignore_errors=True)


class FrozenPackageTests(unittest.TestCase):
    def test_the_frozen_member_list_covers_every_file(self):
        report = V.Report()
        V.check_frozen(FIXTURE, report)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_tampered_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(FIXTURE, package)
            target = package / "gold.json"
            target.write_text(target.read_text(encoding="utf-8").replace("糖酵解", "糖解"))
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-digest:gold.json", failed(report))

    def test_an_unfrozen_member_is_detected(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(FIXTURE, package)
            (package / "extra.md").write_text("多出来的文件\n", encoding="utf-8")
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-members", failed(report))

    def test_a_missing_member_list_is_reported(self):
        with Sandbox() as directory:
            package = directory / "package"
            shutil.copytree(FIXTURE, package)
            (package / V.FROZEN).unlink()
            report = V.Report()
            V.check_frozen(package, report)
            self.assertIn("frozen-present", failed(report))


class CleanRootTests(unittest.TestCase):
    def test_a_root_and_output_built_from_the_replay_are_clean(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            report = audit(root, output, FIXTURE)
            self.assertGreater(len(report.checks), 200)
            self.assertEqual(failed(report), set(), show(report))

    def test_a_root_alone_is_clean(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            report = audit(root, None, FIXTURE)
            self.assertEqual(failed(report), set(), show(report))

    def test_the_control_directory_holds_exactly_five_entries(self):
        with Sandbox() as directory:
            root = build_root(directory, replay_or_fail())
            entries = sorted(item.name for item in (root / "control").iterdir())
            self.assertEqual(entries, list(V.CONTROL_ENTRIES))

    def test_frozen_note_copies_are_the_bytes_the_index_declares(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            for note in replay["project"]["notes"]:
                body = (root / "control" / "notes" / note["path"]).read_text(encoding="utf-8")
                self.assertEqual(body, replay["bodies"][note["note_id"]])


class LayoutTests(unittest.TestCase):
    def test_an_extra_top_level_entry_is_refused(self):
        with Sandbox() as directory:
            root = build_root(directory, replay_or_fail())
            (root / "notes.md").write_text("杂项\n", encoding="utf-8")
            self.assertIn("layout-root-entries", failed(audit(root)))

    def test_an_extra_control_entry_is_refused(self):
        with Sandbox() as directory:
            root = build_root(directory, replay_or_fail())
            (root / "control" / "notes.bak").write_text("杂项\n", encoding="utf-8")
            self.assertIn("layout-control-entries", failed(audit(root)))

    def test_a_symlink_inside_the_root_is_refused(self):
        with Sandbox() as directory:
            root = build_root(directory, replay_or_fail())
            create_symlink_or_skip(root / "control" / "link.md", root / "control" / "project.json")
            self.assertIn("layout-symlinks", failed(audit(root)))

    def test_a_missing_control_directory_is_refused(self):
        with Sandbox() as directory:
            root = directory / "root"
            root.mkdir()
            self.assertIn("layout-control", failed(audit(root)))


class ManifestTests(unittest.TestCase):
    def _root(self, directory):
        replay = replay_or_fail()
        return replay, build_root(directory, replay)

    def test_an_unknown_manifest_field_is_refused(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            path = root / "control" / "manifest.json"
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["extra"] = 1
            path.write_text(V.canonical(manifest) + "\n", encoding="utf-8")
            report = audit(root, None, FIXTURE)
            self.assertIn("manifest-strict", failed(report))

    def test_a_pretty_printed_manifest_is_refused(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            path = root / "control" / "manifest.json"
            manifest = json.loads(path.read_text(encoding="utf-8"))
            path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
            self.assertIn("manifest-strict", failed(audit(root, None, FIXTURE)))

    def test_a_moved_root_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            path = root / "control" / "manifest.json"
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["root"] = "/tmp/elsewhere"
            path.write_text(V.canonical(manifest) + "\n", encoding="utf-8")
            self.assertIn("manifest-root", failed(audit(root, None, FIXTURE)))

    def test_a_project_digest_drift_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            (root / "control" / "project.json").write_text(
                (root / "control" / "project.json").read_text(encoding="utf-8") + " ", encoding="utf-8"
            )
            self.assertIn("manifest-digest:project_sha256", failed(audit(root, None, FIXTURE)))

    def test_a_gold_digest_drift_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            target = root / "control" / "gold.json"
            target.write_text(target.read_text(encoding="utf-8") + " ", encoding="utf-8")
            self.assertIn("manifest-digest:gold_sha256", failed(audit(root, None, FIXTURE)))

    def test_an_inputs_digest_drift_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            target = root / "control" / "inputs.json"
            target.write_text(target.read_text(encoding="utf-8") + " ", encoding="utf-8")
            self.assertIn("manifest-digest:inputs_sha256", failed(audit(root, None, FIXTURE)))

    def test_a_missing_frozen_note_copy_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            (root / "control" / "notes" / replay["project"]["notes"][0]["path"]).unlink()
            self.assertIn("inputs-note-copies", failed(audit(root, None, FIXTURE)))

    def test_a_tampered_frozen_note_copy_is_detected(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            target = root / "control" / "notes" / replay["project"]["notes"][0]["path"]
            target.write_text(target.read_text(encoding="utf-8") + "\n补一句。\n", encoding="utf-8")
            self.assertIn("inputs-notes", failed(audit(root, None, FIXTURE)))

    def test_identity_records_must_keep_their_fields(self):
        with Sandbox() as directory:
            replay, root = self._root(directory)
            path = root / "control" / "manifest.json"
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest["root_identity"] = {"platform": "linux"}
            path.write_text(V.canonical(manifest) + "\n", encoding="utf-8")
            self.assertIn("manifest-identity:root_identity", failed(audit(root, None, FIXTURE)))


class InputsTests(unittest.TestCase):
    def test_a_case_id_drift_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            path = root / "control" / "inputs.json"
            inputs = json.loads(path.read_text(encoding="utf-8"))
            inputs["case_id"] = "other-case"
            path.write_text(V.canonical(inputs), encoding="utf-8")
            self.assertIn("inputs-case", failed(audit(root, None, FIXTURE)))

    def test_a_dropped_note_entry_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            path = root / "control" / "inputs.json"
            inputs = json.loads(path.read_text(encoding="utf-8"))
            inputs["notes"] = inputs["notes"][:-1]
            path.write_text(V.canonical(inputs), encoding="utf-8")
            self.assertIn("inputs-notes", failed(audit(root, None, FIXTURE)))

    def test_an_extra_note_copy_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            (root / "control" / "notes" / "笔记" / "多余.md").write_text("多余\n", encoding="utf-8")
            self.assertIn("inputs-note-copies", failed(audit(root, None, FIXTURE)))


class ExpectedTests(unittest.TestCase):
    def setUp(self) -> None:
        self.replay = replay_or_fail()
        self.prior = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))

    def test_the_prior_matches_the_replay(self):
        report = V.Report()
        V.check_expected(self.replay, self.prior, report)
        self.assertEqual(failed(report), set(), show(report))

    def test_every_prior_count_is_checked(self):
        for key in V.COUNT_FIELDS:
            with self.subTest(key=key):
                prior = copy.deepcopy(self.prior)
                prior["counts"][key] = prior["counts"][key] + 1
                report = V.Report()
                V.check_expected(self.replay, prior, report)
                self.assertIn(f"expected-count:{key}", failed(report))

    def test_a_drifted_link_field_is_detected(self):
        prior = copy.deepcopy(self.prior)
        prior["links"][0]["basis"] = "bare_name"
        report = V.Report()
        V.check_expected(self.replay, prior, report)
        self.assertIn("expected-links", failed(report))

    def test_a_drifted_explanation_is_detected(self):
        prior = copy.deepcopy(self.prior)
        prior["explanations"][0]["canonical_heading"] = "别的标题"
        report = V.Report()
        V.check_expected(self.replay, prior, report)
        self.assertIn("expected-explanations", failed(report))

    def test_a_drifted_atom_verdict_is_detected(self):
        prior = copy.deepcopy(self.prior)
        prior["atoms"][0]["recalled"] = False
        report = V.Report()
        V.check_expected(self.replay, prior, report)
        self.assertIn("expected-atoms", failed(report))

    def test_a_drifted_ratio_is_detected(self):
        prior = copy.deepcopy(self.prior)
        prior["availability"]["display"] = "99.9%"
        report = V.Report()
        V.check_expected(self.replay, prior, report)
        self.assertIn("expected-ratio:availability:display", failed(report))


class ReplayRuleTests(unittest.TestCase):
    def project(self, notes: dict[str, str]) -> dict:
        return {
            "policy": V.POLICY,
            "schema_version": 1,
            "producer": V.AUTHOR_PRODUCER,
            "case_id": "case",
            "description": "测试用工程声明",
            "gold": "gold.json",
            "notes": [
                {"note_id": note_id, "path": f"笔记/{note_id}.md", "title": note_id}
                for note_id in notes
            ],
            "limits": {"max_notes": 8, "max_atoms": 32, "max_links": 64},
        }

    def gold(self, concepts: list[dict], atoms: list[dict]) -> dict:
        return {
            "policy": V.GOLD_POLICY,
            "schema_version": 1,
            "producer": V.AUTHOR_PRODUCER,
            "case_id": "case",
            "description": "测试用金标准",
            "concepts": concepts,
            "atoms": atoms,
        }

    def test_heading_level_requires_a_space_after_the_hashes(self):
        self.assertEqual(V.heading_level("#没有空格"), 0)
        self.assertEqual(V.heading_level("# 有空格"), 1)
        self.assertEqual(V.heading_level("    ## 缩进过深"), 0)

    def test_a_section_owns_every_line_until_the_next_heading(self):
        found = V.sections("# 甲\n\n正文\n\n## 乙\n正文\n")
        self.assertEqual([(item["line"], item["end_line"]) for item in found], [(1, 4), (5, 6)])

    def test_a_marker_needs_the_explicit_prefix(self):
        body = "# 甲\n\n> 完整解释：concept-a\n\n这一段只是散文。\n"
        found = V.markers(body, {"concept-a"})
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["heading"], "甲")
        self.assertEqual(V.markers("# 甲\n\n这段文字提到完整解释但没有标记。\n", {"concept-a"}), [])

    def test_an_unknown_marker_concept_is_refused(self):
        with self.assertRaises(ValueError):
            V.markers("# 甲\n\n> 完整解释：concept-unknown\n", {"concept-a"})

    def test_a_double_bracket_link_keeps_its_target(self):
        found = V.scan_links("见 [[甲#乙|说明]] 与 ![[丙]]。\n")
        self.assertEqual(found[0]["target"], "甲")
        self.assertEqual(found[0]["heading"], "乙")
        self.assertEqual(found[0]["alias"], "说明")
        self.assertFalse(found[0]["embed"])
        self.assertEqual(found[1]["target"], "丙")
        self.assertTrue(found[1]["embed"])

    def test_an_escaped_alias_in_a_table_is_recorded(self):
        found = V.scan_links("| 行 | [[甲#乙\\|说明]] |\n")
        self.assertTrue(found[0]["in_table"])
        self.assertTrue(found[0]["escaped_alias"])
        self.assertEqual(found[0]["alias"], "说明")

    def test_an_unescaped_alias_in_a_table_is_refused(self):
        project = self.project({"甲": "# 甲\n\n> 完整解释：concept-a\n"})
        gold = self.gold(
            [{"concept_id": "concept-a", "canonical": "甲"}],
            [{"atom_id": "atom-1", "concept_id": "concept-a", "key": True, "note_id": "甲", "heading": "甲"}],
        )
        body = "# 甲\n\n> 完整解释：concept-a\n\n| 行 | [[甲#甲|说明]] |\n"
        with self.assertRaises(ValueError):
            V.evaluate(project, gold, {"甲": body})

    def test_a_bare_name_link_resolves_by_unique_file_name(self):
        project = self.project({"n1": "# 乙\n", "n2": "# 丙\n"})
        project["notes"][0]["path"] = "笔记/乙.md"
        project["notes"][1]["path"] = "笔记/丙.md"
        note, basis = V.resolve_note("乙", project)
        self.assertIsNotNone(note)
        self.assertEqual(note["note_id"], "n1")
        self.assertEqual(basis, V.BASIS_BARE)

    def test_a_duplicate_file_name_is_ambiguous(self):
        project = self.project({"n1": "# 甲\n", "n2": "# 甲二\n"})
        project["notes"][0]["path"] = "笔记/甲.md"
        project["notes"][1]["path"] = "别处/甲.md"
        note, basis = V.resolve_note("甲", project)
        self.assertIsNone(note)
        self.assertEqual(basis, V.BASIS_DUP_NOTE)

    def test_a_duplicate_heading_is_ambiguous_not_first_match(self):
        project = self.project({"甲": "# 甲\n"})
        body = {"甲": "# 甲\n\n## 乙\n\n## 乙\n"}
        ok, basis, _ = V.resolve_heading("甲", "乙", body)
        self.assertFalse(ok)
        self.assertEqual(basis, V.BASIS_DUP_HEADING)

    def test_a_missing_heading_is_broken(self):
        project = self.project({"甲": "# 甲\n"})
        ok, basis, _ = V.resolve_heading("甲", "不存在", {"甲": "# 甲\n"})
        self.assertFalse(ok)
        self.assertEqual(basis, V.BASIS_HEADING_MISSING)

    def test_two_markers_are_reported_as_duplicates(self):
        project = self.project({"甲": "# 甲\n"})
        gold = self.gold([{"concept_id": "concept-a", "canonical": "甲"}], [])
        body = "# 甲\n\n> 完整解释：concept-a\n\n> 完整解释：concept-a\n"
        audit = V.evaluate(project, gold, {"甲": body})
        self.assertEqual(audit["counts"]["duplicate_concepts"], 1)
        self.assertEqual(audit["verdict"], "findings")

    def test_a_marker_outside_the_declared_atom_position_is_a_finding(self):
        project = self.project({"甲": "# 甲\n\n## 乙\n\n## 丙\n"})
        gold = self.gold(
            [{"concept_id": "concept-a", "canonical": "甲"}],
            [{"atom_id": "atom-1", "concept_id": "concept-a", "key": True, "note_id": "甲", "heading": "乙"}],
        )
        body = "# 甲\n\n## 乙\n\n## 丙\n\n> 完整解释：concept-a\n"
        audit = V.evaluate(project, gold, {"甲": body})
        kinds = {finding["kind"] for finding in audit["findings"]}
        self.assertIn(V.FINDING_CANONICAL, kinds)

    def test_a_general_atom_shortfall_below_the_floor_is_a_finding(self):
        project = self.project({"甲": "# 甲\n\n## 乙\n"})
        gold = self.gold(
            [{"concept_id": "concept-a", "canonical": "甲"}],
            [{"atom_id": "atom-1", "concept_id": "concept-a", "key": False, "note_id": "甲", "heading": "不存在"}],
        )
        body = "# 甲\n\n## 乙\n\n> 完整解释：concept-a\n"
        audit = V.evaluate(project, gold, {"甲": body})
        self.assertLessEqual(audit["general_recall"]["numerator"], 0)
        self.assertEqual(audit["verdict"], "findings")


class VariantTests(unittest.TestCase):
    def setUp(self) -> None:
        self.prior = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))

    def test_the_three_variants_match_their_priors(self):
        report = V.Report()
        V.check_variants(FIXTURE, self.prior, report)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_broken_variant_reports_a_broken_link(self):
        replay, code, detail = V.replay_package(FIXTURE / "variants" / "broken")
        self.assertIsNotNone(replay, f"{code} {detail}")
        kinds = {finding["kind"] for finding in replay["audit"]["findings"]}
        self.assertIn(V.FINDING_BROKEN, kinds)

    def test_a_duplicate_variant_reports_a_duplicate_explanation(self):
        replay, _, _ = V.replay_package(FIXTURE / "variants" / "duplicate")
        kinds = {finding["kind"] for finding in replay["audit"]["findings"]}
        self.assertIn(V.FINDING_DUPLICATE, kinds)

    def test_a_recall_variant_reports_an_unexplained_concept(self):
        replay, _, _ = V.replay_package(FIXTURE / "variants" / "recall")
        kinds = {finding["kind"] for finding in replay["audit"]["findings"]}
        self.assertIn(V.FINDING_UNEXPLAINED, kinds)

    def test_a_drifted_variant_prior_is_detected(self):
        prior = copy.deepcopy(self.prior)
        prior["variants"]["broken"]["counts"]["links_broken"] = 0
        report = V.Report()
        V.check_variants(FIXTURE, prior, report)
        self.assertIn("variant-count:broken:links_broken", failed(report))


class OutputTests(unittest.TestCase):
    def test_a_clean_output_is_accepted(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            report = audit(root, output, FIXTURE)
            self.assertEqual(failed(report), set(), show(report))

    def test_an_extra_output_entry_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            (output / "extra.json").write_text("{}\n", encoding="utf-8")
            self.assertIn("output-entries", failed(audit(root, output, FIXTURE)))

    def test_a_missing_output_file_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            (output / "report.md").unlink()
            self.assertIn("output-present:report.md", failed(audit(root, output, FIXTURE)))

    def test_a_drifted_link_record_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "links.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["links"][0]["basis"] = "bare_name"
            write_canonical_json(path, value)
            self.assertIn("links-entries", failed(audit(root, output, FIXTURE)))

    def test_a_drifted_report_count_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "report.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["counts"]["links"] = value["counts"]["links"] + 1
            write_canonical_json(path, value)
            self.assertIn("report-counts", failed(audit(root, output, FIXTURE)))

    def test_a_nonzero_model_call_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "report.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["model_calls"] = 1
            write_canonical_json(path, value)
            self.assertIn("report-zero-calls", failed(audit(root, output, FIXTURE)))

    def test_a_drifted_output_manifest_digest_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["links_sha256"] = "0" * 64
            write_canonical_json(path, value)
            self.assertIn("output-manifest-digest:links_sha256", failed(audit(root, output, FIXTURE)))

    def test_a_timestamped_output_manifest_is_refused(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["created_at"] = "2026-11-08T00:00:00Z"
            write_canonical_json(path, value)
            self.assertIn("output-manifest-strict", failed(audit(root, output, FIXTURE)))

    def test_a_missing_markdown_section_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "report.md"
            path.write_text(path.read_text(encoding="utf-8").replace("## 缺口", "## 其他"), encoding="utf-8")
            self.assertIn("markdown-section:## 缺口", failed(audit(root, output, FIXTURE)))

    def test_a_missing_boundary_statement_is_detected(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            path = output / "report.md"
            path.write_text(path.read_text(encoding="utf-8").replace("不判断内容质量", "质量已判定"),
                             encoding="utf-8")
            self.assertIn("markdown-boundary", failed(audit(root, output, FIXTURE)))


class CommandLineTests(unittest.TestCase):
    def test_package_mode_passes(self):
        code, out, err = run_cli(["--package", str(FIXTURE)])
        self.assertEqual((code, err), (0, ""), out)
        self.assertIn("失败 0", out)

    def test_root_and_output_pass(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            output = build_output(directory, replay)
            code, out, _ = run_cli([str(root), "--output", str(output), "--package", str(FIXTURE)])
            self.assertEqual(code, 0, out)

    def test_without_arguments_it_is_a_usage_error(self):
        code, _, err = run_cli([])
        self.assertEqual(code, 2)
        self.assertIn("至少要给一个审计根", err)

    def test_a_tampered_root_fails_with_one(self):
        with Sandbox() as directory:
            replay = replay_or_fail()
            root = build_root(directory, replay)
            path = root / "control" / "manifest.json"
            value = json.loads(path.read_text(encoding="utf-8"))
            value["policy"] = "other-policy"
            path.write_text(V.canonical(value) + "\n", encoding="utf-8")
            code, out, _ = run_cli([str(root), "--package", str(FIXTURE)])
            self.assertEqual(code, 1)
            self.assertIn("manifest-policy", out)

    def test_freeze_without_a_package_is_a_usage_error(self):
        code, _, err = run_cli(["--freeze"])
        self.assertEqual(code, 2)
        self.assertIn("--freeze 需要 --package", err)

    def test_json_mode_reports_the_failure_list(self):
        with Sandbox() as directory:
            root = directory / "root"
            root.mkdir()
            code, out, _ = run_cli([str(root), "--json"])
            self.assertEqual(code, 1)
            payload = json.loads(out)
            self.assertIn("layout-control", payload["failures"])


if __name__ == "__main__":
    unittest.main()
