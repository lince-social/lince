-- SQLite cannot alter CHECK constraints directly. Rebuilding transfer_event is
-- unsafe here because existing settlement and sync cursor tables reference it,
-- so this migration widens only the event_kind CHECK in the stored schema SQL.
PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''item_created'', ''agreement_changed'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied''))',
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''transfer_quantity_changed'', ''transfer_inactivated'', ''item_created'', ''item_edited'', ''interaction_created'', ''interaction_edited'', ''visibility_changed'', ''agreement_changed'', ''message_sent'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied'', ''settlement_reverted'', ''dispute_opened'', ''dispute_resolved''))'
)
WHERE type = 'table'
  AND name = 'transfer_event'
  AND sql LIKE '%event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''item_created'', ''agreement_changed'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied''))%';

PRAGMA writable_schema = OFF;
