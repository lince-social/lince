CREATE TABLE social_context_retention (
    context TEXT PRIMARY KEY REFERENCES record(uid),
    state TEXT NOT NULL CHECK(state IN ('active','dormant','retired','review')),
    retire_after INTEGER NOT NULL DEFAULT 0,
    checked_at INTEGER NOT NULL,
    queue_checked_at INTEGER NOT NULL DEFAULT 0,
    error TEXT CHECK(error IS NULL OR length(CAST(error AS BLOB))<=2048)
);
CREATE INDEX social_context_retention_check ON social_context_retention(checked_at,context);
