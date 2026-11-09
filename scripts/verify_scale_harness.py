#!/usr/bin/env python3
"""Read-only auditor for the synthetic scale harness.

This script never imports the Rust product and never runs the CLI to decide
anything. It re-derives the deterministic half of a measurement from the
parameters alone and compares that with what was written.

Usage: <output root> [--package DIR] [--json] [--freeze]
       --package DIR [--json] [--freeze]

Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "scale-harness-v1"
SCHEMA_VERSION = 1
CASE_ID = "scale-synthetic-v1"
OUTPUT_PRODUCER = "scale_harness_output"
MAX_PAGES = 5000
MAX_HEADINGS = 12
MAX_PARAGRAPHS = 12

STRUCTURE = "structure.json"
TIMINGS = "timings.json"
REPORT = "report.md"
MANIFEST = "manifest.json"
OUTPUT_ENTRIES = (MANIFEST, REPORT, STRUCTURE, TIMINGS)
FROZEN = "frozen-sha256.json"

CONCEPTS = (
    ("concept-glycolysis", "糖酵解"),
    ("concept-gluconeogenesis", "糖异生"),
    ("concept-tca", "三羧酸循环"),
    ("concept-pfk", "磷酸果糖激酶"),
    ("concept-pyruvate", "丙酮酸"),
    ("concept-nadph", "NADPH"),
    ("concept-atp", "ATP"),
    ("concept-lactate", "乳酸"),
)
NAMES = tuple(canonical for _, canonical in CONCEPTS)

SPEC_FIELDS = ("pages", "headings_per_page", "paragraphs_per_page")
COUNT_FIELDS = (
    "pages",
    "bytes",
    "spans",
    "headings",
    "paragraphs",
    "concepts",
    "occurrences",
    "warnings",
)
PAGE_FIELDS = (
    "index",
    "path",
    "sha256",
    "byte_length",
    "span_count",
    "headings",
)
LIMIT_FIELDS = ("max_topics", "max_goals", "max_concepts", "max_atoms", "detail")
STRUCTURE_FIELDS = (
    "policy",
    "schema_version",
    "case_id",
    "spec",
    "counts",
    "pages",
    "warning_counts",
    "limits",
    "nfr10_status",
    "model_calls",
    "network_requests",
)
TIMINGS_FIELDS = (
    "analyse_millis",
    "align_millis",
    "total_millis",
    "peak_rss_bytes",
    "note",
)
MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "spec",
    "structure_sha256",
    "structure_byte_length",
    "counts",
    "nfr10_status",
)
CASES_FIELDS = (
    "schema_version",
    "producer",
    "case_id",
    "policy",
    "sequence",
    "variants",
    "expected",
    "boundaries",
)
EXPECTED_FIELDS = (
    "policy",
    "schema_version",
    "case_id",
    "spec",
    "counts",
    "pages",
    "warning_counts",
    "limits",
    "nfr10_status",
    "model_calls",
    "network_requests",
    "determinism",
    "boundaries",
    "derivation",
)


class Report:
    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append((name, bool(ok), detail))

    @property
    def failures(self) -> list[tuple[str, bool, str]]:
        return [check for check in self.checks if not check[1]]

    def dump(self, as_json: bool) -> int:
        if as_json:
            payload = {
                "checks": [
                    {"name": name, "ok": ok, "detail": detail}
                    for name, ok, detail in self.checks
                ],
                "total": len(self.checks),
                "failures": len(self.failures),
            }
            sys.stdout.write(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
        else:
            for name, _, detail in self.failures:
                sys.stdout.write(f"[失败] {name}：{detail}\n")
            sys.stdout.write(
                f"合计 {len(self.checks)} 项检查：失败 {len(self.failures)}、提示 0\n"
            )
        return 0 if not self.failures else 1


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


class StrictError(ValueError):
    pass


def strict_load(
    path: Path, fields: tuple[str, ...], label: str, canonical_required: bool = True
) -> dict:
    """Read a JSON object whose key set must match exactly.

    Both the canonical encoding and the canonical encoding plus one trailing
    newline are accepted, because the product writes files with a single trailing
    newline while nested records stay canonical inside a parent. Fixture
    declarations are human-authored with indentation, so the package side passes
    ``canonical_required=False``: the check there is the content, not the encoding.
    """
    if not path.is_file():
        raise StrictError(f"{label} 不存在：{path}")
    raw = read_bytes(path)
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise StrictError(f"{label} 不是合法 UTF-8：{error}") from error
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise StrictError(f"{label} 不是合法 JSON：{error}") from error
    if not isinstance(value, dict):
        raise StrictError(f"{label} 必须是 JSON 对象")
    if set(value) != set(fields):
        missing = sorted(set(fields) - set(value))
        extra = sorted(set(value) - set(fields))
        raise StrictError(f"{label} 字段不符：缺少 {missing}，多出 {extra}")
    if canonical_required:
        expected = canonical(value)
        if text not in (expected, expected + "\n"):
            raise StrictError(f"{label} 不是规范编码")
    return value


def page_body(index: int, headings: int, paragraphs: int) -> str:
    out = []
    for heading in range(headings):
        concept = NAMES[(index + heading) % len(NAMES)]
        out.append(f"## {index}.{heading} {concept} 的第 {heading} 节\n\n")
        for paragraph in range(paragraphs):
            first = NAMES[(index + heading + paragraph) % len(NAMES)]
            second = NAMES[(index + heading + paragraph + 3) % len(NAMES)]
            out.append(
                f"第 {paragraph} 段讨论{first}与{second}的关系，"
                "这一段是合成文本，只用来测量规模，不代表任何教材内容。\n\n"
            )
    return "".join(out)


def replay(spec: dict) -> dict:
    """Independently derive the deterministic half of the measurement.

    The span model comes from the segmentation rules: a heading line is one
    heading span, a paragraph block is one text span, and each block is followed
    by a run of blank lines that collapses into a single formatting span.
    """
    pages = spec["pages"]
    headings = spec["headings_per_page"]
    paragraphs = spec["paragraphs_per_page"]
    per_page_spans = headings * 2 + headings * paragraphs * 2
    total_bytes = 0
    occurrences = 0
    records = []
    for index in range(1, pages + 1):
        body = page_body(index, headings, paragraphs)
        raw = body.encode("utf-8")
        total_bytes += len(raw)
        occurrences += sum(body.count(name) for name in NAMES)
        records.append(
            {
                "index": index,
                "path": f"合成/第{index:04}页.md",
                "sha256": sha256(raw),
                "byte_length": len(raw),
                "span_count": per_page_spans,
                "headings": headings,
            }
        )
    counts = {
        "pages": pages,
        "bytes": total_bytes,
        "spans": pages * per_page_spans,
        "headings": pages * headings,
        "paragraphs": pages * headings * paragraphs,
        "concepts": len(NAMES),
        "occurrences": occurrences,
        "warnings": 0,
    }
    return {"counts": counts, "pages": records}


def check_structure(path: Path, report: Report) -> dict | None:
    try:
        structure = strict_load(path, STRUCTURE_FIELDS, "结构产物")
    except StrictError as error:
        report.record("structure-strict", False, str(error))
        return None
    report.record("structure-strict", True)
    report.record("structure-policy", structure["policy"] == POLICY, structure["policy"])
    report.record(
        "structure-schema", structure["schema_version"] == SCHEMA_VERSION,
        str(structure["schema_version"]),
    )
    report.record("structure-case", structure["case_id"] == CASE_ID, structure["case_id"])
    spec = structure["spec"]
    report.record("structure-spec-fields", set(spec) == set(SPEC_FIELDS), str(sorted(spec)))
    report.record("structure-count-fields",
                  set(structure["counts"]) == set(COUNT_FIELDS), str(sorted(structure["counts"])))
    report.record("structure-limit-fields", set(structure["limits"]) == set(LIMIT_FIELDS),
                  str(sorted(structure["limits"])))
    report.record(
        "structure-zero-calls",
        structure["model_calls"] == 0 and structure["network_requests"] == 0,
        f'{structure["model_calls"]}/{structure["network_requests"]}',
    )
    report.record("structure-nfr10", structure["nfr10_status"] == "not_met",
                  structure["nfr10_status"])
    for index, page in enumerate(structure["pages"]):
        if set(page) != set(PAGE_FIELDS):
            report.record(f"structure-page-fields:{index}", False, str(sorted(page)))
    expected = replay(spec)
    report.record("structure-counts", structure["counts"] == expected["counts"],
                  f'{structure["counts"]} != {expected["counts"]}')
    report.record("structure-pages", structure["pages"] == expected["pages"],
                  "页面记录与独立复算不一致")
    report.record("structure-pages-count", len(structure["pages"]) == structure["counts"]["pages"],
                  f'{len(structure["pages"])} != {structure["counts"]["pages"]}')
    return structure


def check_timings(path: Path, report: Report) -> dict | None:
    try:
        timings = strict_load(path, TIMINGS_FIELDS, "耗时产物")
    except StrictError as error:
        report.record("timings-strict", False, str(error))
        return None
    report.record("timings-strict", True)
    total = timings["total_millis"]
    parts = timings["analyse_millis"] + timings["align_millis"]
    report.record("timings-total", total == parts, f"{total} != {parts}")
    report.record(
        "timings-note",
        "不参与确定性比较" in timings["note"],
        timings["note"],
    )
    return timings


def check_manifest(path: Path, structure: dict | None, report: Report) -> dict | None:
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "输出清单")
    except StrictError as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, manifest["policy"])
    report.record("manifest-producer", manifest["producer"] == OUTPUT_PRODUCER,
                  manifest["producer"])
    report.record("manifest-case", manifest["case_id"] == CASE_ID, manifest["case_id"])
    report.record(
        "manifest-no-timings",
        "timings_sha256" not in manifest,
        "输出清单不得依赖耗时产物",
    )
    if structure is None:
        return manifest
    report.record("manifest-counts", manifest["counts"] == structure["counts"],
                  "输出清单的计数与结构产物不一致")
    report.record("manifest-spec", manifest["spec"] == structure["spec"],
                  "输出清单的参数与结构产物不一致")
    report.record(
        "manifest-nfr10",
        manifest["nfr10_status"] == structure["nfr10_status"],
        manifest["nfr10_status"],
    )
    return manifest


def check_report(path: Path, structure: dict | None, report: Report) -> None:
    if not path.is_file():
        report.record("report-present", False, f"缺少 {path.name}")
        return
    text = path.read_text(encoding="utf-8")
    report.record("report-present", True)
    for needle, name in (
        ("合成语料", "report-synthetic"),
        ("不证明真实教材", "report-boundary"),
        ("未达标项", "report-unmet"),
        ("timings.json", "report-timings"),
        ("不调用模型", "report-model"),
    ):
        report.record(name, needle in text, needle)
    report.record("report-no-wall-clock", " ms" not in text,
                  "报告里不应出现耗时读数")
    if structure is not None:
        report.record(
            "report-atoms-cap",
            f"atoms={structure['limits']['max_atoms']}" in text,
            "报告必须写出规划上限",
        )
        report.record(
            "report-spans",
            f"文本片段 {structure['counts']['spans']}" in text,
            "报告必须写出实测片段数",
        )


def check_layout(root: Path, report: Report) -> None:
    if not root.is_dir():
        report.record("layout-root", False, f"{root} 不是目录")
        return
    names = sorted(item.name for item in root.iterdir())
    report.record("layout-entries", names == sorted(OUTPUT_ENTRIES), str(names))
    links = [path for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-no-symlinks", not links, str([str(p) for p in links]))


def check_output(root: Path, report: Report) -> None:
    structure = check_structure(root / STRUCTURE, report)
    check_timings(root / TIMINGS, report)
    manifest = check_manifest(root / MANIFEST, structure, report)
    check_report(root / REPORT, structure, report)
    if manifest is not None and (root / STRUCTURE).is_file():
        digest = sha256(read_bytes(root / STRUCTURE))
        report.record("manifest-structure-digest", manifest["structure_sha256"] == digest,
                      "结构产物摘要不符")
        report.record(
            "manifest-structure-length",
            manifest["structure_byte_length"] == (root / STRUCTURE).stat().st_size,
            "结构产物字节数不符",
        )


def check_package(package: Path, report: Report) -> dict | None:
    if not (package / "spec.json").is_file():
        report.record("package-present", False, f"{package} 缺少 spec.json")
        return None
    report.record("package-present", True)
    try:
        spec = strict_load(
            package / "spec.json",
            ("policy", "schema_version", "producer", "case_id", "description", "spec"),
            "参数声明",
            canonical_required=False,
        )
    except StrictError as error:
        report.record("package-spec-strict", False, str(error))
        return None
    report.record("package-spec-strict", True)
    report.record("package-policy", spec["policy"] == POLICY, spec["policy"])
    report.record("package-case", spec["case_id"] == CASE_ID, spec["case_id"])

    try:
        expected = strict_load(
            package / "expected.json", EXPECTED_FIELDS, "先验", canonical_required=False
        )
    except StrictError as error:
        report.record("package-expected-strict", False, str(error))
        return spec["spec"]
    report.record("package-expected-strict", True)
    report.record("package-expected-spec", expected["spec"] == spec["spec"],
                  "先验的参数与声明不一致")
    replay_result = replay(spec["spec"])
    report.record("expected-counts", expected["counts"] == replay_result["counts"],
                  f'{expected["counts"]} != {replay_result["counts"]}')
    report.record("expected-pages", expected["pages"] == replay_result["pages"],
                  "先验的页面记录与独立复算不一致")
    report.record("expected-nfr10", expected["nfr10_status"] == "not_met",
                  expected["nfr10_status"])
    report.record(
        "expected-boundary",
        expected["boundaries"].get("synthetic") is True
        and expected["boundaries"].get("scientific_status") == "not_run",
        str(expected["boundaries"].get("scientific_status")),
    )
    report.record("expected-derivation", len(expected["derivation"]) >= 5,
                  f'{len(expected["derivation"])} 条推导')
    for key in LIMIT_FIELDS:
        if key == "detail":
            continue
        report.record(f"expected-limit:{key}", expected["limits"][key] > 0,
                      str(expected["limits"][key]))

    try:
        cases = strict_load(
            package / "cases.json", CASES_FIELDS, "用例表", canonical_required=False
        )
    except StrictError as error:
        report.record("package-cases-strict", False, str(error))
        return spec["spec"]
    report.record("package-cases-strict", True)
    report.record("package-cases-policy", cases["policy"] == POLICY, cases["policy"])
    report.record("package-cases-expected", cases["expected"] == "expected.json",
                  cases["expected"])
    names = [variant["name"] for variant in cases["variants"]]
    report.record("package-variants", len(names) >= 5, str(names))
    report.record("package-variants-unique", len(names) == len(set(names)), str(names))
    return spec["spec"]


def check_package_output(package: Path, root: Path, report: Report) -> None:
    """The real run must match the prior written before it."""
    try:
        expected = strict_load(
            package / "expected.json", EXPECTED_FIELDS, "先验", canonical_required=False
        )
    except StrictError as error:
        report.record("output-expected", False, str(error))
        return
    try:
        structure = strict_load(root / STRUCTURE, STRUCTURE_FIELDS, "结构产物")
    except StrictError as error:
        report.record("output-structure", False, str(error))
        return
    report.record("output-expected-counts", structure["counts"] == expected["counts"],
                  f'{structure["counts"]} != {expected["counts"]}')
    report.record("output-expected-pages", structure["pages"] == expected["pages"],
                  "实测的页面记录与先验不一致")
    report.record("output-expected-spec", structure["spec"] == expected["spec"],
                  "实测参数与先验不一致")
    report.record("output-expected-nfr10",
                  structure["nfr10_status"] == expected["nfr10_status"],
                  structure["nfr10_status"])
    for key in ("max_topics", "max_goals", "max_concepts", "max_atoms"):
        report.record(f"output-expected-limit:{key}",
                      structure["limits"][key] == expected["limits"][key],
                      f'{structure["limits"][key]} != {expected["limits"][key]}')


def check_frozen(package: Path, report: Report) -> None:
    frozen_path = package / FROZEN
    if not frozen_path.is_file():
        report.record("frozen-present", False, f"缺少 {FROZEN}")
        return
    report.record("frozen-present", True)
    try:
        frozen = json.loads(frozen_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        report.record("frozen-strict", False, str(error))
        return
    report.record("frozen-strict", True)
    members = frozen.get("files", {})
    actual = {}
    for path in sorted(package.rglob("*")):
        if path.is_file() and path != frozen_path:
            actual[path.relative_to(package).as_posix()] = {
                "sha256": sha256(read_bytes(path)),
                "byte_length": path.stat().st_size,
            }
    report.record("frozen-members", set(members) == set(actual),
                  f"多出 {sorted(set(actual) - set(members))}，缺少 {sorted(set(members) - set(actual))}")
    for name in sorted(set(members) & set(actual)):
        report.record(f"frozen-digest:{name}", members[name] == actual[name],
                      f"{members[name]} != {actual[name]}")


def freeze(package: Path) -> int:
    files = {}
    for path in sorted(package.rglob("*")):
        if path.is_file() and path.name != FROZEN:
            files[path.relative_to(package).as_posix()] = {
                "sha256": sha256(read_bytes(path)),
                "byte_length": path.stat().st_size,
            }
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    sys.stdout.write(f"已冻结 {len(files)} 个成员\n")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("root", nargs="?")
    parser.add_argument("--package")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args(argv)

    if args.freeze:
        if not args.package:
            sys.stderr.write("--freeze 需要 --package。\n")
            return 2
        return freeze(Path(args.package))

    if not args.root and not args.package:
        sys.stderr.write("至少要给一个输出根或 --package。\n")
        return 2

    report = Report()
    if args.root:
        root = Path(args.root)
        if not root.is_dir():
            sys.stderr.write(f"{root} 不是目录。\n")
            return 2
        check_layout(root, report)
        check_output(root, report)
    if args.package:
        package = Path(args.package)
        if not package.is_dir():
            sys.stderr.write(f"{package} 不是目录。\n")
            return 2
        check_package(package, report)
        check_frozen(package, report)
        if args.root:
            check_package_output(package, Path(args.root), report)
    return report.dump(args.json)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
