-- The Organ/Cell split, read-model side (Ontology §11 "Profile vs device
-- surfaces"). `0040_sync_op.sql` split the two identities on the LOG; this
-- makes the read model agree with it.
--
-- Every Record has an origin Organ. `organ_uid` has been nullable since
-- `0010_record_organ_origin.sql` only because origins were stamped by a
-- separate UPDATE after the INSERT, which left a window — and, when no local
-- Organ existed yet, a permanent unattributable row. The split is the moment
-- to require it at write time and delete the unknown-origin state rather than
-- carry it forward.
--
-- Enforced by trigger rather than by rebuilding the table into `NOT NULL`.
-- Not a shortcut: 47 tables carry `REFERENCES record`, the connection runs
-- with `PRAGMA foreign_keys = ON`, and that pragma is a no-op inside a
-- transaction — which is where sqlx runs every migration. The 12-step rebuild
-- therefore is not cleanly available here, and the trigger gives the identical
-- guarantee at the identical place (write time). Do not "fix" this into a
-- rebuild without first solving the pragma problem.

-- Anything already unattributable belongs to this Cell's Organ: it was written
-- here, by us, before the stamp was required. If there is no local Organ yet
-- the table is empty (fresh database), and this updates nothing.
UPDATE record
   SET organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ')
 WHERE organ_uid IS NULL OR organ_uid = '';

CREATE TRIGGER record_origin_required_insert
BEFORE INSERT ON record
WHEN NEW.organ_uid IS NULL OR NEW.organ_uid = ''
BEGIN
    SELECT RAISE(ABORT, 'record.organ_uid is required: every Record has an origin Organ');
END;

-- On UPDATE too, or `set_organ_origin(uid, None)` walks straight back through
-- the front door and re-creates the state this migration deletes.
CREATE TRIGGER record_origin_required_update
BEFORE UPDATE OF organ_uid ON record
WHEN NEW.organ_uid IS NULL OR NEW.organ_uid = ''
BEGIN
    SELECT RAISE(ABORT, 'record.organ_uid is required: every Record has an origin Organ');
END;
