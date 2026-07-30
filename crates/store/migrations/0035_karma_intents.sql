-- Karma K5.2: durable authorized intents and the budget they reserve.
-- Nothing here executes: there is no lease, attempt, receipt, or worker table,
-- and the only reachable states are 'authorized' and 'cancelled'.

CREATE TABLE karma_intent (
    intent_hash           TEXT PRIMARY KEY
                          CHECK (length(intent_hash) = 71 AND intent_hash GLOB 'sha256:[0-9a-f]*'),
    -- One accepted candidate can only ever become one intent.
    candidate_hash        TEXT NOT NULL UNIQUE REFERENCES karma_candidate(candidate_hash),
    grant_uid             TEXT NOT NULL REFERENCES karma_grant(record_uid),
    grant_revision_hash   TEXT NOT NULL,
    grant_handle_revision INTEGER NOT NULL CHECK (grant_handle_revision >= 1),
    program_uid           TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    template              TEXT NOT NULL,
    capability            TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL UNIQUE
                          CHECK (length(idempotency_key) BETWEEN 1 AND 200),
    -- The reservation coordinates, kept as columns so budget accounting is a
    -- query over evidence rather than a counter that can drift.
    window_index          INTEGER CHECK (window_index IS NULL OR window_index >= 0),
    quantity_unit_uid     TEXT,
    quantity_scale        INTEGER CHECK (quantity_scale IS NULL OR quantity_scale BETWEEN 0 AND 9),
    quantity_mantissa     TEXT,
    deadline              TEXT NOT NULL,
    intent_json           TEXT NOT NULL CHECK (json_valid(intent_json)),
    created_at            TEXT NOT NULL,
    CHECK ((quantity_unit_uid IS NULL) = (quantity_mantissa IS NULL)
       AND (quantity_unit_uid IS NULL) = (quantity_scale IS NULL)),
    FOREIGN KEY (grant_uid, grant_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
) STRICT;

-- Which states an intent may hold, and whether holding one still reserves the
-- grant's budget. This is the single place that rule is stated in SQL: every
-- budget query joins here rather than naming statuses, so adding an executing
-- state in K5.3 is a seeded row, not an edit to five WHERE clauses that would
-- otherwise silently under-count and make a reservation double-spendable. The
-- rows seeded here are exactly the states K5.2 can reach, so a status this
-- phase must not produce cannot be written at all.
CREATE TABLE karma_intent_status (
    status            TEXT PRIMARY KEY,
    holds_reservation INTEGER NOT NULL CHECK (holds_reservation IN (0, 1))
) STRICT;

INSERT INTO karma_intent_status (status, holds_reservation) VALUES
    ('authorized', 1),
    ('cancelled', 0);

-- Immutable transition history, chained per intent, matching the Program-state
-- and candidate-review shape: one event log plus one current projection.
CREATE TABLE karma_intent_event (
    event_hash          TEXT PRIMARY KEY
                        CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    intent_hash         TEXT NOT NULL REFERENCES karma_intent(intent_hash),
    state_revision      INTEGER NOT NULL CHECK (state_revision >= 1),
    previous_event_hash TEXT REFERENCES karma_intent_event(event_hash),
    status              TEXT NOT NULL REFERENCES karma_intent_status(status),
    reason              TEXT,
    -- Deliberately not unique: one revocation cancels every intent its grant
    -- authorized, so a single cause legitimately owns many transitions. The
    -- Fact for that cause is reachable through the request, so it is not copied.
    cause_request_id    TEXT NOT NULL REFERENCES karma_request(request_id),
    actor_person_uid    TEXT NOT NULL,
    event_json          TEXT NOT NULL CHECK (json_valid(event_json)),
    created_at          TEXT NOT NULL,
    UNIQUE (intent_hash, state_revision),
    CHECK ((state_revision = 1) = (previous_event_hash IS NULL)),
    CHECK (state_revision > 1 OR reason IS NULL)
) STRICT;

CREATE TABLE karma_intent_state (
    intent_hash        TEXT PRIMARY KEY REFERENCES karma_intent(intent_hash),
    state_revision     INTEGER NOT NULL CHECK (state_revision >= 1),
    status             TEXT NOT NULL REFERENCES karma_intent_status(status),
    current_event_hash TEXT NOT NULL UNIQUE REFERENCES karma_intent_event(event_hash),
    cancelled_reason   TEXT,
    actor_person_uid   TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    CHECK ((status = 'cancelled') = (cancelled_reason IS NOT NULL))
) STRICT;

CREATE INDEX karma_intent_grant_window
    ON karma_intent(grant_uid, window_index, intent_hash);
CREATE INDEX karma_intent_candidate
    ON karma_intent(candidate_hash, intent_hash);
CREATE INDEX karma_intent_state_status
    ON karma_intent_state(status, intent_hash);
CREATE INDEX karma_intent_event_history
    ON karma_intent_event(intent_hash, state_revision);

CREATE TRIGGER karma_intent_immutable_update
BEFORE UPDATE ON karma_intent
BEGIN SELECT RAISE(ABORT, 'Karma intents are immutable'); END;

CREATE TRIGGER karma_intent_immutable_delete
BEFORE DELETE ON karma_intent
BEGIN SELECT RAISE(ABORT, 'Karma intents are immutable'); END;

-- An intent may only exist for a candidate that was actually accepted, and only
-- for an `act` route. Nothing else can be turned into work.
CREATE TRIGGER karma_intent_requires_accepted_act_candidate
BEFORE INSERT ON karma_intent
WHEN NOT EXISTS (
    SELECT 1 FROM karma_candidate candidate
    JOIN karma_candidate_state state ON state.candidate_hash = candidate.candidate_hash
    WHERE candidate.candidate_hash = NEW.candidate_hash
      AND candidate.route = 'act'
      AND state.status = 'accepted'
      AND candidate.program_uid = NEW.program_uid
      AND candidate.program_revision_hash = NEW.program_revision_hash
)
BEGIN SELECT RAISE(ABORT, 'a Karma intent requires an accepted act candidate'); END;

-- The authorizing grant must be active at the moment the intent is created.
CREATE TRIGGER karma_intent_requires_active_grant
BEFORE INSERT ON karma_intent
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant grant_row
    WHERE grant_row.record_uid = NEW.grant_uid
      AND grant_row.status = 'active'
      AND grant_row.active_revision_hash = NEW.grant_revision_hash
      AND grant_row.handle_revision = NEW.grant_handle_revision
)
BEGIN SELECT RAISE(ABORT, 'a Karma intent requires its authorizing grant to be active'); END;

-- Authorized is the only state an intent may enter, and cancellation is final.
CREATE TRIGGER karma_intent_state_starts_authorized
BEFORE INSERT ON karma_intent_state
WHEN NEW.status <> 'authorized' OR NEW.state_revision <> 1
BEGIN SELECT RAISE(ABORT, 'a Karma intent begins authorized at revision 1'); END;

CREATE TRIGGER karma_intent_state_is_one_way
BEFORE UPDATE ON karma_intent_state
WHEN OLD.status <> 'authorized'
  OR NEW.status <> 'cancelled'
  OR NEW.state_revision <> OLD.state_revision + 1
BEGIN SELECT RAISE(ABORT, 'a Karma intent may only move from authorized to cancelled'); END;

CREATE TRIGGER karma_intent_state_immutable_delete
BEFORE DELETE ON karma_intent_state
BEGIN SELECT RAISE(ABORT, 'Karma intent state is append-only'); END;

-- The projection may never drift from the history it summarizes.
CREATE TRIGGER karma_intent_state_matches_event_insert
BEFORE INSERT ON karma_intent_state
WHEN NOT EXISTS (
    SELECT 1 FROM karma_intent_event event
    WHERE event.event_hash = NEW.current_event_hash
      AND event.intent_hash = NEW.intent_hash
      AND event.state_revision = NEW.state_revision
      AND event.status = NEW.status
)
BEGIN SELECT RAISE(ABORT, 'Karma intent state must match its current event'); END;

CREATE TRIGGER karma_intent_state_matches_event_update
BEFORE UPDATE ON karma_intent_state
WHEN NOT EXISTS (
    SELECT 1 FROM karma_intent_event event
    WHERE event.event_hash = NEW.current_event_hash
      AND event.intent_hash = NEW.intent_hash
      AND event.state_revision = NEW.state_revision
      AND event.status = NEW.status
      AND event.previous_event_hash = OLD.current_event_hash
)
BEGIN SELECT RAISE(ABORT, 'Karma intent state must match its current event'); END;

CREATE TRIGGER karma_intent_event_immutable_update
BEFORE UPDATE ON karma_intent_event
BEGIN SELECT RAISE(ABORT, 'Karma intent events are immutable'); END;

CREATE TRIGGER karma_intent_event_immutable_delete
BEFORE DELETE ON karma_intent_event
BEGIN SELECT RAISE(ABORT, 'Karma intent events are immutable'); END;

-- Seeded vocabulary, not data: a phase adds a status in its own migration.
CREATE TRIGGER karma_intent_status_frozen_update
BEFORE UPDATE ON karma_intent_status
BEGIN SELECT RAISE(ABORT, 'Karma intent statuses are frozen'); END;

CREATE TRIGGER karma_intent_status_frozen_delete
BEFORE DELETE ON karma_intent_status
BEGIN SELECT RAISE(ABORT, 'Karma intent statuses are frozen'); END;
