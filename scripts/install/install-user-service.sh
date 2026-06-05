#!/usr/bin/env bash

set -euo pipefail

repo_dir="$(pwd)"
service_name="lince"
description="Lince user service"
data_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/lince"
port="6174"
binary_path=""
http_api_only=0

usage() {
    cat <<'EOF'
Install a user-level systemd service for Lince.

Usage:
  ./scripts/install/install-user-service.sh [options]

Options:
  --repo-dir <dir>       Working directory for the service. Defaults to cwd.
  --service-name <name>  systemd unit name without .service. Default: lince
  --description <text>   Unit description.
  --data-dir <path>      Lince data directory. Default: $XDG_CONFIG_HOME/lince
  --port <port>          HTTP port. Default: 6174
  --binary-path <path>   Run an existing binary instead of `cargo run --release`.
  --http-api-only        Start with --http-api-only.
  -h, --help             Show this help text.
EOF
}

while (($# > 0)); do
    case "$1" in
        --repo-dir)
            repo_dir="$2"
            shift 2
            ;;
        --service-name)
            service_name="$2"
            shift 2
            ;;
        --description)
            description="$2"
            shift 2
            ;;
        --data-dir)
            data_dir="$2"
            shift 2
            ;;
        --port)
            port="$2"
            shift 2
            ;;
        --binary-path)
            binary_path="$2"
            shift 2
            ;;
        --http-api-only)
            http_api_only=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

service_dir="${HOME}/.config/systemd/user"
service_path="${service_dir}/${service_name}.service"
mkdir -p "$service_dir"

if [[ -n "$binary_path" ]]; then
    exec_start="${binary_path} --data-dir ${data_dir} --port ${port}"
else
    cargo_bin="$(command -v cargo)"
    exec_start="${cargo_bin} run --release -- --data-dir ${data_dir} --port ${port}"
fi

if [[ "$http_api_only" -eq 1 ]]; then
    exec_start="${exec_start} --http-api-only"
fi

cat > "$service_path" <<EOF
[Unit]
Description=${description}
After=network.target

[Service]
Type=simple
WorkingDirectory=${repo_dir}
ExecStart=${exec_start}
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable "${service_name}.service" >/dev/null 2>&1 || true
systemctl --user restart "${service_name}.service"

printf 'Installed %s\n' "$service_path"
printf 'Follow logs with:\n'
printf '  journalctl --user -u %s.service -f --output=cat\n' "$service_name"
