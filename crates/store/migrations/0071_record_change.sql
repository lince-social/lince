CREATE TABLE record_change (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    record_uid      TEXT NOT NULL,
    field           TEXT NOT NULL,
    cause           TEXT NOT NULL CHECK (cause IN ('local', 'remote')),
    winner_organ    TEXT,
    displaced       TEXT,
    displaced_local INTEGER NOT NULL DEFAULT 0,
    at              TEXT NOT NULL
);

CREATE INDEX idx_record_change_record ON record_change(record_uid, seq DESC);
CREATE INDEX idx_record_change_at ON record_change(at);
