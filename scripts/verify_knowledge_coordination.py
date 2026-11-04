"""Rebuild bounded coordination from frozen author facts and actual source bytes.

No Rust validator, model, HTTP, credentials or production Vault is used. The
controlled fixture audit proves engineering behavior, not scientific accuracy.
"""
from __future__ import annotations

import argparse
from contextlib import closing
import copy
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
import time
import uuid

# Works both as a script and when imported from elsewhere in the repository.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_extraction_knowledge import (strict_json, fingerprint, actual_source,
                                        verify_report as verify_source_report)

REPO = Path(__file__).resolve().parents[1]
SCOPE = "explicit_identity_duplicate_complement_v1"
ISSUE = "COORD_UNHANDLED_DIFFERENCE"
FILES = {".owner.json": 4096, "review-original.json": 65536,
         "candidates-original.json": 1048576, "gold-original.json": 65536,
         "decision-original.json": 65536, "source-snapshot.json": 4194304,
         "knowledge-ir.json": 8388608, "verification.json": 8388608,
         "verification.md": 8388608}
BINDING = "project_id import_id source_id version_id content_sha256 normalized_sha256 parse_id manifest_sha256"
REPORT = "schema_version report_kind producer request_id scope project_id status knowledge_status published scientific_quality research_status network_calls paid_calls canonical_concepts inputs occurrences facts relations totals artifacts"
IR = "schema_version producer revision request_id scope project_id knowledge_status published scientific_quality research_status canonical_concepts inputs occurrences facts relations totals"
TOTALS = "input_bundles validated_input_bundles input_atoms retained_fact_groups canonical_facts source_occurrences evidence_links independent_sources duplicate_occurrences passed failed needs_review unprocessed"


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def normalized(value):
    return "".join(value.split())


def fields(obj, names):
    assert type(obj) is dict and set(obj) == set(names.split()), "schema fields: " + names


def exact(actual, expected, label):
    """JSON equality must not treat true, 1 and 1.0 as interchangeable."""
    assert type(actual) is type(expected), label + ": type"
    if type(expected) is dict:
        assert set(actual) == set(expected), label + ": fields"
        for key in expected:
            exact(actual[key], expected[key], label + "/" + key)
    elif type(expected) is list:
        assert len(actual) == len(expected), label + ": length"
        for index, (a, e) in enumerate(zip(actual, expected)):
            exact(a, e, label + "/" + str(index))
    else:
        assert actual == expected, label + ": value"


def text(value, limit=1024):
    assert type(value) is str and value.strip() and len(value.encode("utf-8")) <= limit, "text type/limit"
    assert not any(ord(c) < 32 or ord(c) == 127 for c in value), "text control"


def integer(value):
    assert type(value) is int and 0 <= value < 2**64, "integer type/range"


def strings(value, maximum=8):
    assert type(value) is list and len(value) <= maximum, "array type/limit"
    for item in value:
        text(item)
    assert len({normalized(x) for x in value}) == len(value), "duplicate normalized string"


def identifier(value):
    assert type(value) is str and re.fullmatch(r"[a-z][a-z0-9_]{0,31}", value), "input ID"


def uuid_string(value):
    assert type(value) is str and str(uuid.UUID(value)) == value, "canonical UUID"


def safe_name(value):
    text(value)
    assert value not in (".", "..") and not any(c in value for c in '/\\:<>"|?*'), "unsafe path"
    assert not value.endswith((".", " ")), "unsafe path ending"
    assert value.split(".")[0].upper() not in {"CON", "PRN", "AUX", "NUL", *("COM" + str(i) for i in range(1, 10)), *("LPT" + str(i) for i in range(1, 10))}, "reserved path"


def binding(value):
    fields(value, BINDING)
    for key in ("project_id", "import_id", "source_id", "version_id"):
        uuid_string(value[key])
    for key in ("content_sha256", "normalized_sha256", "manifest_sha256"):
        if key == "content_sha256" or value[key] is not None:
            assert type(value[key]) is str and re.fullmatch("[a-f0-9]{64}", value[key]), "binding SHA"
    if value["parse_id"] is not None:
        uuid_string(value["parse_id"])


def inventory(root):
    """Reject links before descending; retain identity and complete byte hashes."""
    root = Path(root)
    result = {}
    def walk(path):
        stat = path.lstat()
        assert not path.is_symlink() and not getattr(stat, "st_file_attributes", 0) & 0x400, "link/reparse path"
        assert path.is_dir() or path.is_file(), "nonordinary path"
        if path.is_file():
            assert stat.st_nlink == 1, "hardlink file"
        result[path.relative_to(root).as_posix()] = {"directory": path.is_dir(), "identity": [stat.st_dev, stat.st_ino], "mtime_ns": stat.st_mtime_ns,
                                                  **(fingerprint(path.read_bytes()) if path.is_file() else {})}
        if path.is_dir():
            for child in sorted(path.iterdir()):
                walk(child)
    walk(root)
    return result


def read(path, limit):
    assert path.is_file() and path.stat().st_size <= limit, "file size/type: " + path.name
    return path.read_bytes()


def metadata(value):
    """Bound metadata recursively; RequestUnit body is validated separately."""
    if type(value) is str:
        assert len(value.encode()) <= 1024 and not any(ord(c) < 32 or ord(c) == 127 for c in value), "metadata limit/control"
    elif type(value) is dict:
        for item in value.values():
            metadata(item)
    elif type(value) is list:
        assert len(value) <= 1024, "metadata array limit"
        for item in value:
            metadata(item)


def role_text(value):
    """Declared bounded plain text, not merely an already-string limit check."""
    text(value)
    assert not any(marker in value for marker in ("%%", "[[", "![")), "source role hidden/link text"
    assert not re.search(r"<[A-Za-z/!?]|\]\s*[(\[:]", value), "source role rich text"


def source_roles_schema(review, gold, decision):
    """Validate every field of the upstream original roles before rebuilding.

    Wire-valid unknown gold and empty decision arrays are representable here;
    coordination's passed/known/single-atom requirement is checked separately.
    """
    def enum(value, choices):
        role_text(value)
        assert value in choices, "source role enum"

    def array(value, maximum):
        assert type(value) is list and len(value) <= maximum, "source role array type/limit"
        for item in value:
            role_text(item)

    fields(review, "schema_version purpose candidate_file gold_file decision_file gold_sha256 subject property allowed_contexts selection reviewer interpretation_timing")
    exact(review["schema_version"], 1, "review version")
    enum(review["purpose"], ("single_measurement_source_review",))
    enum(review["reviewer"], ("automated_source_review",))
    enum(review["interpretation_timing"], ("before_synthetic_response", "after_model_response"))
    for key in ("subject", "property"):
        role_text(review[key])
    for key in ("candidate_file", "gold_file", "decision_file"):
        safe_name(review[key])
    assert type(review["gold_sha256"]) is str and re.fullmatch("[a-f0-9]{64}", review["gold_sha256"]), "review gold SHA type"
    strings(review["allowed_contexts"])
    for context in review["allowed_contexts"]:
        role_text(context)
    assert type(review["selection"]) is list and len(review["selection"]) == 1, "review selection type/count"
    selection = review["selection"][0]
    assert type(selection) is dict, "review selection object"
    enum(selection.get("kind"), ("text", "pdf"))
    if selection["kind"] == "text":
        fields(selection, "kind ordinal")
        integer(selection["ordinal"])
    else:
        fields(selection, "kind page_index region_id")
        integer(selection["page_index"])
        role_text(selection["region_id"])

    assert type(gold) is dict and type(gold.get("schema_version")) is int and gold["schema_version"] in (1, 2), "source gold version"
    fields(gold, "schema_version author producer scientific_quality concept_name atom_content value unit conditions quote verification required knowledge_status published" + (" exceptions value_status" if gold["schema_version"] == 2 else ""))
    for key in ("author", "concept_name", "atom_content", "quote"):
        role_text(gold[key])
    for key, fixed in (("producer", "fake"), ("scientific_quality", "not_run"), ("verification", "unverified"), ("knowledge_status", "pending")):
        enum(gold[key], (fixed,))
    exact(gold["required"], False, "source gold required")
    exact(gold["published"], False, "source gold published")
    strings(gold["conditions"])
    for condition in gold["conditions"]:
        role_text(condition)
    if gold["schema_version"] == 2:
        enum(gold["value_status"], ("known", "unknown"))
        strings(gold["exceptions"])
        for exception in gold["exceptions"]:
            role_text(exception)
        for key in ("value", "unit"):
            if gold[key] is not None:
                role_text(gold[key])
        if gold["value_status"] == "unknown":
            assert gold["value"] is None, "unknown source value must be null"
        else:
            role_text(gold["value"]); role_text(gold["unit"])
    else:
        role_text(gold["value"]); role_text(gold["unit"])

    fields(decision, "schema_version unit_id disposition atom_ids exclusion_reason")
    exact(decision["schema_version"], 1, "decision version")
    role_text(decision["unit_id"])
    enum(decision["disposition"], ("included", "excluded", "needs_review", "pending"))
    array(decision["atom_ids"], 1024)
    if decision["exclusion_reason"] is not None:
        role_text(decision["exclusion_reason"])
    if decision["disposition"] == "excluded":
        role_text(decision["exclusion_reason"])


def verify_markdown(raw, preamble, expected, label):
    """Consume the complete envelope and single JSON fence, including its end."""
    document = raw.decode("utf-8")
    assert document.startswith(preamble + "```json\n"), label + ": visible envelope"
    body = document[len(preamble) + len("```json\n"):]
    assert body.endswith("\n```\n"), label + ": closing fence/end"
    body = body[:-len("\n```\n")]
    exact(strict_json(body), expected, label + ": independent facts")


def selected_source_paths(project, import_id, selection):
    """Check registered paths before the existing helper opens resource bytes."""
    project = project.resolve()
    def registered(base, name, directory=False):
        assert type(name) is str and name and not Path(name).is_absolute(), "registered relative path"
        components = name.replace("\\", "/").split("/")
        for component in components:
            safe_name(component)
        path = base.joinpath(*components)
        assert path.resolve().is_relative_to(project), "registered resource escapes project"
        assert path.is_dir() if directory else path.is_file(), "registered resource type"
        if not directory:
            assert path.stat().st_size <= 64 * 1048576, "source resource byte limit"
        return path
    manifest = strict_json(read(project / "project.json", 65536))
    fields(manifest, "project_id schema_version created_at state database")
    version = manifest["schema_version"]
    assert type(version) is int and version in (2, 3), "source project version"
    assert manifest["database"] == "project.sqlite" and manifest["state"] == "initialized", "source project state/database"
    uuid_string(manifest["project_id"])
    with closing(sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True)) as db:
        db.row_factory = sqlite3.Row
        metadata_rows = db.execute("SELECT project_id, schema_version FROM project_metadata").fetchall()
        assert len(metadata_rows) == 1, "project SQL metadata row"
        assert metadata_rows[0]["project_id"] == manifest["project_id"], "project SQL identity"
        exact(metadata_rows[0]["schema_version"], version, "project SQL schema version")
        exact(db.execute("PRAGMA user_version").fetchone()[0], version, "project user version")
        table = "source_imports" if selection["kind"] == "text" else "pdf_imports"
        imp = db.execute("SELECT * FROM " + table + " WHERE import_id=?", (import_id,)).fetchone()
        assert imp, "selected source import"
        version = db.execute("SELECT * FROM source_versions WHERE version_id=?", (imp["version_id"],)).fetchone()
        assert version, "selected source version"
        registered(project, version["raw_snapshot"])
        if selection["kind"] == "text":
            registered(project, version["normalized_snapshot"])
        else:
            attempt = db.execute("SELECT * FROM pdf_parses WHERE parse_id=?", (imp["parse_id"],)).fetchone()
            assert attempt, "selected PDF parse"
            parse_root = registered(project, attempt["snapshot_path"], directory=True)
            registered(parse_root, "parser/pdf-manifest.json")
            region = db.execute("SELECT * FROM pdf_regions WHERE parse_id=? AND region_id=? AND physical_page_index=?", (imp["parse_id"], selection["region_id"], selection["page_index"])).fetchone()
            assert region, "selected PDF region"
            resource = db.execute("SELECT * FROM pdf_resources WHERE parse_id=? AND resource_id=?", (imp["parse_id"], region["text_resource_id"])).fetchone()
            assert resource, "selected PDF resource"
            registered(parse_root, resource["relative_path"])
def gold_schema(gold):
    fields(gold, "schema_version author scope scientific_quality knowledge_status published expected_status input_ids expected_input_bundles expected_fact_groups expected_source_occurrences expected_independent_sources expected_evidence_links expected_duplicate_occurrences facts")
    exact({k: gold[k] for k in ("schema_version", "scope", "scientific_quality", "knowledge_status", "published")},
          dict(schema_version=1, scope=SCOPE, scientific_quality="not_run", knowledge_status="pending", published=False), "gold fixed")
    text(gold["author"])
    assert gold["expected_status"] in ("passed", "needs_review"), "gold status"
    assert type(gold["input_ids"]) is list and 2 <= len(gold["input_ids"]) <= 3, "gold inputs"
    for item in gold["input_ids"]:
        identifier(item)
    assert len(set(gold["input_ids"])) == len(gold["input_ids"]), "gold duplicate input"
    for key in gold:
        if key.startswith("expected_") and key != "expected_status":
            integer(gold[key])
    assert type(gold["facts"]) is list and 1 <= len(gold["facts"]) <= 3, "gold facts"
    covered = []
    for f in gold["facts"]:
        fields(f, "object_id property_id value unit conditions exceptions input_ids original_atom_ids")
        uuid_string(f["object_id"]); uuid_string(f["property_id"])
        text(f["value"]); text(f["unit"])
        strings(f["conditions"]); strings(f["exceptions"])
        assert type(f["input_ids"]) is list and type(f["original_atom_ids"]) is list and len(f["input_ids"]) == len(f["original_atom_ids"]) > 0, "gold occurrence IDs"
        for i, atom in zip(f["input_ids"], f["original_atom_ids"]):
            identifier(i); text(atom)
            covered.append(i)
    assert sorted(covered) == sorted(gold["input_ids"]), "gold partition"


def source_bundle(project, directory, pin):
    assert {p.name for p in directory.iterdir()} == set(FILES), "source bundle inventory"
    raw = {name: read(directory / name, limit) for name, limit in FILES.items()}
    exact(pin, {name: fingerprint(body) for name, body in raw.items()}, "nine source fingerprints")
    parsed = {name: strict_json(body) for name, body in raw.items() if name.endswith(".json")}
    review, candidate, gold, decision = (parsed[name + "-original.json"] for name in ("review", "candidates", "gold", "decision"))
    source_roles_schema(review, gold, decision)
    for obj in (review, candidate, gold, decision):
        metadata(obj)
    fields(review, "schema_version purpose candidate_file gold_file decision_file gold_sha256 subject property allowed_contexts selection reviewer interpretation_timing")
    exact(review["schema_version"], 1, "review version")
    assert review["purpose"] == "single_measurement_source_review", "review purpose"
    names = [review[k] for k in ("candidate_file", "gold_file", "decision_file")]
    for name in names:
        safe_name(name)
    assert len({"review.json", *[x.lower() for x in names]}) == 4, "source original roles"
    assert type(review["selection"]) is list and len(review["selection"]) == 1, "selection"
    selection = review["selection"][0]
    if selection["kind"] == "text":
        fields(selection, "kind ordinal"); integer(selection["ordinal"])
    else:
        fields(selection, "kind page_index region_id")
        assert selection["kind"] == "pdf", "selection kind"
        integer(selection["page_index"]); text(selection["region_id"])
    strings(review["allowed_contexts"])
    assert review["interpretation_timing"] in ("before_synthetic_response", "after_model_response"), "timing"
    fields(decision, "schema_version unit_id disposition atom_ids exclusion_reason")
    exact(decision["schema_version"], 1, "decision version")
    assert decision["disposition"] == "included" and decision["exclusion_reason"] is None, "included decision"
    fields(gold, "schema_version author producer scientific_quality concept_name atom_content value unit conditions quote verification required knowledge_status published" + (" exceptions value_status" if gold["schema_version"] == 2 else ""))
    assert type(gold["schema_version"]) is int and gold["schema_version"] in (1, 2), "source gold version"
    exact({k: gold[k] for k in ("producer", "scientific_quality", "verification", "required", "knowledge_status", "published")}, dict(producer="fake", scientific_quality="not_run", verification="unverified", required=False, knowledge_status="pending", published=False), "source gold fixed")
    assert gold.get("value_status", "known") == "known", "known source gold"
    text(gold["value"]); text(gold["unit"])
    strings(gold["conditions"]); strings(gold.get("exceptions", []))
    binding(candidate["binding"])
    selected_source_paths(project, candidate["binding"]["import_id"], selection)
    source = actual_source(project, candidate["binding"]["import_id"], selection)
    assert len(source["units"][0]["text"].encode()) <= 1048576, "source body limit"
    inputs = {"review.json": raw["review-original.json"], **dict(zip(names, [raw["candidates-original.json"], raw["gold-original.json"], raw["decision-original.json"]]))}
    verified = verify_source_report(directory, raw["candidates-original.json"], raw["gold-original.json"], source, inputs)
    assert verified["status"] == "passed" and verified["independently_passed"] and verified["candidate_atoms"] == 1, "source independently passed single atom"
    exact(parsed[".owner.json"], dict(schema_version=1, producer="source_review", request_id=candidate["request_id"], purpose="unpublished_source_review"), "source owner")
    # Strengthen the old verifier's Python equality and report numeric fields.
    r, ir = parsed["verification.json"], parsed["knowledge-ir.json"]
    for counts in [r["totals"], *r["quality"].values()]:
        for key, value in counts.items():
            if key != "status":
                integer(value)
    exact(r["schema_version"], 1, "source report version")
    exact(r["network_calls"], 0, "source network"); exact(r["paid_calls"], 0, "source paid")
    exact(r["scope"]["expected_atoms"], 1, "source expected atoms")
    exact(ir["schema_version"], 1, "source IR version"); exact(ir["revision"], 1, "source IR revision")
    for key in ("concepts", "atoms", "evidence"):
        exact(ir[key], candidate["payload"][key], "source IR original " + key)
    exact(ir["checks"], r["checks"], "source IR checks")
    exact(ir["binding"], source["binding"], "source IR binding")
    exact(parsed["source-snapshot.json"], dict(schema_version=1, binding=source["binding"], units=source["units"]), "source snapshot")
    expected_report = rebuild_source_report(candidate, gold, review, decision, source, inputs, raw)
    exact(r, expected_report, "complete source report reconstruction")
    source_preamble = ("# 有界来源知识核对\n\n状态：passed；知识状态：pending；发布：false。\n\n"
                       "科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n"
                       "规则：controlled_measurement_zh_v1；复核主体：automated_source_review；解释时间："
                       + json.dumps(review["interpretation_timing"], ensure_ascii=False) + "。\n\n"
                       "来源范围、对象、问题码、逐类别检查与分母见以下完整事实快照，与 verification.json 一致。\n\n")
    verify_markdown(raw["verification.md"], source_preamble, expected_report, "source Markdown")
    return raw, r, candidate, gold, review, source


def controlled_parts(content, review, gold):
    """Only the existing explicit controlled Chinese grammar, without inference."""
    body = normalized(content)
    roots = [normalized(review["subject"]) + "的" + normalized(review["property"]) + "为" + normalized(gold["value"]) + normalized(gold["unit"])]
    roots += [normalized(c) + "，" + normalized(review["property"]) + "为" + normalized(gold["value"]) + normalized(gold["unit"]) for c in review["allowed_contexts"]]
    for root in roots:
        if not body.startswith(root):
            continue
        tail = body[len(root):].removesuffix("。")
        parts = tail.split("；例外为")
        if len(parts) > 2:
            continue
        match = re.fullmatch(r"[,，;；。]条件[为是](.+)", parts[0]) if parts[0] else None
        if parts[0] and not match:
            continue
        conditions = re.split("，且|且|、|，", match[1]) if match else []
        exceptions = re.split("，且|且|、|，", parts[1]) if len(parts) == 2 else []
        return conditions, exceptions
    raise AssertionError("source controlled grammar")


def rebuild_source_report(candidate, gold, review, decision, source, inputs, raw):
    """Reconstruct the complete passed single-atom saved source report API."""
    payload, u = candidate["payload"], source["units"][0]
    atom = payload["atoms"][0]; a, m = atom["atom"], atom["measurement"]
    conditions, exceptions = controlled_parts(a["content"], review, gold)
    checks = []
    location = {k: u[k] for k in ("unit_id", "start_byte", "end_byte")}
    def add(category, expected, actual, object_id="expected-0", evidence=None):
        loc = location if evidence is None else dict(unit_id=u["unit_id"], start_byte=evidence["excerpt_start_byte"], end_byte=evidence["excerpt_end_byte"])
        checks.append(dict(category=category, object_id=object_id, **loc, status="passed", code=None, expected=expected, actual=actual))
    add("schema", "strict_schema_1", None)
    add("source", "parsed", u["source_status"])
    add("exclusion", "included", decision["disposition"])
    add("coverage", dict(atom_count=1, unit_id=u["unit_id"]), dict(candidate_ids=[a["atom_id"]], decision=decision))
    add("binding", dict(binding=source["binding"], request_id=candidate["request_id"]), dict(binding=payload["binding"], request_id=payload["request_id"]))
    aid = a["atom_id"]
    add("duplicate", "unique_semantic_atom", a["content"], aid)
    support_ids = {link["evidence_id"] for link in a["evidence_links"] if link["relation"] == "supports"}
    evidence = [e for e in payload["evidence"] if e["evidence_id"] in support_ids]
    for e in evidence:
        add("evidence_identity", dict(location), e, e["evidence_id"], e)
    start = u["start_byte"] + u["text"].encode().index(gold["quote"].encode())
    add("evidence", dict(start_byte=start, end_byte=start+len(gold["quote"].encode()), quote=gold["quote"]), evidence, aid)
    names = {normalized(gold["concept_name"]), normalized(review["subject"]), normalized(review["property"])}
    additional = sum(normalized(c["canonical_name"]) not in names for c in payload["concepts"])
    linked = [c for c in payload["concepts"] if c["concept_id"] in a["concept_ids"]]
    add("concept", dict(compound=gold["concept_name"], components=[review["subject"], review["property"]]), dict(linked=linked, additional_source_concepts=additional), aid)
    add("content", gold["atom_content"], a["content"], aid)
    add("fact", dict(value_status="known", value=gold["value"]), m, aid)
    add("unit", gold["unit"], dict(measurement=m["unit"], qualifiers=a["qualifiers"]), aid)
    for condition in gold["conditions"]:
        add("condition", condition, dict(measurement=m["conditions"], qualifiers=a["qualifiers"], body=conditions), aid)
    add("condition_set", dict(required=gold["conditions"], allowed_contexts=review["allowed_contexts"]), dict(measurement=m["conditions"], body=conditions), aid)
    for exception in gold.get("exceptions", []):
        add("exception", exception, dict(qualifiers=a["qualifiers"], body=exceptions), aid)
    add("exception_set", gold.get("exceptions", []), exceptions, aid)
    add("qualifiers", dict(unit=gold["unit"], conditions=gold["conditions"], exceptions=gold.get("exceptions", []), contexts=review["allowed_contexts"]), a["qualifiers"], aid)
    denominators = {k: 1 for k in ("schema", "source", "binding", "concept", "content", "fact", "evidence", "coverage", "exclusion", "unit")}
    denominators.update(condition=len(gold["conditions"]), exception=len(gold.get("exceptions", [])))
    quality = {k: dict(expected=n, passed=n, failed=0, needs_review=0, unprocessed=0, status="passed" if n else "not_applicable") for k, n in denominators.items()}
    result = dict(location, status="passed", issue_codes=[])
    artifacts = {name: fingerprint(raw[name]) for name in (".owner.json", "review-original.json", "candidates-original.json", "gold-original.json", "decision-original.json", "source-snapshot.json", "knowledge-ir.json")}
    return dict(schema_version=1, report_kind="source_knowledge_verification", producer="source_review", request_id=candidate["request_id"], status="passed", knowledge_status="pending", published=False, scientific_quality="not_run", research_status="not_requested", network_calls=0, paid_calls=0,
                scope=dict(purpose=review["purpose"], selection=review["selection"], binding=source["binding"], units=source["units"], expected_atoms=1), binding=source["binding"],
                origin=dict(producer="api", transport_mode=candidate["transport_mode"], reviewer=review["reviewer"], interpretation_timing=review["interpretation_timing"], input_fingerprints={name: fingerprint(body) for name, body in inputs.items()}, gold_sha256=fingerprint(raw["gold-original.json"])["sha256"]),
                rules=dict(grammar="controlled_measurement_zh_v1", **{k: review[k] for k in ("subject", "property", "allowed_contexts", "reviewer", "interpretation_timing")}),
                expected_results=[dict(result, object_id="expected-0")], candidate_results=[dict(result, object_id=aid)], checks=checks,
                totals=dict(expected_atoms=1, passed=1, failed=0, needs_review=0, unprocessed=0, candidate_atoms=1, unexpected_atoms=0, duplicate_atoms=0, required_concept_groups=1, additional_source_concepts=additional), quality=quality, artifacts=artifacts)


def fact_key(mapping, gold):
    return [mapping["object_id"], mapping["property_id"], normalized(gold["value"]), normalized(gold["unit"]),
            sorted(map(normalized, gold["conditions"])), sorted(map(normalized, gold.get("exceptions", [])))]


def _verify(project, case, output, gold_bytes):
    started = time.monotonic()
    gold = strict_json(gold_bytes)  # External immutable expectation precedes candidates.
    gold_schema(gold)
    before = [inventory(p) for p in (project, case, output)]
    for left, right in itertools.combinations((project.resolve(), case.resolve(), output.resolve()), 2):
        assert not left.is_relative_to(right) and not right.is_relative_to(left), "overlapping verifier roots"
    case_raw = read(case / "coordination.json", 65536)
    manifest = strict_json(case_raw)
    fields(manifest, "schema_version purpose project_id identity_file inputs")
    exact(manifest["schema_version"], 1, "case version")
    assert manifest["purpose"] == "explicit_identity_duplicate_complement", "case purpose"
    uuid_string(manifest["project_id"]); safe_name(manifest["identity_file"])
    assert type(manifest["inputs"]) is list and 2 <= len(manifest["inputs"]) <= 3, "case input count"
    map_raw = read(case / manifest["identity_file"], 65536)
    mapping = strict_json(map_raw)
    fields(mapping, "schema_version author concepts concept_mappings atom_mappings")
    exact(mapping["schema_version"], 1, "map version"); text(mapping["author"])
    metadata(mapping)
    for name in ("concepts", "concept_mappings", "atom_mappings"):
        assert type(mapping[name]) is list and len(mapping[name]) <= 1024, "map array type/limit"
    canonical = {}
    for c in mapping["concepts"]:
        fields(c, "canonical_id display_name"); uuid_string(c["canonical_id"]); text(c["display_name"])
        assert c["canonical_id"] not in canonical, "duplicate canonical"
        canonical[c["canonical_id"]] = c
    cm, am = {}, {}
    for c in mapping["concept_mappings"]:
        fields(c, "input_id original_concept_id canonical_id")
        identifier(c["input_id"]); text(c["original_concept_id"])
        key = (c["input_id"], c["original_concept_id"])
        assert key not in cm and c["canonical_id"] in canonical, "concept mapping reference"
        cm[key] = c["canonical_id"]
    for a in mapping["atom_mappings"]:
        fields(a, "input_id original_atom_id object_id property_id")
        identifier(a["input_id"]); text(a["original_atom_id"])
        assert a["input_id"] not in am and a["object_id"] != a["property_id"] and all(a[k] in canonical for k in ("object_id", "property_id")), "atom mapping reference"
        am[a["input_id"]] = a
    inputs, occurrences, keys, original_refs, actual_locations = [], [], {}, set(), set()
    case_names = ["coordination.json", manifest["identity_file"]]
    copied = {"coordination-original.json": case_raw, "identity-original.json": map_raw}
    all_bytes = len(case_raw) + len(map_raw)
    for item in sorted(manifest["inputs"], key=lambda i: i["input_id"]):
        fields(item, "input_id review_bundle binding artifacts")
        ident = item["input_id"]; identifier(ident); safe_name(item["review_bundle"]); binding(item["binding"])
        assert ident not in {x["input_id"] for x in inputs}, "duplicate input ID"
        case_names.append(item["review_bundle"])
        raw, review_report, candidate, source_gold, review, source = source_bundle(project, case / item["review_bundle"], item["artifacts"])
        all_bytes += sum(map(len, raw.values()))
        assert all_bytes <= 96 * 1048576 and time.monotonic() - started <= 30, "input aggregate/time limit"
        exact(item["binding"], source["binding"], "case actual binding")
        assert source["binding"]["project_id"] == manifest["project_id"], "case project"
        atom = candidate["payload"]["atoms"][0]
        a = am[ident]
        assert a["original_atom_id"] == atom["atom"]["atom_id"], "original atom mapping"
        concepts = candidate["payload"]["concepts"]
        for c in concepts:
            original_refs.add((ident, c["concept_id"]))
        refs = atom["atom"]["concept_ids"]
        cc = sorted({cm[(ident, ref)] for ref in refs})
        for role in ("object_id", "property_id"):
            target = review["subject" if role == "object_id" else "property"]
            matches = [c for c in concepts if c["concept_id"] in refs and normalized(target) in map(normalized, [c["canonical_name"], *c["aliases"]])]
            matched_ids = {cm[(ident, c["concept_id"])] for c in matches}
            assert matched_ids == {a[role]}, "original subject/property role"
        expected_f = next(f for f in gold["facts"] if ident in f["input_ids"])
        for k in ("object_id", "property_id"):
            exact(a[k], expected_f[k], "author identity")
        assert atom["atom"]["atom_id"] == expected_f["original_atom_ids"][expected_f["input_ids"].index(ident)], "author original atom ID"
        for k in ("value", "unit", "conditions", "exceptions"):
            exact(source_gold.get(k, []), expected_f[k], "author source fact " + k)
        b, unit = source["binding"], source["units"][0]
        loc = [b[k] for k in ("project_id", "source_id", "version_id", "parse_id")] + [unit[k] for k in ("unit_id", "start_byte", "end_byte")]
        assert compact(loc) not in actual_locations, "duplicate actual source occurrence"
        actual_locations.add(compact(loc))
        occurrence_id = "occ-" + hashlib.sha256(compact([*loc, atom["atom"]["atom_id"]])).hexdigest()
        support = [link["evidence_id"] for link in atom["atom"]["evidence_links"] if link["relation"] == "supports"]
        evidence_by_id = {e["evidence_id"]: e for e in candidate["payload"]["evidence"]}
        assert len(evidence_by_id) == len(candidate["payload"]["evidence"]), "unique source evidence"
        evidence = [evidence_by_id[evidence_id] for evidence_id in support]
        assert evidence, "complete support evidence"
        occurrences.append(dict(occurrence_id=occurrence_id, input_id=ident, binding=b, unit=unit,
                                original_concepts=concepts, original_atom=atom, original_evidence=evidence,
                                canonical_concept_ids=cc, object_id=a["object_id"], property_id=a["property_id"],
                                value=source_gold["value"], unit_value=source_gold["unit"], conditions=source_gold["conditions"],
                                exceptions=source_gold.get("exceptions", []), status="passed", issue_codes=[]))
        keys[occurrence_id] = fact_key(a, source_gold)
        copy_names = {name: "input-" + ident + "--" + name for name in FILES}
        copied.update({copy_names[name]: body for name, body in raw.items()})
        inputs.append(dict(input_id=ident, binding=b, request_id=candidate["request_id"], source_review_status="passed",
                           source_review_checks=review_report["checks"], source_bundle_fingerprints=item["artifacts"], copied_artifacts=copy_names))
    assert set(cm) == original_refs and set(am) == {i["input_id"] for i in inputs}, "complete original map coverage"
    assert set(cm.values()) == set(canonical), "unused canonical concept"
    assert len({x.lower() for x in case_names}) == len(case_names) and {p.name for p in case.iterdir()} == set(case_names), "case inventory/role collision"
    assert sorted(i["input_id"] for i in inputs) == sorted(gold["input_ids"]), "author input members"
    occurrences.sort(key=lambda o: o["occurrence_id"])
    relations = []
    review_ids = set()
    for left, right in itertools.combinations(occurrences, 2):
        lb, rb = left["binding"], right["binding"]
        if lb["source_id"] == rb["source_id"] and (lb["version_id"], lb["parse_id"]) != (rb["version_id"], rb["parse_id"]):
            relation = "needs_review"
        elif left["object_id"] != right["object_id"]:
            relation = "separate_object"
        elif left["property_id"] != right["property_id"]:
            relation = "complement"
        elif keys[left["occurrence_id"]] == keys[right["occurrence_id"]]:
            relation = "duplicate"
        else:
            relation = "needs_review"
        if relation == "needs_review":
            review_ids.update((left["occurrence_id"], right["occurrence_id"]))
        relations.append(dict(left_occurrence_id=left["occurrence_id"], right_occurrence_id=right["occurrence_id"], relation=relation, issue_code=ISSUE if relation == "needs_review" else None))
    facts = {}
    for o in occurrences:
        if o["occurrence_id"] in review_ids:
            o.update(status="needs_review", issue_codes=[ISSUE])
        key = keys[o["occurrence_id"]]
        fid = "fact-" + hashlib.sha256(compact(key)).hexdigest()
        fact = facts.setdefault(fid, dict(fact_id=fid, object_id=key[0], property_id=key[1], value_status="known", value=key[2], unit=key[3], conditions=key[4], exceptions=key[5], occurrence_ids=[], status="passed", issue_codes=[]))
        fact["occurrence_ids"].append(o["occurrence_id"])
        if o["status"] == "needs_review":
            fact.update(status="needs_review", issue_codes=[ISSUE])
    status = "needs_review" if review_ids else "passed"
    n = len(inputs)
    totals = dict(input_bundles=n, validated_input_bundles=n, input_atoms=n, retained_fact_groups=len(facts), canonical_facts=None if review_ids else len(facts), source_occurrences=n,
                  evidence_links=sum(len(o["original_evidence"]) for o in occurrences), independent_sources=len({o["binding"]["source_id"] for o in occurrences}), duplicate_occurrences=n-len(facts), passed=n-len(review_ids), failed=0, needs_review=len(review_ids), unprocessed=0)
    for key in ("input_bundles", "fact_groups", "source_occurrences", "independent_sources", "evidence_links", "duplicate_occurrences"):
        exact(totals["retained_fact_groups" if key == "fact_groups" else key], gold["expected_" + key], "author denominator " + key)
    exact(status, gold["expected_status"], "author expected status")
    report = strict_json(read(output / "coordination.json", 8388608))
    fields(report, REPORT); uuid_string(report["request_id"])
    shared = dict(schema_version=1, producer="knowledge_coordination", request_id=report["request_id"], scope=SCOPE, project_id=manifest["project_id"], knowledge_status="pending", published=False, scientific_quality="not_run", research_status="not_requested", canonical_concepts=sorted(canonical.values(), key=lambda c: c["canonical_id"]), inputs=inputs, occurrences=occurrences, facts=sorted(facts.values(), key=lambda f: f["fact_id"]), relations=relations, totals=totals)
    owner_raw = read(output / ".owner.json", 4096)
    exact(strict_json(owner_raw), dict(schema_version=1, producer="knowledge_coordination", request_id=report["request_id"], purpose="unpublished_coordination"), "coordination owner")
    copied[".owner.json"] = owner_raw
    if status == "passed":
        ir_raw = read(output / "coordination-ir.json", 8388608)
        exact(strict_json(ir_raw), dict(shared, revision=1), "coordination IR independent reconstruction")
        copied["coordination-ir.json"] = ir_raw
    assert {p.name for p in output.iterdir()} == {*copied, "coordination.json", "coordination.md"}, "exact output inventory/IR status"
    for name, body in copied.items():
        assert (output / name).read_bytes() == body, "saved original bytes: " + name
    expected = dict(shared, report_kind="knowledge_coordination", status=status, network_calls=0, paid_calls=0, artifacts={name: fingerprint(body) for name, body in copied.items()})
    exact(report, expected, "coordination report independent reconstruction")
    md = read(output / "coordination.md", 8388608)
    preamble = ("# 有界多来源知识协调\n\n状态：" + status + "；知识状态：pending；发布：false。\n\n"
                "科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n"
                "显式身份重复／互补范围；完整来源、原对象、证据与分母见以下事实，与 coordination.json 一致。\n\n")
    verify_markdown(md, preamble, expected, "coordination Markdown")
    assert [inventory(p) for p in (project, case, output)] == before, "read-only inventory changed"
    assert time.monotonic() - started <= 30, "verification time limit"
    return dict(status=status, independently_verified=True, totals=totals, inputs=[i["input_id"] for i in inputs])


def verify_report(project, case, output, gold_bytes):
    """Read actual SQL/source/gold and all bundles; false success raises AssertionError."""
    try:
        return _verify(Path(project), Path(case), Path(output), gold_bytes)
    except (ValueError, KeyError, TypeError, OSError, StopIteration, sqlite3.Error) as exc:
        raise AssertionError("coordination verification failed: " + type(exc).__name__) from exc


class Audit:
    """Save real CLI commands and protected inputs; never adopt an existing root."""
    def __init__(self, cli, root):
        self.cli, self.root = Path(cli).resolve(), Path(root).resolve()
        self.root.mkdir()
        self.calls, self.rows, self.protected = [], [], {}
        self.sentinel = self.root.parent / ("coordination-sentinel-" + uuid.uuid4().hex + ".txt")
        self.sentinel.write_bytes(b"external human sentinel\n")
        self.protect(self.sentinel)
        fixture = REPO / "fixtures/knowledge-coordination-v1"
        self.authors = {p.name: p.read_bytes() for p in fixture.iterdir() if p.is_file()}
        self.author_dir = self.root / "authors"
        self.author_dir.mkdir()
        for name, body in self.authors.items():
            (self.author_dir / name).write_bytes(body)
            self.protect(fixture / name)
        self.protect(self.author_dir)
        self.cli_fp = fingerprint(self.cli.read_bytes())
        self.write_new(self.root / "author-pins.json", {name: fingerprint(body) for name, body in self.authors.items()})
        self.flush()

    @staticmethod
    def write_new(path, value):
        with path.open("xb") as stream:
            stream.write(encoded(value))

    def protect(self, path):
        self.protected[str(path)] = inventory(path) if path.is_dir() else fingerprint(path.read_bytes())

    def check_protected(self):
        after = {name: inventory(Path(name)) if Path(name).is_dir() else fingerprint(Path(name).read_bytes()) for name in self.protected}
        exact(after, self.protected, "audit protected original hashes")
        return after

    def flush(self, error=None):
        # Versioned summaries never replace failed receipts or previous summaries.
        self.write_new(self.root / ("audit-" + str(len(list(self.root.glob("audit-*.json")))) + ".json"),
                       dict(schema_version=1, cli=str(self.cli), cli_fingerprint=self.cli_fp, root=str(self.root), calls=self.calls,
                            scenarios=self.rows, protected_before=self.protected, error=error, network_calls=0, paid_calls=0,
                            race_coverage="No CLI injection hook; existing Task 1 Rust guard tests are separate evidence."))

    def call(self, name, args, expected=0):
        index = len(self.calls) + 1
        stem = self.root / (f"command-{index:03d}-" + name)
        argv = [str(self.cli), *map(str, args)]
        started = time.monotonic()
        run = subprocess.run(argv, capture_output=True, timeout=40)
        out, err = stem.with_suffix(".stdout.txt"), stem.with_suffix(".stderr.txt")
        out.write_bytes(run.stdout); err.write_bytes(run.stderr)
        receipt = dict(index=index, name=name, argv=argv, exit_code=run.returncode, expected_exit=expected,
                       elapsed_seconds=time.monotonic()-started, stdout=dict(path=str(out), **fingerprint(run.stdout)),
                       stderr=dict(path=str(err), **fingerprint(run.stderr)))
        self.write_new(stem.with_suffix(".json"), receipt)
        self.calls.append(receipt)
        self.flush()
        assert run.returncode == expected, f"CLI {name}: expected {expected}, observed {run.returncode}; see {stem}"
        return run

    def freeze_expected(self, folder, source_gold, ids, same_source=False):
        """Derived only from author inputs before candidate/CLI, never outputs."""
        expected = strict_json(self.authors["expected.json"])
        expected["input_ids"] = ids
        facts = {}
        source_map = strict_json(self.authors["identity-map.json"])
        for ident in ids:
            a = next(a for a in source_map["atom_mappings"] if a["input_id"] == ident)
            g = source_gold[ident]
            key = compact(fact_key(a, g))
            f = facts.setdefault(key, {k: a[k] for k in ("object_id", "property_id")})
            if "value" not in f:
                f.update({k: g[k] for k in ("value", "unit", "conditions", "exceptions")}, input_ids=[], original_atom_ids=[])
            f["input_ids"].append(ident); f["original_atom_ids"].append(a["original_atom_id"])
        expected.update(facts=list(facts.values()), expected_input_bundles=len(ids), expected_fact_groups=len(facts),
                        expected_source_occurrences=len(ids), expected_independent_sources=len(ids)-(1 if same_source else 0),
                        expected_evidence_links=len(ids), expected_duplicate_occurrences=len(ids)-len(facts),
                        expected_status="needs_review" if same_source or len(facts) == 3 else "passed")
        raw = encoded(expected)
        (folder / "gold.json").write_bytes(raw)
        self.protect(folder / "gold.json")
        return raw

    def prepare(self, name, variant=None):
        folder = self.root / name
        folder.mkdir()
        frozen = folder / "frozen-authors"
        frozen.mkdir()
        golds, sources = {}, {}
        for ident in "abc":
            g = strict_json(self.authors[f"gold-{ident}.json"])
            source = self.authors[f"source-{ident}.txt"].decode()
            if ident == "b" and variant in ("condition", "unit"):
                old, new = ("20", "21") if variant == "condition" else ("m/s", "km/s")
                source = source.replace(old, new)
                for key in ("atom_content", "quote", "unit"):
                    g[key] = g[key].replace(old, new)
                g["conditions"] = [c.replace(old, new) for c in g["conditions"]]
            if ident == "b" and variant == "version":
                source += "\n"
            golds[ident], sources[ident] = g, source
            (frozen / f"source-{ident}.txt").write_bytes(source.encode())
            (frozen / f"gold-{ident}.json").write_bytes(encoded(g))
        expected = self.freeze_expected(folder, golds, list("abc"), variant == "version")
        self.protect(frozen)
        project, case = folder / "project", folder / "case"
        self.call(name + "-init", ["project", "init", project])
        case.mkdir()
        (case / "identity.json").write_bytes(self.authors["identity-map.json"])
        inputs = []
        for ident in "abc":
            args = ["source", "import", project, frozen / f"source-{ident}.txt"]
            if ident == "b":
                if variant == "version":
                    source_id = inputs[0]["binding"]["source_id"]
                else:
                    seed = folder / "b-unique-seed.txt"
                    seed.write_bytes(("Independent B registration seed " + uuid.uuid4().hex).encode())
                    imp = strict_json(self.call(name + "-b-seed", ["source", "import", project, seed]).stdout)
                    source_id = imp["version"]["source_id"]
                args += ["--source-id", source_id]
            imported = strict_json(self.call(name + "-import-" + ident, args).stdout)
            selection = {"kind": "text", "ordinal": 0}
            source = actual_source(project, imported["import_id"], selection)
            b, u = source["binding"], source["units"][0]
            review_dir = folder / ("review-" + ident)
            review_dir.mkdir()
            g, gold_raw = golds[ident], (frozen / f"gold-{ident}.json").read_bytes()
            (review_dir / "gold.json").write_bytes(gold_raw)
            atom_id, evidence_id = ident + ("-mass" if ident == "c" else "-speed"), ident + "-support"
            candidate = strict_json((REPO / "fixtures/extraction-review-v1/candidates.json").read_bytes())
            candidate.update(binding=b, request_id="synthetic-coordination-" + ident)
            payload = candidate["payload"]
            payload.update(binding=b, request_id=candidate["request_id"], concepts=[dict(concept_id=ident+"-subject", revision=1, canonical_name="样例 A", aliases=[], needs_full_explanation=True), dict(concept_id=ident+"-property", revision=1, canonical_name="质量" if ident == "c" else "速度", aliases=[], needs_full_explanation=True)])
            quote = g["quote"].encode(); start = u["start_byte"] + u["text"].encode().index(quote)
            payload["evidence"] = [dict(evidence_id=evidence_id, unit_id=u["unit_id"], excerpt_start_byte=start, excerpt_end_byte=start+len(quote), excerpt_sha256=hashlib.sha256(quote).hexdigest(), quote=g["quote"])]
            atom = payload["atoms"][0]
            atom["atom"].update(atom_id=atom_id, concept_ids=[ident+"-subject", ident+"-property"], content=g["atom_content"], qualifiers=[g["unit"], *g["conditions"], *g["exceptions"]], evidence_links=[dict(evidence_id=evidence_id, relation="supports")])
            atom["measurement"] = dict(value_status="known", value=g["value"], unit=g["unit"], conditions=g["conditions"])
            self.write_new(review_dir / "candidates.json", candidate)
            self.write_new(review_dir / "decision.json", dict(schema_version=1, unit_id=u["unit_id"], disposition="included", atom_ids=[atom_id], exclusion_reason=None))
            self.write_new(review_dir / "review.json", dict(schema_version=1, purpose="single_measurement_source_review", candidate_file="candidates.json", gold_file="gold.json", decision_file="decision.json", gold_sha256=fingerprint(gold_raw)["sha256"], subject="样例 A", property="质量" if ident == "c" else "速度", allowed_contexts=[], selection=[selection], reviewer="automated_source_review", interpretation_timing="before_synthetic_response"))
            bundle = case / ("bundle-" + ident)
            self.call(name + "-review-" + ident, ["extract", "verify", project, imported["import_id"], review_dir, bundle])
            inputs.append(dict(input_id=ident, review_bundle=bundle.name, binding=b, artifacts={file: fingerprint((bundle / file).read_bytes()) for file in FILES}))
        self.write_new(case / "coordination.json", dict(schema_version=1, purpose="explicit_identity_duplicate_complement", project_id=inputs[0]["binding"]["project_id"], identity_file="identity.json", inputs=inputs))
        self.protect(project); self.protect(case)
        return folder, project, case, expected, golds

    def coordinate(self, name, project, case, gold, output, exit_code=0):
        before = inventory(case)
        run = self.call(name, ["knowledge", "coordinate", "--", project, case, output], exit_code)
        exact(inventory(case), before, "CLI case readonly")
        self.check_protected()
        checked = verify_report(project, case, output, gold)
        exact(strict_json(run.stdout), strict_json((output / "coordination.json").read_bytes()), "CLI stdout report")
        assert (output / "coordination-ir.json").exists() == (exit_code == 0), "CLI success IR"
        self.rows.append(dict(name=name, kind="ordinary_coordinate_and_independent_check", project=str(project), case=str(case), output=str(output), gold_fingerprint=fingerprint(gold), check=checked, call_index=len(self.calls)))
        self.flush()
        return output

    def rejected(self, name, project, case, output, exit_code, code):
        before = inventory(case)
        run = self.call(name, ["knowledge", "coordinate", project, case, output], exit_code)
        assert code in (run.stdout + run.stderr).decode(), "CLI rejection code " + code
        assert not (output / "coordination-ir.json").exists(), "rejected CLI IR"
        exact(inventory(case), before, "rejected inputs readonly")
        self.check_protected()
        self.rows.append(dict(name=name, kind="ordinary_cli_rejection", project=str(project), case=str(case), output=str(output), expected_code=code, call_index=len(self.calls)))
        self.flush()

    def matrix(self):
        folder, project, case, gold, golds = self.prepare("normal")
        output = self.coordinate("normal-three", project, case, gold, folder / "output")
        # New cases share the immutable project but never modify a previous case.
        two = folder / "two-case"; shutil.copytree(case, two)
        manifest = strict_json((two / "coordination.json").read_bytes())
        manifest["inputs"] = [i for i in manifest["inputs"] if i["input_id"] != "b"]
        # Preserve the copied unselected bundle by moving it to another new path.
        (two / "bundle-b").rename(folder / "two-unselected-b")
        mapping = strict_json((two / "identity.json").read_bytes())
        for key in ("concept_mappings", "atom_mappings"):
            mapping[key] = [i for i in mapping[key] if i["input_id"] != "b"]
        (two / "coordination.json").write_bytes(encoded(manifest)); (two / "identity.json").write_bytes(encoded(mapping))
        two_authors = folder / "two-authors"; two_authors.mkdir()
        two_gold = self.freeze_expected(two_authors, golds, ["a", "c"])
        self.coordinate("normal-two", project, two, two_gold, folder / "two-output")
        reordered = folder / "reordered-case"; shutil.copytree(case, reordered)
        manifest = strict_json((reordered / "coordination.json").read_bytes()); manifest["inputs"].reverse()
        mapping = strict_json((reordered / "identity.json").read_bytes())
        for key in ("concepts", "concept_mappings", "atom_mappings"):
            mapping[key].reverse()
        for c in mapping["concepts"]:
            c["display_name"] += " 显示改名"
        (reordered / "coordination.json").write_bytes(encoded(manifest)); (reordered / "identity.json").write_bytes(encoded(mapping))
        order_output = self.coordinate("order-display", project, reordered, gold, folder / "order-output")
        original_r = strict_json((output / "coordination.json").read_bytes())
        reordered_r = strict_json((order_output / "coordination.json").read_bytes())
        for key in ("occurrences", "facts", "relations", "totals"):
            exact(original_r[key], reordered_r[key], "order stable " + key)
        for variant in ("condition", "unit", "version"):
            vf, vp, vc, vg, _ = self.prepare(variant, variant)
            self.coordinate(variant + "-needs-review", vp, vc, vg, vf / "output", 3)
            if variant == "unit":
                forged = vf / "forged-null-to-zero"; shutil.copytree(vf / "output", forged)
                report = strict_json((forged / "coordination.json").read_bytes())
                assert report["totals"]["canonical_facts"] is None
                report["totals"]["canonical_facts"] = 0
                self.rewrite_forgery(forged, report)
                try:
                    verify_report(vp, vc, forged, vg)
                except AssertionError as exc:
                    self.rows.append(dict(name="needs-review-null-to-zero", kind="independent_coherent_forgery_rejection", project=str(vp), case=str(vc), output=str(forged), diagnostic=str(exc)))
                else:
                    raise AssertionError("needs-review false zero accepted")
                self.check_protected(); self.flush()
        mutations = [
            ("same-sha-binding", 3, "COORD_SOURCE_REVIEW", lambda m, c: m["inputs"][1].update(binding=copy.deepcopy(m["inputs"][0]["binding"]))),
            ("duplicate-input", 2, "COORD_DUPLICATE_INPUT", lambda m, c: self.duplicate_input(m, c)),
            ("unknown-case", 2, "COORD_SCHEMA", lambda m, c: m.update(extra=True)),
            ("original-unknown-count", 3, "COORD_SOURCE_REVIEW", lambda m, c: self.mutate_bundle(m, c, "verification.json", lambda r: r["totals"].update(candidate_atoms=None))),
            ("original-ir-forgery", 3, "COORD_SOURCE_REVIEW", lambda m, c: self.mutate_bundle(m, c, "knowledge-ir.json", lambda r: r["atoms"][0]["measurement"].update(value="11"))),
            ("candidate-all-pins-forgery", 3, "COORD_SOURCE_REVIEW", lambda m, c: self.coherent_source_forgery(m, c)),
            ("source-expected-all-pins-forgery", 3, "COORD_SOURCE_REVIEW", lambda m, c: self.coherent_source_forgery(m, c, expected_only=True)),
            ("missing-support", 3, "COORD_SOURCE_REVIEW", lambda m, c: self.mutate_bundle(m, c, "candidates-original.json", lambda r: r["payload"]["atoms"][0]["atom"].update(evidence_links=[]))),
        ]
        for name, code, issue, mutation in mutations:
            c = folder / (name + "-case"); shutil.copytree(case, c)
            m = strict_json((c / "coordination.json").read_bytes()); mutation(m, c)
            (c / "coordination.json").write_bytes(encoded(m))
            self.rejected(name, project, c, folder / (name + "-output"), code, issue)
        # Fresh original-role wire failures: no expectations are taken from CLI.
        for role, field, values in (("gold", "author", (False, 17, None)), ("review", "reviewer", (False, 17, None)), ("decision", "unit_id", (False,))):
            for index, value in enumerate(values):
                name = f"original-{role}-{field}-type-{index}"
                c = folder / (name + "-case"); shutil.copytree(case, c)
                m = strict_json((c / "coordination.json").read_bytes())
                self.coherent_role_change(m, c, role, field, value)
                (c / "coordination.json").write_bytes(encoded(m))
                self.rejected(name, project, c, folder / (name + "-output"), 2, "COORD_SCHEMA")
        source_md_case = folder / "source-visible-publication-case"; shutil.copytree(case, source_md_case)
        m = strict_json((source_md_case / "coordination.json").read_bytes())
        item = m["inputs"][0]; bundle = source_md_case / item["review_bundle"]
        md_path = bundle / "verification.md"
        md_path.write_bytes(md_path.read_bytes().replace("发布：false".encode(), "发布：true".encode(), 1))
        item["artifacts"] = {name: fingerprint((bundle / name).read_bytes()) for name in FILES}
        (source_md_case / "coordination.json").write_bytes(encoded(m))
        self.rejected("source-visible-publication", project, source_md_case, folder / "source-visible-publication-output", 3, "COORD_SOURCE_REVIEW")
        duplicate = folder / "duplicate-json-case"; shutil.copytree(case, duplicate)
        p = duplicate / "coordination.json"
        p.write_bytes(p.read_bytes().replace(b'"schema_version": 1', b'"schema_version": 1, "schema_version": 1', 1))
        self.rejected("duplicate-json", project, duplicate, folder / "duplicate-json-output", 2, "COORD_SCHEMA")
        collision = folder / "human-output"; collision.mkdir(); (collision / "human.txt").write_bytes(b"keep human bytes")
        self.protect(collision)
        self.rejected("output-collision", project, case, collision, 2, "EXTRACT_OUTPUT")
        self.rejected("output-overlap", project, case, case / "nested-output", 2, "EXTRACT_OUTPUT")
        # Coherent result forgeries: JSON, MD, IR and all output pins agree.
        for name in ("missing-second", "wrong-fact", "missing-second-evidence", "false-independent", "false-published", "unknown-canonical-zero"):
            forged = folder / ("forged-" + name); shutil.copytree(output, forged)
            report = strict_json((forged / "coordination.json").read_bytes())
            if name == "missing-second":
                missing = next(o["occurrence_id"] for o in report["occurrences"] if o["input_id"] == "b")
                report["occurrences"] = [o for o in report["occurrences"] if o["occurrence_id"] != missing]
                for f in report["facts"]:
                    f["occurrence_ids"] = [i for i in f["occurrence_ids"] if i != missing]
                report["relations"] = [r for r in report["relations"] if missing not in (r["left_occurrence_id"], r["right_occurrence_id"])]
                for key in ("source_occurrences", "evidence_links", "independent_sources", "passed", "input_atoms"):
                    report["totals"][key] = 2
                report["totals"]["duplicate_occurrences"] = 0
            elif name == "wrong-fact":
                report["facts"][0]["value"] = "11"
            elif name == "missing-second-evidence":
                next(o for o in report["occurrences"] if o["input_id"] == "b")["original_evidence"] = []
                report["totals"]["evidence_links"] = 2
            elif name == "false-independent":
                report["totals"]["independent_sources"] = 2
            elif name == "false-published":
                report["published"] = True
            else:
                report["totals"]["canonical_facts"] = 0
            self.rewrite_forgery(forged, report)
            before = inventory(forged)
            try:
                verify_report(project, case, forged, gold)
            except AssertionError as exc:
                diagnostic = str(exc)
            else:
                raise AssertionError("coherent forgery accepted " + name)
            exact(before, inventory(forged), "forged output readonly")
            self.check_protected()
            self.rows.append(dict(name=name, kind="independent_coherent_forgery_rejection", project=str(project), case=str(case), output=str(forged), diagnostic=diagnostic))
            self.flush()
        forged = folder / "forged-trailing-publication"; shutil.copytree(output, forged)
        md = forged / "coordination.md"
        md.write_bytes(md.read_bytes() + "\n发布：true；科学质量：passed。\n".encode())
        before = inventory(forged)
        try:
            verify_report(project, case, forged, gold)
        except AssertionError as exc:
            diagnostic = str(exc)
        else:
            raise AssertionError("trailing Markdown publication accepted")
        exact(before, inventory(forged), "trailing Markdown readonly")
        self.check_protected()
        self.rows.append(dict(name="trailing-publication", kind="independent_coherent_forgery_rejection", project=str(project), case=str(case), output=str(forged), diagnostic=diagnostic))
        self.flush()
        after = self.check_protected()
        summary = dict(schema_version=1, root=str(self.root), cli=str(self.cli), cli_fingerprint=self.cli_fp, scenarios=self.rows, scenario_count=len(self.rows), calls=self.calls,
                       command_count=len(self.calls), protected_before=self.protected, protected_after=after, protection_passed=True, network_calls=0, paid_calls=0,
                       limitations=["Self-authored ordinary Text source fixtures, not independent textbooks or scientific accuracy.", "PDF verification uses the existing actual_source helper; this audit does not execute new PDF coordination.", "No CLI race hook; Task 1 Rust guard checks are separate evidence."])
        self.write_new(self.root / "audit.json", summary)
        return summary

    @staticmethod
    def mutate_bundle(manifest, case, file, mutation):
        item = manifest["inputs"][0]; folder = case / item["review_bundle"]
        path = folder / file; obj = strict_json(path.read_bytes()); mutation(obj); path.write_bytes(encoded(obj))
        # Re-pin all nine bytes. Even a coherent fingerprint manifest is no oracle.
        item["artifacts"] = {name: fingerprint((folder / name).read_bytes()) for name in FILES}

    @staticmethod
    def coherent_role_change(manifest, case, role, field, value):
        item = manifest["inputs"][0]; folder = case / item["review_bundle"]
        path = folder / (role + "-original.json")
        obj = strict_json(path.read_bytes()); obj[field] = value; path.write_bytes(encoded(obj))
        review = strict_json((folder / "review-original.json").read_bytes())
        review["gold_sha256"] = fingerprint((folder / "gold-original.json").read_bytes())["sha256"]
        (folder / "review-original.json").write_bytes(encoded(review))
        report = strict_json((folder / "verification.json").read_bytes())
        report["origin"].update(gold_sha256=review["gold_sha256"], reviewer=review["reviewer"])
        report["rules"]["reviewer"] = review["reviewer"]
        for name, saved in (("review.json", "review-original.json"), (review["gold_file"], "gold-original.json"), (review["decision_file"], "decision-original.json")):
            report["origin"]["input_fingerprints"][name] = fingerprint((folder / saved).read_bytes())
        if role == "decision":
            next(c for c in report["checks"] if c["category"] == "coverage")["actual"]["decision"] = obj
            ir = strict_json((folder / "knowledge-ir.json").read_bytes()); ir["checks"] = report["checks"]
            (folder / "knowledge-ir.json").write_bytes(encoded(ir))
        for name in report["artifacts"]:
            report["artifacts"][name] = fingerprint((folder / name).read_bytes())
        (folder / "verification.json").write_bytes(encoded(report))
        md = folder / "verification.md"
        md.write_bytes(md.read_bytes().split(b"```json\n", 1)[0] + b"```json\n" + encoded(report).rstrip() + b"\n```\n")
        item["artifacts"] = {name: fingerprint((folder / name).read_bytes()) for name in FILES}

    @staticmethod
    def coherent_source_forgery(manifest, case, expected_only=False):
        item = manifest["inputs"][0]; folder = case / item["review_bundle"]
        report = strict_json((folder / "verification.json").read_bytes())
        ir = strict_json((folder / "knowledge-ir.json").read_bytes())
        candidate = strict_json((folder / "candidates-original.json").read_bytes())
        if expected_only:
            next(c for c in report["checks"] if c["category"] == "content")["expected"] = "forged independent expectation"
        else:
            candidate["payload"]["atoms"][0]["measurement"]["value"] = "11"
            ir["atoms"] = candidate["payload"]["atoms"]
            next(c for c in report["checks"] if c["category"] == "fact")["actual"] = candidate["payload"]["atoms"][0]["measurement"]
            candidate_raw = encoded(candidate)
            (folder / "candidates-original.json").write_bytes(candidate_raw)
            review = strict_json((folder / "review-original.json").read_bytes())
            report["origin"]["input_fingerprints"][review["candidate_file"]] = fingerprint(candidate_raw)
        ir["checks"] = report["checks"]
        (folder / "knowledge-ir.json").write_bytes(encoded(ir))
        for name in report["artifacts"]:
            report["artifacts"][name] = fingerprint((folder / name).read_bytes())
        (folder / "verification.json").write_bytes(encoded(report))
        md_path = folder / "verification.md"
        prefix = md_path.read_bytes().split(b"```json\n", 1)[0]
        md_path.write_bytes(prefix + b"```json\n" + encoded(report).rstrip() + b"\n```\n")
        item["artifacts"] = {name: fingerprint((folder / name).read_bytes()) for name in FILES}

    @staticmethod
    def duplicate_input(manifest, case):
        first, second = manifest["inputs"][:2]
        for file in FILES:
            (case / second["review_bundle"] / file).write_bytes((case / first["review_bundle"] / file).read_bytes())
        second.update(binding=copy.deepcopy(first["binding"]), artifacts=copy.deepcopy(first["artifacts"]))
        # Preserve original b mapping roles while adapting original atom/concept IDs.
        path = case / manifest["identity_file"]; mapping = strict_json(path.read_bytes())
        for entry in mapping["concept_mappings"]:
            if entry["input_id"] == "b":
                entry["original_concept_id"] = entry["original_concept_id"].replace("b-", "a-", 1)
        next(a for a in mapping["atom_mappings"] if a["input_id"] == "b")["original_atom_id"] = "a-speed"
        path.write_bytes(encoded(mapping))

    @staticmethod
    def rewrite_forgery(output, report):
        ir_path = output / "coordination-ir.json"
        if ir_path.exists():
            ir = strict_json(ir_path.read_bytes())
            for key in ir:
                if key in report:
                    ir[key] = report[key]
            ir_path.write_bytes(encoded(ir))
        for name in report["artifacts"]:
            report["artifacts"][name] = fingerprint((output / name).read_bytes())
        body = encoded(report)
        (output / "coordination.json").write_bytes(body)
        md = (output / "coordination.md").read_bytes().split(b"```json\n", 1)[0]
        (output / "coordination.md").write_bytes(md + b"```json\n" + body.rstrip() + b"\n```\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("check", help="read-only independent verification")
    for name in ("project", "case", "output", "gold"):
        check.add_argument(name, type=Path)
    audit_parser = sub.add_parser("audit", help="actual CLI matrix in an exclusively new root")
    audit_parser.add_argument("cli", type=Path); audit_parser.add_argument("root", type=Path)
    args = parser.parse_args(argv)
    audit = None
    try:
        if args.command == "check":
            result = verify_report(args.project, args.case, args.output, args.gold.read_bytes())
        else:
            audit = Audit(args.cli, args.root)
            result = audit.matrix()
            result = dict(audit=str(audit.root / "audit.json"), scenarios=result["scenario_count"], commands=result["command_count"], protection_passed=True)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as exc:
        if audit:
            audit.flush(error=repr(exc))
        print("coordination verifier: " + str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
