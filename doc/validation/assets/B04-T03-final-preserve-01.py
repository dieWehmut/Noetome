"""Preserve the final review/fix/re-review chain and actual full Python receipt."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import zipfile

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
HEAD = "02e8ab24d29814d427a45ac12aeb1fbffd001dd4"
ARCHIVE = ASSETS / "B04-T03-final-evidence-01.zip"
MANIFEST = ASSETS / "B04-T03-final-evidence-01-manifest.json"


def fp(data):
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def load(path):
    return json.loads(path.read_bytes())


assert not ARCHIVE.exists() and not MANIFEST.exists()
assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO).decode().strip() == HEAD
whole = load(SDK / "final-review-evidence.json")
assert whole["assertions"] == {"executed": 8452, "passed": 8452, "failed": 0}
assert whole["verdict"]["important"] == 1 and whole["verdict"]["minor"] == 1
assert whole["head_before"] == whole["head_after"] == "cccd784f607d506d0b6c8b0a12aea7973fadeb57"
review = (SDK / "final-fix-1-rereview.md").read_text(encoding="utf-8")
assert "ADDRESSED" in review and "NOT ADDRESSED" not in review
rereview = load(SDK / "final-fix-1-rereview-evidence.json")
assert len(rereview["findings"]) == 1
assert rereview["findings"][0]["id"] == "R1" and rereview["findings"][0]["verdict"] == "ADDRESSED"
assert rereview["new_breakage"] in ([], "None", None)
assert rereview["base"] == whole["head_after"] and rereview["head"] == HEAD
python = load(SDK / "root-final-fix-python-tests-command.json")
assert python["passed"] and python["exit_code"] == 0 and not python["timed_out"]
assert python["head_before"] == python["head_after"] == HEAD
assert python["code_before"] == python["code_after"] and len(python["code_before"]) == 81
assert python["binary_before"] == python["binary_after"]
for path, receipt in python["code_after"].items():
    current = fp((REPO / path).read_bytes())
    assert all(current[key] == receipt[key] for key in current), path
for name, receipt in python["log_fingerprints"].items():
    current = fp((SDK / name).read_bytes())
    assert all(current[key] == receipt[key] for key in current), name
stderr = (SDK / python["stderr"]).read_text(encoding="utf-8")
assert re.search(r"Ran 108 tests in [0-9.]+s", stderr) and stderr.rstrip().endswith("OK")
fix = load(ASSETS / "B04-T03-final-fix-1-manifest.json")
assert fp((ASSETS / fix["archive"]).read_bytes()) == {"sha256": fix["archive_sha256"], "byte_length": fix["archive_byte_length"]}
assert fix["entry_count"] == 884 and fix["green_module"] == {"tests": 32, "failures": 0}

entries = {}


def add(name, path):
    assert name not in entries and path.is_file() and not path.is_symlink(), str(path)
    entries[name] = path.read_bytes()


names = [
    "final-review.md", "final-review-evidence.json", "final-review-audit.py", "final-review-package.json",
    "review-bdae557..cccd784-source-doc.diff", "final-fix-1-brief.md", "final-fix-1-report.md",
    "final-fix-1-rereview.md", "final-fix-1-rereview-evidence.json", "final-fix-1-rereview-package.json",
    "final-fix-1-rereview-source.diff", "final-fix-1-rereview-full.diff", "final-fix-1-rereview-brief.md",
    "root-final-fix-python-tests-command.json", "root-final-fix-python-tests-stdout.log", "root-final-fix-python-tests-stderr.log",
    "run-root-validation.py", "prepare-final-fix-1-rereview.py", "promote-final-evidence-01.py", "progress.md",
]
for name in names:
    add("controller/" + name, SDK / name)
for path in sorted(SDK.glob("final-fix-1-rereview*.py")):
    add("rereview-helper/" + path.name, path)
for name in ("scripts/verify_extraction_knowledge.py", "tests/test_extraction_knowledge_verifier.py"):
    add("tested-code/" + name, REPO / name)
add("prior-evidence/B04-T03-final-fix-1-manifest.json", ASSETS / "B04-T03-final-fix-1-manifest.json")
add("controller/B04-T03-root-rulings.md", ASSETS / "B04-T03-root-rulings.md")
copies = {
    "B04-T03-final-review.md": "controller/final-review.md",
    "B04-T03-final-review-evidence.json": "controller/final-review-evidence.json",
    "B04-T03-final-review-audit.py": "controller/final-review-audit.py",
    "B04-T03-final-rereview-1.md": "controller/final-fix-1-rereview.md",
    "B04-T03-final-rereview-1-evidence.json": "controller/final-fix-1-rereview-evidence.json",
    "B04-T03-final-rereview-1-audit.py": "rereview-helper/final-fix-1-rereview-audit.py",
    "B04-T03-final-fix-1-report.md": "controller/final-fix-1-report.md",
    "B04-T03-final-python-command.json": "controller/root-final-fix-python-tests-command.json",
    "B04-T03-final-python-stdout.log": "controller/root-final-fix-python-tests-stdout.log",
    "B04-T03-final-python-stderr.log": "controller/root-final-fix-python-tests-stderr.log",
    "B04-T03-final-preserve-01.py": "controller/promote-final-evidence-01.py",
}
for name in copies:
    assert not (ASSETS / name).exists(), name
with zipfile.ZipFile(ARCHIVE, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for name, data in entries.items():
        archive.writestr(name, data)
assertions = 0
with zipfile.ZipFile(ARCHIVE) as archive:
    assert set(archive.namelist()) == set(entries)
    assertions += 1
    for name, data in entries.items():
        assert archive.read(name) == data
        assertions += 1
readable = {}
for name, member in copies.items():
    with (ASSETS / name).open("xb") as stream:
        stream.write(entries[member])
    assert (ASSETS / name).read_bytes() == entries[member]
    assertions += 1
    readable[name] = {"archive_member": member, **fp(entries[member])}
prior_archives = {}
for name in ("B04-T03-core-evidence.zip", "B04-T03-task-2-evidence.zip", "B04-T03-task-2-fix-1-evidence.zip", "B04-T03-root-evidence-01.zip", "B04-T03-root-evidence-02.zip", "B04-T03-final-fix-1-evidence.zip"):
    prior_archives[name] = fp((ASSETS / name).read_bytes())
result = {
    "schema_version": 1,
    "captured_at": datetime.now(timezone.utc).isoformat(),
    "archive": {"path": ARCHIVE.relative_to(REPO).as_posix(), **fp(ARCHIVE.read_bytes())},
    "entries": {name: fp(data) for name, data in entries.items()},
    "readable_copies": readable,
    "actual_preservation_assertions": assertions,
    "whole_branch_review": {"base": "bdae557a884bd784a7ba52252bb5828e6ffd5c17", "head": whole["head_after"], "verdict": "WithFixes: sole Important R1, Minor R2 deferred", "actual_audit_assertions": whole["assertions"], "archive_members": 7562, "readable_copies": 63},
    "fixed_head": HEAD,
    "fix1": {"raw_red_tests": 4, "raw_red_behavior_failures": 7, "focused_green": 4, "module_green": 32, "actual_cli_calls": 43, "archive_members": 884, "new_model_calls": 0},
    "scoped_rereview": {"report": "B04-T03-final-rereview-1.md", "evidence": "B04-T03-final-rereview-1-evidence.json", "actual_record": rereview},
    "full_python": {"tests": 108, "failures": 0, "exit_code": 0, "elapsed_seconds": python["elapsed_seconds"], "source_files_unchanged": 81, "head": HEAD, "binary_sha256": python["binary_after"]["sha256"]},
    "prior_archives": prior_archives,
    "retained_minor": "R2 state-read recovery alert; explicitly nonblocking and deferred, no UI change in this final fix",
    "limitations": "Archive assertions prove retained bytes. Full branch review first found R1; its historical WithFixes result is not rewritten. The new scoped review covers R1/fix-introduced breakage only. Rust532/browser10/native16 plus two user-confirmed checks and original06 remain prior actual evidence bound to unchanged source/binary. No new network/model call, credential decryption or native action by this preservation step.",
    "private_byte_policy": "Explicit same-plan SDK review/test files and public source only. No raw desktop screen, encrypted credential bytes, service secret values or private-folder scan. Prior seven-MB full diff remains local; its exact fingerprint and committed range are retained with the source/doc package.",
    "pending": ["ledger completion, ordinary commit/push and next-batch planning"],
}
with MANIFEST.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(result, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
print(json.dumps({"archive": result["archive"], "entries": len(entries), "readable_copies": len(readable), "actual_preservation_assertions": assertions, "full_python": result["full_python"]}, ensure_ascii=False))
