#!/usr/bin/env python3
"""Independent read-only audit for the structure-navigation contract (B10-T03).

This script never imports the Rust product and never runs the CLI to decide
anything. It re-reads a frozen author package and a structure root/output on
disk, replays heading detection, skeleton verdicts, link resolution and the
concept placement table from the frozen bytes, and compares the replay with the
JSON the product wrote.

Usage:
    python3 scripts/verify_structure_navigation.py <root> [--output DIR]
        [--package DIR] [--json] [--freeze]

Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "structure-navigation-v1"
CONCEPTS_POLICY = "structure-concepts-v1"
FAMILY_POLICY = "structure-family-v1"
AUTHOR_PRODUCER = "fixture_author"
APP_PRODUCER = "noetome-structure"
OUTPUT_PRODUCER = "structure_navigation_output"
SCHEMA_VERSION = 1

REQ_REQUIRED = "required"
REQ_OPTIONAL = "optional"

STATUS_MATCHED = "matched"
STATUS_MISORDERED = "misordered"
STATUS_EXTRA = "extra"
STATUS_MISSING = "missing"
STATUS_EMPTY = "empty"

LINK_RESOLVED = "resolved"
LINK_TARGET_MISSING = "target_missing"
LINK_HEADING_MISSING = "heading_missing"
LINK_AMBIGUOUS = "ambiguous"

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
FAMILY = "control/family.json"
CONCEPTS = "control/concepts.json"
TEXT_DIR = "control/text"
TEXT_SUFFIX = ".md"

STRUCTURE = "structure.json"
LINKS = "links.json"
REPORT = "report.json"
NAVIGATION = "navigation.md"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"

FROZEN = "frozen-sha256.json"

MANIFEST_FIELDS = (
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
    "family_sha256",
    "concepts_sha256",
    "inputs_sha256",
    "documents",
    "created_at",
)
INPUTS_FIELDS = ("policy", "schema_version", "case_id", "family", "notes")
NOTE_DIGEST_FIELDS = (
    "note_id",
    "path",
    "title",
    "raw_sha256",
    "byte_length",
    "normalized_sha256",
    "span_count",
    "heading_count",
)
OUTPUT_MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "source_root",
    "source_inputs_sha256",
    "structure_sha256",
    "links_sha256",
    "report_sha256",
    "navigation_sha256",
    "markdown_sha256",
    "counts",
)
ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("concepts.json", "family.json", "inputs.json", "manifest.json", "text")
OUTPUT_ENTRIES = ("links.json", "manifest.json", "navigation.md", "report.json", "report.md", "structure.json")


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
    """读取一个必须是规范编码的记录；返回 (值, 错误说明)。"""
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        return None, f"{label} 不是 UTF-8：{error}"
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        return None, f"{label} 不是合法 JSON：{error}"
    if not isinstance(value, dict):
        return None, f"{label} 必须是对象。"
    if tuple(sorted(value.keys())) != tuple(sorted(fields)):
        missing = [name for name in fields if name not in value]
        extra = [name for name in value if name not in fields]
        return None, f"{label} 字段集合不符：缺少 {missing}，多余 {extra}"
    if canonical_required:
        expected = canonical(value)
        # 产品写派生记录时在末尾带一个换行；只允许这一个差异。
        if text != expected and text != expected + "\n":
            return None, f"{label} 不是规范编码（可能重复键、未知字段或美化缩进）。"
    return value, ""


def normalize(raw: bytes) -> str:
    text = raw.decode("utf-8")
    if text.startswith("\ufeff"):
        text = text[1:]
    return text.replace("\r\n", "\n").replace("\r", "\n")


def heading_level(line: str) -> int:
    """与产品的 heading_level 逐条对应：缩进最多 3，1 到 6 个 #，其后为空或空白。"""
    content = line.lstrip(" ")
    if len(line) - len(content) > 3:
        return 0
    hashes = 0
    for value in content:
        if value == "#":
            hashes += 1
        else:
            break
    if not 1 <= hashes <= 6:
        return 0
    rest = content[hashes:]
    if not rest:
        return hashes
    return hashes if rest[0].isspace() else 0


def heading_title(line: str) -> str:
    return line.strip().lstrip("#").strip()


def is_table_row(line: str) -> bool:
    return line.lstrip().startswith("|")


def headings(text: str) -> list[dict[str, object]]:
    """扫标题并统计每节正文行数：从标题行的下一行到下一标题前一行。"""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    found: list[tuple[int, int, str]] = []
    for index, line in enumerate(lines):
        level = heading_level(line)
        if level > 0:
            found.append((index + 1, level, heading_title(line)))
    out: list[dict[str, object]] = []
    for position, (line, level, title) in enumerate(found):
        end = found[position + 1][0] - 1 if position + 1 < len(found) else len(lines)
        body_lines = sum(1 for value in lines[line:end] if value.strip())
        out.append(
            {
                "line": line,
                "level": level,
                "title": title,
                "body_lines": body_lines,
                "empty": body_lines == 0,
            }
        )
    return out


def skeleton_titles(scanned: list[dict[str, object]]) -> list[str]:
    shallow = [item["title"] for item in scanned if item["level"] == 2]
    if shallow:
        return shallow
    # 没有任何 H2 时退化为"所有标题平级"，否则单层笔记永远判成缺全部。
    return [item["title"] for item in scanned]


def verdicts(family: dict, scanned: list[dict[str, object]]) -> list[dict[str, object]]:
    present = skeleton_titles(scanned)
    empty_of = {
        item["title"]: item["empty"] for item in scanned
    }
    out: list[dict[str, object]] = []
    highest: int | None = None
    for section in family["skeleton"]:
        title = section["title"]
        position = present.index(title) if title in present else None
        if position is None:
            status = STATUS_MISSING
        else:
            misplaced = highest is not None and position < highest
            highest = position if highest is None else max(highest, position)
            if empty_of.get(title):
                status = STATUS_EMPTY
            elif misplaced:
                status = STATUS_MISORDERED
            else:
                status = STATUS_MATCHED
        out.append(
            {
                "key": section["key"],
                "title": title,
                "requirement": section["requirement"],
                "status": status,
                "position": position,
            }
        )
    return out


def extra_titles(family: dict, scanned: list[dict[str, object]]) -> list[str]:
    declared = {section["title"] for section in family["skeleton"]}
    return [title for title in skeleton_titles(scanned) if title not in declared]


def raw_links(text: str) -> list[dict[str, object]]:
    out: list[dict[str, object]] = []
    for index, line in enumerate(text.split("\n")):
        table = is_table_row(line)
        cursor = 0
        while cursor + 1 < len(line):
            if line[cursor] == "[" and line[cursor + 1] == "[":
                offset = line.find("]]", cursor + 2)
                if offset < 0:
                    break
                out.append(
                    {
                        "line": index + 1,
                        "raw": line[cursor : offset + 2],
                        "inner": line[cursor + 2 : offset],
                        "in_table": table,
                    }
                )
                cursor = offset + 2
                continue
            cursor += 1
    return out


def split_link(inner: str) -> tuple[str, str | None, str | None, bool]:
    alias_at: tuple[int, bool] | None = None
    for index, value in enumerate(inner):
        if value == "|":
            escaped = index > 0 and inner[index - 1] == "\\"
            alias_at = (index, escaped)
            break
    if alias_at is None:
        body, alias, escaped = inner, None, False
    else:
        index, escaped = alias_at
        body = inner[: index - 1] if escaped else inner[:index]
        alias, escaped_flag = inner[index + 1 :], escaped
        escaped = escaped_flag
    if "#" in body:
        target, heading = body.split("#", 1)
        return target, heading, alias, escaped
    return body, None, alias, escaped


def resolve_target(family: dict, target: str) -> tuple[dict | None, str, str]:
    if not target:
        return None, LINK_TARGET_MISSING, "链接目标不能为空。"
    exact = [note for note in family["notes"] if note["path"] == target]
    if len(exact) == 1:
        return exact[0], LINK_RESOLVED, ""
    stem = target[: -len(".md")] if target.endswith(".md") else target
    by_stem = []
    for note in family["notes"]:
        path_stem = note["path"][: -len(".md")] if note["path"].endswith(".md") else note["path"]
        if path_stem == stem or path_stem.endswith("/" + stem) or note["title"] == stem:
            by_stem.append(note)
    if len(by_stem) == 1:
        return by_stem[0], LINK_RESOLVED, ""
    if not by_stem:
        return None, LINK_TARGET_MISSING, f"链接目标 {target} 不在本主题族声明的笔记里。"
    return (
        None,
        LINK_AMBIGUOUS,
        f"链接目标 {target} 在这份作者包里对应 {len(by_stem)} 份笔记，有歧义。",
    )


def scan_links(family: dict, views: list[dict[str, object]]) -> tuple[list[dict[str, object]], str]:
    """重放正文链接扫描。返回 (链接列表, 拒绝说明)；拒绝说明非空表示整包被拒。"""
    out: list[dict[str, object]] = []
    for view in views:
        for link in raw_links(view["text"]):
            target, heading, alias, escaped = split_link(link["inner"])
            if link["in_table"] and alias is not None and not escaped:
                return [], (
                    "STRUCT_LINK_UNESCAPED_ALIAS",
                    f"笔记 {view['note']['path']} 第 {link['line']} 行在表格单元里写了未转义的别名竖线。",
                )
            note, status, detail = resolve_target(family, target)
            if note is not None and heading is not None:
                body = next(
                    (candidate["text"] for candidate in views if candidate["note"] is note), ""
                )
                if not any(item["title"] == heading for item in headings(body)):
                    status = LINK_HEADING_MISSING
                    detail = f"目标笔记里没有标题「{heading}」。"
            out.append(
                {
                    "note_id": view["note"]["note_id"],
                    "line": link["line"],
                    "raw": link["raw"],
                    "target": target,
                    "heading": heading,
                    "alias": alias,
                    "escaped_alias": escaped,
                    "in_table": link["in_table"],
                    "status": status,
                    "detail": detail,
                }
            )
    return out, ""


def concept_table(family: dict, concepts: dict) -> tuple[list[dict[str, object]], str, str]:
    """重放概念表。返回 (概念位置, 错误码, 说明)。"""
    known = {concept["concept_id"] for concept in concepts["concepts"]}
    for note in family["notes"]:
        for concept_id in list(note["explains"]) + list(note["references"]):
            if concept_id not in known:
                return [], "STRUCT_UNKNOWN_CONCEPT", f"笔记 {note['note_id']} 用的概念 {concept_id} 不在概念表里。"
        overlap = [value for value in note["references"] if value in note["explains"]]
        if overlap:
            return (
                [],
                "STRUCT_ARGUMENT",
                f"笔记 {note['note_id']} 同时解释并引用了概念 {overlap[0]}。",
            )
    out: list[dict[str, object]] = []
    for concept in concepts["concepts"]:
        concept_id = concept["concept_id"]
        explaining = [
            note["note_id"] for note in family["notes"] if concept_id in note["explains"]
        ]
        if len(explaining) > 1:
            return (
                [],
                "STRUCT_DUPLICATE_EXPLANATION",
                f"概念 {concept_id} 被多处完整解释：{'、'.join(explaining)}。",
            )
        referenced_by = [
            note["note_id"] for note in family["notes"] if concept_id in note["references"]
        ]
        out.append(
            {
                "concept_id": concept_id,
                "term": concept["term"],
                "canonical": explaining[0] if explaining else None,
                "explaining": explaining,
                "referenced_by": referenced_by,
            }
        )
    return out, "", ""


def generated(family: dict, placements: list[dict[str, object]], views: list[dict[str, object]]):
    out: list[dict[str, object]] = []
    for note in family["notes"]:
        for sibling in family["notes"]:
            if sibling["note_id"] == note["note_id"]:
                continue
            out.append(
                {
                    "owner": note["note_id"],
                    "purpose": "navigation",
                    "text": sibling["title"],
                    "target_path": sibling["path"],
                    "target_heading": None,
                }
            )
    for note in family["notes"]:
        for concept_id in note["references"]:
            concept = next(
                (item for item in placements if item["concept_id"] == concept_id), None
            )
            if concept is None or concept["canonical"] is None:
                continue
            target = next(
                (item for item in family["notes"] if item["note_id"] == concept["canonical"]),
                None,
            )
            if target is None:
                continue
            out.append(
                {
                    "owner": note["note_id"],
                    "purpose": "concept",
                    "text": concept["term"],
                    "target_path": target["path"],
                    # 规范位置是"哪一份笔记"：猜标题一旦猜错就是假链接。
                    "target_heading": None,
                }
            )
    verified: list[dict[str, object]] = []
    for link in out:
        body = next(
            (view["text"] for view in views if view["note"]["path"] == link["target_path"]), None
        )
        if body is None:
            status = LINK_TARGET_MISSING
        elif link["target_heading"] is None:
            status = LINK_RESOLVED
        else:
            status = (
                LINK_RESOLVED
                if any(item["title"] == link["target_heading"] for item in headings(body))
                else LINK_HEADING_MISSING
            )
        verified.append({**link, "status": status})
    return verified


def replay_package(package: Path):
    """从作者包重放整个判定。返回 (重放结果, 错误码, 说明)。"""
    family = json.loads(read_bytes(package / "family.json").decode("utf-8"))
    concepts = json.loads(read_bytes(package / "concepts.json").decode("utf-8"))
    views: list[dict[str, object]] = []
    for note in family["notes"]:
        path = package / note["path"]
        if not path.is_file():
            return None, "STRUCT_ARGUMENT", f"作者包缺少笔记文件 {note['path']}。"
        raw = read_bytes(path)
        text = normalize(raw)
        views.append({"note": note, "text": text, "raw_sha256": sha256(raw), "byte_length": len(raw)})
    placements, code, detail = concept_table(family, concepts)
    if code:
        return None, code, detail
    links, code = scan_links(family, views)
    if code:
        return None, code[0], code[1]
    generated_links = generated(family, placements, views)
    notes_out: list[dict[str, object]] = []
    for view in views:
        scanned = headings(view["text"])
        table = verdicts(family, scanned)
        extra = extra_titles(family, scanned)
        notes_out.append(
            {
                "note_id": view["note"]["note_id"],
                "path": view["note"]["path"],
                "title": view["note"]["title"],
                "headings": scanned,
                "heading_count": len(scanned),
                "sections": skeleton_titles(scanned),
                "skeleton": table,
                "extra_titles": extra,
                "matched": sum(1 for item in table if item["status"] == STATUS_MATCHED),
                "misordered": sum(1 for item in table if item["status"] == STATUS_MISORDERED),
                "extra": len(extra),
                "missing_required": sum(
                    1
                    for item in table
                    if item["status"] == STATUS_MISSING and item["requirement"] == REQ_REQUIRED
                ),
                "omitted_optional": sum(
                    1
                    for item in table
                    if item["status"] == STATUS_MISSING and item["requirement"] == REQ_OPTIONAL
                ),
                "empty": sum(1 for item in table if item["status"] == STATUS_EMPTY),
                "consistent": all(
                    item["status"] == STATUS_MATCHED
                    or (item["status"] == STATUS_MISSING and item["requirement"] == REQ_OPTIONAL)
                    for item in table
                )
                and not extra,
            }
        )
    section_counts = {
        "notes": len(notes_out),
        "skeleton": len(family["skeleton"]),
        "sections": sum(len(note["sections"]) for note in notes_out),
        "matched": sum(note["matched"] for note in notes_out),
        "misordered": sum(note["misordered"] for note in notes_out),
        "extra": sum(note["extra"] for note in notes_out),
        "missing_required": sum(note["missing_required"] for note in notes_out),
        "omitted_optional": sum(note["omitted_optional"] for note in notes_out),
        "empty": sum(note["empty"] for note in notes_out),
        "consistent_notes": sum(1 for note in notes_out if note["consistent"]),
    }
    link_counts = {
        "source_links": len(links),
        "source_resolved": sum(1 for link in links if link["status"] == LINK_RESOLVED),
        "source_target_missing": sum(
            1 for link in links if link["status"] == LINK_TARGET_MISSING
        ),
        "source_heading_missing": sum(
            1 for link in links if link["status"] == LINK_HEADING_MISSING
        ),
        "source_ambiguous": sum(1 for link in links if link["status"] == LINK_AMBIGUOUS),
        "generated_links": len(generated_links),
        "generated_broken": sum(
            1 for link in generated_links if link["status"] != LINK_RESOLVED
        ),
        "external_references": sum(
            1 for item in placements if item["canonical"] is None and item["referenced_by"]
        ),
    }
    return (
        {
            "family": family,
            "concepts": concepts,
            "views": views,
            "notes": notes_out,
            "source_links": links,
            "generated": generated_links,
            "placements": placements,
            "counts": {"sections": section_counts, "links": link_counts},
        },
        "",
        "",
    )


def check_layout(root: Path, report: Report) -> None:
    entries = sorted(item.name for item in root.iterdir())
    report.record("root-entries", entries == list(ROOT_ENTRIES), f"实际 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("control-directory", False, "缺少 control/")
        return
    report.record("control-directory", True)
    inside = sorted(item.name for item in control.iterdir())
    report.record("control-entries", inside == list(CONTROL_ENTRIES), f"实际 {inside}")
    text_dir = control / "text"
    if not text_dir.is_dir():
        report.record("text-directory", False, "缺少 control/text/")
        return
    report.record("text-directory", True)
    linked = [
        str(path.relative_to(root))
        for path in root.rglob("*")
        if path.is_symlink()
    ]
    report.record("no-symlinks", not linked, f"发现符号链接 {linked}")


def check_manifest(root: Path, package: Path | None, report: Report):
    manifest, error = strict_load(read_bytes(root / MANIFEST), MANIFEST_FIELDS, "control/manifest.json")
    if manifest is None:
        report.record("manifest-strict", False, error)
        return None
    report.record("manifest-strict", True)
    ok = (
        manifest["policy"] == POLICY
        and manifest["producer"] == APP_PRODUCER
        and manifest["schema_version"] == SCHEMA_VERSION
    )
    report.record(
        "manifest-identity",
        ok,
        f"policy={manifest['policy']} producer={manifest['producer']} schema={manifest['schema_version']}",
    )
    root_text = str(root.resolve())
    report.record("manifest-root", manifest["root"] == root_text, f"{manifest['root']} != {root_text}")
    text_dir = root / TEXT_DIR
    if not text_dir.is_dir():
        # 缺目录本身已在 check_layout 记过；这里必须继续报错而不是抛异常，
        # 否则审计器在坏根上崩溃，读者拿不到完整的失败清单。
        report.record("manifest-text-set", False, "缺少 control/text/，无法核对冻结正文")
        return manifest
    text_names = sorted(item.name for item in text_dir.iterdir())
    expected_names = sorted(f"{note['note_id']}{TEXT_SUFFIX}" for note in manifest["documents"])
    report.record("manifest-text-set", text_names == expected_names, f"{text_names} != {expected_names}")
    for digest in manifest["documents"]:
        path = text_dir / f"{digest['note_id']}{TEXT_SUFFIX}"
        body = read_bytes(path) if path.is_file() else b""
        report.record(
            f"manifest-note-{digest['note_id']}",
            sha256(body) == digest["normalized_sha256"] and len(body) == digest["byte_length"],
            "规范化摘要或字节数不符",
        )
    for name, field in (
        ("family.json", "family_sha256"),
        ("concepts.json", "concepts_sha256"),
        ("inputs.json", "inputs_sha256"),
    ):
        body = read_bytes(root / CONTROL / name)
        report.record(
            f"manifest-{name}-digest",
            sha256(body) == manifest[field],
            f"{sha256(body)} != {manifest[field]}",
        )
    if package is not None:
        for name, field in (("family.json", "family_sha256"), ("concepts.json", "concepts_sha256")):
            if (package / name).is_file():
                report.record(
                    f"manifest-matches-package-{name}",
                    sha256(read_bytes(package / name)) == manifest[field],
                    "冻结声明与作者包原文不同",
                )
    return manifest


def check_inputs(root: Path, replay: dict, report: Report):
    inputs, error = strict_load(
        read_bytes(root / INPUTS), INPUTS_FIELDS, "control/inputs.json", canonical_required=False
    )
    if inputs is None:
        report.record("inputs-strict", False, error)
        return None
    report.record("inputs-strict", True)
    report.record(
        "inputs-case", inputs["case_id"] == replay["family"]["case_id"], f"{inputs['case_id']}"
    )
    expected = {note["note_id"]: note for note in replay["notes"]}
    report.record(
        "inputs-note-count", len(inputs["notes"]) == len(expected), f"{len(inputs['notes'])}"
    )
    for digest in inputs["notes"]:
        note_id = digest["note_id"]
        want = expected.get(note_id)
        if want is None:
            report.record(f"inputs-note-{note_id}", False, "重放里没有这份笔记")
            continue
        view = next(item for item in replay["views"] if item["note"]["note_id"] == note_id)
        ok = (
            digest["path"] == want["path"]
            and digest["title"] == want["title"]
            and digest["raw_sha256"] == view["raw_sha256"]
            and digest["byte_length"] == view["byte_length"]
            and digest["heading_count"] == want["heading_count"]
        )
        report.record(
            f"inputs-note-{note_id}",
            ok,
            f"path/标题/摘要/字节数/标题数不符：{digest} vs {want['heading_count']}",
        )
    return inputs


def check_expected(package: Path, replay: dict, report: Report) -> None:
    path = package / "expected.json"
    if not path.is_file():
        report.record("expected-present", False, "作者包缺少 expected.json")
        return
    report.record("expected-present", True)
    expected = json.loads(read_bytes(path).decode("utf-8"))
    report.record(
        "expected-policy",
        expected["policy"] == POLICY and expected["case_id"] == replay["family"]["case_id"],
        f"policy={expected['policy']} case={expected['case_id']}",
    )
    report.record(
        "expected-counts", expected["counts"] == replay["counts"], f"{expected['counts']} vs {replay['counts']}"
    )
    by_id = {note["note_id"]: note for note in replay["notes"]}
    for want in expected["notes"]:
        got = by_id.get(want["note_id"])
        if got is None:
            report.record(f"expected-note-{want['note_id']}", False, "重放里没有这份笔记")
            continue
        ok = all(
            got[key] == want[key]
            for key in (
                "path",
                "title",
                "heading_count",
                "matched",
                "misordered",
                "extra",
                "missing_required",
                "omitted_optional",
                "empty",
                "consistent",
            )
        ) and got["sections"] == want["sections"]
        report.record(f"expected-note-{want['note_id']}", ok, f"{got} vs {want}")
    report.record(
        "expected-source-links",
        expected["source_links"] == [
            {
                "note_id": link["note_id"],
                "line": link["line"],
                "target": link["target"],
                "heading": link["heading"],
                "alias": link["alias"],
                "in_table": link["in_table"],
                "status": link["status"],
            }
            for link in replay["source_links"]
        ],
        "正文链接逐条不符",
    )
    report.record(
        "expected-generated",
        expected["generated"]
        == {
            "navigation": sum(1 for link in replay["generated"] if link["purpose"] == "navigation"),
            "concept": sum(1 for link in replay["generated"] if link["purpose"] == "concept"),
            "broken": replay["counts"]["links"]["generated_broken"],
        },
        f"{expected['generated']} vs {replay['counts']['links']}",
    )
    report.record(
        "expected-navigation-links",
        expected["navigation_links"]
        == [
            [link["owner"], link["target_path"]]
            for link in replay["generated"]
            if link["purpose"] == "navigation"
        ],
        "同族导航链接不符",
    )
    report.record(
        "expected-concepts",
        expected["concepts"]
        == [
            {
                "concept_id": item["concept_id"],
                "canonical": item["canonical"],
                "referenced_by": item["referenced_by"],
            }
            for item in replay["placements"]
        ],
        "概念位置表不符",
    )
    report.record(
        "expected-no-model",
        expected["model_calls"] == 0 and expected["network_requests"] == 0,
        "先验必须声明零模型零网络",
    )
    report.record(
        "expected-boundaries",
        expected["boundaries"]["scientific_status"] == "not_run"
        and expected["boundaries"]["uses_network"] is False
        and expected["boundaries"]["uses_model"] is False,
        "先验边界声明缺失",
    )


def check_rejections(package: Path, report: Report) -> None:
    """三个变体包必须各自被对应错误码整包拒绝。"""
    expected_path = package / "expected.json"
    if not expected_path.is_file():
        return
    expected = json.loads(read_bytes(expected_path).decode("utf-8"))
    for case in expected.get("rejections", []):
        variant = package / case["family"]
        if not variant.is_file():
            report.record(f"rejection-{case['case']}-present", False, "缺少变体作者包")
            continue
        # 变体包以 family.json 为底、只改一处，所以复制到临时目录后整体重放。
        import tempfile

        with tempfile.TemporaryDirectory(prefix="structure-audit-") as scratch:
            scratch_dir = Path(scratch)
            for name in ("concepts.json",):
                (scratch_dir / name).write_bytes(read_bytes(package / name))
            variant_doc = json.loads(read_bytes(variant).decode("utf-8"))
            (scratch_dir / "family.json").write_text(
                json.dumps(variant_doc, ensure_ascii=False), encoding="utf-8"
            )
            for note in variant_doc["notes"]:
                source = package / note["path"]
                target = scratch_dir / note["path"]
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(read_bytes(source))
            _, got_code, got_detail = replay_package(scratch_dir)
        report.record(
            f"rejection-{case['case']}",
            got_code == case["code"],
            f"期望 {case['code']}（exit {case['exit']}），实际 {got_code}：{got_detail}",
        )


def check_frozen(package: Path, report: Report) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, "作者包缺少 frozen-sha256.json")
        return
    report.record("frozen-present", True)
    frozen = json.loads(read_bytes(path).decode("utf-8"))
    recorded = frozen["files"]
    present = {
        str(item.relative_to(package))
        for item in package.rglob("*")
        if item.is_file() and item.name != FROZEN
    }
    report.record(
        "frozen-members", set(recorded.keys()) == present, f"{sorted(recorded.keys())} vs {sorted(present)}"
    )
    for name, entry in recorded.items():
        body = read_bytes(package / name)
        report.record(
            f"frozen-{name}",
            sha256(body) == entry["sha256"] and len(body) == entry["byte_length"],
            "冻结摘要或字节数不符",
        )


def check_output(output: Path, replay: dict, report: Report) -> None:
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"实际 {entries}")
    missing_files = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    for name in OUTPUT_ENTRIES:
        report.record(f"output-present-{name}", (output / name).is_file(), "缺少文件")
    if (output / OUTPUT_MANIFEST).is_file() is False:
        report.record("output-manifest-strict", False, "输出根缺少清单")
        return
    manifest, error = strict_load(
        read_bytes(output / OUTPUT_MANIFEST), OUTPUT_MANIFEST_FIELDS, "输出清单"
    )
    if manifest is None:
        report.record("output-manifest-strict", False, error)
    else:
        report.record("output-manifest-strict", True)
        report.record(
            "output-manifest-produced",
            manifest["policy"] == POLICY
            and manifest["producer"] == OUTPUT_PRODUCER
            and manifest["case_id"] == replay["family"]["case_id"],
            f"producer={manifest['producer']}",
        )
        report.record(
            "output-manifest-digests",
            not missing_files
            and manifest["structure_sha256"] == sha256(read_bytes(output / STRUCTURE))
            and manifest["links_sha256"] == sha256(read_bytes(output / LINKS))
            and manifest["report_sha256"] == sha256(read_bytes(output / REPORT))
            and manifest["navigation_sha256"] == sha256(read_bytes(output / NAVIGATION))
            and manifest["markdown_sha256"] == sha256(read_bytes(output / MARKDOWN)),
            "摘要不匹配" if not missing_files else f"缺少 {missing_files}",
        )
        report.record(
            "output-manifest-counts",
            manifest["counts"] == replay["counts"],
            f"{manifest['counts']} vs {replay['counts']}",
        )

    if missing_files:
        return
    structure = json.loads(read_bytes(output / STRUCTURE).decode("utf-8"))
    report.record("structure-counts", structure["counts"] == replay["counts"]["sections"], "结构计数不符")
    got_notes = []
    for note in structure["notes"]:
        got_notes.append(
            {
                key: note[key]
                for key in (
                    "note_id",
                    "path",
                    "title",
                    "headings",
                    "heading_count",
                    "sections",
                    "skeleton",
                    "extra_titles",
                    "matched",
                    "misordered",
                    "extra",
                    "missing_required",
                    "omitted_optional",
                    "empty",
                    "consistent",
                )
            }
        )
    want_notes = [
        {key: note[key] for key in got_notes[0].keys()} if got_notes else {} for note in replay["notes"]
    ]
    report.record("structure-notes", got_notes == want_notes, "章节树与重放不符")

    links = json.loads(read_bytes(output / LINKS).decode("utf-8"))
    report.record("links-counts", links["counts"] == replay["counts"]["links"], "链接计数不符")
    report.record("links-source", links["source"] == replay["source_links"], "正文链接与重放不符")
    report.record("links-generated", links["generated"] == replay["generated"], "生成链接与重放不符")
    broken = [link for link in links["generated"] if link["status"] != LINK_RESOLVED]
    report.record("links-no-broken", not broken, f"断链 {len(broken)} 条")

    payload = json.loads(read_bytes(output / REPORT).decode("utf-8"))
    report.record("report-counts", payload["counts"] == replay["counts"], "报告计数不符")
    report.record(
        "report-concepts",
        payload["concepts"]
        == [
            {
                "concept_id": item["concept_id"],
                "term": item["term"],
                "canonical": item["canonical"],
                "explaining": item["explaining"],
                "referenced_by": item["referenced_by"],
            }
            for item in replay["placements"]
        ],
        "概念表不符",
    )
    report.record(
        "report-zero-calls",
        payload["model_calls"] == 0 and payload["network_requests"] == 0,
        f"model_calls={payload['model_calls']} network_requests={payload['network_requests']}",
    )

    markdown = read_bytes(output / MARKDOWN).decode("utf-8")
    for heading in ("# 同类结构一致性与导航复习（", "## 结构一致性", "## 结构偏离明细", "## 概念的唯一解释位置", "## 链接", "## 边界"):
        report.record(f"markdown-{heading.strip('# ')[:6]}", heading in markdown, f"缺少 {heading}")
    for note in replay["notes"]:
        for title in note["extra_titles"]:
            report.record(
                f"markdown-extra-{note['note_id']}-{title}",
                title in markdown,
                f"多余节 {title} 没写进报告",
            )
    report.record(
        "markdown-no-created-at",
        "created_at" not in json.dumps(json.loads(read_bytes(output / OUTPUT_MANIFEST).decode("utf-8"))),
        "输出清单不能带时间戳",
    )

    navigation = read_bytes(output / NAVIGATION).decode("utf-8")
    report.record(
        "navigation-title",
        navigation.startswith(f"# 导航与复习（{replay['family']['family']}）"),
        "导航标题不符",
    )
    for note in replay["family"]["notes"]:
        report.record(
            f"navigation-{note['note_id']}",
            f"## {note['title']}" in navigation,
            f"导航缺少 {note['title']}",
        )
    report.record("navigation-no-exam-guess", "不虚构考试频次" in navigation, "缺少不猜考试频次的声明")


def freeze(package: Path) -> None:
    files: dict[str, dict[str, object]] = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        body = read_bytes(item)
        files[str(item.relative_to(package))] = {
            "sha256": sha256(body),
            "byte_length": len(body),
        }
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    sys.stdout.write(f"已冻结 {len(files)} 个成员\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Audit the structure-navigation contract.")
    parser.add_argument("root", nargs="?", help="已建立的结构根")
    parser.add_argument("--output", help="build 写出的输出根")
    parser.add_argument("--package", help="冻结的作者包目录")
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--freeze", action="store_true", help="重写冻结清单")
    args = parser.parse_args(argv)

    package = Path(args.package).resolve() if args.package else None
    if args.freeze:
        if package is None:
            sys.stderr.write("--freeze 需要 --package。\n")
            return 2
        freeze(package)
        return 0

    report = Report()
    replay = None
    if package is not None:
        if not (package / "family.json").is_file():
            sys.stderr.write(f"{package} 不是作者包（缺少 family.json）。\n")
            return 2
        replay, code, detail = replay_package(package)
        if replay is None:
            report.record("replay", False, f"{code}：{detail}")
        else:
            report.record("replay", True)
            check_expected(package, replay, report)
            check_rejections(package, report)
            check_frozen(package, report)
    if args.root:
        root = Path(args.root).resolve()
        if not root.is_dir():
            sys.stderr.write(f"{root} 不是目录。\n")
            return 2
        check_layout(root, report)
        check_manifest(root, package, report)
        if replay is not None:
            check_inputs(root, replay, report)
        if args.output:
            output = Path(args.output).resolve()
            if not output.is_dir():
                sys.stderr.write(f"{output} 不是目录。\n")
                return 2
            if replay is not None:
                check_output(output, replay, report)
    elif package is None:
        sys.stderr.write("至少要给一个结构根或 --package。\n")
        return 2

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
