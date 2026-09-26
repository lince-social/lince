ALTER TABLE recurrence ADD COLUMN name TEXT NOT NULL DEFAULT 'Karma rule';
ALTER TABLE recurrence_revision ADD COLUMN name TEXT NOT NULL DEFAULT 'Karma rule';
