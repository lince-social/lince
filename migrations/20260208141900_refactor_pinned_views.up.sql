-- Refactor pinned views to use separate junction table
-- Drop old columns from view table
ALTER TABLE view DROP COLUMN pinned;
ALTER TABLE view DROP COLUMN position_x;
ALTER TABLE view DROP COLUMN position_y;
ALTER TABLE view DROP COLUMN z_index;

-- Create new pinned_view table (similar to collection_view)
CREATE TABLE pinned_view (
    id INTEGER PRIMARY KEY,
    view_id INTEGER NOT NULL REFERENCES view(id),
    position_x REAL NOT NULL DEFAULT 300.0,
    position_y REAL NOT NULL DEFAULT 200.0,
    z_index INTEGER NOT NULL DEFAULT 0
);
