#!/usr/bin/env bash

set -euo pipefail

state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
status_file="$state_dir/status"
pid_file="$state_dir/deploy.pid"
log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
timeout_seconds=900
interval_seconds=5

while (($# > 0)); do
    case "$1" in
        --timeout-seconds)
            timeout_seconds="$2"
            shift 2
            ;;
        --interval-seconds)
            interval_seconds="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if ! [[ "$timeout_seconds" =~ ^[0-9]+$ ]] || ! [[ "$interval_seconds" =~ ^[0-9]+$ ]]; then
    echo "Timeout and interval must be integers" >&2
    exit 1
fi

if ((timeout_seconds <= 0)); then
    echo "Timeout must be greater than zero" >&2
    exit 1
fi

if ((interval_seconds <= 0)); then
    echo "Interval must be greater than zero" >&2
    exit 1
fi

timestamp() {
    date -Iseconds
}

read_status_line() {
    if [[ -f "$status_file" ]]; then
        cat "$status_file"
    fi
}

status_value() {
    local line
    line="$(read_status_line)"
    if [[ -z "$line" ]]; then
        return 1
    fi
    printf '%s\n' "$line" | awk '{print $2}'
}

running_pid() {
    if [[ -f "$pid_file" ]]; then
        cat "$pid_file"
    fi
}

print_recent_log() {
    if [[ -f "$log_file" ]]; then
        echo "Recent deploy log:"
        tail -n 40 "$log_file"
    else
        echo "Deploy log not found at $log_file"
    fi
}

deadline=$((SECONDS + timeout_seconds))
last_report=""

while ((SECONDS < deadline)); do
    current_status="$(status_value || true)"
    current_pid="$(running_pid || true)"

    if [[ -n "$current_status" && "$current_status" != "$last_report" ]]; then
        echo "[$(timestamp)] deploy status: $current_status"
        last_report="$current_status"
    fi

    case "$current_status" in
        success)
            echo "Deploy completed successfully."
            exit 0
            ;;
        failed*)
            echo "Deploy failed with status: $current_status" >&2
            print_recent_log >&2
            exit 1
            ;;
        skipped*)
            echo "Deploy did not run: $current_status" >&2
            exit 1
            ;;
    esac

    if [[ -n "$current_pid" ]] && kill -0 "$current_pid" 2>/dev/null; then
        sleep "$interval_seconds"
        continue
    fi

    if [[ -n "$current_status" ]]; then
        sleep "$interval_seconds"
        continue
    fi

    echo "Deploy has not started yet. Waiting..." >&2
    sleep "$interval_seconds"
done

echo "Timed out waiting for deploy completion after ${timeout_seconds}s" >&2
print_recent_log >&2
exit 124
