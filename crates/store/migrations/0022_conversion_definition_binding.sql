DROP TRIGGER conversion_version_update;
CREATE TRIGGER conversion_version_update AFTER UPDATE ON concept_conversion
WHEN NEW.numerator IS NOT OLD.numerator OR NEW.denominator IS NOT OLD.denominator
    OR (SELECT a_revision FROM concept_conversion_version WHERE a_uid=NEW.a_uid AND b_uid=NEW.b_uid ORDER BY revision DESC LIMIT 1)
        IS NOT COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.a_uid),1)
    OR (SELECT b_revision FROM concept_conversion_version WHERE a_uid=NEW.a_uid AND b_uid=NEW.b_uid ORDER BY revision DESC LIMIT 1)
        IS NOT COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.b_uid),1)
BEGIN
    INSERT INTO concept_conversion_version SELECT NEW.a_uid,NEW.b_uid,
        COALESCE((SELECT MAX(revision) FROM concept_conversion_version WHERE a_uid=NEW.a_uid AND b_uid=NEW.b_uid),0)+1,
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.a_uid),1),
        COALESCE((SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=NEW.b_uid),1),
        NEW.numerator,NEW.denominator,0;
END;
CREATE TRIGGER conversion_identity_stable BEFORE UPDATE OF a_uid,b_uid ON concept_conversion
WHEN NEW.a_uid IS NOT OLD.a_uid OR NEW.b_uid IS NOT OLD.b_uid
BEGIN SELECT RAISE(ABORT,'Replace conversions using their stable unit identities'); END;
