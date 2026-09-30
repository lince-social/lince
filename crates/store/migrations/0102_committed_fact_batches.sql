CREATE TABLE commit_sequence (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    value INTEGER NOT NULL CHECK(value >= 0)
) STRICT;
INSERT INTO commit_sequence(id, value) VALUES (1, 0);
ALTER TABLE fact ADD COLUMN commit_sequence INTEGER;
CREATE INDEX fact_committed_batch ON fact(commit_sequence);
