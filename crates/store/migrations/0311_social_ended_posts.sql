CREATE TABLE social_ended_post (
    id TEXT PRIMARY KEY,
    authority TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    hash TEXT NOT NULL
);
