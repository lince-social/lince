#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="${REPO_DIR:-$(cd "$script_dir/../.." && pwd)}"
service_name="${SERVICE_NAME:-lince}"
state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
pid_file="$state_dir/deploy.pid"
status_file="$state_dir/status"
lock_dir="$state_dir/lock"

mkdir -p "$state_dir"
touch "$log_file"

exec >>"$log_file" 2>&1

timestamp() {
    date -Iseconds
}

write_status() {
    printf '%s %s\n' "$(timestamp)" "$1" >"$status_file"
}

cleanup() {
    local exit_code=$?
    if [[ $exit_code -eq 0 ]]; then
        write_status "success"
    else
        write_status "failed(${exit_code})"
    fi
    rm -f "$pid_file"
    rm -rf "$lock_dir"
}

trap cleanup EXIT

if ! mkdir "$lock_dir" 2>/dev/null; then
    echo "[$(timestamp)] deploy skipped: another deploy is already running"
    write_status "skipped(already-running)"
    exit 0
fi

echo "$$" >"$pid_file"
write_status "running"

echo "[$(timestamp)] starting background deploy"
echo "[$(timestamp)] repo_dir=$repo_dir service_name=$service_name"

if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
fi

cd "$repo_dir"

export RUSTFLAGS="-D warnings"
write_status "building"
cargo build --locked --release --package lince

write_status "installing-service"
bash "$repo_dir/run/systemd/install-system-service.sh" \
    --service-name "$service_name" \
    --description "Lince Social HTTP API" \
    --user root \
    --working-directory "$repo_dir" \
    --exec-start "$repo_dir/target/release/lince --http-api-only"

write_status "restarting-service"
if ((EUID == 0)); then
    systemctl restart "$service_name"
    systemctl status "$service_name" --no-pager -l
else
    sudo systemctl restart "$service_name"
    sudo systemctl status "$service_name" --no-pager -l
fi

echo "[$(timestamp)] deploy finished"
