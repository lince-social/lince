CREATE TABLE evidence_custody_policy (
    owner_person_uid TEXT PRIMARY KEY REFERENCES record(uid),
    version INTEGER NOT NULL CHECK(version>0),
    record_uid TEXT NOT NULL REFERENCES record(uid),
    policy_json TEXT NOT NULL CHECK(json_valid(policy_json) AND length(CAST(policy_json AS BLOB))<=64000)
);
CREATE TABLE evidence_custody_payload (
    artifact_hash TEXT PRIMARY KEY CHECK(length(artifact_hash)=71),
    kind TEXT NOT NULL CHECK(kind IN ('receipt','contact','claim')),
    body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 800000),
    created_at INTEGER NOT NULL
);
CREATE TABLE evidence_custody_commitment (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    offer_hash TEXT NOT NULL UNIQUE CHECK(length(offer_hash)=71),
    record_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    sender_organ_uid TEXT NOT NULL REFERENCES record(uid),
    artifact_hash TEXT NOT NULL CHECK(length(artifact_hash)=71),
    byte_count INTEGER NOT NULL CHECK(byte_count BETWEEN 1 AND 800000),
    retained_until INTEGER NOT NULL,
    control_until INTEGER NOT NULL CHECK(control_until=retained_until+2592000),
    offer_json TEXT NOT NULL CHECK(json_valid(offer_json) AND length(CAST(offer_json AS BLOB))<=64000),
    acknowledgment_json TEXT NOT NULL CHECK(json_valid(acknowledgment_json) AND length(CAST(acknowledgment_json AS BLOB))<=64000),
    created_at INTEGER NOT NULL
);
CREATE INDEX evidence_custody_owner ON evidence_custody_commitment(owner_person_uid,sequence DESC);
CREATE INDEX evidence_custody_artifact ON evidence_custody_commitment(artifact_hash,retained_until);
CREATE TRIGGER evidence_custody_payload_immutable BEFORE UPDATE ON evidence_custody_payload
BEGIN SELECT RAISE(ABORT,'Retained original bytes are immutable'); END;
CREATE TRIGGER evidence_custody_commitment_immutable BEFORE UPDATE ON evidence_custody_commitment
BEGIN SELECT RAISE(ABORT,'A retention acknowledgment is immutable'); END;
