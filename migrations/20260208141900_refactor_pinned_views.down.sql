-- Rollback refactored pinned views
DROP TABLE pinned_view;

-- Restore old columns to view table
ALTER TABLE view ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE view ADD COLUMN position_x REAL;
ALTER TABLE view ADD COLUMN position_y REAL;
ALTER TABLE view ADD COLUMN z_index INTEGER NOT NULL DEFAULT 0;
