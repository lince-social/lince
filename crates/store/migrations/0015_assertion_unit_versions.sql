DROP TRIGGER assertion_unit_definition;
CREATE TRIGGER assertion_unit_definition AFTER UPDATE OF unit_uid ON record_assertion
WHEN NEW.unit_uid IS NOT OLD.unit_uid BEGIN
    UPDATE record_assertion SET unit_revision=CASE WHEN NEW.unit_uid IS NULL THEN NULL ELSE COALESCE(NEW.unit_revision,(SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.unit_uid),1) END WHERE uid=NEW.uid;
END;
CREATE TRIGGER assertion_unit_version_stable BEFORE UPDATE OF unit_revision ON record_assertion
WHEN NEW.unit_uid IS OLD.unit_uid AND NEW.unit_uid IS NOT NULL AND OLD.unit_revision IS NOT NULL AND NEW.unit_revision IS NOT OLD.unit_revision
BEGIN SELECT RAISE(ABORT, 'An Assertion retains its selected unit definition version'); END;
