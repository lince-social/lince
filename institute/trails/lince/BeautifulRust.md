# Lince Rust refactoring plan

`// non-lean-yet` means the file still needs the complete refactor. Review
and fix every applicable BeautifulRust issue, improve the file's structure,
and simplify the code line by line in the same pass. This plan is the shared
checklist. Use the existing markers as the work queue and start editing as
soon as you review each file.
Sensei's shared registry allows regular source comments and refactor markers.
[Sensei rule registration](../../../crates/sensei/src/rules/mod.rs)

Review every repository Rust file, including tests, examples, build scripts,
firmware, plugins, standalone crates, vendored sources, and checked-in generated
code. Preserve Lince's observable behavior, runtime performance, and readability.
Keep the existing CI, stable toolchain pin, release profiles, and ordinary entry
points. Keep Cargo output under `target/`. Skip language ports.
[Build Record](Build.lingua)

The first demonstration covers Protein and its split crates, including their
tests. Finish that scope before resuming the wider rollout.

## One complete refactor per file

1. **Open one file and trace its responsibility.** Start with crates most
   likely to split and the files that define their boundaries. Read the whole
   file, trace callers and enabled features, and identify the behavior to
   preserve. Take any unmarked pending file from the inventory straight into
   the same workflow.
   [Lince Records](Lince.lingua)
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

## Guidelines to check in every file

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
  imports readable. Keep qualifiers that convey origin or disambiguate. Keep
  glob imports for established framework, test, and local enum-match cases.
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

## BeautifulRust rules and enforcement

Use the pinned [writing skill][writing], [review skill][review], [lint levels][levels],
[Clippy settings][settings], and [Bevy reference][bevy] at
`b9718436d77ee7bcf17cdcb6aa27628feb95d97a`. The tables below govern changes to
shared lint enforcement. Apply the per-file checklist during the refactor
regardless of which lints currently enforce it.

## Shared lint policy

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

## Automatic cleanup with Sensei

Use `cargo run -p sensei -- fix --preview` to list proposed edits and their
non-whitespace character and nonblank line counts. Use `fix` to apply them.
Sensei runs the pinned rustfmt with its default style. It passes Rust 2024 as
the edition and formats each file through stdin and stdout. Review import
grouping and field initializer shorthand during the file refactor. The
inventory includes standalone Rust sources and skips vendored sources, files
marked as generated, and the byte-sensitive paths recorded below.
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
read-only rust-analyzer search. Use `fix --shorten-paths` with the pinned
rust-analyzer and `rust-src` components for the import assist. It works on a
selected set of standard-library free functions, including `std::env::args_os`
and `std::mem::take`. Sensei accepts a rewrite when it saves characters and
rust-analyzer resolves every shortened call to the same definition. Select
standalone manifests explicitly when running the analyzer pass.

Extend the automatic set when the tools provide a mechanical fix that preserves
behavior across relevant configurations. Error and panic policy, numeric
conversions that can lose information, ownership and lifetime design, public
API changes, and safety proofs need human review.

## Changing a shared lint

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
   Keep proven invariants and narrow exceptions. Surface any behavioral
   tradeoff for the owner before enabling the rule.
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

### Safety comments and custom lints

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
| `missing_errors_doc` | Write actionable error contracts for high-value library boundaries when comments are allowed. | Check generated rustdoc and its links. Compare failure cases with the documented categories. Keep workspace-wide enforcement on hold. |
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
| `missing_docs` | **Hold** as a broad compiler lint. Document API contracts during the refactor. Reuse external Markdown with `#[doc = include_str!(...)]` where it serves callers. | Build rustdoc for the affected package and inspect links and exposed items. Require a clean target baseline before considering enforcement. |
| `unreachable_pub` | Audit one library. Identify re-export paths, cross-crate callers, and feature-dependent visibility. | Compile consumers and the relevant feature variants after narrowing visibility. Keep ordinary builds on the current compiler policy while this is an audit. |
| `rust_2018_idioms` | Review individually useful diagnostics. The group includes `elided_lifetimes_in_paths`, an explicit exception in the upstream Bevy reference. | Avoid a blanket group switch. Where warranted, check a specific lint on one package and verify the framework signatures still work. |

## Lince-specific review references

Use these references when a change reaches the corresponding boundary.

- Keep CRDT import failure isolation and diagnostics lock recovery as deliberate
  boundaries. Preserve causal error information and caller-visible diagnostics
  when simplifying errors.
  [Engine errors](../../../crates/engine/src/error.rs) · [CRDT guard](../../../crates/engine/src/collab_guard.rs) · [Diagnostics](../../../crates/utils/src/diagnostics.rs)
- Preserve readable and selectable Protein text, the required Protein-first
  Laboratory case, and edit-panel geometry for affected UI changes.
  [Lince requirements](Lince.lingua)
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
Engine dispatch name-resolution and `Send` errors.
[Engine dispatch](../../../crates/engine/src/actions/dispatch/part_1.rs)

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
