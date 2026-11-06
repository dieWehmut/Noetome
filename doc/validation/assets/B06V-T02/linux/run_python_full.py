"""Capture actual full Python tests and reject changing selected source bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def pin(path):
    raw = path.read_bytes()
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("repository", type=Path)
parser.add_argument("binary", type=Path)
parser.add_argument("new_output", type=Path)
args = parser.parse_args()
root = args.repository.resolve()
out = args.new_output.resolve()
out.mkdir()
names = []
for prefix in ("scripts", "tests", "crates", "fixtures", "sidecars"):
    names.extend(p for p in (root / prefix).rglob("*")
                 if p.is_file() and not p.is_symlink() and "__pycache__" not in p.parts and p.suffix != ".pyc")


def pins():
    return {p.relative_to(root).as_posix(): pin(p) for p in names}


before = pins()
binary = args.binary.resolve()
binary_before = pin(binary)
environment = dict(os.environ)
environment.pop("NOETOME_TANGMAO_API_KEY", None)
environment["NOETOME_TEST_BINARY"] = str(binary)
argv = [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py"]
started = time.monotonic()
with (out / "stdout.bin").open("xb") as stdout, (out / "stderr.bin").open("xb") as stderr:
    result = subprocess.run(argv, cwd=root, env=environment, stdout=stdout, stderr=stderr, timeout=300)
after = pins()
unchanged = before == after and binary_before == pin(binary)
receipt = {"schema_version": 1, "argv": argv, "cwd": str(root), "python_version": sys.version,
           "exit_code": result.returncode, "elapsed_seconds": time.monotonic() - started,
           "source_before": before, "source_after": after, "source_before_after_equal": unchanged,
           "selected_cli": {"path": str(binary), **binary_before}, "runner": pin(Path(__file__).resolve()),
           "real_model_calls": 0, "user_budget_mutations": 0,
           "status": "passed" if result.returncode == 0 and unchanged else "failed"}
(out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"status": receipt["status"], "exit_code": result.returncode,
                  "source_before_after_equal": unchanged, "source_files": len(before)}))
print((out / "stderr.bin").read_text(errors="replace")[-2000:])
assert result.returncode == 0 and unchanged
