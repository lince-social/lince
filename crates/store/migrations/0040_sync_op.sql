-- Sync op log (Ontology §11): every local write on a syncable table becomes a
-- field-level op. `seq` is the outbound feed cursor; AUTOINCREMENT so a pruned
-- seq is never reused (checkpoints depend on that).
--
-- TWO identity columns, and they are not interchangeable (Ontology §11
-- "Profile vs device surfaces"):
--
--   `actor_cell`  WHO WROTE IT — one device. Op identity is
--                 (actor_cell, hlc), so the unique index IS the op uid and the
--                 import idempotency check; there is no UUID column. It must
--                 be the CELL because `nucleus::hlc` is a per-PROCESS counter:
--                 two Cells of one Organ mint identical HLC values as a matter
--                 of course, and with the Organ in this column the second
--                 Cell's op collides on this index and import drops it as an
--                 already-seen duplicate. No error, no log — the op simply
--                 never applies. `hlc.rs` states the invariant ("per-actor
--                 uniqueness"); this column is what makes it true.
--
--   `organ_uid`   WHOSE IT IS — the published identity, what a contact saved
--                 and what `record.organ_uid` is stamped from. It survives
--                 every device change. Deriving it from `actor_cell` would
--                 make one person look like three different Organs to
--                 everyone else, so it travels explicitly.
CREATE TABLE sync_op (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    tbl         TEXT NOT NULL,   -- record | record_extension | concept | record_assertion | fact
    uid         TEXT NOT NULL,   -- row uid in `tbl`
    field       TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL CHECK (kind IN ('set', 'tombstone', 'fact', 'crdt', 'snapshot')),
    -- JSON for set; NULL for tombstone/fact; base64 Loro update for crdt;
    -- base64 Loro SNAPSHOT for snapshot. A snapshot is a real op by a real
    -- Cell, not a synthesized one: the Cell that compacted asserts this doc
    -- state, stamps it with its own HLC, and it is what makes the log
    -- authoritative for collaborative text (Ontology §11, decision 2).
    value       TEXT,
    hlc         INTEGER NOT NULL,
    actor_cell  TEXT NOT NULL,
    organ_uid   TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_sync_op_identity ON sync_op(actor_cell, hlc);
CREATE INDEX idx_sync_op_target ON sync_op(tbl, uid, field);
-- The catch-up feed and the version-vector difference both scan one Organ's
-- ops in seq order, per contact per sync pass. Without this they are a full
-- table scan every time. (The grant channel's equivalent, on `replica_root`,
-- lives in 0042 where that column is added.)
CREATE INDEX idx_sync_op_feed ON sync_op(organ_uid, seq);

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
    -- Part of the key. Record tombstones and crdt ops BOTH use `field = ''`,
    -- so without this they share one slot and replace each other. Delete
    -- replacing edit is harmless; the reverse resurrects a deleted Record for
    -- one contact only, and only that contact — the kind of divergence nobody
    -- would think to look for.
    kind          TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    queued_at     TEXT NOT NULL,
    PRIMARY KEY (contact_organ, tbl, uid, field, kind)
);
