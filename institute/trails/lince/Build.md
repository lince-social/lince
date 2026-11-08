# Local target-cache budget

The owner's 2026-11-08 instruction sets a **300 GB default** (`300_000_000_000` allocated bytes) for this repository's `target/`. Cleanup removes the oldest inactive compiled artifact groups first, using each group's newest modification time. This is an idle cache budget, not a filesystem quota: compiling profiles are deferred until their Cargo locks become available.

The existing `crates/xtask/src/prune.rs` cleanup now also enforces this budget before and after `dev`, `check`, `check-instinct`, `test` and `test-all`. `cargo xtask prune-target --dry-run` previews eviction; `cargo xtask prune-target` runs it directly. These commands use `CARGO_TARGET_DIR` when set. Bootstrapping xtask may compile the helper, so the installed maintenance executable is the route that never starts Cargo.

A user timer covers ordinary Cargo commands as well. It checks this checkout's `target/` every five minutes and after login, with Nice 19, idle I/O and CPUWeight 1. It runs a small standalone helper outside `target/`, requiring no new package or Park binary. Install/update it with:

```sh
python3 -B institute/scripts/install-target-cache.py
```

The helper is `~/.local/libexec/lince-target-cache`; the units are `lince-target-cache.service` and `lince-target-cache.timer`. Its direct preview is `~/.local/libexec/lince-target-cache /absolute/checkout/target --dry-run`. The timer intentionally follows this checkout's ordinary `target/`; custom `CARGO_TARGET_DIR` paths are handled by xtask or an explicit helper invocation.

Eviction groups include hashed dependency artifacts and their fingerprints/build outputs, hardlinked final aliases, and whole incremental variants. Cargo profile locks and a maintenance lock serialize cleanup. Observed running executables are retained. Size accounting deduplicates hardlinks, and symlinks do not extend deletion outside the tree. Source files, validation receipts, unknown files and sccache's independently managed cache entries are preserved; their size still contributes to the directory total. If busy/protected data prevents reaching the budget, the helper reports that condition and retries later. Other filesystem use can still exhaust disk space before this target budget is reached.

Validation: five Python integration checks pass, exercising eleven budget-policy Rust cases, the eight existing incremental-pruning cases and one workflow-entrypoint case. A real private Cargo build proves lock deferral, running-executable retention, inactive eviction and successful recompilation afterward. Systemd parses the generated units, including paths containing spaces/percent signs. The installed timer is enabled and active; Nice/I/O/CPU settings were verified. No existing target artifacts were deleted by installation because the directory had already disappeared.

The Lince/Build Record scan found no applicable pipe-prefixed requirement for target-cache limits. Owner-authored Records were preserved. Native Park performance execution remains pending for the new binary; cache-policy fixtures are separate filesystem/build-tool checks.
