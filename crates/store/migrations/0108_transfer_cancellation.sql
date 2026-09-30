CREATE TABLE transfer_cancellation (
    uid TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    revision INTEGER NOT NULL CHECK (revision > 0),
    exchange_path_uid TEXT NOT NULL REFERENCES transfer_exchange_path(uid),
    quantity_mantissa TEXT NOT NULL,
    quantity_scale INTEGER NOT NULL,
    occurrences TEXT NOT NULL CHECK (json_valid(occurrences)),
    required_people TEXT NOT NULL CHECK (json_valid(required_people)),
    proposer_uid TEXT NOT NULL REFERENCES record(uid),
    proposal_fact_uid TEXT REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    applied_fact_uid TEXT UNIQUE REFERENCES fact(uid)
) STRICT;

CREATE TABLE transfer_cancellation_application (
    request_id TEXT PRIMARY KEY,
    cancellation_uid TEXT NOT NULL UNIQUE REFERENCES transfer_cancellation(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    payload TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_cancellation (
    occurrence_uid TEXT PRIMARY KEY REFERENCES transfer_occurrence(uid),
    cancellation_uid TEXT NOT NULL REFERENCES transfer_cancellation(uid),
    quantity_mantissa TEXT NOT NULL,
    quantity_scale INTEGER NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
) STRICT;

CREATE TRIGGER transfer_occurrence_cancellation_no_update
BEFORE UPDATE ON transfer_occurrence_cancellation BEGIN
    SELECT RAISE(ABORT, 'completed cancellations are immutable');
END;

CREATE TRIGGER transfer_occurrence_cancellation_no_delete
BEFORE DELETE ON transfer_occurrence_cancellation BEGIN
    SELECT RAISE(ABORT, 'completed cancellations are immutable');
END;

CREATE TRIGGER transfer_cancellation_application_no_update
BEFORE UPDATE ON transfer_cancellation_application BEGIN
    SELECT RAISE(ABORT, 'completed cancellation requests are immutable');
END;

CREATE TRIGGER transfer_cancellation_application_no_delete
BEFORE DELETE ON transfer_cancellation_application BEGIN
    SELECT RAISE(ABORT, 'completed cancellation requests are immutable');
END;

CREATE TRIGGER transfer_cancellation_no_term_change
BEFORE UPDATE ON transfer_cancellation
WHEN NEW.uid != OLD.uid OR NEW.transfer_uid != OLD.transfer_uid OR NEW.revision != OLD.revision OR NEW.exchange_path_uid != OLD.exchange_path_uid OR NEW.quantity_mantissa != OLD.quantity_mantissa OR NEW.quantity_scale != OLD.quantity_scale OR NEW.occurrences != OLD.occurrences OR NEW.required_people != OLD.required_people OR NEW.proposer_uid != OLD.proposer_uid OR NEW.request_id != OLD.request_id OR NEW.payload != OLD.payload OR NEW.created_at != OLD.created_at OR (OLD.proposal_fact_uid IS NOT NULL AND NEW.proposal_fact_uid IS NOT OLD.proposal_fact_uid) OR (OLD.applied_fact_uid IS NOT NULL AND NEW.applied_fact_uid IS NOT OLD.applied_fact_uid)
BEGIN
    SELECT RAISE(ABORT, 'cancellation terms and completed evidence are immutable');
END;

CREATE TRIGGER transfer_cancellation_no_delete
BEFORE DELETE ON transfer_cancellation BEGIN
    SELECT RAISE(ABORT, 'cancellation history is immutable');
END;

CREATE TRIGGER transfer_cancellation_application_request_unused
BEFORE INSERT ON transfer_cancellation_application
WHEN EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_revision_cancellation_request_unused
BEFORE INSERT ON transfer_revision
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_invitation_event_cancellation_request_unused
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_agreement_event_cancellation_request_unused
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_open_claim_pair_cancellation_request_unused
BEFORE INSERT ON transfer_open_claim_pair
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase4_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase5_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase5_correction_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase6_bulk_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_correction_link_cancellation_request_unused
BEFORE INSERT ON transfer_correction_link
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_promise_successor_cancellation_request_unused
BEFORE INSERT ON transfer_promise_successor
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;
