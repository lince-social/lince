# Compilation

How many trees may compile, and what a builder is allowed to run in one.
Confirm the scenario with the owner alongside [ISOLATION.md](ISOLATION.md),
before spawning anything.

There are two shapes, and **one build tree is the default**. Many trees is the
corgi shape. Many trees *without* corgi is neither — it is the expensive
mistake both scenarios exist to avoid, and the owner has to ask for it by name.

The whole question is cold versus hot. A tree that has never compiled pays for
all 843 dependency crates. A tree that has compiled before pays only for the
workspace crates that changed and the ones downstream of them. Measured on the
owner's machine, 2026-09-06, 8 cores / 15 GB RAM:

| | time | disk |
|---|---|---|
| clone, no `target/` | — | 214 MB |
| cold `cargo check` | 2m15s | 8.1 GB |
| cold `cargo build` after it | 3m11s | 18 GB total |
| warm `check`, one crate changed | 5s | — |
| warm full `check`, `store` changed | 7s | — |
| warm full `build`, `store` changed | 10s | — |

Cold to warm is a factor of ~20 in time and the entire disk cost. 843 of the
856 crates are dependencies, byte-identical in every tree; only 13 are ours.

## Scenario A — one build tree (default)

Cargo keys artifacts on absolute path, so a second tree shares nothing and pays
the full cold start. So there is **one** tree that compiles, and it belongs to
the inspector, which also does the integrator's re-proving.

- Builders work in source-only checkouts, no `target/`; the historical clone
  measured 214 MB. A worktree shares Git storage, not Cargo artifacts. They edit and
  reason; they do not compile. Say so in the brief — a builder that thinks it
  can compile and meets a 5-minute cold build will report a hang.
- The inspector **pulls each candidate branch into its own tree** and checks it
  there, one at a time. It never `cd`s into a builder's checkout: that tree is
  cold, which turns a 10s verification into a 5-minute one.
- That tree stays warm for the whole run, so each builder's work costs 5–10s to
  check, and six builders' work is about a minute in total.
- Never create a fresh compiling checkout per node. That is the cold-start tax, 18 GB and
  five minutes, paid again every node.

The cost of this shape is real and worth stating to the owner: a Rust builder
that cannot compile is guessing. No type errors, no borrow checker, no test
run. It suits nodes whose work is mostly reading and writing prose-like code,
and suits them badly when the node is a tricky refactor.

**A second or third compiling tree without corgi needs the owner to ask for
it.** If they do: keep them as a fixed reused pool, never one per node, and
never above the concurrency ceiling measured below.

## Scenario B — corgi, many warm trees

Only when the owner asks for corgi. Corgi replaces cargo with a machine-wide
content-addressed store: dependency artifacts are keyed by content, not by
path, so the 843 deps compile once for the machine and every tree after the
first hits cache. A tree pays only for its own diff, and has no `target/` at
all.

Measured on Lince, second tree with a one-line change to `engine`: **4 units
executed, 738 cached, 91s, 139 MB added to the store.** Four workers this way
cost about 11 GB against roughly 73 GB for four cargo trees.

- Every builder may have its own reusable tree and is eligible to compile in
  it. It still needs a machine-wide permit; workers queue when RAM/CPU/test
  capacity is occupied. Source editing and agent capacity are separate from
  compiler capacity. [WORKER_POOL.md](WORKER_POOL.md) owns this scheduling.
- The first tree into a cold store is *slow*, not fast — 508s for 742 units.
  Corgi amortizes; it is not a free lunch on worker one.
- `CORGI_STORE` must point at a disk-backed path **outside the workspace**.
  Inside it, every store path contains the workspace path and corgi rejects its
  own artifacts as not location-free. On this machine the store lives at
  `/home/user/git/lince-social/.corgi`; the default `/var/tmp/corgi` is tmpfs
  here, which is RAM.
- `CORGI_SYSTEM_READS=/nix/store:/run/current-system/sw/share/nix-ld` is
  required on NixOS. Without it every compile fails with a misleading
  `execvp … No such file or directory`.
- Corgi ignores `~/.cargo/config.toml` and links with its own bundled zig, so
  the mold rustflags there neither apply nor conflict.
- Moving the store invalidates it: the `tools/zig-wrappers-*` directories cache
  the old absolute path. Delete them after a move or every link fails with
  `Failed to find zig`.
- Corgi requires a pinned toolchain — a floating `channel = "stable"` is
  refused — and a `corgi.toml` declaring every non-Rust input a build script
  reads. Both already exist in this repo.
- Corgi caches passing deterministic tests. Re-request the accumulated proofs
  on the integrator's exact content/features after each merge; unchanged
  actions may legitimately hit cache. Do not claim that every merge executes
  every test afresh. GPU, live fixture, lifecycle and timing witnesses require
  fresh execution through a supported uncached path or a prepared Cargo binary.

### Running it

```sh
export CORGI_STORE=/home/user/git/lince-social/.corgi
export CORGI_SYSTEM_READS=/nix/store:/run/current-system/sw/share/nix-ld
corgi check -p engine                          # any non-CEF crate
corgi check --root interface -p lince-interface --no-default-features --features native-runtime
```

Both variables are required here and neither has a usable default: the store
would land in tmpfs, and without the reads every compile fails with a
misleading `execvp … No such file or directory`.

### What corgi can and cannot build

The following CEF limitation and measurements describe the inspected prototype,
not a permanent requirement that every desktop uses CEF. Interface Part A's
[build rule](../../../anicca/interface/build.md) explicitly changes the real
production default to native-only. Certify the selected target and actual
features; do not enable the deferred adapter just to repeat a historical gate.

The known blocker in the inspected graph is a **feature**, not a crate:
`cef-runtime` reaches `cef-dll-sys`, whose build script walks three
directories up from `OUT_DIR` to find cargo's `target/` and copies the CEF
runtime there. Writing outside `OUT_DIR` is what the sandbox exists to prevent,
so no configuration fixes it.

`lince-interface` declares no default features. The prototype desktop reaches
CEF through `joined-runtime`. What drags CEF into a bare interface check is corgi's
workspace-wide feature unification: `crates/desktop/Cargo.toml` depends on it
with `features = ["joined-runtime"]`, and that reaches back across the
workspace. Scoping the unification root stops it:

```toml
[roots.interface]
packages = ["lince-interface"]
```

Measured: `corgi check --root interface -p lince-interface` is 232 units, 170s,
zero CEF. Roots are opt-in through `--root`, so unscoped runs are unchanged.

- **corgi** — scoped targets whose feature graph and build-script inputs have
  been proved to work, including the native interface root.
- **cargo** — anything actually enabling `cef-runtime`, and explicitly assigned
  native production/package proofs that Corgi does not cover. The prototype's
  default desktop and joined diagnostics currently enable CEF; Part A removes
  that default dependency before its native acceptance.

`lince-interface` has three levels. Default is the bare crate.
`native-runtime` is intended to contain the full native runtime with CEF left out — engine,
transport, protein, store, AccessKit, browser parity, semantic spatial — and it
is the one to develop against. `joined-runtime` is `native-runtime` plus
`cef-runtime`. The split is a pure refactor: `joined-runtime` resolves to the
same 961-line dependency tree it did before the feature existed.

Feature names alone are not coverage. At the Part A planning baseline some
native modules and the production entry still sit behind joined/CEF gates;
the bootstrap must extract them and prove the real native host. A bare-crate
check is not evidence that this bootstrap already happened.

### A scoped run is not a full run

A scoped check proves only its selected target/features, not every desktop or
adapter. If the milestone requires joined/CEF execution, the inspector must
prove it in its Cargo lane before certification. If the milestone is the
owner-requested CEF-free Part A, certify its actual native desktop, package and
runtime instead; joined evidence is explicitly not applicable. Scoping must
not omit needed native modules, and a passing check alone never proves UI,
packaging or an exclusive runtime gate.

For everything else — the great majority of nodes — the scoped run is the
fast loop and nothing is lost.

Do not alternate modes in one loop. Features are part of the action key, so a
scoped build and an unscoped build of the same crate are different artifacts
and each pays its own compile. Pick one per working session.

## How many may compile at once

Both scenarios are capped by the machine, not by the plan. Corgi removes the
dependency work, not the workspace work, and one build already saturates the
cores on its own.

Measure it rather than guessing, once per machine:

```sh
( corgi check -p <a mid-sized crate> >/dev/null 2>&1 ) & P=$!
max=0
while kill -0 $P 2>/dev/null; do
  s=$(ps -eo rss=,comm= | grep -E 'rustc|corgi|zig|cc1|lld' | awk '{t+=$1} END{print t+0}')
  [ "$s" -gt "$max" ] && max=$s
  sleep 2
done
echo "peak $((max/1024)) MB"
```

Use this as a conservative starting bound, not a throughput guarantee:

- **(available RAM minus operating/test headroom) ÷ measured peak per job** —
  a memory bound. Include compiler/linker children and account for different
  native-check, test-link and release job sizes. Exceed it and the
  machine swaps, which on a compiler's working set means thrashing, not
  slowness. Swap does not raise this number; it only converts an OOM kill into
  a much slower build.
- **cores ÷ 2** — a starting CPU heuristic, not a universal limit. A single
  build may already use all cores. Compare elapsed certification throughput
  at candidate widths; more concurrent commands can make the whole run slower.

On the owner's machine, 2026-09-06: peak RSS **2.9 GB** for one corgi build of
`store` (168 units), against ~11 GB available and 8 cores. So **3 concurrent
builds** under that measured workload. Not the 4 an earlier estimate assumed.
Start with one, then widen only with fresh peak, headroom and throughput
evidence. A fourth lightweight job may be reasonable on a later measured
workload; it is not authorized merely by four idle agents or a RAM snapshot.
Record the revised bound before using it, and reduce it for heavier tests or
release linking. Every compiler and executable test, including integration,
counts toward the relevant resource budget.

Check `/var/tmp` before trusting the RAM figure. `/` is tmpfs on this machine,
so a corgi run that forgets `CORGI_STORE` silently builds a second store in RAM
— that was worth 1.6 GB, and an earlier one worth 6.7 GB, which is the whole
difference between a ceiling of 2 and a ceiling of 3.

Re-measure when the machine changes or when a much larger crate becomes the
common case — `interface` and `desktop` peak higher than `store`.

**While an exclusive job runs, the ceiling is zero.** A benchmark, a frame
timing report, anything measuring the machine: the inspector holds the single
slot and every other tree stops compiling for its duration. A threshold that
passed while two builds saturated the cores proves nothing, and re-running it
later on an idle machine is more expensive than waiting. The planner schedules
around this — it is the inspector's job to refuse to start a measured run
while builders are still compiling, and to say so rather than measure anyway.

## What the builder brief must say

Whichever scenario, state in every brief:

- Which tree the builder works in, and whether it may compile in it.
- If it may not: it edits and reports, and the inspector's tree runs `check`
  and `prove`. Do not let it start a build to "just confirm".
- If it may: `check` is the loop, `prove` is the handoff gate, and the first
  compile in a fresh tree is minutes — expected, not a hang.
- Anything `exclusive` belongs to the inspector, in both scenarios.
