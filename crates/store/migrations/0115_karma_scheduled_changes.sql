ALTER TABLE karma_rule_application RENAME TO karma_rule_application_previous;
CREATE TABLE karma_rule_application (
    event_id TEXT NOT NULL,
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    status TEXT NOT NULL,
    reason TEXT,
    at TEXT NOT NULL,
    intended_at TEXT NOT NULL,
    frequency_uid TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(event_id, rule_uid, rule_revision, attempt)
) STRICT;
INSERT INTO karma_rule_application(event_id, rule_uid, rule_revision, status, reason, at, intended_at, frequency_uid)
SELECT event_id, rule_uid, rule_revision, status, reason, at, intended_at, frequency_uid FROM karma_rule_application_previous;
DROP TABLE karma_rule_application_previous;
CREATE TABLE karma_schedule (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0, 1)),
    actor_uid TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;
CREATE TABLE karma_schedule_revision (
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    revision INTEGER NOT NULL,
    inputs TEXT NOT NULL CHECK(json_valid(inputs)),
    at TEXT NOT NULL,
    PRIMARY KEY(schedule_uid, revision)
) STRICT;
CREATE TABLE karma_schedule_boundary (
    uid TEXT PRIMARY KEY,
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    revision INTEGER NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose IN ('once', 'start', 'end')),
    input TEXT NOT NULL CHECK(json_valid(input)),
    intended_at_ms INTEGER NOT NULL,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    rule_uid TEXT NOT NULL REFERENCES recurrence(uid),
    current INTEGER NOT NULL CHECK(current IN (0, 1)),
    status TEXT NOT NULL,
    event_id TEXT,
    rule_revision INTEGER,
    attempt INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    completed_at TEXT
) STRICT;
CREATE UNIQUE INDEX karma_schedule_current_boundary ON karma_schedule_boundary(schedule_uid, purpose) WHERE current = 1;
CREATE INDEX karma_schedule_boundary_rule ON karma_schedule_boundary(rule_uid);
CREATE TABLE karma_schedule_request (
    request_id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    result TEXT NOT NULL CHECK(json_valid(result))
) STRICT;
CREATE TABLE karma_effect_outcome (
    effect_uid TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL,
    result TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY(effect_uid, attempt)
) STRICT;
