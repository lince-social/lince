ALTER TABLE transfer_exchange_path ADD COLUMN public_exchange_uid TEXT;
CREATE INDEX transfer_exchange_public_route ON transfer_exchange_path(transfer_uid, public_exchange_uid, revision);

CREATE TRIGGER transfer_occurrence_item_source_insert
BEFORE INSERT ON transfer_occurrence
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND NOT EXISTS(SELECT 1 FROM promise WHERE uid = NEW.promise_uid
                   AND transfer_uid = NEW.transfer_uid AND item_json IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound occurrence requires a Transfer item');
END;

CREATE TRIGGER transfer_occurrence_item_source_update
BEFORE UPDATE ON transfer_occurrence
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND NOT EXISTS(SELECT 1 FROM promise WHERE uid = NEW.promise_uid
                   AND transfer_uid = NEW.transfer_uid AND item_json IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound occurrence requires a Transfer item');
END;
