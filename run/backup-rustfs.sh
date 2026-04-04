#!/usr/bin/env bash

set -euo pipefail

umask 022

SOURCE_ALIAS="${RUSTFS_BACKUP_SOURCE_ALIAS:-src}"
DEST_ALIAS="${RUSTFS_BACKUP_DEST_ALIAS:-backup}"
SOURCE_BUCKET="${RUSTFS_BACKUP_SOURCE_BUCKET:-lince}"
DEST_BUCKET="${RUSTFS_BACKUP_DEST_BUCKET:-lince-backup}"
SOURCE_PREFIX="${RUSTFS_BACKUP_SOURCE_PREFIX:-}"
DEST_PREFIX="${RUSTFS_BACKUP_DEST_PREFIX:-}"
SOURCE_ENDPOINT="${RUSTFS_BACKUP_SOURCE_ENDPOINT:-}"
SOURCE_ACCESS_KEY="${RUSTFS_BACKUP_SOURCE_ACCESS_KEY:-}"
SOURCE_SECRET_KEY="${RUSTFS_BACKUP_SOURCE_SECRET_KEY:-}"
DEST_ENDPOINT="${RUSTFS_BACKUP_DEST_ENDPOINT:-}"
DEST_ACCESS_KEY="${RUSTFS_BACKUP_DEST_ACCESS_KEY:-}"
DEST_SECRET_KEY="${RUSTFS_BACKUP_DEST_SECRET_KEY:-}"
DRY_RUN="${RUSTFS_BACKUP_DRY_RUN:-0}"

log() {
    printf '%s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

usage() {
    cat <<'EOF'
Mirror a RustFS bucket into a backup bucket with mc.

Usage:
  ./run/backup-rustfs.sh

Environment overrides:
  RUSTFS_BACKUP_SOURCE_ALIAS
  RUSTFS_BACKUP_DEST_ALIAS
  RUSTFS_BACKUP_SOURCE_BUCKET
  RUSTFS_BACKUP_DEST_BUCKET
  RUSTFS_BACKUP_SOURCE_PREFIX
  RUSTFS_BACKUP_DEST_PREFIX
  RUSTFS_BACKUP_SOURCE_ENDPOINT
  RUSTFS_BACKUP_SOURCE_ACCESS_KEY
  RUSTFS_BACKUP_SOURCE_SECRET_KEY
  RUSTFS_BACKUP_DEST_ENDPOINT
  RUSTFS_BACKUP_DEST_ACCESS_KEY
  RUSTFS_BACKUP_DEST_SECRET_KEY
  RUSTFS_BACKUP_DRY_RUN
EOF
}

trim_trailing_slash() {
    value="$1"
    while [ "${value%/}" != "$value" ]; do
        value="${value%/}"
    done
    printf '%s' "$value"
}

bucket_path() {
    alias_name="$1"
    bucket_name="$2"
    prefix="$3"

    if [ -n "$prefix" ]; then
        printf '%s/%s/%s' "$alias_name" "$bucket_name" "${prefix#/}"
        return
    fi

    printf '%s/%s' "$alias_name" "$bucket_name"
}

require_value() {
    var_name="$1"
    var_value="$2"

    if [ -z "$var_value" ]; then
        fail "$var_name is required"
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

have_cmd mc || fail "required command not found: mc"

require_value "RUSTFS_BACKUP_SOURCE_ENDPOINT" "$SOURCE_ENDPOINT"
require_value "RUSTFS_BACKUP_SOURCE_ACCESS_KEY" "$SOURCE_ACCESS_KEY"
require_value "RUSTFS_BACKUP_SOURCE_SECRET_KEY" "$SOURCE_SECRET_KEY"
require_value "RUSTFS_BACKUP_DEST_ENDPOINT" "$DEST_ENDPOINT"
require_value "RUSTFS_BACKUP_DEST_ACCESS_KEY" "$DEST_ACCESS_KEY"
require_value "RUSTFS_BACKUP_DEST_SECRET_KEY" "$DEST_SECRET_KEY"

SOURCE_ENDPOINT="$(trim_trailing_slash "$SOURCE_ENDPOINT")"
DEST_ENDPOINT="$(trim_trailing_slash "$DEST_ENDPOINT")"

log "configuring mc aliases"
mc alias set "$SOURCE_ALIAS" "$SOURCE_ENDPOINT" "$SOURCE_ACCESS_KEY" "$SOURCE_SECRET_KEY" >/dev/null
mc alias set "$DEST_ALIAS" "$DEST_ENDPOINT" "$DEST_ACCESS_KEY" "$DEST_SECRET_KEY" >/dev/null

if ! mc ls "$DEST_ALIAS/$DEST_BUCKET" >/dev/null 2>&1; then
    log "creating destination bucket ${DEST_ALIAS}/${DEST_BUCKET}"
    mc mb "$DEST_ALIAS/$DEST_BUCKET" >/dev/null
fi

source_path="$(bucket_path "$SOURCE_ALIAS" "$SOURCE_BUCKET" "$SOURCE_PREFIX")"
dest_path="$(bucket_path "$DEST_ALIAS" "$DEST_BUCKET" "$DEST_PREFIX")"

log "mirroring ${source_path} -> ${dest_path}"
if [ "$DRY_RUN" = "1" ] || [ "$DRY_RUN" = "true" ] || [ "$DRY_RUN" = "yes" ]; then
    mc mirror --overwrite --dry-run "$source_path" "$dest_path"
else
    mc mirror --overwrite "$source_path" "$dest_path"
fi
