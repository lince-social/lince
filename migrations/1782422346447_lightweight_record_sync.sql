ALTER TABLE organ_sync_policy ADD COLUMN sync_check_interval_seconds INTEGER NOT NULL DEFAULT 300 CHECK (sync_check_interval_seconds >= 0);

CREATE TABLE IF NOT EXISTS record_sync_peer_state (
    id INTEGER PRIMARY KEY,
    organ_id INTEGER NOT NULL REFERENCES organ(id) ON DELETE CASCADE CHECK (organ_id > 0),
    owner_scope TEXT NOT NULL CHECK (length(trim(owner_scope)) > 0),
    last_fingerprint TEXT CHECK (last_fingerprint IS NULL OR length(trim(last_fingerprint)) > 0),
    last_full_snapshot_at TEXT CHECK (last_full_snapshot_at IS NULL OR julianday(last_full_snapshot_at) IS NOT NULL),
    last_checked_at TEXT CHECK (last_checked_at IS NULL OR julianday(last_checked_at) IS NOT NULL),
    next_check_at TEXT CHECK (next_check_at IS NULL OR julianday(next_check_at) IS NOT NULL),
    last_error TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_sync_peer_state_scope ON record_sync_peer_state(organ_id, owner_scope);
