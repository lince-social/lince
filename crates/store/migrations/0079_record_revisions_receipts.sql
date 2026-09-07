CREATE TABLE record_revision (
    record_uid TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision > 0)
) STRICT;

INSERT INTO record_revision (record_uid, revision)
SELECT uid, 1 FROM record;

CREATE TRIGGER record_revision_record_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO record_revision (record_uid, revision) VALUES (NEW.uid, 1);
END;

CREATE TRIGGER record_revision_record_uid_immutable
BEFORE UPDATE OF uid ON record
WHEN NEW.uid <> OLD.uid
BEGIN
    SELECT RAISE(ABORT, 'Record identity is immutable');
END;

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

CREATE TRIGGER record_revision_assertion_insert
AFTER INSERT ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.subject_uid;
END;

CREATE TRIGGER record_revision_assertion_update
AFTER UPDATE ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid IN (OLD.subject_uid, NEW.subject_uid);
END;

CREATE TRIGGER record_revision_assertion_delete
AFTER DELETE ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = OLD.subject_uid;
END;

CREATE TRIGGER record_revision_extension_insert
AFTER INSERT ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.record_uid;
END;

CREATE TRIGGER record_revision_extension_update
AFTER UPDATE ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid IN (OLD.record_uid, NEW.record_uid);
END;

CREATE TRIGGER record_revision_extension_delete
AFTER DELETE ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = OLD.record_uid;
END;

CREATE TABLE operation_receipt (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    operation_uid TEXT NOT NULL,
    payload_digest BLOB NOT NULL CHECK(typeof(payload_digest) = 'blob' AND length(payload_digest) = 32),
    outcome TEXT NOT NULL CHECK(json_valid(outcome) AND json_type(outcome) = 'object'),
    accepted_at TEXT NOT NULL CHECK(length(accepted_at) > 0),
    PRIMARY KEY (organ_uid, person_uid, operation_uid)
) STRICT;

CREATE TABLE operation_receipt_record (
    organ_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL,
    operation_uid TEXT NOT NULL,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    PRIMARY KEY (organ_uid, person_uid, operation_uid, record_uid),
    FOREIGN KEY (organ_uid, person_uid, operation_uid)
        REFERENCES operation_receipt(organ_uid, person_uid, operation_uid)
) STRICT;

CREATE TRIGGER operation_receipt_immutable_update
BEFORE UPDATE ON operation_receipt
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_immutable_delete
BEFORE DELETE ON operation_receipt
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_record_immutable_update
BEFORE UPDATE ON operation_receipt_record
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_record_immutable_delete
BEFORE DELETE ON operation_receipt_record
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;
