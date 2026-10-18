CREATE TABLE transfer_receipt_event_reference (
    record_uid TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>0),
    event_id TEXT NOT NULL CHECK(length(event_id)=71),
    scope_id TEXT NOT NULL CHECK(length(scope_id)=71),
    source_key TEXT NOT NULL CHECK(length(source_key)=71),
    source_commitment TEXT NOT NULL CHECK(length(source_commitment)=71),
    slot_id TEXT NOT NULL CHECK(length(slot_id)=71),
    parent_event TEXT CHECK(parent_event IS NULL OR length(parent_event)=71),
    need_event TEXT CHECK(need_event IS NULL OR length(need_event)=71),
    issuer_organ TEXT NOT NULL,
    PRIMARY KEY(record_uid,version,event_id),
    FOREIGN KEY(record_uid,version) REFERENCES transfer_receipt_version(record_uid,version) ON DELETE CASCADE
);
CREATE INDEX transfer_receipt_reference_event ON transfer_receipt_event_reference(scope_id,event_id);
CREATE INDEX transfer_receipt_reference_source ON transfer_receipt_event_reference(scope_id,source_commitment,slot_id);
CREATE INDEX transfer_receipt_reference_slot ON transfer_receipt_event_reference(source_key,slot_id,event_id);
CREATE TRIGGER transfer_receipt_event_reference_immutable BEFORE UPDATE ON transfer_receipt_event_reference
BEGIN SELECT RAISE(ABORT,'Append original proof artifacts instead of rewriting their references'); END;
