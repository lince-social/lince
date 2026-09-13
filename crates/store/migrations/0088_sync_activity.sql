CREATE TABLE sync_activity (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    activity TEXT NOT NULL,
    summary TEXT NOT NULL
);
CREATE INDEX sync_activity_at ON sync_activity(at);

CREATE TABLE sync_history_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    seconds INTEGER NOT NULL CHECK (seconds BETWEEN 60 AND 7776000),
    max_entries INTEGER NOT NULL CHECK (max_entries BETWEEN 1 AND 10000)
);
INSERT INTO sync_history_policy VALUES (1, 604800, 1000);
