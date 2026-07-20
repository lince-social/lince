-- Phase 8 completion: cross-Cell settlement handoff details and durable return.
-- Public canonical quantities stay at the origin. Private application values
-- remain only in transfer_local_application on the participant Cell.

ALTER TABLE transfer_remote_application_handoff
    ADD COLUMN application_direction INTEGER NOT NULL DEFAULT 1
    CHECK (application_direction IN (-1, 1));

ALTER TABLE transfer_remote_application_handoff
    ADD COLUMN origin_state TEXT NOT NULL DEFAULT 'pending'
    CHECK (origin_state IN ('pending', 'accepted', 'rejected', 'compensated'));

CREATE TABLE transfer_application_handoff_detail (
    handoff_uid                 TEXT PRIMARY KEY
                                REFERENCES transfer_application_handoff(uid),
    source_promise_uid          TEXT NOT NULL,
    canonical_quantity          REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid          TEXT,
    canonical_cumulative_before REAL NOT NULL CHECK (canonical_cumulative_before >= 0),
    canonical_cumulative_after  REAL NOT NULL CHECK (canonical_cumulative_after > canonical_cumulative_before),
    canonical_remaining_after   REAL NOT NULL CHECK (canonical_remaining_after >= 0),
    application_direction       INTEGER NOT NULL CHECK (application_direction IN (-1, 1)),
    origin_evidence_fact_uid    TEXT UNIQUE REFERENCES fact(uid),
    origin_acceptance_fact_uid  TEXT UNIQUE REFERENCES fact(uid),
    created_at                  TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_application_handoff_detail_identity_immutable
BEFORE UPDATE ON transfer_application_handoff_detail
WHEN NEW.handoff_uid != OLD.handoff_uid
  OR NEW.source_promise_uid != OLD.source_promise_uid
  OR NEW.canonical_quantity != OLD.canonical_quantity
  OR NEW.canonical_unit_uid IS NOT OLD.canonical_unit_uid
  OR NEW.canonical_cumulative_before != OLD.canonical_cumulative_before
  OR NEW.canonical_cumulative_after != OLD.canonical_cumulative_after
  OR NEW.canonical_remaining_after != OLD.canonical_remaining_after
  OR NEW.application_direction != OLD.application_direction
  OR NEW.origin_evidence_fact_uid != OLD.origin_evidence_fact_uid
  OR NEW.created_at != OLD.created_at
BEGIN SELECT RAISE(ABORT, 'Transfer application handoff details are immutable'); END;

CREATE TRIGGER transfer_application_handoff_detail_immutable_delete
BEFORE DELETE ON transfer_application_handoff_detail
BEGIN SELECT RAISE(ABORT, 'Transfer application handoff details are immutable'); END;

CREATE TABLE transfer_application_attestation_outbox (
    attestation_uid TEXT PRIMARY KEY,
    handoff_uid     TEXT NOT NULL REFERENCES transfer_remote_application_handoff(uid),
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid TEXT NOT NULL,
    payload         TEXT NOT NULL CHECK (json_valid(payload)),
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'failed', 'sent')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TEXT NOT NULL,
    last_attempt_at TEXT,
    last_error      TEXT,
    created_at      TEXT NOT NULL,
    sent_at         TEXT
) STRICT;

CREATE INDEX transfer_application_attestation_outbox_due
    ON transfer_application_attestation_outbox(status, next_attempt_at, created_at);

CREATE TRIGGER transfer_application_attestation_outbox_identity_immutable
BEFORE UPDATE ON transfer_application_attestation_outbox
WHEN NEW.attestation_uid != OLD.attestation_uid
  OR NEW.handoff_uid != OLD.handoff_uid
  OR NEW.reference_uid != OLD.reference_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.payload != OLD.payload
  OR NEW.created_at != OLD.created_at
BEGIN SELECT RAISE(ABORT, 'Transfer application attestation identity is immutable'); END;
