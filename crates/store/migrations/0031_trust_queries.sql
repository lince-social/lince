CREATE TABLE trust_query_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    policy_json TEXT NOT NULL CHECK(length(CAST(policy_json AS BLOB))<=65536),
    policy_hash TEXT NOT NULL CHECK(length(policy_hash)=71),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    UNIQUE(owner_person_uid,policy_hash)
);
CREATE TABLE trust_query_request (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 256),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=70000),
    record_uid TEXT NOT NULL REFERENCES trust_query_record(record_uid)
);
CREATE INDEX trust_query_owner ON trust_query_record(owner_person_uid,record_uid);
CREATE TRIGGER trust_query_record_immutable BEFORE UPDATE ON trust_query_record
BEGIN SELECT RAISE(ABORT,'Save a new reader policy instead of rewriting its original'); END;
CREATE TRIGGER trust_query_request_immutable BEFORE UPDATE ON trust_query_request
BEGIN SELECT RAISE(ABORT,'Trust query request bindings are immutable'); END;
