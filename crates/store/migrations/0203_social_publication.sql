CREATE TABLE social_document (
    kind TEXT NOT NULL CHECK(kind IN ('snippet','profile')),
    id TEXT NOT NULL,
    authority TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    hash TEXT NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL,
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    direction TEXT NOT NULL,
    language TEXT NOT NULL,
    area TEXT NOT NULL,
    concept TEXT NOT NULL,
    unit TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY(kind,id)
);
CREATE INDEX social_document_expiry ON social_document(expires_at);
CREATE VIRTUAL TABLE social_search USING fts5(id UNINDEXED, title, text);
CREATE TABLE social_revision (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(kind,id,hash)
);
CREATE TABLE social_publication_job (
    hash TEXT NOT NULL,
    destination TEXT NOT NULL,
    body TEXT NOT NULL,
    kind TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','failed','expired','cancelled')),
    error TEXT,
    PRIMARY KEY(hash,destination)
);
