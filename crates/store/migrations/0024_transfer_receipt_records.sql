CREATE TABLE transfer_receipt_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    kind TEXT NOT NULL CHECK(kind IN ('authority','review','publication')),
    identity_hash TEXT NOT NULL CHECK(length(identity_hash)=71),
    source_origin TEXT,
    source_occurrence TEXT CHECK(source_occurrence IS NULL OR length(source_occurrence)=71),
    expires_at INTEGER NOT NULL CHECK(expires_at>0),
    created_at TEXT NOT NULL,
    UNIQUE(owner_person_uid,kind,identity_hash)
);
CREATE TABLE transfer_receipt_version (
    record_uid TEXT NOT NULL REFERENCES transfer_receipt_record(record_uid) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK(version>0),
    document_hash TEXT NOT NULL CHECK(length(document_hash)=71),
    document_json TEXT NOT NULL CHECK(length(CAST(document_json AS BLOB))<=800000),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    PRIMARY KEY(record_uid,version),
    UNIQUE(record_uid,document_hash)
);
CREATE TABLE transfer_receipt_request (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 256),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=850000),
    record_uid TEXT NOT NULL,
    version INTEGER NOT NULL,
    FOREIGN KEY(record_uid,version) REFERENCES transfer_receipt_version(record_uid,version) ON DELETE CASCADE
);
CREATE TABLE transfer_receipt_local_source (
    record_uid TEXT PRIMARY KEY REFERENCES transfer_receipt_record(record_uid) ON DELETE CASCADE,
    occurrence_uid TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    scope_request_json TEXT NOT NULL CHECK(length(CAST(scope_request_json AS BLOB))<=32768)
);
CREATE INDEX transfer_receipt_source ON transfer_receipt_record(source_origin,source_occurrence,owner_person_uid);
CREATE TRIGGER transfer_receipt_record_immutable BEFORE UPDATE ON transfer_receipt_record
BEGIN SELECT RAISE(ABORT,'Create a new receipt scope instead of rewriting its identity'); END;
CREATE TRIGGER transfer_receipt_version_immutable BEFORE UPDATE ON transfer_receipt_version
BEGIN SELECT RAISE(ABORT,'Append a receipt version instead of rewriting signed evidence'); END;
