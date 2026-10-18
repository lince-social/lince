CREATE TABLE organ_root_recognition (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 64),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    public_key TEXT NOT NULL CHECK(length(public_key)=44),
    review_hash TEXT NOT NULL CHECK(length(review_hash)=71),
    review_json TEXT NOT NULL CHECK(json_valid(review_json) AND length(CAST(review_json AS BLOB))<=32768),
    request_id TEXT NOT NULL UNIQUE CHECK(length(request_id) BETWEEN 1 AND 256),
    request_json TEXT NOT NULL CHECK(json_valid(request_json) AND length(CAST(request_json AS BLOB))<=65536),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    UNIQUE(organ_uid,revision)
);
CREATE INDEX organ_root_recognition_current ON organ_root_recognition(organ_uid,revision DESC);
CREATE TRIGGER organ_root_recognition_immutable_update BEFORE UPDATE ON organ_root_recognition
BEGIN SELECT RAISE(ABORT,'Append a reviewed root recognition instead of replacing its history'); END;
CREATE TRIGGER organ_root_recognition_immutable_delete BEFORE DELETE ON organ_root_recognition
BEGIN SELECT RAISE(ABORT,'Root recognition history must be retained'); END;
