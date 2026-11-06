"""Actual Windows phase-two Python and independent CLI audits of a new pinned source snapshot.

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
cli = root / "noetome-selected.exe"
assert cli.stat().st_nlink == 1
assert pin(cli) == {"sha256":"19c5deb3233d368085340fcd263d15497c96ba94c619aba144e19d0112df4f59","byte_length":25873408}, "phase-one actual built CLI is reused by exact bytes"
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
required = ["python-full"] + audit_stages


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



if not command("python-full", [cfg["python_native"], "-X", "utf8", "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"]):
    sys.exit(1)
for script, name in zip(cfg["audits"], audit_stages, strict=True):
    audit_cli = root / (name + "-cli-1.exe")
    with audit_cli.open("xb") as out:
        out.write(cli.read_bytes())
    assert pin(audit_cli) == pin(cli) and audit_cli.stat().st_nlink == 1
    audit_root = root / (name + "-1")
    if not command(name, [cfg["python_native"], "-X", "utf8", "scripts/" + script, "audit", str(audit_cli), str(audit_root)]):
        sys.exit(1)
latest = {c["name"]: c for c in commands}
assert all(n in latest and latest[n]["exit_code"] == 0 and not latest[n]["timed_out"] for n in required)
audit_results = {name: json.loads(Path(latest[name]["stdout"]["path"]).read_bytes()) for name in audit_stages}
assert all(value["status"] == "passed" for value in audit_results.values())
python_text = Path(latest["python-full"]["stderr"]["path"]).read_text(encoding="utf8")
ran=re.search(r"Ran (\d+) tests",python_text)
ok=re.search(r"\nOK(?: \(skipped=(\d+)\))?\n",python_text)
assert ran and ok
skip_count=int(ok.group(1) or 0)
def cases(suite):
    for item in suite:
        if isinstance(item,unittest.TestSuite):yield from cases(item)
        else:yield item
os.chdir(repo)
static_skips=[]
dynamic_boundaries=[]
for case in cases(unittest.defaultTestLoader.discover(str(repo/"tests"),pattern="test_*.py")):
    method=getattr(case,case._testMethodName)
    owner=case.__class__
    if getattr(method,"__unittest_skip__",False) or getattr(owner,"__unittest_skip__",False):
        static_skips.append({"test":case.id(),"reason":getattr(method,"__unittest_skip_why__",None) or getattr(owner,"__unittest_skip_why__",None)})
    elif 'self.skipTest(' in inspect.getsource(method):dynamic_boundaries.append(case)
boundary_result=unittest.TestResult()
unittest.TestSuite(dynamic_boundaries).run(boundary_result)
assert not boundary_result.errors and not boundary_result.failures,"actual dynamic platform boundary probe must not fail"
dynamic_skips=[{"test":case.id(),"reason":reason} for case,reason in boundary_result.skipped]
all_skips=static_skips+dynamic_skips
assert len(all_skips)==skip_count,"every actual full-run skip is accounted for by explicit skips and actual dynamic platform probes"
save(root/"python-skips.json",{"tests_run":int(ran.group(1)),"passed":int(ran.group(1))-skip_count,"skipped":all_skips,"failed":0,"boundary_probe":{"tests_run":boundary_result.testsRun,"skips":dynamic_skips,"errors":0,"failures":0},"scope":"actual unittest full result, explicit skip discovery and a limited actual rerun of only dynamic platform boundary methods to identify observed skip reasons; skips are not passes"})

save(root / "native-python-final.json", {
    "required_latest_exits": {n: latest[n]["exit_code"] for n in required},
    "source_file_count": len(before), "source_before_after_equal": True,
    "external_sentinel_before_after_equal": True, "source_files": before,
    "cli": {"path":str(cli), **pin(cli), "nlink":cli.stat().st_nlink},
    "audits": audit_results, "python_summary":python_text,
    "python_totals":{"run":int(ran.group(1)),"passed":int(ran.group(1))-skip_count,"skipped":skip_count,"failed":0},"python_skips":all_skips,
    "real_api_calls":0,"real_model_calls":0,"paid_calls":0,
    "rust_regression":"retained_phase_one_exact_same_source_and_PE",
    "scope":"actual Windows final Python/helper plus independent table/flow/formula/image/scan CLI audits in a separate pinned phase-two snapshot; live credential excluded, only explicit artificial loopback HTTP; no real recognition or production Vault",
})
print("ALL_REQUIRED_WINDOWS_PYTHON_AUDIT_GATES_PASSED",flush=True)
