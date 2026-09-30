ALTER TABLE transfer_private_policy_event ADD COLUMN effects_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(effects_json));

CREATE TABLE transfer_private_effect (
    fact_uid TEXT PRIMARY KEY REFERENCES fact(uid),
    primary_fact_uid TEXT NOT NULL REFERENCES fact(uid),
    occurrence_uid TEXT NOT NULL,
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    record_uid TEXT NOT NULL REFERENCES record(uid),
    mode TEXT NOT NULL CHECK (mode IN ('quantity','fulfilment')),
    unit_uid TEXT REFERENCES concept(uid),
    formula TEXT NOT NULL,
    review_hash TEXT NOT NULL,
    UNIQUE (primary_fact_uid, record_uid)
) STRICT;

CREATE INDEX transfer_private_effect_occurrence ON transfer_private_effect(occurrence_uid, person_uid, record_uid);
CREATE INDEX transfer_private_effect_group ON transfer_private_effect(primary_fact_uid);

CREATE TRIGGER transfer_private_effect_immutable_update
BEFORE UPDATE ON transfer_private_effect
BEGIN
    SELECT RAISE(ABORT, 'private effect evidence is immutable');
END;

CREATE TRIGGER transfer_private_effect_immutable_delete
BEFORE DELETE ON transfer_private_effect
BEGIN
    SELECT RAISE(ABORT, 'private effect evidence is immutable');
END;
