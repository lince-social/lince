---
name: graph-work
description: Run a large refactor or build as a dependency graph of proof-carrying nodes worked by parallel agents. Use when the user wants many agents building in parallel, asks to plan/cut/schedule work as a graph or DAG, wants to change the plan or agent count of a run already underway, or asks how work gets certified before it lands.
---

# Graph work

You are the **planner**. You are the only agent the owner talks to, the only
writer of the graph file, and the only spawner of other agents. Everything
below is your job; the other roles exist because you dispatched them.

Three roles sit under you:

- **builder** — implements exactly one node, in its own checkout. Default 4.
- **integrator** — owns merge order, shared files, and re-proving what is
  already merged. Exactly 1.
- **inspector** — owns the exclusive resources: full builds, the running app,
  benchmarks, anything that needs the real machine. Exactly 1, single-slot.

## The graph file

One markdown file, in your checkout, beside the plan it derives from
(`anicca/interface/graph.md` for the interface refactor). You are its only
writer — builders report to you through `SendMessage` and you record. The file
never travels through a merge.

A **node** is one thing one agent finishes in one session, and it carries the
command that proves it:

```
### N7 — Configurable desktop listen port
owns:      crates/desktop/src/runtime.rs, crates/desktop/src/lib.rs
deps:      —
check:     cargo check -p desktop
prove:     cargo test -p desktop listen_addr
exclusive: none
status:    claimed(builder-2) | in-review | merged | certified
```

- **owns** — the file set this builder may edit. Nothing outside it. A node
  that needs a change in someone else's files raises a new node instead.
- **deps** — nodes that must be `certified` first, not merely merged.
- **check** — the fast inner loop, narrowest scope that type-checks the change.
- **prove** — what certifies it. Must be runnable and must fail if the node is
  wrong. A node with no `prove` is not a node yet: split it until it has one.
- **exclusive** — `none`, or the contended resource (`gpu`, `app-port`,
  `data-dir`). Anything non-`none` runs only in the inspector's single slot.

Shared files — `Cargo.toml`, `Cargo.lock`, cross-crate traits, the workspace
manifest — belong to the integrator. No builder owns them.

## Running the graph

1. **Cut the plan into nodes.** Prose plans are not nodes. Do this yourself,
   with at most one helper agent; fanning out here produces a graph nobody can
   schedule. Where the plan is a `.lingua` Record, read it through the `lingua`
   skill — it is the owner's, so you cut nodes from it without editing it.
   Depth is the critical path, so spend the effort making the deep chain short
   and the wide parts genuinely independent.
2. **Confirm isolation with the owner before spawning anything.** See
   [ISOLATION.md](ISOLATION.md). Blocking — parallel builders are fake without
   it.
3. **Land the bootstrap nodes first.** A node that unblocks width — a hardcoded
   port, a data directory that cannot be redirected, a lock two agents contend
   on — runs at width 1, before the fan-out. Width stays 1 until they are
   certified.
4. **Dispatch the frontier.** Nodes whose deps are all `certified`. Spawn each
   builder as a cold agent (`Agent`, default `subagent_type`) with the node
   spec as its brief — a fork inherits your planning conversation and wastes
   the tokens. Wait for the completion notification; do not poll.
5. **Integrate.** The integrator merges one branch at a time, resolves shared
   files, and re-runs `prove` for the merged node *and every node already
   merged*. That re-run is what catches silent regression and is the single
   highest-value thing to spend budget on.
6. **Inspect.** The inspector takes anything `exclusive`, plus a spec-drift
   read: node spec beside the diff, answering only "does this do what was
   planned", separately from "is this correct code". Its queue is serial by
   construction — one GPU, one port, one machine.
7. **Record and re-dispatch.** Write the outcome into the graph file, then fill
   the freed builder slot from the new frontier.

`merged` is not `certified`. A node is certified when its `prove` passed on the
integrator's tree *after* the merge, and its exclusive gates passed on the
inspector's.

## Behaviour you set for builders

Put these in every builder brief:

- Edit only the node's `owns` set. Anything else is a message back to you.
- `check` is the loop; `prove` is the handoff gate. Run both before reporting.
- `cargo check` type-checks without codegen or linking, several times faster,
  and certifies nothing. `cargo test` pays a full build for that crate's graph,
  and shares no artifacts with `check` — for a node whose `prove` is a test,
  run the test and skip the redundant check.
- Full workspace builds belong to the inspector.
- Report as: node id, `check` output, `prove` output, files touched, anything
  wanted outside `owns`.

## Where the budget goes

Not on more builders — past ~4 the drift and re-review cost more than the
parallelism returns. Spend it on:

- Highest effort per builder, plus self-verification before handoff: its own
  `prove`, and the `prove` of every node it shares a boundary with.
- Two independent builders on a risky or ambiguous node, same spec, keep the
  better diff. Cheap against the worst outcome, an architecture everything
  downstream is built on.
- Re-proving the whole merged set after every merge, not once at the end.
- A spec-drift read per merge candidate.

## Changing the plan

The owner changes the plan by talking to you. Re-cut affected nodes, mark
in-flight ones stale, tell their builders to stop, and rewrite the graph file.
Builder count is yours to set on request — 4 is the default, not a limit.

## Worked node

From the interface refactor, where `prove` is a GPU report rather than a test:

```
### C3-2 — Configuration panel reaches every supported scope
owns:      crates/interface/src/style.rs, crates/interface/src/joined_panel.rs
deps:      C3-1
check:     cargo check -p interface --features native
prove:     mise run interface-lab -- --scenario configuration
exclusive: gpu
status:    in-review
```

Its evidence is the report's own numbers — source fingerprint and the frame
p95/p99 thresholds — recorded in the graph file beside the node, because a
threshold that passed on a loaded GPU proves nothing. Benchmarks never run
concurrently.
