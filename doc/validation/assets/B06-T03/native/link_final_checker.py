"""Recheck accepted native source outputs with the final independent checker."""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path


def pin(path):
    b = path.read_bytes()
    return {"sha256": hashlib.sha256(b).hexdigest(), "byte_length": len(b)}


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("capture_roots", type=Path)
parser.add_argument("checker", type=Path)
parser.add_argument("repo", type=Path)
parser.add_argument("new_output_root", type=Path)
args = parser.parse_args()
root = args.new_output_root
root.mkdir()
roots = json.loads(args.capture_roots.read_bytes())
records = []
checker_pin = pin(args.checker)
product_paths = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/noetome-cli/Cargo.toml",
                 "crates/noetome-cli/src/lib.rs", "crates/noetome-cli/src/main.rs", "crates/noetome-cli/src/extraction_output.rs"]
product_paths += [p.relative_to(args.repo).as_posix() for p in sorted((args.repo / "crates/noetome-cli/src").glob("formula*.rs"))]
source_pins = {p: pin(args.repo / p) for p in product_paths}
for name, value in roots["successful_cases"].items():
    capture = Path(value)
    visual = json.loads((capture / "visual-input.json").read_bytes())
    source = Path(visual["source"])
    case = source.parent / "case"
    profile = source.parent / "profile.json"
    expected = Path(visual["spec"]["expected_file"])
    current = {p.name: pin(p) for p in source.iterdir()}
    assert current == visual["source_pins"]
    copied = {p: pin(capture / "vault" / p) for p in current}
    assert copied == current
    result = json.loads((capture / "result.json").read_bytes())
    assert result["status"] == "passed" and result["openedOriginalFiles"] == current
    cmd = [sys.executable, str(args.checker), "check", str(case), str(profile), str(source), str(expected)]
    run = subprocess.run(cmd, capture_output=True, timeout=120)
    (root / (name + ".stdout.bin")).write_bytes(run.stdout)
    (root / (name + ".stderr.bin")).write_bytes(run.stderr)
    assert run.returncode == 0, run.stderr.decode("utf8", "replace")
    checked = json.loads(run.stdout)
    assert checked["status"] == "passed"
    assert pin(args.checker) == checker_pin
    records.append({"case": name, "argv": cmd, "exit_code": run.returncode, "checker_result": checked,
                    "source": str(source), "capture": str(capture), "generated_and_copied_and_native_fingerprints_equal": True,
                    "files": current, "expected": pin(expected), "capture_result": pin(capture / "result.json"),
                    "stdout": pin(root / (name + ".stdout.bin")), "stderr": pin(root / (name + ".stderr.bin"))})
assert all(pin(args.repo / p) == fingerprint for p, fingerprint in source_pins.items())
with (root / "final-checker-linkage.json").open("x", encoding="utf8") as f:
    json.dump({"status": "passed", "checker": {"path": str(args.checker), **checker_pin},
               "case_count": len(records), "cases": records, "product_source_fingerprints": source_pins,
               "scope": "final independent checker against the exact untouched ordinary CLI source Vaults used by accepted native captures; generated, copied and native readback bytes are identical"}, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(json.dumps({"status": "passed", "case_count": len(records), "checker": checker_pin}))
