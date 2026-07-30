-- Karma K4.3: reversible, event-sourced candidate review. Acceptance remains
-- inert; no intent/effect foreign key exists in this phase.

CREATE TABLE karma_candidate_state (
    candidate_hash     TEXT PRIMARY KEY REFERENCES karma_candidate(candidate_hash),
    state_revision     INTEGER NOT NULL CHECK (state_revision >= 1),
    status             TEXT NOT NULL CHECK (status IN ('proposed', 'accepted', 'dismissed', 'snoozed')),
    snoozed_until      TEXT,
    current_event_hash TEXT,
    actor_person_uid   TEXT,
    updated_at         TEXT NOT NULL,
    CHECK ((status = 'snoozed') = (snoozed_until IS NOT NULL)),
    CHECK ((state_revision = 1) = (current_event_hash IS NULL))
) STRICT;

INSERT INTO karma_candidate_state
    (candidate_hash, state_revision, status, snoozed_until,
     current_event_hash, actor_person_uid, updated_at)
SELECT candidate_hash, 1, 'proposed', NULL, NULL, NULL, created_at
FROM karma_candidate;

CREATE TABLE karma_candidate_review_event (
    event_hash          TEXT PRIMARY KEY
                        CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    candidate_hash      TEXT NOT NULL REFERENCES karma_candidate(candidate_hash),
    state_revision      INTEGER NOT NULL CHECK (state_revision >= 2),
    previous_event_hash TEXT REFERENCES karma_candidate_review_event(event_hash),
    request_id          TEXT NOT NULL UNIQUE REFERENCES karma_request(request_id),
    action              TEXT NOT NULL CHECK (action IN ('accept', 'dismiss', 'snooze')),
    status              TEXT NOT NULL CHECK (status IN ('accepted', 'dismissed', 'snoozed')),
    snoozed_until       TEXT,
    actor_person_uid    TEXT,
    evidence_json       TEXT NOT NULL CHECK (json_valid(evidence_json)),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (candidate_hash, state_revision),
    CHECK ((state_revision = 2) = (previous_event_hash IS NULL)),
    CHECK ((status = 'snoozed') = (snoozed_until IS NOT NULL))
) STRICT;

CREATE TABLE karma_candidate_review_request (
    request_id              TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    payload_hash            TEXT NOT NULL,
    candidate_hash          TEXT NOT NULL REFERENCES karma_candidate(candidate_hash),
    expected_state_revision INTEGER NOT NULL,
    result_state_revision   INTEGER NOT NULL,
    result_json             TEXT NOT NULL CHECK (json_valid(result_json)),
    event_hash              TEXT NOT NULL REFERENCES karma_candidate_review_event(event_hash),
    fact_uid                TEXT NOT NULL REFERENCES fact(uid),
    created_at              TEXT NOT NULL
) STRICT;

CREATE TRIGGER karma_candidate_review_event_immutable_update
BEFORE UPDATE ON karma_candidate_review_event
BEGIN SELECT RAISE(ABORT, 'Karma candidate review events are immutable'); END;

CREATE TRIGGER karma_candidate_review_event_immutable_delete
BEFORE DELETE ON karma_candidate_review_event
BEGIN SELECT RAISE(ABORT, 'Karma candidate review events are immutable'); END;

CREATE TRIGGER karma_candidate_review_request_immutable_update
BEFORE UPDATE ON karma_candidate_review_request
BEGIN SELECT RAISE(ABORT, 'Karma candidate review requests are immutable'); END;

CREATE TRIGGER karma_candidate_review_request_immutable_delete
BEFORE DELETE ON karma_candidate_review_request
BEGIN SELECT RAISE(ABORT, 'Karma candidate review requests are immutable'); END;
