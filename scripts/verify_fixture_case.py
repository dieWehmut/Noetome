"""Reproduce read-only fixture checks in a new test directory without using a Vault."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def sha(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(root):
    entries = {}
    for path in sorted(root.rglob("*")):
        metadata = path.stat()
        entries[path.relative_to(root).as_posix()] = {
            "directory": path.is_dir(),
            "mtime_ns": metadata.st_mtime_ns,
            "attributes": getattr(metadata, "st_file_attributes", None),
            "sha256": sha(path.read_bytes()) if path.is_file() else None,
        }
    return entries


def edit(root, name, change):
    path = root / name
    value = json.loads(path.read_bytes())
    change(value)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


def edit_ir(root, change):
    edit(root, "ir.json", change)
    digest = sha((root / "ir.json").read_bytes())
    edit(root, "note-plan.json", lambda plan: plan.update(knowledge_sha256=digest))


def optional_atom(ir):
    atom = copy.deepcopy(ir["atoms"][0])
    atom.update(atom_id="Optional1", required=False, verification="unverified", value_status="unknown", content="<script>alert(1)</script>")
    ir["atoms"].append(atom)


def shortened_evidence(ir):
    entry = next(item for item in ir["evidence"] if item["evidence_id"] == "E5a")
    entry.update(excerpt_end_byte=3, excerpt_sha256=sha("仅".encode()))


def moved_definition(plan):
    definition = plan["notes"][1]["sections"][0]["ordered_nodes"].pop(0)
    assert definition["node_id"] == "Q6z"
    plan["notes"][0]["sections"][0]["ordered_nodes"].append(definition)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True, help="New test root whose parent already exists")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    fixture = repo / "fixtures/text-compare-v1"
    root = args.root.absolute()
    cli = args.cli.resolve(strict=True)
    if root.exists() or not root.parent.is_dir() or fixture.resolve().is_relative_to(root):
        raise ValueError("Choose a new, separate test root with an existing parent")
    root.mkdir()
    sentinel = root / "outside.md"
    sentinel.write_text("独立测试哨兵，验证器不得写入。\n", encoding="utf-8")
    sentinel_before = (sentinel.read_bytes(), sentinel.stat().st_mtime_ns)
    expected_bytes = (fixture / "expected.json").read_bytes()
    expected_sha = sha(expected_bytes)
    results = []
    good_stdout = None
    cases = [
        ("valid", None, 0, None, None),
        ("missing-required-atom", lambda p: edit_ir(p, lambda ir: ir["atoms"].pop(0)), 3, "expected", "EXPECTED_ATOM"),
        ("short-support", lambda p: edit_ir(p, shortened_evidence), 3, "expected", "EXPECTED_EVIDENCE"),
        ("hidden-text", lambda p: edit(p, "note-plan.json", lambda plan: plan["notes"][0]["sections"][0]["ordered_nodes"][0].update(text="<!--" + plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] + "-->")), 3, "note_plan", "PLAN_TEXT_SYNTAX"),
        ("raw-wiki", lambda p: edit(p, "note-plan.json", lambda plan: plan["notes"][0]["sections"][0]["ordered_nodes"][0].update(text=plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] + " [[不存在#幽灵]]")), 3, "note_plan", "PLAN_TEXT_SYNTAX"),
        ("moved-definition", lambda p: edit(p, "note-plan.json", moved_definition), 3, "note_plan", "PLAN_CANONICAL"),
        ("optional-html", lambda p: edit_ir(p, optional_atom), 3, "knowledge", "KNOWLEDGE_ATOM_CONTENT"),
        ("stale-ir-bytes", lambda p: (p / "ir.json").write_bytes((p / "ir.json").read_bytes() + b"\n"), 3, "note_plan", "PLAN_IR_BINDING"),
        ("source-mismatch", lambda p: (p / "sources/材料甲.md").write_bytes(b"x" + (p / "sources/材料甲.md").read_bytes()[1:]), 3, "sources", None),
        ("future-version", lambda p: edit(p, "case.json", lambda case: case.update(schema_version=2)), 2, None, None),
        ("unsafe-input", lambda p: edit(p, "case.json", lambda case: case.update(ir_path="../outside.md")), 2, None, None),
        ("unsafe-output", lambda p: edit(p, "note-plan.json", lambda plan: plan["notes"][0].update(output_path="笔记/甲#歧义.md")), 3, "note_plan", "PLAN_PATH"),
    ]
    for name, mutate, exit_code, check_name, rule in cases:
        case_root = root / name
        shutil.copytree(fixture, case_root)
        if mutate:
            mutate(case_root)
        assert (case_root / "expected.json").read_bytes() == expected_bytes
        before = snapshot(case_root)
        for repetition in range(2 if name == "valid" else 1):
            process = subprocess.run([str(cli), "case", "verify", str(case_root)], capture_output=True, check=False)
            stderr = process.stderr.decode("utf-8")
            assert process.returncode == exit_code, (name, process.returncode, stderr)
            report = json.loads(process.stdout) if process.stdout else None
            if check_name:
                assert report["passed"] is False and report["checks"][check_name]["status"] == "failed", name
                assert "CASE_VERIFICATION_FAILED" in stderr, name
                if rule:
                    assert any(issue["rule_id"] == rule for issue in report["issues"]), (name, rule, report["issues"])
            elif name == "valid":
                assert report["passed"] is True and not stderr
                assert report["counts"] == {"sources": 2, "spans": 12, "atoms": 4, "required_atoms": 4, "notes": 2, "nodes": 4}
                assert len(report["resolved_targets"]) == 2
                if good_stdout is None:
                    good_stdout = process.stdout
                else:
                    assert good_stdout == process.stdout
            else:
                assert not process.stdout and stderr
            if report:
                assert report["checks"]["rendering"]["status"] == "not_run"
                assert report["checks"]["scientific_quality"]["status"] == "not_run"
            after = snapshot(case_root)
            assert before == after, name
            assert (sentinel.read_bytes(), sentinel.stat().st_mtime_ns) == sentinel_before
            results.append({"case": name, "repetition": repetition, "exit_code": process.returncode, "stderr": stderr, "input_unchanged": True, "expected_sha256": expected_sha, "report": report})
    evidence = {
        "schema_version": 1,
        "scope": "Self-authored fixture, read-only contracts; no rendering, publication, models or scientific-quality assessment",
        "cli_path": str(cli), "cli_sha256": sha(cli.read_bytes()), "test_root": str(root),
        "expected_sha256": expected_sha, "calls": len(results),
        "input_bytes_mtime_attributes_and_directories_unchanged": True,
        "external_sentinel_unchanged": True, "results": results,
    }
    (root / "evidence.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"calls": len(results), "passed": True, "evidence": str(root / "evidence.json")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
