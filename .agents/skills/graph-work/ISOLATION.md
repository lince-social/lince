# Isolation

Confirm this with the owner before spawning any builder. Parallelism without it
is agents overwriting each other in one tree.

## Checkouts

Give each source writer its own checkout and branch. The owner now permits
Git worktrees; prefer a fixed reusable worktree pool, with clones still valid
when separate Git metadata is useful. Agents that only read — planning,
review, drift checks — may share a checkout, but review an identified revision
and detect edits during the read. Compilation capacity is separate, under
[COMPILATION.md](COMPILATION.md). Without separate writer checkouts, serialize
implementation at width one.

The owner maintains AGENTS.md and CLAUDE.md. Never edit either to remove the
old ban. Carry the owner's explicit worktree authorization in this run's
briefs; future sessions need the owner to update their persistent instructions
or repeat it. A skill does not override a contrary active project instruction.

Confirm the actual checkout mechanism at launch. A client-managed worktree is
not automatically a separate checkout for every subagent: give each writer an
absolute path and verify it. Use only exposed native controls or ordinary Git
commands, not invented tool parameters. Prefer permanent worktrees for a pool
that must survive chat rotation; if the client manages cleanup, preserve the
pool and candidate refs before archiving its chats.

Worktrees share Git objects and refs, not their working files or Cargo outputs.
The planner/integrator serializes pool and shared-ref operations. Give each
worker a distinct branch; do not check out a branch already used elsewhere,
force-remove a worktree, prune other runs, or rewrite another worker's ref.
Within one repository the integrator resolves the candidate directly; separate
clones require fetching its exact commit. Neither implies a remote push.

Agree the fixed pool once per run, not once per node. Workers reuse their
assigned checkout for successive branches through the handoff protocol in
[WORKER_POOL.md](WORKER_POOL.md). Preserve queued candidate refs and start new
independent work from a certified base; a clean checkout and released process
ownership are prerequisites for switching tasks, not permission to erase work.

Record the real starting source, including exactly which owner-approved dirty
changes belong in it. Plain `git worktree add` starts from a commit and does
not copy uncommitted edits; a client's handoff may copy them. Inspect the result
instead of assuming either behavior. Never capture unrelated credentials,
fixtures or edits in a snapshot. Keep the run ledger and handoff notes outside
auto-cleaned worker trees and compile outputs. Prepare only necessary local
setup; do not copy the owner's database or secrets into each worker.

## Why separate trees, beyond source conflicts

- Cargo takes an exclusive lock on `target/`, so agents sharing a tree
  serialize on the build lock however isolated their edits are.
- A running app binds a fixed port and a fixed data directory. Two of them
  collide regardless of source isolation.

## Per-builder environment

- `CARGO_TARGET_DIR` — one per tree. Each is a full copy of the dependency
  graph: 18 GB and about five minutes the first time. This is why only one tree
  compiles by default — see [COMPILATION.md](COMPILATION.md).
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
