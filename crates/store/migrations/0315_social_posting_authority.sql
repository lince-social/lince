ALTER TABLE social_document ADD COLUMN generation INTEGER NOT NULL DEFAULT 1;
ALTER TABLE social_ended_post ADD COLUMN generation INTEGER NOT NULL DEFAULT 1;
CREATE TABLE social_posting_authority (
    owner TEXT PRIMARY KEY,
    editor TEXT NOT NULL,
    generation INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    body TEXT NOT NULL
);
