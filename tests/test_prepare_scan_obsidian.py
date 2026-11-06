"""Preparation attacks on synthetic saved live-shaped receipts; no real calls.

Runtime bytes and receipt provenance here are explicitly artificial unit fixtures.
These tests cannot establish actual model recognition or native Obsidian rendering.
"""
from __future__ import annotations

import argparse
import importlib.util
import os
import json
from pathlib import Path
import tempfile
import shutil
import subprocess
import stat
import time
from types import SimpleNamespace
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


P = module("prepare_scan_obsidian", ROOT / "scripts/prepare_scan_obsidian.py")
S = module("prepare_scan_synthetic_fixture", ROOT / "tests/test_verify_scan_formula.py")
V = S.V


class PrepareScanObsidianTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def sample(self, *, profile="native", state="clear", blocked=False, launch=False):
        source = self.base / "synthetic-vault"
        case, api, profile_path, expected = S.synthetic_vault(source, state, profile)
        report, binding_raw, _ = S.synthetic_budget_originals(source, (case, api, profile_path, expected))
        binding = V.strict_json(binding_raw)
        if report["status"] == "needs_review":
            ref = binding["receipts"]["settled"]
            phase = V.strict_json(V.read(ref["file"]))
            phase["result"]["cli_exit_code"] = 3
            V.save(ref["file"], phase)
            ref.update(V.file_pin(ref["file"]))
        first = V.strict_json(V.read(binding["receipts"]["before"]["file"]))
        cli = Path(binding["copies"][first["selected_binary"]["path"]]["file"])
        binding_path = self.base / "synthetic-budget-binding.json"
        V.save(binding_path, binding)
        invocation = self.base / "synthetic-invocation.json"
        V.save(invocation, {"command": [str(cli), "knowledge", "scan-formula", str(case), str(api), str(profile_path), str(source)],
                            "exit_code": 0 if report["status"] == "preview_ready" else 3})
        obsidian = self.base / "synthetic-obsidian.exe"
        asar = self.base / "synthetic-obsidian.asar"
        obsidian.write_bytes(b"synthetic not executable Obsidian bytes")
        asar.write_bytes(b"synthetic package bytes")
        args = argparse.Namespace(vault=source, case=case, api_config=api, profile=profile_path, expected=expected,
                                  invocation_receipt=invocation, budget_binding=binding_path,
                                  budget_binding_sha256=V.file_pin(binding_path)["sha256"], cli=cli,
                                  obsidian_exe=obsidian, obsidian_asar=asar, new_root=self.base / "prepared",
                                  include_blocked_report=blocked, native_python=None, powershell=None, playwright_module=None)
        if launch:
            for name in ("native_python", "powershell", "playwright_module"):
                path = self.base / ("synthetic-" + name)
                path.write_bytes(("synthetic no execution " + name).encode())
                setattr(args, name, path)
        fake_pins = {name: P.file_pin(getattr(args, name)) for name in P.RUNTIME_PINS}
        return args, fake_pins

    def prepare(self, args, pins):
        # No substitute checker result: actual independent byte/note/budget verification runs.
        with mock.patch.dict(P.RUNTIME_PINS, pins, clear=True), \
                mock.patch("subprocess.run", side_effect=AssertionError("preparation must not run processes")), \
                mock.patch("subprocess.Popen", side_effect=AssertionError("preparation must not launch applications")), \
                mock.patch("urllib.request.urlopen", side_effect=AssertionError("preparation must not access a network")):
            return P.prepare(args)

    def test_verified_native_preserves_bytes_three_scripts_and_raw_tex_links_without_calls(self):
        args, pins = self.sample(launch=True)
        original = P.inventory(args.vault)
        receipt = self.prepare(args, pins)
        self.assertEqual(P.inventory(args.vault), original)
        self.assertEqual(P.inventory(args.new_root / "selected-output"), original)
        self.assertEqual(receipt["native_capture_status"], "not_run")
        self.assertEqual(receipt["model_requests_in_preparation"], 0)
        spec = json.loads((args.new_root / "capture-spec.json").read_bytes())
        note = spec["notes"][0]
        self.assertEqual(note["file"], "扫描公式预览.md")
        self.assertEqual(note["math_trees"], [P.AUTHOR_TREE])
        self.assertEqual(note["math_trees"][0][1][-1][1][0][1], [["mi", "a"], ["mi", "i"]])
        self.assertIn("recognized.tex", note["link_destinations"])
        self.assertIn("response.bin", note["link_destinations"])
        self.assertIn("original.pdf", note["link_destinations"])
        self.assertEqual(note["click_attachments"], ["original.png", "page.png"])
        raw_recognition = V.strict_json(original["recognition.json"])["recognized_tex"]
        self.assertEqual(note["preformatted_exact"][1], raw_recognition)
        self.assertEqual(len(note["preformatted_exact"]), 19)
        self.assertIn("--capture-script", receipt["cases"][0]["launch_argv"])
        self.assertFalse((args.new_root / "captures" / spec["case"]).exists())

    def test_verified_unverified_profile_retains_actual_image_note_without_native_math(self):
        args, pins = self.sample(profile="unverified")
        original = P.inventory(args.vault)
        receipt = self.prepare(args, pins)
        self.assertEqual(receipt["cases"][0]["representation"], "formula_image")
        spec = json.loads((args.new_root / "capture-spec.json").read_bytes())
        self.assertFalse(spec["require_mathjax_runtime"])
        self.assertEqual(spec["notes"][0]["math_trees"], [])
        self.assertEqual(spec["notes"][0]["images"], [{"file": "original.png", "width": 560, "height": 200}])
        self.assertEqual(P.inventory(args.new_root / "selected-output"), original)
        self.assertNotIn("compiled.tex", original)

    def test_unknown_source_cannot_be_prepared_as_success_even_if_recognition_guesses_clear(self):
        args, pins = self.sample(state="unreadable_subscript")
        with self.assertRaisesRegex(ValueError, "include-blocked-report"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_system_powershell_metadata_may_keep_existing_hardlinks_but_source_cannot(self):
        args, pins = self.sample(launch=True)
        os.link(args.powershell, self.base / "system-powershell-second-link")
        receipt = self.prepare(args, pins)
        self.assertIn("powershell", receipt["launch_runtime"])
        args.new_root = self.base / "second-preparation"
        os.link(args.vault / "original.png", self.base / "source-second-link")
        with self.assertRaisesRegex(ValueError, "ordinary link"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_read_and_hash_growth_stop_at_selected_limit(self):
        selected = self.base / "growth.bin"
        original_open = Path.open
        for operation, grow_on_open in ((lambda: P.file_pin(selected, maximum=32), 1), (lambda: P.read(selected, maximum=32), 2)):
            selected.write_bytes(b"small initial bytes")
            opens = 0
            def growing_open(path, *arguments, **kwargs):
                nonlocal opens
                stream = original_open(path, *arguments, **kwargs)
                if path == selected and arguments and arguments[0] == "rb":
                    opens += 1
                    if opens == grow_on_open:
                        with original_open(selected, "ab") as writer:
                            writer.write(b"x" * 200)
                return stream
            with self.subTest(operation=operation), mock.patch.object(Path, "open", growing_open):
                with self.assertRaisesRegex(ValueError, "changed|exceeds readback limit"):
                    operation()

    @staticmethod
    def changed_stat(info, **changes):
        names = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns", "st_nlink", "st_mode")
        values = {name: getattr(info, name) for name in names}
        if hasattr(info, "st_birthtime_ns"):
            values["st_birthtime_ns"] = info.st_birthtime_ns
        return SimpleNamespace(**dict(values, **changes))

    def test_windows_common_identity_keeps_other_fields_and_linux_ctime_strict(self):
        selected = self.base / "identity.bin"
        selected.write_bytes(b"same selected file")
        info = selected.stat()
        different_ctime = self.changed_stat(info, st_ctime_ns=info.st_ctime_ns + 1)
        self.assertTrue(P.same_open_file(info, different_ctime, windows=True))
        self.assertFalse(P.same_open_file(info, different_ctime, windows=False))
        for name in ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_nlink"):
            with self.subTest(changed=name):
                changed = self.changed_stat(different_ctime, **{name: getattr(info, name) + 1})
                self.assertFalse(P.same_open_file(info, changed, windows=True))
        self.assertFalse(P.same_open_file(info, self.changed_stat(info, st_mode=stat.S_IFIFO), windows=True))
        if hasattr(info, "st_birthtime_ns"):
            self.assertFalse(P.same_open_file(info, self.changed_stat(info, st_birthtime_ns=info.st_birthtime_ns + 1), windows=True))

    @unittest.skipUnless(os.name == "nt", "native Windows creation/change time regression")
    def test_native_windows_created_then_delayed_write_hash_and_read(self):
        selected = self.base / "delayed-write.json"
        selected.write_bytes(b"{}")
        time.sleep(0.06)
        content = b'{"public":"created before this write"}'
        selected.write_bytes(content)
        with selected.open("rb") as stream:
            path_info, descriptor_info = selected.stat(), os.fstat(stream.fileno())
        self.assertNotEqual(path_info.st_ctime_ns, path_info.st_mtime_ns)
        self.assertTrue(P.same_open_file(path_info, descriptor_info))
        self.assertEqual(P.file_pin(selected), P.raw_pin(content))
        self.assertEqual(P.read(selected), content)

    def test_descriptor_ctime_change_during_read_is_still_rejected(self):
        selected = self.base / "descriptor-change.bin"
        selected.write_bytes(b"stable size and mtime")
        actual_fstat = os.fstat
        calls = 0
        def changed_fstat(fd):
            nonlocal calls
            calls += 1
            info = actual_fstat(fd)
            return self.changed_stat(info, st_ctime_ns=info.st_ctime_ns + 1) if calls == 2 else info
        with mock.patch.object(P.os, "fstat", changed_fstat):
            with self.assertRaisesRegex(ValueError, "identity changed during reading"):
                P.file_pin(selected)

    def test_path_ctime_change_after_read_is_still_rejected(self):
        selected = self.base / "path-change.bin"
        selected.write_bytes(b"stable descriptor")
        actual_stat = Path.stat
        calls = 0
        def changed_path_stat(path, *arguments, **kwargs):
            nonlocal calls
            info = actual_stat(path, *arguments, **kwargs)
            if path == selected and kwargs.get("follow_symlinks", True):
                calls += 1
                if calls == 2:
                    return self.changed_stat(info, st_ctime_ns=info.st_ctime_ns + 1)
            return info
        with mock.patch.object(Path, "stat", changed_path_stat):
            with self.assertRaisesRegex(ValueError, "selected file changed while reading"):
                P.file_pin(selected)

    def test_dark_theme_uses_existing_theme_aware_launcher_with_exact_scanning_collector(self):
        args, pins = self.sample(launch=True)
        args.theme = "dark"
        receipt = self.prepare(args, pins)
        spec = json.loads((args.new_root / "capture-spec.json").read_bytes())
        self.assertEqual(spec["theme"], "dark")
        self.assertEqual(receipt["cases"][0]["launch_argv"][1], str(P.LAUNCHER))
        self.assertEqual(receipt["cases"][0]["launch_argv"][-1], str(args.new_root / "capture_scan_formula.mjs"))

    def test_explicit_blocked_capture_only_reads_report_and_requires_absent_success_artifacts(self):
        args, pins = self.sample(state="unreadable_sign", blocked=True)
        receipt = self.prepare(args, pins)
        spec = json.loads((args.new_root / "capture-spec.json").read_bytes())
        self.assertIsNone(spec["representation"])
        self.assertFalse(spec["successful_expression"])
        self.assertEqual(spec["notes"][0]["file"], "scan.md")
        self.assertEqual(spec["notes"][0]["math_trees"], [])
        self.assertEqual(spec["blocked_absent"], ["扫描公式预览.md", "plan.json", "compiled.tex"])
        self.assertEqual(receipt["cases"][0]["exit_code"], 3)

    def test_synchronized_display_minus_forgery_rejected_before_new_root(self):
        args, pins = self.sample()
        preview = args.vault / "扫描公式预览.md"
        preview.write_bytes(preview.read_bytes().replace(b"=-", b"=", 1))
        V.resynchronize_result(args.vault)
        with self.assertRaisesRegex(AssertionError, "actual compiled text bytes"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_unanchored_or_forged_verification_report_cannot_bypass_current_readback(self):
        args, pins = self.sample()
        args.budget_binding_sha256 = "0" * 64
        with self.assertRaisesRegex(AssertionError, "independently captured budget originals manifest"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_changed_selected_runtime_rejected_before_creating_new_root(self):
        args, pins = self.sample()
        args.obsidian_asar.write_bytes(b"different package version")
        with self.assertRaisesRegex(ValueError, "obsidian_asar differs"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_runtime_copy_cannot_mask_different_binary_in_original_launch_receipts(self):
        args, pins = self.sample()
        other = self.base / "synthetic-new-cli"
        other.write_bytes(b"different selected runtime than old invocation")
        args.cli = other
        pins["cli"] = P.file_pin(other)
        with self.assertRaisesRegex(ValueError, "invocation used a different CLI"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_existing_output_and_unknown_source_directory_do_not_get_overwritten_or_traversed(self):
        args, pins = self.sample()
        args.new_root.mkdir()
        sentinel = args.new_root / "human.txt"
        sentinel.write_bytes(b"human original")
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.prepare(args, pins)
        self.assertEqual(sentinel.read_bytes(), b"human original")
        args.new_root = self.base / "new-root"
        (args.vault / "unknown-empty").mkdir()
        with self.assertRaisesRegex(ValueError, "ordinary link"):
            self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_source_symlink_refused_without_reading_the_target(self):
        args, pins = self.sample()
        link = self.base / "source-alias"
        try:
            link.symlink_to(args.vault, target_is_directory=True)
        except OSError:
            self.skipTest("symbolic links unavailable")
        args.vault = link
        with mock.patch.object(P, "inventory", side_effect=AssertionError("alias must be rejected before inventory")):
            with self.assertRaisesRegex(ValueError, "link/reparse"):
                self.prepare(args, pins)

    def test_source_changed_after_independent_check_is_not_copied_as_verified(self):
        args, pins = self.sample()
        actual_check = V.verify_output
        def mutate(*selected, **kwargs):
            checked = actual_check(*selected, **kwargs)
            (args.vault / "original.png").write_bytes(b"replaced after verification")
            return checked
        checker = argparse.Namespace(**vars(V))
        checker.verify_output = mutate
        with mock.patch.object(P, "load_verifier", return_value=checker):
            with self.assertRaisesRegex(ValueError, "source changed before preparation"):
                self.prepare(args, pins)
        self.assertFalse(args.new_root.exists())

    def test_derived_capture_rejects_missing_or_ambiguous_anchor_and_parses_as_module(self):
        for text in (b"no insertion point", (P.CLICK_ANCHOR * 2).encode()):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "exactly once"):
                P.derived_capture(text)
        source = (P.NATIVE / "capture_formula.mjs").read_bytes()
        derived = P.derived_capture(source)
        self.assertEqual(derived.replace(P.CLICK_CHECK.encode(), b"").replace(P.THEME_CHECK.encode(), b""), source)
        self.assertIn(b"view.containerEl.querySelectorAll('img')", derived)
        self.assertIn(b"input.spec.attachment_images[target]", derived)
        self.assertIn(b"result.identity.bodyClass", derived)
        self.assertIn(b"theme-' + input.spec.theme", derived)
        node = shutil.which("node")
        if node:
            result = subprocess.run([node, "--input-type=module", "--check"], input=derived, capture_output=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr.decode())


if __name__ == "__main__":
    unittest.main()
