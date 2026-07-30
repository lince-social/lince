-- Karma K5.1: signed, immutable delegation-grant revisions and a CAS handle.
-- No intent/effect table references this schema in this phase.

-- The revision hash identifies consent *content*, so two people — or one person
-- replacing a revoked grant — can legitimately hold byte-identical terms. The
-- stored revision is therefore identified by the pair, not by the hash alone.
CREATE TABLE karma_grant_revision (
    grant_uid           TEXT NOT NULL
                        REFERENCES karma_grant(record_uid)
                        DEFERRABLE INITIALLY DEFERRED,
    revision_hash       TEXT NOT NULL
                        CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    schema_name         TEXT NOT NULL CHECK (schema_name = 'karma.grant.v1'),
    revision_json       TEXT NOT NULL CHECK (json_valid(revision_json)),
    principal_person_uid TEXT NOT NULL,
    signer_key_id       TEXT NOT NULL CHECK (length(signer_key_id) BETWEEN 1 AND 200),
    revision_signature  TEXT NOT NULL CHECK (length(revision_signature) BETWEEN 1 AND 2048),
    created_at          TEXT NOT NULL,
    PRIMARY KEY (grant_uid, revision_hash)
) STRICT;

CREATE TABLE karma_grant (
    record_uid           TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision      INTEGER NOT NULL CHECK (handle_revision >= 1),
    status               TEXT NOT NULL CHECK (status IN ('draft', 'active', 'revoked')),
    head_revision_hash   TEXT NOT NULL,
    active_revision_hash TEXT,
    principal_person_uid TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    CHECK ((status = 'active') = (active_revision_hash IS NOT NULL)),
    FOREIGN KEY (record_uid, head_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (record_uid, active_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE INDEX karma_grant_principal_status
ON karma_grant(principal_person_uid, status, updated_at, record_uid);

CREATE INDEX karma_grant_revision_grant
ON karma_grant_revision(grant_uid, created_at, revision_hash);

CREATE TABLE karma_grant_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL CHECK (action IN ('create', 'narrow', 'activate', 'revoke')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    grant_uid                TEXT NOT NULL REFERENCES karma_grant(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT,
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL,
    FOREIGN KEY (grant_uid, revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
) STRICT;

CREATE TRIGGER karma_grant_revision_immutable_update
BEFORE UPDATE ON karma_grant_revision
BEGIN SELECT RAISE(ABORT, 'Karma Grant revisions are immutable'); END;

CREATE TRIGGER karma_grant_revision_immutable_delete
BEFORE DELETE ON karma_grant_revision
BEGIN SELECT RAISE(ABORT, 'Karma Grant revisions are immutable'); END;

CREATE TRIGGER karma_grant_request_immutable_update
BEFORE UPDATE ON karma_grant_request
BEGIN SELECT RAISE(ABORT, 'Karma Grant requests are immutable'); END;

CREATE TRIGGER karma_grant_request_immutable_delete
BEFORE DELETE ON karma_grant_request
BEGIN SELECT RAISE(ABORT, 'Karma Grant requests are immutable'); END;

CREATE TRIGGER karma_grant_scope_insert
BEFORE INSERT ON karma_grant
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.grant_uid = NEW.record_uid
      AND revision.principal_person_uid = NEW.principal_person_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'grant'
)
BEGIN SELECT RAISE(ABORT, 'Karma Grant revision scope is invalid'); END;

CREATE TRIGGER karma_grant_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash, principal_person_uid ON karma_grant
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.grant_uid = NEW.record_uid
      AND revision.principal_person_uid = NEW.principal_person_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_grant_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.grant_uid = NEW.record_uid
          AND revision.principal_person_uid = NEW.principal_person_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Grant active revision scope is invalid'); END;
