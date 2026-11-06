"""Freeze final ordinary image CLI outputs and independent author checks."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def pin(path):
    raw = path.read_bytes()
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def save(path, value):
    with path.open("x", encoding="utf8") as f:
        json.dump(value, f, ensure_ascii=False, indent=2)
        f.write("\n")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("cli", type=Path)
parser.add_argument("checker", type=Path)
parser.add_argument("fixture_root", type=Path)
parser.add_argument("new_root", type=Path)
parser.add_argument("--dark-cases", nargs="*", default=["signed_arrows","spatial","escaped_labels","curve_no_data"])
parser.add_argument("--cases",nargs="*")
parser.add_argument("--base-theme",choices=["light","dark"],default="light")
args = parser.parse_args()
root = args.new_root
root.mkdir()
(root / "commands").mkdir()
(root / "cases").mkdir()
frozen = root / "frozen-authors"
shutil.copytree(args.fixture_root, frozen)
cli = root / args.cli.name
shutil.copyfile(args.cli, cli)
cli.chmod(0o755)
checker_before = pin(args.checker)
authors = json.loads((frozen / "authors.json").read_bytes())
author_before = {p.relative_to(frozen).as_posix():pin(p) for p in frozen.rglob("*") if p.is_file()}


def command(name, argv):
    run = subprocess.run([str(p) for p in argv], capture_output=True, timeout=120)
    out, err = root / "commands" / (name + ".stdout.bin"), root / "commands" / (name + ".stderr.bin")
    out.write_bytes(run.stdout)
    err.write_bytes(run.stderr)
    receipt = {"argv":[str(p) for p in argv], "exit_code":run.returncode,"stdout":pin(out),"stderr":pin(err),"cli":pin(cli),"checker":pin(args.checker)}
    save(root / "commands" / (name + ".json"), receipt)
    return run, receipt


records = []
for author in authors["cases"]:
    name = author["name"]
    if args.cases is not None and name not in args.cases:
        continue
    work = root / "cases" / name
    work.mkdir()
    case = work / "case"
    shutil.copytree(frozen / author["case_directory"], case)
    profile = work / "profile.json"
    profile.write_bytes((frozen / author["profile_file"]).read_bytes())
    expected_path = frozen / author["expected_file"]
    expected = json.loads(expected_path.read_bytes())
    output = work / "generated-Vault"
    run, receipt = command(name + "-image", [cli,"knowledge","image",case,profile,output])
    wanted_exit = 0 if expected["representation"] else 3
    assert run.returncode == wanted_exit, run.stderr.decode("utf8","replace")
    check, checker_receipt = command(name + "-independent-check", [sys.executable,args.checker,"check",case,profile,output,expected_path])
    assert check.returncode == 0, check.stderr.decode("utf8","replace")
    assert pin(args.checker) == checker_before
    report = json.loads((output / "image.json").read_bytes())
    assert report["status"] == ("passed" if expected["representation"] else "needs_review")
    s = expected["semantics"]
    if expected["representation"]:
        required = [expected["case"]["title"],s["caption"],*s["conditions"],*s["exceptions"]]
        required.extend(e["label"] for e in s["elements"] if e["label"] is not None)
        required.extend(e["unit"] for key in ("elements","relations") for e in s[key] if e["unit"] is not None)
        required.extend(r["label"] for r in s["relations"] if r["label"])
        required.extend(l["meaning"] for l in s["legend"] if l["meaning"] is not None)
        dims = expected["case"]["dimensions"]
        image_name = "redrawn.svg" if expected["representation"] == "svg_redraw" else "original.png"
        note = {"file":"图像预览.md","math_trees":[],"required_texts":required,
            "images":[{"file":image_name,"width":dims[0],"height":dims[1]}],
            "link_destinations":["original.png"],"click_attachments":["original.png"]}
        if expected["representation"] == "svg_redraw":
            note["svg_semantics"] = {"file":"redrawn.svg","source":s,"dimensions":dims,"expected_fingerprint":pin(expected_path)}
    else:
        note = {"file":"image.md","math_trees":[],"images":[],"required_texts":["needs_review",*expected["issue_codes"]]}
        assert not (output / "图像预览.md").exists() and not (output / "plan.json").exists() and not (output / "redrawn.svg").exists()
    spec = {"case":name,"scope":"final ordinary CLI output after independent frozen author check; manual declaration only; scientific quality not_run",
        "theme":args.base_theme,
        "require_mathjax_runtime":False,"notes":[note],"cli_receipt":receipt,"checker_receipt":checker_receipt,
        "expected_file":str(expected_path),"expected_fingerprint":pin(expected_path),"semantic_source":s,
        "source_fingerprints":expected["source_pins"],"representation":expected["representation"],"issue_codes":expected["issue_codes"],
        "original_inventory":{p.name:pin(p) for p in output.iterdir()},
        "blocked_absent":["图像预览.md","plan.json","redrawn.svg"] if not expected["representation"] else []}
    save(work / "capture-spec.json", spec)
    records.append({"case":name,"output":str(output),"capture_spec":str(work / "capture-spec.json"),"representation":expected["representation"],"expected":str(expected_path),"exit_code":run.returncode})
    if name in args.dark_cases:
        dark_spec={**spec,"case":name+"_dark","theme":"dark","same_cli_output_as":name}
        save(work / "capture-spec-dark.json",dark_spec)
        records.append({"case":name+"_dark","output":str(output),"capture_spec":str(work / "capture-spec-dark.json"),"representation":expected["representation"],"expected":str(expected_path),"exit_code":run.returncode,"same_cli_output_as":name})
assert author_before == {p.relative_to(frozen).as_posix():pin(p) for p in frozen.rglob("*") if p.is_file()}
save(root / "prepared.json", {"status":"passed","copied_cli":str(cli),"cli":pin(cli),"checker":{"path":str(args.checker),**checker_before},
    "cases":records,"ordinary_cli_cases":sum("same_cli_output_as" not in r for r in records),"native_presentations":len(records),"frozen_authors":author_before,"scope":"final ordinary CLI and independent author checks; actual native capture still required"})
print(json.dumps({"status":"passed","root":str(root),"ordinary_cli_cases":sum("same_cli_output_as" not in r for r in records),"native_presentations":len(records)}))
