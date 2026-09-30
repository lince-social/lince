CREATE TABLE transfer_child_requirement (
    parent_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    child_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    required INTEGER NOT NULL CHECK (required IN (0, 1)),
    PRIMARY KEY (parent_uid, child_uid)
);

CREATE INDEX transfer_parent_children ON transfer(parent_uid, record_uid);

CREATE TABLE transfer_child_request (
    request_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
);

CREATE TRIGGER transfer_child_request_immutable_update BEFORE UPDATE ON transfer_child_request
BEGIN SELECT RAISE(ABORT, 'Transfer child requests are immutable'); END;
CREATE TRIGGER transfer_child_request_immutable_delete BEFORE DELETE ON transfer_child_request
BEGIN SELECT RAISE(ABORT, 'Transfer child requests are immutable'); END;
