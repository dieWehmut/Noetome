import argparse
import hashlib
import json
from pathlib import Path

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
parser = argparse.ArgumentParser()
parser.add_argument("--phase", choices=("before-complete", "after-complete"), required=True)
args = parser.parse_args()
pairs = (
    [("final-evidence-preservation", "final-preservation"), ("final-docs-check", "final-docs"),
     ("final-pre-complete-devlog-check", "final-pre-complete-devlog"), ("final-closure-diff-check", "final-diff")]
    if args.phase == "before-complete" else [("final-post-complete-devlog-check", "final-post-complete-devlog")]
)
copies = []
for label, public in pairs:
    command = json.loads((SDK / f"root-{label}-command.json").read_bytes())
    assert command["passed"] and command["exit_code"] == 0
    assert command["code_unchanged"] and command["head_unchanged"]
    for suffix in ("command.json", "stdout.log", "stderr.log"):
        source = SDK / f"root-{label}-{suffix}"
        target = ASSETS / f"B04-T03-{public}-{suffix}"
        data = source.read_bytes()
        with target.open("xb") as stream:
            stream.write(data)
        assert target.read_bytes() == data
        copies.append({"source": source.name, "target": target.relative_to(REPO).as_posix(), "sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)})
if args.phase == "before-complete":
    for source, public in ((SDK / "closure-docs-check.json", "B04-T03-final-docs-check.json"), (Path(__file__), "B04-T03-final-copy-check-receipts.py")):
        data = source.read_bytes()
        with (ASSETS / public).open("xb") as stream:
            stream.write(data)
        assert (ASSETS / public).read_bytes() == data
        copies.append({"source": source.name, "target": "doc/validation/assets/" + public, "sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)})
with (SDK / f"final-{args.phase}-receipt-copies.json").open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(copies, stream, indent=2)
    stream.write("\n")
print(json.dumps({"phase": args.phase, "copies_byte_verified": len(copies)}))
