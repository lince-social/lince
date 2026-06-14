CREATE TABLE IF NOT EXISTS transfer_relation (
    id INTEGER PRIMARY KEY,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    relation_type TEXT NOT NULL CHECK (relation_type IN ('parent', 'depends_on')),
    target_transfer_uid TEXT NOT NULL CHECK (length(trim(target_transfer_uid)) > 0),
    position REAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_relation_transfer_type
ON transfer_relation(transfer_uid, relation_type);
CREATE INDEX IF NOT EXISTS idx_transfer_relation_target_type
ON transfer_relation(target_transfer_uid, relation_type);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_relation_identity
ON transfer_relation(transfer_uid, relation_type, target_transfer_uid);

CREATE TABLE IF NOT EXISTS transfer_tree_config (
    id INTEGER PRIMARY KEY,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    branch_mode TEXT NOT NULL CHECK (branch_mode IN ('inherit', 'duplicated', 'greedy')),
    record_sync_mode TEXT NOT NULL CHECK (record_sync_mode IN ('none', 'copy_once', 'live')),
    source_record_id INTEGER REFERENCES record(id),
    sync_role TEXT CHECK (sync_role IS NULL OR sync_role IN ('need', 'contribution')),
    sync_quantity REAL,
    sync_counterparty_label TEXT,
    sync_target_organ_id INTEGER,
    last_synced_record_head TEXT,
    sync_enabled INTEGER NOT NULL DEFAULT 0,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_tree_config_uid
ON transfer_tree_config(transfer_uid);
