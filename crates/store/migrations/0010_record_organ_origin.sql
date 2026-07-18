-- Blueprint (Sync/CRDT, 2026-07-18): a record's origin organ, so Protein can
-- filter by it and Sync/File Sync can select what travels where by pointing
-- at a Protein instead of a hardcoded rule. NOT a FK: a synced/relayed
-- record's true origin organ is not guaranteed to be materialized as a local
-- record (same reason concept.origin_organ and sync_outbox.organ_uid are
-- plain TEXT) — it travels as identity, verified through introduction/trust,
-- not through a local join.
ALTER TABLE record ADD COLUMN organ_uid TEXT;
CREATE INDEX idx_record_organ ON record(organ_uid);
