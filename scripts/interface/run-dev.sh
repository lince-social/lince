#!/usr/bin/env bash
set -eu

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

dev_data_dir=${LINCE_DATA_DIR:-${LINCE_DIRECTORY:-${XDG_CONFIG_HOME:-$HOME/.config}/lince-dev}}
dev_command=(cargo run --release -p lince --no-default-features --features ui -- --directory "$dev_data_dir")

case "${LINCE_NIXOS:-false}" in
  1 | true | TRUE | yes | YES | on | ON)
    exec nix develop .#interface -c "${dev_command[@]}"
    ;;
  *)
    exec "${dev_command[@]}"
    ;;
esac
