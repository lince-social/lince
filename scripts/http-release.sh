#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
SERVICE_NAME="${SERVICE_NAME:-lince}"
SYSTEMD_UNIT_PATH="${SYSTEMD_UNIT_PATH:-/etc/systemd/system/${SERVICE_NAME}.service}"

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

cd "$REPO_DIR"

"$REPO_DIR/scripts/http-build.sh"

cat <<EOF | sudo tee "$SYSTEMD_UNIT_PATH"
[Unit]
Description=Lince Social
After=network.target

[Service]
User=root
WorkingDirectory=$REPO_DIR
ExecStart=$REPO_DIR/target/release/lince
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable "$SERVICE_NAME"
sudo systemctl restart "$SERVICE_NAME"
sudo systemctl status "$SERVICE_NAME" --no-pager -l
