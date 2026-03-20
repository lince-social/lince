CREATE TABLE app_user (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE view_dependency (
    view_id INTEGER NOT NULL REFERENCES view(id) ON DELETE CASCADE,
    table_name TEXT NOT NULL,
    PRIMARY KEY (view_id, table_name)
);
