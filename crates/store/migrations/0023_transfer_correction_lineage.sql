-- Explicit correction lineage is separate from transfer.source_uid, whose
-- semantics are reserved for first_completes sibling groups.
CREATE TABLE transfer_correction_link (
    uid                    TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL CHECK (kind IN ('remainder', 'reversal')),
    source_transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    source_occurrence_uid  TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    created_transfer_uid   TEXT NOT NULL UNIQUE REFERENCES transfer(record_uid),
    source_revision        INTEGER NOT NULL CHECK (source_revision > 0),
    canonical_quantity     REAL NOT NULL CHECK (canonical_quantity > 0),
    actor_person_uid       TEXT NOT NULL REFERENCES record(uid),
    fact_uid               TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key        TEXT NOT NULL UNIQUE
        REFERENCES transfer_revision(idempotency_key),
    created_at             TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_promise_successor (
    uid                     TEXT PRIMARY KEY,
    transfer_uid            TEXT NOT NULL REFERENCES transfer(record_uid),
    predecessor_promise_uid TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    successor_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    revision                INTEGER NOT NULL CHECK (revision > 0),
    actor_person_uid        TEXT NOT NULL REFERENCES record(uid),
    fact_uid                TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key         TEXT NOT NULL UNIQUE
        REFERENCES transfer_revision(idempotency_key),
    created_at              TEXT NOT NULL,
    CHECK (predecessor_promise_uid != successor_promise_uid)
) STRICT;

CREATE TRIGGER transfer_correction_link_consistent
BEFORE INSERT ON transfer_correction_link
WHEN NEW.source_transfer_uid = NEW.created_transfer_uid
  OR NOT EXISTS (
      SELECT 1 FROM transfer_occurrence occurrence
      WHERE occurrence.uid = NEW.source_occurrence_uid
        AND occurrence.transfer_uid = NEW.source_transfer_uid
  )
  OR NOT EXISTS (
      SELECT 1 FROM transfer_revision revision
      WHERE revision.transfer_uid = NEW.created_transfer_uid
        AND revision.revision = 1
        AND revision.fact_uid = NEW.fact_uid
        AND revision.idempotency_key = NEW.idempotency_key
  )
BEGIN
    SELECT RAISE(ABORT, 'Transfer correction lineage is inconsistent');
END;

CREATE TRIGGER transfer_promise_successor_same_transfer
BEFORE INSERT ON transfer_promise_successor
WHEN NOT EXISTS (
    SELECT 1 FROM promise predecessor
    JOIN promise successor ON successor.uid = NEW.successor_promise_uid
    JOIN transfer_revision revision
      ON revision.transfer_uid = NEW.transfer_uid
     AND revision.revision = NEW.revision
     AND revision.fact_uid = NEW.fact_uid
     AND revision.idempotency_key = NEW.idempotency_key
    WHERE predecessor.uid = NEW.predecessor_promise_uid
      AND predecessor.transfer_uid = NEW.transfer_uid
      AND successor.transfer_uid = NEW.transfer_uid
      AND successor.source_promise_uid = predecessor.uid
      AND successor.revision = NEW.revision
)
BEGIN
    SELECT RAISE(ABORT, 'promise successor lineage does not match Transfer promises');
END;
