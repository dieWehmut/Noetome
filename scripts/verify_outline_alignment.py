#!/usr/bin/env python3
"""Independent read-only audit for the outline-alignment contract (B10-T01).

This script never imports the Rust product and never runs the CLI to decide
anything. It re-reads a frozen author package and an outline root/output on
disk, replays the alignment rules from the frozen bytes, and compares the
replay with the JSON the product wrote.

Usage:
    python3 scripts/verify_outline_alignment.py <root> [--output DIR]
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

POLICY = "outline-alignment-v1"
CONCEPTS_POLICY = "outline-concepts-v1"
AUTHOR_PRODUCER = "fixture_author"
APP_PRODUCER = "noetome-outline"
OUTPUT_PRODUCER = "outline_alignment_output"
SCHEMA_VERSION = 1

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
TEXT_DIR = "control/text"
LEXICON_SUFFIX = ".lexicon.json"
TEXT_SUFFIX = ".md"

TREE = "tree.json"
CONCEPTS = "concepts.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"

CONTEXT_HEADING = "heading"
CONTEXT_BODY = "body"
CONTEXT_CAPTION = "caption"
CONTEXT_APPENDIX = "appendix"

WARNINGS = (
    "heading_without_number",
    "level_jump",
    "duplicate_number",
    "number_depth_mismatch",
    "concept_only_in_caption_or_appendix",
    "heading_concept_missing_from_body",
    "no_headings",
)
FROZEN = "frozen-sha256.json"


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


def strict_load(payload: bytes, fields: tuple[str, ...], label: str, canonical_required: bool = True):
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
# Independent replay of the alignment rules.
# --------------------------------------------------------------------------


def heading_level(line: str) -> int:
    """Same rule as the Rust helper: <=3 spaces of indent, 1..=6 leading hashes,
    and the hashes must be followed by whitespace or the end of the line."""
    indentation = len(line) - len(line.lstrip(" "))
    if indentation > 3:
        return 0
    content = line[indentation:]
    hashes = 0
    for char in content:
        if char != "#":
            break
        hashes += 1
    if not 1 <= hashes <= 6:
        return 0
    rest = content[hashes:]
    if rest and not rest[0].isspace():
        return 0
    return hashes


def heading_title(line: str) -> str:
    content = line.rstrip().lstrip(" ").lstrip("#")
    return content.lstrip(" \u3000").rstrip()


def split_number(title: str) -> tuple[str | None, str]:
    index = 0
    while index < len(title) and (title[index].isdigit() or title[index] == "."):
        index += 1
    if index == 0 or not any(char.isdigit() for char in title[:index]):
        return None, title
    candidate = title[:index].rstrip(".").rstrip("、")
    if not candidate:
        return None, title
    rest = title[index:].lstrip(".").lstrip("、").lstrip(")").lstrip("）").lstrip(" ").lstrip("\u3000")
    return candidate, rest


def number_depth(number: str) -> int:
    return len([segment for segment in number.split(".") if segment])


def caption_line(line: str) -> bool:
    trimmed = line.lstrip(" \t*_")
    for prefix in ("图", "表", "附录", "Figure", "Fig.", "Table", "Appendix"):
        if trimmed.startswith(prefix):
            rest = trimmed[len(prefix) :]
            if rest == "":
                return True
            head = rest[0]
            if head.isspace() or head.isdigit() or head in (":", "：", ".", "、"):
                return True
    return False


def fold(text: str) -> str:
    """ASCII-only case folding, mirroring the Rust byte comparison."""
    return "".join(char.lower() if char.isascii() else char for char in text)


def find_all(haystack: str, needle: str) -> list[int]:
    if not needle:
        return []
    lowered = fold(haystack)
    target = fold(needle)
    found = []
    start = 0
    while True:
        index = lowered.find(target, start)
        if index < 0:
            return found
        found.append(index)
        start = index + 1


def rust_line_count(text: str) -> int:
    """Number of lines the Rust `str::lines()` iterator would yield."""
    if not text:
        return 0
    count = text.count("\n")
    if not text.endswith("\n"):
        count += 1
    return count


def line_of(text: str, byte: int) -> int:
    return text.count("\n", 0, byte) + 1


def line_text(text: str, byte: int) -> str:
    start = text.rfind("\n", 0, byte) + 1
    end = text.find("\n", byte)
    if end < 0:
        end = len(text)
    return text[start:end]


def replay_document(text: str, source_id: str, title: str, concepts: list[dict]) -> dict:
    lines = text.split("\n")
    sections = []
    stack: list[tuple[int, str]] = []
    numbers: dict[str, int] = {}
    warnings = []
    heading_lines: dict[int, int] = {}
    for index, line in enumerate(lines, start=1):
        level = heading_level(line)
        if level == 0:
            continue
        raw_title = heading_title(line)
        number, section_title = split_number(raw_title)
        section_id = f"{source_id}-{len(sections) + 1:04d}"
        while stack and stack[-1][0] >= level:
            stack.pop()
        parent = stack[-1][1] if stack else None
        if parent is not None:
            parent_level = next(
                section["level"] for section in sections if section["section_id"] == parent
            )
            if level - parent_level > 1:
                warnings.append(
                    {
                        "kind": "level_jump",
                        "document": source_id,
                        "section": section_id,
                        "line": index,
                    }
                )
        if number is not None:
            numbers[number] = numbers.get(number, 0) + 1
        if number is None:
            path = raw_title
        elif section_title == "":
            path = number
        else:
            path = f"{number} {section_title}"
        sections.append(
            {
                "section_id": section_id,
                "document": source_id,
                "number": number,
                "level": level,
                "title": section_title,
                "path": path,
                "parent": parent,
                "start_line": index,
            }
        )
        heading_lines[index] = len(sections) - 1
        stack.append((level, section_id))

    if not sections:
        warnings.append(
            {"kind": "no_headings", "document": source_id, "section": None, "line": 1}
        )

    for section in sections:
        if section["level"] == 1:
            continue
        number = section["number"]
        if number is None:
            warnings.append(
                {
                    "kind": "heading_without_number",
                    "document": source_id,
                    "section": section["section_id"],
                    "line": section["start_line"],
                }
            )
            continue
        if numbers.get(number, 0) > 1:
            warnings.append(
                {
                    "kind": "duplicate_number",
                    "document": source_id,
                    "section": section["section_id"],
                    "line": section["start_line"],
                }
            )
        parent = section["parent"]
        if parent is not None:
            parent_number = next(
                item["number"] for item in sections if item["section_id"] == parent
            )
            if parent_number is not None:
                expected = number_depth(parent_number) + 1
                if number_depth(number) != expected:
                    warnings.append(
                        {
                            "kind": "number_depth_mismatch",
                            "document": source_id,
                            "section": section["section_id"],
                            "line": section["start_line"],
                        }
                    )

    total_lines = max(rust_line_count(text), 1)
    for index, section in enumerate(sections):
        if index + 1 < len(sections):
            end = max(sections[index + 1]["start_line"] - 1, section["start_line"])
        else:
            end = total_lines
        section["end_line"] = end

    # Concept occurrences.
    occurrences = []
    for concept in concepts:
        terms = [concept["canonical"], *concept["aliases"]]
        hits = []
        for order, term in enumerate(terms):
            for byte in find_all(text, term):
                hits.append((byte, -len(term), order, term))
        hits.sort()
        claimed: list[tuple[int, int]] = []
        for byte, _, _, term in hits:
            end = byte + len(term)
            if any(byte < stop and end > start for start, stop in claimed):
                continue
            claimed.append((byte, end))
            line = line_of(text, byte)
            in_heading = heading_level(lines[line - 1]) > 0
            section = None
            for candidate in sections:
                if candidate["start_line"] <= line:
                    section = candidate
                else:
                    break
            context = CONTEXT_BODY
            if in_heading:
                context = CONTEXT_HEADING
            elif caption_line(line_text(text, byte)):
                context = CONTEXT_CAPTION
            elif section is not None and (
                "附录" in section["path"] or "appendix" in section["title"].lower()
            ):
                context = CONTEXT_APPENDIX
            occurrences.append(
                {
                    "concept_id": concept["concept_id"],
                    "term": term,
                    "document": source_id,
                    "section": section["section_id"] if section else None,
                    "context": context,
                    "line": line,
                    "byte": byte,
                }
            )

    unaligned = {
        warning["section"]
        for warning in warnings
        if warning["document"] == source_id and warning["section"] is not None
    }
    aligned = [section for section in sections if section["section_id"] not in unaligned]

    return {
        "source_id": source_id,
        "title": title,
        "sections": sections,
        "warnings": warnings,
        "occurrences": occurrences,
        "aligned": len(aligned),
        "unaligned": len(sections) - len(aligned),
    }


def replay_package(package: Path) -> dict:
    author = json.loads(read_bytes(package / "author.json").decode("utf-8"))
    concepts = json.loads(read_bytes(package / author["concepts"]).decode("utf-8"))["concepts"]
    documents = []
    warnings = []
    occurrences = []
    for spec in author["documents"]:
        text = read_bytes(package / spec["path"]).decode("utf-8")
        result = replay_document(text, spec["source_id"], spec["title"], concepts)
        documents.append(result)
        warnings.extend(result["warnings"])
        occurrences.extend(result["occurrences"])

    trees = []
    for document in documents:
        trees.append(
            {
                "source_id": document["source_id"],
                "sections": document["sections"],
                "aligned": document["aligned"],
                "unaligned": document["unaligned"],
            }
        )

    # Concepts named in a heading must be discussed in that section's body.
    for document in documents:
        for section in document["sections"]:
            named = {
                item["concept_id"]
                for item in occurrences
                if item["document"] == document["source_id"]
                and item["context"] == CONTEXT_HEADING
                and item["line"] == section["start_line"]
            }
            for concept_id in sorted(named):
                discussed = any(
                    item["concept_id"] == concept_id
                    and item["document"] == document["source_id"]
                    and section["start_line"] <= item["line"] <= section["end_line"]
                    and item["context"] != CONTEXT_HEADING
                    for item in occurrences
                )
                if not discussed:
                    warnings.append(
                        {
                            "kind": "heading_concept_missing_from_body",
                            "document": document["source_id"],
                            "section": section["section_id"],
                            "line": section["start_line"],
                        }
                    )

    inventory = []
    for concept in concepts:
        listed = [item for item in occurrences if item["concept_id"] == concept["concept_id"]]
        if listed and all(
            item["context"] in (CONTEXT_CAPTION, CONTEXT_APPENDIX) for item in listed
        ):
            warnings.append(
                {
                    "kind": "concept_only_in_caption_or_appendix",
                    "document": listed[0]["document"],
                    "section": listed[0]["section"],
                    "line": listed[0]["line"],
                }
            )
        by_document: dict[str, int] = {}
        for item in listed:
            by_document[item["document"]] = by_document.get(item["document"], 0) + 1
        inventory.append(
            {
                "concept_id": concept["concept_id"],
                "canonical": concept["canonical"],
                "total": len(listed),
                "by_document": by_document,
                "contexts": {
                    key: sum(1 for item in listed if item["context"] == key)
                    for key in (CONTEXT_HEADING, CONTEXT_BODY, CONTEXT_CAPTION, CONTEXT_APPENDIX)
                    if any(item["context"] == key for item in listed)
                },
                "first": listed[0]["document"] + ":" + str(listed[0]["line"]) if listed else None,
                "occurrences": listed,
            }
        )

    warnings.sort(key=lambda item: (item["document"], item["line"], item["kind"]))
    return {
        "author": author,
        "concepts": concepts,
        "trees": trees,
        "warnings": warnings,
        "inventory": inventory,
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
        "layout-root-entries",
        entries == [CONTROL],
        f"顶层只允许 control/，实际 {entries}",
    )
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control/")
        return
    control_entries = sorted(item.name for item in control.iterdir())
    report.record(
        "layout-control-entries",
        control_entries == ["inputs.json", "manifest.json", "text"],
        f"control/ 只允许 manifest.json、inputs.json、text/，实际 {control_entries}",
    )
    for path in [root, control, *control.iterdir(), *(control / "text").iterdir()]:
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
    payload = read_bytes(path)
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
        "author_manifest_sha256",
        "lexicon_sha256",
        "inputs_sha256",
        "documents",
        "created_at",
    )
    try:
        manifest = strict_load(payload, fields, "control/manifest.json")
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
    author_bytes = read_bytes(package / "author.json")
    report.record(
        "manifest-author-sha256",
        manifest["author_manifest_sha256"] == sha256(author_bytes),
        "作者包摘要不符",
    )
    inputs_bytes = read_bytes(root / INPUTS)
    report.record(
        "manifest-inputs-sha256",
        manifest["inputs_sha256"] == sha256(inputs_bytes),
        "inputs.json 摘要不符",
    )
    lexicon = root / TEXT_DIR / f"{manifest['case_id']}{LEXICON_SUFFIX}"
    if not lexicon.is_file():
        report.record("manifest-lexicon-present", False, f"缺少 {lexicon.name}")
    else:
        report.record(
            "manifest-lexicon-sha256",
            manifest["lexicon_sha256"] == sha256(read_bytes(lexicon)),
            "词表摘要不符",
        )
    digests = {item["source_id"]: item for item in manifest["documents"]}
    for document in manifest["documents"]:
        path = root / TEXT_DIR / f"{document['source_id']}{TEXT_SUFFIX}"
        if not path.is_file():
            report.record("manifest-text-present", False, f"缺少 {path.name}")
            continue
        data = read_bytes(path)
        report.record(
            f"manifest-text-{document['source_id']}",
            document["normalized_sha256"] == sha256(data)
            and document["byte_length"] == len(data),
            "冻结文本摘要或长度不符",
        )
    text_entries = sorted(item.name for item in (root / TEXT_DIR).iterdir())
    expected = sorted(
        [f"{item['source_id']}{TEXT_SUFFIX}" for item in digests.values()]
        + [f"{manifest['case_id']}{LEXICON_SUFFIX}"]
    )
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
        text = read_bytes(root / TEXT_DIR / f"{document['source_id']}{TEXT_SUFFIX}")
        spans = document["spans"]
        covered = 0
        contiguous = True
        for span in spans:
            if span["start_byte"] != covered:
                contiguous = False
            if not (
                span["start_byte"]
                <= span["end_byte"]
                <= len(text)
            ):
                contiguous = False
                continue
            slice_bytes = text[span["start_byte"] : span["end_byte"]]
            if sha256(slice_bytes) != span["text_sha256"]:
                contiguous = False
            covered = span["end_byte"]
        report.record(
            f"inputs-spans-{document['source_id']}",
            contiguous and covered == len(text),
            "Span 未连续覆盖冻结文本或片段摘要不符",
        )
        headings = [
            span for span in spans if span["kind"] == "heading"
        ]
        expected_headings = [
            line
            for line in text.decode("utf-8").split("\n")
            if heading_level(line) > 0
        ]
        report.record(
            f"inputs-headings-{document['source_id']}",
            len(headings) == len(expected_headings),
            f"标题 Span {len(headings)} 与独立推导 {len(expected_headings)} 不一致",
        )
    return inputs


def check_replay(report: Report, package: Path, tree: dict, inventory: dict, result: dict) -> None:
    replayed = replay_package(package)
    product_sections = {
        document["source_id"]: document for document in tree["documents"]
    }
    for replay_doc in replayed["trees"]:
        product = product_sections.get(replay_doc["source_id"])
        if product is None:
            report.record(
                "replay-document-present", False, f"缺少文档 {replay_doc['source_id']}"
            )
            continue
        report.record(
            f"replay-aligned-{replay_doc['source_id']}",
            (product["aligned"], product["unaligned"])
            == (replay_doc["aligned"], replay_doc["unaligned"]),
            f"对齐计数 {product['aligned']}/{product['unaligned']} != "
            f"{replay_doc['aligned']}/{replay_doc['unaligned']}",
        )
        left = [
            (
                item["section_id"],
                item["number"],
                item["level"],
                item["title"],
                item["path"],
                item["parent"],
                item["start_line"],
                item["end_line"],
            )
            for item in product["sections"]
        ]
        right = [
            (
                item["section_id"],
                item["number"],
                item["level"],
                item["title"],
                item["path"],
                item["parent"],
                item["start_line"],
                item["end_line"],
            )
            for item in replay_doc["sections"]
        ]
        report.record(
            f"replay-sections-{replay_doc['source_id']}",
            left == right,
            "章节行与独立重放不一致",
        )

    product_warnings = [
        (item["kind"], item["document"], item["line"]) for item in result["warnings"]
    ]
    replay_warnings = [
        (item["kind"], item["document"], item["line"]) for item in replayed["warnings"]
    ]
    report.record(
        "replay-warnings",
        product_warnings == replay_warnings,
        f"警告集合不一致：产品 {product_warnings} 重放 {replay_warnings}",
    )
    report.record(
        "replay-warning-counts",
        result["warning_counts"]
        == {
            kind: sum(1 for item in replayed["warnings"] if item["kind"] == kind)
            for kind in sorted({item["kind"] for item in replayed["warnings"]})
        },
        "warning_counts 与独立重放不一致",
    )

    product_concepts = {item["concept_id"]: item for item in inventory["concepts"]}
    for replay_concept in replayed["inventory"]:
        product = product_concepts.get(replay_concept["concept_id"])
        if product is None:
            report.record(
                "replay-concept-present",
                False,
                f"缺少概念 {replay_concept['concept_id']}",
            )
            continue
        first = product["first"]
        first_text = (
            first["document"] + ":" + str(first["line"]) if isinstance(first, dict) else None
        )
        report.record(
            f"replay-concept-{replay_concept['concept_id']}",
            product["total"] == replay_concept["total"]
            and product["by_document"] == replay_concept["by_document"]
            and first_text == replay_concept["first"]
            and product["contexts"] == replay_concept["contexts"],
            f"概念索引不一致：产品 total={product['total']} first={first_text} "
            f"by_document={product['by_document']} contexts={product['contexts']}；"
            f"重放 total={replay_concept['total']} first={replay_concept['first']} "
            f"by_document={replay_concept['by_document']} contexts={replay_concept['contexts']}",
        )
        report.record(
            f"replay-concept-lines-{replay_concept['concept_id']}",
            [(item["document"], item["line"], item["context"]) for item in product["occurrences"]]
            == [
                (item["document"], item["line"], item["context"])
                for item in replay_concept["occurrences"]
            ],
            "概念出现位置不一致",
        )


def check_output(report: Report, output: Path, tree: dict | None, inventory: dict | None, result: dict | None) -> None:
    if not output.is_dir():
        report.record("output-present", False, f"{output} 不是目录")
        return
    report.record("output-present", True)
    entries = sorted(item.name for item in output.iterdir())
    report.record(
        "output-entries",
        entries == sorted([TREE, CONCEPTS, REPORT, MARKDOWN, OUTPUT_MANIFEST]),
        f"输出根只允许五个文件，实际 {entries}",
    )
    for name in (TREE, CONCEPTS, REPORT):
        if not (output / name).is_file():
            report.record("output-file-present", False, f"缺少 {name}")
            return
    report.record("output-file-present", True)

    tree_bytes = read_bytes(output / TREE)
    concepts_bytes = read_bytes(output / CONCEPTS)
    report_bytes = read_bytes(output / REPORT)
    markdown_bytes = read_bytes(output / MARKDOWN)
    manifest_bytes = read_bytes(output / OUTPUT_MANIFEST)

    try:
        manifest = strict_load(
            manifest_bytes,
            (
                "policy",
                "producer",
                "schema_version",
                "case_id",
                "source_root",
                "source_inputs_sha256",
                "tree_sha256",
                "concepts_sha256",
                "report_sha256",
                "markdown_sha256",
                "counts",
            ),
            "输出 manifest.json",
        )
        report.record("output-manifest-strict", True)
    except (ValueError, json.JSONDecodeError) as error:
        report.record("output-manifest-strict", False, str(error))
        return
    report.record(
        "output-manifest-producer",
        manifest["producer"] == OUTPUT_PRODUCER,
        manifest["producer"],
    )
    report.record(
        "output-manifest-digests",
        manifest["tree_sha256"] == sha256(tree_bytes)
        and manifest["concepts_sha256"] == sha256(concepts_bytes)
        and manifest["report_sha256"] == sha256(report_bytes)
        and manifest["markdown_sha256"] == sha256(markdown_bytes),
        "输出文件摘要不符",
    )
    if result is not None:
        report.record(
            "output-manifest-counts",
            manifest["counts"] == result["counts"],
            "输出清单计数与 report.json 不一致",
        )
    markdown = markdown_bytes.decode("utf-8")
    missing = [
        section
        for section in ("## 章节对齐", "## 全局概念清单", "## 对齐警告", "## 边界")
        if section not in markdown
    ]
    report.record("output-markdown-sections", not missing, f"报告缺少章节：{missing}")
    report.record(
        "output-markdown-no-model-claims",
        "零模型调用" in markdown and "零网络请求" in markdown,
        "报告未声明零模型调用与零网络请求",
    )
    report.record(
        "output-zero-calls",
        result is not None
        and result["model_calls"] == 0
        and result["network_requests"] == 0,
        "report.json 的 model_calls/network_requests 不是 0",
    )


def check_counts(report: Report, tree: dict, inventory: dict, result: dict) -> None:
    sections = sum(len(document["sections"]) for document in tree["documents"])
    aligned = sum(document["aligned"] for document in tree["documents"])
    unaligned = sum(document["unaligned"] for document in tree["documents"])
    occurrences = sum(item["total"] for item in inventory["concepts"])
    counts = result["counts"]
    report.record(
        "counts-derive-from-tables",
        counts["documents"] == len(tree["documents"])
        and counts["sections"] == sections
        and counts["sections_aligned"] == aligned
        and counts["sections_unaligned"] == unaligned
        and counts["concepts"] == len(inventory["concepts"])
        and counts["occurrences"] == occurrences
        and counts["warnings"] == len(result["warnings"]),
        f"计数与表不一致：{counts}",
    )
    report.record(
        "counts-aligned-plus-unaligned",
        counts["sections_aligned"] + counts["sections_unaligned"] == counts["sections"],
        "已对齐与未对齐之和与章节总数不一致",
    )


def check_expected(report: Report, package: Path) -> None:
    """The package's own expected.json must agree with this independent replay."""
    path = package / "expected.json"
    if not path.is_file():
        report.record("expected-present", False, "缺少 expected.json")
        return
    expected = json.loads(read_bytes(path).decode("utf-8"))
    replayed = replay_package(package)
    warnings = [
        (item["kind"], item["document"], item["line"]) for item in replayed["warnings"]
    ]
    declared = [
        (item["kind"], item["document"], item["line"]) for item in expected["warnings"]
    ]
    report.record(
        "expected-warnings",
        declared == warnings,
        f"expected.json 的警告与独立重放不一致：声明 {declared}，重放 {warnings}",
    )
    documents = [
        {
            "source_id": item["source_id"],
            "sections": len(item["sections"]),
            "aligned": item["aligned"],
            "unaligned": item["unaligned"],
        }
        for item in replayed["trees"]
    ]
    report.record(
        "expected-documents",
        expected["documents"] == documents,
        f"expected.json 的每文档对齐数与独立重放不一致：{expected['documents']} != {documents}",
    )
    concepts = [
        {
            "concept_id": item["concept_id"],
            "total": item["total"],
            "documents": len(item["by_document"]),
            "first": item["first"],
        }
        for item in replayed["inventory"]
    ]
    report.record(
        "expected-concepts",
        expected["concepts"] == concepts,
        f"expected.json 的概念索引与独立重放不一致：{expected['concepts']} != {concepts}",
    )
    counts = {
        "documents": len(replayed["trees"]),
        "sections": sum(len(item["sections"]) for item in replayed["trees"]),
        "sections_aligned": sum(item["aligned"] for item in replayed["trees"]),
        "sections_unaligned": sum(item["unaligned"] for item in replayed["trees"]),
        "concepts": len(replayed["inventory"]),
        "occurrences": sum(item["total"] for item in replayed["inventory"]),
        "warnings": len(replayed["warnings"]),
    }
    report.record(
        "expected-counts",
        expected["counts"] == counts,
        f"expected.json 的计数与独立重放不一致：{expected['counts']} != {counts}",
    )
    tally: dict[str, int] = {}
    for item in replayed["warnings"]:
        tally[item["kind"]] = tally.get(item["kind"], 0) + 1
    report.record(
        "expected-warning-counts",
        expected["warning_counts"] == dict(sorted(tally.items())),
        f"expected.json 的警告分类计数与独立重放不一致：{expected['warning_counts']} != "
        f"{dict(sorted(tally.items()))}",
    )
    report.record(
        "expected-zero-calls",
        expected["model_calls"] == 0 and expected["network_requests"] == 0,
        "expected.json 未声明零模型调用与零网络请求",
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
        and (declared[name]["sha256"] != digest["sha256"]
             or declared[name]["byte_length"] != digest["byte_length"])
    ]
    report.record("frozen-digests", not drift, f"成员摘要漂移：{drift}")


def freeze(package: Path) -> int:
    files = {}
    for item in sorted(package.rglob("*")):
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
        "workflow_id": "outline-alignment-v1",
        "case_count": 4,
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

    default_package = Path("fixtures/outline-alignment-v1")
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
            "用法：verify_outline_alignment.py <对齐根> [--output DIR] "
            "[--package DIR] [--json] [--freeze]\n"
        )
        return 2
    root = Path(options.root)
    report = Report()
    check_layout(report, root)
    manifest = check_manifest(report, root, package)
    check_inputs(report, root, manifest)
    check_expected(report, package)
    check_frozen(report, package)

    if options.output:
        output = Path(options.output)
        tree = inventory = result = None
        try:
            tree = json.loads(read_bytes(output / TREE).decode("utf-8"))
            inventory = json.loads(read_bytes(output / CONCEPTS).decode("utf-8"))
            result = json.loads(read_bytes(output / REPORT).decode("utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            report.record("output-load", False, str(error))
        if tree is not None and inventory is not None and result is not None:
            report.record("output-load", True)
            check_counts(report, tree, inventory, result)
            check_output(report, output, tree, inventory, result)
            check_replay(report, package, tree, inventory, result)

    report.dump(options.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
