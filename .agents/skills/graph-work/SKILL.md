---
name: graph-work
description: Run a large refactor or build as a dependency graph of proof-carrying nodes worked by parallel agents. Use when the user wants many agents building in parallel, asks to plan/cut/schedule work as a graph or DAG, wants to change the plan or agent count of a run already underway, or asks how work gets certified before it lands.
---

# Graph work

You are the **planner**. You are the only agent the owner talks to, the only
writer of the graph file, and the only spawner of other agents. Everything
below is your job; the other roles exist because you dispatched them.

Three roles sit under you:

- **builder** — implements one claimed node at a time in a reusable checkout, then returns for another. Target up to 4 only when the session slots, ready work and review capacity allow it.
- **integrator** — owns merge order, shared files, and re-proving what is
  already merged. Exactly 1.
- **inspector** — owns the exclusive resources: full builds, the running app,
  benchmarks, anything that needs the real machine. Exactly 1, single-slot.

Agent slots, source checkouts and compiler permits are separate budgets. Reserve
the planner and verification roles before counting builders. If slots are tight,
one verification agent may integrate and inspect in separate serial phases;
do not call that an independent second review of its own edits. The planner or
another non-author reviews integration-authored changes before certification.
Read [WORKER_POOL.md](WORKER_POOL.md) when running the queue, reusing workers or
choosing models and effort. A capability question is not a build launch.
Read [CONTEXT.md](CONTEXT.md) for every multi-node run. Apply its checkpoint
and resume rules to all roles, not just builders; reusing a checkout does not
require keeping an ever-growing conversation.

## The graph file

One markdown file, in your checkout, beside the plan it derives from
(`anicca/interface/graph.md` for the interface refactor). You are its only
writer — builders report through the session's messaging tools and you record. The file
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
2. **Confirm isolation and the compilation scenario with the owner before
   spawning anything.** See [ISOLATION.md](ISOLATION.md) and
   [COMPILATION.md](COMPILATION.md). Both blocking — parallel builders are fake
   without isolation, and a scenario picked by accident costs five minutes and
   18 GB per node.
3. **Land the bootstrap nodes first.** A node that unblocks width — a hardcoded
   port, a data directory that cannot be redirected, a lock two agents contend
   on — runs at width 1, before the fan-out. Width stays 1 until they are
   certified.
4. **Dispatch the frontier.** Nodes whose deps are all `certified`. Claim each
   node once and assign its checkout and permits. Use a focused fresh brief,
   not the whole planning conversation. An existing worker can receive a new
   assignment after handoff; a fresh agent can reuse that same checkout and
   cache. Follow the actual session tools in [WORKER_POOL.md](WORKER_POOL.md).
   Wait for completion notifications; do not busy-poll or let workers claim
   arbitrary checklist entries themselves.
5. **Integrate.** The integrator merges one branch at a time, resolves shared
   files, and re-runs `prove` for the merged node *and every node already
   merged*. That re-run is what catches silent regression and is the single
   highest-value thing to spend budget on.
6. **Inspect.** The inspector takes anything `exclusive`, plus a spec-drift
   read: node spec beside the diff, answering only "does this do what was
   planned", separately from "is this correct code". Its queue is serial by
   construction — one GPU, one port, one machine.
7. **Record and re-dispatch.** Queue each immutable candidate and refill its
   builder from independent certified-ready work without waiting for unrelated
   review. Bound the pending queue and prioritize repairs and critical-path
   certification. Keep coordinating until the requested milestone is certified
   or a real decision, permission, unavailable proof or session limit requires
   the owner; an idle worker is not a reason to end the run.

`merged` is not `certified`. A node is certified when its `prove` passed on the
integrator's tree *after* the merge, and its exclusive gates passed on the
inspector's.

## Behaviour you set for builders

Put these in every builder brief:

- Which tree it works in, and whether it may compile there — see
  [COMPILATION.md](COMPILATION.md). By default only the inspector's tree
  compiles, so builders edit and report rather than building on demand.
- Edit only the node's `owns` set. Anything else is a message back to you.
- `check` is the fast loop; run the assigned non-exclusive candidate tests
  before reporting. Leave integrated and exclusive proofs to their owners and
  label them pending. A test proof does not need a redundant check immediately
  beforehand.
- `cargo check` type-checks without codegen or linking, several times faster,
  and certifies nothing. `cargo test` pays a full build for that crate's graph,
  and shares no artifacts with `check` — for a node whose `prove` is a test,
  run the test and skip the redundant check.
- Full workspace builds belong to the inspector, which pulls the branch into
  its own warm tree rather than entering the builder's.
- Report as: node id, `check` output, `prove` output, files touched, anything
  wanted outside `owns`.
- Keep the assigned role packet current under [CONTEXT.md](CONTEXT.md).
  Report short results and evidence paths, not entire logs. After compaction
  or reassignment, verify source, claim, dependencies and processes before work.

## Where the budget goes

Optimize certified work per hour, not the number of agents shown as busy.
Beyond about four builders, first check whether integration and review are
keeping up. Choose model and effort by the node's uncertainty and consequences,
not the feature name alone; record the choice in its brief. Spend effort on:

- High-effort reasoning for shared contracts, authority, persistence and
  concurrency; reserve the highest supported effort for difficult design or
  failures. Bounded migrations and mechanical work need not pay that cost.
  Self-verification covers the candidate and affected boundaries, within the
  worker's permitted non-exclusive lane.
- An independent non-author review of risky or ambiguous foundations before
  downstream work starts. If comparative implementations are explicitly wanted,
  assign distinct attempt ids and checkouts and select one candidate; do not
  give two workers an indistinguishable claim or merge both alternatives.
- Re-proving the whole merged set after every merge, not once at the end.
- A spec-drift read per merge candidate.

## Changing the plan

The owner changes the plan by talking to you. Re-cut affected nodes, mark
in-flight ones stale, tell their builders to stop, and rewrite the graph file.
Builder count is yours to set within the agreed run and actual session limits;
four is a starting target, not a promised number of simultaneous compilers.
Do not spawn nested CLI/API runs to evade the session's agent cap. Changing a
configured limit or starting an external orchestrator requires an explicit
owner request and does not change the machine budget.

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
