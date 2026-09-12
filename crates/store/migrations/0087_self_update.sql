CREATE TABLE self_update (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    automatic INTEGER NOT NULL DEFAULT 0 CHECK (automatic IN (0, 1)),
    installed_revision TEXT,
    installed_at TEXT
);
INSERT INTO self_update (id) VALUES (1);
