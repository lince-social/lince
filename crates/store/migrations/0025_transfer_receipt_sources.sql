CREATE TABLE transfer_receipt_source_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    source_key TEXT NOT NULL CHECK(length(source_key)=71),
    author_person_uid TEXT NOT NULL REFERENCES record(uid),
    kind TEXT NOT NULL CHECK(kind IN ('need_binding','giver_statement','receiver_statement')),
    measurement_key TEXT NOT NULL,
    supersedes_uid TEXT REFERENCES record(uid),
    need_binding_uid TEXT REFERENCES record(uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE CHECK(length(request_id) BETWEEN 1 AND 256),
    request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=65536),
    payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB))<=32768),
    created_at TEXT NOT NULL
);
CREATE INDEX transfer_receipt_source_history ON transfer_receipt_source_record(author_person_uid,source_key,created_at);
CREATE UNIQUE INDEX transfer_receipt_source_root ON transfer_receipt_source_record(source_key,author_person_uid,kind,measurement_key) WHERE supersedes_uid IS NULL;
CREATE TRIGGER transfer_receipt_source_immutable BEFORE UPDATE ON transfer_receipt_source_record
BEGIN SELECT RAISE(ABORT,'Append a source snapshot instead of rewriting signed evidence'); END;
