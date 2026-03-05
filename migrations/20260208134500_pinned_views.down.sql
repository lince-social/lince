-- Remove pinned views columns
ALTER TABLE view DROP COLUMN pinned;
ALTER TABLE view DROP COLUMN position_x;
ALTER TABLE view DROP COLUMN position_y;
ALTER TABLE view DROP COLUMN z_index;
