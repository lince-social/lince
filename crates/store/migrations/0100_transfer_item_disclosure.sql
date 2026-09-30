ALTER TABLE promise ADD COLUMN item_json TEXT CHECK (item_json IS NULL OR json_valid(item_json));

CREATE TRIGGER promise_item_source_insert
BEFORE INSERT ON promise
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND (NEW.transfer_uid IS NULL OR NEW.item_json IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound promise requires a Transfer item');
END;

CREATE TRIGGER promise_item_source_update
BEFORE UPDATE ON promise
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND (NEW.transfer_uid IS NULL OR NEW.item_json IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound promise requires a Transfer item');
END;
