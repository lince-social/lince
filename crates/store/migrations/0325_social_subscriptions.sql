CREATE TABLE social_subscription_job (
    id TEXT PRIMARY KEY,
    config_hash TEXT NOT NULL,
    lease_token TEXT NOT NULL DEFAULT '',
    lease_until INTEGER NOT NULL DEFAULT 0,
    last_attempt INTEGER,
    last_network_attempt INTEGER,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    last_completed INTEGER,
    error TEXT CHECK(error IS NULL OR length(CAST(error AS BLOB))<=2048),
    source TEXT NOT NULL DEFAULT 'Not checked',
    results TEXT NOT NULL DEFAULT '[]' CHECK(length(CAST(results AS BLOB))<=16384)
);
CREATE INDEX social_subscription_due ON social_subscription_job(next_attempt,lease_until);
CREATE TABLE social_subscription_seen (
    id TEXT PRIMARY KEY,
    subscription TEXT NOT NULL,
    post TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    matched_at INTEGER NOT NULL,
    notified INTEGER NOT NULL CHECK(notified IN (0,1))
);
CREATE INDEX social_subscription_notice ON social_subscription_seen(subscription,notified,matched_at);
CREATE INDEX social_subscription_expiry ON social_subscription_seen(expires_at);
