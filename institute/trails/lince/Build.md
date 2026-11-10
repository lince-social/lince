# Local target-cache budget

The owner's 2026-11-08 instruction sets a **300 GB default** (`300_000_000_000` allocated bytes) for this repository's `target/`. Cleanup removes the oldest inactive compiled artifact groups first, using each group's newest modification time. This is an idle cache budget, not a filesystem quota: compiling profiles are deferred until their Cargo locks become available.

The existing `crates/xtask/src/prune.rs` cleanup enforces this budget before and after `cargo xtask dev`, `check`, `check-instinct`, `test` and `test-all`. `cargo xtask prune-target --dry-run` previews eviction; `cargo xtask prune-target` runs it directly. These commands use `CARGO_TARGET_DIR` when set.

The owner chose cleanup around build commands only. The periodic systemd timer, its service, installed executable and installer have been removed. Plain `cargo build/check/test` commands bypass the automatic cleanup; use the corresponding xtask entry points for the budget. No scheduled background cleanup remains.

Eviction groups include hashed dependency artifacts and their fingerprints/build outputs, hardlinked final aliases, and whole incremental variants. Cargo profile locks and a maintenance lock serialize cleanup. Observed running executables are retained. Size accounting deduplicates hardlinks, and symlinks do not extend deletion outside the tree. Source files, validation receipts, unknown files and sccache's independently managed cache entries are preserved; their size still contributes to the directory total. If busy/protected data prevents reaching the budget, cleanup reports that condition and retries around the next wrapped build command. Other filesystem use can still exhaust disk space before this target budget is reached.

Validation: three Python integration checks cover eleven budget-policy Rust cases, the eight existing incremental-pruning cases and one workflow-entrypoint case. A real private Cargo build checks lock deferral, running-executable retention, inactive eviction and successful recompilation afterward. The timer and service were disabled and removed; source-level cleanup around xtask commands is unchanged.

The Lince/Build Record scan found no applicable pipe-prefixed requirement for target-cache limits. Owner-authored Records were preserved. Native Park performance execution remains pending for the new binary; cache-policy fixtures are separate filesystem/build-tool checks.
