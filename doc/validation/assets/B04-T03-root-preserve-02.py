"""Append native completion and final CLI receipts; retain all earlier failures."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import socket
import zipfile

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
FINAL = REPO / "target/compiler-temp/B04-T03-root-final-original06-01"
ARCHIVE = ASSETS / "B04-T03-root-evidence-02.zip"
MANIFEST = ASSETS / "B04-T03-root-evidence-02-manifest.json"
def fp(data): return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}
def load(path): return json.loads(path.read_bytes())
assert not ARCHIVE.exists() and not MANIFEST.exists()
first = load(ASSETS / "B04-T03-root-evidence-01-manifest.json")
assert fp((ASSETS / "B04-T03-root-evidence-01.zip").read_bytes()) == {k: first["archive"][k] for k in ["sha256", "byte_length"]}
for name, receipt in first["readable_copies"].items(): assert fp((ASSETS / name).read_bytes()) == {k: receipt[k] for k in ["sha256", "byte_length"]}
original = load(FINAL / "audit.json")
assert original["passed"] and original["ordinary_cli_calls"] == 1 and original["verification"]["independently_passed"]
assert original["original_input_facts_before"] == original["original_input_facts_after"] == load(SDK / "root-inputs-before.json")["facts"]
assert original["binary"]["sha256"] == load(SDK / "root-full-rust-tests-command.json")["binary_after"]["sha256"]
exit_result, restored = load(SDK / "task-3-native-exit-settings-exit.json"), load(SDK / "task-3-native-exit-settings-restore.json")
assert exit_result["passed"] and restored["passed"]
assert restored["restored_preferences"]["value"]["close_behavior"] == restored["original_close_behavior"] == "hide_to_tray"
process = load(SDK / "task-3-native-process-actual-close-exit-02.json")
assert process["passed"] and process["exit_code"] == 0 and process["handle_bound_before_close"]
human = load(SDK / "task-3-native-user-confirmation.json")
assert human["answer"] == "能恢复并正常拖动" and not human["window_position_delta_claimed"]
normal = load(SDK / "task-3-native-normal-01-launch.json")
assert normal["debug_port"] is None and normal["debug_address"] is None and normal["isolated_webview_directory"] is None
normal_window = load(SDK / "task-3-native-final-normal-app.json")
assert normal_window["passed"] and normal_window["windows"][0]["pid"] == normal["pid"] and normal_window["windows"][0]["visible"]
with socket.socket() as probe:
    probe.settimeout(1)
    debug_port_closed = probe.connect_ex(("127.0.0.1", 9243)) != 0
assert debug_port_closed, "Native test debug listener still reachable"
entries = {}
def add(name, path):
    assert name not in entries and path.is_file() and not path.is_symlink()
    entries[name] = path.read_bytes()
for path in sorted(FINAL.rglob("*")):
    if path.is_file(): add("final-original06/" + path.relative_to(FINAL).as_posix(), path)
names = [
    "verify-final-original06-only.py", "root-final-original06-only-command.json", "root-final-original06-only-stdout.log", "root-final-original06-only-stderr.log",
    "promote-root-evidence-01.py", "promote-root-evidence-02.py", "task-3-native-user-confirmation.json", "task-3-native-manual-after-user.json",
    "task-3-native-exit-settings.mjs", "task-3-native-exit-settings-v1.mjs", "task-3-native-exit-first-run-abort.json", "task-3-native-process-watch.py",
    "task-3-native-process-actual-close-exit.json", "task-3-native-process-actual-close-exit-02.json", "task-3-native-exit-settings-exit.json", "task-3-native-exit-settings-restore.json",
    "task-3-native-debug-02-launch.json", "task-3-native-debug-02-stdout.log", "task-3-native-debug-02-stderr.log", "task-3-native-debug-teardown.ps1", "task-3-native-debug-teardown.json",
    "task-3-native-debug-01-stdout.log", "task-3-native-debug-01-stderr.log", "task-3-native-normal-01-launch.json", "task-3-native-normal-01-stdout.log", "task-3-native-normal-01-stderr.log", "task-3-native-final-normal-app.json",
]
for name in names: add("root/" + name, SDK / name)
copies = {
    "B04-T03-root-final-original06-audit.json": "final-original06/audit.json",
    "B04-T03-root-final-original06-verification.json": "final-original06/output/verification.json",
    "B04-T03-root-final-original06-verification.md": "final-original06/output/verification.md",
    "B04-T03-root-final-original06-knowledge-ir.json": "final-original06/output/knowledge-ir.json",
    "B04-T03-root-final-original06-only-command.json": "root/root-final-original06-only-command.json",
    "B04-T03-ui-native-user-confirmation.json": "root/task-3-native-user-confirmation.json",
    "B04-T03-ui-native-exit-settings.json": "root/task-3-native-exit-settings-exit.json",
    "B04-T03-ui-native-preference-restored.json": "root/task-3-native-exit-settings-restore.json",
    "B04-T03-ui-native-exit-process.json": "root/task-3-native-process-actual-close-exit-02.json",
    "B04-T03-ui-native-normal-launch.json": "root/task-3-native-normal-01-launch.json",
    "B04-T03-ui-native-normal-window.json": "root/task-3-native-final-normal-app.json",
    "B04-T03-root-preserve-01.py": "root/promote-root-evidence-01.py",
    "B04-T03-root-preserve-02.py": "root/promote-root-evidence-02.py",
    "B04-T03-root-final-original06-check.py": "root/verify-final-original06-only.py",
}
with zipfile.ZipFile(ARCHIVE, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for name, data in entries.items(): archive.writestr(name, data)
assertions = 0
with zipfile.ZipFile(ARCHIVE) as archive:
    assert set(archive.namelist()) == set(entries); assertions += 1
    for name, data in entries.items(): assert archive.read(name) == data; assertions += 1
readable = {}
for name, member in copies.items():
    target = ASSETS / name
    with target.open("xb") as stream: stream.write(entries[member])
    assert target.read_bytes() == entries[member]; assertions += 1
    readable[name] = {"archive_member": member, **fp(target.read_bytes())}
result = {
    "schema_version": 1, "captured_at": datetime.now(timezone.utc).isoformat(),
    "archive": {"path": ARCHIVE.relative_to(REPO).as_posix(), **fp(ARCHIVE.read_bytes())},
    "entries": {name: fp(data) for name, data in entries.items()}, "readable_copies": readable,
    "actual_preservation_assertions": assertions,
    "final_original06": {"ordinary_cli_calls": 1, "expected_atoms": 1, "passed": 1, "binary_sha256": original["binary"]["sha256"], "same_original_inputs_unchanged": True},
    "native": {"installed_executable_sha256": normal["executable_sha256"], "actual_automated_checks_passed": 11 + len(exit_result["checks"]) + len(restored["checks"]), "user_confirmed_tray_restore_and_drag": True, "native_direct_exit_code": process["exit_code"], "original_close_behavior_restored": True, "normal_launch_pid": normal["pid"], "normal_title_band_height": normal_window["windows"][0]["native_title_band_height"], "debug_port_closed": debug_port_closed, "stderr_png_warnings_debug1": (SDK / "task-3-native-debug-01-stderr.log").read_text().count('libpng warning'), "stderr_png_warnings_debug2": (SDK / "task-3-native-debug-02-stderr.log").read_text().count('libpng warning')},
    "retained_setup_failure": "First process watcher itself bound correctly but CRLF readiness in the Node driver prevented close; timeout258/active259 and owned-executor abort are retained. Corrected driver then observed real user-control exit0. Exit was already saved before retry; later restoration proves the original behavioral value, not an uncaptured original preference-file byte hash.",
    "earlier_failures_and_full_root_matrix": "B04-T03-root-evidence-01.zip",
    "pending": ["final whole-branch review, ledger completion and push"],
    "new_model_calls": 0, "credential_decryptions": 0,
    "private_byte_policy": "No encrypted credential bytes or raw full-screen/PrintWindow image. Only hashes of selected saved service inputs and public native visible-frame crops are archived.",
}
with MANIFEST.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2); stream.write("\n")
print(json.dumps({"archive": result["archive"], "entries": len(entries), "readable_copies": len(readable), "actual_preservation_assertions": assertions, "final_original06": result["final_original06"], "native": result["native"]}, ensure_ascii=False))
