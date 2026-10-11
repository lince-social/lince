CREATE TABLE person_terminology (
    person_uid TEXT PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    language TEXT NOT NULL
);

CREATE TRIGGER concept_term_identity_immutable BEFORE UPDATE OF uid,concept_uid,owner_uid ON concept_term
WHEN NEW.uid IS NOT OLD.uid OR NEW.concept_uid IS NOT OLD.concept_uid OR NEW.owner_uid IS NOT OLD.owner_uid
BEGIN SELECT RAISE(ABORT, 'A term identity cannot change its meaning or owner'); END;

CREATE TRIGGER concept_source_immutable BEFORE UPDATE ON concept_source
BEGIN SELECT RAISE(ABORT, 'A source meaning is immutable; create a distinct Concept'); END;

CREATE TRIGGER concept_source_name_immutable BEFORE UPDATE OF canonical_name ON concept
WHEN NEW.canonical_name != OLD.canonical_name AND EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=OLD.uid)
BEGIN SELECT RAISE(ABORT, 'Edit terms while preserving the original source name'); END;

CREATE TRIGGER saved_location_assertion_insert BEFORE INSERT ON record_assertion
WHEN NEW.retracted_at IS NULL AND EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid='vr_01JA0000000000000000000002' AND concept_uid=NEW.predicate_uid)
BEGIN
    SELECT CASE WHEN NEW.role!='ordinary' OR NEW.object_uid IS NULL OR NEW.subject_uid=NEW.object_uid OR NEW.quantity_mantissa IS NOT NULL OR NEW.unit_uid IS NOT NULL THEN RAISE(ABORT, 'A saved location must be an ordinary unquantified link to another Record') END;
    SELECT CASE WHEN (SELECT COUNT(*) FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid='vr_01JA0000000000000000000002' WHERE a.subject_uid=NEW.subject_uid AND a.retracted_at IS NULL AND a.uid!=NEW.uid)>=64 THEN RAISE(ABORT, 'At most 64 address links per Record') END;
END;

CREATE TRIGGER saved_location_assertion_update BEFORE UPDATE ON record_assertion
WHEN NEW.retracted_at IS NULL AND EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid='vr_01JA0000000000000000000002' AND concept_uid=NEW.predicate_uid)
BEGIN
    SELECT CASE WHEN NEW.role!='ordinary' OR NEW.object_uid IS NULL OR NEW.subject_uid=NEW.object_uid OR NEW.quantity_mantissa IS NOT NULL OR NEW.unit_uid IS NOT NULL THEN RAISE(ABORT, 'A saved location must be an ordinary unquantified link to another Record') END;
    SELECT CASE WHEN (SELECT COUNT(*) FROM record_assertion a JOIN vocabulary_role_concept b ON b.concept_uid=a.predicate_uid AND b.role_uid='vr_01JA0000000000000000000002' WHERE a.subject_uid=NEW.subject_uid AND a.retracted_at IS NULL AND a.uid!=NEW.uid)>=64 THEN RAISE(ABORT, 'At most 64 address links per Record') END;
END;

CREATE TRIGGER saved_location_binding_shape BEFORE INSERT ON vocabulary_role_concept
WHEN NEW.role_uid='vr_01JA0000000000000000000002'
BEGIN
    SELECT CASE WHEN EXISTS(SELECT 1 FROM record_assertion WHERE predicate_uid=NEW.concept_uid AND retracted_at IS NULL AND (role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL)) THEN RAISE(ABORT, 'Existing Assertions do not fit a saved location binding') END;
    SELECT CASE WHEN EXISTS(SELECT subject_uid FROM record_assertion WHERE retracted_at IS NULL AND (predicate_uid=NEW.concept_uid OR predicate_uid IN (SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=NEW.role_uid)) GROUP BY subject_uid HAVING COUNT(*)>64) THEN RAISE(ABORT, 'Too many existing saved address links') END;
END;

CREATE TRIGGER saved_location_write_binding BEFORE UPDATE OF write_concept_uid ON vocabulary_role
WHEN NOT EXISTS(SELECT 1 FROM vocabulary_role_concept WHERE role_uid=NEW.uid AND concept_uid=NEW.write_concept_uid)
BEGIN SELECT RAISE(ABORT, 'The write Concept must be accepted by its feature'); END;

CREATE TRIGGER saved_location_keep_write_binding BEFORE DELETE ON vocabulary_role_concept
WHEN EXISTS(SELECT 1 FROM vocabulary_role WHERE uid=OLD.role_uid AND write_concept_uid=OLD.concept_uid)
BEGIN SELECT RAISE(ABORT, 'Choose a replacement write Concept before removing its binding'); END;
