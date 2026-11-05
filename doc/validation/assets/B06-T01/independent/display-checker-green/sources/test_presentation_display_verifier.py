"""Real coordinated multiline labels and narrowly scoped display-text admission."""
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_knowledge_presentation as VERIFY
from verify_knowledge_coordination import metadata


class DisplayTextBoundaryTests(unittest.TestCase):
    def expected(self, label):
        expected = json.loads((ROOT / "fixtures/active-presentation-v1/expected-two_objects.json").read_bytes())
        expected["objects"][0]["label"] = label
        return expected

    def test_display_labels_preserve_finite_multiline_text(self):
        for label in ("样例\nA\t（两行标签）", "甲 | [乙]", "x < 3 > 1", "a\\b * c", "名称\u2028换行"):
            with self.subTest(label=label):
                VERIFY.expected_schema(self.expected(label))

    def test_display_labels_reject_control_and_hidden_or_link_syntax(self):
        for label in ("", " \n\t", "甲\r乙", "甲\x00乙", "甲\x7f乙", "甲\x85乙", "甲\x9f乙",
                      "a" * 1025, "%%hidden%%", "[[other]]", "![image]", "<br>", "</x>", "<!--x-->",
                      "<?x>", "[x] (target)", "[x]\n[ref]", "[x]\t: target", "<123@example>"):
            with self.subTest(label=label), self.assertRaises(AssertionError):
                VERIFY.expected_schema(self.expected(label))

    def test_metadata_exception_only_applies_to_explicit_identity_display_path(self):
        mapping = {"concepts": [{"display_name": "样例\nA\t"}]}
        metadata(mapping, identity_display_labels=True)
        with self.assertRaises(AssertionError):
            metadata(mapping)
        for mapping in ({"author": "人\n名"}, {"concepts": [{"canonical_id": "id\n"}]},
                        {"concept_mappings": [{"original_concept_id": "id\t"}]},
                        {"concepts": {"display_name": "名\n称"}},
                        {"nested": {"concepts": [{"display_name": "名\n称"}]}},
                        {"payload": {"concepts": [{"display_name": "名\n称"}]}},
                        {"concepts": [{"display_name": "名\r称"}]}):
            with self.subTest(mapping=mapping), self.assertRaises(AssertionError):
                metadata(mapping, identity_display_labels=True)


class MultilinePresentationCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Keep all CLI inputs, outputs and failures. An independent binary copy
        # avoids interpreting Cargo's own build hardlink as a user input alias.
        cls.root = ROOT / "target" / ("presentation-display-python-" + uuid.uuid4().hex)
        cls.root.mkdir(parents=True)
        executable = Path(os.environ.get("NOETOME_TEST_BINARY", str(ROOT / "target/debug" / ("noetome.exe" if os.name == "nt" else "noetome"))))
        cls.cli = cls.root / ("noetome.exe" if os.name == "nt" else "noetome")
        shutil.copyfile(executable, cls.cli)
        cls.cli.chmod(0o755)
        cls.audit = VERIFY.PresentationAudit(cls.cli, cls.root / "audit")
        cls.prepared = {}
        for version, name in ((1, "legacy_v1_two_objects"), (2, "legacy_v2_two_objects"), (3, "two_objects")):
            author, package = next((a, p) for a, p in cls.audit.cases if a["name"] == name)
            folder, project, original = cls.audit.prepare(author, package)
            case = folder / "relabeled-case"
            shutil.copytree(original, case)
            identity = VERIFY.strict_json((case / "identity.json").read_bytes())
            expected = VERIFY.strict_json(cls.audit.packages[package][author["presentation_expected_file"]])
            object_id = expected["objects"][0]["object_id"]
            label = f"样例\nA\t（版本 {version}）"
            next(c for c in identity["concepts"] if c["canonical_id"] == object_id)["display_name"] = label
            expected["objects"][0]["label"] = label
            (case / "identity.json").write_bytes(VERIFY.encoded(identity))
            expectation = folder / "relabeled-expectation"
            expectation.mkdir()
            # The independent unchanged source/atom gold and explicit new label
            # are saved before either ordinary coordination or presentation.
            (expectation / "coordination-expected.json").write_bytes(VERIFY.encoded(author["coordination_expected"]))
            (expectation / "expected.json").write_bytes(VERIFY.encoded(expected))
            cls.audit.write_new(expectation / "frozen-label-pins.json", {
                "phase": "before_coordinate_and_present", "label": label,
                "identity": VERIFY.fingerprint((case / "identity.json").read_bytes()),
                "expected": VERIFY.fingerprint((expectation / "expected.json").read_bytes()),
                "coordination_expected": VERIFY.fingerprint((expectation / "coordination-expected.json").read_bytes()),
            })
            cls.audit.protect(case); cls.audit.protect(expectation)
            coordination, preview = folder / "relabeled-coordination", folder / "relabeled-preview"
            cls.audit.call(name + "-multiline-coordinate", ["knowledge", "coordinate", project, case, coordination])
            cls.audit.protect(coordination)
            cls.audit.call(name + "-multiline-present", ["knowledge", "present", project, case, coordination, preview])
            cls.audit.protect(preview)
            cls.prepared[version] = project, case, coordination, preview, expectation / "expected.json"
        print("Retained multiline presentation verifier root:", cls.root)

    def check_multiline_version(self, version):
        project, case, coordination, preview, expected = self.prepared[version]
        gold = (expected.parent / "coordination-expected.json").read_bytes()
        checked_coordination = VERIFY.verify_coordination(project, case, coordination, gold)
        checked = VERIFY.verify_report(project, case, coordination, preview, expected)
        self.assertEqual(checked_coordination["status"], "passed")
        self.assertEqual(checked["status"], "passed")
        self.assertTrue(checked["independently_verified"])
        plan = VERIFY.strict_json((preview / "plan.json").read_bytes())
        self.assertEqual(plan["coordination_schema_version"], version)
        detail = next(n for n in plan["notes"] if n["title"] == f"样例\nA\t（版本 {version}）")
        self.assertIn(f"# 样例<br>A&#9;（版本 {version}）\n", (preview / detail["output_path"]).read_text(encoding="utf-8"))
        self.audit.write_new(preview.parent / "multiline-independent-check.json", dict(coordination=checked_coordination, presentation=checked))
        self.audit.check_protected()

    def test_multiline_v1_independent_check(self):
        self.check_multiline_version(1)

    def test_multiline_v2_independent_check(self):
        self.check_multiline_version(2)

    def test_multiline_v3_independent_check(self):
        self.check_multiline_version(3)


if __name__ == "__main__":
    unittest.main()
