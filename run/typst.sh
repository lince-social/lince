#!/usr/bin/env bash

set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
repo_root="$(CDPATH= cd -- "${script_dir}/.." >/dev/null 2>&1 && pwd)"

cd "$repo_root"

docs_root="${DOCS_ROOT:-documentation}"
docs_source="${docs_root}/instinto.typ"
lince_version="${LINCE_VERSION:-$(awk 'BEGIN{in_pkg=0} /^[[]package[]]$/{in_pkg=1;next} /^[[]/{in_pkg=0} in_pkg&&/^version = /{print $3;exit}' crates/lince/Cargo.toml | xargs)}"

usage() {
    cat <<'EOF'
Usage:
  ./run/typst.sh docs preview [--light]
  ./run/typst.sh docs compile
EOF
}

preview_dark="true"

compile_docs() {
    echo "  -> Compiling light version..."
    typst compile --root . --input dark=false --input lince_version="$lince_version" \
        "$docs_source" "${docs_root}/lince-documentation-light.pdf"

    echo "  -> Compiling dark version..."
    typst compile --root . --input dark=true --input lince_version="$lince_version" \
        "$docs_source" "${docs_root}/lince-documentation-dark.pdf"

    echo "Done!"
}

docs_preview() {
    trap compile_docs EXIT

    tinymist preview \
        --root . \
        --control-plane-host 127.0.0.1:3002 \
        --data-plane-host 127.0.0.1:3001 \
        --static-file-host 127.0.0.1:3003 \
        --input dark="$preview_dark" \
        --input lince_version="$lince_version" \
        "$docs_source"
}

target="${1:-}"
action="${2:-}"

if [ -z "$target" ] || [ -z "$action" ]; then
    usage
    exit 1
fi

shift 2

while [ $# -gt 0 ]; do
    case "$1" in
        -l|--light)
            preview_dark="false"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
done

case "$target:$action" in
    docs:preview)
        docs_preview
        ;;
    docs:compile)
        compile_docs
        ;;
    *)
        echo "error: unsupported Typst workflow: ${target} ${action}" >&2
        usage
        exit 1
        ;;
esac
