"""Real source-review CLI fixtures plus synchronized v2 semantic forgeries."""
import copy
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_knowledge_coordination import verify_report, strict_json, fingerprint, encoded
from knowledge_variants_audit import (VariantsAudit, rewrite_output, forgery_cases,
                                     forged_case, context_declaration_forgeries)


class VariantsVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = ROOT / "target" / ("python-variants-" + uuid.uuid4().hex)
        cls.audit = VariantsAudit(ROOT / "target/debug/noetome.exe", cls.root)
        cls.prepared = {}
        for author in strict_json(cls.audit.authors["authors.json"])["cases"]:
            name = author["name"]
            folder, project, case, gold = cls.audit.prepare(name)
            output = folder / "output"
            cls.audit.call(name, ["knowledge", "coordinate", "--", project, case, output],
                           0 if author["expected"]["expected_status"] == "passed" else 3)
            cls.prepared[name] = folder, project, case, output, gold
        for name, kwargs in (("multiple-supports", dict(multiple=True)), ("ordering-display", dict(reorder=True))):
            folder, project, case, gold = cls.audit.prepare(name, "conditional", **kwargs)
            output = folder / "output"
            cls.audit.call(name, ["knowledge", "coordinate", "--", project, case, output])
            cls.prepared[name] = folder, project, case, output, gold
        (cls.root / "test-root.json").write_bytes(encoded(dict(root=str(cls.root))))
        print("Retained actual v2 test root:", cls.root)

    def test_conditional_report_matches_frozen_authors(self):
        _, project, case, output, gold = self.prepared["conditional"]
        self.assertTrue(verify_report(project, case, output, gold)["independently_verified"])

    def test_all_frozen_authors(self):
        for name, (_, project, case, output, gold) in self.prepared.items():
            with self.subTest(name=name):
                self.assertTrue(verify_report(project, case, output, gold)["independently_verified"])
        self.audit.check_protected()

    def test_coherent_deleted_condition_is_rejected(self):
        self.check_forgery(*forgery_cases()[0])

    def check_forgery(self, name, author, mutate):
        _, project, case, original, gold = self.prepared[author]
        output = self.root / (name + "-" + uuid.uuid4().hex)
        shutil.copytree(original, output)
        report = strict_json((output / "coordination.json").read_bytes())
        mutate(report); rewrite_output(output, report)
        # Prove agreement and fresh pins before asking the verifier to reject.
        md = (output / "coordination.md").read_bytes().split(b"```json\n")[1].split(b"\n```\n")[0]
        self.assertEqual(strict_json(md), report)
        for name, pin in report["artifacts"].items():
            self.assertEqual(fingerprint((output / name).read_bytes()), pin)
        with self.assertRaises(AssertionError):
            verify_report(project, case, output, gold)

    def test_coherent_semantic_matrix(self):
        for values in forgery_cases()[1:]:
            with self.subTest(name=values[0]):
                self.check_forgery(*values)

    def test_coherent_case_context_against_actual_source(self):
        folder, project, _, _, gold = self.prepared["conditional"]
        for name, field, value in context_declaration_forgeries():
            with self.subTest(name=name):
                case, output = forged_case(folder, self.root / ("case-" + name), field, value)
                with self.assertRaises(AssertionError) as failure:
                    verify_report(project, case, output, gold)
                self.assertNotIn("saved original", str(failure.exception))
                self.assertNotIn("fingerprints", str(failure.exception))

    def test_metadata_lf_opt_in_is_closed_and_v1_default_remains(self):
        from verify_knowledge_coordination import metadata, source_bundle, FILES
        candidate = {"payload": {"evidence": [{"quote": "原文\n事实"}]}}
        with self.assertRaises(AssertionError):
            metadata(candidate)
        metadata(candidate, evidence_quote_lf=True)
        for bad in ("原文\r\n事实", "原文\t事实", "原文\x00事实", "a" * 1025):
            with self.subTest(quote=bad):
                with self.assertRaises(AssertionError):
                    metadata({"payload": {"evidence": [{"quote": bad}]}}, evidence_quote_lf=True)
        for obj in ({"quote": "x\ny"}, {"payload": {"atoms": [{"quote": "x\ny"}]}},
                    {"payload": {"evidence": [{"unit_id": "x\ny"}]}}, {"author": "x\ny"}):
            with self.assertRaises(AssertionError):
                metadata(obj, evidence_quote_lf=True)
        _, project, case, _, _ = self.prepared["conditional"]
        bundle = case / "bundle-a"
        pins = {n: fingerprint((bundle / n).read_bytes()) for n in FILES}
        with self.assertRaisesRegex(AssertionError, "metadata limit/control"):
            source_bundle(project, bundle, pins)
        self.assertEqual(source_bundle(project, bundle, pins, evidence_quote_lf=True)[1]["status"], "passed")

    def test_visible_markdown_counts_and_suffix_are_verified(self):
        _, project, case, original, gold = self.prepared["conditional"]
        for index, mutation in enumerate((lambda r: r.replace("条件已列明：3".encode(), "条件已列明：0".encode()),
                                           lambda r: r + "\n发布：true。\n".encode(),
                                           lambda r: r.replace(b"not_run", b"passed", 1))):
            output = self.root / ("visible-" + str(index))
            shutil.copytree(original, output)
            md = output / "coordination.md"; md.write_bytes(mutation(md.read_bytes()))
            with self.assertRaisesRegex(AssertionError, "Markdown"):
                verify_report(project, case, output, gold)

    def test_strict_version_duplicate_json_and_bool_schema(self):
        folder, project, _, _, gold = self.prepared["conditional"]
        for name, version in (("unsupported", 3), ("boolean", True), ("float", 2.0), ("old-with-new-fields", 1)):
            case, output = forged_case(folder, self.root / ("schema-" + name), "time", dict(status="unknown", precision=None, value=None))
            path = case / "coordination.json"; manifest = strict_json(path.read_bytes()); manifest["schema_version"] = version
            raw = encoded(manifest); path.write_bytes(raw); (output / "coordination-original.json").write_bytes(raw)
            rewrite_output(output, strict_json((output / "coordination.json").read_bytes()))
            with self.assertRaises(AssertionError):
                verify_report(project, case, output, gold)
        case, output = forged_case(folder, self.root / "duplicate-key", "time", dict(status="unknown", precision=None, value=None))
        path = case / "coordination.json"
        path.write_bytes(path.read_bytes().replace(b'"schema_version": 2', b'"schema_version": 2, "schema_version": 2'))
        with self.assertRaises(AssertionError):
            verify_report(project, case, output, gold)

    def test_gregorian_ranges_and_normalization_boundaries(self):
        from verify_knowledge_variants import time_schema
        from verify_knowledge_coordination import normalized
        self.assertEqual(str(time_schema(dict(status="known", precision="day", value="2024-02-29"))[0]), "2024-02-29")
        for value in ("0000", "２０２４", "2024-13-01", "2023-02-29", "1900-02-29"):
            with self.assertRaises((AssertionError, ValueError)):
                time_schema(dict(status="known", precision="year" if len(value) == 4 else "day", value=value))
        self.assertEqual(normalized(" m\u3000/\t s "), "m/s")
        self.assertNotEqual(normalized("m/s"), normalized("M/s"))
        self.assertNotEqual(normalized("01"), normalized("1"))

    def test_top_level_case_types_use_assertion_protocol(self):
        folder, project, _, _, gold = self.prepared["conditional"]
        for index, document in enumerate(([], None, False, 2, "case")):
            case, output = forged_case(folder, self.root / ("top-level-" + str(index)), "time", dict(status="unknown", precision=None, value=None))
            (case / "coordination.json").write_bytes(encoded(document))
            with self.assertRaisesRegex(AssertionError, "case document object"):
                verify_report(project, case, output, gold)

    def test_source_mtime_cannot_create_time(self):
        folder, _, _, _, gold = self.prepared["conditional"]
        destination = self.root / "mtime-invariance"
        destination.mkdir()
        for name in ("project", "case", "output"):
            shutil.copytree(folder / name, destination / name)
        for path in (destination / "project").rglob("*"):
            if path.is_file():
                os.utime(path, (946684800, 946684800))
        result = verify_report(destination / "project", destination / "case", destination / "output", gold)
        self.assertEqual(result["totals"]["dated_occurrences"], 0)
        self.assertEqual(result["totals"]["undated_occurrences"], 3)

    def test_actual_context_adjacency_uniqueness_and_full_range(self):
        from verify_knowledge_variants import rebuild_context
        from verify_knowledge_coordination import source_bundle, FILES
        _, project, case, _, _ = self.prepared["conditional"]
        bundle = case / "bundle-a"
        pins = {n: fingerprint((bundle / n).read_bytes()) for n in FILES}
        _, _, candidate, gold, _, source = source_bundle(project, bundle, pins, evidence_quote_lf=True)
        declaration = strict_json((case / "identity.json").read_bytes())["atom_contexts"][0]
        evidence = candidate["payload"]["evidence"]
        context = rebuild_context(declaration, source, gold, evidence)
        unit = source["units"][0]
        self.assertEqual(context["quote"], unit["text"].encode()[context["start_byte"]-unit["start_byte"]:context["end_byte"]-unit["start_byte"]].decode())
        self.assertGreater(context["start_byte"], unit["start_byte"])
        for name, body in (("gap", unit["text"].replace("\n" + gold["quote"], "\n非相邻段落\n" + gold["quote"], 1)),
                           ("duplicate-fact", source["units"][0]["text"] + gold["quote"]),
                           ("invalid-calendar", source["units"][0]["text"].replace("未给出", "2023-02-29", 1))):
            cloned = copy.deepcopy(source); cloned["units"][0]["text"] = body
            with self.subTest(name=name), self.assertRaises((AssertionError, ValueError)):
                rebuild_context(declaration, cloned, gold, evidence)
        short = copy.deepcopy(evidence)
        short[0]["excerpt_start_byte"] = context["start_byte"] + 1
        with self.assertRaisesRegex(AssertionError, "background support set"):
            rebuild_context(declaration, source, gold, short)


if __name__ == "__main__":
    unittest.main()
