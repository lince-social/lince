#!/usr/bin/env bash

set -euo pipefail

repo_dir="${1:-$(pwd)}"
service_dir="${HOME}/.config/systemd/user"
service_path="${service_dir}/lince.service"
cargo_bin="$(command -v cargo)"

mkdir -p "$service_dir"

cat > "$service_path" <<EOF
[Unit]
Description=Lince user service
After=network.target

[Service]
Type=simple
WorkingDirectory=$repo_dir
ExecStart=$cargo_bin run --release
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable lince.service >/dev/null 2>&1 || true

systemctl --user stop lince.service && journalctl --user -u lince.service -f --output=cat
trap 'systemctl --user restart lince.service && journalctl --user -u lince.service -f --output=cat'
cargo run
