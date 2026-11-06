"""Actual Windows Rust-only regression of a pinned B06V-T02 API source snapshot.

Only the new selected snapshot and D:/Noetome's existing build cache are used.
There is no install, production Vault operation, or integrity-label mutation.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.metadata
import inspect
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import unittest


def fp(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def pin(path):
    return fp(Path(path).read_bytes())


def save(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


parser = argparse.ArgumentParser()
parser.add_argument("root", type=Path)
parser.add_argument("--from-stage", default="fmt", choices=["fmt", "workspace-check", "workspace-tests", "cli-build", "python-full", "audits", "summarize"])
parser.add_argument("--resume-audits",action="store_true")
args = parser.parse_args()
root = args.root
assert sys.platform == "win32" and sys.version_info[:3] == (3, 12, 14)
cfg = json.loads((root / "root.json").read_bytes())
manifest = json.loads((root / "snapshot.json").read_bytes())
repo = Path(cfg["repo_native"])
target = Path(cfg["target_native"])
cli = target / "debug/noetome.exe"
before = {name: pin(repo / name) for name in manifest["files"]}
assert before == manifest["files"], "exact current-worktree snapshot preflight"
runtime_temp = root / "runtime-temp"
runtime_temp.mkdir(exist_ok=True)
env = os.environ.copy()
env.update(
    NOETOME_TEST_PYTHON=cfg["fake_python_native"],
    NOETOME_PDF_PYTHON=cfg["python_native"],
    NOETOME_TEST_BINARY=str(cli),
    CARGO_TARGET_DIR=str(target),
    PYTHONUTF8="1",
    TEMP=str(runtime_temp),
    TMP=str(runtime_temp),
)
credential_present = "NOETOME_TANGMAO_API_KEY" in env
for secret_env in ["NOETOME_TANGMAO_API_KEY", "NOETOME_TEST_SCAN_CREDENTIAL"]:
    env.pop(secret_env, None)
runtime_lock = json.loads((repo / "sidecars/pdf_parser/runtime-lock.json").read_bytes())
package_versions = {name: importlib.metadata.version(name) for name in runtime_lock["packages"]}
assert package_versions == runtime_lock["packages"], "exact selected PDF runtime dependency versions"
fake_probe = subprocess.run([cfg["fake_python_native"], "--version"], capture_output=True, timeout=15)
assert fake_probe.returncode == 0
save(root / "native-runtime.json", {
    "inherited_live_credential_present": credential_present, "live_credential_removed_from_test_env": True,
    "python": sys.version, "executable": sys.executable, "platform": sys.platform,
    "selected_env": {key: env[key] for key in ["NOETOME_TEST_PYTHON", "NOETOME_PDF_PYTHON", "NOETOME_TEST_BINARY", "CARGO_TARGET_DIR", "TEMP", "TMP"]},
    "pdf_packages": package_versions,
    "fake_python_version": (fake_probe.stdout + fake_probe.stderr).decode("utf8","replace").strip(),
    "scope": "actual Windows CPython 3.12.14; exact PDF dependencies and fake runtime selected explicitly",
})
external = root / "external-sentinel.txt"
if not external.exists():
    external.write_bytes(b"Self-owned external Windows formula validation sentinel\r\n")
external_before = pin(external)
log = root / "native-commands.json"
commands = json.loads(log.read_bytes()) if log.exists() else []
base_stages = ["fmt", "workspace-check", "workspace-tests", "cli-build", "python-full", "audits"]
audit_stages = [Path(script).stem.removeprefix("verify_") + "-audit" for script in cfg["audits"]]
required = ["cli-cache-invalidate", "fmt", "workspace-check", "cli-prebuild", "cli-scan-namespace", "workspace-tests", "cli-build"]


def command(name, argv):
    attempt = 1 + sum(c["name"] == name for c in commands)
    stem = name + "-attempt-" + str(attempt)
    out, err = root / (stem + ".stdout.bin"), root / (stem + ".stderr.bin")
    assert not out.exists() and not err.exists()
    start = time.monotonic()
    cli_before = pin(cli) if cli.exists() else None
    print("START", name, "attempt", attempt, flush=True)
    with out.open("xb") as stdout, err.open("xb") as stderr:
        proc = subprocess.Popen(argv, cwd=repo, env=env, stdout=stdout, stderr=stderr)
        save(root / "native-running.json", {"name": name, "attempt": attempt, "pid": proc.pid, "argv": argv, "started_utc": datetime.now(timezone.utc).isoformat(), "status": "running"})
        timed_out = False
        try:
            code = proc.wait(timeout=1800)
        except subprocess.TimeoutExpired:
            timed_out = True
            killed = subprocess.run(["taskkill.exe", "/PID", str(proc.pid), "/T", "/F"], capture_output=True, timeout=20)
            (root / (stem + ".timeout.stdout.bin")).write_bytes(killed.stdout)
            (root / (stem + ".timeout.stderr.bin")).write_bytes(killed.stderr)
            code = proc.wait(timeout=20)
    after = {name: pin(repo / name) for name in manifest["files"]}
    receipt = {
        "name": name, "attempt": attempt, "argv": argv, "cwd": str(repo), "pid": proc.pid,
        "exit_code": code, "timed_out": timed_out, "elapsed_seconds": time.monotonic() - start,
        "stdout": {"path": str(out), **pin(out)}, "stderr": {"path": str(err), **pin(err)},
        "source_before_after_equal": after == before, "external_sentinel_before_after_equal": pin(external) == external_before,
        "cli_before": cli_before, "cli_after": pin(cli) if cli.exists() else None,
        "snapshot_fingerprint": pin(root / "snapshot.json"),
    }
    commands.append(receipt)
    save(root / (stem + ".json"), receipt)
    save(log, commands)
    save(root / "native-running.json", {"name": name, "attempt": attempt, "pid": proc.pid, "status": "terminal", "exit_code": code})
    print("END", name, code, round(receipt["elapsed_seconds"], 2), flush=True)
    assert after == before and pin(external) == external_before
    return code == 0 and not timed_out


ps = ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(repo / "scripts/cargo_local.ps1")]
stages = [
    ("fmt", ps + ["fmt", "--all", "--check"]),
    ("workspace-check", ps + ["check", "--workspace", "--locked", "--offline"]),
    ("workspace-tests", ps + ["test", "--workspace", "--locked", "--offline", "--", "--test-threads=4", "--nocapture"]),
    ("cli-build", ps + ["build", "-p", "noetome-cli", "--locked", "--offline"]),
]
start_index = 6 if args.from_stage=="summarize" else base_stages.index(args.from_stage)
if start_index == 0:
    # A prior candidate observed Cargo reuse the B06-T03 noetome lib/PE across
    # a new source root. Invalidate only this product package in the selected
    # build cache, preserve all dependencies and record the actual operation.
    if not command("cli-cache-invalidate", ps + ["clean", "-p", "noetome-cli"]):
        sys.exit(1)
for index, (name, argv) in enumerate(stages):
    if index == 2 and index >= start_index:
        # Unit-test ProcessCase has no CARGO_BIN_EXE and deliberately uses the
        # source snapshot's target/debug path. Prepare a real current-source
        # executable there; keep the shared cache path for normal integration.
        if not command("cli-prebuild", ps + ["build", "-p", "noetome-cli", "--locked", "--offline"]):
            sys.exit(1)
        if not command("cli-scan-namespace", [str(cli),"knowledge","scan-formula","--help"]):
            sys.exit(1)
        legacy_cli = repo / "target/debug/noetome.exe"
        legacy_cli.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(cli, legacy_cli)
        assert pin(legacy_cli) == pin(cli)
        save(root / "unit-test-cli-copy.json", {"built_cli": str(cli), "unit_test_fallback_cli": str(legacy_cli), "fingerprint": pin(cli), "copied_bytes_equal": True})
    if index >= start_index and not command(name, argv):
        sys.exit(1)
assert cli.is_file()

latest = {c["name"]: c for c in commands}
assert all(n in latest and latest[n]["exit_code"] == 0 and not latest[n]["timed_out"] for n in required)
text = Path(latest["workspace-tests"]["stdout"]["path"]).read_text(encoding="utf-8")
summaries = [tuple(map(int, m)) for m in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)]
assert summaries and all(f == 0 for _, f, _ in summaries)
selected_cli = root / "noetome-selected.exe"
assert not selected_cli.exists()
with selected_cli.open("xb") as out:
    out.write(cli.read_bytes())
assert pin(selected_cli) == pin(cli) and selected_cli.stat().st_nlink == 1
save(root / "native-rust-final.json", {
    "required_latest_exits": {n: latest[n]["exit_code"] for n in required},
    "source_file_count": len(before), "source_before_after_equal": True,
    "external_sentinel_before_after_equal": True, "source_files": before,
    "cli": pin(cli), "selected_cli": {"path":str(selected_cli), **pin(selected_cli), "nlink":selected_cli.stat().st_nlink},
    "rust_suite_summaries": summaries,
    "rust_totals": {"passed": sum(p for p, _, _ in summaries), "failed": sum(f for _, f, _ in summaries), "ignored": sum(i for _, _, i in summaries)},
    "real_api_calls":0, "real_model_calls":0, "paid_calls":0,
    "python_full":"not_run_in_this_stage", "independent_audits":"not_run_in_this_stage",
    "scope": "actual Windows exact-source Rust workspace fmt/check/test/build and scan four-path help; live credential excluded; synthetic provider tests do not prove real recognition; no production Vault write",
})
print("ALL_REQUIRED_WINDOWS_RUST_GATES_PASSED", flush=True)
