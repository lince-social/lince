#!/usr/bin/env bash

set -euo pipefail

service_name="lince"
description="Lince Social HTTP API"
service_user="root"
service_group=""
working_directory=""
exec_start=""
restart_policy="always"
restart_sec="3"
wanted_by="multi-user.target"

while (($# > 0)); do
    case "$1" in
        --service-name)
            service_name="$2"
            shift 2
            ;;
        --description)
            description="$2"
            shift 2
            ;;
        --user)
            service_user="$2"
            shift 2
            ;;
        --group)
            service_group="$2"
            shift 2
            ;;
        --working-directory)
            working_directory="$2"
            shift 2
            ;;
        --exec-start)
            exec_start="$2"
            shift 2
            ;;
        --restart)
            restart_policy="$2"
            shift 2
            ;;
        --restart-sec)
            restart_sec="$2"
            shift 2
            ;;
        --wanted-by)
            wanted_by="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$working_directory" ]]; then
    echo "--working-directory is required" >&2
    exit 1
fi

if [[ -z "$exec_start" ]]; then
    echo "--exec-start is required" >&2
    exit 1
fi

unit_path="/etc/systemd/system/${service_name}.service"

if [[ -n "$service_group" ]]; then
    group_line="Group=${service_group}"
else
    group_line=""
fi

if ((EUID == 0)); then
    run_as_root() {
        "$@"
    }
else
    run_as_root() {
        sudo "$@"
    }
fi

tmpfile="$(mktemp)"
trap 'rm -f "$tmpfile"' EXIT

cat >"$tmpfile" <<EOF
[Unit]
Description=${description}
After=network.target

[Service]
Type=simple
User=${service_user}
${group_line}
WorkingDirectory=${working_directory}
ExecStart=${exec_start}
Restart=${restart_policy}
RestartSec=${restart_sec}

[Install]
WantedBy=${wanted_by}
EOF

run_as_root install -Dm644 "$tmpfile" "$unit_path"
run_as_root systemctl daemon-reload
run_as_root systemctl enable "$service_name"
