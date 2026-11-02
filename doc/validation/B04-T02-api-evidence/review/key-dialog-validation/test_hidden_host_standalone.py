"""Explicit native form test with a hidden PowerShell host; synthetic inputs only."""
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests"))
from test_api_key_dialog import ApiKeyDialogTests, PWSH, SCRIPT


class HiddenHostStandaloneTests(ApiKeyDialogTests):
    def test_form_visible_with_hidden_start_process_host(self):
        quote = lambda value: "'" + str(value).replace("'", "''") + "'"
        arguments = ["-NoProfile", "-STA", "-File", str(SCRIPT)]
        for name, value in self.params.items():
            arguments.extend(["-" + name, value])
        # Start-Process joins its argument array. Quote every public non-credential argument.
        arg_file = self.case_root / "host-arguments.json"
        arg_file.write_text(json.dumps(['"' + a.replace('"', '\\"') + '"' for a in arguments]), encoding="utf-8")
        pid_file = self.case_root / "child-pid.txt"
        host = self.case_root / "hidden-host.ps1"
        host.write_text(
            "$ErrorActionPreference='Stop'\n"
            f"$arguments=Get-Content -LiteralPath {quote(arg_file)} -Raw | ConvertFrom-Json\n"
            f"$child=Start-Process -FilePath {quote(PWSH)} -ArgumentList $arguments -WindowStyle Hidden -PassThru\n"
            f"[IO.File]::WriteAllText({quote(pid_file)},[string]$child.Id)\n"
            "$child.WaitForExit(); exit $child.ExitCode\n", encoding="utf-8")
        process = subprocess.Popen([PWSH, "-NoProfile", "-STA", "-File", str(host)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
        user32.EnumChildWindows.argtypes = [wintypes.HWND, callback_type, wintypes.LPARAM]
        user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        user32.IsWindowVisible.argtypes = [wintypes.HWND]
        user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
        user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
        user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        user32.SetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPCWSTR]
        user32.EnableWindow.argtypes = [wintypes.HWND, wintypes.BOOL]
        pid = None
        form = None
        close_timer = None
        rows, password_controls = [], []

        @callback_type
        def enumerate_selected(hwnd, unused):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid:
                name = ctypes.create_unicode_buffer(256)
                user32.GetClassNameW(hwnd, name, len(name))
                rows.append({"handle": int(hwnd), "class_name": name.value,
                             "visible": bool(user32.IsWindowVisible(hwnd))})
            return True

        @callback_type
        def read_password_style(hwnd, unused):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid:
                name = ctypes.create_unicode_buffer(256)
                user32.GetClassNameW(hwnd, name, len(name))
                if "EDIT" in name.value.upper() and user32.GetWindowLongW(hwnd, -16) & 0x20:
                    password_controls.append(int(hwnd))
                    user32.EnableWindow(hwnd, False)
            return True

        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if pid_file.exists():
                    try:
                        pid = int(pid_file.read_text())
                    except ValueError:
                        pass
                if pid is not None:
                    rows.clear()
                    user32.EnumWindows(enumerate_selected, 0)
                    candidates = [row for row in rows if row["class_name"].startswith("WindowsForms10.Window.")]
                    if candidates:
                        form = candidates[0]
                        # The application's Shown event must finish its own explicit display.
                        if form["visible"]:
                            break
                if process.poll() is not None:
                    break
                time.sleep(0.01)
            self.assertIsNotNone(form, "Hidden host never created its own form")
            user32.SetWindowTextW(form["handle"], "Noetome synthetic hidden-host test · auto close")
            user32.EnumChildWindows(form["handle"], read_password_style, 0)
            waiting = self.status()
            close_timer = threading.Timer(0, lambda: user32.PostMessageW(form["handle"], 0x0010, 0, 0))
            close_timer.start()
            stdout, stderr = process.communicate(timeout=10)
            cancelled = self.status()
            proof = {"synthetic_only": True, "exact_script_sta_file": True,
                     "launcher": "Start-Process -WindowStyle Hidden", "pid": pid,
                     "native_form_visible": form["visible"], "password_style": bool(password_controls),
                     "console_visible": any(r["visible"] for r in rows if r["class_name"] == "ConsoleWindowClass"),
                     "waiting_phase": waiting["phase"], "final_phase": cancelled["phase"],
                     "exit_code": process.returncode, "cli_child_started": cancelled["pid"] is not None,
                     "api_called": Path(self.params["OutputPath"]).exists(),
                     "windows": rows}
            proof_path = Path(__file__).resolve().parent / "hidden-host-native-proof.json"
            proof_path.write_text(json.dumps(proof, indent=2), encoding="utf-8")
            self.assertTrue(form["visible"], "Start-Process Hidden hid the application's password form")
            self.assertFalse(proof["console_visible"], "Console host became visible")
            self.assertTrue(password_controls)
            self.assertEqual((waiting["phase"], waiting["pid"]), ("waiting_for_key", None))
            self.assertEqual((cancelled["phase"], cancelled["pid"]), ("cancelled", None))
            self.assertEqual(process.returncode, 2, stdout + stderr)
            self.assertFalse(Path(self.params["OutputPath"]).exists())
        finally:
            if close_timer is not None:
                close_timer.join(timeout=2)
            if process.poll() is None:
                if form is not None:
                    user32.PostMessageW(form["handle"], 0x0010, 0, 0)
                try:
                    process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()  # Only this synthetic no-input host.
                    process.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main(defaultTest="HiddenHostStandaloneTests.test_form_visible_with_hidden_start_process_host", verbosity=2)
