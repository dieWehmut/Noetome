"""Capture only the two approved current-version format identity supplements.

Uses the previously audited launcher, isolated profiles/Vaults, and no models.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import zipfile

REPO = Path(__file__).resolve().parents[5]
BASE = REPO / "doc/validation/assets/B06V-T03"
ATTEMPT = sys.argv[1] if len(sys.argv) == 2 else "07"
assert ATTEMPT in ("06", "07"), "explicit bounded supplement attempt"
SOURCE = REPO / (".artifacts/b06v-t03-current-format-identity-source-round-" + ATTEMPT)
WORK = Path("/mnt/d/Noetome/.artifacts/b06v-t03-current-format-identity-round-" + ATTEMPT + "-20261106")
OUT = BASE / "current/format-identity-supplement"
ANCHOR = "9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c"
RUNTIME_ANCHOR = "f7f74bbfcd029d6a9effca499fffd4e380b0b17877fcb17c448f7c573fac60cd"
FINAL_TOOL = "9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f"


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def files(root):
    return {str(p.relative_to(root)).replace("\\", "/"): p.read_bytes()
            for p in sorted(root.rglob("*")) if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"}


assert not SOURCE.exists() and not WORK.exists() and not OUT.exists(), "never reuse or overwrite prior evidence roots"
old_source = REPO / ".artifacts/b06v-t03-current-matrix-local-source-round-05"
shutil.copytree(old_source, SOURCE, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
tool = (REPO / "scripts/prepare_visual_compatibility.py").read_bytes()
assert pin(tool)["sha256"] == FINAL_TOOL
(SOURCE / "scripts/prepare_visual_compatibility.py").write_bytes(tool)
source_before = files(SOURCE)
WORK.mkdir()
(WORK / "source-before.json").write_text(json.dumps({n: pin(b) for n, b in source_before.items()}, indent=2) + "\n")
env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
for name in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
    env.pop(name, None)
env.update(NO_PROXY="*", no_proxy="*")
rows = []
for theme in ("light", "dark"):
    prepared = WORK / ("prepared-hand-authored-format-probe-" + theme)
    capture = WORK / ("capture-hand-authored-format-probe-" + theme)
    row = {"case_id": "hand-authored-format-probe", "theme": theme, "role": "current", "app_version": "1.15.0",
           "fixture_sha256": ANCHOR, "runtime_sha256": RUNTIME_ANCHOR, "started_utc": __import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat()}
    prepare_argv = [sys.executable, str(SOURCE / "scripts/prepare_visual_compatibility.py"),
                    "--fixture-root", str(SOURCE / "fixtures/visual-compatibility"), "--fixture-sha256", ANCHOR,
                    "--runtime-record", str(SOURCE / "doc/validation/assets/B06V-T03/runtime/current-runtime-observed.json"),
                    "--runtime-sha256", RUNTIME_ANCHOR, "--case-id", row["case_id"], "--theme", theme, "--new-root", str(prepared)]
    capture_argv = [sys.executable, str(SOURCE / "doc/validation/assets/B06V-T01/native/launch_capture.py"),
                    str(prepared / "selected-output"), str(prepared / "capture-spec.json"), str(capture),
                    "/mnt/c/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe",
                    "/mnt/d/Noetome/.artifacts/b06v-t03-runtime-20261106/win-x64/Obsidian.exe",
                    "/mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe",
                    "/home/lyinn/.npm/_npx/314abffa6aaf2cd1/node_modules/playwright-core/index.mjs",
                    "--capture-script", str(prepared / "capture_visual.mjs")]
    for name, argv, timeout in (("prepare", prepare_argv, 180), ("capture", capture_argv, 240)):
        started = time.monotonic()
        run = subprocess.run(argv, cwd=REPO, env=env, capture_output=True, timeout=timeout)
        (WORK / (theme + "-" + name + ".stdout.bin")).write_bytes(run.stdout)
        (WORK / (theme + "-" + name + ".stderr.bin")).write_bytes(run.stderr)
        row[name] = {"argv": argv, "exit_code": run.returncode, "elapsed_seconds": time.monotonic() - started,
                     "stdout": pin(run.stdout), "stderr": pin(run.stderr)}
        if run.returncode != 0:
            row["status"] = "failed"
            with (WORK / "events.jsonl").open("a") as stream:
                stream.write(json.dumps(row) + "\n")
            raise RuntimeError(name + " actual failure retained at " + str(WORK))
    data = json.loads((capture / "result.json").read_bytes())
    original_input = json.loads((capture / "visual-input.json").read_bytes())
    normalize = lambda p: p.replace("\\", "/").lower()
    assert data["status"] == "passed"
    assert normalize(data["identity"]["userData"]) == normalize(original_input["profile"])
    assert normalize(data["identity"]["vaultPath"]) == normalize(original_input["vault"])
    assert data["identity"]["communityPlugins"] == []
    assert data["compatibilityIdentity"]["appVersion"] == "1.15.0"
    assert "theme-" + theme in data["compatibilityIdentity"]["themeClass"].split()
    cleanup = json.loads((capture / "cleanup-result.json").read_bytes())
    assert cleanup["profileProcessCount"] == 0 and cleanup["ownedListenerAbsent"] and cleanup["ownedPidAbsent"] and cleanup["emergencyTerminated"] == []
    row.update(status="passed", result_pin=pin((capture / "result.json").read_bytes()),
               result_member="capture-hand-authored-format-probe-" + theme + "/result.json",
               actual_identity=data["identity"], cleanup=cleanup,
               actual_capture_script=pin((capture / "capture-script.mjs").read_bytes()),
               prepared_originals=pin((prepared / "prepared.json").read_bytes()))
    rows.append(row)
    with (WORK / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row) + "\n")
    print(json.dumps({"theme": theme, "status": row["status"], "owned_pid": cleanup["ownedPid"], "identity_recorded": True}), flush=True)
assert source_before == files(SOURCE), "source snapshot changed during captures"
OUT.mkdir()
archives = {}
for name, root in (("native-originals.zip", WORK), ("source-originals.zip", SOURCE)):
    originals = files(root)
    originals = {n: b for n, b in originals.items() if "/profile/" not in "/" + n}
    path = OUT / name
    inventory = {n: pin(b) for n, b in originals.items()}
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for n, raw in originals.items():
            z.writestr(n, raw)
        z.writestr("inventory.json", json.dumps(inventory, indent=2) + "\n")
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
        assert all(pin(z.read(n)) == fixed for n, fixed in inventory.items())
    archives[name] = {"path": str(path.relative_to(REPO)), **pin(path.read_bytes()), "inventory_members": len(inventory), "crc_and_sha_readback": "passed"}
receipt = {"schema_version": 1, "status": "passed", "scope": "only two current format identity record supplements; previous current 32/42 evidence kept immutable; two format rows override earlier formats without adding distinct cases",
           "source_tool": pin(tool), "source_snapshot": str(SOURCE), "source_files_unchanged": len(source_before),
           "native_root": str(WORK), "overrides": rows, "archives": archives,
           "model_requests": 0, "scientific_quality": "not_run", "published": False, "linux_full_rust": "not_run"}
(OUT / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"status": "passed", "rows": len(rows), "archives": archives}), flush=True)
