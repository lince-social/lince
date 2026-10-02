CREATE TABLE social_public_asset (
    hash TEXT PRIMARY KEY CHECK(length(hash)=64),
    bytes BLOB NOT NULL CHECK(length(bytes)<=131072),
    width INTEGER NOT NULL CHECK(width BETWEEN 1 AND 2048),
    height INTEGER NOT NULL CHECK(height BETWEEN 1 AND 2048),
    touched_at INTEGER NOT NULL
);
