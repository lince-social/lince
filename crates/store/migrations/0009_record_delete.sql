-- Hard delete (2026-07-17): deleting a record is now DISTINCT from zeroing its
-- quantity (`deactivate`). A deleted record is TOMBSTONED — it disappears from
-- every read surface (record queries, slug resolution, rule inputs) — while
-- its facts stay in the Ledger untouched, so the hash chain never breaks. The
-- slug is freed (record.slug is UNIQUE); the deletion fact records the old one.
ALTER TABLE record ADD COLUMN deleted_at TEXT;
CREATE INDEX idx_record_deleted ON record(deleted_at);
