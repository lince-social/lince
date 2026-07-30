-- Karma K2.1: mutable Program handles, immutable content-addressed revisions,
-- and idempotent mutation evidence. Revisions and handles form a deliberately
-- deferred cycle so both become visible atomically in one transaction.

CREATE TABLE karma_request (
    request_id   TEXT PRIMARY KEY CHECK (length(request_id) BETWEEN 1 AND 200),
    family       TEXT NOT NULL CHECK (length(family) BETWEEN 1 AND 64),
    payload_hash TEXT NOT NULL
                 CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    created_at   TEXT NOT NULL
) STRICT;

CREATE TRIGGER karma_request_immutable_update
BEFORE UPDATE ON karma_request
BEGIN SELECT RAISE(ABORT, 'Karma request identities are immutable'); END;

CREATE TRIGGER karma_request_immutable_delete
BEFORE DELETE ON karma_request
BEGIN SELECT RAISE(ABORT, 'Karma request identities are immutable'); END;

CREATE TABLE karma_program_revision (
    revision_hash TEXT PRIMARY KEY
                  CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid   TEXT NOT NULL
                  REFERENCES karma_program(record_uid)
                  DEFERRABLE INITIALLY DEFERRED,
    schema_name   TEXT NOT NULL CHECK (schema_name = 'karma.program.v1'),
    ast_json      TEXT NOT NULL CHECK (json_valid(ast_json)),
    canonical_dsl TEXT NOT NULL CHECK (length(canonical_dsl) > 0),
    proof_json    TEXT NOT NULL CHECK (json_valid(proof_json)),
    proof_status  TEXT NOT NULL CHECK (proof_status IN ('accepted', 'rejected')),
    created_at    TEXT NOT NULL,
    UNIQUE (program_uid, revision_hash)
) STRICT;

CREATE TABLE karma_program (
    record_uid           TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision      INTEGER NOT NULL CHECK (handle_revision >= 1),
    status               TEXT NOT NULL
                         CHECK (status IN ('draft', 'proven', 'active', 'paused', 'retired')),
    head_revision_hash   TEXT NOT NULL
                         REFERENCES karma_program_revision(revision_hash)
                         DEFERRABLE INITIALLY DEFERRED,
    active_revision_hash TEXT
                         REFERENCES karma_program_revision(revision_hash)
                         DEFERRABLE INITIALLY DEFERRED,
    owner_person_uid     TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    CHECK ((status = 'active') = (active_revision_hash IS NOT NULL))
) STRICT;

CREATE INDEX karma_program_status ON karma_program(status, updated_at, record_uid);
CREATE INDEX karma_program_revision_program ON karma_program_revision(program_uid, created_at);

CREATE TABLE karma_program_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL
                             CHECK (action IN ('create', 'revise', 'activate', 'pause')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid              TEXT NOT NULL REFERENCES karma_program(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT REFERENCES karma_program_revision(revision_hash),
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL
) STRICT;

CREATE TRIGGER karma_program_revision_immutable_update
BEFORE UPDATE ON karma_program_revision
BEGIN SELECT RAISE(ABORT, 'Karma Program revisions are immutable'); END;

CREATE TRIGGER karma_program_revision_immutable_delete
BEFORE DELETE ON karma_program_revision
BEGIN SELECT RAISE(ABORT, 'Karma Program revisions are immutable'); END;

CREATE TRIGGER karma_program_request_immutable_update
BEFORE UPDATE ON karma_program_request
BEGIN SELECT RAISE(ABORT, 'Karma Program requests are immutable'); END;

CREATE TRIGGER karma_program_request_immutable_delete
BEFORE DELETE ON karma_program_request
BEGIN SELECT RAISE(ABORT, 'Karma Program requests are immutable'); END;

CREATE TRIGGER karma_program_revision_scope_insert
BEFORE INSERT ON karma_program
WHEN NOT EXISTS (
    SELECT 1 FROM karma_program_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.program_uid = NEW.record_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'program'
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_program_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.program_uid = NEW.record_uid
          AND revision.proof_status = 'accepted'
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Program head revision belongs to another Program'); END;

CREATE TRIGGER karma_program_revision_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash ON karma_program
WHEN NOT EXISTS (
    SELECT 1 FROM karma_program_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.program_uid = NEW.record_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_program_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.program_uid = NEW.record_uid
          AND revision.proof_status = 'accepted'
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Program revision scope or Proof is invalid'); END;
