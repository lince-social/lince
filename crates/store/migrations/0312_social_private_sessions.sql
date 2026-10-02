CREATE TABLE social_device_state (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('account','session','authority')),
    context TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=716800),
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);
CREATE INDEX social_device_context ON social_device_state(context,kind);
CREATE TABLE social_private_seen (
    envelope TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    cipher_hash TEXT NOT NULL,
    message TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    receipt TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX social_private_seen_cipher ON social_private_seen(cipher_hash);
CREATE TABLE social_message_identity (
    message TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    conversation TEXT NOT NULL
);
CREATE TABLE social_private_outbox (
    id TEXT PRIMARY KEY,
    context TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=32768),
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','ready','expired','held','cancelled')),
    error TEXT
);
CREATE TABLE social_private_destination (
    envelope TEXT NOT NULL REFERENCES social_private_outbox(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','failed','cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    receipt TEXT,
    error TEXT,
    PRIMARY KEY(envelope,service)
);
CREATE TABLE social_owner_control (
    owner TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE social_reply_route (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    signing_key TEXT NOT NULL,
    pickup_key TEXT NOT NULL,
    body TEXT NOT NULL,
    post TEXT,
    state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','closed')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX social_reply_expiry ON social_reply_route(expires_at);
CREATE TABLE social_sender_admission (
    route TEXT NOT NULL REFERENCES social_reply_route(id),
    sender TEXT NOT NULL,
    state TEXT NOT NULL,
    body TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(route,sender)
);
CREATE TABLE social_service_envelope (
    id TEXT PRIMARY KEY,
    route TEXT NOT NULL REFERENCES social_reply_route(id),
    sender TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=32768),
    partition TEXT NOT NULL CHECK(partition IN ('stranger','trusted','control')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX social_service_route ON social_service_envelope(route,created_at,id);
CREATE TABLE social_service_completed (
    id TEXT PRIMARY KEY,
    route TEXT NOT NULL,
    sender TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE social_access_nonce (
    nonce TEXT NOT NULL,
    key TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY(key,nonce)
);
