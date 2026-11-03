"""Preserve observed root gates and original06; no private credential/capture bytes."""
from __future__ import annotations
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import zipfile

SDK = Path(__file__).resolve().parent
REPO = SDK.parents[2]
ASSETS = REPO / "doc/validation/assets"
ORIGINAL = REPO / "target/compiler-temp/B04-T03-root-original06-01"
ARCHIVE = ASSETS / "B04-T03-root-evidence-01.zip"
MANIFEST = ASSETS / "B04-T03-root-evidence-01-manifest.json"


def fp(data: bytes) -> dict:
    return {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}


def load(p: Path):
    return json.loads(p.read_bytes())


def main():
    assert not ARCHIVE.exists() and not MANIFEST.exists()
    audit = load(ORIGINAL / "audit.json")
    assert audit["error"] is None
    assert audit["scenario_count"] == len(audit["rows"]) == 53
    assert len(audit["calls"]) == 61
    assert all(r["engineering_passed"] for r in audit["rows"])
    row = audit["rows"][-1]
    assert row["name"] == "original-sixth-saved-live-input"
    assert row["verification"]["independently_passed"]
    verification = load(ORIGINAL / "out-original-sixth/verification.json")
    assert verification["totals"] == {"expected_atoms": 1, "passed": 1, "failed": 0, "needs_review": 0, "unprocessed": 0, "candidate_atoms": 1, "unexpected_atoms": 0, "duplicate_atoms": 0, "required_concept_groups": 1, "additional_source_concepts": 2}
    assert verification["knowledge_status"] == "pending" and verification["published"] is False
    assert verification["scientific_quality"] == "not_run" and verification["research_status"] == "not_requested"
    assert audit["network_calls"] == audit["paid_calls"] == 0
    before, after = load(SDK / "root-inputs-before.json"), load(SDK / "root-inputs-after.json")
    assert before["facts"] == after["facts"]
    assert before["facts"]["budget"]["accounted_tokens"] == 30097
    labels = ["desktop-build", "desktop-install", "full-rust-fmt", "full-rust-check", "full-rust-tests", "full-python-tests-external-temp", "original06"]
    for label in labels:
        report = load(SDK / f"root-{label}-command.json")
        assert report["passed"] and report["exit_code"] == 0
        assert report["head_unchanged"] and report["code_unchanged"]
    rust = (SDK / "root-full-rust-tests-stdout.log").read_text(encoding="utf-8")
    rust_totals = [tuple(map(int, m)) for m in re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", rust)]
    assert sum(t[0] for t in rust_totals) == 532
    assert all(t[1] == t[2] == 0 for t in rust_totals)
    python_log = (SDK / "root-full-python-tests-external-temp-stderr.log").read_text(encoding="utf-8")
    assert re.search(r"Ran 104 tests in .*\n\nOK\s*$", python_log)
    failed_python = load(SDK / "root-full-python-tests-command.json")
    assert failed_python["exit_code"] == 1 and not failed_python["passed"]
    caption = load(SDK / "task-3-native-title-band-after.json")
    assert caption["passed"] and caption["windows"][0]["native_title_band_height"] == 0
    native = load(SDK / "task-3-native-actions-01.json")
    assert sum(c["passed"] for c in native["checks"]) == 11
    assert not native["passed"]
    entries: dict[str, bytes] = {}
    def add(name: str, p: Path):
        assert name not in entries and p.is_file() and not p.is_symlink()
        entries[name] = p.read_bytes()
    for p in sorted(ORIGINAL.rglob("*")):
        if p.is_file():
            add("original06/" + p.relative_to(ORIGINAL).as_posix(), p)
    names = [
        "capture-inputs.py", "root-inputs-before.json", "root-inputs-after.json",
        "run-root-validation.py", "run-root-validation-v1.py",
        "task-2-review.md", "task-2-review-evidence.json", "task-2-review-audit.py", "task-2-review-probe.py", "task-2-review-finalize.py",
        "task-2-rereview-1.md", "task-2-rereview-1-evidence.json", "task-2-rereview-1-audit.py",
        "task-3-review.md", "task-3-review-evidence.json", "task-3-review-evidence.ps1",
        "task-3-install-receipt.json", "task-3-install-update.ps1", "task-3-prior-install-receipt.json", "task-3-update-old-processes.json",
        "task-3-launch-native.py", "task-3-native-debug-01-launch.json", "task-3-native-debug-01-stdout.log", "task-3-native-debug-01-stderr.log",
        "task-3-native-window-probe.py", "task-3-native-before.json", "task-3-native-after-install.json", "task-3-native-title-band-after.json",
        "task-3-capture-native-frame.py", "task-3-native-full-capture.json", "task-3-public-native-crop.py", "task-3-native-public-crop.json",
        "task-3-native-actions.mjs", "task-3-native-actions-01.json", "task-3-native-actions-01-help.txt", "task-3-native-actions-01-version.txt",
        "task-3-native-manual-prep.mjs", "task-3-native-manual-prep.json", "task-3-native-manual-observation-01.json",
    ]
    for label in labels + ["full-python-tests"]:
        names.extend(f"root-{label}-{suffix}" for suffix in ["command.json", "stdout.log", "stderr.log"])
    names.extend(p.name for p in sorted(SDK.glob("task-3-native-actions-01-*.json")))
    for name in names:
        add("root/" + name, SDK / name)
    for directory in ["task-2-review-probe-diagnostic-identity", "task-2-review-probe-strict-boolean"]:
        for p in sorted((SDK / directory).rglob("*")):
            if p.is_file():
                add("root/" + p.relative_to(SDK).as_posix(), p)
    copies: dict[str, str] = {
        "B04-T03-root-original06-verification.json": "original06/out-original-sixth/verification.json",
        "B04-T03-root-original06-verification.md": "original06/out-original-sixth/verification.md",
        "B04-T03-root-original06-knowledge-ir.json": "original06/out-original-sixth/knowledge-ir.json",
        "B04-T03-root-original06-source-snapshot.json": "original06/out-original-sixth/source-snapshot.json",
        "B04-T03-root-original06-gold.json": "original06/out-original-sixth/gold-original.json",
        "B04-T03-root-original06-candidates.json": "original06/out-original-sixth/candidates-original.json",
        "B04-T03-root-original06-audit.json": "original06/audit.json",
        "B04-T03-root-task-2-review.md": "root/task-2-review.md",
        "B04-T03-root-task-2-rereview-1.md": "root/task-2-rereview-1.md",
        "B04-T03-root-task-3-review.md": "root/task-3-review.md",
        "B04-T03-root-inputs-before.json": "root/root-inputs-before.json",
        "B04-T03-root-inputs-after.json": "root/root-inputs-after.json",
        "B04-T03-ui-native-crop.json": "root/task-3-native-public-crop.json",
        "B04-T03-ui-native-title-band.json": "root/task-3-native-title-band-after.json",
        "B04-T03-ui-native-actions-01.json": "root/task-3-native-actions-01.json",
    }
    for label in labels + ["full-python-tests"]:
        for suffix in ["command.json", "stdout.log", "stderr.log"]:
            copies[f"B04-T03-root-{label}-{suffix}"] = f"root/root-{label}-{suffix}"
    user_before = Path("C:/Users/Lyinn/AppData/Local/Temp/codex-clipboard-1e9efd09-db30-42b8-a201-edf776017d71.png")
    add("native/user-provided-before.png", user_before)
    copies["B04-T03-ui-native-before-user.png"] = "native/user-provided-before.png"
    for name in ["B04-T03-ui-native-full-window.png", "B04-T03-ui-native-header.png"]:
        add("native/" + name, ASSETS / name)
    with zipfile.ZipFile(ARCHIVE, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in entries.items():
            archive.writestr(name, data)
    checks = 0
    with zipfile.ZipFile(ARCHIVE) as archive:
        assert set(archive.namelist()) == set(entries); checks += 1
        for name, data in entries.items():
            assert archive.read(name) == data; checks += 1
    public_copies = {}
    for name, member in copies.items():
        destination = ASSETS / name
        assert not destination.exists()
        with destination.open("xb") as stream:
            stream.write(entries[member])
        assert destination.read_bytes() == entries[member]; checks += 1
        public_copies[name] = {"archive_member": member, **fp(destination.read_bytes())}
    status_counts = Counter(r["verification"]["status"] for r in audit["rows"] if r.get("verification"))
    result = {
        "schema_version": 1, "captured_at": datetime.now(timezone.utc).isoformat(),
        "archive": {"path": ARCHIVE.relative_to(REPO).as_posix(), **fp(ARCHIVE.read_bytes())},
        "entries": {name: fp(data) for name, data in entries.items()}, "readable_copies": public_copies,
        "actual_preservation_assertions": checks, "source_helper_sha256": fp(Path(__file__).read_bytes())["sha256"],
        "observed_gates": {"rust_tests_passed": 532, "python_tests_passed": 104, "offline_scenarios": 53, "ordinary_cli_calls": 61, "dual_reports": audit["report_count"], "report_status_counts": dict(status_counts), "passed_bounded_knowledge_samples": audit["passed_knowledge_samples"], "original06_passed_expected_atoms": 1, "new_model_calls": 0, "native_actions_passed_so_far": 11, "native_complete": False},
        "retained_failures": ["Python first run: project-local TEMP triggered intentional path-in-repository credential protection; all104 passed with explicit external temporary parent and no product change", "native WS_CAPTION-bit oracle: Tao keeps style bits but WM_NCCALCSIZE removes visual title band; actual full visible-frame screenshot and band0 pass", "hidden-window maximize restore probe: existing maximize API does not show a tray-hidden window; physical tray Show/drag check requested, no product workaround"],
        "pending": ["physical native tray restore and drag user validation", "direct-exit preference and final normal launch", "final whole-branch review, completion and push"],
        "excluded_private_bytes": ["saved encrypted DPAPI credential and service files: fingerprints only", "raw screen/PrintWindow captures: local only; public exact DWM-visible-frame crop preserves top edge"],
    }
    with MANIFEST.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2); stream.write("\n")
    print(json.dumps({"archive": result["archive"], "entries": len(entries), "readable_copies": len(copies), "actual_preservation_assertions": checks, "observed_gates": result["observed_gates"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
