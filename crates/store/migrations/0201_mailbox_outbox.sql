CREATE TABLE mailbox_outbox (
    intent TEXT PRIMARY KEY,
    uid TEXT NOT NULL UNIQUE,
    to_organ TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    requested_copies INTEGER NOT NULL CHECK(requested_copies BETWEEN 1 AND 2),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
CREATE INDEX mailbox_outbox_pending ON mailbox_outbox(next_attempt, expires_at);
CREATE TABLE mailbox_outbox_receipt (
    uid TEXT NOT NULL REFERENCES mailbox_outbox(uid) ON DELETE CASCADE,
    carrier_node TEXT NOT NULL,
    carrier_organ TEXT NOT NULL,
    accepted_at TEXT NOT NULL,
    PRIMARY KEY(uid, carrier_node)
);
DROP INDEX mail_left_by_carrier;
DROP INDEX mail_left_by_recipient;
ALTER TABLE mail_left RENAME TO mail_left_previous;
CREATE TABLE mail_left (
    uid TEXT NOT NULL,
    carrier_organ TEXT NOT NULL,
    carrier_node TEXT NOT NULL,
    to_organ TEXT NOT NULL,
    left_at TEXT NOT NULL,
    expired_at TEXT,
    PRIMARY KEY(uid, carrier_node)
);
INSERT INTO mail_left SELECT * FROM mail_left_previous;
DROP TABLE mail_left_previous;
CREATE INDEX mail_left_by_carrier ON mail_left(carrier_node, expired_at);
CREATE INDEX mail_left_by_recipient ON mail_left(to_organ, expired_at);
