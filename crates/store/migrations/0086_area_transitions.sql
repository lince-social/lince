CREATE TABLE interface_area_transition (
    request_id TEXT PRIMARY KEY NOT NULL,
    actor_uid TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);
