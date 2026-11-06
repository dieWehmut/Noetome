"""Read-only preparation tests; inert runtime fixtures are never executed.

The actual frozen thirteen-case sources use their real independent semantic
checkers. Tiny synthetic ASAR/executable bytes only exercise installation pins;
these tests cannot establish actual Obsidian rendering or model recognition.
"""
from __future__ import annotations

import argparse
import copy
import importlib.util
import io
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("prepare_visual_compatibility", ROOT / "scripts/prepare_visual_compatibility.py")
V = importlib.util.module_from_spec(spec)
spec.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/visual-compatibility"
FROZEN_SHA = "9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c"
EXTENSION = ROOT / "fixtures/visual-compatibility-flow-extensions"
EXTENSION_SHA = "e9f6a8b0e86718a38dba5cbf5073c25ffb0a6e39ca0c1d8c43a3648f4952339e"


def asar(contents):
    tree = {"files": {}}
    payload = bytearray()
    for name, raw in contents.items():
        node = tree
        for part in name.split("/")[:-1]:
            node = node["files"].setdefault(part, {"files": {}})
        node["files"][name.split("/")[-1]] = {
            "size": len(raw), "offset": str(len(payload)),
            "integrity": {"algorithm": "SHA256", "hash": V.pin(raw)["sha256"]},
        }
        payload.extend(raw)
    header = json.dumps(tree).encode()
    return struct.pack("<IIII", 4, 8 + len(header), 4 + len(header), len(header)) + header + payload


class VisualCompatibilityPreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name)
        self.runtime = self.base / "inert-runtime"
        self.runtime.mkdir()
        package = b'{"version":"1.6.7"}'
        self.module_bytes = {"lib/mathjax/inert.js": b"inert authored MathJax fixture, never executed",
                             "lib/mermaid.min.js": b"inert authored Mermaid fixture, never executed",
                             "app.js": b"inert authored app bundle fixture, never executed"}
        (self.runtime / "Obsidian.exe").write_bytes(b"inert authored non-executable fixture")
        (self.runtime / "app.asar").write_bytes(asar({"package.json": package}))
        (self.runtime / "obsidian.asar").write_bytes(asar({"package.json": package, **self.module_bytes}))
        self.record = {"schema_version": 1, "app_version": "1.6.7", "scope": "explicit inert runtime bytes, not native evidence"}
        for key, name in (("executable", "Obsidian.exe"), ("app_asar", "app.asar"), ("obsidian_asar", "obsidian.asar")):
            path = self.runtime / name
            self.record[key] = {"path": str(path), **V.pin(path.read_bytes())}
        for key, name, version in (("mathjax", "lib/mathjax/inert.js", "3.2.2"),
                                   ("mermaid", "lib/mermaid.min.js", None)):
            self.record[key] = {"version": version, "member": {"path": name, **V.pin(self.module_bytes[name])}}
        self.record["app_bundle"] = {"member": {"path": "app.js", **V.pin(self.module_bytes["app.js"])}}
        self.record_path = self.base / "synthetic-runtime-record.json"
        self.write_record()

    def tearDown(self):
        self.temporary.cleanup()

    def write_record(self):
        self.record_path.write_text(json.dumps(self.record) + "\n")
        return V.pin(self.record_path.read_bytes())["sha256"]

    def args(self, case="formula-native", root=None):
        return argparse.Namespace(fixture_root=FIXTURE if root is None else root, fixture_sha256=FROZEN_SHA,
                                  runtime_record=self.record_path, runtime_sha256=V.pin(self.record_path.read_bytes())["sha256"],
                                  case_id=case, theme="light", new_root=self.base / (case + "-prepared"))

    def prepare(self, args):
        # Only the synthetic host-path spelling is substituted. Installation
        # SHA/ASAR decoding, full source freeze and actual checkers still run.
        with mock.patch.object(V, "windows_path", side_effect=lambda p: str(p)), \
                mock.patch("subprocess.Popen", side_effect=AssertionError("preparation cannot launch a process")), \
                mock.patch("subprocess.run", side_effect=AssertionError("preparation cannot run processes")), \
                mock.patch("urllib.request.urlopen", side_effect=AssertionError("preparation cannot access a network")):
            return V.prepare(args)

    def test_all_thirteen_actual_frozen_outputs_are_verified_and_copied_without_execution(self):
        before = V.fixture(FIXTURE, FROZEN_SHA)[1]
        ids = [case["case_id"] for case in V.strict_json(before["cases.json"])["cases"]]
        for case in ids:
            with self.subTest(case=case):
                result = self.prepare(self.args(case))
                self.assertEqual(result["native_capture_status"], "not_run")
                self.assertEqual(result["independent_readback"]["status"], "passed")
                self.assertEqual(result["model_requests"], 0)
                self.assertEqual(result["processes_launched"], 0)
                self.assertFalse(result["published"])
                actual = {name: V.pin(raw) for name, raw in V.files(result["output"]).items()}
                self.assertEqual(actual, result["source_pins"])
        self.assertEqual(V.fixture(FIXTURE, FROZEN_SHA)[1], before)

    def test_original_collector_semantic_checks_are_retained_by_exact_derivation(self):
        markers = {"table": "'native table cells'", "flow": "'TD visual arrow direction'",
                   "formula": "'actual MathJax tree retains signs, scripts, equation and fraction order'",
                   "scan": "'actual MathJax tree retains signs, scripts, equation and fraction order'",
                   "image": "'same-version auxiliary SVG rendering decodes exactly the native local SVG img pixels'"}
        for kind, path in V.COLLECTORS.items():
            original = path.read_bytes()
            derived = V.derived_collector(original, kind).decode()
            self.assertIn(markers[kind], derived)
            self.assertIn("Debugger.getScriptSource", derived)
            self.assertIn("actual selected installation bytes", derived)
            self.assertIn("expected.app_version", derived)
            if kind in ("formula", "image", "scan"):
                self.assertIn("actual click opened the native image view", derived)
                self.assertIn("clicked PNG remains a local selected attachment", derived)
            self.assertEqual(path.read_bytes(), original)

    def test_changed_or_duplicate_derivation_anchor_is_refused(self):
        original = V.COLLECTORS["formula"].read_bytes()
        for raw in (original.replace(b"let owned = false;", b"let owned = true;"),
                    original + b"\nlet owned = false;\n"):
            with self.assertRaisesRegex(ValueError, "exactly once"):
                V.derived_collector(raw, "formula")

    def test_synchronized_frozen_formula_or_expected_change_cannot_replace_original_anchor(self):
        clone = self.base / "modified-frozen-fixture"
        shutil.copytree(FIXTURE, clone)
        target = clone / "selected-output/formula-native/公式预览.md"
        target.write_bytes(target.read_bytes().replace(b"=-", b"="))
        expected = clone / "expected-formula-native.json"
        expected.write_bytes(expected.read_bytes().replace("−".encode(), "+".encode()))
        frozen = clone / "frozen-sha256.json"
        manifest = V.strict_json(frozen.read_bytes())
        for file in (target, expected):
            manifest["files"][file.relative_to(clone).as_posix()] = V.pin(file.read_bytes())
        frozen.write_text(json.dumps(manifest) + "\n")
        args = self.args(root=clone)
        with self.assertRaisesRegex(ValueError, "freeze SHA mismatch"):
            self.prepare(args)
        self.assertFalse(args.new_root.exists())

    def test_runtime_record_cannot_relabel_an_installed_package_version(self):
        self.record["app_version"] = "1.15.0"
        args = self.args()
        args.runtime_sha256 = self.write_record()
        with self.assertRaisesRegex(ValueError, "actual ASAR package version"):
            self.prepare(args)
        self.assertFalse(args.new_root.exists())

    def test_changed_executable_or_module_member_cannot_become_valid_by_plain_metadata(self):
        args = self.args()
        (self.runtime / "Obsidian.exe").write_bytes(b"replacement runtime")
        with self.assertRaisesRegex(ValueError, "runtime bytes changed"):
            self.prepare(args)
        (self.runtime / "Obsidian.exe").write_bytes(b"inert authored non-executable fixture")
        self.record["mermaid"]["member"]["sha256"] = "0" * 64
        args.runtime_sha256 = self.write_record()
        with self.assertRaisesRegex(ValueError, "member bytes changed"):
            self.prepare(args)
        self.assertFalse(args.new_root.exists())

    def test_runtime_record_anchor_and_source_unknown_remain_independent(self):
        args = self.args("scan-sign")
        self.record_path.write_bytes(self.record_path.read_bytes() + b" ")
        with self.assertRaisesRegex(ValueError, "runtime record SHA mismatch"):
            self.prepare(args)
        args.runtime_sha256 = self.write_record()
        result = self.prepare(args)
        spec = V.strict_json(Path(result["capture_spec_path"]).read_bytes())
        self.assertEqual(spec["blocked_absent"], ["扫描公式预览.md", "plan.json", "compiled.tex"])
        self.assertFalse(spec["require_mathjax_runtime"])
        self.assertFalse((Path(result["output"]) / "扫描公式预览.md").exists())
        self.assertEqual(spec["runtime"]["mermaid"]["version"], None)

    def test_table_flow_bridge_never_reuses_old_vault_profile_port_or_runtime_identity(self):
        for case in ("table-escaping-names", "flow-branch"):
            result = self.prepare(self.args(case))
            spec = V.strict_json(Path(result["capture_spec_path"]).read_bytes())
            self.assertEqual(set(spec["bridge"]), {"plan", "expected", "case_name"})
            self.assertNotIn("vault", spec["bridge"])
            self.assertNotIn("profile", spec["bridge"])
            self.assertNotIn("port", spec["bridge"])
            self.assertNotIn("runtime", spec["bridge"])

    def test_authorized_three_flow_extension_has_its_own_anchor_and_actual_checkers(self):
        for case in ("flow-complex-labels", "flow-promotes", "flow-inhibits"):
            args = self.args(case, EXTENSION)
            args.fixture_sha256 = EXTENSION_SHA
            result = self.prepare(args)
            self.assertEqual(result["independent_readback"]["status"], "passed")
            self.assertEqual(result["fixture_sha256"], EXTENSION_SHA)
            self.assertEqual(result["native_capture_status"], "not_run")
        args = self.args("formula-native", EXTENSION)
        args.fixture_sha256 = EXTENSION_SHA
        with self.assertRaisesRegex(ValueError, "unknown selected"):
            self.prepare(args)

    def test_bridge_semantics_are_checked_against_authors_before_derivation(self):
        clone = self.base / "altered-bridge"
        shutil.copytree(FIXTURE, clone)
        _, originals = V.fixture(clone, FROZEN_SHA)
        case = next(c for c in V.strict_json(originals["cases.json"])["cases"] if c["case_id"] == "table-escaping-names")
        expected = V.strict_json(originals[case["expected_file"]])
        source = {name: originals[ref["captured_file"]] for name, ref in case["representative_output"]["files"].items()}
        bridge_path = clone / case["collector_input_file"]
        bridge = V.strict_json(bridge_path.read_bytes())
        bridge["expected"]["cells"][0]["facts"][0]["conditions"] = []
        bridge_path.write_text(json.dumps(bridge))
        with self.assertRaisesRegex(ValueError, "independent author semantics: cells"):
            V.verify_case(clone, case, expected, source)

    def test_declared_mermaid_version_without_global_still_requires_original_source_literal(self):
        self.module_bytes["lib/mermaid.min.js"] = b"// Source: https://unpkg.com/mermaid@10.6.1/dist/mermaid.min.js\r\ninert, never executed"
        archive = self.runtime / "obsidian.asar"
        archive.write_bytes(asar({"package.json": b'{"version":"1.6.7"}', **self.module_bytes}))
        self.record["obsidian_asar"].update(V.pin(archive.read_bytes()))
        self.record["mermaid"].update({"version": "10.6.1", "engine": "mermaid", "runtime_exposed_version": None})
        self.record["mermaid"]["member"].update(V.pin(self.module_bytes["lib/mermaid.min.js"]))
        args = self.args("flow-branch")
        args.runtime_sha256 = self.write_record()
        result = self.prepare(args)
        spec = V.strict_json(Path(result["capture_spec_path"]).read_bytes())
        self.assertEqual(spec["runtime"]["mermaid"]["version_binding"]["version"], "10.6.1")
        self.assertIsNone(spec["runtime"]["mermaid"]["version_binding"]["runtime_exposed_version"])
        self.record["mermaid"]["version"] = "99.0.0"
        args = self.args("flow-multiline-labels")
        args.runtime_sha256 = self.write_record()
        with self.assertRaisesRegex(ValueError, "installed source literal"):
            self.prepare(args)

    def test_actual_pele_geometry_check_rejects_signed_text_reversal_and_detached_arrows(self):
        node = shutil.which("node")
        if node is None:
            self.skipTest("Node is required to execute the native collector geometry checker")
        visible = {"text": "A", "rect": {"width": 10, "height": 10}, "bbox": {"width": 10, "height": 10},
                   "style": {"display": "inline", "visibility": "visible", "opacity": "1"}}
        def shape(top):
            return {"tag": "rect", "outline": [{"x": 0, "y": top}, {"x": 10, "y": top},
                                                  {"x": 10, "y": top+10}, {"x": 0, "y": top+10}]}
        actual = {"nodes": [], "edges": [{"dataId": "L_n0_n1_0", "lines": [{"attributes": {"d": "M5,10L5,43"},
            "length": 33, "start": {"x": 5, "y": 10}, "end": {"x": 5, "y": 43}, "nearEnd": {"x": 5, "y": 41}}],
            "markers": [{"attributes": {"d": "M5,50L2,42L8,42Z"}, "points": [{"x": 5, "y": 50}, {"x": 2, "y": 42}, {"x": 8, "y": 42}]}]}],
            "edgeLabels": [{"dataId": "L_n0_n1_0", "labels": [{**visible, "text": "+：促进"}]}]}
        for i, text in enumerate(("A", "B")):
            actual["nodes"].append({"dataId": "n"+str(i), "attributes": {"class": "pele-node pele-shape-rect"},
                "rect": {"width": 10, "height": 10}, "shapes": [shape(i*50)], "labels": [{**visible, "text": text}]})
        expected = {"nodes": [{"id": "s1", "kind": "step", "label": "A"}, {"id": "s2", "kind": "step", "label": "B"}],
                    "edges": [{"id": "r1", "left": "s1", "right": "s2", "kind": "promotes"}]}
        variants = []
        changed = copy.deepcopy(actual); changed["edgeLabels"][0]["labels"][0]["text"] = "−：抑制"; variants.append(changed)
        changed = copy.deepcopy(actual); changed["edges"][0]["dataId"] = "L_n1_n0_0"; variants.append(changed)
        changed = copy.deepcopy(actual); changed["edges"][0]["markers"][0]["points"][0]["y"] = 40; variants.append(changed)
        changed = copy.deepcopy(actual); changed["nodes"][1]["shapes"] = [shape(200)]; variants.append(changed)
        changed = copy.deepcopy(actual); changed["nodes"][0]["labels"][0]["style"]["visibility"] = "hidden"; variants.append(changed)
        script = self.base / "geometry-check.mjs"
        script.write_text("import assert from 'node:assert/strict';\n" + V.PELE_VALIDATE +
            "\nconst expected=" + json.dumps(expected) + ";const cases=" + json.dumps([actual, *variants]) +
            ";console.log(JSON.stringify(cases.map(value=>{try{compatibilityPeleCheck(value,expected);return true;}catch{return false;}})));\n")
        run = subprocess.run([node, str(script)], capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr.decode())
        self.assertEqual(json.loads(run.stdout), [True, False, False, False, False, False])

    def test_format_probe_and_pele_engine_version_are_kept_distinct_from_mermaid_version(self):
        self.module_bytes["lib/mermaid.min.js"] = b"/*! Pele 1.8.0 | MIT License | https://github.com/obsidianmd/pele */\ninert, never executed"
        archive = self.runtime / "obsidian.asar"
        archive.write_bytes(asar({"package.json": b'{"version":"1.6.7"}', **self.module_bytes}))
        self.record["obsidian_asar"].update(V.pin(archive.read_bytes()))
        self.record["mermaid"]["member"].update(V.pin(self.module_bytes["lib/mermaid.min.js"]))
        self.record["mermaid"]["engine"] = {"name": "pele", "version": "1.8.0"}
        self.record["mermaid"]["version"] = None
        args = self.args("hand-authored-format-probe")
        args.runtime_sha256 = self.write_record()
        result = self.prepare(args)
        spec = V.strict_json(Path(result["capture_spec_path"]).read_bytes())
        self.assertEqual(spec["runtime"]["mermaid"]["version"], None)
        self.assertEqual(spec["runtime"]["mermaid"]["engine"], {"name": "pele", "version": "1.8.0"})
        script = Path(result["capture_script_path"]).read_text()
        self.assertIn("native YAML/Properties type and value", script)
        self.assertIn("actual Properties renderer type", script)
        self.assertIn("list:current?'aliases':'multitext'", script)
        self.assertIn("expectedSemanticType=type", script)
        self.assertIn("expectedNativeType=nativeType", script)
        self.assertIn("actual native checkbox preserves the authored boolean", script)
        self.assertIn("actual native aliases/list chips preserve the authored array", script)
        self.assertIn("native Callout type/title/body", script)
        self.assertIn("visibleTarget", script)
        self.assertIn("getScriptSource", script)
        self.assertIn("actual metadata cache retains the exact authored block ID", script)
        self.assertIn("actual clicked source fragment", script)
        self.assertIn("removedOneTerminalLf", script)
        self.assertIn("finalObservation", script)

    def test_format_block_comparison_only_accepts_the_observed_single_terminal_break(self):
        node = shutil.which("node")
        if node is None:
            self.skipTest("Node is required to execute the collector paragraph comparison")
        text = "必须完整保留的原始段落。"
        cases = [(text, False), (text+"\n", True), (text+"\n", False),
                 (text+"\n\n", True), (text+" ", True), ("必须完整保留的\n原始段落。\n", True)]
        script = self.base / "format-comparison.mjs"
        script.write_text(V.FORMAT_TEXT+"\nconst wanted="+json.dumps(text)+";const cases="+json.dumps(cases)+
                          ";console.log(JSON.stringify(cases.map(([raw,br])=>compatibilityParagraphText(raw,br).comparisonText===wanted)));\n")
        run = subprocess.run([node, str(script)], capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr.decode())
        self.assertEqual(json.loads(run.stdout), [True, True, False, False, False, False])

    def test_full_screenshot_png_rgba_is_independently_decoded_and_corruption_is_refused(self):
        from PIL import Image
        node = shutil.which("node")
        if node is None:
            self.skipTest("Node is required to verify the native screenshot PNG decoder")
        images = []
        for mode, channels in (("RGB", 3), ("RGBA", 4)):
            raw = bytes((p*37+c*53) % 256 for p in range(16*7) for c in range(channels))
            image = Image.frombytes(mode, (16, 7), raw)
            out = io.BytesIO(); image.save(out, format="PNG")
            path = self.base/(mode+".png");path.write_bytes(out.getvalue())
            images.append({"path": str(path), "rgba": image.convert("RGBA").tobytes().hex()})
        corrupt = self.base/"bad-crc.png"
        damaged = bytearray(Path(images[0]["path"]).read_bytes());damaged[-1] ^= 1;corrupt.write_bytes(damaged)
        full = Image.new("RGBA", (16, 7), "white");hidden = full.copy()
        for x in range(4, 7):
            for y in range(2, 6):
                full.putpixel((x, y), (0, 0, 0, 255))
        full_path = self.base/"authored-full.png";hidden_path = self.base/"authored-hidden.png"
        full.save(full_path);hidden.save(hidden_path)
        script = self.base/"screenshot-pixel-check.mjs"
        script.write_text("import fs from 'node:fs';import assert from 'node:assert/strict';\n"+V.SCREENSHOT_PIXELS+
            "\nconst images="+json.dumps(images)+";const results=images.map(item=>{const p=compatibilityPng(fs.readFileSync(item.path));return p.width===16&&p.height===7&&p.rgba.toString('hex')===item.rgba;});"+
            "try{compatibilityPng(fs.readFileSync("+json.dumps(str(corrupt))+"));results.push(false);}catch{results.push(true);}"+
            "const difference=compatibilityPixelDifference(compatibilityPng(fs.readFileSync("+json.dumps(str(full_path))+")),compatibilityPng(fs.readFileSync("+json.dumps(str(hidden_path))+")));"+
            "results.push(JSON.stringify(difference)===JSON.stringify({x:4,y:2,width:3,height:4,changedPixels:12}));"+
            "console.log(JSON.stringify(results));\n")
        run = subprocess.run([node, str(script)], capture_output=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr.decode())
        self.assertEqual(json.loads(run.stdout), [True, True, True, True])

    def test_current_svg_branch_keeps_native_pixels_and_legacy_intrinsic_checks(self):
        original = V.COLLECTORS["image"].read_bytes()
        script = V.derived_collector(original, "image").decode()
        self.assertIn("if(input.spec.runtime.app_version==='1.15.0')", script)
        self.assertIn("canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data", script)
        self.assertIn("actual native SVG src does not name the exact owned pinned file", script)
        self.assertIn("const nativePng=await obs.screenshot", script)
        self.assertIn("complete decoded RGBA arrays", script)
        self.assertIn("decoded.height,native.rgbaHeight", script)
        self.assertIn("auxiliaryRgbaSha256, native.decodedRgbaSha256", script)
        self.assertIn("svg-native-before-pixels.html", script)
        self.assertIn("full-versus-one-hidden-text RGBA", script)
        self.assertIn("nominalBbox:label.bbox", script)
        self.assertIn("restored.sha256,full.sha256", script)
        self.assertIn("e.actualInk?.bbox??e.bbox", script)
        self.assertEqual(V.COLLECTORS["image"].read_bytes(), original)

    def test_diagnostic_erratum_has_an_explicit_new_freeze_and_keeps_unknown_blocking(self):
        root = ROOT/"fixtures/visual-compatibility-corrected-v2"
        anchor = "c174f543947b6e72c7ef7e786624c4d2fa870342df79abcd9d0fc7fbb90cc60d"
        for case in ("scan-sign", "scan-subscript"):
            args = self.args(case, root);args.fixture_sha256 = anchor
            result = self.prepare(args);spec = V.strict_json(Path(result["capture_spec_path"]).read_bytes())
            self.assertIn("SCAN_SEMANTICS_UNKNOWN", spec["notes"][0]["required_texts"])
            self.assertNotIn("SCAN_SEMANTIC_UNKNOWN", spec["notes"][0]["required_texts"])
            self.assertEqual(spec["blocked_absent"], ["扫描公式预览.md", "plan.json", "compiled.tex"])
            self.assertFalse(spec["require_mathjax_runtime"])
        self.assertEqual(V.P.file_pin(FIXTURE/"frozen-sha256.json")["sha256"], FROZEN_SHA)

    def test_existing_output_and_ancestor_alias_are_refused_without_touching_sentinel(self):
        args = self.args()
        args.new_root.mkdir()
        sentinel = args.new_root / "user.txt"
        sentinel.write_bytes(b"user-owned bytes")
        with self.assertRaisesRegex(ValueError, "brand-new"):
            self.prepare(args)
        self.assertEqual(sentinel.read_bytes(), b"user-owned bytes")
        try:
            linked = self.base / "fixture-alias"
            linked.symlink_to(FIXTURE, target_is_directory=True)
        except OSError:
            self.skipTest("host does not permit directory symlinks; native Windows reparse guards need separate capture")
        args = self.args(root=linked)
        with self.assertRaisesRegex(ValueError, "link/reparse"):
            self.prepare(args)
        self.assertEqual(sentinel.read_bytes(), b"user-owned bytes")

    def test_embedded_author_freeze_and_unlisted_files_are_not_unpinned_exceptions(self):
        clone = self.base / "modified-inventory"
        shutil.copytree(FIXTURE, clone)
        old = clone / "source-originals/fixtures/formula-fidelity/frozen-sha256.json"
        original = old.read_bytes()
        old.write_bytes(original + b" ")
        with self.assertRaisesRegex(ValueError, "embedded original author freeze"):
            self.prepare(self.args(root=clone))
        old.write_bytes(original)
        (clone / "not-frozen.txt").write_bytes(b"unlisted file")
        with self.assertRaisesRegex(ValueError, "inventory mismatch"):
            self.prepare(self.args(root=clone))

    def test_preparation_cannot_write_into_immutable_fixture(self):
        args = self.args()
        args.new_root = FIXTURE / "new-prepared-output"
        before = V.fixture(FIXTURE, FROZEN_SHA)[1]
        with self.assertRaisesRegex(ValueError, "immutable fixture"):
            self.prepare(args)
        self.assertFalse(args.new_root.exists())
        self.assertEqual(V.fixture(FIXTURE, FROZEN_SHA)[1], before)

    def test_asar_member_bounds_and_embedded_identity_are_checked(self):
        raw = asar({"package.json": b'{"version":"1.6.7"}', "lib/selected.js": b"author-selected bytes"})
        self.assertEqual(V.member(raw, "lib/selected.js"), b"author-selected bytes")
        with self.assertRaisesRegex(ValueError, "escapes"):
            V.member(raw, "../selected.js")
        with self.assertRaisesRegex(ValueError, "out of bounds"):
            V.member(raw[:-1], "lib/selected.js")
        with self.assertRaisesRegex(ValueError, "integrity mismatch"):
            V.member(raw[:-1] + b"X", "lib/selected.js")


if __name__ == "__main__":
    unittest.main()
