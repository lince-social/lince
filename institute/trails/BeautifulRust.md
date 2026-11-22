# Lince Rust refactoring plan

Start with one recurring Rust problem across the codebase at a time. Then
use the crate queue to finish the remaining design, structure, and leanness
work. `// non-lean-yet` marks files that still need that complete review.
Sensei's shared registry allows regular source comments and refactor markers.
[Sensei rule registration](../../../crates/sensei/src/rules/mod.rs)

Review every repository Rust file, including tests, examples, build scripts,
firmware, plugins, standalone crates, vendored sources, and checked-in generated
code. Preserve Lince's observable behavior, runtime performance, and readability.
Keep the existing CI, stable toolchain pin, release profiles, and ordinary entry
points. Keep Cargo output under `target/`. Skip language ports.
[Build Record](Build.lingua)

## Phase 1. Codebase-wide problem sweeps

Audit one problem across all applicable packages before starting the next.
Review call sites in groups that share a contract, even when they live in
different crates. Record the rule, covered targets and features, existing
failures, and scoped exceptions. Bring cases that would change Lince behavior
to the owner with their consequence. A blocked case stays pending while
independent work continues. Close the sweep after applicable coverage and
exceptions are verified. Start Phase 2 after the planned sweeps have run and
the remaining blockers and decision requests are recorded. Keep file markers
during sweeps and leave crate moves and unrelated refactors for Phase 2.

1. **Production `unwrap()` and `unwrap_err()`.** Start here. Use stock Clippy
   to find the calls. Propagate recoverable errors, handle ordinary absence,
   and replace check-then-unwrap with pattern matching. Review invariants and
   keep the test allowance. [Unwrap rollout](#clippy-rollout-for-unwrap_used)
2. **Existing safety and hygiene rules.** Finish coverage for
   `undocumented_unsafe_blocks`, `unsafe_op_in_unsafe_fn`,
   `allow_attributes_without_reason`, `dbg_macro`, and `todo`. Audit each
   separately and retain the justified vendor and platform exceptions.
   [Blocking rules](#local-blocking-rules) · [Compiler settings](#rust-compiler-settings-from-upstream)
3. **Other reachable panics.** Review `expect`, panics in `Result` functions,
   fallible indexing and arithmetic, and assertions with unclear diagnostics.
   Use the existing error or recovery contract for each path.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites)
4. **Lossy conversions.** Review silent truncation, sign loss, and other casts
   where range or rounding matters. Verify the boundary values and ask before
   changing an established clamp, rounding, or rejection policy.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites)
5. **Mechanical cleanup.** Use the existing Sensei entry points for pinned
   rustfmt, machine-applicable Clippy fixes, and checked rust-analyzer import
   assists. Keep formatting, compiler fixes, and import assists reviewable
   separately. [Automatic cleanup](#automatic-cleanup-with-sensei)
6. **Repeated ownership issues.** Review redundant clones, `unnecessary_to_owned`
   findings, and shared buffers. Retain intentional snapshots and ownership
   boundaries. Treat `clone_on_ref_ptr` as optional clarity guidance.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites)
7. **Repeated API issues.** Inventory ambiguous boolean parameters and fields,
   unnecessarily owned inputs, excessive visibility, and public types missing
   relevant traits. Change compatible callers together. Defer public or wire
   contract changes that require a decision.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites)
8. **Repeated idiom and diagnostics issues.** Review string concatenation,
   wildcard imports, and library stdout and stderr. Preserve Bevy and test
   imports, intentional CLI output, and Cargo build directives.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites)
9. **Lint exceptions and documentation.** Review stale suppressions and their
   reasons. Check `# Errors`, `# Panics`, and public documentation where they
   explain affected contracts. Keep the audit and hold decisions below.
   [Audit rules](#clippy-audits-and-settings-with-prerequisites) · [Compiler settings](#rust-compiler-settings-from-upstream)

### BeautifulRust rules and enforcement

Use the pinned [writing skill][writing], [review skill][review], [lint levels][levels],
[Clippy settings][settings], and [Bevy reference][bevy] at
`b9718436d77ee7bcf17cdcb6aa27628feb95d97a`. The tables below govern changes to
shared lint enforcement. Apply the per-file checklist during the refactor
regardless of which lints currently enforce it.

### Shared lint policy

Choose the enforcement for each rule in this order.

1. Use a built-in rustc lint when it covers the rule and should run during an
   ordinary build. Set shared levels in `[workspace.lints.rust]`.
2. Use stock Clippy for the remaining rules it covers. Set levels in
   `[workspace.lints.clippy]` and use `clippy.toml` for options and named API
   bans through `disallowed_methods`, `disallowed_types`, and
   `disallowed_macros`. Inherit both tables with `[lints] workspace = true`.
   Give standalone and vendor manifests equivalent coverage with only the
   exceptions they need.
3. Use Sensei's explicit cleanup command to coordinate standard tools and
   keep custom build rules for Lince-specific policies. Leave the no-comments
   build rule disabled. Use compiler-backed checks for type and method
   resolution. Keep judgment calls in review.

[Cargo lint inheritance][cargo-lints] · [Clippy configuration][clippy-config]

A rule leaves the pending list after codebase-wide adoption and verification.
Audit workspace and standalone crates, including firmware, plugins, build
scripts, tests, examples, and vendored or generated sources. Check applicable
targets and features locally. Record cases that would change behavior for
the owner to decide. Unverified coverage stays pending unless the owner
accepts a documented exception.

### Automatic cleanup with Sensei

Use `cargo run -p sensei -- fix --preview` to list proposed edits and their
non-whitespace character and nonblank line counts. Use `fix` to apply them.
Sensei sends one file at a time to the pinned `rustfmt --edition 2024` and
reads the formatted source from stdout. The edition is the only argument and
matches the first-party packages. Use rustfmt's default style, with no
Sensei-specific formatting configuration. Separate import declarations keep
their own statements. Write nested imports where they improve the file and
review field initializer shorthand during the refactor. The inventory includes
standalone Rust sources and skips vendored sources, files marked as generated,
and the byte-sensitive paths recorded below.
`--manifest-path` narrows the pass to the package directory.

Use `cargo run -p sensei -- fix --compiler-only --workspace` to apply Clippy's
machine-applicable rewrites for `unnecessary_map_or`, `option_as_ref_deref`,
`useless_asref`, `iter_cloned_collect`, and `iter_count` without reformatting.
Use `--manifest-path PATH` for one package. Each package uses its host targets
and default features.
Workspace mode queues first-party packages and standalone manifests independently
and continues if a package fails.

Use `--compiler` for the broader cleanup with rustfmt. This also applies unused
imports, braces and parentheses, field-pattern shorthand, redundant semicolons
and closures, needless borrows and lifetimes, useless conversions, format
captures, collapsible conditions, boolean `matches!` expressions, identity
maps, and direct returns.

Sensei skips overlapping edits and suggestions that touch comments. A failed
baseline check skips the package. After writing edits, Sensei rechecks the
selected targets with the package's ordinary lint levels and restores its
edits if that check fails.

Each applied compiler pass regenerates [problems.md](problems.md) for its selected
packages. It lists remaining Clippy findings by file, position, and lint, including
`unwrap_used` and `unnecessary_to_owned`. Package failures are marked as incomplete
coverage. `--preview` reports proposed changes without writing source or the
report. Alternate feature and target combinations still need review.

Add `--no-comments` to remove ordinary comments. Keep Rustdoc, safety proofs,
attribution, formatter directives, and `// non-lean-yet`. The build hook
continues to check its registered rules. Use `sensei paths PATTERN` for a
read-only rust-analyzer search. Use `fix --shorten-paths --workspace --preview`
with the pinned rust-analyzer and `rust-src` components to preview the import
assist across first-party workspace and standalone Cargo projects. Sensei uses
rust-analyzer's syntax trees to find qualified calls, including `crate::` calls,
and checks that rewritten calls and existing references of the imported name
still resolve to the same definitions. It can add an import for a one-off call.
Calls guarded by conditional attributes keep their qualified paths.
Use `--manifest-path PATH` to scope the pass to one package.

Extend the automatic set when the tools provide a mechanical fix that preserves
behavior across relevant configurations. Error and panic policy, numeric
conversions that can lose information, ownership and lifetime design, public
API changes, and safety proofs need human review.

### Changing a shared lint

`cargo build` and `cargo check` enforce rustc lints and build scripts.
[`cargo clippy`][usage] also enforces Clippy lints. All run without an agent.
Use `deny` for blocking rules with scoped overrides. Inheriting crates already
deny warnings, so use `--force-warn` for the selected lint's nonblocking audit.
[Workspace manifest](../../../Cargo.toml) · [Toolchain](../../../rust-toolchain.toml)

1. **Baseline one package.** Record the revision, toolchain, targets, features,
   ordinary compile result, behavior checks, and lint findings. Consult the
   applicable [Lince Records](../Lince.lingua) and coordinate ongoing edits.
2. **Audit and fix.** Prioritize reachable panics and bad conversions. Fix
   recoverable failures through propagation, a tested fallback, or a branch.
   Keep proven invariants and narrow exceptions. Surface any behavioral
   tradeoff for the owner before enabling the rule.
3. **Deny and verify.** On the same targets and features, prove a violation
   fails and a justified exception passes. Run the ordinary compile command and
   applicable behavior tests. Record coverage gaps. Roll back a failing policy
   switch if needed and retain independently verified fixes.
4. **Expand coverage.** Repeat until codebase-wide completion is verified.

Count eliminated panic paths separately from diagnostic improvements.

### Local blocking rules

| Point | Status | Remaining verification |
| --- | --- | --- |
| `dbg_macro` | Denied in all 48 packages. Smithay keeps its intentional demo and documentation output. | Complete feature and device checks. |
| `todo` | Denied in all 48 packages. Scoped exceptions preserve Gemm’s unfinished SIMD methods and Smithay’s existing fixtures and two runtime branches. | Complete feature and device checks. |
| `allow_attributes_without_reason` | Denied in all 48 packages. Added 57 reasons. Clippy skips Smithay at its Rust 1.80.1 minimum. | Complete affected feature and device checks. |
| `undocumented_unsafe_blocks` | Denied in 42 packages. Six vendored packages in Bevy, Smithay, and Gemm keep warnings. | Finish additional feature and platform checks. |
| `unwrap_used` | Stock Clippy denies it in Ailuros Protocol and firmware. | Finish the rollout before setting the workspace default to `deny`. |

The first three rules passed default-feature Linux Clippy across all Cargo
targets. Fixtures verified blocking and exceptions. Bevy UI passed 202 tests.

#### Clippy rollout for `unwrap_used`

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

#### Safety comments and custom lints

Put a nonempty `// SAFETY:` explanation immediately above the unsafe block
or impl. Clippy checks for the local explanation. Review whether the code
actually meets the safety conditions. Keep these comments and attribution
when removing the refactor marker and temporary notes.

Stock Clippy accepts configuration for its built-in lints. New generally
useful checks can be proposed upstream. Dylint can run private compiler-backed
lints with scoped Rust lint attributes, but requires a separate command and a
pinned nightly. Keep it out of this rollout. Move any future Sensei rule to
stock tools when they cover its behavior.
[Sensei build hook](../../../crates/sensei/build.rs) · [Adding Clippy lints][clippy-add-lints] · [Dylint][dylint]

The safety rule passed on default Linux, both Android ABIs, ESP32-S3, and all
features of the four vendored Bevy crates. Smithay, Gemm, and Parley's library
passed. Parley tests need `parley_dev`. Mobile preview passed 33 of 34 tests.
Its original system ordering creates a cycle. Store's snapshot test timed out.
[Sensei rule](../../../crates/sensei/src/rules/no_comments.rs) · [Missing safety comments][safety-lint] · [Unnecessary safety comments][extra-safety-lint]

### Clippy audits and settings with prerequisites

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
| `missing_errors_doc` | Write actionable error contracts for high-value library boundaries when comments are allowed. | Check generated rustdoc and its links. Compare failure cases with the documented categories. Keep workspace-wide enforcement on hold. |
| `missing_panics_doc` | Identify remaining reachable panics and distinguish technical invariants from ordinary caller failures. | Verify the published panic conditions and any fallible alternative. Keep workspace-wide enforcement on hold. |
| `multiple_unsafe_ops_per_block` | Review whether several FFI or pointer operations implement one invariant. Split only when it clarifies obligations. | Check that ordering, lifetime, and allocation/free pairing are unchanged. Use the platform's applicable checks. Keep this an audit. |
| `all` and `pedantic` | Leave ordinary Clippy defaults available. Sample pedantic findings on a small package first and select useful checks individually. | Report findings by lint and scope. Keep blanket pedantic enforcement off, even if many warnings have simple fixes. |
| `module_name_repetitions` | Keep upstream's allow choice. Read names in their actual API context. | Avoid broad renames. Compile callers if a specific rename has a clear benefit. |
| `must_use_candidate` | Keep allowed. Add `#[must_use]` only when ignoring the value is meaningfully wrong. | Check purposeful callers and the compiler diagnostic for accidental discard. |
| `module_inception` | Keep allowed. Review confusing paths individually. | Preserve imports and re-exports if a targeted move is justified. |
| `enum_glob_use` | Keep allowed. A local enum-variant import can clarify a match. | Check the resulting match stays unambiguous and exhaustive where required. |

### Clippy config from upstream

| Setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `allow-expect-in-tests = true` | Hold while `expect_used` is allowed. | If a package later audits `expect_used`, verify the intended test behavior in that package before adding the option. |
| `allow-print-in-tests = true` | Hold until a selected package enables either print lint. | Verify fixture output remains available while unexpected library output is flagged. |
| `max-fn-params-bools = 0` | Use for an opt-in API audit so even one eligible boolean becomes visible. Classify intent and caller count. | Confirm a named mode or clear pair of operations is an improvement at the affected call sites. Keep the threshold out of the default pilot. |

### Rust compiler settings from upstream

| Setting | Before building or enabling | After building or decision to defer |
| --- | --- | --- |
| `unsafe_op_in_unsafe_fn` | Retain existing enforcement. Rust 2024 already warns about unsafe operations in an unsafe function that lack an explicit block, and Lince denies warnings. | Verify any changed unsafe function with the normal build. Scope new blocks tightly and review their proofs. No new compiler lint setting is needed. |
| `missing_debug_implementations` | Audit at a deliberate API boundary. Identify secrets, handles, and expensive fields before considering `derive(Debug)`. | Verify useful, redacted output and avoid a workspace setting that would turn unrelated missing implementations into errors. |
| `missing_docs` | **Hold** as a broad compiler lint. Document API contracts during the refactor. Reuse external Markdown with `#[doc = include_str!(...)]` where it serves callers. | Build rustdoc for the affected package and inspect links and exposed items. Require a clean target baseline before considering enforcement. |
| `unreachable_pub` | Audit one library. Identify re-export paths, cross-crate callers, and feature-dependent visibility. | Compile consumers and the relevant feature variants after narrowing visibility. Keep ordinary builds on the current compiler policy while this is an audit. |
| `rust_2018_idioms` | Review individually useful diagnostics. The group includes `elided_lifetimes_in_paths`, an explicit exception in the upstream Bevy reference. | Avoid a blanket group switch. Where warranted, check a specific lint on one package and verify the framework signatures still work. |

## Phase 2. Crate-by-crate refactoring

After the shared sweeps, start with the crates most likely to benefit from
clearer boundaries. Paths below are relative to `crates/`. Coordinate
overlapping work and keep related callers in the same verification scope.

1. **Core behavior.** `engine`, then `store` and `nucleus`. Start with action
   dispatch and wire boundaries, then persistence and domain logic.
2. **Desktop and presentation.** `interface`, `host`, `presentation-models`,
   `park`, and `park-clients`. Review shared host and presentation contracts
   before moving their consumers. Coordinate Park correctness and performance
   work with the same scope.
3. **Runtime, transport, and integration.** `transport`, `cell`, `fiote`,
   `plugin`, `facade`, `lince`, `mobile`, `map`, and `hand-input`.
4. **Domain and feature crates.** `karma`, `simulation`, `sandbox`, `geist`,
   `document`, `editor`, `media`, `resources`, `practice`, `i18n`, and
   `ithkuil`.
5. **Infrastructure and tooling.** `utils`, `lingua`, `sensei`, `xtask`, and
   `wgsl`. Include standalone repository Rust scripts in their owning area.
6. **Device and vendored sources.** `ailuros`, `ailuros/protocol`, its
   standalone firmware, and every `vendor/` package. Preserve upstream
   provenance and generated or byte-sensitive contracts.

Reconcile `protein`, `protein-authority`, and `instrumentation` against their
recorded checkpoints and any newly pending paths. Reconcile the queue with
every workspace and standalone manifest and the Rust source inventory before
closing Phase 2. [Workspace members](../../../Cargo.toml) · [Protein checkpoint](#protein-demonstration-checkpoint) · [Instrumentation checkpoint](#shared-instrumentation-boundary)

### One complete refactor per file

1. **Trace the file and its callers.** Within the selected crate, start with
   the files that define its boundaries. Read the whole file, trace callers
   and enabled features, and identify the behavior to preserve. Include any
   unmarked pending file from the inventory in the same workflow.
   [Lince Records](../Lince.lingua)
2. **Restructure and refactor in the same change.** Consider the containing
   crate first, then the file and its neighbors. Make worthwhile splits,
   moves, renames, and merges as you reach them and refactor the affected code
   in place or in its new home. Update callers, manifests, features, and
   re-exports with the move. Complete the checklist for the entire file and
   all of its successors before taking the next item from the queue. A
   neighboring file moved only to wire up a split carries its pending status
   to its new location.
3. **Verify and close each file.** Inspect the formatted diff and use focused
   local checks for the affected entry points and contracts. Remove the marker
   when every applicable check has been addressed and the affected behavior
   verified. Close a file that already satisfies the checklist as soon as its
   review is complete. If work is interrupted or a decision blocks completion,
   keep the marker on the unfinished file or carry it to an unfinished successor.
   Preserve the current behavior while a decision is pending. Record the
   affected path, rule, and consequence of changing it in this plan, then
   continue with independent work. Reopen a completed file only when a later
   change needs it.

`// non-lean-yet` is the only tracking comment. Skip a preliminary annotation
pass, per-file rule lists, and audit-only purpose headers. Keep useful Rustdoc,
invariant explanations, and safety proofs with the code.

Use `git ls-files --cached --others --exclude-standard -z -- '*.rs'` to reconcile
the inventory and `git grep --untracked -nE '^// non-lean-yet[[:space:]]*$' -- '*.rs'`
for the pending source queue. At closeout, reconcile the reviewed paths with
the inventory, including successors and the existing byte-sensitive exceptions
tracked below. A missing marker alone cannot establish that a file was reviewed.
Review generated code at its generator and confirm the output. Preserve
upstream provenance, license notices, and the shebang position.

### Guidelines to check in every file

Use the full pinned writing and review rules as well as the Bevy guidance where
it applies. Fix every applicable issue that can be resolved while preserving
Lince's contracts. Prioritize correctness and soundness, followed by error
handling, API design, idiom, and polish. Use the review skill to identify the
rule and concrete fix during the edit. Treat the list below as a checklist
for every file, including rules with no enabled lint. Check the package's
edition, minimum Rust version, features, and framework version before using
newer syntax or APIs. Favor concrete code in applications and deliberate API
contracts in libraries.
[BeautifulRust writing rules][writing] · [Review rules][review] · [Bevy guidance][bevy]

- **Behavior and contracts.** Preserve valid inputs, outputs, error and recovery
  behavior, ordering, precision, retry and idempotency, permissions, privacy,
  accessibility, and compatibility with persisted data and wire formats. Keep
  invariants enforceable through types and branch structure. Exercise affected
  native UI features through the existing Laboratory cases where applicable.
- **Line-by-line leanness.** Use field shorthand, format captures, useful type
  inference, direct function references, and existing standard-library methods.
  Combine compatible conditions, use guard clauses or `let else` when helpful,
  and choose the clearer loop, iterator, or combinator. Check for needless
  mutable bindings, eager defaults, conversions, and forwarding closures.
  Return expressions directly when an intermediate adds no meaning. Remove
  dead code and pass-through helpers when their callers gain clarity. Use
  lazy fallback where appropriate and preserve evaluation and drop order.
  Use `write!` or `push_str` where an existing buffer can be reused. Keep
  descriptive names, useful intermediate values, explicit types and branches
  where they explain intent, and blank lines that separate steps.
- **Imports, names, and diagnostics.** Group imports with shared roots, for
  example `use std::{env::args_os, process::exit};`. Use direct calls such as
  `args_os()` when the shorter spelling remains clear and rustfmt keeps the
  imports readable. This is Lince's import style. BeautifulRust generally
  qualifies free functions by their module. Keep qualifiers that convey origin
  or disambiguate. Keep glob imports for established framework, test, and local
  enum-match cases.
  Re-export public names explicitly. Check accessor, constructor, iteration,
  and error type names. Use `as_`, `to_`, and `into_` to express conversion
  cost and ownership. Name constants that encode protocol values, limits, or
  tuning decisions. Remove stray `dbg!` and resolve reachable `todo!` paths.
  Keep deliberate CLI and build output and use structured logging for
  operational diagnostics.
- **Ownership and lifetimes.** Use `&str`, `&Path`, slices, and other narrow
  references for read-only inputs. Take ownership when storing or consuming a
  value and move it when ownership transfers. Remove avoidable clones and
  allocations. Keep required snapshots and deliberate shared ownership. Check
  the scope of mutable borrows, `Arc`, interior mutability, and drop guards.
  Shorten borrows or move values with `take` or `replace` before adding a
  clone. Review returned values when mutable output parameters just collect
  results. Keep borrowed fields when their lifetime is part of the design.
  Keep ownership wrappers behind API boundaries where callers can use plain
  values or references. Keep small `Copy` values and Bevy system parameters by
  value where appropriate.
- **Types, state, and constructors.** Use `Option` for absence and
  data-carrying enums for mutually exclusive states. Prefer named modes over
  ambiguous public booleans, validated newtypes over interchangeable primitives,
  and named fields for nontrivial data. Distinguish independent flags from
  exclusive modes. Choose standard types for paths, durations, and bounds.
  Validate at construction, keep `new` and `Default` consistent, and use a
  builder only where configuration or cross-field validation warrants it.
- **APIs and traits.** Keep visibility as narrow as callers allow. Use concrete
  types by default, especially in application code. Add generics or traits
  where they serve real callers. Put bounds where they're used and choose
  closure bounds by how the closure is called. Put operations with a clear
  receiver on that type. Review object safety, re-exports, and third-party
  types at public boundaries. Consider sealed traits and `#[non_exhaustive]`
  for new APIs that will evolve. Preserve existing callers' construction and
  match patterns.
  Derive standard traits where their contracts fit, redact secrets in `Debug`,
  and make `Eq`, `Ord`, and `Hash` agree. Use `From`, `TryFrom`, `FromStr`, and
  iteration traits. Pair `len` with `is_empty`. Keep `Deref` and operator
  overloads aligned with their standard semantics. Return an iterator where a
  library only collects to give callers something to iterate. Add `#[must_use]`
  where discarding a result is a mistake.
- **Errors, panics, and numbers.** Use `Result` and `?` for recoverable failures
  and preserve causes, context, and public diagnostics. Give library errors
  actionable types that implement `std::error::Error`, expose causal chains,
  and meet the required `Send`, `Sync`, and `'static` contracts. Remove
  production `unwrap()` where the error can be handled or propagated. Replace
  check-then-unwrap with pattern matching. Review `expect`, assertions,
  indexing, arithmetic, and casts against reachable inputs. Choose a typed
  error or a contract-preserving fallback for expected failures. State the
  proven invariant for retained panics and report relevant values where safe.
  Use checked conversion or arithmetic where range matters. Keep deliberate
  recovery boundaries described in the Lince-specific references below.
- **Async, resources, and Bevy.** Review lock scope, awaits, task ownership,
  cancellation, queue bounds, blocking work, and cleanup on failure. Check
  `Send` and `Sync` expectations at thread and task boundaries. Release guards
  before `.await` and user callbacks. Use `async fn` when setup can run inside
  the future. Give spawned work an owner and provide explicit fallible
  shutdown where needed. Review destructor work and lock poisoning against
  the existing recovery policy.
  In Bevy check system and message ordering, disjoint queries and resource
  borrows, change detection, deferred commands, query cardinality, and
  handling of missing or despawned entities. Preserve system-parameter and
  prelude conventions. Keep passive component data simple and wrap fields
  where they must enforce a shared invariant.
- **Unsafe and FFI.** Keep unsafe operations behind the smallest sound boundary.
  Check pointer validity, layout, lifetimes, aliasing, thread-safety, and resource
  ownership. Keep an adjacent `// SAFETY:` proof and document caller obligations
  in `# Safety` sections. Check ABI layout, null handling, paired allocation and
  release, and unwinding across FFI. Scrutinize `transmute`, manual `Send` and
  `Sync` impls, and safe APIs that pass caller values into unsafe operations.
  Prefer safe wrappers and preserve upstream proofs. Use Miri or focused
  boundary tests when supported and relevant.
- **Algorithms, allocations, and macros.** Remove repeated work, extra
  traversals, temporary collections, serialization, and avoidable copies.
  Review data structures, ordering, batching, and allocation or lock costs.
  Check for hash-map iteration leaking into deterministic output, and use
  buffer reuse or capacity estimates where they pay off.
  Reuse an existing abstraction if it makes the change simpler. Prefer
  ordinary functions and types for shared logic. A macro must earn its place
  in the expanded code as well as the handwritten code.
- **File and crate boundaries.** Give each file a clear responsibility. Split
  unrelated responsibilities into named files or modules and give independent
  capabilities their own crate when the boundary pays for itself. Replace names
  such as `part_7.rs` with domain names. Group siblings by use case and keep
  directory depth proportionate to the navigation it saves. Keep module roots
  easy to scan and merge wrappers that add indirection without a purpose.
  Check dependencies, feature wiring, entry points, and ownership after a move.
  Carry through existing plugin crate organization and naming conventions.
  Keep dependency direction clear, share workspace versions and lint settings,
  and review the cost of build scripts, proc macros, and new dependencies.
  Check dependency duplication and the feature graph when moving a crate
  boundary. Preserve additive features and deliberate compatibility pins,
  including `iroh` and `loro`.
- **Docs, tests, formatting, and lints.** Document public contracts and the
  reasons for non-obvious invariants, including applicable error, panic, and
  safety sections. Use `?` in fallible rustdoc examples. Check changed public
  docs and links with rustdoc. Use integration tests for public contracts and
  unit tests for private invariants. Test observable behavior and failure
  paths with clear assertions. Control time, I/O, and randomness where tests
  depend on them. Add regression coverage where existing tests would miss an
  affected contract. Format the affected code with rustfmt and run the relevant
  local rustc, Clippy, and behavior checks for the affected features and
  platforms. Prefer a narrowly scoped `#[expect(..., reason = "...")]` for a
  known lint exception where supported. Keep Lince's existing vendor and
  platform accommodations.

## Measure completed work in batches

For equally clear and safe idioms, prefer fewer non-whitespace characters in
formatted source first, then fewer nonblank lines. Fewer macro-expanded lines
are also a workspace goal. Choose clearer or safer code when it takes more
space. Keep useful names, documentation, tests, and blank lines. Count new
support code and all successors of moved, split, or merged files in the same
comparison.

- Format baseline and candidate with the same pinned rustfmt and configuration.
  Count each tracked file once. Exclude empty or whitespace-only lines and
  temporary refactor markers. Report source LOC and character deltas
  for the workspace and affected crates.
- Compare macro-expanded nonblank lines for the same target and feature
  configurations with the same local inspection toolchain. Carry coverage
  across crate splits and record unsupported expansions.
  [Compiler macro expansion][macro-expansion]
- Measure compile time separately when evaluating a batch of dependency,
  macro, or crate changes. Compare like-for-like clean and representative
  incremental builds. File closeout is based on the code and its behavior.
  Measure runtime or allocation changes on relevant workloads when claiming
  an improvement. Record behavior verification gaps and the tradeoffs that
  justify fuller code.
  [Build Record](Build.lingua) · [Cargo timing reports][build-timings]

## Lince-specific review references

Use these references when a change reaches the corresponding boundary.

- Keep CRDT import failure isolation and diagnostics lock recovery as deliberate
  boundaries. Preserve causal error information and caller-visible diagnostics
  when simplifying errors.
  [Engine errors](../../../crates/engine/src/error.rs) · [CRDT guard](../../../crates/engine/src/collab_guard.rs) · [Diagnostics](../../../crates/utils/src/diagnostics.rs)
- Preserve readable and selectable Protein text, the required Protein-first
  Laboratory case, and edit-panel geometry for affected UI changes.
  [Lince requirements](../Lince.lingua)
- Use representative Park workloads when making claims about desktop runtime.
  [Park plan](park/Park.lingua)

### Protein demonstration checkpoint

`protein` owns Store-backed query execution. `protein-authority` owns the
query AST and wire format in `query` and the in-memory evaluator in
`authority`. The combined crate builds and runs the wire and authorization
suites without Store or Engine. The caller inventory found that only these
two crates consumed the former query crate directly. All other workspace
consumers used `protein`, and there was no measured build advantage from
the extra boundary. The public `protein` paths, including
`protein::authority`, continue to re-export the same API.

| Measure | Three crates | Two crates |
| --- | ---: | ---: |
| Formatted nonblank Rust lines | 30,442 | 30,444 |
| Formatted non-whitespace Rust characters | 752,993 | 753,494 |
| Nonblank macro-expanded library lines | 34,157 | 34,273 |
| Nonblank macro-expanded lines in comparable test targets | 16,991 | 16,993 |
| Nonblank macro-expanded build-script lines | 21 | 14 |

Source counts include all affected Rust files, including moved tests and build
scripts. The expansion comparison includes every library and build script and
all moved tests. It excludes ten unchanged Protein test targets that require
Engine. The former monolith at `0f564d91` had 29,142 nonblank Rust lines,
706,714 non-whitespace characters, and 32,688 expanded library lines.

The archived Engine comparison gives the same outcomes for all 49 tests in
Sources, Timeline, Schedule, Ledger, Entries and Rules, and Expression. Three
cases fail on both sides, covering applied recurrence in Timeline, applied
recurrence in the Entries inbox, and the decision queue. Current Transfer row
comparisons use the current Protein and Store code and show byte-identical
results across four cases. Three Store-backed Record comparisons and four
Protein unit tests also pass. All 22 checked public schemas and the query and
role-policy wire examples match byte for byte across the consolidation.
The normal Protein library build and library Clippy checks pass. The
Engine-dependent integration suite remains unverified because of existing
Engine `Send` errors.
[Engine dispatch](../../../crates/engine/src/actions/dispatch.rs)

### Interface host boundary

`lince-host` owns the desktop host contracts, presentation geometry, capture
and input settings, bounded queues, and performance reporting. Its default
build and tests run without Bevy or Engine. The host-only normal dependency
graph has 84 packages including its root. The Interface graph with defaults
disabled has 201, including the presentation resources and practice content.
Interface continues to expose the same `lince_interface::host` paths and
forwards the wake-timer, Bevy, behavior check, and Engine-reporting features.
Park continues to compile against that public path in its server build.

Separate modules hold desktop requests, the host bridge, canvas geometry,
output viewports, and the timing and counter catalogs. The focused host suites
pass 65 default tests and 70 tests with UI and behavior checks. Interface's
UI library builds, and its five activation tests pass through the re-export.
Host Clippy checks pass with dependency linting disabled. The Engine-reporting
feature remains blocked by the existing Engine `Send` errors.

Across the moved Rust sources and the Interface crate root, default rustfmt
gives 8,923 to 8,853 nonblank lines and 287,116 to 281,830 non-whitespace
characters, excluding the queue marker. Excluding the old audit headers as
well, the comparison is 8,777 to 8,851 lines and 278,767 to 281,695 characters.
The comparison includes the new module boundaries and contract documentation.
The catalog-only compiler expansion falls from 14,568 to 12,194 nonblank
lines in the same comparison harness. All 2,079 catalog entries retain their
names, order, and labels. Timing entries also retain byte-identical Rust debug
and JSON output.

Keep the markers on moved files whose complete review is pending. The output
viewport still needs a decision about physical dimensions above `i32::MAX`
before replacing its existing signed casts with a different rejection or
clamping policy.

### Engine action dispatch boundaries

The first two mixed numbered dispatch files now route 85 actions to ten
modules named for their work. Mailbox operations that were spread across both
files now share a module. The dispatcher continues to select exactly one
handler and retains the existing boxed future and continue or break outcomes.
A comparison after default rustfmt matched every relocated action body and
every destination against the corrected baseline. The affected dispatch
source went from 2,690 to 2,821 nonblank lines and from 65,633 to 67,777
non-whitespace characters, excluding markers. These counts include the old
audit headers on the baseline side.

The baseline correction places legacy directory-overlap validation on
`SetExtension`, after the local-owner check. It had referenced the extension
inputs inside contact delivery, where they were out of scope. The new
regression case exercises an overlapping named sync and asserts that the
legacy extension stays unset. Engine's existing async `Send` errors still
block runtime verification. The corrected baseline emits 780 `Send`
diagnostics and the split emits 862. All 336 diagnostics outside the
dispatcher match by location and reported type. The additional diagnostics
are in dispatch code. The affected files retain their markers.
[Dispatcher](../../../crates/engine/src/actions/dispatch.rs) · [File sync tests](../../../crates/engine/tests/file_sync.rs)

A fresh ordinary Engine library check reproduced 862 `Send` diagnostics.
Standalone compiler probes accepted the Store extension and projection futures
named in early diagnostics, including a projection wrapped in
`Execution::scope`. A separate experimental trait-solver invocation
reported 54 trait-checking overflows. These experiments establish the
failure of the composed Engine futures and leave the cause unresolved.
The experiments used the pinned compiler and left its stable build
configuration in place.

The seven remaining numbered files now route through modules named for
Records, workspace components, Fiote, Karma, Transfers, messaging, access,
signals, vocabulary, and saved Protein queries. The root selects the handler
in one exhaustive match, with one boxed future per action. A source comparison
matched all 165 affected action bodies, all 258 dispatcher alternatives, and each
of the 23 former early branches against the previous routing and bodies.

Across the affected dispatcher files, default rustfmt gives 8,078 to 8,342
nonblank lines and 182,118 to 194,429 non-whitespace characters after excluding
temporary audit annotations. Explicit imports and narrower handler functions
add source. The pinned compiler's default-feature expanded Engine output
goes from 274,779 to 275,224 nonblank lines. The library check reports 809
dispatch `Send` diagnostics versus 526 previously. All 336 diagnostics outside
dispatch match the previous locations and types. Runtime verification remains
blocked and the incomplete files retain their markers.
[Dispatcher](../../../crates/engine/src/actions/dispatch.rs)

### Engine Wire module boundaries

The Wire façade now exposes the existing `engine::wire` paths through explicit
re-exports. Ten modules hold the protocol payloads, endpoint setup, inbound
connection handling, request dispatch, peer workflows, replication, mailbox
client, outbound exchange, Cell transport adapter, and socket-level tests.
The payloads still depend on Engine and Store types. Keeping them in Engine
avoids creating a crate that would share those dependencies or duplicate types.

The source comparison found the same 176 items. All protocol declarations,
Serde attributes, method bodies, and test cases matched after accounting
for the documented port-setting rewrite, local visibility, imports, and
formatting. The port probe compared all `u16` inputs and representative
invalid settings. All 65,551 cases returned the same result and error text.
The existing execution identity hash still incorporates Engine source paths
and contents, so the relocation changes the build identity as designed.

The ordinary Engine library check reports the same 1,142 async `Send`
diagnostics before and after the split. All 1,038 outside Wire match their
original locations and types. The 104 inside Wire match after accounting
for the moved protocol type path. The test-target check caught an internal
visibility change in configuration validation. Restoring access within Wire
resolved it. The recheck ended with `Send` diagnostics for the library and
test build. The affected files retain their markers while the build failure
prevents running the socket tests.

Default formatting reduced the façade from 4,494 to 116 nonblank lines.
Across the moved sources and the updated social test import, the formatted
total excluding legacy audit annotations went from 4,655 to 4,788 nonblank
lines and from 110,492 to 114,212 non-whitespace characters. Engine's
default-feature macro expansion went from 275,224 to 275,449 nonblank lines.

[Wire façade](../../../crates/engine/src/wire.rs) · [Protocol payloads](../../../crates/engine/src/wire/protocol.rs) · [Connection handling](../../../crates/engine/src/wire/inbound.rs)

### Store occurrence disputes

Occurrence dispute history, replay, and transactional writes live together in
`transfers/disputes.rs` and keep their existing `store::transfers` public paths.
One SQL statement now selects and writes the effective dispute. It covers a
participant assertion, the fallback to a system dispute, and a cleared
occurrence. The evidence, request guards, and transaction boundary stay the
same. The new Store test passes against SQLite and covers those states,
participant authorization, idempotent replay, and request key conflicts.
Rustdoc builds and exposes the existing import paths. The Clippy audit
matches all 53 findings from the baseline, with none in the new module or
test. Those baseline findings still block strict Store Clippy.

With temporary audit comments excluded, formatted production source goes
from 8,230 to 8,234 nonblank lines and 222,034 to 222,415 non-whitespace
characters. The regression test adds 109 nonblank lines and 3,842 characters.
Store's expanded library goes from 74,261 to 74,298 nonblank lines. The
dispute module and its test are complete. The parent Transfers module keeps
its marker for the remaining responsibilities.
[Dispute storage](../../../crates/store/src/transfers/disputes.rs) ·
[Store dispute test](../../../crates/store/tests/transfer_disputes.rs)

### Store occurrence claims and application formulas

Single claims and bulk completion share fact construction, event insertion,
and claim flag updates. The role selects a static SQL statement. Bulk
completion checks adjacent duplicates in its existing sorted review order
and borrows the previous fact hash when sealing the next fact. Application
formulas have their own storage module. The request and policy types omit
private formulas from `Debug`. Their public paths, stored formulas, and signed
evidence are preserved.

The three Store tests cover role checks, duplicate reviews, rollback on a
second signing failure, deterministic fact order, hash chaining, disputes,
and formula versioning, replay, and redaction. The claim and formula behavior
tests also pass with the pre-split Store source when the two new `Debug`
redaction assertions are excluded. A downstream compiler probe accepted all
three write futures as `Send`. Rustdoc exposes the existing
`store::transfers` paths. Clippy matches the 53 baseline findings, with none
in the new files.

Formatted production source across the affected modules goes from 7,985
to 8,068 nonblank lines and 215,272 to 218,536 non-whitespace characters.
Store's expanded library goes from 74,298 to 74,390 nonblank lines. The new
tests and shared fixture add 208 nonblank lines and 6,374 characters to
the test scope. The three extracted files are complete. The parent Transfers
module still carries its marker.
[Claims](../../../crates/store/src/transfers/occurrence_claims.rs) ·
[Bulk claims](../../../crates/store/src/transfers/occurrence_claims/bulk.rs) ·
[Application formulas](../../../crates/store/src/transfers/application_formulas.rs) ·
[Workflow tests](../../../crates/store/tests/transfer_occurrence_workflows.rs)

### Store activation, occurrence reads, and request IDs

Activation owns the guarded transaction and signed event. Its path module
checks promises and participants and prepares the ordered snapshots.
Occurrence reads and shared request-key lookups have separate modules.
Callers still use the existing `store::transfers` paths.

Activation loads the authorized revision and evidence together, counts
Transfer parties once, and checks both participant counts in one query per
path. Replay matches each requested promise once and accepts a reordered
request with the same terms. The signed evidence takes ownership of the
prepared snapshots. Request-key collision checks retain their order and
continue to surface malformed evidence errors.

All four Store workflow tests passed. The activation case passed against
the previous Store source as well. It covers stale and unsigned revisions,
rollback after a rejected second path, opposite-promise links, read order,
and exact replay. The downstream compiler probe accepted the guarded
activation and read futures as `Send`. Rustdoc built, and the Clippy audit
matched the 53 baseline findings with none in the new files.

Across the parent and four extracted modules, formatted production source
goes from 7,126 to 7,201 nonblank lines and 191,696 to 193,992
non-whitespace characters. The parent drops to 6,262 nonblank lines.
Store's expanded library goes from 74,390 to 74,476 nonblank lines. The
activation test and fixture changes add 161 nonblank lines and 4,842
non-whitespace characters. The four modules are complete. The parent keeps
its marker.
[Activation](../../../crates/store/src/transfers/activation.rs) ·
[Path preparation](../../../crates/store/src/transfers/activation/paths.rs) ·
[Occurrence reads](../../../crates/store/src/transfers/occurrence_rows.rs) ·
[Request IDs](../../../crates/store/src/transfers/request_ids.rs)

### Store settlement and compensation

Settlement writes, progress reads, compensation, and first-completes source
groups live in separate modules behind the existing `store::transfers`
paths. One signing routine appends public settlement, private application,
compensation, winner, and sibling facts. It keeps the original hash chain,
authorization errors, and transaction. Group decisions and their ordered
fact IDs each take one query. The settlement input and slice omit private
formula text from `Debug`.

All five Store workflow tests passed. The new case covers partial settlement,
compensation that preserves the public quantity, rollback on a source-group
signature failure, settlement and compensation replay, and winner-first
fact order with two siblings. It also passed against the previous Store
source with the two new `Debug` assertions excluded. Rustdoc built and a
downstream probe accepted the settlement, compensation, and replay futures
as `Send`. Clippy matched the 53 baseline findings with none in these modules.

Across the parent and the four extracted modules, formatted production
source goes from 6,262 to 6,405 nonblank lines and 167,668 to 171,203
non-whitespace characters. The parent drops to 5,080 nonblank lines.
Store's expanded library goes from 74,476 to 74,625 nonblank lines. The
workflow test adds 162 nonblank lines and 5,531 non-whitespace characters.
The four modules are complete. The parent keeps its marker.
[Settlement](../../../crates/store/src/transfers/settlement.rs) ·
[Compensation](../../../crates/store/src/transfers/settlement/compensation.rs) ·
[Source groups](../../../crates/store/src/transfers/settlement/source_groups.rs) ·
[Workflow tests](../../../crates/store/tests/transfer_occurrence_workflows.rs)

### Store transfer invitations

Invitation reads, event history, evidence-backed transitions, and legacy
row-only writes live in four modules behind the existing `store::transfers`
paths. The transitions share pending-row lookup and commit preparation.
The common commit writes the event, updates the Transfer, reads the
invitation, and commits in the original order. Event status is derived from
the same event kind used in replay. The request ID module shares the
revision and invitation collision query, using one bind for the key.

Both invitation tests pass against the previous Store source and the
refactor. They cover request collisions, expiry, permission checks,
reopening, revision changes, historical replay, and the legacy row-only
contract. The old and new request queries also agree in 22 checks covering
the two request families and all ten participating tables. All seven Store
workflow tests passed. Rustdoc built, the downstream invitation futures
passed the `Send` probe, and Clippy matched all 53 baseline findings with
none in the invitation or request modules.

Across the parent, the request ID module, and the four extracted files,
formatted production source goes from 5,187 to 5,240 nonblank lines and
135,291 to 137,496 non-whitespace characters. Excluding Rustdoc, it drops
by 48 lines and 815 characters. The parent drops to 3,902 nonblank lines.
Store's expanded library goes from 74,625 to 74,774 nonblank lines. The two
invitation tests add 215 nonblank lines and 6,642 characters. The four new
modules are complete. The parent keeps its marker.
[Invitation reads](../../../crates/store/src/transfers/invitations.rs) ·
[Transitions](../../../crates/store/src/transfers/invitations/lifecycle.rs) ·
[Event history](../../../crates/store/src/transfers/invitations/events.rs) ·
[Legacy writes](../../../crates/store/src/transfers/invitations/legacy.rs) ·
[Invitation tests](../../../crates/store/tests/transfer_invitations.rs)

### Store drafts, revisions, and dependencies

Draft creation, revision evidence, snapshots, dependency handling, and
promise item terms have separate modules. Whole-draft editing has its own
promise reconciliation module. Callers keep the same public paths.
Revision writes share fact construction and insertion. Named revision
kinds and reviewed guards replace the positional flags, and internal
callers pass revision metadata together. Snapshots use the same dependency
reader as the Store API.

Single-promise revision now checks request collisions on the write
transaction's connection. It releases that transaction before resolving
replay evidence through the pool. The previous code timed out on a Store
with one connection. The new regression verifies both commit and replay.
The draft and revision behavior test passes on both versions for evidence,
signature validation, conflicts, and dependency rollback.

All nine Store workflow tests passed. Rustdoc built, the downstream
revision futures passed the `Send` probe, and the Clippy audit removed
four baseline findings, leaving 49 elsewhere. The eight extracted files
are complete. The parent and the unfinished callers keep their markers.

Across the affected production files, including the internal caller
updates, formatted source goes from 5,439 to 5,654 nonblank lines and
151,176 to 157,142 non-whitespace characters. The parent drops from
3,902 to 1,694 nonblank lines. Store's expanded library goes from 74,774
to 75,105 nonblank lines. The revision tests add 286 nonblank lines and
7,880 non-whitespace characters.
[Draft creation](../../../crates/store/src/transfers/drafts.rs) ·
[Revision evidence](../../../crates/store/src/transfers/revisions.rs) ·
[Dependencies](../../../crates/store/src/transfers/dependencies.rs) ·
[Revision tests](../../../crates/store/tests/transfer_revisions.rs)

### Store Transfer façade, OPEN claims, and agreements

The Transfer module now declares submodules and re-exports its existing
public API. Catalog reads, correction and successor lineage, OPEN claims,
agreement transitions, frozen coalitions, and readiness reads have
separate modules. They stay in Store because their queries and writes
share its schema, transactions, and signed evidence chain.

Catalog reads share a row decoder. Agreement transitions fetch the party,
current level, and Organ together. Coalition checks use SQL counts and
exact integer threshold arithmetic. OPEN claims share inserts between
the two roles and serialize the negotiated item once. Public-history
and publication reads move values out of their decoded inputs.

The two new regression tests pass on both the previous implementation
and the split. They cover both OPEN reuse modes, invitation admission,
claim replay, rejection of changed terms, agreement evidence rollback,
and frozen coalition membership. All eleven focused Store workflow tests
pass. Rustdoc resolves the existing public paths. A downstream compile
probe verifies that the moved futures still satisfy `Send`. The local
Clippy audit removes one baseline finding and leaves 48 elsewhere.
The broader Engine library check still hits the existing `Send`
generality errors.

Across the affected production files, formatted nonblank lines go from
1,811 to 2,021 and non-whitespace characters from 50,248 to 56,195.
Excluding Rustdoc, the candidate has 1,855 lines and 51,431 characters.
The parent goes from 1,694 to 142 nonblank lines. Store's expanded
library goes from 75,105 to 75,422 nonblank lines. The two tests add
289 nonblank lines and 8,092 characters. Keep the extra module imports
and API documentation for the clearer workflow boundaries.
[Transfer API](../../../crates/store/src/transfers.rs) ·
[OPEN claims](../../../crates/store/src/transfers/open_claims.rs) ·
[Agreement transitions](../../../crates/store/src/transfers/agreements.rs) ·
[Regression tests](../../../crates/store/tests/transfer_agreements.rs)

### Store Transfer delivery boundaries

The public module re-exports all 73 existing names. Origin policies and the
outbox have their own modules. Recipient references, pulls, and snapshots
have separate read and write paths. Application handoff, slice details,
execution, and attestations are grouped under application. The code stays
in Store because these workflows enforce invariants through its schema,
SQLite transactions, and evidence.

Policy writes and origin handoff transitions check request IDs on their
write connection. The previous implementation requested another connection
while holding the only one in an in-memory Store. Handoff creation now
checks replay inside the same transaction as the insert. Outbox reads share
row decoding and reuse the stored entry on exact retries. Nonce insertion
ignores conflicts on the existing sender-and-nonce key.

Policy and handoff regression tests time out on the baseline and pass after
the refactor. Envelope replay, retries, acknowledgement, and nonce tests
pass on both. All fifteen focused Store tests pass. Rustdoc resolves all
public paths and a downstream compile probe confirms representative futures
are `Send`. The Clippy audit goes from 48 to 45 findings with zero additions.

The parent goes from 2,904 to 54 formatted nonblank lines. Across all
delivery production files, formatted source goes from 2,904 to 3,109 nonblank
lines and 84,543 to 91,480 non-whitespace characters. Excluding Rustdoc,
the candidate has 3,005 lines and 88,248 characters. Store's expanded
library goes from 75,422 to 75,784 nonblank lines. The tests add 329
nonblank lines and 9,956 characters.

The two module entry points, policy, outbox, origin handoff, nonces, and
shared validation are reviewed. Nine relocated files keep their markers
for the full review of recipient delivery, remote commands, evidence, and
the remaining application workflows.
[Delivery API](../../../crates/store/src/transfer_delivery.rs) ·
[Policy](../../../crates/store/src/transfer_delivery/policy.rs) ·
[Origin handoff](../../../crates/store/src/transfer_delivery/application/handoff.rs) ·
[Delivery tests](../../../crates/store/tests/transfer_delivery.rs)

### Store recipient delivery and received outcomes

Recipient reference creation obtains the stored UID from its upsert and
preserves the existing identity and signed policy replay checks. Hosted and
replicated snapshots share the policy gate and materialization. Older
replicated envelopes still enter replica history without advancing the
current reference. Advancing envelopes serialize once for the evidence
writes. The outcome reader borrows stored values and parses the origin
timestamp once while preserving its authorization and freshness order.

Pull admission reads only the state and cursor it needs. The writing Cell
check, cursor read, and request insertion run in one write transaction.
Concurrent identical refreshes now return the same stored request. On the
baseline, the concurrent test fails the unique request-ID constraint. The
other four new tests pass on the baseline and the candidate. They cover
recipient policy replay, both snapshot modes and their evidence bytes,
refresh retry and cursor handling, and outcome authority and freshness.

All twenty focused Store tests pass. Rustdoc and a compile probe for the
recipient futures pass. Clippy goes from 45 to 41 findings. The existing
remote policy API retains its arguments with a scoped lint expectation.

Across the four reviewed production files, formatted nonblank lines go
from 763 to 831 and non-whitespace characters from 23,517 to 23,724.
Excluding Rustdoc, the files have 754 lines and 21,171 characters.
The snapshot module alone drops from 192 to 160 nonblank lines.
Store's expanded library goes from 75,784 to 75,851 nonblank lines.
The tests add 418 nonblank lines and 11,849 characters.

References, pulls, snapshots, and received outcomes are reviewed. Six
delivery files retain their markers for review.
[References](../../../crates/store/src/transfer_delivery/references.rs) ·
[Snapshots](../../../crates/store/src/transfer_delivery/snapshots.rs) ·
[Pulls](../../../crates/store/src/transfer_delivery/pulls.rs) ·
[Outcome reader](../../../crates/store/src/transfer_outcomes.rs) ·
[Recipient tests](../../../crates/store/tests/transfer_delivery_recipient.rs)

### Store remote command workflow and delivery evidence

Command persistence owns signed identity, Rule origin, replay, and Fact
linkage. Its queue module owns dispatch eligibility, retry scheduling, and
terminal results. The existing Store entry points stay public.

Command completion attempts its guarded update first. A concurrent caller
that finds the terminal row checks the saved result for exact replay.
Package receipts check the request ID before the receipt identity in one
query. Receipt and conflict replay checks share a write transaction with
their inserts. The original proof bytes and replay precedence are preserved.

Two new tests reproduce the previous races for identical terminal results
and concurrent receipts. Both pass after the change. The other four new
tests pass on both versions and cover command evidence, Rule origin, Cell
ownership, scheduling, and conflict and receipt replay. All 26 focused
Store tests pass. Rustdoc and downstream `Send` probes pass. Clippy goes
from 41 to 40 findings with a scoped expectation on the existing result
API's argument count.

The command entry module goes from 331 to 236 formatted nonblank lines.
Across command and evidence production files, formatted nonblank lines
go from 504 to 573 and non-whitespace characters from 14,368 to 15,992.
Excluding Rustdoc, the candidate has 505 lines and 13,988 characters.
Store's expanded library goes from 75,851 to 75,911 nonblank lines.
The tests add 407 nonblank lines and 11,619 characters.

The command entry module, queue, and evidence are reviewed. The four
remaining delivery markers are for application details, incoming handoffs,
local application, and attestations.
[Command persistence](../../../crates/store/src/transfer_delivery/remote_commands.rs) ·
[Queue](../../../crates/store/src/transfer_delivery/remote_commands/queue.rs) ·
[Delivery evidence](../../../crates/store/src/transfer_delivery/evidence.rs) ·
[Command tests](../../../crates/store/tests/transfer_delivery_commands.rs)

### Store application handoffs and attestations

Local settlement delegates the handoff transition to a dedicated module.
Replay, the saved review hash, and the signed Fact are read inside the
application write transaction. The application reuses its effective
policy when quoting private effects and checks that the group has a
primary effect. Incoming handoffs validate the current recipient snapshot
and store or replay the handoff under one transaction. Origin allocations
and incoming handoffs return the inserted rows directly.

Attestation delivery marks the outbox entry sent and the origin handoff
accepted in one transaction. Injecting a failure into the second update
leaves the attestation retryable. The baseline loses the retry. Concurrent
identical settlement applications return the same signed Fact and apply
the Record change once. The baseline rejects the second application as
stale.

Both regression tests pass after the changes. The other two new tests
pass on both versions and cover exact allocation, rejected unsigned
application, incoming snapshot authority, and signed private effects.
All 30 focused Store tests pass. Rustdoc and downstream `Send` checks
pass. Clippy goes from 40 to 39 findings with a scoped expectation for
the existing allocation API's argument count.

The local entry module goes from 352 to 336 formatted nonblank lines.
Across the affected production files, formatted nonblank lines go
from 1,249 to 1,389 and non-whitespace characters from 41,672 to
44,871. Excluding Rustdoc, the candidate has 1,314 lines and 42,572
characters. Store's expanded library goes from 75,911 to 76,039 lines.
The tests add 464 nonblank lines and 12,235 characters.

The four remaining application files and the new handoff transition module
are reviewed. The Store Transfer delivery tree has no remaining markers.
The shared effects module keeps its marker for the rest of its review.
[Local application](../../../crates/store/src/transfer_delivery/application/local.rs) ·
[Incoming handoffs](../../../crates/store/src/transfer_delivery/application/incoming.rs) ·
[Attestations](../../../crates/store/src/transfer_delivery/application/attestations.rs) ·
[Application tests](../../../crates/store/tests/transfer_delivery_application.rs)

### Store private effect quotes, writes, and evidence

The public effect module keeps its data types and paths. Quotation, ledger
writes and reversals, and evidence reads have separate modules. Quotation
borrows loan adjustments and checks for simulation assumptions once per
group. Its quantity calculations and review encoding retain the existing
order. Application reuses the actor from the primary Fact and borrows
its UID. Evidence reads fetch the related Facts and any original payloads
in one query in ledger order.

The extended application test passes on both versions. It covers scoped
simulation quantities, original Fact precedence, recorded effect order,
and quantity reversal. An unavailable secondary Record causes the whole
correction to roll back, including the primary balance. All 30 focused
Store tests, Rustdoc, and downstream `Send` checks pass. Clippy goes from
39 to 38 findings after removing the duplicate actor argument.

The public entry module goes from 381 to 44 formatted nonblank lines.
Across the affected production files, formatted nonblank lines go from
2,553 to 2,763 and non-whitespace characters from 73,810 to 76,112.
Excluding Rustdoc, lines go from 2,520 to 2,679 and characters from
72,697 to 73,551. Store's expanded library goes from 76,039 to 76,168
lines. The extra test coverage adds 124 nonblank lines and 3,097
non-whitespace characters to the existing application test.

The effects entry module and its three submodules are reviewed. The Fact
and balance modules keep their markers for their broader reviews.
[Effect API](../../../crates/store/src/transfer_effects.rs) ·
[Quotation](../../../crates/store/src/transfer_effects/quote.rs) ·
[Ledger writes](../../../crates/store/src/transfer_effects/writes.rs) ·
[Evidence reads](../../../crates/store/src/transfer_effects/evidence.rs)

### Store Transfer accounting boundaries

The public accounting module re-exports the same types and functions.
Signed policy history, formula binding, exact quantity queries, and
correction transactions have separate Store modules because each depends
on Store's connection and schema. Policy admission checks all destinations
with one query. Policy and effect writes share the stored names for effect
modes. Formula selection borrows existing formula text, and correction
borrows identifiers from a row containing only the fields it needs.

The new policy tests pass on the saved baseline and the refactor. They
check rejection when a secondary Record is retired, replay after later
retirement, and the exact quantity and review hash for unit conversion.
The baseline already rejects the spaced function spelling
`-incoming () / 2`. The conversion fixture uses `-incoming() / 2` and the
refactor preserves the parser's behavior.

All 32 focused Store tests pass. Rustdoc and a downstream `Send` check
pass, and Clippy reports the same 38 findings as the baseline. The
accounting files are reviewed and their refactor marker is closed.

With default rustfmt, the entry module goes from 520 to 16 nonblank lines.
Across the affected production files, nonblank lines go from 564 to 895
and non-whitespace characters from 19,339 to 24,066. Excluding Rustdoc,
the counts go from 561 to 781 lines and 19,172 to 19,871 characters.
Rustdoc, the explicit SQL and bindings, and the new module boundaries
account for the growth. Store's expanded library goes from 76,168 to
76,418 nonblank lines. The test file adds 144 nonblank lines and 3,447
non-whitespace characters.

[Accounting API](../../../crates/store/src/transfer_accounting.rs) ·
[Policies](../../../crates/store/src/transfer_accounting/policy.rs) ·
[Formula binding](../../../crates/store/src/transfer_accounting/binding.rs) ·
[Quantities](../../../crates/store/src/transfer_accounting/quantity.rs) ·
[Corrections](../../../crates/store/src/transfer_accounting/correction.rs) ·
[Policy tests](../../../crates/store/tests/transfer_policy.rs)

### Store Transfer balance projection

The public module keeps the balance and commitment types and their read
entry points. Separate Store modules reconcile Promise revisions, select
private destinations, read occurrence paths, and calculate commitments.
They remain in Store because selection and calculation use the same SQLite
snapshot. Shared indexes borrow their keys, and local occurrence rows go
directly into the projection type. Balance reads fetch occurrences only
for the selected local commitments and share one simulation assumption
table lookup with the effect quotes.

Both new tests pass against the baseline and the refactor. They cover
newest delivered terms, revoked delivery diagnostics, preference for the
Promise's own occurrence on a shared path, exact and legacy cancellations,
single and batch read parity, and private policies that map an activated
Promise to multiple Records. The tests also check the diagnostic for a
policy with no current terms.

All 34 focused Store tests pass, as do Rustdoc, the downstream `Send`
check, and the Protein build check. Clippy goes from 38 to 37 findings
after removing the old commitment function's nine-argument signature.
The balance files are reviewed and their refactor marker is closed.

Default rustfmt reduces the balance entry module from 729 to 190 nonblank
lines. Across the affected production files, including effect quotation,
nonblank lines go from 1,031 to 1,199 and non-whitespace characters from
28,313 to 32,207. Excluding Rustdoc, lines go from 1,012 to 1,127 and
characters from 27,692 to 29,428. Separate modules, typed occurrence
decoding, explicit SQL, and public API docs account for the additional
source. Store's expanded library goes from 76,418 to 76,540 nonblank
lines. The regression tests add 203 nonblank lines and 4,903
non-whitespace characters.

[Balance API](../../../crates/store/src/transfer_balances.rs) ·
[Promise revisions](../../../crates/store/src/transfer_balances/promise.rs) ·
[Selected inputs](../../../crates/store/src/transfer_balances/inputs.rs) ·
[Occurrence paths](../../../crates/store/src/transfer_balances/occurrence.rs) ·
[Commitments](../../../crates/store/src/transfer_balances/commitment.rs) ·
[Balance tests](../../../crates/store/tests/transfer_balances.rs)

### Store hard stock limits

The stock-limit module owns signed changes and their history. The authority
module owns current and historical Cell checks, and the validation module
checks projected surplus and imported authorship. All three stay in Store
because they use the same connection and transaction as the protected
write. Lookups share the limit and version queries. Roster checks iterate
writer entries directly, including the exact expiry boundary for former
writers.

Both regression tests pass against the saved baseline and the refactor.
They check signed payload and signature bytes, exact replay after removing
a limit, a reused request with different contents, rollback after a stock
shortfall, competing reservations, and rejection of an import by another
Cell. The authority test checks a former writer immediately before and at
expiry, with a non-UTC offset on the stored grant.

All 36 focused Store tests pass. Rustdoc, the downstream `Send` check, and
the Protein build check pass, and Clippy shows the same 37 existing
findings. The stock-limit source files are reviewed and their marker is
closed.

With default rustfmt, the entry module goes from 277 to 213 nonblank lines.
Across the three production modules, nonblank lines go from 277 to 388
and non-whitespace characters from 8,677 to 10,930. Excluding Rustdoc,
the production counts are 340 lines and 9,182 characters. Explicit SQL
and bindings, the module split, and the public contracts add source lines.
Store's expanded library goes from 76,540 to 76,627 nonblank lines.
The regression tests add 172 nonblank lines and 5,197 non-whitespace
characters.

[Stock-limit API](../../../crates/store/src/transfer_stock.rs) ·
[Cell authority](../../../crates/store/src/transfer_stock/authority.rs) ·
[Write validation](../../../crates/store/src/transfer_stock/validation.rs) ·
[Stock-limit tests](../../../crates/store/tests/transfer_stock.rs)

### Store Transport journey and capacity

The public Transport module re-exports the existing journey and capacity
functions. Journey projection owns bounded event reads, actor attribution,
and progress. Capacity calculation owns offering-route resolution and
segment reservations. Both stay in Store because they query the same
Transfer and Fact schema. Capacity reads use one SQLite snapshot, evaluate
each Transfer's journey and offering route once per reservation scan, and
compute the peak reservation only for the requested part. Segment selection
moves the projected data and reports an invalid span through `StoreError`.

The new tests compare the baseline and refactor for adjacent and
overlapping bookings, duplicate exchanges, terminal release before all
participants finish, the exact pickup grace boundary, and started journeys
that retain capacity. They also exercise revision filtering, event
attribution, and ignoring unrelated non-JSON Fact payloads.

All 38 focused Store tests pass, including both new tests on the
baseline. Rustdoc, Protein, and the downstream `Send` check pass.
Clippy reports the same 37 findings. The Store Transport files are
reviewed and their marker is closed.

Default rustfmt reduces the entry module from 264 to 10 nonblank lines.
Across the affected production modules, nonblank lines go from 264 to 370
and non-whitespace characters from 8,619 to 9,969. Excluding Rustdoc,
the refactor has 347 lines and 9,202 characters. The explicit SQL,
checked projections, module split, and API docs grow the source.
Store's expanded library goes from 76,627 to 76,720 nonblank lines.
The tests add 292 nonblank lines and 6,643 non-whitespace characters.

[Transport API](../../../crates/store/src/transport.rs) ·
[Journey projection](../../../crates/store/src/transport/journey.rs) ·
[Capacity and reservations](../../../crates/store/src/transport/capacity.rs) ·
[Transport tests](../../../crates/store/tests/transport.rs)

### Nucleus Transport route and event model

Route terms and capacity projections now live together, with a separate
module for event validation and Journey progress. Both are pure Nucleus
types and retain their public `nucleus::transfer::transport` paths. Route
validation and segment selection walk borrowed stops without allocating
intermediate vectors. Event parsing moves the nested value out of the
decoded payload. Enum order, field names, and serialized Journey flags
are preserved. Rustdoc covers the operations and constants. Schema-derived
fields keep their existing descriptions to preserve generated schemas.

The baseline comparison matches 112,896 route pairs, ten JSON Schemas,
and seven serialized examples. The fixtures check duplicate Journey
envelope keys, payload size limits, optional intermediate stops, and the
maximum route and capacity bounds.

The 42 Nucleus Transfer tests, the Transport test without default
features, and four Store Transport and stock-limit tests pass. Rustdoc
and the Protein build check pass. Clippy reports no findings in the
changed modules. The Nucleus Transport files are reviewed and their
marker is closed.

Across the affected files including the moved route test, formatted
nonblank lines go from 201 to 244 and non-whitespace characters from
4,803 to 6,367. Excluding Rustdoc, the refactor has 216 lines and 5,353
characters. Nucleus's expanded library goes from 439,533 to 439,585
nonblank lines. Documentation, the module split, and the route assertions
account for the additional source.

[Transport API](../../../crates/nucleus/src/transfer/transport.rs) ·
[Route terms and capacity](../../../crates/nucleus/src/transfer/transport/route.rs) ·
[Journey events and progress](../../../crates/nucleus/src/transfer/transport/journey.rs)

### Nucleus Transfer disclosure

Authored audiences and item terms, per-Promise redaction, and shared
history have separate modules under the existing public disclosure API.
They stay in Nucleus so Protein, Engine, and simulation can use the same
rules independently of storage. Projection combines field access in one
pass and moves the permitted cancellation fields into their public
objects. The summary field lists discard private keys directly, and
shared simulations still require the existing typed authorization.

The baseline and refactor match byte for byte on 6,192 projections
spanning anonymous, owner, participant, invitee, selected, outside, and
privileged views, with and without `serde_json/preserve_order`.
The comparison also covers duplicate Promise IDs, shared simulations,
cancellation payloads, and the four authored disclosure schemas.
Malformed Promise rows now return the existing disclosure error envelope.
Malformed nested readiness and settlement fields are redacted safely
instead of panicking. Regression tests exercise both cases.

The 44 Nucleus Transfer tests and 11 focused Store tests pass. Rustdoc
and the Protein build check pass. Clippy reports no findings in the
changed modules. The disclosure files are reviewed and their marker
is closed.

Across all affected files, including the moved tests, default rustfmt
gives 891 to 958 nonblank lines and 22,729 to 24,771 non-whitespace
characters. Excluding Rustdoc, the counts go from 890 to 937 lines and
22,658 to 23,833 characters. The module split, explicit redaction, and
regression tests increase the source size. Nucleus's expanded library
goes from 439,585 to 439,605 nonblank lines with default features.

[Disclosure API](../../../crates/nucleus/src/transfer/disclosure.rs) ·
[Audiences and authored terms](../../../crates/nucleus/src/transfer/disclosure/terms.rs) ·
[Promise redaction](../../../crates/nucleus/src/transfer/disclosure/item.rs) ·
[Shared history](../../../crates/nucleus/src/transfer/disclosure/history.rs) ·
[Disclosure tests](../../../crates/nucleus/src/transfer/disclosure/tests.rs)

### Transfer loan agreements and availability

Store keeps the same public loan API. Agreement reads, extension
proposals, delivered terms, effect evidence, and availability projections
now have their own modules. They remain in Store because they use the
existing SQLite connections and signed records. Availability indexes
effects by route and Record and indexes linked returns by source exchange.
The indexes borrow identifiers and preserve the previous order of exact
sums. Simulated receipts are grouped once by exchange and receiver.
Delivered terms decode borrowed JSON and reuse Promise membership by
exchange. Nucleus keeps the small shared interval and reference types
together with their existing serialized layout and half-open date bounds.

Both new tests pass against the baseline and the refactor. They cover
extension replay before a later end is accepted, local return
authorization, simulated receipt totals, disputed and unrelated
occurrences, retained delivery, linked physical debits, unit changes,
and exact availability boundaries.

The 18 focused Store tests and 44 Nucleus Transfer tests pass. Rustdoc,
Protein, and the downstream `Send` check pass. Interval, reference, and
Transfer item schemas match the baseline, as do the serialized interval
and reference examples. Store Clippy reports the same 37 findings. The
reviewed loan files have their markers closed.

With default rustfmt, the Store entry module goes from 219 to 26
nonblank lines. Across the affected Store production files, nonblank
lines go from 742 to 928 and non-whitespace characters from 23,328
to 27,880. Excluding Rustdoc, Store has 852 lines and 25,192 characters.
Nucleus goes from 87 to 104 nonblank lines and 1,886 to 2,357
characters. The new Store tests add 219 nonblank lines and 6,778
characters. With matching compiler flags the expanded Store library
adds 178 nonblank lines and Nucleus adds 16.

[Store loan API](../../../crates/store/src/transfer_loans.rs) ·
[Loan intervals](../../../crates/nucleus/src/transfer/loans.rs) ·
[Availability projection](../../../crates/store/src/transfer_loans/availability.rs) ·
[Loan tests](../../../crates/store/tests/transfer_loans.rs)

### Nucleus Transfer revision model

The public Transfer module re-exports revision snapshots, dependency
rules, occurrence evidence, settlement evidence, and agreement policy.
These types stay in Nucleus because Store, Engine, and Protein share
the same signed data and pure rules. Dependency ordering borrows graph
nodes and copies only the result. Bulk claim review sorts borrowed items
and uses the existing SHA-256 helper. Percentage agreement computes the
required count without overflowing a party-count multiplication.

The baseline and refactor give identical results for 512 dependency
graphs and 150,540 agreement cases. All 32 public type declarations keep
their fields and Serde attributes. The comparison also matches six
serialized examples, seven review or formula hashes, and four JSON
Schemas. The dependency test covers duplicate edges and nodes blocked
downstream of a cycle.

The 45 Nucleus Transfer tests and 14 focused Store tests pass. Rustdoc
and the Protein build check pass, and Clippy reports the same 32 findings.
The Transfer entry file and reviewed model modules have their markers
closed.

Default rustfmt reduces the Transfer entry module from 573 to 44
nonblank lines. Across the affected source files, including the moved
agreement test and new dependency test, the count goes from 580 to 693
nonblank lines and from 14,883 to 19,327 non-whitespace characters.
Excluding Rustdoc, the refactor has 644 lines and 17,028 characters.
The module split, documentation, and coverage increase the source.
Nucleus's expanded library goes from 439,194 to 439,388 nonblank lines
with matching compiler flags.

[Transfer API](../../../crates/nucleus/src/transfer.rs) ·
[Revision snapshots](../../../crates/nucleus/src/transfer/revision.rs) ·
[Dependency rules](../../../crates/nucleus/src/transfer/dependency.rs) ·
[Occurrence evidence](../../../crates/nucleus/src/transfer/occurrence.rs) ·
[Settlement evidence](../../../crates/nucleus/src/transfer/settlement.rs) ·
[Agreement policy](../../../crates/nucleus/src/transfer/agreement.rs)

### Nucleus Transfer receipt model

Receipt scope, approvals, participation authority, event content, wire
documents, and commands have named modules under the existing public
`nucleus::transfer::receipt` entry point. The model stays in Nucleus because
Store and Engine share its evidence types and validation. The private
validation modes name the rules for a review, proof fragment, or complete
publication. Publication validation reuses the same validated scope hash
across events. Binary64 validation checks canonical hex without building a
second string. Merge defers cloning the scope until approval conflicts
have been checked.

The baseline comparison matched all 25 JSON Schemas byte for byte and
preserved document serialization, signing bytes, identities, and command
permissions. It also matched the tested validation results and error
messages, including 8,210 binary64 inputs. All 25 public type declarations
retain their fields and Serde attributes. Existing callers can still
construct the `Command::RecordSource` variant with an owned request and
receive the existing `String` diagnostics from receipt validation.

All 45 Nucleus Transfer tests pass, and Rustdoc builds. Clippy reports the
same 32 findings as the baseline. The Engine receipt integration suite
cannot compile because of the pre-existing async `Send` failures in
Engine. Store and Protein compile as dependencies of that test command.

Default rustfmt reduces the entry module from 1,750 to 37 nonblank lines.
Counting all ten resulting files, including the moved tests, formatted
nonblank lines go from 2,049 to 2,125 and non-whitespace characters go from
51,395 to 53,628. Matching compiler expansion goes from 439,388 to 439,496
nonblank lines for Nucleus with the Store feature configuration. The added
declarations and imports give each module a clear boundary, and validation
does less repeated work in the event loop.

[Receipt API](../../../crates/nucleus/src/transfer/receipt.rs) ·
[Document validation](../../../crates/nucleus/src/transfer/receipt/view.rs) ·
[Receipt tests](../../../crates/nucleus/src/transfer/receipt/tests.rs)

### Nucleus receipt history and fragment resolution

The receipt history entry module exposes event projection and provenance
alongside merged outcome views. Projection and merging now live in separate
modules, with fragment cases grouped under the receipt history tests. The
model stays in Nucleus so Store, Engine, and the in-memory trust evaluator
use the same validation, identifiers, and conflict rules.

Fragment resolution borrows identifiers, validates the scope once, and keeps
each event's dependency count and verdict together. It allocates diagnostic
strings only for unresolved events in the result. Projection reuses the
source and binding hashes and borrows content except when normalizing a Need
link. Merging compares units and quantities against the first known value,
preserves canonical ordering, and borrows identifiers during cycle detection.

Baseline and candidate outputs match byte for byte across 32,768 selections
of available evidence, 512 merge cases, a duplicate dependency case, and
publication projection and references. All 17 affected public type
declarations retain their fields and serialization attributes. All 218 Nucleus
unit tests and nine Store trust-query and evidence-custody tests pass.
Rustdoc builds, and Clippy reports the same 32 findings as the baseline.

In two alternating local runs of the same debug build, the best of three
resolutions for a 5,000-event chain went from 2.36–2.64 seconds to 1.23–1.34
seconds. This measures the resolver with the event map already prepared.

Default rustfmt reduces the history entry module from 325 to eight nonblank
lines. Across the affected sources and moved tests, nonblank lines go from
1,538 to 1,573 and non-whitespace characters from 38,476 to 39,869. Excluding
Rustdoc, the comparison is 1,536 to 1,542 lines and 38,333 to 38,606
characters. Macro-expanded Nucleus goes from 439,496 to 439,591 nonblank
lines in the Store feature configuration.

[History API](../../../crates/nucleus/src/transfer/receipt_history.rs) ·
[Fragment resolution](../../../crates/nucleus/src/transfer/receipt_fragment.rs)

### Nucleus Trust Analysis evidence assembly

Receipt assembly and claim-head evaluation have separate modules under the
existing `trust_analysis::evidence` entry point. Both stay in Nucleus because
they use the same receipt and claim rules as the in-memory trust evaluator.
Receipt assembly indexes borrowed event bodies and provenance, reuses the
scope ID from grouping, and moves each resolved projection into its first
owned copy. Claim heads compare current statement IDs and collect visible
provenance in the same pass. The caller still supplies the authority verdict.

Baseline and candidate match byte for byte across 512 receipt batches and
768 claim-head cases. The same comparison covers missing Need proofs,
duplicate provenance, parallel corrections, hidden newer revisions, and
withdrawal and expiry. The earlier 33,282-line fragment and merge corpus
also matches after the resolver change. The public wire fields and
serialization attributes are preserved.

All 218 Nucleus unit tests and nine Store trust-query and evidence-custody
tests pass. Rustdoc builds, and Clippy reports the same 32 findings.

In two alternating local debug runs, the best of five assemblies for a
1,000-document correction chain went from 842–863 ms to 663–668 ms. Timing
includes document validation and construction of the final receipt group.

With default rustfmt, the entry module goes from 239 to five nonblank
lines. The affected files go from 629 to 737 nonblank lines and 15,502 to
18,071 non-whitespace characters. Excluding Rustdoc, the totals go from
603 to 693 nonblank lines and 14,585 to 16,350 characters. Nucleus macro
expansion with the Store feature configuration goes from 439,591 to 439,698
nonblank lines. The additional source covers the shared resolver entry
point and a borrowed index that eliminates repeated cloning and validation
during evidence assembly.

[Evidence API](../../../crates/nucleus/src/trust_analysis/evidence.rs) ·
[Receipt assembly](../../../crates/nucleus/src/trust_analysis/evidence/receipts.rs) ·
[Claim heads](../../../crates/nucleus/src/trust_analysis/evidence/claims.rs)

### Nucleus Trust Analysis query evaluation

The existing `trust_analysis::query` entry point re-exports policy, result
types, and evaluation. Evidence selection and decimal aggregation have
separate modules. Tests share fixtures and group eligibility, projection,
and weighting by responsibility. The evaluator stays in Nucleus alongside
the receipt and identity rules shared by Store and Engine.

Evaluation moves owned receipt copies into history merging. It collects
provenance only for returned rows and withheld sources, and looks up
Need evidence in the ordered outcome list. Aggregation reads the stored
decimal scale directly and keeps cap usage for a measurement across its
evidence levels. Cap allocation order and half-even rounding are preserved.

Baseline and candidate match byte for byte for all four query JSON Schemas,
default weights, and 4,096 policy and evidence combinations. These cover
contact filtering, duplicate and conflicting sources, withheld results,
caps, rounding, and arithmetic overflow. All 218 Nucleus unit tests and
eleven Store integration tests pass. Rustdoc builds, and Clippy reports the
same 32 findings as the baseline.

In two alternating runs of matching local debug builds, the best of seven
evaluations for 500 prepared sources went from 134–137 ms to 130–134 ms.
Timing covers evaluation and aggregation.

Default rustfmt reduces the query entry module from 772 to ten nonblank
lines. Across all affected sources and moved tests, nonblank lines go from
1,630 to 1,701 and non-whitespace characters from 38,450 to 40,007.
Excluding Rustdoc, the refactor has 1,681 nonblank lines and 39,370
characters. With Store's feature configuration, Nucleus macro expansion
goes from 439,698 to 439,777 nonblank lines.

[Query API](../../../crates/nucleus/src/trust_analysis/query.rs) ·
[Evidence selection](../../../crates/nucleus/src/trust_analysis/query/evaluate.rs) ·
[Decimal aggregation](../../../crates/nucleus/src/trust_analysis/query/aggregate.rs)

### Nucleus profile claims and recipe review

The existing claims API re-exports statement, signed document, and support
types from separate modules. Generated-claim tests share the claim fixtures
and live together. Claim and recipe validation stay in Nucleus alongside the
query policy and proof types that they validate.

Claim checks copy the reader policy fields they use when pinning the
reporting scope. Signing key and signature checks decode and re-encode in
fixed buffers. Recipe review reuses the checked permit bytes for its
commitment. Recipe validation still checks the permit signature after the
unsigned recipe and size checks.

The baseline comparison matches 13,078 outputs byte for byte. It covers ten
JSON Schemas, signing inputs, recipe review commitments, support and trust
decisions, and 12,672 mutations of public keys and signatures. All 22 affected
public declarations retain their fields and serialization attributes.
All 218 Nucleus unit tests and eleven Store tests pass. Rustdoc builds,
and Clippy reports the same 32 findings as the baseline.

In two alternating runs of matching local debug builds, the best of five
runs of 1,000 recipe review commitments went from 819–829 ms to 763–765 ms.

Default rustfmt reduces the claims entry module from 396 to 12 nonblank
lines. Across the affected source and test files, nonblank lines go from
1,322 to 1,397 and non-whitespace characters from 34,400 to 36,588. Excluding
Rustdoc, the candidate has 1,375 lines and 35,765 characters. Nucleus macro
expansion with Store's features goes from 439,777 to 439,850 nonblank lines.

[Claims API](../../../crates/nucleus/src/trust_analysis/claims.rs) ·
[Signed documents](../../../crates/nucleus/src/trust_analysis/claims/document.rs) ·
[Claim support](../../../crates/nucleus/src/trust_analysis/claims/support.rs)

### Nucleus discovery and contact paths

The discovery API separates the public question matcher from the request and
response-page contract. Page binding reuses request validation and checks its
bounded headers in place. The question hash, wire fields, deadline and cursor
rules, and artifact matching remain at the existing API boundary.

Contact search stores predecessors and borrowed edge proofs during traversal.
It builds an owned route only when a target qualifies. Ordered parents and
ordered neighbors keep the first shortest route deterministic without a
separate sort. Direction, exclusions, deduplicated proof preference, and the
depth and identity budgets still apply.

The baseline comparison matches five JSON Schemas, 32,768 route cases, and
665 discovery cases byte for byte. All 218 Nucleus unit tests pass. Rustdoc
builds, and Clippy reports the same 32 findings as the baseline.
Store ran 51 tests across contact disclosures, private contacts, evidence
custody, and trust queries. Forty-nine pass. Both failures reproduce against
the pre-refactor Nucleus snapshot at the same fixture updates, where SQLite
rejects BLOB values in JSON.

In two alternating runs of matching local debug builds, the best of seven
batches of 50 searches over 1,024 identities went from 334–338 ms to
240–251 ms. Each search constructs all 1,023 returned routes.

Default rustfmt reduces the discovery entry module from 342 to 11 nonblank
lines. Across discovery, traversal, and moved tests, nonblank lines go from
502 to 588 and non-whitespace characters from 11,651 to 13,595. Excluding
Rustdoc, the candidate has 569 nonblank lines and 12,917 characters. Nucleus
macro expansion goes from 439,850 to 439,945 nonblank lines with Store's
feature configuration.

[Discovery API](../../../crates/nucleus/src/trust_analysis/discovery.rs) ·
[Contact paths](../../../crates/nucleus/src/trust_analysis/paths.rs)

### Nucleus contact disclosures and relations

The contact API separates audience and statement rules, signed documents,
and relation evaluation. Contact disclosures and profile claims share the
same fixed-buffer base64 check for signing keys and signatures. The document
validation order and public signing inputs are preserved.

Relation evaluation borrows the typed pair and keeps one current revision
per direction. A contradictory statement at that revision suppresses the
relation, including when the contradictory copy is hidden. Readability and
authority apply to copies of that exact statement. The evaluator hashes
only the statements that become returned proofs and retains the existing
edge and proof ordering.

The baseline comparison matches 27,017 outputs byte for byte, including
three JSON Schemas, 14,336 relation cases, signing bytes and IDs, audience
validation, and 12,672 key and signature mutations. All 218 Nucleus unit
tests pass. Rustdoc builds, and Clippy reports the same 32 findings as the
baseline. All 49 selected Store tests pass, with the two SQLite BLOB fixture
failures recorded in the preceding batch skipped.

In two alternating runs of matching local debug builds, the best of five
evaluations over 5,000 disclosures went from 296–302 ms to 169–174 ms. The
input contains 30 revisions of the same directed relation.

Default rustfmt reduces the contact entry module from 267 to ten nonblank
lines. Across the affected sources and tests, nonblank lines go from 798
to 841 and non-whitespace characters from 21,768 to 23,030. Excluding
Rustdoc, the candidate has 814 nonblank lines and 22,077 characters. Nucleus
macro expansion with Store's features goes from 439,945 to 440,002 nonblank
lines.

[Contact API](../../../crates/nucleus/src/trust_analysis/contacts.rs) ·
[Relation evaluation](../../../crates/nucleus/src/trust_analysis/contacts/relations.rs)

### Nucleus evidence serving and chunk assembly

The serving API separates original artifacts, scoped reviews, and chunk
assembly into modules. Serving validates typed contact and claim documents
directly after checking their original byte limits. Review lifetime, reader
policy, and content hash still use the existing public contracts.

Chunk assembly decodes into one output buffer and relies on the standard
base64 engine's canonical padding and trailing-bit checks. It verifies the
declared lengths, ordered offsets, and final artifact hash. A regression
test exercises valid JSON split across two chunks and checks incomplete
retrieval.

The baseline comparison matches all 1,265 recorded outputs, including
five JSON Schemas, canonical bytes and hashes, malformed chunks, bounded
review cases, and artifact validation. All 219 Nucleus unit tests and
14 Store tests pass. Rustdoc builds. Clippy findings decrease from 32 to 31
after using `is_multiple_of` for offset alignment.

In two local debug runs, the best of five batches of 25 assemblies went
from 257–261 ms to 240–245 ms for an 11-chunk artifact. This synthetic
sample pads valid JSON with whitespace to exercise chunk decoding.

Default rustfmt reduces the serving entry module from 322 to 14 nonblank
lines. Across the affected files and new regression test, nonblank lines go
from 322 to 443 and non-whitespace characters from 8,032 to 10,988.
Excluding Rustdoc, the candidate has 424 lines and 10,322 characters.
Nucleus macro expansion with Store's features goes from 440,002 to
440,075 nonblank lines.

[Serving API](../../../crates/nucleus/src/trust_analysis/serving.rs) ·
[Chunk assembly](../../../crates/nucleus/src/trust_analysis/serving/chunk.rs)

### Nucleus evidence custody and remote reviews

Custody separates signed offers and acknowledgments from policy and control
commands. Remote queries separate public question construction from review
binding. Existing module paths export the same types. The contracts stay in
Nucleus alongside the original evidence formats.

Custody acceptance reuses the canonical bytes produced by artifact
validation. Acknowledgments check cheap field mismatches before hashing the
offer. Remote question construction validates each question once, and
review validation checks recipient uniqueness and format in one pass.
Public questions contain the same fields and preserve their order.

The baseline comparison matches eight JSON Schemas, the signing inputs,
512 custody combinations, and 1,024 remote draft and review cases. All 219
Nucleus unit tests and 14 Store integration tests pass. Rustdoc builds,
and Clippy reports the same 31 findings as the baseline.

In two alternating local debug runs, 1,000 custody checks against a small
contact artifact went from 114–117 ms to 93–96 ms. Building 1,000 batches
of 34 public questions went from 839–844 ms to 749–768 ms.

Default rustfmt reduces the custody entry file from 194 to 89 nonblank
lines and the remote entry file from 316 to 60. Across both contracts,
tests, and their serving helper, nonblank lines go from 823 to 887 and
non-whitespace characters from 19,928 to 21,856. Excluding Rustdoc, the
candidate has 860 nonblank lines and 20,834 characters. Nucleus macro
expansion with Store's features goes from 440,075 to 440,139 nonblank lines.

[Custody agreements](../../../crates/nucleus/src/trust_analysis/custody/agreement.rs) ·
[Remote drafts](../../../crates/nucleus/src/trust_analysis/remote/draft.rs) ·
[Remote reviews](../../../crates/nucleus/src/trust_analysis/remote/review.rs)

### Nucleus Trust Analysis and receipt delivery commands

Contact drafts and review commitments have their own module under the
Trust Analysis command API. Receipt delivery separates command routing and
record references from envelope validation. Both command families derive
mutation status from the permission they return. Receipt delivery collects
source and thread references directly into the returned list.

Delivery parsing checks the envelope's routing, term, recipient rules, and
budget. Full validation also checks an embedded receipt document. Public
commands, recipient restrictions, and review commitments retain their
existing contracts.

The baseline comparison matches all 181 outputs, including nine JSON
Schemas, permission and mutation decisions, ordered record references, and
delivery validation and parsing. All 219 Nucleus unit tests and 17 Store
integration tests pass. Rustdoc builds, and Clippy reports the same 31
findings, including the existing artifact enum warning at its new path.

Default rustfmt reduces the Trust Analysis command entry file from 175 to
110 nonblank lines and the receipt delivery entry file from 288 to 11.
Across affected files and moved tests, nonblank lines go from 463 to 483
and non-whitespace characters from 9,813 to 10,661. Excluding Rustdoc, the
candidate has 466 nonblank lines and 10,073 characters. Nucleus macro
expansion with Store's features goes from 440,139 to 440,147 nonblank lines.

[Trust Analysis commands](../../../crates/nucleus/src/trust_analysis/commands.rs) ·
[Receipt delivery API](../../../crates/nucleus/src/transfer/receipt_delivery.rs)

### Store receipt versions and inbox pagination

The receipt repository separates version reads, save transactions, request
replay, and source mappings behind its existing API. Version reads still
check the stored JSON against the signed Fact and verify the chain link.
Replays and version lookups reuse a connection. Writes reuse the signed
payload and skip merging an already identical, validated document. Inbox
pagination compares the timestamp and UID together, preserving the existing
sort and visibility filters when timestamps tie.

Nineteen Store integration tests passed. The two receipt-version tests also
passed against the pre-refactor snapshot and cover replay after an append,
rollback on signing failure, and inbox pagination at equal timestamps.
Rustdoc built. Store Clippy completed with warnings enabled and reported no
findings in these files.

Default rustfmt takes the receipt entry file from 402 to 14 nonblank lines.
Across affected production files, nonblank lines go from 501 to 745 and
non-whitespace characters from 17,197 to 18,792. The increase includes
expanded SQL formatting and separate read and write modules. With both new
regressions, the candidate has 967 nonblank lines and 23,993 characters.
Store macro expansion goes from 76,898 to 77,032 nonblank lines.

[Receipt repository](../../../crates/store/src/transfer_receipts.rs) ·
[Delivery inbox](../../../crates/store/src/artifact_deliveries.rs) ·
[Receipt tests](../../../crates/store/tests/receipt_versions.rs)

### Store Record repository boundaries

`store::records` retains its public imports and delegates to readers,
lifecycle transactions, field writes, exact quantities, and extensions.
Extension modules separate authoring, reviewed-namespace policy, ordinary
reads, and exact numeric reads. These operations remain in Store because
they share the Record schema, transaction, and sync log. The public body
projection flag and selection variants preserve existing callers.

The extension policy shares its comparison against stored reviewed values.
It keeps outcome creation eligible before the first signed row and preserves
the separate deletion rules. Extension writes move parsed fields into the
diff, and batch head lookup serializes the caller's slice directly. Exact
quantity reads share the same query in pooled and transactional callers.
Unit and place validation binds IDs to fixed statements.

All 71 tests across 17 focused Store integration suites passed. The 25
tests in the six direct Record suites also passed before the refactor.
Rustdoc built. Clippy reported no findings in the reorganized files and
33 existing warnings elsewhere in Store, down from 37 before the refactor.

Default rustfmt takes the entry file from 1,904 to 87 nonblank lines.
Including all successor files and their API docs, nonblank lines go from
1,904 to 2,011 and non-whitespace characters from 47,864 to 52,777.
Excluding Rustdoc, the candidate falls to 1,887 lines and 47,211 characters.
Store macro expansion goes from 77,032 to 77,257 nonblank lines.

[Record API](../../../crates/store/src/records.rs) ·
[Extension policy](../../../crates/store/src/records/extensions/policy.rs) ·
[Transaction tests](../../../crates/store/tests/record_transactions.rs)

### Store Assertion repository boundaries

`store::assertions` keeps its 51 public names and delegates to modules for
strict transactions, legacy authoring, validation, row models, read
projections, related Records, binary links, and graph traversal. The modules
stay in Store because they share Assertion rows, transactions, and sync
operations. Strict writes retain their reference and local identity checks.
Legacy paths retain their ensure, transition, and refinement semantics.

Strict insertion binds the timestamp directly in place of constructing a
temporary row. Sync logging constructs the JSON object once and keeps the
original field names and version enrichment. Descendant traversal moves
queued identifiers into the result and retains breadth-first order and
cycle detection.

Seventy-four focused Store tests passed. Both new cases also passed with
the pre-refactor Store library, covering traversal order and exact legacy
sync payloads. `assertion_lifecycle_advances_old_and_new_subjects_only`
failed before and after with revision 3 where the test expected 2.
Protein's library check and Store Rustdoc passed. Clippy reported one
fewer finding and no findings in the reorganized files.

With default rustfmt, the entry file goes from 1,694 to 33 nonblank lines.
Across all production files, nonblank lines go from 1,694 to 1,830 and
non-whitespace characters from 43,197 to 47,907. Excluding Rustdoc, the
candidate has 1,751 lines and 44,074 characters. The module imports and
explicit public re-exports add some source. With both regressions, the
candidate has 1,977 lines and 51,324 characters. Store's expanded code
goes from 77,257 to 77,383 nonblank lines.

[Assertion API](../../../crates/store/src/assertions.rs) ·
[Strict transactions](../../../crates/store/src/assertions/mutation.rs) ·
[Graph and sync regressions](../../../crates/store/tests/assertion_graph.rs)

### Store authorization snapshots

`store::access_snapshot` retains its public types and entry points. Metadata
coordinates the bounded reads for Records, Concepts, Assertions, visibility,
Lingua, and draft ownership. Targeted Record and extension content has its
own reader. This boundary stays in Store because both readers use the
caller's SQLite transaction. The authorization evaluator remains separate.

Preflight and payload queries now share compile-time byte expressions.
All row and cumulative limits are still checked before payload reads.
The expanded SQL matches 22 previous compound statements modulo whitespace.
The remaining Place preflight uses the same aggregate over a subquery and
matched the previous expression on empty, null, text, and BLOB inputs.
Identity indexes borrow decoded UIDs. Parsed JSON objects move into the
result, and targeted reads carry each preflight count with a borrowed UID.

The same 53 Store integration tests pass before and after, including the
17 snapshot cases for bounds, isolation, and malformed storage. Store
Rustdoc passes, and Clippy reports the same 32 findings elsewhere in Store.
The standalone authority suite still has its four baseline assertion-intent
failures. The Engine access suites could not build because of existing async
`Send` errors in Engine.

With default rustfmt, the entry file goes from 1,371 to 18 nonblank lines.
Across successor files, nonblank lines go from 1,371 to 1,517 and
non-whitespace characters from 36,221 to 39,364, including Rustdoc.
Excluding Rustdoc, the candidate has 1,478 lines and 37,612 characters.
Store macro expansion goes from 77,383 to 77,544 nonblank lines.

[Snapshot API](../../../crates/store/src/access_snapshot.rs) ·
[Metadata reader](../../../crates/store/src/access_snapshot/metadata.rs) ·
[Bounded SQL](../../../crates/store/src/access_snapshot/sql.rs) ·
[Snapshot tests](../../../crates/store/tests/access_snapshot.rs)

### Store sync operation log and outbox

`store::sync_ops` retains its 43 public names and wire fields. Writes,
ordinary reads, vector reads, scope checks, outbox operations, and retention
have separate modules. They stay in Store because they share SQLite state
and caller-owned write transactions.

Local and incoming appends share the insert bindings. Local writes also
share the outbox routing bindings, while incoming duplicates keep their
original no-forwarding behavior. General and private vector readers share
their bound coverage query. Outbox replay borrows its fields and retains
the separate scanned and queued counts. Pruning preserves pending rows,
quantity openings, Assertion history, and replica-root boundaries.

Typed SQLx reads return decode errors for malformed rows. Local Assertion
logging returns a protocol error for scalar and array payloads instead of
panicking. Object payloads retain their stored revision fields, and JSON
null retains the existing enrichment behavior.

All 87 focused Store tests passed. The three new compatibility tests also
passed with pre-refactor Store. The two malformed-input tests reproduced the
old panics and now return errors. Rustdoc passed. Clippy removed two prior
findings and reported none in the reorganized files or new tests.

Default rustfmt takes the entry file from 1,133 to 28 nonblank lines. Across
production files, nonblank lines go from 1,133 to 1,111 and non-whitespace
characters from 23,638 to 26,790. Excluding Rustdoc, the candidate has 1,040
lines and 23,179 characters. With the five regressions it has 1,351 lines
and 32,901 characters. Store macro expansion goes from 77,544 to 77,760
nonblank lines.

[Sync log API](../../../crates/store/src/sync_ops.rs) ·
[Write routing](../../../crates/store/src/sync_ops/write.rs) ·
[Regression tests](../../../crates/store/tests/sync_log.rs)

### Store incoming sync projection

`store::sync_apply` retains its 14 public names and separates Record,
extension, Assertion, and Concept replay. They remain in Store because the
projections use its SQL schema and transaction boundaries. All stale-write
checks still compare the same operation identity and hybrid clock.

Record updates select fixed SQL statements and bind borrowed JSON text.
Opening quantity handling retains exact arithmetic, stock guards, and the
zero-opening existence check. Extensions move stored fields into the update
and retain legacy non-object handling. Assertions borrow incoming text,
preserve pinned definition revisions, and retain ordinary tuple winner
selection after tombstones.

All 77 focused Store tests passed. The three new replay cases also passed
with pre-refactor Store. Rustdoc passed, and Clippy reported the same 30
findings elsewhere in Store.

With default rustfmt, the entry file drops from 601 to 51 nonblank lines.
Across production files, nonblank lines go from 601 to 699 and
non-whitespace characters from 14,758 to 15,818. Excluding Rustdoc, the
candidate has 675 lines and 14,538 characters. The production files and
three replay tests total 897 lines and 20,916 characters. Store macro
expansion goes from 77,760 to 77,837 nonblank lines.

[Incoming sync API](../../../crates/store/src/sync_apply.rs) ·
[Assertion projection](../../../crates/store/src/sync_apply/assertions.rs) ·
[Replay tests](../../../crates/store/tests/incoming_projection.rs)

### Store authentication and Person access

`store::auth` retains its 48 public names. Roles and permissions, Person
access, credential writes, and login and principal reads have separate
modules. The bounded retained-access reader is separate from live lookups
and revision-checked changes. They remain in Store because their checks and
writes depend on SQLite schema, triggers, and caller transactions.

Credential creation shares the insert bindings. Replacement and removal
share their existing-credential preflight while retaining the generation
and row-count guards. Login lookups use fixed SQL for both keys. Bounded
Person and Role reads use checked count conversions, and `AuthUser` now
redacts the password hash in debug output. The raw hash remains available to
the credential verifier.

The focused Store suites passed 166 tests. Four failures reproduced against
pre-refactor Store: the bootstrap administrator expectation, Person
recreation's duplicate content revision, and two private-contact fixtures
that hit SQLite's JSON BLOB restriction. The redaction test fails on the old
derived Debug implementation and passes now. Rustdoc passed, and Clippy
reported the same 30 findings elsewhere in Store.

With default rustfmt, the entry file drops from 1,033 to 29 nonblank lines.
Across production files, nonblank lines go from 1,033 to 1,178 and
non-whitespace characters from 26,632 to 31,885. Excluding Rustdoc, the
candidate has 1,094 lines and 28,229 characters. Including the new test it
has 1,201 lines and 32,639 characters. Store macro expansion goes from
77,837 to 78,016 nonblank lines.

[Auth API](../../../crates/store/src/auth.rs) ·
[Credential writes](../../../crates/store/src/auth/credentials.rs) ·
[Debug redaction test](../../../crates/store/tests/auth_user.rs)

### Store Organ identity and contact delivery

`store::organs` retains its public API. Local identity, contacts, delivery,
and quarantine have separate modules within Store, where they share its
SQLite schema and transactions. Contact and presentation reads share a
checked proximity decoder. A stored value outside the `u32` range now
returns a protocol error. Valid values keep the same behavior, including
the full `u32` range.

The focused suites passed 65 tests. The pre-refactor baseline passed its
51 existing tests. The new normal-contact test also passed on the baseline,
and the new malformed-proximity test reproduced the original integer wrap.
Rustdoc passed, and Clippy reported the same 30 findings elsewhere in Store.

With default rustfmt, the entry module goes from 852 to 26 nonblank lines.
Across production files, nonblank lines go from 1,041 to 1,124 and
non-whitespace characters from 25,503 to 28,802. The candidate has 1,058
lines and 25,701 characters excluding Rustdoc, or 1,204 lines and 31,621
characters including the new tests. The increase includes checked decoding,
module declarations, and public API documentation. Store macro expansion
goes from 78,016 to 78,151 nonblank lines.

[Organ API](../../../crates/store/src/organs.rs) ·
[Contact presentation](../../../crates/store/src/organs/presentation.rs) ·
[Contact regression tests](../../../crates/store/tests/organ_contacts.rs)

### Presentation models boundary

`lince-presentation-models` owns renderer-independent settings, projections,
and drafts. Simulation's projection tests use that crate directly. Interface
continues to expose the same module paths for clients. The calendar helper
that shares a timezone parser across display rows is public so native
Interface can still call it through the boundary.

Pure query validation and Record field metadata live in the existing
storage-independent Protein crate. The execution facade keeps its public
entry points and wraps validation failures in the same Store error type.
This leaves the presentation models' normal dependency tree clear of
Store, Engine, SQLx, and Bevy. Transfer draft construction and form editing
live in descriptive sibling modules.

The original 151 Interface library tests passed under the models
configuration. After the move the same tests passed, with 129 in Interface
and 22 in presentation models. Eight presentation tests passed with default
features disabled, five Protein wire tests passed, and the Protein library
check passed. Across the affected Rust files, including
moved files and the Simulation caller, default rustfmt gives 9,007 to
9,032 nonblank lines and 213,247 to 214,476 non-whitespace characters.
The comparison omits markers and old audit headers. The additional
source supports the package boundary and its compatibility paths.
Macro-expanded lines and native Laboratory behavior still need verification.
Structurally moved files retain their pending markers.
[Presentation models](../../../crates/presentation-models/src/lib.rs) · [Query validation](../../../crates/protein-authority/src/query/validation.rs)

### Theme and interface test boundaries

Theme tokens, preset files, and JSON and CSS exchange live together in
Presentation Models. The Bevy color and ECS adapters are opt-in so the
command-line design tools can use the same theme model without the renderer.
Host owns the renderer-independent test protocol and runner, while Interface
retains the Bevy test adapter. Existing `lince_interface::tokens` and
`lince_interface::testing` paths stay available through re-exports.
`xtask` now uses Host for its test protocol and desktop controls and
Presentation Models for themes. Its normal dependency tree went from 213
distinct packages to 182, including `xtask` itself.

The original 134 Interface tests passed by name across the new boundaries.
A focused test also covers nested cancellation scopes and asynchronous
notification. All 112 `xtask` tests, the Interface behavior-check build,
and focused Host and Presentation Models Clippy checks passed locally.
Both preset files have identical blobs after the move.
The runner records dispatch results without copying their diagnostic strings,
and the theme file poller avoids a full copy of changed theme text.

The affected Rust files went from 5,265 to 5,323 formatted nonblank lines
and 132,860 to 135,087 non-whitespace characters, excluding tracking headers.
The same renderer-free production theme and test-driver sources went from
3,794 to 3,842 macro-expanded nonblank lines in a separate expansion harness.
The split adds module interfaces, contract documentation, and a cancellation
test. The isolated macro comparison covers async cancellation and excludes
the Bevy backend, Bevy derives, and test code. Native Laboratory still
requires a passing Engine build.
[Theme tokens](../../../crates/presentation-models/src/tokens.rs) · [Test protocol](../../../crates/host/src/testing/mod.rs)

### Headless Park and desktop layout boundary

Park's headless Wayland server and control examples now use Host directly.
The normal dependency tree for Park with `server-tests` went from 249 to
141 distinct packages, counting Park itself. The headless configuration
uses Host's base contracts. Interface enables Host's renderer-independent
layout policy and re-exports it as `lince_interface::layout`. The workspace
solver has its own module, and native presentation keeps the same 19
layout telemetry labels through the existing Nucleus recorder.

All 114 original layout tests passed by name after the move, along with a
new numeric boundary test. They cover tiling, scratch state, focus,
placement, and preferred output restoration. The telemetry variant, Host
layout Clippy, the Interface behavior-check build, and the headless Park
test entry point passed locally. Park ran 203 tests with seven ignored.
The first baseline invocation compiled to the link step and needed the
locally installed xkbcommon library on the link path. Native Laboratory
still requires a passing Engine build.

The affected formatted sources went from 29,381 to 29,398 nonblank lines
and 706,289 to 706,002 non-whitespace characters, excluding audit headers.
The isolated production layout expansion, with telemetry and tests
disabled, went from 10,910 to 10,960 nonblank lines. The additional source
includes the solver module boundary, API documentation, and the numeric
boundary test. The shared layout rules and sealed control response code
completed their file-level review. The larger moved modules still carry
their markers where further review is due.
[Layout rules](../../../crates/host/src/layout.rs) · [Solver](../../../crates/host/src/layout/desktop/solver.rs) · [Park control transport](../../../crates/park/src/control_protocol.rs)

### Shared instrumentation boundary

Host previously depended on Engine to install a performance observer and
on Nucleus to emit layout readings. The standard-library-only
`lince-instrumentation` crate now owns the single process-wide observer.
Nucleus re-exports it at `nucleus::performance`, and the existing
`engine::performance` re-export still resolves through Nucleus. Host uses
the same observer directly for installation and layout readings. The
monitor keeps its existing report schema and accepts the same allowlisted
labels. Operation marks still use keyed process-local IDs.

Host's normal dependency tree with `backend` fell from 515 packages to 85.
With `layout-telemetry` it fell from 170 to 85. The all-features Host test
entry point now runs independently of Engine. All 192 Host unit tests
passed, including the report privacy check. An integration test verifies
that layout timings and counts reach the shared observer. The same 221
original Nucleus tests passed by name across Nucleus and the recorder.
Store's focused observer test and 15 unit tests passed, with one ignored.
The all-targets Host and Instrumentation Clippy check and recorder
Rustdoc build passed.

Across the affected formatted Rust sources, nonblank lines went from
3,245 to 3,301 and non-whitespace characters from 74,880 to 76,737.
The isolated recorder's production expansion went from 97 to 129
nonblank lines. The source increase includes public API documentation,
debug representations, and the observer integration test. The recorder
and its moved tests completed their file-level review. Native Laboratory
still requires a passing Engine build.
[Recorder](../../../crates/instrumentation/src/lib.rs) · [Host observer test](../../../crates/host/tests/layout_telemetry.rs)

### Optimization model and execution boundary

Nucleus keeps its exact-decimal optimization model, wire types, model
validation, and assignment verification available without a solver
dependency. The `optimization-solver` feature owns simplex execution
and its infeasibility diagnostics. It remains enabled by default when
building Nucleus directly. Workspace dependencies select the model by
default, and Engine explicitly enables execution for its Demeter worker
and callers. The solver stays inside Nucleus alongside the exact types
it consumes.

The normal dependency trees for Presentation Models, Protein Authority,
and Store each shed eight packages, including microlp, ndarray, and sprs.
All three independent library checks passed. Meal solution assembly is
in its own module and borrows supply IDs while totaling purchased amounts.
The report types, ordering, constraint arithmetic, time limits, and
solver version stay the same.

Without the solver, all 219 Nucleus unit tests and both model integration
tests passed. All model-only test targets compiled. The same 30
optimization and Demeter tests passed with the solver before and after
the move. Strict Clippy reports 32 findings outside the changed files.

The affected formatted Rust sources went from 1,256 to 1,303 nonblank
lines and 28,114 to 29,870 non-whitespace characters. The isolated
optimization module expanded to 3,526 nonblank lines before the split,
3,542 with the solver enabled after it, and 3,283 in the new model-only
configuration. The source comparison includes the two new module
boundaries and the feature-specific tests for validation and wire round
trips. It omits old audit headers and queue markers.
[Model](../../../crates/nucleus/src/optimization.rs) · [Solver](../../../crates/nucleus/src/optimization/solver.rs) · [Meal solution](../../../crates/nucleus/src/demeter/meal/solution.rs)

### WGSL compiler boundary

`lince-wgsl` owns the shared shader interface, spatial stage types, and
bounded compiler. It can build and test the compiler independently of
Nucleus and the renderer. Nucleus retains the effect catalog and numeric
bindings and re-exports the existing shader interface, spatial module,
and compiler paths. The model configuration retains serialized spatial
settings and numeric validation. Engine and native Interface explicitly
enable shader compilation and admission. Direct Nucleus builds also
enable it by default.

The normal dependency counts for independent Presentation Models,
Protein Authority, and Store builds fell from 118 to 102, 114 to 97, and
163 to 151 respectively. All three library checks passed. The same 14
shader tests passed across Nucleus and the new crate. Six shader tests
run in model-only Nucleus, and all of its model-only test targets
compile. Standalone WGSL Clippy passed with all targets and features.
A direct comparison against the previous implementation matched 11
surface compilation outputs and three mesh classifications byte for
byte, including the tested diagnostics.

The affected formatted Rust sources went from 1,523 to 1,570 nonblank
lines and 43,481 to 45,148 non-whitespace characters. Isolated
production expansion with compilation enabled went from 5,837 to
5,847 nonblank lines across the affected modules. The comparison omits
tracking headers. The compiler now uses the validated fragment entry
directly, and program identifiers share the existing identifier check.
Native rendering still requires a passing Engine build to verify.
[Compiler](../../../crates/wgsl/src/compiler.rs) · [Spatial admission](../../../crates/wgsl/src/spatial/admission.rs) · [Nucleus shader effects](../../../crates/nucleus/src/shader_effect.rs)

### Karma program and scheduling boundary

`lince-karma` owns the typed program model, DSL, proof and evaluation,
replay, and schedule and deadline planning. Its normal dependency tree has
30 packages, including the crate itself, compared with 96 for model-only
Nucleus before the split. The typed core builds and tests on its own.
Nucleus exposes the existing `nucleus::karma` names and owns the Rule and
consequence adapters that depend on Nucleus types. It also keeps the IANA
timezone provider and its byte-sensitive source.

Canonical encoding writes JSON atoms into the output buffer and sorts
object keys even when the underlying JSON map preserves insertion order.
The canonical digest, its domain rules, the prefixed ULID format, and
Frequency defaults keep their existing behavior. The Frequency builder
uses the checked interval directly and computes civil time only for
calendar schedules.

The independent package passed the same 134 tests by name. The three
bundled timezone tests still pass in Nucleus. A direct comparison matched
14 schema documents and the bundled timezone revision byte for byte.
The canonical model suite passed with insertion-ordered JSON enabled.
The combined model-only Karma and Nucleus suites passed 421 tests. Store
passed all 27 focused program, Frequency, schedule, occurrence, and run
tests. Presentation Models passed its 22 library tests. Store, Protein
Authority, and Presentation Models library checks passed, as did Rustdoc.
Clippy reported ten findings in moved Karma files
that still need review. The fully reviewed files had no findings.
The Engine and Transport build check stopped after reproducing the existing
async `Send` failures in Engine.

Across affected production files and the moved tests, formatted nonblank
lines went from 20,245 to 20,377 and non-whitespace characters from
499,444 to 506,889. The reviewed implementations, excluding Rustdoc,
went from 577 to 569 nonblank lines and 13,436 to 13,227 characters.
Compatibility exports and module declarations account for most of the
source increase. Combined production macro expansion went from 440,147 to
440,298 nonblank lines. The moved sources still carry their pending markers
where a file-level review remains due.

[Karma API](../../../crates/karma/src/lib.rs) · [Nucleus compatibility exports](../../../crates/nucleus/src/karma/mod.rs) · [Canonical model tests](../../../crates/karma/tests/karma_k0.rs)

### Karma DSL lexer, parser, and formatter

The DSL module now exposes its original public entry points and diagnostics.
Its lexer borrows unquoted words and its parser consumes tokens by value.
Program and Frequency grammars have separate modules that share the cursor,
list parsing, and diagnostic positions. The formatter writes recursive
values and cadence components into the caller's output buffer. The lexer
still checks the entire source first, preserving which error is reported
when lexical and grammar errors occur together.

The same 134 tests passed by name, and one new test verifies lexical
precedence and UTF-8 byte columns. A direct baseline comparison matched
canonical text and error diagnostics on 20,599 inputs. Rustdoc and default
format checks passed. Clippy reported eight existing findings outside the
DSL. A scoped expectation keeps the temporary parsed cadence inline.
On seven Program and four Frequency fixtures, heap
allocation and reallocation requests fell from 1,915 to 383 for Program
parsing, from 580 to 189 for Program formatting, from 1,060 to 578 for
Frequency parsing, and from 179 to 92 for Frequency formatting.

Across the DSL files, formatted nonblank lines went from 2,404 to 2,327 and
non-whitespace characters from 58,872 to 56,822, including the new test.
The Karma library's production macro expansion went from 88,972 to 88,758
nonblank lines.

[DSL API](../../../crates/karma/src/program/dsl.rs) · [Lexer](../../../crates/karma/src/program/dsl/lexer.rs) · [Parser](../../../crates/karma/src/program/dsl/parse.rs) · [Formatter](../../../crates/karma/src/program/dsl/format.rs)

### Karma evaluation, controls, and arithmetic

The evaluator reads prior outputs from the trace by node and port. It
moves IDs from the proof order into the trace, borrows program keys for
the index, and constructs JSON pointers when a failure or rounding note
needs one. Node dispatch, temporal controls, expression arithmetic, and
fuel accounting have separate modules. Delay and control updates still
stage state for a subsequent occurrence. Proof inconsistencies in node,
port, and trace lookup now return typed invariant failures.

All 135 Karma tests passed by name. Store passed its 15 run tests,
including restart and control-state persistence. A direct comparison
matched serialized results and failures on 4,020 evaluation cases at
several fuel and depth limits. The same probe counted 802,741 to 752,856
allocation and reallocation requests while evaluating, including proof.
Rustdoc and default formatting checks passed. Clippy reported eight
existing findings outside the evaluator.

Across the evaluator files, formatted nonblank lines went from 1,497 to
1,309 and non-whitespace characters from 33,403 to 32,545. The Karma
library's production macro expansion went from 88,758 to 88,761 nonblank
lines, including the added public contract documentation.

[Evaluator API](../../../crates/karma/src/program/evaluate.rs) · [Controls](../../../crates/karma/src/program/evaluate/control.rs) · [Arithmetic](../../../crates/karma/src/program/evaluate/expression/arithmetic.rs)

### Karma proof and replay

Program validation separates ports and sensitivity, operation contracts,
expression types, and dependency order. The graph borrows node IDs and
indexes edges with lexical slots. It preserves the ready-node tie order
and cycle diagnostics, including delay boundaries. Port validation uses
the resolved input once for compatibility and sensitivity. Expression
checks borrow declared types and format the diagnostic path on failure.

Replay capture uses the revision digest produced by proof. Replay
verification checks the capsule, revision, and result digests in the
existing order. Replay errors now expose an embedded evaluation failure
as their standard error cause. A scoped Clippy expectation keeps the
public error layout and its inline evaluation failure.

All 135 Karma tests and 18 Store program and run tests passed. The four
replay tests also passed with insertion-ordered JSON enabled. A direct
comparison matched 4,690 serialized proof reports and the replay probe's
capsules, results, and failure diagnostics. Proof allocation and
reallocation requests went from 847,297 to 720,182 on those cases.
Rustdoc and default formatting checks passed. Clippy reported four
existing findings outside the reviewed files.

Across production and the replay test, formatted nonblank lines went
from 1,603 to 1,509 and non-whitespace characters from 36,833 to 36,785.
Production alone went from 1,354 to 1,254 nonblank lines and from 29,924
to 29,641 characters. The Karma library's production macro expansion
went from 88,761 to 88,791 nonblank lines.

[Proof API](../../../crates/karma/src/program/proof.rs) · [Dependency order](../../../crates/karma/src/program/proof/graph.rs) · [Replay](../../../crates/karma/src/program/replay.rs)

### Karma deadline dispatch and elapsed occurrence batches

The dispatcher separates admission, exact work-rate accounting, deadline
indexing, and elapsed occurrence wire types. Both planners use the same
host and lane selection in stable deadline order. Demand admission checks
the first exceeded resource directly. The index counts members per lane
and checks heap keys against active entries without cloning the activation
digest. Its debug output now shows lane counts. Same-lane replacements
and stale heap entries retain their prior dispatch behavior.

Occurrence construction checks elapsed boundaries without a panicking
conversion. The workload and tick deserializers use Serde conversion
attributes with their existing wire types, retaining the diagnostics for
malformed values. The replacement test now checks equal-key rearming,
lane counts, and stale entries discarded by a due wake.

The baseline comparison matched 6,603 planning, dispatch, and occurrence
records and 16 focused wire cases. Allocation and reallocation requests
went from 14,748 to 10,367 in the planning probe and from 5,318 to 2,089
in the index probe. All 135 Karma tests passed by name. Store passed its
21 schedule, occurrence, and run tests. Rustdoc and default formatting
checks passed. Clippy reported three existing findings elsewhere in Karma.

Production went from 1,409 to 1,398 formatted nonblank lines and from
34,875 to 36,677 non-whitespace characters. Excluding the added Rustdoc,
production has 1,336 nonblank lines and 33,989 characters. With the
dispatcher test, the totals went from 1,877 to 1,866 lines and from
47,416 to 49,592 characters. The Karma library's production macro
expansion went from 88,791 to 88,914 nonblank lines.

[Dispatcher API](../../../crates/karma/src/scheduling/dispatcher.rs) · [Admission](../../../crates/karma/src/scheduling/dispatcher/admission.rs) · [Deadline index](../../../crates/karma/src/scheduling/dispatcher/deadline.rs)

### Karma Frequency compiler

Frequency keeps its public wire types together and separates parameter
binding from elapsed and calendar schedule compilation. Override validation
checks type and range in one match. Parameter paths are built only for
diagnostics, and calendar step paths are static. Error display writes the
existing kind names directly. The calendar variant stays inline so existing
callers can construct it and match it with the same field types.

The baseline comparison matched 15,960 elapsed and calendar compile results,
including serialized schedules, hashes, error messages, and error paths.
All three JSON Schema snapshots matched. Allocation and reallocation requests
went from 122,658 to 118,479 across the successful cases and from 70,348 to
30,806 across the error cases.

All 135 Karma tests matched the baseline by name. Store passed its eight
Frequency and schedule tests, and Presentation Models passed 22 tests.
Rustdoc and default formatting checks passed. Clippy reported two existing
findings in cadence search and run execution.

With default rustfmt, the three production files total 646 nonblank lines
and 16,626 non-whitespace characters, down from 665 and 16,775. Karma's
production macro expansion rose from 88,914 to 88,934 nonblank lines.

The follow-up generates binding validation and named cadence construction
from one field list. It documents the two compiled hashes and tests all
eight units and first-error ordering. The 48-case comparison matched
compiled results, diagnostics, and allocation requests case by case.
Three JSON Schema snapshots also matched byte for byte. The three
production files now total 652 nonblank lines and 17,027 non-whitespace
characters. Current Karma expansion went from 89,203 to 89,258 nonblank
lines for this follow-up.

[Frequency types](../../../crates/karma/src/scheduling/frequency.rs) · [Parameter bindings](../../../crates/karma/src/scheduling/frequency/bindings.rs) · [Schedule compilation](../../../crates/karma/src/scheduling/frequency/compile.rs)

### Karma civil calendar and timezone resolution

The calendar module re-exports civil values, timezone providers, and
schedule advancement at the existing public paths. Schedule advancement
shares one UTC ordering check across unique, shifted, and folded boundaries.
It validates prior boundaries and keeps the original provider call order
and search budgets. The provider and advancement remain together in Karma
because their contracts use Karma cadence, timer, and error types.

Civil time parses fixed fields without collecting or formatting a temporary
string. Serialization streams both civil time formats through their Display
implementations. Scalar and weekday wrappers use transparent serialization,
with explicit schema attributes preserving the named schema definitions.

The baseline comparison matched 95,258 records covering all 86,400 clock
seconds, malformed input, gap and fold policies, previous boundaries, provider
calls, five JSON Schema snapshots, and legacy rule serialization and hashes.
Civil-time parse allocation and reallocation requests fell from 263,916 to
2,928. The remaining requests in the probe come from error messages.

All 135 Karma tests matched the baseline by name. Nine focused Store tests
and 22 Presentation Models tests passed. Rustdoc and default formatting
checks passed. Clippy reported two existing findings elsewhere in Karma.

With default rustfmt, the four production files total 811 nonblank lines
and 19,188 non-whitespace characters, down from 859 and 19,657. Karma's
production macro expansion stayed at 88,934 nonblank lines.

[Calendar API](../../../crates/karma/src/scheduling/calendar.rs) · [Civil values](../../../crates/karma/src/scheduling/calendar/civil.rs) · [Timezone providers](../../../crates/karma/src/scheduling/calendar/timezone.rs) · [Schedule advance](../../../crates/karma/src/scheduling/calendar/advance.rs)

### Karma cadence recurrence and UTC queries

Cadence keeps the existing rule types and construction API. Indexed civil
recurrence and UTC queries share the checked step size, month resolution,
weekday landing, and bound handling. The forward scan counts valid candidates
once for counted monthly rules with skipped dates. Civil membership computes
the candidate count at its starting point and carries it through the scan.
Next-occurrence queries without weekday landing stop at the first match.
Queries with landing retain the sorted, deduplicated window because landing
can reorder candidates.

The baseline comparison matched 19,826 query, schema, and serialization records.
Three cases at Chrono's limits previously panicked. The range lookback now
clamps at the minimum date. Landing beyond the maximum is treated as exhaustion
for forward and indexed queries, and the predecessor query can return an
earlier representable match. A focused regression test covers both limits.

In the same debug probe, 40 counted monthly range and predecessor pairs took
181.7 ms before and 2.84 ms after. Forty millisecond next-occurrence queries
without weekday landing went from 13.39 ms to 0.023 ms.

All 135 baseline Karma tests passed by name, along with the new limit test.
Eight Store tests and 22 Presentation Models tests passed. Rustdoc and default
formatting checks passed. Clippy reported one existing finding in run execution.

Production went from 594 to 608 formatted nonblank lines and from 13,253 to
13,649 non-whitespace characters, including the shared scan and limit handling.
With the regression test, the new total is 645 lines and 14,421 characters.
Karma's macro-expanded production went from 88,934 to 88,971 nonblank lines.

[Cadence API](../../../crates/karma/src/scheduling/cadence.rs) · [Indexed recurrence](../../../crates/karma/src/scheduling/cadence/recurrence.rs) · [UTC range queries](../../../crates/karma/src/scheduling/cadence/range.rs)

### Karma elapsed schedules and exact rates

Elapsed scheduling separates timer policy, exact rates, occurrence ranges,
the schedule and cursor, and advancement. Existing public paths re-export
those types. The interval carries its nonzero invariant. Advancement derives
lateness from one timestamp difference and partitions the validated due range
without rechecking both endpoints of each part. Lag pauses retain their input
cursor. Exact rate multiplication keeps the existing checked overflow behavior
and skips a second reduction after cancelling the multiplier.

Serde uses validating conversions and retains the existing struct names and
field order. The comparison matched 235,875 baseline records, including rate
arithmetic, policy outcomes, timestamp limits, serialized values, hashes,
schema, and malformed-wire diagnostics. Across three debug samples, the
median for 200,000 replay advances went from 67.1 ms to 37.6 ms.

All 136 Karma tests matched the baseline by name. The 24 focused Store tests
and 22 Presentation Models tests passed. Default formatting and Rustdoc checks
passed. Clippy reported one existing finding in run execution.

With default rustfmt, the six files total 699 nonblank lines and 17,371
non-whitespace characters, down from 740 and 17,821. Karma's macro-expanded
production rose from 88,971 to 89,100 nonblank lines with the Serde derives.

[Schedule API](../../../crates/karma/src/scheduling/schedule.rs) · [Elapsed cursor](../../../crates/karma/src/scheduling/schedule/elapsed.rs) · [Advancement](../../../crates/karma/src/scheduling/schedule/elapsed/advance.rs) · [Occurrence ranges](../../../crates/karma/src/scheduling/schedule/occurrence_range.rs)

### Karma timezone artifacts and provider lookup

The artifact module owns canonical wire data, with separate zone validation
and provider modules. Local resolution searches a bounded UTC slice using
the validated maximum offset and visits candidate instants in UTC order.
Gap resolution finds both neighboring local endpoints in one pass. The
provider computes offset spread and the minimum fold width in one pass.
Artifact serialization borrows the zones, and canonical loading reuses the
verified bytes for the revision hash through the shared canonical hash code.

The comparison matched 20,448 records covering timestamps at both supported
limits, extreme offsets, gaps, folds, malformed artifacts, and revisions.
A focused regression test covers the inclusive and exclusive endpoints of
the lookup window. In a debug fixture with 12,001 segments, 3,273 unique
lookups took 471 ms before and 3.0 ms after, with allocation requests falling
from 3,273 to zero. Canonical provider loading went from 264,100 to 156,055
allocation and reallocation requests.

All 136 baseline Karma tests passed by name, along with the new lookup test.
The three bundled Nucleus timezone tests, eight Store tests, and 22
Presentation Models tests passed. Nine focused tests also passed with
insertion-ordered JSON enabled. Rustdoc and formatting checks passed. Clippy
reported one existing finding in run execution.

Across the three artifact files and the shared hash file, production went
from 613 to 573 formatted nonblank lines and from 16,302 to 16,279
non-whitespace characters. With the regression test, the new total is 608
lines and 17,121 characters. Karma's production macro expansion went from
89,100 to 89,105 nonblank lines.

[Artifact API](../../../crates/karma/src/scheduling/timezone_artifact.rs) · [Zone resolution](../../../crates/karma/src/scheduling/timezone_artifact/zone.rs) · [Provider](../../../crates/karma/src/scheduling/timezone_artifact/provider.rs)

### Karma calendar catch-up and occurrence envelopes

Calendar runtime separates cursor verification, catch-up policy, and durable
occurrence records. Catch-up reuses the verified cursor's discontinuities,
consumes the skipped lists from due cursors, and searches the ordered due
instants for the lateness cutoff. It retains the existing budget, provider
call order, and lag and discontinuity pause behavior.

Occurrence envelopes keep source validation with the derivation of logical
time. Calendar tick and batch wire types have their own module. Envelope
construction and deserialization validate the typed source directly. Identity
methods still hash the same canonical payload when called.

The baseline comparison matched 83,048 calendar catch-up records and 816
occurrence envelope records, including provider calls, hashes, and wire
diagnostics. The debug probe counted 2,760 to 2,400 allocation and reallocation
requests for 120 calendar replay batches and 57,300 to 300 requests for
100 calendar envelope constructions with 50 boundaries each.

All 137 Karma tests matched the baseline by name. The 22 Presentation Models
tests and 21 Store schedule, occurrence, and run tests passed. Rustdoc and
default formatting checks passed. Clippy reported one existing finding in run
execution.

Default rustfmt gives 625 to 620 nonblank lines and 16,337 to 17,421
non-whitespace characters across the two sources and their successors.
Karma's production macro expansion went from 89,105 to 89,151 nonblank lines.

[Calendar runtime](../../../crates/karma/src/scheduling/calendar_runtime.rs) · [Catch-up](../../../crates/karma/src/scheduling/calendar_runtime/catch_up.rs) · [Occurrence envelopes](../../../crates/karma/src/scheduling/occurrence.rs)

### Karma Program execution records

Execution separates frozen epochs, node state events, candidate records,
and runs. The public module re-exports the existing types. Run validation
uses the replay module's canonical seal hash on the borrowed capsule.
Epoch members share validation between construction and epoch verification.
Execution records validate Program UIDs through the same borrowed check
used by the public typed UID constructor.

The comparison matched 655 records covering replay seals, run and epoch
hashes, state chains, candidate review schema, and validation diagnostics.
For 120 successful-run validations, allocation and reallocation requests
went from 71,520 to 60,120. For 120 one-member epoch validations, requests
went from 120 to zero.

All 137 Karma tests matched the baseline by name. The 22 Presentation Models
tests and 18 Store tests passed. Default formatting, Rustdoc, and all-target
Clippy checks passed for Karma.

The public run outcome retains inline fields, with a scoped Clippy
expectation for its variant sizes. Across execution, replay, and references,
formatted production went from 944 to 992 nonblank lines and from 24,060
to 25,503 non-whitespace characters. Karma's production macro expansion
went from 89,151 to 89,229 nonblank lines.

[Execution API](../../../crates/karma/src/program/execution.rs) · [Runs](../../../crates/karma/src/program/execution/run.rs) · [Epochs](../../../crates/karma/src/program/execution/epoch.rs)

### Karma Frequency activation evidence

Frequency activations live beside the Program and Frequency mutation
evidence. The compiled Frequency owns the revision hash, parameter hash,
and effective parameter map. The activation exposes those fields by
reference and writes the existing wire and debug fields from that same
data. Decoding checks that the repeated wire fields agree and verifies
the parameter digest before retaining the compilation.

All 2,479 baseline records matched, including JSON bytes, hashes, debug
output, and malformed activation diagnostics. Postcard serialization matched
the original field layout with and without a previous activation. On the
local target the activation struct went from 416 to 344 bytes. The debug
probe counted 22,500 to 11,300 allocation and reallocation requests for
100 clones with 100 parameters each. Construction of 100 activations in
the same fixture went from 85,700 to 74,400 requests.

All 137 Karma tests matched the baseline by name. Eleven Store tests and
22 Presentation Models tests passed. Default formatting, Rustdoc, and
all-target Clippy passed for Karma.

Default rustfmt gives 246 to 289 nonblank lines and 7,777 to 9,459
non-whitespace characters across the two files. The explicit wire and
debug implementations preserve the existing representation. Karma's
production macro expansion went from 89,229 to 89,200 nonblank lines.

[Mutation evidence](../../../crates/karma/src/program/durable.rs) · [Frequency activation](../../../crates/karma/src/program/durable/activation.rs)

### Karma value types and exact decimals

The value model separates decimal arithmetic, literal typing, and port and
value contracts. Public names and schema definitions remain available at
their existing paths. Decimal rounding shares one adjustment calculation,
and cross-scale comparison and multiplication share their symmetric cases.
The existing checked overflow behavior and canonical decimal text are preserved.

Datum typing retains the inferred inner type after checking it against the
declared type. The nested Datum probe counted 30,000 to 2,400 allocation
and reallocation requests for 100 type derivations at depth 24.
The baseline comparison matched 27,217 records covering arithmetic, errors,
wire values, and the five schema snapshots.

All 137 Karma tests matched the baseline by name. The 22 Presentation Models
tests and 18 Store tests passed. Default formatting, Rustdoc, and all-target
Clippy passed for Karma.

Default rustfmt gives 661 to 638 nonblank lines and 13,902 to 13,919
non-whitespace characters across the four files. Karma's production macro
expansion went from 89,200 to 89,199 nonblank lines.

[Value API](../../../crates/karma/src/model/value.rs) · [Exact decimals](../../../crates/karma/src/model/value/decimal.rs) · [Literal typing](../../../crates/karma/src/model/value/literal.rs) · [Value and port contracts](../../../crates/karma/src/model/value/types.rs)

### Karma fixed decimals and unit intervals

Fixed and dynamic decimals share a formatter that writes into the caller's
buffer. Parsing checks the canonical text before accumulating signed digits
without building a concatenated string. The accumulation handles both signed
mantissa limits and preserves overflow and invalid-text diagnostics. The
unit interval module shares parsing and formatting between Probability and
Confidence. Serde retains the decimal object and scalar-string wire shapes.

The baseline comparison matched 1,487 records, including four schema
snapshots, canonical text, hashes, and invalid inputs. Twelve Postcard
encodings matched and round-tripped. The debug probe counted 500 to zero
allocation and reallocation requests for 400 fixed decimal parses. For 100
triples each of decimal and unit interval formatting and JSON serialization,
requests went from 600 to zero in each probe.

All 137 baseline Karma tests passed by name, along with the signed-limit
regression test. The 22 Presentation Models tests and 18 Store tests passed.
Default formatting, Rustdoc, and all-target Clippy passed for Karma.

Production across the three affected files went from 508 to 497 formatted
nonblank lines and from 12,099 to 12,305 non-whitespace characters. The new
signed-limit regression test brings the total to 520 lines and 12,928
characters. Karma's production macro expansion went from 89,199 to 89,124
nonblank lines.

[Fixed decimals](../../../crates/karma/src/model/exact.rs) · [Unit intervals](../../../crates/karma/src/model/exact/unit_interval.rs) · [Dynamic decimals](../../../crates/karma/src/model/value/decimal.rs)

### Karma Program AST and capability vocabulary

The Program AST groups node operations and sources, expressions and decimal
precision, and state contracts in separate modules. The public paths remain
available through explicit re-exports. Expression input discovery inserts an
owned identifier only for the first occurrence. Capability family names use
Serde's kebab-case rule, and the namespaced capability identifiers retain their
individual wire names.

All 60 baseline records matched, including six schema snapshots, the
comprehensive Program fixture, and the family of each of the 37 capabilities.
The debug probe went from 204,900 to 200 allocation and reallocation requests
for 100 input-set derivations with 2,048 references to the same input. The
regression test covers inputs in each conditional branch and duplicate leaves.

Local verification passed 139 Karma tests, 22 Presentation Models tests,
and 18 Store Program and Run tests. Default formatting, Rustdoc, and
all-target Clippy passed for Karma.

Production across the five files went from 503 to 518 formatted nonblank
lines and from 11,726 to 12,248 non-whitespace characters. With the
regression test, the new total is 542 lines and 12,874 characters. Karma's
macro expansion went from 89,124 to 89,169 nonblank lines.

[Program AST](../../../crates/karma/src/program/ast.rs) · [Expressions](../../../crates/karma/src/program/ast/expression.rs) · [Capabilities](../../../crates/karma/src/model/capability.rs)

### Karma timestamps, failure paths, and lifecycle labels

Timestamp parsing keeps Chrono's calendar validation and the original error
order, then checks the canonical spelling without formatting a second string.
Checked addition tests the supported year range without constructing an error.
The comparison probe measured no allocation requests across 100,000 valid
parses or across 100,000 out-of-range additions. Each baseline loop requested
100,000 allocations. Formatting and serialization still use Chrono's formatter.

Failure paths validate Unicode controls and JSON Pointer escapes in one pass.
Transparent serialization emits the same strings and deserialization still
validates them. Lifecycle labels retain their direct matching and Serde tags.
The 25,236 baseline comparison records matched, including schema snapshots,
formatted timestamps, JSON representations, boundary errors, and the result
of parsing all pointer strings up to four characters over the test alphabet.

Local verification passed 141 Karma tests, 22 Presentation Models tests, and
27 Store tests across Programs, Runs, Frequencies, Occurrences, and Schedules.
Default formatting, Rustdoc, and all-target Clippy passed for Karma. Karma's
remaining markers are in integration tests.

Production goes from 8,488 to 9,242 formatted non-whitespace characters and
from 379 to 415 nonblank lines. The fuller code documents the accepted
spellings and uses an explicit canonicality check. With regressions for leap
seconds, alternate spellings, and pointer escapes, the total is 10,068
characters and 457 lines. Karma's production macro expansion goes from
89,169 to 89,203 nonblank lines.

[Timestamps](../../../crates/karma/src/model/time.rs) · [Failure paths](../../../crates/karma/src/model/failure.rs) · [Lifecycle labels](../../../crates/karma/src/model/state.rs)

### Ergon publication integration suite

The `ergon` integration target keeps all 19 original test names, including
the Hephaestus name used with `--exact`. The subject modules keep their own
fixtures and assertions. Shared helpers handle loading, fresh-library
reopening after relocation, local link targets, and graph reachability.
Reachability indexes successors once and borrows node IDs during traversal.
Hephaestus stores its route sequences by subject and checks embedded program
contents through a table of Records, filenames, and expected text.

The comparisons matched all 19 test names, 119 Record path literals, 56
route sequences with 631 adjacent pairs, and 94 program cases. The full local
Ergon baseline and candidate runs are pending. Leave the suite markers in
place until their runtime results can be compared. The current run logs are
under `target/ergon-refactor/` as `baseline.log` and `candidate.log`.

The export change compiles the Markdown and wiki link patterns once per
export. Both rewrites use their original patterns and preserve failure
handling. All 10 publication tests passed before and after the change,
including offline media, export relocation, wiki links, and transclusions.
Default formatting and the `ergon` target's Clippy check passed after the
change. The export file retains its marker for the rest of its review.

The test suite went from 7,661 to 5,611 formatted nonblank lines, from
189,539 to 143,642 non-whitespace characters, and from 8,699 to 6,967
macro-expanded nonblank lines. Export adds one nonblank line and 18
non-whitespace characters. The Resources library's expansion goes from
8,957 to 8,956 nonblank lines with publication enabled.

[Ergon tests](../../../crates/resources/tests/ergon.rs) · [Shared fixture helpers](../../../crates/resources/tests/ergon/fixture.rs) · [Publication export](../../../crates/resources/src/publication/export.rs)

### Deferred authority decisions

Run the in-memory authority suite with `cargo test -p protein-authority --test authority`.

The Protein authority split preserved the baseline. Three assertion-intent
tests expect an entire mutation to be covered by one matching grant, and one
expects text predicates to be rejected in mutation selectors. The evaluator
combines matching grants and accepts text predicates, so those four assertions
fail before and after the refactor. Confirm the intended grant and selector
contracts before changing authorization behavior.
[Assertion grant tests](../../../crates/protein-authority/tests/authority/intents/grants.rs) · [Assertion visibility tests](../../../crates/protein-authority/tests/authority/intents/visibility.rs) · [Mutation evaluation](../../../crates/protein-authority/src/authority/mutation/record.rs) · [Selector compilation](../../../crates/protein-authority/src/authority/selector.rs)

### Deferred Record sorting decision

Record sorting loads assignment names for `assignee_name`. An order containing
only `assignee` falls back to the default order. The refactor preserves those
results. Confirm whether `assignee` should independently sort by assignment name
before changing the query contract.
[Record sort keys](../../../crates/protein/src/record_order/keys.rs)

### Deferred scoped-link decision

A scoped Record query currently includes link metadata for an edge between
a visible Record and an endpoint outside the reader's Record scope. A
Store-backed comparison confirmed the same one-hop output before and after
the refactor. Confirm the intended disclosure for those references before
changing which edges can be traversed or returned.
[Record link projection](../../../crates/protein/src/record_links/mod.rs)

### Deferred Assertion catalog visibility decision

A scoped Assertion catalog read currently returns metadata for Assertions
whose subject Record is outside the reader's Record scope. A Store-backed
comparison confirmed this result before and after the refactor. Confirm
which Assertion metadata should be visible to a scoped reader before
changing the catalog's visibility.
[Assertion catalog projection](../../../crates/protein/src/sources/vocabulary/assertions.rs)

### Deferred Promise limit decision

With `limit: 0`, a Promise query returns the first matching row when there is
one. An Entry query returns an empty list. The refactor preserves both results.
Confirm the Promise limit contract before changing when the limit is checked.
[Promise selection](../../../crates/protein/src/sources/promises.rs)

## Historical inventory and byte-sensitive exceptions

The source inventory at `0f564d91` included 3,424 Rust files. The initial syntax
and Clippy scan added 3,403 markers and collected provisional findings. Header
line references use the source before the marker was inserted. Workspace Clippy
at that revision stopped at unresolved Engine dispatch inputs. An archived
probe supplied some additional diagnostics for files whose source matched.
Use these findings as optional leads while editing the current source. Review
the paths below alongside the marked files. Their exact bytes prevented a
marker. Defer any edit that would break a digest or a generated-source contract.

<details>
<summary>Byte-sensitive exceptions and their review queue</summary>

These firmware and protocol files are copied verbatim into the Ailuros starter
and contribute to its source digest. The review uses the original line numbers.

- `crates/ailuros/firmware/iroh/build.rs`
  - The target-OS guard at 2–3 keeps host builds independent of ESP-IDF metadata.
    The build hook has a single responsibility and the current fall-through is intentional.
- `crates/ailuros/firmware/iroh/examples/device_check.rs`
  - **§4 Types, §6 Idioms** at 6–19, 29–33. Parse the two positional arguments from
    the iterator and represent direct and relay qualification with a named mode.
    The current length guard proves both indexes. Preserve the usage errors, the
    selected-path check, and the report's path strings.
- `crates/ailuros/firmware/iroh/src/bin/camera.rs`
  - **§6 Idioms** at 39–48. Fetch the bound endpoint once after updating the network.
    Use the update result to decide whether to report its addresses and reset backoff.
    The three SDK calls have local initialization and lifetime proofs at 7–8, 22–23, and 54–55.
- `crates/ailuros/firmware/iroh/src/bin/transport_check.rs`
  - **§3 Ownership** at 22, 67–71. Spell the required shared counter clone as
    `Arc::clone` and move the device secret into the endpoint after readiness.
    Keep the PSRAM placement check and the exact operation count.
  - **§5 Errors, §7 Concurrency** at 20–52. Join all started workers on both spawn
    and join failures. The early error returns can leave another worker detached.
    Preserve the qualification failure and avoid emitting a raw panic payload.
- `crates/ailuros/firmware/iroh/src/client.rs`
  - **§3 Ownership** at 131–147. `control` only reads the frame kind and payload.
    Borrow the frame or decode directly into the result before dropping it. The
    owned inputs to `send` and `demand` are consumed or retained.
  - **§4 Types, §7 Async, §10 Documentation** at 20–27, 65–99, 290–310.
    Document the camera state transitions, frame bounds, and future's cleanup
    contract. Add redaction-safe `Debug` for public data types where useful.
    Keep the wire booleans and make the meaning of the activity flag clear at calls.
  - **§3 Ownership, §12 Performance** at 150–159, 204–215, 274–285. Every pending
    frame copies the epoch string. Share one immutable epoch per demand generation
    if a measurement justifies it. Pending frames must keep the generation in
    which they were sent for receipt validation.
  - **§5 Errors** at 306–310. The cleanup timeout can replace a session error,
    and a completed cleanup error can be discarded. Retain the primary failure
    and cleanup context in diagnostics while preserving disconnect and shutdown behavior.
- `crates/ailuros/firmware/iroh/src/device.rs`
  - **§8 FFI** at 19–32. Wrap the owned `cJSON_PrintUnformatted` result in a
    guard that calls `cJSON_free` so the pointer lifetime spans parsing and
    cleanup is automatic. The configuration tree comes from shared C startup
    and needs a separate lifetime decision.
  - **§6 Idioms, §5 Errors** at 60–72, 236–246, 281–310. Validate the frame rate
    before starting a camera worker. Preserve both session and cleanup failures
    in diagnostics. Return the select loop result directly from `publish_session`
    while keeping the connection guard alive through stream cleanup.
  - **§8 FFI** at 146–163, 200–208. Use an explicit `c_char` array type and
    group resource counters by their SDK preconditions. Keep network-generation
    checks and the local safety proofs for provisioned values and output buffers.
- `crates/ailuros/firmware/iroh/src/device/camera.rs`
  - **§5 Errors, §7 Concurrency** at 79–83, 148–177. Keep both camera-open and
    rollback causes. Return the C JSON parse failure through the worker's error
    path. Today a null parse makes the newly spawned worker exit silently.
  - **§8 FFI** at 107–115, 136–142. The C capture function writes the entire
    frame on success. Consider `MaybeUninit` with initialization documented at
    the successful branch, or keep the existing zero-valid struct initialization.
    The copy into `Bytes` gives the async caller ownership after the SDK frame is returned.
    Keep hardware ownership on the dedicated worker and preserve the single-slot queue.
- `crates/ailuros/firmware/iroh/src/lib.rs`
  - **§10 Documentation** at 10–31. Describe which entry point builds the configured
    provider and which binds a live endpoint. State the binding errors and identity
    ownership. `Arc<CryptoProvider>` and the owned secret fit the Iroh builder contract.
  - **§11 Tests** at 56–97. Give the spawned accept task the same timeout and
    cleanup coverage as the connection attempt. Preserve the test's distinction
    between a wrong identity and failure to start the local endpoint.
- `crates/ailuros/firmware/iroh/src/network.rs`
  - **§4 Types, §7 Async** at 24–28, 35–78. Model the current snapshot and the
    endpoint as one optional active-network value, or document the coupling.
    Preserve retry after a failed open, disposal of the old endpoint, and
    identity validation before publication. The key clone at 62 is needed
    because the builder consumes a key and reconnects reuse the caller's identity.
- `crates/ailuros/firmware/iroh/src/power.rs`
  - **§4 Types** at 8–42. Model idle, active, and restore-pending as explicit
    states. `previous = Some` with `enabled = false` carries a saved mode for
    retry after a failed rollback. Keep that state and the five recovery tests.
  - **§5 Errors** at 30–32, 48–50. Retain rollback failure as diagnostic context
    while returning the original enable error. Keep the fallible explicit
    `disable` path and best-effort restoration on drop.
- `crates/ailuros/firmware/iroh/src/readiness.rs`
  - **§4 Types, §10 Documentation** at 9–21. Name the connection and clock
    inputs at the call site and document the retained synchronization bit.
    The two flags are independent. Preserve lazy consumption of SDK sync
    status, its disconnected guard, and the latch across reconnects.
- `crates/ailuros/firmware/iroh/src/verification.rs`
  - **§4 API** at 8–9, 142–180. Name the cache-statistics fields and retain the
    existing tuple shape at the report boundary. Narrow the algorithm-table
    factory to the built-in provider, or document its fixed-input contract.
    The `OnceLock` uses the first caller's table. Preserve table order, schemes,
    and the deliberate process lifetime of the generated slices.
  - **§5 Panics, §6 Idioms** at 185–195, 220–235. Decode hex through bytes or
    checked chunks so malformed fixture text becomes a qualification error.
    Even byte length alone leaves a risk of indexing across a UTF-8 boundary.
    The good-signature check establishes the lengths before the later slicing
    and mutation. Move the signature into the trailing-data case after its last
    read, or reserve one extra byte in the qualification buffer.
  - **§8 FFI** at 63–87. Keep the borrowed slice lengths, synchronous C call,
    and FreeRTOS yield. The cryptographic work occurs outside the cache lock.
- `crates/ailuros/firmware/iroh/src/verification/cache.rs`
  - **§6 Idioms** at 47–55. Combine position and removal with a let chain while
    preserving one hit or miss and moving a hit to the most-recent position.
  - **§5 Errors, §7 Concurrency** at 77–93, 115–119. Document the intentional
    generic refusal on poison and absent statistics. The verifier callback
    runs after the lookup guard drops. Retain exact input equality, success-only
    insertion, and the existing bounded cache on both paths.
- `crates/ailuros/firmware/iroh/tests/camera_client.rs`
  - **§3 Ownership** at 91–113. Express the shared fixture clones with
    `Arc::clone`. `Receiver::send` only serializes the test value, so a borrowed
    parameter can make temporary ownership clearer.
  - **§11 Tests** at 125–131, 152–165, 210–240, 306–328, 382–414. Bound both
    halves of the metadata/data read. Replace observation sleeps with the
    existing paused-time or explicit fixture acknowledgement approach where
    possible. Keep the panic-on-poison fixture allowances and the exact tests
    for frame count, byte budget, and stale receipts.
- `crates/ailuros/firmware/iroh/tests/certificate_verification.rs`
  - **§4 Types, §11 Tests** at 94–103, 140–165. Name trust and validity at the
    handshake call sites. Keep fixture panics and deadline failures distinct
    from certificate rejection. Carry the negative-case error through to an
    assertion where possible, and give the spawned listener a deadline and cleanup.
- `crates/ailuros/firmware/iroh/tests/public_relay.rs`
  - **§7 Async, §11 Tests** at 21–45. Bring the accept task into the same
    lifetime as the connection attempt and ensure both endpoints close on
    timeout. Keep the opt-in live-relay test and the verified identity and
    transport assertions.
- `crates/ailuros/protocol/src/lib.rs`
  - **§4 Types, §10 Documentation** at 6–16, 32–47, 54–82. Document the frame
    kind bytes, phase transition, per-kind size limits, and which errors are
    returned before consuming a frame. Keep the closed wire discriminants and
    audited enum-to-byte cast.
  - **§5 Panics** at 92–108, 120–125, 148–152. The five-byte guard covers the
    header slice and both reads. Validation bounds the reservation before the
    header is consumed. The test slices use a nonempty fully encoded frame.
    Preserve partial-read and no-partial-write behavior. The conversion errors
    are intentionally classified as `InvalidData` at the protocol boundary.

The following sources contribute exact bytes to persisted or reported digests.

- `crates/nucleus/src/karma/timezone_builtin.rs`
  - **§5 Panics, §10 Documentation** at 17–32, 37–45, 94–105. Use
    `NonZeroU64::MIN` for the one-millisecond lower bound. Document construction
    and the normalized unknown-zone error. Preserve timezone revision identity,
    UTC-provider routing, and the gap and fold boundary semantics.
- `crates/nucleus/src/simulation/checks.rs`
  - **§3 Ownership, §4 Types** at 30–43, 93–103. `Evaluation` contains only
    scalars and unit variants. Evaluate deriving `Copy` for the closed type and
    returning the value directly, or borrow the evaluation when the caller
    only inspects it. Preserve the tagged serialization and default schedule selection.
  - **§5 Panics, §10 Documentation** at 113–141. The scenario validator bounds
    timestamps and rejects zero intervals. Document that precondition or use
    checked/wider intermediate math for arbitrary callers of `next_time`.
    Both the initial subtraction and the increment precede the checked multiplication.
- `crates/simulation/src/checks.rs`
  - **§4 Types, §5 Panics** at 14–22, 54–109, 123–154, 468–480. Keep each
    check's specification, runtime, coverage, and cost together to encode the
    equal-length indexing invariant. The validated scenario establishes valid
    windows and nonzero sampling intervals. Retain order, verdict precedence,
    and per-check budgets.
  - **§3 Ownership, §6 Idioms** at 86–91, 285–337, 427–460. Read the interval once.
    Borrow observed refusal payloads until an actual witness needs ownership.
    Use a lazy genesis fallback so checking an existing fact chain avoids creating
    an unused string. Preserve first-match order and any evidence snapshot copies.
  - **§12 Performance** at 285–348, 619–631. Repeated refusal expectations can
    rescan the same trace prefix, including a queued-message scan per expectation.
    Measure a per-input index that preserves the selected copy and first
    observation. At 274–277, the first-quantity access is guarded by the
    iteration itself. Express it with a first/rest comparison if clearer.
- `crates/simulation/src/checks/quantity.rs`
  - **§3 Ownership** at 97–99, 104–168, 180–192. Keep expected quantities
    borrowed until constructing an owned witness. Batch entries can borrow
    the first and last observation values and clone only on a recorded witness
    or retained state. The eager `or_insert` currently clones even when a
    batch already exists. Preserve the final sample and gap detection.
  - **§4 Types** at 116–141. Use distinct commit and event key variants instead
    of the boolean tag and the signed commit cast. Retain sorting by the last
    event's sequence. The trace cursor indexes an append-only trace and is
    assigned from its length at the call sites.

</details>

[writing]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-best-practices/SKILL.md
[review]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-review/SKILL.md
[levels]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/lints/lints.toml
[settings]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/lints/clippy.toml
[bevy]: https://github.com/morganlinton/BeautifulRust/blob/b9718436d77ee7bcf17cdcb6aa27628feb95d97a/skills/rust-best-practices/references/bevy.md
[usage]: https://doc.rust-lang.org/stable/clippy/usage.html
[cargo-lints]: https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table
[clippy-config]: https://doc.rust-lang.org/clippy/lint_configuration.html
[build-timings]: https://doc.rust-lang.org/stable/cargo/reference/timings.html
[macro-expansion]: https://rustc-dev-guide.rust-lang.org/test-implementation.html
[unwrap-lint]: https://clippy.rust-lang.org/master/index.html#unwrap_used
[lint-attributes]: https://doc.rust-lang.org/reference/attributes/diagnostics.html#lint-check-attributes
[clippy-add-lints]: https://doc.rust-lang.org/clippy/development/adding_lints.html
[dylint]: https://github.com/trailofbits/dylint
[safety-lint]: https://clippy.rust-lang.org/master/index.html#undocumented_unsafe_blocks
[extra-safety-lint]: https://clippy.rust-lang.org/master/index.html#unnecessary_safety_comment
