CREATE TABLE record_extension (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    data_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, namespace),
    CHECK (json_valid(data_json))
);

CREATE INDEX idx_record_extension_namespace_record
ON record_extension(namespace, record_id);

CREATE TABLE record_link (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,
    target_table TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    position REAL,
    data_json TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, link_type, target_table, target_id),
    CHECK (data_json IS NULL OR json_valid(data_json))
);

CREATE INDEX idx_record_link_record_type
ON record_link(record_id, link_type, target_table);

CREATE INDEX idx_record_link_target_type
ON record_link(target_table, target_id, link_type);

CREATE TABLE record_comment (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TEXT
);

CREATE INDEX idx_record_comment_record_created
ON record_comment(record_id, created_at DESC);

CREATE TABLE record_worklog (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    last_heartbeat_at TEXT,
    seconds REAL,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_record_worklog_record_started
ON record_worklog(record_id, started_at DESC);

CREATE INDEX idx_record_worklog_author_started
ON record_worklog(author_user_id, started_at DESC);

CREATE UNIQUE INDEX idx_record_worklog_one_open_interval
ON record_worklog(record_id, author_user_id)
WHERE ended_at IS NULL;

CREATE TABLE record_resource_ref (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_path TEXT NOT NULL,
    title TEXT,
    position REAL,
    data_json TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, provider, resource_path),
    CHECK (data_json IS NULL OR json_valid(data_json))
);

CREATE INDEX idx_record_resource_ref_record_position
ON record_resource_ref(record_id, position, id);
