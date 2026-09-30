ALTER TABLE transfer_local_application RENAME TO transfer_local_application_previous;

CREATE TABLE transfer_local_application (
    uid                         TEXT PRIMARY KEY,
    handoff_uid                 TEXT NOT NULL UNIQUE,
    participant_person_uid      TEXT NOT NULL,
    local_record_uid            TEXT NOT NULL REFERENCES record(uid),
    application_fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_delta                 REAL NOT NULL,
    local_cumulative_before     REAL NOT NULL,
    local_cumulative_after      REAL NOT NULL,
    application_formula         TEXT NOT NULL,
    application_formula_hash    TEXT NOT NULL,
    application_formula_version INTEGER NOT NULL CHECK (application_formula_version >= 0),
    authorization_intent_uid    TEXT REFERENCES signed_action_intent(uid),
    request_id                  TEXT NOT NULL UNIQUE,
    created_at                  TEXT NOT NULL
) STRICT;

INSERT INTO transfer_local_application SELECT * FROM transfer_local_application_previous;
DROP TABLE transfer_local_application_previous;

CREATE TRIGGER transfer_local_application_immutable_update
BEFORE UPDATE ON transfer_local_application
BEGIN
    SELECT RAISE(ABORT, 'Local Transfer applications are immutable');
END;

CREATE TRIGGER transfer_local_application_immutable_delete
BEFORE DELETE ON transfer_local_application
BEGIN
    SELECT RAISE(ABORT, 'Local Transfer applications are immutable');
END;

CREATE TRIGGER transfer_local_application_request_collision
BEFORE INSERT ON transfer_local_application
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

CREATE VIEW transfer_application_effect_handoff AS
SELECT uid, reference_uid, origin_organ_uid, participant_organ_uid, participant_person_uid,
       transfer_uid, occurrence_uid, source_promise_uid, settlement_slice_uid, origin_revision,
       canonical_quantity, canonical_unit_uid, canonical_cumulative_before, canonical_cumulative_after,
       canonical_remaining_after, application_direction, canonical_slice_hash, envelope_uid,
       envelope_payload_hash, origin_created_at, state, origin_state, local_application_uid
FROM transfer_remote_application_handoff
UNION ALL
SELECT h.uid, '', h.origin_organ_uid, h.participant_organ_uid, h.participant_person_uid,
       h.transfer_uid, h.occurrence_uid, d.source_promise_uid, h.settlement_slice_uid, h.origin_revision,
       d.canonical_quantity, d.canonical_unit_uid, d.canonical_cumulative_before, d.canonical_cumulative_after,
       d.canonical_remaining_after, d.application_direction, h.canonical_slice_hash, '', '', h.created_at,
       CASE WHEN a.uid IS NOT NULL THEN 'applied' ELSE h.state END, h.state, a.uid
FROM transfer_application_handoff h
JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
LEFT JOIN transfer_local_application a ON a.handoff_uid = h.uid
WHERE h.origin_organ_uid = h.participant_organ_uid;

CREATE TRIGGER transfer_local_application_handoff_exists
BEFORE INSERT ON transfer_local_application
WHEN NOT EXISTS (SELECT 1 FROM transfer_application_effect_handoff WHERE uid = NEW.handoff_uid
                 AND participant_person_uid = NEW.participant_person_uid)
BEGIN
    SELECT RAISE(ABORT, 'Transfer application requires a handoff for this Person');
END;
