CREATE TABLE person_auth_generation (
    person_uid TEXT NOT NULL PRIMARY KEY,
    generation INTEGER NOT NULL CHECK(generation > 0)
) STRICT;

CREATE TABLE organ_login_generation (
    organ_uid TEXT NOT NULL PRIMARY KEY,
    generation INTEGER NOT NULL CHECK(generation > 0)
) STRICT;

CREATE TABLE person_device (
    person_uid TEXT NOT NULL,
    node_id TEXT NOT NULL,
    revoked INTEGER NOT NULL CHECK(revoked IN (0, 1)),
    revision INTEGER NOT NULL CHECK(revision > 0),
    PRIMARY KEY (person_uid, node_id)
) STRICT;

INSERT INTO person_auth_generation (person_uid, generation)
SELECT uid, 1 FROM record WHERE kind = 'person';

INSERT INTO organ_login_generation (organ_uid, generation)
SELECT organ_uid, 1 FROM organ_login
UNION
SELECT record_uid, 1 FROM organ_contact;

CREATE TRIGGER person_auth_generation_immutable_delete
BEFORE DELETE ON person_auth_generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation tombstones are permanent');
END;

CREATE TRIGGER person_auth_generation_monotonic_update
BEFORE UPDATE ON person_auth_generation
WHEN NEW.person_uid IS NOT OLD.person_uid
    OR typeof(OLD.generation) <> 'integer' OR OLD.generation <= 0
    OR typeof(NEW.generation) <> 'integer' OR NEW.generation <= OLD.generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation must increase without changing identity');
END;

CREATE TRIGGER organ_login_generation_immutable_delete
BEFORE DELETE ON organ_login_generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation tombstones are permanent');
END;

CREATE TRIGGER organ_login_generation_monotonic_update
BEFORE UPDATE ON organ_login_generation
WHEN NEW.organ_uid IS NOT OLD.organ_uid
    OR typeof(OLD.generation) <> 'integer' OR OLD.generation <= 0
    OR typeof(NEW.generation) <> 'integer' OR NEW.generation <= OLD.generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation must increase without changing identity');
END;

CREATE TRIGGER person_device_immutable_delete
BEFORE DELETE ON person_device
BEGIN
    SELECT RAISE(ABORT, 'Device tombstones are permanent');
END;

CREATE TRIGGER person_device_monotonic_update
BEFORE UPDATE ON person_device
WHEN NEW.person_uid IS NOT OLD.person_uid OR NEW.node_id IS NOT OLD.node_id
    OR typeof(OLD.revision) <> 'integer' OR OLD.revision <= 0
    OR typeof(NEW.revision) <> 'integer' OR NEW.revision <= OLD.revision
    OR typeof(OLD.revoked) <> 'integer' OR OLD.revoked NOT IN (0, 1)
    OR typeof(NEW.revoked) <> 'integer' OR NEW.revoked NOT IN (0, 1)
BEGIN
    SELECT RAISE(ABORT, 'Device revision must increase without changing identity');
END;

CREATE TRIGGER session_credential_insert
AFTER INSERT ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = NEW.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_insert
AFTER INSERT ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
        WHERE CAST(NEW.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(NEW.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_insert
AFTER INSERT ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_insert
AFTER INSERT ON organ_contact
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(NEW.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.uid AS uid WHERE NEW.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.uid AS uid WHERE NEW.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT NEW.uid WHERE NEW.kind = 'organ'
        )
    );
END;

CREATE TRIGGER session_credential_update
AFTER UPDATE ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = OLD.person_uid AND kind = 'person'
        )
        UNION
        SELECT NEW.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = NEW.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_update
AFTER UPDATE ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        WHERE CAST(OLD.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(OLD.record_uid AS TEXT) AND kind = 'person'
        )
        UNION
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
        WHERE CAST(NEW.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(NEW.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_update
AFTER UPDATE ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.organ_uid AS TEXT) AS uid
        UNION
        SELECT CAST(NEW.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_update
AFTER UPDATE ON organ_contact
WHEN NEW.record_uid IS NOT OLD.record_uid OR NEW.node_id IS NOT OLD.node_id OR NEW.trust IS NOT OLD.trust
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        UNION
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(OLD.node_id AS TEXT)))
        UNION
        SELECT lower(trim(CAST(NEW.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_update
BEFORE UPDATE ON record
WHEN NEW.uid IS NOT OLD.uid OR NEW.kind IS NOT OLD.kind OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'person'
        UNION
        SELECT NEW.uid AS uid WHERE NEW.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_update
BEFORE UPDATE ON record
WHEN NEW.uid IS NOT OLD.uid OR NEW.kind IS NOT OLD.kind OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'organ'
        UNION
        SELECT NEW.uid AS uid WHERE NEW.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT OLD.uid WHERE OLD.kind = 'organ'
            UNION
            SELECT NEW.uid WHERE NEW.kind = 'organ'
        )
    );
END;

CREATE TRIGGER session_credential_delete
AFTER DELETE ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = OLD.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_delete
AFTER DELETE ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        WHERE CAST(OLD.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(OLD.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_delete
AFTER DELETE ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_delete
AFTER DELETE ON organ_contact
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(OLD.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_delete
BEFORE DELETE ON record
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_delete
BEFORE DELETE ON record
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT OLD.uid WHERE OLD.kind = 'organ'
        )
    );
END;
