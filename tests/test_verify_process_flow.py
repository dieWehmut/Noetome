"""Independent process source/oracle and finite output-reader regressions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("verify_process_flow", ROOT / "scripts/verify_process_flow.py")
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/process-flow-v1"


class ProcessAuthorTests(unittest.TestCase):
    def source(self, name="linear"):
        return (FIXTURE / (name + ".txt")).read_bytes()

    def document(self, name="linear"):
        return V.parse_source(self.source(name))[0]

    def profile(self, name="obsidian_1_6_7_flowchart"):
        return json.loads((FIXTURE / ("profile-" + name + ".json")).read_bytes())

    def test_all_sixteen_frozen_sources_gold_and_expected_independently_agree(self):
        self.assertEqual(len(V.verify_fixture()["cases"]), 16)

    def test_unsigned_changes_cannot_become_new_author_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for file in FIXTURE.iterdir():
                (root / file.name).write_bytes(file.read_bytes())
            changed = json.loads((root / "expected-linear.json").read_bytes())
            changed["nodes"][0]["description"] = "伪造但同步的说明"
            V.save(root / "expected-linear.json", changed)
            with self.assertRaises(AssertionError):
                V.verify_fixture(root)

    def test_json_words_and_punctuation_do_not_change_statement_boundaries(self):
        raw = self.source().decode().replace('"登记卡片"', json.dumps('登记。条件：[x] | "卡片"', ensure_ascii=False), 1).encode()
        doc, unknown, count = V.parse_source(raw)
        self.assertEqual(doc["steps"][0]["label"], '登记。条件：[x] | "卡片"')
        self.assertEqual(unknown, []); self.assertEqual(count, 6)
        for evidence in doc["evidence"]:
            self.assertEqual(raw[evidence["start_byte"]:evidence["end_byte"]].decode(), evidence["quote"])

    def test_multiline_and_tab_are_literal_text_not_extra_source_statements(self):
        doc, unknown, count = V.parse_source(self.source("multiline_labels"))
        self.assertEqual(unknown, []); self.assertEqual(count, len(doc["evidence"]))
        self.assertTrue(any("\n" in s["label"] and "\t" in s["label"] for s in doc["steps"]))
        self.assertEqual(V.classify(doc, self.profile())[0:2], ("ordered_list", ["FLOW_LABEL_WHITESPACE_FALLBACK"]))

    def test_source_order_is_not_execution_order(self):
        raw = self.source("complex_labels")
        doc, unknown, _ = V.parse_source(raw)
        self.assertLess(raw.index("步骤 s2".encode()), raw.index("步骤 s1".encode()))
        self.assertEqual([s["id"] for s in doc["steps"]], ["s1", "s2", "s3"])
        self.assertEqual(V.classify(doc, self.profile())[2], ["s1", "s2", "s3"])
        self.assertEqual(unknown, [])

    def test_unknown_tail_and_negation_are_counted_and_not_affirmed(self):
        doc, unknown, count = V.parse_source(self.source("negated_relation"))
        self.assertEqual(len(unknown), 1); self.assertIn("不导致", unknown[0])
        self.assertEqual(doc["relations"], []); self.assertEqual(count, 4)
        _, tail, count = V.parse_source(self.source("unparsed_tail"))
        self.assertEqual(tail, ["补充：另有未描述的人工决定。"]); self.assertEqual(count, 7)

    def test_source_template_suffix_and_extra_blank_line_are_not_ignored(self):
        for raw in (self.source() + b"\n", self.source().rstrip(b"\n") + " 未说明。\n".encode()):
            with self.subTest(raw=raw[-30:]):
                _, unmatched, _ = V.parse_source(raw)
                self.assertEqual(len(unmatched), 1)

    def test_forbidden_control_hidden_markup_and_autolinks_are_rejected(self):
        illegal = ["\r", "\x00", "\x7f", "\x85", "%%hidden", "[[wikilink]]", "![x]",
                   "<b>x", "<123@example>", "[x] (url)", "[x]\t[label]", "[x]:url"]
        for value in illegal:
            raw = self.source().decode().replace('"登记卡片"', json.dumps("标题" + value), 1).encode()
            with self.subTest(value=repr(value)), self.assertRaises(AssertionError):
                V.parse_source(raw)

    def test_strict_gold_fields_types_and_nullable_values(self):
        gold = json.loads((FIXTURE / "gold-linear.json").read_bytes())
        mutations = [("schema_version", True), ("published", False), ("scientific_quality", "passed")]
        for key, value in mutations:
            changed = copy.deepcopy(gold); changed[key] = value
            with self.subTest(key=key), self.assertRaises(AssertionError):
                V.gold_document(changed)
        for key in ("criterion", "exceptions", "inputs"):
            changed = copy.deepcopy(gold); del changed["steps"][0][key]
            with self.subTest(missing=key), self.assertRaises(AssertionError):
                V.gold_document(changed)

    def test_gold_requires_one_nonoverlapping_byte_evidence_per_object(self):
        for mutation in (lambda d: d["evidence"].pop(),
                         lambda d: d["evidence"][1].update(start_byte=1),
                         lambda d: d["evidence"][1].update(object_id="process")):
            doc = self.document(); mutation(doc)
            with self.assertRaises(AssertionError):
                V.document_schema(doc)

    def test_semantic_blockers_are_not_compatibility_fallback(self):
        cases = {"correlation":"FLOW_CORRELATION_UNDIRECTED", "direction_unknown":"FLOW_DIRECTION_UNKNOWN",
            "missing_branch":"FLOW_DECISION_INCOMPLETE", "cycle":"FLOW_UNSUPPORTED_CYCLE",
            "missing_connection":"FLOW_DISCONNECTED", "unknown_criterion":"FLOW_UNKNOWN_CRITERION"}
        for name, issue in cases.items():
            for profile in (self.profile(), self.profile("unverified_flowchart")):
                with self.subTest(name=name, profile=profile["profile_id"]):
                    self.assertEqual(V.classify(self.document(name), profile), (None, [issue], []))

    def test_effect_types_direction_and_conditions_are_all_author_semantics(self):
        for name, category, key, changed_value in [
            ("linear", "relations", "left", "s2"), ("causes", "relations", "kind", "promotes"),
            ("inhibits", "relations", "kind", "causes"), ("branch", "relations", "branch_outcome", False),
            ("linear", "steps", "conditions", []), ("linear", "relations", "exceptions", []),
            ("linear", "steps", "outputs", ["未知"]), ("linear", "steps", "description", "已丢原说明")]:
            doc = self.document(name)
            target = next((r for r in doc[category] if r.get("kind") == "branch"), doc[category][0])
            target[key] = changed_value
            expected = json.loads((FIXTURE / ("expected-" + name + ".json")).read_bytes())
            with self.subTest(name=name, key=key), self.assertRaises(AssertionError):
                V.verify_expectation(expected, doc, [], len(doc["evidence"]), self.profile())

    def test_unverified_visual_whitespace_falls_back_in_label_or_criterion(self):
        for value in (" 首字符", "末字符 ", "两个  空格", "a\u00a0b", "a\u2003b", "a\u2028b", "a\tb", "a\nb"):
            for field in ("label", "criterion"):
                doc = self.document("branch")
                node = next(s for s in doc["steps"] if s["kind"] == "decision")
                node[field] = value
                with self.subTest(value=repr(value), field=field):
                    self.assertEqual(V.classify(doc, self.profile())[0:2], ("ordered_list", ["FLOW_LABEL_WHITESPACE_FALLBACK"]))
                    self.assertEqual(V.fallback_reasons(doc, ["FLOW_LABEL_WHITESPACE_FALLBACK"])[0]["object_ids"], [node["id"]])
        doc = self.document(); doc["steps"][0]["label"] = "a b"
        self.assertEqual(V.classify(doc, self.profile())[0], "flowchart")


class MarkdownCodecTests(unittest.TestCase):
    def test_ascii_punctuation_entities_preserve_complex_source_labels(self):
        encoded = "#91;α#93; #124; #34;卡片#34; #92; #38; #60; 3"
        self.assertEqual(V.decode_label(encoded), '[α] | "卡片" \\ & < 3')
        for invalid in ('#0;', '#043;', '#128;', '#10;', '#65;', '#1;', '[[x]]', 'a"b', '#39', 'a\tb', 'a\x7fb', 'a\x85b'):
            with self.subTest(invalid=invalid), self.assertRaises(AssertionError):
                V.decode_label(invalid)

    def test_fenced_list_body_preserves_literal_newlines_tabs_and_delimiters(self):
        raw = "   ````text\n   第一行\t原字符\n   ```\n   尾行\n   \n   ````\n\n".encode()
        reader = V.MarkdownReader(raw)
        body, start, end = reader.fence("   ")
        self.assertEqual(body, "第一行\t原字符\n```\n尾行\n")
        self.assertEqual(raw[start:end].decode(), "   第一行\t原字符\n   ```\n   尾行\n   ")
        self.assertEqual(reader.index, len(reader.lines))
        for changed in (raw.replace(b"   ```\n", b"   ````\n"), raw.replace("   尾行".encode(), " 尾行".encode())):
            with self.assertRaises(AssertionError):
                V.MarkdownReader(changed).fence("   ")

    def sample(self):
        doc = V.parse_source((FIXTURE / "inhibits.txt").read_bytes())[0]
        diagram = dict(direction="TD", nodes=[dict(node_id="n0", step_id="s1", kind="step", text="信号输入"),
            dict(node_id="n1", step_id="s2", kind="step", text="状态输出")], edges=[dict(edge_id="e0", relation_id="r1",
                left_node_id="n0", right_node_id="n1", kind="inhibits", branch_outcome=None, text="−：抑制")])
        plan = dict(document=doc, revision=1, diagram=diagram, notes=[dict(note_id="a", output_path="A.md", title="图", sections=[
            dict(section_id="graph", heading="图形", blocks=[dict(kind="diagram", block_id="diagram")])]),
            dict(note_id="b", output_path="B.md", title="空结构测试", sections=[])])
        lines = ['  n0["信号输入"]', '  n1["状态输出"]', '  n0 -->|"−：抑制"| n1']
        a = ('# 图\n\n## 图形\n\n```mermaid\nflowchart TD\n' + '\n'.join(lines) + '\n```\n\n').encode()
        b = '# 空结构测试\n\n'.encode(); actual = {'A.md': a, 'B.md': b}
        mappings = []
        for i, line in enumerate(lines):
            raw = line.encode(); start = a.index(raw)
            mappings.append(dict(section_id="graph", block_id="diagram", kind="diagram_node" if i < 2 else "diagram_edge",
                item_id="n"+str(i) if i < 2 else "e0", source_object_ids=["s"+str(i+1)] if i < 2 else ["r1"], start_byte=start, end_byte=start+len(raw)))
        files = [dict(relative_path=n, note_id="a" if n == "A.md" else "b", plan_revision=1, content_sha256=V.fingerprint(raw)["sha256"],
                      byte_length=len(raw), content_mappings=mappings if n == "A.md" else [], links=[]) for n, raw in actual.items()]
        return plan, files, actual

    def test_readback_rejects_reversed_unsigned_extra_nodes_and_false_ranges_after_hash_sync(self):
        plan, files, actual = self.sample()
        self.assertEqual(V.verify_rendered(plan, files, actual)["content_mappings"], 3)
        for old, new in [(b'n0 -->', b'n1 -->'), ('−：抑制'.encode(), '#43;：促进'.encode()),
                         (b'flowchart TD\n', b'flowchart TD\n  n99["fabricated"]\n')]:
            changed = copy.deepcopy(actual); changed['A.md'] = changed['A.md'].replace(old, new)
            reports = copy.deepcopy(files)
            reports[0]['content_sha256'] = V.fingerprint(changed['A.md'])['sha256']; reports[0]['byte_length'] = len(changed['A.md'])
            with self.subTest(new=new), self.assertRaises(AssertionError): V.verify_rendered(plan, reports, changed)
        for field, value in [('start_byte', 1), ('source_object_ids', ['s2'])]:
            reports = copy.deepcopy(files); reports[0]['content_mappings'][0][field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError): V.verify_rendered(plan, reports, actual)


class BlockerDiagnosticTests(unittest.TestCase):
    def document(self, name):
        return V.parse_source((FIXTURE / (name + ".txt")).read_bytes())[0]

    def test_primary_issues_identify_all_actual_source_objects(self):
        expected = {"correlation": ["r1"], "direction_unknown": ["r1"], "missing_branch": ["d1"],
                    "cycle": ["s1", "s2", "s3"], "missing_connection": ["s3"], "unknown_criterion": ["d1"]}
        profile = json.loads((FIXTURE / "profile-obsidian_1_6_7_flowchart.json").read_bytes())
        for name, ids in expected.items():
            doc = self.document(name); issues = V.blocker_issues(doc)
            with self.subTest(name=name):
                self.assertEqual(issues[0]["object_ids"], ids)
                self.assertEqual([i["code"] for i in issues], V.classify(doc, profile)[1])
                V.verify_blockers(issues, doc)

    def test_empty_unrelated_missing_objects_and_fabricated_reason_are_rejected(self):
        for name in ("correlation", "direction_unknown", "missing_branch", "cycle", "missing_connection", "unknown_criterion"):
            doc = self.document(name)
            for field, value in [("object_ids", []), ("object_ids", ["process"]), ("message", "该问题定位于此步骤。")]:
                issues = V.blocker_issues(doc); issues[0][field] = value
                with self.subTest(name=name, field=field, value=value), self.assertRaises(AssertionError):
                    V.verify_blockers(issues, doc)
        doc = self.document("correlation"); issues = V.blocker_issues(doc); issues[0]["object_ids"] = ["s1"]
        with self.assertRaises(AssertionError): V.verify_blockers(issues, doc)
        doc = self.document("cycle"); issues = V.blocker_issues(doc); issues[0]["object_ids"].pop()
        with self.assertRaises(AssertionError): V.verify_blockers(issues, doc)

    def test_cycle_residual_and_goal_mismatch_diagnostics_are_precise(self):
        doc = self.document("linear")
        doc["relations"].append(dict(id="r3", left="s2", right="s2", kind="sequence", branch_outcome=None, conditions=[], exceptions=[]))
        self.assertEqual(V.blocker_issues(doc)[0]["object_ids"], ["s2", "s3"])
        doc = self.document("linear"); doc["process"]["goals"] = ["s2"]
        issues = V.blocker_issues(doc)
        self.assertEqual(issues[0]["object_ids"], ["process", "s2", "s3"])
        self.assertIn("目标", issues[0]["message"])

    def test_failed_source_reason_cannot_be_replaced_by_graph_claim(self):
        doc = self.document("correlation")
        correct = V.blocker_issues(doc, False)
        self.assertEqual(correct[0]["code"], "FLOW_SOURCE_UNVERIFIED")
        self.assertEqual(correct[0]["object_ids"], [])
        with self.assertRaises(AssertionError): V.verify_blockers(V.blocker_issues(doc), doc, False)


class SelectedSourcePreflightTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.project = Path(self.temp.name) / "project"; self.project.mkdir()
        self.version, self.source, self.import_id = (str(uuid.uuid4()) for _ in range(3))
        self.project_id = str(uuid.uuid4())

    def create(self, raw=None):
        raw = raw if raw is not None else (FIXTURE / "linear.txt").read_bytes()
        normalized = V.normalize_raw_text(raw)
        self.cache = self.project / ".noetome-sources" / self.version; self.cache.mkdir(parents=True)
        (self.cache / "raw.bin").write_bytes(raw); (self.cache / "normalized.txt").write_bytes(normalized)
        self.dbpath = self.project / "project.sqlite"
        V.save(self.project / "project.json", dict(project_id=self.project_id, schema_version=2,
            created_at="2026-11-05T00:00:00Z", state="initialized", database="project.sqlite"))
        db = sqlite3.connect(self.dbpath)
        db.execute("CREATE TABLE project_metadata (project_id TEXT)")
        db.execute("INSERT INTO project_metadata VALUES (?)", (self.project_id,))
        db.execute("CREATE TABLE source_imports (import_id TEXT, status TEXT, source_id TEXT, version_id TEXT)")
        db.execute("INSERT INTO source_imports VALUES (?,?,?,?)", (self.import_id, "ready", self.source, self.version))
        version = dict(version_id=self.version, source_id=self.source, media_type="text/plain", ingest_status="ready",
            normalization_version="utf8-newlines-v1", raw_snapshot=f".noetome-sources/{self.version}/raw.bin",
            normalized_snapshot=f".noetome-sources/{self.version}/normalized.txt", content_sha256=V.fingerprint(raw)["sha256"],
            byte_length=len(raw), normalized_sha256=V.fingerprint(normalized)["sha256"])
        db.execute("CREATE TABLE source_versions (" + ",".join(k+(" INTEGER" if type(v) is int else " TEXT") for k,v in version.items()) + ")")
        db.execute("INSERT INTO source_versions VALUES ("+",".join("?" for _ in version)+")", list(version.values()))
        self.spans = V.text_blocks(normalized)
        db.execute("CREATE TABLE source_spans (span_id TEXT, source_version_id TEXT, ordinal INTEGER, kind TEXT, start_byte INTEGER, end_byte INTEGER, start_line INTEGER, end_line INTEGER, text_sha256 TEXT, parse_status TEXT, parser_version TEXT)")
        for i, span in enumerate(self.spans):
            db.execute("INSERT INTO source_spans VALUES (?,?,?,?,?,?,?,?,?,?,?)", (str(uuid.uuid4()), self.version, i, *span.values(), "blocks-v1"))
        db.commit(); db.close()
        return raw, normalized

    def update(self, statement, args=()):
        db = sqlite3.connect(self.dbpath); db.execute(statement, args); db.commit(); db.close()

    def preflight(self, ordinal=0):
        return V.source_preflight(self.project, self.import_id, dict(kind="text", ordinal=ordinal))

    def test_only_first_bom_and_cr_newlines_are_normalized(self):
        raw = "\ufeff\ufeff# 标题\r\n第一行\r第二行\u2028保留".encode()
        self.assertEqual(V.normalize_raw_text(raw), "\ufeff# 标题\n第一行\n第二行\u2028保留".encode())
        with self.assertRaises(UnicodeDecodeError): V.normalize_raw_text(b"\xef\xbb\xbf\xff")

    def test_heading_blank_prefix_preserves_actual_selected_unit_and_offsets(self):
        source = (FIXTURE / "linear.txt").read_bytes()
        _, normalized = self.create("\ufeff# 前置标题\r\n\r\n".encode()+source)
        self.assertEqual([s["kind"] for s in self.spans], ["heading", "formatting", "text"])
        self.assertEqual(self.preflight(2), source)
        self.assertEqual(self.spans[2]["start_byte"], normalized.index(source))
        self.assertEqual(self.spans[2]["start_line"], 3)

    def test_blocks_cover_fences_deep_indent_unicode_blanks_and_terminal_lf(self):
        raw = "   # heading\n\u2003\n    # ordinary\n```x\n# code\n```\n\t# ordinary\n~~~\nunclosed".encode()
        blocks = V.text_blocks(raw)
        self.assertEqual([(s["kind"], s["parse_status"]) for s in blocks], [
            ("heading", "parsed"), ("formatting", "parsed"), ("text", "parsed"), ("code", "parsed"), ("text", "parsed"), ("code", "needs_review")])
        self.assertEqual(b"".join(raw[s["start_byte"]:s["end_byte"]] for s in blocks), raw)
        self.assertEqual(V.text_blocks(b""), [])
        self.assertEqual(V.text_blocks(b"\x1c\n")[0]["kind"], "text")

    def test_external_parent_or_same_project_alias_paths_are_rejected(self):
        self.create()
        for bad in ("/tmp/outside.txt", "../outside.txt", "ordinary-inside-project.txt", f".noetome-sources/{self.version}/../normalized.txt"):
            self.update("UPDATE source_versions SET normalized_snapshot=?", (bad,))
            with self.subTest(path=bad), self.assertRaises(AssertionError): self.preflight()

    def test_raw_and_normalized_self_consistent_hashes_do_not_replace_derivation(self):
        self.create()
        changed = b"unrelated raw source\n"
        (self.cache / "raw.bin").write_bytes(changed)
        self.update("UPDATE source_versions SET content_sha256=?, byte_length=?", (V.fingerprint(changed)["sha256"], len(changed)))
        with self.assertRaisesRegex(AssertionError, "derive"): self.preflight()

    def test_sql_span_cannot_hide_negative_tail_with_synchronized_hash(self):
        source = (FIXTURE / "linear.txt").read_bytes(); self.create(source+"关系 r3：s1 不导致 s3。条件：[]。例外：[]。\n".encode())
        self.update("UPDATE source_spans SET end_byte=?,end_line=6,text_sha256=?", (len(source), V.fingerprint(source)["sha256"]))
        with self.assertRaisesRegex(AssertionError, "raw-derived source span"): self.preflight()

    def test_single_link_and_canonical_version_identity_are_required(self):
        self.create(); alias = Path(self.temp.name) / "alias.bin"
        os.link(self.cache / "raw.bin", alias)
        with self.assertRaises(AssertionError): self.preflight()
        alias.unlink()
        self.update("UPDATE source_versions SET version_id=?", (self.version.upper(),))
        self.update("UPDATE source_imports SET version_id=?", (self.version.upper(),))
        with self.assertRaises(AssertionError): self.preflight()

    def test_project_manifest_and_database_must_identify_same_project(self):
        self.create(); manifest = json.loads((self.project / "project.json").read_bytes())
        manifest["project_id"] = str(uuid.uuid4()); V.save(self.project / "project.json", manifest)
        with self.assertRaisesRegex(AssertionError, "project manifest / database identity"): self.preflight()


if __name__ == "__main__":
    unittest.main()
