CREATE TABLE social_report_work (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    service TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=16384),
    hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','refused','expired')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
CREATE INDEX social_report_work_actor ON social_report_work(actor,created_at);
CREATE INDEX social_report_work_due ON social_report_work(state,next_attempt);
CREATE TABLE social_received_report (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=16384),
    received_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE social_report_admission (
    source TEXT PRIMARY KEY,
    day INTEGER NOT NULL,
    used INTEGER NOT NULL
);
CREATE TABLE social_report_seen (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
