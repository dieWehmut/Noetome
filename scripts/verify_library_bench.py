#!/usr/bin/env python3
"""Read-only auditor for the library-scale measurement harness.

This script never imports the Rust product and never runs the CLI to decide
anything. It re-derives the synthetic corpus, the four stage products and the
nearest-rank percentiles from the parameters alone, then compares that with what
was written.

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

POLICY = "library-bench-v1"
SCHEMA_VERSION = 1
CASE_ID = "bench-library-v1"
AUTHOR_PRODUCER = "fixture_author"
OUTPUT_PRODUCER = "library_bench_output"
MAX_NOTES = 20_000
MAX_EDITS = 10_000
MAX_SAMPLES = 200
MAX_WARMUP = 50
PAGE_SIZE = 100
QUANTILE = 95

STRUCTURE = "structure.json"
TIMINGS = "timings.json"
REPORT = "report.md"
MANIFEST = "manifest.json"
OUTPUT_ENTRIES = (MANIFEST, REPORT, STRUCTURE, TIMINGS)
FROZEN = "frozen-sha256.json"

STAGE_SCAN = "scan"
STAGE_DETECT = "detect"
STAGE_PLAN = "plan"
STAGE_PAGE = "page"
STAGES = (STAGE_SCAN, STAGE_DETECT, STAGE_PLAN, STAGE_PAGE)

SPEC_FIELDS = ("policy", "schema_version", "producer", "case_id", "description", "spec",
               "threshold_millis", "boundaries")
RUN_SPEC_FIELDS = ("notes", "edits", "samples", "warmup")
COUNT_FIELDS = ("notes", "edits", "changed", "topics", "concepts", "spans", "pages",
                "page_size", "impacts")
CHANGED_FIELDS = ("note_id", "path")
COVERAGE_FIELDS = ("requirement", "status", "detail")
PROTOCOL_FIELDS = ("samples", "warmup", "process_start_included", "percentile",
                   "quantile_method", "sample_unit", "note")
STRUCTURE_FIELDS = ("policy", "schema_version", "case_id", "spec", "counts", "corpus_sha256",
                    "baseline_sha256", "changed_notes", "first_page_sha256", "protocol",
                    "threshold_millis", "coverage", "model_calls", "network_requests")
STAGE_FIELDS = ("stage", "samples_millis", "p95_millis", "median_millis", "min_millis",
                "max_millis")
VERDICT_FIELDS = ("requirement", "stage", "p95_millis", "threshold_millis", "status")
MEMORY_FIELDS = ("rss_before_bytes", "rss_after_scan_bytes", "rss_after_plan_bytes",
                 "peak_rss_bytes", "note")
TIMINGS_FIELDS = ("policy", "case_id", "protocol", "stages", "threshold_millis", "verdicts",
                  "memory", "note")
MANIFEST_FIELDS = ("policy", "producer", "schema_version", "case_id", "spec",
                   "structure_sha256", "structure_byte_length", "counts", "coverage")
CASES_FIELDS = ("schema_version", "producer", "case_id", "policy", "sequence", "expected",
                "variants", "boundaries")
EXPECTED_FIELDS = ("policy", "case_id", "spec", "counts", "corpus_sha256", "baseline_sha256",
                   "first_page_sha256", "changed_notes", "protocol", "threshold_millis",
                   "coverage", "determinism", "stages", "boundaries", "derivation")


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

    The product writes one trailing newline; fixture declarations are
    human-authored with indentation, so the package side passes
    ``canonical_required=False`` and the check there is the content.
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


def topic_count(notes: int) -> int:
    return max(notes // 10, 1)


def concept_count(notes: int) -> int:
    return max(notes * 2, 3)


def concept_name(index: int, concepts: int) -> str:
    return f"concept-{index % concepts:04d}"


def topic_name(index: int, topics: int) -> str:
    return f"topic-{index % topics:03d}"


def concepts_of(index: int, concepts: int) -> list[str]:
    return [
        concept_name(index, concepts),
        concept_name(index + 7919, concepts),
        concept_name(index + 104_729, concepts),
    ]


def body_of(index: int, edited: bool, concepts: int) -> str:
    first, second, _ = concepts_of(index, concepts)
    text = (
        f"# 第 {index} 篇\n\n"
        f"## 定位\n\n{first} 与 {second} 的关系讨论。\n\n"
        f"## 反应\n\n第 {index} 篇的反应描述。\n\n"
    )
    if edited:
        text += "## 人工改动\n\n这一节是人工追加的内容。\n\n"
    return text


def edit_indices(notes: int, edits: int) -> list[int]:
    """Cursor starts at 3, steps by 7 modulo the note count, deduplicated."""
    seen: set[int] = set()
    out: list[int] = []
    cursor = 3 % max(notes, 1)
    while len(out) < edits:
        if cursor not in seen:
            seen.add(cursor)
            out.append(cursor)
        cursor = (cursor + 7) % max(notes, 1)
    return out


def heading(line: str) -> bool:
    indentation = len(line) - len(line.lstrip(" "))
    if indentation > 3:
        return False
    content = line[indentation:]
    hashes = len(content) - len(content.lstrip("#"))
    if not 1 <= hashes <= 6:
        return False
    rest = content[hashes:]
    return rest == "" or rest[0].isspace()


def span_count(text: str) -> int:
    """One span per heading, per blank run and per paragraph block."""
    lines = text.split("\n")
    if text.endswith("\n"):
        lines.pop()
    spans = 0
    index = 0
    while index < len(lines):
        if heading(lines[index]):
            index += 1
            spans += 1
        elif lines[index].strip() == "":
            index += 1
            while index < len(lines) and lines[index].strip() == "":
                index += 1
            spans += 1
        else:
            index += 1
            while (
                index < len(lines)
                and lines[index].strip() != ""
                and not heading(lines[index])
            ):
                index += 1
            spans += 1
    return spans


def replay(spec: dict) -> dict:
    """Independently derive the whole deterministic half of the measurement."""
    notes = spec["notes"]
    edits = spec["edits"]
    topics = topic_count(notes)
    concepts = concept_count(notes)
    changed = edit_indices(notes, edits)
    changed_set = set(changed)

    corpus_parts: list[str] = []
    bodies: list[str] = []
    baselines: list[str] = []
    spans = 0
    for index in range(notes):
        body = body_of(index, index in changed_set, concepts)
        baseline = body_of(index, False, concepts)
        bodies.append(sha256(body.encode("utf-8")))
        baselines.append(sha256(baseline.encode("utf-8")))
        spans += span_count(body)
        corpus_parts.append(f"合成/第{index:05d}篇.md\u0000{body}\u0000")

    impacted = 0
    changed_topics = {topic_name(index, topics) for index in changed}
    changed_concepts = {
        concept for index in changed for concept in concepts_of(index, concepts)
    }
    for index in range(notes):
        if index in changed_set:
            continue
        if topic_name(index, topics) in changed_topics:
            impacted += 1
            continue
        if any(concept in changed_concepts for concept in concepts_of(index, concepts)):
            impacted += 1

    rows = "".join(
        f"| {index} | 合成/第{index:05d}篇.md | {topic_name(index, topics)} |\n"
        for index in changed[:PAGE_SIZE]
    )
    page = (
        f"# 变更清单 第 1 页（每页 {PAGE_SIZE} 条）\n\n"
        "| 序号 | 路径 | 主题 |\n| --- | --- | --- |\n" + rows
    )

    counts = {
        "notes": notes,
        "edits": edits,
        "changed": len(changed),
        "topics": topics,
        "concepts": concepts,
        "spans": spans,
        "pages": (len(changed) + PAGE_SIZE - 1) // PAGE_SIZE,
        "page_size": PAGE_SIZE,
        "impacts": impacted,
    }
    return {
        "counts": counts,
        "corpus_sha256": sha256("".join(corpus_parts).encode("utf-8")),
        "baseline_sha256": sha256(
            "".join(f"{digest}\n" for digest in baselines).encode("utf-8")
        ),
        "first_page_sha256": sha256(page.encode("utf-8")),
        "changed_notes": [
            {"note_id": f"note-{index:05d}", "path": f"合成/第{index:05d}篇.md"}
            for index in changed
        ],
        "protocol": {
            "samples": spec["samples"],
            "warmup": spec["warmup"],
            "process_start_included": False,
            "percentile": QUANTILE,
            "quantile_method": "nearest_rank",
        },
        "stages": list(STAGES),
    }


def nearest_rank(samples: list[int], percentile: int = QUANTILE) -> int:
    if not samples:
        return 0
    ordered = sorted(samples)
    rank = max(1, -(-percentile * len(ordered) // 100))
    return ordered[min(rank, len(ordered)) - 1]


def check_structure(path: Path, report: Report) -> dict | None:
    try:
        structure = strict_load(path, STRUCTURE_FIELDS, "结构产物")
    except StrictError as error:
        report.record("structure-strict", False, str(error))
        return None
    report.record("structure-strict", True)
    report.record("structure-policy", structure["policy"] == POLICY, structure["policy"])
    report.record(
        "structure-schema",
        structure["schema_version"] == SCHEMA_VERSION,
        str(structure["schema_version"]),
    )
    report.record("structure-case", structure["case_id"] == CASE_ID, structure["case_id"])
    spec = structure["spec"]
    report.record("structure-spec-fields", set(spec) == set(RUN_SPEC_FIELDS),
                  str(sorted(spec)))
    report.record("structure-count-fields", set(structure["counts"]) == set(COUNT_FIELDS),
                  str(sorted(structure["counts"])))
    report.record("structure-protocol-fields",
                  set(structure["protocol"]) == set(PROTOCOL_FIELDS),
                  str(sorted(structure["protocol"])))
    report.record(
        "structure-zero-calls",
        structure["model_calls"] == 0 and structure["network_requests"] == 0,
        f'{structure["model_calls"]}/{structure["network_requests"]}',
    )
    report.record("structure-threshold", structure["threshold_millis"] == 2_000,
                  str(structure["threshold_millis"]))
    for index, entry in enumerate(structure["changed_notes"]):
        if set(entry) != set(CHANGED_FIELDS):
            report.record(f"structure-changed-fields:{index}", False, str(sorted(entry)))
    for index, coverage in enumerate(structure["coverage"]):
        if set(coverage) != set(COVERAGE_FIELDS):
            report.record(f"structure-coverage-fields:{index}", False,
                          str(sorted(coverage)))

    replay_result = replay(spec)
    report.record("structure-counts", structure["counts"] == replay_result["counts"],
                  f'{structure["counts"]} != {replay_result["counts"]}')
    report.record(
        "structure-corpus",
        structure["corpus_sha256"] == replay_result["corpus_sha256"],
        f'{structure["corpus_sha256"]} != {replay_result["corpus_sha256"]}',
    )
    report.record(
        "structure-baseline",
        structure["baseline_sha256"] == replay_result["baseline_sha256"],
        f'{structure["baseline_sha256"]} != {replay_result["baseline_sha256"]}',
    )
    report.record(
        "structure-first-page",
        structure["first_page_sha256"] == replay_result["first_page_sha256"],
        f'{structure["first_page_sha256"]} != {replay_result["first_page_sha256"]}',
    )
    report.record(
        "structure-changed",
        [entry["note_id"] for entry in structure["changed_notes"]]
        == [entry["note_id"] for entry in replay_result["changed_notes"]],
        "改动清单与独立复算不一致",
    )
    for key in ("samples", "warmup", "process_start_included", "percentile",
                "quantile_method"):
        report.record(
            f"structure-protocol:{key}",
            structure["protocol"][key] == replay_result["protocol"][key],
            f'{structure["protocol"][key]} != {replay_result["protocol"][key]}',
        )
    requirements = [coverage["requirement"] for coverage in structure["coverage"]]
    report.record("structure-coverage-set", requirements == ["NFR-11", "NFR-12"],
                  str(requirements))
    report.record(
        "structure-coverage-honest",
        all(
            coverage["status"] == "partially_measured"
            for coverage in structure["coverage"]
        ),
        str([coverage["status"] for coverage in structure["coverage"]]),
    )
    report.record(
        "structure-coverage-params",
        f'{spec["notes"]} 篇笔记' in structure["coverage"][0]["detail"]
        and f'{spec["edits"]} 处人工改动' in structure["coverage"][0]["detail"],
        structure["coverage"][0]["detail"],
    )
    report.record(
        "structure-coverage-gui",
        "没有界面" in structure["coverage"][0]["detail"]
        and "B13-T03" in structure["coverage"][1]["detail"],
        "覆盖率说明必须写出未验证的部分",
    )
    return structure


def check_timings(path: Path, structure: dict | None, report: Report) -> dict | None:
    try:
        timings = strict_load(path, TIMINGS_FIELDS, "耗时产物")
    except StrictError as error:
        report.record("timings-strict", False, str(error))
        return None
    report.record("timings-strict", True)
    report.record("timings-policy", timings["policy"] == POLICY, timings["policy"])
    report.record("timings-case", timings["case_id"] == CASE_ID, timings["case_id"])
    report.record("timings-threshold", timings["threshold_millis"] == 2_000,
                  str(timings["threshold_millis"]))
    report.record("timings-note", "不参与确定性比较" in timings["note"], timings["note"])
    report.record(
        "timings-memory-fields", set(timings["memory"]) == set(MEMORY_FIELDS),
        str(sorted(timings["memory"])),
    )
    report.record("timings-memory-note", "VmRSS" in timings["memory"]["note"],
                  timings["memory"]["note"])

    names = [stage["stage"] for stage in timings["stages"]]
    report.record("timings-stages", names == list(STAGES), str(names))
    for stage in timings["stages"]:
        name = stage["stage"]
        if set(stage) != set(STAGE_FIELDS):
            report.record(f"timings-stage-fields:{name}", False, str(sorted(stage)))
            continue
        samples = stage["samples_millis"]
        report.record(
            f"timings-samples:{name}",
            structure is None or len(samples) == structure["spec"]["samples"],
            f'{len(samples)} 个样本',
        )
        report.record(f"timings-min:{name}", stage["min_millis"] == min(samples),
                      f'{stage["min_millis"]} != {min(samples)}')
        report.record(f"timings-max:{name}", stage["max_millis"] == max(samples),
                      f'{stage["max_millis"]} != {max(samples)}')
        report.record(
            f"timings-p95:{name}", stage["p95_millis"] == nearest_rank(samples),
            f'{stage["p95_millis"]} != {nearest_rank(samples)}',
        )
        ordered = sorted(samples)
        report.record(
            f"timings-median:{name}",
            stage["median_millis"] == ordered[len(ordered) // 2],
            f'{stage["median_millis"]} != {ordered[len(ordered) // 2]}',
        )

    verdicts = timings["verdicts"]
    report.record("timings-verdict-count", len(verdicts) == len(STAGES),
                  str(len(verdicts)))
    for verdict in verdicts:
        if set(verdict) != set(VERDICT_FIELDS):
            report.record(f"timings-verdict-fields:{verdict.get('stage')}", False,
                          str(sorted(verdict)))
            continue
        expected = "met" if verdict["p95_millis"] <= verdict["threshold_millis"] else "not_met"
        report.record(
            f"timings-verdict:{verdict['stage']}",
            verdict["status"] == expected and verdict["requirement"] == "NFR-12",
            f'{verdict["status"]} != {expected}',
        )
        report.record(
            f"timings-verdict-threshold:{verdict['stage']}",
            verdict["threshold_millis"] == timings["threshold_millis"],
            str(verdict["threshold_millis"]),
        )
    if structure is not None:
        report.record(
            "timings-protocol-matches-structure",
            timings["protocol"] == structure["protocol"],
            "耗时产物与结构产物的协议不一致",
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
    report.record("manifest-coverage", manifest["coverage"] == structure["coverage"],
                  "输出清单的覆盖率说明与结构产物不一致")
    return manifest


def check_report(path: Path, structure: dict | None, timings: dict | None,
                 report: Report) -> None:
    if not path.is_file():
        report.record("report-present", False, f"缺少 {path.name}")
        return
    text = path.read_text(encoding="utf-8")
    report.record("report-present", True)
    for needle, name in (
        ("语料是**合成的**", "report-synthetic"),
        ("不证明真实笔记库", "report-boundary"),
        ("## 测量协议", "report-protocol"),
        ("nearest_rank", "report-quantile"),
        ("不含进程启动", "report-process-start"),
        ("## 门槛与判定规则", "report-threshold-section"),
        ("## 覆盖范围与未达标项", "report-coverage"),
        ("NFR-11（partially_measured）", "report-nfr11"),
        ("NFR-12（partially_measured）", "report-nfr12"),
        ("没有界面，因此「不因全量渲染冻结界面」没有被验证。", "report-gui-gap"),
        ("不联网、不调用模型", "report-model"),
    ):
        report.record(name, needle in text, needle)

    # The only millisecond figure the report may carry is the constant threshold;
    # any real reading would make two runs of the same parameters differ.
    cursor = 0
    numbers = []
    while True:
        found = text.find(" ms", cursor)
        if found < 0:
            break
        digits = ""
        position = found
        while position > 0 and text[position - 1].isdigit():
            position -= 1
            digits = text[position] + digits
        report.record(f"report-ms-number:{found}", digits != "",
                      f"字节 {found} 处的单位没有数字")
        if digits:
            numbers.append(int(digits))
        cursor = found + 3
    report.record("report-ms-present", bool(numbers), "报告必须写出常量门槛")
    report.record("report-ms-threshold-only",
                  all(value == 2_000 for value in numbers),
                  f"报告只允许出现常量门槛，实际 {numbers}")

    if structure is not None:
        report.record(
            "report-spans",
            f'文本片段 {structure["counts"]["spans"]}' in text,
            "报告必须写出实测片段数",
        )
        report.record(
            "report-impacts",
            f'影响范围 {structure["counts"]["impacts"]} 篇' in text,
            "报告必须写出影响范围",
        )
        report.record(
            "report-pages",
            f'落在 {structure["counts"]["pages"]} 页变更清单里' in text,
            "报告必须写出分页数",
        )
        report.record(
            "report-coverage-detail",
            all(coverage["detail"] in text for coverage in structure["coverage"]),
            "报告必须逐条写出覆盖率说明",
        )
    if timings is not None:
        # Compare the numbers the report actually carries against the measured
        # stage readings. Substring matching would be wrong here: "0 ms" occurs
        # inside "2000 ms", so the constant threshold would look like a leak.
        carried = {value for value in numbers}
        leaked = [
            stage["stage"]
            for stage in timings["stages"]
            if stage["p95_millis"] != 2_000 and stage["p95_millis"] in carried
        ]
        report.record(
            "report-no-stage-numbers",
            not leaked,
            f"报告里出现了阶段读数：{leaked}",
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
    timings = check_timings(root / TIMINGS, structure, report)
    manifest = check_manifest(root / MANIFEST, structure, report)
    check_report(root / REPORT, structure, timings, report)
    if manifest is not None and (root / MANIFEST).is_file() and (root / "report.md").is_file():
        report.record(
            "output-no-extra-entries",
            sorted(item.name for item in root.iterdir()) == sorted(OUTPUT_ENTRIES),
            "输出根只有四个文件",
        )
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
        spec = strict_load(package / "spec.json", SPEC_FIELDS, "参数声明",
                           canonical_required=False)
    except StrictError as error:
        report.record("package-spec-strict", False, str(error))
        return None
    report.record("package-spec-strict", True)
    report.record("package-policy", spec["policy"] == POLICY, spec["policy"])
    report.record("package-case", spec["case_id"] == CASE_ID, spec["case_id"])
    report.record("package-producer", spec["producer"] == AUTHOR_PRODUCER,
                  spec["producer"])
    report.record("package-threshold", spec["threshold_millis"] == 2_000,
                  str(spec["threshold_millis"]))
    report.record("package-run-spec-fields", set(spec["spec"]) == set(RUN_SPEC_FIELDS),
                  str(sorted(spec["spec"])))
    report.record(
        "package-boundary",
        spec["boundaries"].get("synthetic") is True
        and spec["boundaries"].get("scientific_status") == "not_run",
        str(spec["boundaries"].get("scientific_status")),
    )
    run = spec["spec"]
    report.record("package-notes-bound", 1 <= run["notes"] <= MAX_NOTES, str(run["notes"]))
    report.record("package-edits-bound",
                  run["edits"] <= min(run["notes"], MAX_EDITS), str(run["edits"]))
    report.record("package-samples-bound",
                  1 <= run["samples"] <= MAX_SAMPLES, str(run["samples"]))
    report.record("package-warmup-bound", run["warmup"] <= MAX_WARMUP, str(run["warmup"]))

    try:
        expected = strict_load(package / "expected.json", EXPECTED_FIELDS, "先验",
                               canonical_required=False)
    except StrictError as error:
        report.record("package-expected-strict", False, str(error))
        return run
    report.record("package-expected-strict", True)
    report.record("package-expected-spec", expected["spec"] == run,
                  "先验的参数与声明不一致")
    replay_result = replay(run)
    report.record("expected-counts", expected["counts"] == replay_result["counts"],
                  f'{expected["counts"]} != {replay_result["counts"]}')
    report.record(
        "expected-corpus",
        expected["corpus_sha256"] == replay_result["corpus_sha256"],
        f'{expected["corpus_sha256"]} != {replay_result["corpus_sha256"]}',
    )
    report.record(
        "expected-baseline",
        expected["baseline_sha256"] == replay_result["baseline_sha256"],
        f'{expected["baseline_sha256"]} != {replay_result["baseline_sha256"]}',
    )
    report.record(
        "expected-first-page",
        expected["first_page_sha256"] == replay_result["first_page_sha256"],
        f'{expected["first_page_sha256"]} != {replay_result["first_page_sha256"]}',
    )
    report.record(
        "expected-changed",
        expected["changed_notes"]
        == [entry["note_id"] for entry in replay_result["changed_notes"]],
        "先验的改动清单与独立复算不一致",
    )
    report.record("expected-threshold", expected["threshold_millis"] == 2_000,
                  str(expected["threshold_millis"]))
    report.record("expected-stages", expected["stages"] == list(STAGES),
                  str(expected["stages"]))
    report.record(
        "expected-determinism",
        expected["determinism"]["comparable"] == [STRUCTURE, MANIFEST, REPORT]
        and expected["determinism"]["excluded"] == [TIMINGS],
        str(expected["determinism"]),
    )
    report.record(
        "expected-boundary",
        expected["boundaries"].get("synthetic") is True
        and expected["boundaries"].get("scientific_status") == "not_run",
        str(expected["boundaries"].get("scientific_status")),
    )
    report.record(
        "expected-coverage-honest",
        len(expected["coverage"]) == 2
        and all(item["status"] == "partially_measured" for item in expected["coverage"]),
        str(expected["coverage"]),
    )
    report.record("expected-derivation", len(expected["derivation"]) >= 5,
                  f'{len(expected["derivation"])} 条推导')
    for key in ("samples", "warmup", "process_start_included", "percentile",
                "quantile_method"):
        report.record(
            f"expected-protocol:{key}",
            expected["protocol"][key] == replay_result["protocol"][key],
            f'{expected["protocol"][key]} != {replay_result["protocol"][key]}',
        )

    try:
        cases = strict_load(package / "cases.json", CASES_FIELDS, "用例表",
                            canonical_required=False)
    except StrictError as error:
        report.record("package-cases-strict", False, str(error))
        return run
    report.record("package-cases-strict", True)
    report.record("package-cases-policy", cases["policy"] == POLICY, cases["policy"])
    report.record("package-cases-case", cases["case_id"] == CASE_ID, cases["case_id"])
    report.record("package-cases-expected", cases["expected"] == "expected.json",
                  cases["expected"])
    steps = [step["step"] for step in cases["sequence"]]
    report.record("package-sequence", len(steps) >= 2 and steps[0] == "measure",
                  str(steps))
    names = [variant["name"] for variant in cases["variants"]]
    report.record("package-variants", len(names) >= 5, str(names))
    report.record("package-variants-unique", len(names) == len(set(names)), str(names))
    for variant in cases["variants"]:
        expect = variant.get("expect") or {}
        report.record(
            f"package-variant-exit:{variant['name']}",
            expect.get("exit") in (2, 3) and bool(expect.get("code")),
            str(expect),
        )
    return run


def check_package_output(package: Path, root: Path, report: Report) -> None:
    """The real run must match the prior written before it."""
    try:
        expected = strict_load(package / "expected.json", EXPECTED_FIELDS, "先验",
                               canonical_required=False)
    except StrictError as error:
        report.record("output-expected", False, str(error))
        return
    try:
        structure = strict_load(root / STRUCTURE, STRUCTURE_FIELDS, "结构产物")
    except StrictError as error:
        report.record("output-structure", False, str(error))
        return
    report.record("output-expected-spec", structure["spec"] == expected["spec"],
                  "实测参数与先验不一致")
    report.record("output-expected-counts", structure["counts"] == expected["counts"],
                  f'{structure["counts"]} != {expected["counts"]}')
    for key in ("corpus_sha256", "baseline_sha256", "first_page_sha256"):
        report.record(
            f"output-expected:{key}",
            structure[key] == expected[key],
            f'{structure[key]} != {expected[key]}',
        )
    report.record(
        "output-expected-changed",
        [entry["note_id"] for entry in structure["changed_notes"]]
        == expected["changed_notes"],
        "实测改动清单与先验不一致",
    )
    report.record(
        "output-expected-coverage",
        [item["requirement"] for item in structure["coverage"]]
        == [item["requirement"] for item in expected["coverage"]],
        "覆盖率条目与先验不一致",
    )


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
    report.record(
        "frozen-members",
        set(members) == set(actual),
        f"多出 {sorted(set(actual) - set(members))}，缺少 {sorted(set(members) - set(actual))}",
    )
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
