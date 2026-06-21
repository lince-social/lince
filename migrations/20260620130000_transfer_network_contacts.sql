ALTER TABLE organ
ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'known'
CHECK (trust_state IN ('unknown', 'known', 'blocked'));

ALTER TABLE organ
ADD COLUMN contact_discovery_enabled INTEGER NOT NULL DEFAULT 0
CHECK (contact_discovery_enabled IN (0, 1));

ALTER TABLE organ
ADD COLUMN last_seen_at TEXT
CHECK (last_seen_at IS NULL OR julianday(last_seen_at) IS NOT NULL);

ALTER TABLE organ
ADD COLUMN last_transfer_polled_at TEXT
CHECK (last_transfer_polled_at IS NULL OR julianday(last_transfer_polled_at) IS NOT NULL);

ALTER TABLE configuration
ADD COLUMN transfer_known_peer_polling_enabled INTEGER NOT NULL DEFAULT 1
CHECK (transfer_known_peer_polling_enabled IN (0, 1));
