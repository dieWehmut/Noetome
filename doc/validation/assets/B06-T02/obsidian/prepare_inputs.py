"""Run eight frozen successful cases through the ordinary current CLI.

This prepares fresh source-review/flow outputs for Obsidian observation. It is
not the final native audit and does not replace its rejection/guard matrix.
"""
import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path


def pin(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def save(path, data):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(data, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("cli", type=Path)
parser.add_argument("checker", type=Path)
parser.add_argument("new_root", type=Path)
args = parser.parse_args()
sys.path.insert(0, str(args.checker.resolve().parent))
spec = importlib.util.spec_from_file_location("process_observation_checker", args.checker)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
authors = checker.verify_fixture()
root = args.new_root.resolve()
root.mkdir()
(root / "cases").mkdir()
(root / "commands").mkdir()
shutil.copytree(checker.FIXTURE, root / "frozen-authors")
cli = root / ("noetome.exe" if args.cli.suffix == ".exe" else "noetome")
shutil.copyfile(args.cli, cli)
cli.chmod(0o755)
assert pin(cli) == pin(args.cli) and cli.stat().st_nlink == 1
save(root / "provenance.json", {"source_cli": str(args.cli.resolve()), "cli_copy": str(cli), "cli": pin(cli),
                               "checker": pin(args.checker), "python": sys.executable,
                               "head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                               "scope": "fresh ordinary CLI outputs for actual Obsidian; final native gates separate"})
commands = []


def command(label, argv):
    start = time.monotonic()
    result = subprocess.run([str(cli), *map(str, argv)], capture_output=True, timeout=60)
    prefix = root / "commands" / f"{len(commands):03d}-{label}"
    prefix.with_suffix(".stdout.bin").write_bytes(result.stdout)
    prefix.with_suffix(".stderr.bin").write_bytes(result.stderr)
    receipt = {"argv": [str(cli), *map(str, argv)], "exit_code": result.returncode,
               "elapsed_seconds": time.monotonic() - start,
               "stdout": pin(prefix.with_suffix(".stdout.bin")), "stderr": pin(prefix.with_suffix(".stderr.bin"))}
    save(prefix.with_suffix(".json"), receipt)
    commands.append(receipt)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    return result


names = ["linear", "branch", "causes", "promotes", "inhibits", "complex_labels", "multiline_labels", "compatibility_fallback"]
for name in names:
    author = next(case for case in authors["cases"] if case["name"] == name)
    folder = root / "cases" / name
    folder.mkdir()
    source = folder / "source.txt"
    source.write_bytes((root / "frozen-authors" / author["source_file"]).read_bytes())
    profile = folder / "profile.json"
    profile.write_bytes((root / "frozen-authors" / author["profile_file"]).read_bytes())
    expected = root / "frozen-authors" / author["expected_file"]
    project = folder / "project"
    case = folder / "case"
    case.mkdir()
    command(name + "-init", ["project", "init", project])
    imported = command(name + "-import", ["source", "import", project, source])
    import_id = json.loads(imported.stdout)["import_id"]
    actual = checker.actual_source(project, import_id, {"kind": "text", "ordinal": 0})
    assert actual["units"][0]["text"].encode() == source.read_bytes()
    gold = case / "gold.json"
    gold.write_bytes((root / "frozen-authors" / author["gold_file"]).read_bytes())
    save(case / "process-review.json", {"schema_version": 1, "purpose": "process_source_review",
         "project_id": actual["binding"]["project_id"], "import_id": import_id,
         "selection": [{"kind": "text", "ordinal": 0}], "gold_file": "gold.json", "gold_sha256": pin(gold)["sha256"]})
    review = folder / "review"
    vault = folder / "Vault"
    command(name + "-review", ["knowledge", "process-review", project, case, review])
    checker.verify_review(project, case, review, expected)
    command(name + "-flow", ["knowledge", "flow", project, case, review, profile, vault])
    report = checker.verify_flow(project, case, review, profile, vault, expected)
    assert report["status"] == "passed"
    save(folder / "independent.json", {"status": "passed", "representation": report["representation"], "expected": pin(expected)})
assert pin(cli) == pin(args.cli)
save(root / "inputs.json", {"status": "passed", "cases": names, "commands": commands, "cli": pin(cli),
                           "scope": "eight successful product chains; actual Obsidian and final native gates follow"})
print(json.dumps({"status": "passed", "root": str(root), "cases": len(names), "commands": len(commands)}))
