ALTER TABLE organ ADD COLUMN file_sync_enabled INTEGER NOT NULL DEFAULT 0 CHECK (file_sync_enabled IN (0, 1));
ALTER TABLE organ ADD COLUMN file_sync_path TEXT;
