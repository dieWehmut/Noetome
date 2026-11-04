"""Frozen authors and real CLI outputs with coherent conflict report forgeries."""
import copy
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_knowledge_coordination import verify_report, strict_json, fingerprint, encoded
from verify_knowledge_conflicts import decimal_value
from knowledge_conflicts_audit import (
    ConflictsAudit, rewrite_output, forgery_cases, forged_case,
    context_declaration_forgeries, markdown_preamble,
)


class ConflictsVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = ROOT / "target" / ("python-conflicts-" + uuid.uuid4().hex)
        cli = Path(os.environ.get("NOETOME_TEST_BINARY", str(ROOT / "target/debug" / ("noetome.exe" if os.name == "nt" else "noetome"))))
        cls.audit = ConflictsAudit(cli, cls.root)
        cls.prepared = {}
        for author in strict_json(cls.audit.authors["authors.json"])["cases"]:
            name = author["name"]
            folder, project, case, gold = cls.audit.prepare(name)
            output = folder / "output"
            cls.audit.call(name, ["knowledge", "coordinate", "--", project, case, output],
                           0 if author["expected"]["expected_status"] == "passed" else 3)
            cls.prepared[name] = folder, project, case, output, gold
        for name, kwargs in (("multiple-supports", dict(multiple=True)), ("ordering-display", dict(reorder=True)), ("dual-relation", dict(dual_relation=True))):
            folder, project, case, gold = cls.audit.prepare(name, "true_conflict", **kwargs)
            output = folder / "output"
            cls.audit.call(name, ["knowledge", "coordinate", "--", project, case, output], 3)
            cls.prepared[name] = folder, project, case, output, gold
        folder, project, case, gold = cls.audit.prepare("qualifier-link", "counter_support", qualifier=True)
        output = folder / "output"
        cls.audit.call("qualifier-link", ["knowledge", "coordinate", "--", project, case, output], 3)
        cls.prepared["qualifier-link"] = folder, project, case, output, gold
        print("Retained actual v3 test root:", cls.root)

    def test_all_frozen_authors_reconstructed(self):
        for name, (_, project, case, output, gold) in self.prepared.items():
            with self.subTest(name=name):
                checked = verify_report(project, case, output, gold)
                self.assertTrue(checked["independently_verified"])
        self.audit.check_protected()

    def test_conflict_keeps_two_sides_and_blocks_success_ir(self):
        _, project, case, output, gold = self.prepared["true_conflict"]
        checked = verify_report(project, case, output, gold)
        report = strict_json((output / "coordination.json").read_bytes())
        self.assertEqual(checked["status"], "needs_review")
        self.assertEqual({f["value"] for f in report["facts"]}, {"10", "12"})
        self.assertEqual(report["relations"][0]["relation"], "true_conflict")
        self.assertEqual(len(report["processing_records"]), 2)
        self.assertFalse((output / "coordination-ir.json").exists())

    def test_majority_does_not_choose_winner_and_mixed_pass_is_retained(self):
        for name, expected_pass in (("majority_conflict", 0), ("mixed_conflict", 1)):
            _, project, case, output, gold = self.prepared[name]
            checked = verify_report(project, case, output, gold)
            self.assertEqual(checked["totals"]["passed"], expected_pass)
            self.assertEqual(checked["status"], "needs_review")
            report = strict_json((output / "coordination.json").read_bytes())
            self.assertEqual(len(report["occurrences"]), 3)
            self.assertEqual(len(report["processing_records"]), 3)

    def test_counter_evidence_and_history_retained(self):
        _, project, case, output, gold = self.prepared["counter_support"]
        checked = verify_report(project, case, output, gold)
        self.assertEqual(checked["totals"]["counter_evidence_links"], 1)
        report = strict_json((output / "coordination.json").read_bytes())
        counter = next(o for o in report["occurrences"] if o["input_id"] == "a")
        self.assertEqual([e["evidence_id"] for e in counter["original_evidence"]], ["a-support", "a-refutes"])
        self.assertEqual(counter["context"]["evidence_ids"], ["a-support"])
        _, project, case, output, gold = self.prepared["history"]
        self.assertEqual(verify_report(project, case, output, gold)["totals"]["historical_relations"], 1)

    def test_same_evidence_multiple_relations_keeps_link_counts(self):
        _, project, case, output, gold = self.prepared["dual-relation"]
        checked = verify_report(project, case, output, gold)
        self.assertEqual(checked["totals"]["evidence_links"], 3)
        self.assertEqual(checked["totals"]["counter_evidence_links"], 1)
        report = strict_json((output / "coordination.json").read_bytes())
        a = next(o for o in report["occurrences"] if o["input_id"] == "a")
        self.assertEqual(len(a["original_evidence"]), 1)
        self.assertEqual([l["relation"] for l in a["original_atom"]["atom"]["evidence_links"]], ["supports", "refutes"])

    def test_qualifier_link_retained_with_unique_evidence(self):
        _, project, case, output, gold = self.prepared["qualifier-link"]
        checked = verify_report(project, case, output, gold)
        self.assertEqual(checked["totals"]["evidence_links"], 4)
        self.assertEqual(checked["totals"]["counter_evidence_links"], 1)
        report = strict_json((output / "coordination.json").read_bytes())
        a = next(o for o in report["occurrences"] if o["input_id"] == "a")
        self.assertEqual(len(a["original_evidence"]), 2)
        self.assertEqual({l["relation"] for l in a["original_atom"]["atom"]["evidence_links"]}, {"supports", "refutes", "qualifies"})

    def check_forgery(self, name, author, mutate):
        _, project, case, original, gold = self.prepared[author]
        output = self.root / (name + "-" + uuid.uuid4().hex)
        shutil.copytree(original, output)
        report = strict_json((output / "coordination.json").read_bytes())
        mutate(report); rewrite_output(output, report)
        md = (output / "coordination.md").read_bytes().split(b"```json\n")[1].split(b"\n```\n")[0]
        self.assertEqual(strict_json(md), report)
        self.assertTrue((output / "coordination.md").read_bytes().startswith(markdown_preamble(report).encode()))
        for name, pin in report["artifacts"].items():
            self.assertEqual(fingerprint((output / name).read_bytes()), pin)
        if report["status"] == "passed":
            ir = strict_json((output / "coordination-ir.json").read_bytes())
            for field in ("occurrences", "facts", "relations", "totals", "processing_records"):
                self.assertEqual(ir[field], report[field])
        with self.assertRaises(AssertionError) as failure:
            verify_report(project, case, output, gold)
        # Coherence must make semantic reconstruction the reason for rejection.
        self.assertNotIn("fingerprint", str(failure.exception))
        self.assertNotIn("Markdown", str(failure.exception))

    def test_coherent_deleted_opponent(self):
        self.check_forgery(*next(f for f in forgery_cases() if f[0] == "opponent-omitted"))

    def test_coherent_forgery_matrix(self):
        for values in forgery_cases():
            with self.subTest(name=values[0]):
                self.check_forgery(*values)

    def test_actual_context_declaration_and_source_guards(self):
        folder, project, _, _, gold = self.prepared["true_conflict"]
        for name, field, value in context_declaration_forgeries():
            with self.subTest(name=name):
                case, output = forged_case(folder, self.root / ("case-" + name), field, value)
                with self.assertRaises(AssertionError) as failure:
                    verify_report(project, case, output, gold)
                self.assertNotIn("saved original", str(failure.exception))
                self.assertNotIn("fingerprint", str(failure.exception))

    def test_unknown_time_is_null_and_false_zero_is_rejected(self):
        _, project, case, output, gold = self.prepared["undated_difference"]
        report = strict_json((output / "coordination.json").read_bytes())
        self.assertTrue(all(o["context"]["time"]["value"] is None for o in report["occurrences"]))
        self.assertEqual(verify_report(project, case, output, gold)["totals"]["insufficient_evidence_relations"], 1)
        self.check_forgery(*next(f for f in forgery_cases() if f[0] == "default-zero"))

    def test_visible_preamble_counts_suffix_and_science(self):
        _, project, case, original, gold = self.prepared["true_conflict"]
        for mutation in (lambda raw: raw.replace("真实冲突关系：1".encode(), "真实冲突关系：0".encode()),
                         lambda raw: raw + "\n发布成功。\n".encode(),
                         lambda raw: raw.replace(b"not_run", b"passed", 1)):
            output = self.root / ("visible-" + uuid.uuid4().hex)
            shutil.copytree(original, output)
            md = output / "coordination.md"; md.write_bytes(mutation(md.read_bytes()))
            with self.assertRaisesRegex(AssertionError, "Markdown"):
                verify_report(project, case, output, gold)

    def test_strict_schema_counts_and_processing_fields(self):
        _, project, case, original, gold = self.prepared["true_conflict"]
        for mutate in (lambda r: r.update(schema_version=True), lambda r: r["totals"].update(true_conflicts=False),
                       lambda r: r["processing_records"][0].update(extra="ignored"),
                       lambda r: r["processing_records"][0].pop("reason")):
            output = self.root / ("strict-" + uuid.uuid4().hex)
            shutil.copytree(original, output)
            report = strict_json((output / "coordination.json").read_bytes())
            mutate(report); rewrite_output(output, report)
            with self.assertRaises(AssertionError):
                verify_report(project, case, output, gold)

    def test_unrelated_complement_not_marked_as_conflicted(self):
        _, project, case, output, gold = self.prepared["unknown_complement"]
        checked = verify_report(project, case, output, gold)
        self.assertEqual(checked["totals"]["passed"], 1)
        self.assertEqual(checked["totals"]["needs_review"], 1)
        self.assertEqual(checked["totals"]["insufficient_evidence_relations"], 0)
        report = strict_json((output / "coordination.json").read_bytes())
        self.assertEqual(report["relations"][0]["relation"], "complement")

    def test_registered_source_bytes_are_read_and_protected(self):
        folder, _, _, _, gold = self.prepared["true_conflict"]
        destination = self.root / ("source-tamper-" + uuid.uuid4().hex)
        destination.mkdir()
        for name in ("project", "case", "output"):
            shutil.copytree(folder / name, destination / name)
        project = destination / "project"
        selected = strict_json((destination / "case/coordination.json").read_bytes())["inputs"][0]["binding"]["version_id"]
        with sqlite3.connect((project / "project.sqlite").as_uri() + "?mode=ro&immutable=1", uri=True) as db:
            relative = db.execute("SELECT normalized_snapshot FROM source_versions WHERE version_id=?", (selected,)).fetchone()[0]
        resource = project / relative
        resource.write_bytes(resource.read_bytes() + b"tampered source\n")
        with self.assertRaisesRegex(AssertionError, "source SHA"):
            verify_report(project, destination / "case", destination / "output", gold)
        self.audit.check_protected()

    @unittest.skipIf(os.name == "nt", "ancestor symlinks require separate Windows reparse environment")
    def test_ancestor_links_rejected_before_source_read(self):
        folder, project, case, output, gold = self.prepared["true_conflict"]
        alias = self.root / ("alias-parent-" + uuid.uuid4().hex)
        alias.symlink_to(folder, target_is_directory=True)
        for role in ("project", "case", "output"):
            with self.subTest(role=role):
                roots = dict(project=project, case=case, output=output)
                roots[role] = alias / role
                with self.assertRaisesRegex(AssertionError, "ancestor link"):
                    verify_report(roots["project"], roots["case"], roots["output"], gold)
        # A lexical symlink followed by '..' must not disappear under resolve.
        with self.assertRaisesRegex(AssertionError, "ancestor link"):
            verify_report(alias / ".." / folder.name / "project", case, output, gold)
        for script in ("verify_knowledge_coordination.py", "verify_knowledge_conflicts.py"):
            run = subprocess.run([sys.executable, str(ROOT / "scripts" / script), "check",
                                  str(project), str(case), str(output), str(alias / "gold.json")],
                                 capture_output=True, text=True, timeout=40)
            self.assertEqual(run.returncode, 1)
            self.assertIn("ancestor link", run.stderr)
        self.audit.check_protected()

    def test_exact_decimal_boundaries(self):
        self.assertEqual(decimal_value("+0010.000"), decimal_value("10"))
        self.assertEqual(decimal_value("-0.00"), decimal_value("+0"))
        self.assertNotEqual(decimal_value("9007199254740992"), decimal_value("9007199254740993"))
        self.assertNotEqual(decimal_value("0.00000000000000001"), decimal_value("0.00000000000000002"))
        for raw in ("1e3", "NaN", "inf", "1.", ".1", "１２", "1" * 1025):
            with self.subTest(raw=raw):
                self.assertIsNone(decimal_value(raw))


if __name__ == "__main__":
    unittest.main()
