CREATE TABLE profile_claim_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    series_hash TEXT NOT NULL CHECK(length(series_hash)=71),
    statement_hash TEXT NOT NULL CHECK(length(statement_hash)=71),
    document_hash TEXT NOT NULL CHECK(length(document_hash)=71),
    issuer_kind TEXT NOT NULL CHECK(issuer_kind IN ('person','organ')),
    issuer_uid TEXT NOT NULL,
    claim_uid TEXT NOT NULL,
    subject_kind TEXT NOT NULL CHECK(subject_kind IN ('person','organ')),
    subject_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision>0),
    document_json TEXT NOT NULL CHECK(json_valid(document_json) AND length(CAST(document_json AS BLOB))<=32768),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    UNIQUE(owner_person_uid,document_hash)
);
CREATE INDEX profile_claim_series ON profile_claim_record(owner_person_uid,series_hash,revision DESC);
CREATE INDEX profile_claim_subject ON profile_claim_record(subject_kind,subject_uid,record_uid);
CREATE TABLE profile_claim_request (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 256),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    request_json TEXT NOT NULL CHECK(json_valid(request_json) AND length(CAST(request_json AS BLOB))<=65536),
    record_uid TEXT NOT NULL REFERENCES profile_claim_record(record_uid)
);
CREATE TRIGGER profile_claim_record_immutable BEFORE UPDATE ON profile_claim_record
BEGIN SELECT RAISE(ABORT,'Append a signed claim revision instead of rewriting it'); END;
CREATE TRIGGER profile_claim_request_immutable BEFORE UPDATE ON profile_claim_request
BEGIN SELECT RAISE(ABORT,'Profile claim requests retain their original fields'); END;
