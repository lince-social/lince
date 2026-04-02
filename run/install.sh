#!/usr/bin/env bash

set -euo pipefail

umask 022

LINCE_REPOSITORY="${LINCE_REPOSITORY:-lince-social/lince}"
DEFAULT_DOWNLOAD_BASE_URL="https://github.com/${LINCE_REPOSITORY}/releases/download"
DOWNLOAD_BASE_URL="${LINCE_DOWNLOAD_BASE_URL:-$DEFAULT_DOWNLOAD_BASE_URL}"
VERSION="${LINCE_VERSION:-rolling}"
PREFIX="${LINCE_INSTALL_PREFIX:-${HOME:-$PWD}/.local}"
BIN_DIR="${LINCE_BIN_DIR:-$PREFIX/bin}"
TARGET="${LINCE_TARGET:-}"
FORCE=0

tmp_dir=""

log() {
    printf '%s\n' "$*" >&2
}

fail() {
    log "error: $*"
    exit 1
}

usage() {
    cat <<'EOF'
Install Lince from a published binary.

Usage:
  ./run/install.sh [options]

Options:
  --version <tag>      Release tag to install. Defaults to "rolling".
  --prefix <dir>       Installation prefix. Defaults to "$HOME/.local".
  --bin-dir <dir>      Binary directory. Overrides --prefix/bin.
  --base-url <url>     Release download base URL.
  --target <triple>    Override the detected target triple.
  --force              Overwrite an existing binary.
  -h, --help           Show this help text.

Environment overrides:
  LINCE_VERSION
  LINCE_INSTALL_PREFIX
  LINCE_BIN_DIR
  LINCE_DOWNLOAD_BASE_URL
  LINCE_TARGET
EOF
}

cleanup() {
    if [ -n "$tmp_dir" ] && [ -d "$tmp_dir" ]; then
        rm -rf "$tmp_dir"
    fi
}

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

need_cmd() {
    have_cmd "$1" || fail "required command not found: $1"
}

download_file() {
    url="$1"
    destination="$2"

    if have_cmd curl; then
        curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$destination"
        return
    fi

    if have_cmd wget; then
        wget -qO "$destination" "$url"
        return
    fi

    fail "need curl or wget to download ${url}"
}

sha256_file() {
    file_path="$1"

    if have_cmd sha256sum; then
        sha256sum "$file_path" | awk '{print $1}'
        return
    fi

    if have_cmd shasum; then
        shasum -a 256 "$file_path" | awk '{print $1}'
        return
    fi

    return 1
}

verify_checksum_if_possible() {
    checksum_path="$1"
    archive_path="$2"

    if [ ! -f "$checksum_path" ]; then
        return 0
    fi

    if ! actual_sum="$(sha256_file "$archive_path")"; then
        log "warning: skipping checksum verification because neither sha256sum nor shasum is available"
        return 0
    fi

    expected_sum="$(awk '{print $1}' "$checksum_path" | tr -d '\r\n')"
    [ -n "$expected_sum" ] || fail "checksum file at ${checksum_path} is empty"

    if [ "$actual_sum" != "$expected_sum" ]; then
        fail "checksum verification failed for ${archive_path}"
    fi
}

is_glibc_linux() {
    if have_cmd getconf && getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
        return 0
    fi

    if have_cmd ldd && ldd --version 2>&1 | grep -Eiq 'glibc|gnu libc'; then
        return 0
    fi

    return 1
}

detect_target() {
    kernel="$(uname -s)"
    machine="$(uname -m)"

    case "$machine" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            fail "unsupported CPU architecture: ${machine}"
            ;;
    esac

    case "$kernel" in
        Linux)
            is_glibc_linux || fail "published Linux binaries currently require glibc"
            os="unknown-linux-gnu"
            ;;
        Darwin)
            os="apple-darwin"
            ;;
        *)
            fail "unsupported operating system: ${kernel}"
            ;;
    esac

    printf '%s-%s\n' "$arch" "$os"
}

path_has_dir() {
    case ":${PATH:-}:" in
        *":$1:"*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            [ $# -ge 2 ] || fail "--version requires a value"
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            [ $# -ge 2 ] || fail "--prefix requires a value"
            PREFIX="$2"
            BIN_DIR="$PREFIX/bin"
            shift 2
            ;;
        --bin-dir)
            [ $# -ge 2 ] || fail "--bin-dir requires a value"
            BIN_DIR="$2"
            shift 2
            ;;
        --base-url)
            [ $# -ge 2 ] || fail "--base-url requires a value"
            DOWNLOAD_BASE_URL="$2"
            shift 2
            ;;
        --target)
            [ $# -ge 2 ] || fail "--target requires a value"
            TARGET="$2"
            shift 2
            ;;
        --force)
            FORCE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

need_cmd uname
need_cmd mkdir
need_cmd mktemp
need_cmd tar
need_cmd chmod
need_cmd mv
need_cmd rm
need_cmd awk
need_cmd grep
need_cmd tr

if [ -z "$TARGET" ]; then
    TARGET="$(detect_target)"
fi

archive_name="lince-${TARGET}.tar.gz"
checksum_name="${archive_name}.sha256"
archive_url="${DOWNLOAD_BASE_URL%/}/${VERSION}/${archive_name}"
checksum_url="${DOWNLOAD_BASE_URL%/}/${VERSION}/${checksum_name}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/lince-install.XXXXXX")"
trap cleanup EXIT INT TERM

archive_path="${tmp_dir}/${archive_name}"
checksum_path="${tmp_dir}/${checksum_name}"

log "Downloading ${archive_url}"
if ! download_file "$archive_url" "$archive_path"; then
    fail "unable to download ${archive_url}"
fi

if download_file "$checksum_url" "$checksum_path" 2>/dev/null; then
    verify_checksum_if_possible "$checksum_path" "$archive_path"
else
    log "warning: checksum file not found at ${checksum_url}; skipping verification"
fi

tar -xzf "$archive_path" -C "$tmp_dir"

binary_source="${tmp_dir}/lince-${TARGET}"
[ -f "$binary_source" ] || fail "archive did not contain lince-${TARGET}"

mkdir -p "$BIN_DIR"
install_path="${BIN_DIR%/}/lince"

if [ -e "$install_path" ] && [ "$FORCE" -ne 1 ]; then
    fail "${install_path} already exists; rerun with --force to replace it"
fi

chmod 0755 "$binary_source"
mv "$binary_source" "$install_path"

config_root="${XDG_CONFIG_HOME:-${HOME:-$PWD}/.config}"
config_dir="${config_root%/}/lince"

log
log "Lince installed to ${install_path}"
log "Config and SQLite state will live under ${config_dir}"
log
log "Run it:"
log "  ${install_path} --help"
log
log "Run the HTTP server:"
log "  ${install_path} --listen-addr 0.0.0.0:6174"
log
log "Optional ways to keep it running:"
log "  Docker: docker run -d --name lince --restart unless-stopped -p 6174:6174 -v lince-data:/var/lib/lince ghcr.io/lince-social/lince:rolling"

if [ "$(uname -s)" = "Linux" ]; then
    log "  systemd: adapt https://raw.githubusercontent.com/${LINCE_REPOSITORY}/main/run/systemd/lince.service"
fi

if ! path_has_dir "$BIN_DIR"; then
    log
    log "Add Lince to your PATH:"
    log "  export PATH=\"${BIN_DIR}:\$PATH\""
fi
