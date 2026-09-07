#!/usr/bin/env bash
set -eu

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

dev_directory=${LINCE_DIRECTORY:-${XDG_CONFIG_HOME:-$HOME/.config}/lince-dev}
dev_port=${LINCE_PORT:-6176}
dev_command=(cargo run --release -p lince-desktop -- --directory "$dev_directory" --port "$dev_port")

case "${LINCE_NIXOS:-false}" in
  1 | true | TRUE | yes | YES | on | ON)
    exec nix develop .#desktop -c "${dev_command[@]}"
    ;;
  *)
    exec "${dev_command[@]}"
    ;;
esac
