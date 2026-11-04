"""Actual CLI preparation from frozen v3 authors; no coordination-derived gold."""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time
import uuid

from verify_knowledge_coordination import (Audit, REPO, FILES, actual_source,
    strict_json, fingerprint, encoded, exact, inventory, compact, normalized)


def markdown_preamble(report):
    t = report["totals"]
    summary = (f"条件已列明：{t['known_condition_occurrences']}；条件未知：{t['unknown_condition_occurrences']}；条件不适用：{t['not_applicable_condition_occurrences']}；"
               f"时间已知：{t['dated_occurrences']}；时间未知：{t['undated_occurrences']}；条件并列关系：{t['condition_variants']}；历史关系：{t['historical_relations']}；"
               f"真实冲突关系：{t['true_conflicts']}；证据不足关系：{t['insufficient_evidence_relations']}；反证链接：{t['counter_evidence_links']}。完整来源、原对象、证据与分母与 coordination.json 一致。")
    return "# 有界多来源知识协调\n\n状态：" + report["status"] + "；知识状态：" + report["knowledge_status"] + "；发布：" + str(report["published"]).lower() + "。\n\n科学质量：" + report["scientific_quality"] + "；研究：not_requested；网络与付费调用：0。\n\n" + summary + "\n\n"


def rewrite_output(output, report):
    """Synchronize every result representation before a semantic forgery check."""
    # Recompute forged stable IDs from their own displayed data, so rejection
    # cannot be explained solely by leaving an obsolete fact identifier.
    for f in report["facts"]:
        key = ["coordination_v3", f["object_id"], f["property_id"], normalized(f["value"]), normalized(f["unit"]),
               f["applicability_status"], sorted(map(normalized, f["conditions"])), sorted(map(normalized, f["exceptions"])),
               [f["time"].get(k) for k in ("status", "precision", "value")],
               f["occurrence_ids"][0] if f["applicability_status"] == "unknown" else None]
        f["fact_id"] = "fact-v3-" + hashlib.sha256(compact(key)).hexdigest()
    report["facts"].sort(key=lambda f: f["fact_id"])
    ir = output / "coordination-ir.json"
    if report["status"] == "passed":
        if "coordination-ir.json" not in report["artifacts"]:
            report["artifacts"]["coordination-ir.json"] = {}
        body = {k: v for k, v in report.items() if k not in
                ("report_kind", "status", "network_calls", "paid_calls", "artifacts")}
        ir.write_bytes(encoded(dict(body, revision=1)))
    elif ir.exists():
        ir.unlink()
        report["artifacts"].pop("coordination-ir.json", None)
    report["artifacts"] = {name: fingerprint((output / name).read_bytes())
                           for name in report["artifacts"]}
    (output / "coordination.json").write_bytes(encoded(report))
    md = output / "coordination.md"
    md.write_bytes(markdown_preamble(report).encode() + b"```json\n" +
                   encoded(report).rstrip() + b"\n```\n")


def forged_case(folder, destination, field, value):
    """Synchronize modified author declaration and its saved original copy."""
    case, output = destination / "case", destination / "output"
    destination.mkdir()
    shutil.copytree(folder / "case", case); shutil.copytree(folder / "output", output)
    identity = strict_json((case / "identity.json").read_bytes())
    identity["atom_contexts"][0][field] = value
    raw = encoded(identity)
    (case / "identity.json").write_bytes(raw); (output / "identity-original.json").write_bytes(raw)
    rewrite_output(output, strict_json((output / "coordination.json").read_bytes()))
    return case, output


class ConflictsAudit(Audit):
    def __init__(self, cli, root):
        self.cli, self.root = Path(cli).resolve(), Path(root).resolve()
        self.root.mkdir()
        self.calls, self.rows, self.protected = [], [], {}
        self.sentinel = self.root.parent / ("variants-sentinel-" + uuid.uuid4().hex + ".txt")
        self.sentinel.write_bytes(b"external human sentinel\n")
        self.protect(self.sentinel)
        fixture = REPO / "fixtures/knowledge-coordination-v3"
        self.authors = {p.name: p.read_bytes() for p in fixture.iterdir() if p.is_file()}
        frozen = strict_json(self.authors["frozen-sha256.json"])
        # The freeze manifest uses paths relative to the repository.
        for name, pin in frozen["files"].items():
            exact(fingerprint((fixture / name).read_bytes()), pin, "frozen author")
        self.author_dir = self.root / "authors"
        self.author_dir.mkdir()
        for name, raw in self.authors.items():
            (self.author_dir / name).write_bytes(raw)
        self.protect(fixture); self.protect(self.author_dir)
        self.cli_fp = fingerprint(self.cli.read_bytes())
        self.write_new(self.root / "author-pins.json", {k: fingerprint(v) for k, v in self.authors.items()})
        self.source_paths = [*REPO.glob("scripts/*.py"), *REPO.glob("crates/**/*.rs"), REPO / "Cargo.lock", REPO / "Cargo.toml"]
        self.source_pins = {str(p.relative_to(REPO)): fingerprint(p.read_bytes()) for p in self.source_paths}
        self.write_new(self.root / "source-pins.json", self.source_pins)
        self.protect(REPO / "fixtures/extraction-review-v1/candidates.json")
        self.flush()

    def call(self, name, args, expected=0):
        index = len(self.calls) + 1
        stem = self.root / (f"command-{index:03d}-" + name)
        argv = [str(self.cli), *map(str, args)]
        before = fingerprint(self.cli.read_bytes())
        started = time.monotonic()
        error = None
        try:
            run = subprocess.run(argv, capture_output=True, timeout=40, cwd=REPO)
            stdout, stderr, code = run.stdout, run.stderr, run.returncode
        except subprocess.TimeoutExpired as exc:
            stdout, stderr, code = exc.stdout or b"", exc.stderr or b"", None
            error = repr(exc)
        out, err = stem.with_suffix(".stdout.txt"), stem.with_suffix(".stderr.txt")
        out.write_bytes(stdout); err.write_bytes(stderr)
        receipt = dict(index=index, name=name, argv=argv, cwd=str(REPO), exit_code=code, expected_exit=expected,
            elapsed_seconds=time.monotonic()-started, stdout=dict(path=str(out), **fingerprint(stdout)),
            stderr=dict(path=str(err), **fingerprint(stderr)), cli_before=before,
            cli_after=fingerprint(self.cli.read_bytes()), error=error)
        self.write_new(stem.with_suffix(".json"), receipt); self.calls.append(receipt); self.flush(error=error)
        exact(receipt["cli_after"], before, "per-command CLI unchanged")
        assert code == expected, f"CLI {name}: expected {expected}, observed {code}; see {stem}"
        return run

    def prepare(self, name, author_name=None, *, multiple=False, reorder=False, shared_atom=False, dual_relation=False, qualifier=False):
        author = copy.deepcopy(next(c for c in strict_json(self.authors["authors.json"])["cases"]
                                    if c["name"] == (author_name or name)))
        folder = self.root / name
        folder.mkdir(); frozen = folder / "frozen-authors"; frozen.mkdir()
        expected = author["expected"]
        if shared_atom:
            # Original atom IDs belong to each input namespace. Freeze an
            # explicit author override before constructing candidates or CLI IO.
            atom_id = author["inputs"][0]["original_atom_id"]
            expected["case_name"] = name
            for item in author["inputs"]:
                item["original_atom_id"] = atom_id
                item["context"]["original_atom_id"] = atom_id
            for key in ("atom_mappings", "atom_contexts"):
                for item in author["identity"][key]:
                    item["original_atom_id"] = atom_id
            for item in expected["occurrences"]:
                item["original_atom_id"] = atom_id
            for fact in expected["facts"]:
                fact["original_atom_ids"] = [atom_id for _ in fact["input_ids"]]
        if multiple:
            expected["totals"]["evidence_links"] += 1
            expected["totals"]["context_evidence_links"] += 1
            author["identity"]["atom_contexts"][0]["context_evidence_ids"].append("a-support-second")
        if qualifier:
            expected["totals"]["evidence_links"] += 1
        if dual_relation:
            expected["totals"]["evidence_links"] += 1
            expected["totals"]["counter_evidence_links"] += 1
        if reorder:
            for key in ("concepts", "concept_mappings", "atom_mappings", "atom_contexts"):
                author["identity"][key].reverse()
            for c in author["identity"]["concepts"]:
                c["display_name"] += "（显示变体）"
        expected_raw = encoded(expected)
        (folder / "gold.json").write_bytes(expected_raw)
        self.write_new(folder / "expected-pin.json", fingerprint(expected_raw))
        self.write_new(frozen / "author.json", author)
        for item in author["inputs"]:
            (frozen / item["source_file"]).write_bytes(self.authors[item["source_file"]])
            (frozen / (item["input_id"] + "-gold.json")).write_bytes(encoded(item["gold"]))
        self.protect(frozen); self.protect(folder / "gold.json")
        project, case = folder / "project", folder / "case"
        self.call(name + "-init", ["project", "init", project])
        case.mkdir(); self.write_new(case / "identity.json", author["identity"])
        inputs = []
        for item in author["inputs"]:
            ident, g = item["input_id"], item["gold"]
            args = ["source", "import", project, frozen / item["source_file"]]
            if inputs:
                if author["same_source"]:
                    source_id = inputs[0]["binding"]["source_id"]
                else:
                    seed = folder / (ident + "-seed.txt")
                    seed.write_bytes(("independent registration " + uuid.uuid4().hex).encode())
                    source_id = strict_json(self.call(name + "-seed-" + ident, ["source", "import", project, seed]).stdout)["version"]["source_id"]
                args += ["--source-id", source_id]
            imp = strict_json(self.call(name + "-import-" + ident, args).stdout)
            selection = dict(kind="text", ordinal=0)
            source = actual_source(project, imp["import_id"], selection)
            b, u = source["binding"], source["units"][0]
            review = folder / ("review-" + ident); review.mkdir()
            gold_raw = encoded(g); (review / "gold.json").write_bytes(gold_raw)
            candidate = strict_json((REPO / "fixtures/extraction-review-v1/candidates.json").read_bytes())
            candidate.update(binding=b, request_id="synthetic-conflicts-" + ident)
            payload = candidate["payload"]
            payload.update(binding=b, request_id=candidate["request_id"], concepts=[
                dict(concept_id=ident + "-subject", revision=1, canonical_name=item["subject"], aliases=[], needs_full_explanation=True),
                dict(concept_id=ident + "-property", revision=1, canonical_name=item["property"], aliases=[], needs_full_explanation=True)])
            quote = item["context_quote"].encode(); start = u["start_byte"] + u["text"].encode().index(quote)
            ev = dict(evidence_id=ident + "-support", unit_id=u["unit_id"], excerpt_start_byte=start,
                      excerpt_end_byte=start + len(quote), excerpt_sha256=hashlib.sha256(quote).hexdigest(), quote=item["context_quote"])
            payload["evidence"] = [ev]
            if multiple and ident == "a":
                payload["evidence"].insert(0, dict(ev, evidence_id="a-support-second"))
            atom = payload["atoms"][0]
            atom["atom"].update(atom_id=item["original_atom_id"], concept_ids=[ident + "-subject", ident + "-property"],
                content=g["atom_content"], qualifiers=[g["unit"], *g["conditions"], *g["exceptions"]],
                evidence_links=[dict(evidence_id=e["evidence_id"], relation="supports") for e in payload["evidence"]])
            if qualifier and ident == "a":
                atom["atom"]["evidence_links"].append(dict(evidence_id=ident + "-support", relation="qualifies"))
            if dual_relation and ident == "a":
                atom["atom"]["evidence_links"].append(dict(evidence_id=ident + "-support", relation="refutes"))
            for counter in item.get("counter_evidence", []):
                counter_quote = counter["quote"].encode()
                counter_start = u["start_byte"] + u["text"].encode().index(counter_quote)
                payload["evidence"].append(dict(evidence_id=counter["evidence_id"], unit_id=u["unit_id"],
                    excerpt_start_byte=counter_start, excerpt_end_byte=counter_start + len(counter_quote),
                    excerpt_sha256=hashlib.sha256(counter_quote).hexdigest(), quote=counter["quote"]))
                atom["atom"]["evidence_links"].append(dict(evidence_id=counter["evidence_id"], relation="refutes"))
            atom["measurement"] = dict(value_status="known", value=g["value"], unit=g["unit"], conditions=g["conditions"])
            self.write_new(review / "candidates.json", candidate)
            self.write_new(review / "decision.json", dict(schema_version=1, unit_id=u["unit_id"], disposition="included", atom_ids=[item["original_atom_id"]], exclusion_reason=None))
            self.write_new(review / "review.json", dict(schema_version=1, purpose="single_measurement_source_review", candidate_file="candidates.json", gold_file="gold.json", decision_file="decision.json", gold_sha256=fingerprint(gold_raw)["sha256"], subject=item["subject"], property=item["property"], allowed_contexts=[], selection=[selection], reviewer="automated_source_review", interpretation_timing="before_synthetic_response"))
            bundle = case / ("bundle-" + ident)
            self.call(name + "-review-" + ident, ["extract", "verify", project, imp["import_id"], review, bundle])
            inputs.append(dict(input_id=ident, review_bundle=bundle.name, binding=b, artifacts={f: fingerprint((bundle / f).read_bytes()) for f in FILES}))
        if reorder:
            inputs.reverse()
        self.write_new(case / "coordination.json", dict(schema_version=3, purpose="explicit_identity_conflict_review", project_id=inputs[0]["binding"]["project_id"], identity_file="identity.json", inputs=inputs))
        self.protect(project); self.protect(case)
        return folder, project, case, expected_raw

    def coordinate(self, name, project, case, gold, output):
        from verify_knowledge_coordination import verify_report
        expected = strict_json(gold)
        run = self.call(name, ["knowledge", "coordinate", "--", project, case, output], 0 if expected["expected_status"] == "passed" else 3)
        self.check_protected()
        checked = verify_report(project, case, output, gold)
        exact(strict_json(run.stdout), strict_json((output / "coordination.json").read_bytes()), "stdout full report")
        self.rows.append(dict(name=name, checked=checked, expected_fingerprint=fingerprint(gold)))
        self.flush()
        return checked

    def matrix(self):
        from verify_knowledge_coordination import verify_report
        for author in strict_json(self.authors["authors.json"])["cases"]:
            name = author["name"]
            folder, project, case, gold = self.prepare(name)
            self.coordinate(name, project, case, gold, folder / "output")
        for name, opts in (("multiple-supports", dict(multiple=True)), ("ordering-display", dict(reorder=True)), ("dual-relation", dict(dual_relation=True))):
            folder, project, case, gold = self.prepare(name, "true_conflict", **opts)
            self.coordinate(name, project, case, gold, folder / "output")
        folder, project, case, gold = self.prepare("qualifier-link", "counter_support", qualifier=True)
        self.coordinate("qualifier-link", project, case, gold, folder / "output")
        # Synchronize every displayed claim, IR, Markdown preamble and pins.
        for name, author, mutate in forgery_cases():
            folder = self.root / author
            output = self.root / ("forged-" + name)
            shutil.copytree(folder / "output", output)
            report = strict_json((output / "coordination.json").read_bytes())
            mutate(report); rewrite_output(output, report)
            try:
                verify_report(folder / "project", folder / "case", output, (folder / "gold.json").read_bytes())
            except AssertionError as exc:
                self.rows.append(dict(name=name, rejected=True, reason=str(exc), coherent=True))
            else:
                raise AssertionError("coherent forgery accepted: " + name)
            self.flush()
        for name, field, value in context_declaration_forgeries():
            folder = self.root / "true_conflict"
            case, output = forged_case(folder, self.root / ("declared-" + name), field, value)
            try:
                verify_report(folder / "project", case, output, (folder / "gold.json").read_bytes())
            except AssertionError as exc:
                self.rows.append(dict(name="declared-" + name, rejected=True, reason=str(exc), coherent=True))
            else:
                raise AssertionError("coherent author context forgery accepted: " + name)
            self.flush()
        folder = self.root / "true_conflict"
        destination = self.root / "source-tamper"; destination.mkdir()
        for name in ("project", "case", "output"):
            shutil.copytree(folder / name, destination / name)
        project = destination / "project"
        selected = strict_json((destination / "case/coordination.json").read_bytes())["inputs"][0]["binding"]["version_id"]
        with sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True) as db:
            relative = db.execute("SELECT normalized_snapshot FROM source_versions WHERE version_id=?", (selected,)).fetchone()[0]
        resource = project / relative
        resource.write_bytes(resource.read_bytes() + b"tampered source\n")
        try:
            verify_report(project, destination / "case", destination / "output", (folder / "gold.json").read_bytes())
        except AssertionError as exc:
            self.rows.append(dict(name="source-tamper", rejected=True, reason=str(exc)))
        else:
            raise AssertionError("actual source tamper accepted")
        if os.name != "nt":
            folder = self.root / "true_conflict"
            alias = self.root / "aliased-ancestor"
            alias.symlink_to(folder, target_is_directory=True)
            gold = (folder / "gold.json").read_bytes()
            for role in ("project", "case", "output"):
                roots = {key: folder / key for key in ("project", "case", "output")}
                roots[role] = alias / role
                try:
                    verify_report(roots["project"], roots["case"], roots["output"], gold)
                except AssertionError as exc:
                    assert "ancestor link" in str(exc), "ancestor-specific independent rejection"
                else:
                    raise AssertionError("aliased ancestor accepted: " + role)
                cli_roots = dict(roots)
                cli_roots["output"] = alias / ("new-output-" + role) if role == "output" else self.root / ("rejected-output-" + role)
                expected_exit = 3 if role == "project" else 2
                run = self.call("ancestor-" + role, ["knowledge", "coordinate", cli_roots["project"], cli_roots["case"], cli_roots["output"]], expected_exit)
                assert "PROJECT_UNSAFE_PATH" in run.stderr.decode("utf-8"), "CLI actual ancestor-specific refusal"
                assert not cli_roots["output"].exists(), "rejected alias output not written"
                self.rows.append(dict(name="ancestor-" + role, rejected=True, engineering_passed=True))
                self.check_protected(); self.flush()
            from verify_knowledge_conflicts import guard_path
            try:
                guard_path(alias / "gold.json", directory=False)
            except AssertionError as exc:
                assert "ancestor link" in str(exc)
            else:
                raise AssertionError("aliased gold ancestor accepted")
            self.rows.append(dict(name="ancestor-gold", rejected=True, engineering_passed=True))
            self.flush()
        self.check_protected(); exact(fingerprint(self.cli.read_bytes()), self.cli_fp, "CLI unchanged")
        source_after = {str(p.relative_to(REPO)): fingerprint(p.read_bytes()) for p in self.source_paths}
        self.write_new(self.root / "source-pins-after.json", source_after)
        exact(source_after, self.source_pins, "audit source unchanged")
        result = dict(schema_version=3, scenario_count=len(self.rows), command_count=len(self.calls), scenarios=self.rows,
                      calls=self.calls, protected_before=self.protected, protected_after=self.check_protected(), cli_fingerprint=self.cli_fp,
                      source_before=self.source_pins, source_after=source_after)
        self.write_new(self.root / "audit.json", result)
        return result


def forgery_cases():
    def fact(field, value):
        def mutate(r):
            target = r["facts"][0]
            target[field] = value
            for occurrence in r["occurrences"]:
                if occurrence["occurrence_id"] in target["occurrence_ids"]:
                    occurrence["unit_value" if field == "unit" else field] = value
        return mutate
    def context(field, value):
        def mutate(r):
            o = r["occurrences"][0]
            o["context"][field] = value
            if field in ("time", "applicability_status"):
                next(f for f in r["facts"] if o["occurrence_id"] in f["occurrence_ids"])[field] = value
        return mutate
    def verdict(status):
        def mutate(r):
            r["status"] = status
            reviewed = status == "needs_review"
            for o in r["occurrences"]:
                o.update(status=status, issue_codes=["COORD_TRUE_CONFLICT"] if reviewed else [])
            for f in r["facts"]:
                f.update(status=status, issue_codes=["COORD_TRUE_CONFLICT"] if reviewed else [])
            for p in r["processing_records"]:
                p.update(status=status, issue_codes=["COORD_TRUE_CONFLICT"] if reviewed else [],
                         action="retain_for_review" if reviewed else "retained",
                         reason="COORD_TRUE_CONFLICT" if reviewed else "retained with complete source provenance")
            for rel in r["relations"]:
                rel.update(relation="true_conflict" if reviewed else "numeric_equivalent", issue_code="COORD_TRUE_CONFLICT" if reviewed else None)
            r["totals"].update(passed=0 if reviewed else len(r["occurrences"]), needs_review=len(r["occurrences"]) if reviewed else 0,
                              canonical_facts=None if reviewed else len(r["facts"]), true_conflicts=len(r["relations"]) if reviewed else 0,
                              conflicting_occurrences=len(r["occurrences"]) if reviewed else 0)
        return mutate
    def omit_occurrence(r):
        victim = r["occurrences"].pop()
        oid = victim["occurrence_id"]
        r["facts"] = [f for f in r["facts"] if oid not in f["occurrence_ids"]]
        r["relations"] = [p for p in r["relations"] if oid not in (p["left_occurrence_id"], p["right_occurrence_id"])]
        r["processing_records"] = [p for p in r["processing_records"] if p["occurrence_id"] != oid]
        for k in ("source_occurrences", "input_atoms", "evidence_links", "independent_sources", "needs_review", "conflicting_occurrences", "known_condition_occurrences", "dated_occurrences", "context_evidence_links"):
            r["totals"][k] -= 1
        r["totals"]["retained_fact_groups"] = len(r["facts"])
        r["totals"]["true_conflicts"] = len(r["relations"])
    def omit_counter(r):
        o = next(o for o in r["occurrences"] if any(l["relation"] == "refutes" for l in o["original_atom"]["atom"]["evidence_links"]))
        links = o["original_atom"]["atom"]["evidence_links"]
        counter_ids = {l["evidence_id"] for l in links if l["relation"] == "refutes"}
        o["original_atom"]["atom"]["evidence_links"] = [l for l in links if l["relation"] != "refutes"]
        retained_ids = {l["evidence_id"] for l in o["original_atom"]["atom"]["evidence_links"]}
        o["original_evidence"] = [e for e in o["original_evidence"] if e["evidence_id"] in retained_ids]
        r["totals"]["counter_evidence_links"] = 0
        r["totals"]["evidence_links"] -= len(counter_ids)
    return [
        ("deleted-condition", "true_conflict", fact("conditions", [])),
        ("rewritten-condition", "true_conflict", fact("conditions", ["温度 99 ℃"])),
        ("deleted-exception", "true_conflict", fact("exceptions", [])),
        ("lost-context-support", "true_conflict", context("evidence_ids", [])),
        ("invented-time", "history", context("time", dict(status="known", precision="year", value="1999"))),
        ("unknown-as-na", "unknown_condition", context("applicability_status", "not_applicable")),
        ("default-zero", "undated_difference", context("time", dict(status="unknown", precision=None, value=0))),
        ("omitted-null", "undated_difference", lambda r: r["facts"][0]["time"].pop("value")),
        ("opponent-omitted", "true_conflict", omit_occurrence),
        ("history-omitted", "history", omit_occurrence),
        ("counter-omitted", "counter_support", omit_counter),
        ("majority-winner", "majority_conflict", verdict("passed")),
        ("false-success", "true_conflict", verdict("passed")),
        ("false-conflict", "numeric_equal", verdict("needs_review")),
        ("false-condition-conflict", "condition_variant", verdict("needs_review")),
        ("scientific-verified", "true_conflict", lambda r: r.update(scientific_quality="passed", knowledge_status="verified")),
        ("processing-omitted", "mixed_conflict", lambda r: r["processing_records"].pop()),
        ("processing-false-winner", "true_conflict", lambda r: r["processing_records"][0].update(action="winner", reason="newer source")),
        ("identity", "true_conflict", fact("object_id", "00000000-0000-4000-8000-000000000002")),
        ("unit-merge", "unit_difference", fact("unit", "invented-equivalent-unit")),
        ("boolean-count", "true_conflict", lambda r: r["totals"].update(failed=False)),
    ]


def context_declaration_forgeries():
    return [
        ("source-time", "time", dict(status="known", precision="year", value="1999")),
        ("source-applicability", "applicability_status", "not_applicable"),
        ("support-omission", "context_evidence_ids", []),
        ("support-duplicate", "context_evidence_ids", ["a-support", "a-support"]),
        ("cross-input-support", "context_evidence_ids", ["b-support"]),
        ("original-identity", "original_atom_id", "b-speed"),
        ("missing-null", "time", dict(status="unknown", precision=None)),
        ("zero-date", "time", dict(status="known", precision="year", value="0000")),
        ("invalid-leap-day", "time", dict(status="known", precision="day", value="2023-02-29")),
    ]
