-- Blueprint X: Senses. A match rule is a record (kind='rule', quantity =
-- active) with this sidecar; the discovery cache holds known organs' open
-- promises (fed by the Part XV sync layer; the matcher only reads it).
CREATE TABLE sense_rule (
    record_uid     TEXT PRIMARY KEY REFERENCES record(uid),
    watch_concept  TEXT,                            -- NULL = any concept
    max_proximity  INTEGER NOT NULL DEFAULT 1,      -- HARD ceiling
    min_confidence REAL NOT NULL DEFAULT 0.0,
    auto           TEXT NOT NULL DEFAULT 'draft_only'
    -- draft_only | ask | auto_propose (sending lands with Part XV)
);

CREATE TABLE discovery_cache (
    promise_uid  TEXT PRIMARY KEY,                  -- the REMOTE promise uid
    organ        TEXT NOT NULL,
    proximity    INTEGER NOT NULL,
    concept      TEXT,
    unit         TEXT,
    delta        REAL NOT NULL,
    window_start TEXT,
    window_end   TEXT,
    confidence   REAL NOT NULL DEFAULT 0.5,
    fetched_at   TEXT NOT NULL
);
