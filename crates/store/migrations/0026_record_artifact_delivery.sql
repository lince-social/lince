CREATE TABLE record_artifact_delivery (
    message_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    request_id TEXT NOT NULL UNIQUE CHECK(length(request_id) BETWEEN 1 AND 256),
    request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=65536),
    artifact_json TEXT NOT NULL CHECK(length(CAST(artifact_json AS BLOB))<=850000),
    artifact_hash TEXT NOT NULL CHECK(length(artifact_hash)=71),
    recipients_json TEXT NOT NULL CHECK(length(CAST(recipients_json AS BLOB))<=8192),
    source_record_uid TEXT REFERENCES record(uid),
    source_version INTEGER,
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    expires_at INTEGER NOT NULL CHECK(expires_at>0),
    cancelled_at TEXT,
    CHECK((source_record_uid IS NULL AND source_version IS NULL) OR
          (source_record_uid IS NOT NULL AND source_version>0))
);
CREATE INDEX record_artifact_delivery_owner ON record_artifact_delivery(owner_person_uid,expires_at);
CREATE TRIGGER record_artifact_delivery_immutable BEFORE UPDATE OF
    message_uid,owner_person_uid,request_id,request_json,artifact_json,artifact_hash,
    recipients_json,source_record_uid,source_version,fact_uid,expires_at
ON record_artifact_delivery
BEGIN SELECT RAISE(ABORT,'Send a new reviewed artifact instead of rewriting a delivery'); END;
