CREATE TABLE transfer_private_policy_event (
    uid TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    record_uid TEXT NOT NULL REFERENCES record(uid),
    version INTEGER NOT NULL CHECK (version > 0),
    formula TEXT NOT NULL,
    policy_hash TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    UNIQUE (transfer_uid, exchange_uid, person_uid, version)
) STRICT;

CREATE TABLE transfer_private_policy (
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    event_uid TEXT NOT NULL UNIQUE REFERENCES transfer_private_policy_event(uid),
    PRIMARY KEY (transfer_uid, exchange_uid, person_uid)
) STRICT;

CREATE TRIGGER transfer_private_policy_event_immutable_update
BEFORE UPDATE ON transfer_private_policy_event
BEGIN
    SELECT RAISE(ABORT, 'private application policy events are immutable');
END;

CREATE TRIGGER transfer_private_policy_event_immutable_delete
BEFORE DELETE ON transfer_private_policy_event
BEGIN
    SELECT RAISE(ABORT, 'private application policy events are immutable');
END;

CREATE TABLE transfer_private_application_correction (
    uid TEXT PRIMARY KEY,
    application_uid TEXT NOT NULL UNIQUE REFERENCES transfer_local_application(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_private_application_correction_immutable_update
BEFORE UPDATE ON transfer_private_application_correction
BEGIN
    SELECT RAISE(ABORT, 'private application corrections are immutable');
END;

CREATE TRIGGER transfer_private_application_correction_immutable_delete
BEFORE DELETE ON transfer_private_application_correction
BEGIN
    SELECT RAISE(ABORT, 'private application corrections are immutable');
END;
