CREATE TABLE mailbox_roster_floor (
    organ_uid TEXT PRIMARY KEY REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE mailbox_device_ack (
    uid TEXT NOT NULL REFERENCES mailbox_bundle(uid) ON DELETE CASCADE,
    node_id TEXT NOT NULL,
    PRIMARY KEY (uid, node_id)
);
CREATE TABLE mailbox_inbox (
    uid TEXT PRIMARY KEY,
    carrier TEXT NOT NULL,
    body TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    received_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
CREATE INDEX mailbox_inbox_pending ON mailbox_inbox(state, next_attempt);
