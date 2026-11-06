CREATE TEMP TABLE record_body_layout_0044 (
    row_id INTEGER PRIMARY KEY,
    body TEXT NOT NULL
);
INSERT INTO record_body_layout_0044 SELECT rowid, body FROM record;
DROP TRIGGER IF EXISTS projection_UPDATE_record;
DROP TRIGGER record_content_insert;
DROP TRIGGER record_content_update;
DROP TRIGGER record_revision_record_update;
ALTER TABLE record DROP COLUMN body;
ALTER TABLE record ADD COLUMN body TEXT NOT NULL DEFAULT '';
UPDATE record SET body = (SELECT body FROM record_body_layout_0044 WHERE row_id = record.rowid);
DROP TABLE record_body_layout_0044;

CREATE TRIGGER record_revision_record_update
AFTER UPDATE ON record
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.uid;
END;

CREATE TRIGGER record_content_insert AFTER INSERT ON record BEGIN
    INSERT INTO record_content_version VALUES(NEW.uid,1,json_object('slug',NEW.slug,'kind',NEW.kind,'head',NEW.head,'body',NEW.body,'quantity_mantissa',NEW.quantity_mantissa,'quantity_scale',NEW.quantity_scale,'unit_uid',NEW.unit_uid,'place_uid',NEW.place_uid,'deleted_at',NEW.deleted_at),NEW.updated_at);
END;

CREATE TRIGGER record_content_update AFTER UPDATE ON record
WHEN NEW.slug IS NOT OLD.slug OR NEW.kind IS NOT OLD.kind OR NEW.head IS NOT OLD.head OR NEW.body IS NOT OLD.body OR NEW.quantity_mantissa IS NOT OLD.quantity_mantissa OR NEW.quantity_scale IS NOT OLD.quantity_scale OR NEW.unit_uid IS NOT OLD.unit_uid OR NEW.place_uid IS NOT OLD.place_uid OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO record_content_version SELECT NEW.uid,COALESCE(MAX(revision),0)+1,json_object('slug',NEW.slug,'kind',NEW.kind,'head',NEW.head,'body',NEW.body,'quantity_mantissa',NEW.quantity_mantissa,'quantity_scale',NEW.quantity_scale,'unit_uid',NEW.unit_uid,'place_uid',NEW.place_uid,'deleted_at',NEW.deleted_at),NEW.updated_at FROM record_content_version WHERE record_uid=NEW.uid;
END;
