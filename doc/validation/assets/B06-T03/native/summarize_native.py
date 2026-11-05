"""Summarize only recorded app results, source fingerprints and cleanup receipts."""
import argparse
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_bytes())


def cleanup_ok(value):
    return value["ownedPidAbsent"] and value["ownedListenerAbsent"] and value["profileProcessCount"] == 0


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("capture_roots", type=Path)
parser.add_argument("label_audit", type=Path)
parser.add_argument("new_output", type=Path)
args = parser.parse_args()
roots = read(args.capture_roots)
cases = []
for name, value in roots["successful_cases"].items():
    root = Path(value)
    result = read(root / "result.json")
    cleanup = read(root / "cleanup-result.json")
    protected = read(root / "protection-cleanup.json")
    visual = read(root / "visual-input.json")
    spec = visual["spec"]
    assert result["status"] == "passed" and cleanup_ok(cleanup) and not cleanup["emergencyTerminated"]
    assert protected["selected_originals_unchanged"] and protected["copied_originals_unchanged"] and protected["capture_spec_unchanged"]
    math = [m for n in result["notes"] for m in n["actual"]["math"]]
    images = [i for n in result["notes"] for i in n["actual"]["images"]]
    cases.append({"case": name, "capture_root": value, "generated_source": visual["source"],
                  "identity": result["identity"], "representation": spec["representation"], "issue_codes": spec["issue_codes"],
                  "mathjax": result.get("mathjax"), "math_count": len(math),
                  "visible_glyph_count": sum(sum(g["classes"] != "mjx-c2061" for g in m["glyphs"]) for m in math),
                  "invisible_function_apply_markers": sum(sum(g["classes"] == "mjx-c2061" for g in m["glyphs"]) for m in math),
                  "image_count": len(images), "local_attachment_clicks": result["clicks"],
                  "native_original_fingerprints": result["openedOriginalFiles"], "copied_generated_files": visual["source_pins"],
                  "cleanup": cleanup, "protection": protected})
failed = []
processes = []
for round_name, root_value in roots["captures"].items():
    for child in Path(root_value).iterdir():
        if not child.is_dir():
            continue
        result = read(child / "result.json")
        cleanup = read(child / "cleanup-result.json")
        assert cleanup_ok(cleanup)
        processes.append({"round": round_name, "case": child.name, "capture_root": str(child),
                          "result_status": result["status"], "cleanup": cleanup})
        if result["status"] != "passed":
            failed.append({"round": round_name, "case": child.name, "capture_root": str(child),
                           "reason": result["error"]["message"].split("\n")[0]})
probes = []
for child in Path(roots["probe"]["mathjax"]).iterdir():
    if child.is_dir() and (child / "cleanup-result.json").exists():
        cleanup = read(child / "cleanup-result.json")
        assert cleanup_ok(cleanup)
        result = read(child / "result.json") if (child / "result.json").exists() else None
        probes.append({"attempt": child.name, "result_status": result["status"] if result else None,
                       "reason": result.get("error", {}).get("message", "").split("\n")[0] if result else "",
                       "cleanup": cleanup})
summary = {
    "status": "actual_renderer_validation_passed",
    "scope": "ten public self-authored engineering cases from immutable ordinary CLI; scientific quality not_run, knowledge pending, published false",
    "cli": read(Path(roots["inputs"]["corrected-native-spec"]) / "prepared.json")["cli"],
    "successful_case_count": len(cases),
    "successful_native_math_cases": sum(c["representation"] == "native_math" for c in cases),
    "successful_formula_image_cases": sum(c["representation"] == "formula_image" for c in cases),
    "blocked_report_cases": sum(c["representation"] is None for c in cases),
    "actual_math_count": sum(c["math_count"] for c in cases),
    "actual_visible_glyph_count": sum(c["visible_glyph_count"] for c in cases),
    "actual_local_attachment_click_count": sum(len(c["local_attachment_clicks"]) for c in cases),
    "native_metadata_label_groups": read(args.label_audit), "cases": cases,
    "all_product_process_attempts": processes,
    "preserved_unsuccessful_product_attempts": failed,
    "owned_emergency_cleanup_count": sum(bool(p["cleanup"]["emergencyTerminated"]) for p in processes),
    "capability_probe_attempts": probes,
    "initial_preparation_failure": {
        "root": roots["inputs"]["initial-preparation-failure"],
        "reason": "Preparation copied only expected.json but checker requires the whole frozen author manifest; corrected by copying the selected complete public author fixture inventory."},
    "limitations": [
        "Only one installed Obsidian1.6.7/MathJax3.2.2 version combination tested.",
        "Finite author fixtures and token/render identity do not establish scientific correctness, OCR reconstruction or general PDF author source recovery.",
        "Handwritten MathJax capability probe is separately labeled and is not product output.",
        "One failed initial compatibility capture needed emergency termination of its own command-line-verified PID; all ten accepted captures exited normally."]}
with args.new_output.open("x", encoding="utf8") as f:
    json.dump(summary, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(json.dumps({k: summary[k] for k in ["status", "successful_case_count", "actual_math_count", "actual_visible_glyph_count", "actual_local_attachment_click_count", "owned_emergency_cleanup_count"]}))
