"""Check final local document paths and Git raw-byte attributes without index writes."""
import hashlib
import json
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[5]
BASE = ROOT / "doc/validation/assets/B06V-T03/review"
names = list(json.loads((BASE / "final-doc-link-attributes-audit.json").read_bytes())["documents_before"])
names += ["doc/validation/assets/B06V-T03/review/current-final-pe/README.md", "doc/validation/assets/B06V-T03/linux-final-after-portable-test/README.md", "doc/validation/assets/B06V-T03/review/flow-target-test-portable/README.md"]


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


before = {name: pin((ROOT / name).read_bytes()) for name in names}
links, external = [], []
for name in names:
    text = (ROOT / name).read_text()
    for match in re.finditer(r"\[[^\]]*\]\(([^\n)]*)\)", text):
        target = match.group(1).strip().strip("<>")
        if target.startswith(("https://", "http://", "mailto:")):
            external.append({"doc": name, "target": target})
            continue
        if target.startswith("#"):
            continue
        target = unquote(target.split("#", 1)[0])
        resolved = (ROOT / name).parent / target
        links.append({"doc": name, "target": target, "exists": resolved.exists()})
assert all(link["exists"] for link in links), [link for link in links if not link["exists"]]
bases = ["doc/validation/assets/B06V-T03", "fixtures/visual-compatibility", "fixtures/visual-compatibility-flow-extensions", "fixtures/visual-compatibility-corrected-v2"]
paths = subprocess.run(["rg", "--files", "-uuu", "-0", *bases], cwd=ROOT, check=True, capture_output=True).stdout.split(b"\0")
paths = sorted(set(path for path in paths if path))
raw = subprocess.run(["git", "check-attr", "-z", "--stdin", "text", "filter", "working-tree-encoding"], cwd=ROOT, input=b"\0".join(paths)+b"\0", check=True, capture_output=True).stdout
fields = raw.split(b"\0")[:-1]
assert len(fields) == len(paths) * 9
attributes = {}
for index in range(0, len(fields), 3):
    path, attribute, value = [entry.decode("utf-8") for entry in fields[index:index+3]]
    attributes.setdefault(path, {})[attribute] = value
assert len(attributes) == len(paths)
assert all(row["text"] == "unset" and row["filter"] in ("unset", "unspecified") and row["working-tree-encoding"] in ("unset", "unspecified") for row in attributes.values())
previous = json.loads((BASE / "final-doc-link-attributes-audit.json").read_bytes())
samples = []
for row in previous["actual_CRLF_hash_samples"]:
    path = row["path"]
    data = (ROOT / path).read_bytes()
    assert b"\r\n" in data
    filtered = subprocess.run(["git", "hash-object", "--stdin", "--path="+path], cwd=ROOT, input=data, check=True, capture_output=True).stdout.strip().decode()
    plain = subprocess.run(["git", "hash-object", "--stdin", "--no-filters"], cwd=ROOT, input=data, check=True, capture_output=True).stdout.strip().decode()
    assert filtered == plain
    samples.append({"path": path, "raw_pin": pin(data), "blob_sha1": plain, "filtered_equals_unfiltered": True})
after = {name: pin((ROOT / name).read_bytes()) for name in names}
assert before == after
result = {"schema_version": 1, "status": "passed", "scope": "Final eleven documents: local path existence, final count/source/unknown-version boundaries manually reviewed, and actual Git raw-byte normalization readback; external content not fetched", "documents_before": before, "documents_after": after, "local_links": links, "local_links_checked": len(links), "missing_local_links": 0, "external_links_not_fetched": external, "asset_paths_checked": len(paths), "all_asset_text_unset_no_filter_or_encoding": True, "git_attribute_stdout_pin": pin(raw), "actual_CRLF_hash_samples": samples, "review_findings": [], "resolved_previous_findings": "Final WindowsC30+15setup replaces source-unanchored old ELF specialty; final Linux04/Windows03 bind final test42b; failure causes separated; source checks precede target selection; final code audit link corrected", "final_raw_source_and_capability_audit": "final-capability-regression-readback.json", "full_linux_rust": "not_run", "git_index_or_object_writes": False}
output = BASE / "final-document-closeout-audit.json"
output.write_text(json.dumps(result, ensure_ascii=False, indent=2)+"\n")
print(json.dumps({"status": "passed", "documents": len(names), "links": len(links), "asset_paths": len(paths), "report_pin": pin(output.read_bytes())}))
