ALTER TABLE record ADD COLUMN file_origin TEXT;

CREATE INDEX idx_record_file_origin ON record(file_origin)
    WHERE deleted_at IS NULL;

CREATE TABLE directory_sync (
    uid TEXT PRIMARY KEY,
    settings TEXT NOT NULL CHECK (json_valid(settings)),
    removed INTEGER NOT NULL DEFAULT 0 CHECK (removed IN (0, 1))
) STRICT;
