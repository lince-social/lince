CREATE TABLE concept_definition_version (
    concept_uid TEXT NOT NULL REFERENCES concept(uid) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 256),
    definition TEXT NOT NULL CHECK(length(CAST(definition AS BLOB)) <= 16384),
    created_at TEXT NOT NULL,
    PRIMARY KEY(concept_uid, revision)
);
INSERT INTO concept_definition_version
SELECT concept_uid, 1, definition, c.created_at FROM concept_source s JOIN concept c ON c.uid=s.concept_uid;
CREATE TRIGGER concept_definition_source AFTER INSERT ON concept_source BEGIN
    INSERT OR IGNORE INTO concept_definition_version VALUES(NEW.concept_uid, 1, NEW.definition, (SELECT created_at FROM concept WHERE uid=NEW.concept_uid));
END;
CREATE TRIGGER concept_definition_append BEFORE INSERT ON concept_definition_version
WHEN NEW.revision > COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.concept_uid), 0) + 1
BEGIN SELECT RAISE(ABORT, 'Definition versions must be consecutive'); END;
CREATE TRIGGER concept_definition_immutable BEFORE UPDATE ON concept_definition_version
BEGIN SELECT RAISE(ABORT, 'Saved definition versions cannot change'); END;
CREATE TRIGGER concept_definition_keep BEFORE DELETE ON concept_definition_version
WHEN EXISTS(SELECT 1 FROM concept WHERE uid=OLD.concept_uid)
BEGIN SELECT RAISE(ABORT, 'Saved definition versions cannot be removed'); END;

ALTER TABLE record_assertion ADD COLUMN predicate_revision INTEGER NOT NULL DEFAULT 0 CHECK(predicate_revision >= 0 AND predicate_revision <= 256);
ALTER TABLE record_assertion ADD COLUMN unit_revision INTEGER CHECK(unit_revision BETWEEN 1 AND 256);
UPDATE record_assertion SET predicate_revision=1, unit_revision=CASE WHEN unit_uid IS NULL THEN NULL ELSE 1 END;
CREATE TRIGGER assertion_definition_insert AFTER INSERT ON record_assertion BEGIN
    UPDATE record_assertion SET
        predicate_revision=CASE WHEN NEW.predicate_revision=0 THEN COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.predicate_uid),1) ELSE NEW.predicate_revision END,
        unit_revision=CASE WHEN NEW.unit_uid IS NULL THEN NULL ELSE COALESCE(NEW.unit_revision,(SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.unit_uid),1) END
    WHERE uid=NEW.uid;
END;
CREATE TRIGGER assertion_definition_stable BEFORE UPDATE OF predicate_revision ON record_assertion
WHEN OLD.predicate_revision>0 AND NEW.predicate_uid=OLD.predicate_uid AND NEW.predicate_revision!=OLD.predicate_revision
BEGIN SELECT RAISE(ABORT, 'An Assertion retains its selected definition version'); END;
CREATE TRIGGER assertion_unit_definition AFTER UPDATE OF unit_uid ON record_assertion
WHEN NEW.unit_uid IS NOT OLD.unit_uid BEGIN
    UPDATE record_assertion SET unit_revision=CASE WHEN NEW.unit_uid IS NULL THEN NULL ELSE COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.unit_uid),1) END WHERE uid=NEW.uid;
END;

CREATE TABLE record_content_version (
    record_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision>0),
    content TEXT NOT NULL CHECK(json_valid(content)),
    saved_at TEXT NOT NULL,
    PRIMARY KEY(record_uid, revision)
);
INSERT INTO record_content_version SELECT uid,1,json_object('slug',slug,'kind',kind,'head',head,'body',body,'quantity_mantissa',quantity_mantissa,'quantity_scale',quantity_scale,'unit_uid',unit_uid,'place_uid',place_uid,'deleted_at',deleted_at),updated_at FROM record;
CREATE TRIGGER record_content_insert AFTER INSERT ON record BEGIN
    INSERT INTO record_content_version VALUES(NEW.uid,1,json_object('slug',NEW.slug,'kind',NEW.kind,'head',NEW.head,'body',NEW.body,'quantity_mantissa',NEW.quantity_mantissa,'quantity_scale',NEW.quantity_scale,'unit_uid',NEW.unit_uid,'place_uid',NEW.place_uid,'deleted_at',NEW.deleted_at),NEW.updated_at);
END;
CREATE TRIGGER record_content_update AFTER UPDATE ON record
WHEN NEW.slug IS NOT OLD.slug OR NEW.kind IS NOT OLD.kind OR NEW.head IS NOT OLD.head OR NEW.body IS NOT OLD.body OR NEW.quantity_mantissa IS NOT OLD.quantity_mantissa OR NEW.quantity_scale IS NOT OLD.quantity_scale OR NEW.unit_uid IS NOT OLD.unit_uid OR NEW.place_uid IS NOT OLD.place_uid OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO record_content_version SELECT NEW.uid,COALESCE(MAX(revision),0)+1,json_object('slug',NEW.slug,'kind',NEW.kind,'head',NEW.head,'body',NEW.body,'quantity_mantissa',NEW.quantity_mantissa,'quantity_scale',NEW.quantity_scale,'unit_uid',NEW.unit_uid,'place_uid',NEW.place_uid,'deleted_at',NEW.deleted_at),NEW.updated_at FROM record_content_version WHERE record_uid=NEW.uid;
END;
CREATE TRIGGER record_content_immutable BEFORE UPDATE ON record_content_version
BEGIN SELECT RAISE(ABORT, 'Saved Record content cannot change'); END;
CREATE TRIGGER record_content_keep BEFORE DELETE ON record_content_version
BEGIN SELECT RAISE(ABORT, 'Saved Record content cannot be removed'); END;

ALTER TABLE lingua ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0);
CREATE TRIGGER vocabulary_collection_update AFTER UPDATE OF name,visibility ON lingua
WHEN NEW.name!=OLD.name OR NEW.visibility!=OLD.visibility BEGIN
    UPDATE lingua SET revision=revision+1 WHERE uid=NEW.uid;
END;
CREATE TRIGGER vocabulary_collection_insert AFTER INSERT ON lingua_concept BEGIN
    UPDATE lingua SET revision=revision+1 WHERE uid=NEW.lingua_uid;
END;
CREATE TRIGGER vocabulary_collection_remove AFTER DELETE ON lingua_concept BEGIN
    UPDATE lingua SET revision=revision+1 WHERE uid=OLD.lingua_uid;
END;
CREATE TABLE vocabulary_publication (
    uid TEXT PRIMARY KEY,
    vocabulary_uid TEXT NOT NULL REFERENCES lingua(uid),
    revision INTEGER NOT NULL CHECK(revision>0),
    package TEXT NOT NULL CHECK(json_valid(package)),
    withdrawn INTEGER NOT NULL DEFAULT 0 CHECK(withdrawn IN (0,1))
);
CREATE UNIQUE INDEX vocabulary_publication_active ON vocabulary_publication(vocabulary_uid) WHERE withdrawn=0;
CREATE TRIGGER vocabulary_publication_immutable BEFORE UPDATE OF uid,vocabulary_uid,revision,package ON vocabulary_publication
BEGIN SELECT RAISE(ABORT, 'Published snapshots cannot change'); END;
CREATE TRIGGER vocabulary_publication_keep BEFORE DELETE ON vocabulary_publication
BEGIN SELECT RAISE(ABORT, 'Withdraw publications while preserving saved meanings'); END;
CREATE TABLE organ_vocabulary (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    vocabulary_uid TEXT NOT NULL REFERENCES lingua(uid),
    publication_uid TEXT REFERENCES vocabulary_publication(uid),
    adopted_at TEXT NOT NULL,
    PRIMARY KEY(organ_uid,vocabulary_uid)
);
INSERT INTO organ_vocabulary SELECT o.uid,l.uid,NULL,l.created_at FROM record o CROSS JOIN lingua l WHERE o.kind='organ' AND o.slug='local';
