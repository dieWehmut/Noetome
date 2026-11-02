"""Independently verify real CLI outputs against frozen PDF authoring facts.

This tool imports neither the PDF parser nor Noetome implementation helpers.
It uses only selected public fixtures and a new caller-selected test directory.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import subprocess
import time


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fingerprint(path: Path) -> dict:
    data = path.read_bytes()
    return {"sha256": sha(data), "byte_length": len(data)}


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf8")


def write_new(path: Path, data: bytes) -> None:
    with path.open("xb") as output:
        output.write(data)


def run(command: list[str], expected_exit: int) -> tuple[dict, float, str]:
    start = time.perf_counter()
    process = subprocess.run(command, capture_output=True, timeout=45)
    wall_ms = round((time.perf_counter() - start) * 1000, 3)
    diagnostics = process.stderr.decode("utf8", errors="replace")
    assert process.returncode == expected_exit, (command, process.returncode, diagnostics)
    return json.loads(process.stdout), wall_ms, diagnostics


def snapshot(root: Path) -> dict:
    files = {}
    for path in sorted(root.rglob("*")):
        assert not path.is_symlink() and not path.is_junction(), path
        if path.is_file():
            files[path.relative_to(root).as_posix()] = fingerprint(path)
        else:
            assert path.is_dir(), path
    return files


def close_numbers(actual: list, expected: list, tolerance: float) -> None:
    assert len(actual) == len(expected)
    for observed, want in zip(actual, expected):
        assert abs(observed - want) <= tolerance, (actual, expected, tolerance)


def check_case(root: Path, case: dict, report: dict) -> tuple[dict, dict]:
    manifest = json.loads((root / "parser/pdf-manifest.json").read_bytes())
    assert manifest["status"] == report["status"] == case["status"]
    assert manifest["page_count"] == report["page_count"] == case["parse_page_count"]
    expected_fingerprint = {"sha256": case["raw_sha256"], "byte_length": case["byte_length"]}
    assert manifest["input_fingerprint"] == report["input_fingerprint"] == expected_fingerprint
    assert fingerprint(root / "parser/raw.pdf") == expected_fingerprint
    assert manifest["parser"] == report["parser"]
    resources = report["sidecar"]["resources"]
    names = set()
    for resource in resources:
        relative = PurePosixPath(resource["relative_path"])
        assert not relative.is_absolute() and relative.parts
        assert all(part not in (".", "..") for part in relative.parts)
        assert "\\" not in str(relative) and ":" not in str(relative)
        name = "parser/" + relative.as_posix()
        assert name not in names
        names.add(name)
        assert fingerprint(root / name) == resource["fingerprint"], name
    assert report["resource_bytes"] == sum(r["fingerprint"]["byte_length"] for r in resources)
    assert report["manifest_sha256"] == sha((root / "parser/pdf-manifest.json").read_bytes())
    files = snapshot(root)
    assert set(files) == names | {"parse-result.json", "parse-result.md"}
    assert json.loads((root / "parse-result.json").read_bytes()) == report
    markdown = (root / "parse-result.md").read_text(encoding="utf8")
    for field in ("status", "manifest_sha256", "resource_bytes", "page_count", "elapsed_ms"):
        rendered = "null" if report[field] is None else str(report[field])
        assert f"| {field} | {rendered} |" in markdown, field
    pages = manifest["pages"]
    assert len(pages) == len(case["pages"])
    if manifest["page_count"] is not None:
        assert [p["physical_page_index"] for p in pages] == list(range(manifest["page_count"]))
    for actual, want in zip(pages, case["pages"]):
        for field in ("physical_page_index", "display_page_number", "printed_page_label", "status"):
            assert actual[field] == want[field], (case["file"], field)
        assert actual["display_page_number"] == actual["physical_page_index"] + 1
        assert actual["printed_page_label"] is None
        if actual["status"] == "failed":
            assert actual["text"] is None and not actual["glyphs"] and not actual["regions"]
            continue
        geometry = actual["geometry"]
        assert (geometry["unit"], geometry["origin"], geometry["y_axis"]) == ("pt", "crop_top_left", "down")
        for field in ("media_box", "crop_box"):
            close_numbers(geometry[field], want[field], 0.000001)
        assert geometry["rotation_degrees"] == want["rotation_degrees"]
        for field in ("user_unit", "width_pt", "height_pt"):
            close_numbers([geometry[field]], [want["geometry"][field]], 0.000001)
        close_numbers(geometry["pdf_to_display"], want["geometry"]["pdf_to_display"], 0.000001)
        assert len(actual["glyphs"]) == len(want["glyph_positions"])
        for glyph, expected_glyph in zip(actual["glyphs"], want["glyph_positions"]):
            assert glyph["ordinal"] == expected_glyph["ordinal"]
            assert glyph["text"] == expected_glyph["text"]
            close_numbers(glyph["bbox"], expected_glyph["bbox"], expected_glyph["tolerance_pt"])
        if not want["lines"]:
            continue  # Review pages have a character oracle, no reliable reading-order claim.
        text = (root / f"parser/pages/{actual['physical_page_index']:06}.txt").read_bytes()
        expected_text = "".join(line["text"] + "\n" for line in want["lines"]).encode("utf8")
        assert text == expected_text, (case["file"], actual["physical_page_index"])
        assert actual["text"]["sha256"] == sha(text)
        assert len(actual["regions"]) == len(want["lines"])
        cursor = 0
        for region, line in zip(actual["regions"], want["lines"]):
            assert region["locator_kind"] == "pdf_page_region"
            assert region["physical_page_index"] == actual["physical_page_index"]
            reference = region["text"]
            assert reference["resource_id"] == actual["text"]["resource_id"]
            assert reference["resource_sha256"] == sha(text)
            start, end = reference["start_byte"], reference["end_byte"]
            assert start == cursor and text[start:end] == line["text"].encode("utf8")
            assert text[end:end+1] == b"\n"
            text[:start].decode("utf8")
            text[:end].decode("utf8")
            cursor = end + 1
            close_numbers(region["bbox"], line["bbox"], line["tolerance_pt"])
        assert cursor == len(text)
    for status, field in (("parsed", "parsed_page_count"), ("needs_review", "needs_review_page_count"), ("failed", "failed_page_count")):
        assert report[field] == sum(page["status"] == status for page in pages)
    return manifest, files


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path, help="a new selected test root; parent must exist")
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[1]
    fixtures = repository / "fixtures/pdf-text-v1"
    expected_path = fixtures / "expected.json"
    expected = json.loads(expected_path.read_bytes())
    review_path = repository / "doc/validation/assets/B03-T02-authoring-review.json"
    review = json.loads(review_path.read_bytes())
    reviews = {record["file"]: record for record in review["records"]}
    assert review["fixture_count"] == len(expected["cases"])
    assert set(reviews) == {case["file"] for case in expected["cases"]}
    assert review["render_count"] == sum(case["authoring_page_count"] or 0 for case in expected["cases"])
    binary = args.binary.resolve(strict=True)
    runtime = Path(os.environ["NOETOME_PDF_PYTHON"])
    assert runtime.is_absolute() and runtime.is_file()
    args.output.mkdir(exist_ok=False)
    output = args.output.resolve(strict=True)
    sentinel = output / "external-sentinel.bin"
    sentinel_bytes = b"Human bytes outside every parse child.\r\n"
    write_new(sentinel, sentinel_bytes)
    facts = {
        "schema_version": 1,
        "started_at_utc": datetime.now(timezone.utc).isoformat(),
        "verifier": "stdlib authoring oracle and filesystem byte audit, no parser/Core helpers",
        "platform": platform.platform(), "verifier_python": platform.python_version(),
        "binary": {"path": str(binary), **fingerprint(binary)},
        "runtime": str(runtime), "test_root": str(output),
        "expected": fingerprint(expected_path), "prior_visual_review": fingerprint(review_path),
        "measurement": "one real CLI process per operation; wall_ms includes launch, sidecar, Core and reports; elapsed_ms is reported acquisition+child+initial Core; resource_bytes sums validated raw/text/manifest",
        "not_run": ["memory/CPU sampling", "1000-page throughput", "24-hour stability", "cross-platform compatibility", "human scientific gold standard", "OCR", "model/knowledge recall or scientific accuracy", "physical UserUnit scaling by independent renderer", "pdftotext unavailable"],
        "cases": [],
    }
    try:
        version = subprocess.run([str(binary), "--version"], capture_output=True, check=True, timeout=10)
        facts["binary"]["version"] = version.stdout.decode("utf8").strip()
        for case in expected["cases"]:
            selected = fixtures / case["file"]
            original = selected.read_bytes()
            assert sha(original) == case["raw_sha256"] == reviews[case["file"]]["raw_sha256"]
            assert len(original) == case["byte_length"]
            root = output / selected.stem
            exit_code = 0 if case["status"] == "parsed" else 3
            report, wall_ms, diagnostics = run([str(binary), "source", "parse-pdf", str(selected), str(root)], exit_code)
            manifest, files = check_case(root, case, report)
            before_inspect = snapshot(root)
            inspected, inspect_ms, inspect_diagnostics = run([str(binary), "source", "inspect-pdf", str(root)], exit_code)
            assert inspected == report and snapshot(root) == before_inspect
            assert selected.read_bytes() == original and sentinel.read_bytes() == sentinel_bytes
            item = {
                "file": case["file"], "category": case["category"], "raw_sha256": sha(original),
                "input_bytes": len(original), "authoring_page_count": case["authoring_page_count"],
                "page_count": manifest["page_count"], "status": report["status"],
                "parse_exit": exit_code, "inspect_exit": exit_code,
                "wall_ms": wall_ms, "inspect_wall_ms": inspect_ms,
                "reported_elapsed_ms": report["elapsed_ms"], "resource_bytes": report["resource_bytes"],
                "total_output_file_bytes": sum(item["byte_length"] for item in files.values()),
                "parsed_pages": report["parsed_page_count"], "review_pages": report["needs_review_page_count"], "failed_pages": report["failed_page_count"],
                "glyphs_checked": sum(len(page["glyphs"]) for page in manifest["pages"]),
                "literal_lines_checked": sum(len(page["lines"]) for page in case["pages"]),
                "diagnostics": diagnostics, "inspect_diagnostics": inspect_diagnostics,
                "issues": [{"code": issue["code"], "unit_ids": issue["unit_ids"]} for issue in manifest["issues"]],
                "files": files, "inspect_read_only": True, "input_and_sentinel_unchanged": True,
            }
            facts["cases"].append(item)
        facts["real_cli_calls"] = 1 + 2 * len(facts["cases"])
        facts["physical_pages_recorded"] = sum(len(case["pages"]) for case in expected["cases"])
        facts["result"] = "passed"
        write_new(output / "support-matrix.json", json_bytes(facts))
        lines = ["# B03-T02 独立真实 CLI 支持矩阵\n\n", "固定预期来自作者 PDF 绘制操作与事先 Poppler 视觉记录；不由待测 parser 输出生成。每项实际 parse 与只读 inspect，非 parsed 的退出码为3。\n\n", "| 自造样例 | 状态 | 确认总页数 | parsed / review / failed | parse 实际ms | inspect 实际ms | 资源字节 |\n", "| --- | --- | --- | --- | --- | --- | --- |\n"]
        for case in facts["cases"]:
            count = "unknown" if case["page_count"] is None else case["page_count"]
            lines.append(f"| {case['file']} | {case['status']} | {count} | {case['parsed_pages']} / {case['review_pages']} / {case['failed_pages']} | {case['wall_ms']} | {case['inspect_wall_ms']} | {case['resource_bytes']} |\n")
        lines.append("\n时间含原生 CLI 启动、Python、Core及最终报告写入；一次小样例实测，无缓存或统计吞吐承诺。资源字节为raw/text/manifest总和，完整文件SHA在JSON；不等于进程内存。\n\n未运行：" + "、".join(facts["not_run"]) + "。\n")
        write_new(output / "support-matrix.md", "".join(lines).encode("utf8"))
        print(f"Verified {len(facts['cases'])} real authored PDFs / {facts['physical_pages_recorded']} recorded pages / {facts['real_cli_calls']} CLI calls; all literal lines, glyph coordinates, SHA, status and read-only inspect checks passed.")
        print(output / "support-matrix.json")
    except BaseException as error:
        facts["result"] = "failed"
        facts["failure"] = repr(error)
        write_new(output / "verification-failure.json", json_bytes(facts))
        raise


if __name__ == "__main__":
    main()
