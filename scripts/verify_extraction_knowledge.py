"""Audit ordinary source-review CLI from frozen author gold and actual SQL/bytes.

No Rust validator imports, HTTP, model configuration or credentials. This is a
bounded controlled-Chinese fixture audit, not scientific accuracy measurement.
All roots are new; failures and raw command receipts are retained.
"""
from __future__ import annotations

import argparse
from contextlib import closing
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
STATES = ("passed", "failed", "needs_review", "unprocessed")


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def fingerprint(raw):
    return {"sha256": sha(raw), "byte_length": len(raw)}


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def save(path, value):
    path.write_bytes(encoded(value))


def strict_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            assert key not in result, "duplicate JSON key: " + key
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda x: (_ for _ in ()).throw(AssertionError(x)))


def fields(obj, names):
    assert isinstance(obj, dict) and set(obj) == set(names.split()), "schema fields"


def norm(text):
    return "".join(text.split())


def same_set(actual, expected):
    values = [norm(x) for x in actual]
    return len(values) == len(set(values)) and set(values) == {norm(x) for x in expected}


def predicate(content, review, gold, *, source=False):
    """Parse only declared grammar; do not infer correctness from substrings."""
    text = norm(content)
    subject, prop = norm(review["subject"]), norm(review["property"])
    unknown = gold.get("value_status", "known") == "unknown"
    base = subject + "的" + prop + ("未知" if unknown else "为" + norm(gold["value"]) + norm(gold["unit"] or ""))
    roots = [base]
    if not unknown or source:
        roots += [norm(c) + "，" + prop + ("未知" if unknown else "为" + norm(gold["value"]) + norm(gold["unit"] or "")) for c in review["allowed_contexts"]]
    for root in roots:
        if not text.startswith(root):
            continue
        rest = text[len(root):]
        if rest.endswith("。"):
            rest = rest[:-1]
        exception_parts = rest.split("；例外为")
        if len(exception_parts) > 2:
            return None
        tail = exception_parts[0]
        conditions = []
        if tail:
            match = re.fullmatch(r"[,，;；。]条件[为是](.+)", tail)
            if not match:
                continue
            conditions = re.split("，且|且|、|，", match[1])
        exceptions = re.split("，且|且|、|，", exception_parts[1]) if len(exception_parts) == 2 else []
        if any(not x for x in conditions + exceptions):
            return None
        return conditions, exceptions
    return None


def inventory(root):
    """Only explicitly selected test projects/cases; record bytes and identities."""
    values = {}
    for path in [root, *sorted(root.rglob("*"))]:
        assert not path.is_symlink(), "unexpected link"
        stat = path.stat()
        values[path.relative_to(root).as_posix()] = {
            "directory": path.is_dir(), "mtime_ns": stat.st_mtime_ns,
            "identity": [stat.st_dev, stat.st_ino], "links": stat.st_nlink,
            **(fingerprint(path.read_bytes()) if path.is_file() else {})}
    return values


def sql_snapshot(project):
    with closing(sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True)) as db:
        return list(db.iterdump())


def actual_source(project, import_id, selection):
    """Rebuild binding/locator from SQLite and registered resources, not report."""
    project = project.resolve()
    with closing(sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True)) as db:
        db.row_factory = sqlite3.Row
        project_id = db.execute("SELECT project_id FROM project_metadata").fetchone()[0]
        if selection["kind"] == "text":
            imp = db.execute("SELECT * FROM source_imports WHERE import_id=?", (import_id,)).fetchone()
            assert imp and imp["status"] == "ready", "unavailable source"
            ver = db.execute("SELECT * FROM source_versions WHERE version_id=?", (imp["version_id"],)).fetchone()
            span = db.execute("SELECT * FROM source_spans WHERE source_version_id=? AND ordinal=?", (ver["version_id"], selection["ordinal"])).fetchone()
            assert span, "unavailable span"
            raw = (project / ver["raw_snapshot"]).read_bytes()
            resource = (project / ver["normalized_snapshot"]).read_bytes()
            assert sha(raw) == ver["content_sha256"] and sha(resource) == ver["normalized_sha256"], "source SHA"
            start, end = span["start_byte"], span["end_byte"]
            assert sha(resource[start:end]) == span["text_sha256"], "source span SHA"
            locator = {"kind": "text", "span_id": span["span_id"], "ordinal": span["ordinal"], "resource_sha256": sha(resource)}
            status, parse_id, manifest_sha = span["parse_status"], None, None
        else:
            imp = db.execute("SELECT * FROM pdf_imports WHERE import_id=?", (import_id,)).fetchone()
            assert imp and imp["status"] in ("ready", "needs_review"), "unavailable PDF source"
            ver = db.execute("SELECT * FROM source_versions WHERE version_id=?", (imp["version_id"],)).fetchone()
            attempt = db.execute("SELECT * FROM pdf_parses WHERE parse_id=?", (imp["parse_id"],)).fetchone()
            region = db.execute("SELECT * FROM pdf_regions WHERE parse_id=? AND region_id=? AND physical_page_index=?", (imp["parse_id"], selection["region_id"], selection["page_index"])).fetchone()
            assert region, "unavailable PDF region"
            res = db.execute("SELECT * FROM pdf_resources WHERE parse_id=? AND resource_id=?", (imp["parse_id"], region["text_resource_id"])).fetchone()
            page = db.execute("SELECT * FROM pdf_pages WHERE parse_id=? AND physical_page_index=?", (imp["parse_id"], selection["page_index"])).fetchone()
            parse_root = project / attempt["snapshot_path"]
            raw = (project / ver["raw_snapshot"]).read_bytes()
            resource = (parse_root / res["relative_path"]).read_bytes()
            manifest_bytes = (parse_root / "parser/pdf-manifest.json").read_bytes()
            manifest = strict_json(manifest_bytes)
            assert sha(raw) == ver["content_sha256"] and fingerprint(resource) == {"sha256": res["sha256"], "byte_length": res["byte_length"]}, "PDF source SHA"
            assert sha(manifest_bytes) == attempt["manifest_sha256"], "PDF manifest SHA"
            actual_region = next(r for r in manifest["pages"][selection["page_index"]]["regions"] if r["region_id"] == region["region_id"])
            bbox = [region["bbox_" + k] for k in ("left", "top", "right", "bottom")]
            assert actual_region["bbox"] == bbox, "PDF bbox SQL mismatch"
            start, end = region["start_byte"], region["end_byte"]
            assert actual_region["text"]["start_byte"] == start and actual_region["text"]["end_byte"] == end, "PDF range SQL mismatch"
            locator = dict(kind="pdf", parse_id=imp["parse_id"], page_index=selection["page_index"], region_id=region["region_id"], resource_id=res["resource_id"], resource_sha256=sha(resource), bbox=bbox)
            status, parse_id, manifest_sha = page["status"], imp["parse_id"], sha(manifest_bytes)
        binding = {"project_id": project_id, "import_id": import_id, **{k: ver[k] for k in ("source_id", "version_id", "content_sha256")}, "normalized_sha256": ver["normalized_sha256"], "parse_id": parse_id, "manifest_sha256": manifest_sha}
        return {"binding": binding, "units": [{"unit_id": "unit-0", "locator": locator, "source_status": status, "start_byte": start, "end_byte": end, "text": resource[start:end].decode("utf-8")}], "resource_sha256": sha(resource), "raw_sha256": sha(raw)}


def candidate_schema(candidate):
    fields(candidate, "schema_version producer transport_mode request_id binding payload")
    assert candidate["schema_version"] == 1 and candidate["producer"] == "api"
    assert candidate["transport_mode"] in ("live", "local_http_test")
    binding_fields = "project_id import_id source_id version_id content_sha256 normalized_sha256 parse_id manifest_sha256"
    fields(candidate["binding"], binding_fields)
    payload = candidate["payload"]
    fields(payload, "schema_version request_id binding concepts atoms evidence")
    fields(payload["binding"], binding_fields)
    assert payload["schema_version"] == 1 and isinstance(payload["request_id"], str)
    for category in ("concepts", "atoms", "evidence"):
        assert isinstance(payload[category], list) and len(payload[category]) <= 1024
    for c in payload["concepts"]:
        fields(c, "concept_id revision canonical_name aliases needs_full_explanation")
        assert type(c["revision"]) is int and c["revision"] > 0
    for e in payload["evidence"]:
        fields(e, "evidence_id unit_id excerpt_start_byte excerpt_end_byte excerpt_sha256 quote")
    for item in payload["atoms"]:
        fields(item, "atom measurement")
        fields(item["atom"], "atom_id revision kind concept_ids content qualifiers evidence_links required verification value_status")
        fields(item["measurement"], "value_status value unit conditions")
        a, m = item["atom"], item["measurement"]
        assert type(a["revision"]) is int and a["revision"] > 0
        assert a["kind"] in ("definition", "comparison_value", "formula", "procedure", "boundary", "counterexample", "relation")
        assert a["verification"] == "unverified" and a["required"] is False
        assert a["value_status"] in ("known", "unknown") and m["value_status"] in ("known", "unknown")
        for link in a["evidence_links"]:
            fields(link, "evidence_id relation")
            assert link["relation"] in ("supports", "contradicts", "context")


def judgments(candidate, gold, review, decision, source):
    """Independent primary obligations plus every actual occurrence's facts."""
    payload = candidate["payload"]
    atoms, unit = payload["atoms"], source["units"][0]
    yes = lambda v: "passed" if v else "failed"
    result = {"schema": ["passed"], "source": ["passed" if unit["source_status"] == "parsed" else "needs_review"],
              "binding": [yes(candidate["binding"] == payload["binding"] == source["binding"] and candidate["request_id"] == payload["request_id"])],
              "exclusion": [{"included": "passed", "excluded": "failed", "pending": "unprocessed", "needs_review": "needs_review"}[decision["disposition"]]],
              "coverage": [yes(len(atoms) == 1 and decision["unit_id"] == unit["unit_id"] and len(decision["atom_ids"]) == 1 and decision["atom_ids"][0] in [x["atom"]["atom_id"] for x in atoms])]}
    by_atom = []
    for item in atoms:
        a, m = item["atom"], item["measurement"]
        parsed = predicate(a["content"], review, gold)
        quals = a["qualifiers"]
        conditions = m["conditions"]
        ex = gold.get("exceptions", [])
        supported = []
        evidence_ok = False
        for link in a["evidence_links"]:
            if link["relation"] != "supports":
                continue
            e = next((x for x in payload["evidence"] if x["evidence_id"] == link["evidence_id"]), None)
            if not e:
                continue
            start, end = e["excerpt_start_byte"], e["excerpt_end_byte"]
            body = unit["text"].encode()
            valid = (e["unit_id"] == unit["unit_id"] and type(start) is int and type(end) is int and unit["start_byte"] <= start < end <= unit["end_byte"]
                     and body[start-unit["start_byte"]:end-unit["start_byte"]] == e["quote"].encode() and sha(e["quote"].encode()) == e["excerpt_sha256"])
            if valid:
                supported.append(e["quote"])
                evidence_ok |= gold["quote"] in e["quote"]
        linked = [c for c in payload["concepts"] if c["concept_id"] in a["concept_ids"]]
        names = {norm(n) for c in linked for n in [c["canonical_name"], *c["aliases"]]}
        group = norm(gold["concept_name"]) in names or {norm(review["subject"]), norm(review["property"])} <= names
        extras_supported = all(norm(n) in norm(" ".join(supported)) for c in payload["concepts"] for n in [c["canonical_name"], *c["aliases"]] if norm(n) != norm(gold["concept_name"]))
        unknown = gold.get("value_status", "known") == "unknown"
        fact = parsed is not None and a["value_status"] == m["value_status"] == ("unknown" if unknown else "known") and m["value"] == gold["value"]
        condstates = [yes(norm(c) in {norm(x) for x in conditions} and norm(c) in {norm(x) for x in quals} and parsed is not None and norm(c) in parsed[0]) for c in gold["conditions"]]
        exstates = [yes(norm(x) in {norm(q) for q in quals} and parsed is not None and norm(x) in parsed[1]) for x in ex]
        tests = {"concept": [yes(group and extras_supported)], "evidence": [yes(evidence_ok)], "fact": ["needs_review" if unknown and fact else yes(fact)],
                 "content": [yes(parsed is not None and same_set(parsed[0], gold["conditions"]) and same_set(parsed[1], ex))],
                 "condition": condstates, "unit": [yes(m["unit"] == gold["unit"] and gold["unit"] in quals)] if gold["unit"] is not None else [], "exception": exstates}
        context = {norm(c) for c in review["allowed_contexts"]}
        residual_conditions = [x for x in conditions if norm(x) not in context]
        allowed_quals = [*gold["conditions"], *ex, *review["allowed_contexts"], *([gold["unit"]] if gold["unit"] else [])]
        structural = (same_set(residual_conditions, gold["conditions"]) and len(conditions) == len(set(map(norm, conditions))) and same_set(quals, [x for x in allowed_quals if norm(x) in set(map(norm, quals))]))
        structural &= all(cid in {x["concept_id"] for x in payload["concepts"]} for cid in a["concept_ids"]) and bool(a["concept_ids"])
        structural &= len(a["concept_ids"]) == len(set(a["concept_ids"]))
        by_atom.append({"tests": tests, "extra_valid": structural})
    if by_atom:
        result.update(by_atom[0]["tests"])
    return result, by_atom


def verify_report(output, candidate_bytes, gold_bytes, actual_source, inputs):
    """Validate saved externally visible artifacts; AssertionError on forgery."""
    output = Path(output)
    report = strict_json((output / "verification.json").read_bytes())
    fields(report, "schema_version report_kind producer request_id status knowledge_status published scientific_quality research_status network_calls paid_calls scope binding origin rules expected_results candidate_results checks totals quality artifacts")
    md = (output / "verification.md").read_text(encoding="utf-8")
    matches = re.findall(r"```json\n(.*?)\n```", md, re.S)
    assert len(matches) == 1 and strict_json(matches[0]) == report, "Markdown JSON mismatch"
    assert report["schema_version"] == 1 and report["report_kind"] == "source_knowledge_verification" and report["producer"] == "source_review", "report schema"
    assert report["knowledge_status"] == "pending" and report["published"] is False and report["scientific_quality"] == "not_run" and report["research_status"] == "not_requested", "publication/scientific state"
    assert report["network_calls"] == report["paid_calls"] == 0, "network state"
    review = strict_json(inputs["review.json"])
    gold = strict_json(gold_bytes)
    decision = strict_json(inputs[review["decision_file"]])
    gold_fields = "schema_version author producer scientific_quality concept_name atom_content value unit conditions quote verification required knowledge_status published"
    fields(gold, gold_fields + (" exceptions value_status" if gold["schema_version"] == 2 else ""))
    assert gold["schema_version"] in (1, 2), "gold version"
    for context in review["allowed_contexts"]:
        assert norm(context) == "在本夹具的" + norm(review["subject"]) + "中" and context in gold["quote"], "source context declaration"
    fields(report["scope"], "purpose selection binding units expected_atoms")
    fields(report["origin"], "producer transport_mode reviewer interpretation_timing input_fingerprints gold_sha256")
    fields(report["totals"], "expected_atoms passed failed needs_review unprocessed candidate_atoms unexpected_atoms duplicate_atoms required_concept_groups additional_source_concepts")
    assert review["gold_sha256"] == sha(gold_bytes), "gold SHA"
    assert report["binding"] == actual_source["binding"], "source binding mismatch"
    unit = actual_source["units"][0]
    assert report["scope"]["units"] == actual_source["units"] and report["scope"]["binding"] == actual_source["binding"], "source slice/SHA mismatch"
    assert report["scope"]["selection"] == review["selection"] and report["scope"]["expected_atoms"] == 1 and report["scope"]["purpose"] == review["purpose"], "source scope"
    snapshot = strict_json((output / "source-snapshot.json").read_bytes())
    assert snapshot == {"schema_version": 1, "binding": actual_source["binding"], "units": actual_source["units"]}, "source snapshot mismatch"
    assert gold["quote"] in unit["text"], "actual source quote"
    source_predicate = predicate(gold["quote"], review, gold, source=True)
    assert source_predicate and same_set(source_predicate[0], gold["conditions"]) and same_set(source_predicate[1], gold.get("exceptions", [])), "actual source fact"
    assert norm(gold["concept_name"]) == norm(review["subject"] + review["property"]), "gold concept"
    copies = {"review.json": "review-original.json", review["candidate_file"]: "candidates-original.json", review["gold_file"]: "gold-original.json", review["decision_file"]: "decision-original.json"}
    assert inputs[review["candidate_file"]] == candidate_bytes and inputs[review["gold_file"]] == gold_bytes, "frozen input bytes"
    for name, copy_name in copies.items():
        raw = inputs[name]
        assert (output / copy_name).read_bytes() == raw, "copied input bytes"
    assert report["origin"]["input_fingerprints"] == {k: fingerprint(inputs[k]) for k in copies}, "input fingerprint mismatch"
    assert report["origin"]["gold_sha256"] == sha(gold_bytes), "gold fingerprint"
    for key in ("reviewer", "interpretation_timing"):
        assert report["origin"][key] == review[key] == report["rules"][key], "origin provenance mismatch"
    assert report["rules"] == {"grammar": "controlled_measurement_zh_v1", **{k: review[k] for k in ("subject", "property", "allowed_contexts", "reviewer", "interpretation_timing")}}, "rules provenance"
    for name, fp in report["artifacts"].items():
        assert Path(name).name == name and name not in ("verification.json", "verification.md"), "artifact path"
        assert fingerprint((output / name).read_bytes()) == fp, "artifact fingerprint mismatch: " + name
    artifact_names = {".owner.json", *copies.values(), "source-snapshot.json"}
    if report["status"] == "passed":
        artifact_names.add("knowledge-ir.json")
    assert set(report["artifacts"]) == artifact_names, "artifact inventory/IR status"
    owner = strict_json((output / ".owner.json").read_bytes())
    assert owner["producer"] == "source_review" and owner["purpose"] == "unpublished_source_review", "output owner"
    try:
        candidate = strict_json(candidate_bytes)
        syntax = True
    except (ValueError, AssertionError):
        candidate, syntax = None, False
    schema = False
    if syntax:
        try:
            candidate_schema(candidate)
            schema = True
        except (AssertionError, KeyError, TypeError):
            pass
    payload = candidate.get("payload", {}) if isinstance(candidate, dict) else {}
    raw_atoms = payload.get("atoms") if isinstance(payload, dict) else None
    count = len(raw_atoms) if isinstance(raw_atoms, list) else (0 if syntax else None)
    totals = report["totals"]
    assert totals["expected_atoms"] == 1 and totals["required_concept_groups"] == 1, "author denominator"
    assert totals["candidate_atoms"] == count and totals["unexpected_atoms"] == (max(0, count - 1) if count is not None else None), "actual candidate count"
    assert report["status"] in STATES and sum(totals[s] for s in STATES) == 1 and totals[report["status"]] == 1, "status partition"
    expected = report["expected_results"]
    assert len(expected) == 1 and expected[0]["status"] == report["status"] and expected[0]["object_id"] == "expected-0", "expected result status"
    assert len(report["candidate_results"]) == (count or 0), "actual result count"
    for result in [*expected, *report["candidate_results"]]:
        fields(result, "object_id unit_id start_byte end_byte status issue_codes")
        assert result["unit_id"] == unit["unit_id"] and result["start_byte"] == unit["start_byte"] and result["end_byte"] == unit["end_byte"], "result source range"
        assert result["status"] in STATES and isinstance(result["issue_codes"], list), "result state"
    denominators = {k: 1 for k in ("source", "schema", "binding", "concept", "fact", "content", "evidence", "coverage", "exclusion")}
    denominators.update(condition=len(gold["conditions"]), unit=int(gold["unit"] is not None), exception=len(gold.get("exceptions", [])))
    assert set(report["quality"]) == set(denominators), "quality schema"
    for category, n in denominators.items():
        q = report["quality"][category]
        fields(q, "expected passed failed needs_review unprocessed status")
        assert q["expected"] == n and sum(q[s] for s in STATES) == n, "quality denominator: " + category
        state = "not_applicable" if n == 0 else next((s for s in ("failed", "needs_review", "unprocessed") if q[s]), "passed")
        assert q["status"] == state, "quality status: " + category
    for check in report["checks"]:
        fields(check, "category object_id unit_id start_byte end_byte status code expected actual")
        assert check["status"] in STATES and check["unit_id"] == unit["unit_id"] and unit["start_byte"] <= check["start_byte"] < check["end_byte"] <= unit["end_byte"], "check location/state"
    independently_passed = False
    if schema:
        assert report["origin"]["producer"] == "api" and report["origin"]["transport_mode"] == candidate["transport_mode"] and report["request_id"] == candidate["request_id"], "origin provenance"
        if review["interpretation_timing"] == "before_synthetic_response":
            assert candidate["transport_mode"] == "local_http_test", "synthetic provenance"
        obligations, atoms = judgments(candidate, gold, review, decision, actual_source)
        for category, states in obligations.items():
            q = report["quality"][category]
            assert all(q[s] == states.count(s) for s in STATES), "independent " + category + " result"
            observations = [c for c in report["checks"] if c["category"] == category][:len(states)]
            assert [c["status"] for c in observations] == states, "independent check " + category + " state"
        for category in ("fact", "condition", "unit", "exception", "content"):
            observations = [c for c in report["checks"] if c["category"] == category]
            if atoms:
                for check in observations[:len(atoms[0]["tests"].get(category, []))]:
                    item = candidate["payload"]["atoms"][0]
                    if category == "fact":
                        assert check["actual"] == item["measurement"], "check actual measurement"
                    elif category == "unit":
                        assert check["actual"] == {"measurement": item["measurement"]["unit"], "qualifiers": item["atom"]["qualifiers"]}, "check actual unit"
                    elif category == "content":
                        assert check["actual"] == item["atom"]["content"], "check actual content"
                    elif category == "condition":
                        parsed = predicate(item["atom"]["content"], review, gold)
                        assert check["actual"] == {"measurement": item["measurement"]["conditions"], "qualifiers": item["atom"]["qualifiers"], "body": parsed[0] if parsed else None}, "check actual condition"
                        assert check["expected"] in gold["conditions"], "check author condition"
                    elif category == "exception":
                        parsed = predicate(item["atom"]["content"], review, gold)
                        assert check["actual"] == {"qualifiers": item["atom"]["qualifiers"], "body": parsed[1] if parsed else None}, "check actual exception"
                        assert check["expected"] in gold.get("exceptions", []), "check author exception"
        signatures = []
        for item in candidate["payload"]["atoms"]:
            a = item["atom"]
            parsed = predicate(a["content"], review, gold)
            canonical = (norm(gold["concept_name"]) if parsed and same_set(parsed[0], gold["conditions"]) and same_set(parsed[1], gold.get("exceptions", [])) else norm(a["content"]))
            signatures.append(canonical)
        duplicate = len(signatures) - len(set(signatures))
        assert totals["duplicate_atoms"] == duplicate, "semantic duplicate count"
        required_names = {norm(gold["concept_name"]), norm(review["subject"]), norm(review["property"])}
        additional = sum(norm(c["canonical_name"]) not in required_names for c in candidate["payload"]["concepts"]) if atoms else 0
        assert totals["additional_source_concepts"] == additional, "additional concept count"
        independently_passed = (all(all(s == "passed" for s in v) for v in obligations.values()) and all(a["extra_valid"] for a in atoms) and duplicate == 0)
        ids_unique = all(len({x[k] for x in candidate["payload"][category]}) == len(candidate["payload"][category]) for category, k in (("concepts", "concept_id"), ("evidence", "evidence_id")))
        ids = [x["atom"]["atom_id"] for x in candidate["payload"]["atoms"]]
        ids_unique &= len(ids) == len(set(ids))
        independently_passed &= ids_unique
        independent_status = "passed" if independently_passed else "failed"
        if not atoms or decision["disposition"] == "pending":
            independent_status = "unprocessed"
        elif gold.get("value_status") == "unknown" or unit["source_status"] != "parsed" or decision["disposition"] == "needs_review":
            independent_status = "needs_review"
        assert report["status"] == independent_status, "independent report status"
        for index, (item, result) in enumerate(zip(candidate["payload"]["atoms"], report["candidate_results"])):
            assert result["object_id"] == item["atom"]["atom_id"], "result occurrence identity"
            own = atoms[index]
            own_pass = all(all(s == "passed" for s in v) for v in own["tests"].values()) and own["extra_valid"] and ids_unique and signatures[index] not in signatures[:index] and obligations["binding"] == ["passed"]
            own_status = "passed" if own_pass else "failed"
            if own_pass and independent_status in ("needs_review", "unprocessed"):
                own_status = independent_status
            # An unknown but otherwise sound fact is a real review obligation.
            if gold.get("value_status") == "unknown" and not any(s == "failed" for v in own["tests"].values() for s in v) and own["extra_valid"] and ids_unique:
                own_status = "needs_review"
            assert result["status"] == own_status, "result occurrence state"
        if report["status"] == "passed":
            assert independently_passed, "false passed fact/content/condition/unit/exception"
    else:
        assert totals["duplicate_atoms"] is None and report["status"] != "passed", "schema unknown count/status"
        assert report["quality"]["schema"]["failed"] == 1, "schema failure"
    ir_path = output / "knowledge-ir.json"
    assert ir_path.exists() == (report["status"] == "passed"), "IR status mismatch"
    if ir_path.exists():
        assert independently_passed, "IR false success"
        ir = strict_json(ir_path.read_bytes())
        fields(ir, "schema_version producer revision request_id binding source_match concepts atoms evidence checks")
        assert ir["schema_version"] == ir["revision"] == 1 and ir["producer"] == "source_review" and ir["source_match"] == "passed", "IR schema"
        assert ir["binding"] == actual_source["binding"] and ir["request_id"] == candidate["request_id"], "IR binding"
        for key in ("concepts", "atoms", "evidence"):
            assert ir[key] == candidate["payload"][key], "IR original " + key + " mismatch"
        assert ir["checks"] == report["checks"], "IR checks mismatch"
    return {"status": report["status"], "expected_atoms": 1, "candidate_atoms": count, "independently_passed": independently_passed, "denominators": denominators}


class Audit:
    def __init__(self, binary, root):
        self.binary, self.root = binary.resolve(), root.resolve()
        self.root.mkdir()  # exclusive new run, no cleanup/adoption
        self.calls, self.rows, self.projects = [], [], []
        fixture = REPO / "fixtures/extraction-review-v1"
        self.gold_bytes = (fixture / "gold.json").read_bytes()  # before ALL CLI
        self.gold = strict_json(self.gold_bytes)
        self.template = strict_json((fixture / "candidates.json").read_bytes())
        self.review = strict_json((fixture / "review.json").read_bytes())
        self.original_gold = (REPO / "fixtures/extraction-gateway-v1/expected.json").read_bytes()
        self.pdf_gold = {"schema_version": 2, "author": "independent-controlled-pdf-author", "producer": "fake", "scientific_quality": "not_run", "concept_name": "A速度", "atom_content": "A的速度为10m/s，条件为T20C。", "value_status": "known", "value": "10", "unit": "m/s", "conditions": ["T20C"], "exceptions": [], "quote": "A的速度为10m/s。条件是T20C。", "verification": "unverified", "required": False, "knowledge_status": "pending", "published": False}
        self.unknown_gold = dict(self.gold, author="independent-unknown-author", value_status="unknown", value=None, unit=None, atom_content="样例 A 的速度未知，条件为温度 20 ℃且仅适用样例 A；例外为结冰时不适用。", quote="在本夹具的样例 A 中，速度未知。条件是温度 20 ℃、仅适用样例 A；例外为结冰时不适用。")
        for name, raw in (("author-exception-gold.json", self.gold_bytes), ("author-original-gold.json", self.original_gold), ("author-pdf-gold.json", encoded(self.pdf_gold)), ("author-unknown-gold.json", encoded(self.unknown_gold))):
            (self.root / name).write_bytes(raw)
        self.author_receipt = {p.name: fingerprint(p.read_bytes()) for p in self.root.glob("author-*.json")}
        self.project = self.root / "project 中文"
        self.call(["project", "init", str(self.project)], 0)
        imported = self.call(["source", "import", str(self.project), str(fixture / "source.txt")], 0)
        self.import_id = strict_json(imported.stdout)["import_id"]
        self.selection = {"kind": "text", "ordinal": 0}
        self.source = actual_source(self.project, self.import_id, self.selection)
        self.capture(self.project)

    def call(self, args, expected=None):
        argv = [str(self.binary), *args]
        runtime_env = dict(os.environ, NOETOME_PDF_PYTHON=sys.executable, NOETOME_TEST_PYTHON=sys.executable)
        run = subprocess.run(argv, capture_output=True, timeout=40, env=runtime_env)
        receipt = {"argv": argv, "exit_code": run.returncode, "stdout": run.stdout.decode("utf-8"), "stderr": run.stderr.decode("utf-8"), "stdout_fingerprint": fingerprint(run.stdout), "stderr_fingerprint": fingerprint(run.stderr)}
        self.calls.append(receipt)
        save(self.root / f"call-{len(self.calls):03}.json", receipt)
        self.write_audit()
        if expected is not None:
            assert run.returncode == expected, ("product CLI unexpected exit", receipt)
        return run

    def capture(self, project):
        before = {"path": str(project), "inventory": inventory(project), "sql": sql_snapshot(project)}
        self.projects.append(before)
        return before

    def unchanged(self, before):
        project = Path(before["path"])
        assert inventory(project) == before["inventory"], "source bytes/mtime/identity/inventory changed"
        assert sql_snapshot(project) == before["sql"], "source SQL changed"
        before["inventory_after"] = inventory(project)
        before["sql_after"] = sql_snapshot(project)

    def write_audit(self, error=None):
        save(self.root / "audit.json", {"schema_version": 1, "scope": "offline_engineering_matrix", "scientific_quality": "not_run", "research_status": "not_requested", "network_calls": 0, "paid_calls": 0, "binary": str(self.binary), "binary_fingerprint": fingerprint(self.binary.read_bytes()), "author_gold_before_cli": getattr(self, "author_receipt", {}), "calls": self.calls, "rows": self.rows, "scenario_count": len(self.rows), "report_count": sum("verification" in r for r in self.rows), "passed_knowledge_samples": sum(r.get("verification", {}).get("status") == "passed" for r in self.rows), "source_receipts": self.projects, "error": error})

    def make_case(self, name, *, gold_bytes=None, source=None, selection=None, review_change=None, candidate_change=None, decision_change=None, raw_change=None, input_change=None):
        gold_bytes = self.gold_bytes if gold_bytes is None else gold_bytes
        gold = strict_json(gold_bytes)
        source = self.source if source is None else source
        selected = self.selection if selection is None else selection
        case = self.root / ("case-" + name)
        case.mkdir()
        (case / "gold.json").write_bytes(gold_bytes)  # before synthetic serialization
        candidate = copy.deepcopy(self.template)
        candidate["binding"] = copy.deepcopy(source["binding"])
        candidate["payload"]["binding"] = copy.deepcopy(source["binding"])
        candidate["request_id"] = candidate["payload"]["request_id"] = "synthetic-" + name
        candidate["payload"]["concepts"][0]["canonical_name"] = gold["concept_name"]
        a, m = candidate["payload"]["atoms"][0]["atom"], candidate["payload"]["atoms"][0]["measurement"]
        a.update(content=gold["atom_content"], qualifiers=[*([gold["unit"]] if gold["unit"] else []), *gold["conditions"], *gold.get("exceptions", [])], value_status=gold.get("value_status", "known"))
        m.update(value_status=gold.get("value_status", "known"), value=gold["value"], unit=gold["unit"], conditions=gold["conditions"])
        unit = source["units"][0]
        quote = gold["quote"].encode()
        start = unit["start_byte"] + unit["text"].encode().index(quote)
        candidate["payload"]["evidence"][0].update(quote=gold["quote"], excerpt_start_byte=start, excerpt_end_byte=start + len(quote), excerpt_sha256=sha(quote))
        review = copy.deepcopy(self.review)
        review.update(gold_sha256=sha(gold_bytes), selection=[selected])
        if gold == self.pdf_gold:
            review.update(subject="A", property="速度", allowed_contexts=[])
        decision = {"schema_version": 1, "unit_id": unit["unit_id"], "disposition": "included", "atom_ids": [a["atom_id"]], "exclusion_reason": None}
        if candidate_change:
            candidate_change(candidate)
        if review_change:
            review_change(review)
        if decision_change:
            decision_change(decision)
        raw = encoded(candidate)
        if raw_change:
            raw = raw_change(raw)
        (case / "candidates.json").write_bytes(raw)
        save(case / "review.json", review)
        save(case / "decision.json", decision)
        if input_change:
            input_change(case)
        return case

    def run_case(self, name, *, expected=3, code=None, status=None, project=None, import_id=None, source=None, output=None, separator=False, **options):
        project = self.project if project is None else project
        import_id = self.import_id if import_id is None else import_id
        source = self.source if source is None else source
        case = self.make_case(name, source=source, **options)
        before, project_before = inventory(case), self.capture(project)
        output = self.root / ("out-" + name) if output is None else output
        args = ["extract", "verify", *(["--"] if separator else []), str(project), import_id, str(case), str(output)]
        run = self.call(args, expected)
        assert inventory(case) == before, "selected inputs changed"
        self.unchanged(project_before)
        if code:
            assert code in run.stdout.decode() + run.stderr.decode(), (name, code, self.calls[-1])
        row = {"name": name, "expected_exit": expected, "observed_exit": run.returncode, "engineering_passed": True, "case": str(case), "output": str(output), "call_index": len(self.calls)}
        if (output / "verification.json").exists():
            inputs = {p.name: p.read_bytes() for p in case.iterdir() if p.is_file()}
            row["verification"] = verify_report(output, inputs["candidates.json"], inputs["gold.json"], source, inputs)
            if status:
                assert row["verification"]["status"] == status, (name, row)
            assert strict_json(run.stdout) == strict_json((output / "verification.json").read_bytes()), "stdout saved JSON"
            row["output_fingerprints"] = {p.name: fingerprint(p.read_bytes()) for p in output.iterdir() if p.is_file()}
        else:
            assert expected != 0, "missing passed report"
        if expected != 0:
            assert not (output / "knowledge-ir.json").exists(), "rejected case retained IR"
        self.rows.append(row)
        self.write_audit()
        return output

    def matrix(self):
        self.run_case("normal-exception", expected=0, status="passed", separator=True)
        def split(c):
            p = c["payload"]
            p["concepts"] = [{"concept_id": "subject", "revision": 1, "canonical_name": "样例 A", "aliases": [], "needs_full_explanation": True}, {"concept_id": "property", "revision": 1, "canonical_name": "速度", "aliases": [], "needs_full_explanation": True}]
            p["atoms"][0]["atom"]["concept_ids"] = ["subject", "property"]
            p["atoms"][0]["atom"]["content"] = self.gold["quote"]
            p["atoms"][0]["measurement"]["conditions"].append("在本夹具的样例 A 中")
            p["atoms"][0]["atom"]["qualifiers"].append("在本夹具的样例 A 中")
        self.run_case("split-concepts-context", expected=0, candidate_change=split)
        def atomedit(fn):
            return lambda c: fn(c["payload"]["atoms"][0])
        variants = [
            ("condition-missing", "VERIFY_CONDITION", atomedit(lambda a: (a["measurement"].update(conditions=[]), a["atom"].update(qualifiers=["m/s", "结冰时不适用"])))),
            ("unit-missing", "VERIFY_UNIT", atomedit(lambda a: (a["measurement"].update(unit=None), a["atom"].update(qualifiers=self.gold["conditions"] + self.gold["exceptions"])))),
            ("exception-missing", "VERIFY_EXCEPTION", atomedit(lambda a: a["atom"].update(qualifiers=["m/s", *self.gold["conditions"]], content=self.gold["atom_content"].split("；")[0] + "。"))),
            ("wrong-fact-correct-quote", "VERIFY_CONTENT", atomedit(lambda a: a["atom"].update(content=self.gold["atom_content"].replace("10", "11")))),
            ("negated-fact-correct-quote", "VERIFY_CONTENT", atomedit(lambda a: a["atom"].update(content=self.gold["atom_content"].replace("为 10", "不为 10")))),
            ("wrong-measurement", "VERIFY_FACT", atomedit(lambda a: a["measurement"].update(value="11"))),
            ("concept-unlinked", "VERIFY_CONCEPT", atomedit(lambda a: a["atom"].update(concept_ids=[]))),
            ("quote-wrong-sha", "VERIFY_EVIDENCE", lambda c: c["payload"]["evidence"][0].update(excerpt_sha256="0" * 64)),
            ("quote-wrong-text", "VERIFY_EVIDENCE", lambda c: c["payload"]["evidence"][0].update(quote="unrelated")),
            ("quote-wrong-range", "VERIFY_EVIDENCE", lambda c: c["payload"]["evidence"][0].update(excerpt_start_byte=1)),
            ("wrong-binding", "VERIFY_BINDING", lambda c: c["payload"]["binding"].update(source_id="wrong-source")),
            ("wrong-parse", "VERIFY_BINDING", lambda c: c["payload"]["binding"].update(parse_id="wrong-parse")),
            ("missing-atom", "VERIFY_MISSING", lambda c: c["payload"].update(atoms=[])),
        ]
        for name, code, change in variants:
            self.run_case(name, code=code, candidate_change=change)
        def duplicate(c, same_id=False):
            second = copy.deepcopy(c["payload"]["atoms"][0])
            if not same_id:
                second["atom"]["atom_id"] = "second-id"
            c["payload"]["atoms"].append(second)
        self.run_case("duplicate-semantic-new-id", code="VERIFY_DUPLICATE", candidate_change=duplicate)
        def borrowed(c):
            duplicate(c, True)
            c["payload"]["atoms"][0]["measurement"]["conditions"] = []
            c["payload"]["atoms"][0]["atom"]["qualifiers"] = ["m/s", "结冰时不适用"]
        self.run_case("same-id-no-borrowing", code="VERIFY_CONDITION", candidate_change=borrowed)
        for disposition, status in (("excluded", "failed"), ("pending", "unprocessed"), ("needs_review", "needs_review")):
            self.run_case("decision-" + disposition, code="VERIFY_EXCLUSION", status=status, decision_change=lambda d, x=disposition: d.update(disposition=x, exclusion_reason="author fact omitted" if x == "excluded" else None))
        self.run_case("pending-keeps-fact-failure", status="unprocessed", candidate_change=atomedit(lambda a: a["measurement"].update(value="11")), decision_change=lambda d: d.update(disposition="pending"))
        strict_cases = [
            ("candidate-extra", lambda c: c.update(extra=True)),
            ("candidate-unknown-version", lambda c: c.update(schema_version=99)),
            ("candidate-missing-nullable", atomedit(lambda a: a["measurement"].pop("unit"))),
            ("candidate-extra-inner", atomedit(lambda a: a["atom"].update(extra=True))),
        ]
        for name, change in strict_cases:
            self.run_case(name, code="VERIFY_SCHEMA", candidate_change=change)
        for name, raw in (("duplicate-outer-key", lambda b: b.replace(b'"payload": {', b'"payload": {}, "payload": {', 1)), ("duplicate-inner-key", lambda b: b.replace(b'"atoms": [', b'"atoms": [], "atoms": [', 1)), ("trailing-json", lambda b: b + b" {}")):
            self.run_case(name, code="VERIFY_SCHEMA", raw_change=raw)
        for name, change in (("review-unknown", lambda r: r.update(extra=True)), ("review-missing", lambda r: r.pop("reviewer")), ("review-gold-sha", lambda r: r.update(gold_sha256="0" * 64)), ("review-selection", lambda r: r.update(selection=[])), ("review-traversal", lambda r: r.update(candidate_file="../candidates.json"))):
            self.run_case(name, expected=2, review_change=change)
        for name, change in (("decision-missing-nullable", lambda d: d.pop("exclusion_reason")), ("decision-extra", lambda d: d.update(extra=True)), ("decision-unknown", lambda d: d.update(disposition="other"))):
            self.run_case(name, expected=2, decision_change=change)
        def mutate_input(filename, transform):
            return lambda case: (case / filename).write_bytes(transform((case / filename).read_bytes()))
        for name, filename, transform in (
            ("review-duplicate-key", "review.json", lambda b: b.replace(b'"schema_version": 1', b'"schema_version": 1,"schema_version": 1', 1)),
            ("review-trailing-json", "review.json", lambda b: b + b" {}"),
            ("decision-duplicate-key", "decision.json", lambda b: b.replace(b'"schema_version": 1', b'"schema_version": 1,"schema_version": 1', 1)),
            ("gold-extra-field", "gold.json", lambda b: encoded(dict(strict_json(b), extra=True))),
            ("candidate-byte-limit", "candidates.json", lambda b: b + b" " * (1024 * 1024)),
        ):
            self.run_case(name, expected=2, input_change=mutate_input(filename, transform))
        self.additional_sources()
        valid = self.make_case("path-protection")
        sentinel = self.root / "old-output"
        sentinel.mkdir()
        (sentinel / "human.txt").write_text("keep", encoding="utf-8")
        protected = inventory(sentinel)
        for name, output in (("old-output", sentinel), ("inside-source", self.project / "nested-output"), ("inside-input", valid / "nested-output")):
            self.call(["extract", "verify", str(self.project), self.import_id, str(valid), str(output)], 2)
            self.rows.append({"name": name, "engineering_passed": True, "expected_exit": 2, "observed_exit": 2, "call_index": len(self.calls)})
        assert inventory(sentinel) == protected
        hardcase = self.make_case("hardlink-input")
        os.link(hardcase / "candidates.json", self.root / "human-hardlink.json")
        hbefore = inventory(hardcase)
        self.call(["extract", "verify", str(self.project), self.import_id, str(hardcase), str(self.root / "out-hardlink")], 2)
        assert inventory(hardcase) == hbefore and (self.root / "human-hardlink.json").exists()
        self.rows.append({"name": "actual-hardlink-input", "engineering_passed": True, "expected_exit": 2, "observed_exit": 2, "call_index": len(self.calls)})
        self.write_audit()

    def additional_sources(self):
        for name, text, gold_bytes, status, selection in (
            ("nonzero-span", "# Title\n\n" + self.gold["quote"] + "\n", self.gold_bytes, "passed", {"kind": "text", "ordinal": 2}),
            ("source-needs-review", "```\n" + self.gold["quote"] + "\n", self.gold_bytes, "needs_review", {"kind": "text", "ordinal": 0}),
            ("unknown-author", self.unknown_gold["quote"], encoded(self.unknown_gold), "needs_review", {"kind": "text", "ordinal": 0}),
            ("original-gold-no-exceptions", (REPO / "fixtures/extraction-gateway-v1/source.txt").read_text(encoding="utf-8"), self.original_gold, "passed", {"kind": "text", "ordinal": 0}),
        ):
            source_path = self.root / (name + ".txt")
            source_path.write_text(text, encoding="utf-8")
            imp = strict_json(self.call(["source", "import", str(self.project), str(source_path)], 0).stdout)
            source = actual_source(self.project, imp["import_id"], selection)
            self.run_case(name, expected=0 if status == "passed" else 3, status=status, import_id=imp["import_id"], source=source, selection=selection, gold_bytes=gold_bytes)
        # PDF author and frozen gold precede parsing, candidate serialization and
        # library extraction. No downloaded font and no gold derived from parser.
        from reportlab.pdfgen import canvas
        from reportlab.pdfbase import pdfmetrics
        from reportlab.pdfbase.cidfonts import UnicodeCIDFont
        pdfmetrics.registerFont(UnicodeCIDFont("STSong-Light"))
        pdf_path = self.root / "controlled-chinese.pdf"
        document = canvas.Canvas(str(pdf_path), pagesize=(500, 300), invariant=1)
        document.setFont("STSong-Light", 14)
        document.drawString(40, 240, self.pdf_gold["quote"])
        document.save()
        save(self.root / "pdf-author-receipt.json", {"library": "ReportLab", "sentence_before_parse": self.pdf_gold["quote"], "pdf": fingerprint(pdf_path.read_bytes()), "gold": fingerprint(encoded(self.pdf_gold)), "font": "STSong-Light", "physical_pages": 1, "page_width": 500, "page_height": 300})
        imp = strict_json(self.call(["source", "import", str(self.project), str(pdf_path)], 0).stdout)
        parse_root = self.project / imp["pdf"]["snapshot_path"]
        manifest = strict_json((parse_root / "parser/pdf-manifest.json").read_bytes())
        assert len(manifest["pages"]) == 1, "authored physical PDF page"
        page = manifest["pages"][0]
        page_bytes = (parse_root / "parser/pages/000000.txt").read_bytes()
        region = next(r for r in page["regions"] if page_bytes[r["text"]["start_byte"]:r["text"]["end_byte"]].decode() == self.pdf_gold["quote"])
        bbox = region["bbox"]
        assert 0 <= bbox[0] < bbox[2] <= 500 and 0 <= bbox[1] < bbox[3] <= 300, "authored PDF geometry"
        selection = {"kind": "pdf", "page_index": 0, "region_id": region["region_id"]}
        source = actual_source(self.project, imp["import_id"], selection)
        self.run_case("actual-controlled-chinese-pdf", expected=0, status="passed", import_id=imp["import_id"], source=source, selection=selection, gold_bytes=encoded(self.pdf_gold))
        self.run_case("pdf-wrong-parse", code="VERIFY_BINDING", import_id=imp["import_id"], source=source, selection=selection, gold_bytes=encoded(self.pdf_gold), candidate_change=lambda c: c["payload"]["binding"].update(parse_id="wrong-parse"))
        failed = strict_json(self.call(["source", "import", str(self.project), str(REPO / "fixtures/pdf-text-v1/scanned-page.pdf")], 3).stdout)
        case = self.make_case("unavailable-scanned-pdf")
        self.call(["extract", "verify", str(self.project), failed["import_id"], str(case), str(self.root / "out-scanned")], 2)
        self.rows.append({"name": "unavailable-scanned-pdf", "engineering_passed": True, "expected_exit": 2, "observed_exit": 2, "call_index": len(self.calls)})

    def live(self, project, import_id, candidate_path, gold_path, frozen):
        """Root-owned optional saved-input review. No model/API requests."""
        gold_bytes, candidate_bytes = frozen
        assert gold_path.read_bytes() == gold_bytes and candidate_path.read_bytes() == candidate_bytes, "live frozen bytes changed"
        assert sha(gold_bytes) == "7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d", "original live gold fingerprint"
        assert sha(candidate_bytes) == "92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa", "original sixth candidate fingerprint"
        source = actual_source(project, import_id, {"kind": "text", "ordinal": 0})
        assert source["raw_sha256"] == "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a", "original source SHA"
        before = self.capture(project)
        case = self.root / "case-original-sixth"
        case.mkdir()
        (case / "gold.json").write_bytes(gold_bytes)
        (case / "candidates.json").write_bytes(candidate_bytes)
        candidate = strict_json(candidate_bytes)
        review = dict(self.review, gold_sha256=sha(gold_bytes), selection=[{"kind": "text", "ordinal": 0}], interpretation_timing="after_model_response")
        save(case / "review.json", review)
        save(case / "decision.json", {"schema_version": 1, "unit_id": "unit-0", "disposition": "included", "atom_ids": [candidate["payload"]["atoms"][0]["atom"]["atom_id"]], "exclusion_reason": None})
        inputs = {p.name: p.read_bytes() for p in case.iterdir()}
        output = self.root / "out-original-sixth"
        run = self.call(["extract", "verify", str(project), import_id, str(case), str(output)])
        assert run.returncode in (0, 3), "original review did not produce report"
        verified = verify_report(output, candidate_bytes, gold_bytes, source, inputs)
        self.unchanged(before)
        assert candidate_path.read_bytes() == candidate_bytes and gold_path.read_bytes() == gold_bytes, "original bytes changed"
        self.rows.append({"name": "original-sixth-saved-live-input", "engineering_passed": True, "observed_exit": run.returncode, "verification": verified, "output": str(output), "source": source, "call_index": len(self.calls), "output_fingerprints": {p.name: fingerprint(p.read_bytes()) for p in output.iterdir() if p.is_file()}})
        self.write_audit()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--live-project", type=Path)
    parser.add_argument("--live-import")
    parser.add_argument("--live-candidate", type=Path)
    parser.add_argument("--live-gold", type=Path)
    args = parser.parse_args(argv)
    live = (args.live_project, args.live_import, args.live_candidate, args.live_gold)
    if any(live) and not all(live):
        parser.error("all four --live-* options must be supplied together")
    frozen = None
    if all(live):
        # Read true author gold BEFORE even project init/offline matrix calls.
        frozen = (args.live_gold.read_bytes(), args.live_candidate.read_bytes())
        assert sha(frozen[0]) == "7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d", "original live gold fingerprint"
        assert sha(frozen[1]) == "92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa", "original live candidate fingerprint"
        assert not args.output.resolve().is_relative_to(args.live_project.resolve()), "output overlaps selected source"
    audit = None
    try:
        audit = Audit(args.binary, args.output)
        audit.matrix()
        if all(live):
            audit.live(*live, frozen)
        audit.write_audit()
        print(json.dumps({"audit": str(audit.root / "audit.json"), "scenarios": len(audit.rows), "calls": len(audit.calls), "network_calls": 0}, ensure_ascii=False))
        return 0
    except Exception as exc:
        if audit:
            audit.write_audit(error=repr(exc))
        print(repr(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
