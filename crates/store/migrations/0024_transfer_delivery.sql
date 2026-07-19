-- Phase 8: origin-authoritative, recipient-specific Transfer delivery.
-- Replicas remain isolated JSON read models and never populate Transfer sidecars.

CREATE TABLE transfer_delivery_policy (
    uid                  TEXT PRIMARY KEY,
    transfer_uid         TEXT NOT NULL REFERENCES transfer(record_uid),
    origin_organ_uid     TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    mode                 TEXT NOT NULL DEFAULT 'hosted'
                         CHECK (mode IN ('hosted', 'replicated')),
    state                TEXT NOT NULL DEFAULT 'active'
                         CHECK (state IN ('active', 'revoked')),
    revision             INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (transfer_uid, recipient_person_uid, recipient_organ_uid)
) STRICT;

CREATE TABLE transfer_delivery_policy_event (
    uid                  TEXT PRIMARY KEY,
    delivery_uid         TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    revision             INTEGER NOT NULL CHECK (revision > 0),
    kind                 TEXT NOT NULL CHECK (kind IN ('created', 'mode_changed', 'revoked')),
    from_mode            TEXT CHECK (from_mode IS NULL OR from_mode IN ('hosted', 'replicated')),
    to_mode              TEXT NOT NULL CHECK (to_mode IN ('hosted', 'replicated')),
    from_state           TEXT CHECK (from_state IS NULL OR from_state IN ('active', 'revoked')),
    to_state             TEXT NOT NULL CHECK (to_state IN ('active', 'revoked')),
    actor_person_uid     TEXT NOT NULL,
    fact_uid             TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id           TEXT NOT NULL UNIQUE,
    created_at           TEXT NOT NULL,
    UNIQUE (delivery_uid, revision),
    CHECK (
        (kind = 'created' AND revision = 1 AND from_mode IS NULL AND from_state IS NULL)
        OR (kind = 'mode_changed' AND from_mode IS NOT NULL AND from_state = 'active' AND to_state = 'active')
        OR (kind = 'revoked' AND from_mode IS NOT NULL AND from_state = 'active' AND to_state = 'revoked')
    )
) STRICT;

CREATE TRIGGER transfer_delivery_policy_identity_immutable
BEFORE UPDATE ON transfer_delivery_policy
WHEN NEW.transfer_uid != OLD.transfer_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.recipient_person_uid != OLD.recipient_person_uid
  OR NEW.recipient_organ_uid != OLD.recipient_organ_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery identity is immutable');
END;

CREATE TRIGGER transfer_delivery_policy_event_immutable_update
BEFORE UPDATE ON transfer_delivery_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery policy events are immutable');
END;

CREATE TRIGGER transfer_delivery_policy_event_immutable_delete
BEFORE DELETE ON transfer_delivery_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery policy events are immutable');
END;

CREATE TABLE transfer_delivery_outbox (
    uid               TEXT PRIMARY KEY,
    envelope_uid      TEXT NOT NULL UNIQUE,
    delivery_uid      TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    transfer_revision INTEGER NOT NULL CHECK (transfer_revision > 0),
    payload            TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'queued'
                       CHECK (status IN ('queued', 'sent', 'failed', 'cancelled')),
    attempts           INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at    TEXT NOT NULL,
    last_attempt_at    TEXT,
    last_error         TEXT,
    acknowledged_cursor INTEGER CHECK (acknowledged_cursor IS NULL OR acknowledged_cursor >= cursor),
    created_at         TEXT NOT NULL,
    sent_at            TEXT,
    UNIQUE (delivery_uid, cursor)
) STRICT;
CREATE INDEX transfer_delivery_outbox_due
    ON transfer_delivery_outbox(status, next_attempt_at, created_at);

CREATE TABLE transfer_delivery_retry_event (
    uid          TEXT PRIMARY KEY,
    delivery_uid TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    outbox_uid   TEXT NOT NULL REFERENCES transfer_delivery_outbox(uid),
    request_id   TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_delivery_retry_event_immutable_update
BEFORE UPDATE ON transfer_delivery_retry_event
BEGIN SELECT RAISE(ABORT, 'Transfer delivery retry events are immutable'); END;

CREATE TRIGGER transfer_delivery_retry_event_immutable_delete
BEFORE DELETE ON transfer_delivery_retry_event
BEGIN SELECT RAISE(ABORT, 'Transfer delivery retry events are immutable'); END;

CREATE TRIGGER transfer_delivery_outbox_identity_immutable
BEFORE UPDATE ON transfer_delivery_outbox
WHEN NEW.envelope_uid != OLD.envelope_uid
  OR NEW.delivery_uid != OLD.delivery_uid
  OR NEW.cursor != OLD.cursor
  OR NEW.transfer_revision != OLD.transfer_revision
  OR NEW.payload != OLD.payload
  OR NEW.payload_hash != OLD.payload_hash
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery envelope identity is immutable');
END;

CREATE TABLE transfer_delivery_receipt (
    uid               TEXT PRIMARY KEY,
    delivery_uid      TEXT NOT NULL,
    envelope_uid      TEXT NOT NULL,
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    kind              TEXT NOT NULL CHECK (kind IN ('received', 'seen')),
    actor_organ_uid   TEXT NOT NULL,
    payload_hash      TEXT NOT NULL,
    key_id            TEXT NOT NULL,
    signature         TEXT NOT NULL,
    signed_payload    TEXT NOT NULL CHECK (json_valid(signed_payload)),
    local_fact_uid    TEXT UNIQUE REFERENCES fact(uid),
    request_id        TEXT NOT NULL UNIQUE,
    created_at        TEXT NOT NULL,
    UNIQUE (delivery_uid, envelope_uid, kind, actor_organ_uid)
) STRICT;

CREATE TRIGGER transfer_delivery_receipt_immutable_update
BEFORE UPDATE ON transfer_delivery_receipt
BEGIN
    SELECT RAISE(ABORT, 'Transfer package receipts are immutable');
END;

CREATE TRIGGER transfer_delivery_receipt_immutable_delete
BEFORE DELETE ON transfer_delivery_receipt
BEGIN
    SELECT RAISE(ABORT, 'Transfer package receipts are immutable');
END;

CREATE TABLE transfer_remote_reference (
    uid                  TEXT PRIMARY KEY,
    origin_organ_uid     TEXT NOT NULL,
    transfer_uid         TEXT NOT NULL,
    delivery_policy_uid  TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    mode                 TEXT NOT NULL DEFAULT 'hosted'
                         CHECK (mode IN ('hosted', 'replicated')),
    state                TEXT NOT NULL DEFAULT 'active'
                         CHECK (state IN ('active', 'revoked')),
    policy_revision      INTEGER NOT NULL DEFAULT 1 CHECK (policy_revision > 0),
    hosted_url           TEXT,
    last_cursor          INTEGER NOT NULL DEFAULT 0 CHECK (last_cursor >= 0),
    last_transfer_revision INTEGER NOT NULL DEFAULT 0 CHECK (last_transfer_revision >= 0),
    last_envelope_uid    TEXT,
    last_payload_hash    TEXT,
    projection           TEXT CHECK (projection IS NULL OR json_valid(projection)),
    disclosure           TEXT CHECK (disclosure IS NULL OR json_valid(disclosure)),
    last_fetched_at      TEXT,
    last_error           TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (origin_organ_uid, transfer_uid, recipient_person_uid, recipient_organ_uid)
) STRICT;

CREATE TABLE transfer_remote_policy_event (
    uid             TEXT PRIMARY KEY,
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('reference', 'mode_changed', 'revoked')),
    mode            TEXT NOT NULL CHECK (mode IN ('hosted', 'replicated')),
    state           TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    envelope_uid    TEXT,
    payload_hash    TEXT NOT NULL,
    signed_payload  TEXT NOT NULL CHECK (json_valid(signed_payload)),
    received_at     TEXT NOT NULL,
    UNIQUE (reference_uid, policy_revision)
) STRICT;

CREATE TRIGGER transfer_remote_policy_event_immutable_update
BEFORE UPDATE ON transfer_remote_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer policy events are immutable');
END;

CREATE TABLE transfer_delivery_pull_request (
    uid             TEXT PRIMARY KEY,
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    request_id      TEXT NOT NULL UNIQUE,
    after_cursor    INTEGER NOT NULL CHECK (after_cursor >= 0),
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'failed', 'completed', 'cancelled')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TEXT NOT NULL,
    last_attempt_at TEXT,
    last_error      TEXT,
    completed_cursor INTEGER CHECK (completed_cursor IS NULL OR completed_cursor >= after_cursor),
    created_at      TEXT NOT NULL,
    completed_at    TEXT
) STRICT;
CREATE INDEX transfer_delivery_pull_due
    ON transfer_delivery_pull_request(status, next_attempt_at, created_at);

CREATE TRIGGER transfer_delivery_pull_identity_immutable
BEFORE UPDATE ON transfer_delivery_pull_request
WHEN NEW.reference_uid != OLD.reference_uid
  OR NEW.request_id != OLD.request_id
  OR NEW.after_cursor != OLD.after_cursor
BEGIN SELECT RAISE(ABORT, 'Transfer pull request identity is immutable'); END;

CREATE TRIGGER transfer_remote_policy_event_immutable_delete
BEFORE DELETE ON transfer_remote_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer policy events are immutable');
END;

CREATE TABLE transfer_replica_envelope (
    envelope_uid      TEXT PRIMARY KEY,
    reference_uid     TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid  TEXT NOT NULL,
    transfer_uid      TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    transfer_revision INTEGER NOT NULL CHECK (transfer_revision > 0),
    payload           TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash      TEXT NOT NULL,
    received_at       TEXT NOT NULL,
    UNIQUE (reference_uid, cursor)
) STRICT;

CREATE TRIGGER transfer_replica_envelope_immutable_update
BEFORE UPDATE ON transfer_replica_envelope
BEGIN
    SELECT RAISE(ABORT, 'Transfer replica envelopes are immutable');
END;

CREATE TRIGGER transfer_replica_envelope_immutable_delete
BEFORE DELETE ON transfer_replica_envelope
BEGIN
    SELECT RAISE(ABORT, 'Transfer replica envelopes are immutable');
END;

CREATE TABLE transfer_remote_conflict (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    recipient_person_uid   TEXT NOT NULL,
    command_uid            TEXT,
    request_id             TEXT,
    envelope_uid           TEXT,
    submitted_revision     INTEGER CHECK (submitted_revision IS NULL OR submitted_revision >= 0),
    authoritative_revision INTEGER NOT NULL CHECK (authoritative_revision >= 0),
    authoritative_cursor   INTEGER NOT NULL CHECK (authoritative_cursor >= 0),
    code                   TEXT NOT NULL,
    reviewed_payload       TEXT NOT NULL CHECK (json_valid(reviewed_payload)),
    created_at             TEXT NOT NULL,
    UNIQUE (origin_organ_uid, command_uid),
    UNIQUE (origin_organ_uid, request_id)
) STRICT;

CREATE TRIGGER transfer_remote_conflict_immutable_update
BEFORE UPDATE ON transfer_remote_conflict
BEGIN
    SELECT RAISE(ABORT, 'Rejected remote Transfer attempts are immutable');
END;

CREATE TABLE transfer_remote_command (
    command_uid           TEXT PRIMARY KEY,
    request_id            TEXT NOT NULL UNIQUE,
    direction             TEXT NOT NULL CHECK (direction IN ('outgoing', 'incoming')),
    origin_organ_uid      TEXT NOT NULL,
    sender_organ_uid      TEXT NOT NULL,
    transfer_uid          TEXT NOT NULL,
    actor_person_uid      TEXT NOT NULL,
    expected_revision     INTEGER CHECK (expected_revision IS NULL OR expected_revision >= 0),
    payload               TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash          TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'queued'
                          CHECK (status IN ('queued', 'sent', 'accepted', 'rejected', 'failed')),
    attempts              INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at       TEXT NOT NULL,
    last_attempt_at       TEXT,
    last_error_code       TEXT,
    last_error            TEXT,
    result_payload        TEXT CHECK (result_payload IS NULL OR json_valid(result_payload)),
    authoritative_revision INTEGER CHECK (authoritative_revision IS NULL OR authoritative_revision >= 0),
    created_at            TEXT NOT NULL,
    finished_at           TEXT
) STRICT;
CREATE INDEX transfer_remote_command_due
    ON transfer_remote_command(direction, status, next_attempt_at, created_at);

CREATE TRIGGER transfer_remote_command_identity_immutable
BEFORE UPDATE ON transfer_remote_command
WHEN NEW.request_id != OLD.request_id
  OR NEW.direction != OLD.direction
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.sender_organ_uid != OLD.sender_organ_uid
  OR NEW.transfer_uid != OLD.transfer_uid
  OR NEW.actor_person_uid != OLD.actor_person_uid
  OR NEW.expected_revision IS NOT OLD.expected_revision
  OR NEW.payload != OLD.payload
  OR NEW.payload_hash != OLD.payload_hash
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer command identity is immutable');
END;

CREATE TABLE fact_remote_command (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    command_uid TEXT NOT NULL REFERENCES transfer_remote_command(command_uid)
) WITHOUT ROWID, STRICT;
CREATE INDEX fact_remote_command_by_command ON fact_remote_command(command_uid);

CREATE TRIGGER fact_remote_command_immutable_update
BEFORE UPDATE ON fact_remote_command
BEGIN SELECT RAISE(ABORT, 'Remote command Fact links are immutable'); END;

CREATE TRIGGER fact_remote_command_immutable_delete
BEFORE DELETE ON fact_remote_command
BEGIN SELECT RAISE(ABORT, 'Remote command Fact links are immutable'); END;

CREATE TABLE organ_transfer_request_nonce (
    sender_organ_uid TEXT NOT NULL,
    nonce            TEXT NOT NULL,
    request_hash     TEXT NOT NULL,
    method           TEXT NOT NULL,
    path             TEXT NOT NULL,
    request_timestamp TEXT NOT NULL,
    received_at      TEXT NOT NULL,
    PRIMARY KEY (sender_organ_uid, nonce)
) WITHOUT ROWID, STRICT;

-- Phase 8 request ids share the existing Transfer semantic idempotency
-- namespace. These triggers protect both insertion directions.
CREATE TRIGGER transfer_delivery_policy_request_collision
BEFORE INSERT ON transfer_delivery_policy_event
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

CREATE TRIGGER transfer_revision_phase8_request_collision
BEFORE INSERT ON transfer_revision
WHEN NEW.idempotency_key IS NOT NULL AND (
     EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key))
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_invitation_phase8_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_agreement_phase8_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase4_phase8_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase5_phase8_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase5_correction_phase8_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase6_phase8_request_collision
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_remote_conflict_immutable_delete
BEFORE DELETE ON transfer_remote_conflict
BEGIN
    SELECT RAISE(ABORT, 'Rejected remote Transfer attempts are immutable');
END;

CREATE TABLE transfer_application_handoff (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    participant_organ_uid  TEXT NOT NULL,
    participant_person_uid TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    occurrence_uid         TEXT NOT NULL,
    settlement_slice_uid   TEXT NOT NULL,
    origin_revision        INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_slice_hash   TEXT NOT NULL,
    state                  TEXT NOT NULL DEFAULT 'pending'
                           CHECK (state IN ('pending', 'accepted', 'rejected', 'compensated')),
    attestation_uid        TEXT,
    request_id             TEXT NOT NULL UNIQUE,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TABLE transfer_application_attestation (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    participant_organ_uid  TEXT NOT NULL,
    participant_person_uid TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    occurrence_uid         TEXT NOT NULL,
    settlement_slice_uid   TEXT NOT NULL,
    origin_revision        INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_slice_hash   TEXT NOT NULL,
    formula_commitment     TEXT NOT NULL,
    formula_version        TEXT NOT NULL,
    application_fact_uid   TEXT NOT NULL,
    applied_at             TEXT NOT NULL,
    key_id                 TEXT NOT NULL,
    signature              TEXT NOT NULL,
    payload                TEXT NOT NULL CHECK (json_valid(payload)),
    received_at            TEXT NOT NULL,
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TRIGGER transfer_application_attestation_immutable_update
BEFORE UPDATE ON transfer_application_attestation
BEGIN
    SELECT RAISE(ABORT, 'Transfer application attestations are immutable');
END;

CREATE TRIGGER transfer_application_attestation_immutable_delete
BEFORE DELETE ON transfer_application_attestation
BEGIN
    SELECT RAISE(ABORT, 'Transfer application attestations are immutable');
END;

CREATE TABLE transfer_application_handoff_event (
    uid             TEXT PRIMARY KEY,
    handoff_uid     TEXT NOT NULL REFERENCES transfer_application_handoff(uid),
    kind            TEXT NOT NULL CHECK (kind IN ('pending', 'accepted', 'rejected', 'compensated')),
    from_state      TEXT CHECK (from_state IS NULL OR from_state IN ('pending', 'accepted', 'rejected', 'compensated')),
    to_state        TEXT NOT NULL CHECK (to_state IN ('pending', 'accepted', 'rejected', 'compensated')),
    attestation_uid TEXT REFERENCES transfer_application_attestation(uid),
    fact_uid        TEXT REFERENCES fact(uid),
    reason_code     TEXT,
    request_id      TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_application_handoff_event_immutable_update
BEFORE UPDATE ON transfer_application_handoff_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer application handoff events are immutable');
END;

CREATE TRIGGER transfer_application_handoff_event_immutable_delete
BEFORE DELETE ON transfer_application_handoff_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer application handoff events are immutable');
END;

CREATE TRIGGER transfer_application_handoff_request_collision
BEFORE INSERT ON transfer_application_handoff_event
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

-- A participant retains an origin-authenticated settlement proposal beside
-- the isolated remote Transfer projection. It is deliberately not a
-- transfer_occurrence or transfer_occurrence_settlement_slice row: only the
-- origin Cell may own those canonical sidecars.
CREATE TABLE transfer_remote_application_handoff (
    uid                         TEXT PRIMARY KEY,
    reference_uid              TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid            TEXT NOT NULL,
    participant_organ_uid       TEXT NOT NULL,
    participant_person_uid      TEXT NOT NULL,
    transfer_uid                TEXT NOT NULL,
    occurrence_uid              TEXT NOT NULL,
    source_promise_uid          TEXT NOT NULL,
    settlement_slice_uid        TEXT NOT NULL,
    origin_revision             INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_quantity          REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid          TEXT,
    canonical_cumulative_before REAL NOT NULL CHECK (canonical_cumulative_before >= 0),
    canonical_cumulative_after  REAL NOT NULL CHECK (canonical_cumulative_after > canonical_cumulative_before),
    canonical_remaining_after   REAL NOT NULL CHECK (canonical_remaining_after >= 0),
    canonical_slice_hash        TEXT NOT NULL,
    envelope_uid                TEXT NOT NULL,
    envelope_payload_hash       TEXT NOT NULL,
    origin_created_at           TEXT NOT NULL,
    state                       TEXT NOT NULL DEFAULT 'pending'
                                CHECK (state IN ('pending', 'applied')),
    local_application_uid       TEXT,
    received_at                 TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TRIGGER transfer_remote_application_handoff_identity_immutable
BEFORE UPDATE ON transfer_remote_application_handoff
WHEN NEW.uid != OLD.uid
  OR NEW.reference_uid != OLD.reference_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.participant_organ_uid != OLD.participant_organ_uid
  OR NEW.participant_person_uid != OLD.participant_person_uid
  OR NEW.transfer_uid != OLD.transfer_uid
  OR NEW.occurrence_uid != OLD.occurrence_uid
  OR NEW.source_promise_uid != OLD.source_promise_uid
  OR NEW.settlement_slice_uid != OLD.settlement_slice_uid
  OR NEW.origin_revision != OLD.origin_revision
  OR NEW.canonical_quantity != OLD.canonical_quantity
  OR NEW.canonical_unit_uid IS NOT OLD.canonical_unit_uid
  OR NEW.canonical_cumulative_before != OLD.canonical_cumulative_before
  OR NEW.canonical_cumulative_after != OLD.canonical_cumulative_after
  OR NEW.canonical_remaining_after != OLD.canonical_remaining_after
  OR NEW.canonical_slice_hash != OLD.canonical_slice_hash
  OR NEW.envelope_uid != OLD.envelope_uid
  OR NEW.envelope_payload_hash != OLD.envelope_payload_hash
  OR NEW.origin_created_at != OLD.origin_created_at
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer application handoff identity is immutable');
END;

-- This table and its Fact are private to the participant Cell. The public
-- attestation stores commitments only and never references these private
-- quantities or the local Record uid.
CREATE TABLE transfer_local_application (
    uid                         TEXT PRIMARY KEY,
    handoff_uid                 TEXT NOT NULL UNIQUE REFERENCES transfer_remote_application_handoff(uid),
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
