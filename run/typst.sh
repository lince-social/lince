#!/usr/bin/env bash

set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
repo_root="$(CDPATH= cd -- "${script_dir}/.." >/dev/null 2>&1 && pwd)"

cd "$repo_root"

docs_root="${DOCS_ROOT:-documentation/technical}"
first_steps_root="${FIRST_STEPS_ROOT:-documentation/first_steps}"
docs_source="${docs_root}/lince-documentation.typ"
first_steps_source="${first_steps_root}/first_steps.typ"
lince_version="${LINCE_VERSION:-$(awk 'BEGIN{in_pkg=0} /^[[]package[]]$/{in_pkg=1;next} /^[[]/{in_pkg=0} in_pkg&&/^version = /{print $3;exit}' crates/lince/Cargo.toml | xargs)}"

usage() {
    cat <<'EOF'
Usage:
  ./run/typst.sh docs preview [--light]
  ./run/typst.sh docs compile
  ./run/typst.sh first-steps preview [--light]
  ./run/typst.sh first-steps all
  ./run/typst.sh first-steps html
  ./run/typst.sh first-steps slides
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

compile_first_steps_preview_outputs() {
    echo "  -> Compiling first steps light version..."
    typst compile --root . --input dark=false --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-light.pdf"

    echo "  -> Compiling first steps dark version..."
    typst compile --root . --input dark=true --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-dark.pdf"

    echo "Done!"
}

compile_first_steps_lang() {
    lang="$1"
    suffix="$2"
    lang_args=()

    if [ "$lang" = "pt" ]; then
        lang_args+=(--input lang=pt)
    fi

    echo "  -> Compiling ${lang} light version..."
    typst compile --root . "${lang_args[@]}" --input dark=false --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-light${suffix}.pdf"

    echo "  -> Compiling ${lang} dark version..."
    typst compile --root . "${lang_args[@]}" --input dark=true --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-dark${suffix}.pdf"

    echo "  -> Compiling ${lang} slides version..."
    typst compile --root . "${lang_args[@]}" --input slides=true --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-slides${suffix}.pdf"
}

compile_first_steps_html() {
    echo "  -> Compiling English HTML..."
    touying compile --root . --format html \
        --output "${first_steps_root}/first_steps.html" \
        --sys-inputs '{"slides":"true","lang":"en"}' \
        "$first_steps_source"

    echo "  -> Compiling Portuguese HTML..."
    touying compile --root . --format html \
        --output "${first_steps_root}/first_steps-pt.html" \
        --sys-inputs '{"slides":"true","lang":"pt"}' \
        "$first_steps_source"

    echo "Done!"
}

compile_first_steps_slides_preview_outputs() {
    echo "  -> Compiling first steps slides PDF..."
    typst compile --root . --input slides=true --input lince_version="$lince_version" \
        "$first_steps_source" "${first_steps_root}/first_steps-slides.pdf"

    echo "  -> Compiling first steps slides HTML..."
    touying compile --root . --format html \
        --output "${first_steps_root}/first_steps.html" \
        --sys-inputs '{"slides":"true","lang":"en"}' \
        "$first_steps_source"

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

first_steps_preview() {
    trap compile_first_steps_preview_outputs EXIT

    tinymist preview \
        --root . \
        --control-plane-host 127.0.0.1:3012 \
        --data-plane-host 127.0.0.1:3011 \
        --static-file-host 127.0.0.1:3013 \
        --input dark="$preview_dark" \
        --input lince_version="$lince_version" \
        "$first_steps_source"
}

first_steps_all() {
    compile_first_steps_lang en ""
    compile_first_steps_lang pt "-pt"
    echo "Done!"
}

first_steps_slides() {
    trap compile_first_steps_slides_preview_outputs EXIT

    tinymist preview \
        --root . \
        --control-plane-host 127.0.0.1:3022 \
        --data-plane-host 127.0.0.1:3021 \
        --static-file-host 127.0.0.1:3023 \
        --input slides=true \
        --input lince_version="$lince_version" \
        "$first_steps_source"
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
    first-steps:preview)
        first_steps_preview
        ;;
    first-steps:all)
        first_steps_all
        ;;
    first-steps:html)
        compile_first_steps_html
        ;;
    first-steps:slides)
        first_steps_slides
        ;;
    *)
        echo "error: unsupported Typst workflow: ${target} ${action}" >&2
        usage
        exit 1
        ;;
esac
