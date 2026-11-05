"""Generate ordinary CLI formula outputs and independently verify each before capture."""
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path


def pin(path):
    b = path.read_bytes()
    return {"sha256": hashlib.sha256(b).hexdigest(), "byte_length": len(b)}


def write(path, value):
    with path.open("x", encoding="utf8") as f:
        json.dump(value, f, ensure_ascii=False, indent=2)
        f.write("\n")


mi = lambda s: ["mi", s]
mo = lambda s: ["mo", s]
mn = lambda s: ["mn", s]
native = ["row", [
    ["msub", [mi("C"), mi("i")]], mo("("), mi("t"), mo(")"), mo("="), mo("−"),
    ["mfrac", [mi("a"), mi("b")]], ["msup", [mi("t"), mn("2")]]]]
relational = ["row", [["msubsup", [mi("x"), mi("i"), mn("2")]], mo("≤"), ["mfrac", [mi("a"), mi("b")]]]]
exponential = ["row", [
    ["msub", [mi("C"), mi("i")]], mo("("), mi("t"), mo(")"), mo("="),
    ["msub", [mi("C"), mn("0")]], mi("exp"), mo("\u2061"), mo("("), mo("−"), mi("k"), mi("t"), mo(")")]]
# These native trees are hand written from the independently authored input
# equations. They are never learned from plan.json, compiled.tex or renderer DOM.
trees = {"native": native, "compatibility": native, "native_relational": relational, "native_exp": exponential}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("cli", type=Path)
parser.add_argument("checker", type=Path)
parser.add_argument("fixture_root", type=Path)
parser.add_argument("new_root", type=Path)
parser.add_argument("--cases", nargs="*", default=[
    "native", "native_relational", "native_exp", "compatibility", "image_fallback", "region_only",
    "compatibility_disabled", "unknown_symbol", "unreadable_region", "curve_no_data"])
args = parser.parse_args()
root = args.new_root
root.mkdir()
(root / "commands").mkdir()
(root / "cases").mkdir()
shutil.copytree(args.fixture_root, root / "frozen-authors")
cli = root / args.cli.name
shutil.copyfile(args.cli, cli)
cli.chmod(0o755)
authors = json.loads((args.fixture_root / "authors.json").read_bytes())
entries = {e["name"]: e for e in authors["cases"]}
records = []


def command(name, argv):
    run = subprocess.run([str(p) for p in argv], capture_output=True, timeout=120)
    (root / "commands" / (name + ".stdout.bin")).write_bytes(run.stdout)
    (root / "commands" / (name + ".stderr.bin")).write_bytes(run.stderr)
    receipt = {"argv": [str(p) for p in argv], "exit_code": run.returncode,
               "stdout": pin(root / "commands" / (name + ".stdout.bin")),
               "stderr": pin(root / "commands" / (name + ".stderr.bin"))}
    write(root / "commands" / (name + ".json"), receipt)
    return run, receipt


for name in args.cases:
    e = entries[name]
    work = root / "cases" / name
    work.mkdir()
    case = work / "case"
    shutil.copytree(args.fixture_root / e["case_directory"], case)
    profile = work / "profile.json"
    expected_file = root / "frozen-authors" / e["expected_file"]
    profile.write_bytes((args.fixture_root / e["profile_file"]).read_bytes())
    assert expected_file.read_bytes() == (args.fixture_root / e["expected_file"]).read_bytes()
    expected = json.loads(expected_file.read_bytes())
    output = work / "generated-Vault"
    run, cli_receipt = command(name + "-formula", [cli, "knowledge", "formula", case, profile, output])
    exit_code = 0 if expected["representation"] else 3
    assert run.returncode == exit_code, run.stderr.decode("utf8", "replace")
    check, check_receipt = command(name + "-independent-check", [sys.executable, args.checker, "check", case, profile, output, expected_file])
    assert check.returncode == 0, check.stderr.decode("utf8", "replace")
    report = json.loads((output / "formula.json").read_bytes())
    assert report["status"] == ("passed" if expected["representation"] else "needs_review")
    semantics = expected["semantics"]
    metadata = [expected["case"]["title"]]
    for s in semantics["symbols"]:
        metadata.extend([s["symbol"], s["definition"] or "", s["unit"] or "来源未给出单位"])
    for field in ["conditions", "ranges", "exceptions"]:
        metadata.extend(semantics[field])
    images = []
    if expected["representation"] == "formula_image":
        region = expected["case"]["region"]
        images.append({"file": "original.png", "width": region["bounds"][2], "height": region["bounds"][3]})
    if expected["representation"]:
        note = {"file": "公式预览.md", "math_trees": [trees[name]] if expected["representation"] == "native_math" else [],
                "required_texts": metadata, "preformatted_exact": metadata, "images": images,
                "link_destinations": ["original.png"] if expected["case"]["region"] else [],
                "click_attachments": ["original.png"] if expected["case"]["region"] else []}
    else:
        assert not (output / "公式预览.md").exists() and not (output / "plan.json").exists()
        note = {"file": "formula.md", "math_trees": [], "images": [], "required_texts": ["needs_review", *expected["issue_codes"]]}
    spec = {"case": name, "scope": "actual ordinary CLI output after independent frozen-author validation; not scientific verification",
            "mathjax_version": "3.2.2", "require_mathjax_runtime": expected["representation"] == "native_math",
            "notes": [note], "expected_fingerprint": pin(expected_file), "expected_file": str(expected_file),
            "cli_receipt": cli_receipt, "checker_receipt": check_receipt,
            "original_inventory": {p.name: pin(p) for p in sorted(output.iterdir())},
            "source_fingerprints": expected["source_pins"], "semantic_source": semantics,
            "representation": expected["representation"], "issue_codes": expected["issue_codes"],
            "blocked_absent": ["公式预览.md", "plan.json", "compiled.tex"] if not expected["representation"] else []}
    write(work / "capture-spec.json", spec)
    records.append({"case": name, "output": str(output), "capture_spec": str(work / "capture-spec.json"),
                    "expected": str(expected_file), "representation": expected["representation"], "exit_code": run.returncode})
write(root / "prepared.json", {"status": "passed", "cli_source": str(args.cli), "copied_cli": str(cli),
                              "cli": pin(cli), "checker": {"path": str(args.checker), **pin(args.checker)},
                              "cases": records, "scope": "ordinary CLI plus independent checks; actual Obsidian still required"})
print(json.dumps({"status": "passed", "root": str(root), "case_count": len(records)}))
