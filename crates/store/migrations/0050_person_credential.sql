-- One human reference.
--
-- `app_user` and `Person` were two identities for one human, joined by
-- `app_user_person` — a table whose `user_id` was PRIMARY KEY and whose
-- `person_uid` was UNIQUE. Unique on both sides is a bijection, which means
-- the split was never modelling two things; it was one thing wearing a
-- disguise, and it cost real behaviour: `subject` on a transport session came
-- to mean "numeric app user id" on the websocket driver and "Person uid" on
-- the iroh live driver, so a live guest could read but could never write
-- (`begin_action_intent_session` parsed its subject as an integer and failed).
--
-- The original justification (migration 0011) was that the split "prevents
-- clients from claiming an arbitrary Person uid". It does not: what prevents
-- that is the server resolving identity from the authenticated session and
-- never reading it from the client frame, which is what the code already does
-- and continues to do here.
--
-- So: the Person IS the human. A credential is merely a way to prove you are
-- one of them over HTTP, and lives here — LOCAL AND NEVER SYNCED, because
-- Person records sync to contacts and a password hash must not. Note
-- `organ_login` (migration 0048) was already built this way, keyed straight to
-- a Person with no app_user anywhere; this finishes the move it started.
CREATE TABLE person_credential (
    -- NOT NULL is deliberate and load-bearing: SQLite permits NULL in a
    -- non-INTEGER PRIMARY KEY, and the backfill guard below relies on this
    -- rejecting one.
    person_uid    TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    username      TEXT NOT NULL UNIQUE CHECK (length(trim(username)) > 0),
    password_hash TEXT NOT NULL CHECK (length(trim(password_hash)) > 0),
    role_id       INTEGER REFERENCES role(id) CHECK (role_id IS NULL OR role_id > 0),
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

-- Carry over every login that already had a Person.
INSERT INTO person_credential (person_uid, username, password_hash, role_id, created_at)
SELECT b.person_uid, u.username, u.password_hash, u.role_id, u.created_at
FROM app_user u
JOIN app_user_person b ON b.user_id = u.id;

-- Refuse to migrate a store where some login has NO Person: there is no
-- correct uid to invent for it here, and dropping `app_user` would silently
-- delete a real account. This SELECT yields a NULL person_uid for exactly
-- those rows and the NOT NULL above aborts the whole migration, loudly, with
-- the store untouched. It inserts nothing when every login is bound.
INSERT INTO person_credential (person_uid, username, password_hash, role_id)
SELECT NULL, u.username, u.password_hash, u.role_id
FROM app_user u
LEFT JOIN app_user_person b ON b.user_id = u.id
WHERE b.person_uid IS NULL;

DROP TABLE app_user_person;
DROP TABLE app_user;
