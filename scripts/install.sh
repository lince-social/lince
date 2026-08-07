#!/usr/bin/env bash

set -euo pipefail

LINCE_REPOSITORY="${LINCE_REPOSITORY:-lince-social/lince}"
INSTALL_URL="https://raw.githubusercontent.com/${LINCE_REPOSITORY}/main/scripts/install/install.sh"

if command -v curl >/dev/null 2>&1; then
    exec bash <(curl --proto '=https' --tlsv1.2 -fsSL "$INSTALL_URL") "$@"
fi

if command -v wget >/dev/null 2>&1; then
    exec bash <(wget -qO- "$INSTALL_URL") "$@"
fi

echo "error: need curl or wget to bootstrap scripts/install/install.sh" >&2
exit 1
