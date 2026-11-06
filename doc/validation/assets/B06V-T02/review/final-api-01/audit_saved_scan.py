"""Fresh reviewer reads preserved public scan outputs; no CLI or HTTP work."""
from pathlib import Path
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[6]
OUT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "scripts"))
import verify_scan_formula as audit


def pin(path):
    raw = Path(path).read_bytes()
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


source_names = ["crates/noetome-cli/src/scan_api.rs", "crates/noetome-cli/src/scan_formula.rs",
                "crates/noetome-cli/src/scan_input.rs", "crates/noetome-cli/src/scan_model.rs",
                "crates/noetome-cli/src/scan_render.rs", "scripts/verify_scan_formula.py"]
source_before = {name: pin(ROOT / name) for name in source_names}
fixture = audit.verify_fixture()
matrix = ROOT / "doc/validation/assets/B06V-T02/independent/api-cli-final-01"
summary = json.loads((matrix / "summary.json").read_bytes())
records = []
for name in ("clear-native", "clear-original", "clear-no-target", "unknown-sign", "unknown-subscript", "raw-overbar"):
    record = next(r for r in summary["records"] if r["name"] == name)
    selected = matrix / name
    expected = ROOT / ("fixtures/scan-formula/expected-" + record["author_state"] + ".json")
    before = {p.relative_to(selected).as_posix(): pin(p) for p in selected.rglob("*") if p.is_file()}
    result = audit.verify_output(selected / "case", selected / "api.json", selected / "profile.json",
                                 selected / "vault", expected)
    after = {p.relative_to(selected).as_posix(): pin(p) for p in selected.rglob("*") if p.is_file()}
    assert before == after
    assert result["real_model_calls"] == 0
    records.append({"case": name, "result": result, "files_read": len(before),
                    "selected_before_after": before})
source_after = {name: pin(ROOT / name) for name in source_names}
assert source_before == source_after
receipt = {"schema_version": 1, "status": "passed", "scope": "fresh_read_only_existing_public_outputs",
           "fixture": fixture, "saved_outputs": records, "saved_output_count": len(records),
           "source_before": source_before, "source_after": source_after, "source_unchanged": True,
           "matrix_original": pin(matrix / "summary.json"), "review_script": pin(Path(__file__)),
           "new_cli_invocations": 0, "new_http_requests": 0, "real_model_calls": 0,
           "current_credentials_read": False, "current_ledger_read": False,
           "full_suites_executed": False, "live_recognition": "not_run", "scientific_quality": "not_run"}
target = OUT / "saved-output-readback.json"
assert not target.exists()
target.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"status": "passed", "saved_output_count": len(records), "frozen_author_files": fixture["frozen_files"], "new_cli_invocations": 0}))
