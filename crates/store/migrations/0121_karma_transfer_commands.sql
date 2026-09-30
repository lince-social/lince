CREATE TABLE karma_transfer_command (
    command_uid TEXT PRIMARY KEY REFERENCES transfer_remote_command(command_uid),
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    origin TEXT NOT NULL CHECK(json_valid(origin)),
    cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0, 1)),
    reason TEXT,
    dispatched_at TEXT
) STRICT;
CREATE INDEX karma_transfer_command_rule ON karma_transfer_command(rule_uid, rule_revision);
