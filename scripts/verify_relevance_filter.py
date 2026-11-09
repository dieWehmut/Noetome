#!/usr/bin/env python3
"""Independent read-only audit for the relevance-filter contract (B10-T02).

This script never imports the Rust product and never runs the CLI to decide
anything. It re-reads a frozen author package and a filter root/output on disk,
replays segmentation and the keep/exclude decision from the frozen bytes, and
compares the replay with the JSON the product wrote.

Usage:
    python3 scripts/verify_relevance_filter.py <root> [--output DIR]
        [--package DIR] [--json] [--freeze]

Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

POLICY = "relevance-filter-v1"
AUDIENCE_POLICY = "relevance-audience-v1"
REVIEW_POLICY = "relevance-review-v1"
ATOMS_POLICY = "relevance-atoms-v1"
AUTHOR_PRODUCER = "fixture_author"
APP_PRODUCER = "noetome-relevance"
OUTPUT_PRODUCER = "relevance_filter_output"
SCHEMA_VERSION = 1

LEVELS = ("introductory", "university", "advanced")

STATUS_KEPT = "kept"
STATUS_EXCLUDED = "excluded"
STATUS_DEFERRED = "deferred"

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
TEXT_DIR = "control/text"
AUTHOR = "control/author.json"
AUDIENCE = "control/audience.json"
ATOMS = "control/atoms.json"
REVIEW = "control/review.json"
TEXT_SUFFIX = ".md"

FILTER = "filter.json"
DENSITIES = "densities.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"

FROZEN = "frozen-sha256.json"

SPAN_HEADING = "heading"
SPAN_TEXT = "text"
SPAN_CODE = "code"
SPAN_FORMATTING = "formatting"


class Report:
    def __init__(self) -> None:
        self.checks: list[dict[str, object]] = []
        self.failures: list[str] = []
        self.warnings: list[str] = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append({"check": name, "ok": bool(ok), "detail": detail})
        if not ok:
            self.failures.append(f"{name}: {detail}" if detail else name)

    def warn(self, name: str, detail: str) -> None:
        self.checks.append({"check": name, "ok": True, "detail": detail, "warning": True})
        self.warnings.append(f"{name}: {detail}")

    def dump(self, as_json: bool) -> None:
        if as_json:
            payload = {
                "checks": len(self.checks),
                "failures": len(self.failures),
                "warnings": len(self.warnings),
                "failure_detail": self.failures,
                "warning_detail": self.warnings,
                "items": self.checks,
            }
            sys.stdout.write(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
            return
        for item in self.checks:
            if not item["ok"]:
                sys.stdout.write(f"失败  {item['check']}：{item['detail']}\n")
            elif item.get("warning"):
                sys.stdout.write(f"提示  {item['check']}：{item['detail']}\n")
        sys.stdout.write(
            f"合计 {len(self.checks)} 项检查：失败 {len(self.failures)}、"
            f"提示 {len(self.warnings)}\n"
        )


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    with open(path, "rb") as handle:
        return handle.read()


def canonical(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(
    payload: bytes, fields: tuple[str, ...], label: str, canonical_required: bool = True
):
    text = payload.decode("utf-8")
    value = json.loads(text)
    if not isinstance(value, dict):
        raise ValueError(f"{label} 顶层不是对象")
    if tuple(sorted(value.keys())) != tuple(sorted(fields)):
        raise ValueError(
            f"{label} 字段集合不符：多出 {sorted(set(value) - set(fields))}、"
            f"缺少 {sorted(set(fields) - set(value))}"
        )
    if canonical_required and canonical(value) != text.rstrip("\n"):
        raise ValueError(f"{label} 不是规范 JSON（重复键、非规范空白或未知字段）")
    if canonical_required and text[len(text.rstrip("\n")) :] not in ("", "\n"):
        raise ValueError(f"{label} 结尾换行过多")
    return value


# --------------------------------------------------------------------------
# Independent replay of segmentation (the shape of `text::analyze`).
# --------------------------------------------------------------------------


def marker_content(line: str) -> str | None:
    indentation = len(line) - len(line.lstrip(" "))
    if indentation > 3:
        return None
    return line[indentation:]


def is_heading(line: str) -> bool:
    content = marker_content(line)
    if content is None:
        return False
    hashes = 0
    for char in content:
        if char != "#":
            break
        hashes += 1
    if not 1 <= hashes <= 6:
        return False
    rest = content[hashes:]
    return rest == "" or rest[0].isspace()


def opening_fence(line: str) -> tuple[str, int] | None:
    content = marker_content(line)
    if not content:
        return None
    marker = content[0]
    if marker not in ("`", "~"):
        return None
    length = 0
    for char in content:
        if char != marker:
            break
        length += 1
    if length < 3:
        return None
    return marker, length


def closing_fence(line: str, fence: tuple[str, int]) -> bool:
    content = marker_content(line)
    if content is None:
        return False
    marker, needed = fence
    length = 0
    for char in content:
        if char != marker:
            break
        length += 1
    if length < needed:
        return False
    return all(char in (" ", "\t") for char in content[length:])


def normalize(raw: bytes) -> str:
    text = raw.decode("utf-8")
    if text.startswith("\ufeff"):
        text = text[1:]
    return text.replace("\r\n", "\n").replace("\r", "\n")


def segment(text: str) -> list[dict]:
    """Mirror `text::segment`.

    The Rust source splits with `str::split_inclusive('\n')`, so a line's
    `end` offset includes its own newline. Reproducing that matters: a span's
    `text_sha256` covers the newline, and end_byte is exclusive.
    """
    pieces = text.split("\n")
    if text.endswith("\n"):
        # `split_inclusive` yields no trailing empty piece for a final newline.
        pieces.pop()
    lines = []
    offset = 0
    for index, piece in enumerate(pieces):
        start = offset
        offset += len(piece.encode("utf-8"))
        if index < len(pieces) - 1 or text.endswith("\n"):
            offset += 1  # the newline kept by split_inclusive
        lines.append(
            {
                "content": piece,
                "start": start,
                "end": offset,
                "number": index + 1,
            }
        )

    spans = []
    index = 0
    while index < len(lines):
        start = index
        if opening_fence(lines[index]["content"]) is not None:
            fence = opening_fence(lines[index]["content"])
            index += 1
            closed = False
            while index < len(lines):
                closed = closing_fence(lines[index]["content"], fence)
                index += 1
                if closed:
                    break
            kind = SPAN_CODE
            status = "parsed" if closed else "needs_review"
        elif is_heading(lines[index]["content"]):
            index += 1
            kind = SPAN_HEADING
            status = "parsed"
        elif lines[index]["content"].strip() == "":
            index += 1
            while index < len(lines) and lines[index]["content"].strip() == "":
                index += 1
            kind = SPAN_FORMATTING
            status = "parsed"
        else:
            index += 1
            while (
                index < len(lines)
                and lines[index]["content"].strip() != ""
                and not is_heading(lines[index]["content"])
                and opening_fence(lines[index]["content"]) is None
            ):
                index += 1
            kind = SPAN_TEXT
            status = "parsed"
        first = lines[start]
        last = lines[index - 1]
        payload = text.encode("utf-8")[first["start"] : last["end"]]
        spans.append(
            {
                "kind": kind,
                "start_byte": first["start"],
                "end_byte": last["end"],
                "start_line": first["number"],
                "end_line": last["number"],
                "text_sha256": sha256(payload),
                "parse_status": status,
            }
        )
    return spans


def text_span_covering(spans: list[dict], line: int) -> dict | None:
    for span in spans:
        if span["start_line"] <= line <= span["end_line"]:
            return span
    return None


# --------------------------------------------------------------------------
# Independent replay of the keep/exclude decision.
# --------------------------------------------------------------------------


def level_index(level: str) -> int:
    try:
        return LEVELS.index(level)
    except ValueError:
        raise ValueError(
            f"用户水平 {level} 不在允许集合内（{'、'.join(LEVELS)}）。"
        ) from None


def replay_package(package: Path) -> dict:
    author = json.loads(read_bytes(package / "author.json").decode("utf-8"))
    audience = json.loads(
        read_bytes(package / author["audience"]).decode("utf-8")
    )
    atoms = json.loads(read_bytes(package / author["atoms"]).decode("utf-8"))
    review = json.loads(read_bytes(package / author["review"]).decode("utf-8"))

    documents = []
    bodies: dict[str, str] = {}
    for spec in author["sources"]:
        raw = read_bytes(package / spec["path"])
        text = normalize(raw)
        bodies[spec["source_id"]] = text
        spans = segment(text)
        documents.append(
            {
                "source_id": spec["source_id"],
                "title": spec["title"],
                "path": spec["path"],
                "raw_sha256": sha256(raw),
                "byte_length": len(raw),
                "normalized_sha256": sha256(text.encode("utf-8")),
                "spans": spans,
            }
        )

    level = level_index(audience["level"])
    registered = {item["reason"] for item in review["reasons"]}

    exclusions: dict[tuple[str, int], dict] = {}
    for item in review["exclusions"]:
        if item["reason"] not in registered:
            raise ValueError(
                f"排除条目 {item['source_id']} 第 {item['line']} 行用了未登记的排除理由 "
                f"{item['reason']}"
            )
        record = next(
            (doc for doc in documents if doc["source_id"] == item["source_id"]), None
        )
        if record is None:
            raise ValueError(f"排除条目指向未声明的来源 {item['source_id']}")
        span = text_span_covering(record["spans"], item["line"])
        if span is None:
            raise ValueError(f"排除条目 {item['source_id']} 第 {item['line']} 行落不到任何片段")
        if span["kind"] != SPAN_TEXT:
            raise ValueError(
                f"排除条目 {item['source_id']} 第 {item['line']} 行落在 {span['kind']} 片段上"
            )
        key = (item["source_id"], item["line"])
        if key in exclusions:
            raise ValueError(f"来源 {item['source_id']} 第 {item['line']} 行被重复排除")
        exclusions[key] = {"spec": item, "span_id": span["kind"]}

    # Assign span ids the way the product does: source order, 1-based index.
    for record in documents:
        for index, span in enumerate(record["spans"], start=1):
            span["span_id"] = f"{record['source_id']}-{index:05d}"

    excluded_by_span: dict[str, dict] = {}
    for key, item in exclusions.items():
        record = next(doc for doc in documents if doc["source_id"] == key[0])
        span = text_span_covering(record["spans"], key[1])
        excluded_by_span[span["span_id"]] = item["spec"]

    spans_out = []
    reason_counts: dict[str, int] = {}
    kept_spans = 0
    total_text = 0
    total_spans = 0
    for record in documents:
        for span in record["spans"]:
            total_spans += 1
            is_text = span["kind"] == SPAN_TEXT
            if is_text:
                total_text += 1
            hit = excluded_by_span.get(span["span_id"]) if is_text else None
            if hit is not None:
                reason_counts[hit["reason"]] = reason_counts.get(hit["reason"], 0) + 1
                spans_out.append(
                    {
                        "span_id": span["span_id"],
                        "source_id": record["source_id"],
                        "kind": span["kind"],
                        "start_line": span["start_line"],
                        "end_line": span["end_line"],
                        "status": STATUS_EXCLUDED,
                        "reason": hit["reason"],
                        "detail": hit["detail"],
                    }
                )
            else:
                if is_text:
                    kept_spans += 1
                spans_out.append(
                    {
                        "span_id": span["span_id"],
                        "source_id": record["source_id"],
                        "kind": span["kind"],
                        "start_line": span["start_line"],
                        "end_line": span["end_line"],
                        "status": STATUS_KEPT,
                        "reason": None,
                        "detail": None,
                    }
                )

    decisions = []
    for atom in atoms["atoms"]:
        record = next(
            (doc for doc in documents if doc["source_id"] == atom["source_id"]), None
        )
        if record is None:
            raise ValueError(f"原子 {atom['atom_id']} 声明的来源不在作者包里")
        span = text_span_covering(record["spans"], atom["line"])
        if span is None:
            raise ValueError(f"原子 {atom['atom_id']} 的行号落不到任何片段")
        if span["kind"] != SPAN_TEXT:
            raise ValueError(f"原子 {atom['atom_id']} 落在 {span['kind']} 片段上")
        required = any(level_index(candidate) <= level for candidate in atom["levels"])
        status = STATUS_DEFERRED if span["span_id"] in excluded_by_span else STATUS_KEPT
        decisions.append(
            {
                "atom_id": atom["atom_id"],
                "source_id": atom["source_id"],
                "line": atom["line"],
                "kind": atom["kind"],
                "statement": atom["statement"],
                "conditions": atom["conditions"],
                "requires": atom["requires"],
                "covers": atom["covers"],
                "levels": atom["levels"],
                "required": required,
                "status": status,
            }
        )

    blocked = [
        atom["atom_id"] for atom in decisions if atom["required"] and atom["status"] != STATUS_KEPT
    ]
    if blocked:
        raise ValueError(f"用户水平 {audience['level']} 需要的原子被排除：{blocked}")

    provided = {
        cover
        for atom in decisions
        if atom["status"] == STATUS_KEPT
        for cover in atom["covers"]
    }
    declared = set(audience["declared_knowledge"])
    missing = [
        f"{atom['atom_id']} 需要 {need}"
        for atom in decisions
        if atom["status"] == STATUS_KEPT
        for need in atom["requires"]
        if need not in provided and need not in declared
    ]
    if missing:
        raise ValueError(f"必要前提缺失：{missing}")

    kept_atoms = [atom for atom in decisions if atom["status"] == STATUS_KEPT]
    conditions = sum(len(atom["conditions"]) for atom in kept_atoms)
    counts = {
        "sources": len(documents),
        "spans": total_spans,
        "text_spans": total_text,
        "kept_spans": kept_spans,
        "excluded_spans": total_text - kept_spans,
        "atoms": len(decisions),
        "kept_atoms": len(kept_atoms),
        "deferred_atoms": len(decisions) - len(kept_atoms),
        "conditions": conditions,
    }

    return {
        "author": author,
        "audience": audience,
        "atoms": atoms,
        "review": review,
        "documents": documents,
        "bodies": bodies,
        "counts": counts,
        "reason_counts": dict(sorted(reason_counts.items())),
        "spans": spans_out,
        "decisions": decisions,
        "kept_atoms": kept_atoms,
        "excluded_specs": [item["spec"] for item in exclusions.values()],
    }


# --------------------------------------------------------------------------
# Checks.
# --------------------------------------------------------------------------


def check_layout(report: Report, root: Path) -> None:
    if not root.is_dir():
        report.record("layout-root", False, f"{root} 不是目录")
        return
    report.record("layout-root", True)
    entries = sorted(item.name for item in root.iterdir())
    report.record(
        "layout-root-entries", entries == [CONTROL], f"顶层只允许 control/，实际 {entries}"
    )
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control/")
        return
    control_entries = sorted(item.name for item in control.iterdir())
    expected = sorted(
        ["manifest.json", "inputs.json", "text", "author.json", "audience.json", "atoms.json", "review.json"]
    )
    report.record(
        "layout-control-entries",
        control_entries == expected,
        f"control/ 只允许 {expected}，实际 {control_entries}",
    )
    text_dir = control / "text"
    if not text_dir.is_dir():
        report.record("layout-text", False, "缺少 control/text/")
        return
    report.record("layout-text", True)
    for path in [root, control, text_dir, *control.iterdir(), *text_dir.iterdir()]:
        if path.is_symlink():
            report.record("layout-no-links", False, f"{path} 是符号链接")
            return
    report.record("layout-no-links", True)


def check_manifest(report: Report, root: Path, package: Path) -> dict | None:
    path = root / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 control/manifest.json")
        return None
    report.record("manifest-present", True)
    fields = (
        "policy",
        "producer",
        "schema_version",
        "case_id",
        "root",
        "root_identity",
        "parent_identity",
        "control_identity",
        "manifest_identity",
        "inputs_identity",
        "author_sha256",
        "audience_sha256",
        "atoms_sha256",
        "review_sha256",
        "inputs_sha256",
        "documents",
        "created_at",
    )
    try:
        manifest = strict_load(read_bytes(path), fields, "control/manifest.json")
    except (ValueError, json.JSONDecodeError) as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, manifest["policy"])
    report.record(
        "manifest-producer", manifest["producer"] == APP_PRODUCER, manifest["producer"]
    )
    report.record(
        "manifest-schema-version",
        manifest["schema_version"] == SCHEMA_VERSION,
        str(manifest["schema_version"]),
    )
    report.record(
        "manifest-root",
        manifest["root"] == os.path.realpath(root),
        f"{manifest['root']} != {os.path.realpath(root)}",
    )
    for name, relative in (
        ("author_sha256", "author.json"),
        ("audience_sha256", "audience.json"),
        ("atoms_sha256", "atoms.json"),
        ("review_sha256", "review.json"),
    ):
        report.record(
            f"manifest-{name}",
            manifest[name] == sha256(read_bytes(package / relative)),
            f"{relative} 摘要不符",
        )
    inputs_bytes = read_bytes(root / INPUTS)
    report.record(
        "manifest-inputs-sha256",
        manifest["inputs_sha256"] == sha256(inputs_bytes),
        "inputs.json 摘要不符",
    )
    digests = {item["source_id"]: item for item in manifest["documents"]}
    for document in manifest["documents"]:
        frozen = root / TEXT_DIR / f"{document['source_id']}{TEXT_SUFFIX}"
        if not frozen.is_file():
            report.record("manifest-text-present", False, f"缺少 {frozen.name}")
            continue
        data = read_bytes(frozen)
        report.record(
            f"manifest-text-{document['source_id']}",
            document["normalized_sha256"] == sha256(data)
            and document["byte_length"] == len(data),
            "冻结文本摘要或长度不符",
        )
    text_dir = root / TEXT_DIR
    if not text_dir.is_dir():
        report.record("manifest-text-set", False, "缺少 control/text/")
        return manifest
    text_entries = sorted(item.name for item in text_dir.iterdir())
    expected = sorted(f"{source_id}{TEXT_SUFFIX}" for source_id in digests)
    report.record(
        "manifest-text-set",
        text_entries == expected,
        f"text/ 内容不符：{text_entries} != {expected}",
    )
    return manifest


def check_inputs(report: Report, root: Path, manifest: dict | None) -> dict | None:
    path = root / INPUTS
    if not path.is_file():
        report.record("inputs-present", False, "缺少 control/inputs.json")
        return None
    report.record("inputs-present", True)
    try:
        inputs = strict_load(
            read_bytes(path),
            ("policy", "schema_version", "case_id", "documents"),
            "control/inputs.json",
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("inputs-strict", False, str(error))
        return None
    report.record("inputs-strict", True)
    report.record("inputs-policy", inputs["policy"] == POLICY, inputs["policy"])
    report.record(
        "inputs-schema-version",
        inputs["schema_version"] == SCHEMA_VERSION,
        str(inputs["schema_version"]),
    )
    if manifest is not None:
        report.record(
            "inputs-case-id",
            inputs["case_id"] == manifest["case_id"],
            f"{inputs['case_id']} != {manifest['case_id']}",
        )
        report.record(
            "inputs-document-count",
            len(inputs["documents"]) == len(manifest["documents"]),
            "文档数不一致",
        )
    for document in inputs["documents"]:
        frozen = root / TEXT_DIR / f"{document['source_id']}{TEXT_SUFFIX}"
        if not frozen.is_file():
            report.record(f"inputs-spans-{document['source_id']}", False, "缺少冻结文本")
            continue
        text = read_bytes(frozen).decode("utf-8")
        replayed = segment(text)
        declared = document["spans"]
        report.record(
            f"inputs-span-count-{document['source_id']}",
            len(declared) == len(replayed),
            f"Span 数 {len(declared)} 与独立分段 {len(replayed)} 不一致",
        )
        drift = []
        for index, span in enumerate(declared):
            mirror = replayed[index] if index < len(replayed) else None
            if mirror is None:
                drift.append(f"#{index}")
                continue
            for key in ("kind", "start_byte", "end_byte", "start_line", "end_line", "text_sha256"):
                if span[key] != mirror[key]:
                    drift.append(f"#{index}.{key}: {span[key]} != {mirror[key]}")
        report.record(
            f"inputs-spans-{document['source_id']}",
            not drift,
            f"Span 与独立分段不一致：{drift[:6]}",
        )
        covered = 0
        contiguous = True
        raw = read_bytes(frozen)
        for span in declared:
            if span["start_byte"] != covered:
                contiguous = False
            if not (span["start_byte"] <= span["end_byte"] <= len(raw)):
                contiguous = False
                continue
            if sha256(raw[span["start_byte"] : span["end_byte"]]) != span["text_sha256"]:
                contiguous = False
            covered = span["end_byte"]
        report.record(
            f"inputs-coverage-{document['source_id']}",
            contiguous and covered == len(raw),
            "Span 未连续覆盖冻结文本或片段摘要不符",
        )
    return inputs


def check_expected(report: Report, package: Path) -> None:
    path = package / "expected.json"
    if not path.is_file():
        report.record("expected-present", False, "缺少 expected.json")
        return
    expected = json.loads(read_bytes(path).decode("utf-8"))
    try:
        replayed = replay_package(package)
    except ValueError as error:
        report.record("expected-replay", False, str(error))
        return
    report.record("expected-replay", True)
    for key, value in expected["counts"].items():
        report.record(
            f"expected-counts-{key}",
            replayed["counts"][key] == value,
            f"expected {key}={value}，重放 {replayed['counts'][key]}",
        )
    report.record(
        "expected-reason-counts",
        expected["reason_counts"] == replayed["reason_counts"],
        f"expected {expected['reason_counts']} != 重放 {replayed['reason_counts']}",
    )
    declared = sorted(
        (item["source_id"], item["line"], item["reason"]) for item in expected["exclusions"]
    )
    actual = sorted(
        (item["source_id"], item["line"], item["reason"])
        for item in replayed["excluded_specs"]
    )
    report.record(
        "expected-exclusions",
        declared == actual,
        f"expected.json 的排除清单与独立重放不一致：{declared} != {actual}",
    )
    atoms = [
        {
            "atom_id": atom["atom_id"],
            "source_id": atom["source_id"],
            "line": atom["line"],
            "conditions": len(atom["conditions"]),
        }
        for atom in replayed["kept_atoms"]
    ]
    report.record(
        "expected-atoms",
        expected["atoms"] == atoms,
        f"expected.json 的保留原子与独立重放不一致：{expected['atoms']} != {atoms}",
    )
    spans_by_source = {
        record["source_id"]: len(record["spans"]) for record in replayed["documents"]
    }
    report.record(
        "expected-spans-by-source",
        expected["spans_by_source"] == spans_by_source,
        f"expected.json 的每源片段数与独立分段不一致："
        f"{expected['spans_by_source']} != {spans_by_source}",
    )
    text_by_source = {
        record["source_id"]: sum(
            1 for span in record["spans"] if span["kind"] == SPAN_TEXT
        )
        for record in replayed["documents"]
    }
    report.record(
        "expected-text-spans-by-source",
        expected["text_spans_by_source"] == text_by_source,
        f"expected.json 的每源正文片段数与独立分段不一致："
        f"{expected['text_spans_by_source']} != {text_by_source}",
    )
    densities = {
        "minimal": {
            "atoms": replayed["counts"]["kept_atoms"],
            "conditions": replayed["counts"]["conditions"],
        },
        "detailed": {
            "atoms": replayed["counts"]["kept_atoms"],
            "conditions": replayed["counts"]["conditions"],
        },
    }
    report.record(
        "expected-densities",
        expected["densities"] == densities,
        f"expected.json 的两密度计数与独立重放不一致：{expected['densities']} != {densities}",
    )
    report.record(
        "expected-rejections",
        [item["code"] for item in expected["rejections"]]
        == [
            "RELEVANCE_PREREQUISITE_MISSING",
            "RELEVANCE_REQUIRED_EXCLUDED",
            "RELEVANCE_ARGUMENT",
            "RELEVANCE_ARGUMENT",
        ],
        f"expected.json 的拒绝清单不符：{[item['code'] for item in expected['rejections']]}",
    )
    policies = replayed["author"]
    report.record(
        "expected-author-policy",
        policies["policy"] == POLICY
        and policies["producer"] == AUTHOR_PRODUCER
        and policies["schema_version"] == SCHEMA_VERSION,
        "作者包的 policy/producer/schema_version 不符",
    )
    for name, policy in (
        ("audience", AUDIENCE_POLICY),
        ("atoms", ATOMS_POLICY),
        ("review", REVIEW_POLICY),
    ):
        report.record(
            f"expected-{name}-policy",
            replayed[name]["policy"] == policy,
            f"{name}.policy = {replayed[name]['policy']}",
        )
    cases = json.loads(read_bytes(package / "cases.json").decode("utf-8"))
    report.record(
        "expected-case-id",
        cases["case_id"] == replayed["author"]["case_id"],
        f"{cases['case_id']} != {replayed['author']['case_id']}",
    )
    report.record(
        "expected-zero-calls",
        expected["boundaries"]["uses_model"] is False
        and expected["boundaries"]["uses_network"] is False
        and expected["boundaries"]["scientific_status"] == "not_run",
        "expected.json 未声明零模型、零网络与 not_run",
    )


def check_frozen(report: Report, package: Path) -> None:
    path = package / FROZEN
    if not path.is_file():
        return
    try:
        frozen = json.loads(read_bytes(path).decode("utf-8"))
    except json.JSONDecodeError as error:
        report.record("frozen-parse", False, str(error))
        return
    report.record("frozen-parse", True)
    declared = frozen["files"]
    actual = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file():
            continue
        relative = item.relative_to(package).as_posix()
        if relative == FROZEN:
            continue
        data = read_bytes(item)
        actual[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    report.record(
        "frozen-set",
        sorted(declared) == sorted(actual),
        f"清单成员不一致：多 {sorted(set(actual) - set(declared))}、"
        f"少 {sorted(set(declared) - set(actual))}",
    )
    drift = [
        name
        for name, digest in actual.items()
        if name in declared
        and (
            declared[name]["sha256"] != digest["sha256"]
            or declared[name]["byte_length"] != digest["byte_length"]
        )
    ]
    report.record("frozen-digests", not drift, f"成员摘要漂移：{drift}")


def check_replay(report: Report, package: Path, root: Path, manifest: dict | None, inputs: dict | None) -> dict | None:
    """Compare the product's frozen inputs and filter with an independent replay."""
    try:
        replayed = replay_package(package)
    except ValueError as error:
        report.record("replay-package", False, str(error))
        return None
    report.record("replay-package", True)

    if inputs is not None:
        for document, mirror in zip(inputs["documents"], replayed["documents"]):
            report.record(
                f"replay-document-{document['source_id']}",
                document["raw_sha256"] == mirror["raw_sha256"]
                and document["normalized_sha256"] == mirror["normalized_sha256"]
                and document["byte_length"] == mirror["byte_length"],
                "冻结文档摘要与独立重读不一致",
            )
    if manifest is not None:
        report.record(
            "replay-manifest-documents",
            [item["source_id"] for item in manifest["documents"]]
            == [item["source_id"] for item in replayed["documents"]],
            "清单文档顺序与作者包来源顺序不一致",
        )

    for name in (AUDIENCE, ATOMS, REVIEW):
        report.record(
            f"replay-frozen-{name}",
            read_bytes(root / name) == read_bytes(package / Path(name).name),
            f"{name} 与作者包里的同名声明不是同一份字节",
        )

    return replayed


def check_filter(report: Report, output: Path, replayed: dict) -> dict | None:
    path = output / FILTER
    if not path.is_file():
        report.record("filter-present", False, "缺少 filter.json")
        return None
    report.record("filter-present", True)
    try:
        product = strict_load(
            read_bytes(path),
            (
                "policy",
                "schema_version",
                "case_id",
                "level",
                "purpose",
                "counts",
                "reason_counts",
                "spans",
                "atoms",
            ),
            "filter.json",
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("filter-strict", False, str(error))
        return None
    report.record("filter-strict", True)
    report.record("filter-policy", product["policy"] == POLICY, product["policy"])
    report.record(
        "filter-level", product["level"] == replayed["audience"]["level"], product["level"]
    )
    report.record(
        "filter-counts",
        product["counts"] == replayed["counts"],
        f"计数不一致：{product['counts']} != {replayed['counts']}",
    )
    report.record(
        "filter-reason-counts",
        product["reason_counts"] == replayed["reason_counts"],
        f"理由分布不一致：{product['reason_counts']} != {replayed['reason_counts']}",
    )
    report.record(
        "filter-spans",
        product["spans"] == replayed["spans"],
        "片段决策与独立重放不一致",
    )
    report.record(
        "filter-atoms",
        product["atoms"] == replayed["decisions"],
        "原子决策与独立重放不一致",
    )
    return product


def check_densities(report: Report, output: Path, replayed: dict) -> dict | None:
    path = output / DENSITIES
    if not path.is_file():
        report.record("densities-present", False, "缺少 densities.json")
        return None
    report.record("densities-present", True)
    try:
        product = strict_load(
            read_bytes(path),
            (
                "policy",
                "schema_version",
                "case_id",
                "unchanged_across_densities",
                "minimal",
                "detailed",
            ),
            "densities.json",
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("densities-strict", False, str(error))
        return None
    report.record("densities-strict", True)
    minimal = product["minimal"]
    detailed = product["detailed"]
    report.record(
        "densities-names",
        minimal["density"] == "minimal" and detailed["density"] == "detailed",
        f"{minimal['density']} / {detailed['density']}",
    )
    for label, density in (("minimal", minimal), ("detailed", detailed)):
        report.record(
            f"densities-{label}-counts",
            density["counts"]
            == {
                "atoms": replayed["counts"]["kept_atoms"],
                "conditions": replayed["counts"]["conditions"],
            },
            f"{label} 计数与重放不一致：{density['counts']}",
        )
        report.record(
            f"densities-{label}-atom-order",
            [atom["atom_id"] for atom in density["atoms"]]
            == [atom["atom_id"] for atom in replayed["kept_atoms"]],
            f"{label} 的原子集合或顺序与重放不一致",
        )
    report.record(
        "densities-same-atoms",
        product["unchanged_across_densities"] is True,
        "unchanged_across_densities 不是 true",
    )
    # Only expansion may differ: every other field must be identical.
    stripped_minimal = [
        {key: value for key, value in atom.items() if key != "expansion"}
        for atom in minimal["atoms"]
    ]
    stripped_detailed = [
        {key: value for key, value in atom.items() if key != "expansion"}
        for atom in detailed["atoms"]
    ]
    report.record(
        "densities-identical-without-expansion",
        stripped_minimal == stripped_detailed,
        "两种密度除展开文本外还有差异",
    )
    report.record(
        "densities-minimal-bare",
        all(atom["expansion"] is None for atom in minimal["atoms"]),
        "极简密度不应带展开文本",
    )
    report.record(
        "densities-detailed-expanded",
        all(
            isinstance(atom["expansion"], str) and atom["expansion"]
            for atom in detailed["atoms"]
        ),
        "详细密度每条原子都应有展开文本",
    )
    return product


def check_report(report: Report, output: Path, replayed: dict, densities: dict | None) -> dict | None:
    path = output / REPORT
    if not path.is_file():
        report.record("report-present", False, "缺少 report.json")
        return None
    report.record("report-present", True)
    try:
        product = strict_load(
            read_bytes(path),
            (
                "policy",
                "schema_version",
                "case_id",
                "level",
                "counts",
                "reason_counts",
                "densities",
                "unchanged_across_densities",
                "model_calls",
                "network_requests",
            ),
            "report.json",
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("report-strict", False, str(error))
        return None
    report.record("report-strict", True)
    report.record(
        "report-counts",
        product["counts"] == replayed["counts"],
        f"报告计数不一致：{product['counts']}",
    )
    report.record(
        "report-reason-counts",
        product["reason_counts"] == replayed["reason_counts"],
        f"报告理由分布不一致：{product['reason_counts']}",
    )
    report.record(
        "report-densities",
        product["densities"]
        == {
            "minimal": {
                "atoms": replayed["counts"]["kept_atoms"],
                "conditions": replayed["counts"]["conditions"],
            },
            "detailed": {
                "atoms": replayed["counts"]["kept_atoms"],
                "conditions": replayed["counts"]["conditions"],
            },
        },
        f"报告的两密度计数不一致：{product['densities']}",
    )
    report.record(
        "report-unchanged",
        product["unchanged_across_densities"] is True
        and (densities is None or densities["unchanged_across_densities"] is True),
        "报告未声明两密度一致",
    )
    report.record(
        "report-zero-calls",
        product["model_calls"] == 0 and product["network_requests"] == 0,
        "报告未声明零模型调用与零网络请求",
    )
    return product


def check_markdown(report: Report, output: Path, replayed: dict) -> None:
    path = output / MARKDOWN
    if not path.is_file():
        report.record("markdown-present", False, "缺少 report.md")
        return
    report.record("markdown-present", True)
    markdown = read_bytes(path).decode("utf-8")
    missing = [
        section
        for section in ("## 计数", "## 排除理由分布", "## 排除明细", "## 保留原子与两种密度", "## 边界")
        if section not in markdown
    ]
    report.record("markdown-sections", not missing, f"报告缺少章节：{missing}")
    report.record(
        "markdown-counts",
        f"保留正文片段 {replayed['counts']['kept_spans']} 个、"
        f"排除 {replayed['counts']['excluded_spans']} 个" in markdown,
        "报告未写出与重放一致的保留/排除数",
    )
    missing_reasons = [
        reason for reason in replayed["reason_counts"] if f"| {reason} |" not in markdown
    ]
    report.record(
        "markdown-reasons",
        not missing_reasons,
        f"报告缺少排除理由行：{missing_reasons}",
    )
    report.record(
        "markdown-boundary",
        "0 次" in markdown and "不判断科学正确性" in markdown,
        "报告未声明零调用或不判断科学正确性",
    )


def check_output_manifest(report: Report, output: Path, replayed: dict) -> None:
    path = output / OUTPUT_MANIFEST
    if not path.is_file():
        report.record("output-manifest-present", False, "缺少输出 manifest.json")
        return
    report.record("output-manifest-present", True)
    entries = sorted(item.name for item in output.iterdir())
    report.record(
        "output-entries",
        entries == sorted([FILTER, DENSITIES, REPORT, MARKDOWN, OUTPUT_MANIFEST]),
        f"输出根只允许五个文件，实际 {entries}",
    )
    try:
        manifest = strict_load(
            read_bytes(path),
            (
                "policy",
                "producer",
                "schema_version",
                "case_id",
                "source_root",
                "source_inputs_sha256",
                "filter_sha256",
                "densities_sha256",
                "report_sha256",
                "markdown_sha256",
                "counts",
            ),
            "输出 manifest.json",
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("output-manifest-strict", False, str(error))
        return
    report.record("output-manifest-strict", True)
    report.record(
        "output-manifest-producer",
        manifest["producer"] == OUTPUT_PRODUCER,
        manifest["producer"],
    )
    missing = [
        name
        for name in (FILTER, DENSITIES, REPORT, MARKDOWN)
        if not (output / name).is_file()
    ]
    report.record(
        "output-manifest-digests",
        not missing
        and manifest["filter_sha256"] == sha256(read_bytes(output / FILTER))
        and manifest["densities_sha256"] == sha256(read_bytes(output / DENSITIES))
        and manifest["report_sha256"] == sha256(read_bytes(output / REPORT))
        and manifest["markdown_sha256"] == sha256(read_bytes(output / MARKDOWN)),
        f"输出文件摘要不符：" + (f"缺少 {missing}" if missing else "摘要不匹配"),
    )
    report.record(
        "output-manifest-counts",
        manifest["counts"] == replayed["counts"],
        f"输出清单计数与重放不一致：{manifest['counts']}",
    )
    report.record(
        "output-manifest-no-timestamp",
        "created_at" not in manifest,
        "输出清单不得带时间戳，否则两次构建不可能字节相同",
    )


def freeze(package: Path) -> int:
    files = {}
    for item in sorted(package.rglob("*"), key=lambda path: path.relative_to(package).parts):
        if not item.is_file():
            continue
        relative = item.relative_to(package).as_posix()
        if relative == FROZEN:
            continue
        data = read_bytes(item)
        files[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(
            [
                json.loads(read_bytes(package / "author.json").decode("utf-8"))["sources"]
            ][0]
        ),
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
    parser.add_argument("--output")
    parser.add_argument("--package")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    options = parser.parse_args(argv)

    default_package = Path("fixtures/relevance-filter-v1")
    package = Path(options.package) if options.package else default_package
    if options.freeze:
        if not package.is_dir():
            sys.stderr.write(f"未知参数：{package}\n")
            return 2
        return freeze(package)
    if not options.root:
        if options.package:
            only = Report()
            check_expected(only, package)
            check_frozen(only, package)
            only.dump(options.json)
            return 0 if not only.failures else 1
        sys.stderr.write(
            "用法：verify_relevance_filter.py <筛选根> [--output DIR] "
            "[--package DIR] [--json] [--freeze]\n"
        )
        return 2

    root = Path(options.root)
    report = Report()
    check_layout(report, root)
    manifest = check_manifest(report, root, package)
    inputs = check_inputs(report, root, manifest)
    check_expected(report, package)
    check_frozen(report, package)
    replayed = check_replay(report, package, root, manifest, inputs)

    if options.output and replayed is not None:
        output = Path(options.output)
        if not output.is_dir():
            report.record("output-present", False, f"{output} 不是目录")
        else:
            report.record("output-present", True)
            check_filter(report, output, replayed)
            densities = check_densities(report, output, replayed)
            check_report(report, output, replayed, densities)
            check_markdown(report, output, replayed)
            check_output_manifest(report, output, replayed)

    report.dump(options.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
