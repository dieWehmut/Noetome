use rusqlite::{params, types::Value};
use std::collections::BTreeMap;

use super::upgrade_pdf as pdf_upgrade;

const PDF_SCHEMA_FIXTURE: &str = include_str!("source_schema3.sql");
const SCHEMA_PROJECT: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e66";
const SOURCE_B: &str = "2232018c-42e9-4db6-8e2e-58d8e40d9e66";
const SOURCE_PDF: &str = "1232018c-42e9-4db6-8e2e-58d8e40d9e66";
const VERSION_PDF: &str = "3232018c-42e9-4db6-8e2e-58d8e40d9e66";
const PARSE_PDF: &str = "7232018c-42e9-4db6-8e2e-58d8e40d9e66";
const SQL_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn schema_info(version: u32) -> ProjectInfo {
    ProjectInfo {
        project_id: SCHEMA_PROJECT.into(),
        schema_version: version,
        created_at: "2026-11-01T00:00:00Z".into(),
        state: "initialized".into(),
        database: "project.sqlite".into(),
    }
}

fn schema_database(schema: &str, version: u32) -> Connection {
    let database = Connection::open_in_memory().unwrap();
    database.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    database.execute_batch(schema).unwrap();
    database
        .execute(
            "INSERT INTO project_metadata(singleton,project_id,schema_version,created_at,state) VALUES(1,?1,?2,?3,'initialized')",
            params![SCHEMA_PROJECT, version, "2026-11-01T00:00:00Z"],
        )
        .unwrap();
    database
}

fn old_schema_history() -> Connection {
    let database = schema_database(SCHEMA, 2);
    database.execute_batch("INSERT INTO sources(rowid,source_id,created_at) VALUES
        (9,'1232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:01Z'),
        (30,'2232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:06Z');
        INSERT INTO source_versions(rowid,version_id,source_id,revision,created_at,media_type,display_name,content_sha256,byte_length,normalized_sha256,normalization_version,ingest_status,raw_snapshot,normalized_snapshot,error_code,error_message) VALUES
        (4,'3232018c-42e9-4db6-8e2e-58d8e40d9e66','1232018c-42e9-4db6-8e2e-58d8e40d9e66',1,'2026-11-01T00:00:01Z','text/markdown','旧资料.md','ad543912f43b61745e865ed80ef1c9f01be64b06b3d8951f645e14067073c866',26,'de7edffedd0292ace58f69085e0a28577f05a5dfb2350c493f99c312ce004de3','utf8-newlines-v1','ready','.noetome-sources/3232018c-42e9-4db6-8e2e-58d8e40d9e66/raw.bin','.noetome-sources/3232018c-42e9-4db6-8e2e-58d8e40d9e66/normalized.txt',NULL,NULL),
        (9,'4232018c-42e9-4db6-8e2e-58d8e40d9e66','1232018c-42e9-4db6-8e2e-58d8e40d9e66',2,'2026-11-01T00:00:03Z','text/markdown','损坏资料.md','53c523bad1a04b0d2fce18ee10dacb6a7362f3804cdcbdaf68ff1ae76e8a51cd',4,NULL,NULL,'failed','.noetome-sources/4232018c-42e9-4db6-8e2e-58d8e40d9e66/raw.bin',NULL,'SOURCE_INVALID_UTF8','来源不是有效UTF-8'),
        (18,'6232018c-42e9-4db6-8e2e-58d8e40d9e66','2232018c-42e9-4db6-8e2e-58d8e40d9e66',1,'2026-11-01T00:00:06Z','text/plain','旧文本.txt','adc271fa6c76ee499e0ca501689a858b662f1f3078fcff0179ed9dc368cb92e9',11,'726ffeb2c4a06b034e4b0b3828cae09b9062e1764bce3fcf91f66226a6fb6946','utf8-newlines-v1','ready','.noetome-sources/6232018c-42e9-4db6-8e2e-58d8e40d9e66/raw.bin','.noetome-sources/6232018c-42e9-4db6-8e2e-58d8e40d9e66/normalized.txt',NULL,NULL);
        INSERT INTO source_spans(rowid,span_id,source_version_id,ordinal,kind,start_byte,end_byte,start_line,end_line,text_sha256,parser_version,parse_status) VALUES
        (8,'7232018c-42e9-4db6-8e2e-58d8e40d9e66','3232018c-42e9-4db6-8e2e-58d8e40d9e66',0,'heading',0,8,1,1,'6c4961a153569e01ec803eb01fecec4de3bd97352123809ffe2577df11977fbc','blocks-v1','parsed'),
        (14,'8232018c-42e9-4db6-8e2e-58d8e40d9e66','3232018c-42e9-4db6-8e2e-58d8e40d9e66',1,'text',10,22,3,3,'cad63d51d99bdd787e738e0576e53c99db0585874f2dbc9d0d1e5f6cd2c0a3ab','blocks-v1','parsed'),
        (33,'9232018c-42e9-4db6-8e2e-58d8e40d9e66','6232018c-42e9-4db6-8e2e-58d8e40d9e66',0,'text',0,9,1,1,'9cee19cf4fd0be347fae045da8451b1ce5225120200bfca1bbf0248ebba0198b','blocks-v1','parsed');
        INSERT INTO source_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,status,reused,span_count,pending_extraction_count,knowledge_status,error_code,error_message) VALUES
        (7,'a232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:01Z','D:\\test-selected\\旧资料.md','1232018c-42e9-4db6-8e2e-58d8e40d9e66','3232018c-42e9-4db6-8e2e-58d8e40d9e66','ready',0,2,2,'pending',NULL,NULL),
        (11,'b232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:02Z','D:\\test-selected\\重命名.md','1232018c-42e9-4db6-8e2e-58d8e40d9e66','3232018c-42e9-4db6-8e2e-58d8e40d9e66','ready',1,2,2,'pending',NULL,NULL),
        (19,'c232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:03Z','D:\\test-selected\\损坏资料.md','1232018c-42e9-4db6-8e2e-58d8e40d9e66','4232018c-42e9-4db6-8e2e-58d8e40d9e66','failed',0,0,0,'pending','SOURCE_INVALID_UTF8','来源不是有效UTF-8'),
        (26,'d232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:04Z','D:\\test-selected\\损坏资料.md','1232018c-42e9-4db6-8e2e-58d8e40d9e66','4232018c-42e9-4db6-8e2e-58d8e40d9e66','failed',1,0,0,'pending','SOURCE_INVALID_UTF8','来源不是有效UTF-8'),
        (41,'e232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:05Z','D:\\test-selected\\丢失.md',NULL,NULL,'failed',0,0,0,'pending','SOURCE_NOT_FOUND','明确选取的资料不存在'),
        (56,'f232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:06Z','D:\\test-selected\\旧文本.txt','2232018c-42e9-4db6-8e2e-58d8e40d9e66','6232018c-42e9-4db6-8e2e-58d8e40d9e66','ready',0,1,1,'pending',NULL,NULL);").unwrap();
    database
}

fn old_rows(database: &Connection) -> BTreeMap<String, Vec<Vec<Value>>> {
    [
        "project_metadata",
        "sources",
        "source_versions",
        "source_spans",
        "source_imports",
    ]
    .into_iter()
    .map(|table| {
        let mut statement = database
            .prepare(&format!("SELECT rowid,* FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|index| row.get::<_, Value>(index))
                    .collect()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (table.to_owned(), rows)
    })
    .collect()
}

#[test]
fn exact_schema_three_accepts_its_empty_registration_state() {
    let database = schema_database(PDF_SCHEMA_FIXTURE, 3);
    validate(&database).expect("the frozen exact Schema3 must be recognized");
}

#[test]
fn schema_three_migration_preserves_every_legacy_row_value_identity_and_order() {
    let mut database = old_schema_history();
    validate(&database).unwrap();
    let mut expected = old_rows(&database);
    expected.get_mut("project_metadata").unwrap()[0][3] = Value::Integer(3);
    let updated = pdf_upgrade(&mut database, &schema_info(2)).unwrap();
    assert_eq!(updated, schema_info(3));
    assert_eq!(old_rows(&database), expected);
    validate(&database).unwrap();
    let version: u32 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let application: u32 = database
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
    assert_eq!(application, 0x4e4f4554);
    for table in [
        "pdf_parses",
        "pdf_pages",
        "pdf_regions",
        "pdf_resources",
        "pdf_imports",
    ] {
        assert_eq!(
            database
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn pdf_upgrade_of_empty_schema_one_reaches_three_with_the_original_identity() {
    let mut database = schema_database(
        "CREATE TABLE project_metadata (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        project_id TEXT NOT NULL UNIQUE,
        schema_version INTEGER NOT NULL CHECK (schema_version = 1),
        created_at TEXT NOT NULL,
        state TEXT NOT NULL CHECK (state = 'initialized')
        ); PRAGMA user_version=1; PRAGMA application_id=1313817940;",
        1,
    );
    assert_eq!(
        pdf_upgrade(&mut database, &schema_info(1)).unwrap(),
        schema_info(3)
    );
    assert_eq!(
        database
            .query_row(
                "SELECT project_id,created_at,schema_version FROM project_metadata",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?
                ))
            )
            .unwrap(),
        (SCHEMA_PROJECT.into(), "2026-11-01T00:00:00Z".into(), 3)
    );
    validate(&database).unwrap();
}

#[test]
fn legacy_upgrade_keeps_schema_three_without_a_downgrade() {
    let mut database = schema_database(PDF_SCHEMA_FIXTURE, 3);
    let before = old_rows(&database);
    let before_shape = shape(&database).unwrap();
    assert_eq!(
        upgrade(&mut database, &schema_info(3)).unwrap(),
        schema_info(3)
    );
    assert_eq!(old_rows(&database), before);
    assert_eq!(shape(&database).unwrap(), before_shape);
}

#[test]
fn candidate_upgrade_rejects_a_mismatched_identity_without_rewriting_old_rows() {
    let mut database = old_schema_history();
    let before = old_rows(&database);
    let mut mismatched = schema_info(2);
    mismatched.project_id = SOURCE_B.into();
    assert_eq!(
        pdf_upgrade(&mut database, &mismatched).unwrap_err().code(),
        "PROJECT_INVALID_DATABASE"
    );
    assert_eq!(old_rows(&database), before);
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        2
    );
}

fn sql_pdf_records() -> Connection {
    // These rows exercise SQL guards only. No files or parser evidence are
    // invented; manifest and snapshot semantics are tested by the PDF model.
    let database = schema_database(PDF_SCHEMA_FIXTURE, 3);
    database
        .execute(
            "INSERT INTO sources VALUES(?1,'2026-11-01T00:00:01Z')",
            params![SOURCE_PDF],
        )
        .unwrap();
    database.execute("INSERT INTO source_versions(version_id,source_id,revision,created_at,media_type,display_name,content_sha256,byte_length,normalized_sha256,normalization_version,ingest_status,raw_snapshot,normalized_snapshot,error_code,error_message) VALUES(?1,?2,1,'2026-11-01T00:00:01Z','application/pdf','selected.pdf',?3,4,NULL,NULL,'acquired',?4,NULL,NULL,NULL)", params![VERSION_PDF,SOURCE_PDF,SQL_SHA,format!(".noetome-sources/{VERSION_PDF}/raw.pdf")]).unwrap();
    database.execute("INSERT INTO pdf_parses(parse_id,source_version_id,created_at,cache_key,protocol_version,requested_parser_json,parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,region_count,snapshot_path,outcome_sha256,manifest_sha256,error_code,error_message) VALUES(?1,?2,'2026-11-01T00:00:01Z',?3,1,?4,?4,'pdf-import-default-v1',?3,'parsed','validated',1,1,1,0,0,1,?5,?3,?3,NULL,NULL)", params![PARSE_PDF,VERSION_PDF,SQL_SHA,"{\"engine\":\"pdfplumber\",\"engine_version\":\"0.11.9\",\"engine_dependency\":\"pdfminer.six\",\"engine_dependency_version\":\"20251230\",\"adapter_version\":\"noetome-pdf-text-v1\",\"options_version\":\"crop-pt-lines-v1\",\"python_version\":\"3.12.14\"}",format!(".noetome-pdf-parses/{PARSE_PDF}")]).unwrap();
    database
        .execute(
            "INSERT INTO pdf_pages VALUES(?1,0,1,NULL,'parsed','{}')",
            params![PARSE_PDF],
        )
        .unwrap();
    database.execute("INSERT INTO pdf_resources VALUES(?1,'page-text-000000','parser/pages/page-000000.txt',?2,4,'utf8','text/plain','confirmed')", params![PARSE_PDF,SQL_SHA]).unwrap();
    database.execute("INSERT INTO pdf_regions VALUES(?1,'page-000000-region-000000',0,'pdf_page_region',10,20,30,40,'page-text-000000',?2,0,4,'[0]')", params![PARSE_PDF,SQL_SHA]).unwrap();
    database.execute("INSERT INTO pdf_imports VALUES('8232018c-42e9-4db6-8e2e-58d8e40d9e66','2026-11-01T00:00:01Z','D:\\selected\\test.pdf',?1,?2,?3,'ready',0,0,'acquired',NULL,NULL)", params![SOURCE_PDF,VERSION_PDF,PARSE_PDF]).unwrap();
    database
}

fn table_rows(database: &Connection, table: &str) -> Vec<Vec<Value>> {
    let mut statement = database
        .prepare(&format!("SELECT rowid,* FROM {table} ORDER BY rowid"))
        .unwrap();
    let columns = statement.column_count();
    statement
        .query_map([], |row| {
            (0..columns)
                .map(|index| row.get::<_, Value>(index))
                .collect()
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn assert_sql_change_rejected(database: &Connection, table: &str, change: &str) {
    let before = table_rows(database, table);
    let error = database.execute(change, []).unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation),
        "change={change}, error={error}"
    );
    assert_eq!(table_rows(database, table), before, "change={change}");
}

#[test]
fn schema_three_pdf_versions_enforce_acquisition_and_do_not_reinterpret_text_states() {
    let database = sql_pdf_records();
    for change in [
        "UPDATE source_versions SET normalized_sha256='0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'",
        "UPDATE source_versions SET normalization_version='utf8-newlines-v1'",
        "UPDATE source_versions SET normalized_snapshot='.noetome-sources/normalized.txt'",
        "UPDATE source_versions SET error_code='PDF_READ',error_message='failed'",
        "UPDATE source_versions SET raw_snapshot='.noetome-sources/3232018c-42e9-4db6-8e2e-58d8e40d9e66/raw.bin'",
        "UPDATE source_versions SET ingest_status='ready',normalized_sha256='0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',normalization_version='utf8-newlines-v1',normalized_snapshot='normalized.txt'",
        "UPDATE source_versions SET ingest_status='failed',error_code='SOURCE_INVALID_UTF8',error_message='invalid UTF8'",
        "UPDATE source_versions SET media_type='text/plain'",
        "UPDATE source_versions SET content_sha256='ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789ab'",
        "UPDATE source_versions SET byte_length=-1",
    ] {
        assert_sql_change_rejected(&database, "source_versions", change);
    }
    validate(&database).unwrap();
}

#[test]
fn schema_three_preserves_legacy_span_kinds_and_nonempty_ranges() {
    let mut database = old_schema_history();
    pdf_upgrade(&mut database, &schema_info(2)).unwrap();
    for change in [
        "UPDATE source_spans SET kind='pdf_page_region'",
        "UPDATE source_spans SET start_byte=end_byte",
        "UPDATE source_spans SET parse_status='pending'",
        "UPDATE source_imports SET pending_extraction_count=span_count+1",
        "UPDATE source_imports SET knowledge_status='complete'",
    ] {
        let table = if change.starts_with("UPDATE source_spans") {
            "source_spans"
        } else {
            "source_imports"
        };
        assert_sql_change_rejected(&database, table, change);
    }
}

#[test]
fn schema_three_parse_rows_require_verified_or_real_host_failure_outcomes() {
    let database = sql_pdf_records();
    for change in [
        "UPDATE pdf_parses SET protocol_version=2",
        "UPDATE pdf_parses SET cache_key='not-a-sha'",
        "UPDATE pdf_parses SET adapter_sha256='not-a-sha'",
        "UPDATE pdf_parses SET parser_json=NULL",
        "UPDATE pdf_parses SET requested_parser_json='[]'",
        "UPDATE pdf_parses SET parser_json='[]'",
        "UPDATE pdf_parses SET manifest_sha256=NULL",
        "UPDATE pdf_parses SET outcome_sha256='not-a-sha'",
        "UPDATE pdf_parses SET status='ready'",
        "UPDATE pdf_parses SET confirmed_page_count=-1",
        "UPDATE pdf_parses SET page_count=2",
        "UPDATE pdf_parses SET outcome_kind='host_failed'",
        "UPDATE pdf_parses SET status='failed'",
    ] {
        assert_sql_change_rejected(&database, "pdf_parses", change);
    }
    database.execute("UPDATE pdf_parses SET status='failed',outcome_kind='host_failed',page_count=NULL,confirmed_page_count=0,parsed_page_count=0,needs_review_page_count=0,failed_page_count=0,region_count=0,parser_json=NULL,manifest_sha256=NULL,error_code='SIDECAR_RUNTIME',error_message='trusted runtime unavailable'", []).unwrap();
    for change in [
        "UPDATE pdf_parses SET page_count=0",
        "UPDATE pdf_parses SET region_count=1",
        "UPDATE pdf_parses SET confirmed_page_count=1,failed_page_count=1",
        "UPDATE pdf_parses SET parser_json='{}'",
        "UPDATE pdf_parses SET error_message=NULL",
    ] {
        assert_sql_change_rejected(&database, "pdf_parses", change);
    }
}

#[test]
fn schema_three_page_and_resource_columns_reject_wrong_types_and_states() {
    let database = sql_pdf_records();
    for change in [
        "UPDATE pdf_pages SET physical_page_index=-1",
        "UPDATE pdf_pages SET display_page_number=0",
        "UPDATE pdf_pages SET status='pending'",
        "UPDATE pdf_pages SET record_json='[]'",
        "UPDATE pdf_pages SET record_json='not-json'",
        "UPDATE pdf_resources SET byte_length=-1",
        "UPDATE pdf_resources SET kind='text'",
        "UPDATE pdf_resources SET confidence='complete'",
        "UPDATE pdf_resources SET sha256='not-a-sha'",
    ] {
        let table = if change.starts_with("UPDATE pdf_pages") {
            "pdf_pages"
        } else {
            "pdf_resources"
        };
        assert_sql_change_rejected(&database, table, change);
    }
}

#[test]
fn schema_three_region_columns_enforce_finite_bbox_ranges_and_same_parse_foreign_keys() {
    let database = sql_pdf_records();
    for change in [
        "UPDATE pdf_regions SET bbox_left=-1",
        "UPDATE pdf_regions SET bbox_right=bbox_left",
        "UPDATE pdf_regions SET bbox_bottom=bbox_top",
        "UPDATE pdf_regions SET bbox_right=1e999",
        "UPDATE pdf_regions SET bbox_bottom=1e999",
        "UPDATE pdf_regions SET bbox_left='text'",
        "UPDATE pdf_regions SET start_byte=-1",
        "UPDATE pdf_regions SET end_byte=start_byte",
        "UPDATE pdf_regions SET locator_kind='native_text'",
        "UPDATE pdf_regions SET physical_page_index=1",
        "UPDATE pdf_regions SET text_resource_id='missing-resource'",
        "UPDATE pdf_regions SET text_resource_sha256='not-a-sha'",
        "UPDATE pdf_regions SET glyph_indices_json='{}'",
    ] {
        assert_sql_change_rejected(&database, "pdf_regions", change);
    }
    database.execute("INSERT INTO pdf_parses SELECT '9232018c-42e9-4db6-8e2e-58d8e40d9e66',source_version_id,created_at,cache_key,protocol_version,requested_parser_json,parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,region_count,'.noetome-pdf-parses/9232018c-42e9-4db6-8e2e-58d8e40d9e66',outcome_sha256,manifest_sha256,error_code,error_message FROM pdf_parses WHERE parse_id=?1", params![PARSE_PDF]).unwrap();
    database.execute("INSERT INTO pdf_resources VALUES('9232018c-42e9-4db6-8e2e-58d8e40d9e66','other-parse-text','parser/pages/other.txt',?1,4,'utf8','text/plain','confirmed')", params![SQL_SHA]).unwrap();
    assert_sql_change_rejected(
        &database,
        "pdf_regions",
        "UPDATE pdf_regions SET text_resource_id='other-parse-text'",
    );
}

#[test]
fn schema_three_pdf_imports_bind_source_version_and_parse_without_fabricating_failed_versions() {
    let database = sql_pdf_records();
    for change in [
        "UPDATE pdf_imports SET source_id=NULL",
        "UPDATE pdf_imports SET version_id=NULL",
        "UPDATE pdf_imports SET parse_id=NULL",
        "UPDATE pdf_imports SET parse_id='missing-parse'",
        "UPDATE pdf_imports SET acquisition_status='failed'",
        "UPDATE pdf_imports SET status='failed'",
        "UPDATE pdf_imports SET reused=2",
        "UPDATE pdf_imports SET parse_reused=2",
    ] {
        assert_sql_change_rejected(&database, "pdf_imports", change);
    }
    database
        .execute(
            "INSERT INTO sources VALUES(?1,'2026-11-01T00:00:02Z')",
            params![SOURCE_B],
        )
        .unwrap();
    assert_sql_change_rejected(
        &database,
        "pdf_imports",
        "UPDATE pdf_imports SET source_id='2232018c-42e9-4db6-8e2e-58d8e40d9e66'",
    );
    database.execute("UPDATE pdf_imports SET status='failed',source_id=NULL,version_id=NULL,parse_id=NULL,acquisition_status='failed',error_code='SOURCE_NOT_FOUND',error_message='selected file missing'", []).unwrap();
    for change in [
        "UPDATE pdf_imports SET reused=1",
        "UPDATE pdf_imports SET parse_reused=1",
        "UPDATE pdf_imports SET error_code=NULL",
        "UPDATE pdf_imports SET error_message=NULL",
        "UPDATE pdf_imports SET status='ready'",
    ] {
        assert_sql_change_rejected(&database, "pdf_imports", change);
    }
}

#[test]
fn exact_schema_routing_rejects_unknown_tables_indexes_views_triggers_and_constraints() {
    for (schema, version) in [(SCHEMA, 2), (PDF_SCHEMA_FIXTURE, 3)] {
        for change in [
            "CREATE TABLE extra_table(value TEXT)",
            "CREATE INDEX extra_index ON sources(created_at)",
            "CREATE VIEW extra_view AS SELECT * FROM sources",
            "CREATE TRIGGER extra_trigger AFTER INSERT ON sources BEGIN SELECT 1; END",
            "ALTER TABLE source_versions ADD COLUMN unexpected TEXT",
        ] {
            let database = schema_database(schema, version);
            database.execute_batch(change).unwrap();
            assert_eq!(
                validate(&database).unwrap_err().code(),
                "PROJECT_INVALID_DATABASE",
                "change={change}, schema={version}"
            );
        }
        let changed = schema.replace("CHECK (reused IN (0, 1))", "CHECK (reused >= 0)");
        let database = schema_database(&changed, version);
        assert_eq!(
            validate(&database).unwrap_err().code(),
            "PROJECT_INVALID_DATABASE"
        );
    }
}

#[test]
fn exact_schema_routing_rejects_broken_foreign_keys_even_if_writes_disabled_them() {
    for version in [2, 3] {
        let mut database = old_schema_history();
        if version == 3 {
            pdf_upgrade(&mut database, &schema_info(2)).unwrap();
        }
        database.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM sources WHERE source_id='1232018c-42e9-4db6-8e2e-58d8e40d9e66';").unwrap();
        assert_eq!(
            validate(&database).unwrap_err().code(),
            "PROJECT_INVALID_DATABASE"
        );
    }
    let database = sql_pdf_records();
    database
        .execute_batch("PRAGMA foreign_keys=OFF; UPDATE pdf_regions SET physical_page_index=99;")
        .unwrap();
    assert_eq!(
        validate(&database).unwrap_err().code(),
        "PROJECT_INVALID_DATABASE"
    );
}

#[test]
fn schema_upgrades_reject_unknown_versions_and_wrong_version_pairs_without_mutation() {
    for (marked, supplied, code) in [
        (4, 2, "PROJECT_UNSUPPORTED_SCHEMA"),
        (2, 4, "PROJECT_UNSUPPORTED_SCHEMA"),
        (3, 2, "PROJECT_SCHEMA_MISMATCH"),
        (2, 1, "PROJECT_SCHEMA_MISMATCH"),
    ] {
        let mut database = old_schema_history();
        database
            .pragma_update(None, "user_version", marked)
            .unwrap();
        let before = old_rows(&database);
        let before_shape = shape(&database).unwrap();
        assert_eq!(
            pdf_upgrade(&mut database, &schema_info(supplied))
                .unwrap_err()
                .code(),
            code
        );
        assert_eq!(old_rows(&database), before);
        assert_eq!(shape(&database).unwrap(), before_shape);
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            marked
        );
    }
}

#[test]
fn schema_three_noop_upgrades_preserve_all_pdf_rows_and_identities() {
    let mut database = sql_pdf_records();
    let tables = [
        "project_metadata",
        "sources",
        "source_versions",
        "source_spans",
        "source_imports",
        "pdf_parses",
        "pdf_pages",
        "pdf_regions",
        "pdf_resources",
        "pdf_imports",
    ];
    let before: Vec<_> = tables
        .into_iter()
        .map(|table| table_rows(&database, table))
        .collect();
    assert_eq!(
        pdf_upgrade(&mut database, &schema_info(3)).unwrap(),
        schema_info(3)
    );
    assert_eq!(
        upgrade(&mut database, &schema_info(3)).unwrap(),
        schema_info(3)
    );
    let after: Vec<_> = tables
        .into_iter()
        .map(|table| table_rows(&database, table))
        .collect();
    assert_eq!(before, after);
}

struct SchemaSandbox {
    root: std::path::PathBuf,
    parent: std::path::PathBuf,
}

impl SchemaSandbox {
    fn new() -> Self {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/source-schema3-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}

impl Drop for SchemaSandbox {
    fn drop(&mut self) {
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved.starts_with(&self.parent) && resolved != self.parent);
        std::fs::remove_dir_all(resolved).unwrap();
    }
}

#[test]
fn a_real_sqlite_reader_lock_rolls_back_schema_three_rows_and_database_bytes() {
    let sandbox = SchemaSandbox::new();
    let path = sandbox.root.join("candidate.sqlite");
    let seed = old_schema_history();
    seed.execute("VACUUM INTO ?1", params![path.to_str().unwrap()])
        .unwrap();
    let before_rows = old_rows(&seed);
    let before_bytes = std::fs::read(&path).unwrap();
    let mut candidate = Connection::open(&path).unwrap();
    candidate.busy_timeout(std::time::Duration::ZERO).unwrap();
    let reader = Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM source_versions;")
        .unwrap();
    let error = pdf_upgrade(&mut candidate, &schema_info(2)).unwrap_err();
    assert_eq!(error.code(), "PROJECT_DATABASE_BUSY");
    assert_eq!(old_rows(&candidate), before_rows);
    assert_eq!(
        candidate
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        2
    );
    reader.execute_batch("ROLLBACK").unwrap();
    reader.close().unwrap();
    candidate.close().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
    assert_eq!(std::fs::read_dir(&sandbox.root).unwrap().count(), 1);
}

#[test]
fn schema_one_upgrades_recheck_metadata_after_a_real_write_lock_handoff() {
    use std::sync::{Condvar, Mutex, OnceLock};

    static HANDOFF: OnceLock<(Mutex<(bool, bool)>, Condvar)> = OnceLock::new();
    fn wait_for_writer(attempt: i32) -> bool {
        if attempt >= 10 {
            return false;
        }
        let (state, signal) = HANDOFF.get().unwrap();
        let mut state = state.lock().unwrap();
        state.0 = true;
        signal.notify_all();
        drop(signal.wait_while(state, |state| !state.1).unwrap());
        true
    }

    let (state, signal) = HANDOFF.get_or_init(|| (Mutex::new((false, false)), Condvar::new()));
    type Upgrade = fn(&mut Connection, &ProjectInfo) -> Result<ProjectInfo, ProjectError>;
    for (name, upgrader) in [
        ("pdf", pdf_upgrade as Upgrade),
        ("legacy", upgrade as Upgrade),
    ] {
        *state.lock().unwrap() = (false, false);
        let sandbox = SchemaSandbox::new();
        let path = sandbox.root.join(format!("{name}-candidate.sqlite"));
        let seed = schema_database(crate::project::SCHEMA, 1);
        seed.execute("VACUUM INTO ?1", params![path.to_str().unwrap()])
            .unwrap();
        let writer = Connection::open(&path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer
            .execute(
                "UPDATE project_metadata SET project_id=?1",
                params![SOURCE_B],
            )
            .unwrap();
        let mut candidate = Connection::open(&path).unwrap();
        candidate.busy_handler(Some(wait_for_writer)).unwrap();
        let worker = std::thread::spawn(move || upgrader(&mut candidate, &schema_info(1)));
        let (mut waiting, timeout) = signal
            .wait_timeout_while(
                state.lock().unwrap(),
                std::time::Duration::from_secs(5),
                |state| !state.0,
            )
            .unwrap();
        if timeout.timed_out() {
            waiting.1 = true;
            signal.notify_all();
            drop(waiting);
            writer.execute_batch("ROLLBACK").unwrap();
            let result = worker.join().unwrap();
            panic!("{name} candidate did not reach the actual BEGIN IMMEDIATE lock: {result:?}");
        }
        drop(waiting);
        let commit = writer.execute_batch("COMMIT");
        if let Err(error) = commit {
            writer.execute_batch("ROLLBACK").unwrap();
            state.lock().unwrap().1 = true;
            signal.notify_all();
            let result = worker.join().unwrap();
            panic!(
                "{name} competing writer could not commit during the proven lock handoff: {error}; result={result:?}"
            );
        }
        let expected = table_rows(&writer, "project_metadata");
        let before_bytes = std::fs::read(&path).unwrap();
        state.lock().unwrap().1 = true;
        signal.notify_all();
        let result = worker.join().unwrap();
        let actual = table_rows(&writer, "project_metadata");
        writer.close().unwrap();
        assert_eq!(
            result.unwrap_err().code(),
            "PROJECT_INVALID_DATABASE",
            "upgrade={name}"
        );
        assert_eq!(actual, expected, "upgrade={name}");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before_bytes,
            "upgrade={name}"
        );
    }
}
