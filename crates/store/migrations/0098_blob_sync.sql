CREATE TABLE blob_sync (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('incoming', 'outgoing')),
    peer TEXT NOT NULL,
    peer_organ TEXT,
    label TEXT NOT NULL,
    manifest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('offered', 'accepted', 'completed', 'declined', 'cancelled')),
    destination TEXT,
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress >= 0),
    error TEXT NOT NULL DEFAULT '',
    settled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX blob_sync_work ON blob_sync(owner, state, settled);
CREATE INDEX blob_sync_peer ON blob_sync(peer, direction, state);
