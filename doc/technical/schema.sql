-- Noetome storage schema 1 blueprint. Not a shipped database or migration.
-- A fresh database may execute this complete file. Existing schema 0 uses
-- only BEGIN_SCHEMA_1..END_SCHEMA_1 in a migration transaction after backup.
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;

-- BEGIN_SCHEMA_0
CREATE TABLE schema_migration(version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE project(
 project_id TEXT PRIMARY KEY, singleton INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK(singleton=1),
 name TEXT NOT NULL, created_at TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('initialized','active','archived'))
);
CREATE TABLE blob(
 sha256 TEXT PRIMARY KEY CHECK(length(sha256)=64), byte_length INTEGER NOT NULL CHECK(byte_length>=0),
 media_type TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE source(
 source_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(project_id),
 display_name TEXT NOT NULL, origin TEXT NOT NULL CHECK(origin IN ('local','verification','research','manual')),
 created_at TEXT NOT NULL, removed_at TEXT
);
CREATE TABLE source_version(
 version_id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES source(source_id),
 content_sha256 TEXT NOT NULL CHECK(length(content_sha256)=64), byte_length INTEGER NOT NULL CHECK(byte_length>=0),
 media_type TEXT NOT NULL, acquired_at TEXT NOT NULL, location_json TEXT NOT NULL CHECK(json_valid(location_json)),
 raw_blob_sha256 TEXT REFERENCES blob(sha256), availability TEXT NOT NULL CHECK(availability IN ('available','missing','restricted')),
 UNIQUE(source_id,content_sha256)
);
CREATE TABLE import_attempt(
 import_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(project_id), version_id TEXT REFERENCES source_version(version_id),
 requested_location_json TEXT NOT NULL CHECK(json_valid(requested_location_json)), started_at TEXT NOT NULL, ended_at TEXT,
 status TEXT NOT NULL CHECK(status IN ('pending','ready','failed','needs_input')), error_code TEXT, error_message TEXT,
 CHECK(status!='ready' OR version_id IS NOT NULL)
);
CREATE TABLE parse_manifest(
 manifest_id TEXT PRIMARY KEY, version_id TEXT NOT NULL REFERENCES source_version(version_id),
 parser_version TEXT NOT NULL, normalization_version TEXT NOT NULL, parameters_sha256 TEXT NOT NULL,
 normalized_blob_sha256 TEXT REFERENCES blob(sha256), status TEXT NOT NULL CHECK(status IN ('pending','ready','partial','failed')),
 page_count INTEGER CHECK(page_count>=0), created_at TEXT NOT NULL, error_code TEXT,
 UNIQUE(version_id,parser_version,normalization_version,parameters_sha256)
);
CREATE TABLE source_span(
 span_id TEXT PRIMARY KEY, manifest_id TEXT NOT NULL REFERENCES parse_manifest(manifest_id),
 parent_span_id TEXT REFERENCES source_span(span_id), ordinal INTEGER NOT NULL CHECK(ordinal>=0), locator_key TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('heading','text','table','formula','image','caption','footnote','code','formatting','unparsed')),
 start_byte INTEGER, end_byte INTEGER, start_line INTEGER, end_line INTEGER,
 text_blob_sha256 TEXT REFERENCES blob(sha256), text_sha256 TEXT, confidence REAL CHECK(confidence BETWEEN 0 AND 1),
 status TEXT NOT NULL CHECK(status IN ('parsed','failed','needs_review')), payload_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(payload_json)),
 UNIQUE(manifest_id,locator_key), UNIQUE(manifest_id,ordinal),
 CHECK((start_byte IS NULL AND end_byte IS NULL) OR (start_byte>=0 AND end_byte>=start_byte)),
 CHECK((start_line IS NULL AND end_line IS NULL) OR (start_line>=1 AND end_line>=start_line))
);
CREATE INDEX source_span_manifest ON source_span(manifest_id,ordinal);
CREATE INDEX source_version_hash ON source_version(content_sha256);
-- END_SCHEMA_0

-- BEGIN_SCHEMA_1
CREATE TABLE config_revision(
 config_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=1), config_schema INTEGER NOT NULL CHECK(config_schema=1),
 content_sha256 TEXT NOT NULL, config_json TEXT NOT NULL CHECK(json_valid(config_json)), created_at TEXT NOT NULL,
 PRIMARY KEY(config_id,revision)
);
CREATE TABLE style_profile(
 style_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=1), kind TEXT NOT NULL CHECK(kind IN ('minimal','detailed','custom')),
 spec_json TEXT NOT NULL CHECK(json_valid(spec_json)), reference_manifest_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(reference_manifest_json)),
 created_at TEXT NOT NULL, PRIMARY KEY(style_id,revision)
);
CREATE TABLE source_page(
 page_id TEXT PRIMARY KEY, manifest_id TEXT NOT NULL REFERENCES parse_manifest(manifest_id), physical_page INTEGER NOT NULL CHECK(physical_page>=1),
 printed_label TEXT, width_pt REAL NOT NULL CHECK(width_pt>0), height_pt REAL NOT NULL CHECK(height_pt>0),
 rotation INTEGER NOT NULL CHECK(rotation IN (0,90,180,270)), image_blob_sha256 TEXT REFERENCES blob(sha256),
 inspection_status TEXT NOT NULL CHECK(inspection_status IN ('pending','checked','needs_review','failed')),
 figure_detection TEXT NOT NULL CHECK(figure_detection IN ('known','uncertain','failed')), UNIQUE(manifest_id,physical_page)
);
CREATE TABLE span_region(
 span_id TEXT NOT NULL REFERENCES source_span(span_id), region_no INTEGER NOT NULL CHECK(region_no>=0),
 page_id TEXT NOT NULL REFERENCES source_page(page_id), x0 REAL NOT NULL, y0 REAL NOT NULL, x1 REAL NOT NULL, y1 REAL NOT NULL,
 PRIMARY KEY(span_id,region_no), CHECK(0<=x0 AND x0<x1 AND x1<=1 AND 0<=y0 AND y0<y1 AND y1<=1)
);
CREATE TABLE figure_inventory(
 figure_id TEXT PRIMARY KEY, page_id TEXT REFERENCES source_page(page_id), span_id TEXT NOT NULL REFERENCES source_span(span_id),
 figure_kind TEXT NOT NULL CHECK(figure_kind IN ('diagram','table','photo','curve','formula','decoration','unknown')),
 original_blob_sha256 TEXT REFERENCES blob(sha256), decision TEXT NOT NULL CHECK(decision IN ('pending','retain','extract','redraw','reuse','exclude','unresolved')),
 reason TEXT, reviewed_by TEXT, element_inventory_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(element_inventory_json)),
 UNIQUE(span_id), CHECK(decision='pending' OR (reason IS NOT NULL AND length(trim(reason))>0))
);
CREATE TABLE evidence(
 evidence_id TEXT PRIMARY KEY, span_id TEXT NOT NULL REFERENCES source_span(span_id), locator_kind TEXT NOT NULL CHECK(locator_kind IN ('text','region','table_cell','formula')),
 excerpt_start_byte INTEGER, excerpt_end_byte INTEGER, excerpt_sha256 TEXT, locator_json TEXT NOT NULL CHECK(json_valid(locator_json)),
 verification TEXT NOT NULL CHECK(verification IN ('unverified','matched','mismatch','unavailable')), checked_at TEXT,
 CHECK((excerpt_start_byte IS NULL AND excerpt_end_byte IS NULL) OR (excerpt_start_byte>=0 AND excerpt_end_byte>excerpt_start_byte))
);
CREATE INDEX evidence_span ON evidence(span_id);
CREATE TABLE entity(
 entity_id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('concept','claim','atom','relation','topic')),
 created_at TEXT NOT NULL, retired_at TEXT
);
CREATE TABLE entity_revision(
 entity_id TEXT NOT NULL REFERENCES entity(entity_id), revision INTEGER NOT NULL CHECK(revision>=1),
 content_sha256 TEXT NOT NULL, created_at TEXT NOT NULL, producer TEXT NOT NULL CHECK(producer IN ('fixture','deterministic','model','human')),
 reason TEXT NOT NULL, PRIMARY KEY(entity_id,revision)
);
CREATE TABLE concept_revision(
 concept_id TEXT NOT NULL, revision INTEGER NOT NULL, canonical_name TEXT NOT NULL, entity_class TEXT NOT NULL,
 disambiguation TEXT NOT NULL, definition_policy TEXT NOT NULL CHECK(definition_policy IN ('required','external','excluded')),
 PRIMARY KEY(concept_id,revision), FOREIGN KEY(concept_id,revision) REFERENCES entity_revision(entity_id,revision)
);
CREATE TABLE concept_alias(
 concept_id TEXT NOT NULL, revision INTEGER NOT NULL, alias TEXT NOT NULL, language TEXT NOT NULL, scope TEXT NOT NULL DEFAULT '',
 PRIMARY KEY(concept_id,revision,alias,language,scope), FOREIGN KEY(concept_id,revision) REFERENCES concept_revision(concept_id,revision)
);
CREATE INDEX concept_alias_lookup ON concept_alias(alias,language);
CREATE TABLE claim_revision(
 claim_id TEXT NOT NULL, revision INTEGER NOT NULL, subject_id TEXT NOT NULL REFERENCES entity(entity_id), predicate TEXT NOT NULL,
 value_kind TEXT NOT NULL CHECK(value_kind IN ('text','number','range','entity','expression','unknown','not_applicable')),
 value_json TEXT NOT NULL CHECK(json_valid(value_json)), unit TEXT, precision_json TEXT CHECK(precision_json IS NULL OR json_valid(precision_json)),
 context_sha256 TEXT NOT NULL, valid_from TEXT, valid_until TEXT,
 verification TEXT NOT NULL CHECK(verification IN ('unverified','supported','contested','refuted','insufficient','fixture_reviewed')),
 PRIMARY KEY(claim_id,revision), FOREIGN KEY(claim_id,revision) REFERENCES entity_revision(entity_id,revision)
);
CREATE INDEX claim_subject_predicate ON claim_revision(subject_id,predicate,context_sha256);
CREATE TABLE claim_condition(
 claim_id TEXT NOT NULL, revision INTEGER NOT NULL, condition_no INTEGER NOT NULL CHECK(condition_no>=0),
 dimension TEXT NOT NULL, operator TEXT NOT NULL CHECK(operator IN ('eq','neq','in','range','exists','text')),
 value_json TEXT NOT NULL CHECK(json_valid(value_json)), unit TEXT,
 PRIMARY KEY(claim_id,revision,condition_no), FOREIGN KEY(claim_id,revision) REFERENCES claim_revision(claim_id,revision)
);
CREATE TABLE claim_evidence(
 claim_id TEXT NOT NULL, revision INTEGER NOT NULL, evidence_id TEXT NOT NULL REFERENCES evidence(evidence_id),
 role TEXT NOT NULL CHECK(role IN ('supports','refutes','qualifies')), review_status TEXT NOT NULL CHECK(review_status IN ('pending','accepted','rejected')),
 rationale TEXT NOT NULL, PRIMARY KEY(claim_id,revision,evidence_id,role), FOREIGN KEY(claim_id,revision) REFERENCES claim_revision(claim_id,revision)
);
CREATE TABLE atom_revision(
 atom_id TEXT NOT NULL, revision INTEGER NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('definition','statement','formula','reaction','process','causal','classification','comparison','exception','derivation','example')),
 title TEXT NOT NULL, payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 verification TEXT NOT NULL CHECK(verification IN ('unverified','supported','contested','fixture_reviewed')),
 PRIMARY KEY(atom_id,revision), FOREIGN KEY(atom_id,revision) REFERENCES entity_revision(entity_id,revision)
);
CREATE TABLE atom_claim(
 atom_id TEXT NOT NULL, atom_revision INTEGER NOT NULL, claim_id TEXT NOT NULL, claim_revision INTEGER NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 PRIMARY KEY(atom_id,atom_revision,claim_id,claim_revision), UNIQUE(atom_id,atom_revision,ordinal),
 FOREIGN KEY(atom_id,atom_revision) REFERENCES atom_revision(atom_id,revision), FOREIGN KEY(claim_id,claim_revision) REFERENCES claim_revision(claim_id,revision)
);
CREATE TABLE atom_child(
 parent_id TEXT NOT NULL, parent_revision INTEGER NOT NULL, child_id TEXT NOT NULL, child_revision INTEGER NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 role TEXT NOT NULL CHECK(role IN ('step','premise','conclusion','exception','example')), PRIMARY KEY(parent_id,parent_revision,ordinal),
 FOREIGN KEY(parent_id,parent_revision) REFERENCES atom_revision(atom_id,revision), FOREIGN KEY(child_id,child_revision) REFERENCES atom_revision(atom_id,revision), CHECK(parent_id!=child_id)
);
CREATE TABLE atom_concept(
 atom_id TEXT NOT NULL, revision INTEGER NOT NULL, concept_id TEXT NOT NULL REFERENCES entity(entity_id), role TEXT NOT NULL,
 PRIMARY KEY(atom_id,revision,concept_id,role), FOREIGN KEY(atom_id,revision) REFERENCES atom_revision(atom_id,revision)
);
CREATE TABLE relation_revision(
 relation_id TEXT NOT NULL, revision INTEGER NOT NULL, from_id TEXT NOT NULL REFERENCES entity(entity_id), to_id TEXT NOT NULL REFERENCES entity(entity_id),
 predicate TEXT NOT NULL, claim_id TEXT NOT NULL, claim_revision INTEGER NOT NULL,
 PRIMARY KEY(relation_id,revision), FOREIGN KEY(relation_id,revision) REFERENCES entity_revision(entity_id,revision),
 FOREIGN KEY(claim_id,claim_revision) REFERENCES claim_revision(claim_id,revision)
);
CREATE TABLE topic_revision(
 topic_id TEXT NOT NULL, revision INTEGER NOT NULL, title TEXT NOT NULL, topic_type TEXT NOT NULL, template_key TEXT NOT NULL,
 PRIMARY KEY(topic_id,revision), FOREIGN KEY(topic_id,revision) REFERENCES entity_revision(entity_id,revision)
);
CREATE TABLE knowledge_snapshot(
 snapshot_id TEXT PRIMARY KEY, parent_snapshot_id TEXT REFERENCES knowledge_snapshot(snapshot_id),
 status TEXT NOT NULL CHECK(status IN ('building','sealed','rejected')), content_sha256 TEXT, created_at TEXT NOT NULL,
 CHECK(status!='sealed' OR (content_sha256 IS NOT NULL AND length(content_sha256)=64))
);
CREATE TABLE snapshot_member(
 snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshot(snapshot_id), entity_id TEXT NOT NULL, revision INTEGER NOT NULL,
 required INTEGER NOT NULL CHECK(required IN (0,1)), learning_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(learning_json)),
 PRIMARY KEY(snapshot_id,entity_id), FOREIGN KEY(entity_id,revision) REFERENCES entity_revision(entity_id,revision)
);
CREATE TABLE topic_member(
 snapshot_id TEXT NOT NULL, topic_id TEXT NOT NULL, member_id TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('primary','related','prerequisite')),
 PRIMARY KEY(snapshot_id,topic_id,member_id,role), FOREIGN KEY(snapshot_id,topic_id) REFERENCES snapshot_member(snapshot_id,entity_id),
 FOREIGN KEY(snapshot_id,member_id) REFERENCES snapshot_member(snapshot_id,entity_id)
);
CREATE TABLE topic_edge(
 snapshot_id TEXT NOT NULL, parent_id TEXT NOT NULL, child_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
 PRIMARY KEY(snapshot_id,parent_id,child_id), FOREIGN KEY(snapshot_id,parent_id) REFERENCES snapshot_member(snapshot_id,entity_id),
 FOREIGN KEY(snapshot_id,child_id) REFERENCES snapshot_member(snapshot_id,entity_id), CHECK(parent_id!=child_id)
);
CREATE TABLE identity_event(
 event_id TEXT PRIMARY KEY, action TEXT NOT NULL CHECK(action IN ('merge','undo_merge','split')), reverses_event_id TEXT REFERENCES identity_event(event_id),
 before_snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshot(snapshot_id), after_snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshot(snapshot_id),
 decision_json TEXT NOT NULL CHECK(json_valid(decision_json)), reason TEXT NOT NULL, actor TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE concept_resolution(
 snapshot_id TEXT NOT NULL, source_concept_id TEXT NOT NULL, canonical_concept_id TEXT NOT NULL, event_id TEXT REFERENCES identity_event(event_id),
 PRIMARY KEY(snapshot_id,source_concept_id), FOREIGN KEY(snapshot_id,source_concept_id) REFERENCES snapshot_member(snapshot_id,entity_id),
 FOREIGN KEY(snapshot_id,canonical_concept_id) REFERENCES snapshot_member(snapshot_id,entity_id)
);
CREATE TABLE span_alignment(
 alignment_id TEXT PRIMARY KEY, left_span_id TEXT NOT NULL REFERENCES source_span(span_id), right_span_id TEXT NOT NULL REFERENCES source_span(span_id),
 relation TEXT NOT NULL CHECK(relation IN ('same_topic','overlap','related','unmatched')), status TEXT NOT NULL CHECK(status IN ('candidate','accepted','rejected')),
 reason TEXT NOT NULL, created_at TEXT NOT NULL, UNIQUE(left_span_id,right_span_id,relation)
);
CREATE TABLE knowledge_conflict(
 conflict_id TEXT PRIMARY KEY, left_claim_id TEXT NOT NULL, left_revision INTEGER NOT NULL, right_claim_id TEXT NOT NULL, right_revision INTEGER NOT NULL,
 classification TEXT NOT NULL CHECK(classification IN ('duplicate','complement','condition_difference','historical','conflict','insufficient','parse_error')),
 status TEXT NOT NULL CHECK(status IN ('open','resolved','reopened')), policy TEXT NOT NULL, resolution_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(resolution_json)),
 FOREIGN KEY(left_claim_id,left_revision) REFERENCES claim_revision(claim_id,revision), FOREIGN KEY(right_claim_id,right_revision) REFERENCES claim_revision(claim_id,revision)
);
CREATE TABLE run(
 run_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(project_id), config_id TEXT NOT NULL, config_revision INTEGER NOT NULL,
 producer TEXT NOT NULL CHECK(producer IN ('fixture','production')), mode TEXT NOT NULL CHECK(mode IN ('source','research')),
 status TEXT NOT NULL CHECK(status IN ('draft','ready','running','pause_requested','paused','cancel_requested','blocked_input','blocked_config','budget_stopped','failed','cancelled','ended')),
 input_manifest_sha256 TEXT NOT NULL, scope_json TEXT NOT NULL CHECK(json_valid(scope_json)), created_at TEXT NOT NULL, ended_at TEXT,
 FOREIGN KEY(config_id,config_revision) REFERENCES config_revision(config_id,revision)
);
CREATE TABLE run_source(
 run_id TEXT NOT NULL REFERENCES run(run_id), manifest_id TEXT NOT NULL REFERENCES parse_manifest(manifest_id), role TEXT NOT NULL CHECK(role IN ('input','verification','research')),
 PRIMARY KEY(run_id,manifest_id)
);
CREATE TABLE coverage_record(
 run_id TEXT NOT NULL REFERENCES run(run_id), span_id TEXT NOT NULL REFERENCES source_span(span_id),
 disposition TEXT NOT NULL CHECK(disposition IN ('pending','included','excluded','duplicate','failed','needs_review')),
 stage TEXT NOT NULL CHECK(stage IN ('registered','extracted','normalized','planned','rendered','validated')),
 duplicate_of_span_id TEXT REFERENCES source_span(span_id), reason TEXT, decision_source TEXT NOT NULL, policy_version TEXT NOT NULL,
 PRIMARY KEY(run_id,span_id), CHECK(disposition!='excluded' OR (reason IS NOT NULL AND length(trim(reason))>0)),
 CHECK(disposition!='duplicate' OR (duplicate_of_span_id IS NOT NULL AND duplicate_of_span_id!=span_id))
);
CREATE TABLE coverage_atom(
 run_id TEXT NOT NULL, span_id TEXT NOT NULL, atom_id TEXT NOT NULL, revision INTEGER NOT NULL, required INTEGER NOT NULL CHECK(required IN (0,1)),
 PRIMARY KEY(run_id,span_id,atom_id), FOREIGN KEY(run_id,span_id) REFERENCES coverage_record(run_id,span_id), FOREIGN KEY(atom_id,revision) REFERENCES atom_revision(atom_id,revision)
);
CREATE TABLE provider_profile(
 provider_id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('fake','openai_compatible','agent','local')),
 base_url TEXT, model TEXT, credential_ref TEXT, capability_profile_id TEXT, profile_json TEXT NOT NULL CHECK(json_valid(profile_json))
);
CREATE TABLE capability_profile(
 capability_profile_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL REFERENCES provider_profile(provider_id), config_sha256 TEXT NOT NULL,
 tested_at TEXT NOT NULL, expires_at TEXT NOT NULL, capabilities_json TEXT NOT NULL CHECK(json_valid(capabilities_json)),
 evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)), UNIQUE(provider_id,config_sha256,tested_at)
);
CREATE TABLE job(
 job_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), kind TEXT NOT NULL, input_sha256 TEXT NOT NULL, input_json TEXT NOT NULL CHECK(json_valid(input_json)),
 status TEXT NOT NULL CHECK(status IN ('queued','running','retry_wait','blocked','succeeded','failed','cancelled','skipped','uncertain')),
 idempotency_key TEXT NOT NULL UNIQUE, attempt_no INTEGER NOT NULL DEFAULT 0 CHECK(attempt_no>=0), max_attempts INTEGER NOT NULL DEFAULT 3 CHECK(max_attempts BETWEEN 1 AND 3),
 lease_owner TEXT, lease_expires_at TEXT, lease_generation INTEGER NOT NULL DEFAULT 0 CHECK(lease_generation>=0),
 next_attempt_at TEXT, result_blob_sha256 TEXT REFERENCES blob(sha256), error_code TEXT, created_at TEXT NOT NULL
);
CREATE INDEX job_queue ON job(run_id,status,next_attempt_at);
CREATE TABLE job_dependency(job_id TEXT NOT NULL REFERENCES job(job_id), depends_on TEXT NOT NULL REFERENCES job(job_id), PRIMARY KEY(job_id,depends_on), CHECK(job_id!=depends_on));
CREATE TABLE budget_reservation(
 reservation_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), job_id TEXT NOT NULL REFERENCES job(job_id),
 status TEXT NOT NULL CHECK(status IN ('held','settled','released','unknown')), reserved_tokens INTEGER NOT NULL CHECK(reserved_tokens>=0),
 reserved_cost_micros INTEGER CHECK(reserved_cost_micros>=0), currency TEXT, reserved_pages INTEGER NOT NULL DEFAULT 0 CHECK(reserved_pages>=0),
 reserved_requests INTEGER NOT NULL DEFAULT 1 CHECK(reserved_requests>=0), actual_requests INTEGER CHECK(actual_requests>=0),
 actual_tokens INTEGER CHECK(actual_tokens>=0), actual_cost_micros INTEGER CHECK(actual_cost_micros>=0), price_snapshot_json TEXT CHECK(price_snapshot_json IS NULL OR json_valid(price_snapshot_json)),
 created_at TEXT NOT NULL, settled_at TEXT
);
CREATE TABLE external_attempt(
 attempt_id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES job(job_id), attempt_no INTEGER NOT NULL CHECK(attempt_no BETWEEN 1 AND 3),
 provider_id TEXT REFERENCES provider_profile(provider_id), kind TEXT NOT NULL CHECK(kind IN ('model','search','fetch','agent')),
 reservation_id TEXT NOT NULL UNIQUE REFERENCES budget_reservation(reservation_id),
 status TEXT NOT NULL CHECK(status IN ('prepared','dispatched','succeeded','failed','outcome_unknown')),
 request_sha256 TEXT NOT NULL, request_blob_sha256 TEXT REFERENCES blob(sha256), response_blob_sha256 TEXT REFERENCES blob(sha256),
 provider_request_id TEXT, usage_json TEXT CHECK(usage_json IS NULL OR json_valid(usage_json)), error_code TEXT,
 dispatched_at TEXT, finished_at TEXT, UNIQUE(job_id,attempt_no)
);
CREATE TABLE research_item(
 research_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), parent_research_id TEXT REFERENCES research_item(research_id),
 topic TEXT NOT NULL, goal TEXT NOT NULL, scope_sha256 TEXT NOT NULL, scope_class TEXT NOT NULL CHECK(scope_class IN ('authorized_goal','required_prerequisite','new_candidate')),
 status TEXT NOT NULL CHECK(status IN ('candidate','accepted','rejected','deferred','running','satisfied','stopped','blocked_config')),
 required INTEGER NOT NULL CHECK(required IN (0,1)), hop INTEGER NOT NULL CHECK(hop BETWEEN 0 AND 2), reason TEXT NOT NULL,
 approval_id TEXT, stop_reason TEXT, discovery_fingerprint TEXT NOT NULL, UNIQUE(run_id,discovery_fingerprint),
 CHECK(scope_class!='new_candidate' OR required=0)
);
CREATE TABLE research_result(
 research_id TEXT NOT NULL REFERENCES research_item(research_id), attempt_id TEXT NOT NULL REFERENCES external_attempt(attempt_id),
 url TEXT NOT NULL, canonical_url TEXT NOT NULL, rank INTEGER NOT NULL, source_version_id TEXT REFERENCES source_version(version_id),
 status TEXT NOT NULL CHECK(status IN ('discovered','fetched','rejected','unavailable')), reason TEXT,
 PRIMARY KEY(research_id,canonical_url)
);
CREATE TABLE note_plan(
 plan_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshot(snapshot_id),
 style_id TEXT NOT NULL, style_revision INTEGER NOT NULL, revision INTEGER NOT NULL CHECK(revision>=1), parent_plan_id TEXT REFERENCES note_plan(plan_id),
 status TEXT NOT NULL CHECK(status IN ('candidate','validated','rejected')), content_sha256 TEXT NOT NULL,
 FOREIGN KEY(style_id,style_revision) REFERENCES style_profile(style_id,revision)
);
CREATE TABLE planned_note(
 plan_id TEXT NOT NULL REFERENCES note_plan(plan_id), note_id TEXT NOT NULL, title TEXT NOT NULL, relative_path TEXT NOT NULL, path_key TEXT NOT NULL,
 note_type TEXT NOT NULL, template_key TEXT NOT NULL, rationale TEXT NOT NULL,
 PRIMARY KEY(plan_id,note_id), UNIQUE(plan_id,path_key)
);
CREATE TABLE planned_section(
 plan_id TEXT NOT NULL, section_id TEXT NOT NULL, note_id TEXT NOT NULL, heading TEXT NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 PRIMARY KEY(plan_id,section_id), UNIQUE(plan_id,note_id,ordinal), FOREIGN KEY(plan_id,note_id) REFERENCES planned_note(plan_id,note_id)
);
CREATE TABLE note_node(
 plan_id TEXT NOT NULL, node_id TEXT NOT NULL, section_id TEXT NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 kind TEXT NOT NULL CHECK(kind IN ('paragraph','ordered_list','bullet_list','table','callout','formula','flowchart','sequence','timeline','image','embed')),
 origin TEXT NOT NULL CHECK(origin IN ('fixture','source_reproduction','expression_optimization')), rationale TEXT NOT NULL,
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), PRIMARY KEY(plan_id,node_id), UNIQUE(plan_id,section_id,ordinal),
 FOREIGN KEY(plan_id,section_id) REFERENCES planned_section(plan_id,section_id)
);
CREATE TABLE node_atom(
 plan_id TEXT NOT NULL, node_id TEXT NOT NULL, atom_id TEXT NOT NULL, revision INTEGER NOT NULL,
 PRIMARY KEY(plan_id,node_id,atom_id), FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id), FOREIGN KEY(atom_id,revision) REFERENCES atom_revision(atom_id,revision)
);
CREATE TABLE node_element(
 plan_id TEXT NOT NULL, node_id TEXT NOT NULL, element_id TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('label','statement','edge','cell','symbol','legend')),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), PRIMARY KEY(plan_id,node_id,element_id), FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id)
);
CREATE TABLE element_atom(
 plan_id TEXT NOT NULL, node_id TEXT NOT NULL, element_id TEXT NOT NULL, atom_id TEXT NOT NULL, revision INTEGER NOT NULL,
 PRIMARY KEY(plan_id,node_id,element_id,atom_id), FOREIGN KEY(plan_id,node_id,element_id) REFERENCES node_element(plan_id,node_id,element_id),
 FOREIGN KEY(atom_id,revision) REFERENCES atom_revision(atom_id,revision)
);
CREATE TABLE canonical_placement(
 plan_id TEXT NOT NULL, concept_id TEXT NOT NULL REFERENCES entity(entity_id), note_id TEXT NOT NULL, section_id TEXT, node_id TEXT,
 target_kind TEXT NOT NULL CHECK(target_kind IN ('file','heading','block')), block_id TEXT,
 PRIMARY KEY(plan_id,concept_id), FOREIGN KEY(plan_id,note_id) REFERENCES planned_note(plan_id,note_id),
 FOREIGN KEY(plan_id,section_id) REFERENCES planned_section(plan_id,section_id), FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id),
 CHECK((target_kind='file') OR (target_kind='heading' AND section_id IS NOT NULL) OR (target_kind='block' AND node_id IS NOT NULL AND block_id IS NOT NULL))
);
CREATE TABLE semantic_link(
 plan_id TEXT NOT NULL, node_id TEXT NOT NULL, link_id TEXT NOT NULL, target_entity_id TEXT NOT NULL REFERENCES entity(entity_id),
 display_text TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('reference','embed')), PRIMARY KEY(plan_id,link_id), FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id)
);
CREATE TABLE visual_asset(
 asset_id TEXT PRIMARY KEY, blob_sha256 TEXT NOT NULL REFERENCES blob(sha256), origin TEXT NOT NULL CHECK(origin IN ('original','redraw','generated')),
 figure_id TEXT REFERENCES figure_inventory(figure_id), plan_id TEXT, node_id TEXT, format TEXT NOT NULL CHECK(format IN ('svg','png','jpeg','webp')),
 verification TEXT NOT NULL CHECK(verification IN ('pending','passed','failed','fallback')), checks_json TEXT NOT NULL CHECK(json_valid(checks_json)),
 FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id)
);
CREATE TABLE structure_lock(
 lock_id TEXT PRIMARY KEY, target_kind TEXT NOT NULL CHECK(target_kind IN ('directory','note','section','order','placement','content')),
 target_key TEXT NOT NULL, fields_json TEXT NOT NULL CHECK(json_valid(fields_json)), expected_sha256 TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>=1), active INTEGER NOT NULL CHECK(active IN (0,1)), created_at TEXT NOT NULL
);
CREATE TABLE rendered_file(
 file_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL REFERENCES note_plan(plan_id), note_id TEXT, relative_path TEXT NOT NULL, path_key TEXT NOT NULL,
 blob_sha256 TEXT NOT NULL REFERENCES blob(sha256), role TEXT NOT NULL CHECK(role IN ('note','asset','index')),
 UNIQUE(plan_id,path_key), UNIQUE(file_id,plan_id), FOREIGN KEY(plan_id,note_id) REFERENCES planned_note(plan_id,note_id)
);
CREATE TABLE content_mapping(
 file_id TEXT NOT NULL, plan_id TEXT NOT NULL, node_id TEXT NOT NULL, mapping_no INTEGER NOT NULL CHECK(mapping_no>=0),
 atom_id TEXT NOT NULL, atom_revision INTEGER NOT NULL, element_id TEXT, start_byte INTEGER NOT NULL CHECK(start_byte>=0), end_byte INTEGER NOT NULL,
 PRIMARY KEY(file_id,mapping_no), FOREIGN KEY(file_id,plan_id) REFERENCES rendered_file(file_id,plan_id),
 FOREIGN KEY(plan_id,node_id) REFERENCES note_node(plan_id,node_id), FOREIGN KEY(atom_id,atom_revision) REFERENCES atom_revision(atom_id,revision), CHECK(end_byte>start_byte)
);
CREATE TABLE dependency_edge(
 from_kind TEXT NOT NULL, from_id TEXT NOT NULL, from_revision TEXT NOT NULL,
 to_kind TEXT NOT NULL, to_id TEXT NOT NULL, to_revision TEXT NOT NULL, reason TEXT NOT NULL,
 PRIMARY KEY(from_kind,from_id,from_revision,to_kind,to_id,to_revision)
);
CREATE INDEX dependency_reverse ON dependency_edge(to_kind,to_id,to_revision);
CREATE TABLE validation_result(
 validation_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), plan_id TEXT REFERENCES note_plan(plan_id),
 candidate_manifest_sha256 TEXT NOT NULL, validator_version TEXT NOT NULL, checks_json TEXT NOT NULL CHECK(json_valid(checks_json)),
 status TEXT NOT NULL CHECK(status IN ('running','passed','failed','not_run')), created_at TEXT NOT NULL
);
CREATE TABLE validation_issue(
 issue_id TEXT PRIMARY KEY, validation_id TEXT REFERENCES validation_result(validation_id), run_id TEXT NOT NULL REFERENCES run(run_id),
 rule_id TEXT NOT NULL, severity TEXT NOT NULL CHECK(severity IN ('info','warning','blocking')), status TEXT NOT NULL CHECK(status IN ('open','resolved','waived','reopened')),
 entity_id TEXT REFERENCES entity(entity_id), span_id TEXT REFERENCES source_span(span_id), file_id TEXT REFERENCES rendered_file(file_id),
 message TEXT NOT NULL, resolution TEXT, feedback_round INTEGER NOT NULL DEFAULT 0 CHECK(feedback_round BETWEEN 0 AND 2), created_at TEXT NOT NULL
);
CREATE INDEX issue_open ON validation_issue(run_id,status,severity);
CREATE TABLE release(
 release_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES run(run_id), plan_id TEXT NOT NULL REFERENCES note_plan(plan_id),
 parent_release_id TEXT REFERENCES release(release_id), validation_id TEXT NOT NULL REFERENCES validation_result(validation_id),
 status TEXT NOT NULL CHECK(status IN ('staged','verified','publishing','published','recovery_required','rolled_back')),
 manifest_sha256 TEXT NOT NULL, target_descriptor_json TEXT NOT NULL CHECK(json_valid(target_descriptor_json)), created_at TEXT NOT NULL, published_at TEXT
);
CREATE TABLE change_set(
 change_set_id TEXT PRIMARY KEY, release_id TEXT NOT NULL REFERENCES release(release_id),
 base_manifest_sha256 TEXT NOT NULL, current_manifest_sha256 TEXT NOT NULL, candidate_manifest_sha256 TEXT NOT NULL,
 change_set_sha256 TEXT NOT NULL UNIQUE, status TEXT NOT NULL CHECK(status IN ('candidate','pending_approval','approved','rejected','stale','applied')),
 created_at TEXT NOT NULL
);
CREATE TABLE file_change(
 change_set_id TEXT NOT NULL REFERENCES change_set(change_set_id), change_id TEXT NOT NULL, operation TEXT NOT NULL CHECK(operation IN ('create','replace','move','delete')),
 old_path TEXT, new_path TEXT, base_sha256 TEXT, current_sha256 TEXT, candidate_sha256 TEXT REFERENCES blob(sha256),
 human_affected INTEGER NOT NULL CHECK(human_affected IN (0,1)), status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','stale','applied')),
 PRIMARY KEY(change_set_id,change_id)
);
CREATE TABLE change_dependency(
 change_set_id TEXT NOT NULL, change_id TEXT NOT NULL, depends_on TEXT NOT NULL,
 PRIMARY KEY(change_set_id,change_id,depends_on), FOREIGN KEY(change_set_id,change_id) REFERENCES file_change(change_set_id,change_id),
 FOREIGN KEY(change_set_id,depends_on) REFERENCES file_change(change_set_id,change_id), CHECK(change_id!=depends_on)
);
CREATE TABLE approval(
 approval_id TEXT PRIMARY KEY, change_set_id TEXT NOT NULL REFERENCES change_set(change_set_id), change_set_sha256 TEXT NOT NULL,
 current_manifest_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('pending','accepted','rejected','stale')),
 actor TEXT, decision_at TEXT, note TEXT, selected_changes_json TEXT NOT NULL CHECK(json_valid(selected_changes_json))
);
CREATE TABLE release_file(
 release_id TEXT NOT NULL REFERENCES release(release_id), path_key TEXT NOT NULL, relative_path TEXT NOT NULL,
 blob_sha256 TEXT NOT NULL REFERENCES blob(sha256), file_id TEXT REFERENCES rendered_file(file_id),
 ownership TEXT NOT NULL CHECK(ownership IN ('generated','human','unmanaged')), PRIMARY KEY(release_id,path_key)
);
CREATE TABLE publication_step(
 release_id TEXT NOT NULL REFERENCES release(release_id), sequence_no INTEGER NOT NULL CHECK(sequence_no>=0),
 action TEXT NOT NULL, relative_path TEXT NOT NULL, expected_sha256 TEXT, candidate_sha256 TEXT, backup_path TEXT,
 state TEXT NOT NULL CHECK(state IN ('intent','old_backed_up','new_installed','verified','compensated','recovery_required')),
 created_at TEXT NOT NULL, PRIMARY KEY(release_id,sequence_no)
);
CREATE TABLE completion_dimension(
 run_id TEXT NOT NULL REFERENCES run(run_id), dimension TEXT NOT NULL CHECK(dimension IN ('source_coverage','knowledge_compilation','quality_validation','optional_research')),
 status TEXT NOT NULL CHECK(status IN ('complete','incomplete','not_applicable')), required INTEGER NOT NULL CHECK(required>=0),
 total INTEGER NOT NULL CHECK(total>=0), done INTEGER NOT NULL CHECK(done>=0), reason_codes_json TEXT NOT NULL CHECK(json_valid(reason_codes_json)),
 release_id TEXT REFERENCES release(release_id), PRIMARY KEY(run_id,dimension), CHECK(done<=total AND required<=total)
);
CREATE TABLE export_bundle(
 export_id TEXT PRIMARY KEY, release_id TEXT NOT NULL REFERENCES release(release_id), schema_version INTEGER NOT NULL CHECK(schema_version=1),
 manifest_blob_sha256 TEXT NOT NULL REFERENCES blob(sha256), includes_sources INTEGER NOT NULL CHECK(includes_sources IN (0,1)), created_at TEXT NOT NULL
);
CREATE TABLE audit_event(
 event_id TEXT PRIMARY KEY, run_id TEXT REFERENCES run(run_id), kind TEXT NOT NULL, actor TEXT NOT NULL,
 object_refs_json TEXT NOT NULL CHECK(json_valid(object_refs_json)), payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), created_at TEXT NOT NULL
);
CREATE INDEX audit_run_time ON audit_event(run_id,created_at);
CREATE INDEX budget_active ON budget_reservation(run_id,status);
CREATE INDEX coverage_status ON coverage_record(run_id,disposition,stage);
CREATE INDEX snapshot_entity ON snapshot_member(entity_id,revision);
CREATE VIRTUAL TABLE source_span_search USING fts5(span_id UNINDEXED, text, zh_bigrams, tokenize='unicode61 remove_diacritics 0');
CREATE TABLE command_receipt(
 client_command_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(project_id), command_kind TEXT NOT NULL,
 request_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('accepted','completed','failed')),
 result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)), created_at TEXT NOT NULL, completed_at TEXT
);
CREATE TABLE project_event(
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE, run_id TEXT REFERENCES run(run_id),
 kind TEXT NOT NULL, payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), created_at TEXT NOT NULL
);
CREATE TABLE project_head(
 project_id TEXT PRIMARY KEY REFERENCES project(project_id), revision INTEGER NOT NULL CHECK(revision>=1),
 current_snapshot_id TEXT REFERENCES knowledge_snapshot(snapshot_id), current_plan_id TEXT REFERENCES note_plan(plan_id),
 current_release_id TEXT REFERENCES release(release_id), updated_at TEXT NOT NULL
);
-- END_SCHEMA_1
PRAGMA user_version = 1;
COMMIT;
