ALTER TABLE record_assertion ADD COLUMN object_uri TEXT
    CHECK (object_uri IS NULL OR
        (typeof(object_uri) = 'text' AND length(CAST(object_uri AS BLOB)) BETWEEN 1 AND 8192
         AND instr(object_uri, char(0)) = 0 AND object_uid IS NULL AND role = 'ordinary'));

DROP INDEX idx_assertion_unary_active;
CREATE UNIQUE INDEX idx_assertion_unary_active
    ON record_assertion(subject_uid, predicate_uid)
    WHERE object_uid IS NULL AND object_uri IS NULL AND retracted_at IS NULL;
CREATE UNIQUE INDEX idx_assertion_uri_active
    ON record_assertion(subject_uid, predicate_uid, object_uri)
    WHERE object_uri IS NOT NULL AND retracted_at IS NULL;

CREATE TRIGGER assertion_uri_endpoint_update BEFORE UPDATE ON record_assertion
WHEN NEW.object_uri IS NOT NULL AND (NEW.object_uid IS NOT NULL OR NEW.role != 'ordinary')
BEGIN SELECT RAISE(ABORT, 'A URI endpoint is an ordinary assertion target'); END;
