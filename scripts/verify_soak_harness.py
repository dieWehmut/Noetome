#!/usr/bin/env python3
"""Read-only auditor for the soak harness.

This script never imports the Rust product and never runs the CLI to decide
anything. It re-derives the rule block from the specification, re-checks the
pause gate against the recorded per-round states, and compares that with what
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

POLICY = "soak-harness-v1"
SCHEMA_VERSION = 1
CASE_ID = "soak-synthetic-v1"
AUTHOR_PRODUCER = "fixture_author"
OUTPUT_PRODUCER = "soak_harness_output"
MAX_ROUNDS = 100_000
MAX_INTERVAL_MS = 1_000
MAX_PAUSE_AFTER_MS = 600_000
MAX_RSS_GROWTH_BYTES = 1_073_741_824
PAUSE_BOUND_MILLIS = 2_000
NFR09_REQUIRED_MILLIS = 86_400_000

STATE_SCHEDULED = "scheduled"
STATE_REFUSED = "refused"
COVERAGE_NOT_MET = "not_met"
COVERAGE_PARTIAL = "partially_measured"

UNIT_NOTE = "每轮的工作单元是对一页合成文本跑一次只读分析并核对摘要；不写文件、不联网、不调用模型。"

STRUCTURE = "structure.json"
TIMINGS = "timings.json"
REPORT = "report.md"
MANIFEST = "manifest.json"
OUTPUT_ENTRIES = (MANIFEST, REPORT, STRUCTURE, TIMINGS)
FROZEN = "frozen-sha256.json"

SPEC_FILE = "spec.json"
EXPECTED_FILE = "expected.json"
CASES_FILE = "cases.json"

DECLARATION_FIELDS = ("policy", "schema_version", "producer", "case_id", "description",
                      "spec", "paused_spec", "boundaries")
RUN_SPEC_FIELDS = ("rounds", "interval_ms", "pause_after_ms", "max_rss_growth_bytes")
STRUCTURE_FIELDS = ("policy", "schema_version", "case_id", "spec", "enforcement", "coverage",
                    "reproduce", "model_calls", "network_requests")
ENFORCEMENT_FIELDS = ("pause_bound_millis", "required_millis", "unit", "planned_floor_millis",
                      "note")
COVERAGE_FIELDS = ("requirement", "status", "detail")
TIMINGS_FIELDS = ("policy", "case_id", "rounds", "summary", "pause", "note")
ROUND_FIELDS = ("round", "state", "elapsed_millis", "work_millis", "total_millis", "rss_bytes",
                "error_code")
SUMMARY_FIELDS = ("rounds", "scheduled", "refused", "units", "errors", "scheduled_after_pause",
                  "total_millis", "max_work_millis", "median_work_millis", "first_rss_bytes",
                  "last_rss_bytes", "peak_rss_bytes", "rss_growth_bytes")
PAUSE_FIELDS = ("after_ms", "effective_round", "latency_millis", "bound_millis", "within_bound",
                "scheduled_after_pause", "note")
MANIFEST_FIELDS = ("policy", "producer", "schema_version", "case_id", "spec",
                   "structure_sha256", "structure_byte_length", "coverage")
CASES_FIELDS = ("schema_version", "producer", "case_id", "policy", "sequence", "variants",
                "expected", "boundaries")
EXPECTED_FIELDS = ("policy", "schema_version", "case_id", "enforcement", "coverage", "no_pause",
                   "paused", "rejections", "determinism", "boundaries", "derivation")
CASE_PRIOR_FIELDS = ("spec", "summary", "pause", "every_round_scheduled", "every_round_refused")


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


def planned_floor_millis(spec: dict) -> int:
    return spec["rounds"] * spec["interval_ms"]


def reproduce(spec: dict) -> str:
    pause = "none" if spec["pause_after_ms"] is None else str(spec["pause_after_ms"])
    return (
        f'noetome soak run --rounds {spec["rounds"]} --interval-ms {spec["interval_ms"]} '
        f'--pause-after-ms {pause} --max-rss-growth-bytes {spec["max_rss_growth_bytes"]} '
        "--output <输出根>"
    )


def coverage(spec: dict) -> list[dict]:
    return [
        {
            "requirement": "NFR-09",
            "status": COVERAGE_NOT_MET,
            "detail": (
                f"NFR-09 要求连续 24 小时（{NFR09_REQUIRED_MILLIS} 毫秒）基准运行。"
                "本 harness 按轮次运行，本轮计划时长下限只有 "
                f"{planned_floor_millis(spec)} 毫秒；实际跑了多久写在 timings.json，"
                "未跑满 24 小时即未达到 NFR-09，不把短时结果当长时结论。"
            ),
        },
        {
            "requirement": "NFR-13",
            "status": COVERAGE_PARTIAL,
            "detail": (
                "闸门关闭后不再调度新单元这件事由可观测计数证明："
                "每轮开始时记下距起点的毫秒数，暂停时点之后的所有轮次都必须是 refused。"
                "真正的「应用暂停命令」还没有界面，所以这里只标 partially_measured，"
                f"时限常量是 {PAUSE_BOUND_MILLIS} 毫秒。"
            ),
        },
    ]


def enforcement(spec: dict) -> dict:
    return {
        "pause_bound_millis": PAUSE_BOUND_MILLIS,
        "required_millis": NFR09_REQUIRED_MILLIS,
        "unit": UNIT_NOTE,
        "planned_floor_millis": planned_floor_millis(spec),
        "note": (
            "结构产物只记录规格与规则常量：任何一次运行的读数写进来，"
            "同一参数两次运行就不再相同。"
        ),
    }


def prior_summary(spec: dict) -> dict:
    """The two frozen cases have closed-form counts: the gate either never
    closes (every round scheduled) or closes before round one (all refused)."""
    rounds = spec["rounds"]
    if spec["pause_after_ms"] is None:
        return {
            "rounds": rounds,
            "scheduled": rounds,
            "refused": 0,
            "units": rounds,
            "errors": 0,
            "scheduled_after_pause": 0,
        }
    return {
        "rounds": rounds,
        "scheduled": 0,
        "refused": rounds,
        "units": 0,
        "errors": 0,
        "scheduled_after_pause": 0,
    }


def prior_pause(spec: dict) -> dict:
    if spec["pause_after_ms"] is None:
        return {
            "after_ms": None,
            "effective_round": None,
            "within_bound": None,
            "scheduled_after_pause": 0,
        }
    return {
        "after_ms": spec["pause_after_ms"],
        "effective_round": 1,
        "within_bound": True,
        "scheduled_after_pause": 0,
    }


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
    report.record(
        "structure-zero-calls",
        structure["model_calls"] == 0 and structure["network_requests"] == 0,
        f'{structure["model_calls"]}/{structure["network_requests"]}',
    )
    block = structure["enforcement"]
    report.record("structure-enforcement-fields", set(block) == set(ENFORCEMENT_FIELDS),
                  str(sorted(block)))
    expected_block = enforcement(spec)
    report.record("structure-enforcement", block == expected_block,
                  f"{block} != {expected_block}")
    report.record("structure-planned-floor",
                  block["planned_floor_millis"] == planned_floor_millis(spec),
                  f'{block["planned_floor_millis"]} != {planned_floor_millis(spec)}')
    report.record("structure-pause-bound", block["pause_bound_millis"] == 2_000,
                  str(block["pause_bound_millis"]))
    report.record("structure-required", block["required_millis"] == 86_400_000,
                  str(block["required_millis"]))
    report.record("structure-reproduce", structure["reproduce"] == reproduce(spec),
                  structure["reproduce"])

    expected_coverage = coverage(spec)
    report.record("structure-coverage", structure["coverage"] == expected_coverage,
                  "覆盖率条目与独立复算不一致")
    requirements = [entry["requirement"] for entry in structure["coverage"]]
    report.record("structure-coverage-set", requirements == ["NFR-09", "NFR-13"],
                  str(requirements))
    report.record(
        "structure-coverage-honest",
        [entry["status"] for entry in structure["coverage"]]
        == [COVERAGE_NOT_MET, COVERAGE_PARTIAL],
        f"短时运行不得声称 NFR-09 达标：{[entry['status'] for entry in structure['coverage']]}",
    )
    for index, entry in enumerate(structure["coverage"]):
        report.record(f"structure-coverage-fields:{index}",
                      set(entry) == set(COVERAGE_FIELDS), str(sorted(entry)))
    return structure


def check_timings(path: Path, structure: dict | None, report: Report) -> dict | None:
    try:
        timings = strict_load(path, TIMINGS_FIELDS, "计时产物")
    except StrictError as error:
        report.record("timings-strict", False, str(error))
        return None
    report.record("timings-strict", True)
    report.record("timings-policy", timings["policy"] == POLICY, timings["policy"])
    report.record("timings-case", timings["case_id"] == CASE_ID, timings["case_id"])
    summary = timings["summary"]
    pause = timings["pause"]
    rounds = timings["rounds"]
    report.record("timings-summary-fields", set(summary) == set(SUMMARY_FIELDS),
                  str(sorted(summary)))
    report.record("timings-pause-fields", set(pause) == set(PAUSE_FIELDS), str(sorted(pause)))
    for index, round in enumerate(rounds):
        if set(round) != set(ROUND_FIELDS):
            report.record(f"timings-round-fields:{index}", False, str(sorted(round)))
    report.record("timings-note", "不参与确定性比较" in timings["note"], timings["note"])

    # 逐轮计数必须自洽：scheduled + refused == rounds，units == scheduled。
    scheduled = sum(1 for round in rounds if round["state"] == STATE_SCHEDULED)
    refused = sum(1 for round in rounds if round["state"] == STATE_REFUSED)
    unknown = len(rounds) - scheduled - refused
    report.record("timings-states", unknown == 0, f"{unknown} 轮的状态不是 scheduled/refused")
    report.record("timings-summary-rounds", summary["rounds"] == len(rounds),
                  f'{summary["rounds"]} != {len(rounds)}')
    report.record("timings-summary-scheduled", summary["scheduled"] == scheduled,
                  f'{summary["scheduled"]} != {scheduled}')
    report.record("timings-summary-refused", summary["refused"] == refused,
                  f'{summary["refused"]} != {refused}')
    report.record("timings-summary-units", summary["units"] == scheduled,
                  f'{summary["units"]} != {scheduled}')
    report.record("timings-errors", summary["errors"] == 0,
                  f'{summary["errors"]} 个单元失败')
    report.record("timings-no-error-codes",
                  all(round["error_code"] is None for round in rounds),
                  "有轮次记下了错误码")

    # 闸门：关闭之后每一轮都必须是 refused，且这一事实由计数单独证明。
    effective = pause["effective_round"]
    after = pause["after_ms"]
    if effective is None:
        report.record("timings-gate-pause", True, "本次不暂停")
        report.record("timings-gate-states", all(state == STATE_SCHEDULED
                                                 for state in
                                                 (round["state"] for round in rounds)),
                      "不暂停时所有轮次都应当被调度")
    else:
        report.record("timings-gate-pause", after is not None, "闸门生效却没有暂停时点")
        after_value = after if after is not None else 0
        late = [
            round["round"]
            for round in rounds
            if round["round"] >= effective
            and (round["state"] != STATE_REFUSED or round["elapsed_millis"] < after_value)
        ]
        report.record("timings-gate-states", not late,
                      f"闸门关闭后仍有轮次被调度或起点早于暂停时点：{late}")
        early = [round["round"] for round in rounds
                 if round["round"] < effective and round["state"] != STATE_SCHEDULED]
        report.record("timings-gate-order", not early,
                      f"闸门关闭之前的轮次应当是 scheduled：{early}")
    report.record("timings-gate-count", pause["scheduled_after_pause"] == 0,
                  f'{pause["scheduled_after_pause"]} 轮在闸门关闭后仍被调度')
    report.record("timings-gate-summary-count", summary["scheduled_after_pause"] == 0,
                  f'{summary["scheduled_after_pause"]} 轮在闸门关闭后仍被调度')
    report.record("timings-gate-bound", pause["bound_millis"] == PAUSE_BOUND_MILLIS,
                  str(pause["bound_millis"]))
    if pause["latency_millis"] is not None:
        report.record(
            "timings-gate-latency",
            pause["latency_millis"] <= PAUSE_BOUND_MILLIS,
            f'{pause["latency_millis"]} 毫秒超过 {PAUSE_BOUND_MILLIS} 毫秒',
        )
        report.record(
            "timings-gate-within",
            pause["within_bound"] is True,
            str(pause["within_bound"]),
        )
    else:
        report.record("timings-gate-within", pause["within_bound"] is None,
                      str(pause["within_bound"]))

    # 读数自洽：total >= work，峰值不低于首末，增长等于末减首。
    report.record(
        "timings-rounds-monotone",
        all(round["total_millis"] >= round["work_millis"] for round in rounds),
        "有轮次的 total 小于 work",
    )
    report.record(
        "timings-rounds-ordered",
        [round["round"] for round in rounds] == list(range(1, len(rounds) + 1)),
        "轮次编号必须从 1 连续递增",
    )
    first = summary["first_rss_bytes"]
    last = summary["last_rss_bytes"]
    peak = summary["peak_rss_bytes"]
    growth = summary["rss_growth_bytes"]
    if scheduled == 0:
        # 没有任何轮次被调度时也就没有任何读数：这时只能写 null，
        # 编一个 0 出来会把「不可得」伪装成「量到了」。
        report.record(
            "timings-rss-absent",
            first is None and last is None and peak is None and growth is None,
            f"没有调度任何轮次，内存读数应当全部为 null：{first}/{last}/{peak}/{growth}",
        )
    elif first is None or last is None or peak is None or growth is None:
        report.record("timings-rss-present", False, "内存读数缺失")
    else:
        report.record("timings-rss-present", True)
        report.record("timings-rss-peak", peak >= max(first, last),
                      f"峰值 {peak} 低于首末 {first}/{last}")
        report.record("timings-rss-growth", growth == last - first,
                      f'{growth} != {last - first}')

    if structure is not None:
        report.record(
            "timings-structure-spec",
            timings["policy"] == structure["policy"],
            "计时产物与结构产物的 policy 不一致",
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
    report.record("manifest-spec-fields", set(manifest["spec"]) == set(RUN_SPEC_FIELDS),
                  str(sorted(manifest["spec"])))
    # 清单必须不带读数：它得在两次运行之间字节相同。
    report.record("manifest-no-readings",
                  "summary" not in manifest and "pause" not in manifest
                  and "timings_sha256" not in manifest,
                  "清单不得包含任何本次运行的读数")
    if structure is not None:
        report.record("manifest-structure-digest", isinstance(manifest["structure_sha256"], str)
                      and len(manifest["structure_sha256"]) == 64,
                      manifest["structure_sha256"])
        report.record("manifest-coverage",
                      [entry["requirement"] for entry in manifest["coverage"]]
                      == [entry["requirement"] for entry in structure["coverage"]],
                      "清单的覆盖率条目与结构产物不一致")
        report.record("manifest-spec", manifest["spec"] == structure["spec"],
                      "清单的参数与结构产物不一致")
    return manifest


def check_report(path: Path, structure: dict | None, report: Report) -> None:
    if not path.is_file():
        report.record("report-present", False, f"缺少 {REPORT}")
        return
    report.record("report-present", True)
    text = path.read_text(encoding="utf-8")
    for needle in ("NFR-09", "not_met", "NFR-13", "partially_measured",
                   "## 覆盖范围与未达标项", "## 边界", "2000", "86400000"):
        report.record(f"report-needle:{needle}", needle in text, "报告缺少这条说明")
    report.record("report-synthetic", "合成" in text and "不代表真实项目负载" in text,
                  "报告必须写明这是合成负载")

    # 报告里允许出现的毫秒数只有规则常量与参数：任何本次运行的读数写进来，
    # 同一参数两次运行就不再相同。
    allowed = {PAUSE_BOUND_MILLIS, NFR09_REQUIRED_MILLIS}
    if structure is not None:
        spec = structure["spec"]
        allowed.add(spec["interval_ms"])
        # 计划时长下限是「轮次 × 轮间隔」这个规则量，由参数推导而不是读数；
        # 它出现在覆盖率说明里，所以必须算作允许值，否则大轮次样例会误报。
        allowed.add(planned_floor_millis(spec))
        if spec["pause_after_ms"] is not None:
            allowed.add(spec["pause_after_ms"])
    numbers: list[int] = []
    cursor = 0
    while True:
        found = text.find("毫秒", cursor)
        if found < 0:
            break
        position = found
        # 产品写成「2000 毫秒」（数字与单位之间有空格），只从紧邻字符往前取数字
        # 会让这条检查永远取不到任何数——那样它就是个空转的检查。
        while position > 0 and text[position - 1].isspace():
            position -= 1
        digits = ""
        while position > 0 and text[position - 1].isdigit():
            position -= 1
            digits = text[position] + digits
        if digits:
            numbers.append(int(digits))
        cursor = found + 2
    leaked = sorted({value for value in numbers if value not in allowed})
    report.record("report-no-readings", not leaked,
                  f"报告里出现了疑似读数：{leaked}")
    # 不另设「报告不含计时读数」的检查：读数的值可能与参数或常量恰好相同
    # （暂停样例里每轮 work 都是 0，轮间隔也是 0），按值判断只会误报。
    # 真正的保证是同一参数两次运行的 report.md 字节相同，那条由 Rust
    # 集成测试 `two_runs_with_the_same_parameters_are_byte_identical…` 负责。
    report.record("report-reproduce",
                  structure is not None and structure["reproduce"] in text,
                  "报告必须写出复现命令")


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
    check_report(root / REPORT, structure, report)
    if manifest is not None and (root / STRUCTURE).is_file():
        digest = sha256(read_bytes(root / STRUCTURE))
        report.record("manifest-structure-digest-value",
                      manifest["structure_sha256"] == digest, "结构产物摘要不符")
        report.record(
            "manifest-structure-length",
            manifest["structure_byte_length"] == (root / STRUCTURE).stat().st_size,
            "结构产物字节数不符",
        )
    report.record(
        "output-no-extra-entries",
        sorted(item.name for item in root.iterdir()) == sorted(OUTPUT_ENTRIES),
        "输出根只有四个文件",
    )


def check_package(package: Path, report: Report) -> dict | None:
    if not (package / SPEC_FILE).is_file():
        report.record("package-present", False, f"{package} 缺少 {SPEC_FILE}")
        return None
    report.record("package-present", True)
    try:
        declaration = strict_load(package / SPEC_FILE, DECLARATION_FIELDS, "soak 声明",
                                  canonical_required=False)
    except StrictError as error:
        report.record("package-declaration-strict", False, str(error))
        return None
    report.record("package-declaration-strict", True)
    report.record("package-policy", declaration["policy"] == POLICY, declaration["policy"])
    report.record("package-case", declaration["case_id"] == CASE_ID, declaration["case_id"])
    report.record("package-producer", declaration["producer"] == AUTHOR_PRODUCER,
                  declaration["producer"])
    report.record(
        "package-boundary",
        declaration["boundaries"].get("synthetic") is True
        and declaration["boundaries"].get("scientific_status") == "not_run",
        str(declaration["boundaries"].get("scientific_status")),
    )
    specs = {"spec": declaration["spec"], "paused_spec": declaration["paused_spec"]}
    for name, spec in specs.items():
        report.record(f"package-spec-fields:{name}", set(spec) == set(RUN_SPEC_FIELDS),
                      str(sorted(spec)))
        report.record(f"package-rounds-bound:{name}",
                      1 <= spec["rounds"] <= MAX_ROUNDS, str(spec["rounds"]))
        report.record(f"package-interval-bound:{name}",
                      spec["interval_ms"] <= MAX_INTERVAL_MS, str(spec["interval_ms"]))
        report.record(
            f"package-pause-bound:{name}",
            spec["pause_after_ms"] is None
            or spec["pause_after_ms"] <= MAX_PAUSE_AFTER_MS,
            str(spec["pause_after_ms"]),
        )
        report.record(
            f"package-growth-bound:{name}",
            spec["max_rss_growth_bytes"] <= MAX_RSS_GROWTH_BYTES,
            str(spec["max_rss_growth_bytes"]),
        )

    try:
        expected = strict_load(package / EXPECTED_FILE, EXPECTED_FIELDS, "先验",
                               canonical_required=False)
    except StrictError as error:
        report.record("package-expected-strict", False, str(error))
        return declaration
    report.record("package-expected-strict", True)
    # 先验只承载判据，不复制产品的措辞：规则块比四个判据字段，
    # 覆盖率只比 (requirement, status)，说明文字由审计器自己推导。
    prior_block = expected["enforcement"]
    derived_block = enforcement(specs["spec"])
    judged_fields = ("pause_bound_millis", "required_millis", "planned_floor_millis", "unit")
    report.record(
        "expected-enforcement",
        all(prior_block.get(key) == derived_block[key] for key in judged_fields),
        f"{ {key: prior_block.get(key) for key in judged_fields} } != "
        f"{ {key: derived_block[key] for key in judged_fields} }",
    )
    prior_coverage = [
        (entry.get("requirement"), entry.get("status")) for entry in expected["coverage"]
    ]
    derived_coverage = [
        (entry["requirement"], entry["status"]) for entry in coverage(specs["spec"])
    ]
    report.record("expected-coverage", prior_coverage == derived_coverage,
                  f"{prior_coverage} != {derived_coverage}")
    for name in specs:
        case = expected["no_pause"] if name == "spec" else expected["paused"]
        label = "no_pause" if name == "spec" else "paused"
        report.record(f"expected-{label}-fields",
                      set(case) <= set(CASE_PRIOR_FIELDS), str(sorted(case)))
        report.record(f"expected-{label}-spec", case["spec"] == specs[name],
                      "先验的参数与声明不一致")
        report.record(f"expected-{label}-summary",
                      case["summary"] == prior_summary(specs[name]),
                      f'{case["summary"]} != {prior_summary(specs[name])}')
        prior_pause_block = prior_pause(specs[name])
        recorded = {key: case["pause"][key] for key in prior_pause_block}
        report.record(f"expected-{label}-pause", recorded == prior_pause_block,
                      f"{recorded} != {prior_pause_block}")
    report.record(
        "expected-determinism",
        expected["determinism"].get("comparable") == [STRUCTURE, MANIFEST, REPORT]
        and expected["determinism"].get("excluded") == [TIMINGS],
        str(expected["determinism"]),
    )
    report.record(
        "expected-boundary",
        expected["boundaries"].get("synthetic") is True
        and expected["boundaries"].get("scientific_status") == "not_run",
        str(expected["boundaries"].get("scientific_status")),
    )
    report.record("expected-derivation", len(expected["derivation"]) >= 6,
                  f'{len(expected["derivation"])} 条推导')
    rejections = expected["rejections"]
    report.record("expected-rejections", len(rejections) == 5, f"{len(rejections)} 条拒绝")
    scores = {(entry["code"], entry["exit"]) for entry in rejections}
    report.record(
        "expected-rejection-codes",
        scores == {("SOAK_ARGUMENT", 2), ("CLI_INVALID_ARGUMENT", 2)},
        str(sorted(scores)),
    )

    try:
        cases = strict_load(package / CASES_FILE, CASES_FIELDS, "执行序列",
                            canonical_required=False)
    except StrictError as error:
        report.record("package-cases-strict", False, str(error))
        return declaration
    report.record("package-cases-strict", True)
    report.record("cases-policy", cases["policy"] == POLICY, cases["policy"])
    report.record("cases-case", cases["case_id"] == CASE_ID, cases["case_id"])
    report.record("cases-expected", cases["expected"] == EXPECTED_FILE, cases["expected"])
    report.record("cases-sequence", len(cases["sequence"]) >= 3, str(len(cases["sequence"])))
    report.record("cases-variants",
                  [entry["name"] for entry in cases["variants"]]
                  == [entry["name"] for entry in rejections],
                  "执行序列的变体与先验的拒绝清单不一致")
    return declaration


def check_package_output(package: Path, root: Path, report: Report) -> None:
    """当输出根由 package 里声明的某个 spec 跑出来时，核对两者的对应关系。"""
    try:
        declaration = strict_load(package / SPEC_FILE, DECLARATION_FIELDS, "soak 声明",
                                  canonical_required=False)
    except StrictError:
        return
    structure_path = root / STRUCTURE
    if not structure_path.is_file():
        return
    try:
        structure = json.loads(structure_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return
    spec = structure.get("spec")
    if not isinstance(spec, dict):
        return
    matched = [name for name in ("spec", "paused_spec") if declaration[name] == spec]
    if not matched:
        # 这个输出根不是用包里的声明跑出来的（比如中途暂停的样例）。根自身的
        # 检查已经全部跑过，这里只是没有对照对象，不该当成失败。
        report.record("output-expected-spec", True,
                      f"输出根的参数 {spec} 不在冻结声明里，跳过与先验的对应核对")
        return
    report.record("output-expected-spec", True, matched[0])
    name = matched[0]
    report.record("output-expected-enforcement",
                  structure["enforcement"] == enforcement(spec),
                  "输出根的规则块与独立复算不一致")
    report.record("output-expected-coverage", structure["coverage"] == coverage(spec),
                  "输出根的覆盖率条目与独立复算不一致")
    report.record("output-expected-reproduce", structure["reproduce"] == reproduce(spec),
                  "输出根的复现命令与独立复算不一致")
    timings_path = root / TIMINGS
    if timings_path.is_file():
        try:
            timings = json.loads(timings_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return
        prior = prior_summary(spec)
        summary = timings.get("summary", {})
        agree = all(summary.get(key) == value for key, value in prior.items())
        report.record("output-expected-summary", agree,
                      f'{ {key: summary.get(key) for key in prior} } != {prior}')


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
    for path in sorted(package.rglob("*"), key=lambda path: path.relative_to(package).parts):
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
