CREATE TABLE transfer_draft_discard (
    request_id TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL UNIQUE REFERENCES transfer(record_uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    expected_revision INTEGER NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
);
