"""Archive an already completed isolated whitespace probe without rerunning it."""
import argparse
import hashlib
import json
import shutil
import zipfile
from pathlib import Path


def pin(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("completed_work", type=Path)
parser.add_argument("archive_directory", type=Path)
args = parser.parse_args()
work = args.completed_work
out = args.archive_directory
assert work.is_dir() and out.is_dir()
result = json.loads((work / "result.json").read_text())
protection = json.loads((work / "protection-cleanup.json").read_text())
author = json.loads((work / "author-before.json").read_text())
assert result["status"] == "completed" and len(result["notes"]) == 11
assert protection["ownedPidAbsent"] and protection["ownedListenerAbsent"]
assert protection["markdown_unchanged"]
current = {p.name: pin(p.read_bytes()) for p in (work / "vault").glob("*.md")}
assert current == author["markdown_pins"] == protection["markdown_after"]
assert result["identity"]["communityPlugins"] == []
assert len(result["navigation"]) == 2 and all(n["actualClick"] for n in result["navigation"])
parent_pins = {
    name: pin((out.parent / name).read_bytes())
    for name in ["originals.zip", "archive.json", "summary.json"]
}
files = [p for p in work.iterdir() if p.is_file()]
files += [p for p in (work / "vault").rglob("*") if p.is_file()]
# The profile is owned by this test. Preserve direct configuration and log files;
# browser GPU/network/cache databases are unnecessary for replay and omitted.
files += [p for p in (work / "profile").iterdir() if p.is_file()]
members = {}
with zipfile.ZipFile(out / "originals.zip", "x", zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(files):
        name = path.relative_to(work).as_posix()
        data = path.read_bytes()
        members[name] = pin(data)
        archive.writestr(name, data)
with zipfile.ZipFile(out / "originals.zip") as archive:
    assert archive.testzip() is None
    assert set(archive.namelist()) == set(members)
    assert all(pin(archive.read(name)) == expected for name, expected in members.items())
archive_record = {
    "source_work": str(work), "archive": pin((out / "originals.zip").read_bytes()),
    "member_count": len(members), "members": members,
    "all_members_read_back_verified": True,
    "scope": "already executed self-authored renderer capability probe; no product validation",
}
write(out / "archive.json", archive_record)
observations = [
    ("single_ascii_space_preserved", "Internal single ASCII space retained its codepoint and one visible gap."),
    ("leading_spaces_removed", "Both leading spaces disappeared before DOM text readback."),
    ("trailing_spaces_removed", "Both trailing spaces disappeared before DOM text readback."),
    ("double_ascii_space_collapsed", "DOM retained two spaces; innerText had one and the second Range had width zero."),
    ("unicode_line_separator_not_line_break", "U+2028 survived text and innerText but both Chinese glyphs stayed on the same baseline; separator geometry matched an ASCII space."),
    ("unicode_paragraph_separator_not_paragraph_break", "U+2029 survived text and innerText but both Chinese glyphs stayed on the same baseline; separator geometry matched an ASCII space."),
    ("nbsp_single_gap_preserved_limited", "NBSP codepoint and single gap retained. The nowrap label does not establish independent nonbreaking behavior."),
]
cases = []
for note, (finding, description) in zip(result["notes"][4:], observations, strict=True):
    node = note["actual"]["nodes"][0]
    cases.append({
        "name": note["name"], "source": note["file"],
        "expected_text": note["expected"]["nodes"][0],
        "dom_text": node["text"], "inner_text": node["visibleText"],
        "css_white_space": node["whiteSpace"], "glyph_ranges_css_pixels": node["glyphRanges"],
        "node_rect_css_pixels": node["rect"], "finding": finding,
        "observation": description, "screenshot_archive_member": note["screenshot"],
        "dom_equals_expected": node["text"] == note["expected"]["nodes"][0],
        "inner_text_equals_expected": node["visibleText"] == note["expected"]["nodes"][0],
    })
screenshots = {
    "leading-spaces.png": "note-5.png", "double-spaces.png": "note-7.png",
    "unicode-line-separator.png": "note-8.png", "unicode-paragraph-separator.png": "note-9.png",
    "nbsp.png": "note-10.png",
}
for target, source in screenshots.items():
    assert not (out / target).exists()
    shutil.copyfile(work / source, out / target)
summary = {
    "status": "completed_observations", "scope": result["scope"],
    "product_feature_verified": False, "scientific_quality": "not_run",
    "finished_utc": result["finishedUtc"], "identity": result["identity"],
    "mermaid_version": "10.6.1", "mermaid_version_evidence": "../bundled-mermaid.json",
    "flowchart_config": result["mermaidRuntime"]["config"]["flowchart"],
    "security_level": result["mermaidRuntime"]["config"]["securityLevel"],
    "notes_captured": len(result["notes"]), "whitespace_cases": cases,
    "baseline_controls": [{"name": n["name"], "checks": n["checks"]} for n in result["notes"][:4]],
    "actual_link_navigation": result["navigation"], "protection_cleanup": protection,
    "recommended_finite_policy": {
        "allow_whitespace": "Only isolated interior ASCII U+0020 space in graph labels.",
        "fallback_whitespace": "Leading/trailing ASCII spaces, consecutive ASCII spaces, and any non-ASCII whitespace use source-preserving text fallback.",
        "basis": "Conservative product boundary, distinct from observed unsupported cases; the NBSP single-gap result was not a demonstrated loss.",
        "prior_lf_tab_evidence": "../summary.json and its immutable originals.zip",
    },
    "readback_caveat": "The original normalized nodesMatch and visualWhitespacePreserved flags do not by themselves establish visual fidelity. Unicode line/paragraph separators require Range geometry plus screenshot interpretation.",
    "parent_evidence_before": parent_pins,
    "archive": {"path": "originals.zip", **archive_record["archive"], "member_count": len(members)},
    "screenshots": {name: {"archive_member": source, **pin((out / name).read_bytes())} for name, source in screenshots.items()},
    "limitations": ["Only one installed Obsidian/Mermaid pair and default clean profile were observed.", "No product CLI, source parsing, flow planning, science or full Unicode compatibility was tested by these authored notes.", "Geometry is in CSS pixels at this recorded viewport; it is not a cross-platform layout guarantee."],
}
assert parent_pins == {name: pin((out.parent / name).read_bytes()) for name in parent_pins}
summary["parent_evidence_unchanged"] = True
write(out / "summary.json", summary)
print(json.dumps({"status": "archived", "members": len(members), **archive_record["archive"]}))
