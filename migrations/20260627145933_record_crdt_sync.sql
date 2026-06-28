ALTER TABLE record ADD COLUMN owner_organ_id INTEGER REFERENCES organ(id);
ALTER TABLE record ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
ALTER TABLE record ADD COLUMN created_at TEXT;
ALTER TABLE record ADD COLUMN updated_at TEXT;
UPDATE record SET created_at = COALESCE(created_at, CURRENT_TIMESTAMP), updated_at = COALESCE(updated_at, CURRENT_TIMESTAMP);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_sync_uid ON record(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE record_extension ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record_extension ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_extension_sync_uid ON record_extension(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE record_link ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record_link ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_link_sync_uid ON record_link(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE record_comment ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record_comment ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_comment_sync_uid ON record_comment(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE record_worklog ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record_worklog ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_worklog_sync_uid ON record_worklog(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE record_resource_ref ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE record_resource_ref ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_resource_ref_sync_uid ON record_resource_ref(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE work_metadata ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE work_metadata ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_metadata_sync_uid ON work_metadata(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE work_subject ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE work_subject ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_sync_uid ON work_subject(sync_uid) WHERE sync_uid IS NOT NULL;

ALTER TABLE work_assignment ADD COLUMN sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0);
ALTER TABLE work_assignment ADD COLUMN origin_organ_id INTEGER REFERENCES organ(id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_assignment_sync_uid ON work_assignment(sync_uid) WHERE sync_uid IS NOT NULL;

CREATE TABLE IF NOT EXISTS organ_sync_policy (
    id INTEGER PRIMARY KEY,
    organ_id INTEGER NOT NULL REFERENCES organ(id) ON DELETE CASCADE CHECK (organ_id > 0),
    sync_resources TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(sync_resources)),
    record_sync_mode TEXT NOT NULL DEFAULT 'none' CHECK (record_sync_mode IN ('none', 'sync_outgoing', 'sync_incoming', 'sync_both')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_organ_sync_policy_organ ON organ_sync_policy(organ_id);

CREATE TABLE IF NOT EXISTS record_sync_operation (
    id INTEGER PRIMARY KEY,
    operation_uid TEXT NOT NULL CHECK (length(trim(operation_uid)) > 0),
    source_organ_id INTEGER NOT NULL REFERENCES organ(id) CHECK (source_organ_id > 0),
    actor_user_id INTEGER REFERENCES app_user(id) CHECK (actor_user_id IS NULL OR actor_user_id > 0),
    root_record_sync_uid TEXT NOT NULL CHECK (length(trim(root_record_sync_uid)) > 0),
    table_name TEXT NOT NULL CHECK (length(trim(table_name)) > 0),
    row_sync_uid TEXT NOT NULL CHECK (length(trim(row_sync_uid)) > 0),
    action TEXT NOT NULL CHECK (action IN ('insert', 'update', 'delete')),
    field_payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(field_payload_json)),
    operation_clock TEXT NOT NULL CHECK (length(trim(operation_clock)) > 0),
    source_operation_uid TEXT CHECK (source_operation_uid IS NULL OR length(trim(source_operation_uid)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    applied_at TEXT CHECK (applied_at IS NULL OR julianday(applied_at) IS NOT NULL),
    sent_at TEXT CHECK (sent_at IS NULL OR julianday(sent_at) IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_sync_operation_uid ON record_sync_operation(operation_uid);
CREATE INDEX IF NOT EXISTS idx_record_sync_operation_root_clock ON record_sync_operation(root_record_sync_uid, operation_clock);
CREATE INDEX IF NOT EXISTS idx_record_sync_operation_source_clock ON record_sync_operation(source_organ_id, operation_clock);
CREATE INDEX IF NOT EXISTS idx_record_sync_operation_row ON record_sync_operation(table_name, row_sync_uid);

CREATE TABLE IF NOT EXISTS record_sync_tombstone (
    id INTEGER PRIMARY KEY,
    table_name TEXT NOT NULL CHECK (length(trim(table_name)) > 0),
    row_sync_uid TEXT NOT NULL CHECK (length(trim(row_sync_uid)) > 0),
    root_record_sync_uid TEXT NOT NULL CHECK (length(trim(root_record_sync_uid)) > 0),
    delete_clock TEXT NOT NULL CHECK (length(trim(delete_clock)) > 0),
    source_organ_id INTEGER NOT NULL REFERENCES organ(id) CHECK (source_organ_id > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_sync_tombstone_row ON record_sync_tombstone(table_name, row_sync_uid);

CREATE TABLE IF NOT EXISTS record_sync_ack (
    id INTEGER PRIMARY KEY,
    organ_id INTEGER NOT NULL REFERENCES organ(id) ON DELETE CASCADE CHECK (organ_id > 0),
    last_ack_operation_clock TEXT CHECK (last_ack_operation_clock IS NULL OR length(trim(last_ack_operation_clock)) > 0),
    last_ack_operation_uid TEXT CHECK (last_ack_operation_uid IS NULL OR length(trim(last_ack_operation_uid)) > 0),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_sync_ack_organ ON record_sync_ack(organ_id);

CREATE TABLE IF NOT EXISTS record_sync_pending_dependency (
    id INTEGER PRIMARY KEY,
    operation_uid TEXT NOT NULL CHECK (length(trim(operation_uid)) > 0),
    missing_table_name TEXT NOT NULL CHECK (length(trim(missing_table_name)) > 0),
    missing_row_sync_uid TEXT NOT NULL CHECK (length(trim(missing_row_sync_uid)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    resolved_at TEXT CHECK (resolved_at IS NULL OR julianday(resolved_at) IS NOT NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_sync_pending_dependency_missing ON record_sync_pending_dependency(missing_table_name, missing_row_sync_uid);
