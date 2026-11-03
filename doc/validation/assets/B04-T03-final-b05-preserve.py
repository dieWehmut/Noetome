from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import zipfile

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
ARCHIVE = ASSETS / "B04-T03-final-b05-evidence.zip"
MANIFEST = ASSETS / "B04-T03-final-b05-manifest.json"
assert not ARCHIVE.exists() and not MANIFEST.exists()


def fp(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


entries = {}
for label in ("final-b05-render", "final-b05-batch", "final-b05-planning-check", "final-b05-docs-check", "final-b05-devlog-check", "final-b05-diff-check"):
    result = json.loads((SDK / f"root-{label}-command.json").read_bytes())
    assert result["passed"] and result["exit_code"] == 0 and result["code_unchanged"] and result["head_unchanged"]
    for suffix in ("command.json", "stdout.log", "stderr.log"):
        name = f"root-{label}-{suffix}"
        entries["commands/" + name] = (SDK / name).read_bytes()
for name in ("roll-b05-planning.py", "check-b05-planning.py", "check-final-docs.py", "promote-b05-planning-evidence.py"):
    entries["helpers/" + name] = (SDK / name).read_bytes()
for name in ("NEXT_STEPS.md", "ROADMAP.md", "B04R_STEPS.md"):
    entries["planned-documents/" + name] = (REPO / "doc/execution" / name).read_bytes()
entries["results/b05-final-docs-check.json"] = (SDK / "b05-final-docs-check.json").read_bytes()
entries["results/B04-T03-final-b05-planning-check.json"] = (ASSETS / "B04-T03-final-b05-planning-check.json").read_bytes()
copies = {
    "B04-T03-final-b05-docs-check.json": "results/b05-final-docs-check.json",
    "B04-T03-final-b05-planning-check.json": "results/B04-T03-final-b05-planning-check.json",
    "B04-T03-final-b05-preserve.py": "helpers/promote-b05-planning-evidence.py",
}
with zipfile.ZipFile(ARCHIVE, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for name, data in entries.items():
        archive.writestr(name, data)
checks = 0
with zipfile.ZipFile(ARCHIVE) as archive:
    assert set(archive.namelist()) == set(entries)
    checks += 1
    for name, data in entries.items():
        assert archive.read(name) == data
        checks += 1
readable = {}
for name, member in copies.items():
    target = ASSETS / name
    if name == "B04-T03-final-b05-planning-check.json":
        assert target.read_bytes() == entries[member]
    else:
        with target.open("xb") as stream:
            stream.write(entries[member])
    assert target.read_bytes() == entries[member]
    checks += 1
    readable[name] = {"archive_member": member, **fp(entries[member])}
docs = json.loads(entries["results/b05-final-docs-check.json"])
planning = json.loads(entries["results/B04-T03-final-b05-planning-check.json"])
assert docs["passed"] and planning["passed"] and planning["in_progress"] == 0
result = {
    "schema_version": 1, "captured_at": datetime.now(timezone.utc).isoformat(),
    "scope": "B04 closeout next-batch plan and actual devlog/data/document checks only",
    "archive": {"path": ARCHIVE.relative_to(REPO).as_posix(), **fp(ARCHIVE.read_bytes())},
    "entries": {name: fp(data) for name, data in entries.items()}, "readable_copies": readable,
    "actual_preservation_assertions": checks,
    "actual_planning_result": {"batch": "B05", "new_todo_tasks": 3, "old_done_tasks": 15, "in_progress": 0, "old_history_unchanged": True, "documents_checked": len(docs["documents"]), "local_links_passed": docs["local_links_passed"], "product_source_files_unchanged": docs["tested_product_source_files_unchanged"]},
    "limitations": "No B05 product implementation/tests/CLI/model calls are claimed. The archive retains actual render/batch/check command receipts and planned documentation. Historical B04 technical/native/review evidence remains separate, no private credentials or desktop background bytes are copied.",
}
with MANIFEST.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(json.dumps({"archive": result["archive"], "entries": len(entries), "readable_copies": len(readable), "actual_preservation_assertions": checks, "actual_planning_result": result["actual_planning_result"]}))
