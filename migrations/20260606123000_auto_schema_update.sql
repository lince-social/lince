CREATE TABLE IF NOT EXISTS transfer_gossip_package (
    id INTEGER PRIMARY KEY,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    package_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(package_json)),
    source_base_url TEXT,
    target_base_url TEXT,
    observed_from_base_url TEXT,
    event_count INTEGER NOT NULL DEFAULT 0,
    latest_event_created_at TEXT,
    first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_pulsed_at TEXT
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_gossip_package_uid
ON transfer_gossip_package(transfer_uid);
