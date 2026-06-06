CREATE TABLE IF NOT EXISTS transfer_node_identity (
    id INTEGER PRIMARY KEY,
    label TEXT NOT NULL CHECK (length(trim(label)) > 0),
    public_key TEXT NOT NULL CHECK (length(trim(public_key)) > 0),
    secret_key TEXT NOT NULL CHECK (length(trim(secret_key)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_identity (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    parent_transfer_uid TEXT,
    source_transfer_uid TEXT,
    state TEXT NOT NULL CHECK (length(trim(state)) > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    coordinator_label TEXT NOT NULL CHECK (length(trim(coordinator_label)) > 0),
    proposer_label TEXT NOT NULL CHECK (length(trim(proposer_label)) > 0),
    counterparty_label TEXT NOT NULL CHECK (length(trim(counterparty_label)) > 0),
    contribution_actor_label TEXT NOT NULL CHECK (length(trim(contribution_actor_label)) > 0),
    contribution_public_key TEXT,
    need_actor_label TEXT NOT NULL CHECK (length(trim(need_actor_label)) > 0),
    need_public_key TEXT,
    target_organ_id INTEGER,
    target_organ_name TEXT,
    target_base_url TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_identity_uid ON transfer_identity(transfer_uid);
ALTER TABLE transfer_event ADD COLUMN transfer_uid TEXT;
ALTER TABLE transfer_event ADD COLUMN event_uid TEXT;
ALTER TABLE transfer_event ADD COLUMN actor_public_key TEXT;
ALTER TABLE transfer_event ADD COLUMN previous_event_uid TEXT;
ALTER TABLE transfer_event ADD COLUMN signature TEXT;
CREATE TABLE IF NOT EXISTS transfer_local_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    local_record_id INTEGER NOT NULL REFERENCES record(id),
    local_actor_label TEXT NOT NULL CHECK (length(trim(local_actor_label)) > 0),
    local_quantity_delta REAL NOT NULL,
    event_id INTEGER REFERENCES transfer_event(id),
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_local_settlement_transfer_actor ON transfer_local_settlement(transfer_id, local_actor_label);
