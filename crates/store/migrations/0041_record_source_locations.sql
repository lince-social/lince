CREATE TABLE record_source_location (
    record_uid TEXT NOT NULL REFERENCES record(uid) ON DELETE CASCADE,
    source_uid TEXT NOT NULL CHECK (typeof(source_uid) = 'text' AND length(source_uid) = 28 AND substr(source_uid, 1, 2) = 'r_'),
    uri TEXT NOT NULL CHECK (typeof(uri) = 'text' AND length(CAST(uri AS BLOB)) BETWEEN 1 AND 8192 AND instr(uri, char(0)) = 0),
    hash TEXT NOT NULL CHECK (typeof(hash) = 'text' AND length(hash) = 64 AND lower(hash) NOT GLOB '*[^0-9a-f]*'),
    PRIMARY KEY (record_uid, uri)
);
CREATE INDEX idx_record_source_location_uri ON record_source_location(uri);

CREATE VIEW record_assertion_link AS
SELECT a.*,
    COALESCE(a.object_uid, (
        SELECT CASE WHEN MIN(l.record_uid) = MAX(l.record_uid) THEN MIN(l.record_uid) END
        FROM record_source_location l
        JOIN record object ON object.uid = l.record_uid AND object.deleted_at IS NULL
        JOIN record subject ON subject.uid = a.subject_uid
        WHERE l.uri = a.object_uri AND object.organ_uid IS subject.organ_uid
          AND (object.replica_root IS NULL OR object.replica_root IS subject.replica_root)
    )) AS target_uid
FROM record_assertion a;
