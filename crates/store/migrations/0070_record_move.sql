CREATE TABLE record_move (
    record_uid     TEXT PRIMARY KEY,
    contact_organ  TEXT NOT NULL,
    started_at     TEXT NOT NULL,
    handed_over_at TEXT
);

CREATE INDEX record_move_by_contact ON record_move (contact_organ);
