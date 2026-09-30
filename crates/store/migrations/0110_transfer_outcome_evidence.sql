ALTER TABLE transfer_dependency ADD COLUMN upstream_origin_uid TEXT;
ALTER TABLE transfer_dependency ADD COLUMN upstream_reference_uid TEXT REFERENCES transfer_remote_reference(uid);

CREATE TABLE transfer_outcome_evidence (
    envelope_uid TEXT PRIMARY KEY,
    reference_uid TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    cursor INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    policy_revision INTEGER NOT NULL,
    origin_created_at TEXT NOT NULL,
    received_at TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash TEXT NOT NULL,
    UNIQUE (reference_uid, cursor)
);

CREATE TRIGGER transfer_outcome_evidence_immutable_update BEFORE UPDATE ON transfer_outcome_evidence
BEGIN SELECT RAISE(ABORT, 'Transfer outcome evidence is immutable'); END;
CREATE TRIGGER transfer_outcome_evidence_immutable_delete BEFORE DELETE ON transfer_outcome_evidence
BEGIN SELECT RAISE(ABORT, 'Transfer outcome evidence is immutable'); END;
