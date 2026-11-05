"""Link native stable-build note bytes to already captured real Obsidian notes."""
import argparse
import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def pin(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def local(path):
    return Path(subprocess.check_output(["wslpath", "-u", path], text=True).strip())


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("native_root", type=Path)
parser.add_argument("stable_head")
args = parser.parse_args()
root = args.native_root
output = Path(__file__).with_name("final-source-linkage.json")
assert not output.exists(), "do not overwrite saved final evidence"
summary_path = Path(__file__).with_name("summary.json")
summary = read(summary_path)
final = read(root / "native-final.json")
snapshot = read(root / "snapshot.json")
assert final["head"] == snapshot["head"] == args.stable_head
assert all(code == 0 for code in final["required_latest_exits"].values())
assert final["tracked_before_after_match"]
assert final["presentation_audit"]["status"] == "passed"
repo = root / "repo"
assert pin(repo / "target/debug/noetome.exe") == final["cli"]
for name, expected in snapshot["files"].items():
    assert pin(repo / name) == expected, "final stable snapshot source changed: " + name
test_receipt = read(root / "workspace-tests-attempt-2.json")
assert test_receipt["snapshot_head"] == args.stable_head and test_receipt["exit_code"] == 0
assert test_receipt["cli_after"] == final["cli"]
test_log = root / "workspace-tests-attempt-2.stdout.bin"
assert pin(test_log) == {key: test_receipt["stdout"][key] for key in ("sha256", "byte_length")}
raw = test_log.read_text(encoding="utf-8")
assert "test present_preserves_multiline_display_name_and_stable_detail_link ... ok" in raw
matched = []
for native_path in re.findall(r"prepared_presentation_root=(.*?) case=two_objects", raw):
    preview = local(native_path) / "preview-Vault"
    if (preview / "plan.json").exists():
        plan = read(preview / "plan.json")
        if any(note["title"] == "样例\nA\t（两行标签）" for note in plan["notes"]):
            matched.append(preview)
assert len(matched) == 1, "identify the actual multiline test by its full label, not output order"
audit_root = local(final["presentation_audit_root"])
rows = []
for case in summary["cases"]:
    name = case["case_name"]
    preview = matched[0] if name == "multiline_label" else audit_root / name / "preview-Vault"
    plan = read(preview / "plan.json")
    report = read(preview / "presentation.json")
    assert report["status"] == "passed"
    assert plan["knowledge_status"] == "pending" and plan["scientific_quality"] == "not_run" and not plan["published"]
    expected = case["copied_note_pins"]
    assert {n["output_path"] for n in plan["notes"]} == set(expected), "same complete note inventory"
    actual = {name: pin(preview / name) for name in expected}
    assert actual == expected, "final native bytes differ from real captured Obsidian notes: " + name
    rows.append({"case_name": name, "native_preview": str(preview), "note_pins": actual,
                 "all_note_bytes_match_captured_obsidian": True, "native_plan": pin(preview / "plan.json"),
                 "native_report": pin(preview / "presentation.json")})
result = {
    "status": "passed", "checked_utc": datetime.now(timezone.utc).isoformat(),
    "stable_head": args.stable_head, "native_cli": final["cli"],
    "native_final": pin(root / "native-final.json"), "native_snapshot": pin(root / "snapshot.json"),
    "snapshot_file_count_rechecked": len(snapshot["files"]),
    "workspace_test_receipt": pin(root / "workspace-tests-attempt-2.json"),
    "workspace_test_stdout": pin(test_log), "captured_obsidian_summary": pin(summary_path),
    "case_count": len(rows), "note_count": sum(len(row["note_pins"]) for row in rows),
    "cases": rows,
    "scope": "Final stable native CLI generated exactly the Markdown bytes already tested in installed Obsidian 1.6.7; no new rendering or complete compatibility matrix claim",
}
with output.open("x", encoding="utf-8") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(json.dumps({key: result[key] for key in ("status", "stable_head", "case_count", "note_count")}))
