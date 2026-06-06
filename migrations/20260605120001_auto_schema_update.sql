CREATE TABLE IF NOT EXISTS transfer_event (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    actor_label TEXT NOT NULL CHECK (length(trim(actor_label)) > 0),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('transfer_created', 'item_created', 'agreement_changed', 'delivery_confirmed', 'receipt_confirmed', 'settlement_applied')),
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    previous_event_id INTEGER REFERENCES transfer_event(id),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
    my_record_id INTEGER NOT NULL REFERENCES record(id),
    server_record_id INTEGER NOT NULL REFERENCES record(id),
    my_quantity_delta REAL NOT NULL,
    server_quantity_delta REAL NOT NULL,
    event_id INTEGER REFERENCES transfer_event(id),
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_sync_cursor (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    peer_label TEXT NOT NULL CHECK (length(trim(peer_label)) > 0),
    last_event_id INTEGER REFERENCES transfer_event(id),
    last_synced_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_sync_cursor_transfer_peer ON transfer_sync_cursor(transfer_id, peer_label);
