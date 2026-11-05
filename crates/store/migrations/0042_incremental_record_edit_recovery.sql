ALTER TABLE record_edit_draft RENAME TO record_edit_draft_snapshot;

CREATE TABLE record_edit_draft (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    generation TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    checkpoint TEXT NOT NULL CHECK (length(CAST(checkpoint AS BLOB)) <= 25165824),
    pending TEXT NOT NULL CHECK (json_valid(pending) AND json_type(pending) = 'array'),
    update_count INTEGER NOT NULL DEFAULT 0 CHECK (update_count >= 0),
    update_bytes INTEGER NOT NULL DEFAULT 0 CHECK (update_bytes >= 0),
    request_bytes INTEGER NOT NULL DEFAULT 0 CHECK (request_bytes >= 0),
    write_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source, record_uid)
) STRICT;

CREATE TABLE record_edit_request (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    request_uid TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_type(payload) = 'object'),
    PRIMARY KEY (source, record_uid, request_uid),
    FOREIGN KEY (source, record_uid) REFERENCES record_edit_draft(source, record_uid) ON DELETE CASCADE
) STRICT;

CREATE TABLE record_edit_update (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    payload BLOB NOT NULL,
    PRIMARY KEY (source, record_uid, sequence, position),
    FOREIGN KEY (source, record_uid) REFERENCES record_edit_draft(source, record_uid) ON DELETE CASCADE
) STRICT;

INSERT INTO record_edit_draft (
    source, record_uid, generation, sequence, checkpoint, pending, request_bytes, write_hash, updated_at
)
SELECT d.source, d.record_uid, lower(hex(randomblob(16))), 0,
    json_extract(d.state, '$.snapshot'),
    (
        SELECT json_group_array(request_uid)
        FROM (
            SELECT json_extract(value, '$.id') AS request_uid
            FROM json_each(d.state, '$.pending')
            ORDER BY CAST(key AS INTEGER)
        )
    ),
    (
        SELECT COALESCE(SUM(length(CAST(value AS BLOB))), 0)
        FROM json_each(d.state, '$.pending')
    ),
    '', d.updated_at
FROM record_edit_draft_snapshot d;

INSERT INTO record_edit_request (source, record_uid, request_uid, payload)
SELECT d.source, d.record_uid, json_extract(p.value, '$.id'), p.value
FROM record_edit_draft_snapshot d, json_each(d.state, '$.pending') p;

DROP TABLE record_edit_draft_snapshot;
