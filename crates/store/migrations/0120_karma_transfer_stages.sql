CREATE TABLE karma_transfer_stage (
    boundary_uid TEXT PRIMARY KEY REFERENCES karma_schedule_boundary(uid),
    parent_rule_uid TEXT NOT NULL,
    parent_revision INTEGER NOT NULL,
    position INTEGER NOT NULL,
    change_uid TEXT NOT NULL,
    origin TEXT NOT NULL CHECK(json_valid(origin)),
    UNIQUE(parent_rule_uid, parent_revision, position, change_uid)
) STRICT;
