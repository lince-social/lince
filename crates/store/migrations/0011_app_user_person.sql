-- An authenticated app user and a social Person are deliberately different
-- identities. This explicit one-to-one binding prevents clients from claiming
-- an arbitrary Person uid when authoring Transfer commitments.
CREATE TABLE app_user_person (
    user_id     INTEGER PRIMARY KEY REFERENCES app_user(id) ON DELETE CASCADE,
    person_uid  TEXT NOT NULL UNIQUE REFERENCES record(uid),
    assigned_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

