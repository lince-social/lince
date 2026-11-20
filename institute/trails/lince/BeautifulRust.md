# BeautifulRust adoption plan for Lince

**Status: local rollout in progress.**

Keep CI, toolchain, build and test entry points, and release profiles. Run
Clippy locally with Cargo output under `target/` per the [Build Record](Build.lingua). Skip porting.

Use the [writing skill][writing], [review skill][review], [lint levels][levels],
[Clippy settings][settings], and [Bevy reference][bevy] from upstream revision
`b9718436d77ee7bcf17cdcb6aa27628feb95d97a`. The tables below give each setting
an explicit disposition. **Pilot** means a local blocking check once the chosen
scope is clean. **Audit** means use the rule to find and evaluate candidates.
**Hold** means a prerequisite or outsize migration cost prevents enabling it.

## Review and completion

Choose the enforcement for each rule in this order.

1. Use a built-in rustc lint when it covers the rule and should run during an
   ordinary build. Set shared levels in `[workspace.lints.rust]`.
2. Use stock Clippy for the remaining rules it covers. Set levels in
   `[workspace.lints.clippy]` and use `clippy.toml` for options and named API
   bans through `disallowed_methods`, `disallowed_types`, and
   `disallowed_macros`. Inherit both tables with `[lints] workspace = true`.
   Give standalone and vendor manifests equivalent coverage with only the
   exceptions they need.
3. Use Sensei for agreed, mechanically checkable Lince-specific source policies
   that built-in tooling cannot express. A rule requiring type or method
   resolution needs compiler analysis. Keep it in review until there is a
   suitable compiler-backed check. Keep judgment calls in review.

[Cargo lint inheritance][cargo-lints] · [Clippy configuration][clippy-config]

A rule leaves the pending list after codebase-wide adoption and verification.
Audit workspace and standalone crates, including firmware, plugins, build
scripts, tests, examples, and vendored or generated sources. Check applicable
targets and features locally. Record cases that would change behavior for
the owner to decide. Unverified coverage stays pending unless the owner
accepts a documented exception.

## Evidence and rollout

`cargo build` and `cargo check` enforce rustc lints and build scripts.
[`cargo clippy`][usage] also enforces Clippy lints. All run without an agent.
Use `deny` for blocking rules with scoped overrides. Inheriting crates already
deny warnings, so use `--force-warn` for the selected lint's nonblocking audit.
[Workspace manifest](../../../Cargo.toml) · [Toolchain](../../../rust-toolchain.toml)

1. **Baseline one package.** Record the revision, toolchain, targets, features,
   ordinary compile result, behavior checks, and lint findings. Consult the
   applicable [Lince Records](Lince.lingua) and coordinate ongoing edits.
2. **Audit and fix.** Prioritize reachable panics and bad conversions. Fix
   recoverable failures through propagation, a tested fallback, or a branch.
   Keep proven invariants and narrow exceptions. Bring behavior changes or
   broad churn to the owner before enabling the rule.
3. **Deny and verify.** On the same targets and features, prove a violation
   fails and a justified exception passes. Run the ordinary compile command and
   applicable behavior tests. Record coverage gaps. Roll back a failing policy
   switch if needed and retain independently verified fixes.
4. **Expand coverage.** Repeat until codebase-wide completion is verified.

Count eliminated panic paths separately from diagnostic improvements.

## Local blocking rules

| Point | Status | Remaining verification |
| --- | --- | --- |
| `dbg_macro` | Denied in all 48 packages. Smithay keeps its intentional demo and documentation output. | Complete feature and device checks. |
| `todo` | Denied in all 48 packages. Scoped exceptions preserve Gemm’s unfinished SIMD methods and Smithay’s existing fixtures and two runtime branches. | Complete feature and device checks. |
| `allow_attributes_without_reason` | Denied in all 48 packages. Added 57 reasons. Clippy skips Smithay at its Rust 1.80.1 minimum. | Complete affected feature and device checks. |
| `undocumented_unsafe_blocks` | Denied in 42 packages. Six vendored packages in Bevy, Smithay, and Gemm keep warnings. | Finish additional feature and platform checks. |
| `unwrap_used` | Stock Clippy denies it in Ailuros Protocol and firmware. | Finish the rollout before setting the workspace default to `deny`. |

The first three rules passed default-feature Linux Clippy across all Cargo
targets. Fixtures verified blocking and exceptions. Bevy UI passed 202 tests.

### Clippy rollout for `unwrap_used`

`clippy::unwrap_used` resolves `Option::unwrap`, `Result::unwrap`, and
`Result::unwrap_err` by type. Use this lint as the sole automatic check for
those calls. Keep [`allow-unwrap-in-tests = true`](../../../clippy.toml) for
ordinary tests and review app-built fixtures separately. Clippy defaults to
allowing unwraps evaluated entirely at compile time. Treat `expect` as a
separate review decision about the failure contract.
[Clippy unwrap lint][unwrap-lint]

```sh
cargo clippy --locked -p PACKAGE --all-targets --keep-going -- --force-warn clippy::unwrap_used
cargo clippy --locked -p PACKAGE --all-targets --keep-going -- -D clippy::unwrap_used
```

Use `--manifest-path` for standalone packages. A retained call needs a scoped
reason stating its invariant or panic contract. Use `allow` for conditional
diagnostics and `expect` for consistent ones. Enable workspace `deny` after the
remaining paths and exceptions pass. Rust 1.99.1 fixtures verified the deny
and its exceptions. [Rust lint attributes][lint-attributes]

Strict Clippy passed for Engine, Simulation, Nucleus, Protein, Transport,
`lince-map`, Cell, Plugins, xtask, and Android Mobile. The Map and Transport
host run passed 20 tests, and shared navigation built for ARM64.
Transfer editing, presentation, replies, and Transport handle stale state.
The Transfer rerun is pending. Interface last reported 5,722 findings.
Laboratory, Engine, and Simulation failures remain open.

### Safety comments and a smaller Sensei

Put a nonempty `// SAFETY:` explanation immediately above the unsafe block
or impl. Clippy checks for the local explanation. Review whether the code
actually meets the safety conditions. Sensei's comment rule preserves that
comment, its same-indent continuation, and attribution. It also provides
the existing build hook and fixer for Lince's comment policy.

Stock Clippy accepts configuration for its built-in lints. New generally
useful checks can be proposed upstream. Dylint can run private compiler-backed
lints with scoped Rust lint attributes, but requires a separate command and a
pinned nightly. Keep it out of this rollout. Move any future Sensei rule to
stock tools when they cover its behavior. Retiring Sensei itself requires an
owner decision about the remaining comment policy, or a replacement that
preserves build-time coverage, allowed safety and attribution comments, and
the fixer.
[Sensei build hook](../../../crates/sensei/build.rs) · [Adding Clippy lints][clippy-add-lints] · [Dylint][dylint]

The safety rule passed on default Linux, both Android ABIs, ESP32-S3, and all
features of the four vendored Bevy crates. Smithay, Gemm, and Parley's library
passed. Parley tests need `parley_dev`. Mobile preview passed 33 of 34 tests.
Its original system ordering creates a cycle. Store's snapshot test timed out.
[Sensei rule](../../../crates/sensei/src/rules/no_comments.rs) · [Missing safety comments][safety-lint] · [Unnecessary safety comments][extra-safety-lint]

## Clippy audits and settings with prerequisites

| Upstream setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `expect_used` | Keep upstream's allow level. Identify reachable panics in review and require a concrete invariant for `expect`. | An `expect` message states why the value exists. If failure is expected, exercise the actual error path. |
| `panic_in_result_fn` | Read each reported panic alongside the function's failure contract. Decide whether to propagate a typed error or preserve an invariant assertion. | Test error propagation through the caller. Review panic sources the lint may miss, such as indexing or calls into another layer, where relevant. |
| `missing_assert_message` | Identify assertions whose failure leaves the relevant state ambiguous. Prefer values and context that help find the cause. | Inspect one failure diagnostic where useful. Preserve the assertion condition and avoid exposing sensitive values. |
| `allow_attributes` | Audit existing allows separately from the reason check. Review which suppressions are scoped to specific features or targets. | Confirm each replacement expectation is fulfilled in the builds where it appears. Defer a blanket ban while legitimate conditional allows remain. |
| `fn_params_excessive_bools` | Review ambiguous mode arguments. The `source_text(uri, limit, refresh)` example has 16 source and test call sites. Prefer a named cache policy if the migration is worth it. Keep clear setters and short local helpers. | Compile all callers and run the HTTP cache and publication tests. Check cached, forced refresh, and local-file reads against the original semantics. |
| `struct_excessive_bools` | Separate independent switches from mutually exclusive states. Check any serialized fields or schema exposed to other crates. | Verify the allowed states and round-trip existing persisted values. Preserve independent settings and defaults. |
| `redundant_clone` | Inspect ownership and lifetimes before moving values. Keep copies that represent distinct catalog entries or snapshots. | Verify output, mutation, and retry behavior. Measure allocations or runtime only if a performance gain is claimed. |
| `clone_on_ref_ptr` | Use as optional clarity guidance for distinguishing shared pointer clones from data copies. Avoid a rename-only sweep. | Confirm the clone still shares the same value and preserve existing lifetimes. Changing the spelling alone gives no speedup. |
| `rc_buffer` | Check whether sharing can use a slice or `str` and whether callers need mutation or recovery of an owned buffer. | Compile downstream callers and check ownership and serialization. Claim allocation or memory savings only with corresponding evidence. |
| `cast_possible_truncation` | Start with dimensions, lengths, and persisted or platform-bound values. Decide how to round, reject, or saturate before changing a conversion. | Exercise minimum, maximum, and out-of-range values. For document cropping, check the same pixels and keep offset arithmetic in the checked integer width. [Crop code](../../../crates/document/src/lib.rs) |
| `cast_sign_loss` | Determine whether negatives are errors, meaningful signed coordinates, or values intentionally clamped before conversion. | Check negative, zero, and boundary values. Preserve legitimate offscreen geometry and reject invalid lengths where intended. |
| `as_conversions` | Use as a focused inventory. Prefer `From` for lossless conversions, `TryFrom` when range matters, and deliberate casts for documented rounding. | Keep the blanket rule off. Review justified conversions and existing numerical behavior before making a narrow audit stricter. |
| `exhaustive_enums` | Identify truly external types expected to gain variants. Closed domain or wire enums may need exhaustive matches. | Compile consumers and preserve serialized variants and match behavior. Defer changes to existing public types if downstream compatibility cannot be established. |
| `string_add` | Distinguish construction from appending to a reusable buffer. Check whether a proposed `format!`, `write!`, or `push_str` is clearer for the actual use. | Compare emitted text or bytes where they are a contract. Measure hot paths before claiming reduced allocation. |
| `wildcard_imports` | Identify application globs separately from Bevy preludes and conventional test imports. | Check targeted modules and their feature variants. Preserve the framework and test exceptions. |
| `print_stdout` | Identify ordinary CLI output, scripts consuming stdout, and Cargo build directives. Move stray library diagnostics to tracing. | Verify stdout and output formats through the same entry points. Keep intentional output. |
| `print_stderr` | Identify intentional CLI error reporting and library logging. Check launchers and scripts that capture diagnostics. | Verify failure messages, exit status, and log routing. Adopt only in libraries where the distinction is clear. |
| `missing_errors_doc` | Write actionable error contracts for high-value library boundaries. Decide where source docs live under Sensei's no-comments rule. | Check generated rustdoc and its links. Compare failure cases with the documented categories. Keep workspace-wide enforcement on hold. |
| `missing_panics_doc` | Identify remaining reachable panics and distinguish technical invariants from ordinary caller failures. | Verify the published panic conditions and any fallible alternative. Keep workspace-wide enforcement on hold. |
| `multiple_unsafe_ops_per_block` | Review whether several FFI or pointer operations implement one invariant. Split only when it clarifies obligations. | Check that ordering, lifetime, and allocation/free pairing are unchanged. Use the platform's applicable checks. Keep this an audit. |
| `all` and `pedantic` | Leave ordinary Clippy defaults available. Sample pedantic findings on a small package first and select useful checks individually. | Report findings by lint and scope. Keep blanket pedantic enforcement off, even if many warnings have simple fixes. |
| `module_name_repetitions` | Keep upstream's allow choice. Read names in their actual API context. | Avoid broad renames. Compile callers if a specific rename has a clear benefit. |
| `must_use_candidate` | Keep allowed. Add `#[must_use]` only when ignoring the value is meaningfully wrong. | Check purposeful callers and the compiler diagnostic for accidental discard. |
| `module_inception` | Keep allowed. Review confusing paths individually. | Preserve imports and re-exports if a targeted move is justified. |
| `enum_glob_use` | Keep allowed. A local enum-variant import can clarify a match. | Check the resulting match stays unambiguous and exhaustive where required. |

## Clippy config from upstream

| Setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `allow-expect-in-tests = true` | Hold while `expect_used` is allowed. | If a package later audits `expect_used`, verify the intended test behavior in that package before adding the option. |
| `allow-print-in-tests = true` | Hold until a selected package enables either print lint. | Verify fixture output remains available while unexpected library output is flagged. |
| `max-fn-params-bools = 0` | Use for an opt-in API audit so even one eligible boolean becomes visible. Classify intent and caller count. | Confirm a named mode or clear pair of operations is an improvement at the affected call sites. Keep the threshold out of the default pilot. |

## Rust compiler settings from upstream

| Setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `unsafe_op_in_unsafe_fn` | Retain existing enforcement. Rust 2024 already warns about unsafe operations in an unsafe function that lack an explicit block, and Lince denies warnings. | Verify any changed unsafe function with the normal build. Scope new blocks tightly and review their proofs. No new compiler lint setting is needed. |
| `missing_debug_implementations` | Audit at a deliberate API boundary. Identify secrets, handles, and expensive fields before considering `derive(Debug)`. | Verify useful, redacted output and avoid a workspace setting that would turn unrelated missing implementations into errors. |
| `missing_docs` | **Hold** as a broad compiler lint. Sensei disallows doc comments. For selected contracts, `#[doc = include_str!(...)]` can reuse external Markdown and satisfies `missing_docs` on the pinned toolchain. | Build rustdoc for a chosen package and inspect links and exposed items. Require a compatible source policy and clean target baseline before considering enforcement. |
| `unreachable_pub` | Audit one library. Identify re-export paths, cross-crate callers, and feature-dependent visibility. | Compile consumers and the relevant feature variants after narrowing visibility. Keep ordinary builds on the current compiler policy while this is an audit. |
| `rust_2018_idioms` | Review individually useful diagnostics. The group includes `elided_lifetimes_in_paths`, an explicit exception in the upstream Bevy reference. | Avoid a blanket group switch. Where warranted, check a specific lint on one package and verify the framework signatures still work. |

## Adapt the writing and review skills to Lince

Use a short repo skill under `.agents/skills/` only in the later
implementation. Pair a writing workflow with a focused review workflow in
that skill. Keep the five upstream style-guide digests and the Bevy digest as
references to consult for the specific question at hand. There is no need to
copy the rules into the README or add an always-loaded instruction. Point the
skill to actual local commands and preserve the shared Lince and Lingua
instructions. Review findings should identify the location, rule, observable
consequence, and smallest credible fix, with correctness ahead of style.

| Guidance taken | Before building or editing | After building or reviewing |
| --- | --- | --- |
| Ownership and borrowed inputs | Inspect caller needs. Prefer `&str`, `&Path`, and slices for readers. Accept owned values when stored, and check whether `Arc` or interior mutability expresses real shared state. | Verify lifetimes, ownership transfer, and output. Preserve deliberate sharing and snapshots. Use the simplest form supported by the caller. |
| Types, visibility, and API evolution | Prefer validated constructors, semantic newtypes, and named modes where they rule out real mistakes. Review stable serialized shapes, re-exports, and public call sites before changing fields or signatures. Use standard traits where their contracts fit. | Compile consumers and round-trip persisted or wire data. Check any `Eq`/`Ord`/`Hash` contracts, defaults, and previously valid input. Keep builders, extension traits, and generics proportionate. |
| Error contracts and recovery | Preserve causes where callers or diagnostics need them. `EngineError` already holds upstream errors and implements `Error` without `source()`. That is a concrete candidate before a wider string-error migration. Preserve documented error messages and the distinct CRDT import isolation and diagnostics lock recovery policies. | Verify cause chains and unchanged caller-visible text/codes. Exercise the original refusal, recovery, and cleanup behavior. Do not apply upstream's general prohibition on catching panics or recovering poisoned locks to these boundaries mechanically. [Engine errors](../../../crates/engine/src/error.rs) · [CRDT guard](../../../crates/engine/src/collab_guard.rs) · [Diagnostics](../../../crates/utils/src/diagnostics.rs) |
| Idioms and deterministic output | Choose a short iterator when clearer, a loop when it explains control flow better, and a deterministic order wherever output or persistence depends on it. | Compare actual outputs and ordering. Treat review-only idioms as opportunities on the code being worked on. |
| Async, locks, and cleanup | Read awaits, callbacks, lock scopes, queue limits, and `Drop` paths. Check which errors can be retried and which resources need deterministic release. | Exercise success, failure, disconnection, or cancellation where affected. Verify no extra contention, blocking, or unbounded queues were introduced. |
| Bevy correctness | Use the exact Bevy version pinned in Lince's manifest. Preserve by-value system parameters, the prelude, passive data fields, and justified complex queries. Treat upstream's allowances for elided lifetimes, pass-by-value, type complexity, and argument count as framework guidance. Check disjoint queries, deferred commands, disappearing entities, and same-frame message dependencies. | Exercise affected systems in existing headless or Laboratory cases. Check the systems together where ordering matters. Map applicable owner requirements to cases. Preserve readable/selectable Protein text and the required Protein-first case when those paths are touched, and verify edit-panel geometry for affected layout changes. [Bevy dependency](../../../crates/interface/Cargo.toml) · [Lince requirements](Lince.lingua) |
| Documentation, formatting, and tests | Identify what callers need to know about errors, safety, inputs, and side effects. Use existing local format and test commands for the package. Add regression coverage for behavior the current tests would miss. Consider upstream fuzzing or Miri locally only for a parser or unsafe boundary with a concrete need. | Review rustdoc where adopted, exercise the normal entry point, and check the diff for unrelated formatting. Record what was inspected or tested and any uncovered feature or system. No new CI work is proposed. |
| Crates, features, and dependencies | Prefer small coherent changes over speculative crate splits. Preserve additive feature behavior and exact pins with compatibility reasons, including `iroh` and `loro`. Identify consequences for the lockfile and shipping feature graph before changing either. | Verify affected feature combinations and protocol or persistence compatibility. Record measured build differences if requesting a build optimization. |
| Performance | Name the work expected to disappear, such as an allocation, copy, wait, or redraw. Establish the relevant existing workload before changing a hot path. | Run matched Park measurements for performance claims and verify the applicable behavior. Report regressions and mixed results. A lint count or an isolated microbenchmark alone does not establish a desktop improvement. [Park plan](park/Park.lingua) |
| Review workflow | Scope the review to the requested file, crate, or diff. Identify whether it is an external API or application implementation and note what local checks are applicable. | Report priority, location, rule, concrete small fix, exceptions, and what the code already does well. Say which checks actually ran. Keep accepted rule changes in the lint configuration and review decisions in the appropriate documentation. |

[writing]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-best-practices/SKILL.md
[review]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-review/SKILL.md
[levels]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/lints/lints.toml
[settings]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/lints/clippy.toml
[bevy]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-best-practices/references/bevy.md
[usage]: https://doc.rust-lang.org/stable/clippy/usage.html
[cargo-lints]: https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table
[clippy-config]: https://doc.rust-lang.org/clippy/lint_configuration.html
[unwrap-lint]: https://clippy.rust-lang.org/master/index.html#unwrap_used
[lint-attributes]: https://doc.rust-lang.org/reference/attributes/diagnostics.html#lint-check-attributes
[clippy-add-lints]: https://doc.rust-lang.org/clippy/development/adding_lints.html
[dylint]: https://github.com/trailofbits/dylint
[safety-lint]: https://clippy.rust-lang.org/master/index.html#undocumented_unsafe_blocks
[extra-safety-lint]: https://clippy.rust-lang.org/master/index.html#unnecessary_safety_comment
