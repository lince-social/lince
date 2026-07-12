-- Blueprint XV: organ contacts, the sync outbox, and the import quarantine.
-- An organ is a record (kind='organ'); the contact sidecar carries the trust
-- state, numeric proximity (Senses ceilings, restricted visibility), and the
-- per-organ sync policy.
CREATE TABLE organ_contact (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    trust      TEXT NOT NULL DEFAULT 'unknown',   -- unknown | known | blocked
    proximity  INTEGER NOT NULL DEFAULT 1,
    sync_out   INTEGER NOT NULL DEFAULT 0,        -- push our visible facts
    sync_in    INTEGER NOT NULL DEFAULT 0         -- pull/accept theirs
);

CREATE TABLE sync_outbox (
    uid        TEXT PRIMARY KEY,
    organ_uid  TEXT NOT NULL,                     -- destination contact
    payload    TEXT NOT NULL,                     -- serialized Package
    status     TEXT NOT NULL DEFAULT 'queued',    -- queued | sent | failed
    attempts   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    sent_at    TEXT
);

CREATE TABLE sync_quarantine (
    uid        TEXT PRIMARY KEY,
    from_organ TEXT NOT NULL,
    reason     TEXT NOT NULL,
    payload    TEXT NOT NULL,                     -- the rejected row, verbatim
    at         TEXT NOT NULL
);
