-- Phase 2 keeps one invitation identity across later attempts and records each
-- lifecycle transition as an immutable signed Fact. OPEN claims retain their
-- provenance when the source is duplicated.
ALTER TABLE transfer_invitation
    ADD COLUMN attempt INTEGER NOT NULL DEFAULT 1 CHECK (attempt > 0);

ALTER TABLE promise
    ADD COLUMN source_promise_uid TEXT REFERENCES promise(uid);

CREATE INDEX promise_source
    ON promise(source_promise_uid);

CREATE UNIQUE INDEX transfer_invitation_event_identity
    ON transfer_invitation(uid, transfer_uid);

CREATE TABLE transfer_invitation_event (
    uid             TEXT PRIMARY KEY,
    invitation_uid  TEXT NOT NULL REFERENCES transfer_invitation(uid),
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    attempt         INTEGER NOT NULL CHECK (attempt > 0),
    kind            TEXT NOT NULL CHECK (kind IN (
                        'addressed', 'accepted', 'rejected', 'withdrawn',
                        'expired', 'reopened'
                    )),
    actor_uid       TEXT REFERENCES record(uid),
    revision        INTEGER CHECK (revision IS NULL OR revision > 0),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    created_at      TEXT NOT NULL,
    FOREIGN KEY (invitation_uid, transfer_uid)
        REFERENCES transfer_invitation(uid, transfer_uid)
) STRICT;

CREATE INDEX transfer_invitation_event_history
    ON transfer_invitation_event(invitation_uid, attempt, created_at);

CREATE INDEX transfer_invitation_event_transfer
    ON transfer_invitation_event(transfer_uid, created_at);
