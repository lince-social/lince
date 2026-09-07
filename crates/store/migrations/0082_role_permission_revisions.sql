CREATE TABLE role_permission_revision (
    role_id INTEGER NOT NULL PRIMARY KEY CHECK (role_id > 0),
    revision INTEGER NOT NULL CHECK (revision > 0)
) STRICT;

INSERT INTO role_permission_revision (role_id, revision)
SELECT id, 1 FROM role WHERE id > 0;

CREATE TRIGGER role_permission_revision_retained_insert
BEFORE INSERT ON role_permission_revision
WHEN EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.role_id)
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision identity is already retained');
END;

CREATE TRIGGER role_permission_revision_retained_delete
BEFORE DELETE ON role_permission_revision
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision tombstones are permanent');
END;

CREATE TRIGGER role_permission_revision_monotonic_update
BEFORE UPDATE ON role_permission_revision
WHEN NEW.role_id IS NOT OLD.role_id
    OR typeof(OLD.role_id) <> 'integer' OR OLD.role_id <= 0
    OR typeof(NEW.role_id) <> 'integer' OR NEW.role_id <= 0
    OR typeof(OLD.revision) <> 'integer' OR OLD.revision <= 0
    OR typeof(NEW.revision) <> 'integer' OR NEW.revision <= OLD.revision
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision must increase without changing identity');
END;

CREATE TRIGGER role_permission_revision_role_insert
AFTER INSERT ON role
WHEN NEW.id > 0
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND (typeof(role_id) <> 'integer' OR role_id <= 0
                OR typeof(revision) <> 'integer' OR revision <= 0
                OR revision = 9223372036854775807)
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.id;
    INSERT INTO role_permission_revision (role_id, revision)
    SELECT NEW.id, 1
    WHERE NOT EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id);
END;

CREATE TRIGGER role_permission_revision_role_delete
BEFORE DELETE ON role
WHEN OLD.id > 0
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = OLD.id;
END;

CREATE TRIGGER role_permission_revision_role_move
AFTER UPDATE OF id ON role
WHEN NEW.id IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE OLD.id > 0 AND NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Role permission revision is corrupt or exhausted')
    WHERE NEW.id > 0 AND EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND (typeof(role_id) <> 'integer' OR role_id <= 0
                OR typeof(revision) <> 'integer' OR revision <= 0
                OR revision = 9223372036854775807)
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (OLD.id, NEW.id) AND role_id > 0;
    INSERT INTO role_permission_revision (role_id, revision)
    SELECT NEW.id, 1 WHERE NEW.id > 0
        AND NOT EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id);
END;

CREATE TRIGGER role_permission_revision_member_insert
AFTER INSERT ON role_permission
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.role_id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.role_id;
END;

CREATE TRIGGER role_permission_revision_member_delete
AFTER DELETE ON role_permission
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.role_id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = OLD.role_id;
END;

CREATE TRIGGER role_permission_revision_member_update
AFTER UPDATE OF role_id, permission_id ON role_permission
WHEN NEW.role_id IS NOT OLD.role_id OR NEW.permission_id IS NOT OLD.permission_id
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM (SELECT OLD.role_id AS id UNION SELECT NEW.role_id) AS affected
        LEFT JOIN role_permission_revision AS revision ON revision.role_id = affected.id
        WHERE typeof(revision.role_id) <> 'integer' OR revision.role_id <= 0
            OR typeof(revision.revision) <> 'integer' OR revision.revision <= 0
            OR revision.revision = 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (OLD.role_id, NEW.role_id);
END;

CREATE TRIGGER role_permission_revision_permission_update
AFTER UPDATE OF id, subject, action ON permission
WHEN NEW.id IS NOT OLD.id OR NEW.subject IS NOT OLD.subject OR NEW.action IS NOT OLD.action
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM (
            SELECT DISTINCT role_id FROM role_permission WHERE permission_id IN (OLD.id, NEW.id)
        ) AS affected
        LEFT JOIN role_permission_revision AS revision ON revision.role_id = affected.role_id
        WHERE typeof(revision.role_id) <> 'integer' OR revision.role_id <= 0
            OR typeof(revision.revision) <> 'integer' OR revision.revision <= 0
            OR revision.revision = 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (
        SELECT role_id FROM role_permission WHERE permission_id IN (OLD.id, NEW.id)
    );
END;
