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
    CHECK ((media_type IN ('text/plain', 'text/markdown') AND
            ((ingest_status = 'ready' AND normalized_sha256 IS NOT NULL AND normalization_version IS NOT NULL AND normalized_snapshot IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
             OR (ingest_status = 'failed' AND normalized_sha256 IS NULL AND normalization_version IS NULL AND normalized_snapshot IS NULL AND error_code IS NOT NULL AND error_message IS NOT NULL)))
        OR (media_type = 'application/pdf' AND ingest_status = 'acquired' AND normalized_sha256 IS NULL AND normalization_version IS NULL AND normalized_snapshot IS NULL AND error_code IS NULL AND error_message IS NULL AND raw_snapshot = '.noetome-sources/' || version_id || '/raw.pdf'))
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
    source_version_id TEXT NOT NULL REFERENCES source_versions(version_id),
    created_at TEXT NOT NULL,
    cache_key TEXT NOT NULL CHECK (length(cache_key) = 64 AND cache_key NOT GLOB '*[^0-9a-f]*'),
    protocol_version INTEGER NOT NULL CHECK (protocol_version = 1),
    requested_parser_json TEXT NOT NULL CHECK (json_valid(requested_parser_json) AND json_type(requested_parser_json) = 'object'),
    parser_json TEXT CHECK (parser_json IS NULL OR (json_valid(parser_json) AND json_type(parser_json) = 'object')),
    import_options_version TEXT NOT NULL,
    adapter_sha256 TEXT NOT NULL CHECK (length(adapter_sha256) = 64 AND adapter_sha256 NOT GLOB '*[^0-9a-f]*'),
    status TEXT NOT NULL CHECK (status IN ('parsed', 'needs_review', 'failed')),
    outcome_kind TEXT NOT NULL CHECK (outcome_kind IN ('validated', 'host_failed')),
    page_count INTEGER CHECK (page_count IS NULL OR page_count >= 0),
    confirmed_page_count INTEGER NOT NULL CHECK (confirmed_page_count >= 0),
    parsed_page_count INTEGER NOT NULL CHECK (parsed_page_count >= 0),
    needs_review_page_count INTEGER NOT NULL CHECK (needs_review_page_count >= 0),
    failed_page_count INTEGER NOT NULL CHECK (failed_page_count >= 0),
    region_count INTEGER NOT NULL CHECK (region_count >= 0),
    snapshot_path TEXT NOT NULL UNIQUE,
    outcome_sha256 TEXT NOT NULL CHECK (length(outcome_sha256) = 64 AND outcome_sha256 NOT GLOB '*[^0-9a-f]*'),
    manifest_sha256 TEXT CHECK (manifest_sha256 IS NULL OR (length(manifest_sha256) = 64 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*')),
    error_code TEXT,
    error_message TEXT,
    UNIQUE (parse_id, source_version_id),
    CHECK (confirmed_page_count = parsed_page_count + needs_review_page_count + failed_page_count),
    CHECK (page_count IS NULL OR page_count = confirmed_page_count),
    CHECK ((status IN ('parsed', 'needs_review') AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'failed' AND error_code IS NOT NULL AND error_message IS NOT NULL)),
    CHECK ((outcome_kind = 'validated' AND parser_json IS NOT NULL AND manifest_sha256 IS NOT NULL)
        OR (outcome_kind = 'host_failed' AND status = 'failed' AND page_count IS NULL AND confirmed_page_count = 0 AND parsed_page_count = 0 AND needs_review_page_count = 0 AND failed_page_count = 0 AND region_count = 0 AND parser_json IS NULL AND manifest_sha256 IS NULL))
);
CREATE TABLE pdf_pages (
    parse_id TEXT NOT NULL REFERENCES pdf_parses(parse_id),
    physical_page_index INTEGER NOT NULL CHECK (physical_page_index >= 0),
    display_page_number INTEGER NOT NULL CHECK (display_page_number = physical_page_index + 1),
    printed_page_label TEXT,
    status TEXT NOT NULL CHECK (status IN ('parsed', 'needs_review', 'failed')),
    record_json TEXT NOT NULL CHECK (json_valid(record_json) AND json_type(record_json) = 'object'),
    PRIMARY KEY (parse_id, physical_page_index)
);
CREATE TABLE pdf_resources (
    parse_id TEXT NOT NULL REFERENCES pdf_parses(parse_id),
    resource_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('binary', 'utf8')),
    media_type TEXT NOT NULL,
    confidence TEXT NOT NULL CHECK (confidence IN ('confirmed', 'unconfirmed', 'diagnostic')),
    PRIMARY KEY (parse_id, resource_id),
    UNIQUE (parse_id, relative_path)
);
CREATE TABLE pdf_regions (
    parse_id TEXT NOT NULL,
    region_id TEXT NOT NULL,
    physical_page_index INTEGER NOT NULL CHECK (physical_page_index >= 0),
    locator_kind TEXT NOT NULL CHECK (locator_kind = 'pdf_page_region'),
    bbox_left REAL NOT NULL CHECK (bbox_left >= 0 AND bbox_left <= 1.7976931348623157e308),
    bbox_top REAL NOT NULL CHECK (bbox_top >= 0 AND bbox_top <= 1.7976931348623157e308),
    bbox_right REAL NOT NULL CHECK (bbox_right > bbox_left AND bbox_right <= 1.7976931348623157e308),
    bbox_bottom REAL NOT NULL CHECK (bbox_bottom > bbox_top AND bbox_bottom <= 1.7976931348623157e308),
    text_resource_id TEXT NOT NULL,
    text_resource_sha256 TEXT NOT NULL CHECK (length(text_resource_sha256) = 64 AND text_resource_sha256 NOT GLOB '*[^0-9a-f]*'),
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte > start_byte),
    glyph_indices_json TEXT NOT NULL CHECK (json_valid(glyph_indices_json) AND json_type(glyph_indices_json) = 'array'),
    PRIMARY KEY (parse_id, region_id),
    FOREIGN KEY (parse_id, physical_page_index) REFERENCES pdf_pages(parse_id, physical_page_index),
    FOREIGN KEY (parse_id, text_resource_id) REFERENCES pdf_resources(parse_id, resource_id)
);
CREATE TABLE pdf_imports (
    import_id TEXT PRIMARY KEY NOT NULL,
    imported_at TEXT NOT NULL,
    selected_path TEXT NOT NULL,
    source_id TEXT REFERENCES sources(source_id),
    version_id TEXT,
    parse_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('ready', 'needs_review', 'failed')),
    reused INTEGER NOT NULL CHECK (reused IN (0, 1)),
    parse_reused INTEGER NOT NULL CHECK (parse_reused IN (0, 1)),
    acquisition_status TEXT NOT NULL CHECK (acquisition_status IN ('acquired', 'failed')),
    error_code TEXT,
    error_message TEXT,
    FOREIGN KEY (version_id, source_id) REFERENCES source_versions(version_id, source_id),
    FOREIGN KEY (parse_id, version_id) REFERENCES pdf_parses(parse_id, source_version_id),
    CHECK ((status IN ('ready', 'needs_review') AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'failed' AND error_code IS NOT NULL AND error_message IS NOT NULL)),
    CHECK ((acquisition_status = 'acquired' AND source_id IS NOT NULL AND version_id IS NOT NULL AND parse_id IS NOT NULL)
        OR (acquisition_status = 'failed' AND status = 'failed' AND version_id IS NULL AND parse_id IS NULL AND reused = 0 AND parse_reused = 0))
);
PRAGMA user_version = 3;
PRAGMA application_id = 1313817940;
