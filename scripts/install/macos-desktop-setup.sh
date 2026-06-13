#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${1:-/Applications/Lince.app}"
BIN="$APP_PATH/Contents/MacOS/lince-desktop"

if [[ ! -x "$BIN" ]]; then
  osascript -e 'display dialog "Lince.app was not found in /Applications. Install Lince first, then run setup again." buttons {"OK"} default button "OK" with icon stop'
  exit 1
fi

args=(--stage-desktop-install-setup)

if osascript -e 'button returned of (display dialog "Start Lince when you sign in?" buttons {"No", "Yes"} default button "Yes")' | grep -qx "Yes"; then
  args+=(--start-on-login)
  if osascript -e 'button returned of (display dialog "Open Lince silently to the system tray when started automatically?" buttons {"No", "Yes"} default button "Yes")' | grep -qx "Yes"; then
    args+=(--start-silent)
  fi
fi

if osascript -e 'button returned of (display dialog "Enable authentication for Lince?" buttons {"No", "Yes"} default button "No")' | grep -qx "Yes"; then
  password="$(osascript -e 'text returned of (display dialog "Choose the initial admin password for Lince." default answer "" hidden answer true buttons {"OK"} default button "OK")')"
  if [[ -z "$password" ]]; then
    osascript -e 'display dialog "A password is required when authentication is enabled." buttons {"OK"} default button "OK" with icon stop'
    exit 1
  fi
  args+=(--auth-enabled --initial-admin-password "$password")
fi

"$BIN" "${args[@]}"
osascript -e 'display dialog "Lince setup is ready. Open Lince once to apply these choices." buttons {"OK"} default button "OK"'
