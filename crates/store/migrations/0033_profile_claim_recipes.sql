CREATE TABLE profile_claim_recipe_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    recipe_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision>0),
    recipe_hash TEXT NOT NULL CHECK(length(recipe_hash)=71),
    recipe_json TEXT NOT NULL CHECK(json_valid(recipe_json) AND length(CAST(recipe_json AS BLOB))<=98304),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    UNIQUE(owner_person_uid,recipe_uid,revision),
    UNIQUE(owner_person_uid,recipe_hash)
);
CREATE INDEX profile_claim_recipe_owner ON profile_claim_recipe_record(owner_person_uid,recipe_uid,revision DESC);
CREATE TABLE profile_claim_recipe_request (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 256),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    request_json TEXT NOT NULL CHECK(json_valid(request_json) AND length(CAST(request_json AS BLOB))<=131072),
    record_uid TEXT NOT NULL REFERENCES profile_claim_recipe_record(record_uid)
);
CREATE TABLE profile_claim_recipe_control (
    record_uid TEXT NOT NULL REFERENCES profile_claim_recipe_record(record_uid),
    revision INTEGER NOT NULL CHECK(revision>=0),
    mode TEXT NOT NULL CHECK(mode IN ('enabled','paused','revoked')),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE,
    PRIMARY KEY(record_uid,revision)
);
CREATE TABLE profile_claim_recipe_runtime (
    record_uid TEXT PRIMARY KEY REFERENCES profile_claim_recipe_record(record_uid),
    status TEXT NOT NULL,
    checked_at TEXT,
    next_at TEXT,
    input_hash TEXT CHECK(input_hash IS NULL OR length(input_hash)=71),
    output_record_uid TEXT REFERENCES profile_claim_record(record_uid)
);
ALTER TABLE profile_claim_record ADD COLUMN generation_recipe_uid TEXT REFERENCES profile_claim_recipe_record(record_uid);
CREATE TRIGGER profile_claim_recipe_record_immutable BEFORE UPDATE ON profile_claim_recipe_record
BEGIN SELECT RAISE(ABORT,'Review a new recipe version instead of rewriting its authorization'); END;
CREATE TRIGGER profile_claim_recipe_request_immutable BEFORE UPDATE ON profile_claim_recipe_request
BEGIN SELECT RAISE(ABORT,'Recipe request bindings are immutable'); END;
CREATE TRIGGER profile_claim_recipe_control_immutable BEFORE UPDATE ON profile_claim_recipe_control
BEGIN SELECT RAISE(ABORT,'Append a signed recipe control instead of rewriting it'); END;
