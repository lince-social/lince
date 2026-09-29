CREATE TABLE projection_source (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    runtime TEXT
);
INSERT INTO projection_source VALUES (1, 0, NULL);
CREATE TABLE projection_window (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    cache_key TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    base_ms INTEGER NOT NULL,
    expires_ms INTEGER NOT NULL,
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL,
    incomplete TEXT CHECK (incomplete IS NULL OR json_valid(incomplete))
);
CREATE TABLE projection_span (
    id TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL,
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload))
);
CREATE INDEX projection_span_window ON projection_span(from_ms, until_ms);
CREATE INDEX projection_span_record ON projection_span(record_uid, from_ms, until_ms);
