"""Behavioral corruption tests against real ordinary CLI output, no model calls."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("knowledge_audit", REPO / "scripts/verify_extraction_knowledge.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")


class KnowledgeVerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Author gold is frozen before project import and synthetic candidates.
        fixture = REPO / "fixtures/extraction-review-v1"
        cls.gold_bytes = (fixture / "gold.json").read_bytes()
        cls.gold = json.loads(cls.gold_bytes)
        cls.root = Path(tempfile.mkdtemp(prefix="knowledge-validator-", dir=REPO / "target/compiler-temp"))
        cls.project, cls.case, cls.output = [cls.root / n for n in ("project", "case", "original-output")]
        cls.case.mkdir()
        cls.binary = Path(os.environ.get("NOETOME_TEST_BINARY", REPO / "target/debug/noetome.exe"))
        cls.calls = []

        def call(args):
            run = subprocess.run([str(cls.binary), *map(str, args)], capture_output=True, timeout=40)
            cls.calls.append({"argv": run.args, "exit": run.returncode, "stdout": run.stdout.decode(), "stderr": run.stderr.decode()})
            if run.returncode != 0:
                raise AssertionError(cls.calls[-1])
            return json.loads(run.stdout) if run.stdout.lstrip().startswith(b"{") else None

        call(["project", "init", cls.project])
        imported = call(["source", "import", cls.project, fixture / "source.txt"])
        version = imported["version"]
        binding = dict(project_id=json.loads((cls.project / "project.json").read_bytes())["project_id"],
                       import_id=imported["import_id"], **{k: version[k] for k in ("source_id", "version_id", "content_sha256", "normalized_sha256")},
                       parse_id=None, manifest_sha256=None)
        span = imported["spans"][0]
        resource = (cls.project / version["normalized_snapshot"]).read_bytes()
        cls.source = {"binding": binding, "units": [{"unit_id": "unit-0", "locator": {"kind": "text", "span_id": span["span_id"], "ordinal": 0, "resource_sha256": hashlib.sha256(resource).hexdigest()}, "source_status": span["parse_status"], "start_byte": span["start_byte"], "end_byte": span["end_byte"], "text": resource[span["start_byte"]:span["end_byte"]].decode()}]}
        candidate = json.loads((fixture / "candidates.json").read_bytes())
        candidate["binding"] = candidate["payload"]["binding"] = binding
        quote = cls.gold["quote"].encode()
        start = resource.index(quote)
        candidate["payload"]["evidence"][0].update(excerpt_start_byte=start, excerpt_end_byte=start + len(quote), excerpt_sha256=hashlib.sha256(quote).hexdigest())
        save(cls.case / "candidates.json", candidate)
        (cls.case / "gold.json").write_bytes(cls.gold_bytes)
        review = json.loads((fixture / "review.json").read_bytes())
        review["gold_sha256"] = hashlib.sha256(cls.gold_bytes).hexdigest()
        save(cls.case / "review.json", review)
        shutil.copyfile(fixture / "decision.json", cls.case / "decision.json")
        cls.candidate_bytes = (cls.case / "candidates.json").read_bytes()
        cls.inputs = {n: (cls.case / n).read_bytes() for n in ("review.json", "candidates.json", "gold.json", "decision.json")}
        call(["extract", "verify", cls.project, imported["import_id"], cls.case, cls.output])
        save(cls.root / "setup-calls.json", cls.calls)

    def setUp(self):
        self.output = self.root / self.id().split(".")[-1]
        shutil.copytree(type(self).output, self.output)
        self.candidate = json.loads(self.candidate_bytes)
        self.source = copy.deepcopy(type(self).source)
        self.inputs = dict(type(self).inputs)

    def verify(self):
        return audit.verify_report(self.output, self.inputs["candidates.json"], self.gold_bytes, self.source, self.inputs)

    def change_report(self, edit):
        report = json.loads((self.output / "verification.json").read_bytes())
        edit(report)
        save(self.output / "verification.json", report)
        (self.output / "verification.md").write_text("```json\n" + json.dumps(report, ensure_ascii=False) + "\n```\n", encoding="utf-8")

    def corrupt_candidate(self, edit):
        edit(self.candidate["payload"]["atoms"][0])
        raw = json.dumps(self.candidate, ensure_ascii=False).encode()
        self.inputs["candidates.json"] = raw
        (self.output / "candidates-original.json").write_bytes(raw)
        fp = {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}
        self.change_report(lambda r: (r["origin"]["input_fingerprints"].update({"candidates.json": fp}), r["artifacts"].update({"candidates-original.json": fp})))

    def test_actual_valid_cli_artifacts_pass(self):
        self.verify()

    def test_forged_pass_wrong_fact(self):
        self.corrupt_candidate(lambda a: a["atom"].update(content="样例 A 的速度为 11 m/s，条件为温度 20 ℃且仅适用样例 A；例外为结冰时不适用。"))
        with self.assertRaisesRegex(AssertionError, "fact|content|IR"):
            self.verify()

    def test_forged_pass_wrong_unit(self):
        self.corrupt_candidate(lambda a: a["measurement"].update(unit="kg"))
        with self.assertRaisesRegex(AssertionError, "unit|IR"):
            self.verify()

    def test_forged_pass_missing_conditions(self):
        self.corrupt_candidate(lambda a: (a["measurement"].update(conditions=[]), a["atom"].update(qualifiers=["m/s", "结冰时不适用"])))
        with self.assertRaisesRegex(AssertionError, "condition|IR"):
            self.verify()

    def test_forged_pass_missing_exception(self):
        self.corrupt_candidate(lambda a: a["atom"].update(qualifiers=["m/s", "温度 20 ℃", "仅适用样例 A"]))
        with self.assertRaisesRegex(AssertionError, "exception|IR"):
            self.verify()

    def test_fabricated_denominator(self):
        self.change_report(lambda r: r["quality"]["condition"].update(expected=1, passed=1))
        with self.assertRaisesRegex(AssertionError, "denominator"):
            self.verify()

    def test_fabricated_actual_counts(self):
        self.change_report(lambda r: r["totals"].update(candidate_atoms=0))
        with self.assertRaisesRegex(AssertionError, "count"):
            self.verify()

    def test_swapped_ir(self):
        ir = json.loads((self.output / "knowledge-ir.json").read_bytes())
        ir["atoms"][0]["atom"]["content"] = "unrelated claim"
        save(self.output / "knowledge-ir.json", ir)
        with self.assertRaisesRegex(AssertionError, "IR|fingerprint"):
            self.verify()

    def test_false_source_binding(self):
        self.source["binding"]["source_id"] = "other-source"
        with self.assertRaisesRegex(AssertionError, "binding|source"):
            self.verify()

    def test_false_source_slice(self):
        self.source["units"][0]["text"] = "wrong actual bytes"
        with self.assertRaisesRegex(AssertionError, "source|slice"):
            self.verify()

    def test_false_source_sha(self):
        self.source["units"][0]["locator"]["resource_sha256"] = "0" * 64
        with self.assertRaisesRegex(AssertionError, "source|SHA"):
            self.verify()

    def test_wrong_copied_input_fingerprint(self):
        self.change_report(lambda r: r["origin"]["input_fingerprints"]["gold.json"].update(sha256="0" * 64))
        with self.assertRaisesRegex(AssertionError, "fingerprint"):
            self.verify()

    def test_json_markdown_mismatch(self):
        (self.output / "verification.md").write_text('```json\n{}\n```', encoding="utf-8")
        with self.assertRaisesRegex(AssertionError, "Markdown"):
            self.verify()

    def test_false_provenance(self):
        self.change_report(lambda r: r["origin"].update(interpretation_timing="after_model_response"))
        with self.assertRaisesRegex(AssertionError, "origin|provenance"):
            self.verify()

    def test_unknown_state_cannot_keep_ir(self):
        self.change_report(lambda r: r.update(status="needs_review"))
        with self.assertRaisesRegex(AssertionError, "IR|status"):
            self.verify()

    def test_false_additional_concept_count(self):
        self.change_report(lambda r: r["totals"].update(additional_source_concepts=99))
        with self.assertRaisesRegex(AssertionError, "concept.*count"):
            self.verify()

    def test_wrong_candidate_result_identity(self):
        self.change_report(lambda r: r["candidate_results"][0].update(object_id="other-atom"))
        with self.assertRaisesRegex(AssertionError, "result.*identity"):
            self.verify()

    def test_wrong_candidate_result_state(self):
        self.change_report(lambda r: r["candidate_results"][0].update(status="failed"))
        with self.assertRaisesRegex(AssertionError, "result.*state"):
            self.verify()

    def test_false_failure_of_correct_candidate(self):
        self.change_report(lambda r: (r.update(status="failed"), r["totals"].update(passed=0, failed=1), r["expected_results"][0].update(status="failed")))
        (self.output / "knowledge-ir.json").unlink()
        self.change_report(lambda r: r["artifacts"].pop("knowledge-ir.json"))
        with self.assertRaisesRegex(AssertionError, "independent.*status"):
            self.verify()
