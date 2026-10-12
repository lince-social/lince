CREATE TABLE microcontroller_authority (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    audience TEXT NOT NULL UNIQUE
);

INSERT INTO microcontroller_authority VALUES (1, lower(hex(randomblob(32))));

CREATE TABLE microcontroller (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key TEXT NOT NULL UNIQUE,
    signal_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    key_epoch INTEGER NOT NULL CHECK (key_epoch > 0),
    cooldown_seconds INTEGER NOT NULL CHECK (cooldown_seconds BETWEEN 1 AND 86400),
    minimum TEXT NOT NULL,
    maximum TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    last_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
    last_fingerprint TEXT,
    last_fact_uid TEXT REFERENCES fact(uid),
    last_received_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE TABLE microcontroller_audit (
    device_uid TEXT NOT NULL REFERENCES microcontroller(uid),
    revision INTEGER NOT NULL,
    actor_uid TEXT,
    at INTEGER NOT NULL,
    configuration TEXT NOT NULL,
    PRIMARY KEY (device_uid, revision)
);
