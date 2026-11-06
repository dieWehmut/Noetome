"""Actual Windows helper boundaries with public fake keys and an isolated fake ledger."""
from __future__ import annotations

import json
import hashlib
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
EVIDENCE_ENV = "NOETOME_SCAN_TEST_EVIDENCE"


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


class ScanCredentialChecks:
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="noetome-scan-credential-")
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(self.preserve_public_evidence)
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
            "sys.stdout.reconfigure(encoding='utf-8',newline='\\n');sys.stderr.reconfigure(encoding='utf-8',newline='\\n')\n"
            "Path('child-started.json').write_text(json.dumps({'pid':os.getpid(),'parent_pid':os.getppid()}))\n"
            "while os.environ.get('NOETOME_SCAN_TEST_WAIT')=='1' and not Path('release').exists(): time.sleep(.01)\n"
            "key=os.environ.get('NOETOME_TANGMAO_API_KEY','')\n"
            "mode=os.environ.get('NOETOME_SCAN_TEST_MODE','valid')\n"
            "usage={'network_attempts':1,'model_usage_status':'validated','model_usage':{'input_tokens':10,'output_tokens':5,'total_tokens':15}}\n"
            "if mode=='unknown': usage.update(model_usage_status='not_validated',model_usage=None)\n"
            "if mode=='zero': usage.update(network_attempts=0,model_usage_status='not_validated',model_usage=None)\n"
            "print(json.dumps({'args':sys.argv[1:],'unicode_probe':'公式 xᵢ = −aᵢ/bᵢ','key_matches':key==os.environ['NOETOME_SCAN_PUBLIC_TEST_INPUT'],'api':{'usage':usage,'actual_model':'noetome-test-observed-model','response_receipt':{'sha256':'a'*64}}},ensure_ascii=False))\n"
            "print('诊断：原始UTF-8字节 ✓',file=sys.stderr)\n"
            "if mode=='leak':\n"
            " print(key);print(''.join('\\\\u%04x'%ord(c) for c in key),file=sys.stderr)\n"
            "if mode in ('nested_leak','mixed_leak','deep_leak'):\n"
            " encoded=''.join('\\\\u%04x'%ord(c) for c in key)\n"
            " if mode=='nested_leak': encoded=json.dumps(encoded)\n"
            " if mode=='mixed_leak': encoded=json.dumps(json.dumps(''.join(c if i%3==0 else '\\\\u%04x'%ord(c) for i,c in enumerate(key))))\n"
            " if mode=='deep_leak':\n"
            "  encoded='\\\\u%04x'%ord(key[0])+key[1:]\n"
            "  for _ in range(int(os.environ.get('NOETOME_SCAN_TEST_ESCAPE_DEPTH','40'))): encoded='\\\\u005c'+encoded[1:]\n"
            " print('回显：'+encoded+' ✓');print('诊断：'+encoded+' xᵢ',file=sys.stderr)\n"
            "if mode=='malformed': print('public non-JSON tail')\n"
            "sys.exit(int(os.environ.get('NOETOME_SCAN_TEST_EXIT','0')))\n", encoding="utf-8",
        )
        result = self.run_ps(
            f"& {quote(SETTER)} -CredentialFile {quote(self.credential)} "
            f"-ServiceUrl 'https://example.invalid' -Model 'public-test-only' -FromEnvironment '{INPUT_ENV}'"
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def preserve_public_evidence(self):
        selected = os.environ.get(EVIDENCE_ENV)
        if not selected:
            return
        destination = Path(selected) / self.__class__.__name__ / self._testMethodName
        destination.mkdir(parents=True, exist_ok=False)
        for source in self.work.iterdir():
            if source.is_file() and source.name != "public-key.dpapi" and source.name != "public-hardlink":
                data = source.read_bytes()
                if PUBLIC_KEY.encode() in data:
                    self.fail("public key unexpectedly persisted in evidence")
                (destination / source.name).write_bytes(data)

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
            [self.shell, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", code + "; exit $LASTEXITCODE"],
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
        number = len(list(self.work.glob("capture-*.stdout.txt")))
        (self.work / f"capture-{number}.stdout.txt").write_bytes(out)
        (self.work / f"capture-{number}.stderr.txt").write_bytes(err)
        (self.work / f"capture-{number}.result.json").write_text(json.dumps({"exit_code":proc.returncode}), encoding="utf-8")
        return subprocess.CompletedProcess(proc.args, proc.returncode, stdout, stderr)

    def current(self):
        return json.loads(self.ledger.read_text(encoding="utf-8-sig"))

    def test_valid_usage_settles_even_when_scan_cli_returns_semantic_failure(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        value = json.loads(result.stdout)
        self.assertTrue(value["key_matches"])
        self.assertEqual(value["unicode_probe"],"公式 xᵢ = −aᵢ/bᵢ")
        self.assertEqual(result.stderr,"诊断：原始UTF-8字节 ✓\n")
        self.assertEqual(value["args"], ["scan-formula", str(self.work / "case with spaces"),
                                       str(self.config), str(self.work / "profile.json"), str(self.work / "new vault")])
        ledger = self.current()
        self.assertEqual((ledger["accounted_tokens"], ledger["pending_reserved_tokens"], ledger["remaining_tokens"]),
                         (15, 0, 9999985))
        self.assertEqual(len(ledger["records"]), 1)
        self.assertEqual(len(list(self.work.glob("usage.json.scan-before-*.json"))), 1)

    def test_actual_four_phase_receipts_bind_selected_bytes_and_usage(self):
        before = self.ledger.read_bytes()
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        record = self.current()["records"][0]
        prefix = str(self.ledger) + ".scan-" + record["invocation_id"]
        receipts = {phase: json.loads(Path(prefix + "." + phase + ".json").read_bytes())
                    for phase in ("before", "reserved", "launch", "settled")}
        ledgers = {}
        for phase, receipt in receipts.items():
            self.assertEqual(receipt["invocation_id"], record["invocation_id"])
            self.assertEqual(receipt["phase"], phase)
            for name, source in (("selected_binary",Path(sys.executable)), ("selected_config",self.config)):
                self.assertEqual(receipt[name]["sha256"], hashlib.sha256(source.read_bytes()).hexdigest())
                self.assertEqual(receipt[name]["byte_length"], source.stat().st_size)
            snapshot = receipt["ledger_snapshot"]
            data = Path(snapshot["path"]).read_bytes()
            self.assertEqual(snapshot["sha256"], hashlib.sha256(data).hexdigest())
            self.assertEqual(snapshot["byte_length"], len(data))
            ledgers[phase] = json.loads(data)
            self.assertEqual(receipt["reservation"], {"policy":"bounded_request_total_cap","max_total_tokens":65536})
        self.assertEqual(Path(receipts["before"]["ledger_snapshot"]["path"]).read_bytes(), before)
        self.assertEqual((ledgers["before"]["accounted_tokens"],ledgers["before"]["pending_reserved_tokens"]), (0,0))
        self.assertEqual((ledgers["reserved"]["remaining_tokens"],ledgers["reserved"]["pending_reserved_tokens"]), (9934464,65536))
        self.assertEqual(ledgers["reserved"], ledgers["launch"])
        child = json.loads((self.work/"child-started.json").read_bytes())
        # A Windows venv's Scripts/python.exe is a launcher for the base interpreter.
        # Bind its actual PID to the running fake interpreter or that interpreter's parent.
        self.assertIn(receipts["launch"]["launch"]["pid"], (child["pid"], child["parent_pid"]))
        final = receipts["settled"]
        self.assertEqual(Path(final["ledger_snapshot"]["path"]).read_bytes(), self.ledger.read_bytes())
        self.assertEqual(final["settlement"], {"record_status":"settled_valid_usage_scan","confirmed_total_tokens":15,"network_attempts":1})
        self.assertEqual(final["result"]["cli_exit_code"],3)
        for name,text in (("stdout", result.stdout), ("stderr",result.stderr)):
            receipt = final["result"][name]
            data = text.encode()
            self.assertEqual(receipt["original_sha256"], hashlib.sha256(data).hexdigest())
            self.assertEqual(receipt["original_byte_length"], len(data))
            self.assertIsNone(receipt["omission_reason"])
            self.assertEqual(Path(receipt["saved_copy"]["path"]).read_bytes(),data)
        self.assertEqual(record["actual_model"],"noetome-test-observed-model")
        self.assertEqual(record["response_sha256"],"a"*64)

    def test_unknown_usage_keeps_reservation(self):
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "unknown", "NOETOME_SCAN_TEST_EXIT": "3"})
        self.assertEqual(result.returncode, 3, result.stderr)
        ledger = self.current()
        self.assertEqual((ledger["accounted_tokens"], ledger["pending_reserved_tokens"], ledger["remaining_tokens"]),
                         (0, 65536, 9934464))
        self.assertIsNone(ledger["records"][0]["confirmed_total_tokens"])

    def test_prior_history_and_unconfirmed_reservation_are_preserved(self):
        prior = {"invocation_id": "public-prior-failure", "reservation_tokens": 65536,
                 "confirmed_total_tokens": 936, "network_attempts": 1,
                 "status": "settled_valid_usage_response_failed", "reason": "public historical failure"}
        data = self.current()
        data.update(accounted_tokens=936, remaining_tokens=9999064, model_attempts=1, records=[prior])
        self.ledger.write_text(json.dumps(data), encoding="utf-8")
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "unknown"})
        self.assertEqual(result.returncode, 0, result.stderr)
        unknown = self.current()
        self.assertEqual(unknown["records"][0], prior)
        self.assertEqual(unknown["pending_reserved_tokens"], 65536)
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 0, result.stderr)
        settled = self.current()
        self.assertEqual(settled["records"][0], prior)
        self.assertEqual(len(settled["records"]), 3)
        self.assertEqual((settled["accounted_tokens"], settled["pending_reserved_tokens"], settled["remaining_tokens"]),
                         (951, 65536, 9933513))

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
        code = "$beforeEncoding=[Console]::OutputEncoding.CodePage; $env:NOETOME_TANGMAO_API_KEY=$env:NOETOME_SCAN_PUBLIC_TEST_INPUT; " + self.command(credential=False)
        code += "; $code=$LASTEXITCODE; if ([Console]::OutputEncoding.CodePage -ne $beforeEncoding) { exit 8 }; if ($env:NOETOME_TANGMAO_API_KEY -cne $env:NOETOME_SCAN_PUBLIC_TEST_INPUT) { exit 9 }; exit $code"
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

    def test_duplicate_and_unicode_escaped_ledger_keys_rejected(self):
        original = self.ledger.read_text(encoding="utf-8")
        for key in ('accounted_tokens', 'accounted_\\u0074okens', 'ACCOUNTED_TOKENS'):
            with self.subTest(key=key):
                value = original[:-1] + ',"' + key + '":0}'
                self.ledger.write_text(value, encoding="utf-8")
                before = self.ledger.read_bytes()
                result = self.run_ps(self.command())
                self.assertEqual(result.returncode, 2)
                self.assertEqual(self.ledger.read_bytes(), before)
                self.assertFalse((self.work / "child-started.json").exists())

    def test_duplicate_configuration_key_rejected_before_credential_use(self):
        value = self.config.read_text(encoding="utf-8")[:-1] + ',"credential_env":"NOETOME_TANGMAO_API_KEY"}'
        self.config.write_text(value, encoding="utf-8")
        before = self.ledger.read_bytes()
        result = self.run_ps(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.ledger.read_bytes(), before)
        self.assertFalse((self.work / "child-started.json").exists())

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
        result = self.run_ps(self.command(), {"NOETOME_SCAN_TEST_MODE": "leak"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[redacted]", result.stdout)
        self.assertIn("[redacted]", result.stderr)
        self.assertNotIn("\\u006e\\u006f\\u0065", result.stderr)
        record = self.current()["records"][0]
        receipt = json.loads(Path(str(self.ledger)+".scan-"+record["invocation_id"]+".settled.json").read_bytes())
        for name in ("stdout","stderr"):
            retained = receipt["result"][name]
            self.assertEqual(retained["omission_reason"],"credential_echo_redacted")
            data = Path(retained["saved_copy"]["path"]).read_bytes()
            self.assertNotIn(PUBLIC_KEY.encode(),data)
            self.assertNotIn(b"\\u006e\\u006f\\u0065",data)
            self.assertIn(b"[redacted]",data)
            self.assertEqual(retained["saved_copy"]["sha256"],hashlib.sha256(data).hexdigest())
        self.assertEqual(self.current()["pending_reserved_tokens"],65536)

    def check_nested_echo_omitted(self, mode, depth=None):
        before_encoding = "$encoding=[Console]::OutputEncoding.CodePage; "
        code = before_encoding + self.command()
        code += "; $code=$LASTEXITCODE; if ([Console]::OutputEncoding.CodePage -ne $encoding) { exit 8 }; exit $code"
        env={"NOETOME_SCAN_TEST_MODE":mode}
        if depth is not None: env["NOETOME_SCAN_TEST_ESCAPE_DEPTH"]=str(depth)
        result = self.run_ps(code, env)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(result.stdout,"[redacted]","Whole nested-echo stdout must be omitted")
        self.assertEqual(result.stderr,"[redacted]","Whole nested-echo stderr must be omitted")
        record=self.current()["records"][-1]
        receipt=json.loads(Path(str(self.ledger)+".scan-"+record["invocation_id"]+".settled.json").read_bytes())
        for name in ("stdout","stderr"):
            stream=receipt["result"][name]
            self.assertEqual(stream["omission_reason"],"credential_echo_redacted")
            self.assertGreater(stream["original_byte_length"],len(b"[redacted]"))
            retained=Path(stream["saved_copy"]["path"]).read_bytes()
            self.assertEqual(retained,b"[redacted]")
            self.assertEqual(stream["saved_copy"]["sha256"],hashlib.sha256(retained).hexdigest())
            self.assertNotEqual(stream["original_sha256"],stream["saved_copy"]["sha256"])
        self.assertIsNone(record["confirmed_total_tokens"])
        self.assertEqual(record["status"],"usage_unknown")

    def test_nested_and_mixed_json_credential_echo_omits_complete_streams(self):
        for mode in ("nested_leak","mixed_leak"):
            with self.subTest(mode=mode): self.check_nested_echo_omitted(mode)
        self.assertEqual(self.current()["pending_reserved_tokens"],131072)

    def test_deep_recursive_credential_escape_fails_closed(self):
        for depth in (31,32,33,40):
            with self.subTest(depth=depth): self.check_nested_echo_omitted("deep_leak",depth)
        self.assertEqual(self.current()["pending_reserved_tokens"],262144)

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
