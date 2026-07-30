-- Phase 5 corrections preserve settlement history. Compensation reverses only
-- the private Record application; disputes are independent participant claims.
ALTER TABLE transfer_occurrence
    ADD COLUMN system_disputed INTEGER NOT NULL DEFAULT 0
        CHECK (system_disputed IN (0, 1));

ALTER TABLE transfer_occurrence
    ADD COLUMN system_dispute_fact_uid TEXT REFERENCES fact(uid);

ALTER TABLE transfer_occurrence
    ADD COLUMN system_disputed_at TEXT;

-- Existing revision-invalidation disputes predate participant assertions.
UPDATE transfer_occurrence
SET system_disputed = disputed,
    system_dispute_fact_uid = dispute_fact_uid,
    system_disputed_at = disputed_at;

CREATE TABLE transfer_phase5_correction_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('compensation', 'dispute')),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
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
    ) OR EXISTS (
        SELECT 1 FROM transfer_phase5_request
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase5_correction_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_invitation_phase5_correction_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_agreement_phase5_correction_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_phase4_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_phase5_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TABLE transfer_occurrence_settlement_compensation (
    uid                           TEXT PRIMARY KEY,
    settlement_uid                TEXT NOT NULL UNIQUE
        REFERENCES transfer_occurrence_settlement_slice(uid),
    occurrence_uid                TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid                  TEXT NOT NULL REFERENCES transfer(record_uid),
    owner_person_uid              TEXT NOT NULL REFERENCES record(uid),
    original_application_fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    compensation_fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_record_uid              TEXT NOT NULL REFERENCES record(uid),
    inverse_delta                 REAL NOT NULL,
    idempotency_key               TEXT NOT NULL UNIQUE
        REFERENCES transfer_phase5_correction_request(idempotency_key),
    created_at                    TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_occurrence_settlement_compensation_matches_slice
BEFORE INSERT ON transfer_occurrence_settlement_compensation
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence_settlement_slice slice
    JOIN fact correction ON correction.uid = NEW.compensation_fact_uid
    WHERE slice.uid = NEW.settlement_uid
      AND slice.occurrence_uid = NEW.occurrence_uid
      AND slice.transfer_uid = NEW.transfer_uid
      AND slice.owner_person_uid = NEW.owner_person_uid
      AND slice.application_fact_uid = NEW.original_application_fact_uid
      AND slice.local_record_uid = NEW.local_record_uid
      AND NEW.inverse_delta = -slice.local_delta
      AND correction.record_uid = NEW.local_record_uid
      -- Exact pair vs the transfer side's REAL: 10^scale is built as text so
      -- this needs no math extension, and scale is capped at 18 by the kernel.
      AND CAST(correction.delta_mantissa AS REAL)
          / CAST(SUBSTR('1000000000000000000', 1, correction.delta_scale + 1) AS REAL)
          = NEW.inverse_delta
      AND correction.actor_uid = NEW.owner_person_uid
      AND correction.cause_kind = 'compensation'
      AND correction.cause_uid = NEW.original_application_fact_uid
)
BEGIN
    SELECT RAISE(ABORT, 'settlement compensation does not match its slice');
END;

CREATE TRIGGER transfer_occurrence_settlement_compensation_immutable_update
BEFORE UPDATE ON transfer_occurrence_settlement_compensation
BEGIN
    SELECT RAISE(ABORT, 'settlement compensations are immutable');
END;

CREATE TRIGGER transfer_occurrence_settlement_compensation_immutable_delete
BEFORE DELETE ON transfer_occurrence_settlement_compensation
BEGIN
    SELECT RAISE(ABORT, 'settlement compensations are immutable');
END;

CREATE TABLE transfer_occurrence_dispute_event (
    uid              TEXT PRIMARY KEY,
    occurrence_uid   TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid     TEXT NOT NULL REFERENCES transfer(record_uid),
    actor_person_uid TEXT NOT NULL REFERENCES record(uid),
    asserted         INTEGER NOT NULL CHECK (asserted IN (0, 1)),
    fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key  TEXT NOT NULL UNIQUE
        REFERENCES transfer_phase5_correction_request(idempotency_key),
    created_at       TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_occurrence_dispute_participant
BEFORE INSERT ON transfer_occurrence_dispute_event
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    JOIN fact evidence ON evidence.uid = NEW.fact_uid
    WHERE occurrence.uid = NEW.occurrence_uid
      AND occurrence.transfer_uid = NEW.transfer_uid
      AND NEW.actor_person_uid IN (
          occurrence.giver_person_uid, occurrence.receiver_person_uid
      )
      AND evidence.record_uid = NEW.transfer_uid
      AND evidence.delta_mantissa = '0'
      AND evidence.actor_uid = NEW.actor_person_uid
)
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute actor is not a participant');
END;

CREATE INDEX transfer_occurrence_dispute_history
    ON transfer_occurrence_dispute_event(
        occurrence_uid, actor_person_uid, created_at, uid
    );

CREATE TRIGGER transfer_occurrence_dispute_immutable_update
BEFORE UPDATE ON transfer_occurrence_dispute_event
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute events are immutable');
END;

CREATE TRIGGER transfer_occurrence_dispute_immutable_delete
BEFORE DELETE ON transfer_occurrence_dispute_event
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute events are immutable');
END;
