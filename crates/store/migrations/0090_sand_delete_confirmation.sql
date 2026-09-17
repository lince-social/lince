ALTER TABLE configuration ADD COLUMN sand_delete_confirmation INTEGER NOT NULL DEFAULT 1 CHECK (sand_delete_confirmation IN (0, 1));
