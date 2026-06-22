CREATE TABLE IF NOT EXISTS transfer_visibility_wave (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    karma_id INTEGER,
    max_visible_proximity INTEGER NOT NULL CHECK (max_visible_proximity >= 0),
    active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
    reason TEXT NOT NULL DEFAULT 'karma_consequence',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

CREATE INDEX IF NOT EXISTS idx_transfer_visibility_wave_transfer
ON transfer_visibility_wave(transfer_id);

CREATE INDEX IF NOT EXISTS idx_transfer_visibility_wave_karma
ON transfer_visibility_wave(karma_id);
