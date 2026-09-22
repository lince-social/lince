CREATE TABLE karma_field (
    uid TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('condition', 'threshold', 'consequence')),
    source TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE karma_field_binding (
    rule_uid TEXT NOT NULL REFERENCES recurrence(uid) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    field_uid TEXT NOT NULL REFERENCES karma_field(uid),
    PRIMARY KEY (rule_uid, kind)
);
CREATE INDEX karma_field_readers ON karma_field_binding(field_uid);
CREATE TABLE karma_editor_request (
    request_id TEXT PRIMARY KEY,
    result_uid TEXT NOT NULL
);
INSERT INTO karma_field (uid, kind, source)
SELECT uid || ':condition', 'condition', condition_src FROM recurrence WHERE condition_src IS NOT NULL;
INSERT INTO karma_field (uid, kind, source)
SELECT uid || ':threshold', 'threshold', gate FROM recurrence WHERE condition_src IS NOT NULL;
INSERT INTO karma_field (uid, kind, source)
SELECT uid || ':consequence', 'consequence', json_object('target', record_uid, 'consequences', json(consequences_json)) FROM recurrence WHERE condition_src IS NOT NULL;
INSERT INTO karma_field_binding (rule_uid, kind, field_uid)
SELECT r.uid, f.kind, f.uid FROM recurrence r JOIN karma_field f ON f.uid = r.uid || ':' || f.kind;
