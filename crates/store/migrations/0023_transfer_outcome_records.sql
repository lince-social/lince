CREATE TABLE transfer_outcome_record (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    occurrence_uid TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    author_person_uid TEXT NOT NULL REFERENCES record(uid),
    kind TEXT NOT NULL CHECK(kind IN ('need_binding','giver_statement','receiver_statement')),
    measurement_key TEXT NOT NULL CHECK(length(measurement_key)=64),
    supersedes_uid TEXT REFERENCES transfer_outcome_record(record_uid),
    need_binding_uid TEXT REFERENCES transfer_outcome_record(record_uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE CHECK(length(request_id) BETWEEN 1 AND 256),
    request_json TEXT NOT NULL CHECK(length(CAST(request_json AS BLOB))<=32768),
    payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB))<=32768)
);
CREATE INDEX transfer_outcome_history ON transfer_outcome_record(occurrence_uid,kind,measurement_key);
CREATE TRIGGER transfer_outcome_immutable BEFORE UPDATE ON transfer_outcome_record
BEGIN SELECT RAISE(ABORT,'Append an outcome correction instead of rewriting evidence'); END;
CREATE TRIGGER transfer_outcome_keep BEFORE DELETE ON transfer_outcome_record
BEGIN SELECT RAISE(ABORT,'Outcome history cannot be removed'); END;
