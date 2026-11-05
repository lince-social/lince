# 7. Remove executed bloat and improve the shipping binary

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

Rank waste in executed paths first. Remove one obsolete owner/adapter/derivation and its wiring, then measure the corresponding runtime or startup/build cost. Keep semantic functions, feature availability, licenses and declared behavior. This stage is not permission to edit the web crate, migrations already committed, AGENTS, README or owner Records. The source feature inventory P124 and critical path P66 feed the bloat inventory P86.

The user accepts slower production compilation for a better runtime binary. Test release variants with the same frozen source, feature flags and workload after structural changes stabilize; no local release build starts merely because these tasks exist. Codegen units affect compilation partitioning, not runtime worker count. Incremental release compilation is already disabled in the audited shipping recipe; uncommenting that alone adds no runtime optimization.


- [ ] **P86 — Rank actual bloat in active paths.**

    **Why:** large files and dependency counts alone do not identify wasted work. **Scope:** inspect active feature/dependency reachability, initialization, registered systems, duplicate derivations/adapters and ownership plumbing across interface and the backend paths P66 implicates. **Close with:** a short ranked inventory naming one removable or consolidatable unit per entry, evidence of its current use, and its runtime, startup/build or maintenance cost. Exclude archived/compiled-out code from FPS claims and do not turn this into a rewrite of every crate.

- [ ] **P87 — Remove one proven obsolete active-code path.**

    **Why:** dead or superseded branches increase maintenance and can retain unnecessary initialization/dispatch work. **Scope:** choose one exact P86 entry, trace callers and feature configurations, then remove its obsolete implementation and wiring without preserving unused-version compatibility. **Verify:** declared current capabilities, applicable checks and any measured resource effect. Protected files and migrations remain untouched. If no executed work changes, describe the benefit as simplification/build cleanup rather than a responsiveness fix.

- [ ] **P88 — Consolidate one duplicated pure coordinate-mapping operation.**

    **Why:** projection/mapping formulas appear in multiple presentation and input paths, increasing derivation cost and disagreement risk. **Scope:** assess host/raster, presentation/mapping and native_input projection/transfer/pointer; share one genuinely identical pure operation. Keep each caller's authority, lifetime and eligibility checks. **Verify:** scaling, rotated/projected geometry, clipping, output epochs, queued presses and replacement. Different native/client/capture policies must not be merged merely because the arithmetic resembles each other.

- [ ] **P89 — Consolidate one redundant derived-state cache or resolver layer.**

    **Why:** overlapping caches can repeat work and obscure invalidation/retirement. **Scope:** select one P86-proven overlap in shared style/metric/geometry/data derivation, reuse the existing canonical family and remove redundant storage/callers. **Verify:** keys, source revisions, zero/restored values, reparenting, removals, memory bounds and small/changing workloads. Do not merge unrelated semantics into an all-purpose global cache or cache permission decisions. Maintain a single explicit invalidation/lifetime contract for the selected value.

- [ ] **P90 — Remove one unused dependency or redundant feature activation.**

    **Why:** unnecessary dependency activation can enlarge builds, startup work and resident resources. **Scope:** choose one P86-proven unused active dependency/flag and verify its transitive uses in the supported shipping configuration. **Verify:** package/check coverage, startup/build or memory change and availability of existing features. Do not disable a supported Castle, decoder or backend to make benchmarks faster. A build-size improvement is worthwhile but must not be reported as a measured frame-time gain without runtime evidence.

- [ ] **P92 — Initialize one unused-at-startup resource family on demand.**

    **Why:** optional backend/watch/decoder/model resources may incur cost before any view or operation needs them. **Scope:** choose an actual startup allocation/worker from P86, retain the feature and coalesce first-use initialization through existing ownership infrastructure. **Verify:** startup and idle resources, first-open latency, simultaneous consumers, close/reopen, cancellation and cleanup. Preserve required background semantic operations. Registering a plugin does not prove its resources initialize eagerly; establish that first.

- [ ] **P96 — Compare cross-crate ThinLTO with the current release profile.**

    **Why:** cross-crate optimization may remove overhead across the many interface/backend crate boundaries. **Scope:** compare the recorded baseline profile with release lto = "thin" on identical frozen source, keeping codegen units, target, features and panic behavior fixed. **Verify:** the representative CPU preparation, input latency, FPS, startup and binary-size results, with build/link duration and peak memory recorded. Run the affected correctness/capture checks on the resulting artifact. Accept slower compilation if runtime gains justify it; enabling the flag is not itself evidence of a faster desktop.

- [ ] **P97 — Compare release codegen-units = 1 with the current partition count.**

    **Why:** fewer partitions may give LLVM more optimization opportunities inside each crate. **Scope:** compare one versus the recorded baseline count of 16 while holding LTO and all other options fixed. If the selected LTO mode later changes, repeat only the comparison needed to establish that interaction. **Verify:** complete-path CPU/input performance and small/large scene controls, plus compilation cost and memory. This does not make Park single-threaded at runtime. Do not assume the combined gain is the sum of independent LTO and codegen results.

- [ ] **P98 — Compare full LTO with the best measured ThinLTO release.**

    **Why:** the user prefers final binary quality over short production builds, and full cross-crate optimization may provide an additional gain. **Scope:** test lto = true/"fat" against the P96 result at a fixed codegen count and otherwise identical configuration. **Verify:** representative runtime and correctness, including a control scene, and record linker/toolchain support, peak build memory and CI duration. Full LTO is not guaranteed faster than ThinLTO. If it wins, make the necessary production build budget explicit; retain ThinLTO when full LTO adds cost without a dependable benefit.

- [ ] **P99 — Test representative profile-guided optimization for the selected release profile.**

    **Why:** actual branch/call frequency can guide optimization beyond generic compiler settings. **Scope:** after the major runtime changes stabilize, train an instrumented candidate on normal native content, interaction and client activity; merge profiles with a matching LLVM toolchain and evaluate the final non-instrumented artifact. Keep flags/source consistent between training and use. **Verify:** held-out daily workloads, startup and correctness, stale/missing profile diagnostics and repeatability. Do not train solely on the Firefox FPS fixture or benchmark the instrumented binary as the final result. Keep this conditional on a maintainable release workflow and measured benefit.

- [ ] **P100 — Compare a deliberate CPU target with the shipping target.**

    **Why:** available SIMD/instructions may help some CPU work, while an accidental CI-host target may produce the wrong artifact. **Scope:** establish the actual machine's capabilities, then compare the current target with one supported explicit CPU/ISA target at fixed LTO/codegen settings. A machine-specific Nix build and a distributable CI artifact must have clearly identified target assumptions; do not use target-cpu=native on a random runner as a proxy for this laptop. **Verify:** actual execution, CPU-heavy and interaction workloads, code size and thermal effects. Keep the existing target if there is no measured gain; changing a linker or CPU name alone does not establish better runtime code.

## Established release baseline

The completed release-profile audit verified the running c9f56419e4ac38dd1d83a9a18d6d66786837c674 binary against its completed CI build log and Nix packaging recipe. Its baseline is Rust 1.99.1 (58cf8f9c3), x86_64-unknown-linux-gnu, ui/instinct/park, opt-level = 3, debug = false and panic = "unwind". The lto, codegen-units and incremental lines are commented: Cargo supplies lto = false (local ThinLTO, not cross-crate LTO), codegen-units = 16 and incremental = false. CI additionally sets CARGO_INCREMENTAL = 0, so uncommenting incremental = false alone does not improve this binary. Codegen units govern compilation partitioning, not runtime thread count. Preserve panic recovery in comparisons. The receipt distinguishes observed flags from inferred Cargo defaults; optimized = true alone does not attest all compiler settings.

The observed shipping RUSTFLAGS are -D warnings -C link-arg=-fuse-ld=mold. They override .cargo/config.toml's target-cpu=native, leaving rustc's target default. The installed Nix derivation repackages this CI artifact without recompiling; a Nix source build instead selects clang/wild, and the development shell can select native CPU instructions. Treat those as separate variants. Twelve recipe inputs matched the shipping commit and all seven existing cache-identity tests passed. The receipt records the source, artifact hashes and baseline identity; current uncommitted work is excluded. For subsequent experiments freeze identical source for both variants and record their effective flags and artifact identities. P100 tests a deliberate target rather than blindly enabling runner-native instructions.
