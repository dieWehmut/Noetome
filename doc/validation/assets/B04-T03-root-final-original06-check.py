"""One saved-input check on the final workspace-built CLI; no matrix/model rerun."""
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
sys.path.insert(0, str(REPO / "scripts"))
import verify_extraction_knowledge as independent
spec = importlib.util.spec_from_file_location("selected_input_receipts", SDK / "capture-inputs.py")
receipts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(receipts)
facts_before = receipts.capture()
assert facts_before == json.loads((SDK / "root-inputs-before.json").read_bytes())["facts"]
live = REPO / "target/compiler-temp/b04-t02-live-v2-d62b632edc08458eb7dcccdfc7a68556"
project = live / "source-project"
import_id = "9f0abe91-99c5-44d0-8e15-2e9ea48da551"
candidate_bytes = (live / "api-live-06/candidates.json").read_bytes()
gold_bytes = (live / "gold-before.json").read_bytes()
source = independent.actual_source(project, import_id, {"kind": "text", "ordinal": 0})
case_original = REPO / "target/compiler-temp/B04-T03-root-original06-01/case-original-sixth"
inputs = {name: (case_original / name).read_bytes() for name in ["review.json", "gold.json", "decision.json", "candidates.json"]}
assert inputs["gold.json"] == gold_bytes and inputs["candidates.json"] == candidate_bytes
root = REPO / "target/compiler-temp/B04-T03-root-final-original06-01"
root.mkdir()
case = root / "case"
case.mkdir()
for name, data in inputs.items():
    (case / name).write_bytes(data)
binary = REPO / "target/debug/noetome.exe"
binary_before = independent.fingerprint(binary.read_bytes())
command = [str(binary), "extract", "verify", str(project), import_id, str(case), str(root / "output")]
started = datetime.now(timezone.utc).isoformat()
run = subprocess.run(command, capture_output=True, timeout=30, check=False)
(root / "stdout.log").write_bytes(run.stdout)
(root / "stderr.log").write_bytes(run.stderr)
verified = independent.verify_report(root / "output", candidate_bytes, gold_bytes, source, inputs)
report = independent.strict_json((root / "output/verification.json").read_bytes())
assert independent.strict_json(run.stdout) == report and not run.stderr
assert run.returncode == 0 and verified["independently_passed"] and verified["expected_atoms"] == 1
facts_after = receipts.capture()
assert facts_after == facts_before
assert independent.fingerprint(binary.read_bytes()) == binary_before
record = {
    "schema_version": 1, "started_at": started, "finished_at": datetime.now(timezone.utc).isoformat(),
    "command": command, "exit_code": run.returncode, "binary": binary_before,
    "stdout": independent.fingerprint(run.stdout), "stderr": independent.fingerprint(run.stderr),
    "verification": verified, "original_input_facts_before": facts_before, "original_input_facts_after": facts_after,
    "source_candidate_gold_response_config_budget_unchanged": True,
    "new_model_calls": 0, "paid_calls": 0, "ordinary_cli_calls": 1,
    "reason": "Workspace test rebuilt CLI with different binary SHA; bind final original06 behavior to the final program without rerunning the already-passed52-scenario matrix",
    "source_helper": independent.fingerprint(Path(__file__).read_bytes()),
    "independent_verifier": independent.fingerprint((REPO / "scripts/verify_extraction_knowledge.py").read_bytes()),
    "passed": True,
}
with (root / "audit.json").open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(record, stream, ensure_ascii=False, indent=2); stream.write("\n")
print(json.dumps({"audit": str(root / "audit.json"), "ordinary_cli_calls": 1, "binary": binary_before, "verification": verified, "original_inputs_unchanged": True, "passed": True}, ensure_ascii=False))
