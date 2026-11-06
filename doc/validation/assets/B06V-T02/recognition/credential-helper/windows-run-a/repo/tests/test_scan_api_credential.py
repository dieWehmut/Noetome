"""Actual Windows helper boundaries with public fake keys and an isolated fake ledger."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
INVOKER = ROOT / "scripts/invoke_scan_api_credential.ps1"
SETTER = ROOT / "scripts/set_api_credential.ps1"
PUBLIC_KEY = "noetome-test-scan-public-only-20261106"
INPUT_ENV = "NOETOME_SCAN_PUBLIC_TEST_INPUT"


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


class ScanCredentialChecks:
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="noetome-scan-credential-")
        self.addCleanup(self.directory.cleanup)
        self.work = Path(self.directory.name)
        self.credential = self.work / "public-key.dpapi"
        self.config = self.work / "api.json"
        self.ledger = self.work / "usage.json"
        self.configuration = {
            "schema_version": 1, "provider": "openai_chat_completions",
            "transport_mode": "live", "endpoint": "https://ai.1343263.xyz/v1/chat/completions",
            "model": "deepseek-flash", "credential_env": "NOETOME_TANGMAO_API_KEY",
            "required_input": "vision", "capabilities": {"text": True, "vision": True, "structured_output": True},
            "limits": {"max_input_bytes": 1048576, "max_response_bytes": 1048576,
                       "max_output_tokens": 8192, "timeout_ms": 5000},
            "budget": {"max_requests": 1, "max_total_tokens": 65536, "max_cost_usd": None},
        }
        self.write_config()
        self.ledger.write_text(json.dumps({
            "schema_version": 1, "approved_total_tokens": 10000000, "accounted_tokens": 0,
            "pending_reserved_tokens": 0, "remaining_tokens": 10000000,
            "model_attempts": 0, "records": [], "updated_at": "public-test-only",
        }), encoding="utf-8")
        # This executable is Python; the first literal CLI word selects this fake script.
        # It never performs HTTP and never reads the real user ledger or key.
        (self.work / "knowledge").write_text(
            "import json, os, sys, time\nfrom pathlib import Path\n"
            "Path('child-started.json').write_text(json.dumps({'pid':os.getpid()}))\n"
            "while os.environ.get('NOETOME_SCAN_TEST_WAIT')=='1' and not Path('release').exists(): time.sleep(.01)\n"
            "key=os.environ.get('NOETOME_TANGMAO_API_KEY','')\n"
            "mode=os.environ.get('NOETOME_SCAN_TEST_MODE','valid')\n"
            "usage={'network_attempts':1,'model_usage_status':'validated','model_usage':{'input_tokens':10,'output_tokens':5,'total_tokens':15}}\n"
            "if mode=='unknown': usage.update(model_usage_status='not_validated',model_usage=None)\n"
            "if mode=='zero': usage.update(network_attempts=0,model_usage_status='not_validated',model_usage=None)\n"
            "print(json.dumps({'args':sys.argv[1:],'key_matches':key==os.environ['NOETOME_SCAN_PUBLIC_TEST_INPUT'],'api':{'usage':usage}}))\n"
            "if mode=='leak':\n"
            " print(key);print(''.join('\\\\u%04x'%ord(c) for c in key),file=sys.stderr)\n"
            "if mode=='malformed': print('public non-JSON tail')\n"
            "sys.exit(int(os.environ.get('NOETOME_SCAN_TEST_EXIT','0')))\n", encoding="utf-8",
        )
        result = self.run_ps(
            f"& {quote(SETTER)} -CredentialFile {quote(self.credential)} "
            f"-ServiceUrl 'https://example.invalid' -Model 'public-test-only' -FromEnvironment '{INPUT_ENV}'"
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def write_config(self):
        self.config.write_text(json.dumps(self.configuration), encoding="utf-8")

    def environment(self, extra=None):
        env = os.environ.copy()
        for name in list(env):
            if name.upper() == "PSMODULEPATH":
                del env[name]
        env.pop("NOETOME_TANGMAO_API_KEY", None)
        env[INPUT_ENV] = PUBLIC_KEY
        if extra:
            env.update(extra)
        return env

    def command(self, credential=True, ledger=True, binary=None):
        return (
            f"& {quote(INVOKER)} -Binary {quote(binary or sys.executable)} "
            f"-CaseDirectory {quote(self.work / 'case with spaces')} -ApiConfig {quote(self.config)} "
            f"-Profile {quote(self.work / 'profile.json')} -OutputRoot {quote(self.work / 'new vault')} "
            + (f"-CredentialFile {quote(self.credential)} " if credential else "")
            + (f"-UsageLedgerFile {quote(self.ledger)} " if ledger else "")
        )

    def spawn(self, code, extra=None):
        return subprocess.Popen(
            [self.shell, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", code],
            cwd=self.work, env=self.environment(extra), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def run_ps(self, code, extra=None):
        proc = self.spawn(code, extra)
        try:
            out, err = proc.communicate(timeout=35)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()
            raise
        stdout, stderr = out.decode("utf-8", "replace"), err.decode("utf-8", "replace")
        if PUBLIC_KEY in stdout or PUBLIC_KEY in stderr:
            self.fail("public synthetic credential leaked into helper output")
        return subprocess.CompletedProcess(proc.args, proc.returncode, stdout, stderr)

    def current(self):
        return json.loads(self.ledger.read_text(encoding="utf-8-sig"))

    def test_valid_usage_settles_even_when_scan_cli_returns_semantic_failure(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        value = json.loads(result.stdout)
        self.assertTrue(value["key_matches"])
        self.assertEqual(value["args"], ["scan-formula", str(self.work / "case with spaces"),
                                       str(self.config), str(self.work / "profile.json"), str(self.work / "new vault")])
        ledger = self.current()
        self.assertEqual((ledger["accounted_tokens"], ledger["pending_reserved_tokens"], ledger["remaining_tokens"]),
                         (15, 0, 9999985))
        self.assertEqual(len(ledger["records"]), 1)
        self.assertEqual(len(list(self.work.glob("usage.json.scan-before-*.json"))), 1)

    def test_unknown_usage_keeps_reservation(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "unknown", "NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        ledger = self.current()
        self.assertEqual((ledger["accounted_tokens"], ledger["pending_reserved_tokens"], ledger["remaining_tokens"]),
                         (0, 65536, 9934464))
        self.assertIsNone(ledger["records"][0]["confirmed_total_tokens"])

    def test_verified_zero_request_refunds_without_spend(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "zero", "NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        ledger = self.current()
        self.assertEqual((ledger["accounted_tokens"], ledger["pending_reserved_tokens"], ledger["remaining_tokens"]),
                         (0, 0, 10000000))

    def test_missing_current_ledger_never_creates_zero_account(self):
        self.ledger.unlink()
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertIn("ledger-missing", result.stderr)
        self.assertFalse(self.ledger.exists())
        self.assertFalse((self.work / "child-started.json").exists())

    def test_missing_key_never_launches_or_mutates_ledger(self):
        before = self.ledger.read_bytes()
        self.credential.unlink()
        result = self.run_ps(self.command(credential=False))
        self.assertEqual(result.returncode, 2)
        self.assertIn("credential-missing", result.stderr)
        self.assertEqual(self.ledger.read_bytes(), before)
        self.assertFalse((self.work / "child-started.json").exists())

    def test_parent_environment_unchanged_and_existing_env_reference_reused(self):
        code = "$env:NOETOME_TANGMAO_API_KEY=$env:NOETOME_SCAN_PUBLIC_TEST_INPUT; " + self.command(credential=False)
        code += "; $code=$LASTEXITCODE; if ($env:NOETOME_TANGMAO_API_KEY -cne $env:NOETOME_SCAN_PUBLIC_TEST_INPUT) { exit 9 }; exit $code"
        result = self.run_ps(code)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(json.loads(result.stdout)["key_matches"])

    def test_corrupt_ledger_arithmetic_and_fake_quota_rejected_before_launch(self):
        original = self.current()
        for field, value in [("remaining_tokens", 999), ("approved_total_tokens", 20000000), ("schema_version", True)]:
            with self.subTest(field=field):
                data = dict(original)
                data[field] = value
                self.ledger.write_text(json.dumps(data), encoding="utf-8")
                before = self.ledger.read_bytes()
                result = self.run_ps(self.command())
                self.assertEqual(result.returncode, 2)
                self.assertEqual(self.ledger.read_bytes(), before)
                self.assertFalse((self.work / "child-started.json").exists())

    def test_record_totals_cannot_hide_prior_usage(self):
        data = self.current()
        data["records"] = [{"invocation_id": "public-prior", "reservation_tokens": 100,
                            "confirmed_total_tokens": 100}]
        self.ledger.write_text(json.dumps(data), encoding="utf-8")
        before = self.ledger.read_bytes()
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.ledger.read_bytes(), before)

    def test_hardlinked_credential_or_ledger_rejected(self):
        for selected in (self.credential, self.ledger):
            with self.subTest(selected=selected.name):
                link = self.work / "public-hardlink"
                os.link(selected, link)
                before = self.ledger.read_bytes()
                try:
                    result = self.run_ps(self.command())
                    self.assertEqual(result.returncode, 2)
                    self.assertEqual(self.ledger.read_bytes(), before)
                    self.assertFalse((self.work / "child-started.json").exists())
                finally:
                    link.unlink()

    def test_zero_budget_and_invalid_route_never_touch_ledger(self):
        self.configuration["budget"]["max_requests"] = 0
        self.write_config()
        before = self.ledger.read_bytes()
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.ledger.read_bytes(), before)
        self.configuration["endpoint"] = "https://example.invalid/changed-route"
        self.write_config()
        (self.work / "child-started.json").unlink()
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.work / "child-started.json").exists())

    def test_plain_and_unicode_escaped_echo_redacted(self):
        self.configuration["budget"]["max_requests"] = 0
        self.write_config()
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "leak"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[redacted]", result.stdout)
        self.assertIn("[redacted]", result.stderr)
        self.assertNotIn("\\u006e\\u006f\\u0065", result.stderr)

    def test_malformed_child_output_retains_unconfirmed_reservation(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "malformed"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.current()["pending_reserved_tokens"], 65536)

    def test_concurrent_second_invocation_cannot_reserve_same_ledger(self):
        first = self.spawn(self.command(), {"NOETOME_SCAN_TEST_WAIT": "1"})
        try:
            until = time.monotonic() + 15
            marker = self.work / "child-started.json"
            while not marker.exists() and first.poll() is None and time.monotonic() < until:
                time.sleep(0.02)
            self.assertTrue(marker.is_file(), "the actual fake child must start before lock probe")
            self.assertIsNone(first.poll(), "first helper process must still be live")
            pending = self.ledger.read_bytes()
            second = self.run_ps(self.command())
            self.assertEqual(second.returncode, 2)
            self.assertEqual(self.ledger.read_bytes(), pending)
        finally:
            (self.work / "release").write_bytes(b"release-public-test-child")
            try:
                out, err = first.communicate(timeout=15)
            except subprocess.TimeoutExpired:
                first.kill()
                first.communicate()
                raise
        self.assertEqual(first.returncode, 0, err.decode("utf-8", "replace"))
        self.assertEqual(self.current()["accounted_tokens"], 15)


if os.name == "nt":
    shells = []
    for name in ("powershell", "pwsh"):
        shell = shutil.which(name)
        if shell:
            shells.append((name, shell))
    for name, shell in shells:
        globals()["ScanCredentials_" + name] = type(
            "ScanCredentials_" + name, (ScanCredentialChecks, unittest.TestCase), {"shell": shell})
else:
    @unittest.skip("Actual Windows DPAPI and file sharing are tested on the selected Windows runtime")
    class ScanCredentialsUnavailable(unittest.TestCase):
        def test_actual_windows_required(self):
            pass


if __name__ == "__main__":
    unittest.main()
