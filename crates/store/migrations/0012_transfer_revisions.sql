-- Transfer terms are signed as immutable, monotonically numbered revisions.
-- Existing transfers predate that contract and deliberately remain revision 0;
-- callers must explicitly re-propose them before accepting new agreements.
ALTER TABLE transfer
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0);

ALTER TABLE transfer_agreement
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0);

ALTER TABLE promise
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0);

CREATE TABLE transfer_revision (
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, revision),
    UNIQUE (idempotency_key),
    CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) > 0)
) STRICT;
