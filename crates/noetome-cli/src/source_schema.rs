//! Schema for source registration, independent of knowledge processing.
use rusqlite::Connection;

use crate::project::{ProjectError, ProjectInfo};

pub(crate) const SCHEMA: &str = "
CREATE TABLE project_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    project_id TEXT NOT NULL UNIQUE,
    schema_version INTEGER NOT NULL CHECK (schema_version = 2),
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
    media_type TEXT NOT NULL CHECK (media_type IN ('text/plain', 'text/markdown')),
    display_name TEXT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64 AND content_sha256 NOT GLOB '*[^0-9a-f]*'),
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    normalized_sha256 TEXT,
    normalization_version TEXT,
    ingest_status TEXT NOT NULL CHECK (ingest_status IN ('ready', 'failed')),
    raw_snapshot TEXT NOT NULL,
    normalized_snapshot TEXT,
    error_code TEXT,
    error_message TEXT,
    UNIQUE (source_id, content_sha256),
    UNIQUE (source_id, revision),
    UNIQUE (version_id, source_id),
    CHECK ((ingest_status = 'ready' AND normalized_sha256 IS NOT NULL AND normalization_version IS NOT NULL AND normalized_snapshot IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
        OR (ingest_status = 'failed' AND normalized_sha256 IS NULL AND normalization_version IS NULL AND normalized_snapshot IS NULL AND error_code IS NOT NULL AND error_message IS NOT NULL))
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
PRAGMA user_version = 2;
PRAGMA application_id = 1313817940;
";

pub(crate) fn shape(
    connection: &Connection,
) -> Result<Vec<(String, String, String)>, ProjectError> {
    connection
        .prepare("SELECT name, type, coalesce(sql, '') FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name")
        .and_then(|mut statement| {
            statement.query_map([], |row| {
                let sql: String = row.get(2)?;
                Ok((row.get(0)?, row.get(1)?, sql.split_whitespace().collect::<Vec<_>>().join(" ")))
            })?.collect()
        })
        .map_err(ProjectError::database)
}

pub(crate) fn validate(connection: &Connection) -> Result<(), ProjectError> {
    let reference = Connection::open_in_memory().map_err(ProjectError::database)?;
    reference
        .execute_batch(SCHEMA)
        .map_err(ProjectError::database)?;
    if shape(connection)? != shape(&reference)? {
        return Err(ProjectError::invalid_database(
            "Schema 2 表、字段或约束不符合来源登记契约；没有修改数据库",
        ));
    }
    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(ProjectError::database)?;
    if foreign_keys
        .query([])
        .map_err(ProjectError::database)?
        .next()
        .map_err(ProjectError::database)?
        .is_some()
    {
        return Err(ProjectError::invalid_database("来源数据库存在无效外键引用"));
    }
    Ok(())
}

pub(crate) fn upgrade(
    connection: &mut Connection,
    info: &ProjectInfo,
) -> Result<ProjectInfo, ProjectError> {
    if info.schema_version == 3 {
        crate::pdf_source_schema::validate(connection)?;
        return Ok(info.clone());
    }
    if info.schema_version == 2 {
        validate(connection)?;
        return Ok(info.clone());
    }
    if info.schema_version != 1 {
        return Err(ProjectError::new(
            "PROJECT_UNSUPPORTED_SCHEMA",
            2,
            "来源登记只支持项目Schema 1、2和3。",
        ));
    }
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(ProjectError::database)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(ProjectError::database)?;
    transaction
        .execute_batch("DROP TABLE project_metadata;")
        .map_err(ProjectError::database)?;
    transaction
        .execute_batch(SCHEMA)
        .map_err(ProjectError::database)?;
    transaction.execute("INSERT INTO project_metadata(singleton,project_id,schema_version,created_at,state) VALUES(1,?1,2,?2,?3)", rusqlite::params![info.project_id,info.created_at,info.state]).map_err(ProjectError::database)?;
    validate(&transaction)?;
    transaction.commit().map_err(ProjectError::database)?;
    let mut updated = info.clone();
    updated.schema_version = 2;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_schema_rejects_unknown_objects_and_changed_constraints() {
        for change in [
            "CREATE TABLE unknown_user_table (value TEXT)",
            "DROP TABLE source_imports; CREATE TABLE source_imports (import_id TEXT)",
        ] {
            let database = Connection::open_in_memory().unwrap();
            database.execute_batch(SCHEMA).unwrap();
            database.execute_batch(change).unwrap();
            assert!(
                validate(&database).is_err(),
                "accepted schema change: {change}"
            );
        }
    }

    #[test]
    fn exact_source_schema_accepts_its_empty_registration_state() {
        let database = Connection::open_in_memory().unwrap();
        database.execute_batch(SCHEMA).unwrap();
        assert!(validate(&database).is_ok());
    }

    #[test]
    fn upgrading_a_candidate_preserves_identity_and_rejects_unknown_schema_atomically() {
        let mut database = Connection::open_in_memory().unwrap();
        database.execute_batch("CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK(singleton=1), project_id TEXT NOT NULL UNIQUE, schema_version INTEGER NOT NULL CHECK(schema_version=1), created_at TEXT NOT NULL, state TEXT NOT NULL CHECK(state='initialized')); PRAGMA user_version=1; PRAGMA application_id=1313817940;").unwrap();
        let info = ProjectInfo {
            project_id: "5232018c-42e9-4db6-8e2e-58d8e40d9e66".into(),
            schema_version: 1,
            created_at: "2026-11-01T00:00:00Z".into(),
            state: "initialized".into(),
            database: "project.sqlite".into(),
        };
        database
            .execute(
                "INSERT INTO project_metadata VALUES(1,?1,1,?2,'initialized')",
                rusqlite::params![info.project_id, info.created_at],
            )
            .unwrap();
        database
            .execute_batch("CREATE TABLE unknown_user_table(value TEXT)")
            .unwrap();
        assert!(upgrade(&mut database, &info).is_err());
        assert_eq!(
            database
                .query_row("SELECT schema_version FROM project_metadata", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            1
        );
        database
            .execute_batch("DROP TABLE unknown_user_table")
            .unwrap();
        let updated = upgrade(&mut database, &info).unwrap();
        assert_eq!(updated.schema_version, 2);
        assert_eq!(updated.project_id, info.project_id);
        assert_eq!(updated.created_at, info.created_at);
        assert_eq!(
            database
                .query_row(
                    "SELECT project_id, created_at, schema_version FROM project_metadata",
                    [],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?
                    ))
                )
                .unwrap(),
            (info.project_id, info.created_at, 2)
        );
        assert!(validate(&database).is_ok());
    }
}
