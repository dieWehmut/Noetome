#!/usr/bin/env python3
"""Audit B19's fixed chapter reference against a reviewed, actual export (stdlib only).

CLI: --candidate snapshot.json --notes-root export --review review.json --output report.json
The optional --reference/--errata paths must contain the exact fixed bytes below.
No semantic verdict is inferred from a quote, substring, candidate flag, or model score.

Snapshot v1: {schema_version, producer:{kind,id}, run_id, source_id, source_sha256,
 source_mapping:{reference_source_id,actual_source_id,source_sha256,physical_pages,
 page_ranges}, atoms:[{id,payload:{atom:<complete native atom including measurement>,
 evidence:[{relation,evidence:<complete native evidence>}]},note_paths:[relative.md]}]}.
native_concepts/native_issues are required complete native arrays, not summaries.
The notes root must be an actual precise native export with its complete indexes,
source/scope reports and manifest; all wrapper atoms and copies must match it.

Review v1: {schema_version,reference_sha256,errata_sha256,candidate_sha256,
 independent:true,candidate_producer_id,reviewer:{kind:ai|human,id,
 human_review:pending|complete},note_files:[{path,sha256,bytes}],
 expected:[{id,reference_atom_sha256,candidate_ids,checks,reason,note_anchors}],
 candidates:[{id,payload_sha256,checks,reason,note_anchors}],
 inventory:[{kind:visual|formula|unknown,id,reference_item_sha256,disposition:
 resolved|preserved_unresolved|missing,candidate_ids,checks,reason,note_anchors}]}.
Every reference atom, candidate atom, and inventory item needs exactly one row.
Fingerprints use canonical_sha256; reference atom hashes include the applied errata.
Checks have exactly CHECKS' eight keys; values are pass/fail/unknown/not_applicable.
A note anchor is {path,utf8_start,utf8_end,sha256}, a nonempty UTF-8 byte slice.
Preserved unresolved rows also require source_bindings:[{page_number,page_sha256,
localization:"page_only",issue_codes:[...]}] covering every reference physical page.
They must match the actual source-locations index and the corresponding page heading,
page SHA and original issue codes in an anchored section of 处理范围.md.
The reviewer must read source meaning and actual notes, including conditions, units,
exceptions, formula signs, symbols and details; hashes bind this declared review to
its inputs, but do not authenticate its author or prove the judgments correct.

Frozen AI reference/errata and open unknowns remain AI/pending. Metric success is
reported separately from review completeness. This program never issues a formal
scientific correctness, human-gold, or 200-atom benchmark claim.
"""

import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import sys

REFERENCE_SHA256 = "d0dfef79ad6bcb164a103db7d424991723975ae7e5e326216787e2c355903442"
ERRATA_SHA256 = "cb4dfdc1ed0aeb4476bdeafef531d47eeb3974cf3076b655d2b4fcda3fd18466"
REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REFERENCE = REPO_ROOT / ".artifacts/B19-T01/reference/frozen-reference.json"
DEFAULT_ERRATA = REPO_ROOT / ".artifacts/B19-T01/reference-errata/errata-v1.json"
CHECKS = ("claim", "conditions", "units", "symbols", "details", "support",
          "localization", "note_presence")
SEMANTIC_CHECKS = CHECKS[:5]
STATES = {"pass", "fail", "unknown", "not_applicable"}
MANDATORY = {"claim", "support", "localization", "note_presence"}
RELATIONS = {"supports", "refutes", "qualifies"}
MAX_JSON_BYTES = 64 * 1024 * 1024
MAX_NOTE_BYTES = 16 * 1024 * 1024
MAX_NOTES_BYTES = 128 * 1024 * 1024
MAX_ITEMS = 10000
SHA_PATTERN = re.compile(r"[0-9a-f]{64}\Z")


class EvaluationError(ValueError):
    """An incomplete, inconsistent, or unbound review must fail closed."""


def require(condition, message):
    if not condition:
        raise EvaluationError(message)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def canonical_sha256(value):
    """SHA256 of UTF-8 JSON, sorted keys, no spaces, original Unicode, no NaN."""
    return sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                             separators=(",", ":"), allow_nan=False).encode("utf-8"))


def _object(value, label):
    require(isinstance(value, dict), label + " must be an object")
    return value


def _array(value, label):
    require(isinstance(value, list) and len(value) <= MAX_ITEMS,
            label + " must be a bounded array")
    return value


def _text(value, label):
    require(isinstance(value, str) and bool(value.strip()), label + " must be nonempty text")
    return value


def _sha(value, label):
    require(isinstance(value, str) and SHA_PATTERN.fullmatch(value), label + " must be SHA256")
    return value


def _unique(rows, label, key="id"):
    result = {}
    for row in _array(rows, label):
        _object(row, label + " row")
        identity = _text(row.get(key), label + "." + key)
        require(identity not in result, label + " duplicate ID: " + identity)
        result[identity] = row
    return result


def _strings(value, label):
    values = _array(value, label)
    for item in values:
        _text(item, label + " item")
    require(len(set(values)) == len(values), label + " contains duplicate entries")
    return values


def _read_bytes(path, limit):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), "not a regular input file: " + str(path))
    require(path.stat().st_size <= limit, "input exceeds size limit: " + str(path))
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "input exceeds size limit: " + str(path))
    return data


def _decode_json(data, label):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, label + " duplicate JSON key: " + key)
            result[key] = value
        return result

    def constant(value):
        raise EvaluationError(label + " nonfinite JSON value: " + value)

    def floating(value):
        number = float(value)
        require(math.isfinite(number), label + " nonfinite JSON number: " + value)
        return number

    try:
        return _object(json.loads(data.decode("utf-8"), object_pairs_hook=pairs,
                                  parse_constant=constant, parse_float=floating), label)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise EvaluationError(label + " invalid UTF-8 JSON: " + str(error)) from error


def _read_json(path):
    data = _read_bytes(path, MAX_JSON_BYTES)
    return _decode_json(data, str(path)), sha256(data)


def apply_errata(reference, errata):
    """Return a copy; only the exact old claim of a unique atom can be corrected."""
    require(errata.get("schema_version") == 1 and errata.get("errata_version") == 1,
            "unsupported errata schema/version")
    require(errata.get("base_reference_sha256") == REFERENCE_SHA256,
            "errata base reference SHA mismatch")
    effective = copy.deepcopy(reference)
    atoms = _unique(effective.get("atoms"), "reference.atoms")
    seen = set()
    for entry in _array(errata.get("entries"), "errata.entries"):
        _object(entry, "errata entry")
        identity = _text(entry.get("atom_id"), "errata.atom_id")
        require(identity in atoms and identity not in seen, "bad/duplicate errata atom: " + identity)
        require(atoms[identity].get("claim") == entry.get("original_claim"),
                "errata original claim mismatch: " + identity)
        atoms[identity]["claim"] = _text(entry.get("corrected_claim"), "errata.corrected_claim")
        seen.add(identity)
    return effective


def load_reference(reference_path=DEFAULT_REFERENCE, errata_path=DEFAULT_ERRATA):
    reference, reference_hash = _read_json(reference_path)
    errata, errata_hash = _read_json(errata_path)
    require(reference_hash == REFERENCE_SHA256, "fixed reference SHA mismatch")
    require(errata_hash == ERRATA_SHA256, "fixed errata SHA mismatch")
    require(reference.get("schema_version") == 1, "unsupported reference schema")
    return apply_errata(reference, errata), errata


def _relative_path(value):
    name = _text(value, "relative note path")
    path = PurePosixPath(name)
    require(not path.is_absolute() and "\\" not in name and ":" not in name
            and ".." not in path.parts and path.as_posix() == name
            and name not in {".", ""}, "unsafe relative path: " + name)
    return name


def _note_files(notes_root):
    root = Path(notes_root)
    require(root.is_dir() and not root.is_symlink(), "notes root must be a real directory")
    result, total, entries = {}, 0, 0
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in sorted(directories + files):
            entries += 1
            require(entries <= MAX_ITEMS, "too many note directory entries")
            require(not (Path(directory) / name).is_symlink(), "symlink in notes root")
        for name in sorted(files):
            path = Path(directory) / name
            if path.suffix.lower() != ".md":
                continue
            relative = _relative_path(path.relative_to(root).as_posix())
            data = _read_bytes(path, MAX_NOTE_BYTES)
            try:
                data.decode("utf-8")
            except UnicodeDecodeError as error:
                raise EvaluationError("note is not UTF-8: " + relative) from error
            total += len(data)
            require(total <= MAX_NOTES_BYTES, "total notes exceed size limit")
            result[relative] = data
    return result


def _verify_note_inventory(review, notes):
    declared = _unique(review.get("note_files"), "review.note_files", "path")
    require(set(declared) == set(notes), "note inventory mismatch (missing, extra, or renamed file)")
    for name, data in notes.items():
        row = declared[name]
        _relative_path(name)
        require(type(row.get("bytes")) is int and row["bytes"] == len(data)
                and row.get("sha256") == sha256(data), "note fingerprint mismatch: " + name)


def _slice(data, start, end, label):
    require(type(start) is int and type(end) is int and 0 <= start < end <= len(data),
            label + " invalid UTF-8 byte interval")
    try:
        data[:start].decode("utf-8")
        selected = data[start:end]
        selected.decode("utf-8")
    except UnicodeDecodeError as error:
        raise EvaluationError(label + " splits a UTF-8 character") from error
    return selected


def _note_anchors(row, notes, allowed=None):
    paths, seen = set(), set()
    for anchor in _array(row.get("note_anchors"), "review.note_anchors"):
        _object(anchor, "note anchor")
        path = _relative_path(anchor.get("path"))
        require(path in notes and (allowed is None or path in allowed), "note anchor path mismatch: " + path)
        start, end = anchor.get("utf8_start"), anchor.get("utf8_end")
        selected = _slice(notes[path], start, end, "note anchor " + path)
        require(anchor.get("sha256") == sha256(selected), "note anchor fingerprint mismatch: " + path)
        key = (path, start, end)
        require(key not in seen, "duplicate note anchor")
        paths.add(path)
        seen.add(key)
    return paths


def _checks(row, expected=None, atom=None):
    checks = _object(row.get("checks"), "review checks")
    require(set(checks) == set(CHECKS), "review must explicitly judge all eight check dimensions")
    for key, state in checks.items():
        require(isinstance(state, str) and state in STATES, "invalid review verdict: " + key)
        required = key in MANDATORY
        if expected is not None:
            required |= key in {"conditions", "units", "symbols"} and bool(expected.get(key))
            required |= key == "symbols" and bool(expected.get("normalized_expression"))
            required |= key == "details" and bool(expected.get("exceptions") or expected.get("details")
                                                   or expected.get("semantics"))
        if atom is not None:
            measurement = atom.get("measurement")
            qualifiers = atom.get("qualifiers", [])
            required |= key == "conditions" and bool(qualifiers or (measurement or {}).get("conditions"))
            required |= key == "units" and bool((measurement or {}).get("unit"))
            required |= key == "details" and bool(qualifiers or measurement is not None
                                                   or atom.get("exceptions") or atom.get("details"))
            formula_parts = [atom.get("content", "")] + list(qualifiers)
            if measurement is not None:
                formula_parts += [measurement.get("value"), measurement.get("unit")]
                formula_parts += list(measurement.get("conditions", []))
            formula_text = " ".join(str(part or "") for part in formula_parts)
            required |= key == "symbols" and (atom.get("kind") in {"formula", "equation"}
                    or bool(re.search(r"[=<>+\-*/$\\|−±×÷∑√∞^_{}\u0370-\u03ff]", formula_text)))
        require(not (required and state == "not_applicable"), "not_applicable forbidden for " + key)
    _text(row.get("reason"), "independent review reason")
    return checks


def _ids(row, candidates):
    values = _strings(row.get("candidate_ids"), "review.candidate_ids")
    require(set(values) <= set(candidates), "review refers to unknown candidate IDs")
    return values


def _accepted(checks, keys=CHECKS):
    return all(checks[key] in {"pass", "not_applicable"} for key in keys)


def _source_mapping(reference, candidate):
    source = _object(reference.get("source"), "reference.source")
    mapping = _object(candidate.get("source_mapping"), "candidate.source_mapping")
    source_id = _text(candidate.get("source_id"), "candidate.source_id")
    require(mapping.get("reference_source_id") == source["source_id"]
            and mapping.get("actual_source_id") == source_id, "source mapping ID mismatch")
    require(candidate.get("source_sha256") == source["content_sha256"]
            and mapping.get("source_sha256") == source["content_sha256"], "source content SHA mismatch")
    count = mapping.get("physical_pages")
    require(type(count) is int and count == source["page_count"], "source total physical pages mismatch")
    ranges = reference["scope"]["sources"][0]["page_ranges"]
    require(mapping.get("page_ranges") == ranges, "source chapter page ranges mismatch")
    return source_id, source["content_sha256"], ranges


def _export_bytes(root, name):
    path = Path(root)
    for component in PurePosixPath(_relative_path(name)).parts:
        path = path / component
        require(not path.is_symlink(), "symlink in native export: " + name)
    return _read_bytes(path, MAX_JSON_BYTES)


def _native_export(reference, candidate, notes_root, notes, candidates):
    """Read the actual native IR; a caller's wrapper/counts cannot define coverage."""
    root = Path(notes_root)
    require(not (root / "compile-export.incomplete.json").exists(), "native export is incomplete")
    records, fingerprints = {}, {}
    kinds = ("atoms", "evidence", "concepts", "issues", "note-blocks", "notes", "pages", "source-locations")
    for kind in kinds:
        name = ".noetome-index/" + kind + ".ndjson"
        data = _export_bytes(root, name)
        lines = data.splitlines()
        require(len(lines) <= MAX_ITEMS, "native index has too many rows: " + kind)
        records[kind] = [_decode_json(line, name + ":" + str(number))
                         for number, line in enumerate(lines, 1)]
        fingerprints[name] = {"bytes": len(data), "sha256": sha256(data)}
    metadata = {}
    for name in ("source-index.json", "处理范围.json", "export-manifest.json"):
        data = _export_bytes(root, name)
        metadata[name] = _decode_json(data, name)
        fingerprints[name] = {"bytes": len(data), "sha256": sha256(data)}
    source_index, scope, manifest = (metadata[name] for name in
                                    ("source-index.json", "处理范围.json", "export-manifest.json"))
    summary = _object(scope.get("summary"), "native scope summary")
    run_id = _text(candidate.get("run_id"), "candidate.run_id")
    for name, item in (*metadata.items(), ("native summary", summary)):
        require(item.get("schema_version") == 1 and item.get("run_id") == run_id,
                "native export schema/run mismatch: " + name)
    require(manifest.get("status") == "complete" and manifest.get("self_included") is False,
            "native export manifest is not complete")
    require(source_index.get("mapping_directory") == ".noetome-index"
            and scope.get("manifest") == "export-manifest.json", "native export index paths mismatch")
    strategy_version = summary.get("request_strategy_version")
    require(type(strategy_version) is int and strategy_version in (2, 3, 4, 5),
            "native export must use precise request strategy 2, 3, 4 or 5")
    sources = _array(source_index.get("sources"), "native sources")
    require(len(sources) == 1 and canonical_sha256(sources) == canonical_sha256(summary.get("sources")),
            "native source inventory mismatch")
    source = _object(sources[0], "native source")
    source_id, source_sha, ranges = _source_mapping(reference, candidate)
    physical = reference["source"]["page_count"]
    selected = sum(item["end_page"] - item["start_page"] + 1 for item in ranges)
    require(source.get("source_id") == source_id and source.get("source_sha256") == source_sha
            and type(source.get("physical_pages")) is int and source["physical_pages"] == physical
            and source.get("scope_schema_version") == 1 and source.get("page_ranges") == ranges,
            "native source/scope mismatch")
    require(scope.get("scope_schema_version") == 1
            and scope.get("scope") == ("all_physical_pages" if selected == physical
                                      else "selected_physical_page_ranges"), "native scope kind mismatch")
    for key, count in (("physical_pages", physical), ("selected_pages", selected),
                       ("out_of_scope_pages", physical - selected)):
        require(type(summary.get(key)) is int and summary[key] == count, "native summary count mismatch: " + key)
    atoms = _unique(records["atoms"], "native atoms", "atom_id")
    evidence = _unique(records["evidence"], "native evidence", "evidence_id")
    concepts = _unique(records["concepts"], "native concepts", "concept_id")
    issues = _unique(records["issues"], "native issues", "issue_id")
    for kind, rows in (("atoms", atoms), ("evidence", evidence), ("concepts", concepts)):
        require(type(summary.get(kind)) is int and summary[kind] == len(rows),
                "native summary count mismatch: " + kind)
    require(set(candidates) == set(atoms), "candidate IDs do not cover actual native atom inventory")
    for field, actual, key in (("native_concepts", concepts, "concept_id"),
                               ("native_issues", issues, "issue_id")):
        declared = _unique(candidate.get(field), "candidate." + field, key)
        require(set(declared) == set(actual) and all(canonical_sha256(declared[identity])
                == canonical_sha256(value) for identity, value in actual.items()),
                "candidate full native inventory mismatch: " + field)
    pages = {}
    for page in records["pages"]:
        number = page.get("page_number")
        require(page.get("source_id") == source_id and page.get("source_sha256") == source_sha
                and type(number) is int and 1 <= number <= physical, "native page source/number mismatch")
        require(number not in pages, "duplicate native physical page")
        in_scope = any(item["start_page"] <= number <= item["end_page"] for item in ranges)
        require(type(page.get("in_scope")) is bool and page["in_scope"] == in_scope,
                "native page scope mismatch")
        if page.get("page_sha256") is not None:
            _sha(page["page_sha256"], "native physical page SHA")
        pages[number] = page
    require(len(pages) == physical, "native physical page inventory incomplete")
    for identity, item in evidence.items():
        number = item.get("page_number")
        require(item.get("source_id") == source_id and item.get("source_sha256") == source_sha
                and type(number) is int and number in pages and pages[number]["in_scope"] is True,
                "native evidence source/scope mismatch: " + identity)
        require(item.get("page_sha256") is not None
                and item["page_sha256"] == pages[number].get("page_sha256"),
                "native evidence/page fingerprint mismatch: " + identity)
    locations = {}
    for location in records["source-locations"]:
        number = location.get("page_number")
        require(location.get("schema_version") == 1 and location.get("run_id") == run_id
                and location.get("source_id") == source_id and location.get("source_sha256") == source_sha
                and type(number) is int and number in pages and pages[number]["in_scope"] is True
                and number not in locations, "native source location identity/scope mismatch")
        require(location.get("page_sha256") is not None
                and location["page_sha256"] == pages[number].get("page_sha256"),
                "native source location/page fingerprint mismatch")
        require(location.get("localization") in {"page_only", "page_with_candidate_regions"}
                and location.get("issue_region_mapping") == "unresolved"
                and location.get("note_path") == "处理范围.md" and location["note_path"] in notes,
                "native unresolved source location metadata mismatch")
        _object(location.get("geometry"), "native source geometry")
        _array(location.get("regions"), "native source regions")
        for issue in _array(location.get("source_issues"), "native original source issues"):
            _object(issue, "native original source issue")
            _text(issue.get("code"), "native original issue code")
            _text(issue.get("message"), "native original issue message")
        locations[number] = location
    note_rows = _unique(records["notes"], "native notes", "note_id")
    require(set(note_rows) == set(concepts), "native note/concept inventory mismatch")
    paths, ordinals, atom_notes, seen_blocks = {}, {}, {identity: set() for identity in atoms}, set()
    for identity, row in note_rows.items():
        name = _relative_path(row.get("path"))
        require(name in notes and name not in paths.values(), "native note path missing or duplicated")
        require(row.get("revision") == concepts[identity].get("revision")
                and row.get("title") == concepts[identity].get("canonical_name"), "native note/concept metadata mismatch")
        paths[identity], ordinals[identity] = name, set()
    for block in records["note-blocks"]:
        nid, aid, ordinal = block.get("note_id"), block.get("atom_id"), block.get("block_ordinal")
        require(isinstance(nid, str) and isinstance(aid, str) and nid in paths and aid in atoms,
                "native note block refers to unknown note/atom")
        require(type(ordinal) is int and ordinal >= 0 and ordinal not in ordinals[nid]
                and (nid, aid) not in seen_blocks, "duplicate/invalid native note block")
        require(block.get("path") == paths[nid] and block.get("atom_revision") == atoms[aid].get("revision")
                and canonical_sha256(block.get("evidence_links")) == canonical_sha256(atoms[aid].get("evidence_links")),
                "native note block path/revision/evidence mismatch")
        ordinals[nid].add(ordinal)
        seen_blocks.add((nid, aid))
        atom_notes[aid].add(paths[nid])
    for nid, row in note_rows.items():
        count = row.get("blocks")
        require(type(count) is int and count >= 0 and ordinals[nid] == set(range(count)),
                "native note block count/ordinal mismatch")
    for identity, atom in atoms.items():
        declared = candidates[identity]
        require(canonical_sha256(declared["payload"]["atom"]) == canonical_sha256(atom),
                "candidate full native atom mismatch: " + identity)
        concept_ids = _strings(atom.get("concept_ids"), "native atom concept IDs")
        require(set(concept_ids) <= set(paths) and atom_notes[identity] == {paths[cid] for cid in concept_ids},
                "native atom note/concept mapping incomplete: " + identity)
        require(set(declared["note_paths"]) == atom_notes[identity],
                "candidate note paths omit/change actual native note copies: " + identity)
        for bound in declared["payload"]["evidence"]:
            item = bound["evidence"]
            eid = item.get("evidence_id")
            require(eid in evidence and canonical_sha256(item) == canonical_sha256(evidence[eid]),
                    "candidate full native evidence mismatch: " + str(eid))
    declared_files = _unique(manifest.get("files"), "native manifest files", "path")
    scope_files = _unique(scope.get("files"), "native scope files", "path")
    require(set(scope_files) == set(declared_files) - {"处理范围.json"}
            and all(canonical_sha256(value) == canonical_sha256(declared_files[name])
                    for name, value in scope_files.items()), "native scope/manifest file inventory mismatch")
    for name, row in declared_files.items():
        _relative_path(name)
        _object(row.get("fingerprint"), "native manifest fingerprint")
        _sha(row["fingerprint"].get("sha256"), "native manifest SHA")
        require(type(row["fingerprint"].get("bytes")) is int and row["fingerprint"]["bytes"] >= 0,
                "invalid native manifest byte count")
    checked = {**fingerprints, **{name: {"bytes": len(data), "sha256": sha256(data)}
                                 for name, data in notes.items()}}
    for name, pin in checked.items():
        if name == "export-manifest.json":
            continue
        require(name in declared_files and declared_files[name]["fingerprint"] == pin,
                "native manifest fingerprint mismatch: " + name)
    return {"run_id": run_id, "atom_ids": sorted(atoms),
            "source_locations": copy.deepcopy(records["source-locations"]),
            "actual_counts": {kind: len(rows) for kind, rows in records.items()},
            "files": [{"path": name, **pin} for name, pin in sorted(fingerprints.items())]}


def _preserved_source_bindings(row, item, notes, native_export):
    """Prove page-level retention only; region/meaning resolution is never inferred."""
    pages = set(item.get("pages", []))
    if "physical_page" in item:
        pages.add(item["physical_page"])
    pages.update(support["physical_page"] for support in item.get("supports", []))
    require(bool(pages) and all(type(number) is int for number in pages),
            "preserved reference item needs explicit physical pages")
    locations = {location["page_number"]: location for location in native_export["source_locations"]}
    seen = set()
    for binding in _array(row.get("source_bindings"), "preserved source_bindings"):
        _object(binding, "preserved source binding")
        number = binding.get("page_number")
        require(type(number) is int and number in pages and number in locations and number not in seen,
                "preserved source binding physical page mismatch")
        seen.add(number)
        location = locations[number]
        require(binding.get("page_sha256") == location["page_sha256"]
                and binding.get("localization") == "page_only", "preserved source binding page fingerprint/localization mismatch")
        codes = _strings(binding.get("issue_codes"), "preserved original issue codes")
        actual_codes = {issue["code"] for issue in location["source_issues"]}
        require(bool(codes) and set(codes) <= actual_codes, "preserved source binding needs original source issues")
        anchored = False
        for anchor in row["note_anchors"]:
            if anchor["path"] != location["note_path"]:
                continue
            data = _slice(notes[anchor["path"]], anchor["utf8_start"], anchor["utf8_end"], "preserved source anchor")
            text = re.sub(r"\\([!-/:-@\[-\x60{-~])", r"\1", data.decode("utf-8"))
            text = text.replace("\r\n", "\n").replace("\r", "\n")
            heading = re.search(r"(?m)^#{1,6} +物理页 +" + str(number) + r"[ \t]*$", text)
            if heading:
                section = re.split(r"(?m)^#{1,6} +", text[heading.end():], maxsplit=1)[0]
                anchored |= location["page_sha256"] in section and all(code in section for code in codes)
        require(anchored, "preserved source needs a page heading, actual page SHA and original issue code in its note anchor")
    require(seen == pages, "preserved source bindings do not cover all reference physical pages")


def _verify_export_unchanged(notes_root, snapshot):
    for pin in snapshot["files"]:
        data = _export_bytes(notes_root, pin["path"])
        require(len(data) == pin["bytes"] and sha256(data) == pin["sha256"],
                "native export changed during evaluation: " + pin["path"])


def _reference_pages(reference):
    pages = {}
    for collection in ("atoms", "visuals", "formulas"):
        for item in reference[collection]:
            for support in item.get("supports", []):
                page = support["physical_page"]
                pin = (support["page_text_file"], support["page_text_sha256"])
                require(page not in pages or pages[page] == pin, "inconsistent reference page pins")
                pages[page] = pin
    return pages


def _candidate_data(reference, candidate, notes, reference_root):
    source_id, source_sha, ranges = _source_mapping(reference, candidate)
    candidates = _unique(candidate.get("atoms"), "candidate.atoms")
    page_pins, page_bytes, evidence_by_id, mechanical = _reference_pages(reference), {}, {}, {}
    for identity, row in candidates.items():
        payload = _object(row.get("payload"), "candidate.payload")
        atom = _object(payload.get("atom"), "candidate.payload.atom")
        require(atom.get("atom_id") == identity, "candidate/native atom ID mismatch: " + identity)
        _text(atom.get("content"), "native atom content")
        _array(atom.get("qualifiers"), "native atom qualifiers")
        require("measurement" in atom, "native atom measurement must be retained (or null)")
        if atom["measurement"] is not None:
            measurement = _object(atom["measurement"], "native atom measurement")
            require(set(measurement) >= {"value_status", "value", "unit", "conditions"}
                    and measurement["value_status"] in {"known", "unknown", "not_applicable"}
                    and (measurement["unit"] is None or isinstance(measurement["unit"], str))
                    and (measurement["value"] is None or isinstance(measurement["value"], str)),
                    "native measurement fields must be complete")
            if measurement["value_status"] == "known":
                _text(measurement["value"], "native known measurement value")
                _text(measurement["unit"], "native known measurement unit")
            elif measurement["value_status"] == "unknown":
                require(measurement["value"] is None, "native unknown measurement value must be null")
            else:
                require(measurement["value"] is None and measurement["unit"] is None,
                        "native not_applicable measurement value/unit must be null")
            if measurement["unit"] is not None:
                _text(measurement["unit"], "native measurement unit")
            for condition in _array(measurement["conditions"], "native measurement conditions"):
                _text(condition, "native measurement condition")
        note_paths = _strings(row.get("note_paths"), "candidate.note_paths")
        for path in note_paths:
            require(_relative_path(path) in notes, "candidate note path missing from actual notes: " + path)
        links = []
        for link in _array(atom.get("evidence_links"), "native evidence_links"):
            _object(link, "native evidence link")
            eid = _text(link.get("evidence_id"), "native evidence_id")
            relation = link.get("relation")
            require(isinstance(relation, str) and relation in RELATIONS, "invalid native evidence relation")
            links.append((eid, relation))
        require(len(links) == len(set(links)), "duplicate native evidence link")
        actual_links, errors = [], []
        for bound in _array(payload.get("evidence"), "candidate.payload.evidence"):
            _object(bound, "candidate evidence binding")
            evidence = _object(bound.get("evidence"), "candidate native evidence")
            eid = _text(evidence.get("evidence_id"), "evidence ID")
            relation = bound.get("relation")
            require(isinstance(relation, str) and relation in RELATIONS, "invalid evidence binding relation")
            actual_links.append((eid, relation))
            digest = canonical_sha256(evidence)
            require(eid not in evidence_by_id or evidence_by_id[eid] == digest,
                    "one evidence ID has conflicting native payloads: " + eid)
            evidence_by_id[eid] = digest
            require(evidence.get("source_id") == source_id
                    and evidence.get("source_sha256") == source_sha, "native evidence source mismatch: " + eid)
            _sha(evidence.get("page_sha256"), "native page SHA")
            page = evidence.get("page_number")
            require(type(page) is int and any(r["start_page"] <= page <= r["end_page"] for r in ranges),
                    "native evidence outside fixed physical page scope: " + eid)
            require(page in page_pins, "reference has no text pin for physical page " + str(page))
            if page not in page_bytes:
                name, digest = page_pins[page]
                require(_relative_path(name) == name, "unsafe reference text path")
                data = _read_bytes(Path(reference_root) / name, MAX_NOTE_BYTES)
                require(sha256(data) == digest, "frozen reference page text SHA mismatch: " + name)
                page_bytes[page] = data
            try:
                quote = _text(evidence.get("quote"), "native evidence quote").encode("utf-8")
                selected = _slice(page_bytes[page], evidence.get("start_byte"), evidence.get("end_byte"), eid)
                require(selected == quote and evidence.get("excerpt_sha256") == sha256(selected),
                        "native evidence quote/offset/SHA mismatch: " + eid)
            except EvaluationError as error:
                errors.append(str(error))
        require(len(actual_links) == len(set(actual_links)) and set(actual_links) == set(links),
                "candidate evidence IDs/relations do not match full native atom: " + identity)
        mechanical[identity] = {"evidence_valid": bool(links) and not errors,
                                "supporting_evidence_valid": any(relation == "supports" for _, relation in links)
                                and not errors, "errors": errors, "evidence_count": len(links)}
    return candidates, mechanical


def _review_metadata(candidate, review):
    require(candidate.get("schema_version") == 1 and review.get("schema_version") == 1,
            "unsupported candidate/review schema")
    producer = _object(candidate.get("producer"), "candidate.producer")
    producer_id = _text(producer.get("id"), "candidate.producer.id")
    _text(producer.get("kind"), "candidate.producer.kind")
    reviewer = _object(review.get("reviewer"), "review.reviewer")
    reviewer_id = _text(reviewer.get("id"), "reviewer.id")
    require(reviewer.get("kind") in {"ai", "human"}
            and reviewer.get("human_review") in {"pending", "complete"}, "invalid reviewer metadata")
    require(reviewer.get("kind") != "ai" or reviewer["human_review"] == "pending",
            "AI reviewer cannot claim completed human review")
    require(review.get("independent") is True and review.get("candidate_producer_id") == producer_id
            and reviewer_id.strip().casefold() != producer_id.strip().casefold(),
            "independent reviewer must have a distinct declared identity")
    return reviewer


def _metric(passed, all_ids, unknown=()):
    denominator = len(all_ids)
    return {"numerator": len(passed), "denominator": denominator,
            "rate": len(passed) / denominator if denominator else None,
            "failed_ids": sorted(set(all_ids) - set(passed)), "unknown_ids": sorted(unknown)}


def _evaluate(reference, errata, candidate, review, notes_root, reference_root, hashes):
    """Internal scoring core; the public entry point enforces both fixed file SHAs."""
    reviewer = _review_metadata(candidate, review)
    for key in ("reference_sha256", "errata_sha256", "candidate_sha256"):
        require(review.get(key) == hashes[key], "review input fingerprint mismatch: " + key)
    notes = _note_files(notes_root)
    _verify_note_inventory(review, notes)
    expected = _unique(reference.get("atoms"), "reference.atoms")
    candidates, mechanical = _candidate_data(reference, candidate, notes, reference_root)
    expected_rows = _unique(review.get("expected"), "review.expected")
    candidate_rows = _unique(review.get("candidates"), "review.candidates")
    require(set(expected_rows) == set(expected), "expected review IDs incomplete or unknown")
    require(set(candidate_rows) == set(candidates), "candidate review IDs incomplete or unknown")
    candidate_checks, expected_checks, mappings = {}, {}, {}
    for identity, row in candidate_rows.items():
        require(row.get("payload_sha256") == canonical_sha256(candidates[identity]["payload"]),
                "candidate payload fingerprint mismatch: " + identity)
        checks = _checks(row, atom=candidates[identity]["payload"]["atom"])
        paths = _note_anchors(row, notes, set(candidates[identity]["note_paths"]))
        if checks["note_presence"] == "pass":
            require(bool(paths) and paths == set(candidates[identity]["note_paths"]),
                    "passing candidate note review needs anchors in every actual note: " + identity)
        candidate_checks[identity] = checks
    native_export = _native_export(reference, candidate, notes_root, notes, candidates)
    candidate_ids = set(native_export["atom_ids"])
    for identity, row in expected_rows.items():
        require(row.get("reference_atom_sha256") == canonical_sha256(expected[identity]),
                "effective reference atom fingerprint mismatch: " + identity)
        checks = _checks(row, expected[identity])
        ids = _ids(row, candidates)
        require(bool(ids) or checks["claim"] != "pass", "passing expected review has no candidate mapping")
        allowed = {path for cid in ids for path in candidates[cid]["note_paths"]}
        paths = _note_anchors(row, notes, allowed)
        if checks["note_presence"] == "pass":
            require(bool(paths), "passing expected note review requires an actual note anchor: " + identity)
        expected_checks[identity], mappings[identity] = checks, ids
    inventory_refs = {(kind, item["id"]): item for kind, collection in
                      (("visual", "visuals"), ("formula", "formulas"), ("unknown", "unknowns"))
                      for item in reference[collection]}
    inventory_rows = {}
    for row in _array(review.get("inventory"), "review.inventory"):
        _object(row, "inventory review row")
        kind, identity = row.get("kind"), row.get("id")
        require(isinstance(kind, str) and isinstance(identity, str), "invalid inventory identity")
        key = (kind, identity)
        require(key in inventory_refs and key not in inventory_rows, "bad/duplicate inventory review ID")
        inventory_rows[key] = row
    require(set(inventory_rows) == set(inventory_refs), "inventory review IDs incomplete")
    # A reference-side condition/unit/sign failure cannot be hidden by candidate-side self-rating.
    mapped_checks = {identity: [] for identity in candidates}
    for identity, ids in mappings.items():
        for cid in ids:
            mapped_checks[cid].append(expected_checks[identity])
    for key, row in inventory_rows.items():
        checks = _checks(row, inventory_refs[key])
        for cid in _ids(row, candidates):
            mapped_checks[cid].append(checks)
    def uncertain(cid, keys):
        return any(ratings[key] == "unknown"
                   for ratings in [candidate_checks[cid]] + mapped_checks[cid] for key in keys)

    correct, supported, localized, present = set(), set(), set(), set()
    dimension_metrics = {}
    for dimension in SEMANTIC_CHECKS:
        passed = {cid for cid, checks in candidate_checks.items()
                  if _accepted(checks, (dimension,))
                  and all(_accepted(other, (dimension,)) for other in mapped_checks[cid])}
        unknown = {cid for cid, checks in candidate_checks.items() if checks[dimension] == "unknown"
                   or any(other[dimension] == "unknown" for other in mapped_checks[cid])}
        dimension_metrics[dimension] = _metric(passed, candidate_ids, unknown)
    for cid, checks in candidate_checks.items():
        if checks["note_presence"] == "pass" and all(
                other["note_presence"] == "pass" for other in mapped_checks[cid]):
            present.add(cid)
        semantic_ok = _accepted(checks, SEMANTIC_CHECKS) and all(
            _accepted(other, SEMANTIC_CHECKS) for other in mapped_checks[cid])
        if semantic_ok and cid in present:
            correct.add(cid)
        if (cid in correct and checks["support"] == "pass" and mechanical[cid]["supporting_evidence_valid"]
                and all(other["support"] == "pass" for other in mapped_checks[cid])):
            supported.add(cid)
        if (checks["localization"] == "pass" and mechanical[cid]["evidence_valid"]
                and all(other["localization"] == "pass" for other in mapped_checks[cid])):
            localized.add(cid)
    covered = {identity for identity, checks in expected_checks.items()
               if _accepted(checks) and mappings[identity]
               and all(cid in correct & supported & localized & present for cid in mappings[identity])}
    critical = {key for key, atom in expected.items() if atom.get("priority") == "critical"}
    necessary = {key for key, atom in expected.items() if atom.get("priority") == "necessary"}
    require(critical | necessary == set(expected), "unknown reference atom priority")
    inventory_summary, unresolved = {}, []
    for kind in ("visual", "formula", "unknown"):
        ids, resolved, retained, required_ids = set(), set(), set(), set()
        for (item_kind, identity), item in inventory_refs.items():
            if item_kind != kind:
                continue
            row = inventory_rows[(kind, identity)]
            require(row.get("reference_item_sha256") == canonical_sha256(item),
                    "inventory reference fingerprint mismatch: " + identity)
            checks = _checks(row, item)
            mapped = _ids(row, candidates)
            paths = _note_anchors(row, notes)
            disposition = row.get("disposition")
            require(disposition in {"resolved", "preserved_unresolved", "missing"}, "bad inventory disposition")
            require(not (kind == "unknown" and disposition == "resolved"),
                    "frozen open unknown cannot be promoted to resolved: " + identity)
            ids.add(identity)
            if item.get("required_for_scope", True):
                required_ids.add(identity)
            if disposition == "resolved":
                mapped_paths = {path for cid in mapped for path in candidates[cid]["note_paths"]}
                require(paths <= mapped_paths, "resolved inventory anchors must be in mapped candidate notes")
                require(bool(mapped) and bool(paths) and _accepted(checks),
                        "resolved inventory needs complete independent checks, candidates and notes")
                if all(cid in correct & supported & localized & present for cid in mapped):
                    resolved.add(identity)
            elif disposition == "preserved_unresolved":
                require(bool(paths) and checks["note_presence"] == "pass",
                        "preserved unresolved inventory must actually appear in notes")
                _preserved_source_bindings(row, item, notes, native_export)
                retained.add(identity)
            if identity not in resolved:
                unresolved.append({"kind": kind, "id": identity, "disposition": disposition,
                                   "required": identity in required_ids, "reason": row["reason"],
                                   "reference_status": item.get("status", "unknown")})
        inventory_summary[kind] = {**_metric(resolved, ids), "preserved_unresolved_ids": sorted(retained),
                                   "required_denominator": len(required_ids),
                                   "required_resolved_numerator": len(resolved & required_ids)}
    unknown_expected = {identity for identity, checks in expected_checks.items()
                        if "unknown" in checks.values()
                        or any(uncertain(cid, CHECKS) for cid in mappings[identity])}
    metrics = {"recall": _metric(covered, expected, unknown_expected),
               "critical_recall": _metric(covered & critical, critical, unknown_expected & critical),
               "necessary_recall": _metric(covered & necessary, necessary, unknown_expected & necessary),
               "accuracy": _metric(correct, candidate_ids, {cid for cid in candidates
                   if uncertain(cid, SEMANTIC_CHECKS + ("note_presence",))}),
               "evidence_support": _metric(supported, candidate_ids, {cid for cid in candidates
                   if uncertain(cid, SEMANTIC_CHECKS + ("support", "note_presence"))}),
               "localization": _metric(localized, candidate_ids, {cid for cid in candidates
                   if uncertain(cid, ("localization",))}),
               "note_presence": _metric(present, candidate_ids, {cid for cid in candidates
                   if uncertain(cid, ("note_presence",))})}
    metrics.update({key + "_fidelity": value for key, value in dimension_metrics.items()})
    metric_gates = {"recall_at_least_98_percent": metrics["recall"]["rate"] is not None
                    and 100 * len(covered) >= 98 * len(expected),
                    "critical_recall_100_percent": bool(critical) and covered >= critical,
                    "accuracy_at_least_98_percent": bool(candidates) and 100 * len(correct) >= 98 * len(candidate_ids),
                    "evidence_support_100_percent": bool(candidates) and len(supported) == len(candidate_ids),
                    "localization_100_percent": bool(candidates) and len(localized) == len(candidate_ids),
                    "note_presence_100_percent": bool(candidates) and len(present) == len(candidate_ids)}
    gates = {**metric_gates,
             "required_inventory_resolved": not any(item["required"] for item in unresolved),
             "reference_human_review_complete": reference.get("annotator") == "human"
             and reference.get("human_review") == "complete",
             "errata_human_review_complete": errata.get("annotator") == "human"
             and errata.get("human_review") == "complete",
             "independent_human_review_complete": reviewer["kind"] == "human"
             and reviewer["human_review"] == "complete"}
    gaps = [{"id": identity, "candidate_ids": mappings[identity], "checks": expected_checks[identity],
             "reason": expected_rows[identity]["reason"]} for identity in sorted(necessary - covered)]
    _verify_export_unchanged(notes_root, native_export)
    return {"schema_version": 1, "status": "evaluated", "input_fingerprints": hashes,
            "native_export": native_export,
            "reference_id": reference["reference_id"], "reference_annotator": reference.get("annotator"),
            "reference_human_review": reference.get("human_review"),
            "errata": {"version": errata["errata_version"], "annotator": errata.get("annotator"),
                       "human_review": errata.get("human_review"), "applied_atom_ids": [
                           entry["atom_id"] for entry in errata["entries"]]},
            "reviewer": copy.deepcopy(reviewer), "producer": copy.deepcopy(candidate["producer"]),
            "independence": "declared distinct reviewer identity; author identity is not authenticated",
            "source_mapping": copy.deepcopy(candidate["source_mapping"]), "metrics": metrics,
            "expected_results": [{"id": identity, "covered": identity in covered,
                                  "review": copy.deepcopy(row)} for identity, row in sorted(expected_rows.items())],
            "candidate_results": [{"id": identity, "correct": identity in correct,
                                   "supported": identity in supported, "localized": identity in localized,
                                   "note_present": identity in present, "review": copy.deepcopy(row)}
                                  for identity, row in sorted(candidate_rows.items())],
             "inventory": inventory_summary,
             "inventory_results": [{"kind": kind, "id": identity, "review": copy.deepcopy(row)}
                                   for (kind, identity), row in sorted(inventory_rows.items())],
             "unresolved_inventory": unresolved, "necessary_gaps": gaps,
            "mechanical_evidence_failures": {key: value["errors"] for key, value in mechanical.items()
                                              if value["errors"]},
            "metric_gates": metric_gates, "metric_gate_passed": all(metric_gates.values()),
            "gates": gates, "gate_passed": all(gates.values()), "scientific_pass_claim": False,
            "human_gold_claim": False, "formal_200_atom_benchmark_claim": False,
            "limitations": ["Independent semantic verdicts are supplied by the named reviewer, not inferred by this tool.",
                            "Frozen AI/pending reference and errata are not expert scientific ground truth.",
                            "Preserved unresolved visuals/formulas/unknowns are not counted as resolved coverage."],
            "reviewed_note_files": [{"path": path, "bytes": len(data), "sha256": sha256(data)}
                                    for path, data in sorted(notes.items())]}


def evaluate_chapter(candidate_path, notes_root, review_path, reference_path=DEFAULT_REFERENCE,
                     errata_path=DEFAULT_ERRATA):
    reference, errata = load_reference(reference_path, errata_path)
    candidate, candidate_hash = _read_json(candidate_path)
    review, review_hash = _read_json(review_path)
    hashes = {"reference_sha256": REFERENCE_SHA256, "errata_sha256": ERRATA_SHA256,
              "candidate_sha256": candidate_hash, "review_sha256": review_hash}
    report = _evaluate(reference, errata, candidate, review, notes_root, Path(reference_path).parent, hashes)
    # Catch ordinary input drift during evaluation; immutable identities are pinned again.
    for path, digest in ((reference_path, REFERENCE_SHA256), (errata_path, ERRATA_SHA256),
                         (candidate_path, candidate_hash), (review_path, review_hash)):
        require(sha256(_read_bytes(path, MAX_JSON_BYTES)) == digest, "input changed during evaluation")
    require(report["reviewed_note_files"] == [
        {"path": name, "bytes": len(data), "sha256": sha256(data)}
        for name, data in sorted(_note_files(notes_root).items())], "notes changed during evaluation")
    _verify_export_unchanged(notes_root, report["native_export"])
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=DEFAULT_REFERENCE)
    parser.add_argument("--errata", type=Path, default=DEFAULT_ERRATA)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--notes-root", type=Path, required=True)
    parser.add_argument("--review", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    protected = {path.resolve() for path in (args.reference, args.errata, args.candidate, args.review)}
    try:
        require(args.output.resolve() not in protected and args.output.suffix.lower() == ".json",
                "report output must be a separate JSON file")
        require(not args.output.is_symlink(), "report output cannot be a symlink")
        report = evaluate_chapter(args.candidate, args.notes_root, args.review, args.reference, args.errata)
        code = 0 if report["gate_passed"] else 1
    except (EvaluationError, OSError, ValueError, KeyError, TypeError, RecursionError) as error:
        report = {"schema_version": 1, "status": "invalid_input", "gate_passed": False,
                  "scientific_pass_claim": False, "human_gold_claim": False,
                  "formal_200_atom_benchmark_claim": False, "errors": [str(error)]}
        code = 2
    if args.output.resolve() in protected or args.output.is_symlink() or args.output.suffix.lower() != ".json":
        print(json.dumps(report, ensure_ascii=False), file=sys.stderr)
        return 2
    try:
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    except OSError as error:
        print("cannot write evaluation report: " + str(error), file=sys.stderr)
        return 2
    print(json.dumps({"status": report["status"], "gate_passed": report["gate_passed"],
                      "scientific_pass_claim": False, "report": str(args.output)}, ensure_ascii=False))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
