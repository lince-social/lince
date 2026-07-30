-- Karma K3.3: freeze the active Program revision set per occurrence before
-- evaluation and advance one durable Cell-order processing cursor.

CREATE TABLE karma_occurrence_processing_state (
    singleton          INTEGER PRIMARY KEY CHECK (singleton = 1),
    next_cell_sequence INTEGER NOT NULL CHECK (next_cell_sequence >= 1)
) STRICT;

INSERT INTO karma_occurrence_processing_state (singleton, next_cell_sequence)
VALUES (1, 1);

CREATE TABLE karma_occurrence_program_epoch (
    occurrence_hash    TEXT PRIMARY KEY REFERENCES karma_occurrence(occurrence_hash),
    cell_sequence      INTEGER NOT NULL UNIQUE CHECK (cell_sequence >= 1),
    epoch_hash         TEXT NOT NULL UNIQUE
                       CHECK (length(epoch_hash) = 71 AND epoch_hash GLOB 'sha256:[0-9a-f]*'),
    epoch_json         TEXT NOT NULL CHECK (json_valid(epoch_json)),
    member_count       INTEGER NOT NULL CHECK (member_count >= 0),
    next_member_ordinal INTEGER NOT NULL DEFAULT 0 CHECK (next_member_ordinal >= 0),
    completed          INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    completed_at       TEXT,
    CHECK (next_member_ordinal <= member_count),
    CHECK ((completed = 1) = (next_member_ordinal = member_count)),
    CHECK ((completed = 1) = (completed_at IS NOT NULL))
) STRICT;

CREATE INDEX karma_occurrence_program_epoch_pending
    ON karma_occurrence_program_epoch(completed, cell_sequence);

CREATE TABLE karma_run (
    run_hash             TEXT PRIMARY KEY
                         CHECK (length(run_hash) = 71 AND run_hash GLOB 'sha256:[0-9a-f]*'),
    occurrence_hash      TEXT NOT NULL REFERENCES karma_occurrence(occurrence_hash),
    cell_sequence        INTEGER NOT NULL CHECK (cell_sequence >= 1),
    program_epoch_hash   TEXT NOT NULL REFERENCES karma_occurrence_program_epoch(epoch_hash),
    member_ordinal       INTEGER NOT NULL CHECK (member_ordinal >= 0),
    program_uid          TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    status               TEXT NOT NULL
                         CHECK (status IN ('succeeded', 'not-applicable', 'blocked',
                                           'evaluation-failed')),
    fuel_used            INTEGER NOT NULL CHECK (fuel_used >= 0),
    run_json             TEXT NOT NULL CHECK (json_valid(run_json)),
    created_at           TEXT NOT NULL,
    UNIQUE (occurrence_hash, program_revision_hash),
    UNIQUE (occurrence_hash, member_ordinal)
) STRICT;

CREATE INDEX karma_run_replay_order
    ON karma_run(cell_sequence, member_ordinal, run_hash);
CREATE INDEX karma_run_program_order
    ON karma_run(program_uid, cell_sequence, member_ordinal);

CREATE TRIGGER karma_occurrence_program_epoch_frozen
BEFORE UPDATE OF occurrence_hash, cell_sequence, epoch_hash, epoch_json, member_count
ON karma_occurrence_program_epoch
BEGIN SELECT RAISE(ABORT, 'Karma Program epoch identity is immutable'); END;

CREATE TRIGGER karma_occurrence_program_epoch_no_delete
BEFORE DELETE ON karma_occurrence_program_epoch
BEGIN SELECT RAISE(ABORT, 'Karma Program epochs are immutable'); END;

CREATE TRIGGER karma_run_immutable_update
BEFORE UPDATE ON karma_run
BEGIN SELECT RAISE(ABORT, 'Karma runs are immutable'); END;

CREATE TRIGGER karma_run_immutable_delete
BEFORE DELETE ON karma_run
BEGIN SELECT RAISE(ABORT, 'Karma runs are immutable'); END;
