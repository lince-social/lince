CREATE TABLE contact_disclosure_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    relation_hash TEXT NOT NULL CHECK(length(relation_hash)=71),
    statement_hash TEXT NOT NULL CHECK(length(statement_hash)=71),
    document_hash TEXT NOT NULL CHECK(length(document_hash)=71),
    issuer_kind TEXT NOT NULL CHECK(issuer_kind IN ('person','organ')),
    issuer_uid TEXT NOT NULL,
    contact_kind TEXT NOT NULL CHECK(contact_kind IN ('person','organ')),
    contact_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision>0),
    issued_at INTEGER NOT NULL CHECK(issued_at>0),
    expires_at INTEGER NOT NULL CHECK(expires_at>issued_at AND expires_at-issued_at<=31622400),
    withdrawn INTEGER NOT NULL CHECK(withdrawn IN (0,1)),
    document_json TEXT NOT NULL CHECK(json_valid(document_json) AND length(CAST(document_json AS BLOB))<=32768),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    CHECK(issuer_uid<>contact_uid),
    UNIQUE(owner_person_uid,document_hash)
);
CREATE INDEX contact_disclosure_relation ON contact_disclosure_record(relation_hash,issuer_uid,revision DESC);
CREATE INDEX contact_disclosure_owner ON contact_disclosure_record(owner_person_uid,record_uid);
CREATE TABLE contact_disclosure_request (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 256),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    request_json TEXT NOT NULL CHECK(json_valid(request_json) AND length(CAST(request_json AS BLOB))<=65536),
    record_uid TEXT NOT NULL REFERENCES contact_disclosure_record(record_uid)
);
CREATE TRIGGER contact_disclosure_record_immutable BEFORE UPDATE ON contact_disclosure_record
BEGIN SELECT RAISE(ABORT,'Append a signed contact disclosure instead of rewriting it'); END;
CREATE TRIGGER contact_disclosure_request_immutable BEFORE UPDATE ON contact_disclosure_request
BEGIN SELECT RAISE(ABORT,'Contact disclosure requests retain their original binding'); END;
