# Lince `.lingua` bootstrap

This file locates current authorities. It is not a second schema.

## Format and File Sync

- `crates/engine/src/lingua_file.rs`: parser, renderer, and round-trip rules.
- `crates/engine/src/file_sync.rs`: write-back, refusal, precedence, and
  deletion behavior.
- `crates/engine/tests/file_sync.rs`: executable behavior, including task
  assignment.

Read the applicable source. A property may not exist in every revision.

## `docs/records` bundle

- `tools/instinct/CONTRACT.txt`: bundle rules.
- `tools/instinct/check_lingua.js`: adoptability check.
- `crates/engine/src/instinct.rs`: vocabulary, hierarchy, ordering, and import.
- `crates/engine/build.rs`: embedding and the `@instinct` invariant.

The shipped bundle freezes its vocabulary and requires internal links to
resolve. A live `@assigned-to` link may therefore be valid in File Sync but
invalid in the source bundle when its Agent Record is outside the bundle. Run
the checker before writing coordination state. If the bundle cannot represent
it, record the assignment in live Lince or the current harness; do not weaken
the bundle or fabricate an Agent inside it.

After changing `docs/records`, run:

```sh
node tools/instinct/check_lingua.js docs/records
cargo check -p engine
```

When format, import, or File Sync behavior changes, also run:

```sh
cargo test -p engine --test file_sync --test agents
```

## Context order

Load only what the task needs:

1. Relevant `@@chapter` and `@@idea` Instinct Records for Lince fundamentals.
2. `Ontology - 14. Lince today, in one read.lingua` for the implementation map.
3. Exact topic Concepts and their structural links.
4. The relevant `@@document` or `@@section` Records.
5. The relevant `@@task` Record.

The Records contain Lince's evolving explanation; do not duplicate it here.
