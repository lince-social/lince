CREATE TABLE transfer_stock_limit (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    writer_cell_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    minimum_mantissa TEXT NOT NULL,
    minimum_scale INTEGER NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    event_uid TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE transfer_stock_limit_event (
    uid TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_stock_limit_event_no_update
BEFORE UPDATE ON transfer_stock_limit_event BEGIN
    SELECT RAISE(ABORT, 'stock limit history is immutable');
END;

CREATE TRIGGER transfer_stock_limit_event_no_delete
BEFORE DELETE ON transfer_stock_limit_event BEGIN
    SELECT RAISE(ABORT, 'stock limit history is immutable');
END;

CREATE TABLE transfer_stock_roster_history (
    organ_uid TEXT NOT NULL,
    version INTEGER NOT NULL,
    payload TEXT NOT NULL,
    not_after TEXT NOT NULL,
    PRIMARY KEY (organ_uid, version)
) STRICT;

INSERT INTO transfer_stock_roster_history (organ_uid, version, payload, not_after)
SELECT organ_uid, version, payload, not_after FROM organ_roster;

CREATE TRIGGER transfer_stock_roster_history_insert
AFTER INSERT ON organ_roster BEGIN
    INSERT OR IGNORE INTO transfer_stock_roster_history (organ_uid, version, payload, not_after)
    VALUES (NEW.organ_uid, NEW.version, NEW.payload, NEW.not_after);
END;

CREATE TRIGGER transfer_stock_roster_history_update
AFTER UPDATE ON organ_roster BEGIN
    INSERT OR IGNORE INTO transfer_stock_roster_history (organ_uid, version, payload, not_after)
    VALUES (NEW.organ_uid, NEW.version, NEW.payload, NEW.not_after);
END;

CREATE TRIGGER transfer_stock_roster_writer_insert
BEFORE INSERT ON organ_roster
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit l JOIN record r ON r.uid = l.record_uid WHERE r.organ_uid = NEW.organ_uid AND
    (NOT EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') = l.writer_cell_uid AND cap.value = 'write') OR
     EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') != l.writer_cell_uid AND cap.value = 'write')))
BEGIN
    SELECT RAISE(ABORT, 'remove hard stock limits before changing the Organ writer');
END;

CREATE TRIGGER transfer_stock_roster_writer_update
BEFORE UPDATE ON organ_roster
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit l JOIN record r ON r.uid = l.record_uid WHERE r.organ_uid = NEW.organ_uid AND
    (NOT EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') = l.writer_cell_uid AND cap.value = 'write') OR
     EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') != l.writer_cell_uid AND cap.value = 'write')))
BEGIN
    SELECT RAISE(ABORT, 'remove hard stock limits before changing the Organ writer');
END;

CREATE TRIGGER transfer_stock_limit_record_identity
BEFORE UPDATE OF unit_uid, organ_uid, deleted_at ON record
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit WHERE record_uid = OLD.uid)
 AND (NEW.unit_uid IS NOT OLD.unit_uid OR NEW.organ_uid IS NOT OLD.organ_uid OR NEW.deleted_at IS NOT OLD.deleted_at)
BEGIN
    SELECT RAISE(ABORT, 'remove the hard stock limit before changing the Record unit, owner or deletion state');
END;
