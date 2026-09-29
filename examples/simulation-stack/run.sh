#!/usr/bin/env bash
set -euo pipefail

probe_root=$(cd "$(dirname "$0")/../.." && pwd)
probe_dir="$probe_root/examples/simulation-stack"
probe_output=${LINCE_STACK_EXPERIMENT_DIR:-"$probe_root/target/simulation-experiments"}
probe_jobs=${LINCE_STACK_EXPERIMENT_JOBS:-2}
octopii_revision=b54c112d7db7ed1195dd4f600b4b786df85c8e7e
patchbay_revision=c4147e142136249933a868337e8f3c6bf4bdd18f
mkdir -p "$probe_output"
cd "$probe_root"

prepare() {
    local name=$1 repository=$2 revision=$3
    if [ ! -f "$probe_output/$name/.lince-probe-revision" ]; then
        if [ -e "$probe_output/$name" ]; then
            printf 'Unmarked source directory exists: %s\n' "$probe_output/$name" >&2
            exit 1
        fi
        mkdir -p "$probe_output/$name"
        curl -fsSL "https://codeload.github.com/$repository/tar.gz/$revision" |
            tar -xz --strip-components=1 -C "$probe_output/$name"
        printf '%s\n' "$revision" > "$probe_output/$name/.lince-probe-revision"
    fi
    test "$(cat "$probe_output/$name/.lince-probe-revision")" = "$revision"
}

owned() {
    rustc --edition=2024 -D warnings -C link-arg=-fuse-ld=bfd --test \
        "$probe_dir/owned_probe.rs" -o "$probe_output/owned-probe"
    "$probe_output/owned-probe" --nocapture | tee "$probe_output/owned-probe-test.log"
}

octopii() {
    command -v sqlite3 >/dev/null
    prepare octopii octopii-rs/octopii "$octopii_revision"
    cp "$probe_dir/octopii_probe.rs" "$probe_output/octopii/tests/lince_probe.rs"
    if ! rg -q '^\[workspace\]' "$probe_output/octopii/Cargo.toml"; then
        printf '\n[workspace]\nexclude = ["openraft"]\n' >> "$probe_output/octopii/Cargo.toml"
    fi
    export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
    cargo check --manifest-path "$probe_output/octopii/Cargo.toml" \
        --features simulation --test lince_probe --locked -j "$probe_jobs" \
        2>&1 | tee "$probe_output/octopii-probe-check.log"
    cargo test --manifest-path "$probe_output/octopii/Cargo.toml" \
        --features simulation --test lince_probe --locked -j "$probe_jobs" \
        -- --nocapture --test-threads=1 2>&1 | tee "$probe_output/octopii-probe-test.log"
}

patchbay() {
    command -v nft >/dev/null
    command -v tc >/dev/null
    prepare patchbay n0-computer/patchbay "$patchbay_revision"
    cp "$probe_dir/patchbay_probe.rs" "$probe_output/patchbay/patchbay/examples/lince_probe.rs"
    if ! rg -q '^\[dev-dependencies.iroh\]' "$probe_output/patchbay/patchbay/Cargo.toml"; then
        printf '\n[dev-dependencies.iroh]\nversion = "=1.2.0"\n' >> "$probe_output/patchbay/patchbay/Cargo.toml"
    fi
    if ! rg -q '^\[dev-dependencies.iroh-mdns-address-lookup\]' "$probe_output/patchbay/patchbay/Cargo.toml"; then
        printf '\n[dev-dependencies.iroh-mdns-address-lookup]\nversion = "=0.5.0"\n' >> "$probe_output/patchbay/patchbay/Cargo.toml"
    fi
    cp "$probe_dir/patchbay.lock" "$probe_output/patchbay/Cargo.lock"
    export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
    cargo check --manifest-path "$probe_output/patchbay/Cargo.toml" \
        -p patchbay --example lince_probe --locked -j "$probe_jobs" \
        2>&1 | tee "$probe_output/patchbay-probe-check.log"
    timeout 600 cargo run --manifest-path "$probe_output/patchbay/Cargo.toml" \
        -p patchbay --example lince_probe --locked -j "$probe_jobs" \
        2>&1 | tee "$probe_output/patchbay-probe-test.log"
}

patchbay_lince() {
    command -v nft >/dev/null
    command -v tc >/dev/null
    local worker=${LINCE_SIMULATION_WORKER:-"$probe_root/target/debug/lince-simulation-worker"}
    test -x "$worker"
    prepare patchbay n0-computer/patchbay "$patchbay_revision"
    cp "$probe_dir/lince_patchbay.rs" "$probe_output/patchbay/patchbay/examples/lince_cells.rs"
    if ! rg -q '^\[dev-dependencies.iroh\]' "$probe_output/patchbay/patchbay/Cargo.toml"; then
        printf '\n[dev-dependencies.iroh]\nversion = "=1.2.0"\n' >> "$probe_output/patchbay/patchbay/Cargo.toml"
    fi
    if ! rg -q '^\[dev-dependencies.iroh-mdns-address-lookup\]' "$probe_output/patchbay/patchbay/Cargo.toml"; then
        printf '\n[dev-dependencies.iroh-mdns-address-lookup]\nversion = "=0.5.0"\n' >> "$probe_output/patchbay/patchbay/Cargo.toml"
    fi
    cp "$probe_dir/patchbay.lock" "$probe_output/patchbay/Cargo.lock"
    export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
    cargo check --manifest-path "$probe_output/patchbay/Cargo.toml" \
        -p patchbay --example lince_cells --locked -j "$probe_jobs"
    local result
    result=$(mktemp -d "$probe_output/full-cells.XXXXXX")
    rmdir "$result"
    timeout 600 cargo run --manifest-path "$probe_output/patchbay/Cargo.toml" \
        -p patchbay --example lince_cells --locked -j "$probe_jobs" -- "$worker" "$result"
}

lince() {
    cargo check -p engine --test simulation_stack_probe --locked -j "$probe_jobs" \
        2>&1 | tee "$probe_output/lince-probe-check.log"
    cargo test -p engine --test simulation_stack_probe --locked -j "$probe_jobs" \
        -- --ignored --nocapture --test-threads=1 2>&1 | tee "$probe_output/lince-probe-test.log"
}

case "${1:-all}" in
    owned) owned ;;
    octopii) octopii ;;
    patchbay) patchbay ;;
    patchbay-lince) patchbay_lince ;;
    lince) lince ;;
    all) (owned); (octopii); (patchbay); (lince) ;;
    *) printf 'Usage: bash %s [owned|octopii|patchbay|patchbay-lince|lince|all]\n' "$0" >&2; exit 2 ;;
esac
