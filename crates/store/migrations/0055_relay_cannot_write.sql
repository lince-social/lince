-- "The front door holds no signing material" becomes ENFORCED, not merely
-- true (Ontology §11, cluster C4).
--
-- A relay Cell holds `relay_capabilities()` — no write, no karma, no
-- represent — and until now that was a promise kept by everything simply not
-- asking it to write. A property you can defeat by pointing a second client at
-- the same store is not a security property, so the refusal lives in the
-- DATABASE, below every client.
--
-- `local_capability` is this Cell's own capability set, flattened out of the
-- signed roster whenever one naming us is stored. It is a projection, never
-- the source of truth: the signed blob in `organ_roster` is, and this table is
-- rebuilt from it.
CREATE TABLE local_capability (
    capability TEXT PRIMARY KEY
);

-- Refuse any op authored BY THIS CELL when this Cell has no `write`.
--
-- Three conditions, and each one matters:
--   * `NEW.actor_cell` is ours — an IMPORTED op is authored by someone else
--     and must still be storable, or a relay could not carry anything at all.
--   * our own Organ has a roster — before one exists a Cell is the whole
--     Organ and constraining it would break first boot.
--   * `write` is absent from the projection.
CREATE TRIGGER sync_op_requires_write_capability
BEFORE INSERT ON sync_op
WHEN NEW.actor_cell = (SELECT uid FROM record WHERE slug = 'local-cell' LIMIT 1)
 AND EXISTS (
       SELECT 1 FROM organ_roster
        WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' LIMIT 1)
     )
 AND NOT EXISTS (SELECT 1 FROM local_capability WHERE capability = 'write')
BEGIN
    SELECT RAISE(
        ABORT,
        'this Cell has no write capability in its Organ: a relay carries traffic and authors nothing'
    );
END;
