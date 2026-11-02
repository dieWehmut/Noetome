//! Synthetic persistence-contract fixtures. These do not test an alternate PDF
//! parser or establish text/scientific quality; the ordinary CLI owns that gate.
use super::*;
use rusqlite::params;
use serde_json::json;

#[cfg(windows)]
mod source_cache_races {
    include!("source_cache_race_tests.rs");
}

const PROJECT: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e66";
const SOURCE: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e67";
const VERSION: &str = "37d03d00-ff4c-4bdf-b126-969a4b073e3f";
const PARSE: &str = "87d03d00-ff4c-4bdf-b126-969a4b073e3f";
const IMPORT: &str = "12facf7a-4fbd-42a7-8c2f-f96208bf740f";
const TIME: &str = "2026-11-02T00:00:00Z";
const RAW: &[u8] = b"%PDF-1.4\nsynthetic controlled raw capture\n";

fn fingerprint() -> Fingerprint {
    Fingerprint {
        sha256: crate::text::sha256(RAW),
        byte_length: RAW.len() as u64,
    }
}

fn not_attempted() -> serde_json::Value {
    json!({
        "parse_id":null,"reused":false,"status":"not_attempted",
        "requested_parser":null,"parser":null,"protocol_version":null,
        "import_options_version":null,"adapter_sha256":null,"cache_key":null,
        "page_count":null,"confirmed_page_count":0,"parsed_page_count":0,
        "needs_review_page_count":0,"failed_page_count":0,"region_count":0,
        "snapshot_path":null,"outcome_kind":null,"outcome_sha256":null,
        "manifest_sha256":null,"issues":[],"error":null
    })
}

#[test]
fn nullable_pdf_report_fields_are_required_and_unknown_fields_are_rejected() {
    let complete = not_attempted();
    assert!(serde_json::from_value::<PdfStatus>(complete.clone()).is_ok());
    for field in [
        "parse_id",
        "parser",
        "requested_parser",
        "protocol_version",
        "page_count",
        "import_options_version",
        "adapter_sha256",
        "cache_key",
        "snapshot_path",
        "outcome_kind",
        "outcome_sha256",
        "manifest_sha256",
        "error",
    ] {
        let mut missing = complete.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PdfStatus>(missing).is_err(),
            "missing nullable {field} accepted"
        );
    }
    let mut unknown = complete;
    unknown["knowledge_complete"] = json!(true);
    assert!(serde_json::from_value::<PdfStatus>(unknown).is_err());
}

#[test]
fn cache_key_binds_every_parser_component_and_import_option() {
    let input = fingerprint();
    let parser = ParserIdentity::pinned();
    let adapter = "a".repeat(64);
    let base = cache_key(&input, &parser, &adapter, "pdf-import-default-v1").unwrap();
    // Independently generated with Python hashlib over the hand-authored,
    // sorted compact JSON contract (42-byte synthetic input).
    assert_eq!(
        base,
        "532f83799b2be66b4137acaef649b1b2e296055a1a0d847d284643ce8c4ac3f3"
    );
    assert_eq!(base.len(), 64);
    let mut variants = Vec::new();
    let mut changed = input.clone();
    changed.sha256 = "b".repeat(64);
    variants.push(cache_key(&changed, &parser, &adapter, "pdf-import-default-v1").unwrap());
    let mut changed = input.clone();
    changed.byte_length += 1;
    variants.push(cache_key(&changed, &parser, &adapter, "pdf-import-default-v1").unwrap());
    for field in [
        "engine",
        "engine_version",
        "engine_dependency",
        "engine_dependency_version",
        "adapter_version",
        "options_version",
        "python_version",
    ] {
        let mut value = serde_json::to_value(&parser).unwrap();
        value[field] = json!(format!("synthetic-contract-{field}-v2"));
        let alternative: ParserIdentity = serde_json::from_value(value).unwrap();
        variants.push(cache_key(&input, &alternative, &adapter, "pdf-import-default-v1").unwrap());
    }
    variants.push(cache_key(&input, &parser, &"c".repeat(64), "pdf-import-default-v1").unwrap());
    variants.push(cache_key(&input, &parser, &adapter, "pdf-import-fresh-v1").unwrap());
    let distinct: BTreeSet<_> = variants.iter().collect();
    assert_eq!(distinct.len(), variants.len());
    assert!(!variants.contains(&base));
    assert!(cache_key(&input, &parser, &adapter, "unknown-option").is_err());
    assert!(cache_key(&input, &parser, "bad-sha", "pdf-import-default-v1").is_err());
}

fn host_database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../../src/source_schema3.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO project_metadata VALUES(1,?1,3,?2,'initialized')",
            params![PROJECT, TIME],
        )
        .unwrap();
    connection
        .execute("INSERT INTO sources VALUES(?1,?2)", params![SOURCE, TIME])
        .unwrap();
    let raw = fingerprint();
    connection.execute("INSERT INTO source_versions VALUES(?1,?2,1,?3,'application/pdf','synthetic.pdf',?4,?5,NULL,NULL,'acquired',?6,NULL,NULL,NULL)",
        params![VERSION,SOURCE,TIME,raw.sha256,i64::try_from(raw.byte_length).unwrap(),format!(".noetome-sources/{VERSION}/raw.pdf")]).unwrap();
    // Fixture values are hand-authored and intentionally not a real parser result.
    connection.execute("INSERT INTO pdf_parses VALUES(?1,?2,?3,?4,1,?5,NULL,'pdf-import-default-v1',?6,'failed','host_failed',NULL,0,0,0,0,0,?7,?8,NULL,'PDF_RUNTIME','controlled runtime unavailable')",
        params![PARSE,VERSION,TIME,"532f83799b2be66b4137acaef649b1b2e296055a1a0d847d284643ce8c4ac3f3",serde_json::to_string(&ParserIdentity::pinned()).unwrap(),"a".repeat(64),format!(".noetome-pdf-parses/{PARSE}"),crate::text::sha256(&failure_bytes())]).unwrap();
    connection.execute("INSERT INTO pdf_resources VALUES(?1,'failure-json','failure.json',?2,?3,'utf8','application/json','diagnostic')",
        params![PARSE,crate::text::sha256(&failure_bytes()),i64::try_from(failure_bytes().len()).unwrap()]).unwrap();
    connection.execute("INSERT INTO pdf_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message) VALUES(1,?1,?2,'D:/selected/synthetic.pdf',?3,?4,?5,'failed',0,0,'acquired','PDF_RUNTIME','controlled runtime unavailable')",
        params![IMPORT,TIME,SOURCE,VERSION,PARSE]).unwrap();
    connection
}

fn failure_bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":1,"report_kind":"pdf_parse_failure","status":"failed",
        "raw_acquisition":"not_confirmed","input_fingerprint":null,"page_count":null,"confirmed_pages":[],
        "error":{"code":"PDF_RUNTIME","message":"controlled runtime unavailable","diagnostics":""}})).unwrap()
}

#[test]
fn shared_store_loads_complete_pdf_edges_before_validating_version_history() {
    let connection = host_database();
    let result = crate::source_model::Store::load(&connection, PROJECT);
    assert!(
        result.is_ok(),
        "complete synthetic PDF history rejected: {result:?}"
    );
}

#[test]
fn shared_pdf_reader_rejects_orphans_mismatched_keys_and_untruthful_states() {
    crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    for sql in [
        "DELETE FROM pdf_imports",
        "DELETE FROM pdf_resources",
        "UPDATE pdf_imports SET parse_reused=1",
        "UPDATE pdf_imports SET reused=1",
        "UPDATE pdf_imports SET rowid=0",
        "UPDATE pdf_imports SET status='ready', error_code=NULL, error_message=NULL",
        "UPDATE pdf_parses SET cache_key=printf('%064d',0)",
        "UPDATE pdf_parses SET import_options_version='unknown'",
        "UPDATE pdf_parses SET page_count=1, confirmed_page_count=1, parsed_page_count=1",
        "UPDATE pdf_parses SET source_version_id='5232018c-42e9-4db6-8e2e-58d8e40d9e68'",
        "UPDATE pdf_parses SET snapshot_path='.noetome-pdf-parses/unregistered'",
        "UPDATE source_versions SET normalized_snapshot='fake-normalized.txt'",
        "UPDATE pdf_resources SET confidence='confirmed'",
        "UPDATE pdf_resources SET relative_path='../external.txt'",
        "UPDATE pdf_resources SET resource_id='unregistered-failure'",
        "UPDATE pdf_imports SET version_id=NULL, parse_id=NULL, acquisition_status='failed', reused=0, parse_reused=0",
    ] {
        let connection = host_database();
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON; PRAGMA foreign_keys=OFF;")
            .unwrap();
        connection.execute_batch(sql).unwrap();
        assert!(
            crate::source_model::Store::load(&connection, PROJECT).is_err(),
            "accepted {sql}"
        );
    }
}

struct Sandbox(std::path::PathBuf);
impl Sandbox {
    fn new() -> Self {
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pdf-source-model-tests");
        std::fs::create_dir_all(&base).unwrap();
        let root = base
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn host_snapshots(root: &Path) {
    let sources = root.join(".noetome-sources");
    std::fs::create_dir(&sources).unwrap();
    std::fs::write(
        sources.join(".owner.json"),
        format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\"}}"),
    )
    .unwrap();
    std::fs::create_dir(sources.join(VERSION)).unwrap();
    std::fs::write(sources.join(VERSION).join("raw.pdf"), RAW).unwrap();
    let parses = root.join(".noetome-pdf-parses");
    std::fs::create_dir(&parses).unwrap();
    std::fs::write(
        parses.join(".owner.json"),
        format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\"}}"),
    )
    .unwrap();
    std::fs::create_dir(parses.join(PARSE)).unwrap();
    std::fs::write(parses.join(PARSE).join("failure.json"), failure_bytes()).unwrap();
}

#[test]
fn controlled_raw_capture_and_host_failure_are_checked_without_claiming_pages() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    let raw = sandbox
        .0
        .join(".noetome-sources")
        .join(VERSION)
        .join("raw.pdf");
    let before = std::fs::metadata(&raw).unwrap().modified().unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    assert_eq!(std::fs::metadata(&raw).unwrap().modified().unwrap(), before);
    assert_eq!(std::fs::read(&raw).unwrap(), RAW);
    std::fs::write(&raw, b"changed controlled bytes").unwrap();
    assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
}

#[test]
fn pdf_snapshot_reader_rejects_unknown_content_and_hardlinked_owners_and_raw() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    let cache = sandbox.0.join(".noetome-pdf-parses");
    std::fs::write(cache.join("human.txt"), b"human sentinel").unwrap();
    assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
    assert_eq!(
        std::fs::read(cache.join("human.txt")).unwrap(),
        b"human sentinel"
    );
    std::fs::remove_file(cache.join("human.txt")).unwrap();
    for path in [
        cache.join(".owner.json"),
        cache.join(PARSE).join("failure.json"),
        sandbox.0.join(".noetome-sources/.owner.json"),
        sandbox
            .0
            .join(".noetome-sources")
            .join(VERSION)
            .join("raw.pdf"),
    ] {
        let alias = sandbox.0.join("outside-original.bin");
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(
            store.validate_snapshots(&sandbox.0, PROJECT, None).is_err(),
            "hardlink accepted: {}",
            path.display()
        );
        std::fs::remove_file(alias).unwrap();
        store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    }
}

#[test]
fn report_hydration_rejects_caller_altered_sql_facts() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    let mut report = store.pdf.reports[0].clone();
    report.pending_extraction_count = 999;
    report.pdf.status = "parsed".into();
    assert!(
        store
            .pdf
            .report_from_snapshot(&sandbox.0, &report, None)
            .is_err(),
        "hydration returned a caller-altered report as if it came from SQL"
    );
}

#[test]
fn report_hydration_rejects_foreign_import_identity() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    let mut report = store.pdf.reports[0].clone();
    report.import_id = "12facf7a-4fbd-42a7-8c2f-f96208bf740e".into();
    assert!(
        store
            .pdf
            .report_from_snapshot(&sandbox.0, &report, None)
            .is_err(),
        "hydration adopted an unregistered import identity"
    );
}

// A complete hand-authored native protocol sample. Its raw bytes are synthetic;
// it proves SQL/manifest/UTF-8 boundary checks, not parsing a physical PDF.
fn normal_fixture(cut_utf8: bool) -> (Connection, BTreeMap<String, Vec<u8>>) {
    let text = b"A\xc3\xa9 B\n";
    let text_sha = crate::text::sha256(text);
    let input = serde_json::to_value(fingerprint()).unwrap();
    let parser_resources = json!([
        {"resource_id":"raw-pdf","relative_path":"raw.pdf","kind":"binary","fingerprint":input},
        {"resource_id":"page-text-000000","relative_path":"pages/000000.txt","kind":"utf8_text",
         "fingerprint":{"sha256":text_sha,"byte_length":text.len()}}
    ]);
    let identity = serde_json::to_value(ParserIdentity::pinned()).unwrap();
    let manifest = json!({
        "schema_version":1,"format":"pdf-page-regions-v1","input_fingerprint":input,
        "parser":identity,"raw_resource_id":"raw-pdf","page_count":1,"status":"parsed",
        "pages":[{
            "physical_page_index":0,"display_page_number":1,"printed_page_label":null,"status":"parsed",
            "geometry":{"unit":"pt","origin":"crop_top_left","y_axis":"down","media_box":[0,0,600,800],
                "crop_box":[0,0,600,800],"rotation_degrees":0,"user_unit":1,"width_pt":600,"height_pt":800,
                "pdf_to_display":[1,0,0,-1,0,800]},
            "text":{"resource_id":"page-text-000000","sha256":text_sha,"byte_length":text.len()},
            "glyphs":[
                {"ordinal":0,"text":"A","bbox":[72,62,80,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":1,"text":"é","bbox":[80,62,86,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":2,"text":" ","bbox":[86,62,90,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":3,"text":"B","bbox":[90,62,98,74],"font_name":"Helvetica","font_size":12,"upright":true}
            ],
            "regions":[{"region_id":"region-000000-000000","locator_kind":"pdf_page_region","physical_page_index":0,
                "bbox":[72,62,98,74],"text":{"resource_id":"page-text-000000","resource_sha256":text_sha,
                    "start_byte":if cut_utf8 {2} else {0},"end_byte":5},"glyph_indices":[0,1,2,3]}],
            "visuals":[]
        }],
        "resources":parser_resources,"issues":[]
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_sha = crate::text::sha256(&manifest_bytes);
    let mut response_resources = parser_resources.as_array().unwrap().clone();
    response_resources.push(
        json!({"resource_id":"pdf-manifest","relative_path":"pdf-manifest.json","kind":"utf8_text",
        "fingerprint":{"sha256":manifest_sha,"byte_length":manifest_bytes.len()}}),
    );
    let response = json!({
        "protocol_version":1,"request_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "workspace_capability_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","input_fingerprint":input,
        "producer":"parser","parser":{"name":"pdfplumber","version":"0.11.9","capability_version":1,"capabilities":["source_units_v1"]},
        "unit_count":1,"units":[{"unit_id":"page-000000","ordinal":0,"kind":"text","status":"parsed",
            "text":{"resource_id":"page-text-000000","start_byte":0,"end_byte":6},
            "resource_ids":["raw-pdf","pdf-manifest","page-text-000000"]}],
        "resources":response_resources,"issues":[]
    });
    let resource_bytes = RAW.len() + text.len() + manifest_bytes.len();
    let report = json!({
        "schema_version":1,"report_kind":"pdf_parse_sandbox","input_fingerprint":input,
        "parser":identity,"status":"parsed","page_count":1,"parsed_page_count":1,
        "needs_review_page_count":0,"failed_page_count":0,"manifest_path":"parser/pdf-manifest.json",
        "manifest_sha256":manifest_sha,"elapsed_ms":0,"resource_bytes":resource_bytes,"sidecar":response
    });
    let report_bytes = serde_json::to_vec(&report).unwrap();
    let markdown = format!(
        "# PDF 单文件解析沙盒\n\n| 字段 | 值 |\n| --- | --- |\n| schema_version | 1 |\n| report_kind | pdf_parse_sandbox |\n| status | parsed |\n| input_sha256 | {} |\n| input_byte_length | {} |\n| parser | pdfplumber 0.11.9 / pdfminer.six 20251230 / noetome-pdf-text-v1 / crop-pt-lines-v1 / Python 3.12.14 |\n| page_count | 1 |\n| parsed_page_count | 1 |\n| needs_review_page_count | 0 |\n| failed_page_count | 0 |\n| manifest_path | parser/pdf-manifest.json |\n| manifest_sha256 | {} |\n| elapsed_ms | 0 |\n| resource_bytes | {} |\n\n物理索引从0开始；显示物理页为索引+1；印刷页码未确认。知识抽取、OCR与科学验证未运行。\n\n| 物理索引 | 显示物理页 | 状态 |\n| --- | --- | --- |\n| 0 | 1 | parsed |\n\n| 问题 | 来源单元 | 原因 |\n| --- | --- | --- |\n",
        fingerprint().sha256,RAW.len(),manifest_sha,resource_bytes
    ).into_bytes();
    let files = BTreeMap::from([
        ("parser/raw.pdf".into(), RAW.to_vec()),
        ("parser/pages/000000.txt".into(), text.to_vec()),
        ("parser/pdf-manifest.json".into(), manifest_bytes),
        ("parse-result.json".into(), report_bytes),
        ("parse-result.md".into(), markdown),
    ]);
    let connection = host_database();
    connection
        .execute_batch(
            "DELETE FROM pdf_imports; DELETE FROM pdf_resources; DELETE FROM pdf_parses;",
        )
        .unwrap();
    connection.execute(
        "INSERT INTO pdf_parses VALUES(?1,?2,?3,?4,1,?5,?5,'pdf-import-default-v1',?6,'parsed','validated',1,1,1,0,0,1,?7,?8,?9,NULL,NULL)",
        params![PARSE,VERSION,TIME,"532f83799b2be66b4137acaef649b1b2e296055a1a0d847d284643ce8c4ac3f3",
            serde_json::to_string(&ParserIdentity::pinned()).unwrap(),"a".repeat(64),
            format!(".noetome-pdf-parses/{PARSE}"),crate::text::sha256(&files["parse-result.json"]),manifest_sha]
    ).unwrap();
    connection
        .execute(
            "INSERT INTO pdf_pages VALUES(?1,0,1,NULL,'parsed',?2)",
            params![PARSE, serde_json::to_string(&manifest["pages"][0]).unwrap()],
        )
        .unwrap();
    for (id, path, kind, media, confidence) in [
        (
            "raw-pdf",
            "parser/raw.pdf",
            "binary",
            "application/pdf",
            "confirmed",
        ),
        (
            "pdf-manifest",
            "parser/pdf-manifest.json",
            "utf8",
            "application/json",
            "confirmed",
        ),
        (
            "page-text-000000",
            "parser/pages/000000.txt",
            "utf8",
            "text/plain",
            "confirmed",
        ),
        (
            "parse-result-json",
            "parse-result.json",
            "utf8",
            "application/json",
            "diagnostic",
        ),
        (
            "parse-result-md",
            "parse-result.md",
            "utf8",
            "text/markdown",
            "diagnostic",
        ),
    ] {
        let bytes = &files[path];
        connection
            .execute(
                "INSERT INTO pdf_resources VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    PARSE,
                    id,
                    path,
                    crate::text::sha256(bytes),
                    i64::try_from(bytes.len()).unwrap(),
                    kind,
                    media,
                    confidence
                ],
            )
            .unwrap();
    }
    connection.execute(
        "INSERT INTO pdf_regions VALUES(?1,'region-000000-000000',0,'pdf_page_region',72.0,62.0,98.0,74.0,'page-text-000000',?2,?3,5,'[0,1,2,3]')",
        params![PARSE,text_sha,if cut_utf8 {2} else {0}]
    ).unwrap();
    connection.execute(
        "INSERT INTO pdf_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message) VALUES(1,?1,?2,'D:/selected/synthetic.pdf',?3,?4,?5,'ready',0,0,'acquired',NULL,NULL)",
        params![IMPORT,TIME,SOURCE,VERSION,PARSE]
    ).unwrap();
    (connection, files)
}
fn normal_snapshots(root: &Path, files: &BTreeMap<String, Vec<u8>>) {
    host_snapshots(root);
    let parse = root.join(".noetome-pdf-parses").join(PARSE);
    std::fs::remove_file(parse.join("failure.json")).unwrap();
    for (path, bytes) in files {
        let target = parse.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
}

#[test]
fn native_snapshot_reader_binds_full_page_region_and_core_resource_inventory() {
    let sandbox = Sandbox::new();
    let (connection, files) = normal_fixture(false);
    normal_snapshots(&sandbox.0, &files);
    let mut store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    store.hydrate_pdf_reports(&sandbox.0, None).unwrap();
    assert_eq!(store.pdf.reports[0].pending_extraction_count, 1);
    assert_eq!(store.pdf.reports[0].pdf.page_count, Some(1));
    assert_eq!(store.pdf.resources[PARSE].len(), 5);
    for sql in [
        "UPDATE pdf_pages SET display_page_number=2",
        "UPDATE pdf_regions SET bbox_right=99.0",
        "UPDATE pdf_regions SET glyph_indices_json='[0,1,2]'",
        "UPDATE pdf_resources SET sha256=printf('%064d',0) WHERE resource_id='parse-result-md'",
        "DELETE FROM pdf_pages",
        "DELETE FROM pdf_regions",
    ] {
        let (mutated, _) = normal_fixture(false);
        mutated
            .execute_batch("PRAGMA ignore_check_constraints=ON; PRAGMA foreign_keys=OFF;")
            .unwrap();
        mutated.execute_batch(sql).unwrap();
        let result = crate::source_model::Store::load(&mutated, PROJECT)
            .and_then(|store| store.validate_snapshots(&sandbox.0, PROJECT, None));
        assert!(
            result.is_err(),
            "accepted synthetic persisted mutation {sql}"
        );
    }
}

#[test]
fn native_semantic_inspector_rejects_utf8_cut_even_with_sql_and_all_outer_shas_updated() {
    let sandbox = Sandbox::new();
    let (connection, files) = normal_fixture(true);
    normal_snapshots(&sandbox.0, &files);
    // SQL, JSON indexes, all hashes, byte counts and Core report facts agree.
    // The byte range still cuts é between its two UTF-8 bytes.
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    let error = store
        .validate_snapshots(&sandbox.0, PROJECT, None)
        .unwrap_err();
    assert_eq!(error.code(), "SOURCE_INVALID_SNAPSHOT");
    assert!(error.to_string().contains("PDF_TEXT"), "{error}");
}

fn legacy_failure(connection: &Connection, id: &str, order: i64, source: Option<&str>, time: &str) {
    connection.execute(
        "INSERT INTO source_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,status,reused,span_count,pending_extraction_count,knowledge_status,error_code,error_message) VALUES(?1,?2,?3,'D:/selected/synthetic.pdf',?4,NULL,'failed',0,0,0,'pending','SOURCE_READ_ERROR','controlled selected input unavailable')",
        params![order,id,time,source]
    ).unwrap();
}

#[test]
fn global_import_history_rejects_cross_table_ids_and_rowid_collisions() {
    crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    for (id, order) in [(IMPORT, 2), ("12facf7a-4fbd-42a7-8c2f-f96208bf740e", 1)] {
        let connection = host_database();
        legacy_failure(&connection, id, order, None, TIME);
        let error = crate::source_model::Store::load(&connection, PROJECT).unwrap_err();
        assert_eq!(error.code(), "SOURCE_INVALID_RECORD");
        assert!(error.to_string().contains("全局正序号重复"), "{error}");
    }
}

#[test]
fn unified_source_helpers_follow_global_rowid_even_when_clock_moves_backwards() {
    let connection = host_database();
    const SOURCE2: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e68";
    const VERSION2: &str = "37d03d00-ff4c-4bdf-b126-969a4b073e3e";
    const PARSE2: &str = "87d03d00-ff4c-4bdf-b126-969a4b073e3e";
    const IMPORT2: &str = "12facf7a-4fbd-42a7-8c2f-f96208bf740e";
    const EARLIER: &str = "2026-11-01T00:00:00Z";
    connection
        .execute(
            "INSERT INTO sources VALUES(?1,?2)",
            params![SOURCE2, EARLIER],
        )
        .unwrap();
    connection.execute(
        "INSERT INTO source_versions SELECT ?1,?2,1,?3,media_type,display_name,content_sha256,byte_length,NULL,NULL,'acquired',?4,NULL,NULL,NULL FROM source_versions WHERE version_id=?5",
        params![VERSION2,SOURCE2,EARLIER,format!(".noetome-sources/{VERSION2}/raw.pdf"),VERSION]
    ).unwrap();
    connection.execute(
        "INSERT INTO pdf_parses SELECT ?1,?2,?3,cache_key,protocol_version,requested_parser_json,parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,region_count,?4,outcome_sha256,manifest_sha256,error_code,error_message FROM pdf_parses WHERE parse_id=?5",
        params![PARSE2,VERSION2,EARLIER,format!(".noetome-pdf-parses/{PARSE2}"),PARSE]
    ).unwrap();
    connection.execute(
        "INSERT INTO pdf_resources SELECT ?1,resource_id,relative_path,sha256,byte_length,kind,media_type,confidence FROM pdf_resources WHERE parse_id=?2",
        params![PARSE2,PARSE]
    ).unwrap();
    connection.execute(
        "INSERT INTO pdf_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message) SELECT 2,?1,?2,selected_path,?3,?4,?5,status,0,0,acquisition_status,error_code,error_message FROM pdf_imports WHERE import_id=?6",
        params![IMPORT2,EARLIER,SOURCE2,VERSION2,PARSE2,IMPORT]
    ).unwrap();
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    assert_eq!(
        store
            .source_for_path("D:/selected/synthetic.pdf")
            .as_deref(),
        Some(SOURCE2)
    );
    assert_eq!(
        store.source_for_hash(&fingerprint().sha256).as_deref(),
        Some(SOURCE)
    );
    assert_eq!(store.next_import_order().unwrap(), 3);
    // A synthetic legacy failed attempt can still retain an explicitly selected
    // existing Source. This exercises the union, not a media-support claim.
    legacy_failure(
        &connection,
        "12facf7a-4fbd-42a7-8c2f-f96208bf740d",
        3,
        Some(SOURCE),
        EARLIER,
    );
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    assert_eq!(
        store
            .source_for_path("D:/selected/synthetic.pdf")
            .as_deref(),
        Some(SOURCE)
    );
    assert_eq!(
        store.source_for_hash(&fingerprint().sha256).as_deref(),
        Some(SOURCE)
    );
    assert_eq!(store.next_import_order().unwrap(), 4);
}

#[test]
fn next_global_import_order_reports_i64_overflow_without_wrapping() {
    let connection = host_database();
    connection
        .execute("UPDATE pdf_imports SET rowid=?1", params![i64::MAX])
        .unwrap();
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    assert!(store.next_import_order().is_err());
}

#[test]
fn staged_pdf_reader_only_accepts_registered_ids_in_the_current_writer_paths() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let writer = sandbox.0.join(".noetome-writing");
    std::fs::create_dir(&writer).unwrap();
    let staged_version = writer.join("version");
    let staged_parse = writer.join("parse");
    std::fs::rename(
        sandbox.0.join(".noetome-sources").join(VERSION),
        &staged_version,
    )
    .unwrap();
    std::fs::rename(
        sandbox.0.join(".noetome-pdf-parses").join(PARSE),
        &staged_parse,
    )
    .unwrap();
    let mut store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    store
        .validate_snapshots_staged(
            &sandbox.0,
            PROJECT,
            Some((VERSION, &staged_version)),
            Some((PARSE, &staged_parse)),
        )
        .unwrap();
    store
        .hydrate_pdf_reports(&sandbox.0, Some((PARSE, &staged_parse)))
        .unwrap();
    assert!(
        store
            .validate_snapshots_staged(
                &sandbox.0,
                PROJECT,
                Some((VERSION, &staged_version)),
                Some((SOURCE, &staged_parse))
            )
            .is_err()
    );
    let foreign = sandbox.0.join("foreign-stage");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("failure.json"), failure_bytes()).unwrap();
    assert!(
        store
            .validate_snapshots_staged(
                &sandbox.0,
                PROJECT,
                Some((VERSION, &staged_version)),
                Some((PARSE, &foreign))
            )
            .is_err()
    );
}

#[test]
fn host_failure_keeps_partial_non_utf8_parser_bytes_unconfirmed() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let residue = b"{unfinished manifest \xff";
    let connection = host_database();
    connection.execute(
        "INSERT INTO pdf_resources VALUES(?1,'pdf-manifest','parser/pdf-manifest.json',?2,?3,'utf8','application/json','unconfirmed')",
        params![PARSE,crate::text::sha256(residue),i64::try_from(residue.len()).unwrap()]
    ).unwrap();
    let parser = sandbox
        .0
        .join(".noetome-pdf-parses")
        .join(PARSE)
        .join("parser");
    std::fs::create_dir(&parser).unwrap();
    std::fs::write(parser.join("pdf-manifest.json"), residue).unwrap();
    let mut store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    store.hydrate_pdf_reports(&sandbox.0, None).unwrap();
    let report = &store.pdf.reports[0];
    assert_eq!(report.status, "failed");
    assert_eq!(report.acquisition_status, "acquired");
    assert!(report.pdf.parser.is_none());
    assert!(report.pdf.page_count.is_none());
    assert_eq!(report.pdf.confirmed_page_count, 0);
    assert_eq!(report.pdf.region_count, 0);
    assert_eq!(
        std::fs::read(parser.join("pdf-manifest.json")).unwrap(),
        residue
    );
    connection
        .execute(
            "UPDATE pdf_resources SET confidence='confirmed' WHERE resource_id='pdf-manifest'",
            [],
        )
        .unwrap();
    assert!(crate::source_model::Store::load(&connection, PROJECT).is_err());
}

#[test]
fn host_failure_rejects_fabricated_confirmed_pages_even_with_outer_hashes_updated() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let mut value: serde_json::Value = serde_json::from_slice(&failure_bytes()).unwrap();
    value["page_count"] = json!(1);
    let bytes = serde_json::to_vec(&value).unwrap();
    let sha = crate::text::sha256(&bytes);
    let connection = host_database();
    connection
        .execute("UPDATE pdf_parses SET outcome_sha256=?1", params![sha])
        .unwrap();
    connection
        .execute(
            "UPDATE pdf_resources SET sha256=?1,byte_length=?2",
            params![sha, i64::try_from(bytes.len()).unwrap()],
        )
        .unwrap();
    std::fs::write(
        sandbox
            .0
            .join(".noetome-pdf-parses")
            .join(PARSE)
            .join("failure.json"),
        bytes,
    )
    .unwrap();
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
}

#[test]
fn historical_adapter_sha_recomputes_key_while_unsupported_actual_parser_is_rejected() {
    let connection = host_database();
    let historical_sha = "c".repeat(64);
    let key = cache_key(
        &fingerprint(),
        &ParserIdentity::pinned(),
        &historical_sha,
        DEFAULT_IMPORT_OPTIONS,
    )
    .unwrap();
    connection
        .execute(
            "UPDATE pdf_parses SET adapter_sha256=?1,cache_key=?2",
            params![historical_sha, key],
        )
        .unwrap();
    crate::source_model::Store::load(&connection, PROJECT).unwrap();
    let mut unsupported = ParserIdentity::pinned();
    unsupported.engine_version = "synthetic-contract-alternate".into();
    let key = cache_key(
        &fingerprint(),
        &unsupported,
        &historical_sha,
        DEFAULT_IMPORT_OPTIONS,
    )
    .unwrap();
    connection
        .execute(
            "UPDATE pdf_parses SET requested_parser_json=?1,cache_key=?2",
            params![serde_json::to_string(&unsupported).unwrap(), key],
        )
        .unwrap();
    assert!(crate::source_model::Store::load(&connection, PROJECT).is_err());
}

#[test]
fn pdf_report_requires_nullable_fields_in_its_embedded_original_version() {
    let store = crate::source_model::Store::load(&host_database(), PROJECT).unwrap();
    let value = serde_json::to_value(&store.pdf.reports[0]).unwrap();
    assert!(serde_json::from_value::<PdfImportReport>(value.clone()).is_ok());
    for field in ["source_id", "version_id", "version", "error"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PdfImportReport>(missing).is_err(),
            "missing report {field} accepted"
        );
    }
    for field in [
        "normalized_sha256",
        "normalization_version",
        "normalized_snapshot",
        "error",
    ] {
        let mut missing = value.clone();
        missing["version"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PdfImportReport>(missing).is_err(),
            "missing nullable embedded version {field} accepted"
        );
    }
}

fn review_fixture() -> (Connection, BTreeMap<String, Vec<u8>>) {
    let (connection, mut files) = normal_fixture(false);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&files["parser/pdf-manifest.json"]).unwrap();
    let mut report: serde_json::Value =
        serde_json::from_slice(&files["parse-result.json"]).unwrap();
    let old_manifest_sha = report["manifest_sha256"].as_str().unwrap().to_owned();
    let old_resource_bytes = report["resource_bytes"].as_u64().unwrap();
    let issue = json!({
        "code":"PDF_SYMBOL_FONT","severity":"blocking",
        "message":"Synthetic symbolic font requires review",
        "unit_ids":["page-000000"],"resource_ids":["raw-pdf","page-text-000000"]
    });
    manifest["status"] = json!("needs_review");
    manifest["pages"][0]["status"] = json!("needs_review");
    manifest["pages"][0]["glyphs"][0]["font_name"] = json!("Symbol");
    manifest["issues"] = json!([issue]);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let manifest_sha = crate::text::sha256(&manifest_bytes);
    let resource_bytes = RAW.len() + files["parser/pages/000000.txt"].len() + manifest_bytes.len();
    report["status"] = json!("needs_review");
    report["parsed_page_count"] = json!(0);
    report["needs_review_page_count"] = json!(1);
    report["manifest_sha256"] = json!(manifest_sha);
    report["resource_bytes"] = json!(resource_bytes);
    report["sidecar"]["units"][0]["status"] = json!("needs_review");
    report["sidecar"]["issues"] = json!([issue]);
    report["sidecar"]["resources"][2]["fingerprint"] =
        json!({"sha256":manifest_sha,"byte_length":manifest_bytes.len()});
    let markdown = String::from_utf8(files["parse-result.md"].clone())
        .unwrap()
        .replace("| status | parsed |", "| status | needs_review |")
        .replace("| parsed_page_count | 1 |", "| parsed_page_count | 0 |")
        .replace(
            "| needs_review_page_count | 0 |",
            "| needs_review_page_count | 1 |",
        )
        .replace("| 0 | 1 | parsed |", "| 0 | 1 | needs_review |")
        .replace(&old_manifest_sha, &manifest_sha)
        .replace(
            &format!("| resource_bytes | {old_resource_bytes} |"),
            &format!("| resource_bytes | {resource_bytes} |"),
        )
        + "| PDF_SYMBOL_FONT | page-000000 | Synthetic symbolic font requires review |\n";
    files.insert("parser/pdf-manifest.json".into(), manifest_bytes);
    files.insert(
        "parse-result.json".into(),
        serde_json::to_vec(&report).unwrap(),
    );
    files.insert("parse-result.md".into(), markdown.into_bytes());
    connection.execute(
        "UPDATE pdf_parses SET status='needs_review',parsed_page_count=0,needs_review_page_count=1,outcome_sha256=?1,manifest_sha256=?2",
        params![crate::text::sha256(&files["parse-result.json"]),manifest_sha]
    ).unwrap();
    connection
        .execute(
            "UPDATE pdf_pages SET status='needs_review',record_json=?1",
            params![serde_json::to_string(&manifest["pages"][0]).unwrap()],
        )
        .unwrap();
    connection
        .execute("UPDATE pdf_imports SET status='needs_review'", [])
        .unwrap();
    for (path, bytes) in &files {
        connection
            .execute(
                "UPDATE pdf_resources SET sha256=?1,byte_length=?2 WHERE relative_path=?3",
                params![
                    crate::text::sha256(bytes),
                    i64::try_from(bytes.len()).unwrap(),
                    path
                ],
            )
            .unwrap();
    }
    (connection, files)
}

#[test]
fn needs_review_reports_hydrate_actual_bound_issues_and_keep_knowledge_pending() {
    let sandbox = Sandbox::new();
    let (connection, files) = review_fixture();
    normal_snapshots(&sandbox.0, &files);
    let mut store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    store.hydrate_pdf_reports(&sandbox.0, None).unwrap();
    let report = &store.pdf.reports[0];
    assert_eq!(report.status, "needs_review");
    assert_eq!(report.pdf.status, "needs_review");
    assert_eq!(report.pdf.parsed_page_count, 0);
    assert_eq!(report.pdf.needs_review_page_count, 1);
    assert_eq!(report.knowledge_status, "pending");
    assert_eq!(report.pending_extraction_count, 1);
    assert!(report.error.is_none());
    assert!(report.pdf.error.is_none());
    assert_eq!(
        serde_json::to_value(&report.pdf.issues).unwrap(),
        json!([{
            "code":"PDF_SYMBOL_FONT","severity":"blocking",
            "message":"Synthetic symbolic font requires review",
            "unit_ids":["page-000000"],"resource_ids":["raw-pdf","page-text-000000"]
        }])
    );
}

#[test]
fn bounded_host_diagnostics_survive_json_control_character_expansion() {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let mut value: serde_json::Value = serde_json::from_slice(&failure_bytes()).unwrap();
    // This is exactly the 64 KiB diagnostic policy, but JSON escapes each NUL
    // into six source bytes. The failure remains a bounded, valid diagnostic.
    value["error"]["diagnostics"] = json!("\0".repeat(64 * 1024));
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(bytes.len() > 256 * 1024 && bytes.len() < 512 * 1024);
    let sha = crate::text::sha256(&bytes);
    let connection = host_database();
    connection
        .execute("UPDATE pdf_parses SET outcome_sha256=?1", params![sha])
        .unwrap();
    connection
        .execute(
            "UPDATE pdf_resources SET sha256=?1,byte_length=?2",
            params![sha, i64::try_from(bytes.len()).unwrap()],
        )
        .unwrap();
    std::fs::write(
        sandbox
            .0
            .join(".noetome-pdf-parses")
            .join(PARSE)
            .join("failure.json"),
        bytes,
    )
    .unwrap();
    let loaded = crate::source_model::Store::load(&connection, PROJECT);
    assert!(
        loaded.is_ok(),
        "bounded escaped diagnostics rejected: {loaded:?}"
    );
    loaded
        .unwrap()
        .validate_snapshots(&sandbox.0, PROJECT, None)
        .unwrap();
}

#[cfg(windows)]
#[test]
fn a_new_exclusively_occupied_cache_root_is_rejected_during_absence_recheck() {
    use std::os::windows::fs::OpenOptionsExt;
    let sandbox = Sandbox::new();
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let guarded =
        CacheGuards::capture(&sandbox.0, PROJECT, &BTreeMap::new(), None, &control).unwrap();
    let cache = sandbox.0.join(".noetome-pdf-parses");
    std::fs::create_dir(&cache).unwrap();
    let _exclusive = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .custom_flags(0x02000000)
        .open(&cache)
        .unwrap();
    assert!(
        guarded.verify(&control).is_err(),
        "an occupied new cache root was treated as still absent"
    );
}
