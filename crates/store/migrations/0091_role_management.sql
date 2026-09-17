CREATE INDEX person_access_role ON person_access(role_id);

CREATE TRIGGER role_permission_revision_role_rename
AFTER UPDATE OF name ON role
WHEN NEW.name IS NOT OLD.name
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.id;
END;
