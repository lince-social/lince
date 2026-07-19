-- Addressed invitations are not parties until the addressed Person accepts.
-- Terminal rows remain as history; a partial index prevents duplicate pending
-- invitations without preventing a later re-invitation after a rejection.
CREATE UNIQUE INDEX transfer_party_invitation_identity
    ON transfer_party (uid, transfer_uid, actor_uid);

CREATE TABLE transfer_invitation (
    uid                   TEXT PRIMARY KEY,
    transfer_uid          TEXT NOT NULL REFERENCES transfer(record_uid),
    addressed_person_uid  TEXT NOT NULL REFERENCES record(uid),
    invited_by_person_uid TEXT NOT NULL REFERENCES record(uid),
    status                TEXT NOT NULL DEFAULT 'pending'
                          CHECK (status IN ('pending', 'accepted', 'rejected',
                                            'withdrawn', 'expired')),
    party_uid             TEXT UNIQUE,
    expires_at            TEXT,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    CHECK (
        (status = 'accepted' AND party_uid IS NOT NULL)
        OR (status != 'accepted' AND party_uid IS NULL)
    ),
    FOREIGN KEY (party_uid, transfer_uid, addressed_person_uid)
        REFERENCES transfer_party(uid, transfer_uid, actor_uid)
);

CREATE UNIQUE INDEX transfer_invitation_one_pending
    ON transfer_invitation (transfer_uid, addressed_person_uid)
    WHERE status = 'pending';

CREATE INDEX transfer_invitation_by_transfer
    ON transfer_invitation (transfer_uid, created_at);

CREATE INDEX transfer_invitation_inbox
    ON transfer_invitation (addressed_person_uid, status, created_at);
