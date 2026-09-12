ALTER TABLE configuration ADD COLUMN interface_save_seconds INTEGER NOT NULL DEFAULT 30 CHECK (interface_save_seconds BETWEEN 1 AND 86400);
ALTER TABLE configuration ADD COLUMN interface_backup_seconds INTEGER NOT NULL DEFAULT 300 CHECK (interface_backup_seconds BETWEEN interface_save_seconds AND 604800);
ALTER TABLE configuration ADD COLUMN interface_backup_count INTEGER NOT NULL DEFAULT 10 CHECK (interface_backup_count BETWEEN 1 AND 100);
