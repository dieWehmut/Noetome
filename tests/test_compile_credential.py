"""Native Windows regression of the synthetic DPAPI compile worker bridge."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(sys.platform == "win32", "requires actual Windows CurrentUser DPAPI and native Jobs")
class CompileCredentialTest(unittest.TestCase):
    def test_synthetic_dpapi_launch_containment_cleanup_and_rejection(self):
        powershell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
        with tempfile.TemporaryDirectory(prefix="noetome-compile-credential-test-") as parent:
            destination = Path(parent) / "fresh-run"
            result = subprocess.run(
                [str(powershell), "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                 str(ROOT / "scripts/check_compile_credential.ps1"), "-Root", str(destination)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90,
            )
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
            report = json.loads(result.stdout.decode("utf-8-sig"))
            self.assertEqual(report["mode"], "native_synthetic_zero_model_requests")
            self.assertEqual(report["passed"], 9)
            self.assertEqual(report["failed"], 0)
            self.assertEqual(len(report["checks"]), 9)
            self.assertTrue(all(check["passed"] for check in report["checks"]))
            self.assertEqual(json.loads((destination / "result.json").read_text()), report)


if __name__ == "__main__":
    unittest.main()
