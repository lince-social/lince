CREATE TABLE mailbox_outbox_scope (
    uid TEXT PRIMARY KEY REFERENCES mailbox_outbox(uid) ON DELETE CASCADE,
    scope_json TEXT NOT NULL CHECK(json_valid(scope_json) AND length(CAST(scope_json AS BLOB))<=262144)
);
CREATE TRIGGER mailbox_outbox_scope_immutable BEFORE UPDATE ON mailbox_outbox_scope
BEGIN SELECT RAISE(ABORT,'Prepare a new delivery instead of changing saved mail scope'); END;
