#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir_default="$(cd "$script_dir/../.." && pwd)"

timestamp() {
    date -Iseconds
}

run_as_root() {
    if ((EUID == 0)); then
        "$@"
    else
        sudo "$@"
    fi
}

install_remote_binary() {
    local repo_dir="${REPO_DIR:-/root/git/lince-social/lince}"
    local branch="${BRANCH:-dev}"
    local service_name="${SERVICE_NAME:-lince}"
    local remote_binary="${REMOTE_BINARY:-/tmp/lince-deploy/deploy-artifact/lince}"

    cd "$repo_dir"

    git fetch origin "$branch"
    git checkout "$branch"
    git reset --hard "origin/$branch"

    install -d "$repo_dir/target/release"
    install -m 755 "$remote_binary" "$repo_dir/target/release/lince"

    bash "$repo_dir/scripts/install/install-system-service.sh" \
        --service-name "$service_name" \
        --description "Lince Social HTTP API" \
        --user root \
        --working-directory "$repo_dir" \
        --exec-start "$repo_dir/target/release/lince --http-api-only"

    run_as_root systemctl restart "$service_name"
    run_as_root systemctl status "$service_name" --no-pager -l
}

worker() {
    local repo_dir="${REPO_DIR:-$repo_dir_default}"
    local service_name="${SERVICE_NAME:-lince}"
    local state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
    local log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
    local pid_file="$state_dir/deploy.pid"
    local status_file="$state_dir/status"
    local lock_dir="$state_dir/lock"

    mkdir -p "$state_dir"
    touch "$log_file"

    exec >>"$log_file" 2>&1

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
    bash "$repo_dir/scripts/install/install-system-service.sh" \
        --service-name "$service_name" \
        --description "Lince Social HTTP API" \
        --user root \
        --working-directory "$repo_dir" \
        --exec-start "$repo_dir/target/release/lince --http-api-only"

    write_status "restarting-service"
    run_as_root systemctl restart "$service_name"
    run_as_root systemctl status "$service_name" --no-pager -l

    echo "[$(timestamp)] deploy finished"
}

trigger() {
    local repo_dir="${REPO_DIR:-$repo_dir_default}"
    local state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
    local log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
    local pid_file="$state_dir/deploy.pid"

    mkdir -p "$state_dir"

    if [[ -f "$pid_file" ]]; then
        local existing_pid
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
        bash "$script_dir/dev.sh" worker </dev/null >/dev/null 2>&1 &

    local new_pid=$!
    echo "$new_pid" >"$pid_file"

    echo "Started background deploy with pid $new_pid"
    echo "Log: $log_file"
}

poll() {
    local state_dir="${LINCE_DEPLOY_STATE_DIR:-/var/tmp/lince-deploy-dev}"
    local status_file="$state_dir/status"
    local pid_file="$state_dir/deploy.pid"
    local log_file="${LINCE_DEPLOY_LOG_FILE:-$state_dir/deploy.log}"
    local timeout_seconds=900
    local interval_seconds=5

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

    if ((timeout_seconds <= 0 || interval_seconds <= 0)); then
        echo "Timeout and interval must be greater than zero" >&2
        exit 1
    fi

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

    local deadline=$((SECONDS + timeout_seconds))
    local last_report=""

    while ((SECONDS < deadline)); do
        local current_status
        local current_pid
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
}

usage() {
    cat <<'EOF'
Deployment commands for the dev environment.

Usage:
  ./scripts/deploy/dev.sh <command> [options]

Commands:
  trigger                Start a background deploy on the current host.
  poll                   Wait for a background deploy to finish.
  worker                 Internal worker command used by trigger.
  install-remote-binary  Install a prebuilt binary on the VPS and restart systemd.
EOF
}

main() {
    local command="${1:-}"
    case "$command" in
        trigger)
            shift
            trigger "$@"
            ;;
        poll)
            shift
            poll "$@"
            ;;
        worker)
            shift
            worker "$@"
            ;;
        install-remote-binary)
            shift
            install_remote_binary "$@"
            ;;
        -h|--help|help)
            usage
            ;;
        *)
            echo "Unknown or missing command: ${command:-<none>}" >&2
            usage >&2
            exit 1
            ;;
    esac
}

main "$@"
