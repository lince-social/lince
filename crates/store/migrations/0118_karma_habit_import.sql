CREATE TABLE karma_habit_import (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    tutorial TEXT NOT NULL,
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
    PRIMARY KEY(organ_uid, tutorial)
) STRICT;

CREATE TABLE karma_habit_object (
    organ_uid TEXT NOT NULL,
    tutorial TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('record', 'frequency', 'rule')),
    uid TEXT NOT NULL UNIQUE,
    created INTEGER NOT NULL DEFAULT 0 CHECK(created IN (0, 1)),
    PRIMARY KEY(organ_uid, tutorial, kind),
    FOREIGN KEY(organ_uid, tutorial) REFERENCES karma_habit_import(organ_uid, tutorial) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER karma_habit_record_created AFTER INSERT ON record BEGIN
    UPDATE karma_habit_object SET created = 1 WHERE uid = new.uid AND kind IN ('record', 'frequency');
END;

CREATE TRIGGER karma_habit_rule_created AFTER INSERT ON recurrence BEGIN
    UPDATE karma_habit_object SET created = 1 WHERE uid = new.uid AND kind = 'rule';
END;

CREATE TABLE karma_habit_request (
    organ_uid TEXT NOT NULL,
    tutorial TEXT NOT NULL,
    request_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    result TEXT CHECK(result IS NULL OR json_valid(result)),
    PRIMARY KEY(organ_uid, request_id),
    FOREIGN KEY(organ_uid, tutorial) REFERENCES karma_habit_import(organ_uid, tutorial) ON DELETE CASCADE
) STRICT;
