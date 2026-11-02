"""Offline Windows checks with a synthetic credential and synthetic native CLI.

These default tests never open a visible dialog, use a real API, or read user
credentials. Visible standalone checks live only in an explicit .artifacts runner.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "invoke_api_with_key.ps1"
PWSH = shutil.which("pwsh")
CSC = Path(os.environ.get("SystemRoot", "C:/Windows")) / "Microsoft.NET/Framework64/v4.0.30319/csc.exe"
SYNTHETIC = " synthetic-offline-key-本地 "


@unittest.skipUnless(os.name == "nt" and PWSH and CSC.is_file(), "Windows PowerShell 7 and local C# compiler required")
class ApiKeyDialogTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        artifact_root = ROOT / ".artifacts"
        artifact_root.mkdir(exist_ok=True)
        cls.temporary = tempfile.TemporaryDirectory(prefix="synthetic-key-dialog-", dir=artifact_root)
        cls.root = Path(cls.temporary.name)
        cls.exe = cls.root / "synthetic cli.exe"
        source = cls.root / "synthetic.cs"
        source.write_text(r'''
using System;
using System.Collections;
using System.IO;
using System.Text;
using System.Threading;
class SyntheticCli {
  static int Main(string[] args) {
    const string expected = " synthetic-offline-key-本地 ";
    string key = Environment.GetEnvironmentVariable("NOETOME_AI134_KEY");
    bool envOk = key == expected && Environment.GetEnvironmentVariables().Count == 2
      && Environment.GetEnvironmentVariable("SystemRoot") != null;
    bool argsOk = args.Length == 9 && args[0] == "api" && args[1] == "extract"
      && args[7] == "--python" && Array.IndexOf(args, expected) == -1;
    if (args.Length != 9) return 8;
    // Deliberately echo the synthetic key into both pipes: the parent must discard it.
    for (int i = 0; i < 600; i++) { Console.WriteLine(key); Console.Error.WriteLine(key); }
    using (var f = new FileStream(args[6], FileMode.CreateNew)) {
      byte[] report = Encoding.UTF8.GetBytes("{\"synthetic\":true,\"environment_ok\":"
        + envOk.ToString().ToLowerInvariant() + ",\"arguments_ok\":"
        + argsOk.ToString().ToLowerInvariant() + "}");
      f.Write(report, 0, report.Length);
    }
    Thread.Sleep(120);
    return envOk && argsOk ? (args[3] == "nonzero" ? 17 : 0) : 9;
  }
}
''', encoding="utf-8")
        result = subprocess.run([str(CSC), "/nologo", "/target:exe", f"/out:{cls.exe}", str(source)], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError("Synthetic CLI compilation failed: " + result.stdout + result.stderr)
        cls.sha = hashlib.sha256(cls.exe.read_bytes()).hexdigest()

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def setUp(self):
        self.case_root = Path(tempfile.mkdtemp(prefix="case-", dir=self.root))
        self.out_root = self.case_root / "fresh result"
        self.out_root.mkdir()
        self.project = self.case_root / "project"
        self.project.mkdir()
        self.config = self.case_root / "config.json"
        self.auth = self.case_root / "authorization.json"
        self.config.write_text("{}", encoding="utf-8")
        self.auth.write_text("{}", encoding="utf-8")
        self.params = {
            "CliPath": str(self.exe), "ExpectedCliSha256": self.sha,
            "ProjectPath": str(self.project), "VersionId": "synthetic-version",
            "ConfigPath": str(self.config), "AuthorizationPath": str(self.auth),
            "OutputPath": str(self.out_root / "result.json"),
            "PythonPath": str(self.exe), "StatusPath": str(self.out_root / "status.json"),
        }

    def run_ps(self, body, *, params=None, script_path=None):
        case_file = self.case_root / "parameters.json"
        case_file.write_text(json.dumps(params or self.params), encoding="utf-8")
        runner = self.case_root / "offline.ps1"
        quote = lambda value: "'" + str(value).replace("'", "''") + "'"
        runner.write_text(
            "$ErrorActionPreference = 'Stop'\n"
            "$testClock = [Diagnostics.Stopwatch]::StartNew()\n"
            f". {quote(script_path or SCRIPT)}\n"
            f"$params = Get-Content -LiteralPath {quote(case_file)} -Raw | ConvertFrom-Json -AsHashtable\n"
            "$synthetic = ' synthetic-offline-key-本地 '\n"
            + """
function Wait-SyntheticKeyRun {
  param($Session, $Ui = $null)
  while ($Session.Phase -eq 'running') {
    if ($null -ne $Ui) { [Windows.Forms.Application]::DoEvents() }
    else { [void] (Update-NoetomeKeyRun -Session $Session) }
    if ($testClock.ElapsedMilliseconds -ge 35000) { throw 'synthetic_run_deadline' }
    if ($Session.Phase -eq 'running') { Start-Sleep -Milliseconds 10 }
  }
  if ($null -eq $Session.Process -or -not $Session.Process.HasExited -or
      @($Session.Drains | Where-Object { -not $_.IsCompleted }).Count -ne 0) {
    throw 'stopped_before_child_exit_and_drain'
  }
}
"""
            + body, encoding="utf-8")
        result = subprocess.run([PWSH, "-NoProfile", "-STA", "-File", str(runner)], capture_output=True, text=True, encoding="utf-8", timeout=40)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn(SYNTHETIC, result.stdout + result.stderr)
        return result.stdout.strip()

    def status(self):
        data = json.loads(Path(self.params["StatusPath"]).read_text(encoding="utf-8"))
        self.assertEqual(set(data), {"schema_version", "phase", "pid", "exit_code", "error_code"})
        self.assertEqual(data["schema_version"], 1)
        return data

    def test_child_env_arguments_drain_and_single_call(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
try {
  Start-NoetomeKeyRun -Session $s -Credential $synthetic
  try { Start-NoetomeKeyRun -Session $s -Credential $synthetic; throw 'second_call_accepted' }
  catch { if ($_.Exception.Message -ne 'already_attempted') { throw } }
  Wait-SyntheticKeyRun -Session $s
  if ($s.StatusFailed -or $s.Phase -ne 'finished') { throw 'terminal_status_not_published' }
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(json.loads(Path(self.params["OutputPath"]).read_text()), {
            "synthetic": True, "environment_ok": True, "arguments_ok": True})
        status = self.status()
        self.assertEqual(status["phase"], "finished")
        self.assertEqual(status["exit_code"], 0)
        self.assertIsInstance(status["pid"], int)
        self.assertIsNone(status["error_code"])
        for path in self.out_root.iterdir():
            self.assertNotIn(SYNTHETIC.encode(), path.read_bytes())

    def test_nonzero_is_fixed_status_without_child_body(self):
        self.params["VersionId"] = "nonzero"
        self.run_ps("""
$s = New-NoetomeKeySession @params
try { Start-NoetomeKeyRun -Session $s -Credential $synthetic
  Wait-SyntheticKeyRun -Session $s
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status()["exit_code"], 17)
        self.assertEqual(self.status()["error_code"], "cli_failed")

    def test_bad_hash_before_form_and_zero_child(self):
        self.params["ExpectedCliSha256"] = "0" * 64
        self.run_ps("try { $s = New-NoetomeKeySession @params; throw 'bad_hash_accepted' } catch { if ($_.Exception.Message -ne 'cli_hash_mismatch') { throw } }")
        self.assertFalse(Path(self.params["OutputPath"]).exists())
        self.assertEqual(self.status()["phase"], "failed")
        self.assertEqual(self.status()["error_code"], "preflight_failed")

    def test_non_pe_even_with_matching_hash_rejected(self):
        path = self.case_root / "non-pe.exe"
        path.write_bytes(b"synthetic not executable")
        self.params["CliPath"] = str(path)
        self.params["ExpectedCliSha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.run_ps("try { $s = New-NoetomeKeySession @params; throw 'non_pe_accepted' } catch { if ($_.Exception.Message -ne 'cli_not_pe') { throw } }")

    def test_existing_status_and_output_not_overwritten(self):
        for name in ("StatusPath", "OutputPath"):
            with self.subTest(name=name):
                path = Path(self.params[name])
                path.write_bytes(b"protected")
                self.run_ps("try { $s = New-NoetomeKeySession @params; throw 'existing_accepted' } catch { if ($_.Exception.Message -ne 'result_root_not_fresh') { throw } }")
                self.assertEqual(path.read_bytes(), b"protected")
                path.unlink()

    def test_output_created_while_waiting_is_preserved_and_zero_child(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
try {
  [IO.File]::WriteAllText($params.OutputPath, 'protected')
  try { Start-NoetomeKeyRun -Session $s -Credential $synthetic; throw 'late_output_accepted' }
  catch { if ($_.Exception.Message -ne 'output_exists') { throw } }
  if ($null -ne $s.Process) { throw 'child_started' }
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(Path(self.params["OutputPath"]).read_text(), "protected")
        self.assertEqual(self.status()["phase"], "failed")
        self.assertEqual(self.status()["error_code"], "preflight_failed")

    def test_verified_cli_cannot_be_changed_while_waiting(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
try {
  try { $f = [IO.File]::Open($params.CliPath, 'Open', 'Write', 'ReadWrite'); $f.Dispose(); throw 'cli_write_accepted' }
  catch { if ($_.Exception.Message -eq 'cli_write_accepted') { throw } }
  Stop-NoetomeKeyBeforeStart -Session $s
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(hashlib.sha256(self.exe.read_bytes()).hexdigest(), self.sha)

    def test_standalone_shared_cli_refused_before_any_form_or_cli_child(self):
        shared = self.case_root / "synthetic shared cli.exe"
        os.link(self.exe, shared)
        self.assertEqual(os.stat(shared).st_nlink, 2)
        params = dict(self.params, CliPath=str(shared))
        command = [PWSH, "-NoProfile", "-STA", "-File", str(SCRIPT)]
        for name, value in params.items():
            command.extend(["-" + name, value])
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=15,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertEqual(result.stdout.strip(), "key_dialog_failed")
            self.assertEqual(result.stderr, "")
            status = self.status()
            self.assertEqual(status["phase"], "failed")
            self.assertEqual(status["error_code"], "preflight_failed")
            self.assertIsNone(status["pid"])
            self.assertFalse(Path(self.params["OutputPath"]).exists())
        finally:
            shared.unlink()

    def test_replace_sharing_conflict_preserves_owned_identity(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$reader = [IO.File]::Open($s.Paths.StatusPath, 'Open', 'Read', 'Read')
try {
  try { $s.Status.Write('running', $null, $null, $null); throw 'sharing_lock_ignored' }
  catch { if ($_.Exception.Message -eq 'sharing_lock_ignored') { throw } }
} finally { $reader.Dispose() }
try {
  $s.Status.AssertOwned()
  Stop-NoetomeKeyBeforeStart -Session $s
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status()["phase"], "cancelled")

        # Deterministic fault at the native Replace boundary in a test-only source snapshot.
        # Windows can return 1175 instead of 32 when it cannot remove the replaced file.
        source = SCRIPT.read_text(encoding="utf-8")
        marker = "File.Replace(temporary, path, null);"
        self.assertEqual(source.count(marker), 1, "Cannot inject the reviewed IO boundary")
        for name in ("retained-owner", "foreign-owner", "foreign-temporary"):
            with self.subTest(case=name):
                directory = self.case_root / name
                directory.mkdir()
                params = dict(self.params, StatusPath=str(directory / "status.json"),
                              OutputPath=str(directory / "result.json"))
                replacement = ""
                if name == "foreign-owner":
                    replacement = 'File.Delete(path); File.WriteAllText(path, "protected");'
                elif name == "foreign-temporary":
                    replacement = 'File.Delete(temporary); File.WriteAllText(temporary, "protected");'
                injection = (
                    "if (Interlocked.Increment(ref syntheticReplaceFailures) == 1) { "
                    + replacement
                    + ' throw new IOException("synthetic_replace_1175", unchecked((int)0x80070497)); } '
                    + marker
                )
                snapshot = source.replace("private Identity.Info owned;",
                                          "private Identity.Info owned; private int syntheticReplaceFailures;", 1)
                snapshot = snapshot.replace(marker, injection, 1)
                script = self.case_root / (name + ".synthetic.ps1")
                script.write_text(snapshot, encoding="utf-8")
                body = """
$s = New-NoetomeKeySession @params
try {
  Stop-NoetomeKeyBeforeStart -Session $s
  if ($s.Phase -ne 'cancelled' -or $null -ne $s.Process) { throw 'cancel_retry_restarted_child' }
} finally { Close-NoetomeKeySession -Session $s }
"""
                if name == "foreign-owner":
                    body = """
$s = New-NoetomeKeySession @params
try {
  try { Stop-NoetomeKeyBeforeStart -Session $s; throw 'foreign_owner_accepted' }
  catch { if ($_.Exception.Message -ne 'status_ownership_lost') { throw } }
  if ([IO.File]::ReadAllText($params.StatusPath) -ne 'protected') { throw 'foreign_owner_changed' }
  if ($null -ne $s.Process) { throw 'child_started' }
} finally { Close-NoetomeKeySession -Session $s }
"""
                elif name == "foreign-temporary":
                    body = """
$s = New-NoetomeKeySession @params
try {
  try { Stop-NoetomeKeyBeforeStart -Session $s; throw 'foreign_temporary_accepted' }
  catch { if ($_.Exception.Message -ne 'status_ownership_lost') { throw } }
  $entries = @([IO.Directory]::EnumerateFiles([IO.Path]::GetDirectoryName($params.StatusPath), '.noetome-key-status-*'))
  if ($entries.Count -ne 1 -or [IO.File]::ReadAllText($entries[0]) -ne 'protected') { throw 'foreign_temporary_deleted_or_changed' }
  if ($null -ne $s.Process) { throw 'child_started' }
} finally { Close-NoetomeKeySession -Session $s }
"""
                self.run_ps(body, params=params, script_path=script)
                if name == "retained-owner":
                    self.assertEqual(json.loads(Path(params["StatusPath"]).read_text())["phase"], "cancelled")

    def test_terminal_status_waits_for_reader_without_claiming_finished(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$reader = $null
try {
  Start-NoetomeKeyRun -Session $s -Credential $synthetic
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while (-not $s.Process.HasExited -or @($s.Drains | Where-Object { -not $_.IsCompleted }).Count -ne 0) {
    if ($clock.ElapsedMilliseconds -gt 5000) { throw 'native_child_deadline' }
    Start-Sleep -Milliseconds 10
  }
  $reader = [IO.File]::Open($s.Paths.StatusPath, 'Open', 'Read', 'Read')
  if (Update-NoetomeKeyRun -Session $s) { throw 'finished_before_publish' }
  if ($s.Phase -ne 'running' -or $s.StatusFailed) { throw 'sharing_conflict_became_terminal' }
  $reader.Dispose(); $reader = $null
  while (-not (Update-NoetomeKeyRun -Session $s)) {
    if ($clock.ElapsedMilliseconds -gt 5000) { throw 'status_publish_deadline' }
    Start-Sleep -Milliseconds 10
  }
  if ($s.StatusFailed -or $s.Phase -ne 'finished') { throw 'terminal_status_not_published' }
} finally { if ($null -ne $reader) { $reader.Dispose() }; Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status()["phase"], "finished")

    def test_gui_timer_publishes_after_reader_releases_without_restarting_child(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$ui = New-NoetomeKeyForm -Session $s
$reader = $null
$script:syntheticTicks = 0
$ui.Timer.Add_Tick({ $script:syntheticTicks += 1 })
try {
  $ui.KeyBox.Text = $synthetic
  $flags = [Reflection.BindingFlags]'Instance,NonPublic'
  [void] $ui.RunButton.GetType().GetMethod('OnClick', $flags).Invoke($ui.RunButton, @([EventArgs]::Empty))
  $firstPid = $s.Process.Id
  $reader = [IO.File]::Open($s.Paths.StatusPath, 'Open', 'Read', 'Read')
  while ($script:syntheticTicks -lt 2 -or -not $s.Process.HasExited -or
      @($s.Drains | Where-Object { -not $_.IsCompleted }).Count -ne 0) {
    [Windows.Forms.Application]::DoEvents()
    if ($testClock.ElapsedMilliseconds -ge 35000) { throw 'synthetic_timer_deadline' }
    Start-Sleep -Milliseconds 10
  }
  if ($s.Phase -ne 'running' -or $s.StatusFailed -or $ui.CancelButton.Enabled) { throw 'timer_finished_while_status_locked' }
  $reader.Dispose(); $reader = $null
  Wait-SyntheticKeyRun -Session $s -Ui $ui
  if ($s.Phase -ne 'finished' -or $s.StatusFailed -or $s.Process.Id -ne $firstPid) { throw 'timer_failed_to_publish_same_child' }
  if ($ui.RunButton.Enabled -or -not $ui.CancelButton.Enabled -or $ui.Form.Visible) { throw 'timer_ui_state_invalid' }
} finally {
  if ($null -ne $reader) { $reader.Dispose() }
  $ui.Timer.Dispose(); $ui.Form.Dispose(); Close-NoetomeKeySession -Session $s
}
""")
        self.assertEqual(self.status()["phase"], "finished")
        self.assertEqual(self.status()["exit_code"], 0)

    def test_permanent_reader_lock_is_explicit_failure_after_local_publish_deadline(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$reader = $null
try {
  Start-NoetomeKeyRun -Session $s -Credential $synthetic
  while (-not $s.Process.HasExited -or @($s.Drains | Where-Object { -not $_.IsCompleted }).Count -ne 0) {
    if ($testClock.ElapsedMilliseconds -ge 35000) { throw 'synthetic_child_deadline' }
    Start-Sleep -Milliseconds 10
  }
  $firstPid = $s.Process.Id
  $reader = [IO.File]::Open($s.Paths.StatusPath, 'Open', 'Read', 'Read')
  Wait-SyntheticKeyRun -Session $s
  if ($s.Phase -ne 'failed' -or -not $s.StatusFailed -or $s.ErrorCode -ne 'status_publish_failed') { throw 'publish_failure_was_hidden' }
  if ($s.StatusPublishClock.ElapsedMilliseconds -lt 2000 -or $s.Process.Id -ne $firstPid) { throw 'wrong_publish_deadline_or_restarted_child' }
  $reader.Dispose(); $reader = $null
} finally { if ($null -ne $reader) { $reader.Dispose() }; Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status()["phase"], "failed")
        self.assertEqual(self.status()["error_code"], "status_publish_failed")
        self.assertEqual(self.status()["exit_code"], 0)

    def test_cancel_before_start_preserves_parent_environment(self):
        self.run_ps("""
[Environment]::SetEnvironmentVariable('NOETOME_AI134_KEY', 'synthetic-parent-sentinel', 'Process')
$s = New-NoetomeKeySession @params
try { Stop-NoetomeKeyBeforeStart -Session $s
  if ([Environment]::GetEnvironmentVariable('NOETOME_AI134_KEY','Process') -ne 'synthetic-parent-sentinel') { throw 'parent_changed' }
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status(), {"schema_version": 1, "phase": "cancelled", "pid": None, "exit_code": None, "error_code": None})
        self.assertFalse(Path(self.params["OutputPath"]).exists())

    def test_invalid_keys_never_start_child(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
try {
  foreach ($key in @('', ('a' * 4097), ('本' * 1366), "a`0b", "a`nb", "a`rb")) {
    try { Start-NoetomeKeyRun -Session $s -Credential $key; throw 'invalid_key_accepted' }
    catch { if ($_.Exception.Message -ne 'invalid_key') { throw } }
  }
  Stop-NoetomeKeyBeforeStart -Session $s
} finally { Close-NoetomeKeySession -Session $s }
""")
        self.assertFalse(Path(self.params["OutputPath"]).exists())
        self.assertEqual(self.status()["phase"], "cancelled")

    def test_password_control_and_running_close_guard_without_showing(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$ui = New-NoetomeKeyForm -Session $s
try {
  if (-not $ui.KeyBox.UseSystemPasswordChar) { throw 'unmasked_control' }
  if ($ui.Form.Visible) { throw 'form_became_visible' }
  Start-NoetomeKeyRun -Session $s -Credential $synthetic
  if (Test-NoetomeKeyCanClose -Session $s) { throw 'running_can_close' }
  Wait-SyntheticKeyRun -Session $s
  if (-not (Test-NoetomeKeyCanClose -Session $s)) { throw 'finished_cannot_close' }
} finally { $ui.Timer.Dispose(); $ui.Form.Dispose(); Close-NoetomeKeySession -Session $s }
""")

    def test_button_events_clear_input_disable_retry_and_block_close(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
$ui = New-NoetomeKeyForm -Session $s
$flags = [Reflection.BindingFlags]'Instance,NonPublic'
try {
  $ui.KeyBox.Text = $synthetic
  $click = $ui.RunButton.GetType().GetMethod('OnClick', $flags)
  [void] $click.Invoke($ui.RunButton, @([EventArgs]::Empty))
  if ($s.Phase -ne 'running' -or $ui.KeyBox.Text -ne '' -or $ui.RunButton.Enabled -or $ui.CancelButton.Enabled) { throw 'run_event_failed' }
  [void] $click.Invoke($ui.RunButton, @([EventArgs]::Empty))
  $closing = [Windows.Forms.FormClosingEventArgs]::new([Windows.Forms.CloseReason]::UserClosing, $false)
  [void] $ui.Form.GetType().GetMethod('OnFormClosing', $flags).Invoke($ui.Form, @($closing))
  if (-not $closing.Cancel) { throw 'close_event_did_not_block' }
  Wait-SyntheticKeyRun -Session $s -Ui $ui
  if ($s.Process.StartInfo.Environment.ContainsKey('NOETOME_AI134_KEY')) { throw 'startinfo_retained_key' }
  if ($ui.Form.Visible) { throw 'form_became_visible' }
} finally { $ui.Timer.Dispose(); $ui.Form.Dispose(); Close-NoetomeKeySession -Session $s }
""")
        self.assertEqual(self.status()["exit_code"], 0)

    def test_status_replaced_by_hardlink_is_rejected(self):
        self.run_ps("""
$s = New-NoetomeKeySession @params
try {
  $outside = Join-Path $params.ProjectPath 'sentinel'
  [IO.File]::WriteAllText($outside, 'protected')
  Remove-Item -LiteralPath $params.StatusPath
  New-Item -ItemType HardLink -Path $params.StatusPath -Target $outside | Out-Null
  try { Stop-NoetomeKeyBeforeStart -Session $s; throw 'replacement_accepted' }
  catch { if ($_.Exception.Message -ne 'status_ownership_lost') { throw } }
  if ([IO.File]::ReadAllText($outside) -ne 'protected') { throw 'sentinel_changed' }
} finally { Close-NoetomeKeySession -Session $s }
""")


    def test_junction_parent_rejected(self):
        junction = self.case_root / "junction"
        self.params["StatusPath"] = str(junction / "status.json")
        self.params["OutputPath"] = str(junction / "result.json")
        target = str(self.out_root).replace("'", "''")
        link = str(junction).replace("'", "''")
        self.run_ps(f"New-Item -ItemType Junction -Path '{link}' -Target '{target}' | Out-Null\ntry {{ $s = New-NoetomeKeySession @params; throw 'junction_accepted' }} catch {{ if ($_.Exception.Message -ne 'unsafe_path') {{ throw }} }}")
        self.assertEqual(list(self.out_root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
