CREATE TABLE fiote_activation (
    request_id TEXT PRIMARY KEY,
    fiote_uid TEXT NOT NULL REFERENCES record(uid),
    actor_uid TEXT,
    value TEXT NOT NULL,
    cause_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued', 'waiting', 'running', 'finished', 'interrupted', 'cancelled', 'refused')),
    thread_uid TEXT REFERENCES record(uid),
    detail TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE INDEX fiote_activation_pending ON fiote_activation(fiote_uid, state, created_at);
