#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-/root/git/lince-social/lince}"
BRANCH="${BRANCH:-dev}"

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

cd "$REPO_DIR"

git fetch origin "$BRANCH"
git checkout "$BRANCH"
git reset --hard "origin/$BRANCH"

RUSTFLAGS="-D warnings" cargo build --release --no-default-features --features "karma http"

cat <<EOF | sudo tee /etc/systemd/system/lince.service
[Unit]
Description=Lince Social
After=network.target

[Service]
User=root
WorkingDirectory=/root/git/lince-social/lince
EnvironmentFile=/root/git/lince-social/lince/.env
ExecStart=/root/git/lince-social/lince/target/release/lince
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable lince
sudo systemctl restart lince
sudo systemctl status lince --no-pager -l
