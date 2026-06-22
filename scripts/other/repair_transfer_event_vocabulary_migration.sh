#!/usr/bin/env bash
set -euo pipefail

# Set this to the SQLite database file you want to repair, or pass it as argv[1].
DB_PATH="${1:-${DB_PATH:-}}"

if [[ -z "$DB_PATH" ]]; then
  echo "usage: DB_PATH=/path/to/lince.db $0"
  echo "   or: $0 /path/to/lince.db"
  exit 64
fi

if [[ ! -f "$DB_PATH" ]]; then
  echo "database not found: $DB_PATH" >&2
  exit 66
fi

if command -v sqlite3 >/dev/null 2>&1; then
  sqlite_scalar() {
    sqlite3 "$DB_PATH" "$1"
  }
  sqlite_script() {
    {
      printf '.dbconfig defensive off\n'
      cat
    } | sqlite3 "$DB_PATH"
  }
elif command -v python3 >/dev/null 2>&1; then
  sqlite_scalar() {
    python3 - "$DB_PATH" "$1" <<'PY'
import sqlite3
import sys

db_path, sql = sys.argv[1], sys.argv[2]
connection = sqlite3.connect(db_path)
try:
    row = connection.execute(sql).fetchone()
    connection.commit()
    if row is not None and len(row) > 0 and row[0] is not None:
        print(row[0])
finally:
    connection.close()
PY
  }
  sqlite_script() {
    python3 - "$DB_PATH" <<'PY'
import sqlite3
import sys

db_path = sys.argv[1]
sql = sys.stdin.read()
connection = sqlite3.connect(db_path)
try:
    connection.executescript(sql)
    connection.commit()
finally:
    connection.close()
PY
  }
else
  script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd -- "$script_dir/../.." && pwd)"
  if [[ "${LINCE_REPAIR_IN_NIX:-0}" != "1" && -f "$repo_root/flake.nix" && -x "$(command -v nix 2>/dev/null)" ]]; then
    echo "sqlite3/python3 not found; retrying inside nix develop..."
    exec env LINCE_REPAIR_IN_NIX=1 nix develop "$repo_root" -c "$BASH" "$script_dir/repair_transfer_event_vocabulary_migration.sh" "$DB_PATH"
  fi
  echo "sqlite3 or python3 is required but neither was found on PATH" >&2
  echo "From this repo, try: nix develop -c $0 \"$DB_PATH\"" >&2
  exit 69
fi

backup_path="${DB_PATH}.backup.$(date +%Y%m%d%H%M%S)"

echo "Repairing: $DB_PATH"
echo "Backup:    $backup_path"

sqlite_scalar "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null
cp -p "$DB_PATH" "$backup_path"

has_transfer_event="$(sqlite_scalar "SELECT COUNT(1) FROM sqlite_schema WHERE type = 'table' AND name = 'transfer_event';")"
if [[ "$has_transfer_event" != "1" ]]; then
  echo "transfer_event table is missing; this repair is for databases that already reached migration 20260614143000" >&2
  exit 70
fi

has_sqlx_migrations="$(sqlite_scalar "SELECT COUNT(1) FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations';")"
if [[ "$has_sqlx_migrations" != "1" ]]; then
  echo "_sqlx_migrations table is missing; this does not look like a migrated sqlx database" >&2
  exit 70
fi

sqlite_script <<'SQL'
PRAGMA foreign_keys = OFF;
PRAGMA writable_schema = ON;
BEGIN IMMEDIATE;

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''item_created'', ''agreement_changed'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied''))',
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''transfer_quantity_changed'', ''transfer_inactivated'', ''item_created'', ''item_edited'', ''interaction_created'', ''interaction_edited'', ''visibility_changed'', ''agreement_changed'', ''message_sent'', ''delivery_confirmed'', ''receipt_confirmed'', ''package_received'', ''package_seen'', ''settlement_applied'', ''settlement_reverted'', ''dispute_opened'', ''dispute_resolved''))'
)
WHERE type = 'table'
  AND name = 'transfer_event'
  AND sql LIKE '%event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''item_created'', ''agreement_changed'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied''))%';

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''transfer_quantity_changed'', ''transfer_inactivated'', ''item_created'', ''item_edited'', ''interaction_created'', ''interaction_edited'', ''visibility_changed'', ''agreement_changed'', ''message_sent'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied'', ''settlement_reverted'', ''dispute_opened'', ''dispute_resolved''))',
    'event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''transfer_quantity_changed'', ''transfer_inactivated'', ''item_created'', ''item_edited'', ''interaction_created'', ''interaction_edited'', ''visibility_changed'', ''agreement_changed'', ''message_sent'', ''delivery_confirmed'', ''receipt_confirmed'', ''package_received'', ''package_seen'', ''settlement_applied'', ''settlement_reverted'', ''dispute_opened'', ''dispute_resolved''))'
)
WHERE type = 'table'
  AND name = 'transfer_event'
  AND sql LIKE '%event_kind TEXT NOT NULL CHECK (event_kind IN (''transfer_created'', ''transfer_quantity_changed'', ''transfer_inactivated'', ''item_created'', ''item_edited'', ''interaction_created'', ''interaction_edited'', ''visibility_changed'', ''agreement_changed'', ''message_sent'', ''delivery_confirmed'', ''receipt_confirmed'', ''settlement_applied'', ''settlement_reverted'', ''dispute_opened'', ''dispute_resolved''))%';

DELETE FROM _sqlx_migrations
WHERE version = 20260614143000;

COMMIT;

PRAGMA writable_schema = OFF;
PRAGMA foreign_keys = ON;
SQL

integrity_check="$(sqlite_scalar "PRAGMA integrity_check;")"
if [[ "$integrity_check" != "ok" ]]; then
  echo "integrity_check failed: $integrity_check" >&2
  echo "The original database backup is at: $backup_path" >&2
  exit 70
fi

echo
echo "Done."
echo "Next start of Lince should rerun migration 20260614143000 and store its current checksum."
echo "If startup still fails on a later migration checksum, repeat the same metadata repair pattern for that version only after checking its SQL."
