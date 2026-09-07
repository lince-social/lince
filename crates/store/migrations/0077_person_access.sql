CREATE TABLE person_access (
    person_uid TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    role_id INTEGER REFERENCES role(id) CHECK (role_id IS NULL OR role_id > 0),
    read_filter TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)
) STRICT;

INSERT INTO person_access (person_uid, role_id, read_filter)
SELECT credential.person_uid, credential.role_id, credential.read_filter
FROM person_credential AS credential
JOIN record AS person ON person.uid = credential.person_uid
WHERE person.kind = 'person' AND person.deleted_at IS NULL;

ALTER TABLE person_credential DROP COLUMN read_filter;
ALTER TABLE person_credential DROP COLUMN role_id;
