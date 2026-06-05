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

profile=""
lince_bin=""
data_dir=""
port=""
http_api_only=1

usage() {
    cat <<'EOF'
Install a system-level systemd service for Lince.

Generic mode:
  ./scripts/install/install-system-service.sh \
    --working-directory /path/to/repo \
    --exec-start "/path/to/lince --http-api-only --data-dir /path --port 6174"

Profile mode:
  ./scripts/install/install-system-service.sh --profile institute --lince-bin /path/to/lince
  ./scripts/install/install-system-service.sh --profile global --lince-bin /path/to/lince

Options:
  --profile <name>           One of: institute, global
  --lince-bin <path>         Binary path used by profile mode.
  --data-dir <path>          Override profile data dir.
  --port <port>              Override profile port.
  --http-api-only            Add --http-api-only in profile mode (default).
  --no-http-api-only         Skip --http-api-only in profile mode.

  --service-name <name>      Unit name without .service. Default: lince
  --description <text>       Unit description.
  --user <user>              Service user. Default: root
  --group <group>            Service group.
  --working-directory <dir>  Required in generic mode.
  --exec-start <command>     Required in generic mode.
  --restart <policy>         Restart policy. Default: always
  --restart-sec <seconds>    Restart delay. Default: 3
  --wanted-by <target>       Install target. Default: multi-user.target
  -h, --help                 Show this help text.
EOF
}

while (($# > 0)); do
    case "$1" in
        --profile)
            profile="$2"
            shift 2
            ;;
        --lince-bin)
            lince_bin="$2"
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
        --http-api-only)
            http_api_only=1
            shift
            ;;
        --no-http-api-only)
            http_api_only=0
            shift
            ;;
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

if [[ -n "$profile" ]]; then
    [[ -n "$lince_bin" ]] || {
        echo "--lince-bin is required in profile mode" >&2
        exit 1
    }

    case "$profile" in
        institute)
            service_name="${service_name:-lince-institute}"
            if [[ "$service_name" == "lince" ]]; then
                service_name="lince-institute"
            fi
            description="Lince Social HTTP API - institute"
            : "${data_dir:=/root/.config/lince}"
            : "${port:=6174}"
            ;;
        global)
            if [[ "$service_name" == "lince" ]]; then
                service_name="lince-global"
            fi
            description="Lince Social HTTP API - global"
            : "${data_dir:=/root/.config/global_lince}"
            : "${port:=6175}"
            ;;
        *)
            echo "Unknown profile: $profile" >&2
            exit 1
            ;;
    esac

    if [[ -z "$working_directory" ]]; then
        working_directory="$(dirname "$lince_bin")"
    fi

    exec_start="${lince_bin} --data-dir ${data_dir} --port ${port}"
    if [[ "$http_api_only" -eq 1 ]]; then
        exec_start="${exec_start} --http-api-only"
    fi
fi

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
run_as_root systemctl restart "$service_name"

printf 'Installed %s\n' "$unit_path"
printf 'Follow logs with:\n'
printf '  journalctl -u %s.service -f --output=cat\n' "$service_name"
