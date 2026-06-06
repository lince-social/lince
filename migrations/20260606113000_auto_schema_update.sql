CREATE TABLE IF NOT EXISTS transfer_sync_outbox (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    target_base_url TEXT NOT NULL CHECK (length(trim(target_base_url)) > 0),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempt_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_sync_outbox_transfer_target
ON transfer_sync_outbox(transfer_id, target_base_url);
