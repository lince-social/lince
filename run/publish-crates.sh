#!/usr/bin/env bash

set -euo pipefail

dry_run=false

usage() {
    cat <<'EOF'
Usage: ./run/publish-crates.sh [--dry-run]

Publishes the internal lince-* crates first and the lince binary last.
Use --dry-run to validate the publish order without uploading crates.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run)
            dry_run=true
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
    shift
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required" >&2
    exit 1
fi

if [ "${dry_run}" != "true" ] \
    && [ ! -f "${HOME}/.cargo/credentials.toml" ] \
    && [ ! -f "${HOME}/.cargo/credentials" ] \
    && [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
    echo "crates.io authentication is missing." >&2
    echo "Set CARGO_REGISTRY_TOKEN or run cargo login first." >&2
    exit 1
fi

index_wait_seconds="${LINCE_CRATES_INDEX_WAIT_SECONDS:-15}"

packages=(
    lince-utils
    lince-domain
    lince-persistence
    lince-injection
    lince-application
    lince-web
    lince-gui
    lince-tui
    lince
)

for i in "${!packages[@]}"; do
    package="${packages[$i]}"
    if [ "${dry_run}" = "true" ]; then
        echo "Dry run for ${package}..."
        cargo publish --locked --dry-run -p "${package}"
        continue
    fi

    echo "Publishing ${package}..."
    cargo publish --locked -p "${package}"

    if [ "$i" -lt "$((${#packages[@]} - 1))" ]; then
        echo "Waiting ${index_wait_seconds}s for crates.io index propagation..."
        sleep "${index_wait_seconds}"
    fi
done
