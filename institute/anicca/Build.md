# Remaining compilation-speed work

Scope: faster development and release compilation, locally and in GitHub Actions, without making behavior worse or reducing final release-binary performance. Completed experiments and defaults are recorded in [Build.lingua](../trails/lince/Build.lingua).

## Fixed task list

**Agents must not increase the number of tasks.** Do not add, split, duplicate or create checkbox subtasks, or turn newly discovered work into additional tasks. Grouping existing tasks is allowed if their IDs, scope and count remain unchanged. Only an explicit instruction from the user may authorize changing the count. Report discoveries or blockers in plain text for the user to decide; do not execute that additional work.

Choose **one atomic checkbox per turn**. Do not start another without the user's instruction. Each checkbox tests one candidate in one workflow or one CI cache scenario. Its measurements, required validation and decision belong to that checkbox, not new tasks. Mark it complete when the scoped result and choice are recorded, including a justified rejection or deferral. An unchecked item may already be in progress; do not restart its existing experiment. If you encounter candidates for extra tasks, bring that to the user at the end of one task being done.

## Rules for every test

Record pre-existing failures without repairing them. Use local correctness checks and relevant runtime measurements to detect regressions; if existing failures prevent a sound comparison, record the limitation and defer promotion.

GitHub Actions runs compilation checks and requested packages, **without test suites**; local tests and benchmarks remain allowed. Preserve warnings as errors, recovery/hardening, cache invalidation, package verification and publication order. Separate one-time setup from repeated compilation. Do not lower release optimization, change the release CPU requirement or trade shipping runtime performance for build speed.

Record the tested command/settings, before/after timings, relevant correctness/invalidation findings and the adopt/retain/defer choice in Build.lingua through its established workflow. Do not add follow-up tasks to this file.

## 1. Compiler-cache process

Observed during release preparation: several queued compile requests, eight available CPUs and one active compiler; a fresh isolated cache process restored parallel compilation. This is concurrency evidence, not a controlled percentage gain.

For each comparison below, use at least three balanced complete iterations per side, including shell/cache startup. Start the candidate process outside Cargo while retaining compiler flags, disk cache and release optimization. Verify relevant invalidation before making a useful recipe the normal-command default; otherwise keep the default and report the limitation. Do not stop shared cache processes or clear caches.

- [x] **1.1 — Test the Park iteration with an independently started compiler-cache process.** Retain the existing default: median complete cycle 7.34s versus 7.24s, below the useful-gain gate and not consistent across samples. Protocol cases pass; results recorded in Build.lingua.
- [ ] **1.2 — Test the shared-Interface iteration with an independently started compiler-cache process.** Compare the normal persistent-shell shared-Interface edit/check/restart workflow against the same workflow using the candidate process.

## 2. Development-only Interface setting

An earlier shared-edit cycle improved 24.91%; adoption remains unqualified. Keep shipping release settings unchanged. Park test-only 512 was already rejected and must not be repeated. Existing product failures are comparison limitations, not repair tasks.

- [ ] **2.1 — Test Interface 512 versus 256 codegen units in the development workflow established by group 1.** Adopt only a repeatable complete-iteration gain with no new behavior/runtime regressions; otherwise retain 256.

## 3. Release linkers — work already in progress

Task 3.1 is complete: use Wild locally and retain mold for Actions. The cached release result and final choice are in Build.lingua; no further Actions linker comparison is queued. Task 3.2 covers the separate Nix source-package build. Do not restart completed measurements when continuing another scope.

Use the actual Linux release artifact/flags, pinned Wild and constant compiler optimization, CPU target, hardening, unwind/recovery and native inputs. Each scope needs complete build/link measurements, binary correctness, packaging and representative optimized-runtime validation before its own adoption decision. Prior local linker timings are not shipping evidence. Do not benchmark unrelated linker versions again.

- [x] **3.1 — Test Wild against the current GitHub Actions mold linker for Linux release.** Keep all other release settings identical and record the decision for the Actions release scope.
- [x] **3.2 — Choose the Nix source-package linker.** Selected Wild through Clang for x86-64 Linux source packages using existing same-input link evidence; the owner explicitly waived new testing for this scope. Other platform linkers and release settings are unchanged. Decision recorded in Build.lingua.

## 5. GitHub runner measurements

The two consolidated platform workflows and compatible compilation caches are implemented and locally validated. These tasks measure that revision after it is available remotely; they do not rebuild the consolidation.

Each checkbox measures one workflow/cache scenario against the new check/build commands. Record whole-job time, compilation time, cache restore/save time and archive size. Retain only caches with useful overall gains and correct source/tool/native invalidation; account for archive transfer overhead and competition between platform caches. Keep Actions free of test suites. Removing test compilation was requested coverage scope, not a measured cache speedup.

For these measurements, cold means no reusable compilation artifacts for the selected workflow, warm means unchanged source with its compatible cache restored, and source-edit means one recorded representative edit with the compatible baseline cache. Record actual registry/tool cache state separately.

- [x] **6 — Keep local and Actions builds on the same latest stable Rust.** Shared Rust 1.99.1 pin; daily automatic updates validate Linux and both Android architectures before advancing it. Implementation and validation are recorded in Build.lingua.
