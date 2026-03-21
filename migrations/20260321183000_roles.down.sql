CREATE TABLE app_user_old (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO app_user_old(id, name, username, password_hash, created_at, updated_at)
SELECT id, name, username, password_hash, created_at, updated_at
FROM app_user;

DROP TABLE app_user;

ALTER TABLE app_user_old RENAME TO app_user;

DROP TABLE role;
