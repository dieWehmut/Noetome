#!/usr/bin/env python3
"""Tests for the independent desktop packaging auditor.

The fixtures here are built by copying the repository packaging surface into a
temporary directory and then mutating exactly one thing, so a passing audit is
never the product of an un-tampered tree.
"""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
MODULE_PATH = SCRIPTS / "verify_desktop_package.py"

spec = importlib.util.spec_from_file_location("verify_desktop_package", MODULE_PATH)
V = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(V)

COPIED = [
    V.CONFIG,
    V.CAPABILITIES,
    V.ICON_MANIFEST,
    *V.ICON_FILES,
    V.PACKAGE_JSON,
    V.ACCEPTANCE,
    V.MAIN_RS,
    V.FROZEN,
    *V.INSTALLER_SURFACE,
]


def build_tree(directory: Path) -> Path:
    for relative in COPIED:
        source = ROOT / relative
        target = directory / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    # Vite names assets by their current content. Copy this build's entry and
    # complete output together so referenced files never come from an old hash.
    shutil.copytree(ROOT / V.DIST, directory / V.DIST)
    return directory


class PackagingTest(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory(prefix="noetome-package-")
        self.root = build_tree(Path(self._temp.name))
        self.addCleanup(self._temp.cleanup)

    # helpers -------------------------------------------------------------
    def load(self, relative: str) -> dict:
        return json.loads((self.root / relative).read_text(encoding="utf-8"))

    def store(self, relative: str, payload: dict) -> None:
        (self.root / relative).write_text(
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

    def names(self) -> set[str]:
        return {item["name"] for item in V.audit(self.root).failures}

    def assert_clean(self) -> None:
        report = V.audit(self.root)
        failures = report.failures
        self.assertEqual(failures, [], f"audit should be clean, failures={failures}")

    def assert_fails(self, *expected: str) -> None:
        failures = self.names()
        for name in expected:
            prefix = name if not name.endswith("-") else name
            matched = {item for item in failures if item == name or item.startswith(name)}
            self.assertTrue(matched, f"expected a failure starting with {name!r}, got {sorted(failures)}")

    # clean tree ----------------------------------------------------------
    def test_frozen_tree_passes_every_check(self) -> None:
        self.assert_clean()

    def test_fixture_preserves_all_current_built_dist_bytes(self) -> None:
        source = ROOT / V.DIST
        expected = {path.relative_to(source): path.read_bytes()
                    for path in source.rglob("*") if path.is_file()}
        copied = self.root / V.DIST
        actual = {path.relative_to(copied): path.read_bytes()
                  for path in copied.rglob("*") if path.is_file()}
        self.assertEqual(actual, expected)

    def test_freeze_is_reproducible_and_excludes_itself(self) -> None:
        first = self.load(V.FROZEN)
        self.assertNotIn(V.FROZEN, first["files"])
        for relative, entry in first["files"].items():
            data = (self.root / relative).read_bytes()
            self.assertEqual(entry["byte_length"], len(data), relative)
        V.freeze(self.root)
        second = self.load(V.FROZEN)
        self.assertEqual(first["files"], second["files"])

    def test_frozen_manifest_must_cover_the_whole_surface(self) -> None:
        frozen = self.load(V.FROZEN)
        expected = {
            V.CONFIG,
            V.CAPABILITIES,
            V.ICON_MANIFEST,
            *V.ICON_FILES,
            V.PACKAGE_JSON,
            V.ACCEPTANCE,
            *V.INSTALLER_SURFACE,
        }
        self.assertEqual(set(frozen["files"]), expected)
        self.assertEqual(frozen["producer"], "independent_packaging_freeze")
        self.assertEqual(frozen["workflow_id"], "noetome-desktop-personal-edition")

    def test_installer_glob_mapping_cannot_flatten_python_layout(self) -> None:
        overlay = self.load(V.INSTALL_CONFIG)
        overlay["bundle"]["resources"] = {"../../target/windows-installer-payload/**/*": ""}
        self.store(V.INSTALL_CONFIG, overlay)
        self.assert_fails("installer-resource-layout")

    def test_installer_bootstrapper_cannot_replace_offline_runtime(self) -> None:
        overlay = self.load(V.INSTALL_CONFIG)
        overlay["bundle"]["windows"]["webviewInstallMode"] = {"type": "downloadBootstrapper", "silent": True}
        self.store(V.INSTALL_CONFIG, overlay)
        self.assert_fails("installer-webview")

    def test_installer_overlay_cannot_expand_application_scope(self) -> None:
        overlay = self.load(V.INSTALL_CONFIG)
        overlay["app"] = {"security": {"csp": None}}
        self.store(V.INSTALL_CONFIG, overlay)
        self.assert_fails("installer-scope")

    def test_installer_resources_must_be_explicitly_selected(self) -> None:
        (self.root / "crates/noetome-desktop/tauri.windows.conf.json").write_text("{}", encoding="utf-8")
        self.assert_fails("installer-opt-in")

    def test_installer_malformed_object_reports_failure(self) -> None:
        self.store(V.INSTALL_CONFIG, {"bundle": []})
        self.assert_fails("installer-shape")

    # configuration -------------------------------------------------------
    def test_installer_target_drift_is_detected(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["targets"] = ["msi"]
        self.store(V.CONFIG, config)
        self.assert_fails("bundle-targets")

    def test_install_mode_drift_is_detected(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["windows"]["nsis"]["installMode"] = "perMachine"
        self.store(V.CONFIG, config)
        self.assert_fails("nsis-install-mode")

    def test_installer_language_drift_is_detected(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["windows"]["nsis"]["languages"] = ["SimpChinese"]
        self.store(V.CONFIG, config)
        self.assert_fails("nsis-languages")

    def test_language_selector_cannot_be_turned_on_silently(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["windows"]["nsis"]["displayLanguageSelector"] = True
        self.store(V.CONFIG, config)
        self.assert_fails("nsis-language-selector")

    def test_icon_list_drift_is_detected(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["icon"] = ["icons/icon.ico"]
        self.store(V.CONFIG, config)
        self.assert_fails("bundle-icons")

    def test_inactive_bundle_is_detected(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["active"] = False
        self.store(V.CONFIG, config)
        self.assert_fails("bundle-active")

    def test_identifier_and_minimum_window_are_frozen(self) -> None:
        config = self.load(V.CONFIG)
        config["identifier"] = "com.example.other"
        config["app"]["windows"][0]["minWidth"] = 320
        self.store(V.CONFIG, config)
        self.assert_fails("config-identifier", "window-minimum")

    def test_unknown_config_key_is_reported(self) -> None:
        config = self.load(V.CONFIG)
        config["bundle"]["windows"]["nsis"]["installerVersion"] = 3
        self.store(V.CONFIG, config)
        self.assert_fails("nsis-keys")

    def test_csp_must_not_open_remote_origins(self) -> None:
        config = self.load(V.CONFIG)
        config["app"]["security"]["csp"] = "default-src 'self' https://example.com; connect-src https://api.example.com"
        self.store(V.CONFIG, config)
        self.assert_fails("security-csp-ipc", "security-csp-remote")

    def test_csp_must_keep_default_src_self(self) -> None:
        config = self.load(V.CONFIG)
        config["app"]["security"]["csp"] = "default-src *; connect-src ipc:"
        self.store(V.CONFIG, config)
        self.assert_fails("security-csp-default")

    def test_decorations_and_second_window_are_rejected(self) -> None:
        config = self.load(V.CONFIG)
        config["app"]["windows"][0]["decorations"] = True
        self.store(V.CONFIG, config)
        self.assert_fails("window-decorations")

    # capabilities --------------------------------------------------------
    def test_file_plugin_permission_is_rejected(self) -> None:
        capabilities = self.load(V.CAPABILITIES)
        capabilities["permissions"].append("fs:allow-read-text-file")
        self.store(V.CAPABILITIES, capabilities)
        self.assert_fails("capabilities-forbidden", "capabilities-namespace")

    def test_non_core_namespace_is_rejected(self) -> None:
        capabilities = self.load(V.CAPABILITIES)
        capabilities["permissions"].append("shell:allow-execute")
        self.store(V.CAPABILITIES, capabilities)
        self.assert_fails("capabilities-forbidden", "capabilities-namespace")

    def test_capability_scope_must_match_the_main_window(self) -> None:
        capabilities = self.load(V.CAPABILITIES)
        capabilities["windows"] = ["other"]
        self.store(V.CAPABILITIES, capabilities)
        self.assert_fails("capabilities-windows")

    # icons ---------------------------------------------------------------
    def test_icon_byte_drift_is_detected(self) -> None:
        path = self.root / V.ICON_FILES[0]
        data = bytearray(path.read_bytes())
        data[0] ^= 0xFF
        path.write_bytes(bytes(data))
        failures = self.names()
        self.assertTrue(
            {"icon-digest", "frozen-digest"} & failures,
            f"icon mutation was not detected: {sorted(failures)}",
        )

    def test_missing_icon_file_is_detected(self) -> None:
        (self.root / V.ICON_FILES[2]).unlink()
        self.assert_fails("icon-file", "frozen-missing")

    def test_manifest_path_drift_is_detected(self) -> None:
        manifest = self.load(V.ICON_MANIFEST)
        manifest["assets"].append(dict(manifest["assets"][0], path="icons/extra.png"))
        self.store(V.ICON_MANIFEST, manifest)
        self.assert_fails("icon-manifest-paths")

    # build surface -------------------------------------------------------
    def test_build_script_drift_is_detected(self) -> None:
        package = self.load(V.PACKAGE_JSON)
        package["scripts"]["desktop:build"] = "tauri build"
        self.store(V.PACKAGE_JSON, package)
        self.assert_fails("package-script-desktop:build")

    def test_node_engine_must_be_declared(self) -> None:
        package = self.load(V.PACKAGE_JSON)
        del package["engines"]
        self.store(V.PACKAGE_JSON, package)
        self.assert_fails("package-engines-node")

    def test_dist_entry_must_reference_built_assets(self) -> None:
        index = self.root / V.DIST_INDEX
        index.write_text("<html><body><div id=\"root\"></div></body></html>", encoding="utf-8")
        self.assert_fails("dist-index-assets")

    def test_missing_dist_is_a_warning_not_a_pass(self) -> None:
        (self.root / V.DIST_INDEX).unlink()
        report = V.audit(self.root)
        self.assertEqual(report.failures, [])
        self.assertTrue(report.warnings, "a missing build output must be reported as a warning")

    # shell commands / acceptance -----------------------------------------
    def test_shell_command_removal_is_detected(self) -> None:
        path = self.root / V.MAIN_RS
        text = path.read_text(encoding="utf-8").replace("fn project_open(", "fn project_view(")
        path.write_text(text, encoding="utf-8")
        self.assert_fails("main-rs-command")

    def test_defined_command_missing_from_handler_is_detected(self) -> None:
        path = self.root / V.MAIN_RS
        text = path.read_text(encoding="utf-8")
        start = text.index("generate_handler![")
        text = text[:start] + text[start:].replace("source_extract,", "", 1)
        path.write_text(text, encoding="utf-8", newline="\n")
        self.assertIn("fn source_extract(", text)
        self.assert_fails("main-rs-handler")

    def test_unreviewed_extra_handler_command_is_detected(self) -> None:
        path = self.root / V.MAIN_RS
        text = path.read_text(encoding="utf-8").replace("generate_handler![", "generate_handler![arbitrary_process,")
        path.write_text(text, encoding="utf-8", newline="\n")
        self.assert_fails("main-rs-handler")

    def test_acceptance_checklist_must_cover_every_area(self) -> None:
        path = self.root / V.ACCEPTANCE
        text = path.read_text(encoding="utf-8").replace("任务中心", "任务列表")
        path.write_text(text, encoding="utf-8")
        self.assert_fails("acceptance-section")

    def test_acceptance_must_say_static_checks_are_not_enough(self) -> None:
        path = self.root / V.ACCEPTANCE
        text = path.read_text(encoding="utf-8").replace("不能替代", "可以参考")
        path.write_text(text, encoding="utf-8")
        self.assert_fails("acceptance-disclaimer")

    def test_missing_acceptance_file_is_a_failure(self) -> None:
        (self.root / V.ACCEPTANCE).unlink()
        self.assert_fails("acceptance-file", "frozen-missing")

    # frozen manifest -----------------------------------------------------
    def test_frozen_drift_after_freeze_is_detected(self) -> None:
        self.assert_clean()
        config = self.load(V.CONFIG)
        config["bundle"]["shortDescription"] = "另一个描述"
        self.store(V.CONFIG, config)
        self.assert_fails("frozen-digest")

    def test_missing_freeze_is_only_a_warning(self) -> None:
        (self.root / V.FROZEN).unlink()
        report = V.audit(self.root)
        self.assertEqual(report.failures, [])
        names = {item["name"] for item in report.warnings}
        self.assertIn("frozen-manifest", names)


class CommandLineTest(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory(prefix="noetome-package-cli-")
        self.root = build_tree(Path(self._temp.name))
        self.addCleanup(self._temp.cleanup)

    def test_audit_exit_code_follows_failures(self) -> None:
        self.assertEqual(V.main([str(self.root)]), 0)
        config = self.load(V.CONFIG)
        config["identifier"] = "com.example.other"
        self.store(V.CONFIG, config)
        self.assertEqual(V.main([str(self.root)]), 1)

    def test_json_output_is_machine_readable(self) -> None:
        report = V.audit(self.root)
        payload = report.dump()
        self.assertEqual(payload["failures"], 0)
        self.assertGreater(payload["total"], 50)
        self.assertTrue(all({"name", "ok", "detail"} <= set(item) for item in payload["items"]))

    def test_unknown_flag_is_refused(self) -> None:
        self.assertEqual(V.main([str(self.root), "--nonsense"]), 2)

    def test_root_must_exist(self) -> None:
        self.assertEqual(V.main([str(self.root / "absent")]), 2)

    def load(self, relative: str) -> dict:
        return json.loads((self.root / relative).read_text(encoding="utf-8"))

    def store(self, relative: str, payload: dict) -> None:
        (self.root / relative).write_text(
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    unittest.main()
