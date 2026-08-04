-- Individual replica (Ontology §11 "Threads"): per-record-per-contact sync,
-- as distinct from the whole-Organ `sync_out`/`sync_in` feed. This is what
-- "I choose what to sync with whom" is made of, and what a conversation with
-- one contact rides on.

-- The grant root governing this Record. NULL = rides the ordinary feed, which
-- is every Record that exists today, so this migration needs no backfill.
--
-- LOCAL-ONLY and IMMUTABLE, and both halves are load-bearing:
--   * local-only — it is never a settable synced field, or a contact could
--     send a `set` moving a Record between roots and re-scope what gets
--     shared with third parties;
--   * immutable — it is stamped at CREATION and never changes, which is what
--     makes the denormalized copy on `sync_op` below safe from drift, and
--     what stops a Record from logging ops on the general feed and only then
--     becoming private (those ops would already have been served).
ALTER TABLE record ADD COLUMN replica_root TEXT;
CREATE INDEX idx_record_replica_root
    ON record(replica_root) WHERE replica_root IS NOT NULL;

-- The same root, denormalized onto each op at append time.
--
-- Why duplicate it: the three enforcement points (outbox enqueue, feed-serve,
-- import gate) must all agree, and the earlier design had each of them walk
-- Assertions transitively — three graph traversals that could disagree, plus
-- a depth limit and a denial-of-service surface. With the root on the op, all
-- three become the same indexed equality test. Traverse once at creation,
-- never per op. This is only sound because `record.replica_root` is
-- immutable; if that ever changes, this column becomes a bug.
ALTER TABLE sync_op ADD COLUMN replica_root TEXT;
CREATE INDEX idx_sync_op_replica_root ON sync_op(replica_root, seq);

-- One grant per (root, contact). A root may be granted to many contacts; a
-- Record belongs to exactly one root.
--
-- `offered` is the sharer's side before the receiver has agreed; `accepted`
-- is what actually moves ops. Acceptance is what turns "you may see this"
-- into "I keep a copy", and it is also what stops an Organ from pushing
-- unwanted Records into someone's store. Revoking is DELETING the row.
CREATE TABLE replica_grant (
    root_record   TEXT NOT NULL,
    contact_organ TEXT NOT NULL,
    state         TEXT NOT NULL CHECK (state IN ('offered', 'accepted')),
    created_at    TEXT NOT NULL,
    PRIMARY KEY (root_record, contact_organ)
);
CREATE INDEX idx_replica_grant_contact ON replica_grant(contact_organ);
