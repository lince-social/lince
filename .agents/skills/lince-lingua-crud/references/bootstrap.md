# Lince `.lingua` bootstrap

Use this reference to find the live authorities. It is a route, not a second
copy of the format schema.

## Format authority

- `crates/engine/src/lingua_file.rs`: current parser, renderer, data shape,
  stable semantics, and focused round-trip tests.
- `crates/engine/src/file_sync.rs`: how disk edits become Record changes,
  precedence between formats, refusal, and deletion behavior.
- `crates/engine/tests/file_sync.rs`: executable File Sync and `.lingua`
  behavior.

Read the applicable source rather than assuming every property exists in
every revision.

## Shipped documentation bundle

- `tools/instinct/CONTRACT.txt`: current `docs/records` bundle rules.
- `tools/instinct/check_lingua.js`: deterministic adoptability check.
- `crates/engine/src/instinct.rs`: vocabulary, hierarchy, ordering, and import
  behavior shared by Instinct and First Steps.
- `crates/engine/build.rs`: embedding and the `@instinct` selection invariant.

Run after changing `docs/records`:

```sh
node tools/instinct/check_lingua.js docs/records
cargo check -p engine
```

Run the focused tests when format, import, or File Sync behavior changes:

```sh
cargo test -p engine --test file_sync --test agents
```

## Progressive Lince context

Start with the least context that answers the task:

1. `docs/records/First Steps.lingua` and its `@chapter` children for the human
   introduction.
2. `docs/records/Ontology - 14. Lince today, in one read.lingua` for a compact
   implementation map.
3. Exact topic Concepts when present, then `@part-of`, `@chapter`, and
   `@see-also` links.
4. The topic's `@@document`/`@@section` Records for architecture.
5. Its `@@task` Record for open work and ordering.

Do not load the entire bundle by default. Do not duplicate these Records into
the skill; they are the evolving Lince understanding the skill exists to
unlock.
