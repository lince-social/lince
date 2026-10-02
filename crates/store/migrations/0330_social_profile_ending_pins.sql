CREATE TABLE social_profile_authority_next (
    organ TEXT PRIMARY KEY,
    root_key TEXT NOT NULL,
    editor_key TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation>=0 AND (generation>0 OR editor_key='')),
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=8192),
    expires_at INTEGER NOT NULL,
    profile_revision INTEGER NOT NULL DEFAULT 0,
    profile_hash TEXT
);
INSERT INTO social_profile_authority_next(organ,root_key,editor_key,generation,body,expires_at,profile_revision,profile_hash)
    SELECT organ,root_key,editor_key,generation,body,expires_at,profile_revision,profile_hash FROM social_profile_authority;
DROP TABLE social_profile_authority;
ALTER TABLE social_profile_authority_next RENAME TO social_profile_authority;
