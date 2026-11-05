"""Independent Schema3 conflicts, retained opposition and processing reconstruction."""
from __future__ import annotations

import argparse
from datetime import date
import hashlib
import itertools
from pathlib import Path
import re
import sqlite3
import stat
import sys
import time

from verify_knowledge_coordination import (REPORT, TOTALS as V1_TOTALS, FILES,
    strict_json, fingerprint, encoded, compact, normalized, fields, exact, text,
    strings, integer, identifier, uuid_string, safe_name, binding, metadata,
    inventory, read, source_bundle, verify_markdown, display_text)

SCOPE = "explicit_identity_conflict_review_v3"
TOTALS = V1_TOTALS + " condition_variants historical_relations known_condition_occurrences unknown_condition_occurrences not_applicable_condition_occurrences dated_occurrences undated_occurrences context_evidence_links true_conflicts insufficient_evidence_relations conflicting_occurrences insufficient_evidence_occurrences counter_evidence_links"
FACT_AUTHOR = "object_id property_id value_status value unit conditions exceptions applicability_status time input_ids original_atom_ids status issue_codes"
OCC_AUTHOR = "input_id original_atom_id value unit conditions exceptions applicability_status time context_quote status issue_codes"
REL_AUTHOR = "left_input_id right_input_id relation issue_code earlier_input_id later_input_id"


def guard_path(path, *, directory=None):
    """Reject links in supplied lexical components before resolving or reading."""
    supplied = Path(path)
    parts = supplied.parts if supplied.is_absolute() else (*Path.cwd().parts, *supplied.parts)
    prefix = Path(parts[0])
    for index, part in enumerate(parts):
        if index:
            prefix = prefix / part
        info = prefix.lstat()
        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400, "ancestor link/reparse path"
        assert stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), "nonordinary path component"
        if index < len(parts) - 1:
            assert stat.S_ISDIR(info.st_mode), "nondirectory ancestor"
    if directory is True:
        assert stat.S_ISDIR(info.st_mode), "root directory required"
    elif directory is False:
        assert stat.S_ISREG(info.st_mode) and info.st_nlink == 1, "ordinary single-link file required"


def time_schema(t):
    fields(t, "status precision value")
    if t["status"] == "unknown":
        exact(t, dict(status="unknown", precision=None, value=None), "unknown time")
        return None
    assert t["status"] == "known" and t["precision"] in ("year", "day"), "known time enum"
    value = t["value"]
    assert type(value) is str, "time value type"
    if t["precision"] == "year":
        assert re.fullmatch(r"[0-9]{4}", value) and 1 <= int(value) <= 9999, "Gregorian year"
        return date(int(value), 1, 1), date(int(value), 12, 31)
    assert re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", value), "Gregorian day"
    d = date.fromisoformat(value)
    return d, d


def applicability(status, conditions):
    assert status in ("known", "unknown", "not_applicable"), "applicability enum"
    assert bool(conditions) == (status == "known"), "full applicability conditions"


def gold_schema(g):
    fields(g, "schema_version author scope scientific_quality knowledge_status published case_name expected_status input_ids facts occurrences relations totals")
    exact({k: g[k] for k in ("schema_version", "scope", "scientific_quality", "knowledge_status", "published")},
          dict(schema_version=3, scope=SCOPE, scientific_quality="not_run", knowledge_status="pending", published=False), "v3 gold fixed")
    text(g["author"]); text(g["case_name"])
    assert g["expected_status"] in ("passed", "needs_review"), "author status"
    assert type(g["input_ids"]) is list and 2 <= len(g["input_ids"]) <= 3 and len(set(g["input_ids"])) == len(g["input_ids"]), "author inputs"
    for ident in g["input_ids"]:
        identifier(ident)
    for collection, names in (("facts", FACT_AUTHOR), ("occurrences", OCC_AUTHOR), ("relations", REL_AUTHOR)):
        assert type(g[collection]) is list and len(g[collection]) <= 3, "author array bounds"
        for item in g[collection]:
            fields(item, names)
            if collection == "relations":
                assert item["relation"] in ("duplicate", "complement", "separate_object", "condition_variant", "historical_evolution", "numeric_equivalent", "true_conflict", "insufficient_evidence"), "author relation"
                for k in ("left_input_id", "right_input_id"):
                    identifier(item[k]); assert item[k] in g["input_ids"], "author pair input"
                for k in ("earlier_input_id", "later_input_id"):
                    if item["relation"] == "historical_evolution":
                        assert item[k] in (item["left_input_id"], item["right_input_id"]), "author historical direction"
                    else:
                        exact(item[k], None, "author required relation null")
                assert (item["issue_code"] is None) == (item["relation"] not in ("true_conflict", "insufficient_evidence")), "author relation issue"
                continue
            for k in ("value", "unit"):
                text(item[k])
            strings(item["conditions"]); strings(item["exceptions"]); strings(item["issue_codes"])
            applicability(item["applicability_status"], item["conditions"]); time_schema(item["time"])
            assert item["status"] in ("passed", "needs_review"), "author occurrence/fact status"
            if collection == "facts":
                uuid_string(item["object_id"]); uuid_string(item["property_id"])
                exact(item["value_status"], "known", "author value status")
                strings(item["input_ids"])
                # Original IDs are input-qualified; different inputs can retain
                # the same original bytes without becoming the same occurrence.
                originals = item["original_atom_ids"]
                assert type(originals) is list and len(originals) <= 8, "author original IDs array bounds"
                for original in originals:
                    text(original)
                assert len(item["input_ids"]) == len(item["original_atom_ids"]), "author fact member pairing"
            else:
                identifier(item["input_id"]); text(item["original_atom_id"])
                assert type(item["context_quote"]) is str and len(item["context_quote"].encode()) <= 1024, "author quote bounds"
    fields(g["totals"], TOTALS)
    for k, v in g["totals"].items():
        if k == "canonical_facts" and g["expected_status"] == "needs_review":
            exact(v, None, "author canonical required null")
        else:
            integer(v)
    exact(sorted(x["input_id"] for x in g["occurrences"]), sorted(g["input_ids"]), "author occurrence coverage")
    exact(sorted(i for f in g["facts"] for i in f["input_ids"]), sorted(g["input_ids"]), "author fact coverage")
    pairs = {tuple(sorted((r["left_input_id"], r["right_input_id"]))) for r in g["relations"]}
    assert len(pairs) == len(g["relations"]) and pairs == set(itertools.combinations(sorted(g["input_ids"]), 2)), "author complete unique pairs"


def rebuild_context(declaration, source, gold, evidence):
    fields(declaration, "input_id original_atom_id applicability_status time context_evidence_ids")
    time_schema(declaration["time"]); applicability(declaration["applicability_status"], gold["conditions"])
    strings(declaration["context_evidence_ids"], 1024)
    assert declaration["context_evidence_ids"], "context evidence nonempty"
    unit = source["units"][0]
    body, quote = unit["text"], gold["quote"]
    assert body.count(quote) == 1, "unique actual gold quote"
    position = body.index(quote)
    assert position and body[position - 1] == "\n", "adjacent background LF"
    begin = body.rfind("\n", 0, position - 1) + 1
    line = body[begin:position - 1]
    match = re.fullmatch(r"适用条件：(已列明|未给出|不适用)；观点时间：(未给出|[0-9]{4}年|[0-9]{4}-[0-9]{2}-[0-9]{2})。", line)
    assert match, "complete adjacent actual background"
    state = {"已列明": "known", "未给出": "unknown", "不适用": "not_applicable"}[match[1]]
    stamp = match[2]
    t = (dict(status="unknown", precision=None, value=None) if stamp == "未给出" else
         dict(status="known", precision="year" if stamp.endswith("年") else "day", value=stamp.removesuffix("年")))
    time_schema(t); applicability(state, gold["conditions"])
    exact(declaration["applicability_status"], state, "actual source applicability")
    exact(declaration["time"], t, "actual source time")
    union = body[begin:position + len(quote)]
    start = unit["start_byte"] + len(body[:begin].encode())
    end = start + len(union.encode())
    ids = sorted(e["evidence_id"] for e in evidence if e["unit_id"] == unit["unit_id"] and
                 e["excerpt_start_byte"] <= start and e["excerpt_end_byte"] >= end)
    assert ids and ids == sorted(declaration["context_evidence_ids"]), "exact actual background support set"
    return dict(applicability_status=state, time=t, evidence_ids=ids, unit_id=unit["unit_id"],
                start_byte=start, end_byte=end, quote=union, sha256=hashlib.sha256(union.encode()).hexdigest())


def fact_key(o):
    c = o["context"]; t = c["time"]
    return ["coordination_v3", o["object_id"], o["property_id"], normalized(o["value"]), normalized(o["unit_value"]),
            c["applicability_status"], sorted(map(normalized, o["conditions"])), sorted(map(normalized, o["exceptions"])),
            [t["status"], t["precision"], t["value"]], o["occurrence_id"] if c["applicability_status"] == "unknown" else None]


def chronological(left, right):
    l, r = time_schema(left["context"]["time"]), time_schema(right["context"]["time"])
    if l and r:
        if l[1] < r[0]:
            return left["occurrence_id"], right["occurrence_id"]
        if r[1] < l[0]:
            return right["occurrence_id"], left["occurrence_id"]
    return None


def decimal_value(value):
    """Exact finite decimal tuple, with bounded ASCII grammar and no rounding."""
    value = normalized(value)
    if len(value.encode()) > 1024 or not re.fullmatch(r"[-+]?[0-9]+(?:\.[0-9]+)?", value):
        return None
    negative = value.startswith("-")
    whole, _, fraction = value.lstrip("+-").partition(".")
    whole = whole.lstrip("0") or "0"
    fraction = fraction.rstrip("0")
    return (negative if whole != "0" or fraction else False, whole, fraction)


def classify(left, right):
    lk, rk = fact_key(left), fact_key(right)
    lb, rb = left["binding"], right["binding"]
    direction = chronological(left, right)
    relation, issue = "insufficient_evidence", None
    if lk[1] != rk[1]:
        relation = "separate_object"
    elif lk[2] != rk[2]:
        relation = "complement"
    elif "unknown" in (lk[5], rk[5]):
        issue = "COORD_UNKNOWN_CONDITION"
    elif lb["source_id"] == rb["source_id"] and (lb["version_id"], lb["parse_id"]) != (rb["version_id"], rb["parse_id"]) and not direction:
        issue = "COORD_UNHANDLED_VERSION"
    elif lk[4] != rk[4]:
        issue = "COORD_UNIT_NOT_COMPARABLE"
    elif lk == rk:
        relation = "duplicate"
    elif lk[5:8] != rk[5:8]:
        relation = "condition_variant"
    elif direction:
        relation = "historical_evolution"
    elif "unknown" in (lk[8][0], rk[8][0]) or lk[8] != rk[8]:
        issue = "COORD_TIME_NOT_COMPARABLE"
    else:
        a, b = decimal_value(lk[3]), decimal_value(rk[3])
        if a is not None and b is not None:
            if a == b:
                relation = "numeric_equivalent"
            else:
                relation, issue = "true_conflict", "COORD_TRUE_CONFLICT"
        else:
            issue = "COORD_VALUE_NOT_COMPARABLE"
    return dict(left_occurrence_id=left["occurrence_id"], right_occurrence_id=right["occurrence_id"], relation=relation,
                issue_code=issue, earlier_occurrence_id=direction[0] if relation == "historical_evolution" else None,
                later_occurrence_id=direction[1] if relation == "historical_evolution" else None)


def _verify(project, case, output, gold_bytes):
    started = time.monotonic()
    for root in (project, case, output):
        guard_path(root, directory=True)
    gold = strict_json(gold_bytes); gold_schema(gold)
    for l, r in itertools.combinations((project.resolve(), case.resolve(), output.resolve()), 2):
        assert not l.is_relative_to(r) and not r.is_relative_to(l), "overlapping verifier roots"
    before = [inventory(p) for p in (project, case, output)]
    case_raw = read(case / "coordination.json", 65536); manifest = strict_json(case_raw)
    fields(manifest, "schema_version purpose project_id identity_file inputs")
    exact(manifest["schema_version"], 3, "case v3"); exact(manifest["purpose"], "explicit_identity_conflict_review", "case purpose")
    uuid_string(manifest["project_id"]); safe_name(manifest["identity_file"])
    assert type(manifest["inputs"]) is list and 2 <= len(manifest["inputs"]) <= 3, "case input count"
    map_raw = read(case / manifest["identity_file"], 65536); mapping = strict_json(map_raw)
    fields(mapping, "schema_version author concepts concept_mappings atom_mappings atom_contexts")
    exact(mapping["schema_version"], 3, "map v3"); text(mapping["author"]); metadata(mapping, identity_display_labels=True)
    for k in ("concepts", "concept_mappings", "atom_mappings", "atom_contexts"):
        assert type(mapping[k]) is list and len(mapping[k]) <= 1024, "map bounds"
    canonical, cm, am, contexts = {}, {}, {}, {}
    for c in mapping["concepts"]:
        fields(c, "canonical_id display_name"); uuid_string(c["canonical_id"]); display_text(c["display_name"])
        assert c["canonical_id"] not in canonical, "unique canonical"
        canonical[c["canonical_id"]] = c
    for c in mapping["concept_mappings"]:
        fields(c, "input_id original_concept_id canonical_id"); identifier(c["input_id"]); text(c["original_concept_id"])
        key = c["input_id"], c["original_concept_id"]
        assert key not in cm and c["canonical_id"] in canonical, "concept reference"
        cm[key] = c["canonical_id"]
    for a in mapping["atom_mappings"]:
        fields(a, "input_id original_atom_id object_id property_id"); identifier(a["input_id"]); text(a["original_atom_id"])
        assert a["input_id"] not in am and a["object_id"] != a["property_id"] and all(a[k] in canonical for k in ("object_id", "property_id")), "atom role reference"
        am[a["input_id"]] = a
    for c in mapping["atom_contexts"]:
        fields(c, "input_id original_atom_id applicability_status time context_evidence_ids")
        identifier(c["input_id"]); text(c["original_atom_id"])
        assert c["input_id"] not in contexts, "unique context"
        contexts[c["input_id"]] = c
    inputs, occurrences, refs, locations = [], [], set(), set()
    copied = {"coordination-original.json": case_raw, "identity-original.json": map_raw}
    names = ["coordination.json", manifest["identity_file"]]; byte_count = len(case_raw) + len(map_raw)
    for item in sorted(manifest["inputs"], key=lambda i: i["input_id"]):
        fields(item, "input_id review_bundle binding artifacts"); ident = item["input_id"]
        identifier(ident); safe_name(item["review_bundle"]); binding(item["binding"])
        assert ident not in {i["input_id"] for i in inputs}, "unique input"
        names.append(item["review_bundle"])
        raw, review_report, candidate, source_gold, review, source = source_bundle(project, case / item["review_bundle"], item["artifacts"], evidence_quote_lf=True)
        byte_count += sum(map(len, raw.values()))
        assert byte_count <= 96 * 1048576 and time.monotonic() - started <= 30, "input aggregate/time"
        exact(item["binding"], source["binding"], "actual input binding")
        exact(source["binding"]["project_id"], manifest["project_id"], "actual project")
        atom = candidate["payload"]["atoms"][0]; a = am[ident]; declaration = contexts[ident]
        exact(a["original_atom_id"], atom["atom"]["atom_id"], "actual atom identity")
        exact(declaration["original_atom_id"], a["original_atom_id"], "context actual atom")
        concepts = candidate["payload"]["concepts"]
        refs.update((ident, c["concept_id"]) for c in concepts)
        cc = sorted({cm[(ident, r)] for r in atom["atom"]["concept_ids"]})
        for role in ("object_id", "property_id"):
            target = review["subject" if role == "object_id" else "property"]
            matched = {cm[(ident, c["concept_id"])] for c in concepts if c["concept_id"] in atom["atom"]["concept_ids"] and normalized(target) in map(normalized, [c["canonical_name"], *c["aliases"]])}
            exact(matched, {a[role]}, "actual original subject/property role")
        author = next(f for f in gold["facts"] if ident in f["input_ids"])
        for k in ("object_id", "property_id"):
            exact(a[k], author[k], "author identity")
        exact(a["original_atom_id"], author["original_atom_ids"][author["input_ids"].index(ident)], "author original atom")
        for k in ("value", "unit", "conditions", "exceptions"):
            exact(source_gold[k], author[k], "author source " + k)
        b, unit = source["binding"], source["units"][0]
        loc = [b[k] for k in ("project_id", "source_id", "version_id", "parse_id")] + [unit[k] for k in ("unit_id", "start_byte", "end_byte")]
        assert compact(loc) not in locations, "unique actual location"
        locations.add(compact(loc))
        oid = "occ-" + hashlib.sha256(compact([*loc, a["original_atom_id"]])).hexdigest()
        evs = {e["evidence_id"]: e for e in candidate["payload"]["evidence"]}
        assert len(evs) == len(candidate["payload"]["evidence"]), "unique evidence"
        links = atom["atom"]["evidence_links"]
        evidence = list({l["evidence_id"]: evs[l["evidence_id"]] for l in links}.values())
        supports = [evs[l["evidence_id"]] for l in links if l["relation"] == "supports"]
        context = rebuild_context(declaration, source, source_gold, supports)
        exact(context["applicability_status"], author["applicability_status"], "author applicability")
        exact(context["time"], author["time"], "author time")
        occurrences.append(dict(occurrence_id=oid, input_id=ident, binding=b, unit=unit, original_concepts=concepts,
            original_atom=atom, original_evidence=evidence, canonical_concept_ids=cc, object_id=a["object_id"], property_id=a["property_id"],
            value=source_gold["value"], unit_value=source_gold["unit"], conditions=source_gold["conditions"], exceptions=source_gold["exceptions"],
            context=context, status="passed", issue_codes=[]))
        copy_names = {name: "input-" + ident + "--" + name for name in FILES}
        copied.update({copy_names[k]: v for k, v in raw.items()})
        inputs.append(dict(input_id=ident, binding=b, request_id=candidate["request_id"], source_review_status="passed",
                           source_review_checks=review_report["checks"], source_bundle_fingerprints=item["artifacts"], copied_artifacts=copy_names))
    members = {i["input_id"] for i in inputs}
    assert set(cm) == refs and set(am) == members and set(contexts) == members and set(cm.values()) == set(canonical), "complete identity/context coverage"
    assert len({x.lower() for x in names}) == len(names) and {p.name for p in case.iterdir()} == set(names), "exact case roles/inventory"
    exact(sorted(members), sorted(gold["input_ids"]), "author input coverage")
    occurrences.sort(key=lambda o: o["occurrence_id"])
    relations = [classify(l, r) for l, r in itertools.combinations(occurrences, 2)]
    issues = {o["occurrence_id"]: ({"COORD_UNKNOWN_CONDITION"} if o["context"]["applicability_status"] == "unknown" else set()) for o in occurrences}
    for r in relations:
        if r["issue_code"]:
            for k in ("left_occurrence_id", "right_occurrence_id"):
                issues[r[k]].add(r["issue_code"])
    facts = {}
    for o in occurrences:
        oid = o["occurrence_id"]; key = fact_key(o)
        o.update(status="needs_review" if issues[oid] else "passed", issue_codes=sorted(issues[oid]))
        fid = "fact-v3-" + hashlib.sha256(compact(key)).hexdigest()
        f = facts.setdefault(fid, dict(fact_id=fid, object_id=key[1], property_id=key[2], value_status="known", value=key[3], unit=key[4],
            applicability_status=key[5], conditions=key[6], exceptions=key[7], time=o["context"]["time"], occurrence_ids=[], status="passed", issue_codes=[]))
        f["occurrence_ids"].append(oid)
        f["issue_codes"] = sorted(set(f["issue_codes"]) | issues[oid])
        if f["issue_codes"]:
            f["status"] = "needs_review"
    status = "needs_review" if any(issues.values()) else "passed"; n = len(occurrences)
    totals = dict(input_bundles=n, validated_input_bundles=n, input_atoms=n, retained_fact_groups=len(facts),
        canonical_facts=len(facts) if status == "passed" else None, source_occurrences=n, evidence_links=sum(len(o["original_atom"]["atom"]["evidence_links"]) for o in occurrences),
        independent_sources=len({o["binding"]["source_id"] for o in occurrences}), duplicate_occurrences=n-len(facts), passed=sum(not issues[o["occurrence_id"]] for o in occurrences), failed=0,
        needs_review=sum(bool(issues[o["occurrence_id"]]) for o in occurrences), unprocessed=0,
        condition_variants=sum(r["relation"] == "condition_variant" for r in relations), historical_relations=sum(r["relation"] == "historical_evolution" for r in relations),
        known_condition_occurrences=sum(o["context"]["applicability_status"] == "known" for o in occurrences),
        unknown_condition_occurrences=sum(o["context"]["applicability_status"] == "unknown" for o in occurrences),
        not_applicable_condition_occurrences=sum(o["context"]["applicability_status"] == "not_applicable" for o in occurrences),
        dated_occurrences=sum(o["context"]["time"]["status"] == "known" for o in occurrences), undated_occurrences=sum(o["context"]["time"]["status"] == "unknown" for o in occurrences),
        context_evidence_links=sum(len(o["context"]["evidence_ids"]) for o in occurrences),
        true_conflicts=sum(r["relation"] == "true_conflict" for r in relations),
        insufficient_evidence_relations=sum(r["relation"] == "insufficient_evidence" for r in relations),
        conflicting_occurrences=sum("COORD_TRUE_CONFLICT" in issues[o["occurrence_id"]] for o in occurrences),
        insufficient_evidence_occurrences=len({r[k] for r in relations if r["relation"] == "insufficient_evidence" for k in ("left_occurrence_id", "right_occurrence_id")} | {o["occurrence_id"] for o in occurrences if o["context"]["applicability_status"] == "unknown"}),
        counter_evidence_links=sum(l["relation"] == "refutes" for o in occurrences for l in o["original_atom"]["atom"]["evidence_links"]))
    exact(totals, gold["totals"], "all author denominators"); exact(status, gold["expected_status"], "author status")
    by_oid = {o["occurrence_id"]: o for o in occurrences}
    author_occurrences = []
    for o in occurrences:
        author_occurrences.append(dict(input_id=o["input_id"], original_atom_id=o["original_atom"]["atom"]["atom_id"], value=o["value"], unit=o["unit_value"], conditions=o["conditions"], exceptions=o["exceptions"],
            applicability_status=o["context"]["applicability_status"], time=o["context"]["time"], context_quote=o["context"]["quote"], status=o["status"], issue_codes=o["issue_codes"]))
    exact(sorted(author_occurrences, key=lambda o: o["input_id"]), sorted(gold["occurrences"], key=lambda o: o["input_id"]), "all author occurrences")
    author_facts = []
    for f in facts.values():
        ids = sorted(by_oid[oid]["input_id"] for oid in f["occurrence_ids"])
        author_facts.append({**{k: f[k] for k in ("object_id", "property_id", "value_status", "value", "unit", "applicability_status", "time", "status", "issue_codes")},
            "conditions": next(o["conditions"] for o in occurrences if o["occurrence_id"] in f["occurrence_ids"]),
            "exceptions": next(o["exceptions"] for o in occurrences if o["occurrence_id"] in f["occurrence_ids"]), "input_ids": ids,
            "original_atom_ids": [am[ident]["original_atom_id"] for ident in ids]})
    exact(sorted(author_facts, key=lambda f: f["input_ids"]), sorted(gold["facts"], key=lambda f: f["input_ids"]), "all author fact groups")
    author_relations = []
    for r in relations:
        ids = sorted(by_oid[r[k]]["input_id"] for k in ("left_occurrence_id", "right_occurrence_id"))
        author_relations.append(dict(left_input_id=ids[0], right_input_id=ids[1], relation=r["relation"], issue_code=r["issue_code"],
            earlier_input_id=by_oid[r["earlier_occurrence_id"]]["input_id"] if r["earlier_occurrence_id"] else None,
            later_input_id=by_oid[r["later_occurrence_id"]]["input_id"] if r["later_occurrence_id"] else None))
    exact(sorted(author_relations, key=lambda r: (r["left_input_id"], r["right_input_id"])), sorted(gold["relations"], key=lambda r: (r["left_input_id"], r["right_input_id"])), "all author pair directions")
    report = strict_json(read(output / "coordination.json", 8388608)); fields(report, REPORT + " processing_records"); uuid_string(report["request_id"])
    shared = dict(schema_version=3, producer="knowledge_coordination", request_id=report["request_id"], scope=SCOPE, project_id=manifest["project_id"], knowledge_status="pending", published=False,
        scientific_quality="not_run", research_status="not_requested", canonical_concepts=sorted(canonical.values(), key=lambda c: c["canonical_id"]), inputs=inputs,
        occurrences=occurrences, facts=sorted(facts.values(), key=lambda f: f["fact_id"]), relations=relations, totals=totals,
        processing_records=[dict(occurrence_id=o["occurrence_id"], object_id=o["object_id"], property_id=o["property_id"],
            status=o["status"], issue_codes=o["issue_codes"], action="retain_for_review" if o["issue_codes"] else "retained",
            reason="; ".join(o["issue_codes"]) if o["issue_codes"] else "retained with complete source provenance") for o in occurrences])
    owner = read(output / ".owner.json", 4096)
    exact(strict_json(owner), dict(schema_version=1, producer="knowledge_coordination", request_id=report["request_id"], purpose="unpublished_coordination"), "owner")
    copied[".owner.json"] = owner
    if status == "passed":
        ir = read(output / "coordination-ir.json", 8388608)
        exact(strict_json(ir), dict(shared, revision=1), "IR source/author reconstruction")
        copied["coordination-ir.json"] = ir
    assert {p.name for p in output.iterdir()} == {*copied, "coordination.json", "coordination.md"}, "exact output inventory/status"
    for name, raw in copied.items():
        exact((output / name).read_bytes(), raw, "saved original " + name)
    expected = dict(shared, report_kind="knowledge_coordination", status=status, network_calls=0, paid_calls=0, artifacts={k: fingerprint(v) for k, v in copied.items()})
    exact(report, expected, "report source/author reconstruction")
    summary = (f"条件已列明：{totals['known_condition_occurrences']}；条件未知：{totals['unknown_condition_occurrences']}；条件不适用：{totals['not_applicable_condition_occurrences']}；"
               f"时间已知：{totals['dated_occurrences']}；时间未知：{totals['undated_occurrences']}；条件并列关系：{totals['condition_variants']}；历史关系：{totals['historical_relations']}；真实冲突关系：{totals['true_conflicts']}；证据不足关系：{totals['insufficient_evidence_relations']}；反证链接：{totals['counter_evidence_links']}。完整来源、原对象、证据与分母与 coordination.json 一致。")
    preamble = "# 有界多来源知识协调\n\n状态：" + status + "；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n" + summary + "\n\n"
    verify_markdown(read(output / "coordination.md", 8388608), preamble, expected, "v3 visible Markdown and full JSON")
    exact([inventory(p) for p in (project, case, output)], before, "read-only inventory")
    assert time.monotonic() - started <= 30, "verification time limit"
    return dict(status=status, independently_verified=True, totals=totals, inputs=[i["input_id"] for i in inputs])


def verify_report(project: Path, case: Path, output: Path, gold: bytes) -> dict:
    try:
        return _verify(Path(project), Path(case), Path(output), gold)
    except (ValueError, KeyError, TypeError, OSError, StopIteration, sqlite3.Error) as exc:
        raise AssertionError("v3 coordination verification failed: " + type(exc).__name__) from exc


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    check = sub.add_parser("check")
    for name in ("project", "case", "output", "gold"):
        check.add_argument(name, type=Path)
    audit = sub.add_parser("audit"); audit.add_argument("cli", type=Path); audit.add_argument("root", type=Path)
    args = parser.parse_args(argv); runner = None
    try:
        if args.command == "check":
            for root in (args.project, args.case, args.output):
                guard_path(root, directory=True)
            guard_path(args.gold, directory=False)
            result = verify_report(args.project, args.case, args.output, args.gold.read_bytes())
        else:
            from knowledge_conflicts_audit import ConflictsAudit
            runner = ConflictsAudit(args.cli, args.root); result = runner.matrix()
        print(encoded(result).decode(), end=""); return 0
    except Exception as exc:
        if runner:
            runner.flush(error=repr(exc))
        print("v3 verifier: " + str(exc), file=sys.stderr); return 1


if __name__ == "__main__":
    raise SystemExit(main())
