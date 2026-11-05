"""WSL driver for a new, isolated installed Windows Obsidian validation.

All environment paths are explicit arguments. Inputs are generated audit cases;
the original audit files are never opened for writing or passed to Obsidian.
"""
import argparse
import hashlib
import json
import subprocess
import sys
import time
import urllib.request
import uuid
from pathlib import Path


def pin(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def inventory(root):
    result = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            # Audit roots can contain intentionally rejected aliases. Record
            # their spelling without reading anything through those aliases.
            result[path.relative_to(root).as_posix()] = {"symlink_target": str(path.readlink())}
        elif path.is_file():
            result[path.relative_to(root).as_posix()] = pin(path)
    return result


def win(path):
    return subprocess.check_output(["wslpath", "-w", str(path)], text=True).strip()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("audit_root", type=Path)
parser.add_argument("case_name")
parser.add_argument("new_work", type=Path)
parser.add_argument("obsidian_exe", type=Path)
parser.add_argument("powershell_exe", type=Path)
parser.add_argument("native_python_exe", type=Path)
parser.add_argument("playwright_module", type=Path)
parser.add_argument("--preview-name", default="Vault")
parser.add_argument("--expected", type=Path)
parser.add_argument("--checker", type=Path, required=True)
parser.add_argument("--checker-python", type=Path, default=Path(sys.executable))
parser.add_argument("--checker-native", action="store_true")
args = parser.parse_args()
base = args.audit_root / "cases" / args.case_name
assert base.parent == args.audit_root / "cases" and base.is_dir()
before = inventory(base)
preview = base / args.preview_name
assert preview.parent == base and not preview.is_symlink()
plan = json.loads((preview / "plan.json").read_text(encoding="utf-8"))
expected_path = args.expected or args.audit_root / "frozen-authors" / ("expected-" + args.case_name + ".json")
expected = json.loads(expected_path.read_text(encoding="utf-8"))
expected_pin = pin(expected_path)
report = json.loads((preview / "flow.json").read_text(encoding="utf-8"))
assert expected["flow_status"] == report["status"] == "passed"
assert plan["representation"] == expected["representation"] and not plan["published"]
assert plan["document"]["process"] == expected["process"]
assert plan["document"]["steps"] == sorted(expected["nodes"], key=lambda x: x["id"])
assert plan["document"]["relations"] == sorted(expected["edges"], key=lambda x: x["id"])
assert plan["knowledge_status"] == "pending" and plan["scientific_quality"] == "not_run"
work = args.new_work
work.mkdir()
(work / "capture-script.mjs").write_bytes(Path(__file__).with_name("capture_obsidian.mjs").read_bytes())
(work / "launcher-script.py").write_bytes(Path(__file__).read_bytes())
# Bind the plan and Markdown to independently reconstructed source semantics
# before treating plan.notes as the literal DOM readback target.
checker_paths = [args.checker, base / "project", base / "case", base / "review", base / "profile.json", preview, expected_path]
checker_argv = [str(args.checker_python), win(checker_paths[0]) if args.checker_native else str(checker_paths[0]), "check"] + [win(p) if args.checker_native else str(p) for p in checker_paths[1:]]
checked = subprocess.run(checker_argv, capture_output=True, timeout=120)
(work / "independent-check.stdout.bin").write_bytes(checked.stdout)
(work / "independent-check.stderr.bin").write_bytes(checked.stderr)
write(work / "independent-check.json", {"argv": checker_argv, "exit_code": checked.returncode, "checker": pin(args.checker), "stdout": pin(work / "independent-check.stdout.bin"), "stderr": pin(work / "independent-check.stderr.bin")})
assert checked.returncode == 0, checked.stderr.decode("utf-8", errors="replace")
assert json.loads(checked.stdout)["status"] == "passed"
vault = work / "vault"
profile = work / "profile"
vault.mkdir()
profile.mkdir()
notes = {}
for note in plan["notes"]:
    name = note["output_path"]
    assert Path(name).name == name and name.endswith(".md") and not name.startswith(".")
    original = preview / name
    assert not original.is_symlink(), "only regular generated notes are copied"
    (vault / name).write_bytes(original.read_bytes())
    notes[name] = pin(original)
(vault / ".obsidian").mkdir()
write(vault / ".obsidian/app.json", {"defaultViewMode": "preview"})
write(vault / ".obsidian/community-plugins.json", [])
write(profile / "obsidian.json", {"updateDisabled": True, "vaults": {
    uuid.uuid4().hex[:16]: {"path": win(vault), "ts": int(time.time() * 1000), "open": True}}})
launch = '''import json,socket,subprocess,sys
from datetime import datetime,timezone
from pathlib import Path
exe,profile,work=sys.argv[1:]
with socket.socket() as sock:
    sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
argv=[exe,'--user-data-dir='+profile,'--remote-debugging-port='+str(port),'--remote-debugging-address=127.0.0.1']
with (Path(work)/'obsidian.stdout.bin').open('xb') as out,(Path(work)/'obsidian.stderr.bin').open('xb') as err:
    process=subprocess.Popen(argv,stdout=out,stderr=err,creationflags=subprocess.DETACHED_PROCESS|subprocess.CREATE_NEW_PROCESS_GROUP)
print(json.dumps(dict(pid=process.pid,port=port,argv=argv,startedUtc=datetime.now(timezone.utc).isoformat())))
'''
(work / "launch.py").write_text(launch, encoding="utf-8")
cmd = [str(args.native_python_exe), win(work / "launch.py"), win(args.obsidian_exe), win(profile), win(work)]
receipt = None
try:
    run = subprocess.run(cmd, capture_output=True, timeout=30)
    (work / "launch.stdout.bin").write_bytes(run.stdout)
    (work / "launch.stderr.bin").write_bytes(run.stderr)
    assert run.returncode == 0, run.stderr
    receipt = json.loads(run.stdout.decode("utf-8-sig"))
    write(work / "launch-receipt.json", dict(launch_argv=cmd, exit_code=run.returncode, **receipt))
    port = receipt["port"]
    write(work / "visual-input.json", {
        "case_name": args.case_name, "compiled_source": str(preview),
        "compiled_source_pins": before, "copied_note_pins": notes,
        "plan": plan, "expected": expected, "expected_pin": expected_pin, "flow_report": report,
        "profile": win(profile), "vault": win(vault), "port": port,
        "launcher": {"path": str(args.obsidian_exe), **pin(args.obsidian_exe)}})
    for attempt in range(120):
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version", timeout=1) as response:
                version = json.load(response)
            break
        except OSError:
            time.sleep(.25)
    else:
        raise RuntimeError("Own installed Obsidian CDP not available; inspect launch receipt without restarting")
    write(work / "cdp-version.json", version)
    cmd = ["node", str(Path(__file__).with_name("capture_obsidian.mjs")), str(args.playwright_module), str(work / "visual-input.json")]
    run = subprocess.run(cmd, capture_output=True, timeout=300)
    (work / "capture.stdout.bin").write_bytes(run.stdout)
    (work / "capture.stderr.bin").write_bytes(run.stderr)
    write(work / "capture-receipt.json", dict(argv=cmd, exit_code=run.returncode))
finally:
    # Only inspect/terminate this freshly created profile, never a general app
    # instance. PID plus command-line ownership is checked before emergency cleanup.
    owned_profile = win(profile).replace("'", "''")
    owned_pid = int(receipt["pid"]) if receipt else 0
    owned_port = int(receipt["port"]) if receipt else 0
    cleanup = f"""$ErrorActionPreference='Stop'
    $profilePath='{owned_profile}'
    $receiptPid={owned_pid}
    $receiptPort={owned_port}
    $candidates=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains($profilePath) }})
    $owned=@()
    foreach($candidate in $candidates) {{
      if ($receiptPid -eq 0 -or $candidate.ProcessId -eq $receiptPid) {{
        $owned += $candidate.ProcessId
        taskkill.exe /PID $candidate.ProcessId /T /F | Out-Null
      }}
    }}
    Start-Sleep -Milliseconds 300
    $remaining=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains($profilePath) }})
    $pidState=if($receiptPid -ne 0){{Get-Process -Id $receiptPid -ErrorAction SilentlyContinue}}else{{$null}}
    $listener=if($receiptPort -ne 0){{Get-NetTCPConnection -LocalPort $receiptPort -State Listen -ErrorAction SilentlyContinue}}else{{$null}}
    @{{ownedPid=$receiptPid;ownedPort=$receiptPort;ownedPidAbsent=($null -eq $pidState);ownedListenerAbsent=($null -eq $listener);profileProcessCount=$remaining.Count;emergencyTerminated=$owned;checkedUtc=[DateTime]::UtcNow.ToString('o')}} | ConvertTo-Json -Compress
    """
    (work / "cleanup.ps1").write_text(cleanup, encoding="utf-8")
    probe = subprocess.run([str(args.powershell_exe), "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", win(work / "cleanup.ps1")], capture_output=True, timeout=30)
    (work / "cleanup.stdout.bin").write_bytes(probe.stdout)
    (work / "cleanup.stderr.bin").write_bytes(probe.stderr)
    assert probe.returncode == 0, probe.stderr
    cleanup_result = json.loads(probe.stdout.decode("utf-8-sig"))
    write(work / "cleanup-result.json", cleanup_result)
after = inventory(base)
assert pin(expected_path) == expected_pin, "frozen expected changed"
assert after == before, "original evidence changed"
assert all(pin(vault / name) == p for name, p in notes.items()), "copied note bytes changed"
write(work / "protection-cleanup.json", {
    "original_audit_unchanged": before == after,
    "copied_notes_unchanged": True, "copied_note_pins": notes, "cleanup": cleanup_result})
assert cleanup_result["ownedPidAbsent"] and cleanup_result["ownedListenerAbsent"] and cleanup_result["profileProcessCount"] == 0, cleanup_result
assert run.returncode == 0, run.stderr.decode("utf-8", errors="replace")
print(json.dumps(dict(status="passed", case=args.case_name, work=str(work)), ensure_ascii=False))
