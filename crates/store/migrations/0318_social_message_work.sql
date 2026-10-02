CREATE TABLE social_message_work (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    conversation TEXT NOT NULL REFERENCES record(uid),
    context TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
ALTER TABLE social_private_outbox ADD COLUMN record_uid TEXT REFERENCES record(uid);
ALTER TABLE social_private_outbox ADD COLUMN recipient_owner TEXT NOT NULL DEFAULT '';
ALTER TABLE social_private_outbox ADD COLUMN recipient_generation INTEGER NOT NULL DEFAULT 0;
CREATE INDEX social_private_message ON social_private_outbox(record_uid,state);
CREATE TABLE social_session_peer (
    context TEXT NOT NULL,
    peer TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES social_device_state(id) ON DELETE CASCADE,
    PRIMARY KEY(context,peer)
);
ALTER TABLE social_service_envelope ADD COLUMN next_pickup INTEGER NOT NULL DEFAULT 0;
CREATE TABLE social_pickup_work (
    context TEXT NOT NULL,
    service TEXT NOT NULL,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    PRIMARY KEY(context,service)
);
CREATE TABLE social_peer_work (
    conversation TEXT PRIMARY KEY REFERENCES record(uid),
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
CREATE TABLE social_message_event (
    event TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    fact TEXT NOT NULL REFERENCES fact(uid),
    issued_at INTEGER NOT NULL,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT
);
