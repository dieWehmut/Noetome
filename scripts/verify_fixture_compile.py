"""Exercise the real fixture CLI in a new test root and reread its publication evidence.

This standard-library checker does not call the Rust renderer or rewrite expected.
The fixed note literals were selected in B02-T03 before running the implementation.
"""

import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import time
import uuid


ALPHA = (
    "# 甲容器\n\n## 定义与比较\n\n"
    "甲容器是虚构的蓝砂收纳盒。参见乙容器。 [[笔记/乙容器#定义与条件|乙容器]]\n\n"
    "| 比较属性 | 甲容器 | 乙容器 |\n| --- | --- | --- |\n"
    "| 虚构刻度 | 甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。 | "
    "乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。 |\n"
    "| 单价 | 未给出 | 未给出 |\n\n"
)
BETA = (
    "# 乙容器\n\n## 定义与条件\n\n"
    "乙容器是虚构的红砂收纳盒。参见甲容器。 [[笔记/甲容器#定义与比较|甲容器]]\n\n"
    "1. 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。\n\n"
)
ALIAS = "[[笔记/乙容器#定义与条件\\|乙容器入口]]"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_bytes())


def snapshot(root):
    return {str(p.relative_to(root)): (p.is_dir(), p.stat().st_mtime_ns,
            getattr(p.stat(), "st_file_attributes", None), sha(p.read_bytes()) if p.is_file() else None)
            for p in sorted(root.rglob("*"))}


def edit(root, name, change):
    path = root / name
    value = read_json(path)
    change(value)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


def decode_cell(value):
    # Decode each generated escape once, so escaped literal entities stay literal.
    return re.sub(r"\\([!\"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])|<br>|&(?:amp|lt|gt|#9|#13);",
                  lambda m: m[1] if m[1] is not None else ("\n" if m[0] == "<br>" else html.unescape(m[0])), value)


def tables(markdown):
    section = None
    result = {}
    for line in markdown.splitlines():
        if line.startswith("## "):
            section = line[3:]
            result[section] = []
        elif line.startswith("| "):
            cells, text, slashes = [], "", 0
            for character in line[1:-1]:
                if character == "|" and slashes % 2 == 0:
                    cells.append(decode_cell(text.strip()))
                    text = ""
                else:
                    text += character
                slashes = slashes + 1 if character == "\\" else 0
            cells.append(decode_cell(text.strip()))
            result[section].append(cells)
    return {key: rows[2:] for key, rows in result.items() if rows}


def verify(root, fixture, outcome, alias=False):
    assert outcome["release_status"] == "published" and outcome["output_created"] is True
    report_path = root / outcome["paths"]["json_report"]
    markdown_path = root / outcome["paths"]["markdown_report"]
    report = read_json(report_path)
    assert outcome["report"] == report
    assert report["result"] == "validated" and report["producer"] == "fixture"
    assert report["issues"] == []
    for name in ("source_coverage", "knowledge_compilation", "quality_validation"):
        assert report["dimensions"][name]["state"] == "complete"
    assert report["dimensions"]["optional_research"]["state"] == "not_applicable"
    assert report["checks"]["scientific_quality"] == {"status": "not_run", "issue_count": 0}
    assert all(c == {"status": "passed", "issue_count": 0} for n, c in report["checks"].items() if n != "scientific_quality")
    expected = read_json(fixture / "expected.json")
    atoms = {a["atom_id"]: a for a in expected["required_atoms"]}
    alpha = ALPHA.replace("破损时不适用。 |", f"破损时不适用。 {ALIAS} |") if alias else ALPHA
    literals = {"笔记/甲容器.md": alpha, "笔记/乙容器.md": BETA}
    assert len(report["files"]) == 2
    seen_atoms, mappings, links = set(), 0, 0
    for file in report["files"]:
        path = file["relative_path"]
        data = (root / "vault" / path).read_bytes()
        assert data == literals[path].encode("utf-8") == (root / "staging/vault" / path).read_bytes()
        assert len(data) == file["byte_length"] and sha(data) == file["content_sha256"]
        for mapping in file["content_mappings"]:
            body = decode_cell(data[mapping["start_byte"]:mapping["end_byte"]].decode("utf-8"))
            for atom_id in mapping["atom_ids"]:
                atom = atoms[atom_id]
                assert atom["content"] in body and all(q in body for q in atom["qualifiers"])
                seen_atoms.add(atom_id)
            mappings += 1
        actual_links = re.findall(r"\[\[.*?\]\]", data.decode("utf-8"))
        assert len(actual_links) == len(file["links"])
        for link in file["links"]:
            token = data[link["start_byte"]:link["end_byte"]].decode("utf-8")
            target = (root / "vault" / link["target_path"]).read_text(encoding="utf-8")
            assert target.splitlines().count("## " + link["target_heading"]) == 1
            separator = "\\|" if link["scope"]["kind"] == "table_cell" else "|"
            want = f'[[{link["target_path"][:-3]}#{link["target_heading"]}{separator}{link["display_text"]}]]'
            assert token == want and token in actual_links
            links += 1
    assert seen_atoms == set(atoms) and mappings == 9 and links == (3 if alias else 2)
    assert report["counts"]["rendered_files"] == 2 and report["counts"]["mappings"] == mappings and report["counts"]["links"] == links
    md = tables(markdown_path.read_text(encoding="utf-8"))
    assert md["四维状态"] == [[n, d["state"], d["reason"]] for n, d in report["dimensions"].items()]
    counts = [["overall", n, str(c)] for n, c in report["counts"].items()]
    counts += [[n, k, str(c)] for n, d in report["dimensions"].items() for k, c in d["counts"].items()]
    assert md["计数"] == counts
    assert md["检查"] == [[n, c["status"], str(c["issue_count"])] for n, c in report["checks"].items()]
    assert md["文件"] == [[f["relative_path"], f["note_id"], str(f["plan_revision"]), str(f["byte_length"]), f["content_sha256"]] for f in report["files"]]
    assert md["问题"] == []
    assert len(md["内容映射"]) == mappings and len(md["语义引用"]) == links
    link_rows = []
    for f in report["files"]:
        for link in f["links"]:
            s = link["scope"]
            locator = ":".join([s["kind"], s["item_id"] or "", s["row_id"] or "", s["column_id"] or ""])
            link_rows.append([f["relative_path"], s["section_id"], s["node_id"], locator, link["target_concept_id"], link["target_path"], link["target_heading"], link["display_text"], str(link["start_byte"]), str(link["end_byte"])])
    assert md["语义引用"] == link_rows
    for key, filename in [("case_sha256", "case.json"), ("ir_sha256", "ir.json"), ("plan_sha256", "note-plan.json"), ("expected_sha256", "expected.json")]:
        assert report["input_hashes"][key] == sha((fixture / filename).read_bytes())
    database_path = root / "state/compile.sqlite"
    assert all(not Path(str(database_path) + suffix).exists() for suffix in ("-journal", "-wal", "-shm"))
    with sqlite3.connect(database_path.as_uri() + "?mode=ro", uri=True) as db:
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA user_version").fetchone()[0] == 1
        assert db.execute("PRAGMA application_id").fetchone()[0] == 0x4E434D50
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        assert db.execute("SELECT state,stage FROM runs").fetchone() == ("succeeded", "finished")
        release = db.execute("SELECT release_id,run_id,status,manifest_json FROM releases").fetchone()
        assert release[:3] == (report["release_id"], report["run_id"], "published") and json.loads(release[3]) == report["files"]
        assert db.execute("SELECT count(*) FROM coverage_records WHERE stage='validated'").fetchone()[0] == 12
        coverage = db.execute("SELECT span_id,disposition,atom_ids_json,required_atom_ids_json,duplicate_of_span_id,exclusion_reason,decision_source,policy_version FROM coverage_records ORDER BY span_id").fetchall()
        ir_coverage = {c["span_id"]: c for c in read_json(fixture / "ir.json")["coverage"]}
        assert set(c[0] for c in coverage) == set(ir_coverage)
        for row in coverage:
            c = ir_coverage[row[0]]
            assert row[1:] == (c["disposition"], json.dumps(c["atom_ids"], separators=(",", ":")), json.dumps(c["required_atom_ids"], separators=(",", ":")), c["duplicate_of_span_id"], c["exclusion_reason"], c["decision_source"], c["policy_version"])
        commit = db.execute("SELECT release_id,report_json_path,report_json_sha256,report_md_path,report_md_sha256,manifest_sha256 FROM publication_commits").fetchall()
        manifest_sha = sha(json.dumps(report["files"], ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        assert commit == [(report["release_id"], "reports/validation.json", sha(report_path.read_bytes()), "reports/validation.md", sha(markdown_path.read_bytes()), manifest_sha)]
        assert db.execute("SELECT count(*) FROM rendered_files").fetchone()[0] == 2
    return {"root": str(root), "run_id": report["run_id"], "release_id": report["release_id"],
            "files": report["files"], "report_sha256": sha(report_path.read_bytes()), "markdown_sha256": sha(markdown_path.read_bytes()),
            "counts": report["counts"], "dimensions": report["dimensions"], "input_hashes": report["input_hashes"], "versions": report["versions"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True, help="Absent new test root with existing parent")
    args = parser.parse_args()
    root, cli = args.root.absolute(), args.cli.resolve(strict=True)
    repo = Path(__file__).resolve().parent.parent
    fixture = repo / "fixtures/text-compare-v1"
    if root.exists() or not root.parent.is_dir() or fixture.resolve().is_relative_to(root) or root.is_relative_to(fixture.resolve()):
        raise ValueError("Select an absent test root outside the fixture")
    root.mkdir()
    sentinel = root / "outside.md"
    sentinel.write_text("人工内容必须保持不变\n", encoding="utf-8")
    original, original_expected = snapshot(fixture), (fixture / "expected.json").read_bytes()
    sentinel_before = (sentinel.read_bytes(), sentinel.stat().st_mtime_ns)
    results, calls = [], []
    def run(case, output, code, diagnostic=None):
        before = snapshot(case)
        started = time.time_ns()
        p = subprocess.run([str(cli), "case", "compile", str(case), str(output)], capture_output=True, check=False, timeout=30)
        assert p.returncode == code, (str(output), p.returncode, p.stderr.decode("utf-8"))
        assert snapshot(case) == before
        if diagnostic:
            assert diagnostic in p.stderr.decode("utf-8")
        else:
            assert not p.stderr
        value = json.loads(p.stdout) if p.stdout else None
        calls.append({"case": str(case), "output": str(output), "exit_code": p.returncode, "stderr": p.stderr.decode("utf-8"), "stdout_sha256": sha(p.stdout), "started_ns": started, "finished_ns": time.time_ns()})
        return value
    for name in ("output-a", "output-b"):
        results.append(verify(root / name, fixture, run(fixture, root / name, 0)))
    assert results[0]["files"] == results[1]["files"]
    assert results[0]["run_id"] != results[1]["run_id"] and results[0]["release_id"] != results[1]["release_id"]
    for value in results:
        assert uuid.UUID(value["run_id"]).version == 4 and uuid.UUID(value["release_id"]).version == 4
    before = snapshot(root / "output-a")
    assert run(fixture, root / "output-a", 3, "COMPILE_OUTPUT_EXISTS") is None
    assert snapshot(root / "output-a") == before
    assert run(fixture, fixture, 3, "COMPILE_INPUT_OUTPUT_OVERLAP") is None
    assert run(fixture, root / "missing-parent/new", 3, "PROJECT_IO_ERROR") is None
    assert not (root / "missing-parent").exists()
    for name, filename, change in [
        ("missing-condition", "note-plan.json", lambda p: p["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][1].update(text="甲容器的虚构刻度为5 cm。")),
        ("pending-coverage", "ir.json", lambda p: p["coverage"][0].update(disposition="pending")),
    ]:
        case = root / name
        shutil.copytree(fixture, case)
        edit(case, filename, change)
        if filename == "ir.json":
            edit(case, "note-plan.json", lambda p: p.update(knowledge_sha256=sha((case / filename).read_bytes())))
        assert (case / "expected.json").read_bytes() == original_expected
        output = root / (name + "-output")
        value = run(case, output, 3, "CASE_COMPILATION_FAILED")
        assert value["output_created"] is False and value["release_status"] == "not_started" and value["report"] is None and value["issues"]
        assert not output.exists()
    variant = root / "table-alias-case"
    shutil.copytree(fixture, variant)
    link = {"target_concept_id": "C6h", "display_text": "乙容器入口"}
    edit(variant, "note-plan.json", lambda p: p["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][1].update(links=[link]))
    def expected_alias(p):
        p["tables"][0]["rows"][0]["cells"][1]["links"] = [link]
        p["required_links"].append({"from_node_id": "T5x", **link})
    edit(variant, "expected.json", expected_alias)
    results.append(verify(root / "table-alias-output", variant, run(variant, root / "table-alias-output", 0), alias=True))
    assert snapshot(fixture) == original and (sentinel.read_bytes(), sentinel.stat().st_mtime_ns) == sentinel_before
    evidence = {"schema_version": 1, "scope": "fixture_compile_standard_library_reread", "cli": str(cli), "cli_sha256": sha(cli.read_bytes()), "root": str(root), "calls": calls, "published": results, "input_and_external_sentinel_unchanged": True, "original_expected_sha256": sha(original_expected), "scientific_quality": "not_run", "native_obsidian": "separate_check_required"}
    path = root / "evidence.json"
    path.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"evidence": str(path), "calls": len(calls), "published": len(results), "passed": True}, ensure_ascii=False))


if __name__ == "__main__":
    main()
