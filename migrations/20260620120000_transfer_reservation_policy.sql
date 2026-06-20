CREATE TABLE IF NOT EXISTS record_transfer_availability (
    record_id INTEGER PRIMARY KEY REFERENCES record(id) ON DELETE CASCADE,
    actual_quantity REAL NOT NULL DEFAULT 0,
    proposed_outgoing_quantity REAL NOT NULL DEFAULT 0,
    proposed_incoming_quantity REAL NOT NULL DEFAULT 0,
    reserved_quantity REAL NOT NULL DEFAULT 0,
    reserved_incoming_quantity REAL NOT NULL DEFAULT 0,
    available_quantity REAL NOT NULL DEFAULT 0,
    planned_quantity REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
