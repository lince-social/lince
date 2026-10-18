CREATE TABLE record_evidence_serving (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    record_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    source_record_uid TEXT NOT NULL REFERENCES record(uid),
    source_kind TEXT NOT NULL CHECK(source_kind IN ('receipt','contact','claim')),
    source_version INTEGER NOT NULL CHECK(source_version>0),
    artifact_hash TEXT NOT NULL CHECK(length(artifact_hash)=71),
    expires_at INTEGER NOT NULL,
    review_hash TEXT NOT NULL CHECK(length(review_hash)=71),
    review_json TEXT NOT NULL CHECK(json_valid(review_json) AND length(CAST(review_json AS BLOB))<=900000),
    request_id TEXT NOT NULL UNIQUE CHECK(length(request_id) BETWEEN 1 AND 256),
    request_json TEXT NOT NULL CHECK(json_valid(request_json) AND length(CAST(request_json AS BLOB))<=950000),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL
);
CREATE INDEX record_evidence_serving_source ON record_evidence_serving(owner_person_uid,source_record_uid,source_kind,sequence DESC);
CREATE INDEX record_evidence_serving_artifact ON record_evidence_serving(artifact_hash,sequence DESC);
CREATE TRIGGER record_evidence_serving_immutable_update BEFORE UPDATE ON record_evidence_serving
BEGIN SELECT RAISE(ABORT,'Review a new evidence serving permission instead of rewriting it'); END;
CREATE TRIGGER record_evidence_serving_immutable_delete BEFORE DELETE ON record_evidence_serving
BEGIN SELECT RAISE(ABORT,'Evidence serving control history must be retained'); END;
