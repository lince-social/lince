ALTER TABLE configuration ADD COLUMN automatic_update_channel TEXT NOT NULL DEFAULT 'rolling';
ALTER TABLE configuration ADD COLUMN automatic_update_notify_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE configuration ADD COLUMN automatic_update_install_enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE configuration ADD COLUMN automatic_update_last_seen_revision TEXT;
