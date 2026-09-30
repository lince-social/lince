CREATE TABLE fact_origin (
    fact_uid TEXT PRIMARY KEY REFERENCES fact(uid) ON DELETE CASCADE,
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_extract(payload, '$.uid') = fact_uid)
) STRICT;

CREATE TRIGGER fact_origin_immutable_update
BEFORE UPDATE ON fact_origin
BEGIN
    SELECT RAISE(ABORT, 'original Fact evidence is immutable');
END;

CREATE TRIGGER fact_origin_immutable_delete
BEFORE DELETE ON fact_origin
WHEN EXISTS (SELECT 1 FROM fact WHERE uid = OLD.fact_uid)
BEGIN
    SELECT RAISE(ABORT, 'original Fact evidence is immutable');
END;
