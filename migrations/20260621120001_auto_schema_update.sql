ALTER TABLE configuration ADD COLUMN transfer_send_received_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_received_receipts IN (0, 1));
ALTER TABLE configuration ADD COLUMN transfer_send_seen_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_seen_receipts IN (0, 1));
ALTER TABLE configuration ADD COLUMN transfer_anonymous_package_viewing INTEGER NOT NULL DEFAULT 0 CHECK (transfer_anonymous_package_viewing IN (0, 1));
