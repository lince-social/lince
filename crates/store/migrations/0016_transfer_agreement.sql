-- Phase 3 binds every agreement transition to one signed Transfer revision.
-- Mutable agreement rows remain a current-state cache; these rows are the
-- immutable evidence and request-idempotency boundary.
-- Pre-Phase-3 cache values had no signed event authority and cannot seed a
-- quorum. Preserve the revision but require every Person to sign from level 0.
UPDATE transfer_agreement SET level = 0;
UPDATE promise SET state = 'proposed'
WHERE transfer_uid IS NOT NULL AND state = 'agreed';

CREATE TABLE transfer_agreement_event (
    uid             TEXT PRIMARY KEY,
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    party_uid       TEXT NOT NULL REFERENCES transfer_party(uid),
    person_uid      TEXT NOT NULL REFERENCES record(uid),
    from_level      INTEGER NOT NULL CHECK (from_level BETWEEN 0 AND 2),
    to_level        INTEGER NOT NULL CHECK (to_level BETWEEN 0 AND 2),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    created_at      TEXT NOT NULL,
    CHECK (abs(to_level - from_level) = 1),
    FOREIGN KEY (party_uid, transfer_uid, person_uid)
        REFERENCES transfer_party(uid, transfer_uid, actor_uid)
) STRICT;

CREATE INDEX transfer_agreement_event_history
    ON transfer_agreement_event(transfer_uid, revision, party_uid, created_at, uid);

-- The three Transfer command logs share one request-id namespace. Repository
-- preflight provides useful errors; triggers close the concurrent-insert race.
CREATE TRIGGER transfer_agreement_event_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_agreement_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_agreement_event
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by agreement action');
END;

CREATE TRIGGER transfer_invitation_agreement_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_agreement_event
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by agreement action');
END;

-- A percentage coalition is frozen once, on the transition that first crosses
-- quorum. Keeping it revision-keyed preserves old evidence while naturally
-- leaving a new revision without a coalition.
CREATE UNIQUE INDEX transfer_party_agreement_identity
    ON transfer_party(uid, transfer_uid);

CREATE TABLE transfer_agreement_coalition (
    transfer_uid   TEXT NOT NULL REFERENCES transfer(record_uid),
    revision       INTEGER NOT NULL CHECK (revision > 0),
    threshold_pct  INTEGER NOT NULL CHECK (threshold_pct BETWEEN 1 AND 100),
    eligible_count INTEGER NOT NULL CHECK (eligible_count > 0),
    frozen_by_event_uid TEXT NOT NULL UNIQUE REFERENCES transfer_agreement_event(uid),
    frozen_at      TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, revision)
) STRICT;

CREATE TABLE transfer_agreement_coalition_member (
    transfer_uid TEXT NOT NULL,
    revision     INTEGER NOT NULL,
    party_uid    TEXT NOT NULL REFERENCES transfer_party(uid),
    PRIMARY KEY (transfer_uid, revision, party_uid),
    FOREIGN KEY (transfer_uid, revision)
        REFERENCES transfer_agreement_coalition(transfer_uid, revision),
    FOREIGN KEY (party_uid, transfer_uid)
        REFERENCES transfer_party(uid, transfer_uid)
) STRICT;

-- Current dependency terms are a projection of the complete signed revision
-- snapshot. Targets are polymorphic Transfer/promise identities and are
-- validated in the repository transaction, where ownership can also be checked.
CREATE TABLE transfer_dependency (
    uid             TEXT PRIMARY KEY,
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    scope           TEXT NOT NULL CHECK (scope IN ('transfer', 'promise')),
    promise_uid     TEXT REFERENCES promise(uid),
    upstream_kind   TEXT NOT NULL CHECK (upstream_kind IN ('transfer', 'promise')),
    upstream_uid    TEXT NOT NULL CHECK (length(trim(upstream_uid)) > 0),
    required_state  TEXT NOT NULL DEFAULT 'kept' CHECK (required_state IN (
                        'open', 'proposed', 'agreed', 'active', 'kept',
                        'broken', 'withdrawn'
                    )),
    CHECK (
        (scope = 'transfer' AND promise_uid IS NULL)
        OR (scope = 'promise' AND promise_uid IS NOT NULL)
    )
) STRICT;

CREATE INDEX transfer_dependency_current
    ON transfer_dependency(transfer_uid, revision, scope, promise_uid);

CREATE INDEX transfer_dependency_upstream
    ON transfer_dependency(upstream_kind, upstream_uid);
