#!/usr/bin/env python3
"""Independent, read-only auditor for the portable source index (source-index-v1).

The auditor never imports the Rust product and never runs the CLI to decide
anything. It re-derives the index, the relocation verdicts and the rejection
codes from the frozen author package with its own copy of the rules, then
compares that derivation against what the product actually wrote.

Usage: <root> [--output DIR] [--package DIR] [--json] [--freeze]
Exit codes: 0 all checks passed, 1 failures, 2 usage error
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

POLICY = "source-index-v1"
AUTHOR_PRODUCER = "fixture_author"
APP_PRODUCER = "noetome-index"
OUTPUT_PRODUCER = "source_index_output"
INDEX_VERSION = 1
SCHEMA_VERSION = 1

ROLE_NOTE = "note"
ROLE_RESOURCE = "resource"

STATUS_RELOCATED = "relocated"
STATUS_MISSING = "missing"
STATUS_DIGEST_MISMATCH = "digest_mismatch"

# 与产品同名同集合：凭据字样一律不进索引。
SECRET_MARKERS = ("api_key", "apikey", "password", "secret", "token=", "authorization:")

CONTROL = "control"
MANIFEST = "manifest.json"
INPUTS = "inputs.json"
PROJECT = "project.json"
FILES = "files"
INDEX = "index.json"
RELOCATION = "relocation.json"
REPORT = "report.json"
MARKDOWN = "report.md"
OUTPUT_MANIFEST = "manifest.json"
FROZEN = "frozen-sha256.json"

PROJECT_FIELDS = (
    "policy",
    "schema_version",
    "producer",
    "case_id",
    "description",
    "output_area",
    "runtime_area",
    "temp_area",
    "notes",
    "resources",
    "sources",
    "limits",
    "relocate_area",
)
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
    "files_identity",
    "inputs_identity",
    "project_sha256",
    "inputs_sha256",
    "created_at",
)
INPUTS_FIELDS = (
    "policy",
    "schema_version",
    "case_id",
    "output_area",
    "runtime_area",
    "temp_area",
    "relocate_area",
    "files",
    "materials",
    "runtime_entries",
    "temp_entries",
    "candidates",
)
FILE_DIGEST_FIELDS = ("path", "role", "sha256", "byte_length")
MATERIAL_FIELDS = ("source_id", "path", "sha256", "byte_length")
EXCLUDED_FIELDS = ("path", "sha256", "byte_length")
INDEX_FIELDS = ("policy", "index_version", "case_id", "output_area", "files", "notes", "sources")
INDEX_NOTE_FIELDS = ("note_id", "path", "title", "headings", "sha256")
INDEX_SOURCE_FIELDS = (
    "source_id",
    "note_id",
    "heading",
    "description",
    "locator",
    "path",
    "sha256",
    "byte_length",
    "digest_short",
)
RELOCATION_FIELDS = (
    "policy",
    "index_version",
    "case_id",
    "directory",
    "relocated",
    "missing",
    "digest_mismatch",
    "sources",
)
RELOCATION_SOURCE_FIELDS = ("source_id", "status", "found", "detail")
REPORT_FIELDS = (
    "policy",
    "schema_version",
    "case_id",
    "description",
    "areas",
    "counts",
    "model_calls",
    "network_requests",
)
AREA_FIELDS = ("output", "runtime", "temp", "relocate")
COUNT_FIELDS = (
    "notes",
    "resources",
    "sources",
    "headings",
    "output_files",
    "excluded_runtime_entries",
    "excluded_temp_entries",
    "citation_marks",
    "absolute_paths",
    "secret_markers",
    "relocated",
    "missing",
    "digest_mismatch",
)
OUTPUT_MANIFEST_FIELDS = (
    "policy",
    "producer",
    "schema_version",
    "case_id",
    "source_root",
    "source_inputs_sha256",
    "index_sha256",
    "relocation_sha256",
    "report_sha256",
    "markdown_sha256",
    "counts",
)

ROOT_ENTRIES = ("control",)
CONTROL_ENTRIES = ("files", "inputs.json", "manifest.json", "project.json")
OUTPUT_ENTRIES = ("index.json", "manifest.json", "relocation.json", "report.json", "report.md")


class Report:
    def __init__(self) -> None:
        self.checks: list[dict[str, object]] = []

    def record(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks.append({"name": name, "ok": bool(ok), "detail": detail})
        return bool(ok)

    def warn(self, name: str, detail: str) -> None:
        self.checks.append({"name": name, "ok": True, "warning": True, "detail": detail})

    @property
    def failures(self) -> list[dict[str, object]]:
        return [item for item in self.checks if not item["ok"]]

    @property
    def warnings(self) -> list[dict[str, object]]:
        return [item for item in self.checks if item.get("warning")]

    def dump(self, as_json: bool = False) -> None:
        if as_json:
            print(
                json.dumps(
                    {
                        "checks": self.checks,
                        "failures": len(self.failures),
                        "warnings": len(self.warnings),
                    },
                    ensure_ascii=False,
                    indent=2,
                )
            )
            return
        for item in self.failures:
            print(f"[失败] {item['name']}：{item['detail']}")
        for item in self.warnings:
            print(f"[提示] {item['name']}：{item['detail']}")
        print(
            f"合计 {len(self.checks)} 项检查：失败 {len(self.failures)}、提示 {len(self.warnings)}"
        )


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bytes(path: Path) -> bytes:
    return Path(path).read_bytes()


def canonical(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def strict_load(
    payload: bytes,
    fields: tuple[str, ...],
    label: str,
    canonical_required: bool = True,
) -> tuple[dict | None, str]:
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        return None, f"{label} 不是合法UTF-8文本：{error}"
    stripped = text[:-1] if text.endswith("\n") else text
    try:
        value = json.loads(stripped)
    except json.JSONDecodeError as error:
        return None, f"{label} 不是合法JSON：{error}"
    if not isinstance(value, dict):
        return None, f"{label} 顶层必须是对象"
    if set(value.keys()) != set(fields):
        missing = sorted(set(fields) - set(value.keys()))
        extra = sorted(set(value.keys()) - set(fields))
        return None, f"{label} 字段不符：缺少 {missing}，多余 {extra}"
    if canonical_required:
        expected = canonical(value)
        if text == expected + "\n":
            return value, ""
        if text != expected:
            return None, f"{label} 不是规范编码：请用紧凑 JSON、无重复键、无未知字段"
    return value, ""


def normalize(raw: bytes) -> str:
    text = raw.decode("utf-8")
    if text.startswith("\ufeff"):
        text = text[1:]
    return text.replace("\r\n", "\n").replace("\r", "\n")


def heading_level(line: str) -> int:
    content = line.lstrip(" ")
    indentation = len(line) - len(content)
    if indentation > 3:
        return 0
    hashes = 0
    while hashes < len(content) and content[hashes] == "#":
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
    while hashes < len(content) and content[hashes] == "#":
        hashes += 1
    return content[hashes:].strip()


def heading_titles(text: str) -> list[str]:
    return [heading_title(line) for line in text.split("\n") if heading_level(line) > 0]


def citation_marks(text: str) -> list[str]:
    marks: list[str] = []
    for line in text.split("\n"):
        title = heading_title(line)
        if heading_level(line) > 0 and ("参考文献" in title or "引用文献" in title):
            marks.append(f"「{title}」小节")
    index = 0
    digits = "0123456789"
    while index < len(text):
        if text[index] == "[":
            end = index + 1
            count = 0
            while end < len(text) and text[end] in digits:
                end += 1
                count += 1
            if count and end < len(text) and text[end] == "]":
                marks.append(text[index : end + 1])
                index = end + 1
                continue
        index += 1
    return marks


def under(path: str, area: str) -> bool:
    return path == area or path.startswith(area + "/")


def absolute_paths(paths) -> list[str]:
    return [path for path in paths if path.startswith("/") or ":" in path]


def secret_markers(values) -> list[str]:
    found = set()
    for value in values:
        lowered = value.lower()
        for marker in SECRET_MARKERS:
            if marker in lowered:
                found.add(marker)
    return sorted(found)


def digest_short(digest: str) -> str:
    return digest[:16]


def scan_area(package: Path, area: str) -> list[dict[str, object]]:
    root = package / area
    entries: list[dict[str, object]] = []
    if not root.exists():
        return entries
    if not root.is_dir():
        raise ValueError(f"{area} 不是目录")
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"{path} 是符号链接")
        if not path.is_file():
            continue
        data = path.read_bytes()
        entries.append(
            {
                "path": path.relative_to(package).as_posix(),
                "sha256": sha256(data),
                "byte_length": len(data),
            }
        )
    return entries


def load_project(package: Path, name: str = PROJECT) -> tuple[dict | None, str]:
    path = package / name
    if not path.is_file():
        return None, f"{name} 不存在"
    return strict_load(path.read_bytes(), PROJECT_FIELDS, name, canonical_required=False)


def evaluate(project: dict, package: Path) -> tuple[str, str, dict]:
    """Replay the product's accept/reject decision; empty code means accepted."""
    areas = [
        project["output_area"],
        project["runtime_area"],
        project["temp_area"],
        project["relocate_area"],
    ]
    for index, left in enumerate(areas):
        for right in areas[index + 1 :]:
            if under(left, right) or under(right, left):
                return "INDEX_ARGUMENT", f"区域互相包含：{left} 与 {right}。", {}

    notes: dict[str, dict] = {}
    for spec in project["notes"]:
        if not under(spec["path"], project["output_area"]):
            return (
                "INDEX_AREA_LEAK",
                f"笔记 {spec['note_id']} 的路径 {spec['path']} 不在输出区 {project['output_area']} 里；只有输出区里的文件才会随输出一起复制。",
                {},
            )
        path = package / spec["path"]
        if not path.is_file():
            return "INDEX_ARGUMENT", f"笔记 {spec['note_id']} 的正文 {spec['path']} 不存在。", {}
        text = normalize(path.read_bytes())
        titles = heading_titles(text)
        if not titles:
            return (
                "INDEX_ARGUMENT",
                f"笔记 {spec['note_id']} 没有任何标题，无法建立章节级追溯。",
                {},
            )
        notes[spec["note_id"]] = {"spec": spec, "text": text, "headings": titles}

    resources: dict[str, dict] = {}
    for spec in project["resources"]:
        if not under(spec["path"], project["output_area"]):
            return (
                "INDEX_AREA_LEAK",
                f"资源 {spec['resource_id']} 的路径 {spec['path']} 不在输出区 {project['output_area']} 里。",
                {},
            )
        path = package / spec["path"]
        if not path.is_file():
            return (
                "INDEX_ARGUMENT",
                f"资源 {spec['resource_id']} 的文件 {spec['path']} 不存在。",
                {},
            )
        resources[spec["resource_id"]] = {"spec": spec, "bytes": path.read_bytes()}

    for spec in project["sources"]:
        if spec["note_id"] not in notes:
            return (
                "INDEX_ARGUMENT",
                f"来源 {spec['source_id']} 指向的笔记 {spec['note_id']} 不在本次声明里。",
                {},
            )
        if not (package / spec["path"]).is_file():
            return (
                "INDEX_ARGUMENT",
                f"来源 {spec['source_id']} 的原始资料 {spec['path']} 不存在。",
                {},
            )
        for area in (project["output_area"], project["runtime_area"], project["temp_area"]):
            if under(spec["path"], area):
                return (
                    "INDEX_AREA_LEAK",
                    f"来源 {spec['source_id']} 的路径 {spec['path']} 落在 {area} 里：原始资料必须放在输出区、运行库与临时区之外。",
                    {},
                )

    for note_id, note in notes.items():
        marks = citation_marks(note["text"])
        if marks:
            detail = "；".join(f"{note_id} 里出现 {mark}" for mark in marks)
            return (
                "INDEX_CITATION_BURDEN",
                f"笔记正文里出现了文献编号或参考文献小节：{detail}。默认不在正文插入编号、逐句脚注或长出处列表，来源由索引维护。",
                {},
            )

    for note_id, note in notes.items():
        for spec in project["sources"]:
            if spec["note_id"] != note_id:
                continue
            if spec["heading"] not in note["headings"]:
                available = "、".join(note["headings"])
                return (
                    "INDEX_LOCATION_MISSING",
                    f"来源 {spec['source_id']} 声明的章节「{spec['heading']}」在笔记 {note_id} 里不存在；可用的章节是 {available}。",
                    {},
                )

    texts = [note["text"] for note in notes.values()]
    texts += [item["spec"]["description"] for item in resources.values()]
    texts += [spec["description"] for spec in project["sources"]]
    texts += [spec["locator"] for spec in project["sources"]]
    texts += [project["description"]]
    paths = [spec["path"] for spec in project["notes"]]
    paths += [spec["path"] for spec in project["resources"]]
    paths += [spec["path"] for spec in project["sources"]]
    if absolute_paths(paths):
        return (
            "INDEX_ARGUMENT",
            f"索引里出现了绝对路径或盘符：{absolute_paths(paths)}。索引要能跨机器复制，只能写相对路径。",
            {},
        )
    if secret_markers(texts):
        return (
            "INDEX_SECRET_MARKER",
            f"正文或来源描述里出现了疑似凭据字样：{secret_markers(texts)}。凭据不能进索引。",
            {},
        )
    return "", "", {"notes": notes, "resources": resources}


def build_index(project: dict, package: Path, notes: dict, resources: dict) -> dict:
    files: list[dict[str, object]] = []
    note_entries: list[dict[str, object]] = []
    for spec in project["notes"]:
        note = notes[spec["note_id"]]
        payload = (package / spec["path"]).read_bytes()
        digest = sha256(payload)
        files.append(
            {
                "path": spec["path"],
                "role": ROLE_NOTE,
                "sha256": digest,
                "byte_length": len(payload),
            }
        )
        note_entries.append(
            {
                "note_id": spec["note_id"],
                "path": spec["path"],
                "title": spec["title"],
                "headings": note["headings"],
                "sha256": digest,
            }
        )
    for spec in project["resources"]:
        payload = resources[spec["resource_id"]]["bytes"]
        files.append(
            {
                "path": spec["path"],
                "role": ROLE_RESOURCE,
                "sha256": sha256(payload),
                "byte_length": len(payload),
            }
        )
    sources: list[dict[str, object]] = []
    for spec in project["sources"]:
        payload = (package / spec["path"]).read_bytes()
        digest = sha256(payload)
        sources.append(
            {
                "source_id": spec["source_id"],
                "note_id": spec["note_id"],
                "heading": spec["heading"],
                "description": spec["description"],
                "locator": spec["locator"],
                "path": spec["path"],
                "sha256": digest,
                "byte_length": len(payload),
                "digest_short": digest_short(digest),
            }
        )
    return {
        "policy": POLICY,
        "index_version": INDEX_VERSION,
        "case_id": project["case_id"],
        "output_area": project["output_area"],
        "files": files,
        "notes": note_entries,
        "sources": sources,
    }


def build_relocation(project: dict, index: dict, candidates: list[dict]) -> dict:
    return relocation_of(
        project["case_id"], project["relocate_area"], index["sources"], candidates
    )


def relocation_of(
    case_id: str, directory: str, sources: list[dict], candidates: list[dict]
) -> dict:
    by_digest: dict[str, str] = {}
    for candidate in candidates:
        by_digest.setdefault(candidate["sha256"], candidate["path"])
    records: list[dict[str, object]] = []
    for source in sources:
        found = by_digest.get(source["sha256"])
        if found is not None:
            records.append(
                {
                    "source_id": source["source_id"],
                    "status": STATUS_RELOCATED,
                    "found": found,
                    "detail": f"在 {directory} 里找到内容摘要一致的文件：{found}（来源描述：{source['description']}）。",
                }
            )
            continue
        name = source["path"].rsplit("/", 1)[-1]
        names = [
            candidate["path"]
            for candidate in candidates
            if candidate["path"].rsplit("/", 1)[-1] == name
        ]
        if names:
            records.append(
                {
                    "source_id": source["source_id"],
                    "status": STATUS_DIGEST_MISMATCH,
                    "found": None,
                    "detail": f"{directory} 里有同名文件 {'、'.join(names)}，但内容摘要不同：同名不能证明是同一份来源，需要重建映射。",
                }
            )
        else:
            records.append(
                {
                    "source_id": source["source_id"],
                    "status": STATUS_MISSING,
                    "found": None,
                    "detail": f"{directory} 里没有内容摘要一致的文件，需要重建映射；来源描述「{source['description']}」，定位 {source['locator']}。",
                }
            )
    return {
        "policy": POLICY,
        "index_version": INDEX_VERSION,
        "case_id": case_id,
        "directory": directory,
        "relocated": sum(1 for item in records if item["status"] == STATUS_RELOCATED),
        "missing": sum(1 for item in records if item["status"] == STATUS_MISSING),
        "digest_mismatch": sum(
            1 for item in records if item["status"] == STATUS_DIGEST_MISMATCH
        ),
        "sources": records,
    }


def build_counts(index: dict, inputs: dict, relocation: dict) -> dict:
    return {
        "notes": len(index["notes"]),
        "resources": len(inputs["files"]) - len(index["notes"]),
        "sources": len(index["sources"]),
        "headings": sum(len(note["headings"]) for note in index["notes"]),
        "output_files": len(index["files"]),
        "excluded_runtime_entries": len(inputs["runtime_entries"]),
        "excluded_temp_entries": len(inputs["temp_entries"]),
        "citation_marks": 0,
        "absolute_paths": 0,
        "secret_markers": 0,
        "relocated": relocation["relocated"],
        "missing": relocation["missing"],
        "digest_mismatch": relocation["digest_mismatch"],
    }


def build_inputs(project: dict, package: Path, index: dict) -> dict:
    materials = []
    for spec in project["sources"]:
        payload = (package / spec["path"]).read_bytes()
        materials.append(
            {
                "source_id": spec["source_id"],
                "path": spec["path"],
                "sha256": sha256(payload),
                "byte_length": len(payload),
            }
        )
    return {
        "policy": POLICY,
        "schema_version": SCHEMA_VERSION,
        "case_id": project["case_id"],
        "output_area": project["output_area"],
        "runtime_area": project["runtime_area"],
        "temp_area": project["temp_area"],
        "relocate_area": project["relocate_area"],
        "files": index["files"],
        "materials": materials,
        "runtime_entries": scan_area(package, project["runtime_area"]),
        "temp_entries": scan_area(package, project["temp_area"]),
        "candidates": scan_area(package, project["relocate_area"]),
    }


def replay_package(package: Path, project_name: str = PROJECT):
    project, detail = load_project(package, project_name)
    if project is None:
        return None, "INDEX_ARGUMENT", detail
    code, detail, data = evaluate(project, package)
    if code:
        return None, code, detail
    index = build_index(project, package, data["notes"], data["resources"])
    inputs = build_inputs(project, package, index)
    relocation = build_relocation(project, index, inputs["candidates"])
    counts = build_counts(index, inputs, relocation)
    bodies = {spec["path"]: (package / spec["path"]).read_bytes() for spec in project["notes"]}
    bodies.update(
        {spec["path"]: (package / spec["path"]).read_bytes() for spec in project["resources"]}
    )
    return (
        {
            "package": str(package),
            "project": project,
            "project_name": project_name,
            "inputs": inputs,
            "index": index,
            "relocation": relocation,
            "counts": counts,
            "bodies": bodies,
        },
        "",
        "",
    )


def check_expected(report: Report, package: Path, replay: dict) -> None:
    path = package / "expected.json"
    if not path.is_file():
        report.record("expected-present", False, "expected.json 不存在")
        return
    try:
        prior = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        report.record("expected-parse", False, f"expected.json 不是合法JSON：{error}")
        return
    report.record("expected-parse", True)
    report.record(
        "expected-policy",
        prior.get("policy") == POLICY and prior.get("case_id") == replay["project"]["case_id"],
        f"policy/case_id：{prior.get('policy')} / {prior.get('case_id')}",
    )
    for key in COUNT_FIELDS:
        report.record(
            f"expected-count:{key}",
            prior.get("counts", {}).get(key) == replay["counts"][key],
            f"{key}：先验 {prior.get('counts', {}).get(key)}，重放 {replay['counts'][key]}",
        )
    report.record(
        "expected-files",
        prior.get("files") == replay["index"]["files"],
        f"files：先验 {len(prior.get('files', []))} 条，重放 {len(replay['index']['files'])} 条",
    )
    report.record(
        "expected-notes",
        prior.get("notes") == replay["index"]["notes"],
        "notes 逐条比对",
    )
    report.record(
        "expected-sources",
        prior.get("sources") == replay["index"]["sources"],
        "sources 逐条比对",
    )
    # 先验只承载判据（哪条来源落到哪个文件），不复制产品写的说明句；
    # 因此把重放记录的 detail 投影掉再比，否则先验会被迫跟着产品措辞漂移。
    prior_relocation = prior.get("relocation") or {}
    computed_relocation = replay["relocation"]
    comparable = {
        key: computed_relocation[key] for key in ("relocated", "missing", "digest_mismatch")
    }
    comparable_sources = [
        {key: item[key] for key in ("source_id", "status", "found")}
        for item in computed_relocation["sources"]
    ]
    report.record(
        "expected-relocation",
        prior_relocation.get("directory") == computed_relocation["directory"]
        and all(prior_relocation.get(key) == value for key, value in comparable.items())
        and prior_relocation.get("sources") == comparable_sources,
        f"先验 {prior_relocation.get('relocated')}/{prior_relocation.get('missing')}/"
        f"{prior_relocation.get('digest_mismatch')}，重放 "
        f"{computed_relocation['relocated']}/{computed_relocation['missing']}/"
        f"{computed_relocation['digest_mismatch']}",
    )
    for entry in prior.get("ad_hoc_relocations", []):
        directory = entry["directory"]
        try:
            candidates = scan_area(package, directory)
        except ValueError as error:
            report.record(f"expected-adhoc:{directory}", False, str(error))
            continue
        computed = relocation_of(
            replay["project"]["case_id"],
            directory,
            replay["index"]["sources"],
            candidates,
        )
        # mismatched 只承载「同名但摘要不同」这类需要重建映射的来源；
        # missing 已经由 missing 计数表达，混进来会掩盖两者的区别。
        mismatched = sorted(
            item["source_id"]
            for item in computed["sources"]
            if item["status"] == STATUS_DIGEST_MISMATCH
        )
        ok = (
            computed["relocated"] == entry["relocated"]
            and computed["missing"] == entry["missing"]
            and computed["digest_mismatch"] == entry["digest_mismatch"]
            and mismatched == sorted(entry.get("mismatched", []))
        )
        report.record(
            f"expected-adhoc:{directory}",
            ok,
            f"先验 {entry['relocated']}/{entry['missing']}/{entry['digest_mismatch']}，重放 {computed['relocated']}/{computed['missing']}/{computed['digest_mismatch']}",
        )
    report.record(
        "expected-zero-model",
        prior.get("model_calls") == 0 and prior.get("network_requests") == 0,
        f"model_calls={prior.get('model_calls')} network_requests={prior.get('network_requests')}",
    )
    boundaries = prior.get("boundaries", {})
    report.record(
        "expected-boundaries",
        boundaries.get("scientific_status") == "not_run"
        and boundaries.get("uses_network") is False
        and boundaries.get("uses_model") is False,
        f"boundaries={boundaries}",
    )


def check_rejections(report: Report, package: Path) -> None:
    path = package / "expected.json"
    if not path.is_file():
        return
    prior = json.loads(path.read_text(encoding="utf-8"))
    accepted, code, detail = replay_package(package)
    report.record(
        "rejection:baseline",
        accepted is not None and not code,
        f"基线工程被拒：{code} {detail}",
    )
    for entry in prior.get("rejections", []):
        name = entry["project"]
        _, got, detail = replay_package(package, name)
        report.record(
            f"rejection:{name}",
            got == entry["code"],
            f"期望 {entry['code']}，重放 {got or '（通过）'} {detail}",
        )


def check_layout(report: Report, root: Path) -> None:
    entries = sorted(item.name for item in root.iterdir())
    report.record("layout-root-entries", entries == list(ROOT_ENTRIES), f"顶层条目 {entries}")
    control = root / CONTROL
    if not control.is_dir():
        report.record("layout-control", False, "缺少 control/")
        return
    report.record("layout-control", True)
    names = sorted(item.name for item in control.iterdir())
    report.record("layout-control-entries", names == list(CONTROL_ENTRIES), f"control 条目 {names}")
    report.record("layout-files", (control / FILES).is_dir(), "缺少 control/files/")
    links = [str(path) for path in root.rglob("*") if path.is_symlink()]
    report.record("layout-no-symlinks", not links, f"符号链接 {links}")


def check_manifest(report: Report, root: Path) -> dict | None:
    path = root / CONTROL / MANIFEST
    if not path.is_file():
        report.record("manifest-present", False, "缺少 control/manifest.json")
        return None
    payload = read_bytes(path)
    value, detail = strict_load(payload, MANIFEST_FIELDS, "control/manifest.json")
    if value is None:
        report.record("manifest-strict", False, detail)
        return None
    report.record("manifest-strict", True)
    report.record("manifest-policy", value["policy"] == POLICY, f"policy={value['policy']}")
    report.record(
        "manifest-producer", value["producer"] == APP_PRODUCER, f"producer={value['producer']}"
    )
    report.record(
        "manifest-schema",
        value["schema_version"] == SCHEMA_VERSION,
        f"schema_version={value['schema_version']}",
    )
    report.record(
        "manifest-root",
        Path(value["root"]).resolve() == root.resolve(),
        f"root={value['root']}",
    )
    project_path = root / CONTROL / PROJECT
    inputs_path = root / CONTROL / INPUTS
    report.record(
        "manifest-project-digest",
        project_path.is_file() and sha256(read_bytes(project_path)) == value["project_sha256"],
        "project_sha256 与 control/project.json 不符",
    )
    report.record(
        "manifest-inputs-digest",
        inputs_path.is_file() and sha256(read_bytes(inputs_path)) == value["inputs_sha256"],
        "inputs_sha256 与 control/inputs.json 不符",
    )
    for key in ("root_identity", "parent_identity", "control_identity", "manifest_identity",
                "files_identity", "inputs_identity"):
        identity = value[key]
        ok = (
            isinstance(identity, dict)
            and set(identity.keys()) == {"platform", "device", "inode"}
            and isinstance(identity["inode"], int)
        )
        report.record(f"manifest-identity:{key}", ok, f"{key}={identity}")
    return value


def check_project(report: Report, root: Path) -> dict | None:
    path = root / CONTROL / PROJECT
    if not path.is_file():
        report.record("project-present", False, "缺少 control/project.json")
        return None
    payload = read_bytes(path)
    value, detail = strict_load(payload, PROJECT_FIELDS, "control/project.json", canonical_required=False)
    if value is None:
        report.record("project-strict", False, detail)
        return None
    report.record("project-strict", True)
    report.record("project-policy", value["policy"] == POLICY, f"policy={value['policy']}")
    report.record(
        "project-producer",
        value["producer"] == AUTHOR_PRODUCER,
        f"producer={value['producer']}",
    )
    return value


def check_inputs(report: Report, root: Path, replay: dict) -> None:
    path = root / CONTROL / INPUTS
    if not path.is_file():
        report.record("inputs-present", False, "缺少 control/inputs.json")
        return
    value, detail = strict_load(read_bytes(path), INPUTS_FIELDS, "control/inputs.json")
    if value is None:
        report.record("inputs-strict", False, detail)
        return
    report.record("inputs-strict", True)
    expected = replay["inputs"]
    report.record(
        "inputs-case",
        value["case_id"] == expected["case_id"],
        f"case_id={value['case_id']}",
    )
    report.record(
        "inputs-areas",
        all(value[key] == expected[key] for key in ("output_area", "runtime_area", "temp_area", "relocate_area")),
        f"区域 {value['output_area']}/{value['runtime_area']}/{value['temp_area']}/{value['relocate_area']}",
    )
    for key, fields in (
        ("files", FILE_DIGEST_FIELDS),
        ("materials", MATERIAL_FIELDS),
        ("runtime_entries", EXCLUDED_FIELDS),
        ("temp_entries", EXCLUDED_FIELDS),
        ("candidates", EXCLUDED_FIELDS),
    ):
        report.record(
            f"inputs-{key}",
            value[key] == expected[key],
            f"{key} 与重放扫描不符：{len(value[key])} 条 vs {len(expected[key])} 条",
        )
        for index, item in enumerate(value[key]):
            report.record(
                f"inputs-{key}:{index}:fields",
                set(item.keys()) == set(fields),
                f"字段 {sorted(item.keys())}",
            )
    files_root = root / CONTROL / FILES
    copies = {}
    if files_root.is_dir():
        for item in sorted(files_root.rglob("*")):
            if item.is_file() and not item.is_symlink():
                copies[item.relative_to(files_root).as_posix()] = sha256(item.read_bytes())
    report.record(
        "inputs-file-copies",
        copies == {item["path"]: item["sha256"] for item in value["files"]},
        f"control/files 与 inputs.files 不符：{sorted(copies)}",
    )


def check_frozen(report: Report, package: Path) -> None:
    path = package / FROZEN
    if not path.is_file():
        report.record("frozen-present", False, "缺少 frozen-sha256.json")
        return
    frozen = json.loads(path.read_text(encoding="utf-8"))
    actual = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.is_symlink():
            continue
        relative = item.relative_to(package).as_posix()
        if relative == FROZEN:
            continue
        payload = item.read_bytes()
        actual[relative] = {"sha256": sha256(payload), "byte_length": len(payload)}
    report.record(
        "frozen-members",
        set(frozen.get("files", {}).keys()) == set(actual.keys()),
        f"成员不一致：缺少 {sorted(set(frozen.get('files', {})) - set(actual))}，多余 {sorted(set(actual) - set(frozen.get('files', {})))}",
    )
    for relative, digest in sorted(actual.items()):
        recorded = frozen.get("files", {}).get(relative)
        report.record(
            f"frozen-digest:{relative}",
            recorded == digest,
            f"{relative}：记录 {recorded}，实际 {digest}",
        )


def check_output(report: Report, output: Path, replay: dict) -> None:
    entries = sorted(item.name for item in output.iterdir())
    report.record("output-entries", entries == list(OUTPUT_ENTRIES), f"输出条目 {entries}")
    missing = [name for name in OUTPUT_ENTRIES if not (output / name).is_file()]
    for name in OUTPUT_ENTRIES:
        report.record(f"output-present:{name}", (output / name).is_file(), f"缺少 {name}")
    if not (output / OUTPUT_MANIFEST).is_file():
        report.record("output-manifest-present", False, "缺少输出清单")
        return
    manifest, detail = strict_load(
        read_bytes(output / OUTPUT_MANIFEST), OUTPUT_MANIFEST_FIELDS, "输出清单"
    )
    if manifest is None:
        report.record("output-manifest-strict", False, detail)
        return
    report.record("output-manifest-strict", True)
    report.record(
        "output-manifest-produced",
        manifest["policy"] == POLICY
        and manifest["producer"] == OUTPUT_PRODUCER
        and manifest["case_id"] == replay["index"]["case_id"],
        f"policy/producer/case_id={manifest['policy']}/{manifest['producer']}/{manifest['case_id']}",
    )
    if missing:
        report.record("output-manifest-digests", False, f"缺少输出文件 {missing}")
    else:
        report.record(
            "output-manifest-digests",
            manifest["index_sha256"] == sha256(read_bytes(output / INDEX))
            and manifest["relocation_sha256"] == sha256(read_bytes(output / RELOCATION))
            and manifest["report_sha256"] == sha256(read_bytes(output / REPORT))
            and manifest["markdown_sha256"] == sha256(read_bytes(output / MARKDOWN)),
            "输出文件摘要与清单不符",
        )
    report.record(
        "output-manifest-counts",
        manifest["counts"] == replay["counts"],
        "清单计数与重放不符",
    )
    if missing:
        return
    index, detail = strict_load(read_bytes(output / INDEX), INDEX_FIELDS, INDEX)
    if index is None:
        report.record("index-strict", False, detail)
    else:
        report.record("index-strict", True)
        report.record("index-entries", index == replay["index"], "索引与重放不符")
        for note in index["notes"]:
            report.record(
                f"index-note:{note['note_id']}:fields",
                set(note.keys()) == set(INDEX_NOTE_FIELDS),
                f"字段 {sorted(note.keys())}",
            )
        for source in index["sources"]:
            report.record(
                f"index-source:{source['source_id']}:fields",
                set(source.keys()) == set(INDEX_SOURCE_FIELDS),
                f"字段 {sorted(source.keys())}",
            )
            report.record(
                f"index-source:{source['source_id']}:digest-short",
                source["digest_short"] == digest_short(source["sha256"]),
                f"digest_short={source['digest_short']}",
            )
    relocation, detail = strict_load(
        read_bytes(output / RELOCATION), RELOCATION_FIELDS, RELOCATION
    )
    if relocation is None:
        report.record("relocation-strict", False, detail)
    else:
        report.record("relocation-strict", True)
        report.record(
            "relocation-entries", relocation == replay["relocation"], "重定位结果与重放不符"
        )
    info, detail = strict_load(read_bytes(output / REPORT), REPORT_FIELDS, REPORT)
    if info is None:
        report.record("report-strict", False, detail)
    else:
        report.record("report-strict", True)
        report.record(
            "report-areas",
            info["areas"]
            == {
                "output": replay["project"]["output_area"],
                "runtime": replay["project"]["runtime_area"],
                "temp": replay["project"]["temp_area"],
                "relocate": replay["project"]["relocate_area"],
            },
            f"areas={info['areas']}",
        )
        report.record("report-counts", info["counts"] == replay["counts"], "计数与重放不符")
        report.record(
            "report-zero-calls",
            info["model_calls"] == 0 and info["network_requests"] == 0,
            f"model_calls={info['model_calls']} network_requests={info['network_requests']}",
        )
    body = (output / MARKDOWN).read_text(encoding="utf-8")
    for heading in ("# 来源索引", "## 区域", "## 计数", "## 文件清单", "## 来源映射", "## 重定位结果", "## 边界"):
        report.record(
            f"markdown-section:{heading}", heading in body, f"缺少小节 {heading}"
        )
    report.record(
        "markdown-boundary",
        "不判断内容质量与科学正确性" in body and "运行库" in body and "临时区" in body,
        "边界段缺少必要声明",
    )
    report.record(
        "markdown-sources",
        all(source["source_id"] in body for source in replay["index"]["sources"]),
        "来源映射表缺少来源",
    )


def freeze(package: Path) -> int:
    files = {}
    for item in sorted(package.rglob("*")):
        if not item.is_file() or item.is_symlink():
            continue
        relative = item.relative_to(package).as_posix()
        if relative == FROZEN:
            continue
        payload = item.read_bytes()
        files[relative] = {"sha256": sha256(payload), "byte_length": len(payload)}
    record = {
        "schema_version": 1,
        "producer": "independent_fixture_author_freeze",
        "workflow_id": POLICY,
        "case_count": len(files),
        "files": files,
    }
    (package / FROZEN).write_text(
        json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"已冻结 {len(files)} 个成员")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="verify_source_index.py",
        description="独立只读审计 portable source index（source-index-v1）。",
    )
    parser.add_argument("root", nargs="?", help="索引根")
    parser.add_argument("--output", help="索引输出根")
    parser.add_argument("--package", help="冻结作者包")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出检查结果")
    parser.add_argument("--freeze", action="store_true", help="重写 frozen-sha256.json")
    args = parser.parse_args(argv)

    if args.freeze:
        if not args.package:
            print("--freeze 需要 --package。", file=sys.stderr)
            return 2
        return freeze(Path(args.package))
    if not args.root and not args.package:
        print("至少要给一个索引根或 --package。", file=sys.stderr)
        return 2

    report = Report()
    replay = None
    if args.package:
        package = Path(args.package)
        if not (package / PROJECT).is_file():
            print(f"{package} 不是索引包（缺少 project.json）。", file=sys.stderr)
            return 2
        replay, code, detail = replay_package(package)
        report.record(
            "replay", replay is not None, f"重放失败：{code or '（通过）'} {detail}".strip()
        )
        if replay is None:
            report.dump(args.json)
            return 0 if not report.failures else 1
        check_expected(report, package, replay)
        check_rejections(report, package)
        check_frozen(report, package)

    if args.root:
        root = Path(args.root)
        if not root.is_dir():
            print(f"{root} 不是目录。", file=sys.stderr)
            return 2
        check_layout(report, root)
        manifest = check_manifest(report, root)
        project = check_project(report, root)
        if replay is not None and manifest is not None and project is not None:
            replay["root"] = str(root.resolve())
            check_inputs(report, root, replay)

    if args.output:
        output = Path(args.output)
        if not output.is_dir():
            print(f"{output} 不是目录。", file=sys.stderr)
            return 2
        if replay is None:
            report.record("output-needs-package", False, "--output 需要 --package 才能重放")
        else:
            check_output(report, output, replay)

    report.dump(args.json)
    return 0 if not report.failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
