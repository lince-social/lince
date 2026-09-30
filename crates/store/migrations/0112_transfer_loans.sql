CREATE TABLE transfer_loan_agreement (
    transfer_uid TEXT NOT NULL REFERENCES record(uid),
    exchange_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL CHECK (until_ms > from_ms),
    agreement_fact_uid TEXT NOT NULL REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, exchange_uid, revision)
) STRICT;

CREATE TABLE transfer_loan_extension (
    request_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
) STRICT;

CREATE TRIGGER transfer_loan_extension_immutable_update
BEFORE UPDATE ON transfer_loan_extension
BEGIN
    SELECT RAISE(ABORT, 'loan extension proposals are immutable');
END;

CREATE TRIGGER transfer_loan_extension_immutable_delete
BEFORE DELETE ON transfer_loan_extension
BEGIN
    SELECT RAISE(ABORT, 'loan extension proposals are immutable');
END;

CREATE TRIGGER transfer_loan_agreement_immutable_update
BEFORE UPDATE ON transfer_loan_agreement
BEGIN
    SELECT RAISE(ABORT, 'accepted loan terms are immutable');
END;

CREATE TRIGGER transfer_loan_agreement_immutable_delete
BEFORE DELETE ON transfer_loan_agreement
BEGIN
    SELECT RAISE(ABORT, 'accepted loan terms are immutable');
END;
