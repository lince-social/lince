CREATE TABLE mailbox_completion (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,
    body_hash TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    completed_at TEXT NOT NULL
);
CREATE INDEX mailbox_completion_expiry ON mailbox_completion(expires_at);
