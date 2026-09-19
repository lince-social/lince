#!/usr/bin/env bash
set -eu

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

dev_data_dir=${LINCE_DATA_DIR:-${LINCE_DIRECTORY:-${XDG_CONFIG_HOME:-$HOME/.config}/test_lince}}
dev_command=(cargo run --release -p lince --no-default-features --features ui -- --directory "$dev_data_dir" --port "${LINCE_PORT:-6176}" --no-tray "$@")

dev_nixos=${LINCE_NIXOS:-false}
if [ -z "${LINCE_NIXOS+x}" ] && [ -e /etc/NIXOS ]; then
  dev_nixos=true
fi

case "$dev_nixos" in
  1 | true | TRUE | yes | YES | on | ON)
    exec nix develop .#interface -c "${dev_command[@]}"
    ;;
  *)
    exec "${dev_command[@]}"
    ;;
esac
