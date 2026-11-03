"""Windows-only offline credential boundaries; all key values are public synthetic data."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SETTER = ROOT / "scripts/set_api_credential.ps1"
INVOKER = ROOT / "scripts/invoke_api_credential.ps1"
PUBLIC_KEY = "noetome-test-public-synthetic-only-20261103"
INPUT_ENV = "NOETOME_TEST_PUBLIC_INPUT"
CHILD_ENV = "NOETOME_TEST_CHILD_KEY"


def ps_quote(value):
    return "'" + str(value).replace("'", "''") + "'"


class CredentialChecks:
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="noetome-credential-test-")
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.credential = self.work / "credential.dpapi"
        self.config = self.work / "config.json"
        self.config.write_text(json.dumps({"credential_env": CHILD_ENV}), encoding="utf-8")

    def run_ps(self, code, key=PUBLIC_KEY, extra_env=None):
        environment = os.environ.copy()
        # Let each engine establish its own standard module paths; a pwsh parent
        # otherwise gives Windows PowerShell incompatible Security assemblies.
        for name in list(environment):
            if name.upper() == "PSMODULEPATH":
                del environment[name]
        environment[INPUT_ENV] = key
        if extra_env:
            environment.update(extra_env)
        result = subprocess.run(
            [self.shell, "-NoLogo", "-NoProfile", "-NonInteractive", "-STA",
             "-ExecutionPolicy", "Bypass", "-Command", code],
            cwd=self.work, env=environment, capture_output=True, text=True,
            encoding="utf-8", errors="replace", timeout=30,
        )
        # Do not include a possible leaked input in an assertion diagnostic.
        if key and (key in result.stdout or key in result.stderr):
            self.fail("credential value leaked into process output")
        return result

    def save(self, path=None, key=PUBLIC_KEY, options=""):
        return self.run_ps(
            f"& {ps_quote(SETTER)} -CredentialFile {ps_quote(path or self.credential)} "
            f"-ServiceUrl 'https://example.invalid' -Model 'public-test-model' "
            f"-FromEnvironment '{INPUT_ENV}' {options}", key=key,
        )

    def make_child(self):
        # Python treats the existing `extract` entry as its script. No service is used.
        (self.work / "extract").write_text(
            "import json, os, sys\n"
            f"value = os.environ.get('{CHILD_ENV}', '')\n"
            f"print(json.dumps({{'args': sys.argv[1:], 'key_matches': value == os.environ['{INPUT_ENV}']}}))\n"
            "print(value)\nprint(value, file=sys.stderr)\n"
            "sys.exit(int(os.environ.get('NOETOME_TEST_EXIT', '0')))\n",
            encoding="utf-8",
        )

    def invoke_code(self, credential=None, config=None, cancel=False):
        return (
            f"& {ps_quote(INVOKER)} -Binary {ps_quote(sys.executable)} "
            f"-Project {ps_quote(self.work / 'project with spaces')} -ImportId 'public-import' "
            f"-Config {ps_quote(config or self.config)} -OutputRoot {ps_quote(self.work / 'new output')} "
            f"-CredentialFile {ps_quote(credential or self.credential)} "
            + (f"-CancelFile {ps_quote(self.work / 'cancel signal')}" if cancel else "")
        )

    def test_encrypts_and_roundtrips_without_plaintext(self):
        result = self.save()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.credential.is_file(), "setter must create external DPAPI file")
        self.assertNotIn(PUBLIC_KEY.encode(), self.credential.read_bytes())
        result = self.run_ps(
            f"$secure = Get-Content -LiteralPath {ps_quote(self.credential)} -Raw | ConvertTo-SecureString; "
            "$ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure); "
            "try { if ([Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr) -ceq "
            f"$env:{INPUT_ENV}) {{ 'roundtrip-ok' }} else {{ exit 8 }} }} "
            "finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr); $secure.Dispose() }"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("roundtrip-ok", result.stdout)

    def test_existing_file_never_overwritten(self):
        self.credential.write_bytes(b"public-sentinel")
        result = self.save()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("credential-set: path-rejected", result.stderr)
        self.assertEqual(self.credential.read_bytes(), b"public-sentinel")

    def test_repository_relative_ads_and_missing_parent_paths_stop(self):
        for path in (ROOT / "target" / "credential-test.dpapi", Path("relative.dpapi"),
                     str(self.work / "credential.dpapi") + ":stream", self.work / "missing" / "key"):
            with self.subTest(path=str(path)):
                result = self.save(path)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("credential-set: path-rejected", result.stderr)
        self.assertFalse(self.credential.exists())

    def test_invalid_keys_never_create_file(self):
        for key in ("", "has space", "has\ttab", "has\nnewline", "unicode-密钥", "x" * 8193):
            with self.subTest(kind="invalid input", length=len(key)):
                result = self.save(key=key)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("credential-set: input-invalid", result.stderr)
                self.assertFalse(self.credential.exists())

    def test_maximum_ascii_key_is_accepted(self):
        result = self.save(key="x" * 8192)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_setter_rejects_non_origin_and_conflicting_inputs(self):
        for url in ("http://example.invalid", "https://example.invalid/path", "https://user@example.invalid", "https://example.invalid/?q=x"):
            result = self.run_ps(
                f"& {ps_quote(SETTER)} -CredentialFile {ps_quote(self.credential)} "
                f"-ServiceUrl {ps_quote(url)} -Model 'test' -FromEnvironment '{INPUT_ENV}'"
            )
            self.assertNotEqual(result.returncode, 0)
        self.assertNotEqual(self.save(options="-Dialog").returncode, 0)
        self.assertFalse(self.credential.exists())

    def test_junction_ancestor_rejected_by_both_scripts(self):
        real = self.work / "ordinary"
        real.mkdir()
        link = self.work / "junction"
        result = self.run_ps(f"New-Item -ItemType Junction -Path {ps_quote(link)} -Target {ps_quote(real)} | Out-Null")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.addCleanup(lambda: os.rmdir(link) if link.exists() else None)
        result = self.save(link / "key.dpapi")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((real / "key.dpapi").exists())
        self.assertEqual(self.save(real / "key.dpapi").returncode, 0)
        result = self.run_ps(self.invoke_code(credential=link / "key.dpapi"))
        self.assertNotEqual(result.returncode, 0)

    def test_child_gets_explicit_environment_arguments_redaction_and_exit_code(self):
        self.assertEqual(self.save().returncode, 0)
        self.make_child()
        result = self.run_ps(self.invoke_code(cancel=True) + "; exit $LASTEXITCODE", extra_env={"NOETOME_TEST_EXIT": "23"})
        self.assertEqual(result.returncode, 23, result.stderr)
        data = json.loads(result.stdout.splitlines()[0])
        self.assertTrue(data["key_matches"])
        self.assertEqual(data["args"], ["api", str(self.work / "project with spaces"), "public-import",
                                       str(self.config), str(self.work / "new output"), "--cancel-file",
                                       str(self.work / "cancel signal")])
        self.assertIn("[redacted]", result.stdout)
        self.assertIn("[redacted]", result.stderr)
        self.assertFalse((self.work / "new output").exists())

    def test_parent_environment_preserved_when_present_or_absent(self):
        self.assertEqual(self.save().returncode, 0)
        self.make_child()
        for present in (False, True):
            code = (f"$env:{CHILD_ENV} = 'public-old-value'; " if present else f"[Environment]::SetEnvironmentVariable('{CHILD_ENV}', $null, 'Process'); ")
            code += self.invoke_code() + "; $childExit = $LASTEXITCODE; "
            code += (f"if ($env:{CHILD_ENV} -cne 'public-old-value') {{ exit 9 }}; " if present else f"if (-not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable('{CHILD_ENV}', 'Process'))) {{ exit 9 }}; ")
            code += "'restoration-ok'; exit $childExit"
            result = self.run_ps(code)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("restoration-ok", result.stdout)

    def test_invalid_environment_and_corrupt_credential_stop_before_child(self):
        self.assertEqual(self.save().returncode, 0)
        self.make_child()
        for name in ("x", "NOETOME-INVALID", "A", "A" * 65, "1INVALID", None):
            self.config.write_text(json.dumps({"credential_env": name}), encoding="utf-8")
            result = self.run_ps(self.invoke_code())
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("credential-invoke: configuration-invalid", result.stderr)
            self.assertNotIn("key_matches", result.stdout)
        self.config.write_text(json.dumps({"credential_env": CHILD_ENV}), encoding="utf-8")
        self.credential.write_text("public-invalid-dpapi", encoding="ascii")
        result = self.run_ps(self.invoke_code())
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("credential-invoke: credential-invalid", result.stderr)
        self.assertNotIn("key_matches", result.stdout)

    def test_parent_environment_preserved_after_launch_failure(self):
        self.assertEqual(self.save().returncode, 0)
        code = f"$env:{CHILD_ENV}='public-old-value'; "
        code += self.invoke_code().replace(ps_quote(sys.executable), ps_quote(self.work / "missing-cli.exe"))
        code += f"; if ($env:{CHILD_ENV} -cne 'public-old-value') {{ exit 9 }}; 'failure-restoration-ok'"
        result = self.run_ps(code)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("credential-invoke: launch-failed", result.stderr)
        self.assertIn("failure-restoration-ok", result.stdout)

    def test_invoker_rejects_repository_credential_and_directory(self):
        for path in (ROOT / "Cargo.toml", self.work):
            result = self.run_ps(self.invoke_code(credential=path))
            self.assertNotEqual(result.returncode, 0)

    def test_real_wpf_controls_load_without_showing_operator_dialog(self):
        # Extract the factory, then load real WPF and instantiate controls. Never ShowDialog.
        result = self.run_ps(
            f"$tokens=$null; $errors=$null; $ast=[Management.Automation.Language.Parser]::ParseFile({ps_quote(SETTER)}, [ref]$tokens, [ref]$errors); "
            "$factory=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'New-CredentialDialog'}, $true); "
            "if ($null -eq $factory) { exit 7 }; Invoke-Expression $factory.Extent.Text; "
            "$ui=New-CredentialDialog -ServiceUrl 'https://example.invalid' -Model 'public-test-model'; "
            "if ($ui.PasswordBox -isnot [System.Windows.Controls.PasswordBox] -or $ui.Save.IsDefault -or -not $ui.Cancel.IsCancel) { exit 8 }; "
            "if ($ui.Window.Title -notmatch 'Noetome') { exit 9 }; "
            "$ui.Window.Close(); 'wpf-controls-ok'"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("wpf-controls-ok", result.stdout)

    def test_real_wpf_save_handler_rejects_empty_and_whitespace_without_showing(self):
        result = self.run_ps(
            f"$tokens=$null; $errors=$null; $ast=[Management.Automation.Language.Parser]::ParseFile({ps_quote(SETTER)}, [ref]$tokens, [ref]$errors); "
            "foreach ($name in @('New-CredentialDialog','Test-CredentialInput')) { "
            "$f=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name},$true); Invoke-Expression $f.Extent.Text }; "
            "$ui=New-CredentialDialog -ServiceUrl 'https://example.invalid' -Model 'public-test-model'; "
            "$state=@{Secure=$null}; $validateInput=${function:Test-CredentialInput}; "
            "$handler=$ast.Find({param($n) $n -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Extent.Text.StartsWith('$ui.Save.Add_Click(')},$true); "
            "if ($null -eq $handler) { exit 7 }; Invoke-Expression $handler.Extent.Text; "
            "foreach ($inputText in @('', 'public invalid')) { $ui.PasswordBox.Password=$inputText; "
            "$ui.Save.RaiseEvent((New-Object System.Windows.RoutedEventArgs([System.Windows.Controls.Button]::ClickEvent))); "
            "if ($null -ne $state.Secure -or -not $ui.Status.Text) { exit 8 } }; "
            "$ui.PasswordBox.Clear(); $ui.Window.Close(); 'wpf-validation-ok'"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("wpf-validation-ok", result.stdout)


SHELLS = []
if os.name == "nt":
    for name in ("powershell", "pwsh"):
        found = shutil.which(name)
        if found:
            SHELLS.append((name, found))
if SHELLS:
    for name, executable in SHELLS:
        globals()["TestCredentials_" + name] = type(
            "TestCredentials_" + name, (CredentialChecks, unittest.TestCase), {"shell": executable}
        )
else:
    @unittest.skip("Windows PowerShell and CurrentUser DPAPI are required")
    class TestCredentialsUnavailable(CredentialChecks, unittest.TestCase):
        pass


if __name__ == "__main__":
    unittest.main()
