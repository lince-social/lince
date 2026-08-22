# Lince `.lingua` bootstrap

This file locates current authorities. It is not a second schema.

## One source tree and one parser

- `anicca/`: checked-in Lince documentation, declarations, and task plan.
- `anicca/Lingua.lingua`: living language explanation and design decisions.
- `anicca/Ontology.lingua`: the one project task list.
- `crates/anicca/src/grammar.rs`: typed `rust-sitter` syntax authority.
- `crates/anicca/src/lib.rs`: projection, validation, formatting, uid minting,
  and machine-state contract.
- `crates/anicca/src/main.rs`: `anicca check` and `anicca fmt` interface.
- `crates/engine/src/file_sync.rs`: database application, write-back,
  commitments, conflicts, and execution order when Anicca File Sync is wired.
- `crates/engine/tests/file_sync.rs`: executable File Sync behavior.

`docs/records/`, `crates/engine/src/lingua_file.rs`, and JavaScript converters
belong to the superseded projection format. Do not read them as authority,
write new files there, or add a compatibility parser.

## Validation

After changing checked-in `.lingua`:

```sh
cargo run --offline -p anicca -- check anicca
cargo run --offline -p anicca -- fmt anicca
```

After changing grammar, formatting, projection, or File Sync behavior:

```sh
cargo test --offline -p anicca
cargo test --offline -p engine --test file_sync
cargo check --offline -p engine
```

Warnings are errors. Use `cargo check`, not `cargo build`.

## Context order

1. `anicca/Lingua.lingua` for syntax or projection work.
2. The relevant `is #chapter` or other identity Record.
3. Exact Concepts and binary assertions connected to it.
4. `anicca/Ontology.lingua` for current work and ordering.

The Records contain Lince's evolving explanation; do not duplicate it here.
