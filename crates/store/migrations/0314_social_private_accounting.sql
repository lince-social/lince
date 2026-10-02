CREATE TABLE social_mailbox_pin (
    mailbox TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    signing_key TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    pickup_key TEXT NOT NULL
);
CREATE TABLE social_sender_counter (
    route TEXT NOT NULL,
    sender TEXT NOT NULL,
    introductions INTEGER NOT NULL DEFAULT 0,
    provisional INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(route,sender)
);
ALTER TABLE social_service_completed ADD COLUMN receipt TEXT;
