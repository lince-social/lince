#!/usr/bin/env bash
set -eu

legacy_url=http://127.0.0.1:6174
legacy_args=(--listen-addr 127.0.0.1:6174)
if [ -n "${LINCE_LEGACY_DATA_DIR:-}" ]; then
  legacy_args+=(--data-dir "$LINCE_LEGACY_DATA_DIR")
fi
cargo run -p lince -- "${legacy_args[@]}" &
legacy_pid=$!

stop_legacy() {
  if kill -0 "$legacy_pid" 2>/dev/null; then
    kill "$legacy_pid" 2>/dev/null || true
    wait "$legacy_pid" 2>/dev/null || true
  fi
}

trap stop_legacy EXIT INT TERM

legacy_attempt=0
while [ "$legacy_attempt" -lt 2400 ]; do
  if ! kill -0 "$legacy_pid" 2>/dev/null; then
    if wait "$legacy_pid"; then
      legacy_status=0
    else
      legacy_status=$?
    fi
    trap - EXIT INT TERM
    exit "$legacy_status"
  fi
  if curl --fail --silent --output /dev/null "$legacy_url"; then
    printf 'Legacy Lince interface: %s\n' "$legacy_url"
    if [ "${LINCE_NO_OPEN:-0}" != 1 ] && ! xdg-open "$legacy_url" >/dev/null 2>&1; then
      printf 'Open %s in a browser.\n' "$legacy_url"
    fi
    if wait "$legacy_pid"; then
      legacy_status=0
    else
      legacy_status=$?
    fi
    trap - EXIT INT TERM
    exit "$legacy_status"
  fi
  legacy_attempt=$((legacy_attempt + 1))
  sleep 0.25
done

printf 'Legacy Lince did not become ready at %s.\n' "$legacy_url" >&2
exit 1
