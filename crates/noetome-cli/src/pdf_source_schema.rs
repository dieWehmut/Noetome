//! Exact Schema3 identity and candidate-only upgrades.
use crate::project::{ProjectError, ProjectInfo};
use rusqlite::Connection;

pub(crate) const SCHEMA: &str = "
CREATE TABLE project_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    project_id TEXT NOT NULL UNIQUE,
    schema_version INTEGER NOT NULL CHECK (schema_version = 3),
    created_at TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state = 'initialized')
);
CREATE TABLE sources (
    source_id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE source_versions (
    version_id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(source_id),
    revision INTEGER NOT NULL CHECK (revision > 0),
    created_at TEXT NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('text/plain', 'text/markdown', 'application/pdf')),
    display_name TEXT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64 AND content_sha256 NOT GLOB '*[^0-9a-f]*'),
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    normalized_sha256 TEXT,
    normalization_version TEXT,
    ingest_status TEXT NOT NULL CHECK (ingest_status IN ('ready', 'failed', 'acquired')),
    raw_snapshot TEXT NOT NULL,
    normalized_snapshot TEXT,
    error_code TEXT,
    error_message TEXT,
    UNIQUE (source_id, content_sha256),
    UNIQUE (source_id, revision),
    UNIQUE (version_id, source_id),
    CHECK ((media_type IN ('text/plain', 'text/markdown') AND ingest_status = 'ready' AND normalized_sha256 IS NOT NULL AND normalization_version IS NOT NULL AND normalized_snapshot IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
        OR (media_type IN ('text/plain', 'text/markdown') AND ingest_status = 'failed' AND normalized_sha256 IS NULL AND normalization_version IS NULL AND normalized_snapshot IS NULL AND error_code IS NOT NULL AND error_message IS NOT NULL)
        OR (media_type = 'application/pdf' AND ingest_status = 'acquired' AND normalized_sha256 IS NULL AND normalization_version IS NULL AND normalized_snapshot IS NULL AND error_code IS NULL AND error_message IS NULL))
);
CREATE TABLE source_spans (
    span_id TEXT PRIMARY KEY NOT NULL,
    source_version_id TEXT NOT NULL REFERENCES source_versions(version_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('heading', 'text', 'code', 'formatting')),
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte > start_byte),
    start_line INTEGER NOT NULL CHECK (start_line > 0),
    end_line INTEGER NOT NULL CHECK (end_line >= start_line),
    text_sha256 TEXT NOT NULL CHECK (length(text_sha256) = 64 AND text_sha256 NOT GLOB '*[^0-9a-f]*'),
    parser_version TEXT NOT NULL,
    parse_status TEXT NOT NULL CHECK (parse_status IN ('parsed', 'failed', 'needs_review')),
    UNIQUE (source_version_id, ordinal)
);
CREATE TABLE source_imports (
    import_id TEXT PRIMARY KEY NOT NULL,
    imported_at TEXT NOT NULL,
    selected_path TEXT NOT NULL,
    source_id TEXT REFERENCES sources(source_id),
    version_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('ready', 'failed')),
    reused INTEGER NOT NULL CHECK (reused IN (0, 1)),
    span_count INTEGER NOT NULL CHECK (span_count >= 0),
    pending_extraction_count INTEGER NOT NULL CHECK (pending_extraction_count = span_count),
    knowledge_status TEXT NOT NULL CHECK (knowledge_status = 'pending'),
    error_code TEXT,
    error_message TEXT,
    FOREIGN KEY (version_id, source_id) REFERENCES source_versions(version_id, source_id),
    CHECK ((status = 'ready' AND source_id IS NOT NULL AND version_id IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'failed' AND error_code IS NOT NULL AND error_message IS NOT NULL)),
    CHECK (version_id IS NOT NULL OR (reused = 0 AND span_count = 0))
);
CREATE TABLE pdf_parses (
    parse_id TEXT PRIMARY KEY NOT NULL,
    version_id TEXT NOT NULL REFERENCES source_versions(version_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    created_at TEXT NOT NULL,
    cache_key TEXT NOT NULL CHECK (length(cache_key) = 64 AND cache_key NOT GLOB '*[^0-9a-f]*'),
    config_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('ready', 'needs_review', 'failed')),
    page_count INTEGER CHECK (page_count >= 0),
    report_path TEXT NOT NULL,
    report_sha256 TEXT NOT NULL CHECK (length(report_sha256) = 64 AND report_sha256 NOT GLOB '*[^0-9a-f]*'),
    report_byte_length INTEGER NOT NULL CHECK (report_byte_length > 0),
    error_code TEXT,
    error_message TEXT,
    UNIQUE (version_id, ordinal),
    UNIQUE (parse_id, version_id),
    CHECK ((status = 'ready' AND page_count IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'needs_review' AND page_count IS NOT NULL AND error_code IS NOT NULL AND error_message IS NOT NULL)
        OR (status = 'failed' AND error_code IS NOT NULL AND error_message IS NOT NULL))
);
CREATE TABLE pdf_artifacts (
    parse_id TEXT NOT NULL REFERENCES pdf_parses(parse_id),
    artifact_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('binary', 'utf8_text')),
    relative_path TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    PRIMARY KEY (parse_id, artifact_id)
);
CREATE TABLE pdf_pages (
    parse_id TEXT NOT NULL REFERENCES pdf_parses(parse_id),
    physical_page_index INTEGER NOT NULL CHECK (physical_page_index >= 0),
    printed_page_label TEXT,
    status TEXT NOT NULL CHECK (status IN ('parsed', 'needs_review', 'failed')),
    geometry_json TEXT,
    glyph_count INTEGER CHECK (glyph_count >= 0),
    excluded_glyph_ordinals_json TEXT NOT NULL,
    text_artifact_id TEXT,
    text_artifact_sha256 TEXT,
    PRIMARY KEY (parse_id, physical_page_index),
    FOREIGN KEY (parse_id, text_artifact_id) REFERENCES pdf_artifacts(parse_id, artifact_id),
    CHECK ((text_artifact_id IS NULL AND text_artifact_sha256 IS NULL)
        OR (text_artifact_id IS NOT NULL AND text_artifact_sha256 IS NOT NULL))
);
CREATE TABLE pdf_regions (
    parse_id TEXT NOT NULL,
    physical_page_index INTEGER NOT NULL,
    region_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    bbox_json TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte > start_byte),
    glyph_ordinals_json TEXT NOT NULL,
    PRIMARY KEY (parse_id, region_id),
    UNIQUE (parse_id, physical_page_index, ordinal),
    FOREIGN KEY (parse_id, physical_page_index) REFERENCES pdf_pages(parse_id, physical_page_index),
    FOREIGN KEY (parse_id, artifact_id) REFERENCES pdf_artifacts(parse_id, artifact_id)
);
CREATE TABLE pdf_imports (
    import_id TEXT PRIMARY KEY NOT NULL,
    imported_at TEXT NOT NULL,
    selected_path TEXT NOT NULL,
    source_id TEXT REFERENCES sources(source_id),
    version_id TEXT,
    parse_id TEXT,
    raw_status TEXT NOT NULL CHECK (raw_status IN ('acquired', 'failed')),
    parse_status TEXT NOT NULL CHECK (parse_status IN ('ready', 'needs_review', 'failed', 'not_run')),
    reused_version INTEGER NOT NULL CHECK (reused_version IN (0, 1)),
    reused_parse INTEGER NOT NULL CHECK (reused_parse IN (0, 1)),
    knowledge_status TEXT NOT NULL CHECK (knowledge_status = 'pending'),
    exit_code INTEGER NOT NULL CHECK (exit_code BETWEEN 0 AND 3),
    error_code TEXT,
    error_message TEXT,
    FOREIGN KEY (version_id, source_id) REFERENCES source_versions(version_id, source_id),
    FOREIGN KEY (parse_id, version_id) REFERENCES pdf_parses(parse_id, version_id),
    CHECK ((raw_status = 'acquired' AND source_id IS NOT NULL AND version_id IS NOT NULL AND parse_id IS NOT NULL AND parse_status != 'not_run')
        OR (raw_status = 'failed' AND version_id IS NULL AND parse_id IS NULL AND parse_status = 'not_run' AND reused_version = 0 AND reused_parse = 0)),
    CHECK ((parse_status = 'ready' AND exit_code = 0 AND error_code IS NULL AND error_message IS NULL)
        OR (parse_status != 'ready' AND exit_code > 0 AND error_code IS NOT NULL AND error_message IS NOT NULL)),
    CHECK (reused_parse = 0 OR (reused_version = 1 AND parse_status = 'ready'))
);
CREATE UNIQUE INDEX pdf_ready_cache ON pdf_parses(version_id, cache_key) WHERE status = 'ready';
PRAGMA user_version = 3;
PRAGMA application_id = 1313817940;
";

pub(crate) fn validate(connection: &Connection) -> Result<(), ProjectError> {
    let reference = Connection::open_in_memory().map_err(ProjectError::database)?;
    reference
        .execute_batch(SCHEMA)
        .map_err(ProjectError::database)?;
    if crate::source_schema::shape(connection)? != crate::source_schema::shape(&reference)? {
        return Err(ProjectError::invalid_database(
            "Schema 3 表、索引、字段或约束不符合 PDF 来源契约；没有修改数据库",
        ));
    }
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(ProjectError::database)?;
    let application: u32 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(ProjectError::database)?;
    if version != 3 || application != 1313817940 {
        return Err(ProjectError::invalid_database(
            "Schema 3 版本或应用标识无效",
        ));
    }
    let mut query = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(ProjectError::database)?;
    if query
        .query([])
        .map_err(ProjectError::database)?
        .next()
        .map_err(ProjectError::database)?
        .is_some()
    {
        return Err(ProjectError::invalid_database(
            "PDF 来源数据库存在无效外键引用",
        ));
    }
    Ok(())
}

pub(crate) fn upgrade(
    connection: &mut Connection,
    info: &ProjectInfo,
) -> Result<ProjectInfo, ProjectError> {
    if info.schema_version == 3 {
        validate(connection)?;
        return Ok(info.clone());
    }
    if !matches!(info.schema_version, 1 | 2) {
        return Err(ProjectError::new(
            "PROJECT_UNSUPPORTED_SCHEMA",
            2,
            "PDF 来源登记只支持 Schema 1、2 和 3",
        ));
    }
    let current = crate::source_schema::upgrade(connection, info)?;
    crate::source_schema::validate(connection)?;
    let before = crate::source_model::Store::load(connection, &info.project_id)?;
    // The caller owns a copied candidate. Preserve every old field and original
    // row order, including failed/reused imports, without renaming FK parents.
    let mut tables = Vec::new();
    for name in [
        "sources",
        "source_versions",
        "source_spans",
        "source_imports",
    ] {
        let mut statement = connection
            .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
            .map_err(ProjectError::database)?;
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|index| row.get::<_, rusqlite::types::Value>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(ProjectError::database)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(ProjectError::database)?;
        tables.push((name, count, rows));
    }
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .map_err(ProjectError::database)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(ProjectError::database)?;
    transaction.execute_batch("DROP TABLE source_imports; DROP TABLE source_spans; DROP TABLE source_versions; DROP TABLE sources; DROP TABLE project_metadata;").map_err(ProjectError::database)?;
    transaction
        .execute_batch(SCHEMA)
        .map_err(ProjectError::database)?;
    transaction
        .execute(
            "INSERT INTO project_metadata VALUES(1,?1,3,?2,?3)",
            rusqlite::params![current.project_id, current.created_at, current.state],
        )
        .map_err(ProjectError::database)?;
    for (name, count, rows) in tables {
        let sql = format!("INSERT INTO {name} VALUES({})", vec!["?"; count].join(","));
        for row in rows {
            transaction
                .execute(&sql, rusqlite::params_from_iter(row))
                .map_err(ProjectError::database)?;
        }
    }
    validate(&transaction)?;
    let after = crate::source_model::Store::load(&transaction, &info.project_id)?;
    if before.sources != after.sources
        || before.versions != after.versions
        || before.spans != after.spans
        || before.reports != after.reports
        || !after.pdf_imports.is_empty()
    {
        return Err(ProjectError::invalid_database(
            "PDF 候选迁移改变了旧来源历史",
        ));
    }
    transaction.commit().map_err(ProjectError::database)?;
    let mut updated = current;
    updated.schema_version = 3;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_schema_rejects_extra_objects_weakened_constraints_and_wrong_identity() {
        for change in [
            "CREATE TABLE human_data(value TEXT)",
            "CREATE VIEW human_view AS SELECT parse_id FROM pdf_parses",
            "CREATE INDEX human_index ON pdf_parses(created_at)",
            "CREATE TRIGGER human_trigger AFTER INSERT ON pdf_imports BEGIN SELECT 1; END",
            "DROP INDEX pdf_ready_cache; CREATE INDEX pdf_ready_cache ON pdf_parses(version_id,cache_key)",
            "PRAGMA user_version=2",
            "PRAGMA application_id=1",
        ] {
            let database = Connection::open_in_memory().unwrap();
            database.execute_batch(SCHEMA).unwrap();
            assert!(validate(&database).is_ok());
            database.execute_batch(change).unwrap();
            assert!(validate(&database).is_err(), "accepted {change}");
        }
    }
    #[test]
    fn ready_cache_is_unique_while_real_failed_attempts_append() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection.execute("INSERT INTO sources VALUES('32175542-40d5-47e4-a62b-31209ee9fe52','2026-11-01T00:00:00Z')",[]).unwrap();
        connection.execute("INSERT INTO source_versions VALUES('42175542-40d5-47e4-a62b-31209ee9fe52','32175542-40d5-47e4-a62b-31209ee9fe52',1,'2026-11-01T00:00:00Z','application/pdf','old.pdf',?1,1,NULL,NULL,'acquired','.noetome-sources/42175542-40d5-47e4-a62b-31209ee9fe52/raw.bin',NULL,NULL,NULL)",["a".repeat(64)]).unwrap();
        for (ordinal, status) in [(0, "failed"), (1, "failed"), (2, "ready"), (3, "ready")] {
            let id = uuid::Uuid::new_v4().to_string();
            let result=connection.execute("INSERT INTO pdf_parses VALUES(?1,'42175542-40d5-47e4-a62b-31209ee9fe52',?2,'2026-11-01T00:00:00Z',?3,'{}',?4,?5,?6,?7,1,?8,?9)",rusqlite::params![id,ordinal,"b".repeat(64),status,(status == "ready").then_some(1),format!(".noetome-pdf-parses/{id}/report.json"),"c".repeat(64),(status == "failed").then_some("SIDECAR_TIMEOUT"),(status == "failed").then_some("actual timeout")]);
            assert_eq!(result.is_ok(), ordinal != 3);
        }
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pdf_parses", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn candidate_upgrade_preserves_metadata_and_legacy_history() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(crate::source_schema::SCHEMA)
            .unwrap();
        let info = ProjectInfo {
            project_id: "92175542-40d5-47e4-a62b-31209ee9fe52".into(),
            schema_version: 2,
            created_at: "2026-11-01T00:00:00Z".into(),
            state: "initialized".into(),
            database: "project.sqlite".into(),
        };
        connection
            .execute(
                "INSERT INTO project_metadata VALUES(1,?1,2,?2,'initialized')",
                rusqlite::params![info.project_id, info.created_at],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sources VALUES(?1,?2)",
                rusqlite::params!["32175542-40d5-47e4-a62b-31209ee9fe52", info.created_at],
            )
            .unwrap();
        connection.execute("INSERT INTO source_versions VALUES(?1,?2,1,?3,'text/plain','old.txt',?4,1,NULL,NULL,'failed',?5,NULL,'SOURCE_INVALID_UTF8','invalid UTF-8')", rusqlite::params!["42175542-40d5-47e4-a62b-31209ee9fe52","32175542-40d5-47e4-a62b-31209ee9fe52",info.created_at,"a".repeat(64),".noetome-sources/42175542-40d5-47e4-a62b-31209ee9fe52/raw.bin"]).unwrap();
        for (id, reused) in [
            ("52175542-40d5-47e4-a62b-31209ee9fe52", 0),
            ("62175542-40d5-47e4-a62b-31209ee9fe52", 1),
        ] {
            connection.execute("INSERT INTO source_imports VALUES(?1,?2,'D:/selected/old.txt',?3,?4,'failed',?5,0,0,'pending','SOURCE_INVALID_UTF8','invalid UTF-8')",rusqlite::params![id,info.created_at,"32175542-40d5-47e4-a62b-31209ee9fe52","42175542-40d5-47e4-a62b-31209ee9fe52",reused]).unwrap();
        }
        connection.execute("INSERT INTO source_versions VALUES(?1,?2,2,?3,'text/markdown','ready.md',?4,2,?5,?6,'ready',?7,?8,NULL,NULL)",rusqlite::params!["72175542-40d5-47e4-a62b-31209ee9fe52","32175542-40d5-47e4-a62b-31209ee9fe52",info.created_at,"b".repeat(64),"c".repeat(64),crate::text::NORMALIZATION_VERSION,".noetome-sources/72175542-40d5-47e4-a62b-31209ee9fe52/raw.bin",".noetome-sources/72175542-40d5-47e4-a62b-31209ee9fe52/normalized.txt"]).unwrap();
        connection
            .execute(
                "INSERT INTO source_spans VALUES(?1,?2,0,'text',0,2,1,1,?3,?4,'parsed')",
                rusqlite::params![
                    "82175542-40d5-47e4-a62b-31209ee9fe52",
                    "72175542-40d5-47e4-a62b-31209ee9fe52",
                    "d".repeat(64),
                    crate::text::PARSER_VERSION
                ],
            )
            .unwrap();
        for (id, reused) in [
            ("a2175542-40d5-47e4-a62b-31209ee9fe52", 0),
            ("b2175542-40d5-47e4-a62b-31209ee9fe52", 1),
        ] {
            connection.execute("INSERT INTO source_imports VALUES(?1,?2,'D:/selected/ready.md',?3,?4,'ready',?5,1,1,'pending',NULL,NULL)",rusqlite::params![id,info.created_at,"32175542-40d5-47e4-a62b-31209ee9fe52","72175542-40d5-47e4-a62b-31209ee9fe52",reused]).unwrap();
        }
        let before = crate::source_model::Store::load(&connection, &info.project_id).unwrap();
        let updated = upgrade(&mut connection, &info).unwrap();
        assert_eq!(updated.schema_version, 3);
        assert_eq!(updated.project_id, info.project_id);
        assert_eq!(updated.created_at, info.created_at);
        assert!(validate(&connection).is_ok());
        let after = crate::source_model::Store::load(&connection, &info.project_id).unwrap();
        assert_eq!(after.sources, before.sources);
        assert_eq!(after.versions, before.versions);
        assert_eq!(after.spans, before.spans);
        assert_eq!(after.reports, before.reports);
        assert_eq!(
            crate::source_schema::upgrade(&mut connection, &updated).unwrap(),
            updated
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            3
        );
    }
}
