ALTER TABLE sync_history_policy ADD COLUMN enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1));

CREATE TABLE file_sync_baseline (
    organ_uid TEXT NOT NULL REFERENCES record(uid) ON DELETE CASCADE,
    directory TEXT NOT NULL,
    path TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    PRIMARY KEY(organ_uid, directory, path)
) STRICT;
