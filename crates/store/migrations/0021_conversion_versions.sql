CREATE TABLE concept_conversion_version (
    a_uid TEXT NOT NULL REFERENCES concept(uid),
    b_uid TEXT NOT NULL REFERENCES concept(uid),
    revision INTEGER NOT NULL CHECK(revision > 0),
    a_revision INTEGER NOT NULL CHECK(a_revision > 0),
    b_revision INTEGER NOT NULL CHECK(b_revision > 0),
    numerator TEXT NOT NULL CHECK(length(numerator) BETWEEN 1 AND 39 AND numerator NOT GLOB '*[^0-9]*' AND numerator NOT LIKE '0%'),
    denominator TEXT NOT NULL CHECK(length(denominator) BETWEEN 1 AND 39 AND denominator NOT GLOB '*[^0-9]*' AND denominator NOT LIKE '0%'),
    withdrawn INTEGER NOT NULL CHECK(withdrawn IN (0,1)),
    PRIMARY KEY(a_uid,b_uid,revision)
) STRICT;
INSERT INTO concept_conversion_version
SELECT a_uid,b_uid,1,
    COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=a_uid),1),
    COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=b_uid),1),
    numerator,denominator,0
FROM concept_conversion;
CREATE TRIGGER conversion_version_insert AFTER INSERT ON concept_conversion BEGIN
    INSERT INTO concept_conversion_version SELECT NEW.a_uid,NEW.b_uid,
        COALESCE((SELECT MAX(revision) FROM concept_conversion_version WHERE a_uid=NEW.a_uid AND b_uid=NEW.b_uid),0)+1,
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.a_uid),1),
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.b_uid),1),
        NEW.numerator,NEW.denominator,0;
END;
CREATE TRIGGER conversion_version_update AFTER UPDATE ON concept_conversion
WHEN NEW.numerator IS NOT OLD.numerator OR NEW.denominator IS NOT OLD.denominator BEGIN
    INSERT INTO concept_conversion_version SELECT NEW.a_uid,NEW.b_uid,
        COALESCE((SELECT MAX(revision) FROM concept_conversion_version WHERE a_uid=NEW.a_uid AND b_uid=NEW.b_uid),0)+1,
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.a_uid),1),
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.b_uid),1),
        NEW.numerator,NEW.denominator,0;
END;
CREATE TRIGGER conversion_version_delete AFTER DELETE ON concept_conversion BEGIN
    INSERT INTO concept_conversion_version SELECT OLD.a_uid,OLD.b_uid,
        COALESCE((SELECT MAX(revision) FROM concept_conversion_version WHERE a_uid=OLD.a_uid AND b_uid=OLD.b_uid),0)+1,
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=OLD.a_uid),1),
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=OLD.b_uid),1),
        OLD.numerator,OLD.denominator,1;
END;
CREATE TRIGGER conversion_version_immutable BEFORE UPDATE ON concept_conversion_version
BEGIN SELECT RAISE(ABORT,'Saved conversion versions cannot change'); END;
CREATE TRIGGER conversion_version_keep BEFORE DELETE ON concept_conversion_version
BEGIN SELECT RAISE(ABORT,'Saved conversion versions cannot be removed'); END;
