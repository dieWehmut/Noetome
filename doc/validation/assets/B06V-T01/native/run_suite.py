"""Run each explicitly prepared case in its own fresh installed Obsidian instance."""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("prepared_input", type=Path)
parser.add_argument("new_root", type=Path)
parser.add_argument("native_python", type=Path)
parser.add_argument("obsidian", type=Path)
parser.add_argument("powershell", type=Path)
parser.add_argument("playwright", type=Path)
parser.add_argument("--cases", nargs="*")
args = parser.parse_args()
root = args.new_root
root.mkdir()
prepared = json.loads((args.prepared_input / "prepared.json").read_bytes())
records = []
def local(value):
    return subprocess.check_output(["wslpath", "-u", value], text=True).strip() if re.match(r"^[A-Za-z]:[\\/]",value) else value
for case in prepared["cases"]:
    if args.cases is not None and case["case"] not in args.cases:
        continue
    launcher = Path(__file__).with_name("launch_capture.py")
    cmd = [sys.executable, str(launcher),
           local(case["output"]), local(case["capture_spec"]), str(root / case["case"]),
           str(args.native_python), str(args.obsidian), str(args.powershell), str(args.playwright),
           "--capture-script", str(Path(__file__).with_name("capture_image.mjs"))]
    print(json.dumps({"case": case["case"], "state": "capturing"}), flush=True)
    run = subprocess.run(cmd, capture_output=True, timeout=400)
    (root / (case["case"] + ".stdout.bin")).write_bytes(run.stdout)
    (root / (case["case"] + ".stderr.bin")).write_bytes(run.stderr)
    result_file = root / case["case"] / "result.json"
    result = json.loads(result_file.read_bytes()) if result_file.exists() else None
    record = {"case": case["case"], "argv": cmd, "exit_code": run.returncode,
              "work": str(root / case["case"]), "result_status": result["status"] if result else None}
    records.append(record)
    with (root / "suite.json").open("w", encoding="utf8") as f:
        json.dump({"status": "running", "prepared_input": str(args.prepared_input), "cases": records}, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(json.dumps({"case": case["case"], "exit_code": run.returncode, "result_status": record["result_status"]}), flush=True)
status = "passed" if records and all(r["exit_code"] == 0 for r in records) else "failed"
with (root / "suite.json").open("w", encoding="utf8") as f:
    json.dump({"status": status, "prepared_input": str(args.prepared_input), "cases": records}, f, ensure_ascii=False, indent=2)
    f.write("\n")
raise SystemExit(0 if status == "passed" else 1)
