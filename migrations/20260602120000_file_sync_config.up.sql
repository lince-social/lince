ALTER TABLE configuration ADD COLUMN file_sync_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE configuration ADD COLUMN file_sync_path TEXT;
