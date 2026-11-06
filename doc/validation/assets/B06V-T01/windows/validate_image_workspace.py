"""Actual Windows regression of a pinned current-worktree source snapshot.

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
runtime_lock = json.loads((repo / "sidecars/pdf_parser/runtime-lock.json").read_bytes())
package_versions = {name: importlib.metadata.version(name) for name in runtime_lock["packages"]}
assert package_versions == runtime_lock["packages"], "exact selected PDF runtime dependency versions"
fake_probe = subprocess.run([cfg["fake_python_native"], "--version"], capture_output=True, timeout=15)
assert fake_probe.returncode == 0
save(root / "native-runtime.json", {
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
required = base_stages[:-1] + audit_stages


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
    ("python-full", [cfg["python_native"], "-X", "utf8", "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"]),
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
        if not command("cli-image-namespace", [str(cli),"knowledge","image","--help"]):
            sys.exit(1)
        legacy_cli = repo / "target/debug/noetome.exe"
        legacy_cli.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(cli, legacy_cli)
        assert pin(legacy_cli) == pin(cli)
        save(root / "unit-test-cli-copy.json", {"built_cli": str(cli), "unit_test_fallback_cli": str(legacy_cli), "fingerprint": pin(cli), "copied_bytes_equal": True})
    if index >= start_index and not command(name, argv):
        sys.exit(1)
assert cli.is_file()
for script, name in (zip(cfg["audits"], audit_stages, strict=True) if start_index<=5 else []):
    previous=[c for c in commands if c["name"]==name]
    if args.resume_audits and previous and previous[-1]["exit_code"]==0 and previous[-1]["cli_after"]==pin(cli) and previous[-1]["source_before_after_equal"]:
        print("RETAIN_SUCCESS",name,flush=True)
        continue
    attempt = 1 + sum(c["name"] == name for c in commands)
    audit_cli = root / (name + "-cli-" + str(attempt) + ".exe")
    assert not audit_cli.exists()
    with audit_cli.open("xb") as out:
        out.write(cli.read_bytes())
    assert pin(audit_cli) == pin(cli) and audit_cli.stat().st_nlink == 1
    audit_root = root / (name + "-" + str(attempt))
    if not command(name, [cfg["python_native"], "-X", "utf8", "scripts/" + script, "audit", str(audit_cli), str(audit_root)]):
        sys.exit(1)
latest = {c["name"]: c for c in commands}
assert all(n in latest and latest[n]["exit_code"] == 0 and not latest[n]["timed_out"] for n in required)
text = Path(latest["workspace-tests"]["stdout"]["path"]).read_text(encoding="utf-8")
summaries = [tuple(map(int, m)) for m in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)]
assert summaries and all(f == 0 for _, f, _ in summaries)
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
save(root / "native-final.json", {
    "required_latest_exits": {n: latest[n]["exit_code"] for n in required},
    "source_file_count": len(before), "source_before_after_equal": True,
    "external_sentinel_before_after_equal": True, "source_files": before,
    "cli": pin(cli), "rust_suite_summaries": summaries,
    "rust_totals": {"passed": sum(p for p, _, _ in summaries), "failed": sum(f for _, f, _ in summaries), "ignored": sum(i for _, _, i in summaries)},
    "audits": audit_results, "python_summary": python_text,"python_totals":{"run":int(ran.group(1)),"passed":int(ran.group(1))-skip_count,"skipped":skip_count,"failed":0},"python_skips":all_skips,
    "scope": "actual Windows exact-source workspace fmt/check/test/build, Python full and table/flow/formula/image CLI audits; no desktop reinstall or production Vault write",
})
print("ALL_REQUIRED_WINDOWS_GATES_PASSED", flush=True)
