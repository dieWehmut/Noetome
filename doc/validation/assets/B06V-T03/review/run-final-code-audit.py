"""Read source, run selected verifier checks, and mutate disposable test copies.

This review helper never runs a model, app, CLI, or writes author fixtures.
"""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile

REPO = Path(__file__).resolve().parents[5]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(REPO / "scripts"))
sys.path.insert(0, str(REPO / "tests"))
import verify_formula as F
import verify_image as I
import verify_process_flow as P


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


paths = [
    "crates/noetome-cli/src/visual_capabilities.rs",
    "crates/noetome-cli/src/formula.rs",
    "crates/noetome-cli/src/scan_formula.rs",
    "crates/noetome-cli/src/image_fidelity.rs",
    "crates/noetome-cli/src/process_flow.rs",
    "crates/noetome-cli/src/process_flow_model.rs",
    "crates/noetome-cli/src/process_flow_plan.rs",
    "crates/noetome-cli/src/lib.rs",
    "scripts/visual_capability_registry.py",
    "scripts/verify_formula.py", "scripts/verify_image.py",
    "scripts/verify_process_flow.py", "scripts/verify_scan_formula.py",
    "scripts/prepare_visual_compatibility.py",
    "tests/test_visual_capability_registry.py",
    "tests/test_verify_formula.py", "tests/test_verify_image.py",
    "tests/test_verify_process_flow.py", "tests/test_prepare_visual_compatibility.py",
]
before_bytes = {name: (REPO / name).read_bytes() for name in paths}
before = {name: pin(raw) for name, raw in before_bytes.items()}
pack = REPO / "fixtures/visual-compatibility"
author_before = F.pins(F.inventory(pack))
checks = []
raw_outputs = {}
for name, argv in [
    ("registry-source-gates", [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_visual_capability_registry.py", "-v"]),
    ("flow-schema-two", [sys.executable, "-m", "unittest", "test_verify_process_flow.FlowRendererProfileTests", "-v"]),
]:
    env = dict(__import__("os").environ)
    env["PYTHONPATH"] = str(REPO / "tests")
    start = time.monotonic()
    run = subprocess.run(argv, cwd=REPO, env=env, capture_output=True, timeout=90)
    checks.append({"name": name, "argv": argv, "exit_code": run.returncode,
                   "elapsed_seconds": time.monotonic() - start,
                   "stdout": pin(run.stdout), "stderr": pin(run.stderr)})
    raw_outputs[name + ".stdout.bin"] = run.stdout
    raw_outputs[name + ".stderr.bin"] = run.stderr
    assert run.returncode == 0, run.stderr.decode(errors="replace")

mutations = []


def rejected(name, test, copies, expected_fragment=None):
    try:
        test()
    except AssertionError as error:
        message = str(error)
        assert expected_fragment is None or expected_fragment in message, message
        mutations.append({"name": name, "status": "rejected", "actual_error": message,
                          "mutant_files": {key: pin(path.read_bytes()) for key, path in copies.items()}})
        for key, path in copies.items():
            raw_outputs["mutation-" + name + "/" + key] = path.read_bytes()
    else:
        raise AssertionError("forged output unexpectedly accepted: " + name)


with tempfile.TemporaryDirectory(prefix="noetome-final-readonly-review-") as tmp:
    work = Path(tmp)
    for kind, checker, case_id, basename in [
        ("formula", F, "formula-native", "native"),
        ("image", I, "image-signed-arrows", "signed_arrows"),
    ]:
        source = pack / ("source-originals/fixtures/" + ("formula-fidelity" if kind == "formula" else "image-fidelity"))
        baseline = pack / "selected-output" / case_id
        expected = source / ("expected-" + basename + ".json")
        checked = checker.verify_output(source / basename, baseline / "profile.json", baseline, expected)
        assert checked["representation"] == ("native_math" if kind == "formula" else "svg_redraw")
        forged = work / (kind + "-unknown-version")
        shutil.copytree(baseline, forged)
        target = work / (kind + "-selected-unknown-target.json")
        profile = F.strict_json((baseline / "profile.json").read_bytes())
        profile["app_version"] = "99.0"
        F.save(target, profile)
        (forged / "profile.json").write_bytes(target.read_bytes())
        report = F.strict_json((forged / (kind + ".json")).read_bytes())
        report["profile"] = profile
        report["inputs"]["profile.json"] = F.fingerprint(target.read_bytes())
        F.save(forged / (kind + ".json"), report)
        checker.resynchronize_result(forged)
        rejected(kind + "-synchronized-unknown-native-claim",
                 lambda c=checker, s=source, b=baseline, v=forged, e=expected, t=target, n=basename:
                 c.verify_output(s / n, b / "profile.json", v, e, target_profile=t),
                 {"target-profile.json": target, kind + ".json": forged / (kind + ".json")},
                 "source-grounded " + ("expression" if kind == "formula" else "image") + " outcome")
        semantic = work / (kind + "-semantic-loss")
        shutil.copytree(baseline, semantic)
        if kind == "formula":
            note = semantic / "公式预览.md"
            raw = note.read_bytes(); changed = raw.replace(b"= -", b"= +", 1)
            assert raw != changed
            note.write_bytes(changed)
        else:
            checker.mutate_svg(semantic, "relation-gain-head", lambda raw: b"")
            note = semantic / "redrawn.svg"
        checker.resynchronize_result(semantic)
        rejected(kind + "-synchronized-semantic-loss",
                 lambda c=checker, s=source, b=baseline, v=semantic, e=expected, n=basename:
                 c.verify_output(s / n, b / "profile.json", v, e, target_profile=b / "profile.json"),
                 {note.name: note, kind + ".json": semantic / (kind + ".json")})
    inputs = pack / "verification-inputs/flow-branch"
    expected = pack / "source-originals/fixtures/process-flow-v1/expected-branch.json"
    P.verify_flow(inputs / "project", inputs / "case", inputs / "review", inputs / "profile.json", inputs / "Vault", expected)
    forged = work / "flow-unknown-engine"
    shutil.copytree(inputs / "Vault", forged)
    target = work / "flow-selected-unknown-engine.json"
    profile = {"schema_version": 2, "producer": "flow_render_profile",
               "profile_id": "obsidian_1_15_0_pele_flowchart", "app": "Obsidian", "app_version": "1.15.0",
               "mermaid_version": None, "flowchart_support": "supported", "html_labels": False,
               "label_encoding": "decimal_ascii_punctuation_v1",
               "renderer": {"name": "pele", "version": "99.0", "source_sha256": "9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0"}}
    P.profile_schema(profile)
    P.save(target, profile)
    (forged / ".profile.json").write_bytes(target.read_bytes())
    for name in ["plan.json", "flow.json"]:
        report = P.strict_json((forged / name).read_bytes()); report["profile"] = profile
        P.save(forged / name, report)
    P.refresh_flow(forged)
    rejected("flow-synchronized-unknown-engine-flowchart-claim",
             lambda: P.verify_flow(inputs / "project", inputs / "case", inputs / "review", inputs / "profile.json", forged, expected, target_profile=target),
             {"target-profile.json": target, "plan.json": forged / "plan.json", "flow.json": forged / "flow.json"},
             "flow issue_codes")
    semantic = work / "flow-lost-source-condition"
    shutil.copytree(inputs / "Vault", semantic)
    note = semantic / "过程明细.md"
    raw = note.read_bytes(); original = "签名区域可读".encode()
    assert original in raw
    note.write_bytes(raw.replace(original, "签名区域不可读".encode(), 1))
    P.refresh_flow(semantic)
    rejected("flow-synchronized-visible-source-loss",
             lambda: P.verify_flow(inputs / "project", inputs / "case", inputs / "review", inputs / "profile.json", semantic, expected, target_profile=inputs / "profile.json"),
             {note.name: note, "flow.json": semantic / "flow.json"})

after = {name: pin((REPO / name).read_bytes()) for name in paths}
assert before == after, "reviewed code changed during actual focused checks"
assert author_before == F.pins(F.inventory(pack)), "frozen author fixtures changed during checks"
raw_outputs.update({"source/" + name: raw for name, raw in before_bytes.items()})
archive = HERE / "final-code-audit-originals.zip"
assert not archive.exists(), "retain prior review original archive"
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
    for name, raw in sorted(raw_outputs.items()):
        z.writestr(name, raw)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    assert all(z.read(name) == raw for name, raw in raw_outputs.items())
result = {"schema_version": 1, "status": "passed", "scope": "whole current visual capability/source/verifier delta read-only review plus independent disposable-output forgery checks",
          "checks": checks, "synchronized_mutations": mutations, "source_before": before,
          "source_after": after, "sources_unchanged": True, "author_files_unchanged": True,
          "app_launches": 0, "model_requests": 0, "ordinary_cli_runs": 0,
          "linux_full_rust": "not_run", "archive": str(archive.relative_to(REPO)), "archive_pin": pin(archive.read_bytes())}
(HERE / "final-code-capability-audit.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"status": result["status"], "checks": len(checks), "synchronized_mutations": len(mutations), "source_files": len(before), "archive_pin": result["archive_pin"]}))
