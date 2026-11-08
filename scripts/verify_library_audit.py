#!/usr/bin/env python3
"""Independent read-only auditor for `noetome audit` roots and outputs.

This script never imports the Rust product and never runs the CLI to decide
anything. It re-derives headings, markers, links, explanations and recalls from
the frozen bytes, then compares the derived numbers with the frozen prior in
`expected.json` and with whatever the product wrote.

Usage:
    verify_library_audit.py <root> [--output DIR] [--package DIR] [--json] [--freeze]

Exit codes:
    0  every check passed
    1  at least one check failed
    2  usage error
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "library-audit-v1"
GOLD_POLICY = "library-gold-v1"
APP_PRODUCER = "noetome-audit"
OUTPUT_PRODUCER = "library_audit_output"
AUTHOR_PRODUCER = "fixture_author"
SCHEMA_VERSION = 1
MARKER_PREFIX = "> 完整解释："
GENERAL_RECALL_PERCENT = 98

STATUS_RESOLVED = "resolved"
STATUS_BROKEN = "broken"
STATUS_AMBIGUOUS = "ambiguous"

BASIS_EXACT = "exact_path"
BASIS_BARE = "bare_name"
BASIS_DUP_NOTE = "duplicate_note_name"
BASIS_PATH_MISSING = "path_missing"
BASIS_DUP_HEADING = "duplicate_heading"
BASIS_HEADING_MISSING = "heading_missing"

FINDING_BROKEN = "broken_link"
FINDING_AMBIGUOUS = "ambiguous_link"
FINDING_DUPLICATE = "duplicate_explanation"
FINDING_UNEXPLAINED = "concept_unexplained"
FINDING_CANONICAL = "canonical_position_mismatch"
FINDING_ATOM = "atom_missing"
FINDING_RECALL = "recall_below_threshold"

CONTROL = "control"
MANIFEST = "control/manifest.json"
INPUTS = "control/inputs.json"
PROJECT = "control/project.json"
GOLD = "control/gold.json"
NOTES_DIR = "control/notes"
LINKS = "links.json"
EXPLANATIONS = "explanations.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("gold.json", "inputs.json", "manifest.json", "notes", "project.json")
OUTPUT_ENTRIES = (LINKS, EXPLANATIONS, REPORT, MARKDOWN, OUTPUT_MANIFEST)

PROJECT_FIELDS = {
    "policy", "schema_version", "producer", "case_id", "description",
    "gold", "notes", "limits",
}
NOTE_SPEC_FIELDS = {"note_id", "path", "title"}
LIMIT_FIELDS = {"max_notes", "max_atoms", "max_links"}
GOLD_FIELDS = {
    "policy", "schema_version", "producer", "case_id", "description",
    "concepts", "atoms",
}
CONCEPT_SPEC_FIELDS = {"concept_id", "canonical"}
ATOM_SPEC_FIELDS = {"atom_id", "concept_id", "key", "note_id", "heading"}
MANIFEST_FIELDS = {
    "policy", "producer", "schema_version", "case_id", "root", "root_identity",
    "parent_identity", "control_identity", "manifest_identity", "notes_identity",
    "inputs_identity", "project_sha256", "gold_sha256", "inputs_sha256", "created_at",
}
IDENTITY_FIELDS = {"platform", "device", "inode"}
INPUTS_FIELDS = {"policy", "schema_version", "case_id", "gold_sha256", "notes"}
NOTE_DIGEST_FIELDS = {"note_id", "path", "title", "sha256", "byte_length"}
COUNT_FIELDS = (
    "notes", "headings", "links", "links_resolved", "links_broken", "links_ambiguous",
    "links_embedded", "links_in_table", "links_escaped_alias", "concepts", "markers",
    "atoms", "atoms_key", "atoms_general", "atoms_recalled", "atoms_missing",
    "duplicate_concepts", "findings", "model_calls", "network_requests",
)
RATIO_FIELDS = {"numerator", "denominator", "applicable", "display", "note"}
LINK_FIELDS = {
    "note_id", "path", "line", "raw", "target", "heading", "alias", "embed",
    "in_table", "escaped_alias", "status", "basis", "resolved_note", "resolved_heading",
}
MARKER_FIELDS = {"note_id", "heading", "line"}
CONCEPT_RECORD_FIELDS = {
    "concept_id", "canonical", "markers", "canonical_note", "canonical_heading", "unique",
}
ATOM_RECORD_FIELDS = {"atom_id", "concept_id", "key", "note_id", "heading", "recalled", "reason"}
LINKS_FILE_FIELDS = {"policy", "case_id", "links"}
EXPLANATIONS_FILE_FIELDS = {"policy", "case_id", "concepts", "atoms"}
REPORT_FIELDS = {
    "policy", "schema_version", "case_id", "description", "verdict", "counts",
    "availability", "key_recall", "general_recall", "findings", "model_calls",
    "network_requests",
}
OUTPUT_MANIFEST_FIELDS = {
    "policy", "producer", "schema_version", "case_id", "source_root",
    "source_inputs_sha256", "links_sha256", "explanations_sha256", "report_sha256",
    "markdown_sha256", "verdict", "counts",
}


class Report:
    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> None:
        self.checks.append((name, bool(ok), detail))

    def warn(self, name: str, detail: str = "") -> None:
        self.checks.append((name, True, detail))

    @property
    def failures(self) -> list[tuple[str, bool, str]]:
        return [check for check in self.checks if not check[1]]

    def dump(self, as_json: bool) -> None:
        if as_json:
            payload = {
                "total": len(self.checks),
                "failures": [name for name, ok, _ in self.checks if not ok],
                "checks": [
                    {"name": name, "ok": ok, "detail": detail}
                    for name, ok, detail in self.checks
                ],
            }
            print(json.dumps(payload, ensure_ascii=False, indent=2))
            return
        for name, ok, detail in self.checks:
            if not ok:
                print(f"[失败] {name}：{detail}")
        failed = len(self.failures)
        print(f"合计 {len(self.checks)} 项检查：失败 {failed}、提示 0")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def canonical(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(path: Path, fields: set[str] | None, label: str, canonical_required: bool = True):
    """Load one canonical JSON object, or raise ValueError with a readable reason."""
    text = path.read_text(encoding="utf-8")
    value = json.loads(text)
    if not isinstance(value, dict):
        raise ValueError(f"{label} 不是 JSON 对象")
    if fields is not None and set(value) != fields:
        raise ValueError(f"{label} 字段集合是 {sorted(value)}，期望 {sorted(fields)}")
    if canonical_required:
        expected = canonical(value)
        if text not in (expected, expected + "\n"):
            raise ValueError(f"{label} 不是规范编码")
    return value


def normalize(text: str) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n")


def heading_level(line: str) -> int:
    content = line.lstrip(" ")
    indentation = len(line) - len(content)
    if indentation > 3:
        return 0
    hashes = 0
    for char in content:
        if char == "#":
            hashes += 1
        else:
            break
    if not 1 <= hashes <= 6:
        return 0
    rest = content[hashes:]
    if rest and not rest[0].isspace():
        return 0
    return hashes


def heading_title(line: str) -> str:
    content = line.strip()
    content = content.lstrip("#")
    return content.strip()


def sections(text: str) -> list[dict]:
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    total = len(lines)
    found: list[dict] = []
    for index, line in enumerate(lines):
        level = heading_level(line)
        if level == 0:
            continue
        found.append(
            {
                "line": index + 1,
                "level": level,
                "title": heading_title(line),
                "end_line": total,
            }
        )
    for position, section in enumerate(found):
        following = found[position + 1]["line"] if position + 1 < len(found) else total + 1
        section["end_line"] = max(section["line"], following - 1)
    return found


def markers(text: str, known: set[str]) -> list[dict]:
    found: list[dict] = []
    mapping = sections(text)
    for index, line in enumerate(text.split("\n")):
        number = index + 1
        stripped = line.strip()
        if not stripped.startswith(MARKER_PREFIX):
            continue
        concept_id = stripped[len(MARKER_PREFIX):].strip()
        if concept_id not in known:
            raise ValueError(f"未知的完整解释标记：{concept_id}")
        heading = None
        for section in mapping:
            if section["line"] <= number <= section["end_line"]:
                heading = section["title"]
        found.append({"concept_id": concept_id, "line": number, "heading": heading})
    return found


def scan_links(text: str) -> list[dict]:
    found: list[dict] = []
    for index, line in enumerate(text.split("\n")):
        number = index + 1
        in_table = line.lstrip().startswith("|")
        cursor = 0
        while cursor + 1 < len(line):
            embed = line[cursor] == "!" and line[cursor + 1] == "["
            opened = (
                line[cursor + 2] == "[" if embed and cursor + 2 < len(line)
                else line[cursor] == "[" and line[cursor + 1] == "["
            )
            if not opened:
                cursor += 1
                continue
            start = cursor + 3 if embed else cursor + 2
            end = start
            closed = False
            while end + 1 < len(line):
                if line[end] == "]" and line[end + 1] == "]":
                    closed = True
                    break
                end += 1
            if not closed:
                break
            found.append(
                parse_link(line[start:end], number, embed, in_table, line[cursor:end + 2])
            )
            cursor = end + 2
    return found


def parse_link(inner: str, line: int, embed: bool, in_table: bool, raw: str) -> dict:
    alias = None
    escaped = False
    body = inner
    marker = inner.find("\\|")
    if marker >= 0:
        body = inner[:marker]
        alias = inner[marker + 2:]
        escaped = True
    else:
        plain = inner.find("|")
        if plain >= 0:
            body = inner[:plain]
            alias = inner[plain + 1:]
    heading = None
    target = body
    if "#" in body:
        target, _, rest = body.partition("#")
        heading = rest.strip() or None
    target = target.strip()
    if target.endswith(".md"):
        target = target[: -len(".md")]
    return {
        "line": line,
        "raw": raw,
        "target": target,
        "heading": heading,
        "alias": alias,
        "embed": embed,
        "in_table": in_table,
        "escaped_alias": escaped,
    }


def note_paths(project: dict) -> dict[str, dict]:
    return {note["note_id"]: note for note in project["notes"]}


def resolve_note(target: str, project: dict) -> tuple[dict | None, str]:
    for note in project["notes"]:
        path = note["path"]
        if target in (path, path[: -len(".md")] if path.endswith(".md") else path, note["note_id"]):
            return note, BASIS_EXACT
    if "/" in target:
        return None, BASIS_PATH_MISSING
    matches = [
        note for note in project["notes"]
        if note["path"].rsplit("/", 1)[-1].removesuffix(".md") == target
    ]
    if len(matches) == 1:
        return matches[0], BASIS_BARE
    if len(matches) > 1:
        return None, BASIS_DUP_NOTE
    return None, BASIS_PATH_MISSING


def resolve_heading(note_id: str, heading: str | None, bodies: dict[str, str]) -> tuple[bool, str, str | None]:
    if heading is None:
        return True, BASIS_EXACT, None
    titles = [section["title"] for section in sections(bodies[note_id])]
    count = titles.count(heading)
    if count == 1:
        return True, BASIS_EXACT, heading
    if count == 0:
        return False, BASIS_HEADING_MISSING, None
    return False, BASIS_DUP_HEADING, None


def ratio(numerator: int, denominator: int, note: str) -> dict:
    applicable = denominator > 0
    display = f"{numerator / denominator * 100:.1f}%" if applicable else "不适用"
    return {
        "numerator": numerator,
        "denominator": denominator,
        "applicable": applicable,
        "display": display,
        "note": note,
    }


def evaluate(project: dict, gold: dict, bodies: dict[str, str]) -> dict:
    if gold["case_id"] != project["case_id"]:
        raise ValueError("金标准的 case_id 与工程声明不一致")
    known = {concept["concept_id"] for concept in gold["concepts"]}
    for atom in gold["atoms"]:
        if atom["note_id"] not in {note["note_id"] for note in project["notes"]}:
            raise ValueError(f"原子 {atom['atom_id']} 指向未声明的笔记 {atom['note_id']}")

    all_markers: list[dict] = []
    per_note_markers: dict[str, list[dict]] = {}
    for note in project["notes"]:
        found = markers(bodies[note["note_id"]], known)
        per_note_markers[note["note_id"]] = found
        for marker in found:
            all_markers.append(
                {"note_id": note["note_id"], "heading": marker["heading"], "line": marker["line"],
                 "concept_id": marker["concept_id"]}
            )

    links: list[dict] = []
    findings: list[dict] = []
    for note in project["notes"]:
        for raw in scan_links(bodies[note["note_id"]]):
            if raw["in_table"] and raw["alias"] is not None and not raw["escaped_alias"]:
                raise ValueError("表格单元里别名前的竖线没有转义")
            resolved = resolve_note(raw["target"], project)
            note_record, basis = resolved
            status = STATUS_RESOLVED
            resolved_note = None
            resolved_heading = None
            if note_record is None:
                status = STATUS_AMBIGUOUS if basis == BASIS_DUP_NOTE else STATUS_BROKEN
            else:
                resolved_note = note_record["note_id"]
                ok, heading_basis, heading = resolve_heading(
                    note_record["note_id"], raw["heading"], bodies
                )
                if not ok:
                    status = (
                        STATUS_AMBIGUOUS if heading_basis == BASIS_DUP_HEADING else STATUS_BROKEN
                    )
                    basis = heading_basis
                    resolved_note = note_record["note_id"]
                else:
                    resolved_heading = heading
                    basis = heading_basis if raw["heading"] is not None else basis
            links.append(
                {
                    "note_id": note["note_id"],
                    "path": note["path"],
                    "line": raw["line"],
                    "raw": raw["raw"],
                    "target": raw["target"],
                    "heading": raw["heading"],
                    "alias": raw["alias"],
                    "embed": raw["embed"],
                    "in_table": raw["in_table"],
                    "escaped_alias": raw["escaped_alias"],
                    "status": status,
                    "basis": basis,
                    "resolved_note": resolved_note,
                    "resolved_heading": resolved_heading,
                }
            )
            if status != STATUS_RESOLVED:
                findings.append(
                    {
                        "kind": FINDING_BROKEN if status == STATUS_BROKEN else FINDING_AMBIGUOUS,
                        "severity": "critical",
                        "subject": f"{note['note_id']}:{raw['line']}",
                        "detail": f"链接 {raw['raw']} 无法唯一解析（{basis}）。",
                    }
                )

    concepts: list[dict] = []
    for concept in gold["concepts"]:
        own = [
            {"note_id": marker["note_id"], "heading": marker["heading"], "line": marker["line"]}
            for marker in all_markers
            if marker["concept_id"] == concept["concept_id"]
        ]
        if not own:
            findings.append(
                {
                    "kind": FINDING_UNEXPLAINED,
                    "severity": "critical",
                    "subject": concept["concept_id"],
                    "detail": "整个发布集合里没有完整解释标记。",
                }
            )
        elif len(own) > 1:
            findings.append(
                {
                    "kind": FINDING_DUPLICATE,
                    "severity": "critical",
                    "subject": concept["concept_id"],
                    "detail": f"有 {len(own)} 处完整解释标记。",
                }
            )
        concepts.append(
            {
                "concept_id": concept["concept_id"],
                "canonical": concept["canonical"],
                "markers": own,
                "canonical_note": own[0]["note_id"] if own else None,
                "canonical_heading": own[0]["heading"] if own else None,
                "unique": len(own) == 1,
            }
        )

    for record in concepts:
        if not record["unique"]:
            continue
        covered = any(
            atom["concept_id"] == record["concept_id"]
            and atom["note_id"] == record["canonical_note"]
            and atom["heading"] == record["canonical_heading"]
            for atom in gold["atoms"]
        )
        if not covered:
            findings.append(
                {
                    "kind": FINDING_CANONICAL,
                    "severity": "critical",
                    "subject": record["concept_id"],
                    "detail": "完整解释标记落在金标准没有声明原子的位置。",
                }
            )

    atoms: list[dict] = []
    for atom in gold["atoms"]:
        concept = next(
            record for record in concepts if record["concept_id"] == atom["concept_id"]
        )
        titles = [section["title"] for section in sections(bodies[atom["note_id"]])]
        if not concept["markers"]:
            recalled, reason = False, "concept_unexplained"
        elif len(concept["markers"]) > 1:
            recalled, reason = False, "duplicate_explanation"
        elif atom["heading"] not in titles:
            recalled, reason = False, "heading_missing"
        else:
            recalled, reason = True, "recalled"
        if not recalled:
            findings.append(
                {
                    "kind": FINDING_ATOM,
                    "severity": "critical" if atom["key"] else "general",
                    "subject": atom["atom_id"],
                    "detail": f"期望位置 {atom['note_id']}#{atom['heading']}，实际结论：{reason}。",
                }
            )
        atoms.append(
            {
                "atom_id": atom["atom_id"],
                "concept_id": atom["concept_id"],
                "key": atom["key"],
                "note_id": atom["note_id"],
                "heading": atom["heading"],
                "recalled": recalled,
                "reason": reason,
            }
        )

    key_total = sum(1 for atom in atoms if atom["key"])
    key_hit = sum(1 for atom in atoms if atom["key"] and atom["recalled"])
    general_total = len(atoms) - key_total
    general_hit = sum(1 for atom in atoms if not atom["key"] and atom["recalled"])
    general_ok = general_total == 0 or general_hit * 100 >= general_total * GENERAL_RECALL_PERCENT
    if general_total and not general_ok:
        findings.append(
            {
                "kind": FINDING_RECALL,
                "severity": "general",
                "subject": "general",
                "detail": f"一般原子召回 {general_hit}/{general_total} 低于下限。",
            }
        )
    if key_total and key_hit < key_total:
        findings.append(
            {
                "kind": FINDING_RECALL,
                "severity": "critical",
                "subject": "key",
                "detail": f"关键原子召回 {key_hit}/{key_total} 不是 100%。",
            }
        )

    resolved = sum(1 for link in links if link["status"] == STATUS_RESOLVED)
    broken = sum(1 for link in links if link["status"] == STATUS_BROKEN)
    ambiguous = sum(1 for link in links if link["status"] == STATUS_AMBIGUOUS)
    blocking = {
        FINDING_DUPLICATE, FINDING_UNEXPLAINED, FINDING_CANONICAL,
    }
    verdict = "pass" if (
        broken == 0
        and ambiguous == 0
        and not any(finding["kind"] in blocking for finding in findings)
        and key_hit == key_total
        and general_ok
    ) else "findings"

    counts = {
        "notes": len(project["notes"]),
        "headings": sum(len(sections(body)) for body in bodies.values()),
        "links": len(links),
        "links_resolved": resolved,
        "links_broken": broken,
        "links_ambiguous": ambiguous,
        "links_embedded": sum(1 for link in links if link["embed"]),
        "links_in_table": sum(1 for link in links if link["in_table"]),
        "links_escaped_alias": sum(1 for link in links if link["escaped_alias"]),
        "concepts": len(concepts),
        "markers": len(all_markers),
        "atoms": len(atoms),
        "atoms_key": key_total,
        "atoms_general": general_total,
        "atoms_recalled": key_hit + general_hit,
        "atoms_missing": len(atoms) - key_hit - general_hit,
        "duplicate_concepts": sum(1 for record in concepts if len(record["markers"]) > 1),
        "findings": len(findings),
        "model_calls": 0,
        "network_requests": 0,
    }
    return {
        "verdict": verdict,
        "counts": counts,
        "availability": ratio(resolved, len(links), "NFR-05 要求本轮发布范围内链接有效率 100%"),
        "key_recall": ratio(key_hit, key_total, "NFR-02 要求关键原子 100%"),
        "general_recall": ratio(general_hit, general_total, f"NFR-02 允许一般原子最多缺 {GENERAL_RECALL_PERCENT}% 以下的缺口"),
        "findings": findings,
        "links": links,
        "concepts": concepts,
        "atoms": atoms,
    }


def load_package(package: Path, project_name: str = "project.json") -> dict:
    project = strict_load(package / project_name, PROJECT_FIELDS, "工程声明", canonical_required=False)
    gold = strict_load(package / project["gold"], GOLD_FIELDS, "金标准", canonical_required=False)
    bodies = {
        note["note_id"]: normalize((package / note["path"]).read_text(encoding="utf-8"))
        for note in project["notes"]
    }
    return {"package": package, "project_name": project_name, "project": project,
            "gold": gold, "bodies": bodies}


def replay_package(package: Path, project_name: str = "project.json"):
    try:
        loaded = load_package(package, project_name)
        audit = evaluate(loaded["project"], loaded["gold"], loaded["bodies"])
    except (ValueError, OSError, KeyError, json.JSONDecodeError) as error:
        return None, "AUDIT_ARGUMENT", str(error)
    loaded["audit"] = audit
    return loaded, "", ""


def check_layout(root: Path, report: Report) -> None:
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control 目录")
        return
    control_entries = sorted(item.name for item in control.iterdir())
    report.record(
        "layout-control-entries", control_entries == list(CONTROL_ENTRIES),
        f"控制目录条目 {control_entries}",
    )
    links = [path for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-symlinks", not links, f"符号链接 {links}")


def check_manifest(root: Path, report: Report):
    path = root / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 control/manifest.json")
        return None
    try:
        manifest = strict_load(path, MANIFEST_FIELDS, "冻结清单")
    except (ValueError, json.JSONDecodeError) as error:
        report.record("manifest-strict", False, str(error))
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", manifest["policy"] == POLICY, manifest["policy"])
    report.record("manifest-producer", manifest["producer"] == APP_PRODUCER, manifest["producer"])
    report.record("manifest-schema", manifest["schema_version"] == SCHEMA_VERSION, str(manifest["schema_version"]))
    report.record("manifest-root", Path(manifest["root"]).resolve() == root.resolve(), manifest["root"])
    for key, relative in (
        ("project_sha256", PROJECT),
        ("gold_sha256", GOLD),
        ("inputs_sha256", INPUTS),
    ):
        actual = sha256(read_bytes(root / relative))
        report.record(f"manifest-digest:{key}", actual == manifest[key], f"{actual} != {manifest[key]}")
    for key, relative in (
        ("root_identity", "control"),
        ("control_identity", "control"),
        ("manifest_identity", MANIFEST),
        ("notes_identity", NOTES_DIR),
        ("inputs_identity", INPUTS),
    ):
        report.record(
            f"manifest-identity:{key}",
            set(manifest[key]) == IDENTITY_FIELDS,
            f"{relative} -> {sorted(manifest[key])}",
        )
    return manifest


def check_project(root: Path, replay: dict, report: Report) -> None:
    """The frozen project declaration is the source bytes, so compare content not encoding."""
    try:
        frozen = strict_load(
            root / PROJECT, PROJECT_FIELDS, "冻结工程声明", canonical_required=False
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("project-frozen", False, str(error))
        return
    report.record("project-frozen", True)
    report.record("project-content", frozen == replay["project"], "工程声明与重放不一致")


def check_inputs(root: Path, replay: dict, manifest: dict, report: Report) -> None:
    path = root / INPUTS
    if not path.is_file():
        report.record("inputs-present", False, "缺少 control/inputs.json")
        return
    try:
        inputs = strict_load(path, INPUTS_FIELDS, "冻结输入")
    except (ValueError, json.JSONDecodeError) as error:
        report.record("inputs-strict", False, str(error))
        return
    report.record("inputs-strict", True)
    report.record("inputs-case", inputs["case_id"] == replay["project"]["case_id"], inputs["case_id"])
    report.record(
        "inputs-gold-digest",
        inputs["gold_sha256"] == sha256(read_bytes(root / GOLD)),
        inputs["gold_sha256"],
    )
    want = []
    absent = []
    for note in replay["project"]["notes"]:
        copy = root / NOTES_DIR / note["path"]
        if not copy.is_file():
            absent.append(note["path"])
            continue
        want.append(
            {
                "note_id": note["note_id"],
                "path": note["path"],
                "title": note["title"],
                "sha256": sha256(copy.read_bytes()),
                "byte_length": copy.stat().st_size,
            }
        )
    report.record("inputs-note-copies-present", not absent, f"缺少冻结副本 {absent}")
    report.record("inputs-notes", inputs["notes"] == want, "冻结笔记摘要与现场不一致")
    for entry in inputs["notes"]:
        report.record(
            f"inputs-note-fields:{entry['note_id']}",
            set(entry) == NOTE_DIGEST_FIELDS,
            f"{sorted(entry)}",
        )
    copies = sorted(
        path.relative_to(root / NOTES_DIR).as_posix()
        for path in (root / NOTES_DIR).rglob("*") if path.is_file()
    )
    report.record(
        "inputs-note-copies",
        copies == sorted(note["path"] for note in replay["project"]["notes"]),
        f"{copies}",
    )


def check_frozen(package: Path, report: Report) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, f"缺少 {FROZEN}")
        return
    frozen = json.loads(path.read_text(encoding="utf-8"))
    actual = {
        item.relative_to(package).as_posix(): {
            "sha256": sha256(item.read_bytes()),
            "byte_length": item.stat().st_size,
        }
        for item in package.rglob("*")
        if item.is_file() and item.name != FROZEN
    }
    report.record("frozen-members", set(frozen["files"]) == set(actual),
                  f"清单 {len(frozen['files'])} 项，现场 {len(actual)} 项")
    for name, digest in sorted(frozen["files"].items()):
        report.record(f"frozen-digest:{name}", actual.get(name) == digest,
                      f"{actual.get(name)} != {digest}")


def project_record(value: dict, fields: tuple[str, ...]) -> dict:
    return {key: value.get(key) for key in fields}


def check_expected(replay: dict, prior: dict, report: Report) -> None:
    audit = replay["audit"]
    report.record("expected-policy", prior["policy"] == POLICY, prior["policy"])
    report.record("expected-case", prior["case_id"] == replay["project"]["case_id"], prior["case_id"])
    report.record("expected-verdict", prior["verdict"] == audit["verdict"],
                  f"{prior['verdict']} != {audit['verdict']}")
    for key in COUNT_FIELDS:
        report.record(f"expected-count:{key}", prior["counts"].get(key) == audit["counts"][key],
                      f"{prior['counts'].get(key)} != {audit['counts'][key]}")
    for key in ("availability", "key_recall", "general_recall"):
        for field in ("numerator", "denominator", "display"):
            report.record(
                f"expected-ratio:{key}:{field}",
                prior[key].get(field) == audit[key][field],
                f"{prior[key].get(field)} != {audit[key][field]}",
            )
    want_links = prior["links"]
    got_links = [{key: link[key] for key in want_links[0]} for link in audit["links"]]
    report.record("expected-links", got_links == want_links, f"{len(got_links)} 条链接")
    want_concepts = prior["explanations"]
    got_concepts = [
        {
            "concept_id": record["concept_id"],
            "canonical": record["canonical"],
            "markers": record["markers"],
            "canonical_note": record["canonical_note"],
            "canonical_heading": record["canonical_heading"],
            "unique": record["unique"],
        }
        for record in audit["concepts"]
    ]
    report.record("expected-explanations", got_concepts == want_concepts, f"{len(got_concepts)} 个概念")
    got_atoms = [
        {"atom_id": atom["atom_id"], "recalled": atom["recalled"], "reason": atom["reason"]}
        for atom in audit["atoms"]
    ]
    report.record("expected-atoms", got_atoms == prior["atoms"], f"{len(got_atoms)} 个原子")
    report.record("expected-zero-calls",
                  audit["counts"]["model_calls"] == 0 and audit["counts"]["network_requests"] == 0,
                  "模型或网络调用不为 0")


def check_variants(package: Path, prior: dict, report: Report) -> None:
    for name, want in sorted(prior["variants"].items()):
        variant_package = package / "variants" / name
        if not variant_package.is_dir():
            report.record(f"variant-present:{name}", False, "缺少变体目录")
            continue
        loaded, code, detail = replay_package(variant_package)
        if loaded is None:
            report.record(f"variant-replay:{name}", False, f"{code} {detail}")
            continue
        audit = loaded["audit"]
        report.record(f"variant-verdict:{name}", audit["verdict"] == want["verdict"],
                      f"{audit['verdict']} != {want['verdict']}")
        for key, value in want["counts"].items():
            report.record(f"variant-count:{name}:{key}", audit["counts"][key] == value,
                          f"{audit['counts'][key]} != {value}")
        for ratio_key in ("availability", "key_recall", "general_recall"):
            if ratio_key not in want:
                continue
            for field in ("numerator", "denominator", "display"):
                report.record(
                    f"variant-ratio:{name}:{ratio_key}:{field}",
                    audit[ratio_key][field] == want[ratio_key][field],
                    f"{audit[ratio_key][field]} != {want[ratio_key][field]}",
                )
        kinds = sorted(finding["kind"] for finding in audit["findings"])
        report.record(f"variant-findings:{name}", kinds == sorted(want["findings"]), f"{kinds}")


def check_output(root: Path, output: Path, replay: dict, report: Report) -> None:
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == sorted(OUTPUT_ENTRIES), f"输出条目 {entries}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    for name in OUTPUT_ENTRIES:
        report.record(f"output-present:{name}", (output / name).is_file(), f"缺少 {name}")
    manifest_path = output / OUTPUT_MANIFEST
    if not manifest_path.is_file():
        report.record("output-manifest-strict", False, "缺少输出清单")
        return
    try:
        manifest = strict_load(manifest_path, OUTPUT_MANIFEST_FIELDS, "输出清单")
    except (ValueError, json.JSONDecodeError) as error:
        report.record("output-manifest-strict", False, str(error))
        return
    report.record("output-manifest-strict", True)
    report.record("output-manifest-produced",
                  manifest["policy"] == POLICY and manifest["producer"] == OUTPUT_PRODUCER
                  and manifest["case_id"] == replay["project"]["case_id"],
                  f"{manifest['policy']}/{manifest['producer']}/{manifest['case_id']}")
    report.record("output-manifest-no-timestamp", "created_at" not in manifest, "输出清单不得含时间戳")
    for key, name in (
        ("links_sha256", LINKS),
        ("explanations_sha256", EXPLANATIONS),
        ("report_sha256", REPORT),
        ("markdown_sha256", MARKDOWN),
    ):
        if name in missing:
            report.record(f"output-manifest-digest:{key}", False, f"缺少 {name}")
            continue
        actual = sha256((output / name).read_bytes())
        report.record(f"output-manifest-digest:{key}", actual == manifest[key], f"{actual} != {manifest[key]}")
    audit = replay["audit"]
    report.record("output-manifest-verdict", manifest["verdict"] == audit["verdict"],
                  f"{manifest['verdict']} != {audit['verdict']}")
    report.record("output-manifest-counts", manifest["counts"] == audit["counts"], "输出清单计数不一致")
    if missing:
        return

    try:
        report_value = strict_load(output / REPORT, REPORT_FIELDS, "报告", canonical_required=False)
    except (ValueError, json.JSONDecodeError) as error:
        report.record("report-strict", False, str(error))
        return
    report.record("report-strict", True)
    report.record("report-verdict", report_value["verdict"] == audit["verdict"],
                  f"{report_value['verdict']} != {audit['verdict']}")
    report.record("report-counts", report_value["counts"] == audit["counts"], "报告计数不一致")
    for key in ("availability", "key_recall", "general_recall"):
        report.record(f"report-ratio:{key}", report_value[key] == audit[key],
                      f"{report_value[key]} != {audit[key]}")
    report.record("report-zero-calls",
                  report_value["model_calls"] == 0 and report_value["network_requests"] == 0,
                  "模型或网络调用不为 0")

    try:
        links_file = strict_load(output / LINKS, LINKS_FILE_FIELDS, "链接清单", canonical_required=False)
    except (ValueError, json.JSONDecodeError) as error:
        report.record("links-strict", False, str(error))
        links_file = None
    if links_file is not None:
        report.record("links-strict", True)
        report.record("links-count", len(links_file["links"]) == len(audit["links"]),
                      f"{len(links_file['links'])} != {len(audit['links'])}")
        for index, link in enumerate(links_file["links"]):
            report.record(f"links-fields:{index}", set(link) == LINK_FIELDS, f"{sorted(link)}")
        projected = [{key: link[key] for key in LINK_FIELDS} for link in audit["links"]]
        report.record("links-entries", links_file["links"] == projected, "链接明细与重放不一致")

    try:
        explanations = strict_load(
            output / EXPLANATIONS, EXPLANATIONS_FILE_FIELDS, "解释清单", canonical_required=False
        )
    except (ValueError, json.JSONDecodeError) as error:
        report.record("explanations-strict", False, str(error))
        explanations = None
    if explanations is not None:
        report.record("explanations-strict", True)
        for index, record in enumerate(explanations["concepts"]):
            report.record(
                f"explanations-fields:{index}",
                set(record) == CONCEPT_RECORD_FIELDS,
                f"{sorted(record)}",
            )
            for marker in record["markers"]:
                report.record(
                    f"explanations-marker-fields:{index}", set(marker) == MARKER_FIELDS, f"{sorted(marker)}"
                )
        got = [
            {key: record[key] for key in CONCEPT_RECORD_FIELDS}
            for record in explanations["concepts"]
        ]
        report.record("explanations-concepts", got == audit["concepts"], "概念记录与重放不一致")
        for index, atom in enumerate(explanations["atoms"]):
            report.record(f"explanations-atom-fields:{index}", set(atom) == ATOM_RECORD_FIELDS,
                          f"{sorted(atom)}")
        got_atoms = [{key: atom[key] for key in ATOM_RECORD_FIELDS} for atom in explanations["atoms"]]
        report.record("explanations-atoms", got_atoms == audit["atoms"], "原子记录与重放不一致")

    markdown = (output / MARKDOWN).read_text(encoding="utf-8")
    for heading in ("## 结论", "## 口径", "## 链接明细", "## 唯一解释位置", "## 缺口", "## 边界"):
        report.record(f"markdown-section:{heading}", heading in markdown, f"缺少 {heading}")
    report.record("markdown-boundary", "不判断内容质量" in markdown, "缺少边界声明")
    report.record(
        "markdown-ratios",
        audit["availability"]["display"] in markdown
        and audit["key_recall"]["display"] in markdown
        and audit["general_recall"]["display"] in markdown,
        "口径表缺少三个比率",
    )


def freeze(package: Path) -> int:
    files = {
        item.relative_to(package).as_posix(): {
            "sha256": sha256(item.read_bytes()),
            "byte_length": item.stat().st_size,
        }
        for item in package.rglob("*")
        if item.is_file() and item.name != FROZEN
    }
    payload = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("root", nargs="?")
    parser.add_argument("--output")
    parser.add_argument("--package")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--freeze", action="store_true")
    args = parser.parse_args(argv)

    if args.freeze:
        if not args.package:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        package = Path(args.package).resolve()
        if not (package / "project.json").is_file():
            print(f"{package} 不是审计包（缺少 project.json）。", file=sys.stderr)
            return 2
        return freeze(package)

    report = Report()
    replay = None
    if args.package:
        package = Path(args.package).resolve()
        if not (package / "project.json").is_file():
            print(f"{package} 不是审计包（缺少 project.json）。", file=sys.stderr)
            return 2
        replay, code, detail = replay_package(package)
        if replay is None:
            report.record("replay", False, f"{code} {detail}")
        else:
            check_frozen(package, report)
            prior = json.loads((package / "expected.json").read_text(encoding="utf-8"))
            check_expected(replay, prior, report)
            check_variants(package, prior, report)

    if args.root:
        root = Path(args.root).resolve()
        if not root.is_dir():
            print(f"{root} 不是目录。", file=sys.stderr)
            return 2
        check_layout(root, report)
        manifest = check_manifest(root, report)
        if replay is not None and manifest is not None:
            check_project(root, replay, report)
            check_inputs(root, replay, manifest, report)
        if replay is None and manifest is not None:
            report.record("replay-available", False, "没有 --package 时无法重放冻结输入")
        if args.output:
            output = Path(args.output).resolve()
            if not output.is_dir():
                print(f"{output} 不是目录。", file=sys.stderr)
                return 2
            if replay is not None:
                check_output(root, output, replay, report)

    if not args.root and not args.package:
        print("至少要给一个审计根或 --package。", file=sys.stderr)
        return 2

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
