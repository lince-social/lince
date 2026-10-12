CREATE TABLE signal_endpoint_config (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signal_uid TEXT NOT NULL REFERENCES record(uid),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    credential_epoch INTEGER NOT NULL CHECK (credential_epoch > 0),
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

INSERT INTO signal_endpoint_config SELECT * FROM signal_endpoint;

CREATE TABLE signal_endpoint_token_next (
    endpoint_uid TEXT PRIMARY KEY REFERENCES signal_endpoint_config(uid),
    digest TEXT NOT NULL UNIQUE
);

INSERT INTO signal_endpoint_token_next SELECT * FROM signal_endpoint_token;

CREATE TABLE signal_endpoint_audit_next (
    endpoint_uid TEXT NOT NULL,
    revision INTEGER NOT NULL,
    actor_uid TEXT,
    at INTEGER NOT NULL,
    configuration TEXT NOT NULL,
    PRIMARY KEY (endpoint_uid, revision)
);

INSERT INTO signal_endpoint_audit_next SELECT * FROM signal_endpoint_audit;

DROP TABLE signal_endpoint_token;
DROP TABLE signal_endpoint_audit;
DROP TABLE signal_endpoint;

ALTER TABLE signal_endpoint_config RENAME TO signal_endpoint;
ALTER TABLE signal_endpoint_token_next RENAME TO signal_endpoint_token;
ALTER TABLE signal_endpoint_audit_next RENAME TO signal_endpoint_audit;
