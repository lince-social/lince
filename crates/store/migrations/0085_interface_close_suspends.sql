ALTER TABLE configuration ADD COLUMN interface_close_suspends INTEGER NOT NULL DEFAULT 1 CHECK (interface_close_suspends IN (0, 1));
