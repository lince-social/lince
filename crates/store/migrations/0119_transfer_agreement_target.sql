ALTER TABLE transfer_agreement ADD COLUMN last_event_uid TEXT;

CREATE TABLE transfer_agreement_target_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    revision INTEGER NOT NULL CHECK (revision > 0),
    target_level INTEGER NOT NULL CHECK (target_level BETWEEN 0 AND 2),
    fingerprint TEXT NOT NULL,
    result TEXT NOT NULL CHECK (json_valid(result)),
    created_at TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_agreement_clear_changed_authority
AFTER UPDATE OF level, revision, at ON transfer_agreement
WHEN NEW.last_event_uid IS OLD.last_event_uid
    AND (NEW.level <> OLD.level OR NEW.revision <> OLD.revision OR NEW.at <> OLD.at)
    AND (SELECT importing FROM transfer_sync_control WHERE id = 1) = 0
BEGIN
    UPDATE transfer_agreement SET last_event_uid = NULL WHERE uid = NEW.uid;
END;

CREATE TRIGGER transfer_agreement_target_request_unused
BEFORE INSERT ON transfer_agreement_target_request
WHEN EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_loan_extension WHERE request_id = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_child_request WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_revision_agreement_target_request_unused
BEFORE INSERT ON transfer_revision
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_invitation_event_agreement_target_request_unused
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_agreement_event_agreement_target_request_unused
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_open_claim_pair_agreement_target_request_unused
BEFORE INSERT ON transfer_open_claim_pair
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase4_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase5_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase5_correction_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase6_bulk_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_correction_link_agreement_target_request_unused
BEFORE INSERT ON transfer_correction_link
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_promise_successor_agreement_target_request_unused
BEFORE INSERT ON transfer_promise_successor
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_cancellation_application_agreement_target_request_unused
BEFORE INSERT ON transfer_cancellation_application
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_loan_extension_agreement_target_request_unused
BEFORE INSERT ON transfer_loan_extension
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_child_request_agreement_target_request_unused
BEFORE INSERT ON transfer_child_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;
