# BeautifulRust adoption plan for Lince

**Status: local rollout in progress.** This plan selects rules from
BeautifulRust, records the checks in use, and defines when each item is done.

Keep Linux and Android CI jobs, commands, and feature matrices as they are.
Use the Rust compiler policy already in place for ordinary builds and run
Clippy locally. Keep the pinned toolchain, existing build and test entry
points, and release profiles. Rust porting is out of scope. All Cargo output
for future implementation stays under the repository's `target/`. Follow the
[Build Record](Build.lingua) when choosing commands.

Use the [writing skill][writing], [review skill][review], [lint levels][levels],
[Clippy settings][settings], and [Bevy reference][bevy] from upstream revision
`b9718436d77ee7bcf17cdcb6aa27628feb95d97a`. The tables below give each setting
an explicit disposition. **Pilot** means a local blocking check once the chosen
scope is clean. **Audit** means use the rule to find and evaluate candidates.
**Hold** means a prerequisite or outsize migration cost prevents enabling it.

## Review and completion

Use rustc, Cargo, and stock Clippy for rules they already support. Declare
lint levels in Cargo manifests, use `clippy.toml` for supported options, and
put justified exceptions on the smallest appropriate source item. Use the
tools' own diagnostics for audits. Keep custom source parsers, diagnostic
filters, and exception inventories out of the enforcement path. Consider
Sensei only for an agreed Lince-specific rule that the standard tools cannot
express. Use review guidance for decisions that need human judgment.
[Cargo lint inheritance][cargo-lints] · [Clippy configuration][clippy-config]

Investigate one rule at a time before asking for implementation approval.
Search for occurrences across all Rust crates in the checkout, including
standalone Cargo projects, build scripts, tests, examples, and vendored code.
Read enough context to tell whether the straightforward edit preserves
behavior and still compiles. Record any exception, why it matters, and what
would need a decision. In the conversation, report whether occurrences exist,
whether straightforward removal would break anything, and whether any
exception needs the owner's instructions. Keep the occurrence details here.
Wait for the owner's approval before implementing the rule or editing an
exception.

A package pilot counts as evidence. A rule leaves the pending list only when
adopted and verified across the codebase. Inventory workspace members and
standalone manifests, including firmware and plugins. Check applicable targets
and feature configurations locally, and record the outcome for vendored and
generated sources. Leave unverified crates or targets pending unless the owner
explicitly accepts a documented exception. Keep CI unchanged.

## Evidence and rollout

- **Existing build.** Lince uses Rust 2024 and pins Rust 1.99.1, including
  Clippy and rustfmt. The workspace denies warnings for inheriting crates.
  Plain Cargo checks run compiler lints and existing build scripts. They do
  not run Clippy. Adding a Clippy rule to a manifest does not make it a build
  or merge gate. A Rust compiler lint can affect ordinary builds immediately,
  even when its level is written as `warn`.
  [Workspace manifest](../../../Cargo.toml) · [Toolchain](../../../rust-toolchain.toml)
- **Measured sample.** Direct Rust 1.99.1 probes of `geist` passed the
  existing compiler policy and four focused Clippy checks (`unwrap_used`,
  `dbg_macro`, `todo`, and `allow_attributes_without_reason`). The full
  upstream profile with warnings denied produced 54 diagnostics for the library
  and 57 for its test target. The library result included 29 `missing_docs`,
  seven `missing_errors_doc`, and 14 cast diagnostics, some on the same
  expression. These were metadata checks of the source and test target through
  the compiler/Clippy driver. Cargo build scripts, full feature builds, and test
  execution were outside the probe. Re-establish the full Cargo baseline
  locally before enabling anything.
- **Scale.** Earlier unwrap scans include source candidates that Clippy has
  not confirmed. Use direct Clippy diagnostics by package, target, and
  feature to establish the migration scope. Review reported calls for
  reachable panics before deciding how to fix them.

1. **Baseline one package.** The `dbg_macro` check on `geist` proved the local
   workflow for that rule. See the pilot evidence below.
   The first package with production findings should be a small, owned area
   where recoverable failure can be distinguished from invariants, such as a
   document or resource-reading path. Before changing either, record the source
   revision, working-tree context, pinned toolchain, package, targets and
   features, ordinary compile result, existing behavior checks, and findings
   by lint. Look at relevant [Lince Records](Lince.lingua) and their shared
   behavior. Coordinate with work already in progress on the affected files.
2. **Fix findings under the old policy.** Use the proposed lint in report mode
   first. With `warnings = "deny"`, an ordinary `-W` can fail the run.
   `--force-warn clippy::unwrap_used` is suitable for a diagnostic inventory
   and can reveal findings even through a local suppression. A normal scoped
   `-D clippy::unwrap_used` run is the acceptance check because it respects
   justified exceptions. Separate recoverable failures, proven invariants,
   fixtures, and generated or vendored code. Fix recoverable failures with
   error propagation, a tested fallback, or an explicit handling branch.
   Prefer eliminating an impossible state through a type when that is a small
   change. A mechanical `unwrap` to `expect` pass does not remove panics.
3. **Enable locally only after the scoped fixes pass.** Check the same package,
   targets, and relevant features with the selected lint denied. Confirm that
   the rule catches a known violation in a disposable fixture and that valid
   test use still passes if exempted. Run the corresponding ordinary compile
   command and the existing tests for changed behavior under the same local
   environment. Report unresolved or unavailable targets explicitly. A passing
   lint of one feature set does not cover the others.
4. **Finish the rule across the codebase.** Expand the local invocation by
   package and target until the whole-codebase completion criteria above are
   met. Prioritize the removal of reachable panics and bad conversions.
   If a rule needs broad churn or a behavior-changing workaround, record the
   tradeoff and keep it pending for the owner to decide. Roll back the policy
   switch if needed and retain independently verified fixes.

Record each adoption with **rule, package and feature scope, baseline finding
count, real fixes, justified exceptions, post-change lint and normal compile
results, behavior checks, and open coverage**. Compare equivalent runs. Count
separately the findings fixed by eliminating a failure path and those addressed
only by clearer intent, documentation, or suppression.

## Local pilot evidence

**`dbg_macro` in `geist`** passed on Rust 1.99.1 at checkout `e6b9f081`.
The baseline `cargo check --locked -p geist` passed with the current compiler
policy. The focused Clippy check covered all targets and produced no findings,
so no debug calls needed removal. The ordinary `cargo build --locked -p geist`
also passed. A disposable fixture containing `dbg!` failed with
`clippy::dbg_macro`, and the same fixture with the expression alone passed.

```sh
cargo +1.99.1 clippy --locked -p geist --all-targets -- -A clippy::all -D clippy::dbg_macro
```

The pilot used a manual local check. When removing a `dbg!`, preserve argument
evaluation and use structured tracing for diagnostics worth keeping. Rerun the
local lint and the normal build, and verify any affected output through the
relevant entry point.

### `dbg_macro` rollout

The rule is now set to `deny` in all 48 package manifests in the checkout.
Thirty-eight inherit it from the root workspace. Ten declare it directly
because they use their own lint table or sit outside that workspace, including
the standalone [Nomad plugin](../../../crates/plugins/plugin-nomad/Cargo.toml),
Ailuros firmware, and the vendored crates with their own policies. The
[plugin host](../../../crates/plugin/Cargo.toml) and the workspace plugins
under `crates/plugins/` inherit the root policy.
After the move, both locked Linux Cargo dependency graphs resolved, all 48
packages retained the rule, and Nomad passed Clippy on all targets from its
new location.
These settings apply when running Clippy locally. Normal build and CI commands
remain as they were.

The scan found no first-party calls. Five commented-out invocations were
removed from vendored Parley. The [Smithay compositor demo](../../../crates/vendor/smithay/examples/compositor.rs)
has the only remaining executable call and a method-level exception. Deleting
it would remove the example's intentional stderr diagnostic and leave the
surface argument unused. Its upstream rustdoc examples also retain their
intentional output. The exception's explanation is in a comment to keep the
attribute compatible with Smithay's Rust 1.80.1 minimum.

The manifest-driven all-targets Clippy check passed for nine workspace
packages: Geist, Sensei, Ailuros protocol, Ithkuil, i18n, hand input, Park
clients, Flexaudio Mic, and Laya. Geist also passed an ordinary Cargo check and
build. A disposable Cargo workspace verified that an inherited deny rejects
an unguarded call, that the narrow exception passes, and that another call
outside the exception still fails. The same fixture passed an ordinary Cargo
check. A standalone check of Smithay's library and compositor example passed
with the exception and failed on `dbg!` when the exception was removed from a
disposable copy. Parley's library, a standalone all-targets check of vendored
`gemm-common`, and the Ailuros firmware's Linux host check with all targets and
features passed.

Keep the rule pending until the remaining verification is resolved. Before
the dependency upgrades, the workspace and Nomad checks hit Sensei comment
violations, and Parley's standalone test target failed separately on an
unresolved `parley_dev` reference. After the upgrades, the audit again
confirmed all 48 manifests and the same single executable exception. Sensei's
repository check passed across 3,246 files, and Nomad passed Clippy on all
targets. The Ailuros firmware's Linux host check passed again. Parley 0.11.1
passed its standalone library check, and its test target still fails to
resolve `parley_dev`. The post-upgrade workspace check finished with no
`dbg_macro` findings or Sensei failures. It exited 101 on Rust compile
errors in the Bevy UI test target, the editor's performance example, and
the facade library and test target. Android and ESP32
device target checks have not run here. The Android NDK is absent at the
configured SDK path and the pinned ESP32 Rust toolchain is not installed.

For the workspace follow-up, run the local check in the Interface Nix shell.
This command selects all workspace packages and their available targets.
Standalone packages and platform-specific feature sets still need separate
invocations.

```sh
cargo clippy --locked --workspace --all-targets --keep-going -- -A clippy::all
```

### `todo` rollout

The source scan found no first-party `todo!()` calls. All 38 non-comment
matches are in two vendored crates. [Gemm's half-precision SIMD implementation](../../../crates/vendor/gemm-common/src/simd.rs)
has 13 unfinished trait methods. Smithay has 23 [renderer test stubs](../../../crates/vendor/smithay/src/backend/renderer/element/tests.rs)
and two runtime branches in its [Wayland handler](../../../crates/vendor/smithay/src/wayland/single_pixel_buffer/handlers.rs)
and [Vulkan allocator](../../../crates/vendor/smithay/src/backend/allocator/vulkan/mod.rs).
Deleting these macros can leave methods without return values or change
which paths panic. Ordinary documentation samples are separate.

The rule is set to `deny` for all 48 packages, using the same workspace
inheritance and direct manifest settings as `dbg_macro`. The approved
exceptions cover only Gemm's unfinished trait implementation, Smithay's
renderer fixture implementations, and the two individual runtime branches.
The existing calls and their behavior are preserved. CI is unchanged.

A disposable Cargo workspace confirmed that the manifest rule rejects new
placeholders in both library and test code, accepts an explicit narrow
exception, and leaves ordinary Cargo checks alone. Geist, Sensei, and Nomad
passed their all-targets Clippy checks with the manifest setting. The Gemm check
passed on all targets with `f16` enabled. Smithay passed its library, tests,
and compositor example check with `wayland_frontend`, `renderer_gl`, and
`backend_vulkan` enabled. Those checks compiled all 38 vendored occurrences
with their scoped exceptions. Full workspace and device-target verification
remains open under the same conditions noted for `dbg_macro`.

## Pending local blocking rules

| Take from BeautifulRust | Before building or enabling | After building and running the local check |
| --- | --- | --- |
| `dbg_macro` | Scope audit, manifest changes, dormant call removal, and the Smithay exception are recorded in the rollout above. | Finish the checks across the crate and target inventory. Keep pending while required build verification is blocked. |
| `unwrap_used` | Use direct Clippy diagnostics to review legacy calls by package, target, and feature. Fix recoverable failures and justify any exceptions before raising the lint level. | Set `unwrap_used = "deny"` through Cargo for the whole package inventory, verify all applicable targets, and retire the Python pilot and its inventory. |
| `todo` | The rollout above records the enabled rule and approved vendor exceptions. For a first-party placeholder found later, finish the path or return its intended unsupported outcome once that behavior is understood. | The manifest audit, negative probes, and affected vendor checks passed. Full workspace and device-target verification remain pending. |
| `allow_attributes_without_reason` | Enabled through the manifests across all 48 packages. The rollout documented 57 existing suppressions outside Smithay. Smithay's declared Rust 1.80.1 minimum makes Clippy skip this rule there, as verified with a fixture at 1.80.1 and 1.81.0. | Confirm local checks on each affected feature set as it becomes available. No minimum Rust version or CI command changed. |

### Native Clippy rollout for `unwrap_used`

Stock Clippy checks `Option::unwrap`, `Result::unwrap`, and
`Result::unwrap_err` by type. Keep [`allow-unwrap-in-tests = true`](../../../clippy.toml)
for ordinary tests. Audit helpers and feature-built fixtures with the same
build configurations that use them. [Clippy unwrap lint][unwrap-lint]

Run the audit and acceptance checks directly in the normal local build
environment. Start with one package and replace `PACKAGE` with its name.

```sh
cargo clippy --locked -p PACKAGE --all-targets --keep-going -- --force-warn clippy::unwrap_used
cargo clippy --locked -p PACKAGE --all-targets --keep-going -- -D clippy::unwrap_used
```

The first command inventories findings even through source-level `allow`
attributes. The second checks the reviewed code with normal scoped
exceptions. Use `--manifest-path` for standalone packages and the applicable
feature and target flags for firmware, plugins, and platform builds.

Set `unwrap_used = "deny"` in `[workspace.lints.clippy]` once inheriting
packages are ready. Set it directly in `[lints.clippy]` for packages with
their own lint tables. This is a blocking local `cargo clippy` rule. Ordinary
`cargo build` and `cargo check` use rustc's own lints, and CI follows its
existing commands. [Cargo lint inheritance][cargo-lints] · [Clippy usage][usage]

Favor error propagation for recoverable failures and explicit invariant
messages where a panic is intentional. If a call must remain, use a narrow
`#[allow(clippy::unwrap_used, reason = "...")]` with a concrete explanation.
Use `#[expect(..., reason = "...")]` only where the lint reliably fires in
the applicable configurations. A legacy call earns an exception through
review of its behavior. [Rust lint levels][lint-levels]

The Python pilot used typed Clippy output and a source-line fingerprint
inventory. It is superseded by the native plan. Remove the pilot and its
inventory when replacing it with manifest enforcement. Use direct Clippy
reports for migration work from here on.

The initial cleanup reuses validated UIDs and cache entries, returns the
existing stream error on a missing stream, and keeps the video input server
alive for decoding without a paired `Option` unwrap. PDF and EPUB assumptions
use explicit invariant messages, and queued EPUB asset access reports a
poisoned lock. The terminal watch receiver's message describes why it exists
until drop. These changes leave the remaining legacy inventory under review.

Pilot checks passed on selected host targets for Geist, Sensei, Resources,
and Ailuros firmware. All six HTTP cache tests and Sensei's repository scan
also passed. A disposable Clippy fixture on Rust 1.99.1 confirmed that a
manifest deny rejects an ordinary unwrap while allowing a reason-bearing
source exception and the configured test exemption.
The workspace audit encountered unrelated compile errors in Engine's
knowledge dump, the editor's performance example, Document's Quick XML calls,
and vendored Bevy UI tests. The standalone locked checks for Nomad and
Parley need refreshed dependency lockfiles. Smithay needs a standalone lockfile
for a locked check. Device target verification stays open.

### Sensei and local safety justifications

Sensei runs its Lince-specific source-comment rule during ordinary builds
and offers a fixer. Keep that policy in Sensei while it is needed. Stock
Clippy ships a fixed set of lints with configurable options. Its documented
path for a new built-in lint is an upstream contribution. Dylint can load
third-party Rust lints in a separate command, but brings compiler-toolchain
maintenance and would need a separate plan for preserving build-time
coverage. Prefer the existing Sensei rule for this one source policy. Sensei
can be retired if Lince drops the comment policy or a supported replacement
can preserve its coverage, exceptions, and build behavior.
[Sensei build hook](../../../crates/sensei/build.rs) · [Adding Clippy lints][clippy-add-lints] · [Dylint][dylint]

Clippy already supports `// SAFETY:` through `undocumented_unsafe_blocks`.
The current obstacle is that Sensei's checker reports those comments and its
fixer strips them. The proposed narrow exception preserves nonempty
`// SAFETY:` comments and any needed continuation lines. Use Clippy to
check whether the justification is missing or attached to safe code, using
`undocumented_unsafe_blocks` and a targeted `unnecessary_safety_comment`
audit. This keeps Rust syntax analysis in Clippy. Write the applicable
validity, alignment, bounds, aliasing, ownership, or lifetime argument.
After the source policy change is agreed, verify that Sensei check and fix
preserve these comments and retain their existing handling of other comments.
Then audit unsafe sites and verify the applicable Clippy diagnostics.
A disposable Rust 1.99.1 fixture already confirmed Clippy accepts a safety
comment above the block and fails when it is removed. Review the proof
itself for soundness.
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
| `undocumented_unsafe_blocks` | **Hold.** Agree on the narrow Sensei exception above, then audit unsafe blocks and impls and add the concrete safety arguments. | Verify Sensei check and fix preserve the justifications. Use Clippy for presence and placement and review for soundness. |
| `multiple_unsafe_ops_per_block` | Review whether several FFI or pointer operations implement one invariant. Split only when it clarifies obligations. | Check that ordering, lifetime, and allocation/free pairing are unchanged. Use the platform's applicable checks. Keep this an audit. |
| `all` and `pedantic` | Leave ordinary Clippy defaults available. Sample pedantic findings on a small package first and select useful checks individually. | Report findings by lint and scope. Keep blanket pedantic enforcement off, even if many warnings have simple fixes. |
| `module_name_repetitions` | Keep upstream's allow choice. Read names in their actual API context. | Avoid broad renames. Compile callers if a specific rename has a clear benefit. |
| `must_use_candidate` | Keep allowed. Add `#[must_use]` only when ignoring the value is meaningfully wrong. | Check purposeful callers and the compiler diagnostic for accidental discard. |
| `module_inception` | Keep allowed. Review confusing paths individually. | Preserve imports and re-exports if a targeted move is justified. |
| `enum_glob_use` | Keep allowed. A local enum-variant import can clarify a match. | Check the resulting match stays unambiguous and exhaustive where required. |

## Clippy config from upstream

| Setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `allow-unwrap-in-tests = true` | Adopt for the local pilot. Identify fixtures compiled as feature-enabled library code as well as ordinary tests. `#[cfg_attr(test, test)]` alone does not exempt a Laboratory fixture when built as ordinary feature code. | Check an ordinary test and the same kinds of fixture under the affected feature. Handle justified feature-built fixtures narrowly and keep production failures visible. |
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
[lint-levels]: https://doc.rust-lang.org/rustc/lints/levels.html
[clippy-add-lints]: https://doc.rust-lang.org/clippy/development/adding_lints.html
[dylint]: https://github.com/trailofbits/dylint
[safety-lint]: https://clippy.rust-lang.org/master/index.html#undocumented_unsafe_blocks
[extra-safety-lint]: https://clippy.rust-lang.org/master/index.html#unnecessary_safety_comment
