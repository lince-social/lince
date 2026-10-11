CREATE TABLE plugin_package (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1))
);

CREATE TABLE plugin_binding (
    uid TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    settings TEXT NOT NULL,
    protein_uid TEXT,
    incoming_filter TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('incoming', 'outgoing', 'both')),
    interval_seconds INTEGER NOT NULL CHECK (interval_seconds BETWEEN 30 AND 86400),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    cursor TEXT NOT NULL DEFAULT 'null',
    last_run INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'Stopped',
    revision INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE plugin_object (
    binding_uid TEXT NOT NULL REFERENCES plugin_binding(uid) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    record_uid TEXT REFERENCES record(uid) ON DELETE SET NULL,
    content_digest TEXT NOT NULL,
    PRIMARY KEY (binding_uid, external_id)
);

CREATE TABLE plugin_outbox (
    uid TEXT PRIMARY KEY,
    binding_uid TEXT NOT NULL REFERENCES plugin_binding(uid) ON DELETE CASCADE,
    record_uid TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    input TEXT NOT NULL,
    prepared TEXT,
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'delivered', 'failed', 'uncertain')),
    result TEXT NOT NULL DEFAULT '',
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    UNIQUE (binding_uid, record_uid, content_digest)
);

CREATE INDEX plugin_outbox_pending ON plugin_outbox(binding_uid, state, created_at);
