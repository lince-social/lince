#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="${REPO_DIR:-$(cd "$script_dir/../.." && pwd)}"
state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
pid_file="$state_dir/deploy.pid"
worker="$script_dir/deploy-dev-worker.sh"

mkdir -p "$state_dir"

if [[ -f "$pid_file" ]]; then
    existing_pid="$(cat "$pid_file" 2>/dev/null || true)"
    if [[ -n "$existing_pid" ]] && kill -0 "$existing_pid" 2>/dev/null; then
        echo "Deploy already running with pid $existing_pid"
        echo "Log: $log_file"
        exit 0
    fi
    rm -f "$pid_file"
fi

nohup env \
    REPO_DIR="$repo_dir" \
    SERVICE_NAME="${SERVICE_NAME:-lince}" \
    LINCE_DEPLOY_STATE_DIR="$state_dir" \
    LINCE_DEPLOY_LOG_FILE="$log_file" \
    bash "$worker" </dev/null >/dev/null 2>&1 &

new_pid=$!
echo "$new_pid" >"$pid_file"

echo "Started background deploy with pid $new_pid"
echo "Log: $log_file"
