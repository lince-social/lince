#!/usr/bin/env bash

set -euo pipefail

repo_dir=""
lince_bin=""
service_user="root"
service_group=""

usage() {
    cat <<'EOF'
Install both institute and global Lince systemd services.

Usage:
  ./scripts/install/install-institute-services.sh \
    --repo-dir /root/git/lince-social/lince \
    --lince-bin /root/git/lince-social/lince/target/release/lince

Options:
  --repo-dir <dir>     Working directory for both services.
  --lince-bin <path>   Path to the built lince binary.
  --user <user>        Service user. Default: root
  --group <group>      Service group.
  -h, --help           Show this help text.
EOF
}

while (($# > 0)); do
    case "$1" in
        --repo-dir)
            repo_dir="$2"
            shift 2
            ;;
        --lince-bin)
            lince_bin="$2"
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

[[ -n "$repo_dir" ]] || {
    echo "--repo-dir is required" >&2
    exit 1
}

[[ -n "$lince_bin" ]] || {
    echo "--lince-bin is required" >&2
    exit 1
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
installer="${script_dir}/install-system-service.sh"

common_args=(
    --working-directory "$repo_dir"
    --lince-bin "$lince_bin"
    --user "$service_user"
)

if [[ -n "$service_group" ]]; then
    common_args+=(--group "$service_group")
fi

"$installer" --profile institute "${common_args[@]}"
"$installer" --profile global "${common_args[@]}"

printf 'Installed institute and global services.\n'
