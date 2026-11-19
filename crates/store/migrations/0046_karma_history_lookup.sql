-- History selects one Rule and the newest applications. The existing primary
-- key starts with event_id and cannot serve that lookup. With rule_uid fixed,
-- reverse traversal of (at, implicit rowid) matches at DESC, rowid DESC.
CREATE INDEX karma_rule_application_history
    ON karma_rule_application(rule_uid, at);
