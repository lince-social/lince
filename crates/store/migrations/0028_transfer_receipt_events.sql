CREATE TABLE transfer_receipt_event (
    record_uid TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>0),
    event_id TEXT NOT NULL CHECK(length(event_id)=71),
    source_key TEXT NOT NULL CHECK(length(source_key)=71),
    source_binding TEXT NOT NULL CHECK(length(source_binding)=71),
    slot_id TEXT NOT NULL CHECK(length(slot_id)=71),
    outcome_id TEXT NOT NULL CHECK(length(outcome_id)=71),
    semantic_hash TEXT NOT NULL CHECK(length(semantic_hash)=71),
    parent_outcome TEXT CHECK(parent_outcome IS NULL OR length(parent_outcome)=71),
    need_outcome TEXT CHECK(need_outcome IS NULL OR length(need_outcome)=71),
    issuer_organ TEXT NOT NULL,
    PRIMARY KEY(record_uid,version,event_id),
    FOREIGN KEY(record_uid,version) REFERENCES transfer_receipt_version(record_uid,version) ON DELETE CASCADE
);
CREATE INDEX transfer_receipt_event_source ON transfer_receipt_event(source_key,slot_id,outcome_id);
CREATE INDEX transfer_receipt_event_outcome ON transfer_receipt_event(outcome_id,semantic_hash);
CREATE TRIGGER transfer_receipt_event_immutable BEFORE UPDATE ON transfer_receipt_event
BEGIN SELECT RAISE(ABORT,'Index a new original receipt version instead of rewriting evidence'); END;
