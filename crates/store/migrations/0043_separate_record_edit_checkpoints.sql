CREATE TABLE record_edit_checkpoint (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 25165824),
    PRIMARY KEY (source, record_uid),
    FOREIGN KEY (source, record_uid) REFERENCES record_edit_draft(source, record_uid) ON DELETE CASCADE
) STRICT;

INSERT INTO record_edit_checkpoint (source, record_uid, payload)
SELECT source, record_uid, checkpoint FROM record_edit_draft;

ALTER TABLE record_edit_draft DROP COLUMN checkpoint;
