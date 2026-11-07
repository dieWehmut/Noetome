#!/usr/bin/env python3
"""Independent read-only audit for the link-migration contract (B11-T01).

This script never imports the Rust product and never runs the CLI to decide
anything. It re-reads a frozen migration package and a migration root/output on
disk, replays link scanning, target resolution and object attribution from the
frozen bytes, and compares the replay with the JSON the product wrote.

Usage:
    python3 scripts/verify_link_migration.py <root> [--output DIR]
        [--package DIR] [--json] [--freeze]

Exit codes: 0 all checks passed, 1 failures, 2 usage error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "link-migration-v1"
OBJECTS_POLICY = "link-objects-v1"
AUTHOR_PRODUCER = "fixture_author"
APP_PRODUCER = "noetome-migrate"
OUTPUT_PRODUCER = "link_migration_output"
SCHEMA_VERSION = 1

SIDE_BEFORE = "before"
SIDE_AFTER = "after"

CHANGE_UNCHANGED = "unchanged"
CHANGE_LINKS_ONLY = "links_only"
CHANGE_PROMOTED = "promoted"
CHANGE_CREATED = "created"
CHANGES = (CHANGE_UNCHANGED, CHANGE_LINKS_ONLY, CHANGE_PROMOTED, CHANGE_CREATED)

MATCH_CANONICAL = "canonical"
MATCH_REDIRECT = "redirect"
LINK_RESOLVED = "resolved"

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
PLAN = "control/plan.json"
OBJECTS = "control/objects.json"
TEXT_SUFFIX = ".md"

LINKS = "links.json"
PLACEMENTS = "objects.json"
REPORT = "report.json"
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
    "plan_sha256",
    "objects_sha256",
    "inputs_sha256",
    "documents",
    "created_at",
)
INPUTS_FIELDS = ("policy", "schema_version", "case_id", "before_dir", "after_dir", "notes")
NOTE_DIGEST_FIELDS = (
    "side",
    "note_id",
    "path",
    "title",
    "counterpart",
    "change",
    "raw_sha256",
    "byte_length",
    "normalized_sha256",
    "span_count",
)
OUTPUT_MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "source_root",
    "source_inputs_sha256",
    "links_sha256",
    "placements_sha256",
    "report_sha256",
    "markdown_sha256",
    "counts",
)
COUNT_FIELDS = (
    "notes_before",
    "notes_after",
    "objects",
    "links_before",
    "links_after",
    "objects_referenced",
    "objects_unreferenced",
    "identity_violations",
    "broken_links",
    "canonical_links_after",
    "redirect_links_after",
    "table_links_before",
    "table_links_after",
    "escaped_alias_before",
    "escaped_alias_after",
    "unchanged_notes",
    "stripped_equal_notes",
    "promoted_notes",
    "created_notes",
    "rewritten_targets",
)
LINK_COUNT_FIELDS = (
    "links_before",
    "links_after",
    "resolved_before",
    "resolved_after",
    "canonical_links_after",
    "redirect_links_after",
    "broken_links",
    "table_links_before",
    "table_links_after",
    "escaped_alias_before",
    "escaped_alias_after",
    "rewritten_targets",
)
ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("after", "before", "inputs.json", "manifest.json", "objects.json", "plan.json")
OUTPUT_ENTRIES = ("links.json", "manifest.json", "objects.json", "report.json", "report.md")


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
        if value != "#":
            break
        hashes += 1
    if not 1 <= hashes <= 6:
        return 0
    rest = content[hashes:]
    if rest and not rest[0].isspace():
        return 0
    return hashes


def heading_title(line: str) -> str:
    content = line.lstrip(" ")
    hashes = 0
    for value in content:
        if value != "#":
            break
        hashes += 1
    return content[hashes:].strip()


def heading_titles(text: str) -> list[str]:
    out: list[str] = []
    for line in text.split("\n"):
        if heading_level(line) > 0:
            out.append(heading_title(line))
    return out


def is_table_row(line: str) -> bool:
    return line.strip().startswith("|")


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
        alias = inner[index + 1 :]
    if "#" in body:
        target, heading = body.split("#", 1)
        return target, heading, alias, escaped
    return body, None, alias, escaped


def strip_links(text: str) -> str:
    result: list[str] = []
    cursor = 0
    while cursor < len(text):
        if text[cursor : cursor + 2] == "[[":
            offset = text.find("]]", cursor + 2)
            if offset >= 0:
                cursor = offset + 2
                continue
        result.append(text[cursor])
        cursor += 1
    return "".join(result)


def resolve_note(notes: list[dict[str, object]], target: str):
    """返回 (笔记, 错误kind, 说明)；kind 为 ambiguous/missing。"""
    exact = [note for note in notes if note["path"] == target]
    if not exact:
        with_suffix = target + ".md"
        exact = [note for note in notes if note["path"] == with_suffix]
    if len(exact) == 1:
        return exact[0], "", ""
    if len(exact) > 1:
        return None, "ambiguous", f"目标 {target} 精确匹配到 {len(exact)} 份笔记，有歧义。"

    def stem_of(path: str) -> str:
        tail = path.rsplit("/", 1)[-1]
        return tail[: -len(".md")] if tail.endswith(".md") else tail

    by_stem = [
        note for note in notes if stem_of(note["path"]) == target or note["title"] == target
    ]
    if len(by_stem) == 1:
        return by_stem[0], "", ""
    if not by_stem:
        return None, "missing", f"目标 {target} 不在本迁移计划声明的笔记里。"
    return None, "ambiguous", f"目标 {target} 对应 {len(by_stem)} 份笔记，有歧义；请写完整路径。"


def location_key(note_id: str, heading: str | None) -> str:
    return f"{note_id}#{heading}" if heading is not None else f"{note_id}#"


def location_text(plan: dict, side: str, location: dict) -> str:
    specs = plan["before"] if side == SIDE_BEFORE else plan["after"]
    path = next(
        (spec["path"] for spec in specs if spec["note_id"] == location["note_id"]),
        location["note_id"],
    )
    heading = location["heading"]
    return f"[[{path}#{heading}]]" if heading is not None else f"[[{path}]]"


def load_sides(package: Path, plan: dict):
    sides: dict[str, list[dict[str, object]]] = {}
    for side, specs, directory in (
        (SIDE_BEFORE, plan["before"], plan["before_dir"]),
        (SIDE_AFTER, plan["after"], plan["after_dir"]),
    ):
        views: list[dict[str, object]] = []
        for spec in specs:
            path = package / directory / spec["path"]
            if not path.is_file():
                return None, "MIGRATE_INTEGRITY", f"无法读取笔记正文 {path}。"
            raw = read_bytes(path)
            views.append({"note": spec, "raw": raw, "text": normalize(raw)})
        sides[side] = views
    return sides, "", ""


def evaluate(plan: dict, objects: dict, sides: dict):
    """对象位置校验 + 两侧链接扫描。返回 (records, 错误码, 说明)。"""
    canonical_index: dict[str, int] = {}
    redirect_index: dict[str, int] = {}
    for position, object in enumerate(objects["objects"]):
        canonical_index[location_key(object["before"]["note_id"], object["before"]["heading"])] = position
        canonical_index[location_key(object["after"]["note_id"], object["after"]["heading"])] = position
        for location in object.get("redirects") or []:
            redirect_index[location_key(location["note_id"], location["heading"])] = position

    for object in objects["objects"]:
        for side, key in ((SIDE_BEFORE, "before"), (SIDE_AFTER, "after")):
            location = object[key]
            view = next(
                (item for item in sides[side] if item["note"]["note_id"] == location["note_id"]),
                None,
            )
            if view is None:
                return None, "MIGRATE_OBJECT_MISSING", f"对象 {object['object_id']} 的位置指向未声明的笔记。"
            heading = location["heading"]
            if heading is not None and heading not in heading_titles(view["text"]):
                return (
                    None,
                    "MIGRATE_OBJECT_MISSING",
                    f"知识对象 {object['object_id']} 声明的位置 {heading} 在笔记 {view['note']['path']} 里不存在这个标题；位置必须真实存在，不能靠猜。",
                )
        for location in object.get("redirects") or []:
            view = next(
                (item for item in sides[SIDE_AFTER] if item["note"]["note_id"] == location["note_id"]),
                None,
            )
            if view is None:
                return None, "MIGRATE_OBJECT_MISSING", f"对象 {object['object_id']} 的跳转位置指向未声明的笔记。"
            heading = location["heading"]
            if heading is not None and heading not in heading_titles(view["text"]):
                return (
                    None,
                    "MIGRATE_OBJECT_MISSING",
                    f"知识对象 {object['object_id']} 的跳转位置 {heading} 在笔记 {view['note']['path']} 里不存在。",
                )

    records: dict[str, list[dict[str, object]]] = {SIDE_BEFORE: [], SIDE_AFTER: []}
    for side in (SIDE_BEFORE, SIDE_AFTER):
        catalogue = [item["note"] for item in sides[side]]
        for view in sides[side]:
            for link in raw_links(view["text"]):
                target, heading, alias, escaped = split_link(link["inner"])
                if link["in_table"] and alias is not None and not escaped:
                    return (
                        None,
                        "MIGRATE_UNESCAPED_ALIAS",
                        f"笔记 {view['note']['path']} 第 {link['line']} 行在表格单元里写了未转义的别名竖线：{link['raw']}。",
                    )
                resolved, kind, detail = resolve_note(catalogue, target)
                if resolved is None:
                    code = "MIGRATE_AMBIGUOUS_TARGET" if kind == "ambiguous" else "MIGRATE_TARGET_MISSING"
                    return None, code, detail
                if heading is not None:
                    body = next(
                        item["text"]
                        for item in sides[side]
                        if item["note"]["note_id"] == resolved["note_id"]
                    )
                    if heading not in heading_titles(body):
                        return (
                            None,
                            "MIGRATE_TARGET_MISSING",
                            f"笔记 {view['note']['path']} 第 {link['line']} 行的链接指向 {resolved['path']}，但那份笔记里没有标题「{heading}」。",
                        )
                key = location_key(resolved["note_id"], heading)
                if key in canonical_index:
                    position, matched = canonical_index[key], MATCH_CANONICAL
                elif key in redirect_index:
                    position, matched = redirect_index[key], MATCH_REDIRECT
                else:
                    return (
                        None,
                        "MIGRATE_OBJECT_MISSING",
                        f"笔记 {view['note']['path']} 第 {link['line']} 行的链接 {link['raw']} 解析到 {resolved['path']}，但那个位置没有登记为知识对象。",
                    )
                records[side].append(
                    {
                        "side": side,
                        "note_id": view["note"]["note_id"],
                        "path": view["note"]["path"],
                        "line": link["line"],
                        "raw": link["raw"],
                        "target": target,
                        "heading": heading,
                        "alias": alias,
                        "in_table": link["in_table"],
                        "escaped_alias": escaped,
                        "object_id": objects["objects"][position]["object_id"],
                        "matched": matched,
                        "status": LINK_RESOLVED,
                    }
                )
    return records, "", ""


def load_package(package: Path, plan_file: str = "plan.json"):
    """读取并校验计划与对象表。返回 (plan, objects, 错误码, 说明)。"""
    path = package / plan_file
    if not path.is_file():
        return None, None, "MIGRATE_ARGUMENT", f"{package} 不是迁移包（缺少 {plan_file}）。"
    plan, detail = strict_load(
        read_bytes(path),
        (
            "policy",
            "schema_version",
            "producer",
            "case_id",
            "description",
            "before_dir",
            "after_dir",
            "objects",
            "before",
            "after",
            "promotions",
            "limits",
        ),
        "迁移计划",
        canonical_required=False,
    )
    if plan is None:
        return None, None, "MIGRATE_ARGUMENT", detail
    if plan["policy"] != POLICY:
        return None, None, "MIGRATE_ARGUMENT", f"计划 policy 必须是 {POLICY}。"
    objects_path = package / plan["objects"]
    if not objects_path.is_file():
        return None, None, "MIGRATE_ARGUMENT", f"知识对象表 {plan['objects']} 不存在。"
    objects, detail = strict_load(
        read_bytes(objects_path),
        ("policy", "schema_version", "case_id", "objects"),
        "知识对象表",
        canonical_required=False,
    )
    if objects is None:
        return None, None, "MIGRATE_ARGUMENT", detail
    if objects["policy"] != OBJECTS_POLICY:
        return None, None, "MIGRATE_ARGUMENT", f"对象表 policy 必须是 {OBJECTS_POLICY}。"
    if objects["case_id"] != plan["case_id"]:
        return None, None, "MIGRATE_ARGUMENT", "对象表 case_id 与计划不一致。"
    return plan, objects, "", ""


def replay_plan(package: Path, plan: dict, plan_file: str = "plan.json"):
    """按给定计划重放。返回 (错误码, 说明) 表示是否被整包拒绝。"""
    objects_path = package / plan["objects"]
    objects, detail = strict_load(
        read_bytes(objects_path),
        ("policy", "schema_version", "case_id", "objects"),
        "知识对象表",
        canonical_required=False,
    )
    if objects is None:
        return "MIGRATE_ARGUMENT", detail
    sides, code, detail = load_sides(package, plan)
    if sides is None:
        return code, detail
    records, code, detail = evaluate(plan, objects, sides)
    if records is None:
        return code, detail
    return "", ""


def replay_package(package: Path):
    """从冻结作者包重放迁移核对。返回 (replay, 错误码, 说明)。"""
    plan, objects, code, detail = load_package(package)
    if plan is None:
        return None, code, detail
    sides, code, detail = load_sides(package, plan)
    if sides is None:
        return None, code, detail
    records, code, detail = evaluate(plan, objects, sides)
    if records is None:
        return None, code, detail
    return (
        {
            "plan": plan,
            "objects": objects,
            "sides": sides,
            "before": records[SIDE_BEFORE],
            "after": records[SIDE_AFTER],
        },
        "",
        "",
    )


def replay_notes(replay: dict) -> list[dict[str, object]]:
    """重放 after 侧每篇笔记的核对结论。

    核对结论只对 after 侧成立：before 侧没有可判的「是否保持」，
    硬凑一行只会让读者以为每篇笔记都有一个可核对的结果。
    created 没有对照物，三个判定一律记 None（不适用），不是 False。
    """
    plan = replay["plan"]
    before = {item["note"]["note_id"]: item for item in replay["sides"][SIDE_BEFORE]}
    after = {item["note"]["note_id"]: item for item in replay["sides"][SIDE_AFTER]}
    out: list[dict[str, object]] = []
    for spec in plan["after"]:
        change = spec.get("change")
        view = after[spec["note_id"]]
        bytes_equal = None
        stripped_equal = None
        promotion_satisfied = None
        if change in (CHANGE_UNCHANGED, CHANGE_LINKS_ONLY, CHANGE_PROMOTED):
            counterpart = before.get(spec.get("counterpart"))
            if counterpart is None:
                raise KeyError(f"迁移后笔记 {spec['note_id']} 的对照笔记不存在。")
            bytes_equal = counterpart["text"] == view["text"]
            if change == CHANGE_PROMOTED:
                stripped_equal = False
                promotion_satisfied = spec.get("counterpart") in {
                    item["note_id"] for item in plan["before"]
                }
            else:
                stripped_equal = strip_links(counterpart["text"]) == strip_links(view["text"])
        out.append(
            {
                "note_id": spec["note_id"],
                "path": spec["path"],
                "counterpart": spec.get("counterpart"),
                "change": change,
                "bytes_equal": bytes_equal,
                "stripped_equal": stripped_equal,
                "links_links_only_ok": stripped_equal if change == CHANGE_LINKS_ONLY else None,
                "promotion_satisfied": promotion_satisfied,
            }
        )
    return out


def replay_expected(replay: dict) -> dict[str, object]:
    """按冻结包重放计数与每对象归属，供与 expected.json 对照。"""
    plan = replay["plan"]
    objects = replay["objects"]
    before = replay["before"]
    after = replay["after"]

    placements: list[dict[str, object]] = []
    identity_violations = 0
    for object in objects["objects"]:
        links_before = [link for link in before if link["object_id"] == object["object_id"]]
        links_after = [link for link in after if link["object_id"] == object["object_id"]]
        canonical = [link for link in links_after if link["matched"] == MATCH_CANONICAL]
        redirect = [link for link in links_after if link["matched"] == MATCH_REDIRECT]
        referenced_before = bool(links_before)
        referenced_after = bool(links_after)
        identity_preserved = (not referenced_before) or referenced_after
        if not identity_preserved:
            identity_violations += 1
        placements.append(
            {
                "object_id": object["object_id"],
                "kind": object["kind"],
                "term": object["term"],
                "before_target": location_text(plan, SIDE_BEFORE, object["before"]),
                "after_target": location_text(plan, SIDE_AFTER, object["after"]),
                "redirect_targets": [
                    location_text(plan, SIDE_AFTER, location)
                    for location in (object.get("redirects") or [])
                ],
                "links_before": len(links_before),
                "links_after": len(links_after),
                "links_after_canonical": len(canonical),
                "links_after_redirect": len(redirect),
                "referenced_before": referenced_before,
                "referenced_after": referenced_after,
                "identity_preserved": identity_preserved,
            }
        )

    path_of = {
        SIDE_BEFORE: {spec["note_id"]: spec["path"] for spec in plan["before"]},
        SIDE_AFTER: {spec["note_id"]: spec["path"] for spec in plan["after"]},
    }

    def moved(object: dict) -> bool:
        before_path = path_of[SIDE_BEFORE].get(object["before"]["note_id"])
        after_path = path_of[SIDE_AFTER].get(object["after"]["note_id"])
        return before_path != after_path or object["before"]["heading"] != object["after"]["heading"]

    rewritten_targets = 0
    for link in after:
        if link["matched"] != MATCH_CANONICAL:
            continue
        object = next(
            (item for item in objects["objects"] if item["object_id"] == link["object_id"]), None
        )
        if object is not None and moved(object):
            rewritten_targets += 1

    unreferenced = [item for item in placements if not item["referenced_after"] and not item["referenced_before"]]

    counts = {
        "notes_before": len(plan["before"]),
        "notes_after": len(plan["after"]),
        "objects": len(objects["objects"]),
        "links_before": len(before),
        "links_after": len(after),
        "resolved_before": sum(1 for link in before if link["status"] == LINK_RESOLVED),
        "resolved_after": sum(1 for link in after if link["status"] == LINK_RESOLVED),
        "objects_referenced": len(placements) - len(unreferenced),
        "objects_unreferenced": len(unreferenced),
        "identity_violations": identity_violations,
        "broken_links": 0,
        "canonical_links_after": sum(1 for link in after if link["matched"] == MATCH_CANONICAL),
        "redirect_links_after": sum(1 for link in after if link["matched"] == MATCH_REDIRECT),
        "table_links_before": sum(1 for link in before if link["in_table"]),
        "table_links_after": sum(1 for link in after if link["in_table"]),
        "escaped_alias_before": sum(1 for link in before if link["escaped_alias"]),
        "escaped_alias_after": sum(1 for link in after if link["escaped_alias"]),
        "unchanged_notes": sum(1 for spec in plan["after"] if spec.get("change") == CHANGE_UNCHANGED),
        "stripped_equal_notes": sum(1 for spec in plan["after"] if spec.get("change") == CHANGE_LINKS_ONLY),
        "promoted_notes": sum(1 for spec in plan["after"] if spec.get("change") == CHANGE_PROMOTED),
        "created_notes": sum(1 for spec in plan["after"] if spec.get("change") == CHANGE_CREATED),
        "rewritten_targets": rewritten_targets,
    }
    return {"counts": counts, "placements": placements}


def check_expected(package: Path, replay: dict, report: Report) -> None:
    expected_path = package / "expected.json"
    if not expected_path.is_file():
        report.record("expected-present", False, "缺少 expected.json。")
        return
    expected = json.loads(expected_path.read_text(encoding="utf-8"))
    replay_prior = replay_expected(replay)

    for key in COUNT_FIELDS:
        report.record(
            f"expected-count:{key}",
            replay_prior["counts"][key] == expected["counts"][key],
            f"重放 {replay_prior['counts'][key]}、期望 {expected['counts'][key]}",
        )

    wish = {item["object_id"]: item for item in expected["objects"]}
    for item in replay_prior["placements"]:
        prior = wish.get(item["object_id"])
        if prior is None:
            report.record("expected-object", False, f"期望里没有对象 {item['object_id']}。")
            continue
        report.record(
            f"expected-object:{item['object_id']}:target",
            item["after_target"] == prior["expected_target"],
            f"重放 {item['after_target']}、期望 {prior['expected_target']}",
        )
        for key in (
            "kind",
            "term",
            "links_before",
            "links_after",
            "links_after_canonical",
            "links_after_redirect",
            "referenced_before",
            "referenced_after",
            "identity_preserved",
        ):
            report.record(
                f"expected-object:{item['object_id']}:{key}",
                item[key] == prior[key],
                f"重放 {item[key]}、期望 {prior[key]}",
            )

    prior_notes = expected.get("notes") or []
    notes = replay_notes(replay)
    report.record(
        "expected-notes:length",
        len(notes) == len(prior_notes),
        f"重放 {len(notes)}、期望 {len(prior_notes)}",
    )
    for index, note in enumerate(notes[: len(prior_notes)]):
        prior = prior_notes[index]
        for key in (
            "note_id",
            "path",
            "counterpart",
            "change",
            "bytes_equal",
            "stripped_equal",
            "promotion_satisfied",
        ):
            report.record(
                f"expected-notes:{index}:{key}",
                note[key] == prior.get(key),
                f"重放 {note[key]}、期望 {prior.get(key)}",
            )
        if "links_links_only_ok" in prior:
            report.record(
                f"expected-notes:{index}:links_links_only_ok",
                note["links_links_only_ok"] == prior["links_links_only_ok"],
                f"重放 {note['links_links_only_ok']}、期望 {prior['links_links_only_ok']}",
            )

    for side, prior_key, records in (
        (SIDE_BEFORE, "before_links", replay["before"]),
        (SIDE_AFTER, "after_links", replay["after"]),
    ):
        priors = expected[prior_key]
        report.record(
            f"expected-links:{side}:length",
            len(records) == len(priors),
            f"重放 {len(records)}、期望 {len(priors)}",
        )
        for index, (link, prior) in enumerate(zip(records, priors)):
            for key, prior_field in (
                ("note_id", "note_id"),
                ("path", "path"),
                ("line", "line"),
                ("raw", "raw"),
                ("target", "target"),
                ("heading", "heading"),
                ("alias", "alias"),
                ("in_table", "in_table"),
                ("escaped_alias", "escaped"),
                ("object_id", "object_id"),
                ("matched", "match"),
                ("status", "status"),
            ):
                report.record(
                    f"expected-links:{side}:{index}:{prior_field}",
                    link[key] == prior[prior_field],
                    f"重放 {link[key]}、期望 {prior[prior_field]}",
                )

    report.record(
        "expected-zero-model",
        expected.get("model_calls") == 0 and expected.get("network_requests") == 0,
        "先验必须声明零模型零网络。",
    )
    boundaries = expected.get("boundaries") or {}
    report.record(
        "expected-boundaries",
        boundaries.get("scientific_status") == "not_run",
        f"scientific_status={boundaries.get('scientific_status')}",
    )


def check_rejections(package: Path, report: Report) -> None:
    variants = [
        ("plan-ambiguous.json", "MIGRATE_AMBIGUOUS_TARGET"),
        ("plan-missing-target.json", "MIGRATE_TARGET_MISSING"),
        ("plan-unescaped-table.json", "MIGRATE_UNESCAPED_ALIAS"),
        ("plan-unsafe.json", "MIGRATE_OBJECT_MISSING"),
    ]
    for name, code in variants:
        path = package / name
        if not path.is_file():
            report.record(f"rejection:{name}", False, "变体计划不存在。")
            continue
        plan, detail = strict_load(
            read_bytes(path),
            (
                "policy",
                "schema_version",
                "producer",
                "case_id",
                "description",
                "before_dir",
                "after_dir",
                "objects",
                "before",
                "after",
                "promotions",
                "limits",
            ),
            name,
            canonical_required=False,
        )
        if plan is None:
            report.record(f"rejection:{name}", False, detail)
            continue
        got, message = replay_plan(package, plan, name)
        report.record(
            f"rejection:{name}",
            got == code,
            f"期望 {code}、实际 {got or '未拒绝'}（{message}）",
        )




def check_layout(root: Path, report: Report) -> None:
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control/。")
        return
    names = sorted(item.name for item in control.iterdir())
    report.record("layout-control-entries", names == list(CONTROL_ENTRIES), f"control 条目 {names}")
    for side in (SIDE_BEFORE, SIDE_AFTER):
        directory = control / side
        report.record(f"layout-{side}-dir", directory.is_dir(), f"{directory} 不是目录。")
    links = [path for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-no-links", not links, f"根下有符号链接：{links}")


def check_manifest(root: Path, report: Report) -> dict | None:
    path = root / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 control/manifest.json。")
        return None
    payload = read_bytes(path)
    manifest, detail = strict_load(payload, MANIFEST_FIELDS, "冻结清单")
    if manifest is None:
        report.record("manifest-strict", False, detail)
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, f"policy={manifest['policy']}")
    report.record(
        "manifest-producer", manifest["producer"] == APP_PRODUCER, f"producer={manifest['producer']}"
    )
    report.record(
        "manifest-schema", manifest["schema_version"] == SCHEMA_VERSION, f"schema={manifest['schema_version']}"
    )
    report.record(
        "manifest-root",
        Path(manifest["root"]).resolve() == root.resolve(),
        f"清单 root={manifest['root']}、实际 {root}",
    )
    plan_path = root / PLAN
    objects_path = root / OBJECTS
    inputs_path = root / INPUTS
    report.record(
        "manifest-plan-digest",
        plan_path.is_file() and sha256(read_bytes(plan_path)) == manifest["plan_sha256"],
        "计划摘要不匹配。",
    )
    report.record(
        "manifest-objects-digest",
        objects_path.is_file() and sha256(read_bytes(objects_path)) == manifest["objects_sha256"],
        "对象表摘要不匹配。",
    )
    report.record(
        "manifest-inputs-digest",
        inputs_path.is_file() and sha256(read_bytes(inputs_path)) == manifest["inputs_sha256"],
        "输入清单摘要不匹配。",
    )

    expected = {(item["side"], item["note_id"]): item for item in manifest["documents"]}
    seen: set[tuple[str, str]] = set()
    for side in (SIDE_BEFORE, SIDE_AFTER):
        directory = root / CONTROL / side
        if not directory.is_dir():
            report.record(f"manifest-{side}-dir", False, f"缺少 {directory}。")
            continue
        files = sorted(item.name for item in directory.iterdir())
        wanted = sorted(
            f"{note_id}{TEXT_SUFFIX}"
            for (digest_side, note_id) in expected
            if digest_side == side
        )
        report.record(f"manifest-{side}-files", files == wanted, f"{side} 文件 {files}、清单 {wanted}")
        for name in files:
            note_id = name[: -len(TEXT_SUFFIX)]
            digest = expected.get((side, note_id))
            if digest is None:
                report.record(f"manifest-{side}-unknown:{name}", False, "清单里没有这份笔记。")
                continue
            body = read_bytes(directory / name)
            report.record(
                f"manifest-{side}-digest:{note_id}",
                len(body) == digest["byte_length"] and sha256(body) == digest["normalized_sha256"],
                "正文长度或摘要与清单不符。",
            )
            seen.add((side, note_id))
    report.record(
        "manifest-documents",
        len(seen) == len(expected) and len(manifest["documents"]) == len(seen),
        f"冻结文档 {len(seen)} 份、清单 {len(manifest['documents'])} 条。",
    )
    return manifest


def check_inputs(root: Path, replay: dict, report: Report) -> None:
    path = root / INPUTS
    if not path.is_file():
        report.record("inputs-present", False, "缺少 control/inputs.json。")
        return
    inputs, detail = strict_load(read_bytes(path), INPUTS_FIELDS, "输入清单")
    if inputs is None:
        report.record("inputs-strict", False, detail)
        return
    report.record("inputs-strict", True)
    plan = replay["plan"]
    report.record("inputs-case", inputs["case_id"] == plan["case_id"], f"case_id={inputs['case_id']}")
    report.record(
        "inputs-dirs",
        inputs["before_dir"] == plan["before_dir"] and inputs["after_dir"] == plan["after_dir"],
        "两侧目录与计划不一致。",
    )
    wanted = {(SIDE_BEFORE, spec["note_id"]) for spec in plan["before"]} | {
        (SIDE_AFTER, spec["note_id"]) for spec in plan["after"]
    }
    got = {(note["side"], note["note_id"]) for note in inputs["notes"]}
    report.record("inputs-notes", wanted == got, f"输入清单笔记 {sorted(got)}")
    for note in inputs["notes"]:
        report.record(
            f"inputs-fields:{note['side']}:{note['note_id']}",
            tuple(sorted(note.keys())) == tuple(sorted(NOTE_DIGEST_FIELDS)),
            "输入清单字段集合不符。",
        )


def check_frozen(package: Path, report: Report) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, "缺少 frozen-sha256.json。")
        return
    frozen = json.loads(path.read_text(encoding="utf-8"))
    actual = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        body = read_bytes(item)
        actual[str(item.relative_to(package))] = {"sha256": sha256(body), "byte_length": len(body)}
    report.record(
        "frozen-members",
        sorted(actual) == sorted(frozen["files"]),
        f"实际成员 {sorted(actual)}、清单 {sorted(frozen['files'])}",
    )
    for name, digest in frozen["files"].items():
        if name not in actual:
            continue
        report.record(
            f"frozen-digest:{name}",
            actual[name]["sha256"] == digest["sha256"]
            and actual[name]["byte_length"] == digest["byte_length"],
            "冻结成员摘要或长度不符。",
        )


def check_output(output: Path, replay: dict, report: Report) -> None:
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"实际 {entries}")
    missing_files = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    for name in OUTPUT_ENTRIES:
        report.record(f"output-present-{name}", (output / name).is_file(), "缺少文件")
    if not (output / OUTPUT_MANIFEST).is_file():
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
            and manifest["case_id"] == replay["plan"]["case_id"],
            f"producer={manifest['producer']}",
        )
        report.record(
            "output-manifest-digests",
            not missing_files
            and manifest["links_sha256"] == sha256(read_bytes(output / LINKS))
            and manifest["placements_sha256"] == sha256(read_bytes(output / PLACEMENTS))
            and manifest["report_sha256"] == sha256(read_bytes(output / REPORT))
            and manifest["markdown_sha256"] == sha256(read_bytes(output / MARKDOWN)),
            "摘要不匹配" if not missing_files else f"缺少 {missing_files}",
        )
        report.record(
            "output-manifest-inputs-digest",
            manifest["source_inputs_sha256"] == sha256(read_bytes(Path(replay["root"]) / INPUTS))
            if replay.get("root")
            else True,
            "输入清单摘要不匹配",
        )

    if missing_files:
        return

    prior = replay_expected(replay)
    links = json.loads(read_bytes(output / LINKS).decode("utf-8"))
    report.record(
        "links-counts",
        links["counts"] == {key: prior["counts"][key] for key in LINK_COUNT_FIELDS},
        "链接计数与重放不符",
    )
    report.record("links-before", links["before"] == replay["before"], "迁移前链接与重放不符")
    report.record("links-after", links["after"] == replay["after"], "迁移后链接与重放不符")
    broken = [link for link in links["after"] if link["status"] != LINK_RESOLVED]
    report.record("links-no-broken", not broken and links["counts"]["broken_links"] == 0, f"断链 {len(broken)} 条")

    placements = json.loads(read_bytes(output / PLACEMENTS).decode("utf-8"))
    report.record(
        "placements-objects", placements["objects"] == prior["placements"], "知识对象归属与重放不符"
    )
    report.record(
        "placements-counts",
        placements["counts"]["identity_violations"] == 0
        and placements["counts"]["objects"] == len(prior["placements"]),
        f"counts={placements['counts']}",
    )

    payload = json.loads(read_bytes(output / REPORT).decode("utf-8"))
    report.record(
        "report-counts",
        payload["counts"] == {key: prior["counts"][key] for key in COUNT_FIELDS},
        "报告计数与重放不符",
    )
    report.record(
        "report-zero-calls",
        payload["model_calls"] == 0 and payload["network_requests"] == 0,
        f"model_calls={payload['model_calls']} network_requests={payload['network_requests']}",
    )
    sides = {note["note_id"]: note for note in payload["notes"]}
    report.record(
        "report-notes-after-only",
        all(note["side"] == SIDE_AFTER for note in payload["notes"]),
        "报告里的核对结论只能对 after 侧成立",
    )
    report.record(
        "report-notes-complete",
        len(payload["notes"]) == len(replay["plan"]["after"]),
        f"报告笔记 {len(payload['notes'])} 条、计划 after {len(replay['plan']['after'])} 份",
    )
    for spec in replay["plan"]["after"]:
        note = sides.get(spec["note_id"])
        report.record(
            f"report-note:{spec['note_id']}",
            note is not None and note["change"] == spec.get("change"),
            "报告里的变更类型与计划不符",
        )

    markdown = read_bytes(output / MARKDOWN).decode("utf-8")
    for heading in ("# 链接迁移报告（", "## 计数", "## 知识对象身份", "## 迁移后引用", "## 笔记核对", "## 边界"):
        report.record(f"markdown-{heading.strip('# （')[:6]}", heading in markdown, f"缺少 {heading}")
    report.record(
        "markdown-boundary",
        "不判断内容质量与科学正确性" in markdown,
        "缺少不判断科学正确性的边界声明",
    )
    report.record(
        "markdown-redirect-visible",
        "redirect" in markdown and any(
            link["matched"] == MATCH_REDIRECT for link in replay["after"]
        )
        == ("redirect" in markdown),
        "跳转位置必须在报告里可见",
    )


def freeze(package: Path) -> None:
    files: dict[str, dict[str, object]] = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.name == FROZEN:
            continue
        body = read_bytes(item)
        files[str(item.relative_to(package))] = {"sha256": sha256(body), "byte_length": len(body)}
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    sys.stdout.write(f"已冻结 {len(files)} 个成员\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Audit the link-migration contract.")
    parser.add_argument("root", nargs="?", help="已建立的迁移根")
    parser.add_argument("--output", help="build 写出的输出根")
    parser.add_argument("--package", help="冻结的迁移包目录")
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
        if not (package / "plan.json").is_file():
            sys.stderr.write(f"{package} 不是迁移包（缺少 plan.json）。\n")
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
        manifest = check_manifest(root, report)
        if replay is not None and manifest is not None:
            replay["root"] = str(root)
            check_inputs(root, replay, report)
        if args.output:
            output = Path(args.output).resolve()
            if not output.is_dir():
                sys.stderr.write(f"{output} 不是目录。\n")
                return 2
            if replay is not None:
                check_output(output, replay, report)
    elif package is None:
        sys.stderr.write("至少要给一个迁移根或 --package。\n")
        return 2

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
