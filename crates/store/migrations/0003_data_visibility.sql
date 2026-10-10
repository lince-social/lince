CREATE TABLE data_visibility (
    record_uid TEXT NOT NULL REFERENCES record(uid) ON DELETE CASCADE,
    data TEXT NOT NULL CHECK(data IN ('record','place','live_location')),
    controller_uid TEXT REFERENCES record(uid) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision > 0),
    policy_json TEXT NOT NULL CHECK(json_valid(policy_json)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY(record_uid,data)
);
