"""Actual CLI preparation from frozen v2 authors; no coordination-derived gold."""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import uuid

from verify_knowledge_coordination import (Audit, REPO, FILES, actual_source,
    strict_json, fingerprint, encoded, exact, inventory, compact, normalized)


def rewrite_output(output, report):
    """Synchronize every result representation before a semantic forgery check."""
    # Recompute forged stable IDs from their own displayed data, so rejection
    # cannot be explained solely by leaving an obsolete fact identifier.
    for f in report["facts"]:
        key = ["coordination_v2", f["object_id"], f["property_id"], normalized(f["value"]), normalized(f["unit"]),
               f["applicability_status"], sorted(map(normalized, f["conditions"])), sorted(map(normalized, f["exceptions"])),
               [f["time"].get(k) for k in ("status", "precision", "value")],
               f["occurrence_ids"][0] if f["applicability_status"] == "unknown" else None]
        f["fact_id"] = "fact-v2-" + hashlib.sha256(compact(key)).hexdigest()
    report["facts"].sort(key=lambda f: f["fact_id"])
    ir = output / "coordination-ir.json"
    if ir.exists():
        body = {k: v for k, v in report.items() if k not in
                ("report_kind", "status", "network_calls", "paid_calls", "artifacts")}
        ir.write_bytes(encoded(dict(body, revision=1)))
    report["artifacts"] = {name: fingerprint((output / name).read_bytes())
                           for name in report["artifacts"]}
    (output / "coordination.json").write_bytes(encoded(report))
    md = output / "coordination.md"
    md.write_bytes(md.read_bytes().split(b"```json\n", 1)[0] + b"```json\n" +
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


class VariantsAudit(Audit):
    def __init__(self, cli, root):
        self.cli, self.root = Path(cli).resolve(), Path(root).resolve()
        self.root.mkdir()
        self.calls, self.rows, self.protected = [], [], {}
        self.sentinel = self.root.parent / ("variants-sentinel-" + uuid.uuid4().hex + ".txt")
        self.sentinel.write_bytes(b"external human sentinel\n")
        self.protect(self.sentinel)
        fixture = REPO / "fixtures/knowledge-coordination-v2"
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

    def prepare(self, name, author_name=None, *, multiple=False, reorder=False, shared_atom=False):
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
            candidate.update(binding=b, request_id="synthetic-variants-" + ident)
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
            atom["measurement"] = dict(value_status="known", value=g["value"], unit=g["unit"], conditions=g["conditions"])
            self.write_new(review / "candidates.json", candidate)
            self.write_new(review / "decision.json", dict(schema_version=1, unit_id=u["unit_id"], disposition="included", atom_ids=[item["original_atom_id"]], exclusion_reason=None))
            self.write_new(review / "review.json", dict(schema_version=1, purpose="single_measurement_source_review", candidate_file="candidates.json", gold_file="gold.json", decision_file="decision.json", gold_sha256=fingerprint(gold_raw)["sha256"], subject=item["subject"], property=item["property"], allowed_contexts=[], selection=[selection], reviewer="automated_source_review", interpretation_timing="before_synthetic_response"))
            bundle = case / ("bundle-" + ident)
            self.call(name + "-review-" + ident, ["extract", "verify", project, imp["import_id"], review, bundle])
            inputs.append(dict(input_id=ident, review_bundle=bundle.name, binding=b, artifacts={f: fingerprint((bundle / f).read_bytes()) for f in FILES}))
        if reorder:
            inputs.reverse()
        self.write_new(case / "coordination.json", dict(schema_version=2, purpose="explicit_identity_condition_time", project_id=inputs[0]["binding"]["project_id"], identity_file="identity.json", inputs=inputs))
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
        for name, opts in (("multiple-supports", dict(multiple=True)), ("ordering-display", dict(reorder=True))):
            folder, project, case, gold = self.prepare(name, "conditional", **opts)
            self.coordinate(name, project, case, gold, folder / "output")
        folder, project, case, gold = self.prepare("shared-original-atom-id", "not_applicable", shared_atom=True)
        self.coordinate("shared-original-atom-id", project, case, gold, folder / "output")
        folder = self.root / "conditional"
        destination = self.root / "mtime-invariance"; destination.mkdir()
        gold = (folder / "gold.json").read_bytes()
        (destination / "gold.json").write_bytes(gold)
        self.write_new(destination / "expected-pin.json", fingerprint(gold))
        for name in ("project", "case"):
            shutil.copytree(folder / name, destination / name)
        for path in (destination / "project").rglob("*"):
            if path.is_file():
                os.utime(path, (946684800, 946684800))
        self.protect(destination / "project"); self.protect(destination / "case"); self.protect(destination / "gold.json")
        self.coordinate("mtime-invariance", destination / "project", destination / "case", gold, destination / "output")
        # Every coherent mutation has synchronized Markdown, IR and artifact pins.
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
            folder = self.root / "conditional"
            case, output = forged_case(folder, self.root / ("declared-" + name), field, value)
            try:
                verify_report(folder / "project", case, output, (folder / "gold.json").read_bytes())
            except AssertionError as exc:
                self.rows.append(dict(name="declared-" + name, rejected=True, reason=str(exc), coherent=True))
            else:
                raise AssertionError("coherent author context forgery accepted: " + name)
            self.flush()
        self.check_protected(); exact(fingerprint(self.cli.read_bytes()), self.cli_fp, "CLI unchanged")
        source_after = {str(p.relative_to(REPO)): fingerprint(p.read_bytes()) for p in self.source_paths}
        self.write_new(self.root / "source-pins-after.json", source_after)
        exact(source_after, self.source_pins, "audit source unchanged")
        result = dict(schema_version=2, scenario_count=len(self.rows), command_count=len(self.calls), scenarios=self.rows,
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
    def relation(r):
        pair = r["relations"][0]
        pair.update(relation="historical_evolution", issue_code=None,
                    earlier_occurrence_id=pair["left_occurrence_id"], later_occurrence_id=pair["right_occurrence_id"])
    return [
        ("deleted-condition", "conditional", fact("conditions", [])),
        ("rewritten-condition", "conditional", fact("conditions", ["温度 99 ℃"])),
        ("deleted-exception", "conditional", fact("exceptions", [])),
        ("rewritten-exception", "conditional", fact("exceptions", ["新增例外"])),
        ("lost-context-support", "conditional", context("evidence_ids", [])),
        ("invented-time", "history", context("time", dict(status="known", precision="year", value="1999"))),
        ("unknown-as-na", "unknown_condition", context("applicability_status", "not_applicable")),
        ("default-zero", "conditional", context("time", dict(status="unknown", precision=None, value=0))),
        ("omitted-null", "conditional", lambda r: r["facts"][0]["time"].pop("value")),
        ("omitted-occurrence", "conditional", lambda r: r["occurrences"].pop()),
        ("identity", "conditional", fact("object_id", "00000000-0000-4000-8000-000000000002")),
        ("version", "same_source_history", lambda r: r["occurrences"][0]["binding"].update(version_id=r["occurrences"][1]["binding"]["version_id"])),
        ("unit-merge", "unit_difference", fact("unit", "invented-equivalent-unit")),
        ("overlap-order", "overlapping_time", relation),
        ("boolean-count", "conditional", lambda r: r["totals"].update(failed=False)),
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
