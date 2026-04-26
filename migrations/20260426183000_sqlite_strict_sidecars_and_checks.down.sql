ALTER TABLE record_extension RENAME TO record_extension__strict;
CREATE TABLE record_extension (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    freestyle_data_structure TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, namespace),
    CHECK (json_valid(freestyle_data_structure))
);
INSERT INTO record_extension (
    id,
    record_id,
    namespace,
    version,
    freestyle_data_structure,
    created_at,
    updated_at
)
SELECT
    id,
    record_id,
    namespace,
    version,
    freestyle_data_structure,
    created_at,
    updated_at
FROM record_extension__strict;
DROP TABLE record_extension__strict;
CREATE INDEX idx_record_extension_namespace_record
ON record_extension(namespace, record_id);

ALTER TABLE record_link RENAME TO record_link__strict;
CREATE TABLE record_link (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,
    target_table TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    position REAL,
    freestyle_data_structure TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, link_type, target_table, target_id),
    CHECK (freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure))
);
INSERT INTO record_link (
    id,
    record_id,
    link_type,
    target_table,
    target_id,
    position,
    freestyle_data_structure,
    created_at,
    updated_at
)
SELECT
    id,
    record_id,
    link_type,
    target_table,
    target_id,
    position,
    freestyle_data_structure,
    created_at,
    updated_at
FROM record_link__strict;
DROP TABLE record_link__strict;
CREATE INDEX idx_record_link_record_type
ON record_link(record_id, link_type, target_table);
CREATE INDEX idx_record_link_target_type
ON record_link(target_table, target_id, link_type);

ALTER TABLE role RENAME TO role__strict;
CREATE TABLE role (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
INSERT INTO role (id, name)
SELECT id, name
FROM role__strict;

ALTER TABLE app_user RENAME TO app_user__strict;
CREATE TABLE app_user (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    role_id INTEGER REFERENCES role(id)
);
INSERT INTO app_user (
    id,
    name,
    username,
    password_hash,
    created_at,
    updated_at,
    role_id
)
SELECT
    id,
    name,
    username,
    password_hash,
    created_at,
    updated_at,
    role_id
FROM app_user__strict;

ALTER TABLE record_comment RENAME TO record_comment__strict;
CREATE TABLE record_comment (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TEXT
);
INSERT INTO record_comment (
    id,
    record_id,
    author_user_id,
    body,
    created_at,
    updated_at,
    deleted_at
)
SELECT
    id,
    record_id,
    author_user_id,
    body,
    created_at,
    updated_at,
    deleted_at
FROM record_comment__strict;
DROP TABLE record_comment__strict;
CREATE INDEX idx_record_comment_record_created
ON record_comment(record_id, created_at DESC);

ALTER TABLE record_worklog RENAME TO record_worklog__strict;
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
INSERT INTO record_worklog (
    id,
    record_id,
    author_user_id,
    started_at,
    ended_at,
    last_heartbeat_at,
    seconds,
    note,
    created_at,
    updated_at
)
SELECT
    id,
    record_id,
    author_user_id,
    started_at,
    ended_at,
    last_heartbeat_at,
    seconds,
    note,
    created_at,
    updated_at
FROM record_worklog__strict;
DROP TABLE record_worklog__strict;
CREATE INDEX idx_record_worklog_record_started
ON record_worklog(record_id, started_at DESC);
CREATE INDEX idx_record_worklog_author_started
ON record_worklog(author_user_id, started_at DESC);
CREATE UNIQUE INDEX idx_record_worklog_one_open_interval
ON record_worklog(record_id, author_user_id)
WHERE ended_at IS NULL;

DROP TABLE app_user__strict;
DROP TABLE role__strict;

ALTER TABLE record_resource_ref RENAME TO record_resource_ref__strict;
CREATE TABLE record_resource_ref (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_path TEXT NOT NULL,
    title TEXT,
    position REAL,
    freestyle_data_structure TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, provider, resource_path),
    CHECK (freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure))
);
INSERT INTO record_resource_ref (
    id,
    record_id,
    provider,
    resource_kind,
    resource_path,
    title,
    position,
    freestyle_data_structure,
    created_at,
    updated_at
)
SELECT
    id,
    record_id,
    provider,
    resource_kind,
    resource_path,
    title,
    position,
    freestyle_data_structure,
    created_at,
    updated_at
FROM record_resource_ref__strict;
DROP TABLE record_resource_ref__strict;
CREATE INDEX idx_record_resource_ref_record_position
ON record_resource_ref(record_id, position, id);

ALTER TABLE organ RENAME TO organ__strict;
CREATE TABLE organ (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL
);
INSERT INTO organ (id, name, base_url)
SELECT id, name, base_url
FROM organ__strict;
DROP TABLE organ__strict;

ALTER TABLE view_dependency RENAME TO view_dependency__strict;
CREATE TABLE view_dependency (
    view_id INTEGER NOT NULL REFERENCES view(id) ON DELETE CASCADE,
    table_name TEXT NOT NULL,
    PRIMARY KEY (view_id, table_name)
);
INSERT INTO view_dependency (view_id, table_name)
SELECT view_id, table_name
FROM view_dependency__strict;
DROP TABLE view_dependency__strict;
