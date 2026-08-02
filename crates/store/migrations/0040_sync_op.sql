-- Sync op log (Ontology §11): every local write on a syncable table becomes a
-- field-level op. `seq` is the outbound feed cursor; AUTOINCREMENT so a pruned
-- seq is never reused (checkpoints depend on that). Op identity is
-- (actor_organ, hlc) — an HLC is unique per actor, so the unique index IS the
-- op uid and the import idempotency check; there is no UUID column.
CREATE TABLE sync_op (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    tbl         TEXT NOT NULL,   -- record | record_extension | concept | record_assertion | fact
    uid         TEXT NOT NULL,   -- row uid in `tbl`
    field       TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL CHECK (kind IN ('set', 'tombstone', 'fact', 'crdt')),
    value       TEXT,            -- JSON for set; NULL for tombstone/fact; base64 Loro update for crdt
    hlc         INTEGER NOT NULL,
    actor_organ TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_sync_op_identity ON sync_op(actor_organ, hlc);
CREATE INDEX idx_sync_op_target ON sync_op(tbl, uid, field);

-- Per-contact sync state (Ontology §11 "Catch-up reconciliation", "Modes").
ALTER TABLE organ_contact ADD COLUMN last_synced_seq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE organ_contact ADD COLUMN mode TEXT NOT NULL DEFAULT 'replica';
ALTER TABLE organ_contact ADD COLUMN catchup_interval_secs INTEGER NOT NULL DEFAULT 30;
-- Address book (Ontology §11 "Peers"): the last address a SIGNED exchange
-- succeeded from — a cached hint, never identity. Written only after the
-- challenge verifies; discovery announces alone never touch it.
ALTER TABLE organ_contact ADD COLUMN last_seen_addr TEXT;

-- Bounded outbox (Ontology §11 "Reactive deltas"): at most ONE queued op per
-- (contact, table, uid, field) — a newer set replaces the queued one, since
-- LWW makes intermediate values dead weight. Rows reference the op log by seq;
-- the old Package-payload outbox is gone (pre-op format, no back-compat).
-- Persisted Loro record-doc (Ontology §11 "Merge"): one CRDT doc per record
-- holding its collaborative text (head/body). Loading a doc = import the
-- snapshot + every crdt op past `through_seq`; compaction refreshes the
-- snapshot so loads never replay full history.
CREATE TABLE record_doc (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    snapshot    BLOB NOT NULL,
    through_seq INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL
);

DROP TABLE sync_outbox;
CREATE TABLE sync_outbox (
    contact_organ TEXT NOT NULL,
    tbl           TEXT NOT NULL,
    uid           TEXT NOT NULL,
    field         TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    queued_at     TEXT NOT NULL,
    PRIMARY KEY (contact_organ, tbl, uid, field)
);
