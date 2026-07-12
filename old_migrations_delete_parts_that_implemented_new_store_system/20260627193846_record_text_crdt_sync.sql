CREATE TABLE IF NOT EXISTS record_text_crdt_update (
    id INTEGER PRIMARY KEY,
    update_uid TEXT NOT NULL CHECK (length(trim(update_uid)) > 0),
    document_uid TEXT NOT NULL CHECK (length(trim(document_uid)) > 0),
    record_sync_uid TEXT NOT NULL CHECK (length(trim(record_sync_uid)) > 0),
    field_name TEXT NOT NULL CHECK (field_name IN ('head', 'body')),
    source_organ_id INTEGER NOT NULL REFERENCES organ(id) CHECK (source_organ_id > 0),
    actor_user_id INTEGER REFERENCES app_user(id) CHECK (actor_user_id IS NULL OR actor_user_id > 0),
    update_clock TEXT NOT NULL CHECK (length(trim(update_clock)) > 0),
    update_kind TEXT NOT NULL CHECK (update_kind IN ('delta', 'snapshot')),
    update_bytes_base64 TEXT NOT NULL CHECK (length(trim(update_bytes_base64)) > 0),
    materialized_text TEXT,
    sent_at TEXT CHECK (sent_at IS NULL OR julianday(sent_at) IS NOT NULL),
    compacted_at TEXT CHECK (compacted_at IS NULL OR julianday(compacted_at) IS NOT NULL),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_record_text_crdt_update_uid
    ON record_text_crdt_update(update_uid);

CREATE INDEX IF NOT EXISTS idx_record_text_crdt_update_document_clock
    ON record_text_crdt_update(document_uid, update_clock);

CREATE INDEX IF NOT EXISTS idx_record_text_crdt_update_record_field
    ON record_text_crdt_update(record_sync_uid, field_name);

CREATE INDEX IF NOT EXISTS idx_record_text_crdt_update_source_clock
    ON record_text_crdt_update(source_organ_id, update_clock);
