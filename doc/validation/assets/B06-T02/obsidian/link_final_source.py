"""Bind actual Obsidian captures to the completed exact-source native CLI audit."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def pin(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def wsl(path):
    return Path(subprocess.check_output(["wslpath", "-u", str(path)], text=True).strip())


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("native_root", type=Path)
parser.add_argument("capture_root", type=Path, help="case directory or saved capture-roots.json")
parser.add_argument("new_result", type=Path)
args = parser.parse_args()
native = args.native_root
final = load(native / "native-final.json")
snapshot = load(native / "snapshot.json")
config = load(native / "root.json")
assert final["head"] == snapshot["head"] == config["snapshot_head"]
required = ["fmt", "workspace-check", "workspace-tests", "python-full", "presentation-audit", "process-flow-audit"]
assert final["required_latest_exits"] == dict.fromkeys(required, 0)
repo = wsl(config["repo_native"])
assert all(pin(repo / name) == expected for name, expected in snapshot["files"].items())
assert pin(repo / "target/debug/noetome.exe") == final["cli"]
audit = final["audits"]["process-flow-audit"]
assert audit["result"]["status"] == "passed"
audit_root = wsl(audit["root"])
cases = []
capture_paths = load(args.capture_root)["cases"] if args.capture_root.is_file() else None
for name in ["linear", "branch", "causes", "promotes", "inhibits", "complex_labels", "multiline_labels", "compatibility_fallback"]:
    capture = Path(capture_paths[name]) if capture_paths else args.capture_root / name
    visual = load(capture / "visual-input.json")
    result = load(capture / "render-result.json")
    protection = load(capture / "protection-cleanup.json")
    assert result["status"] == "passed"
    assert result["caseName"] == visual["case_name"] == name
    assert protection["original_audit_unchanged"] and protection["copied_notes_unchanged"]
    assert protection["cleanup"]["ownedPidAbsent"] and protection["cleanup"]["ownedListenerAbsent"]
    generated = audit_root / "cases" / name / "Vault"
    final_plan = load(generated / "plan.json")
    assert load(generated / "flow.json")["status"] == "passed"
    assert pin(audit_root / "frozen-authors" / ("expected-" + name + ".json")) == visual["expected_pin"]
    # Request UUIDs can differ between fresh invocations. Every semantic,
    # rendering and note-content field must still be exactly identical.
    semantic_keys = ["document", "profile", "representation", "decision", "topological_order", "diagram", "notes"]
    assert all(final_plan[key] == visual["plan"][key] for key in semantic_keys)
    notes = {}
    for note in final_plan["notes"]:
        filename = note["output_path"]
        expected = visual["copied_note_pins"][filename]
        assert pin(generated / filename) == pin(capture / "vault" / filename) == expected
        notes[filename] = expected
    cases.append({"case": name, "representation": final_plan["representation"],
                  "native_generated_root": str(generated), "capture_root": str(capture),
                  "notes": notes, "all_markdown_bytes_identical": True,
                  "all_semantic_plan_fields_identical": True,
                  "actual_clicks": len(result["clicks"]), "render_result": pin(capture / "render-result.json"),
                  "native_plan": pin(generated / "plan.json")})
record = {"status": "passed", "head": final["head"], "native_cli": final["cli"],
          "native_source_files_rechecked": len(snapshot["files"]),
          "native_six_gate_exits": final["required_latest_exits"], "cases": cases,
          "case_count": len(cases), "note_count": sum(len(c["notes"]) for c in cases),
          "actual_click_count": sum(c["actual_clicks"] for c in cases),
          "scope": "Actual installed Obsidian captures linked byte-for-byte to the final native CLI audit. Engineering source/rendering validation only; scientific quality not_run and published false.",
          "limitations": ["Installed Obsidian 1.6.7 and bundled Mermaid 10.6.1 only; no complete version compatibility matrix.", "Targeted NTFS/live-handle protection gates are recorded separately."]}
with args.new_result.open("x", encoding="utf-8") as stream:
    json.dump(record, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(json.dumps({"status": "passed", "head": final["head"], "cases": len(cases), "notes": record["note_count"]}))
