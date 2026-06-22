CREATE TABLE IF NOT EXISTS transfer_package_receipt (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    source_base_url TEXT,
    received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    seen_at TEXT,
    received_receipt_generated INTEGER NOT NULL DEFAULT 0,
    seen_receipt_generated INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_package_receipt_transfer_source
ON transfer_package_receipt(transfer_id, source_base_url);
