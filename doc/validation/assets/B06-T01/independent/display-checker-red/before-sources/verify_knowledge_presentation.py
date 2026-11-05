"""Independently rebuild an active presentation from author/source evidence.

The checker reads selected local SQL/resources and the saved review originals,
then reuses the independent v1/v2/v3 coordination checkers. It never invokes a
Rust planner to obtain an expectation, reads credentials, or scans a user Vault.
The authored audit fixtures establish engineering behavior, not science quality.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import sqlite3
import string
import subprocess
import sys
import time
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_knowledge_coordination import (
    FILES, actual_source, compact, encoded, exact, fields, fingerprint, identifier,
    inventory, read, safe_name, source_bundle, strict_json, text,
    uuid_string, verify_report as verify_coordination, verify_markdown,
)
from verify_knowledge_conflicts import guard_path, time_schema

REPO = Path(__file__).resolve().parents[1]
SCOPE = "active_presentation_v1"
EXPECTED_FIELDS = "schema_version author scope scientific_quality knowledge_status published case_name expected_status expect_successful_preview representation objects properties cells required_atom_refs required_missing_text"
SEMANTIC_FACT_FIELDS = "value_status value unit conditions exceptions applicability_status time input_ids original_atom_ids"
MISSING = "选定事实未给出"
RULES = {
    "table": ("PRESENT_SHARED_PROPERTY_TABLE", "多个明确对象共享已核对属性，以比较表并列完整事实；单位与条件保留，不作数值裁决。"),
    "single": ("PRESENT_SINGLE_OBJECT_TEXT", "选定事实仅涉及一个对象，以正文保留各属性、条件和原始表述。"),
    "disjoint": ("PRESENT_NO_SHARED_PROPERTY_TEXT", "选定对象没有共同的显式属性，以正文分别保留各自事实。"),
}
MAX_BYTES = 64 * 1048576
AUTHOR_PACKAGES = {
    "active_presentation_fixture_author_v1": ("active-presentation-v1", 11),
    "active_presentation_legacy_fixture_author_v1": ("active-presentation-legacy-v1", 2),
    "active_presentation_cross_role_fixture_author_v1": ("active-presentation-cross-role-v1", 1),
}


def unique(items):
    return list(dict.fromkeys(items))


def expected_schema(expected):
    """Require the independent semantic author contract, including JSON types."""
    fields(expected, EXPECTED_FIELDS)
    for key, value in dict(schema_version=1, scope="active_presentation_semantics_v1",
                           scientific_quality="not_run", knowledge_status="pending", published=False).items():
        exact(expected[key], value, "author " + key)
    text(expected["author"]); identifier(expected["case_name"])
    assert expected["expected_status"] in ("passed", "blocked"), "author status"
    exact(expected["expect_successful_preview"], expected["expected_status"] == "passed", "author successful preview")
    assert expected["representation"] in ("table", "paragraph", None), "author representation"
    assert (expected["representation"] is not None) == expected["expect_successful_preview"], "author representation/status"
    for name, id_field in (("objects", "object_id"), ("properties", "property_id")):
        assert type(expected[name]) is list and 1 <= len(expected[name]) <= 3, "author labels"
        for label in expected[name]:
            fields(label, id_field + " label"); uuid_string(label[id_field]); text(label["label"])
        assert len({v[id_field] for v in expected[name]}) == len(expected[name]), "unique author labels"
    assert type(expected["cells"]) is list and len(expected["cells"]) <= 9, "author cells"
    keys = set()
    for cell in expected["cells"]:
        fields(cell, "object_id property_id state missing_meaning facts")
        uuid_string(cell["object_id"]); uuid_string(cell["property_id"])
        key = (cell["object_id"], cell["property_id"])
        assert key not in keys, "duplicate author cell"; keys.add(key)
        assert cell["state"] in ("facts", "missing"), "author cell state"
        exact(cell["missing_meaning"], "not_given_in_selected_facts" if cell["state"] == "missing" else None, "author missing meaning")
        assert type(cell["facts"]) is list and len(cell["facts"]) <= 3, "author fact groups"
        assert bool(cell["facts"]) == (cell["state"] == "facts"), "author cell facts/state"
        for fact in cell["facts"]:
            fields(fact, SEMANTIC_FACT_FIELDS)
            exact(fact["value_status"], "known", "author known value")
            for name in ("value", "unit"):
                text(fact[name])
            for name in ("conditions", "exceptions", "input_ids", "original_atom_ids"):
                assert type(fact[name]) is list and len(fact[name]) <= 8, "author fact array"
                for value in fact[name]:
                    text(value)
            assert 1 <= len(fact["input_ids"]) == len(fact["original_atom_ids"]) <= 3, "author fact origins"
            for value in fact["input_ids"]:
                identifier(value)
            assert fact["applicability_status"] in (None, "known", "unknown", "not_applicable"), "author applicability"
            if fact["time"] is not None:
                time_schema(fact["time"])
    assert type(expected["required_atom_refs"]) is list and 2 <= len(expected["required_atom_refs"]) <= 3, "author required refs"
    for ref in expected["required_atom_refs"]:
        fields(ref, "input_id original_atom_id"); identifier(ref["input_id"]); text(ref["original_atom_id"])
    assert len({compact(r) for r in expected["required_atom_refs"]}) == len(expected["required_atom_refs"]), "unique author refs"
    exact(expected["required_missing_text"], MISSING if any(c["state"] == "missing" for c in expected["cells"]) else None, "author missing text")


def choose(facts):
    objects = {f["object_id"] for f in facts}
    shared = sorted({f["property_id"] for f in facts if len({g["object_id"] for g in facts if g["property_id"] == f["property_id"]}) >= 2})
    kind = "table" if len(objects) >= 2 and shared else "single" if len(objects) == 1 else "disjoint"
    rule, reason = RULES[kind]
    return ("table" if kind == "table" else "paragraph"), dict(rule_id=rule, reason=reason, shared_property_ids=shared)


def semantic_cells(facts, objects, properties, *, table=True):
    result = []
    for obj in objects:
        for prop in properties:
            matching = [f for f in facts if f["object_id"] == obj["object_id"] and f["property_id"] == prop["property_id"]]
            if not matching and not table:
                continue
            result.append(dict(object_id=obj["object_id"], property_id=prop["property_id"],
                               state="facts" if matching else "missing",
                               missing_meaning=None if matching else "not_given_in_selected_facts",
                               facts=[{k: copy.deepcopy(f.get(k)) for k in SEMANTIC_FACT_FIELDS.split()} for f in matching]))
    return result


def canonical_semantics(cells):
    """Semantic authors do not prescribe the hash ordering of retained groups."""
    result = copy.deepcopy(cells)
    for cell in result:
        for fact in cell["facts"]:
            refs = sorted(zip(fact["input_ids"], fact["original_atom_ids"]))
            fact["input_ids"] = [a for a, _ in refs]
            fact["original_atom_ids"] = [b for _, b in refs]
        cell["facts"].sort(key=compact)
    return sorted(result, key=lambda c: (c["object_id"], c["property_id"]))


def verify_author_semantics(author, expected):
    """Rebuild the authored cells before any candidate or presentation output."""
    expected_schema(expected)
    exact(author["name"], expected["case_name"], "frozen author case")
    identity = author["identity"]
    concepts = {c["canonical_id"]: c["display_name"] for c in identity["concepts"]}
    facts = [dict(f, value_status=f.get("value_status", "known")) for f in author["coordination_expected"]["facts"]]
    objects = [dict(object_id=i, label=concepts[i]) for i in sorted({f["object_id"] for f in facts})]
    properties = [dict(property_id=i, label=concepts[i]) for i in sorted({f["property_id"] for f in facts})]
    exact(expected["objects"], objects, "frozen author objects")
    exact(expected["properties"], properties, "frozen author properties")
    good = author["coordination_expected"]["expected_status"] == "passed"
    exact(expected["expected_status"], "passed" if good else "blocked", "frozen author status")
    exact(expected["representation"], choose(facts)[0] if good else None, "independent representation decision")
    cells = semantic_cells(facts, objects, properties, table=choose(facts)[0] == "table") if good else []
    exact(canonical_semantics(expected["cells"]), canonical_semantics(cells), "independent authored cell groups")
    refs = [dict(input_id=i["input_id"], original_atom_id=i["original_atom_id"]) for i in sorted(author["inputs"], key=lambda i: i["input_id"])]
    exact(sorted(expected["required_atom_refs"], key=compact), sorted(refs, key=compact), "complete authored atom references")
    for item in author["inputs"]:
        ref = next(f for f in facts if item["input_id"] in f["input_ids"])
        for key in ("value_status", "value", "unit", "conditions", "exceptions"):
            exact(item["gold"][key], ref[key], "independent author source " + key)


def guarded_read(path, limit):
    guard_path(path, directory=False)
    return read(Path(path), limit)


def names_only(root, wanted):
    """Check role names and lstat all entries before reading their contents."""
    guard_path(root, directory=True)
    entries = list(Path(root).iterdir())
    assert {p.name for p in entries} == set(wanted), "exact directory inventory: " + str(root)
    assert len({p.name.casefold() for p in entries}) == len(entries), "case alias collision"
    for path in entries:
        guard_path(path)


def preflight_case(case, coord):
    manifest = strict_json(guarded_read(case / "coordination.json", 65536))
    assert type(manifest) is dict and type(manifest.get("schema_version")) is int and manifest["schema_version"] in (1, 2, 3), "coordination case version"
    safe_name(manifest["identity_file"])
    assert type(manifest["inputs"]) is list and 2 <= len(manifest["inputs"]) <= 3, "coordination input count"
    case_names = ["coordination.json", manifest["identity_file"]]
    copied = [".owner.json", "coordination-original.json", "identity-original.json", "coordination.json", "coordination.md"]
    for item in manifest["inputs"]:
        identifier(item["input_id"]); safe_name(item["review_bundle"])
        case_names.append(item["review_bundle"])
    assert len(set(n.casefold() for n in case_names)) == len(case_names), "case role aliases"
    names_only(case, case_names)
    for item in manifest["inputs"]:
        bundle = case / item["review_bundle"]
        names_only(bundle, FILES)
        for name in FILES:
            guard_path(bundle / name, directory=False)
            copied.append("input-" + item["input_id"] + "--" + name)
    # Both inventories are valid before the upstream checker reads either one.
    actual = {p.name for p in coord.iterdir()}
    assert actual in (set(copied), {*copied, "coordination-ir.json"}), "saved coordination inventory"
    names_only(coord, actual)
    for name in actual:
        guard_path(coord / name, directory=False)
    return manifest


def author_source_path(parent, item):
    ident = item["input_id"]; identifier(ident)
    frozen = parent / ("selected-" + ident + ".txt")
    if os.path.lexists(frozen):
        return frozen
    safe_name(item["source_file"])
    return parent / item["source_file"]


def load_expectation(expected_path):
    raw = guarded_read(expected_path, 1048576)
    expected = strict_json(raw); expected_schema(expected)
    parent = expected_path.parent
    gold_path = parent / "coordination-expected.json"
    gold_raw = guarded_read(gold_path, 1048576) if os.path.lexists(gold_path) else None
    author_path = parent / "frozen-author.json"
    original_path = parent / "frozen-authors-original.json"
    if not os.path.lexists(original_path):
        original_path = parent / "authors.json"
    authors_raw = guarded_read(original_path, 1048576) if os.path.lexists(original_path) else None
    authors = strict_json(authors_raw) if authors_raw is not None else None
    if os.path.lexists(author_path):
        author = strict_json(guarded_read(author_path, 1048576))
    elif authors is not None:
        matches = [c for c in authors["cases"] if c["name"] == expected["case_name"]]
        assert len(matches) == 1, "unique original authored case"
        author = matches[0]
    else:
        author = None
    if author is not None:
        verify_author_semantics(author, expected)
        if gold_raw is None:
            gold_raw = encoded(author["coordination_expected"])
        exact(strict_json(gold_raw), author["coordination_expected"], "frozen coordination author")
        if authors is not None:
            matches = [c for c in authors["cases"] if c["name"] == author["name"]]
            assert len(matches) == 1, "unique original authored case"
            exact(author, matches[0], "complete frozen author original")
        if authors is not None and authors.get("scope") in AUTHOR_PACKAGES:
            fixture = REPO / "fixtures" / AUTHOR_PACKAGES[authors["scope"]][0]
            guard_path(fixture, directory=True)
            freeze = strict_json(guarded_read(fixture / "frozen-sha256.json", 1048576))
            fixture_authors = guarded_read(fixture / "authors.json", 1048576)
            exact(fingerprint(fixture_authors), freeze["files"]["authors.json"], "repository author freeze")
            exact(authors_raw, fixture_authors, "original authored fixture package bytes")
            name = author["presentation_expected_file"]; safe_name(name)
            exact(fingerprint(raw), freeze["files"][name], "frozen presentation expectation bytes")
            for item in author["inputs"]:
                source_name = item["source_file"]; safe_name(source_name)
                selected = guarded_read(author_source_path(parent, item), 4194304)
                exact(fingerprint(selected), freeze["files"][source_name], "frozen original selected source bytes")
        for item in author["inputs"]:
            ident = item["input_id"]; identifier(ident)
            frozen_gold = parent / ("frozen-gold-" + ident + ".json")
            if os.path.lexists(frozen_gold):
                exact(strict_json(guarded_read(frozen_gold, 65536)), item["gold"], "frozen single-source gold")
    assert gold_raw is not None, "independent coordination author required beside expected"
    return expected, gold_raw, author


def fact_text(fact):
    value = fact["value"] + " " + fact["unit"]
    value += "；条件：" + ("；".join(fact["conditions"]) or "未列出")
    value += "；例外：" + ("；".join(fact["exceptions"]) or "未列出")
    if fact["applicability_status"] is None:
        value += "；适用背景：本协调版本未声明"
    else:
        value += "；适用条件：" + {"known": "已列明", "unknown": "未知", "not_applicable": "不适用"}[fact["applicability_status"]]
    if fact["time"] is None:
        value += "；观点时间：本协调版本未声明"
    elif fact["time"]["status"] == "unknown":
        value += "；观点时间：未知"
    else:
        value += "；观点时间：" + fact["time"]["value"] + "（" + {"year": "年", "day": "日"}[fact["time"]["precision"]] + "）"
    return value


def refs_for(facts):
    return dict(fact_ids=[f["fact_id"] for f in facts],
                occurrence_ids=unique(o for f in facts for o in f["occurrence_ids"]),
                concept_ids=unique(i for f in facts for i in (f["object_id"], f["property_id"])))


def rebuild_plan(coordination, identity, candidates, request_id, ir_fp):
    """Projection of independently checked evidence, never of saved plan.json."""
    occurrences = coordination["occurrences"]
    by_occ = {o["occurrence_id"]: o for o in occurrences}
    contexts = {c["input_id"]: c for c in identity.get("atom_contexts", [])}
    provenance = []
    for occurrence in occurrences:
        candidate = candidates[occurrence["input_id"]]
        atom = next(a for a in candidate["payload"]["atoms"] if a["atom"]["atom_id"] == occurrence["original_atom"]["atom"]["atom_id"])
        evidence = {e["evidence_id"]: e for e in candidate["payload"]["evidence"]}
        linked = unique(l["evidence_id"] for l in atom["atom"]["evidence_links"])
        provenance.append(dict(occurrence_id=occurrence["occurrence_id"], input_id=occurrence["input_id"],
            original_atom_id=atom["atom"]["atom_id"], binding=copy.deepcopy(occurrence["binding"]),
            unit=copy.deepcopy(occurrence["unit"]), original_concepts=copy.deepcopy(candidate["payload"]["concepts"]),
            original_atom=copy.deepcopy(atom), evidence=[copy.deepcopy(evidence[e]) for e in linked],
            context=copy.deepcopy(contexts.get(occurrence["input_id"]))))
    facts = []
    for original in coordination["facts"]:
        first = by_occ[original["occurrence_ids"][0]]
        facts.append(dict(fact_id=original["fact_id"], object_id=original["object_id"], property_id=original["property_id"],
            value_status="known", value=first["value"], unit=first["unit_value"],
            conditions=copy.deepcopy(first["conditions"]), exceptions=copy.deepcopy(first["exceptions"]),
            applicability_status=original.get("applicability_status"),
            time=copy.deepcopy(original.get("time")), occurrence_ids=copy.deepcopy(original["occurrence_ids"])))
    concepts = {c["canonical_id"]: c["display_name"] for c in coordination["canonical_concepts"]}
    def labels(role):
        return [dict(concept_id=i, label=concepts[i], occurrence_ids=[o["occurrence_id"] for o in occurrences if o[role] == i])
                for i in sorted({f[role] for f in facts})]
    objects, properties = labels("object_id"), labels("property_id")
    representation, decision = choose(facts)
    def link(object_id):
        return dict(target_concept_id=object_id, target_path="对象-" + object_id + ".md", target_heading="知识明细", display_text="查看详情")
    def paragraph(fact, detail=False):
        value = concepts[fact["object_id"]] + "的" + concepts[fact["property_id"]] + "：" + fact_text(fact)
        if detail:
            raw = unique(by_occ[o]["original_atom"]["atom"]["content"] for o in fact["occurrence_ids"])
            value += "\n原始表述：" + "\n".join(raw)
        return dict(kind="paragraph", node_id=("detail-" if detail else "overview-") + fact["fact_id"],
                    text=value, **refs_for([fact]), links=[] if detail else [link(fact["object_id"])])
    if representation == "table":
        columns = [dict(column_id="object", property_id=None, label="对象", concept_ids=[], occurrence_ids=[])]
        columns += [dict(column_id=p["concept_id"], property_id=p["concept_id"], label=p["label"],
                         concept_ids=[p["concept_id"]], occurrence_ids=p["occurrence_ids"]) for p in properties]
        rows = []
        for obj in objects:
            cells = [dict(column_id="object", kind="label", text=obj["label"], fact_ids=[], occurrence_ids=obj["occurrence_ids"],
                          concept_ids=[obj["concept_id"]], links=[link(obj["concept_id"])])]
            for prop in properties:
                matching = [f for f in facts if f["object_id"] == obj["concept_id"] and f["property_id"] == prop["concept_id"]]
                refs = refs_for(matching)
                cells.append(dict(column_id=prop["concept_id"], kind="facts" if matching else "missing",
                    text="\n".join(fact_text(f) for f in matching) if matching else MISSING, **refs, links=[]))
            rows.append(dict(row_id=obj["concept_id"], object_id=obj["concept_id"], cells=cells))
        nodes = [dict(kind="table", node_id="comparison", columns=columns, rows=rows)]
    else:
        nodes = [paragraph(f) for f in facts]
    notes = [dict(note_id="overview", title="知识比较", output_path="知识比较.md", section_id="overview", heading="比较概览", nodes=nodes)]
    notes += [dict(note_id="object-" + o["concept_id"], title=o["label"], output_path="对象-" + o["concept_id"] + ".md",
                   section_id="details", heading="知识明细", nodes=[paragraph(f, True) for f in facts if f["object_id"] == o["concept_id"]]) for o in objects]
    return dict(schema_version=1, producer="knowledge_presentation", scope=SCOPE, representation_origin="deterministic_planner", revision=1,
        request_id=request_id, project_id=coordination["project_id"], coordination_schema_version=coordination["schema_version"],
        coordination_ir_fingerprint=ir_fp, knowledge_status="pending", published=False, scientific_quality="not_run", research_status="not_requested",
        representation=representation, decision=decision, objects=objects, properties=properties, facts=facts, provenance=provenance, notes=notes)


def plan_semantics(plan):
    by_occ = {o["occurrence_id"]: o for o in plan["provenance"]}
    facts = []
    for f in plan["facts"]:
        origins = sorted((by_occ[i]["input_id"], by_occ[i]["original_atom_id"]) for i in f["occurrence_ids"])
        facts.append(dict(f, input_ids=[i for i, _ in origins], original_atom_ids=[a for _, a in origins]))
    objects = [dict(object_id=o["concept_id"], label=o["label"]) for o in plan["objects"]]
    properties = [dict(property_id=o["concept_id"], label=o["label"]) for o in plan["properties"]]
    return objects, properties, semantic_cells(facts, objects, properties, table=plan["representation"] == "table")


def encode_text(value):
    return "".join("<br>" if c == "\n" else "&#9;" if c == "\t" else "\\" + c if c in string.punctuation else c for c in value)


def render_independent(plan):
    """Build exact expected UTF8 bytes and separate payload/link intervals."""
    rendered = {}
    for note in plan["notes"]:
        parts, mappings, links = [], [], []
        length = 0
        def push(value):
            nonlocal length
            parts.append(value); length += len(value.encode("utf-8"))
            assert length <= MAX_BYTES, "render size limit"
        def scope(node, kind, row=None, column=None):
            return dict(section_id=note["section_id"], node_id=node["node_id"], kind=kind, row_id=row, column_id=column)
        def payload(value, refs, semantic_links, location, table=False):
            start = length; push(encode_text(value))
            mappings.append(dict(scope=location, **refs, start_byte=start, end_byte=length))
            for link in semantic_links:
                push(" "); start = length
                stem = link["target_path"][:-3]
                alias_separator = "\\|" if table else "|"
                push("[[" + stem + "#" + link["target_heading"] + alias_separator + encode_text(link["display_text"]) + "]]")
                links.append(dict(scope=location, **link, start_byte=start, end_byte=length))
        push("# " + encode_text(note["title"]) + "\n\n")
        push("## " + encode_text(note["heading"]) + "\n\n")
        for node in note["nodes"]:
            if node["kind"] == "paragraph":
                refs = {k: node[k] for k in ("fact_ids", "occurrence_ids", "concept_ids")}
                payload(node["text"], refs, node["links"], scope(node, "paragraph"))
                push("\n\n")
            else:
                push("| ")
                for index, column in enumerate(node["columns"]):
                    if index: push(" | ")
                    payload(column["label"], dict(fact_ids=[], occurrence_ids=column["occurrence_ids"], concept_ids=column["concept_ids"]), [], scope(node, "table_header", column=column["column_id"]), True)
                push(" |\n| " + " | ".join("---" for _ in node["columns"]) + " |\n")
                for row in node["rows"]:
                    push("| ")
                    for index, cell in enumerate(row["cells"]):
                        if index: push(" | ")
                        refs = {k: cell[k] for k in ("fact_ids", "occurrence_ids", "concept_ids")}
                        payload(cell["text"], refs, cell["links"], scope(node, cell["kind"], row["row_id"], cell["column_id"]), True)
                    push(" |\n")
                push("\n")
        raw = "".join(parts).encode("utf-8")
        file = dict(relative_path=note["output_path"], note_id=note["note_id"], plan_revision=1,
                    content_sha256=hashlib.sha256(raw).hexdigest(), byte_length=len(raw), content_mappings=mappings, links=links)
        rendered[note["output_path"]] = (raw, file)
    return rendered


def verify_report(project, case, coordination_output, preview, expected_path):
    """Five supplied paths; false/synchronously forged output raises AssertionError."""
    try:
        return _verify(*(Path(p) for p in (project, case, coordination_output, preview, expected_path)))
    except (ValueError, KeyError, TypeError, OSError, StopIteration, sqlite3.Error) as exc:
        raise AssertionError("presentation verification failed: " + type(exc).__name__) from exc


def _verify(project, case, coord, preview, expected_path):
    started = time.monotonic()
    for root in (project, case, coord, preview):
        guard_path(root, directory=True)
    guard_path(expected_path, directory=False)
    roots = [r.resolve() for r in (project, case, coord, preview)]
    for a, b in itertools.combinations(roots, 2):
        assert not a.is_relative_to(b) and not b.is_relative_to(a), "overlapping presentation roots"
    assert not any(expected_path.resolve().is_relative_to(r) for r in roots), "expected inside checked roots"
    manifest = preflight_case(case, coord)
    expected, gold_raw, author = load_expectation(expected_path)
    before = [inventory(p) for p in (project, case, coord)]
    expected_before = fingerprint(guarded_read(expected_path, 1048576))
    checked = verify_coordination(project, case, coord, gold_raw)
    coordination = strict_json(guarded_read(coord / "coordination.json", 8388608))
    identity = strict_json(guarded_read(case / manifest["identity_file"], 65536))
    canonical_labels = {c["canonical_id"]: c["display_name"] for c in coordination["canonical_concepts"]}
    exact(expected["objects"], [dict(object_id=i, label=canonical_labels[i]) for i in sorted({f["object_id"] for f in coordination["facts"]})], "all actual authored objects")
    exact(expected["properties"], [dict(property_id=i, label=canonical_labels[i]) for i in sorted({f["property_id"] for f in coordination["facts"]})], "all actual authored properties")
    actual_refs = [dict(input_id=o["input_id"], original_atom_id=o["original_atom"]["atom"]["atom_id"]) for o in coordination["occurrences"]]
    exact(sorted(actual_refs, key=compact), sorted(expected["required_atom_refs"], key=compact), "all actual authored atom origins")
    candidates = {}
    copies = {".selection-original.json": guarded_read(case / "coordination.json", 65536),
              ".identity-original.json": guarded_read(case / manifest["identity_file"], 65536),
              ".coordination-owner.json": guarded_read(coord / ".owner.json", 4096),
              ".coordination-report.json": guarded_read(coord / "coordination.json", 8388608),
              ".coordination-report.md": guarded_read(coord / "coordination.md", 8388608)}
    for item in manifest["inputs"]:
        ident = item["input_id"]; directory = case / item["review_bundle"]
        raw, _, candidate, source_gold, _, source = source_bundle(project, directory, item["artifacts"], evidence_quote_lf=manifest["schema_version"] >= 2)
        candidates[ident] = candidate
        copies.update({".input-" + ident + "--" + name: body for name, body in raw.items()})
        if author:
            authored = next(i for i in author["inputs"] if i["input_id"] == ident)
            exact(source_gold, authored["gold"], "actual original authored Gold")
            selected = guarded_read(author_source_path(expected_path.parent, authored), 4194304)
            exact(source["units"][0]["text"].encode(), selected, "selected authored source bytes")
    good = checked["status"] == "passed"
    exact(expected["expect_successful_preview"], good, "independent preview admission")
    actual_names = {p.name for p in preview.iterdir()}
    ir_raw = guarded_read(coord / "coordination-ir.json", 8388608) if good else None
    ir_fp = fingerprint(ir_raw) if good else None
    if good:
        copies[".coordination-ir.json"] = ir_raw
    initial_wanted = {*copies, ".owner.json", "presentation.json", "presentation.md"}
    if good:
        initial_wanted.update({"plan.json", "知识比较.md", *("对象-" + i + ".md" for i in {f["object_id"] for f in coordination["facts"]})})
    names_only(preview, initial_wanted)
    for name in initial_wanted:
        guard_path(preview / name, directory=False)
    owner_raw = guarded_read(preview / ".owner.json", 4096)
    owner = strict_json(owner_raw)
    fields(owner, "schema_version producer request_id purpose"); uuid_string(owner["request_id"])
    assert uuid.UUID(owner["request_id"]).version == 4, "presentation request UUIDv4"
    exact(owner, dict(schema_version=1, producer="knowledge_presentation", request_id=owner["request_id"], purpose="unpublished_presentation"), "presentation owner")
    plan = rebuild_plan(coordination, identity, candidates, owner["request_id"], ir_fp) if good else None
    rendered = render_independent(plan) if good else {}
    wanted = {*copies, ".owner.json", "presentation.json", "presentation.md", *rendered}
    if good: wanted.add("plan.json")
    assert actual_names == wanted, "presentation output role inventory"
    names_only(preview, wanted)
    for name in wanted: guard_path(preview / name, directory=False)
    preview_before = inventory(preview)
    for name, raw in copies.items():
        exact(guarded_read(preview / name, max(MAX_BYTES, len(raw))), raw, "saved presentation original " + name)
    artifacts = {".owner.json": fingerprint(owner_raw), **{n: fingerprint(b) for n, b in copies.items()}}
    if good:
        plan_raw = guarded_read(preview / "plan.json", MAX_BYTES)
        exact(strict_json(plan_raw), plan, "full independently rebuilt presentation plan")
        objects, properties, cells = plan_semantics(plan)
        exact(objects, expected["objects"], "actual authored objects")
        exact(properties, expected["properties"], "actual authored properties")
        exact(plan["representation"], expected["representation"], "actual authored representation")
        exact(canonical_semantics(cells), canonical_semantics(expected["cells"]), "actual authored cells")
        exact(sorted([dict(input_id=p["input_id"], original_atom_id=p["original_atom_id"]) for p in plan["provenance"]], key=compact),
              sorted(expected["required_atom_refs"], key=compact), "complete actual original atoms")
        artifacts["plan.json"] = fingerprint(plan_raw)
        for name, (raw, _) in rendered.items():
            exact(guarded_read(preview / name, MAX_BYTES), raw, "actual Markdown payload/header/link read-back " + name)
            artifacts[name] = fingerprint(raw)
        plan_fp = fingerprint(plan_raw)
    else:
        plan_fp = None
    files = [item[1] for item in rendered.values()]
    body_cells = [cell for note in (plan["notes"] if plan else []) for node in note["nodes"] if node["kind"] == "table" for row in node["rows"] for cell in row["cells"]]
    totals = dict(input_bundles=len(coordination["inputs"]), facts=len(coordination["facts"]), source_occurrences=len(coordination["occurrences"]),
                  objects=len({f["object_id"] for f in coordination["facts"]}), properties=len({f["property_id"] for f in coordination["facts"]}),
                  table_cells=len(body_cells), missing_cells=sum(c["kind"] == "missing" for c in body_cells), rendered_notes=len(files),
                  content_mappings=sum(len(f["content_mappings"]) for f in files), links=sum(len(f["links"]) for f in files))
    status, preview_status = ("passed", "rendered") if good else ("needs_review", "not_run")
    report = dict(schema_version=1, producer="knowledge_presentation", report_kind="knowledge_presentation", scope=SCOPE,
        request_id=owner["request_id"], project_id=coordination["project_id"], coordination_schema_version=coordination["schema_version"],
        status=status, preview_status=preview_status, knowledge_status="pending", published=False, scientific_quality="not_run", research_status="not_requested",
        network_calls=0, paid_calls=0, checks=dict(source_review="passed", coordination="passed" if good else "needs_review", note_plan="passed" if good else "not_run",
            rendering="passed" if good else "not_run", saved_output="passed"), totals=totals,
        issue_codes=[] if good else ["PRESENT_COORDINATION_UNRESOLVED"], decision=plan["decision"] if good else None,
        coordination_ir_fingerprint=ir_fp, plan_fingerprint=plan_fp, inputs={n: fingerprint(b) for n, b in copies.items()}, files=files, artifacts=artifacts)
    actual_report_raw = guarded_read(preview / "presentation.json", MAX_BYTES)
    exact(strict_json(actual_report_raw), report, "full presentation report independent reconstruction")
    preamble = ("# 有据主动表达预览\n\n状态：" + status + "；预览：" + preview_status + "；知识状态：pending；发布：false。\n\n"
                "科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n")
    md = guarded_read(preview / "presentation.md", MAX_BYTES)
    verify_markdown(md, preamble, report, "presentation Markdown full report")
    exact(md, (preamble + "```json\n" + actual_report_raw.decode().rstrip("\n") + "\n```\n").encode(), "presentation dual-report raw equality")
    exact([inventory(p) for p in (project, case, coord)], before, "selected original inventories unchanged")
    exact(inventory(preview), preview_before, "presentation read-only inventory unchanged")
    exact(fingerprint(guarded_read(expected_path, 1048576)), expected_before, "author expected unchanged")
    assert sum(v.get("byte_length", 0) for v in preview_before.values()) <= 96 * 1048576 and time.monotonic() - started <= 30, "presentation aggregate/time budget"
    return dict(status=status, preview_status=preview_status, independently_verified=True, totals=totals, representation=plan["representation"] if plan else None,
                knowledge_status="pending", scientific_quality="not_run", published=False, network_calls=0, paid_calls=0)


class PresentationAudit:
    """Freeze independent authors, then record ordinary CLI behavior and forgeries."""
    def __init__(self, cli, root):
        cli, root = Path(cli), Path(root)
        assert cli.is_absolute(), "audit CLI path must be absolute"
        guard_path(cli, directory=False)
        # Do not resolve a supplied target until each lexical ancestor is guarded.
        guard_path(root.parent, directory=True)
        assert not os.path.lexists(root), "audit target must not already exist"
        safe_name(root.name)
        self.cli, self.root = cli.resolve(), root.parent.resolve() / root.name
        self.cli_fp = fingerprint(guarded_read(self.cli, 256 * 1048576))
        self.root.mkdir()
        self.calls, self.rows, self.protected = [], [], {}
        self.revision = 0
        self.sentinel = self.root.parent / ("presentation-sentinel-" + uuid.uuid4().hex + ".txt")
        self.sentinel.write_bytes(b"external human sentinel\n")
        self.protect(self.cli)
        self.protect(self.sentinel)
        self.author_dir = self.root / "authors"; self.author_dir.mkdir()
        self.packages, self.cases = {}, []
        for scope, (folder_name, count) in AUTHOR_PACKAGES.items():
            fixture = REPO / "fixtures" / folder_name
            guard_path(fixture, directory=True)
            freeze_raw = guarded_read(fixture / "frozen-sha256.json", 1048576)
            freeze = strict_json(freeze_raw)
            fields(freeze, "schema_version author files"); exact(freeze["schema_version"], 1, "author freeze version")
            text(freeze["author"])
            assert type(freeze["files"]) is dict, "frozen author file map"
            names_only(fixture, {*freeze["files"], "frozen-sha256.json"})
            copied = self.author_dir / folder_name; copied.mkdir()
            package = {}
            for name, pin in freeze["files"].items():
                safe_name(name)
                raw = guarded_read(fixture / name, 1048576)
                exact(pin, fingerprint(raw), "frozen author bytes " + name)
                package[name] = raw; (copied / name).write_bytes(raw)
            (copied / "frozen-sha256.json").write_bytes(freeze_raw)
            self.protect(fixture)
            authors = strict_json(package["authors.json"])
            exact(authors["scope"], scope, "author package scope")
            assert len(authors["cases"]) == count, "fixed author scenario members"
            for case in authors["cases"]:
                verify_author_semantics(case, strict_json(package[case["presentation_expected_file"]]))
                self.cases.append((case, folder_name))
            self.packages[folder_name] = package
        assert len({c["name"] for c, _ in self.cases}) == len(self.cases), "unique audit scenario names"
        self.protect(self.author_dir)
        self.write_new(self.root / "author-pins.json", {name: {n: fingerprint(b) for n, b in package.items()} for name, package in self.packages.items()})
        self.flush()

    @staticmethod
    def write_new(path, value):
        with path.open("xb") as stream:
            stream.write(encoded(value))

    def protect(self, path):
        guard_path(path)
        self.protected[str(path)] = inventory(path) if path.is_dir() else fingerprint(guarded_read(path, 256 * 1048576))

    def check_protected(self):
        current = {}
        for name in self.protected:
            path = Path(name); guard_path(path)
            current[name] = inventory(path) if path.is_dir() else fingerprint(guarded_read(path, 256 * 1048576))
        exact(current, self.protected, "audit protected original inventories")

    def flush(self, error=None):
        self.write_new(self.root / ("audit-" + str(self.revision) + ".json"),
                       dict(schema_version=1, producer="independent_python_presentation_audit", cli=str(self.cli), cli_fingerprint=self.cli_fp,
                            root=str(self.root), calls=self.calls, scenarios=self.rows, protected_before=self.protected, error=error,
                            knowledge_status="pending", scientific_quality="not_run", published=False, network_calls=0, paid_calls=0,
                            race_coverage="Read-only original inventories and ordinary CLI path checks; injected races are separate Rust evidence."))
        self.revision += 1

    def call(self, name, args, expected=0):
        index = len(self.calls) + 1
        stem = self.root / (f"command-{index:03d}-" + name)
        argv = [str(self.cli), *map(str, args)]
        started = time.monotonic()
        run = subprocess.run(argv, capture_output=True, timeout=40)
        out = stem.with_suffix(".stdout.txt"); err = stem.with_suffix(".stderr.txt")
        out.write_bytes(run.stdout); err.write_bytes(run.stderr)
        receipt = dict(index=index, name=name, argv=argv, exit_code=run.returncode, expected_exit=expected,
                       elapsed_seconds=time.monotonic() - started, stdout=dict(path=str(out), **fingerprint(run.stdout)),
                       stderr=dict(path=str(err), **fingerprint(run.stderr)))
        self.write_new(stem.with_suffix(".json"), receipt)
        self.calls.append(receipt); self.flush()
        assert run.returncode == expected, f"CLI {name}: expected {expected}, observed {run.returncode}; see {stem}"
        self.check_protected()
        return run

    def prepare(self, author, package_name):
        package = self.packages[package_name]
        name = author["name"]; folder = self.root / name; folder.mkdir()
        self.write_new(folder / "frozen-author.json", author)
        (folder / "frozen-authors-original.json").write_bytes(package["authors.json"])
        self.write_new(folder / "coordination-expected.json", author["coordination_expected"])
        (folder / "expected.json").write_bytes(package[author["presentation_expected_file"]])
        for item in author["inputs"]:
            ident = item["input_id"]
            (folder / ("selected-" + ident + ".txt")).write_bytes(package[item["source_file"]])
            self.write_new(folder / ("frozen-gold-" + ident + ".json"), item["gold"])
        # Every author expectation and original is now present before init/import.
        for path in folder.iterdir(): self.protect(path)
        project, case = folder / "project", folder / "case"
        self.call(name + "-init", ["project", "init", project])
        case.mkdir(); self.write_new(case / "identity.json", author["identity"])
        inputs = []
        for item in author["inputs"]:
            ident = item["input_id"]; gold = item["gold"]
            args = ["source", "import", project, folder / ("selected-" + ident + ".txt")]
            if inputs:
                if author["same_source"]:
                    source_id = inputs[0]["binding"]["source_id"]
                else:
                    seed = folder / (ident + "-unique-seed.txt")
                    seed.write_bytes(("独立来源登记种子 " + ident + " " + uuid.uuid4().hex).encode())
                    imported = strict_json(self.call(name + "-seed-" + ident, ["source", "import", project, seed]).stdout)
                    source_id = imported["version"]["source_id"]
                args += ["--source-id", source_id]
            imported = strict_json(self.call(name + "-import-" + ident, args).stdout)
            selection = {"kind": "text", "ordinal": 0}
            source = actual_source(project, imported["import_id"], selection)
            binding, unit = source["binding"], source["units"][0]
            review_dir = folder / ("review-" + ident); review_dir.mkdir()
            gold_raw = guarded_read(folder / ("frozen-gold-" + ident + ".json"), 65536)
            (review_dir / "gold.json").write_bytes(gold_raw)
            atom_id, evidence_id = item["original_atom_id"], ident + "-support"
            candidate = strict_json(guarded_read(REPO / "fixtures/extraction-review-v1/candidates.json", 1048576))
            candidate.update(binding=binding, request_id="synthetic-coordination-" + ident)
            payload = candidate["payload"]
            payload.update(binding=binding, request_id=candidate["request_id"], concepts=[
                dict(concept_id=ident + "-subject", revision=1, canonical_name=item["subject"], aliases=[], needs_full_explanation=True),
                dict(concept_id=ident + "-property", revision=1, canonical_name=item["property"], aliases=[], needs_full_explanation=True)])
            def evidence(eid, quote):
                quote_raw = quote.encode(); start = unit["start_byte"] + unit["text"].encode().index(quote_raw)
                return dict(evidence_id=eid, unit_id=unit["unit_id"], excerpt_start_byte=start, excerpt_end_byte=start + len(quote_raw),
                            excerpt_sha256=hashlib.sha256(quote_raw).hexdigest(), quote=quote)
            support_quote = gold["quote"] if author["identity"]["schema_version"] == 1 else item["context_quote"]
            payload["evidence"] = [evidence(evidence_id, support_quote)]
            atom = payload["atoms"][0]
            atom["atom"].update(atom_id=atom_id, concept_ids=[ident + "-subject", ident + "-property"], content=gold["atom_content"],
                                qualifiers=[gold["unit"], *gold["conditions"], *gold["exceptions"]], evidence_links=[dict(evidence_id=evidence_id, relation="supports")])
            atom["measurement"] = dict(value_status="known", value=gold["value"], unit=gold["unit"], conditions=gold["conditions"])
            for counter in item.get("counter_evidence", []):
                payload["evidence"].append(evidence(counter["evidence_id"], counter["quote"]))
                atom["atom"]["evidence_links"].append(dict(evidence_id=counter["evidence_id"], relation="refutes"))
            for additional in item.get("additional_evidence", []):
                payload["evidence"].append(evidence(additional["evidence_id"], additional["quote"]))
                assert additional["relation"] in ("supports", "refutes", "qualifies"), "authored evidence relation"
                atom["atom"]["evidence_links"].append(dict(evidence_id=additional["evidence_id"], relation=additional["relation"]))
            self.write_new(review_dir / "candidates.json", candidate)
            self.write_new(review_dir / "decision.json", dict(schema_version=1, unit_id=unit["unit_id"], disposition="included", atom_ids=[atom_id], exclusion_reason=None))
            self.write_new(review_dir / "review.json", dict(schema_version=1, purpose="single_measurement_source_review", candidate_file="candidates.json", gold_file="gold.json",
                decision_file="decision.json", gold_sha256=fingerprint(gold_raw)["sha256"], subject=item["subject"], property=item["property"], allowed_contexts=[],
                selection=[selection], reviewer="automated_source_review", interpretation_timing="before_synthetic_response"))
            bundle = case / ("bundle-" + ident)
            self.call(name + "-review-" + ident, ["extract", "verify", project, imported["import_id"], review_dir, bundle])
            inputs.append(dict(input_id=ident, review_bundle=bundle.name, binding=binding,
                               artifacts={file: fingerprint(guarded_read(bundle / file, FILES[file])) for file in FILES}))
        version = author["identity"]["schema_version"]
        purpose = {1: "explicit_identity_duplicate_complement", 2: "explicit_identity_condition_time", 3: "explicit_identity_conflict_review"}[version]
        self.write_new(case / "coordination.json", dict(schema_version=version, purpose=purpose, project_id=inputs[0]["binding"]["project_id"], identity_file="identity.json", inputs=inputs))
        self.protect(project); self.protect(case)
        return folder, project, case

    @staticmethod
    def synchronize(folder):
        """A hostile editor repairs every report hash after changing a real note."""
        report_path = folder / "presentation.json"; report = strict_json(report_path.read_bytes())
        if (folder / "plan.json").exists():
            plan_raw = (folder / "plan.json").read_bytes()
            plan = strict_json(plan_raw)
            if fingerprint(plan_raw) != report["plan_fingerprint"]:
                # The attacker can repair its own plan, Markdown, UTF8 mapping,
                # counts and hashes together; these are never the expectation.
                forged_render = render_independent(plan)
                for name, (raw, _) in forged_render.items():
                    (folder / name).write_bytes(raw)
                report["files"] = [v[1] for v in forged_render.values()]
                body = [c for n in plan["notes"] for node in n["nodes"] if node["kind"] == "table" for r in node["rows"] for c in r["cells"]]
                report["totals"].update(facts=len(plan["facts"]), source_occurrences=len(plan["provenance"]), objects=len(plan["objects"]), properties=len(plan["properties"]),
                    table_cells=len(body), missing_cells=sum(c["kind"] == "missing" for c in body), rendered_notes=len(report["files"]),
                    content_mappings=sum(len(f["content_mappings"]) for f in report["files"]), links=sum(len(f["links"]) for f in report["files"]))
            report["plan_fingerprint"] = fingerprint(plan_raw)
        for name in report["artifacts"]:
            report["artifacts"][name] = fingerprint((folder / name).read_bytes())
        for file in report["files"]:
            raw = (folder / file["relative_path"]).read_bytes()
            file.update(content_sha256=hashlib.sha256(raw).hexdigest(), byte_length=len(raw))
        report_path.write_bytes(encoded(report))
        preamble = ("# 有据主动表达预览\n\n状态：" + report["status"] + "；预览：" + report["preview_status"] + "；知识状态：pending；发布：false。\n\n"
                    "科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n")
        (folder / "presentation.md").write_bytes((preamble + "```json\n" + encoded(report).decode().rstrip("\n") + "\n```\n").encode())

    def forged(self, name, project, case, coord, preview, expected, mutate):
        forged = preview.parent / ("forged-" + name); shutil.copytree(preview, forged)
        mutate(forged); self.synchronize(forged)
        before = inventory(forged)
        try:
            verify_report(project, case, coord, forged, expected)
        except AssertionError as exc:
            result = dict(name=name, kind="independent_synchronized_forgery_rejection", rejection=str(exc), forged=str(forged))
        else:
            raise AssertionError("independent verifier accepted synchronized forgery: " + name)
        exact(inventory(forged), before, "forgery checker readonly")
        self.check_protected(); self.rows.append(result); self.flush()

    def matrix(self):
        cases = {}
        for author, package_name in self.cases:
            name = author["name"]; folder, project, case = self.prepare(author, package_name)
            coord, preview = folder / "coordination-output", folder / "preview-Vault"
            good = author["coordination_expected"]["expected_status"] == "passed"
            coordinate = self.call(name + "-coordinate", ["knowledge", "coordinate", "--", project, case, coord], 0 if good else 3)
            verify_coordination(project, case, coord, guarded_read(folder / "coordination-expected.json", 1048576))
            exact(strict_json(coordinate.stdout), strict_json(guarded_read(coord / "coordination.json", 8388608)), "coordinate stdout")
            self.protect(coord)
            present = self.call(name + "-present", ["knowledge", "present", "--", project, case, coord, preview], 0 if good else 3)
            checked = verify_report(project, case, coord, preview, folder / "expected.json")
            exact(strict_json(present.stdout), strict_json(guarded_read(preview / "presentation.json", MAX_BYTES)), "present stdout")
            self.rows.append(dict(name=name, kind="ordinary_cli_and_independent_rebuild", check=checked, folder=str(folder), call_index=len(self.calls)))
            self.protect(preview); self.flush()
            cases[name] = (project, case, coord, preview, folder / "expected.json")
        normal = cases["two_objects"]
        def change_plan(folder, mutation):
            path = folder / "plan.json"; plan = strict_json(path.read_bytes()); mutation(plan); path.write_bytes(encoded(plan))
        def alter_fact_everywhere(plan, field, value):
            fact = plan["facts"][0]; old_text = fact_text(fact); fact[field] = value; new_text = fact_text(fact)
            for note in plan["notes"]:
                for node in note["nodes"]:
                    if node["kind"] == "paragraph" and fact["fact_id"] in node["fact_ids"]:
                        node["text"] = node["text"].replace(old_text, new_text)
                    elif node["kind"] == "table":
                        for row in node["rows"]:
                            for cell in row["cells"]:
                                if fact["fact_id"] in cell["fact_ids"]:
                                    cell["text"] = cell["text"].replace(old_text, new_text)
        for name, field, value in (("delete-conditions", "conditions", []), ("delete-unit", "unit", ""), ("delete-exceptions", "exceptions", []),
                                   ("drop-time", "time", dict(status="unknown", precision=None, value=None))):
            self.forged(name, *normal, lambda folder, field=field, value=value: change_plan(folder, lambda plan: alter_fact_everywhere(plan, field, value)))
        def misplaced_cells(plan):
            rows = plan["notes"][0]["nodes"][0]["rows"]
            rows[0]["cells"][1], rows[1]["cells"][1] = rows[1]["cells"][1], rows[0]["cells"][1]
        self.forged("wrong-object-row", *normal, lambda folder: change_plan(folder, misplaced_cells))
        def misplaced_property_cells(plan):
            table = plan["notes"][0]["nodes"][0]
            row = next(r for r in table["rows"] if sum(c["kind"] == "facts" for c in r["cells"]) >= 2)
            left, right = [c for c in row["cells"] if c["kind"] == "facts"][:2]
            # Keep the column positions, but move each complete fact payload and
            # provenance to the other property. The attacker repairs mappings.
            left_id, right_id = left["column_id"], right["column_id"]
            left_payload, right_payload = copy.deepcopy(left), copy.deepcopy(right)
            left.clear(); left.update(right_payload, column_id=left_id)
            right.clear(); right.update(left_payload, column_id=right_id)
        self.forged("wrong-property-column", *cases["sparse_properties"],
                    lambda folder: change_plan(folder, misplaced_property_cells))
        def mislabeled_property_headers(plan):
            columns = plan["notes"][0]["nodes"][0]["columns"][1:]
            assert len(columns) >= 2, "two authored property columns for attack"
            for field in ("label", "concept_ids", "occurrence_ids"):
                columns[0][field], columns[1][field] = columns[1][field], columns[0][field]
        self.forged("wrong-property-header-provenance", *cases["sparse_properties"],
                    lambda folder: change_plan(folder, mislabeled_property_headers))
        self.forged("drop-source-evidence", *normal, lambda folder: change_plan(folder, lambda plan: plan["provenance"][0]["evidence"].clear()))
        def note_edit(folder):
            path = folder / "知识比较.md"; raw = path.read_bytes()
            assert "温度 20 ℃".encode() in raw, "real rendered condition for attack"
            path.write_bytes(raw.replace("温度 20 ℃".encode(), "温度 21 ℃".encode(), 1))
        self.forged("actual-note-changed-and-rehashed", *normal, note_edit)
        def counts_bool(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes())
            report["totals"]["input_bundles"] = True; path.write_bytes(encoded(report))
        self.forged("boolean-count", *normal, counts_bool)
        def extra_field(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes()); report["unknown"] = 0; path.write_bytes(encoded(report))
        self.forged("unknown-report-field", *normal, extra_field)
        def false_science(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes())
            report.update(scientific_quality="passed", published=True); path.write_bytes(encoded(report))
        self.forged("science-publish-false-pass", *normal, false_science)
        def status_string(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes())
            report["checks"]["source_review"] = "true"; path.write_bytes(encoded(report))
        self.forged("invalid-string-check-status", *normal, status_string)
        def wrong_range(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes())
            report["files"][0]["content_mappings"][0]["start_byte"] += 1; path.write_bytes(encoded(report))
        self.forged("wrong-utf8-content-range", *normal, wrong_range)
        def wrong_link(folder):
            def mutate(plan):
                rows = plan["notes"][0]["nodes"][0]["rows"]
                left, right = rows[0]["cells"][0]["links"][0], rows[1]["cells"][0]["links"][0]
                left["target_concept_id"], left["target_path"] = right["target_concept_id"], right["target_path"]
            change_plan(folder, mutate)
        self.forged("wrong-table-link-target", *normal, wrong_link)
        def missing_zero(folder):
            def mutate(plan):
                cell = next(c for r in plan["notes"][0]["nodes"][0]["rows"] for c in r["cells"] if c["kind"] == "missing")
                cell["text"] = "0"
            change_plan(folder, mutate)
        self.forged("missing-default-zero", *cases["sparse_properties"], missing_zero)
        def missing_group(folder):
            def mutate(plan):
                cell = next(c for r in plan["notes"][0]["nodes"][0]["rows"] for c in r["cells"] if len(c["fact_ids"]) > 1)
                cell["fact_ids"].pop(); cell["text"] = cell["text"].split("\n")[0]
            change_plan(folder, mutate)
        self.forged("drop-second-context-group", *cases["multi_context_cell"], missing_group)
        self.forged("drop-history-version", *cases["history_cell"], missing_group)
        def drop_legacy_qualifier(folder):
            def mutate(plan):
                provenance = next(p for p in plan["provenance"] if p["input_id"] == "a")
                atom = provenance["original_atom"]["atom"]
                removed = {l["evidence_id"] for l in atom["evidence_links"] if l["relation"] == "qualifies"}
                assert removed, "real authored qualifier evidence for attack"
                atom["evidence_links"] = [l for l in atom["evidence_links"] if l["evidence_id"] not in removed]
                provenance["evidence"] = [e for e in provenance["evidence"] if e["evidence_id"] not in removed]
            change_plan(folder, mutate)
        for version in (1, 2):
            self.forged("v" + str(version) + "-drop-qualifier-provenance", *cases["legacy_v" + str(version) + "_two_objects"], drop_legacy_qualifier)
        def false_pass(folder):
            path = folder / "presentation.json"; report = strict_json(path.read_bytes())
            report.update(status="passed", preview_status="rendered", issue_codes=[])
            report["checks"] = dict.fromkeys(report["checks"], "passed")
            path.write_bytes(encoded(report))
        self.forged("unresolved-coordination-false-pass", *cases["blocked_conflict"], false_pass)
        self.forged("unknown-preview-file", *normal, lambda folder: (folder / "human.txt").write_bytes(b"unknown human content\n"))
        # Each rejection uses a new output and preserves external sentinels.
        project, case, coord, preview, expected = normal
        existing = preview.parent / "existing-human-Vault"; existing.mkdir()
        sentinel = existing / "human.txt"; sentinel.write_bytes(b"human vault content\n"); self.protect(existing)
        run = self.call("existing-output", ["knowledge", "present", project, case, coord, existing], 2)
        assert not (existing / "plan.json").exists(), "existing Vault not adopted"
        self.rows.append(dict(name="existing-output", kind="ordinary_cli_path_rejection", call_index=len(self.calls)))
        # A real unknown file is rejected before any successful expression.
        unknown_case = preview.parent / "unknown-case"; shutil.copytree(case, unknown_case)
        (unknown_case / "human.txt").write_bytes(b"keep unknown input\n"); self.protect(unknown_case)
        rejected = preview.parent / "unknown-output"
        self.call("unknown-case-file", ["knowledge", "present", project, unknown_case, coord, rejected], 2)
        assert not rejected.exists(), "invalid case rejected before creating output"
        self.rows.append(dict(name="unknown-case-file", kind="ordinary_cli_path_rejection", call_index=len(self.calls)))
        link = preview.parent / "ancestor-link"
        try:
            link.symlink_to(preview.parent, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.rows.append(dict(name="ancestor-link", kind="not_available_on_this_host"))
        else:
            linked_output = link / "linked-output"
            self.call("linked-output-ancestor", ["knowledge", "present", project, case, coord, linked_output], 2)
            assert not linked_output.exists(), "linked output ancestor rejected"
            self.rows.append(dict(name="ancestor-link", kind="ordinary_cli_path_rejection", call_index=len(self.calls)))
        self.check_protected(); self.flush()
        return dict(status="passed", scenarios=len(self.rows), ordinary_cli_calls=len(self.calls), root=str(self.root),
                    independently_verified=True, knowledge_status="pending", scientific_quality="not_run", published=False)


def audit(cli, root):
    driver = PresentationAudit(cli, root)
    try:
        return driver.matrix()
    except (AssertionError, ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as exc:
        driver.flush(error=str(exc))
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    check = sub.add_parser("check")
    for name in ("project", "coordination_case", "coordination_output", "preview_root", "expected"):
        check.add_argument(name, type=Path)
    runner = sub.add_parser("audit")
    runner.add_argument("absolute_cli", type=Path); runner.add_argument("nonexistent_root", type=Path)
    args = parser.parse_args(argv)
    try:
        result = (audit(args.absolute_cli, args.nonexistent_root) if args.mode == "audit" else
                  verify_report(args.project, args.coordination_case, args.coordination_output, args.preview_root, args.expected))
    except (AssertionError, ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as exc:
        print("independent presentation verification rejected: " + str(exc), file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
