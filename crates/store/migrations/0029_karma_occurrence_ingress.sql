-- Karma K3.1: one immutable semantic occurrence log with a Cell-local replay
-- order. Source identity is semantic and independent from arrival time.

CREATE TABLE karma_occurrence_sequence (
    singleton     INTEGER PRIMARY KEY CHECK (singleton = 1),
    next_sequence INTEGER NOT NULL CHECK (next_sequence >= 1)
) STRICT;

INSERT INTO karma_occurrence_sequence (singleton, next_sequence) VALUES (1, 1);

CREATE TABLE karma_occurrence (
    occurrence_hash       TEXT PRIMARY KEY
                          CHECK (length(occurrence_hash) = 71 AND occurrence_hash GLOB 'sha256:[0-9a-f]*'),
    cell_sequence         INTEGER NOT NULL UNIQUE CHECK (cell_sequence >= 1),
    source_kind           TEXT NOT NULL
                          CHECK (source_kind IN ('schedule-tick', 'schedule-coalesced',
                                                 'calendar-tick', 'calendar-coalesced')),
    source_identity       TEXT NOT NULL
                          CHECK (length(source_identity) = 71 AND source_identity GLOB 'sha256:[0-9a-f]*'),
    logical_at            TEXT NOT NULL,
    parent_occurrence_hash TEXT REFERENCES karma_occurrence(occurrence_hash),
    envelope_json         TEXT NOT NULL CHECK (json_valid(envelope_json)),
    received_at           TEXT NOT NULL,
    UNIQUE (source_kind, source_identity)
) STRICT;

CREATE INDEX karma_occurrence_replay_order
    ON karma_occurrence(cell_sequence, occurrence_hash);
CREATE INDEX karma_occurrence_logical_order
    ON karma_occurrence(logical_at, source_kind, source_identity);
CREATE INDEX karma_occurrence_parent
    ON karma_occurrence(parent_occurrence_hash, cell_sequence);

CREATE TABLE karma_schedule_occurrence_expansion (
    schedule_occurrence_hash TEXT PRIMARY KEY
                             REFERENCES karma_schedule_occurrence(occurrence_hash),
    cadence_kind          TEXT NOT NULL CHECK (cadence_kind IN ('elapsed', 'calendar')),
    emission_kind         TEXT NOT NULL CHECK (emission_kind IN ('individual', 'coalesced')),
    next_ordinal          INTEGER NOT NULL DEFAULT 0 CHECK (next_ordinal >= 0),
    total_items           INTEGER NOT NULL CHECK (total_items >= 1),
    completed             INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    CHECK (next_ordinal <= total_items),
    CHECK ((completed = 1) = (next_ordinal = total_items))
) STRICT;

CREATE INDEX karma_schedule_occurrence_expansion_pending
    ON karma_schedule_occurrence_expansion(completed, schedule_occurrence_hash);

CREATE TRIGGER karma_occurrence_immutable_update
BEFORE UPDATE ON karma_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma occurrences are immutable'); END;

CREATE TRIGGER karma_occurrence_immutable_delete
BEFORE DELETE ON karma_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma occurrences are immutable'); END;
