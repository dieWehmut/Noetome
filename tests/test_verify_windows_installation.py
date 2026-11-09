"""Reject tampered payloads and preserve actual subprocess/shortcut boundaries."""

from __future__ import annotations

import base64
import copy
import importlib.util
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "installation_smoke", ROOT / "scripts/verify_windows_installation.py"
)
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)
WSL_SHELL = Path("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
SHELL = (
    shutil.which("powershell.exe")
    if os.name == "nt"
    else (str(WSL_SHELL) if WSL_SHELL.is_file() and shutil.which("wslpath") else None)
)


def pe(dependencies=(), machine=0x8664):
    """Small author PE image with real COFF/data-directory/import structures."""
    raw = bytearray(2048)
    raw[:2] = b"MZ"
    struct.pack_into("<I", raw, 60, 0x80)
    raw[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", raw, 0x84, machine, 1)
    struct.pack_into("<H", raw, 0x94, 240)
    struct.pack_into("<H", raw, 0x98, 0x20B)
    struct.pack_into("<IIII", raw, 0x188 + 8, 1024, 0x1000, 1024, 512)
    if dependencies:
        struct.pack_into(
            "<II", raw, 0x98 + 112 + 8, 0x1000, 20 * (len(dependencies) + 1)
        )
        for index, name in enumerate(dependencies):
            offset = 768 + index * 100
            struct.pack_into(
                "<IIIII", raw, 512 + index * 20, 1, 0, 0, 0x1000 + offset - 512, 1
            )
            encoded = name.encode("ascii") + b"\0"
            raw[offset : offset + len(encoded)] = encoded
    return bytes(raw)


class InstallationVerifierTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="noetome-install-check-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.install = self.root / "install"
        self.install.mkdir()
        self.value = {
            "schema_version": 1,
            "producer": "noetome-windows-installer-payload",
            "source_commit": "a" * 40,
            "target": "x86_64-pc-windows-msvc",
            "build_profile": "release",
            "source_inputs": {},
            "files": [],
            "totals": {"files": 0, "bytes": 0},
        }
        for name, content in {
            "noetome.exe": pe(["VCRUNTIME140.dll"]),
            "vcruntime140.dll": pe(),
            "pdf-python/python.exe": pe(["VCRUNTIME140.dll"]),
            "pdf-python/vcruntime140.dll": pe(),
            "samples/中文 空格.pdf": b"author sample",
        }.items():
            path = self.install / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            self.value["files"].append(
                {
                    "path": name,
                    "bytes": len(content),
                    "sha256": SMOKE.sha(content),
                    "source": "author test",
                }
            )
        self.value["totals"] = {
            "files": len(self.value["files"]),
            "bytes": sum(row["bytes"] for row in self.value["files"]),
        }
        self.manifest_bytes = json.dumps(self.value, ensure_ascii=False).encode("utf-8")
        self.checksum = (SMOKE.sha(self.manifest_bytes) + "\n").encode("ascii")
        for name, content in {
            "payload-manifest.json": self.manifest_bytes,
            "payload-manifest.sha256": self.checksum,
            "noetome-desktop.exe": pe(),
            "uninstall.exe": pe(),
        }.items():
            (self.install / name).write_bytes(content)

    def test_manifest_installed_tree_and_vc_imports_accept_exact_author_payload(self):
        value = SMOKE.manifest(self.manifest_bytes)
        proof = SMOKE.verify_payload(
            self.install, value, self.manifest_bytes, self.checksum
        )
        self.assertEqual(proof["files"], 5)
        imports = SMOKE.verify_pe(self.install, value)
        self.assertEqual(
            next(row for row in imports if row["path"] == "noetome.exe")["imports"],
            ["vcruntime140.dll"],
        )

    def test_manifest_rejects_duplicate_json_and_case_aliases(self):
        with self.assertRaises(ValueError):
            SMOKE.manifest(b'{"schema_version":1,"schema_version":1}')
        value = copy.deepcopy(self.value)
        row = copy.deepcopy(value["files"][0])
        row["path"] = row["path"].upper()
        value["files"].append(row)
        with self.assertRaises(ValueError):
            SMOKE.manifest(json.dumps(value).encode())

    def test_external_preparation_sha_rejects_a_self_consistent_tampered_manifest(self):
        trusted = SMOKE.sha(self.manifest_bytes)
        self.assertEqual(
            SMOKE.validate_stage_manifest(self.manifest_bytes, self.checksum, trusted),
            self.value,
        )
        changed = copy.deepcopy(self.value)
        changed["files"][0]["sha256"] = "0" * 64
        changed_bytes = json.dumps(changed).encode("utf-8")
        changed_checksum = (SMOKE.sha(changed_bytes) + "\n").encode("ascii")
        self.assertEqual(SMOKE.manifest(changed_bytes), changed)
        with self.assertRaisesRegex(ValueError, "trusted preparation receipt"):
            SMOKE.validate_stage_manifest(changed_bytes, changed_checksum, trusted)
        with self.assertRaises(ValueError):
            SMOKE.validate_stage_manifest(self.manifest_bytes, self.checksum, "short")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            SMOKE.validate_stage_manifest(
                self.manifest_bytes, b"not-the-checksum", trusted
            )

    def test_manifest_rejects_windows_devices_traversal_and_noncanonical_members(self):
        for name in (
            "../escape",
            "a/../escape",
            "/absolute",
            "a//b",
            "a\\b",
            "CON.txt",
            "a/NUL",
            "a. /b",
            "name.",
            "a:b",
            "./a",
            "a/./b",
            "a/",
        ):
            with self.subTest(name=name), self.assertRaises(ValueError):
                SMOKE.member(name)
        self.assertEqual(
            SMOKE.member("samples/中文 空格.pdf").as_posix(), "samples/中文 空格.pdf"
        )

    def test_manifest_rejects_unknown_identity_missing_commit_false_bytes_and_wrong_totals(
        self,
    ):
        for mutate in (
            lambda v: v.update(producer="fake"),
            lambda v: v.update(source_commit="short"),
            lambda v: v["files"][0].update(bytes=True),
            lambda v: v["totals"].update(bytes=0),
            lambda v: v["files"][0].update(sha256="0"),
            lambda v: v.update(files=[]),
        ):
            value = copy.deepcopy(self.value)
            mutate(value)
            with self.subTest(value=value), self.assertRaises((ValueError, TypeError)):
                SMOKE.manifest(json.dumps(value).encode())

    def test_installed_payload_rejects_changed_missing_extra_and_installed_manifest(
        self,
    ):
        source = self.install / "samples/中文 空格.pdf"
        original = source.read_bytes()
        for change in ("alter", "delete", "extra", "manifest"):
            with self.subTest(change=change):
                if change == "alter":
                    source.write_bytes(original + b"tamper")
                elif change == "delete":
                    source.unlink()
                elif change == "extra":
                    (self.install / "dev-fallback.dll").write_bytes(b"unexpected")
                else:
                    (self.install / "payload-manifest.json").write_bytes(
                        self.manifest_bytes + b" "
                    )
                with self.assertRaises(ValueError):
                    SMOKE.verify_payload(
                        self.install, self.value, self.manifest_bytes, self.checksum
                    )
                source.write_bytes(original)
                (self.install / "dev-fallback.dll").unlink(missing_ok=True)
                (self.install / "payload-manifest.json").write_bytes(
                    self.manifest_bytes
                )

    def test_archive_captures_exact_installed_bytes_and_preserves_separate_manual_files(
        self,
    ):
        empty = self.install / "empty-installed-directory"
        empty.mkdir()
        sentinel = self.root / "manual-sentinel.md"
        sentinel.write_bytes(b"outside selected installation")
        before = SMOKE.snapshot(self.install)
        output = self.root / "installed-payload.zip"
        receipt = SMOKE.archive_installed(self.install, output, before)
        self.assertEqual(receipt["sha256"], SMOKE.sha(output.read_bytes()))
        self.assertEqual(
            receipt["installed_executable"],
            SMOKE.fingerprint(self.install / "noetome-desktop.exe"),
        )
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(set(archive.namelist()), set(before))
            self.assertEqual(
                archive.read("noetome.exe"), (self.install / "noetome.exe").read_bytes()
            )
            self.assertNotIn("manual-sentinel.md", archive.namelist())
        self.assertEqual(sentinel.read_bytes(), b"outside selected installation")
        with self.assertRaises(ValueError):
            SMOKE.archive_installed(self.install, output, before)
        (self.install / "noetome.exe").write_bytes(b"changed")
        rejected = self.root / "changed-payload.zip"
        with self.assertRaises(ValueError):
            SMOKE.archive_installed(self.install, rejected, before)
        self.assertFalse(rejected.exists())
        with self.assertRaises(ValueError):
            SMOKE.archive_installed(
                self.install, self.install / "archive.zip", SMOKE.snapshot(self.install)
            )

    def test_selected_tree_refuses_symlink_and_hardlink(self):
        linked = self.install / "linked"
        try:
            linked.symlink_to(self.root, target_is_directory=True)
        except OSError:
            pass
        else:
            with self.assertRaises(ValueError):
                SMOKE.snapshot(self.install)
            linked.unlink()
        original = self.install / "noetome.exe"
        os.link(original, linked)
        with self.assertRaises(ValueError):
            SMOKE.snapshot(self.install)
        linked.unlink()

    def test_vc_dll_required_even_when_the_system_has_it(self):
        value = copy.deepcopy(self.value)
        value["files"] = [
            row for row in value["files"] if row["path"] != "vcruntime140.dll"
        ]
        with self.assertRaisesRegex(ValueError, "System32 is insufficient"):
            SMOKE.verify_pe(self.install, value)
        (self.install / "noetome.exe").write_bytes(pe(["MSVCP140.dll"]))
        with self.assertRaisesRegex(ValueError, "unbundled VC dependency"):
            SMOKE.verify_pe(self.install, self.value)

    def test_pe_rejects_other_machine_and_truncated_structure(self):
        (self.install / "noetome.exe").write_bytes(pe(machine=0x14C))
        with self.assertRaisesRegex(ValueError, "not x64"):
            SMOKE.verify_pe(self.install, self.value)
        for raw in (b"", b"MZ", pe()[:200], pe(["vcruntime140.dll"])[:550]):
            with self.subTest(length=len(raw)), self.assertRaises(ValueError):
                SMOKE.pe_imports(raw)

    def test_only_exact_pip_foreign_cpu_templates_with_selected_bytes_are_allowed(self):
        value = copy.deepcopy(self.value)
        for name, machine in SMOKE.PIP_LAUNCHER_TEMPLATES.items():
            data = pe(machine=machine)
            target = self.install / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            value["files"].append(
                {
                    "path": name,
                    "bytes": len(data),
                    "sha256": SMOKE.sha(data),
                    "source": "complete upstream pip wheel",
                }
            )
        proof = SMOKE.verify_pe(self.install, value)
        self.assertEqual(
            sum(row["role"] == "pip_launcher_template" for row in proof), 4
        )
        name = "pdf-python/lib/site-packages/pip/_vendor/distlib/t32.exe"
        target = self.install / name
        target.write_bytes(pe(machine=0x8664))
        with self.assertRaisesRegex(ValueError, "selected stage bytes"):
            SMOKE.verify_pe(self.install, value)
        record = next(row for row in value["files"] if row["path"] == name)
        record["sha256"] = SMOKE.sha(target.read_bytes())
        with self.assertRaises(ValueError):
            SMOKE.verify_pe(self.install, value)
        unknown = "pdf-python/lib/site-packages/pip/_vendor/distlib/unapproved32.exe"
        target = self.install / unknown
        target.write_bytes(pe(machine=0x14C))
        value["files"] = [row for row in value["files"] if row["path"] != name]
        value["files"].append(
            {
                "path": unknown,
                "bytes": target.stat().st_size,
                "sha256": SMOKE.sha(target.read_bytes()),
                "source": "unapproved",
            }
        )
        with self.assertRaisesRegex(ValueError, "not x64"):
            SMOKE.verify_pe(self.install, value)

    def test_child_environment_removes_dev_paths_overrides_and_credentials(self):
        with patch.dict(
            os.environ,
            {
                "SystemRoot": str(self.root),
                "PATH": "/developer/python:/developer/cargo",
                "NOETOME_PDF_PYTHON": "developer-python",
                "NOETOME_REAL_SECRET": "public-test-placeholder",
                "PYTHONPATH": "dev-site",
                "PYTHONHOME": "dev-python",
                "VIRTUAL_ENV": "dev-venv",
                "CARGO_HOME": "dev-cargo",
                "PSModulePath": "developer-PowerShell7-modules",
                "PSMODULEPATH": "developer-uppercase-modules",
            },
        ):
            parent = dict(os.environ)
            environment = SMOKE.clean_environment()
            self.assertEqual(dict(os.environ), parent)
        self.assertFalse(any(key.upper().startswith("NOETOME_") for key in environment))
        self.assertNotIn("developer", environment["PATH"])
        for name in (
            "PYTHONPATH",
            "PYTHONHOME",
            "VIRTUAL_ENV",
            "CARGO_HOME",
            "RUSTUP_HOME",
        ):
            self.assertNotIn(name, environment)
        self.assertIn("System32", environment["PATH"])
        self.assertEqual(
            environment["PSModulePath"],
            str(self.root / "System32/WindowsPowerShell/v1.0/Modules"),
        )
        self.assertEqual(
            [key for key in environment if key.upper() == "PSMODULEPATH"],
            ["PSModulePath"],
        )

    @unittest.skipUnless(SHELL, "Native Windows PowerShell is unavailable")
    def test_native_winps_hash_uses_builtin_modules_despite_inherited_core_path(self):
        windows_root = os.environ.get("SystemRoot", "C:\\Windows")
        with patch.dict(os.environ, {"SystemRoot": windows_root}):
            outer_env = SMOKE.clean_environment()
            builtin_modules = outer_env["PSModulePath"]
        # A new, deliberately unusable Core module fixture models a foreign
        # shell's module path. No module is installed into any host directory.
        command = r"""
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$original=$env:PSModulePath
$owned=Join-Path ([IO.Path]::GetTempPath()) ('noetome-winps-module-test-'+[Guid]::NewGuid().ToString('N'))
try {
  $modules=Join-Path $owned 'PowerShell7-Modules'
  $utility=Join-Path $modules 'Microsoft.PowerShell.Utility'
  [IO.Directory]::CreateDirectory($utility) | Out-Null
  $manifest="@{ModuleVersion='7.0.0'; CompatiblePSEditions=@('Core'); PowerShellVersion='3.0'; CmdletsToExport=@('Get-FileHash'); NestedModules=@('Unavailable.PowerShell7.Utility.dll')}"
  [IO.File]::WriteAllText((Join-Path $utility 'Microsoft.PowerShell.Utility.psd1'),$manifest)
  $sample=Join-Path $owned 'author-input.txt'; [IO.File]::WriteAllText($sample,'owned SHA probe')
  $script=Join-Path $owned 'hash.ps1'
  [IO.File]::WriteAllText($script, @'
param([string]$InputFile)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$hash=Get-FileHash -LiteralPath $InputFile -Algorithm SHA256
[Console]::WriteLine($hash.Hash.ToLowerInvariant())
'@)
  function Probe([string]$ModulePath) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $info.Arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+$script+'" -InputFile "'+$sample+'"'
    $info.UseShellExecute=$false; $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.CreateNoWindow=$true
    $info.EnvironmentVariables['PSModulePath']=$ModulePath
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
    if (-not $process.Start()) {throw 'hash-probe-spawn-failed'}
    $out=$process.StandardOutput.ReadToEndAsync(); $err=$process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(20000)) { $process.Kill(); throw 'owned-hash-probe-timeout' }
    return @{exit_code=$process.ExitCode;stdout=$out.Result.Trim();stderr=$err.Result}
  }
  $wrong=$modules+';'+(Join-Path $env:ProgramFiles 'WindowsPowerShell\Modules')
  $red=Probe $wrong; $green=Probe BUILTIN
  $expected=(Get-FileHash -LiteralPath $sample -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($red.exit_code -eq 0 -or $red.stderr -notmatch 'Get-FileHash') {throw 'foreign-module-path-did-not-fail'}
  if ($green.exit_code -ne 0 -or $green.stdout -cne $expected) {throw 'builtin-module-path-did-not-restore-real-hash'}
  if ($env:PSModulePath -cne $original) {throw 'parent-module-path-changed'}
  @{status='passed';red=$red;green=$green;expected_sha256=$expected;parent_unchanged=$true;module_installs=0} | ConvertTo-Json -Depth 5 -Compress
} finally { if ([IO.Directory]::Exists($owned)) { [IO.Directory]::Delete($owned,$true) } }
""".replace("BUILTIN", "'" + builtin_modules.replace("'", "''") + "'")
        encoded = base64.b64encode(command.encode("utf-16le")).decode("ascii")
        result = subprocess.run(
            [SHELL, "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
            env=outer_env,
            capture_output=True,
            timeout=60,
        )
        evidence = os.environ.get("NOETOME_INSTALLATION_TEST_EVIDENCE")
        if evidence:
            output = Path(evidence)
            output.mkdir(parents=True, exist_ok=True)
            (output / "native-psmodulepath.stdout.log").write_bytes(result.stdout)
            (output / "native-psmodulepath.stderr.log").write_bytes(result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        proof = json.loads(result.stdout)
        self.assertEqual(proof["status"], "passed")
        self.assertNotEqual(proof["red"]["exit_code"], 0)
        self.assertEqual(proof["green"]["exit_code"], 0)
        self.assertEqual(proof["green"]["stdout"], proof["expected_sha256"])
        self.assertTrue(proof["parent_unchanged"])

    def test_existing_app_registration_shortcut_and_unknown_installed_content_are_preserved(
        self,
    ):
        local = self.root / "local"
        local.mkdir()
        target = local / "Programs/Noetome-CI-test"
        old = local / "Programs/Noetome"
        old.mkdir(parents=True)
        sentinel = old / "user.md"
        sentinel.write_bytes(b"manual user content")
        with self.assertRaises(ValueError):
            SMOKE.ensure_clean_install(target, local, [], [old], [])
        self.assertEqual(sentinel.read_bytes(), b"manual user content")
        self.assertFalse(target.exists())
        with self.assertRaises(ValueError):
            SMOKE.ensure_clean_install(
                target, local, [], [], [{"DisplayName": "Noetome"}]
            )
        shortcut = self.root / "Noetome.lnk"
        shortcut.write_bytes(b"existing selected shortcut")
        with self.assertRaises(ValueError):
            SMOKE.ensure_clean_install(target, local, [shortcut], [], [])
        with self.assertRaises(ValueError):
            SMOKE.ensure_clean_install(self.install, local, [], [], [])
        with self.assertRaises(ValueError):
            SMOKE.ensure_clean_install(self.root / "outside", local, [], [], [])
        SMOKE.ensure_clean_install(target, local, [], [], [])

    def test_runner_retains_real_nonzero_exit_and_raw_stderr(self):
        runner = SMOKE.Runner(self.root)
        with self.assertRaisesRegex(ValueError, "actual exit 17"):
            runner.run(
                "red",
                [
                    sys.executable,
                    "-c",
                    "import sys; print('partial stdout'); print('actual stderr',file=sys.stderr); sys.exit(17)",
                ],
                os.environ.copy(),
            )
        receipt = json.loads((self.root / "red.result.json").read_text())
        self.assertEqual(receipt["exit_code"], 17)
        self.assertEqual(receipt["state"], "exited")
        self.assertEqual(
            (self.root / "red.stderr.log").read_bytes().strip(), b"actual stderr"
        )
        self.assertEqual(
            receipt["stdout"], SMOKE.fingerprint(self.root / "red.stdout.log")
        )

    def test_runner_retains_timeout_partial_output_and_missing_process(self):
        runner = SMOKE.Runner(self.root)
        with self.assertRaisesRegex(ValueError, "timed out"):
            runner.run(
                "timeout",
                [
                    sys.executable,
                    "-c",
                    "import time; print('before timeout',flush=True); time.sleep(30)",
                ],
                os.environ.copy(),
                timeout=0.8,
            )
        receipt = json.loads((self.root / "timeout.result.json").read_text())
        self.assertIsNone(receipt["exit_code"])
        self.assertEqual(receipt["state"], "timed_out")
        self.assertEqual(
            (self.root / "timeout.stdout.log").read_bytes().strip(), b"before timeout"
        )
        with self.assertRaisesRegex(ValueError, "could not start"):
            runner.run(
                "missing", [str(self.root / "no-such-program")], os.environ.copy()
            )
        self.assertEqual(runner.commands[-1]["state"], "spawn_failed")

    def test_non_windows_or_non_ci_main_refuses_before_writing(self):
        if os.name == "nt":
            with (
                patch.dict(os.environ, {"CI": "false", "GITHUB_ACTIONS": "false"}),
                self.assertRaises(SystemExit),
            ):
                self.refused_main()
        else:
            with self.assertRaises(SystemExit):
                self.refused_main()
        self.assertFalse((self.root / "evidence").exists())

    def test_main_preserves_a_failed_summary_when_source_fingerprinting_is_unreadable(
        self,
    ):
        fake_os = types.SimpleNamespace(
            name="nt",
            pathsep=os.pathsep,
            environ={
                "CI": "true",
                "GITHUB_ACTIONS": "true",
                "SystemRoot": str(self.root),
                "GITHUB_SHA": "a" * 40,
            },
        )
        with (
            patch.object(SMOKE, "os", fake_os),
            patch.object(
                SMOKE, "fingerprint", side_effect=OSError("source-input-unreadable")
            ),
            patch.object(SMOKE.Runner, "run") as run,
        ):
            result = self.refused_main()
        self.assertEqual(result, 1)
        run.assert_not_called()
        summary = json.loads((self.root / "evidence/summary.json").read_text())
        self.assertEqual(summary["status"], "failed")
        self.assertEqual(summary["uninstall"], "not_run")
        self.assertEqual(summary["commands"], [])
        self.assertIn("source-input-unreadable", summary["failure"])
        self.assertFalse((self.root / "new-install").exists())

    def refused_main(self):
        return SMOKE.main(
            [
                "--installer",
                str(self.root / "installer.exe"),
                "--stage-manifest",
                str(self.root / "manifest.json"),
                "--manifest-sha256",
                SMOKE.sha(self.manifest_bytes),
                "--evidence-dir",
                str(self.root / "evidence"),
                "--install-dir",
                str(self.root / "new-install"),
            ]
        )

    @unittest.skipUnless(SHELL, "Native Windows PowerShell is unavailable")
    def test_native_shortcuts_only_known_three_paths_without_programs_recursion(self):
        def quote(value):
            return "'" + value.replace("'", "''") + "'"

        script = ROOT / "scripts/install_desktop.ps1"
        win_script = (
            str(script)
            if os.name == "nt"
            else subprocess.check_output(
                ["wslpath", "-w", str(script)], text=True
            ).strip()
        )
        command = r"""
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false)
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile(SCRIPT,[ref]$tokens,[ref]$errors)
if ($errors.Count -ne 0) { throw 'production-script-parse-failed' }
$node=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-NoetomeShortcutPaths'},$true)
if ($null -eq $node) { throw 'production-function-missing' }
Invoke-Expression $node.Extent.Text
$owned=Join-Path ([IO.Path]::GetTempPath()) ('noetome-install-shortcuts-'+[Guid]::NewGuid().ToString('N'))
try {
  $programs=Join-Path $owned 'Programs'; $desktop=Join-Path $owned 'Desktop'; $other=Join-Path $programs 'PrivateOther'
  [IO.Directory]::CreateDirectory((Join-Path $programs 'Noetome')) | Out-Null
  [IO.Directory]::CreateDirectory($desktop) | Out-Null
  [IO.Directory]::CreateDirectory($other) | Out-Null
  $expected=@((Join-Path $programs 'Noetome.lnk'),(Join-Path $programs 'Noetome\Noetome.lnk'),(Join-Path $desktop 'Noetome.lnk'))
  foreach($path in $expected) { [IO.File]::WriteAllText($path,'owned selected shortcut') }
  $private=Join-Path $other 'Noetome.lnk'; [IO.File]::WriteAllText($private,'unknown must stay untouched')
  function Get-ChildItem { throw 'enumeration-is-not-authorized' }
  $actual=@(Get-NoetomeShortcutPaths -Programs $programs -Desktop $desktop)
  if ($actual.Count -ne 3 -or @($actual | Where-Object {$_ -notin $expected}).Count -ne 0) {throw 'wrong-shortcut-scope'}
  if ([IO.File]::ReadAllText($private) -cne 'unknown must stay untouched') {throw 'private-changed'}
  [IO.File]::Delete($expected[0])
  if (@(Get-NoetomeShortcutPaths -Programs $programs -Desktop $desktop).Count -ne 2) {throw 'missing-shortcut-not-handled'}
  [Console]::WriteLine('{"status":"passed","selected":3,"after_missing":2,"programs_enumeration":false,"unknown_preserved":true}')
} finally {
  if ([IO.Directory]::Exists($owned)) { [IO.Directory]::Delete($owned,$true) }
}
""".replace("SCRIPT", quote(win_script))
        encoded = base64.b64encode(command.encode("utf-16le")).decode("ascii")
        result = subprocess.run(
            [SHELL, "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
            check=False,
            capture_output=True,
            timeout=30,
        )
        evidence = os.environ.get("NOETOME_INSTALLATION_TEST_EVIDENCE")
        if evidence:
            output = Path(evidence)
            output.mkdir(parents=True, exist_ok=True)
            (output / "native-shortcuts.stdout.log").write_bytes(result.stdout)
            (output / "native-shortcuts.stderr.log").write_bytes(result.stderr)
            (output / "native-shortcuts.result.json").write_text(
                json.dumps(
                    {
                        "exit_code": result.returncode,
                        "production_source": SMOKE.fingerprint(script),
                        "installed_or_uninstalled": False,
                    }
                )
                + "\n",
                encoding="utf-8",
            )
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        self.assertEqual(
            json.loads(result.stdout),
            {
                "status": "passed",
                "selected": 3,
                "after_missing": 2,
                "programs_enumeration": False,
                "unknown_preserved": True,
            },
        )


if __name__ == "__main__":
    unittest.main()
