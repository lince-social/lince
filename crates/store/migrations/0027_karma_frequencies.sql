-- Karma K2.2: mutable Frequency handles, immutable authored revisions, and
-- immutable effective activation epochs. No table in this migration is a
-- scheduler queue or cursor.

CREATE TABLE karma_frequency_revision (
    revision_hash         TEXT PRIMARY KEY
                          CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid         TEXT NOT NULL
                          REFERENCES karma_frequency(record_uid)
                          DEFERRABLE INITIALLY DEFERRED,
    schema_name           TEXT NOT NULL CHECK (schema_name = 'karma.frequency.v1'),
    ast_json              TEXT NOT NULL CHECK (json_valid(ast_json)),
    canonical_dsl         TEXT NOT NULL CHECK (length(canonical_dsl) > 0),
    default_compiled_json TEXT NOT NULL CHECK (json_valid(default_compiled_json)),
    created_at            TEXT NOT NULL,
    UNIQUE (frequency_uid, revision_hash)
) STRICT;

CREATE TABLE karma_frequency (
    record_uid             TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision        INTEGER NOT NULL CHECK (handle_revision >= 1),
    status                 TEXT NOT NULL
                           CHECK (status IN ('proven', 'active', 'paused', 'retired')),
    head_revision_hash     TEXT NOT NULL
                           REFERENCES karma_frequency_revision(revision_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    active_revision_hash   TEXT
                           REFERENCES karma_frequency_revision(revision_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    active_activation_hash TEXT
                           REFERENCES karma_frequency_activation(activation_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    latest_activation_hash TEXT
                           REFERENCES karma_frequency_activation(activation_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    owner_person_uid       TEXT,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    CHECK ((status = 'active') =
           (active_revision_hash IS NOT NULL AND active_activation_hash IS NOT NULL)),
    CHECK ((active_revision_hash IS NULL) = (active_activation_hash IS NULL)),
    CHECK (active_activation_hash IS NULL OR active_activation_hash = latest_activation_hash)
) STRICT;

CREATE INDEX karma_frequency_status
    ON karma_frequency(status, updated_at, record_uid);
CREATE INDEX karma_frequency_revision_frequency
    ON karma_frequency_revision(frequency_uid, created_at);

CREATE TABLE karma_frequency_activation (
    activation_hash             TEXT PRIMARY KEY
                                CHECK (length(activation_hash) = 71 AND activation_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid               TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    activating_handle_revision  INTEGER NOT NULL CHECK (activating_handle_revision >= 2),
    definition_revision_hash    TEXT NOT NULL REFERENCES karma_frequency_revision(revision_hash),
    effective_parameter_hash    TEXT NOT NULL
                                CHECK (length(effective_parameter_hash) = 71 AND effective_parameter_hash GLOB 'sha256:[0-9a-f]*'),
    effective_parameters_json   TEXT NOT NULL CHECK (json_valid(effective_parameters_json)),
    compiled_json               TEXT NOT NULL CHECK (json_valid(compiled_json)),
    epoch_json                  TEXT NOT NULL CHECK (json_valid(epoch_json)),
    previous_activation_hash    TEXT REFERENCES karma_frequency_activation(activation_hash),
    cause_action                TEXT NOT NULL
                                CHECK (cause_action IN ('activate-revision', 'set-parameters', 'reset-parameters')),
    activated_at                TEXT NOT NULL,
    UNIQUE (frequency_uid, activating_handle_revision)
) STRICT;

CREATE INDEX karma_frequency_activation_frequency
    ON karma_frequency_activation(frequency_uid, activating_handle_revision);
CREATE INDEX karma_frequency_activation_resolution
    ON karma_frequency_activation(frequency_uid, effective_parameter_hash);

CREATE TABLE karma_frequency_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL
                             CHECK (action IN ('create', 'revise', 'activate', 'set-parameters', 'reset-parameters', 'pause')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid            TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT REFERENCES karma_frequency_revision(revision_hash),
    activation_hash          TEXT REFERENCES karma_frequency_activation(activation_hash),
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL
) STRICT;

CREATE TRIGGER karma_frequency_revision_immutable_update
BEFORE UPDATE ON karma_frequency_revision
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revisions are immutable'); END;

CREATE TRIGGER karma_frequency_revision_immutable_delete
BEFORE DELETE ON karma_frequency_revision
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revisions are immutable'); END;

CREATE TRIGGER karma_frequency_activation_immutable_update
BEFORE UPDATE ON karma_frequency_activation
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activations are immutable'); END;

CREATE TRIGGER karma_frequency_activation_immutable_delete
BEFORE DELETE ON karma_frequency_activation
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activations are immutable'); END;

CREATE TRIGGER karma_frequency_request_immutable_update
BEFORE UPDATE ON karma_frequency_request
BEGIN SELECT RAISE(ABORT, 'Karma Frequency requests are immutable'); END;

CREATE TRIGGER karma_frequency_request_immutable_delete
BEFORE DELETE ON karma_frequency_request
BEGIN SELECT RAISE(ABORT, 'Karma Frequency requests are immutable'); END;

CREATE TRIGGER karma_frequency_revision_scope_insert
BEFORE INSERT ON karma_frequency
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.frequency_uid = NEW.record_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'frequency'
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency head revision scope is invalid'); END;

CREATE TRIGGER karma_frequency_activation_scope_insert
BEFORE INSERT ON karma_frequency_activation
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.definition_revision_hash
      AND revision.frequency_uid = NEW.frequency_uid
) OR (
    NEW.previous_activation_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.previous_activation_hash
          AND activation.frequency_uid = NEW.frequency_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activation scope is invalid'); END;

CREATE TRIGGER karma_frequency_revision_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash, active_activation_hash, latest_activation_hash
ON karma_frequency
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.frequency_uid = NEW.record_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.active_activation_hash
          AND activation.frequency_uid = NEW.record_uid
          AND activation.definition_revision_hash = NEW.active_revision_hash
    )
) OR (
    NEW.latest_activation_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.latest_activation_hash
          AND activation.frequency_uid = NEW.record_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revision or activation scope is invalid'); END;
