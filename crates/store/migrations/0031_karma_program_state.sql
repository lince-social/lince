-- Karma K4.1: immutable Program-node state history plus one validated current
-- projection. State changes commit in the same transaction as their source run.

CREATE TABLE karma_program_state_event (
    event_hash                 TEXT PRIMARY KEY
                               CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid                TEXT NOT NULL REFERENCES karma_program(record_uid),
    node_id                    TEXT NOT NULL,
    state_revision             INTEGER NOT NULL CHECK (state_revision >= 1),
    previous_event_hash        TEXT REFERENCES karma_program_state_event(event_hash),
    source_run_hash            TEXT NOT NULL REFERENCES karma_run(run_hash),
    definition_revision_hash   TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    activation_handle_revision INTEGER NOT NULL CHECK (activation_handle_revision >= 1),
    reset_reason               TEXT
                               CHECK (reset_reason IS NULL OR reset_reason IN
                                      ('program-activation', 'revision-change', 'migration-reset')),
    state_kind                 TEXT NOT NULL CHECK (state_kind IN ('delay', 'control', 'reset')),
    state_json                 TEXT CHECK (state_json IS NULL OR json_valid(state_json)),
    event_json                 TEXT NOT NULL CHECK (json_valid(event_json)),
    created_at                 TEXT NOT NULL,
    UNIQUE (program_uid, node_id, state_revision),
    UNIQUE (source_run_hash, node_id),
    CHECK ((state_revision = 1) = (previous_event_hash IS NULL)),
    CHECK ((state_kind = 'reset') = (state_json IS NULL)),
    CHECK (state_kind != 'reset' OR reset_reason IS NOT NULL)
) STRICT;

CREATE INDEX karma_program_state_event_history
    ON karma_program_state_event(program_uid, node_id, state_revision);

CREATE TABLE karma_program_node_state (
    program_uid                TEXT NOT NULL REFERENCES karma_program(record_uid),
    node_id                    TEXT NOT NULL,
    state_revision             INTEGER NOT NULL CHECK (state_revision >= 1),
    current_event_hash         TEXT NOT NULL UNIQUE REFERENCES karma_program_state_event(event_hash),
    definition_revision_hash   TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    activation_handle_revision INTEGER NOT NULL CHECK (activation_handle_revision >= 1),
    state_kind                 TEXT NOT NULL CHECK (state_kind IN ('delay', 'control', 'reset')),
    state_json                 TEXT CHECK (state_json IS NULL OR json_valid(state_json)),
    updated_at                 TEXT NOT NULL,
    PRIMARY KEY (program_uid, node_id),
    CHECK ((state_kind = 'reset') = (state_json IS NULL))
) STRICT;

CREATE TRIGGER karma_program_state_event_immutable_update
BEFORE UPDATE ON karma_program_state_event
BEGIN SELECT RAISE(ABORT, 'Karma Program state events are immutable'); END;

CREATE TRIGGER karma_program_state_event_immutable_delete
BEFORE DELETE ON karma_program_state_event
BEGIN SELECT RAISE(ABORT, 'Karma Program state events are immutable'); END;
