"""Launch one fresh owned installed Obsidian, preserve inputs, capture and clean up.

Only explicitly selected generated test files are copied. This launcher never
reads a user Vault and never renders Markdown itself.
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
    for p in sorted(root.iterdir()):
        assert not p.is_symlink(), "selected capture files cannot be aliases"
        assert p.is_file(), "capture accepts a flat generated test output"
        result[p.name] = pin(p)
    return result


def win(path):
    return subprocess.check_output(["wslpath", "-w", str(path)], text=True).strip()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("selected_output", type=Path)
parser.add_argument("capture_spec", type=Path)
parser.add_argument("new_work", type=Path)
parser.add_argument("native_python", type=Path)
parser.add_argument("obsidian_exe", type=Path)
parser.add_argument("powershell_exe", type=Path)
parser.add_argument("playwright_module", type=Path)
parser.add_argument("--capture-script", type=Path, default=Path(__file__).with_name("capture_formula.mjs"))
args = parser.parse_args()
spec = json.loads(args.capture_spec.read_bytes())
before = inventory(args.selected_output)
spec_before = pin(args.capture_spec)
work = args.new_work
work.mkdir()
vault = work / "vault"
profile = work / "profile"
vault.mkdir()
profile.mkdir()
for name in before:
    (vault / name).write_bytes((args.selected_output / name).read_bytes())
assert inventory(vault) == before
write(work / "source-before.json", before)
(work / "launcher-script.py").write_bytes(Path(__file__).read_bytes())
(work / "capture-script.mjs").write_bytes(args.capture_script.read_bytes())
(work / "capture-spec.json").write_bytes(args.capture_spec.read_bytes())
(vault / ".obsidian").mkdir()
write(vault / ".obsidian/app.json", {"defaultViewMode": "preview"})
write(vault / ".obsidian/community-plugins.json", [])
write(profile / "obsidian.json", {"updateDisabled": True, "vaults": {
    uuid.uuid4().hex[:16]: {"path": win(vault), "ts": int(time.time()*1000), "open": True}}})
launch = '''import socket,subprocess,json,sys
from pathlib import Path
exe,profile,work=sys.argv[1:]
with socket.socket() as sock:
    sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
argv=[exe,'--user-data-dir='+profile,'--remote-debugging-port='+str(port),'--remote-debugging-address=127.0.0.1']
with (Path(work)/'obsidian.stdout.bin').open('xb') as out,(Path(work)/'obsidian.stderr.bin').open('xb') as err:
    process=subprocess.Popen(argv,stdout=out,stderr=err,creationflags=subprocess.DETACHED_PROCESS|subprocess.CREATE_NEW_PROCESS_GROUP)
print(json.dumps(dict(pid=process.pid,port=port,argv=argv)))
'''
(work / "launch_native.py").write_text(launch, encoding="utf-8")
receipt = None
capture = None
cleanup_result = None
try:
    cmd = [str(args.native_python), win(work / "launch_native.py"), win(args.obsidian_exe), win(profile), win(work)]
    run = subprocess.run(cmd, capture_output=True, timeout=30)
    (work / "launch.stdout.bin").write_bytes(run.stdout)
    (work / "launch.stderr.bin").write_bytes(run.stderr)
    assert run.returncode == 0, run.stderr
    receipt = json.loads(run.stdout.decode("utf-8-sig"))
    write(work / "launch-receipt.json", {**receipt, "launch_argv": cmd, "exit_code": run.returncode,
                                          "launcher": {"path": str(args.obsidian_exe), **pin(args.obsidian_exe)}})
    write(work / "visual-input.json", {
        "profile": win(profile), "vault": win(vault), "port": receipt["port"],
        "spec": spec, "source": str(args.selected_output), "source_pins": before,
        "spec_pin": spec_before})
    for _ in range(120):
        try:
            with urllib.request.urlopen(f'http://localhost:{receipt["port"]}/json/version', timeout=1) as response:
                write(work / "cdp-version.json", json.load(response))
            break
        except OSError:
            time.sleep(.25)
    else:
        raise RuntimeError("Own Obsidian CDP not available; inspect live receipt before restarting")
    cmd = ["node", str(args.capture_script), str(args.playwright_module), str(work / "visual-input.json")]
    capture = subprocess.run(cmd, capture_output=True, timeout=300)
    (work / "capture.stdout.bin").write_bytes(capture.stdout)
    (work / "capture.stderr.bin").write_bytes(capture.stderr)
    write(work / "capture-receipt.json", {"argv": cmd, "exit_code": capture.returncode})
finally:
    own_profile = win(profile).replace("'", "''")
    own_pid = int(receipt["pid"]) if receipt else 0
    own_port = int(receipt["port"]) if receipt else 0
    cleanup = f'''$ErrorActionPreference='Stop'
$profilePath='{own_profile}'
$receiptPid={own_pid}
$receiptPort={own_port}
$candidates=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains($profilePath) }})
$terminated=@()
foreach($candidate in $candidates) {{
    if ($receiptPid -eq 0 -or $candidate.ProcessId -eq $receiptPid) {{
        $terminated += $candidate.ProcessId
        taskkill.exe /PID $candidate.ProcessId /T /F | Out-Null
    }}
}}
Start-Sleep -Milliseconds 300
$remaining=@(Get-CimInstance Win32_Process -Filter "Name='Obsidian.exe'" | Where-Object {{ $_.CommandLine -and $_.CommandLine.Contains($profilePath) }})
$pidState=if($receiptPid -ne 0){{Get-Process -Id $receiptPid -ErrorAction SilentlyContinue}}else{{$null}}
$listener=if($receiptPort -ne 0){{Get-NetTCPConnection -LocalPort $receiptPort -State Listen -ErrorAction SilentlyContinue}}else{{$null}}
@{{ownedPid=$receiptPid;ownedPort=$receiptPort;ownedPidAbsent=($null -eq $pidState);ownedListenerAbsent=($null -eq $listener);profileProcessCount=$remaining.Count;emergencyTerminated=$terminated;checkedUtc=[DateTime]::UtcNow.ToString('o')}} | ConvertTo-Json -Compress
'''
    (work / "cleanup.ps1").write_text(cleanup, encoding="utf-8")
    probe = subprocess.run([str(args.powershell_exe), "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", win(work / "cleanup.ps1")], capture_output=True, timeout=30)
    (work / "cleanup.stdout.bin").write_bytes(probe.stdout)
    (work / "cleanup.stderr.bin").write_bytes(probe.stderr)
    assert probe.returncode == 0, probe.stderr
    cleanup_result = json.loads(probe.stdout.decode("utf-8-sig"))
    write(work / "cleanup-result.json", cleanup_result)
after = inventory(args.selected_output)
copied_after = {name: pin(vault / name) for name in before}
write(work / "protection-cleanup.json", {
    "selected_originals_unchanged": before == after, "copied_originals_unchanged": before == copied_after,
    "capture_spec_unchanged": spec_before == pin(args.capture_spec), "cleanup": cleanup_result})
assert before == after == copied_after
assert spec_before == pin(args.capture_spec)
assert cleanup_result["ownedPidAbsent"] and cleanup_result["ownedListenerAbsent"] and cleanup_result["profileProcessCount"] == 0
assert capture is not None and capture.returncode == 0, capture.stderr.decode("utf8", "replace") if capture else "capture not run"
print(json.dumps({"status": "passed", "work": str(work), "case": spec["case"]}))
