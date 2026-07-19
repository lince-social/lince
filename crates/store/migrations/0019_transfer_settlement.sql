-- Phase 5 settles concrete occurrences through immutable canonical slices.
-- Public progress is a zero-delta Transfer Fact; the owned-Record mutation and
-- formula text remain local/private and are linked by the settlement row.
ALTER TABLE configuration
    ADD COLUMN transfer_remainder_policy TEXT NOT NULL DEFAULT 'visible'
        CHECK (transfer_remainder_policy IN ('visible', 'local_draft'));

CREATE TABLE transfer_phase5_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind = 'settlement'),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_phase5_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_agreement_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_phase4_request
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase5_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_invitation_phase5_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_agreement_phase5_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_phase4_phase5_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

-- An absent row inherits configuration.transfer_remainder_policy. This policy
-- may request creation of a local draft later; it never sends or advances one.
CREATE TABLE transfer_occurrence_remainder_policy (
    occurrence_uid   TEXT PRIMARY KEY REFERENCES transfer_occurrence(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    policy           TEXT NOT NULL CHECK (policy IN ('visible', 'local_draft')),
    updated_at       TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_settlement_slice (
    uid                       TEXT PRIMARY KEY,
    occurrence_uid            TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid              TEXT NOT NULL REFERENCES transfer(record_uid),
    promise_uid               TEXT NOT NULL REFERENCES promise(uid),
    owner_person_uid          TEXT NOT NULL REFERENCES record(uid),
    canonical_quantity        REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid        TEXT REFERENCES concept(uid),
    cumulative_before         REAL NOT NULL CHECK (cumulative_before >= 0),
    cumulative_after          REAL NOT NULL CHECK (cumulative_after > cumulative_before),
    remaining_after           REAL NOT NULL CHECK (remaining_after >= 0),
    evidence_fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    application_fact_uid      TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_record_uid          TEXT NOT NULL REFERENCES record(uid),
    local_delta               REAL NOT NULL,
    local_cumulative_before   REAL NOT NULL,
    local_cumulative_after    REAL NOT NULL,
    application_formula       TEXT NOT NULL CHECK (length(trim(application_formula)) > 0),
    application_formula_hash  TEXT NOT NULL CHECK (length(application_formula_hash) = 64),
    application_formula_version INTEGER NOT NULL CHECK (application_formula_version >= 0),
    remainder_policy          TEXT NOT NULL CHECK (remainder_policy IN ('visible', 'local_draft')),
    idempotency_key           TEXT NOT NULL UNIQUE REFERENCES transfer_phase5_request(idempotency_key),
    created_at                TEXT NOT NULL,
    CHECK (cumulative_after = cumulative_before + canonical_quantity),
    CHECK (local_cumulative_after = local_cumulative_before + local_delta),
    CHECK (evidence_fact_uid != application_fact_uid)
) STRICT;

CREATE INDEX transfer_occurrence_settlement_history
    ON transfer_occurrence_settlement_slice(occurrence_uid, created_at, uid);

CREATE TRIGGER transfer_occurrence_settlement_eligible
BEFORE INSERT ON transfer_occurrence_settlement_slice
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    JOIN promise source ON source.uid = occurrence.promise_uid
    WHERE occurrence.uid = NEW.occurrence_uid
      AND occurrence.transfer_uid = NEW.transfer_uid
      AND occurrence.promise_uid = NEW.promise_uid
      AND occurrence.record_uid = NEW.local_record_uid
      AND occurrence.delivery_claimed = 1
      AND occurrence.receipt_claimed = 1
      AND occurrence.disputed = 0
      AND source.party_uid = NEW.owner_person_uid
      AND source.state = 'active'
)
BEGIN
    SELECT RAISE(ABORT, 'occurrence is not eligible for this settlement');
END;

CREATE TRIGGER transfer_occurrence_settlement_progress
BEFORE INSERT ON transfer_occurrence_settlement_slice
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    WHERE occurrence.uid = NEW.occurrence_uid
      AND NEW.cumulative_before = COALESCE((
          SELECT SUM(existing.canonical_quantity)
          FROM transfer_occurrence_settlement_slice existing
          WHERE existing.occurrence_uid = NEW.occurrence_uid
      ), 0.0)
      AND NEW.cumulative_after <= occurrence.quantity
      AND NEW.remaining_after = occurrence.quantity - NEW.cumulative_after
)
BEGIN
    SELECT RAISE(ABORT, 'settlement slice does not match current occurrence progress');
END;

CREATE TRIGGER transfer_occurrence_settlement_immutable_update
BEFORE UPDATE ON transfer_occurrence_settlement_slice
BEGIN
    SELECT RAISE(ABORT, 'settlement slices are immutable');
END;

CREATE TRIGGER transfer_occurrence_settlement_immutable_delete
BEFORE DELETE ON transfer_occurrence_settlement_slice
BEGIN
    SELECT RAISE(ABORT, 'settlement slices are immutable');
END;
