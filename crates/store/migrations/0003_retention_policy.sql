-- Blueprint II.2: retention horizon per record kind. Facts of a record whose
-- kind has a policy here, older than BOTH the horizon and the record's last
-- checkpoint, are eligible for compaction (folded into the checkpoint and
-- archived to a cold file).
CREATE TABLE retention_policy (
    kind            TEXT PRIMARY KEY,
    horizon_seconds INTEGER NOT NULL CHECK (horizon_seconds >= 0)
);
