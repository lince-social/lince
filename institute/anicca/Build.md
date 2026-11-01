# Build workflow optimization plan

Updated: 2026-11-01. Current phase: **Common Workflow Optimization Phase — in progress**. This revision changes the plan and its priorities; it does not start experiments or change application code.

Optimize useful edit-to-result time while preserving correct behavior and final binary performance. For development, prioritize compilation speed, then disk footprint, then RAM/swap; memory remains a feasibility constraint. Completed work belongs only in the compact report at the end. Keep unfinished work here, with its next action and promotion condition.

## Priority and scope

| Priority | Workflow | Measure the complete useful result |
| --- | --- | --- |
| 1 | Local incremental feature development | Representative source edit → affected checks → executable preparation/restart when needed. Include application leaf, shared Interface, Engine/Cell/Store and native/headless/model switching. |
| 2 | Testing a feature | Test-only or implementation edit → compile the affected tests → execute the required cases. Include fixtures, setup and teardown. |
| 3 | GitHub Actions | Push/PR → required workflow result and publication where applicable. Separate queue time, critical-path wall time, runner time, cache restore/save and compilation/testing. |
| 4 | Local full builds, the rarest workflow | Fresh artifact preparation with declared compiler/dependency caches, then executable/package readiness. Never treat retained-cache setup as a controlled cold build. |

Optimize in this order. Within a workflow, select the largest demonstrated removable cost; use observed frequency when available, not invented usage weights. A low-priority experiment may run earlier only when it is necessary to validate a higher-priority change; record that dependency. A CI packaging bottleneck belongs to priority 3 even when the same technique also helps a rare local full build.

The first phase targets bounded, practical improvements to these workflows. Exhaustive compiler flags, architectural rewrites and unavailable infrastructure belong to the Details Phase unless a common-workflow measurement makes one a clear prerequisite. Do not repeat rejected configurations without a changed hypothesis.

## Current handoff and retained baseline

- R01–R70 are recorded results, not 70 completed plan tasks. R71 / `12a` / D14 is an unfinished experiment. No fixed final R number exists.
- Before switching experimental source/configuration, the main execution thread must finish or safely close R71. Its live handoff, original/trial files and contamination/confirmation protocol are in `target/build-iteration/12a/handoff.json` and `12a/bevy-dylib/`. The initial six-block summary now exists, but contamination/overlap leaves gains unqualified and the bounded confirmation/decision outstanding. Its source probes report restored; experimental Interface manifest/Cargo.lock variants may remain active. Verify and restore owned changes as required, preserving R68/R69 and unrelated work. Do not restart it from this document.
- R71 proves Cargo CLI loading and shared-target static/dynamic coexistence. Shared-target preparation costs 423.301 s; isolated preparation is separate setup evidence. Later runtime/invalidation gates remain necessary if compilation passes. Dev-profile evidence cannot promote the release-based `cargo xtask dev` entry.
- Retain ordinary Cargo/Rust 1.96, LLVM, Clang/Wild 0.9 on the measured Linux/Nix path, automatic eight jobs, incremental dev compilation with line tables and 256 Interface codegen units. Verify effective pins/flags before new comparisons. Shipping settings are a separate scope.
- Retain adopted dependency scoping, Engine-bound typed identity and Store's fail-closed behavior, simulation input guards, generator caching/recovery, scanner improvements, local PATH normalization and corrected content discovery. Keep R57's explicit cache-socket recipe and R62/R10's scoped test recipes.
- R34's scoped AWS-LC/native compiler workaround remains experimental, not a global or shipping default. Preserve warning denial, hardening and jitterentropy's required unoptimized path.
- Freedoom is already excluded unless `stuff` is enabled. Its existing optional arrangement is closed work, not a new common-workflow candidate.

No controlled overall full-build, release-build, `xtask dev` or GitHub Actions percentage has been established. Workflow gains below have specific scopes and different baselines; never add their percentages.

## Common Workflow Optimization Phase

Execute CW0–CW6 sequentially. Refine concrete architectural changes before implementation; this planning revision does not approve a new unresolved ownership contract.

### CW0 — Close the active experiment

Read the live R71 handoff, preserve every sample and follow its bounded contamination protocol. Decide adopt for a proven scope, opt-in, reject/restore, or defer with a condition. Preserve unrelated work; verify probe restoration and the original static selection/freshness when required. Record one compact result, then follow the workflow priorities below rather than automatically resuming the old technique-number queue.

### CW1 — Establish the actual common-workflow baseline

1. Inventory normal entry points from xtask, Cargo/Nix and GitHub workflow files. Record exact commands, feature/profile/target selection, compiler/linker/native flags and cache state. `cargo xtask dev` currently uses `--release`; distinguish it from dev-profile `cargo check` and `cargo run`.
2. Select representative, behavior-preserving edits for application leaf, shared Interface, Engine/Cell/Store and feature-test code. Include the narrow headless/models path when relevant. Reuse valid existing evidence; refresh only where inputs or the actual workflow differ materially.
3. Measure unchanged, edited and restoration runs. Separate checking, codegen/linking, restart/load and selected execution, while retaining an end-to-end total. Track which units rebuilt and why. Keep test compilation and test execution separately visible.
4. Populate the dashboard below. First characterize local iteration and feature testing; collect CI timings at CW4 and rare full-build timings at CW5. Do not launch expensive cold preparation merely to fill a dashboard cell.

| Workflow | Current accepted reference | Remaining measurement |
| --- | --- | --- |
| Local affected checks | Scoped gains in R05/R40/R46; helper gains in R06 | Current representative shared/leaf edits through normal commands and switching. |
| Local executable iteration | Native controls R39/R55; R71 in progress | Actual `xtask dev` edit-to-ready and relevant restart/behavior cost. |
| Feature tests | R08/R10/R62 for their exact scopes | Other commonly needed feature selections and total edit-to-test time. |
| GitHub Actions | Existing caching/timestamp restoration is inventory, not a new gain | Comparable job/step durations and workflow critical path. |
| Rare local full build/package | Preparation logs exist with different cache histories | Controlled before/after only when a candidate warrants the cost. |

### CW2 — Shorten local incremental feature development

1. Use Cargo timing/fingerprint/cache evidence to rank repeated work. First remove accidental reruns, unstable generated outputs, duplicate-only environment variation, unnecessary helper preparation and avoidable feature/profile variants. Preserve real source/tool/configuration invalidation.
2. Audit the smallest useful check/run graph for the edited feature. Reuse existing models/headless/native boundaries; remove a dependency or feature only after finding its live consumers. A smaller crate count is not itself a goal. Compare switching as well as staying in one selection.
3. Test the largest bounded candidate one factor at a time. Require useful complete leaf/shared iteration savings and no unacceptable regression in another common selection. Include setup amortization and relevant runtime checks. Close R71/D14; qualify D30's profile candidate only through its runtime prerequisite.
4. Compare the actual normal entry point before adoption, including release-based development commands. A new profile or fast-check recipe must state exactly what behavior it exercises. If restart/load is material, consider a separately scoped restart-avoidance or hot-reload prototype; dynamic linking alone is not hot reload.
5. Stop when the measured, feasible low-hanging candidates are resolved. Move costly ownership rewrites or speculative tool changes to the register unless their removable common-workflow cost justifies bringing them forward. D28 gates only changes affecting a demonstrated provenance gap.

Deliverable: current before/after edit-to-check and edit-to-run results, adopted scope, required invalidation/runtime checks and remaining bottlenecks.

### CW3 — Shorten testing done for features

1. Map common feature edits to their required test targets and fixtures. Preserve distinct assertions, failure reporting and required integration coverage; avoid preparing unrelated targets when the feature has an established narrow test path.
2. Measure complete test-only and implementation-edit cycles. Rank fixture initialization, avoidable deadline waits, dependency/harness compilation, execution and teardown separately.
3. Reuse R62 for its four Store transaction targets and R10 for its seven Store cases. Their combination is untested. Extend optimization or scheduling only after measuring the new selection and auditing shared state/resources; retain doctests and real execution where required.
4. Test fixture reuse, deterministic completion, supported selective dependency optimization or scheduling when they remove measured time. Preserve production deadlines and privacy/permission semantics. Cached test results require separate invalidation proof and cannot replace physical/session acceptance.
5. Establish a valid bounded native behavior control where needed for a promising common-workflow candidate. D30 has six unresolved control failures; diagnose the existing fixtures before comparing a profile candidate. Do not weaken assertions, bypass mail permissions or blindly extend timeouts.
6. Validate adopted recipes through ordinary Nix/xtask/Cargo entry points. Record preparation plus execution; a test-runtime gain is not faster compilation. Broaden testing only for the affected change or a concrete unresolved failure.

Deliverable: repeatable feature-test recipes and before/after total time for the representative edits, with failures and scope limitations visible.

### CW4 — Reduce GitHub Actions time

1. Inspect `.github/workflows/{build,park,park-validation,park-probe,transfers,android}.yml` and the scripts they invoke. Use actual comparable run logs when available; record unavailable timing evidence rather than inventing CI gains.
2. Identify the required workflow critical path and repeated setup: tool/native-library installation, source timestamp restoration, registry/git/artifact cache restore/save, compilation, tests, packaging and publication. Distinguish elapsed workflow time from aggregate runner time.
3. Test the largest low-hanging candidate: remove duplicated setup/work, fix unjustified cache misses, narrow a job to its intended target/features, or share a compatible dependency artifact. Include cache transfer/unpack costs and changed-source/dependency/native/toolchain invalidation. Keep required platform/feature coverage and publication/validation ordering.
4. Evaluate `08a` source filtering and D06/`08b` Crane reuse here if repeated CI source packaging is material. Verify effective package/CI linker and flags under `08c`; the development shell's Wild/native-CPU settings do not establish shipping settings. Keep warning denial and deployment CPU compatibility.
5. Compare equivalent CI runs with a bounded sample set, relevant cache-miss/hit cases and output checks. Do not publish an experimental binary merely to benchmark it. Measure actual selected runner/platform behavior before promotion; local results are only preflight.
6. Record workflow and job/step medians, cache/setup cost, artifact/install checks and the chosen scope. If runner access/evidence is unavailable, leave that explicit item pending and continue independent authorized work.

Deliverable: measured CI bottlenecks and compatible workflow changes; a clear absence of evidence where CI timing is not available.

### CW5 — Optimize rare local full builds last

1. After earlier workflows' low-hanging opportunities, select a remaining costly full executable or source-package preparation. State whether the target is fresh and which compiler/dependency/Nix caches are populated; retain existing artifacts and never use `cargo clean`.
2. Inspect whole-build critical paths, external native producers, frontend/codegen/linking and package invalidation. Revisit expensive producer or dependency reuse only when this workload actually repeats the work.
3. For `08a`, enumerate workspace manifests/path dependencies, generators and fingerprint inputs before narrowing Nix source filtering. Test an irrelevant edit, relevant application edit and relevant added/deleted input; retain complete installed runtime content.
4. For eligible `08b`, pin Crane and compare dependency preparation plus two application-only revisions, then a controlled dependency/toolchain/native-input change. Align features/profiles/targets/flags, stubs and build scripts. Verify actual reused compilation, not only a Nix hit message.
5. For `08c`, separately validate package/publication linker/profile changes and the installed artifact outside the development shell. Preserve licenses/credits, build information, runtime libraries, session registration, complete Trails content and the published-prebuilt installation path.
6. Adopt only if total preparation savings amortize at this workflow's actual frequency and do not harm higher-priority iteration, behavior or final performance.

Deliverable: a controlled full-preparation result when justified, otherwise a concrete deferral. No aggregate cold-build claim from incomparable setup logs.

### CW6 — Validate the useful combination and close the phase

- [ ] Each of the four workflow classes has a current baseline or a concrete unavailable/not-applicable explanation and revisit condition.
- [ ] The largest measured, feasible low-hanging candidates have an adopt/opt-in/reject/defer decision. Earlier-priority workflows were addressed before lower-priority tuning.
- [ ] Adopted changes pass their relevant behavior/invalidation checks and normal entry points. Check interactions with accepted cache/profile/linker choices; do not sum individual gains.
- [ ] Comparable end-to-end measurements show the useful combination's effect on local feature iteration, feature tests and CI where available. Rare full builds remain last.
- [ ] Temporary probes/rejected edits are restored, partial/contaminated samples remain labeled, and unresolved items remain in the register.
- [ ] Summarize what improved, by how much and under which commands. State missing full-build/release/CI evidence explicitly.

Track completion by these workflow gates, not by the number of R records. Then mark **Common Workflow Optimization Phase complete**, retain its small workflow summary, and start the **Details Phase**. Unsupported upstream features, unapproved infrastructure and speculative candidates do not hold this phase open indefinitely; record their conditions. An unmeasured accessible common workflow or unvalidated proposed default is not complete merely because many experiments exist.

## Details Phase — remaining optimization register

After CW6, select eligible items by measured opportunity. An item may move into the common phase only with a specific common-workflow bottleneck and a recorded reason. Closed configurations stay closed until their named condition changes. D IDs preserve the original backlog; legacy trial IDs still identify raw evidence.

| ID / legacy trial | Remaining candidate or revisit condition |
| --- | --- |
| D01 / `02c` | Scoped feature unification/cargo-hakari: R49/R50 find real variants and no safe owned shader-edge removal. Need a removable edge/reuse design, faster normal switching and relevant D28 coverage; preserve narrow graphs. |
| D02 / `04c` | Broader nextest/scheduling: R10 is valid only for seven cases. Need a new frequent test selection, isolation/resource audit, identical inventory/assertions and complete-cycle comparison. |
| D03 / `05c-external` | External native/binding reuse: R54 finds current audited producers fresh. Revisit repeated SPA/PipeWire/Speech Dispatcher/AWS-LC/native helper generation in actual profile/CI/package preparation; preserve ABI, patches and licenses. |
| D04 / `07b` | Further crate/API boundaries: identify an expensive live dependency edge and coherent pure ownership, then a concrete design removing real downstream work. No wrappers around the same graph. |
| D05 / `07c` | Asset separation/loading: first attribute material embedded emission/link/invalidation cost. Validate source/asset identity, installed/offline discovery, complete content and credits before changing loading. |
| D06 / `08b` | Crane dependency artifacts: eligible when repeated source packaging recompiles dependencies. Follow CW4/CW5's application-only reuse, dependency/tool/native invalidation and installed-artifact gates. |
| D07 / `09b` | Broader optimized fixtures or nextest combination: R62's four-target recipe is retained; native-wide SQLite2 was rejected. Need a separately measured selection and supported input/profile design. |
| D08 / `09c` | Optimized host tools: R63 attributes little repeated time to expansion/current build scripts. Need material host execution in an actual common/package workflow; retain identical generated output and invalidation. |
| D09 / `09d,10b` | Optimized-preparation incrementality: R64 fails consistency; R66 fails optimized runtime. Need a distinct diagnosis/configuration preserving runtime, useful leaf/shared cycles and normal entry. |
| D10 / `10d` | Fixed CPU target: no accidental machine/flag fragmentation demonstrated. Need real cache-reuse loss or codegen benefit, runtime validation and deployment-compatible CPU choice. |
| D11 / `10d` | Faster storage/bounded RAM inputs: attribute owned process/cgroup/file I/O first. Persistent mounts share one NVMe/Btrfs device. Need a removable hotspot and peak headroom; no full-target RAM or global remount. No-CoW needs write/CoW evidence. |
| D12 / `11a` | Further linkers: R41 retains Wild 0.9. Revisit a materially changed linker/workload with same inputs, full iterations and relevant native/recovery behavior; package linking is separate. |
| D13 / `11b` | Incremental linking: latest checked Wild 0.11/main lacked implementation on 2026-11-01. Need a pinned implementation; test changed/added/removed objects, native inputs/flags, fallback and fresh-link equivalence. |
| D14 / `12a` | Bevy dylib: finish R71's recorded protocol first. Require useful leaf/shared cycles, setup amortization, dylib invalidation/loading and native runtime; release development/shipping is separate. |
| D15 / `13a` | Cranelift: preview recovery and full-helper intrinsic failures are established; mixed helper gives no useful gain. Need a new codegen-heavy common workload or backend/profile improvement, same-compiler controls and full required recovery/SIMD/native behavior. |
| D16 / `14a` | Frontend parallelism: R19 rejects four workers for Engine body edits. Need a different measured checking bottleneck and supported compiler; coordinate Cargo jobs, memory, identical compiler settings and total workflow cost. |
| D17 / `15a` | `hint-mostly-unused`: verify pinned compiler support and an expensive broadly exposed, narrowly used non-generic API. Measure actual dependency rebuilding plus all consuming binaries; reject displaced/duplicated downstream work. |
| D18 / `16a` | Stable compiler upgrade: select one pin with relevant changes; verify actual Cargo/Nix compiler, warnings, proc macros/native/targets, runtime and common-edit gains against cache-reset setup cost. |
| D19 / `16b` | Blaze: historical v0.20261027.0 panic-path failure is not a new benchmark. Need a candidate plausibly fixing it; pass reproducer, recovery/native/helper/target gates before actual comparisons. |
| D20 / `17a–17c` | huacnlee Cargo: helper gain missed the useful threshold. Broader native/package-switch reuse needs a measured opportunity, compiler/profile parity, real nested xtask selection and source/feature/tool/native invalidation. |
| D21 / `18a–18d` | Corgi: pinned 0.1.96 fails Cargo relative-path env tables. Reopen only with supported semantics/tool-side fix; then generators/xtask/feature integration, matched performance and independent cached-test invalidation. Preserve sandbox/runtime roots, exact tool pin and retention policy. |
| D22 / `19a` | Remote cache/distribution/Nix substitution: need an available trusted resource and transferable bottleneck. Separate upload/download/setup/fallback and invalidation. New service, account, cost or private artifact upload needs its own agreed scope. |
| D23 / `19b` | Bazel/Buck2/container replacement: need a concrete Cargo/Nix limitation and savings exceeding migration/maintenance cost. Prototype a representative compatible target; no build-system migration is implied. |
| D24 / `19c` | PGO/BOLT/full LTO/single release unit: need a separately selected runtime objective and representative training workload. Include training/release cost, recovery and packaging; build turnaround alone is insufficient. |
| D25 / `20a` | Recurring reviews: manual evidence-backed simplification belongs to batch review. Automated recurrence needs an explicit request and one useful demonstrated manual review; no scheduler or automatic deletion is planned. |
| D26 / `04b` | Action serialization/codegen: keep live schema and strict parsing; first propose a representation-preserving boundary and runtime workload. Do not remove derives or assume Engine-owned commands have a pure new owner. |
| D27 / `05a,06a` | Simulation policy/context boundary: pass the small Cargo fixture and preserve the composition contract below before production APIs. Identity-only relocation leaves policy-triggered downstream checks. |
| D28 / `06a` | Complete effective-input identity: agree supported inputs and actual build/composition provenance. Gate affected cache/replay promotion, not independent optimizations; retain fail-closed Store behavior. |
| D29 / `07a` | Pure Protein contract/optional runtime: both R48 layouts were restored after runtime regressions. Need a materially different runtime-preserving design with exact schema/security/output behavior and complete iteration gains. |
| D30 / `09a` | Interface512: compilation gain is real for its measured scope, adoption is deferred. Establish valid native controls and matched runtime/render/recovery/normal-entry behavior before replacing 256. |

### Keep these unresolved contracts explicit

**D26 — serialization.** `transport/src/native/catalog.rs` consumes the Action schema at runtime. Action includes Engine-owned Sources/Demeter/Optimization commands. Preserve exact representations, schema, malformed-input rejection, round trips and optimized execution; IR counts alone do not qualify a refactor.

**D27 — simulation composition.** First prove reduced downstream work in a small policy/dependency fixture, including normal library and `--tests` selections. Keep automatic policy enforcement, additions/deletions and all migrated provenance inputs; a manual policy command, fixed identity or test-source exclusion is insufficient. R44's identity-only control saves 7.26%, below its 1 s/10% gate.

If qualified, solve worker/package placement before adding dependencies: the worker currently shares simulation's package, so its normal dependencies also affect the library. Keep the nine original roots, manifest/lock, directory watches and producer coverage. Pass one validated immutable identity through execution/session/replay, campaign partition/resume/minimize/regressions, comparison, CLI, preview and worker Ready reporting. Missing/mismatched identity must refuse execution/writes; preserve path/key and evidence semantics without runtime build-path dependence.

Supply context independently of optional Cell: native `simulation_castle::runner::run` supports isolated no-Cell jobs. Trace sandbox/Park/CLI/worker/test composition, `cell_bridge::{connect_session,start,reconnect}`, Practice remote bridge and live Laboratory/preview consumers. No global/task-local fallback or editable model identity. Validate isolated stepping/Stop/replay, campaigns, preview, reconnect, worker protocol, deterministic outputs, privacy and persistence after a useful compile result. Evidence: `06a/{simulation-diagnosis,simulation-context}/`.

**D28 — supported effective inputs.** Audits show dependency-feature collisions, local dependencies outside the six Engine/nine simulation roots, ignored included/generated content, native/tool/wrapper inputs and effective cfg/profile gaps. Native/headless graphs can have equal identities despite different Naga features; this is a coverage gap, not a demonstrated wrong replay.

Prototype an explicit input manifest or dependency provenance protocol from the actual build/composition pipeline. Trace relevant included WGSL/JSON/assets rather than blindly hashing everything. Cover effective resolved features, local/generated source closure, native tools/sources/flags and actual configuration. Do not use Cargo's private fingerprints as a stable API, an uncontrolled ambient environment or revision-only fallback. Test normal-Cargo unchanged reuse, relevant changed inputs/restoration, missing/failed identity, valid/mismatched caches/replays, supported loading/platforms and startup/runtime cost. Evidence: `06a/input-audit/`, `02c/switching/`.

**D30 — valid runtime control.** R59's original control has six unresolved failures after four socket-path harness failures were corrected. Use repo-root `LINCE_CONTENT=institute/trails/lince`, guarded content and short private XDG runtime paths. First test proper `capture::install` composition for the two customization fixtures missing capture-policy Windows/Status resources. Diagnose the four Practice cases: missing Protein rows, explicit mail-sharing denial, unfinished vocabulary confirmation and pending WGSL compilation. Preserve privacy/defaults/assertions and locate the post-catalogue timeout before matched candidate runtime, rendering/recovery/backtrace and normal-entry checks. R69 fixes shell discovery, not these cases. Evidence: `09a/native/interface-cgu-512/runtime-readonly-triage.json`.

## Experiment workflow and reporting

1. Before a trial, record workflow/priority, hypothesis, exact baseline/candidate pin and commands, edited files, useful-saving threshold, bounded sample/time budget, setup/cache state and restoration method. Refine unresolved architecture or runtime tradeoffs before implementation.
2. Verify platform/features/profiles/compiler/linker/native/panic compatibility before expensive preparation. Use `cargo check` for checking and Nix/xtask for execution. Warnings remain errors. No global tool replacement, cache clearing or competing benchmark builds.
3. Change one factor. Use comparable behavior-preserving edits and at least three balanced uncontaminated samples per side for a performance claim; preflight failures may close before timing. Retain all raw attempts. Separate setup, unchanged, edit, restore and execution. Record medians/ranges, rebuilt units/cache changes and relevant owned resources. Foreign compilation or overlapping variation needs explicit treatment; never select only favorable samples.
4. Require a useful end-to-end saving beyond observed variation, no unacceptable regression in other affected common workflows, and preserved behavior/final runtime. Choose thresholds before running; reuse the active experiment's thresholds rather than changing them after results.
5. Validate required invalidation, source additions/deletions, features/flags/tools/native inputs and output recovery where affected. Test the exact normal entry point and relevant persisted/runtime behavior. Full Park/session acceptance remains separately scoped in [Park.md](Park.md); do not run the whole suite for every local trial.
6. Restore rejected/probe changes without reverting unrelated work. Choose adopt for a named scope, opt-in, reject or defer with a concrete condition. Promotion requires its tests, not merely successful compilation.
7. Save exact recipes, patches/digests, tool pins, samples/exclusions, timing/resource/output logs, validation and restoration receipts under `target/build-iteration/<trial>/<attempt>/`. The plan gets only one compact report row: tested scope, numerical result or missing evidence, and choice. Update a deferred condition in place; remove completed instructions and duplicate handoffs.
8. Review newly eligible backlog items at each batch end and measure the useful combination. Keep missing measurements explicit. No speedup is inferred from dependency counts, cache-hit messages or incomparable preparation times.

All percentages use `(baseline - candidate) / baseline`; report absolute seconds too. Disclose setup amortization when relevant. Memory/RSS/cgroup and system pressure have different scopes; do not attribute host I/O or summed shared mappings to owned build time. Keep source/artifact guards from perturbing timing interpretation visible in receipts.

## Retained scoped recipes

R57's explicit local socket is an opt-in recipe, not evidence that every cacheable/native build is 63.16% faster. Source `target/build-iteration/10b/r57-pool/environment.sh` after the selected common environment. It sets `SCCACHE_SERVER_UDS=$PWD/target/sccache/r57.sock` and unsets `SCCACHE_SERVER_PORT`; preserve other workers' daemons/artifacts.

R62's validated four-target Store command (ordinary Cargo/Nix, four test threads; no R34 requirement):

```sh
env -u SCCACHE_SERVER_PORT \
  SCCACHE_SERVER_UDS="$PWD/target/sccache/r57.sock" \
  CARGO_CACHE_AUTO_CLEAN_FREQUENCY=never \
  nix develop .#interface -c cargo \
  --config 'profile.dev.package.libsqlite3-sys.opt-level=2' \
  test --locked --offline -p store \
  --test account_transactions --test assertion_transactions \
  --test record_transactions --test standing_transactions \
  -- --test-threads=4
```

R10 uses pinned nextest 0.9.114 for Store `read_only` and `concurrency`, four slots, zero retries and the trial's config. Exact command/config: `target/build-iteration/04c/`. It does not replace all Cargo tests/doctests, and combining it with R62 needs its own measurement.

## Compact report — completed investigations and decisions

These are historical scoped results, not a current whole-build baseline or a task count. Raw evidence remains under `target/build-iteration/`; directory references below are relative to that root. Closed trials do not carry implementation instructions forward.

| Records / tested scope | Result | Choice |
| --- | --- | --- |
| R00/R01/R39/R55 — inventory and native controls | Native no-op about 1 s; later leaf-edit controls about 10.5–10.9 s. Preparation caches differ. | Reference only; no cold-build or overall gain. |
| R02 — macro host dependencies (`02a/interleave`) | 127 fewer native checking units, 6.24%; elapsed gain unproven. | Retain five-dependency pruning. |
| R03 — SQLx feature pruning (`02a/sqlx`) | No removed units/useful gain; same two durable-store failures in both controls. | Restore original features. |
| R04 — rqrr image adapter (`02a/rqrr`) | Two dependencies removed from Engine/headless; native retains a live image consumer. | Adopt; no elapsed-time claim. |
| R05 — Park Store scope (`03a`) | Narrow Store-edit check 9.258 → 0.454 s, 95.1% faster; 318 fewer units. | Adopt scoped dependency. |
| R06 — lightweight xtask (`03b`) | Authoring-edit help 1.541 → 0.444 s, 71.2%; core-edit help 1.587 → 1.248 s, 21.4%. | Adopt optional authoring dispatch; preserve tests. |
| R07/R52/R53 — Store harness consolidation (`03c`) | Two-target gain inconclusive; four-target edited preparation 3.540 → 3.340 s saves only 0.201 s. | Reject layout changes; keep original targets. |
| R08 — Park test deadlines (`04a`) | Warm selected group 23.196 → 1.197 s, 94.8% faster. | Adopt fixture-only controls; production deadlines unchanged. |
| R09 — Engine IR (`04b`) | About 6.22 million IR lines; substantial Action/Serde generation. | Diagnostic only; D26 retains schema contract. |
| R10 — nextest (`04c`) | Seven Store cases 11.749 → 9.025 s, 23.18% faster. | Retain scoped opt-in, no blanket runner change. |
| R11/R14 — source invalidation (`05a`) | Added/deleted inputs now detected; no-op overhead within variation. | Adopt correctness coverage; no speedup. |
| R12/R13 — Sensei scanning (`05b`) | Policy stage 0.345808 → 0.127032 s, then 0.127035 → 0.112177 s; 30.3 MB irrelevant reads avoided. | Adopt; 63.27% then 11.70% stage gains, not whole-build gains. |
| R15 — Anicca output reuse (`05c/anicca`) | Edit/repeat sequence 21.484 → 12.384 s, 42.36% faster; follow-up 9.917 → 0.563 s. | Adopt stable output/native reuse; first edited check slightly slower. |
| R16 — optional Freedoom C generation (`05c/freedoom`) | Repeated stage 12.721 → 1.193 s, 90.62% faster; first cache population costs 5.14 s extra. | Adopt object caching; already gated by `stuff`. |
| R17/R18 — icons (`05c/icons-recovery`) | Standalone speed gain inconclusive; missing output now regenerates identical pixels. | Retain correctness repair; no speedup claim. |
| R19 — frontend workers (`14a`) | Engine edit 17.40 → 17.82 s, 2.41% slower. | Reject measured four-worker default. |
| R20/R21/R23/R24/R37/R38 — Cranelift (`13a,13b`) | Preview fails recovery; full helper fails SSE2 intrinsic. Mixed helper edits 1.62 → 1.69 s; warm tests 1.61 → 3.52 s. | Keep LLVM; reopen only a changed backend/workload. |
| R22 — atomic API name (`13a`) | Stable/nightly/backend checks pass with identical closure/orderings. | Keep supported `try_update`; no speed claim. |
| R25/R30/R31/R32/R35/R36 — huacnlee Cargo (`17a,17b`) | Selected helper behavior passes; edited help 1.55 → 1.46 s saves 0.09 s, below gate. | Keep ordinary Cargo; broader reuse unmeasured. |
| R26/R28/R29 — Corgi (`18a`) | Linux fixture passes; immutable compiler improves diagnostic setup only. Real workspace rejects relative-path env tables. | Experimental/blocked; no workspace speed claim. |
| R27 — unused workspace declarations (`02a/unused-workspace`) | 17 warnings removed; used dependency metadata unchanged. | Adopt cleanup; no compilation gain. |
| R33/R34 — native compiler prerequisite (`05c/native-fortify`) | Scoped warning/hardening/jitterentropy recipe passes crypto and byte-aligned native checks. | Experimental only; no global/shipping adoption or speed claim. |
| R40 — Engine execution identity (`06a/execution`) | Store edit check 10.55 → 1.92 s, 81.80% faster; focused identity/runtime/invalidation checks pass. | Adopt explicit typed binding and fail-closed Store; D28 remains. |
| R41 — linkers (`11a/replay`) | Wild0.9 2.85 s; Wild0.11 3.45; mold 4.57; LLD 8.74 on identical inputs. | Keep Wild0.9; package/runtime scope separate. |
| R42/R43/R44 — simulation identity (`06a/simulation-context`) | Static-only saves 1.77%; identity-only control saves 7.26% and still checks nine targets. | Restore candidates; production rewrite deferred under D27. |
| R45 — input coverage (`06a/input-audit`) | Own optimization/debug/target/compiler distinctions fixed; broader closure gaps demonstrated. | Adopt guard; D28 implementation unfinished, no speed claim. |
| R46 — Fiote boundary (`07a/model-contract`) | Models-only Fiote-edit check 1.80 → 0.46 s, 74.44% faster; 298 fewer artifacts. | Adopt native-only dependency; native graph preserved. |
| R47/R48 — Protein runtime boundary (`07a`) | Model checks improve 85.76–87.06%, but optimized runtime regresses 0.93–2.27%. | Restore both layouts; retain independent fixture repair; D29. |
| R49/R50 — graph/switch attribution (`02c/switching,02a/native-feature-audit`) | Required shader/upstream consumers explain variants; no safe owned removal. | Diagnostic only; no feature unification adopted. |
| R51 — helper Glam (`02a/xtask-glam`) | Metadata 14.30 → 3.81 MB, 73.32% smaller; check time within variation. | Adopt std-only helper features; no speed claim. |
| R54/R63 — native/host attribution (`05c/external,09c/host-attribution`) | Native producers fresh; diagnostic expansion 2.184 of 52.324 s, checking dominates. | No blanket native cache/host optimization; D03/D08 conditional. |
| R56 — dependency debug removal (`09a/native`) | Leaf edits 37.56% faster and ELF 28.76% smaller; shared cycles 8.87% slower. | Reject broad override; restore debug settings. |
| R57 — local cache socket (`10b/r57-pool`) | Three-crate miss batch 5.189 → 1.911 s, 63.16% faster; outputs match. | Retain explicit socket recipe; no whole-native percentage. |
| R58 — Interface128 (`09a/native/interface-cgu`) | Shared cycles 110.118 → 130.226 s, 18.26% slower. | Reject; keep256. |
| R59 — Interface512 (`09a/native/interface-cgu-512`) | Shared cycles 105.549 → 79.254 s, 24.91% faster; runtime control invalid. | Defer D30; keep256 until required behavior validation. |
| R60/R61 — broad SQLite2 (`09b/sqlite-opt`) | Selected execution 64.07% faster, but complete shared edits 5.49% slower. | Reject general native default; keep SQLite0. |
| R62 — Store-only SQLite2 (`09b/store-sqlite`) | Unchanged/fixture/Store edits 148.213/154.259/154.699 → 101.861/102.704/102.076 s, 31.27/33.42/34.02% faster. | Retain four-target opt-in; 90 distinct cases pass; native/shipping unchanged. |
| R64/R66 — optimized incrementality (`09d/optimized-query,10b/selective-query`) | Broad candidate fails warmed-control consistency. Binary-only shared edits 57.10% faster but runtime 1.58% slower. | Reject both profiles; restore optimized runtime baseline. |
| R65 — Cargo jobs (`10a/native-jobs`) | Four jobs provides no dependable shared/leaf gain; samples include recorded interference. | Retain automatic eight; no causal gain claim. |
| R67 — invocation paths (`10c/invocations`) | Persistent/sibling-shell/xtask checks 0.470/0.900/0.932 s; 0.430 s saving misses gate. | No default/ editor change. |
| R68 — PATH normalization (`10c/path-normalization`) | Nested-shell/return cycle 17.916 → 1.380 s, 92.30% faster; native executable byte-identical. | Adopt local hooks; real tool invalidation preserved. |
| R69 — content discovery (`10c/content-discovery`) | Focused content tests and 16 shell/override cases pass. | Adopt repository-root default/empty override repair; no speed claim. |
| R70 — CPU/storage/link eligibility (`10d/eligibility`) | No accidental CPU variants/faster mounted device; incremental Wild linking unimplemented at checked pin. | Keep defaults; D10/D11/D13 retain prerequisites. |

H01's older optimized-fixture note supplies no additional verified gain; current R61/R62 supersede it for their scopes. H02 is only the historical Blaze blocker retained in D19. R71 remains open in CW0/D14 and must receive a final scoped row when its protocol closes.
