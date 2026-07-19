-- Phase 6 preserves source-group satiation and reviewed bulk claim batches as
-- append-only evidence. Individual claim events remain the authoritative
-- participant assertions.
CREATE TABLE transfer_source_group_result (
    uid                 TEXT PRIMARY KEY,
    source_uid          TEXT NOT NULL REFERENCES record(uid),
    policy              TEXT NOT NULL CHECK (policy = 'first_completes'),
    transfer_uid        TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision   INTEGER NOT NULL CHECK (transfer_revision > 0),
    settlement_uid      TEXT NOT NULL UNIQUE REFERENCES transfer_occurrence_settlement_slice(uid),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (source_uid, policy)
) STRICT;

CREATE TABLE transfer_source_group_loser (
    uid                 TEXT PRIMARY KEY,
    result_uid          TEXT NOT NULL REFERENCES transfer_source_group_result(uid),
    transfer_uid        TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision   INTEGER NOT NULL CHECK (transfer_revision > 0),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (result_uid, transfer_uid)
) STRICT;

CREATE TABLE transfer_phase6_bulk_request (
    uid                 TEXT PRIMARY KEY,
    idempotency_key     TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    actor_person_uid    TEXT NOT NULL REFERENCES record(uid),
    review_token        TEXT NOT NULL CHECK (length(review_token) = 64),
    item_count          INTEGER NOT NULL CHECK (item_count > 0),
    created_at          TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_phase6_bulk_item (
    bulk_uid                    TEXT NOT NULL REFERENCES transfer_phase6_bulk_request(uid),
    ordinal                     INTEGER NOT NULL CHECK (ordinal >= 0),
    occurrence_uid              TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid                TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision           INTEGER NOT NULL CHECK (transfer_revision > 0),
    role                        TEXT NOT NULL CHECK (role IN ('delivery', 'receipt')),
    expected_delivery_claimed   INTEGER NOT NULL CHECK (expected_delivery_claimed IN (0, 1)),
    expected_receipt_claimed    INTEGER NOT NULL CHECK (expected_receipt_claimed IN (0, 1)),
    claim_event_uid             TEXT NOT NULL UNIQUE REFERENCES transfer_occurrence_claim_event(uid),
    fact_uid                    TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    PRIMARY KEY (bulk_uid, ordinal),
    UNIQUE (bulk_uid, occurrence_uid, role)
) STRICT;

CREATE TRIGGER transfer_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase6_bulk_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_invitation_phase6_bulk_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_agreement_phase6_bulk_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase4_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase5_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase5_correction_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_parent_not_self_insert
BEFORE INSERT ON transfer WHEN NEW.parent_uid = NEW.record_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer cannot be its own parent');
END;

CREATE TRIGGER transfer_parent_not_self_update
BEFORE UPDATE OF parent_uid ON transfer WHEN NEW.parent_uid = NEW.record_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer cannot be its own parent');
END;
