"""Native Windows DPAPI/profile boundaries using a public synthetic key and new directories.

WSL can exercise the actual Windows PowerShell host; ordinary Linux skips these
checks explicitly. Every credential and ledger stays in this test's Windows TEMP
folder. These checks never contact a model, open real credential files, or show WPF.
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
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "doc/validation/assets/B06V-T02/recognition/vision-live-final/actual-final-ledger.json"
PROOF = ROOT / "doc/validation/assets/B06V-T02/review/continuation-current-ledger.json"
PUBLIC_SECRET = "noetome-test-secret"
EVIDENCE_ENV = "NOETOME_TEXT_PROFILE_TEST_EVIDENCE"
WSL_SHELL = Path("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
SHELL = shutil.which("powershell.exe") if os.name == "nt" else (str(WSL_SHELL) if WSL_SHELL.is_file() and shutil.which("wslpath") else None)


def quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def winpath(path: Path) -> str:
    if os.name == "nt":
        return str(path)
    return subprocess.check_output(["wslpath", "-w", str(path)], text=True).strip()


@unittest.skipUnless(SHELL, "Native Windows PowerShell / CurrentUser DPAPI is unavailable")
class TextModelProfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run([SHELL, "-NoProfile", "-NonInteractive", "-Command", "[IO.Path]::GetTempPath()"], capture_output=True, timeout=20)
        if result.returncode:
            raise unittest.SkipTest("Windows PowerShell could not resolve its isolated TEMP directory")
        selected = result.stdout.decode("utf-8", "strict").strip()
        cls.windows_temp = Path(selected) if os.name == "nt" else Path(subprocess.check_output(["wslpath", "-u", selected], text=True).strip())
        cls.baseline_bytes = BASELINE.read_bytes()
        cls.baseline = json.loads(cls.baseline_bytes)
        if len(cls.baseline_bytes) != 23989 or hashlib.sha256(cls.baseline_bytes).hexdigest() != "ea6b32432cb8bc1c325be48be01ce42f1dde933f2fc35f8c739c9d3fc07d8ffd":
            raise AssertionError("The public recovery fixture bytes changed")

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="noetome-text-profile-", dir=self.windows_temp)
        self.work = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(self.preserve_evidence)
        for name in ("text_model_profile.ps1", "text_api_support.ps1", "set_text_api_credential.ps1", "invoke_text_api_credential.ps1"):
            shutil.copyfile(ROOT / "scripts" / name, self.work / name)
        self.profile = self.work / "text_model_profile.ps1"
        self.credential = self.work / "credentials" / "public.dpapi"
        self.ledger = self.work / "model-services" / "usage.json"
        self.recovery = self.work / "actual-final-ledger.json"
        self.proof = self.work / "continuation-current-ledger.json"
        self.recovery.write_bytes(self.baseline_bytes)
        self.proof.write_bytes(PROOF.read_bytes())
        self.captures = 0

    def preserve_evidence(self):
        chosen = os.environ.get(EVIDENCE_ENV)
        if not chosen:
            return
        destination = Path(chosen) / self._testMethodName
        destination.mkdir(parents=True, exist_ok=False)
        for source in self.work.rglob("*"):
            if not source.is_file() or source.suffix == ".dpapi":
                continue
            data = source.read_bytes()
            if PUBLIC_SECRET.encode() in data:
                self.fail("Synthetic credential appeared in preserved output")
            target = destination / source.relative_to(self.work)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)

    def run_ps(self, *, mode="Status", confirmed=False, credential=None, ledger=None, prefix="", suffix=""):
        args = ["-Mode", mode, "-CredentialFile", winpath(credential or self.credential), "-UsageLedgerFile", winpath(ledger or self.ledger)]
        if mode == "RestoreLedger":
            args += ["-RecoveryLedger", winpath(self.recovery), "-RecoveryProof", winpath(self.proof)]
        elif mode == "Store":
            args += ["-CredentialSetter", winpath(self.work / "set_text_api_credential.ps1")]
        if confirmed:
            args += ["-Confirmed"]
        code = prefix + "& " + quote(winpath(self.profile)) + " " + " ".join(arg if arg.startswith("-") else quote(arg) for arg in args) + "; " + suffix + "exit $LASTEXITCODE"
        return self.capture(code)

    def capture(self, code):
        environment = os.environ.copy()
        for name in list(environment):
            if name.upper() == "PSMODULEPATH":
                del environment[name]
        process = subprocess.run([SHELL, "-NoLogo", "-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-Command", code], env=environment, capture_output=True, timeout=35)
        out, err = process.stdout.decode("utf-8", "strict"), process.stderr.decode("utf-8", "replace")
        if PUBLIC_SECRET in out or PUBLIC_SECRET in err:
            self.fail("Synthetic credential leaked to helper output")
        (self.work / f"capture-{self.captures}.stdout.log").write_bytes(process.stdout)
        (self.work / f"capture-{self.captures}.stderr.log").write_bytes(process.stderr)
        (self.work / f"capture-{self.captures}.result.json").write_text(json.dumps({"exit_code": process.returncode}), encoding="utf-8", newline="\n")
        self.captures += 1
        return subprocess.CompletedProcess(process.args, process.returncode, out, err)

    def make_credential(self):
        self.credential.parent.mkdir(parents=True)
        # A direct test-only DPAPI fixture, not a production hook or real key lookup.
        code = "$secure=ConvertTo-SecureString -AsPlainText -Force " + quote(PUBLIC_SECRET) + "; try { [IO.File]::WriteAllText(" + quote(winpath(self.credential)) + ",(ConvertFrom-SecureString $secure),[Text.Encoding]::ASCII) } finally {$secure.Dispose()}"
        result = self.capture(code)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(PUBLIC_SECRET.encode(), self.credential.read_bytes())

    def write_ledger(self, value=None):
        self.ledger.parent.mkdir(parents=True, exist_ok=True)
        if value is None:
            self.ledger.write_bytes(self.baseline_bytes)
        else:
            self.ledger.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8", newline="\n")

    def status(self, **kwargs):
        result = self.run_ps(**kwargs)
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["endpoint"], "https://ai.1343263.xyz/v1/chat/completions")
        self.assertEqual(payload["model"], "deepseek-flash")
        return payload

    def test_missing_credential_and_ledger_are_unknown_and_do_not_create_files(self):
        payload = self.status()
        self.assertFalse(payload["ready"])
        self.assertEqual((payload["credential_state"], payload["ledger_state"]), ("missing", "missing"))
        for field in ("accounted_tokens", "pending_reserved_tokens", "remaining_tokens", "model_attempts"):
            self.assertIsNone(payload[field])
        self.assertFalse(self.credential.exists())
        self.assertFalse(self.ledger.exists())

    def test_valid_dpapi_and_complete_frozen_history_are_ready_without_mutation(self):
        self.make_credential()
        self.write_ledger()
        before = self.credential.read_bytes(), self.ledger.read_bytes()
        payload = self.status()
        self.assertTrue(payload["ready"])
        self.assertEqual((payload["credential_state"], payload["ledger_state"]), ("ready", "ready"))
        self.assertEqual((payload["accounted_tokens"], payload["pending_reserved_tokens"], payload["remaining_tokens"], payload["model_attempts"]), (33185, 0, 9966815, 10))
        self.assertEqual(before, (self.credential.read_bytes(), self.ledger.read_bytes()))

    def test_corrupt_directory_and_denied_credential_are_invalid(self):
        self.write_ledger()
        self.credential.parent.mkdir(parents=True)
        self.credential.write_bytes(b"invalid public ciphertext")
        before = self.credential.read_bytes()
        payload = self.status()
        self.assertEqual(payload["credential_state"], "invalid")
        self.assertFalse(payload["ready"])
        self.assertEqual(self.credential.read_bytes(), before)
        self.credential.unlink()
        self.credential.mkdir()
        self.assertEqual(self.status()["credential_state"], "invalid")
        self.credential.rmdir()
        self.credential.parent.rmdir()
        self.make_credential()
        held = "$held=[IO.File]::Open(" + quote(winpath(self.credential)) + ",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None); try {"
        payload = self.status(prefix=held, suffix="} finally {$held.Dispose()}; ")
        self.assertEqual(payload["credential_state"], "invalid")
        self.assertFalse(payload["ready"])

    def test_directory_and_denied_ledger_are_invalid_without_overwrite(self):
        self.make_credential()
        self.ledger.mkdir(parents=True)
        payload = self.status()
        self.assertEqual(payload["ledger_state"], "invalid")
        self.assertFalse(payload["ready"])
        self.ledger.rmdir()
        self.write_ledger()
        before = self.ledger.read_bytes()
        held = "$held=[IO.File]::Open(" + quote(winpath(self.ledger)) + ",[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None); try {"
        payload = self.status(prefix=held, suffix="} finally {$held.Dispose()}; ")
        self.assertEqual(payload["ledger_state"], "invalid")
        self.assertFalse(payload["ready"])
        self.assertEqual(self.ledger.read_bytes(), before)

    def test_store_existing_credential_refuses_without_opening_dialog_or_overwriting(self):
        self.credential.parent.mkdir(parents=True)
        self.credential.write_bytes(b"public existing ciphertext sentinel")
        before = self.credential.read_bytes()
        result = self.run_ps(mode="Store")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("operation-refused", result.stderr)
        self.assertEqual(self.credential.read_bytes(), before)
        self.assertFalse(self.ledger.exists())

    def test_restore_requires_confirmation_then_copies_full_bytes_and_existing_refuses(self):
        self.make_credential()
        result = self.run_ps(mode="RestoreLedger")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.ledger.exists())
        result = self.run_ps(mode="RestoreLedger", confirmed=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertTrue(payload["ready"])
        self.assertEqual(self.ledger.read_bytes(), self.baseline_bytes)
        self.assertEqual(json.loads(self.ledger.read_bytes())["records"], self.baseline["records"])
        before = self.ledger.read_bytes()
        result = self.run_ps(mode="RestoreLedger", confirmed=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.ledger.read_bytes(), before)

    def test_tampered_recovery_bytes_cannot_restore_a_ledger(self):
        self.recovery.write_bytes(self.baseline_bytes + b"\n")
        result = self.run_ps(mode="RestoreLedger", confirmed=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.ledger.exists())

    def test_reset_deleted_prefix_changed_history_and_false_attempts_are_invalid(self):
        self.make_credential()
        cases = {}
        value = copy.deepcopy(self.baseline)
        value.update(records=[], accounted_tokens=0, pending_reserved_tokens=0, remaining_tokens=10000000, model_attempts=0)
        cases["zero_reset"] = value
        value = copy.deepcopy(self.baseline)
        deleted = value["records"].pop(0)
        value["accounted_tokens"] -= deleted["confirmed_total_tokens"]
        value["remaining_tokens"] += deleted["confirmed_total_tokens"]
        value["model_attempts"] -= 1
        cases["deleted_prefix"] = value
        value = copy.deepcopy(self.baseline)
        value["records"][0]["reason"] = "changed old history"
        cases["changed_history"] = value
        value = copy.deepcopy(self.baseline)
        value["model_attempts"] = 0
        cases["false_attempts"] = value
        for label, value in cases.items():
            with self.subTest(case=label):
                self.write_ledger(value)
                before = self.ledger.read_bytes()
                payload = self.status()
                self.assertFalse(payload["ready"])
                self.assertEqual(payload["ledger_state"], "invalid")
                self.assertEqual(self.ledger.read_bytes(), before)

    def test_legal_unknown_reserved_no_request_and_text_settlement_keep_accounting(self):
        self.make_credential()
        for status, confirmed, attempts in (("reserved", None, None), ("usage_unknown", None, 1), ("settled_no_request", 0, 0), ("settled_valid_usage_text", 15, 1)):
            with self.subTest(status=status):
                value = copy.deepcopy(self.baseline)
                value["records"].append({"invocation_id": str(uuid.uuid4()), "task_id": "B15-T02", "status": status, "reservation_tokens": 1000, "confirmed_total_tokens": confirmed, "network_attempts": attempts})
                value["accounted_tokens"] += confirmed or 0
                value["pending_reserved_tokens"] = 1000 if confirmed is None else 0
                value["remaining_tokens"] = 10000000 - value["accounted_tokens"] - value["pending_reserved_tokens"]
                value["model_attempts"] += attempts or 0
                self.write_ledger(value)
                before = self.ledger.read_bytes()
                payload = self.status()
                self.assertTrue(payload["ready"])
                self.assertEqual(payload["ledger_state"], "ready")
                self.assertEqual(payload["accounted_tokens"], value["accounted_tokens"])
                self.assertEqual(payload["pending_reserved_tokens"], value["pending_reserved_tokens"])
                self.assertEqual(payload["remaining_tokens"], value["remaining_tokens"])
                self.assertEqual(payload["model_attempts"], value["model_attempts"])
                self.assertEqual(self.ledger.read_bytes(), before)

    def test_empty_duplicate_uuid_and_illegal_record_state_are_invalid(self):
        self.make_credential()
        for label, change in (("empty_uuid", lambda record: record.update(invocation_id="")), ("duplicate_uuid", lambda record: record.update(invocation_id=self.baseline["records"][0]["invocation_id"])), ("illegal_status", lambda record: record.update(status="invented_status")), ("unknown_released", lambda record: record.update(confirmed_total_tokens=0)), ("false_no_request", lambda record: record.update(status="settled_no_request", confirmed_total_tokens=0, network_attempts=1))):
            with self.subTest(case=label):
                value = copy.deepcopy(self.baseline)
                record = {"invocation_id": str(uuid.uuid4()), "task_id": "B15-T02", "status": "usage_unknown", "reservation_tokens": 1000, "confirmed_total_tokens": None, "network_attempts": None}
                change(record)
                value["records"].append(record)
                if record["confirmed_total_tokens"] is None:
                    value["pending_reserved_tokens"] = 1000
                    value["remaining_tokens"] -= 1000
                if record["network_attempts"] is not None:
                    value["model_attempts"] += record["network_attempts"]
                self.write_ledger(value)
                before = self.ledger.read_bytes()
                payload = self.status()
                self.assertFalse(payload["ready"])
                self.assertEqual(payload["ledger_state"], "invalid")
                self.assertEqual(self.ledger.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
