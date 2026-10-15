CREATE TABLE knowledge_source (
    uid TEXT PRIMARY KEY,
    owner_key TEXT NOT NULL,
    name TEXT NOT NULL,
    package TEXT NOT NULL CHECK(json_valid(package) AND length(CAST(package AS BLOB))<=4194304),
    created_at TEXT NOT NULL
);
CREATE INDEX knowledge_source_owner ON knowledge_source(owner_key,uid);

CREATE TABLE knowledge_cache (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_key TEXT NOT NULL,
    source_uid TEXT NOT NULL,
    item_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    content TEXT NOT NULL CHECK(json_valid(content) AND length(CAST(content AS BLOB))<=4194304),
    fetched_at TEXT NOT NULL,
    accessed_at TEXT NOT NULL,
    UNIQUE(owner_key,source_uid,item_id,revision)
);
CREATE INDEX knowledge_cache_owner ON knowledge_cache(owner_key,accessed_at,sequence);

CREATE TABLE knowledge_reference (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    source_uid TEXT NOT NULL,
    item_id TEXT NOT NULL,
    record_uid TEXT NOT NULL UNIQUE REFERENCES record(uid) ON DELETE CASCADE,
    PRIMARY KEY(organ_uid,source_uid,item_id)
);

CREATE TABLE knowledge_mapping (
    owner_key TEXT NOT NULL,
    source_uid TEXT NOT NULL,
    external_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('predicate','unit')),
    concept_uid TEXT NOT NULL,
    definition_revision INTEGER NOT NULL,
    PRIMARY KEY(owner_key,source_uid,external_id,kind),
    FOREIGN KEY(concept_uid,definition_revision) REFERENCES concept_definition_version(concept_uid,revision)
);

CREATE TABLE knowledge_review (
    uid TEXT PRIMARY KEY,
    owner_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('save','update')),
    content TEXT NOT NULL CHECK(json_valid(content) AND length(CAST(content AS BLOB))<=16777216),
    created_at TEXT NOT NULL,
    applied TEXT CHECK(applied IS NULL OR json_valid(applied))
);
