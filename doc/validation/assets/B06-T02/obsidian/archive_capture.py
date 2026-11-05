"""Archive completed actual Obsidian runs, preserving unsuccessful attempts."""
import argparse
import hashlib
import json
import shutil
import zipfile
from pathlib import Path


def pin(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def capture_files(root):
    files = [p for p in root.iterdir() if p.is_file()]
    files += [p for p in (root / "vault").rglob("*") if p.is_file()]
    files += [p for p in (root / "profile").iterdir() if p.is_file()]
    assert not any(p.is_symlink() for p in files)
    return sorted(files)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("input_root", type=Path)
parser.add_argument("capture_index", type=Path)
parser.add_argument("archive_directory", type=Path)
args = parser.parse_args()
index = load(args.capture_index)
out = args.archive_directory
assert len(index["cases"]) == 8
members = {}
cases = []
failed = []
with zipfile.ZipFile(out / "originals.zip", "x", zipfile.ZIP_DEFLATED) as archive:
    def add(path, name):
        assert name not in members
        data = path.read_bytes()
        archive.writestr(name, data)
        members[name] = pin(data)

    for path in sorted(args.input_root.rglob("*")):
        if not path.is_file() or path == args.input_root / "noetome":
            continue
        assert not path.is_symlink()
        add(path, "inputs/" + path.relative_to(args.input_root).as_posix())
    for name, root_name in index["cases"].items():
        root = Path(root_name)
        result = load(root / "render-result.json")
        visual = load(root / "visual-input.json")
        protection = load(root / "protection-cleanup.json")
        check = load(root / "independent-check.json")
        assert result["status"] == "passed" and len(result["rendered"]) == 2
        assert all(r["status"] == "passed" for r in result["rendered"])
        assert all(c["actualLocatorClick"] and c["fragmentVisible"] for c in result["clicks"])
        assert protection["original_audit_unchanged"] and protection["copied_notes_unchanged"]
        cleanup = protection["cleanup"]
        assert cleanup["ownedPidAbsent"] and cleanup["ownedListenerAbsent"] and cleanup["profileProcessCount"] == 0
        assert check["exit_code"] == 0
        assert result["identity"]["communityPlugins"] == []
        assert result["identity"]["appVersion"] == "1.6.7"
        assert all(pin((root / "vault" / n).read_bytes()) == expected for n, expected in visual["copied_note_pins"].items())
        for path in capture_files(root):
            add(path, "success/" + name + "/" + path.relative_to(root).as_posix())
        cases.append({
            "case": name, "root": root_name, "representation": visual["plan"]["representation"],
            "identity": result["identity"], "notes": visual["copied_note_pins"],
            "diagram_nodes": sum(len(r["actual"]["nodes"]) for r in result["rendered"]),
            "diagram_edges": sum(len(r["actual"]["edges"]) for r in result["rendered"]),
            "exact_preformatted_blocks": sum(len(r["actual"]["codeBlocks"]) for r in result["rendered"]),
            "ordered_lists": sum(len(r["actual"]["lists"]) for r in result["rendered"]),
            "ordered_items": sum(len(l["items"]) for r in result["rendered"] for l in r["actual"]["lists"]),
            "actual_fragment_clicks": len(result["clicks"]), "edge_directions": [d for r in result["rendered"] for d in r.get("edgeDirections", [])],
            "multiline_geometry": [g for r in result["rendered"] for g in r.get("multilineGeometry", [])],
            "independent_check": check, "cleanup": cleanup, "finished_utc": result["finishedUtc"]
        })
    for attempt in index["failed_attempts"]:
        root = Path(attempt["root"])
        result = load(root / "render-result.json")
        protection = load(root / "protection-cleanup.json")
        cleanup = protection["cleanup"]
        assert result["status"] == "failed"
        assert cleanup["ownedPidAbsent"] and cleanup["ownedListenerAbsent"] and cleanup["profileProcessCount"] == 0
        assert protection["original_audit_unchanged"] and protection["copied_notes_unchanged"]
        for path in capture_files(root):
            add(path, "failed/" + attempt["attempt"] + "/" + path.relative_to(root).as_posix())
        failed.append({**attempt, "actual_status": result["status"], "actual_error": result["error"], "cleanup": cleanup})
with zipfile.ZipFile(out / "originals.zip") as archive:
    assert archive.testzip() is None and set(archive.namelist()) == set(members)
    assert all(pin(archive.read(n)) == expected for n, expected in members.items())
archive = {"file": "originals.zip", **pin((out / "originals.zip").read_bytes()), "member_count": len(members),
           "members": members, "all_members_crc_sha_length_readback_passed": True,
           "binary_omission": "The 139 MB Linux CLI copy remains at the input provenance path; its full fingerprint and ordinary command receipts are retained. No executable bytes were substituted or presented as native Windows."}
write(out / "archive.json", archive)
screenshots = {
    "linear.png": ("linear", "note-0.png"), "branch.png": ("branch", "note-0.png"),
    "causes.png": ("causes", "note-0.png"), "promotes.png": ("promotes", "note-0.png"),
    "inhibits.png": ("inhibits", "note-0.png"), "complex-labels.png": ("complex_labels", "note-0.png"),
    "multiline-fallback.png": ("multiline_labels", "note-0-section-1.png"),
    "compatibility-fallback.png": ("compatibility_fallback", "note-0.png"),
    "actual-fragment.png": ("linear", "note-0-click-2.png")
}
for name, (case, source) in screenshots.items():
    assert not (out / name).exists()
    shutil.copyfile(Path(index["cases"][case]) / source, out / name)
summary = {"status": "actual_renderer_validation_passed", "scope": "eight frozen self-authored engineering source cases through actual ordinary CLI and installed Obsidian; scientific quality not_run, knowledge pending, published false",
           "input_preparation": load(args.input_root / "provenance.json"), "ordinary_cli_commands": len(load(args.input_root / "inputs.json")["commands"]),
           "case_count": len(cases), "note_count": sum(len(c["notes"]) for c in cases),
           "actual_fragment_click_count": sum(c["actual_fragment_clicks"] for c in cases),
           "diagram_node_count": sum(c["diagram_nodes"] for c in cases), "diagram_edge_count": sum(c["diagram_edges"] for c in cases),
           "exact_preformatted_block_count": sum(c["exact_preformatted_blocks"] for c in cases),
           "ordered_list_count": sum(c["ordered_lists"] for c in cases), "ordered_item_count": sum(c["ordered_items"] for c in cases),
           "cases": cases, "unsuccessful_capture_attempts": failed,
           "observation_method": "Actual reading-view scrolling measures virtualized blocks, then merges real DOM content by stable document position. All sections and all actual fragment clicks have screenshots. Fragment targets must become visible without corrective target scrolling, allowing one CSS pixel for fractional layout rounding.",
           "mermaid_version": "10.6.1", "mermaid_version_evidence": "../capability-probe/bundled-mermaid.json",
           "final_native_source_linkage": {"required": True, "result_path": "final-source-linkage.json", "not_implied_by_this_summary": True},
           "archive": {k: v for k, v in archive.items() if k != "members"},
           "representative_screenshots": {n: {"case": c, "source": s, **pin((out / n).read_bytes())} for n, (c, s) in screenshots.items()},
           "limitations": ["One installed Obsidian/Mermaid version pair with default clean profile; not a complete compatibility matrix.", "These finite engineering author cases do not demonstrate scientific correctness or arbitrary process-language extraction.", "Independent source/negative/tamper/native protection gates are separate evidence; actual rendering cannot replace them."]}
write(out / "summary.json", summary)
print(json.dumps({"status": "archived", "cases": len(cases), "notes": summary["note_count"], "clicks": summary["actual_fragment_click_count"], "members": len(members), "archive": archive["sha256"]}))
