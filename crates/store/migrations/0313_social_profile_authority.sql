CREATE TABLE social_profile_authority (
    organ TEXT PRIMARY KEY,
    root_key TEXT NOT NULL,
    editor_key TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation>0),
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=8192),
    expires_at INTEGER NOT NULL,
    profile_revision INTEGER NOT NULL DEFAULT 0,
    profile_hash TEXT
);
