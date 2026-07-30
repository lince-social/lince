-- Karma K4.2: inert proposals materialized atomically from successful run
-- traces. Lifecycle mutation and authority are deliberately not present yet.

CREATE TABLE karma_candidate (
    candidate_hash        TEXT PRIMARY KEY
                          CHECK (length(candidate_hash) = 71 AND candidate_hash GLOB 'sha256:[0-9a-f]*'),
    source_run_hash       TEXT NOT NULL REFERENCES karma_run(run_hash),
    occurrence_hash       TEXT NOT NULL REFERENCES karma_occurrence(occurrence_hash),
    program_uid           TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    node_id               TEXT NOT NULL,
    output_port           TEXT NOT NULL,
    route                 TEXT NOT NULL CHECK (route IN ('observe', 'recommend', 'draft', 'ask', 'act')),
    template              TEXT NOT NULL,
    status                TEXT NOT NULL CHECK (status = 'proposed'),
    proposal_json         TEXT NOT NULL CHECK (json_valid(proposal_json)),
    created_at            TEXT NOT NULL,
    UNIQUE (source_run_hash, node_id, output_port)
) STRICT;

CREATE INDEX karma_candidate_program_order
    ON karma_candidate(program_uid, created_at, candidate_hash);
CREATE INDEX karma_candidate_occurrence_order
    ON karma_candidate(occurrence_hash, candidate_hash);

CREATE TRIGGER karma_candidate_immutable_update
BEFORE UPDATE ON karma_candidate
BEGIN SELECT RAISE(ABORT, 'Karma candidate proposals are immutable in K4.2'); END;

CREATE TRIGGER karma_candidate_immutable_delete
BEFORE DELETE ON karma_candidate
BEGIN SELECT RAISE(ABORT, 'Karma candidate proposals are immutable in K4.2'); END;
