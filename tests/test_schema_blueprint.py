"""Execute the SQL design against SQLite and verify relational invariants.

These tests exercise a blueprint, not implemented product repositories,
model quality, semantic validators, or the filesystem publication protocol.
"""

import hashlib
import json
from pathlib import Path
import sqlite3
import unittest


SQL = (Path(__file__).resolve().parents[1] / "doc" / "technical" / "schema.sql").read_text(encoding="utf-8-sig")
STAMP = "2026-11-01T00:00:00Z"
HASH = hashlib.sha256(b"fixture").hexdigest()


def insert(db, table, **values):
    columns = ",".join(values)
    marks = ",".join("?" for _ in values)
    db.execute(f"INSERT INTO {table}({columns}) VALUES({marks})", tuple(values.values()))


def seed(db):
    insert(db, "project", project_id="fx_project", name="Fixture", created_at=STAMP, state="initialized")
    insert(db, "blob", sha256=HASH, byte_length=7, media_type="text/plain", created_at=STAMP)
    insert(db, "source", source_id="fx_source", project_id="fx_project", display_name="Fixture", origin="local", created_at=STAMP)
    insert(db, "source_version", version_id="fx_version", source_id="fx_source", content_sha256=HASH, byte_length=7,
           media_type="text/plain", acquired_at=STAMP, location_json='{"kind":"local","storage":"reference","selected_path":"fixture.txt"}', availability="available")
    insert(db, "parse_manifest", manifest_id="fx_manifest", version_id="fx_version", parser_version="text_v1",
           normalization_version="utf8_v1", parameters_sha256=HASH, normalized_blob_sha256=HASH, status="ready", created_at=STAMP)
    insert(db, "source_span", span_id="fx_span", manifest_id="fx_manifest", ordinal=0, locator_key="bytes:0:7", kind="text",
           start_byte=0, end_byte=7, start_line=1, end_line=1, text_blob_sha256=HASH, text_sha256=HASH,
           status="parsed", payload_json='{"coverage_role":"leaf"}')
    insert(db, "evidence", evidence_id="fx_evidence", span_id="fx_span", locator_kind="text", excerpt_start_byte=0,
           excerpt_end_byte=7, excerpt_sha256=HASH, locator_json='{"kind":"text"}', verification="matched")
    for ident, kind in (("fx_concept", "concept"), ("fx_claim", "claim"), ("fx_atom", "atom")):
        insert(db, "entity", entity_id=ident, kind=kind, created_at=STAMP)
        insert(db, "entity_revision", entity_id=ident, revision=1, content_sha256=HASH,
               created_at=STAMP, producer="fixture", reason="Relational fixture")
    insert(db, "concept_revision", concept_id="fx_concept", revision=1, canonical_name="Fixture concept",
           entity_class="concept", disambiguation="Fixture", definition_policy="required")
    insert(db, "claim_revision", claim_id="fx_claim", revision=1, subject_id="fx_concept", predicate="has_property",
           value_kind="text", value_json='{"text":"fixture"}', context_sha256=HASH, verification="fixture_reviewed")
    insert(db, "claim_evidence", claim_id="fx_claim", revision=1, evidence_id="fx_evidence", role="supports", review_status="accepted", rationale="Fixture")
    insert(db, "atom_revision", atom_id="fx_atom", revision=1, kind="statement", title="Fixture",
           payload_json='{"segments":[{"kind":"claim_ref","ref":"fx_claim"}]}', verification="fixture_reviewed")
    insert(db, "atom_claim", atom_id="fx_atom", atom_revision=1, claim_id="fx_claim", claim_revision=1, ordinal=0)
    insert(db, "knowledge_snapshot", snapshot_id="fx_snapshot", status="sealed", content_sha256=HASH, created_at=STAMP)
    for ident in ("fx_concept", "fx_claim", "fx_atom"):
        insert(db, "snapshot_member", snapshot_id="fx_snapshot", entity_id=ident, revision=1, required=1)
    # Configuration payload validation belongs to the future CONFIG repository;
    # this test intentionally checks relational DDL, not full config semantics.
    insert(db, "config_revision", config_id="fx_project", revision=1, config_schema=1, content_sha256=HASH, config_json='{}', created_at=STAMP)
    insert(db, "style_profile", style_id="fx_style", revision=1, kind="detailed", spec_json='{}', created_at=STAMP)
    insert(db, "run", run_id="fx_run", project_id="fx_project", config_id="fx_project", config_revision=1,
           producer="fixture", mode="source", status="running", input_manifest_sha256=HASH, scope_json='{}', created_at=STAMP)
    insert(db, "run_source", run_id="fx_run", manifest_id="fx_manifest", role="input")
    insert(db, "coverage_record", run_id="fx_run", span_id="fx_span", disposition="included", stage="extracted", decision_source="fixture_review", policy_version="1")
    insert(db, "coverage_atom", run_id="fx_run", span_id="fx_span", atom_id="fx_atom", revision=1, required=1)
    insert(db, "note_plan", plan_id="fx_plan", run_id="fx_run", snapshot_id="fx_snapshot", style_id="fx_style", style_revision=1,
           revision=1, status="validated", content_sha256=HASH)
    insert(db, "planned_note", plan_id="fx_plan", note_id="fx_note", title="Fixture", relative_path="Fixture.md", path_key="fixture.md",
           note_type="concept", template_key="concept_v1", rationale="Fixture")
    insert(db, "planned_section", plan_id="fx_plan", section_id="fx_section", note_id="fx_note", heading="Definition", ordinal=0)
    insert(db, "note_node", plan_id="fx_plan", node_id="fx_node", section_id="fx_section", ordinal=0, kind="paragraph",
           origin="fixture", rationale="Fixture", payload_json='{"segments":[{"kind":"claim_ref","ref":"fx_claim"}]}')
    insert(db, "node_atom", plan_id="fx_plan", node_id="fx_node", atom_id="fx_atom", revision=1)
    insert(db, "canonical_placement", plan_id="fx_plan", concept_id="fx_concept", note_id="fx_note", section_id="fx_section", target_kind="heading")
    insert(db, "rendered_file", file_id="fx_file", plan_id="fx_plan", note_id="fx_note", relative_path="Fixture.md", path_key="fixture.md", blob_sha256=HASH, role="note")
    insert(db, "content_mapping", file_id="fx_file", plan_id="fx_plan", node_id="fx_node", mapping_no=0, atom_id="fx_atom", atom_revision=1, start_byte=0, end_byte=7)
    db.commit()


class SchemaBlueprintTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.addCleanup(self.db.close)
        self.db.executescript(SQL)
        seed(self.db)

    def test_valid_evidence_to_rendered_output_join(self):
        found = self.db.execute("""SELECT e.span_id, cm.file_id FROM evidence e
          JOIN claim_evidence ce ON ce.evidence_id=e.evidence_id
          JOIN atom_claim ac ON ac.claim_id=ce.claim_id AND ac.claim_revision=ce.revision
          JOIN content_mapping cm ON cm.atom_id=ac.atom_id AND cm.atom_revision=ac.atom_revision""").fetchall()
        self.assertEqual(found, [("fx_span", "fx_file")])
        self.assertEqual(self.db.execute("PRAGMA foreign_key_check").fetchall(), [])
        self.assertEqual(self.db.execute("PRAGMA integrity_check").fetchone()[0], "ok")
        self.assertEqual(self.db.execute("PRAGMA user_version").fetchone()[0], 1)

    def test_unknown_evidence_foreign_key_is_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "claim_evidence", claim_id="fx_claim", revision=1, evidence_id="missing",
                   role="refutes", review_status="pending", rationale="Invalid fixture")

    def test_second_canonical_placement_in_one_plan_is_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "canonical_placement", plan_id="fx_plan", concept_id="fx_concept", note_id="fx_note", target_kind="file")

    def test_illegal_runtime_state_is_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE run SET status='succeeded' WHERE run_id='fx_run'")

    def test_exclusion_requires_nonempty_reason_including_null(self):
        for reason in (None, "", "   "):
            with self.subTest(reason=reason), self.assertRaises(sqlite3.IntegrityError):
                self.db.execute("UPDATE coverage_record SET disposition='excluded',reason=?", (reason,))
        self.db.execute("UPDATE coverage_record SET disposition='excluded',reason='Reviewed formatting'")

    def test_unsealed_hash_null_cannot_be_marked_sealed(self):
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "knowledge_snapshot", snapshot_id="fx_invalid", status="sealed", content_sha256=None, created_at=STAMP)

    def test_duplicate_normalized_path_is_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "planned_note", plan_id="fx_plan", note_id="fx_note2", title="Second",
                   relative_path="FIXTURE.md", path_key="fixture.md", note_type="concept", template_key="concept_v1", rationale="Collision")

    def test_chinese_bigrams_and_latin_terms_are_searchable(self):
        self.db.execute("INSERT INTO source_span_search(span_id,text,zh_bigrams) VALUES(?,?,?)",
                        ("fx_span", "糖酵解 Glycolysis", "糖酵 酵解"))
        for query in ('zh_bigrams : "糖酵"', 'text : "glycolysis"'):
            self.assertEqual(self.db.execute("SELECT span_id FROM source_span_search WHERE source_span_search MATCH ?", (query,)).fetchall(), [("fx_span",)])

    def test_schema_zero_upgrade_preserves_import_identity(self):
        db = sqlite3.connect(":memory:")
        self.addCleanup(db.close)
        base = SQL.split("-- BEGIN_SCHEMA_0", 1)[1].split("-- END_SCHEMA_0", 1)[0]
        upgrade = SQL.split("-- BEGIN_SCHEMA_1", 1)[1].split("-- END_SCHEMA_1", 1)[0]
        db.executescript("PRAGMA foreign_keys=ON; BEGIN;" + base + "PRAGMA user_version=0; COMMIT;")
        insert(db, "project", project_id="fx_existing", name="Existing", created_at=STAMP, state="initialized")
        insert(db, "source", source_id="fx_old_source", project_id="fx_existing", display_name="Existing source", origin="local", created_at=STAMP)
        insert(db, "source_version", version_id="fx_old_version", source_id="fx_old_source", content_sha256=HASH, byte_length=7,
               media_type="text/plain", acquired_at=STAMP, location_json='{"kind":"local","storage":"reference","selected_path":"old.txt"}', availability="available")
        insert(db, "parse_manifest", manifest_id="fx_old_manifest", version_id="fx_old_version", parser_version="text_v1",
               normalization_version="utf8_v1", parameters_sha256=HASH, status="ready", created_at=STAMP)
        insert(db, "source_span", span_id="fx_old_span", manifest_id="fx_old_manifest", ordinal=0, locator_key="bytes:0:7", kind="text",
               start_byte=0, end_byte=7, start_line=1, end_line=1, text_sha256=HASH, status="parsed")
        insert(db, "import_attempt", import_id="fx_old_import", project_id="fx_existing", version_id="fx_old_version",
               requested_location_json='{"path":"old.txt"}', started_at=STAMP, ended_at=STAMP, status="ready")
        db.commit()
        db.executescript("BEGIN;" + upgrade + "PRAGMA user_version=1; COMMIT;")
        self.assertEqual(db.execute("SELECT project_id FROM project").fetchone()[0], "fx_existing")
        self.assertEqual(db.execute("SELECT span_id,start_byte,end_byte FROM source_span").fetchone(), ("fx_old_span", 0, 7))
        self.assertEqual(db.execute("SELECT version_id,content_sha256 FROM source_version").fetchone(), ("fx_old_version", HASH))
        self.assertEqual(db.execute("SELECT import_id,version_id FROM import_attempt").fetchone(), ("fx_old_import", "fx_old_version"))
        self.assertEqual(db.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_feedback_and_hop_limits_match_frozen_configuration(self):
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "validation_issue", issue_id="fx_issue", run_id="fx_run", rule_id="fixture",
                   severity="blocking", status="open", message="Invalid feedback count", feedback_round=3, created_at=STAMP)
        with self.assertRaises(sqlite3.IntegrityError):
            insert(self.db, "research_item", research_id="fx_research", run_id="fx_run", topic="Fixture", goal="Fixture",
                   scope_sha256=HASH, scope_class="authorized_goal", status="accepted", required=1,
                   hop=3, reason="Invalid hop", discovery_fingerprint=HASH)


if __name__ == "__main__":
    unittest.main()
