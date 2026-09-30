CREATE TABLE karma_rule_evidence (
    event_id TEXT NOT NULL,
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    attempt INTEGER NOT NULL,
    evidence TEXT NOT NULL CHECK(json_valid(evidence)),
    PRIMARY KEY(event_id, rule_uid, rule_revision, attempt)
) STRICT;
