"""Explicit standalone native-window test; synthetic inputs only, never default discovery."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests"))
from test_api_key_dialog import ApiKeyDialogTests, PWSH, SCRIPT, SYNTHETIC

class VisibleStandaloneTests(ApiKeyDialogTests):
    def test_reviewed_pin_standalone_visible_and_cancelled(self):
        """Exercise the exact -File entry, targeted native visibility, and timer close."""
        shared = self.case_root / "synthetic shared cli.exe"
        os.link(self.exe, shared)
        params = dict(self.params, CliPath=str(shared))
        command = [PWSH, "-NoProfile", "-STA", "-File", str(SCRIPT)]
        for name, value in params.items():
            command.extend(["-" + name, value])
        try:
            refused = subprocess.run(command, capture_output=True, text=True, timeout=15,
                                     creationflags=subprocess.CREATE_NO_WINDOW)
            self.assertEqual(refused.returncode, 1, refused.stdout + refused.stderr)
            self.assertEqual(self.status()["phase"], "failed")
            self.assertEqual(self.status()["error_code"], "preflight_failed")
            self.assertFalse(Path(self.params["OutputPath"]).exists())
        finally:
            shared.unlink()

        # Preserve the refused root. This is a new independent result directory and pin.
        pin = self.case_root / "synthetic pinned cli.exe"
        with pin.open("xb") as handle:
            handle.write(self.exe.read_bytes())
        self.assertEqual(os.stat(pin).st_nlink, 1)
        fresh = self.case_root / "standalone fresh result"
        fresh.mkdir()
        params = dict(self.params, CliPath=str(pin), StatusPath=str(fresh / "status.json"),
                      OutputPath=str(fresh / "result.json"))
        command = [PWSH, "-NoProfile", "-STA", "-File", str(SCRIPT)]
        for name, value in params.items():
            command.extend(["-" + name, value])

        user32 = ctypes.WinDLL("user32", use_last_error=True)
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
        user32.EnumChildWindows.argtypes = [wintypes.HWND, callback_type, wintypes.LPARAM]
        user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        user32.IsWindowVisible.argtypes = [wintypes.HWND]
        user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
        user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
        user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        user32.SetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPCWSTR]
        user32.EnableWindow.argtypes = [wintypes.HWND, wintypes.BOOL]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   text=True, creationflags=subprocess.CREATE_NO_WINDOW)
        close_timer = None
        visible = []
        password_controls = []

        @callback_type
        def find_window(hwnd, unused):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            # Never read another app's window title or controls.
            if owner.value == process.pid and user32.IsWindowVisible(hwnd):
                visible.append(hwnd)
            return True

        @callback_type
        def find_password(hwnd, unused):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == process.pid:
                name = ctypes.create_unicode_buffer(256)
                user32.GetClassNameW(hwnd, name, len(name))
                if "EDIT" in name.value.upper() and user32.GetWindowLongW(hwnd, -16) & 0x20:
                    password_controls.append(hwnd)
                    user32.EnableWindow(hwnd, False)
            return True

        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                user32.EnumWindows(find_window, 0)
                if visible or process.poll() is not None:
                    break
                time.sleep(0.01)
            if not visible:
                stdout, stderr = process.communicate(timeout=5)
                self.fail("Standalone entry did not show a form: " + stdout + stderr)
            window = visible[0]
            user32.SetWindowTextW(window, "Noetome synthetic offline window test · auto close")
            user32.EnumChildWindows(window, find_password, 0)
            self.assertTrue(password_controls, "Visible standalone form has no native password edit")
            waiting = json.loads(Path(params["StatusPath"]).read_text(encoding="utf-8"))
            self.assertEqual(waiting["phase"], "waiting_for_key")
            self.assertIsNone(waiting["pid"])
            close_timer = threading.Timer(0, lambda: user32.PostMessageW(window, 0x0010, 0, 0))
            close_timer.start()
            stdout, stderr = process.communicate(timeout=10)
            self.assertNotIn(SYNTHETIC, stdout + stderr)
            self.assertEqual(process.returncode, 2, stdout + stderr)
            cancelled = json.loads(Path(params["StatusPath"]).read_text(encoding="utf-8"))
            self.assertEqual(cancelled, {"schema_version": 1, "phase": "cancelled", "pid": None,
                                         "exit_code": None, "error_code": None})
            self.assertFalse(Path(params["OutputPath"]).exists())
            proof = {"synthetic_only": True, "exact_script_file_entry": True,
                     "cli_shared_then_independently_copied": True,
                     "native_visible": bool(visible), "native_password_edit": bool(password_controls),
                     "waiting_phase": waiting["phase"], "cancelled_phase": cancelled["phase"],
                     "exit_code": process.returncode, "cli_child_started": False,
                     "api_called": False, "real_credential_read": False}
            (Path(__file__).resolve().parent / "standalone-ui-proof.json").write_text(
                json.dumps(proof, indent=2), encoding="utf-8")
        finally:
            if close_timer is not None:
                close_timer.join(timeout=2)
            if process.poll() is None:
                process.kill()  # Only this synthetic cancel-only launcher; no CLI child was started.
                process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main(defaultTest="VisibleStandaloneTests.test_reviewed_pin_standalone_visible_and_cancelled", verbosity=2)
