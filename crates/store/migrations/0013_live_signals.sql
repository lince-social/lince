CREATE TABLE signal_stream (
    signal_uid TEXT PRIMARY KEY REFERENCES record(uid),
    value_type TEXT NOT NULL CHECK (value_type IN ('scalar', 'jpeg')),
    source_url TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)
);
