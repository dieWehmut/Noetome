"""Native DPAPI/quota/process boundaries with a synthetic CLI and public recovery history.

The fake executable never performs HTTP. It reports declared fake usage solely
for exercising settlement and process failures; real CLI/HTTP is verified separately.
"""

from __future__ import annotations
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

from tests.test_text_model_profile import BASELINE, PUBLIC_SECRET, SHELL, quote, winpath

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ENV = "NOETOME_TEXT_API_TEST_EVIDENCE"
FAKE_CLI = r"""
using System;
using System.IO;
using System.Diagnostics;
using System.Threading;
public class FakeTextCli {
 public static int Main(string[] args) {
  string root=args[2], mode=File.ReadAllText(Path.Combine(root,"test-mode.txt"));
  File.WriteAllText(Path.Combine(root,"child-started.pid"),Process.GetCurrentProcess().Id.ToString());
  if(mode=="wait") while(!File.Exists(Path.Combine(root,"release"))) Thread.Sleep(10);
  if(mode=="parent_exit_pipe" || mode=="tree_timeout" || mode=="tree_output_limit" || mode=="cancel") {
   var info=new ProcessStartInfo();
   info.FileName=Path.Combine(Environment.GetEnvironmentVariable("SystemRoot"),"System32\\WindowsPowerShell\\v1.0\\powershell.exe");
   string marker=Path.Combine(root,"descendant.pid").Replace("'","''");
   info.Arguments="-NoProfile -Command \"[IO.File]::WriteAllText('"+marker+"',[string]$PID);Start-Sleep -Seconds 180\"";
   info.UseShellExecute=false; info.CreateNoWindow=true;
   Process.Start(info);
   while(!File.Exists(Path.Combine(root,"descendant.pid"))) Thread.Sleep(10);
   if(mode=="parent_exit_pipe") return 0;
   if(mode=="tree_output_limit") { var output=Console.OpenStandardOutput();var bytes=new byte[16777217];output.Write(bytes,0,bytes.Length);output.Flush(); }
   Thread.Sleep(180000);
  }
  bool credential=Environment.GetEnvironmentVariable("NOETOME_TANGMAO_API_KEY")!=null;
  string usage="{\"network_attempts\":1,\"model_usage_status\":\"validated\",\"model_usage\":{\"input_tokens\":10,\"output_tokens\":5,\"total_tokens\":15}}";
  if(mode=="unknown") usage="{\"network_attempts\":1,\"model_usage_status\":\"not_validated\",\"model_usage\":null}";
  if(mode=="zero") usage="{\"network_attempts\":0,\"model_usage_status\":\"not_validated\",\"model_usage\":null}";
  Console.WriteLine("{\"synthetic_cli\":true,\"credential_present\":"+credential.ToString().ToLowerInvariant()+",\"usage\":"+usage+",\"actual_model\":\"public-fake-model\"}");
  if(mode=="malformed") Console.WriteLine("public malformed tail");
  if(mode=="leak") { string key=Environment.GetEnvironmentVariable("NOETOME_TANGMAO_API_KEY");Console.WriteLine(key);Console.Error.WriteLine(key); }
  return mode=="semantic_failure" ? 3 : 0;
 }
}
"""


@unittest.skipUnless(
    SHELL, "Native Windows PowerShell / CurrentUser DPAPI is unavailable"
)
class TextApiCredentialTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        selected = (
            subprocess.check_output(
                [SHELL, "-NoProfile", "-Command", "[IO.Path]::GetTempPath()"],
                timeout=20,
            )
            .decode()
            .strip()
        )
        cls.windows_temp = (
            Path(selected)
            if os.name == "nt"
            else Path(
                subprocess.check_output(["wslpath", "-u", selected], text=True).strip()
            )
        )
        cls.shared = tempfile.TemporaryDirectory(
            prefix="noetome-fake-text-cli-", dir=cls.windows_temp
        )
        cls.binary = Path(cls.shared.name) / "fake-text-cli.exe"
        source = Path(cls.shared.name) / "fake.cs"
        source.write_text(FAKE_CLI, encoding="utf-8", newline="\n")
        code = (
            "Add-Type -TypeDefinition ([IO.File]::ReadAllText("
            + quote(winpath(source))
            + ")) -OutputAssembly "
            + quote(winpath(cls.binary))
            + " -OutputType ConsoleApplication;exit 0"
        )
        result = subprocess.run(
            [SHELL, "-NoProfile", "-NonInteractive", "-Command", code],
            capture_output=True,
            timeout=30,
        )
        if result.returncode or not cls.binary.is_file():
            cls.shared.cleanup()
            raise AssertionError(result.stderr.decode("utf-8", "replace"))
        cls.base = json.loads(BASELINE.read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.shared.cleanup()

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(
            prefix="noetome-text-invoke-", dir=self.windows_temp
        )
        self.work = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(self.preserve)
        for name in ("invoke_text_api_credential.ps1", "text_api_support.ps1"):
            shutil.copyfile(ROOT / "scripts" / name, self.work / name)
        self.helper = self.work / "invoke_text_api_credential.ps1"
        self.credential = self.work / "public.dpapi"
        self.ledger = self.work / "usage.json"
        self.config = self.work / "api.json"
        self.ledger.write_bytes(BASELINE.read_bytes())
        self.captures = 0
        self.configuration = {
            "schema_version": 1,
            "provider": "openai_chat_completions",
            "transport_mode": "live",
            "endpoint": "https://ai.1343263.xyz/v1/chat/completions",
            "model": "deepseek-flash",
            "credential_env": "NOETOME_TANGMAO_API_KEY",
            "required_input": "text",
            "capabilities": {"text": True, "vision": False, "structured_output": True},
            "selection": [{"kind": "text", "ordinal": 0}],
            "limits": {
                "max_units": 64,
                "max_input_bytes": 65536,
                "max_response_bytes": 1048576,
                "max_output_tokens": 8192,
                "timeout_ms": 1000,
            },
            "budget": {
                "max_requests": 1,
                "max_total_tokens": 65536,
                "max_cost_usd": None,
            },
        }
        self.write_config()
        self.mode("valid")
        result = self.capture(
            "$s=ConvertTo-SecureString -AsPlainText -Force "
            + quote(PUBLIC_SECRET)
            + ";try{[IO.File]::WriteAllText("
            + quote(winpath(self.credential))
            + ",(ConvertFrom-SecureString $s),[Text.Encoding]::ASCII)}finally{$s.Dispose()};exit 0"
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def write_config(self):
        self.config.write_text(
            json.dumps(self.configuration), encoding="utf-8", newline="\n"
        )

    def mode(self, value):
        (self.work / "test-mode.txt").write_text(value, encoding="ascii")

    def command(self, cancel=False):
        args = {
            "Binary": self.binary,
            "Project": self.work,
            "ImportId": "00000000-0000-4000-8000-000000000001",
            "Config": self.config,
            "OutputRoot": self.work / "unused-output",
            "CredentialFile": self.credential,
            "UsageLedgerFile": self.ledger,
        }
        if cancel:
            args["CancelFile"] = self.work / "cancel.signal"
        return (
            "& "
            + quote(winpath(self.helper))
            + " "
            + " ".join(
                "-"
                + key
                + " "
                + quote(winpath(value) if isinstance(value, Path) else value)
                for key, value in args.items()
            )
            + ";exit $LASTEXITCODE"
        )

    def spawn(self, code):
        env = os.environ.copy()
        for name in list(env):
            if name.upper() in ("PSMODULEPATH", "NOETOME_TANGMAO_API_KEY"):
                env.pop(name)
        return subprocess.Popen(
            [
                SHELL,
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                code,
            ],
            cwd=self.work,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def finish(self, process, timeout=30):
        try:
            out, err = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            raise
        self.assertNotIn(PUBLIC_SECRET.encode(), out)
        self.assertNotIn(PUBLIC_SECRET.encode(), err)
        (self.work / f"capture-{self.captures}.stdout.log").write_bytes(out)
        (self.work / f"capture-{self.captures}.stderr.log").write_bytes(err)
        (self.work / f"capture-{self.captures}.result.json").write_text(
            json.dumps({"exit_code": process.returncode}),
            encoding="utf-8",
            newline="\n",
        )
        self.captures += 1
        return subprocess.CompletedProcess(
            process.args,
            process.returncode,
            out.decode("utf-8", "strict"),
            err.decode("utf-8", "replace"),
        )

    def capture(self, code):
        return self.finish(self.spawn(code))

    def current(self):
        return json.loads(self.ledger.read_bytes())

    def preserve(self):
        chosen = os.environ.get(EVIDENCE_ENV)
        if not chosen:
            return
        target = Path(chosen) / self._testMethodName
        target.mkdir(parents=True, exist_ok=False)
        for source in self.work.iterdir():
            if not source.is_file() or source.suffix == ".dpapi":
                continue
            data = source.read_bytes()
            self.assertNotIn(PUBLIC_SECRET.encode(), data)
            (target / source.name).write_bytes(data)

    def unchanged_history(self, current):
        self.assertEqual(current["records"][:10], self.base["records"])

    def test_valid_and_semantic_failure_settle_observed_usage_preserving_history(self):
        for mode in ("valid", "semantic_failure"):
            with self.subTest(mode=mode):
                self.mode(mode)
                result = self.capture(self.command())
                self.assertEqual(
                    result.returncode,
                    3 if mode == "semantic_failure" else 0,
                    result.stderr,
                )
                report = json.loads(result.stdout)
                self.assertTrue(report["credential_present"])
                current = self.current()
                self.unchanged_history(current)
                self.assertEqual(current["pending_reserved_tokens"], 0)
                count = len(current["records"]) - 10
                self.assertEqual(current["accounted_tokens"], 33185 + count * 15)
                self.assertEqual(current["model_attempts"], 10 + count)
                self.assertEqual(
                    current["records"][-1]["status"], "settled_valid_usage_text"
                )

    def test_unknown_and_malformed_output_keep_full_reservation(self):
        for mode in ("unknown", "malformed"):
            self.mode(mode)
            result = self.capture(self.command())
            self.assertEqual(result.returncode, 0, result.stderr)
        current = self.current()
        self.unchanged_history(current)
        self.assertEqual(current["accounted_tokens"], 33185)
        self.assertEqual(current["pending_reserved_tokens"], 131072)
        self.assertEqual(current["model_attempts"], 11)
        for record in current["records"][10:]:
            self.assertIsNone(record["confirmed_total_tokens"])

    def test_verified_no_request_refunds_without_erasing_prior_spend(self):
        self.mode("zero")
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 0, result.stderr)
        current = self.current()
        self.unchanged_history(current)
        self.assertEqual(current["accounted_tokens"], 33185)
        self.assertEqual(current["pending_reserved_tokens"], 0)
        self.assertEqual(current["model_attempts"], 10)
        self.assertEqual(current["records"][-1]["status"], "settled_no_request")

    def test_missing_credential_missing_ledger_and_reset_ledger_never_launch(self):
        original = self.ledger.read_bytes()
        self.credential.unlink()
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertEqual(original, self.ledger.read_bytes())
        self.assertFalse((self.work / "child-started.pid").exists())
        # Restore only this test's public synthetic credential.
        self.capture(
            "$s=ConvertTo-SecureString -AsPlainText -Force "
            + quote(PUBLIC_SECRET)
            + ";[IO.File]::WriteAllText("
            + quote(winpath(self.credential))
            + ",(ConvertFrom-SecureString $s));$s.Dispose();exit 0"
        )
        self.ledger.unlink()
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertFalse(self.ledger.exists())
        self.assertFalse((self.work / "child-started.pid").exists())
        reset = copy.deepcopy(self.base)
        reset.update(
            records=[], accounted_tokens=0, model_attempts=0, remaining_tokens=10000000
        )
        self.ledger.write_text(json.dumps(reset), encoding="utf-8", newline="\n")
        before = self.ledger.read_bytes()
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 2)
        self.assertEqual(before, self.ledger.read_bytes())
        self.assertFalse((self.work / "child-started.pid").exists())

    def test_four_phase_receipts_bind_original_bytes_and_settlement(self):
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 0, result.stderr)
        record = self.current()["records"][-1]
        prefix = str(self.ledger) + ".text-" + record["invocation_id"]
        for phase in ("before", "reserved", "launch", "settled"):
            receipt = json.loads(Path(prefix + "." + phase + ".json").read_bytes())
            self.assertEqual(receipt["phase"], phase)
            for key in ("selected_config", "selected_binary", "ledger_snapshot"):
                item = receipt[key]
                path = (
                    Path(item["path"])
                    if os.name == "nt"
                    else Path(
                        subprocess.check_output(
                            ["wslpath", "-u", item["path"]], text=True
                        ).strip()
                    )
                )
                data = path.read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), item["sha256"])
                self.assertEqual(len(data), item["byte_length"])
        self.assertEqual(receipt["result"]["cli_exit_code"], 0)
        self.assertEqual(receipt["settlement"]["confirmed_total_tokens"], 15)

    def test_plaintext_echo_is_omitted_from_capture_and_saved_receipts(self):
        self.mode("leak")
        result = self.capture(self.command())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.lower(), "[redacted]")
        self.assertEqual(result.stderr.lower(), "[redacted]")
        for path in self.work.glob("usage.json.text-*.*"):
            self.assertNotIn(PUBLIC_SECRET.encode(), path.read_bytes())

    def test_parallel_invocation_cannot_reserve_the_same_ledger_twice(self):
        self.mode("wait")
        first = self.spawn(self.command())
        try:
            deadline = time.monotonic() + 10
            while (
                not (self.work / "child-started.pid").exists()
                and first.poll() is None
                and time.monotonic() < deadline
            ):
                time.sleep(0.02)
            self.assertTrue((self.work / "child-started.pid").exists())
            before = self.ledger.read_bytes()
            second = self.capture(self.command())
            self.assertEqual(second.returncode, 2)
            self.assertEqual(before, self.ledger.read_bytes())
        finally:
            (self.work / "release").write_bytes(b"public-test-release")
            result = self.finish(first)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.current()["records"]), 11)

    def tree_case(self, mode, diagnostic):
        self.mode(mode)
        self.configuration["limits"]["timeout_ms"] = 50
        self.write_config()
        process = self.spawn(self.command(cancel=mode == "cancel"))
        ids = []
        try:
            deadline = time.monotonic() + 15
            while (
                not (self.work / "descendant.pid").exists()
                and process.poll() is None
                and time.monotonic() < deadline
            ):
                time.sleep(0.02)
            self.assertTrue(
                (self.work / "descendant.pid").exists(),
                "Real native descendant must acknowledge before failure",
            )
            ids = [
                int((self.work / name).read_text())
                for name in ("child-started.pid", "descendant.pid")
            ]
            if mode == "cancel":
                (self.work / "cancel.signal").write_bytes(b"public-test-cancel")
            result = self.finish(process, 25)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn(diagnostic, result.stderr)
            probe = self.capture(
                "$ids=@("
                + ",".join(map(str, ids))
                + ");foreach($id in $ids){if(Get-Process -Id $id -ErrorAction SilentlyContinue){exit 1}};exit 0"
            )
            self.assertEqual(probe.returncode, 0, probe.stderr)
            current = self.current()
            self.unchanged_history(current)
            self.assertEqual(current["accounted_tokens"], 33185)
            self.assertEqual(current["pending_reserved_tokens"], 65536)
            record = current["records"][-1]
            self.assertEqual(record["status"], "usage_unknown")
            receipt = json.loads(
                Path(
                    str(self.ledger)
                    + ".text-"
                    + record["invocation_id"]
                    + ".failed.json"
                ).read_bytes()
            )
            termination = receipt["failure"]["termination"]
            self.assertTrue(termination["root_exit_confirmed"])
            self.assertTrue(termination["job_empty_confirmed"])
            (self.work / "native-process-check.json").write_text(
                json.dumps(
                    {
                        "synthetic_cli": True,
                        "model_requests": 0,
                        "acknowledged_pids": ids,
                        "all_exited": True,
                        "failure_receipt": receipt["failure"],
                    }
                ),
                encoding="utf-8",
                newline="\n",
            )
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=10)
            for pid in ids:
                self.capture(
                    "$p=Get-Process -Id "
                    + str(pid)
                    + " -ErrorAction SilentlyContinue;if($p){Stop-Process -Id "
                    + str(pid)
                    + " -Force};exit 0"
                )

    def test_root_exit_with_inherited_pipes_is_bounded_and_descendant_terminated(self):
        self.tree_case("parent_exit_pipe", "child-timeout")

    def test_live_root_and_descendant_timeout_preserve_unknown_reservation(self):
        self.tree_case("tree_timeout", "child-timeout")

    def test_output_limit_terminates_native_tree_and_preserves_failure_receipt(self):
        self.tree_case("tree_output_limit", "child-output-limit")

    def test_cancellation_terminates_native_tree_and_keeps_pending_reservation(self):
        self.tree_case("cancel", "child-cancelled")
