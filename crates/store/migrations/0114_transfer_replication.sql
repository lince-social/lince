CREATE TABLE transfer_sync_control (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    importing INTEGER NOT NULL DEFAULT 0 CHECK (importing IN (0, 1))
) STRICT;
INSERT INTO transfer_sync_control(id) VALUES (1);

CREATE TABLE transfer_sync_journal (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    commit_sequence INTEGER NOT NULL,
    table_name TEXT NOT NULL,
    before_json TEXT CHECK (before_json IS NULL OR json_valid(before_json)),
    after_json TEXT CHECK (after_json IS NULL OR json_valid(after_json))
) STRICT;
CREATE INDEX transfer_sync_journal_commit ON transfer_sync_journal(cell_uid, commit_sequence, seq);

CREATE TABLE transfer_sync_message (
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    journal_end INTEGER,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    PRIMARY KEY (organ_uid, cell_uid, sequence)
) STRICT;

CREATE TRIGGER transfer_sync_message_no_update
BEFORE UPDATE ON transfer_sync_message BEGIN
    SELECT RAISE(ABORT, 'signed Transfer sync messages are immutable');
END;
CREATE TRIGGER transfer_sync_message_no_delete
BEFORE DELETE ON transfer_sync_message BEGIN
    SELECT RAISE(ABORT, 'signed Transfer sync messages are immutable');
END;

CREATE TABLE transfer_sync_owner (
    table_name TEXT NOT NULL,
    row_key TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    PRIMARY KEY (table_name, row_key)
) STRICT;

DROP TRIGGER transfer_occurrence_settlement_compensation_matches_slice;
CREATE TRIGGER transfer_occurrence_settlement_compensation_matches_slice
BEFORE INSERT ON transfer_occurrence_settlement_compensation
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence_settlement_slice slice
    JOIN fact correction ON correction.uid = NEW.compensation_fact_uid
    WHERE slice.uid = NEW.settlement_uid
      AND slice.occurrence_uid = NEW.occurrence_uid
      AND slice.transfer_uid = NEW.transfer_uid
      AND slice.owner_person_uid = NEW.owner_person_uid
      AND slice.application_fact_uid = NEW.original_application_fact_uid
      AND slice.local_record_uid = NEW.local_record_uid
      AND NEW.inverse_delta = -slice.local_delta
      AND correction.record_uid = NEW.local_record_uid
      AND CAST(correction.delta_mantissa AS REAL)
          / CAST(SUBSTR('1000000000000000000', 1, correction.delta_scale + 1) AS REAL)
          = NEW.inverse_delta
      AND correction.actor_uid = NEW.owner_person_uid
      AND ((correction.cause_kind = 'compensation' AND correction.cause_uid = NEW.original_application_fact_uid) OR EXISTS (SELECT 1 FROM fact_origin o JOIN record own ON own.uid = o.organ_uid AND own.slug = 'local-organ' WHERE o.fact_uid = correction.uid AND json_extract(o.payload, '$.cause.kind') = 'compensation' AND json_extract(o.payload, '$.cause.uid') = NEW.original_application_fact_uid))
)
BEGIN
    SELECT RAISE(ABORT, 'settlement compensation does not match its slice');
END;
