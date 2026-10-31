# Remaining codebase and build improvements

Updated: 2026-10-30. This is a backlog of work left to investigate or implement. The document edit does not start these tasks. Refine the selected approach, architecture and validation with the user before implementation.

Follow the numbered order for the core codebase work. Later experiments are conditional: use the measurements from step 1 to decide whether they address an actual bottleneck, and record a reason when deferring them. Implement one measurable improvement at a time. Remove completed tasks after verification; leave deferred work and its reason. Keep Park's functional acceptance in [Park.md](Park.md).

Preserve the existing local Wild/Clang configuration, persistent sccache, incremental development compilation, lightweight development debug information, independent session/capture client binaries and content-aware generated-output caching. Those are already implemented. Keep the existing CI ordering that uploads and publishes the binary before advisory checks and helper compilation; accelerating a separate checks workflow is not the objective.

Use `cargo check` for compilation checks and Cargo xtask or Nix commands for executable preparation and execution. Treat compiler warnings as errors. Retain target directories and caches, and avoid competing Cargo processes against the same target. Do not change existing migrations, migration checksums or guards, edit AGENTS.md/README.md, modify the unplugged web crate, or preserve obsolete-version compatibility. Keep unwind/recovery behavior and existing input, privacy, persistence, media, map, GLB and WGSL functionality.

## 1. Establish repeatable edit-to-result measurements

- [ ] Record where time is spent before choosing the next optimization.

Use the existing development shell and fixed toolchain, features, target and profile. Measure an unchanged repeat, a Park test-only edit, an application leaf edit and a shared interface/engine/Cell edit. Separate shell acquisition, dependency compilation, changed-crate checking/code generation, linking, fixture preparation and actual execution. Record rebuilt units, Cargo lock waits, peak memory or swapping when observable, and cache hits/misses. Save Cargo timing reports and compiler/linker commands under `target/build-iteration` without cleaning existing artifacts.

Use three comparable edited iterations and report their median. Restore temporary benchmark edits before moving on. Collect expensive release measurements only when the selected task affects release preparation. `cargo check` timings establish checking costs; executable preparation must also measure code generation and linking. Old observations whose raw receipts disappeared are historical evidence, not a fresh baseline. See [Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html).

Completion: retain a reproducible recipe and identify the dominant costs for each edit class. Each later change must name the compile unit, dependency edge, repeated work or measured stage it is expected to remove, and compare the same recipe afterward.

## 2. Remove unused features and unnecessary dependency duplication

- [ ] Audit the dependency graphs of the actual development, fixture, native release and headless artifacts.

Start with `Cargo.toml`, `Cargo.lock`, and the manifests for `crates/interface`, `crates/park`, `crates/engine`, `crates/cell` and the affected helpers. Inspect selected graphs with `cargo tree -e features` and `cargo tree -d`, including the inverse path that enables each expensive dependency. Distinguish workspace members from separate host, target, profile and feature compilation units.

Remove dependencies and feature activations with no retained consumer. Align duplicate versions only where their required APIs and feature combinations permit it. Engine and Cell already have empty default features; adding `default-features = false` alone cannot remove their unconditional dependencies. Delete obsolete scaffolding instead of retaining compatibility layers, while preserving current functionality.

Completion: show the removed dependency/feature paths and resulting unit-count or timing change. Check affected native and headless configurations and any Android/shared configuration touched by the change. A duplicate version with a real incompatible consumer remains, with the reason recorded.

## 3. Reduce the compilation scope of remaining helpers and test targets

- [ ] Find probes, fixtures and tests that still compile broader application graphs than they need.

Use `crates/park/Cargo.toml`, `crates/park-clients/Cargo.toml` and `crates/xtask/src/park/validation/final_suite/scenarios.rs` as starting points. Session and capture probes already have independent client targets; inspect the remaining probes rather than repeating that extraction. Audit Park's unconditional dev-dependencies, including `store`, against the fixtures that actually use them.

Where the graph justifies it, separate protocol/policy checks, graphics checks and complete application fixtures into explicit targets or feature-selected modules. Move a heavy test dependency into the narrow fixture package or explicit test-support feature that owns its use. Cargo dev-dependencies cannot simply be made optional; select a valid target/feature arrangement. Preserve access to necessary internal behavior without creating broad production APIs solely for tests.

Completion: a narrow edit/check compiles fewer actual units, the complete fixture selection remains available, and the runner still executes the intended cases. Runtime name filters alone do not reduce what Rust compiles into a test executable. Do not introduce another target if Cargo feature unification leaves the expensive graph unchanged.

## 4. Simplify repeated test bodies and fixture preparation

- [ ] Consolidate duplicate setup and reduce avoidable generated code while retaining distinct behavior checks.

Review Park's server, presentation and input fixtures and their shared interface helpers. Replace repeated large implementations with a shared concrete implementation and small case data when the behavior is the same. Investigate generic helpers and deeply nested closure/async bodies when profiling identifies repeated monomorphization or expensive type checking. Choose concrete types or a narrow dynamic boundary only where it reduces measured work without harming the hot path.

Inspect the remaining size/orientation/provider matrices, repeated frame loops and repeated workspace setup. Preserve representative paths, bug regressions, narrow/fractional layouts, keyboard/accessibility behavior and dedicated stress/performance checks. Existing shortened loops, batching and result reuse should be retained. Verify any remaining runtime reductions in an approved run instead of declaring them complete from source changes alone.

Completion: compare compilation, executable preparation and test runtime separately. Record which assertions continue to protect each removed repetition. Keep real distinct checks and delete unnecessary coverage rather than generating a replacement test for every removed line. Do not reopen the old full acceptance catalog merely to evaluate this task.

## 5. Narrow build-script reruns and stabilize generated output

- [ ] Identify build-script inputs that cause unrelated code to rebuild.

Inspect `crates/sensei/src/lesson.rs`, owned `build.rs` files and generator input/output stamps. `sensei::teach` scans source, test, benchmark and example directories; a test-only edit can therefore rerun a library build script. Record the actual invalidation chain before changing it.

Give generated outputs explicit, complete content/tool/configuration identities, and avoid rewriting files whose bytes are unchanged. Separate test/example verification from library compilation inputs only if the same policy rules remain enforced whenever those targets are checked. Reuse the existing Anicca generator-fingerprint approach where appropriate; its parser cache is already implemented. Preserve required native dependency detection and all source-policy and migration enforcement.

Completion: an unchanged repeat produces no unnecessary generator writes or build-script reruns, relevant source/tool/configuration changes still invalidate the output, and newly added or deleted inputs are detected. Keep broad watchers when a narrower scheme cannot preserve those guarantees.

## 6. Move whole-program identity out of widely shared compilation units

- [ ] Design a smaller boundary for execution and simulation provenance.

`crates/nucleus/build.rs` fingerprints nucleus/store/engine/protein/anicca/utils sources plus the root manifest and lockfile. `crates/simulation/build.rs` fingerprints simulation/Cell/engine/nucleus/store/protein/utils/transport/anicca sources and the root Cargo inputs. Both include target/profile/flags, features and compiler identity. Their results are embedded through `LINCE_EXECUTION_BUILD_HASH` and `LINCE_SIMULATION_BUILD_HASH`, consumed by `crates/nucleus/src/projection.rs` and `crates/simulation/src/lib.rs`.

Map every consumer and the cache, replay and evidence decisions it makes. During refinement, choose whether complete build identity should be computed at executable/test-runner composition, supplied through a small existing contract, or represented by appropriately scoped identities. Keep deterministic inputs, source coverage and toolchain/feature identity. Do not remove engine/store inputs merely to suppress recompilation or substitute a revision string for actual content identity.

Completion: a consumer-only edit no longer recompiles an otherwise unchanged shared core merely to update the composed identity. Relevant behavior changes still reject stale execution/simulation results, while unchanged inputs retain stable identity. Provide focused invalidation checks for source edits, features, compiler flags and compiler changes before replacing the current boundary.

## 7. Reduce interface coupling to application services

- [ ] Extract a small, real dependency boundary for presentation and pure policy logic.

Inspect `crates/interface/Cargo.toml`, host-control and policy models, and their uses of Cell, engine, nucleus and simulation. The `native-presentation` feature currently activates a broad application graph. Identify which data and operations truly require live services and which can live behind existing lightweight types or interfaces.

Move pure models and policy logic to the narrowest appropriate existing owner. Keep service integration at the host/composition layer. Create a new crate only if it removes substantial dependency edges and has a coherent ownership boundary; a new wrapper around the same dependency graph does not improve compilation. Avoid cycles or merging unrelated concepts just to reduce the crate count.

Completion: selected model/policy checks and presentation edits rebuild less of the service graph, while real native interaction, input routing, persistence and service integration still work. Document the before/after dependency graph and shared-crate edit timings. This is broader architectural work and needs its boundary agreed during refinement.

## 8. Reuse compiled dependencies in Nix source packaging

- [ ] Evaluate a separate dependency-artifact derivation when repeated source packaging is a measured bottleneck.

The source package in `flake.nix` uses `buildRustPackage`; it does not currently expose a separate reusable compiled-dependency stage. Prototype Crane's `buildDepsOnly` and `cargoArtifacts` arrangement with a pinned input if measurements justify the change. Keep toolchain, target, native libraries, features, profile and flags aligned between dependency preparation and final application compilation. Account for workspace/path dependencies and build scripts when defining the dependency-stage source inputs. See [Crane's example](https://crane.dev/examples/quick-start.html).

Retain the narrowed source filter, separate runtime content derivation, existing package outputs, licenses/credits, build information, session registration and runtime closure. The prebuilt binary installation path must continue to use the published binary. Do not copy an arbitrary local Cargo target into the reproducible package or move tests back before binary publication.

Completion: two comparable source-package preparations with an application-only change reuse the dependency artifact, while a dependency/toolchain/native-input change invalidates it. Validate the installed package outside the development shell. Defer this task if packaging is infrequent or the reusable stage does not remove enough work to justify its maintenance.

## 9. Tune profiles around total iteration cost

- [ ] Compare development debugging and optimized acceptance preparation with matching shared dependencies.

Keep the current low-optimization incremental development profile as the baseline. Evaluate whether a dedicated fixture profile or recipe can reuse optimized dependencies while leaving frequently edited Park test code cheap to compile. Align shared dependency features with the native application's actual feature set and keep intentional test/client additions explicit. The earlier optimized-fixture experiment is evidence to reproduce, not an implemented default recipe.

Measure both edited rebuild time and fixture execution time. Optimize runtime-critical dependencies selectively. If release code generation dominates repeated preparation, evaluate codegen-unit counts and incremental release in a clearly identified experimental profile. Include changed shared-interface code, since reoptimizing it can erase a fixture-only gain. Keep shipping optimization, panic recovery and release-based FPS evidence intact. See [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).

Completion: adopt a profile or recipe only when it improves the representative edit-to-result workload with acceptable runtime. Keep ordinary debugging, optimized acceptance fixtures and shipping artifacts clearly identified so an experimental executable cannot become the release by accident.

## 10. Measure resource use and compiler-cache effectiveness

- [ ] Tune jobs and cache policy from observed behavior rather than available thread counts.

Measure the selected workloads with bounded Cargo job counts and observe memory pressure, swapping, lock waits and concurrent desktop load. Collect sccache statistics and errors to distinguish dependency hits from non-cacheable incremental workspace compilations. Investigate unexpected misses, repeated toolchain/flag variants and actual cache eviction before increasing cache size or adding another cache layer.

Retain the existing idle-timeout correction and persistent cache. If another server failure leaves duplicate compilers, establish process/output ownership before stopping anything. Do not disable incremental compilation globally to increase cache-hit statistics. Avoid putting the full target in a RAM disk on a memory-constrained host; consider a narrowly scoped temporary-input experiment only with measured memory headroom.

Completion: record the chosen job/cache policy, representative wall time and memory behavior. Leave the current defaults when differences are noise. Do not run concurrent comparison builds against the same target directory.

## 11. Evaluate further linker improvements only where linking dominates

- [ ] Recheck the real full-application link and eligible Wild improvements when link time is still material.

Wild is already selected locally on x86_64 Linux; installing or enabling it is not pending work. Verify actual compiler/linker commands so global Cargo configuration or explicit `RUSTFLAGS` cannot invalidate the comparison. Compare the same saved link inputs and then complete application preparation with the pinned Wild, mold and LLD as relevant. Retain Android target-specific linker behavior.

Consider a newer pinned Wild only after compatibility and measurements justify it. The configured Wild 0.9.0 uses full linking; future incremental-link support is a conditional revisit requiring verified upstream support and a real compatibility trial. Do not claim Rust incremental compilation already provides incremental linking. See [Wild](https://github.com/wild-linker/wild).

Completion: retain the faster compatible choice with repeatable link and total-preparation evidence. Defer further linker work when frontend/code generation dominates or the existing Wild setup already makes linking insignificant.

## 12. Try Bevy dynamic linking for development

- [ ] Run a development-only dynamic-linking trial if step 11 shows final application relinking is expensive.

Determine the compatible Bevy 0.19.1 feature and dylib arrangement for this workspace. Pin the experimental recipe, preserve the ordinary artifact variant, and measure dependency preparation, edited application relinks, startup and the required native graphics/Wayland behavior. Account for the new artifact/feature variant and runtime library discovery.

Completion: keep the option only if it materially improves full-app iterations after its setup cost. Shipping archives and installed packages must remain self-consistent and must not depend on an accidental development dylib or development-shell library path. Reject the experiment if Rust frontend work is the dominant cost and total iteration does not improve.

## 13. Try the official Cranelift code-generation backend

- [ ] Perform a bounded compatibility trial before benchmarking an alternate development backend.

Use an explicitly selected compatible compiler/backend and a persistent experimental target location so it does not replace the stable default or churn the existing target on every invocation. Check the workspace's required architecture intrinsics, native dependencies, panic unwinding/recovery and graphics behavior. Upstream currently documents partial `std::arch` support and experimental unwinding; inspect the pinned version before choosing the recipe. See [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift).

Completion: record either the first concrete compatibility blocker or comparable development edit/check/run timings with working required behavior. Do not silently switch the application to panic abort to make the trial succeed. Stable LLVM remains the authority for shipping and release performance measurements unless a separately refined change is approved.

## 14. Try compiler frontend parallelism

- [ ] Evaluate a compiler-supported frontend parallelism setting if profiling shows checking/type analysis dominates.

Inspect the exact pinned compiler's available interfaces before selecting flags. Rust 1.96.0 did not expose the newer frontend-jobs interface in the earlier study; any nightly experiment must pin its compiler and keep its artifacts separate from the stable baseline. Do not use `RUSTC_BOOTSTRAP` or assume a flag from another compiler version applies.

Coordinate per-crate frontend threads with Cargo's crate jobs and the host memory budget. Measure representative engine, Cell, interface and simulation edits and complete iteration wall time. Completion: retain the option only with a repeatable full-workload improvement, acceptable memory use and correct required builds. Otherwise record the unsupported interface or oversubscription result and defer it.

## 15. Try selective code-generation hints

- [ ] Investigate `hint-mostly-unused` for a measured expensive crate with a broad API used narrowly.

Verify support in the selected compiler and whether the candidate's binding/API usage fits the optimization. Compare the candidate's own code generation, downstream monomorphization and complete workload. Avoid applying it globally or assuming generic-heavy, mostly-used crates benefit. Preserve generated bindings and application behavior. See [Rust's guidance](https://blog.rust-lang.org/inside-rust/2025/07/15/call-for-testing-hint-mostly-unused/).

Completion: keep a narrowly scoped setting only with supported syntax and repeatable improvement. Record inapplicable candidates and regressions so the same speculative experiment is not repeated without new evidence.

## 16. Revisit compiler upgrades and Blaze selectively

- [ ] Evaluate a newer pinned stable Rust when its compiler improvements address the measured bottleneck.

Compare the same representative edit workloads and required native/headless/shared targets, including the one-time cost of losing compiler-specific cache reuse. Keep a working stable baseline and avoid repeatedly changing the compiler during unrelated debugging. Upgrade only after the actual dependency graph and required behavior pass.

- [ ] Revisit Blaze only after a release addresses the observed compatibility blocker.

The earlier Blaze v0.20261027.0 trials failed in the standard library panic path with both attempted backends. First reproduce a small representative compatibility trial, checking its owned toolchain, native build scripts/proc macros, target/argument support and panic recovery. Stop at the first unresolved blocker instead of launching a full application build. See [Blaze's Cargo documentation](https://github.com/indexable-inc/blaze/blob/main/docs/cargo.md).

Completion: retain a reproducible result for the selected version and workload. Any affected-test cache must distinguish hermetic computation from Park's real Wayland/graphics/session interactions and preserve executable, environment/tool and evidence identity. Ordinary Cargo checks and actual required executions remain authoritative.

## 17. Test the huacnlee Cargo fork in one checkout

- [ ] Pin and inspect [huacnlee/cargo](https://github.com/huacnlee/cargo) before a compatibility trial.

Record the fork's commit/release and Cargo version, the selected rustc version, and every behavior that differs from ordinary Cargo. Invoke an explicitly selected experimental binary; keep the rustup Cargo proxy and normal shell configuration intact. Check `rust-toolchain.toml`, locked dependency resolution, workspace/path dependencies, package selection, feature selection, Cargo aliases and the existing xtask entry points. Verify that nested Cargo invocations actually use the intended binary. Distinguish Cargo orchestration changes from a change of rustc, linker, debug information, optimization or backend.

Upstream currently documents fixed shared build directories, ignored target-directory overrides, automatic cache pruning and changed dependency debug information. Before running against Lince, verify the pinned version's actual configuration and establish isolation from retained baseline artifacts and other active builds. Do not assume `CARGO_TARGET_DIR` provides isolation or let an installer replace the normal Cargo binary. Record a blocker if the tool cannot preserve the existing artifacts and required behavior.

- [ ] Compare ordinary Cargo and the fork on this existing checkout and branch.

Use step 1's recipes with the same toolchain, Wild/Clang, features, profile, job count and source edits. Measure initial preparation separately, then three unchanged repeats and three comparable test-only, leaf and shared-crate edits. Include executable preparation and startup when supported, using Nix/xtask execution, and retain medians, rebuilt units, cache evidence and failures under `target/build-iteration`. Run each tool sequentially. Do not create another worktree or include cross-worktree/project benchmarks in this trial.

Compare the current usable defaults first, then control differing profile/debug/backend settings where the fork permits it. Report any setting that cannot be matched. A smaller artifact directory or reuse demonstrated across several worktrees does not establish faster editing in this checkout. No `cargo clean` or deletion of retained caches is part of the experiment.

- [ ] Verify dependency reuse and invalidation inside the single checkout.

Exercise repeated package selection, a workspace-only edit and controlled changes of features, profile and compiler flags. Inspect actual compiler invocations to distinguish unchanged dependencies reused from recompilation; verify relevant dependency or compiler changes invalidate incompatible artifacts. Check the existing interaction between sccache and incremental workspace compilation. Confirm warnings remain errors, the intended linker is selected, and build identity, generated outputs and required behavior remain correct.

Completion: record compatible operations, concrete failures, setup costs and comparable edit-to-result timings. Recommend adoption only if the fork improves Lince's single-checkout workflow without weakening correctness or disrupting retained artifacts. Leave cross-worktree sharing as an untested upstream claim for this scope. This section records future tasks; no fork trial is authorized to start by this document edit.

## 18. Test Corgi in the same single checkout

- [ ] Pin [ConradIrwin/corgi](https://github.com/ConradIrwin/corgi) and verify platform eligibility.

Identify the exact `corgi-build` release/commit and its supported operating systems before preparing the experiment. Upstream currently describes macOS-only support; verify whether the selected version can run on this Linux/Nix host. Record unsupported platform/toolchain requirements as a blocker rather than presenting a planned benchmark as a completed test. Keep the ordinary Cargo toolchain and the current checkout intact.

- [ ] Check Cargo compatibility against Lince's actual build inputs.

Verify package/features/profile selection, lockfile and path dependencies, build scripts, proc macros, native-library discovery and generated outputs. Account explicitly for Corgi's sandboxed tools/environment/input declarations: inspect `pkg-config`, Clang/libclang, native graphics/session libraries, Anicca generation, source-policy enforcement and execution/simulation source fingerprints. Keep migration guards enforced and source coverage complete. Record any required experimental configuration separately from the application defaults.

Test existing xtask integration explicitly: upstream does not promise support for every inherited Cargo subcommand. Verify which commands use Corgi and which still call Cargo. Its feature unification may activate a different graph; compare native, headless and narrow helper selections and report added dependencies or changed features instead of attributing those differences to cache speed.

- [ ] Benchmark Corgi against ordinary Cargo and the huacnlee fork using section 17's single-checkout protocol.

Use the same representative source edits, baseline conditions, three-run medians, preparation/runtime separation and compiler/cache receipts. Establish tool-supported cache isolation before the trial and retain existing caches. Measure repeated package selections and actual dependency reuse within this checkout. Record compiler/backend/profile differences that cannot be controlled. No new worktrees, second repository, remote cache service or cross-project sharing experiment is needed.

- [ ] Verify the correctness of Corgi's cached test results before relying on them.

Separate deterministic checks from fixtures that use real graphics, Wayland, session buses or external tools. Verify that relevant executable, source, environment, tool and evidence changes invalidate retained results. Select a supported way to rerun or exclude unsuitable tests from result caching, and compare those executions with ordinary Cargo. A cache hit must not substitute for required physical/session acceptance or release performance measurements.

Completion: retain platform and compatibility findings, matched performance measurements where execution is supported, required configuration and a recommendation. If blocked, leave the exact unsupported requirement and the condition for revisiting it. Do not adopt or globally install Corgi as part of planning these tasks.

## 19. Consider larger build infrastructure only after local improvements

- [ ] Reassess distributed compilation or shared remote artifact caching only if another available machine or trusted service would remove a measured remaining cost.

Specify machine availability, cache trust, target/CPU/toolchain/native-library compatibility, invalidation, transfer/setup overhead and behavior when the service is unavailable. Preserve a usable local path. Defer this work while the intended solution is local and no suitable resources have been identified.

- [ ] Reassess Bazel/Buck, Docker/cargo-chef or another build-system replacement only if Cargo/Nix still cannot address an established bottleneck.

Identify the exact dependency or cache problem the replacement solves and compare migration/maintenance cost with the remaining saved time. Do not add a second build framework merely to reproduce working Cargo/Nix caching.

- [ ] Consider PGO, BOLT, full LTO or a single release codegen unit only for a separately established runtime-performance objective.

These are conditional runtime optimizations, not defaults for faster editing. Include training/optimization cost, runtime measurements and repeated release preparation before adopting any of them. Keep them deferred when compile turnaround is the active objective.

## 20. Make future simplification reviews actionable

- [ ] Define a focused review recipe for recurring Fiote/codebase cleanup if the user requests recurring reviews.

Use the existing trail mechanism. Each review should identify a duplicated or unused behavior, the concrete ownership/dependency simplification, the compile unit or execution stage removed, a comparable before/after measurement and the retained observable behavior. Prefer one small improvement per iteration and delete dead abstractions instead of adding maintenance layers.

Completion: the review produces implementable, evidence-backed tasks using the criteria above. Do not create an agent scheduler, enable automatic deletions, schedule recurring work or modify runtime Records without the user's corresponding instruction.
