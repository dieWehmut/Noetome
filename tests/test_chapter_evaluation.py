"""Synthetic audit-engine tests, never evidence of textbook/scientific quality.

The tiny reference is passed only to the internal scoring core. Public/CLI entry
points have no hash override and must use the exact B19 reference and errata.
Semantic judgments below are authored test inputs: neither these fixtures nor the
evaluator derive a semantic pass from matching text or a candidate's own verdict.
"""

import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("chapter_evaluation_under_test", ROOT / "scripts/evaluate_chapter.py")
evaluation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evaluation)


def checks(**overrides):
    result = {key: "pass" for key in evaluation.CHECKS}
    result.update(overrides)
    return result


class ChapterEvaluationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.reference_root = self.root / "reference"
        self.notes_root = self.root / "export"
        self.reference_root.mkdir()
        (self.notes_root / "知识").mkdir(parents=True)
        self.quotes = ["At 25 °C, ΔG = ΔH − TΔS in kJ mol−1.",
                       "At fixed temperature, a catalyst changes the rate, not the equilibrium constant.",
                       "This document is a synthetic evaluation fixture."]
        source = ("\n".join(self.quotes) + "\n").encode("utf-8")
        (self.reference_root / "page-0034.txt").write_bytes(source)
        self.reference = {"schema_version": 1, "reference_id": "synthetic-only-not-scientific",
                          "annotator": "ai", "human_review": "pending",
                          "source": {"source_id": "frozen-source", "content_sha256": "1" * 64,
                                     "page_count": 100},
                          "scope": {"schema_version": 1, "sources": [{"source_id": "frozen-source",
                                    "page_ranges": [{"start_page": 34, "end_page": 34}]}]},
                          "atoms": [], "visuals": [{"id": "V1", "conditions": [], "unknowns": ["U1"],
                                    "required_for_scope": True, "physical_page": 34, "status": "partial_semantics", "supports": []}],
                          "formulas": [{"id": "F1", "units": ["kJ mol−1"], "conditions": ["25 °C"],
                                       "symbols": ["−"], "physical_page": 34, "required_for_scope": True, "supports": []}],
                          "unknowns": [{"id": "U1", "pages": [34], "status": "open", "issue": "3D structure unresolved"}]}
        self.candidate = {"schema_version": 1, "producer": {"kind": "ai", "id": "extractor-fixture"},
                          "source_id": "new-actual-source", "source_sha256": "1" * 64, "run_id": "run-synthetic",
                          "source_mapping": {"reference_source_id": "frozen-source",
                                             "actual_source_id": "new-actual-source", "source_sha256": "1" * 64,
                                             "physical_pages": 100,
                                             "page_ranges": [{"start_page": 34, "end_page": 34}]},
                          "atoms": [], "native_concepts": [{"concept_id": "concept-fixture"}],
                          "native_issues": [{"issue_id": "issue-fixture", "kind": "review_pending"}]}
        for index, quote in enumerate(self.quotes, 1):
            cid, rid, eid = "C" + str(index), "A" + str(index), "E" + str(index)
            selected = quote.encode("utf-8")
            start = source.index(selected)
            conditions = ["25 °C"] if index == 1 else (["fixed temperature"] if index == 2 else [])
            units = ["kJ mol−1"] if index == 1 else []
            support = {"source_id": "frozen-source", "physical_page": 34, "page_text_file": "page-0034.txt",
                       "page_text_sha256": evaluation.sha256(source), "page_sha256": "2" * 64,
                       "utf8_start": start, "utf8_end": start + len(selected), "quote": quote,
                       "quote_sha256": evaluation.sha256(selected)}
            self.reference["atoms"].append({"id": rid, "claim": quote,
                  "priority": "critical" if index < 3 else "necessary", "conditions": conditions,
                  "units": units, "symbols": ["−"] if index == 1 else [], "exceptions": [], "supports": [support]})
            native_evidence = {"schema_version": 1, "evidence_id": eid, "revision": 1,
                  "source_id": "new-actual-source", "source_sha256": "1" * 64, "page_number": 34,
                  "page_sha256": "3" * 64, "unit_id": "unit-fixture", "start_byte": start,
                  "end_byte": start + len(selected), "excerpt_sha256": evaluation.sha256(selected),
                  "quote": quote, "bbox": None, "request_id": "request-fixture"}
            # C2 is a reviewed paraphrase, deliberately not a source substring.
            content = "At constant temperature, catalysts affect rate without altering K." if index == 2 else quote
            atom = {"schema_version": 1, "atom_id": cid, "revision": 1, "kind": "statement",
                    "concept_ids": ["concept-fixture"], "content": content, "qualifiers": list(conditions) + units,
                    "evidence_links": [{"evidence_id": eid, "relation": "supports"}],
                    "verification": "unverified", "value_status": "known" if index == 1 else "not_applicable",
                    "measurement": {"value_status": "known", "value": "ΔH − TΔS", "unit": "kJ mol−1",
                                    "conditions": list(conditions)} if index == 1 else None}
            self.candidate["atoms"].append({"id": cid,
                  "payload": {"atom": atom, "evidence": [{"relation": "supports", "evidence": native_evidence}]},
                  "note_paths": ["知识/" + cid + ".md"]})
        self.errata = {"schema_version": 1, "errata_version": 1,
                       "base_reference_sha256": evaluation.REFERENCE_SHA256,
                       "annotator": "ai", "human_review": "pending", "entries": []}
        self.review = {"schema_version": 1, "reference_sha256": evaluation.REFERENCE_SHA256,
                       "errata_sha256": evaluation.ERRATA_SHA256, "candidate_sha256": "",
                       "independent": True, "candidate_producer_id": "extractor-fixture",
                       "reviewer": {"kind": "ai", "id": "independent-review-fixture", "human_review": "pending"},
                       "note_files": [], "expected": [], "candidates": [], "inventory": []}
        for expected, candidate in zip(self.reference["atoms"], self.candidate["atoms"]):
            self.review["expected"].append({"id": expected["id"],
                  "reference_atom_sha256": evaluation.canonical_sha256(expected),
                  "candidate_ids": [candidate["id"]], "checks": checks(),
                  "reason": "Authored fixture judgment: meaning, bounds and actual note are faithful.", "note_anchors": []})
            self.review["candidates"].append({"id": candidate["id"], "payload_sha256": "", "checks": checks(),
                  "reason": "Independent fixture review of native payload and actual rendered note.", "note_anchors": []})
        for kind, collection in (("visual", "visuals"), ("formula", "formulas"), ("unknown", "unknowns")):
            item = self.reference[collection][0]
            self.review["inventory"].append({"kind": kind, "id": item["id"],
                  "reference_item_sha256": evaluation.canonical_sha256(item),
                  "disposition": "resolved" if kind == "formula" else "preserved_unresolved",
                  "candidate_ids": ["C1"] if kind == "formula" else [],
                  "checks": checks() if kind == "formula" else checks(claim="unknown", support="unknown", localization="unknown"),
                  "reason": "Formula sign/unit checked." if kind == "formula" else "Unresolved detail explicitly preserved in the notice note.",
                  "note_anchors": [], "source_bindings": [] if kind == "formula" else [
                       {"page_number": 34, "page_sha256": "3" * 64, "localization": "page_only",
                        "issue_codes": ["ORIGINAL_REVIEW"]}]})
        self.render_notes()
        self.pin_review()

    def render_notes(self):
        concepts = {}
        for candidate in self.candidate["atoms"]:
            ids = [Path(name).stem for name in candidate["note_paths"]]
            candidate["payload"]["atom"]["concept_ids"] = ids
            for identity in ids:
                concepts[identity] = {"concept_id": identity, "revision": 1, "canonical_name": "合成 " + identity}
        self.candidate["native_concepts"] = list(concepts.values())
        for candidate in self.candidate["atoms"]:
            for name in candidate["note_paths"]:
                path = self.notes_root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(("# 合成测试\n\n" + candidate["payload"]["atom"]["content"] + "\n").encode("utf-8"))
        notice = ("# 处理范围\n\n### 物理页 34\n\n原件 SHA-256：" + "1" * 64
                  + "；页 SHA-256：" + "3" * 64 + "\n- ORIGINAL\\_REVIEW：原页保留待核对问题。\n"
                  + "V1: 图形细节待复核。\nU1: 三维结构未重建，待专家复核。\n")
        (self.notes_root / "处理范围.md").write_bytes(notice.encode("utf-8"))
        self.write_native_export()

    def write_native_export(self, request_strategy_version=2):
        """Author fixture-native files explicitly; pin_review never rewrites these."""
        index = self.notes_root / ".noetome-index"
        index.mkdir(exist_ok=True)
        atoms = [copy.deepcopy(row["payload"]["atom"]) for row in self.candidate["atoms"]]
        evidence = {}
        for row in self.candidate["atoms"]:
            for bound in row["payload"]["evidence"]:
                evidence[bound["evidence"]["evidence_id"]] = copy.deepcopy(bound["evidence"])
        blocks, notes = [], []
        for concept in self.candidate["native_concepts"]:
            nid = concept["concept_id"]
            path = "知识/" + nid + ".md"
            related = [atom for atom in atoms if nid in atom["concept_ids"]]
            notes.append({"note_id": nid, "revision": concept["revision"],
                          "title": concept["canonical_name"], "path": path, "blocks": len(related)})
            blocks.extend({"note_id": nid, "path": path, "section": "知识与过程", "block_ordinal": ordinal,
                           "atom_id": atom["atom_id"], "atom_revision": atom["revision"],
                           "evidence_links": copy.deepcopy(atom["evidence_links"])}
                          for ordinal, atom in enumerate(related))
        pages = [{"source_id": "new-actual-source", "source_sha256": "1" * 64, "page_number": number,
                  "page_sha256": "3" * 64 if number == 34 else None, "in_scope": number == 34}
                 for number in range(1, 101)]
        records = {"atoms": atoms, "evidence": list(evidence.values()),
                   "concepts": self.candidate["native_concepts"], "issues": self.candidate["native_issues"],
                    "note-blocks": blocks, "notes": notes, "pages": pages,
                    "source-locations": [{"schema_version": 1, "run_id": "run-synthetic",
                        "source_id": "new-actual-source", "source_sha256": "1" * 64, "page_number": 34,
                        "page_sha256": "3" * 64, "status": "review_pending", "units_failed": 0, "units_pending": 0,
                        "geometry": {"width_pt": 100, "height_pt": 100, "rotation": 0, "geometry": "pdf_display"},
                        "source_issues": [{"code": "ORIGINAL_REVIEW", "message": "原页保留待核对问题。"}],
                        "regions": [], "localization": "page_only", "issue_region_mapping": "unresolved",
                        "note_path": "处理范围.md"}]}
        for kind, rows in records.items():
            (index / (kind + ".ndjson")).write_text("".join(json.dumps(row, ensure_ascii=False) + "\n"
                                                          for row in rows), encoding="utf-8")
        sources = [{"source_id": "new-actual-source", "source_sha256": "1" * 64,
                    "display_name": "synthetic only", "byte_length": 100, "physical_pages": 100,
                    "scope_schema_version": 1, "page_ranges": [{"start_page": 34, "end_page": 34}]}]
        run_id = "run-synthetic"
        source_index = {"schema_version": 1, "run_id": run_id, "sources": sources,
                        "mapping_directory": ".noetome-index", "originals_included": False}
        (self.notes_root / "source-index.json").write_text(json.dumps(source_index), encoding="utf-8")
        summary = {"schema_version": 1, "run_id": run_id, "sources": sources,
                   "request_strategy_version": request_strategy_version,
                   "physical_pages": 100, "selected_pages": 1, "out_of_scope_pages": 99,
                   "atoms": len(atoms), "evidence": len(evidence), "concepts": len(self.candidate["native_concepts"])}
        names = [".noetome-index/" + kind + ".ndjson" for kind in records] + ["source-index.json"]
        names += [path.relative_to(self.notes_root).as_posix() for path in sorted(self.notes_root.rglob("*.md"))]
        def pin(name):
            data = (self.notes_root / name).read_bytes()
            return {"path": name, "fingerprint": {"bytes": len(data), "sha256": evaluation.sha256(data)}}
        files = [pin(name) for name in names]
        scope = {"schema_version": 1, "run_id": run_id, "scope": "selected_physical_page_ranges",
                 "scope_schema_version": 1, "summary": summary, "manifest": "export-manifest.json", "files": files}
        (self.notes_root / "处理范围.json").write_text(json.dumps(scope, ensure_ascii=False), encoding="utf-8")
        manifest = {"schema_version": 1, "run_id": run_id, "files": files + [pin("处理范围.json")],
                    "self_included": False, "status": "complete"}
        (self.notes_root / "export-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")

    def refresh_native_pins(self):
        """Re-pin deliberately malformed native fixtures without altering any IR rows."""
        scope_path = self.notes_root / "处理范围.json"
        manifest_path = self.notes_root / "export-manifest.json"
        scope = json.loads(scope_path.read_text(encoding="utf-8"))
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        for row in scope["files"]:
            data = (self.notes_root / row["path"]).read_bytes()
            row["fingerprint"] = {"bytes": len(data), "sha256": evaluation.sha256(data)}
        scope_path.write_text(json.dumps(scope, ensure_ascii=False), encoding="utf-8")
        data = scope_path.read_bytes()
        manifest["files"] = scope["files"] + [{"path": "处理范围.json",
                           "fingerprint": {"bytes": len(data), "sha256": evaluation.sha256(data)}}]
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")

    def anchor(self, name):
        data = (self.notes_root / name).read_bytes()
        return {"path": name, "utf8_start": 0, "utf8_end": len(data), "sha256": evaluation.sha256(data)}

    def pin_review(self, payloads=True):
        """Refresh byte bindings only; NEVER change an authored semantic verdict."""
        self.review["candidate_sha256"] = evaluation.canonical_sha256(self.candidate)
        candidates = {item["id"]: item for item in self.candidate["atoms"]}
        self.review["note_files"] = [{"path": path.relative_to(self.notes_root).as_posix(),
              "sha256": evaluation.sha256(path.read_bytes()), "bytes": len(path.read_bytes())}
              for path in sorted(self.notes_root.rglob("*.md"))]
        for row in self.review["candidates"]:
            candidate = candidates[row["id"]]
            if payloads:
                row["payload_sha256"] = evaluation.canonical_sha256(candidate["payload"])
            row["note_anchors"] = [self.anchor(path) for path in candidate["note_paths"]]
        for row in self.review["expected"]:
            row["note_anchors"] = [self.anchor(path) for cid in row["candidate_ids"]
                                   for path in candidates[cid]["note_paths"]]
        for row in self.review["inventory"]:
            names = [path for cid in row["candidate_ids"] for path in candidates[cid]["note_paths"]]
            row["note_anchors"] = [self.anchor(path) for path in names or ["处理范围.md"]]

    def score(self):
        hashes = {"reference_sha256": evaluation.REFERENCE_SHA256, "errata_sha256": evaluation.ERRATA_SHA256,
                  "candidate_sha256": evaluation.canonical_sha256(self.candidate), "review_sha256": "9" * 64}
        return evaluation._evaluate(self.reference, self.errata, self.candidate, self.review,
                                    self.notes_root, self.reference_root, hashes)

    def assert_fraction(self, report, metric, numerator, denominator):
        self.assertEqual((report["metrics"][metric]["numerator"], report["metrics"][metric]["denominator"]),
                         (numerator, denominator))

    def test_reviewed_paraphrase_passes_metrics_but_ai_pending_never_science(self):
        self.assertNotIn(self.candidate["atoms"][1]["payload"]["atom"]["content"], self.quotes[1])
        report = self.score()
        for key in ("recall", "accuracy", "evidence_support", "localization", "note_presence"):
            self.assert_fraction(report, key, 3, 3)
        self.assert_fraction(report, "critical_recall", 2, 2)
        self.assert_fraction(report, "necessary_recall", 1, 1)
        self.assertTrue(report["metric_gate_passed"])
        self.assertFalse(report["gate_passed"])
        self.assertFalse(report["scientific_pass_claim"])
        self.assertFalse(report["human_gold_claim"])
        self.assertEqual(report["reviewer"], self.review["reviewer"])
        self.assertEqual(len(report["expected_results"]), 3)
        self.assertEqual(len(report["candidate_results"]), 3)
        self.assertEqual(report["inventory"]["unknown"]["preserved_unresolved_ids"], ["U1"])
        self.assertEqual(report["inventory"]["visual"]["numerator"], 0)

    def test_known_precise_strategy_versions_use_full_native_evaluation(self):
        path = self.notes_root / "处理范围.json"
        self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["summary"]["request_strategy_version"], 2)
        for version in (2, 3, 4, 5):
            with self.subTest(version=version):
                self.write_native_export(request_strategy_version=version)
                scope = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(scope["summary"]["request_strategy_version"], version)
                report = self.score()
                self.assertEqual(report["status"], "evaluated")
                for metric in ("recall", "accuracy", "evidence_support", "localization", "note_presence"):
                    self.assert_fraction(report, metric, 3, 3)
                self.assert_fraction(report, "critical_recall", 2, 2)
                self.assert_fraction(report, "necessary_recall", 1, 1)
                self.assertTrue(report["metric_gate_passed"])
                for flag in ("gate_passed", "scientific_pass_claim", "human_gold_claim",
                             "formal_200_atom_benchmark_claim"):
                    self.assertFalse(report[flag])
                self.assertEqual(report["native_export"]["atom_ids"], ["C1", "C2", "C3"])
                self.assertEqual(report["native_export"]["actual_counts"]["atoms"], 3)
                self.assertEqual(report["inventory"]["unknown"]["preserved_unresolved_ids"], ["U1"])
                self.assertEqual(report["inventory"]["visual"]["preserved_unresolved_ids"], ["V1"])
                self.assertEqual(len(report["native_export"]["files"]), 11)
                for pin in report["native_export"]["files"]:
                    data = (self.notes_root / pin["path"]).read_bytes()
                    self.assertEqual((pin["bytes"], pin["sha256"]), (len(data), evaluation.sha256(data)))

    def test_nonprecise_unknown_or_noninteger_native_strategy_versions_are_rejected(self):
        for version in (1, 0, -1, 6, 99, None, "2", "3", "4", "5", 2.0, 3.0, 4.0, 5.0, True, False, [], {}):
            with self.subTest(version=version):
                self.write_native_export(request_strategy_version=version)
                with self.assertRaisesRegex(evaluation.EvaluationError, "precise request strategy 2, 3, 4 or 5"):
                    self.score()

    def test_missing_native_strategy_version_is_rejected_after_repinning(self):
        path = self.notes_root / "处理范围.json"
        scope = json.loads(path.read_text(encoding="utf-8"))
        del scope["summary"]["request_strategy_version"]
        path.write_text(json.dumps(scope, ensure_ascii=False), encoding="utf-8")
        self.refresh_native_pins()
        with self.assertRaisesRegex(evaluation.EvaluationError, "precise request strategy 2, 3, 4 or 5"):
            self.score()

    def test_wrong_fact_with_correct_quote_and_self_report_fails(self):
        candidate = self.candidate["atoms"][1]
        original_evidence = copy.deepcopy(candidate["payload"]["evidence"])
        candidate["payload"]["atom"]["content"] = "At fixed temperature, a catalyst raises the equilibrium constant."
        candidate["payload"]["atom"]["verification"] = "verified"
        candidate["payload"]["atom"]["semantic_success"] = True
        self.review["expected"][1]["checks"].update(claim="fail", support="fail")
        self.review["candidates"][1]["checks"].update(claim="fail", support="fail")
        self.review["candidates"][1]["reason"] = "Correct quote contradicts the candidate's catalyst/equilibrium assertion."
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assertEqual(candidate["payload"]["evidence"], original_evidence)
        self.assert_fraction(report, "accuracy", 2, 3)
        self.assert_fraction(report, "evidence_support", 2, 3)
        self.assert_fraction(report, "localization", 3, 3)
        self.assert_fraction(report, "critical_recall", 1, 2)
        self.assertFalse(report["metric_gate_passed"])

    def test_removed_condition_from_body_and_fields_still_fails_reference_review(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        atom["content"] = "ΔG = ΔH − TΔS in kJ mol−1."
        atom["measurement"]["conditions"] = []
        atom["qualifiers"] = ["kJ mol−1"]
        self.review["expected"][0]["checks"]["conditions"] = "fail"
        self.review["expected"][0]["reason"] = "25 °C absent in both actual note body and structured fields."
        # Candidate-side all-pass ratings cannot override the independent expected row.
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "accuracy", 2, 3)
        self.assert_fraction(report, "conditions_fidelity", 2, 3)
        self.assert_fraction(report, "critical_recall", 1, 2)

    def test_formula_sign_change_is_not_rescued_by_correct_quote(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        atom["content"] = atom["content"].replace("− TΔS", "+ TΔS")
        atom["measurement"]["value"] = "ΔH + TΔS"
        row = self.review["inventory"][1]
        row.update(disposition="missing", reason="Actual equation has the wrong entropy-term sign.")
        row["checks"]["symbols"] = "fail"
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "symbols_fidelity", 2, 3)
        self.assert_fraction(report, "accuracy", 2, 3)
        self.assertFalse(report["metric_gate_passed"])

    def test_unit_altered_in_both_body_and_measurement_is_wrong(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        atom["content"] = atom["content"].replace("kJ", "J")
        atom["measurement"]["unit"] = "J mol−1"
        atom["qualifiers"] = ["25 °C", "J mol−1"]
        self.review["expected"][0]["checks"]["units"] = "fail"
        self.review["expected"][0]["reason"] = "Unconverted energy expression now asserts J rather than kJ."
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "units_fidelity", 2, 3)
        self.assert_fraction(report, "accuracy", 2, 3)

    def test_unknown_claim_is_explicit_not_counted_as_covered(self):
        self.review["expected"][2]["checks"]["claim"] = "unknown"
        self.review["expected"][2]["reason"] = "Independent reviewer could not settle this claim."
        report = self.score()
        self.assert_fraction(report, "recall", 2, 3)
        self.assertEqual(report["metrics"]["necessary_recall"]["unknown_ids"], ["A3"])
        self.assertEqual(report["necessary_gaps"][0]["id"], "A3")
        self.assert_fraction(report, "accuracy", 2, 3)

    def test_candidate_unknown_and_reference_note_failure_propagate(self):
        self.review["candidates"][2]["checks"]["claim"] = "unknown"
        report = self.score()
        self.assertEqual(report["metrics"]["recall"]["unknown_ids"], ["A3"])
        self.assertEqual(report["metrics"]["accuracy"]["unknown_ids"], ["C3"])
        self.review["candidates"][2]["checks"]["claim"] = "pass"
        self.review["expected"][2]["checks"]["note_presence"] = "fail"
        report = self.score()
        self.assert_fraction(report, "note_presence", 2, 3)
        self.assert_fraction(report, "recall", 2, 3)

    def test_resolved_inventory_cannot_anchor_an_unrelated_notice(self):
        self.review["inventory"][1]["note_anchors"] = [self.anchor("处理范围.md")]
        with self.assertRaisesRegex(evaluation.EvaluationError, "mapped candidate notes"):
            self.score()

    def test_candidate_only_hallucination_remains_in_accuracy_denominator(self):
        candidate = copy.deepcopy(self.candidate["atoms"][2])
        candidate["id"] = candidate["payload"]["atom"]["atom_id"] = "C4"
        candidate["payload"]["atom"]["content"] = "All catalysts make equilibrium constants infinite."
        candidate["note_paths"] = ["知识/C4.md"]
        self.candidate["atoms"].append(candidate)
        self.review["candidates"].append({"id": "C4", "payload_sha256": "", "checks": checks(claim="fail", support="fail"),
                     "reason": "Extra actual candidate is unsupported and false; no reference mapping hides it.", "note_anchors": []})
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "recall", 3, 3)
        self.assert_fraction(report, "accuracy", 3, 4)
        self.assert_fraction(report, "evidence_support", 3, 4)

    def add_candidate_only(self, source_index=0):
        candidate = copy.deepcopy(self.candidate["atoms"][source_index])
        candidate["id"] = candidate["payload"]["atom"]["atom_id"] = "C4"
        candidate["note_paths"] = ["知识/C4.md"]
        self.candidate["atoms"].append(candidate)
        row = {"id": "C4", "payload_sha256": "", "checks": checks(),
               "reason": "Authored independent fixture review of an additional actual native atom.", "note_anchors": []}
        self.review["candidates"].append(row)
        self.render_notes()
        self.pin_review()
        return candidate, row

    def test_wrapper_cannot_drop_actual_bad_candidate_after_repinning_review(self):
        self.test_candidate_only_hallucination_remains_in_accuracy_denominator()
        self.candidate["atoms"] = self.candidate["atoms"][:-1]
        self.review["candidates"] = self.review["candidates"][:-1]
        self.pin_review()
        self.assertTrue((self.notes_root / "知识/C4.md").is_file())
        with self.assertRaisesRegex(evaluation.EvaluationError, "actual native atom inventory"):
            self.score()

    def test_native_summary_counts_are_cross_checked_with_real_rows(self):
        path = self.notes_root / "处理范围.json"
        for kind in ("atoms", "evidence", "concepts", "selected_pages", "out_of_scope_pages"):
            with self.subTest(kind=kind):
                scope = json.loads(path.read_text(encoding="utf-8"))
                scope["summary"][kind] -= 1
                path.write_text(json.dumps(scope), encoding="utf-8")
                self.refresh_native_pins()
                with self.assertRaisesRegex(evaluation.EvaluationError, "native summary count mismatch"):
                    self.score()
                self.write_native_export()

    def test_wrapper_cannot_drop_measurement_or_other_native_atom_fields(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        original = copy.deepcopy(atom)
        for key, value in (("measurement", None), ("qualifiers", []), ("content", "Silently shortened."),
                           ("verification", "verified")):
            with self.subTest(key=key):
                atom[key] = value
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "full native atom mismatch"):
                    self.score()
                atom.clear()
                atom.update(copy.deepcopy(original))
        self.pin_review()

    def test_native_measurement_key_must_not_be_omitted_even_in_actual_export(self):
        del self.candidate["atoms"][0]["payload"]["atom"]["measurement"]
        self.write_native_export()
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "measurement must be retained"):
            self.score()

    def test_wrapper_cannot_change_native_evidence_metadata_or_relation(self):
        candidate = self.candidate["atoms"][0]
        bound = candidate["payload"]["evidence"][0]
        bound["evidence"]["request_id"] = "different-request"
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "full native evidence mismatch"):
            self.score()
        bound["evidence"]["request_id"] = "request-fixture"
        bound["relation"] = candidate["payload"]["atom"]["evidence_links"][0]["relation"] = "qualifies"
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "full native atom mismatch"):
            self.score()

    def test_wrapper_cannot_omit_an_actual_native_note_copy(self):
        self.candidate["atoms"][0]["note_paths"].append("知识/C1-copy.md")
        self.render_notes()
        self.pin_review()
        self.candidate["atoms"][0]["note_paths"].pop()
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "actual native note copies"):
            self.score()

    def test_full_native_concepts_and_issues_are_mandatory_and_not_summaries(self):
        for field in ("native_concepts", "native_issues"):
            original = copy.deepcopy(self.candidate[field])
            for value in ([], [{**original[0], "extra_changed_field": True}]):
                with self.subTest(field=field, value=value):
                    self.candidate[field] = value
                    self.pin_review()
                    with self.assertRaisesRegex(evaluation.EvaluationError, "full native inventory mismatch"):
                        self.score()
            del self.candidate[field]
            self.pin_review()
            with self.assertRaisesRegex(evaluation.EvaluationError, "bounded array"):
                self.score()
            self.candidate[field] = original
        self.pin_review()

    def test_duplicate_native_atoms_and_missing_blocks_cannot_be_repinned_away(self):
        path = self.notes_root / ".noetome-index/atoms.ndjson"
        data = path.read_bytes()
        path.write_bytes(data + data.splitlines(keepends=True)[0])
        self.refresh_native_pins()
        with self.assertRaisesRegex(evaluation.EvaluationError, "native atoms duplicate ID"):
            self.score()
        self.write_native_export()
        path = self.notes_root / ".noetome-index/note-blocks.ndjson"
        path.write_bytes(b"".join(path.read_bytes().splitlines(keepends=True)[1:]))
        self.refresh_native_pins()
        with self.assertRaisesRegex(evaluation.EvaluationError, "native note block count/ordinal"):
            self.score()

    def test_native_page_fingerprint_and_scope_are_cross_checked(self):
        path = self.notes_root / ".noetome-index/pages.ndjson"
        for key, value, message in (("page_sha256", "4" * 64, "evidence/page fingerprint"),
                                    ("in_scope", False, "native page scope")):
            with self.subTest(key=key):
                rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
                rows[33][key] = value
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
                self.refresh_native_pins()
                with self.assertRaisesRegex(evaluation.EvaluationError, message):
                    self.score()
                self.write_native_export()

    def test_missing_native_ir_or_mismatched_run_fails_closed(self):
        self.candidate["run_id"] = "other-run"
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "schema/run mismatch"):
            self.score()
        self.candidate["run_id"] = "run-synthetic"
        self.pin_review()
        (self.notes_root / ".noetome-index/evidence.ndjson").unlink()
        with self.assertRaisesRegex(evaluation.EvaluationError, "regular input file"):
            self.score()

    def test_manifest_pins_are_verified_against_actual_bytes(self):
        path = self.notes_root / ".noetome-index/atoms.ndjson"
        path.write_bytes(path.read_bytes().replace(b'{', b'{ ', 1))
        with self.assertRaisesRegex(evaluation.EvaluationError, "native manifest fingerprint mismatch"):
            self.score()

    def test_native_file_fingerprints_and_actual_denominator_are_reported(self):
        report = self.score()
        snapshot = report["native_export"]
        self.assertEqual(snapshot["atom_ids"], ["C1", "C2", "C3"])
        self.assertEqual(snapshot["actual_counts"]["atoms"], 3)
        self.assertEqual(len(snapshot["files"]), 11)
        for pin in snapshot["files"]:
            data = (self.notes_root / pin["path"]).read_bytes()
            self.assertEqual((pin["bytes"], pin["sha256"]), (len(data), evaluation.sha256(data)))
        path = self.notes_root / ".noetome-index/issues.ndjson"
        path.write_bytes(path.read_bytes() + b" ")
        with self.assertRaisesRegex(evaluation.EvaluationError, "native export changed during evaluation"):
            evaluation._verify_export_unchanged(self.notes_root, snapshot)

    def test_actual_native_drift_during_scoring_is_rejected(self):
        original = evaluation._accepted
        changed = False
        def alter_once(*args, **kwargs):
            nonlocal changed
            if not changed:
                path = self.notes_root / ".noetome-index/atoms.ndjson"
                path.write_bytes(path.read_bytes() + b" ")
                changed = True
            return original(*args, **kwargs)
        with mock.patch.object(evaluation, "_accepted", side_effect=alter_once):
            with self.assertRaisesRegex(evaluation.EvaluationError, "native export changed during evaluation"):
                self.score()

    def test_unknown_measurement_nullable_unit_is_evaluated_without_passing_unknown_review(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        for unit in (None, "kJ mol−1"):
            with self.subTest(unit=unit):
                atom.update(value_status="unknown", qualifiers=["25 °C"] + ([unit] if unit else []),
                            measurement={"value_status": "unknown", "value": None, "unit": unit,
                                         "conditions": ["25 °C"]})
                self.review["candidates"][0]["checks"]["claim"] = "unknown"
                self.render_notes()
                self.pin_review()
                report = self.score()
                self.assertEqual(report["status"], "evaluated")
                for metric in ("recall", "accuracy", "evidence_support"):
                    self.assert_fraction(report, metric, 2, 3)
                self.assertEqual(report["metrics"]["accuracy"]["unknown_ids"], ["C1"])
                self.assertEqual(report["metrics"]["recall"]["unknown_ids"], ["A1"])
                self.assertFalse(report["metric_gate_passed"])
                self.assertFalse(report["gate_passed"])

    def test_not_applicable_measurement_null_fields_still_require_explicit_details_review(self):
        atom = self.candidate["atoms"][2]["payload"]["atom"]
        atom["measurement"] = {"value_status": "not_applicable", "value": None, "unit": None, "conditions": []}
        row = self.review["candidates"][2]
        row["checks"].update(conditions="not_applicable", units="not_applicable", symbols="not_applicable")
        self.render_notes()
        self.pin_review()
        report = self.score()
        self.assertEqual(report["status"], "evaluated")
        self.assert_fraction(report, "accuracy", 3, 3)
        self.assertFalse(report["gate_passed"])
        row["checks"]["details"] = "not_applicable"
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for details"):
            self.score()

    def test_known_measurement_requires_nonempty_unit(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        for unit in (None, "", " \t"):
            with self.subTest(unit=unit):
                atom["measurement"]["unit"] = unit
                self.render_notes()
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "native known measurement unit"):
                    self.score()

    def test_measurement_state_requires_contract_null_values(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        for status, value, unit in (("unknown", "0", None), ("not_applicable", "0", None),
                                    ("not_applicable", None, "kJ mol−1")):
            with self.subTest(status=status, value=value, unit=unit):
                atom.update(value_status=status, qualifiers=[unit] if unit else [],
                            measurement={"value_status": status, "value": value, "unit": unit, "conditions": []})
                self.render_notes()
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "native " + status + " measurement"):
                    self.score()

    def test_nullable_measurement_fields_cannot_be_missing_or_nonstring(self):
        atom = self.candidate["atoms"][0]["payload"]["atom"]
        for field in ("value", "unit"):
            for defect in ("missing", "nonstring"):
                with self.subTest(field=field, defect=defect):
                    atom.update(value_status="unknown", qualifiers=[],
                                measurement={"value_status": "unknown", "value": None, "unit": None, "conditions": []})
                    if defect == "missing":
                        del atom["measurement"][field]
                    else:
                        atom["measurement"][field] = 0
                    self.render_notes()
                    self.pin_review()
                    with self.assertRaisesRegex(evaluation.EvaluationError, "native measurement fields must be complete"):
                        self.score()

    def test_candidate_only_qualifiers_and_measurement_conditions_forbid_na(self):
        candidate, row = self.add_candidate_only()
        atom = candidate["payload"]["atom"]
        for qualifiers, conditions in ((["Only transparent samples."], []), ([], ["20 °C, dark"])):
            with self.subTest(qualifiers=qualifiers, conditions=conditions):
                atom["qualifiers"] = [atom["measurement"]["unit"]] + qualifiers + conditions
                atom["measurement"]["conditions"] = conditions
                row["checks"]["conditions"] = "not_applicable"
                self.render_notes()
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for conditions"):
                    self.score()

    def test_candidate_only_measurement_unit_forbids_na(self):
        _, row = self.add_candidate_only()
        row["checks"]["units"] = "not_applicable"
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for units"):
            self.score()

    def test_candidate_only_known_value_requires_details_review(self):
        candidate, row = self.add_candidate_only()
        atom = candidate["payload"]["atom"]
        atom.update(content="A scalar measurement.", qualifiers=["mM"],
                    measurement={"value_status": "known", "value": "2.5", "unit": "mM", "conditions": []})
        row["checks"]["details"] = "not_applicable"
        self.render_notes()
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for details"):
            self.score()

    def test_candidate_only_formula_content_or_value_requires_symbols_review(self):
        candidate, row = self.add_candidate_only()
        atom = candidate["payload"]["atom"]
        cases = [(content, None) for content in ("x = y", "x-y", "x/y", "x*y", "$x$", r"\(x\)", r"\[x\]")]
        cases.append(("A measured expression.",
                      {"value_status": "known", "value": "x_i", "unit": "mM", "conditions": []}))
        for content, measurement in cases:
            with self.subTest(content=content):
                atom.update(content=content, measurement=measurement,
                            value_status=measurement["value_status"] if measurement else "not_applicable",
                            qualifiers=[measurement["unit"]] if measurement else [])
                row["checks"]["symbols"] = "not_applicable"
                self.render_notes()
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for symbols"):
                    self.score()

    def test_candidate_only_formula_indicators_in_all_measurement_fields_require_symbols_review(self):
        candidate, row = self.add_candidate_only(source_index=2)
        atom = candidate["payload"]["atom"]
        for field in ("value", "unit", "qualifiers", "conditions"):
            for expression in ("x-y", "x/y", "x*y", "$x$"):
                with self.subTest(field=field, expression=expression):
                    measurement = {"value_status": "known", "value": "2.5", "unit": "mM", "conditions": []}
                    qualifiers = ["mM"]
                    if field == "qualifiers":
                        qualifiers.append(expression)
                    elif field == "conditions":
                        measurement["conditions"] = [expression]
                        qualifiers.append(expression)
                    else:
                        measurement[field] = expression
                        qualifiers = [measurement["unit"]]
                    atom.update(content="A scalar measurement.", value_status="known",
                                qualifiers=qualifiers, measurement=measurement)
                    row["checks"]["symbols"] = "not_applicable"
                    self.render_notes()
                    self.pin_review()
                    with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for symbols"):
                        self.score()

    def test_plain_candidate_without_applicable_fields_can_still_use_na(self):
        _, row = self.add_candidate_only(source_index=2)
        row["checks"].update({key: "not_applicable" for key in ("conditions", "units", "symbols", "details")})
        report = self.score()
        self.assert_fraction(report, "accuracy", 4, 4)
        self.assert_fraction(report, "evidence_support", 4, 4)

    def test_resolved_visual_semantics_requires_details_judgment(self):
        visual = self.reference["visuals"][0]
        visual["semantics"] = "This diagram encodes a specific relationship and direction."
        row = self.review["inventory"][0]
        row.update(reference_item_sha256=evaluation.canonical_sha256(visual), disposition="resolved",
                   candidate_ids=["C3"], checks=checks(details="not_applicable"))
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for details"):
            self.score()

    def test_native_measurement_and_all_top_level_data_are_fingerprinted(self):
        self.candidate["native_issues"][0]["kind"] = "silently_changed"
        with self.assertRaisesRegex(evaluation.EvaluationError, "input fingerprint"):
            self.score()
        self.review["candidate_sha256"] = evaluation.canonical_sha256(self.candidate)
        self.candidate["atoms"][0]["payload"]["atom"]["measurement"]["value"] = "wrong"
        self.pin_review(payloads=False)
        with self.assertRaisesRegex(evaluation.EvaluationError, "payload fingerprint"):
            self.score()

    def test_actual_note_changes_cannot_reuse_old_review(self):
        path = self.notes_root / "知识/C1.md"
        path.write_bytes(path.read_bytes().replace("25 °C".encode(), "26 °C".encode()))
        with self.assertRaisesRegex(evaluation.EvaluationError, "note fingerprint"):
            self.score()

    def test_extra_note_file_cannot_be_silently_unreviewed(self):
        (self.notes_root / "unreviewed.md").write_text("An extra unsupported assertion", encoding="utf-8")
        with self.assertRaisesRegex(evaluation.EvaluationError, "note inventory mismatch"):
            self.score()

    def test_missing_actual_note_cannot_be_replaced_by_snapshot_body(self):
        (self.notes_root / "知识/C2.md").unlink()
        with self.assertRaisesRegex(evaluation.EvaluationError, "note inventory mismatch"):
            self.score()

    def test_passing_candidate_needs_anchors_in_every_note_copy(self):
        self.candidate["atoms"][0]["note_paths"].append("知识/C1-copy.md")
        self.render_notes()
        self.pin_review()
        self.review["candidates"][0]["note_anchors"].pop()
        with self.assertRaisesRegex(evaluation.EvaluationError, "every actual note"):
            self.score()

    def test_anchor_sha_and_utf8_boundaries_are_verified(self):
        row = self.review["candidates"][0]
        row["note_anchors"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(evaluation.EvaluationError, "note anchor fingerprint"):
            self.score()
        self.pin_review()
        row["note_anchors"][0]["utf8_start"] = 3  # inside 合, after '# '
        with self.assertRaisesRegex(evaluation.EvaluationError, "splits a UTF-8"):
            self.score()

    def test_bad_native_byte_location_fails_even_when_review_says_pass(self):
        self.candidate["atoms"][0]["payload"]["evidence"][0]["evidence"]["start_byte"] = 1
        self.write_native_export()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "accuracy", 3, 3)
        self.assert_fraction(report, "evidence_support", 2, 3)
        self.assert_fraction(report, "localization", 2, 3)
        self.assertIn("C1", report["mechanical_evidence_failures"])

    def test_refutes_only_relation_cannot_be_counted_as_support(self):
        candidate = self.candidate["atoms"][2]
        candidate["payload"]["atom"]["evidence_links"][0]["relation"] = "refutes"
        candidate["payload"]["evidence"][0]["relation"] = "refutes"
        self.write_native_export()
        self.pin_review()
        report = self.score()
        self.assert_fraction(report, "evidence_support", 2, 3)
        self.assert_fraction(report, "localization", 3, 3)

    def test_incomplete_duplicate_or_unknown_review_ids_fail_closed(self):
        for collection in ("expected", "candidates", "inventory"):
            original = copy.deepcopy(self.review[collection])
            with self.subTest(collection=collection, defect="missing"):
                self.review[collection] = original[:-1]
                with self.assertRaises(evaluation.EvaluationError):
                    self.score()
            with self.subTest(collection=collection, defect="duplicate"):
                self.review[collection] = original + [copy.deepcopy(original[0])]
                with self.assertRaises(evaluation.EvaluationError):
                    self.score()
            with self.subTest(collection=collection, defect="unknown"):
                self.review[collection] = copy.deepcopy(original)
                self.review[collection][0]["id"] = "not-a-real-id"
                with self.assertRaises(evaluation.EvaluationError):
                    self.score()
            self.review[collection] = original

    def test_explicit_judgments_cannot_be_omitted_or_na_for_needed_condition(self):
        row = self.review["expected"][0]
        del row["checks"]["conditions"]
        with self.assertRaisesRegex(evaluation.EvaluationError, "all eight"):
            self.score()
        row["checks"]["conditions"] = "not_applicable"
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden"):
            self.score()

    def test_formula_expression_requires_sign_review_without_a_symbols_field(self):
        formula = self.reference["formulas"][0]
        del formula["symbols"]
        formula["normalized_expression"] = "ΔG = ΔH − TΔS"
        row = self.review["inventory"][1]
        row["reference_item_sha256"] = evaluation.canonical_sha256(formula)
        row["checks"]["symbols"] = "not_applicable"
        with self.assertRaisesRegex(evaluation.EvaluationError, "not_applicable forbidden for symbols"):
            self.score()

    def test_wrong_mapping_and_claim_fingerprint_fail_closed(self):
        row = self.review["expected"][0]
        row["candidate_ids"] = ["not-exported"]
        with self.assertRaisesRegex(evaluation.EvaluationError, "unknown candidate"):
            self.score()
        row["candidate_ids"] = ["C1"]
        row["reference_atom_sha256"] = "0" * 64
        with self.assertRaisesRegex(evaluation.EvaluationError, "effective reference atom"):
            self.score()

    def test_source_mapping_allows_new_id_but_not_different_source_or_range(self):
        self.assertEqual(self.score()["source_mapping"]["actual_source_id"], "new-actual-source")
        for key, value in (("reference_source_id", "wrong"), ("actual_source_id", "wrong"),
                           ("source_sha256", "0" * 64), ("physical_pages", 99),
                           ("page_ranges", [{"start_page": 34, "end_page": 35}])):
            with self.subTest(key=key):
                original = copy.deepcopy(self.candidate["source_mapping"][key])
                self.candidate["source_mapping"][key] = value
                self.pin_review()
                with self.assertRaises(evaluation.EvaluationError):
                    self.score()
                self.candidate["source_mapping"][key] = original
        self.pin_review()
        self.candidate["atoms"][0]["payload"]["evidence"][0]["evidence"]["source_id"] = "frozen-source"
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "native evidence source mismatch"):
            self.score()

    def test_self_review_and_ai_claiming_human_completion_are_rejected(self):
        reviewer = self.review["reviewer"]
        reviewer["id"] = " EXTRACTOR-FIXTURE "
        with self.assertRaisesRegex(evaluation.EvaluationError, "distinct declared identity"):
            self.score()
        reviewer["id"] = "independent-review-fixture"
        reviewer["human_review"] = "complete"
        with self.assertRaisesRegex(evaluation.EvaluationError, "AI reviewer"):
            self.score()

    def test_preserved_source_section_accepts_markdown_line_endings(self):
        path = self.notes_root / "处理范围.md"
        original = path.read_bytes().decode("utf-8")
        for newline in ("\r\n", "\r"):
            with self.subTest(newline=repr(newline)):
                path.write_bytes(original.replace("\n", newline).encode("utf-8"))
                self.refresh_native_pins()
                self.pin_review()
                report = self.score()
                self.assertEqual(report["status"], "evaluated")
                self.assertEqual(report["inventory"]["unknown"]["preserved_unresolved_ids"], ["U1"])
                self.assertFalse(report["gate_passed"])

    def test_preserved_source_bindings_decode_commonmark_ascii_punctuation(self):
        path = self.notes_root / "处理范围.md"
        original_notice = path.read_text(encoding="utf-8")
        location_path = self.notes_root / ".noetome-index/source-locations.ndjson"
        location = json.loads(location_path.read_text(encoding="utf-8"))
        punctuation = "".join(chr(code) for code in range(33, 127) if not chr(code).isalnum())
        cases = [("REVIEW" + punctuation + "END", "REVIEW" + "".join("\\" + char for char in punctuation) + "END"),
                 ("REVIEW_%&|~(A)", r"REVIEW\_\%\&\|\~\(A\)"),
                 ("-REVIEW", r"\-REVIEW"), ("+REVIEW", r"\+REVIEW"),
                 ("=REVIEW", r"\=REVIEW"), ("12.REVIEW", r"12\.REVIEW")]
        for code, encoded in cases:
            with self.subTest(code=code):
                location["source_issues"][0]["code"] = code
                location_path.write_text(json.dumps(location, ensure_ascii=False) + "\n", encoding="utf-8")
                path.write_text(original_notice.replace(r"ORIGINAL\_REVIEW", encoded), encoding="utf-8")
                for row in self.review["inventory"]:
                    for binding in row["source_bindings"]:
                        binding["issue_codes"] = [code]
                self.refresh_native_pins()
                self.pin_review()
                report = self.score()
                self.assertEqual(report["status"], "evaluated")
                self.assertEqual(report["inventory"]["unknown"]["preserved_unresolved_ids"], ["U1"])
                self.assertEqual(report["inventory"]["unknown"]["numerator"], 0)
                self.assertFalse(report["gate_passed"])

    def test_preserved_source_bindings_do_not_unescape_letters(self):
        path = self.notes_root / "处理范围.md"
        path.write_text(path.read_text(encoding="utf-8").replace(r"ORIGINAL\_REVIEW", r"ORIGINAL_\REVIEW"),
                        encoding="utf-8")
        self.refresh_native_pins()
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "page heading, actual page SHA"):
            self.score()

    def test_generic_unresolved_notice_cannot_claim_original_source_retention(self):
        (self.notes_root / "处理范围.md").write_text("图形和三维结构仍待复核。\n", encoding="utf-8")
        self.refresh_native_pins()
        self.pin_review()
        with self.assertRaisesRegex(evaluation.EvaluationError, "page heading, actual page SHA"):
            self.score()

    def test_preserved_source_binding_cannot_use_a_different_physical_page(self):
        self.review["inventory"][2]["source_bindings"][0]["page_number"] = 35
        with self.assertRaisesRegex(evaluation.EvaluationError, "physical page mismatch"):
            self.score()

    def test_preserved_source_binding_needs_original_issue_codes(self):
        self.review["inventory"][2]["source_bindings"][0]["issue_codes"] = ["INVENTED_ISSUE"]
        with self.assertRaisesRegex(evaluation.EvaluationError, "needs original source issues"):
            self.score()

    def test_preserved_source_binding_requires_all_reference_pages(self):
        item = self.reference["unknowns"][0]
        item["pages"] = [34, 35]
        self.review["inventory"][2]["reference_item_sha256"] = evaluation.canonical_sha256(item)
        with self.assertRaisesRegex(evaluation.EvaluationError, "do not cover all reference physical pages"):
            self.score()

    def test_source_fingerprint_and_issue_must_appear_inside_the_correct_page_section(self):
        path = self.notes_root / "处理范围.md"
        for level in range(1, 7):
            with self.subTest(next_page_heading_level=level):
                path.write_text("# 处理范围\n\n### 物理页 34\n仅有通用通知。\n\n"
                                + "#" * level + " 物理页 35\n" + "3" * 64 + "\nORIGINAL_REVIEW\n", encoding="utf-8")
                self.refresh_native_pins()
                self.pin_review()
                with self.assertRaisesRegex(evaluation.EvaluationError, "page heading, actual page SHA"):
                    self.score()

    def test_preserved_source_binding_cannot_be_omitted(self):
        del self.review["inventory"][2]["source_bindings"]
        with self.assertRaisesRegex(evaluation.EvaluationError, "source_bindings"):
            self.score()

    def test_native_original_source_location_cannot_forge_page_fingerprint(self):
        path = self.notes_root / ".noetome-index/source-locations.ndjson"
        location = json.loads(path.read_text(encoding="utf-8"))
        location["page_sha256"] = "4" * 64
        path.write_text(json.dumps(location) + "\n", encoding="utf-8")
        self.refresh_native_pins()
        with self.assertRaisesRegex(evaluation.EvaluationError, "source location/page fingerprint mismatch"):
            self.score()

    def test_unknown_cannot_be_promoted_or_preserved_without_actual_notice(self):
        row = self.review["inventory"][2]
        row["disposition"] = "resolved"
        with self.assertRaisesRegex(evaluation.EvaluationError, "frozen open unknown"):
            self.score()
        row["disposition"] = "preserved_unresolved"
        row["note_anchors"] = []
        with self.assertRaisesRegex(evaluation.EvaluationError, "actually appear in notes"):
            self.score()

    def test_effective_errata_only_changes_exact_old_claim_and_preserves_metadata(self):
        original = copy.deepcopy(self.reference)
        errata = copy.deepcopy(self.errata)
        errata["entries"] = [{"atom_id": "A3", "original_claim": self.quotes[2],
                              "corrected_claim": "A synthetic document used only for unit testing."}]
        effective = evaluation.apply_errata(self.reference, errata)
        self.assertEqual(self.reference, original)
        self.assertEqual(effective["atoms"][2]["claim"], errata["entries"][0]["corrected_claim"])
        self.assertEqual(effective["atoms"][2]["conditions"], original["atoms"][2]["conditions"])
        self.assertEqual(effective["annotator"], "ai")
        errata["entries"][0]["original_claim"] = "stale reference claim"
        with self.assertRaisesRegex(evaluation.EvaluationError, "original claim mismatch"):
            evaluation.apply_errata(self.reference, errata)
        errata["base_reference_sha256"] = "0" * 64
        with self.assertRaisesRegex(evaluation.EvaluationError, "base reference SHA mismatch"):
            evaluation.apply_errata(self.reference, errata)

    def test_public_loader_rejects_forged_reference_and_cli_writes_failure_report(self):
        reference_path, errata_path = self.root / "forged.json", self.root / "errata.json"
        reference_path.write_text(json.dumps(self.reference), encoding="utf-8")
        errata_path.write_text(json.dumps(self.errata), encoding="utf-8")
        with self.assertRaisesRegex(evaluation.EvaluationError, "fixed reference SHA mismatch"):
            evaluation.load_reference(reference_path, errata_path)
        output = self.root / "report.json"
        with contextlib.redirect_stdout(io.StringIO()):
            code = evaluation.main(["--reference", str(reference_path), "--errata", str(errata_path),
                                    "--candidate", str(self.root / "unused-candidate.json"),
                                    "--review", str(self.root / "unused-review.json"),
                                    "--notes-root", str(self.notes_root), "--output", str(output)])
        self.assertEqual(code, 2)
        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "invalid_input")
        self.assertFalse(report["gate_passed"])
        self.assertFalse(report["scientific_pass_claim"])

    def test_duplicate_and_nonfinite_json_fail_closed(self):
        for data in (b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":Infinity}', b'{"x":1e999}'):
            with self.subTest(data=data), self.assertRaises(evaluation.EvaluationError):
                evaluation._decode_json(data, "bad fixture")

    def test_canonical_hash_is_unicode_preserving_and_full_payload_sensitive(self):
        left = {"字": "ΔG − TΔS", "measurement": {"unit": "kJ", "value": 2}}
        right = {"measurement": {"value": 2, "unit": "kJ"}, "字": "ΔG − TΔS"}
        self.assertEqual(evaluation.canonical_sha256(left), evaluation.canonical_sha256(right))
        right["measurement"]["unit"] = "J"
        self.assertNotEqual(evaluation.canonical_sha256(left), evaluation.canonical_sha256(right))

    @unittest.skipUnless(evaluation.DEFAULT_REFERENCE.is_file() and evaluation.DEFAULT_ERRATA.is_file(),
                         "fixed B19 reference artifacts not installed in this checkout")
    def test_real_frozen_reference_pins_and_separate_a162_errata(self):
        original = evaluation.DEFAULT_REFERENCE.read_bytes()
        raw = json.loads(original)
        effective, errata = evaluation.load_reference()
        self.assertEqual(evaluation.sha256(original), evaluation.REFERENCE_SHA256)
        self.assertEqual(evaluation.sha256(evaluation.DEFAULT_ERRATA.read_bytes()), evaluation.ERRATA_SHA256)
        self.assertEqual(len(effective["atoms"]), 173)
        raw_atom = next(atom for atom in raw["atoms"] if atom["id"] == "A162")
        corrected = next(atom for atom in effective["atoms"] if atom["id"] == "A162")
        self.assertEqual(raw_atom["claim"], "特定基因组可响应激素而表达，同一细胞无激素时不表达。")
        self.assertEqual(corrected["claim"], "特定的一组基因可响应激素而表达，同一细胞无激素时不表达。")
        self.assertEqual(errata["human_review"], "pending")
        self.assertEqual(evaluation.DEFAULT_REFERENCE.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
