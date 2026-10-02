CREATE TABLE social_discovery_source (
    post TEXT NOT NULL,
    hash TEXT NOT NULL,
    source TEXT NOT NULL CHECK(length(CAST(source AS BLOB))<=128),
    checked_at INTEGER NOT NULL,
    PRIMARY KEY(post,hash,source)
);
CREATE INDEX social_discovery_source_age ON social_discovery_source(checked_at,post,hash,source);
CREATE INDEX social_discovery_conflict_page ON social_document(kind,state,id);
CREATE TABLE social_discovery_conflict (
    post TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    first_hash TEXT NOT NULL,
    first_body TEXT NOT NULL,
    second_hash TEXT NOT NULL,
    second_body TEXT NOT NULL,
    observed_at INTEGER NOT NULL
);
