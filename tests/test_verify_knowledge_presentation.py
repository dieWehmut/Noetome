"""Independent author and read-back regressions for active presentation."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_knowledge_presentation", ROOT / "scripts/verify_knowledge_presentation.py")
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class PresentationAuthorTests(unittest.TestCase):
    def setUp(self):
        self.fixture = ROOT / "fixtures/active-presentation-v1"
        self.authors = json.loads((self.fixture / "authors.json").read_bytes())

    def test_all_frozen_cases_rebuild_cells_without_a_note_plan(self):
        for case in self.authors["cases"]:
            with self.subTest(case=case["name"]):
                expected = json.loads((self.fixture / case["presentation_expected_file"]).read_bytes())
                VERIFY.verify_author_semantics(case, expected)

    def test_fixture_expected_loads_its_author_gold_without_rust_outputs(self):
        for folder in (self.fixture, ROOT / "fixtures/active-presentation-legacy-v1", ROOT / "fixtures/active-presentation-cross-role-v1"):
            authors = json.loads((folder / "authors.json").read_bytes())
            for case in authors["cases"]:
                with self.subTest(case=case["name"]):
                    expected, gold, author = VERIFY.load_expectation(folder / case["presentation_expected_file"])
                    self.assertEqual(json.loads(gold), case["coordination_expected"])
                    self.assertEqual(author, case)
                    self.assertEqual(expected["case_name"], case["name"])

    def test_shared_concept_can_have_different_roles_in_distinct_atoms(self):
        fixture = ROOT / "fixtures/active-presentation-cross-role-v1"
        case = json.loads((fixture / "authors.json").read_bytes())["cases"][0]
        expected = json.loads((fixture / case["presentation_expected_file"]).read_bytes())
        VERIFY.verify_author_semantics(case, expected)
        facts = case["coordination_expected"]["facts"]
        self.assertEqual(facts[0]["object_id"], facts[1]["property_id"])
        representation, decision = VERIFY.choose(facts)
        self.assertEqual(representation, "paragraph"); self.assertEqual(decision["shared_property_ids"], [])
        self.assertEqual(len(expected["cells"]), 2)

    def test_second_condition_history_and_unit_are_not_optional(self):
        for name, field in [("two_objects", "conditions"), ("history_cell", "time"),
                            ("mixed_units", "unit"), ("two_objects", "exceptions")]:
            case = next(c for c in self.authors["cases"] if c["name"] == name)
            expected = json.loads((self.fixture / case["presentation_expected_file"]).read_bytes())
            changed = copy.deepcopy(expected)
            fact = changed["cells"][0]["facts"][0]
            fact[field] = {"status": "unknown", "precision": None, "value": None} if field == "time" else [] if type(fact[field]) is list else "0"
            with self.subTest(field=field), self.assertRaises(AssertionError):
                VERIFY.verify_author_semantics(case, changed)

    def test_multi_context_cell_keeps_every_fact_group(self):
        case = next(c for c in self.authors["cases"] if c["name"] == "multi_context_cell")
        expected = json.loads((self.fixture / case["presentation_expected_file"]).read_bytes())
        self.assertEqual(len(expected["cells"][0]["facts"]), 2)
        changed = copy.deepcopy(expected)
        changed["cells"][0]["facts"].pop()
        with self.assertRaises(AssertionError):
            VERIFY.verify_author_semantics(case, changed)

    def test_missing_is_distinct_from_zero_and_not_applicable(self):
        case = next(c for c in self.authors["cases"] if c["name"] == "sparse_properties")
        expected = json.loads((self.fixture / case["presentation_expected_file"]).read_bytes())
        missing = next(c for c in expected["cells"] if c["state"] == "missing")
        self.assertEqual(missing["facts"], [])
        missing["missing_meaning"] = "not_applicable"
        with self.assertRaises(AssertionError):
            VERIFY.verify_author_semantics(case, expected)

    def test_expected_rejects_unknown_fields_and_boolean_version(self):
        raw = json.loads((self.fixture / "expected-two_objects.json").read_bytes())
        for field, value in [("schema_version", True), ("published", "false"),
                             ("scientific_quality", "passed"), ("unknown_field", 0)]:
            changed = copy.deepcopy(raw); changed[field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                VERIFY.expected_schema(changed)

    def test_legacy_authors_preserve_raw_sources_and_version_context(self):
        fixture = ROOT / "fixtures/active-presentation-legacy-v1"
        authors = json.loads((fixture / "authors.json").read_bytes())
        for case in authors["cases"]:
            expected = json.loads((fixture / case["presentation_expected_file"]).read_bytes())
            VERIFY.verify_author_semantics(case, expected)
            version = case["identity"]["schema_version"]
            if version == 1:
                self.assertNotIn("atom_contexts", case["identity"])
                self.assertIsNone(expected["cells"][0]["facts"][0]["time"])
            else:
                self.assertEqual(expected["cells"][0]["facts"][0]["time"]["value"], "2020")
            for item in case["inputs"]:
                origin = authors["source_origins"][item["source_file"]]
                self.assertEqual((fixture / item["source_file"]).read_bytes(), (ROOT / origin["repository_path"]).read_bytes())
            self.assertEqual(case["inputs"][0]["additional_evidence"][0]["relation"], "qualifies")

    def test_ancestor_link_is_rejected_before_any_read_or_resolution(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); real = root / "real"; real.mkdir()
            target = real / "expected.json"; target.write_text("human sentinel")
            link = root / "linked"
            try:
                link.symlink_to(real, target_is_directory=True)
            except (OSError, NotImplementedError):
                self.skipTest("directory symbolic links unavailable")
            with self.assertRaises(AssertionError):
                VERIFY.guard_path(link / "expected.json", directory=False)
            with self.assertRaises(AssertionError):
                VERIFY.guard_path(link / ".." / "real", directory=True)
            self.assertEqual(target.read_text(), "human sentinel")

    def test_hardlinked_expected_cannot_be_adopted(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); source = root / "a.json"; source.write_text("protected")
            alias = root / "b.json"
            try:
                os.link(source, alias)
            except OSError:
                self.skipTest("hard links unavailable")
            with self.assertRaises(AssertionError):
                VERIFY.guard_path(alias, directory=False)


class PresentationRebuildTests(unittest.TestCase):
    @staticmethod
    def inputs(version=3):
        object_a, object_b, prop = [f"60000000-0000-4000-8000-00000000000{i}" for i in (1, 2, 3)]
        coordination = dict(schema_version=version, project_id="70000000-0000-4000-8000-000000000001",
            canonical_concepts=[dict(canonical_id=object_a, display_name="甲 | A"), dict(canonical_id=object_b, display_name="乙"), dict(canonical_id=prop, display_name="速度")],
            facts=[], occurrences=[])
        identity = dict(atom_contexts=[]); candidates = {}
        for input_id, object_id, value, condition, year in [("a", object_a, "10", "温度 20 ℃", "2020"),
                                                            ("b", object_a, "12", "温度 30 ℃", "2021"),
                                                            ("c", object_b, "11", "温度 20 ℃", "2020")]:
            atom = dict(atom=dict(atom_id=input_id + "-speed", content="原句 " + value,
                evidence_links=[dict(evidence_id="support", relation="supports"), dict(evidence_id="counter", relation="refutes"),
                                dict(evidence_id="support", relation="qualifies")]), measurement=dict(value_status="known", value=value, unit="m/s", conditions=[condition]))
            candidates[input_id] = dict(payload=dict(atoms=[atom], concepts=[dict(concept_id=input_id + "-subject", canonical_name="原对象")],
                evidence=[dict(evidence_id="support", quote="含支持原文", excerpt_start_byte=0, excerpt_end_byte=15),
                          dict(evidence_id="counter", quote="含限定反证", excerpt_start_byte=15, excerpt_end_byte=30)]))
            context = dict(applicability_status="known", time=dict(status="known", precision="year", value=year))
            occurrence = dict(occurrence_id="occ-" + input_id, input_id=input_id, object_id=object_id, property_id=prop,
                binding=dict(source_id="source-" + input_id, version_id="version-" + input_id, parse_id=None),
                unit=dict(unit_id="unit-0", start_byte=0, end_byte=30, text="含支持原文含限定反证"),
                original_atom=atom, value=value, unit_value="m/s", conditions=[condition, "成年样例"], exceptions=["结冰时不适用"])
            if version != 1:
                occurrence["context"] = context
                identity["atom_contexts"].append(dict(input_id=input_id, original_atom_id=input_id + "-speed", **context, context_evidence_ids=["support"]))
            coordination["occurrences"].append(occurrence)
            fact = dict(fact_id="fact-" + input_id, object_id=object_id, property_id=prop, occurrence_ids=["occ-" + input_id],
                        value=value, unit="m/s", conditions=[condition.replace(" ", ""), "成年样例"], exceptions=["结冰时不适用"])
            if version != 1:
                fact.update(context)
            coordination["facts"].append(fact)
        return coordination, identity, candidates

    def test_full_raw_evidence_and_context_groups_are_rebuilt(self):
        data = self.inputs()
        plan = VERIFY.rebuild_plan(*data, "request", dict(sha256="0" * 64, byte_length=1))
        self.assertEqual(plan["facts"][0]["conditions"], ["温度 20 ℃", "成年样例"])
        self.assertEqual(plan["facts"][1]["time"]["value"], "2021")
        source = plan["provenance"][0]
        self.assertEqual([e["evidence_id"] for e in source["evidence"]], ["support", "counter"])
        self.assertEqual([l["relation"] for l in source["original_atom"]["atom"]["evidence_links"]], ["supports", "refutes", "qualifies"])
        cells = plan["notes"][0]["nodes"][0]["rows"][0]["cells"]
        self.assertEqual(cells[1]["fact_ids"], ["fact-a", "fact-b"])
        self.assertIn("2020（年）", cells[1]["text"]); self.assertIn("2021（年）", cells[1]["text"])
        self.assertEqual(plan["provenance"][0]["context"], data[1]["atom_contexts"][0])

    def test_v1_keeps_context_absent_and_includes_counter_evidence(self):
        plan = VERIFY.rebuild_plan(*self.inputs(1), "request", dict(sha256="0" * 64, byte_length=1))
        self.assertIsNone(plan["facts"][0]["applicability_status"])
        self.assertIsNone(plan["facts"][0]["time"])
        self.assertIsNone(plan["provenance"][0]["context"])
        value = VERIFY.fact_text(plan["facts"][0])
        self.assertIn("适用背景：本协调版本未声明", value)
        self.assertIn("观点时间：本协调版本未声明", value)
        self.assertEqual(len(plan["provenance"][0]["evidence"]), 2)

    def test_literal_codec_and_utf8_ranges_keep_links_separate(self):
        plan = VERIFY.rebuild_plan(*self.inputs(), "request", dict(sha256="0" * 64, byte_length=1))
        raw, rendered = VERIFY.render_independent(plan)["知识比较.md"]
        self.assertTrue(raw.startswith("# 知识比较\n\n## 比较概览\n\n| 对象 | 速度 |\n| --- | --- |\n".encode()))
        self.assertIn("甲 \\| A [[对象-60000000-0000-4000-8000-000000000001#知识明细\\|查看详情]]".encode(), raw)
        self.assertEqual(VERIFY.encode_text("字|<br>\\\n*"), "字\\|\\<br\\>\\\\<br>\\*")
        label = next(m for m in rendered["content_mappings"] if m["scope"]["kind"] == "label")
        self.assertEqual(raw[label["start_byte"]:label["end_byte"]].decode(), "甲 \\| A")
        link = rendered["links"][0]
        self.assertGreater(link["start_byte"], label["end_byte"])
        self.assertTrue(raw[link["start_byte"]:link["end_byte"]].decode().startswith("[[对象-"))
        for mapping in rendered["content_mappings"] + rendered["links"]:
            raw[:mapping["start_byte"]].decode(); raw[:mapping["end_byte"]].decode()

    def test_audit_admission_preserves_existing_root(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); cli = root / "cli"; cli.write_text("test executable sentinel")
            existing = root / "existing"; existing.mkdir(); sentinel = existing / "human.txt"; sentinel.write_text("keep")
            with self.assertRaises(AssertionError):
                VERIFY.PresentationAudit(cli, existing)
            self.assertEqual(list(existing.iterdir()), [sentinel]); self.assertEqual(sentinel.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
