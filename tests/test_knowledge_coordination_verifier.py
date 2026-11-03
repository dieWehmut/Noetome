"""Coherent forgeries of actual ordinary CLI outputs must be refused."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_knowledge_coordination import verify_report


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def fp(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


class CoordinationVerifierTests(unittest.TestCase):
    def setUp(self):
        # Kept on disk, including failed tests; never delete replay evidence.
        self.root = Path(tempfile.mkdtemp(prefix="noetome-coordination-verifier-"))
        archive_path = ROOT / "doc/validation/assets/B05-T01-source-review-fixture.zip"
        self.assertEqual(fp(archive_path.read_bytes())["sha256"], "56226f12ff26f6e4199029e2c0230cfaecc5048b00272410b161bafc7e15682f")
        with zipfile.ZipFile(archive_path) as archive:
            self.assertEqual(len(archive.infolist()), 131)
            for entry in archive.infolist():
                name = Path(entry.filename)
                self.assertFalse(name.is_absolute() or ".." in name.parts or "\\" in entry.filename or ":" in entry.filename)
                target = self.root / name
                if entry.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(archive.read(entry))
        self.project, self.case, self.output = (self.root / x for x in ("project", "case", "output"))
        self.gold_bytes = (self.root / "gold.json").read_bytes()
        self.report = json.loads((self.output / "coordination.json").read_bytes())

    def rewrite(self):
        ir_path = self.output / "coordination-ir.json"
        if ir_path.exists():
            ir = json.loads(ir_path.read_bytes())
            for key in ir:
                if key in self.report:
                    ir[key] = copy.deepcopy(self.report[key])
            ir_path.write_bytes(encoded(ir))
        for name in self.report["artifacts"]:
            self.report["artifacts"][name] = fp((self.output / name).read_bytes())
        raw = encoded(self.report)
        (self.output / "coordination.json").write_bytes(raw)
        md = (self.output / "coordination.md").read_bytes().split(b"```json\n", 1)[0]
        (self.output / "coordination.md").write_bytes(md + b"```json\n" + raw.rstrip() + b"\n```\n")

    def test_baseline(self):
        self.assertEqual(verify_report(self.project, self.case, self.output, self.gold_bytes)["status"], "passed")

    def test_missing_second_occurrence_is_rejected(self):
        missing = next(o["occurrence_id"] for o in self.report["occurrences"] if o["input_id"] == "b")
        self.report["occurrences"] = [o for o in self.report["occurrences"] if o["occurrence_id"] != missing]
        self.report["inputs"] = [i for i in self.report["inputs"] if i["input_id"] != "b"]
        for fact in self.report["facts"]:
            fact["occurrence_ids"] = [x for x in fact["occurrence_ids"] if x != missing]
        self.report["relations"] = [r for r in self.report["relations"] if missing not in (r["left_occurrence_id"], r["right_occurrence_id"])]
        for key in ("input_bundles", "validated_input_bundles", "input_atoms", "source_occurrences", "evidence_links", "independent_sources", "passed"):
            self.report["totals"][key] = 2
        self.report["totals"]["duplicate_occurrences"] = 0
        self.rewrite()
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def refuse(self):
        self.rewrite()
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_coherent_wrong_fact_groups(self):
        for key, value in (("value", "11"), ("unit", "km/s"), ("conditions", []), ("exceptions", []), ("property_id", self.report["facts"][1]["property_id"])):
            with self.subTest(field=key):
                original = copy.deepcopy(self.report)
                self.report["facts"][0][key] = value
                self.refuse()
                self.report = original

    def test_coherent_original_objects_evidence_and_bindings(self):
        changes = [
            lambda o: o.update(original_concepts=[]),
            lambda o: o.update(original_evidence=[]),
            lambda o: o["original_atom"]["atom"].update(content="伪造"),
            lambda o: o["original_atom"]["measurement"].update(value="0"),
            lambda o: o["binding"].update(source_id=self.report["occurrences"][1]["binding"]["source_id"]),
            lambda o: o["binding"].update(version_id=self.report["occurrences"][1]["binding"]["version_id"]),
            lambda o: o["unit"].update(start_byte=1),
            lambda o: o["unit"].update(text="错误范围"),
            lambda o: o.update(occurrence_id="occ-" + "0" * 64),
            lambda o: o.update(object_id=o["property_id"], property_id=o["object_id"]),
        ]
        for index, change in enumerate(changes):
            with self.subTest(mutation=index):
                original = copy.deepcopy(self.report)
                change(self.report["occurrences"][0])
                self.refuse()
                self.report = original

    def test_strict_report_types_fixed_states_and_nullable(self):
        changes = [lambda r: r.update(schema_version=True), lambda r: r.update(network_calls=False),
                   lambda r: r.update(published=True), lambda r: r.update(scientific_quality="passed"),
                   lambda r: r.update(status="needs_review"), lambda r: r.update(extra=1),
                   lambda r: r["totals"].update(source_occurrences=3.0), lambda r: r["totals"].update(failed=False),
                   lambda r: r["totals"].pop("canonical_facts"), lambda r: r["totals"].update(canonical_facts=None),
                   lambda r: r["relations"][0].pop("issue_code"), lambda r: r["relations"][0].update(relation="duplicate")]
        for index, change in enumerate(changes):
            with self.subTest(mutation=index):
                original = copy.deepcopy(self.report)
                change(self.report)
                self.refuse()
                self.report = original

    def test_duplicate_json_key(self):
        path = self.output / "coordination.json"
        path.write_bytes(path.read_bytes().replace(b'"schema_version": 1', b'"schema_version": 1, "schema_version": 1', 1))
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_extra_output_inventory(self):
        (self.output / "human.txt").write_text("keep", encoding="utf-8")
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)
        self.assertEqual((self.output / "human.txt").read_text(), "keep")

    def test_missing_output_inventory(self):
        (self.output / "coordination-ir.json").unlink()
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def repin_bundle(self, ident="a"):
        manifest_path = self.case / "coordination.json"
        manifest = json.loads(manifest_path.read_bytes())
        item = next(i for i in manifest["inputs"] if i["input_id"] == ident)
        directory = self.case / item["review_bundle"]
        for name in item["artifacts"]:
            body = (directory / name).read_bytes()
            item["artifacts"][name] = fp(body)
            (self.output / ("input-" + ident + "--" + name)).write_bytes(body)
        manifest_path.write_bytes(encoded(manifest))
        (self.output / "coordination-original.json").write_bytes(encoded(manifest))
        self.report["inputs"][next(n for n, i in enumerate(self.report["inputs"]) if i["input_id"] == ident)]["source_bundle_fingerprints"] = item["artifacts"]

    def test_coherent_original_ir_replacement(self):
        path = self.case / "bundle-a/knowledge-ir.json"
        ir = json.loads(path.read_bytes())
        ir["atoms"][0]["measurement"]["value"] = "11"
        path.write_bytes(encoded(ir))
        self.repin_bundle()
        self.refuse()

    def test_source_owner_extra_and_wrong_type(self):
        path = self.case / "bundle-a/.owner.json"
        original = path.read_bytes()
        for change in (lambda o: o.update(extra=True), lambda o: o.update(schema_version=True)):
            owner = json.loads(original)
            change(owner)
            path.write_bytes(encoded(owner))
            self.repin_bundle()
            self.refuse()

    def test_actual_source_resource_tamper(self):
        import sqlite3
        with sqlite3.connect(self.project / "project.sqlite") as db:
            resource = db.execute("SELECT normalized_snapshot FROM source_versions LIMIT 1").fetchone()[0]
        path = self.project / resource
        path.write_bytes(path.read_bytes() + b"tampered")
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_same_sha_binding_swap_in_case(self):
        path = self.case / "coordination.json"
        manifest = json.loads(path.read_bytes())
        manifest["inputs"][1]["binding"] = copy.deepcopy(manifest["inputs"][0]["binding"])
        path.write_bytes(encoded(manifest))
        (self.output / "coordination-original.json").write_bytes(encoded(manifest))
        self.refuse()

    def test_original_unknown_denominator_is_not_zero(self):
        path = self.case / "bundle-a/verification.json"
        report = json.loads(path.read_bytes())
        report["totals"]["candidate_atoms"] = None
        path.write_bytes(encoded(report))
        self.repin_bundle()
        self.refuse()

    def test_case_extra_unknown_duplicate_and_missing_nullable(self):
        path = self.case / "coordination.json"
        original = path.read_bytes()
        variants = []
        case = json.loads(original); case["extra"] = 1; variants.append(encoded(case))
        case = json.loads(original); case["inputs"][0]["binding"].pop("parse_id"); variants.append(encoded(case))
        variants.append(original.replace(b'"schema_version": 1', b'"schema_version": 1, "schema_version": 1', 1))
        for raw in variants:
            with self.subTest(raw_hash=fp(raw)["sha256"]):
                path.write_bytes(raw)
                with self.assertRaises(AssertionError):
                    verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_map_role_reversal_even_with_synchronized_outputs(self):
        path = self.case / "identity.json"
        mapping = json.loads(path.read_bytes())
        for atom in mapping["atom_mappings"]:
            atom["object_id"], atom["property_id"] = atom["property_id"], atom["object_id"]
        path.write_bytes(encoded(mapping))
        (self.output / "identity-original.json").write_bytes(encoded(mapping))
        for category in ("facts", "occurrences"):
            for item in self.report[category]:
                item["object_id"], item["property_id"] = item["property_id"], item["object_id"]
        self.refuse()

    def test_independent_gold_rejects_coherent_fact_and_id_forgery(self):
        # All result objects can agree but cannot redefine the author's facts.
        for item in self.report["occurrences"]:
            if item["input_id"] in ("a", "b"):
                item["value"] = "11"
        self.report["facts"][0]["value"] = "11"
        self.refuse()

    def test_read_only_replay_inventory(self):
        from verify_knowledge_coordination import inventory
        before = [inventory(p) for p in (self.project, self.case, self.output)]
        verify_report(self.project, self.case, self.output, self.gold_bytes)
        self.assertEqual(before, [inventory(p) for p in (self.project, self.case, self.output)])

    def test_coherent_source_report_expected_fact_forgery(self):
        directory = self.case / "bundle-a"
        path = directory / "verification.json"
        report = json.loads(path.read_bytes())
        next(c for c in report["checks"] if c["category"] == "content")["expected"] = "伪作者期望"
        ir_path = directory / "knowledge-ir.json"
        ir = json.loads(ir_path.read_bytes()); ir["checks"] = report["checks"]
        ir_path.write_bytes(encoded(ir))
        for name in report["artifacts"]:
            report["artifacts"][name] = fp((directory / name).read_bytes())
        path.write_bytes(encoded(report))
        md_path = directory / "verification.md"
        md_path.write_bytes(md_path.read_bytes().split(b"```json\n", 1)[0] + b"```json\n" + encoded(report).rstrip() + b"\n```\n")
        self.report["inputs"][0]["source_review_checks"] = report["checks"]
        self.repin_bundle()
        self.refuse()

    def test_false_markdown_visible_status(self):
        path = self.output / "coordination.md"
        path.write_bytes(path.read_bytes().replace("发布：false".encode(), "发布：true".encode(), 1))
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_hardlinked_input_is_rejected_without_writing(self):
        import os
        target = self.case / "bundle-a/gold-original.json"
        link = self.root / "human-hardlink.json"
        os.link(target, link)
        before = target.read_bytes()
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)
        self.assertEqual(target.read_bytes(), before)
        self.assertEqual(link.read_bytes(), before)

    def test_cli_help_and_invalid_arguments(self):
        import contextlib
        import io
        from verify_knowledge_coordination import main
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as good:
                main(["--help"])
            self.assertEqual(good.exception.code, 0)
            with self.assertRaises(SystemExit) as bad:
                main(["check"])
            self.assertEqual(bad.exception.code, 2)

    def test_registered_resource_cannot_escape_selected_project(self):
        import sqlite3
        with sqlite3.connect(self.project / "project.sqlite") as db:
            version, resource = db.execute("SELECT version_id, normalized_snapshot FROM source_versions WHERE normalized_snapshot IS NOT NULL LIMIT 1").fetchone()
            external = self.root / "external-source.txt"
            external.write_bytes((self.project / resource).read_bytes())
            db.execute("UPDATE source_versions SET normalized_snapshot=? WHERE version_id=?", ("../external-source.txt", version))
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)

    def test_project_manifest_identity_must_match_sql(self):
        path = self.project / "project.json"
        manifest = json.loads(path.read_bytes())
        manifest["project_id"] = "00000000-0000-0000-0000-000000000000"
        path.write_bytes(encoded(manifest))
        with self.assertRaises(AssertionError):
            verify_report(self.project, self.case, self.output, self.gold_bytes)


if __name__ == "__main__":
    unittest.main()
