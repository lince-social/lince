ALTER TABLE organ_contact ADD COLUMN closed_by_default INTEGER NOT NULL DEFAULT 0;

UPDATE organ_contact
   SET closed_by_default = 1,
       sync_out = 0
 WHERE sync_out = 1
   AND record_uid NOT IN (SELECT uid FROM record WHERE slug = 'local-organ');

CREATE TABLE quarantine_tally (
    from_organ TEXT PRIMARY KEY,
    recorded INTEGER NOT NULL,
    at TEXT NOT NULL
);

INSERT INTO quarantine_tally (from_organ, recorded, at)
SELECT from_organ, COUNT(1), MAX(at) FROM sync_quarantine GROUP BY from_organ;
