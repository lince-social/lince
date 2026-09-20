CREATE TABLE karma_rule_frequency (
    recurrence_uid TEXT PRIMARY KEY REFERENCES recurrence(uid) ON DELETE CASCADE,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    rule_revision INTEGER NOT NULL
) STRICT;

CREATE TABLE karma_signal_frequency (
    signal_uid TEXT PRIMARY KEY REFERENCES signal(record_uid) ON DELETE CASCADE,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid)
) STRICT;

CREATE TABLE karma_rule_application (
    event_id TEXT NOT NULL,
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    status TEXT NOT NULL,
    reason TEXT,
    at TEXT NOT NULL,
    intended_at TEXT NOT NULL,
    frequency_uid TEXT,
    PRIMARY KEY(event_id, rule_uid, rule_revision)
) STRICT;

CREATE TABLE karma_rule_progress (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    next_sequence INTEGER NOT NULL
) STRICT;

INSERT INTO karma_rule_progress VALUES (1, 1);

CREATE TABLE karma_frequency_usage (
    frequency_uid TEXT PRIMARY KEY REFERENCES karma_frequency(record_uid),
    enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
    auto_paused INTEGER NOT NULL DEFAULT 0 CHECK(auto_paused IN (0, 1))
) STRICT;

ALTER TABLE effect_queue ADD COLUMN request_id TEXT;
CREATE UNIQUE INDEX effect_queue_request ON effect_queue(request_id) WHERE request_id IS NOT NULL;

ALTER TABLE signal ADD COLUMN actor_uid TEXT;
