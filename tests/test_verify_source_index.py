"""Tests for the independent portable-source-index auditor (B11-T02).

The auditor never imports the product and never runs the CLI to decide anything.
These tests build an index root and an output root with the auditor's own
replay, so a green run proves the checks read the documented layout instead of
mirroring whatever the product happens to write.
"""

from __future__ import annotations

import copy
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
SCRIPT = ROOT / "scripts" / "verify_source_index.py"
PACKAGE = ROOT / "fixtures" / "source-index-v1"
FIXED_AT = "2026-11-08T00:00:00Z"


def load_auditor(module_name: str = "verify_source_index"):
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


def replay_or_fail(package: Path = PACKAGE, project_name: str = V.PROJECT) -> dict:
    replay, code, detail = V.replay_package(package, project_name)
    assert replay is not None, f"{code}：{detail}"
    return replay


def build_root(directory: Path, package: Path = PACKAGE) -> tuple[Path, dict]:
    """Build an index root from the auditor's replay of the frozen package."""
    replay = replay_or_fail(package)
    project = replay["project"]
    root = directory / "root"
    control = root / "control"
    copies = control / "files"
    copies.mkdir(parents=True)

    (control / V.PROJECT).write_bytes((package / V.PROJECT).read_bytes())
    write_json(control / V.INPUTS, replay["inputs"], trailing_newline=False)

    for item in replay["index"]["files"]:
        target = copies / item["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((package / item["path"]).read_bytes())

    manifest = {
        "policy": V.POLICY,
        "producer": V.APP_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": project["case_id"],
        "root": str(root.resolve()),
        "root_identity": fake_identity(1),
        "parent_identity": fake_identity(2),
        "control_identity": fake_identity(3),
        "manifest_identity": fake_identity(4),
        "files_identity": fake_identity(5),
        "inputs_identity": fake_identity(6),
        "project_sha256": V.sha256((control / V.PROJECT).read_bytes()),
        "inputs_sha256": V.sha256((control / V.INPUTS).read_bytes()),
        "created_at": FIXED_AT,
    }
    write_json(control / V.MANIFEST, manifest, trailing_newline=False)
    return root, replay


def report_markdown(replay: dict) -> str:
    project = replay["project"]
    counts = replay["counts"]
    lines = [
        f"# 来源索引（{replay['index']['case_id']}）",
        "",
        project["description"],
        "",
        "## 区域",
        "",
        f"- 输出区：`{project['output_area']}`（复制出去的东西只有这些）",
        f"- 运行库：`{project['runtime_area']}`（数据库与缓存，不随输出复制）",
        f"- 临时区：`{project['temp_area']}`",
        f"- 重定位目录：`{project['relocate_area']}`",
        "",
        "## 计数",
        "",
        f"- 笔记 {counts['notes']} 份、资源 {counts['resources']} 份、来源 {counts['sources']} 条、标题 {counts['headings']} 个",
        f"- 输出区文件 {counts['output_files']} 个；未纳入的运行库条目 "
        f"{counts['excluded_runtime_entries']} 个、临时区条目 {counts['excluded_temp_entries']} 个",
        "",
        "## 文件清单",
        "",
        "| 文件 | 角色 | 摘要 | 字节 |",
        "| --- | --- | --- | --- |",
    ]
    for item in replay["index"]["files"]:
        lines.append(
            f"| `{item['path']}` | {item['role']} | {V.digest_short(item['sha256'])} | "
            f"{item['byte_length']} |"
        )
    lines += [
        "",
        "## 来源映射",
        "",
        "| 来源 | 笔记 | 章节 | 描述 | 定位 | 摘要 |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for item in replay["index"]["sources"]:
        lines.append(
            f"| {item['source_id']} | {item['note_id']} | {item['heading']} | "
            f"{item['description']} | {item['locator']} | {item['digest_short']} |"
        )
    lines += ["", f"## 重定位结果", "", f"在 `{replay['relocation']['directory']}` 里核对：", ""]
    for item in replay["relocation"]["sources"]:
        lines.append(f"- {item['source_id']}：{item['status']}。{item['detail']}")
    lines += [
        "",
        "## 边界",
        "",
        "- 正文保持干净：索引不插入文献编号、脚注或出处列表，人工新增文字也不会自动获得来源。",
        "- 来源按内容摘要识别，不按文件名：同名但内容不同不算同一份来源，缺失时只提示需要重建映射，不指向任何页面。",
        "- 运行库、缓存、临时文件与凭据不进入输出区与索引；模型调用与网络请求 0 次，不判断内容质量与科学正确性。",
        "",
    ]
    return "\n".join(lines)


def build_output(directory: Path, replay: dict) -> Path:
    """Write the output root a correct build produces."""
    root = directory / "root"
    out = directory / "out"
    out.mkdir(parents=True)

    write_json(out / V.INDEX, replay["index"])
    write_json(out / V.RELOCATION, replay["relocation"])
    info = {
        "policy": V.POLICY,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["index"]["case_id"],
        "description": replay["project"]["description"],
        "areas": {
            "output": replay["project"]["output_area"],
            "runtime": replay["project"]["runtime_area"],
            "temp": replay["project"]["temp_area"],
            "relocate": replay["project"]["relocate_area"],
        },
        "counts": replay["counts"],
        "model_calls": 0,
        "network_requests": 0,
    }
    write_json(out / V.REPORT, info)
    (out / V.MARKDOWN).write_bytes(report_markdown(replay).encode("utf-8"))
    manifest = {
        "policy": V.POLICY,
        "producer": V.OUTPUT_PRODUCER,
        "schema_version": V.SCHEMA_VERSION,
        "case_id": replay["index"]["case_id"],
        "source_root": str(root.resolve()),
        "source_inputs_sha256": V.sha256((root / V.CONTROL / V.INPUTS).read_bytes()),
        "index_sha256": V.sha256((out / V.INDEX).read_bytes()),
        "relocation_sha256": V.sha256((out / V.RELOCATION).read_bytes()),
        "report_sha256": V.sha256((out / V.REPORT).read_bytes()),
        "markdown_sha256": V.sha256((out / V.MARKDOWN).read_bytes()),
        "counts": replay["counts"],
    }
    write_json(out / V.OUTPUT_MANIFEST, manifest)
    return out


def audit(root: Path, output: Path | None = None, package: Path = PACKAGE):
    report = V.Report()
    V.check_layout(report, root)
    manifest = V.check_manifest(report, root)
    project = V.check_project(report, root)
    replay = replay_or_fail(package)
    if manifest is not None and project is not None:
        replay["root"] = str(root.resolve())
        V.check_inputs(report, root, replay)
    V.check_expected(report, package, replay)
    V.check_rejections(report, package)
    V.check_frozen(report, package)
    if output is not None:
        V.check_output(report, output, replay)
    return report


def failed(report) -> set[str]:
    return {item["name"] for item in report.checks if not item["ok"]}


def show(report) -> str:
    """Render only the failing checks, so a green run stays silent."""
    return "\n".join(
        f"{item['name']}：{item['detail']}" for item in report.checks if not item["ok"]
    )


class Sandbox:
    def __init__(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="source-index-audit-"))

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
        self.output = build_output(self.sandbox.directory, self.replay)

    def rewrite(self, path: Path, value: object) -> None:
        write_json(path, value)


class FrozenPackageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sandbox = Sandbox()
        self.addCleanup(self.sandbox.close)

    def test_the_frozen_manifest_covers_every_member_of_the_package(self) -> None:
        frozen = json.loads((PACKAGE / V.FROZEN).read_text(encoding="utf-8"))
        members = {
            item.relative_to(PACKAGE).as_posix()
            for item in PACKAGE.rglob("*")
            if item.is_file() and item.name != V.FROZEN
        }
        self.assertEqual(set(frozen["files"]), members)
        self.assertEqual(frozen["workflow_id"], V.POLICY)
        self.assertEqual(frozen["case_count"], len(members))

    def test_a_tampered_member_is_detected(self) -> None:
        copy = self.sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "vault" / "笔记" / "糖酵解.md").write_bytes(b"# tampered\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-digest:vault/笔记/糖酵解.md", failed(report))

    def test_an_extra_member_is_detected(self) -> None:
        copy = self.sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        (copy / "extra.md").write_bytes(b"# extra\n")
        report = V.Report()
        V.check_frozen(report, copy)
        self.assertIn("frozen-members", failed(report))

    def test_refreezing_a_copy_writes_identical_bytes(self) -> None:
        copy = self.sandbox.path("package")
        shutil.copytree(PACKAGE, copy)
        self.assertEqual(V.freeze(copy), 0)
        self.assertEqual(
            (copy / V.FROZEN).read_bytes(), (PACKAGE / V.FROZEN).read_bytes()
        )


class CleanRootTests(SandboxCase):
    def test_the_built_root_and_output_pass_every_check(self) -> None:
        report = audit(self.root, self.output)
        self.assertEqual(failed(report), set(), show(report))
        self.assertGreater(len(report.checks), 100)

    def test_the_root_alone_passes_every_check(self) -> None:
        report = audit(self.root)
        self.assertEqual(failed(report), set(), show(report))

    def test_the_root_holds_exactly_the_declared_control_entries(self) -> None:
        names = sorted(item.name for item in (self.root / V.CONTROL).iterdir())
        self.assertEqual(names, sorted(V.CONTROL_ENTRIES))
        for entry in self.replay["inputs"]["runtime_entries"]:
            self.assertTrue(entry["path"].startswith("runtime/"), entry["path"])
        for entry in self.replay["inputs"]["temp_entries"]:
            self.assertTrue(entry["path"].startswith("temp/"), entry["path"])

    def test_the_runtime_and_temp_entries_are_recorded_but_not_copied(self) -> None:
        copied = {
            item.relative_to(self.root / V.CONTROL / V.FILES).as_posix()
            for item in (self.root / V.CONTROL / V.FILES).rglob("*")
            if item.is_file()
        }
        self.assertEqual(len(copied), self.replay["counts"]["output_files"])
        for entry in self.replay["inputs"]["runtime_entries"]:
            self.assertNotIn(entry["path"], copied)
        for entry in self.replay["inputs"]["temp_entries"]:
            self.assertNotIn(entry["path"], copied)


class LayoutTests(SandboxCase):
    def test_an_extra_top_level_entry_is_refused(self) -> None:
        (self.root / "note.txt").write_bytes(b"hello\n")
        report = audit(self.root)
        self.assertIn("layout-root-entries", failed(report))

    def test_an_extra_control_entry_is_refused(self) -> None:
        (self.root / V.CONTROL / "extra.json").write_bytes(b"{}\n")
        report = audit(self.root)
        self.assertIn("layout-control-entries", failed(report))

    def test_a_symlink_inside_the_root_is_refused(self) -> None:
        create_symlink_or_skip(self.root / V.CONTROL / "link", self.root / V.CONTROL / V.PROJECT)
        report = audit(self.root)
        self.assertIn("layout-no-symlinks", failed(report))


class ManifestTests(SandboxCase):
    def test_an_unknown_field_is_refused(self) -> None:
        path = self.root / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["extra"] = 1
        self.rewrite(path, value)
        report = audit(self.root)
        self.assertIn("manifest-strict", failed(report))

    def test_a_pretty_printed_manifest_is_refused(self) -> None:
        path = self.root / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        path.write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
        report = audit(self.root)
        self.assertIn("manifest-strict", failed(report))

    def test_a_drifted_root_path_is_refused(self) -> None:
        path = self.root / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["root"] = str(self.sandbox.path("elsewhere"))
        self.rewrite(path, value)
        report = audit(self.root)
        self.assertIn("manifest-root", failed(report))

    def test_a_drifted_project_digest_is_refused(self) -> None:
        path = self.root / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["project_sha256"] = "0" * 64
        self.rewrite(path, value)
        report = audit(self.root)
        self.assertIn("manifest-project-digest", failed(report))

    def test_a_drifted_inputs_digest_is_refused(self) -> None:
        path = self.root / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["inputs_sha256"] = "0" * 64
        self.rewrite(path, value)
        report = audit(self.root)
        self.assertIn("manifest-inputs-digest", failed(report))

    def test_a_missing_file_copy_is_refused(self) -> None:
        target = self.root / V.CONTROL / V.FILES / "vault" / "笔记" / "糖酵解.md"
        target.unlink()
        report = audit(self.root)
        self.assertIn("inputs-file-copies", failed(report))

    def test_a_tampered_file_copy_is_refused(self) -> None:
        target = self.root / V.CONTROL / V.FILES / "vault" / "资源" / "术语表.md"
        target.write_bytes(b"# tampered\n")
        report = audit(self.root)
        self.assertIn("inputs-file-copies", failed(report))


class InputsTests(SandboxCase):
    def rewrite_inputs(self, mutate) -> None:
        path = self.root / V.CONTROL / V.INPUTS
        value = json.loads(path.read_text(encoding="utf-8"))
        mutate(value)
        self.rewrite(path, value)
        manifest_path = self.root / V.CONTROL / V.MANIFEST
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["inputs_sha256"] = V.sha256(path.read_bytes())
        self.rewrite(manifest_path, manifest)

    def test_a_drifted_case_id_is_refused(self) -> None:
        self.rewrite_inputs(lambda value: value.__setitem__("case_id", "other-case"))
        report = audit(self.root)
        self.assertIn("inputs-case", failed(report))

    def test_a_dropped_file_entry_is_refused(self) -> None:
        self.rewrite_inputs(lambda value: value["files"].pop())
        report = audit(self.root)
        self.assertIn("inputs-files", failed(report))

    def test_a_dropped_runtime_entry_is_refused(self) -> None:
        self.rewrite_inputs(lambda value: value["runtime_entries"].pop())
        report = audit(self.root)
        self.assertIn("inputs-runtime_entries", failed(report))

    def test_a_swapped_area_is_refused(self) -> None:
        self.rewrite_inputs(lambda value: value.__setitem__("temp_area", "scratch"))
        report = audit(self.root)
        self.assertIn("inputs-areas", failed(report))


class ExpectedTests(unittest.TestCase):
    def setUp(self) -> None:
        self.replay = replay_or_fail()

    def test_every_prior_count_matches_the_replay(self) -> None:
        report = V.Report()
        V.check_expected(report, PACKAGE, self.replay)
        self.assertEqual(failed(report), set(), show(report))

    def test_a_drifted_prior_count_is_refused(self) -> None:
        prior = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        report = V.Report()
        V.check_expected(report, PACKAGE, self.replay)
        self.assertEqual(failed(report), set())
        for key in ("notes", "sources", "headings", "relocated", "digest_mismatch"):
            drifted = copy.deepcopy(self.replay)
            drifted["counts"][key] = drifted["counts"][key] + 1
            other = V.Report()
            V.check_expected(other, PACKAGE, drifted)
            self.assertIn(f"expected-count:{key}", failed(other), key)
        self.assertEqual(prior["counts"]["notes"], self.replay["counts"]["notes"])

    def test_the_prior_relocation_belongs_to_the_moved_directory(self) -> None:
        prior = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        self.assertEqual(prior["relocation"]["directory"], "moved")
        self.assertEqual(
            [item["found"] for item in prior["relocation"]["sources"]],
            [item["found"] for item in self.replay["relocation"]["sources"]],
        )

    def test_the_prior_mismatched_list_only_lists_digest_mismatches(self) -> None:
        prior = json.loads((PACKAGE / "expected.json").read_text(encoding="utf-8"))
        for entry in prior["ad_hoc_relocations"]:
            candidates = V.scan_area(PACKAGE, entry["directory"])
            computed = V.relocation_of(
                self.replay["project"]["case_id"],
                entry["directory"],
                self.replay["index"]["sources"],
                candidates,
            )
            mismatched = [
                item["source_id"]
                for item in computed["sources"]
                if item["status"] == V.STATUS_DIGEST_MISMATCH
            ]
            self.assertEqual(sorted(mismatched), sorted(entry["mismatched"]))


class ReplayRuleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.replay = replay_or_fail()

    def test_under_compares_path_components_not_prefixes(self) -> None:
        self.assertTrue(V.under("vault/x.md", "vault"))
        self.assertTrue(V.under("vault", "vault"))
        self.assertFalse(V.under("vault2/x.md", "vault"))

    def test_headings_are_only_markdown_headings_with_a_space(self) -> None:
        text = "# 标题\n##没有空格\n  ### 三级\n#\n"
        self.assertEqual(V.heading_titles(text), ["标题", "三级", ""])

    def test_citation_marks_cover_numbers_and_reference_sections(self) -> None:
        self.assertEqual(V.citation_marks("正文 [1] 与 [12]。\n"), ["[1]", "[12]"])
        self.assertEqual(
            V.citation_marks("## 参考文献\n- 某书\n"), ["「参考文献」小节"]
        )
        self.assertEqual(V.citation_marks("## 引用文献\n"), ["「引用文献」小节"])
        self.assertEqual(V.citation_marks("no marks [a] here\n"), [])

    def test_absolute_paths_and_secret_markers_are_detected(self) -> None:
        self.assertEqual(V.absolute_paths(["vault/a.md"]), [])
        self.assertEqual(V.absolute_paths(["/etc/passwd", "C:/x"]), ["/etc/passwd", "C:/x"])
        self.assertEqual(V.secret_markers(["clean text"]), [])
        self.assertEqual(
            V.secret_markers(["API_KEY=1", "authorization: bearer"]),
            ["api_key", "authorization:"],
        )

    def test_relocation_follows_the_content_digest_not_the_name(self) -> None:
        sources = self.replay["index"]["sources"]
        candidates = V.scan_area(PACKAGE, "moved")
        relocated = V.relocation_of("case", "moved", sources, candidates)
        self.assertEqual((relocated["relocated"], relocated["missing"]), (3, 0))
        self.assertTrue(all(item["found"] for item in relocated["sources"]))

        partial = V.relocation_of(
            "case", "moved-partial", sources, V.scan_area(PACKAGE, "moved-partial")
        )
        self.assertEqual(
            (partial["relocated"], partial["missing"], partial["digest_mismatch"]),
            (2, 0, 1),
        )
        mismatch = next(
            item for item in partial["sources"] if item["status"] == V.STATUS_DIGEST_MISMATCH
        )
        self.assertIsNone(mismatch["found"])
        self.assertIn("同名不能证明是同一份来源", mismatch["detail"])

        foreign = V.relocation_of(
            "case", "moved-foreign", sources, V.scan_area(PACKAGE, "moved-foreign")
        )
        self.assertEqual((foreign["relocated"], foreign["missing"]), (0, 3))
        self.assertIn("需要重建映射", foreign["sources"][0]["detail"])

    def test_evaluate_accepts_the_declared_project(self) -> None:
        project, detail = V.load_project(PACKAGE)
        self.assertIsNotNone(project, detail)
        code, detail, data = V.evaluate(project, PACKAGE)
        self.assertEqual(code, "", detail)
        self.assertEqual(len(data["notes"]), 2)

    def test_the_three_variants_are_refused_with_their_own_codes(self) -> None:
        codes = {}
        for name in ("project-citation.json", "project-leak.json", "project-bad-heading.json"):
            _, code, _ = V.replay_package(PACKAGE, name)
            codes[name] = code
        self.assertEqual(codes["project-citation.json"], "INDEX_CITATION_BURDEN")
        self.assertEqual(codes["project-leak.json"], "INDEX_AREA_LEAK")
        self.assertEqual(codes["project-bad-heading.json"], "INDEX_LOCATION_MISSING")

    def test_every_recorded_count_is_derived_from_the_frozen_sources(self) -> None:
        counts = self.replay["counts"]
        self.assertEqual(counts["notes"], 2)
        self.assertEqual(counts["resources"], 1)
        self.assertEqual(counts["sources"], 3)
        self.assertEqual(counts["headings"], 7)
        self.assertEqual(counts["output_files"], 3)
        self.assertEqual(counts["citation_marks"], 0)
        self.assertEqual(counts["absolute_paths"], 0)
        self.assertEqual(counts["secret_markers"], 0)


class OutputTests(SandboxCase):
    def rewrite_output(self, name: str, mutate) -> None:
        path = self.output / name
        value = json.loads(path.read_text(encoding="utf-8"))
        mutate(value)
        self.rewrite(path, value)
        manifest_path = self.output / V.OUTPUT_MANIFEST
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        key = {
            V.INDEX: "index_sha256",
            V.RELOCATION: "relocation_sha256",
            V.REPORT: "report_sha256",
        }[name]
        manifest[key] = V.sha256(path.read_bytes())
        self.rewrite(manifest_path, manifest)

    def test_an_extra_output_entry_is_refused(self) -> None:
        (self.output / "extra.json").write_bytes(b"{}\n")
        report = audit(self.root, self.output)
        self.assertIn("output-entries", failed(report))

    def test_a_missing_output_entry_is_refused(self) -> None:
        (self.output / V.RELOCATION).unlink()
        report = audit(self.root, self.output)
        self.assertIn("output-present:relocation.json", failed(report))

    def test_a_drifted_index_entry_is_refused(self) -> None:
        self.rewrite_output(
            V.INDEX, lambda value: value["sources"][0].__setitem__("sha256", "0" * 64)
        )
        report = audit(self.root, self.output)
        self.assertIn("index-entries", failed(report))

    def test_a_shortened_digest_is_refused(self) -> None:
        self.rewrite_output(
            V.INDEX, lambda value: value["sources"][0].__setitem__("digest_short", "short")
        )
        report = audit(self.root, self.output)
        self.assertIn("index-source:src-textbook-a:digest-short", failed(report))

    def test_a_drifted_relocation_entry_is_refused(self) -> None:
        self.rewrite_output(
            V.RELOCATION, lambda value: value["sources"][0].__setitem__("found", "moved/x.md")
        )
        report = audit(self.root, self.output)
        self.assertIn("relocation-entries", failed(report))

    def test_a_relocation_status_must_match_its_counts(self) -> None:
        self.rewrite_output(
            V.RELOCATION, lambda value: value.__setitem__("relocated", 99)
        )
        report = audit(self.root, self.output)
        self.assertIn("relocation-entries", failed(report))

    def test_a_drifted_report_count_is_refused(self) -> None:
        self.rewrite_output(V.REPORT, lambda value: value["counts"].__setitem__("notes", 9))
        report = audit(self.root, self.output)
        self.assertIn("report-counts", failed(report))

    def test_a_model_call_is_refused(self) -> None:
        self.rewrite_output(V.REPORT, lambda value: value.__setitem__("model_calls", 1))
        report = audit(self.root, self.output)
        self.assertIn("report-zero-calls", failed(report))

    def test_a_drifted_output_manifest_count_is_refused(self) -> None:
        path = self.output / V.OUTPUT_MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["counts"]["headings"] = 0
        self.rewrite(path, value)
        report = audit(self.root, self.output)
        self.assertIn("output-manifest-counts", failed(report))

    def test_a_drifted_output_manifest_digest_is_refused(self) -> None:
        path = self.output / V.OUTPUT_MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["index_sha256"] = "0" * 64
        self.rewrite(path, value)
        report = audit(self.root, self.output)
        self.assertIn("output-manifest-digests", failed(report))

    def test_an_output_manifest_with_a_timestamp_is_refused(self) -> None:
        path = self.output / V.OUTPUT_MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["created_at"] = FIXED_AT
        self.rewrite(path, value)
        report = audit(self.root, self.output)
        self.assertIn("output-manifest-strict", failed(report))

    def test_a_missing_markdown_section_is_refused(self) -> None:
        path = self.output / V.MARKDOWN
        text = path.read_text(encoding="utf-8").replace("## 来源映射", "## 映射")
        path.write_bytes(text.encode("utf-8"))
        report = audit(self.root, self.output)
        self.assertIn("markdown-section:## 来源映射", failed(report))

    def test_a_missing_boundary_statement_is_refused(self) -> None:
        path = self.output / V.MARKDOWN
        text = path.read_text(encoding="utf-8").replace(
            "不判断内容质量与科学正确性", "（无）"
        )
        path.write_bytes(text.encode("utf-8"))
        report = audit(self.root, self.output)
        self.assertIn("markdown-boundary", failed(report))

    def test_a_missing_source_row_is_refused(self) -> None:
        path = self.output / V.MARKDOWN
        text = path.read_text(encoding="utf-8").replace("src-lecture", "src-other")
        path.write_bytes(text.encode("utf-8"))
        report = audit(self.root, self.output)
        self.assertIn("markdown-sources", failed(report))


class CommandLineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sandbox = Sandbox()
        cls.root, cls.replay = build_root(cls.sandbox.directory)
        cls.output = build_output(cls.sandbox.directory, cls.replay)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.sandbox.close()

    def run_cli(self, *arguments: str):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *arguments],
            capture_output=True,
            text=True,
            encoding="utf-8",
        )

    def test_the_frozen_package_passes(self) -> None:
        done = self.run_cli("--package", str(PACKAGE))
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn("失败 0", done.stdout)

    def test_a_built_root_and_output_pass(self) -> None:
        done = self.run_cli(str(self.root), "--output", str(self.output), "--package", str(PACKAGE))
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn("失败 0", done.stdout)

    def test_no_arguments_is_a_usage_error(self) -> None:
        done = self.run_cli()
        self.assertEqual(done.returncode, 2)
        self.assertIn("至少要给一个索引根", done.stderr)

    def test_a_tampered_root_fails(self) -> None:
        copy = self.sandbox.path("tampered")
        shutil.copytree(self.root, copy)
        path = copy / V.CONTROL / V.MANIFEST
        value = json.loads(path.read_text(encoding="utf-8"))
        value["policy"] = "other-policy"
        write_json(path, value, trailing_newline=False)
        done = self.run_cli(str(copy), "--package", str(PACKAGE))
        self.assertEqual(done.returncode, 1)
        self.assertIn("manifest-policy", done.stdout)

    def test_freeze_without_a_package_is_a_usage_error(self) -> None:
        done = self.run_cli("--freeze")
        self.assertEqual(done.returncode, 2)
        self.assertIn("--freeze 需要 --package", done.stderr)

    def test_json_mode_reports_every_check(self) -> None:
        done = self.run_cli("--package", str(PACKAGE), "--json")
        self.assertEqual(done.returncode, 0, done.stderr)
        payload = json.loads(done.stdout)
        self.assertEqual(payload["failures"], 0)
        self.assertGreater(len(payload["checks"]), 20)


if __name__ == "__main__":
    unittest.main()
