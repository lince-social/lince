#!/usr/bin/env python3
import argparse
import hashlib
import re
import sqlite3
import sys
from pathlib import Path


ADD_COLUMN_RE = re.compile(
    r"\bALTER\s+TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s+ADD\s+COLUMN\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    re.IGNORECASE,
)


def migration_description(path: Path, version: int) -> str:
    name = path.name
    for suffix in (".up.sql", ".down.sql", ".sql"):
        if name.endswith(suffix):
            name = name[: -len(suffix)]
            break
    prefix = f"{version}_"
    if name.startswith(prefix):
        return name[len(prefix) :].replace("_", " ")
    return name.replace("_", " ")


def table_columns(connection: sqlite3.Connection, table: str) -> set[str]:
    rows = connection.execute(f"PRAGMA table_info({table})").fetchall()
    return {row[1] for row in rows}


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Mark a sqlx migration as applied only when every ALTER TABLE ADD COLUMN "
            "statement in that migration is already reflected in the SQLite schema."
        )
    )
    parser.add_argument("db_path", type=Path)
    parser.add_argument("migration_path", type=Path)
    parser.add_argument("--version", type=int, required=True)
    args = parser.parse_args()

    if not args.db_path.is_file():
        print(f"database not found: {args.db_path}", file=sys.stderr)
        return 66
    if not args.migration_path.is_file():
        print(f"migration not found: {args.migration_path}", file=sys.stderr)
        return 66

    migration_bytes = args.migration_path.read_bytes()
    migration_sql = migration_bytes.decode("utf-8")
    add_columns = ADD_COLUMN_RE.findall(migration_sql)
    if not add_columns:
        print(
            "refusing to mark migration: no ALTER TABLE ... ADD COLUMN statements were found",
            file=sys.stderr,
        )
        return 65

    checksum = hashlib.sha384(migration_bytes).digest()
    description = migration_description(args.migration_path, args.version)
    backup_path = args.db_path.with_name(
        f"{args.db_path.name}.backup.mark-{args.version}"
    )

    connection = sqlite3.connect(args.db_path)
    try:
        connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        if not backup_path.exists():
            backup_path.write_bytes(args.db_path.read_bytes())

        missing = []
        for table, column in add_columns:
            if column not in table_columns(connection, table):
                missing.append(f"{table}.{column}")
        if missing:
            print(
                "refusing to mark migration: DB is missing expected columns: "
                + ", ".join(missing),
                file=sys.stderr,
            )
            return 70

        existing = connection.execute(
            "SELECT success, checksum FROM _sqlx_migrations WHERE version = ?",
            (args.version,),
        ).fetchone()
        if existing is not None:
            success, existing_checksum = existing
            if success and existing_checksum == checksum:
                print(f"migration {args.version} is already marked with the current checksum")
                return 0
            print(
                f"refusing to overwrite existing _sqlx_migrations row for {args.version}",
                file=sys.stderr,
            )
            return 70

        connection.execute(
            """
            INSERT INTO _sqlx_migrations(
                version,
                description,
                installed_on,
                success,
                checksum,
                execution_time
            )
            VALUES (?, ?, CURRENT_TIMESTAMP, 1, ?, 0)
            """,
            (args.version, description, checksum),
        )
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            connection.rollback()
            print(f"integrity_check failed: {integrity}", file=sys.stderr)
            return 70
        connection.commit()
        print(f"marked migration {args.version} as applied")
        print(f"description: {description}")
        print(f"checksum bytes: {len(checksum)}")
        print(f"backup: {backup_path}")
        print("integrity_check: ok")
        return 0
    finally:
        connection.close()


if __name__ == "__main__":
    raise SystemExit(main())
