# Isolation

Confirm this with the owner before spawning any builder. Parallelism without it
is agents overwriting each other in one tree.

## Checkouts

Give each builder that compiles its own **clone** on its own branch, and push
back for the integrator to merge. Agents that only read — planning, review,
drift checks — share one checkout and need nothing.

Clones, specifically: this project bans worktrees. Do not call `EnterWorktree`,
and do not pass `isolation: "worktree"` to `Agent`. If the owner declines the
disk cost of clones, the fallback is serializing builders on `dev`, which means
width 1 — say so plainly rather than pretending the run is parallel.

## Why separate trees, beyond source conflicts

- Cargo takes an exclusive lock on `target/`, so agents sharing a tree
  serialize on the build lock however isolated their edits are.
- A running app binds a fixed port and a fixed data directory. Two of them
  collide regardless of source isolation.

## Per-builder environment

- `CARGO_TARGET_DIR` — one per tree. Expect a full copy of the dependency graph
  per builder; heavy graphics stacks make this the real disk cost.
- `CARGO_HOME` — shared. The registry is read-mostly and has its own lock.
- `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` — one per builder, so each app run gets
  its own database and cache. Check how the project resolves its data
  directory; if it is a compile-time constant rather than an environment
  lookup, making it configurable is a bootstrap node.
- Listen port — one per builder. A hardcoded port is a bootstrap node.

## Exclusive resources

Some proofs cannot be parallelized on one machine: GPU and display sessions,
frame-timing benchmarks, anything binding a well-known port, anything measuring
the machine. These run only in the inspector's single slot, one at a time, with
nothing else loading the machine. Builders never run them.
