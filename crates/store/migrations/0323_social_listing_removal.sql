CREATE TABLE social_listing_removal (
    post TEXT PRIMARY KEY,
    reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB))<=2048),
    removed_at INTEGER NOT NULL
);
