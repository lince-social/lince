CREATE TABLE record_evidence_forwarding (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    record_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    source_record_uid TEXT NOT NULL REFERENCES record(uid),
    source_mode TEXT NOT NULL CHECK(source_mode IN ('serving','custody')),
    policy_version INTEGER NOT NULL CHECK(policy_version>0),
    artifact_hash TEXT NOT NULL CHECK(length(artifact_hash)=71),
    series TEXT NOT NULL CHECK(length(series) BETWEEN 1 AND 256),
    control INTEGER NOT NULL CHECK(control IN (0,1)),
    peer_organ_uid TEXT NOT NULL REFERENCES record(uid),
    endpoint TEXT NOT NULL CHECK(length(endpoint) BETWEEN 1 AND 256),
    offer_hash TEXT NOT NULL UNIQUE CHECK(length(offer_hash)=71),
    offer_json TEXT NOT NULL CHECK(json_valid(offer_json) AND length(CAST(offer_json AS BLOB))<=64000),
    state TEXT NOT NULL CHECK(state IN ('pending','sending','stored','held','failed','cancelled','expired')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
    sent_bytes INTEGER NOT NULL DEFAULT 0 CHECK(sent_bytes BETWEEN 0 AND 800000),
    next_attempt INTEGER NOT NULL,
    retained_until INTEGER NOT NULL,
    acknowledgment_json TEXT CHECK(acknowledgment_json IS NULL OR (json_valid(acknowledgment_json) AND length(CAST(acknowledgment_json AS BLOB))<=64000)),
    error TEXT CHECK(error IS NULL OR length(error)<=500),
    created_at INTEGER NOT NULL
);
CREATE INDEX record_evidence_forwarding_pending ON record_evidence_forwarding(state,control DESC,next_attempt,sequence);
CREATE INDEX record_evidence_forwarding_original ON record_evidence_forwarding(owner_person_uid,artifact_hash,peer_organ_uid);
CREATE TRIGGER record_evidence_forwarding_scope_immutable BEFORE UPDATE OF record_uid,owner_person_uid,source_record_uid,source_mode,policy_version,artifact_hash,series,control,peer_organ_uid,endpoint,offer_hash,offer_json,retained_until,created_at ON record_evidence_forwarding
BEGIN SELECT RAISE(ABORT,'Review a new evidence delivery instead of expanding its original scope'); END;
