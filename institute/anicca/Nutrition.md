# Nutrition — portable Trail source locations

Status: outstanding work and technical handoff. Refine the source-location and persistence choices with the user before implementation, following `AGENTS.md`. Complete the tasks below in order, remove each completed task after verification, and delete this document when all work is finished.

Demeter's nutrition targets, personal price assertions, food constraints, optimization, inventory, production tasks, and native Castle are implemented. The remaining work here is reliable discovery, opening, and persistence of its external Trail sources. Integrate this with the shared external-content and URI work in [Trail.md](Trail.md); keep one general source-loading workflow.

## Confirmed problem

The **Demeter Castle → Meal Preparation Trail** shortcut supplies a repository-relative filesystem path without establishing a content base:

```text
institute/trails/health/Meal-preparation.lingua
```

The local Cell/backend converts that string into an absolute file URL using its process working directory. It does not identify the repository or an installed content location.

Example reproduction:

1. Start Lince with `/home/user` as the local Cell/backend's working directory.
2. Open **Sand Store → Demeter Castle**.
3. Click **Meal Preparation Trail**, then **Preview source**.
4. The loader targets `/home/user/institute/trails/health/Meal-preparation.lingua`.
5. The development file actually lives at `/home/user/git/lince-social/lince/institute/trails/health/Meal-preparation.lingua`.

Preview/import therefore targets a missing file. Starting from the repository root resolves the intended development file. Installed applications may have neither that working directory nor a checkout.

This diagnosis is confirmed by code inspection. A GUI regression test exercising the actual shortcut from another working directory remains to be added. Use the source-loading backend's working directory when reproducing the problem, including configurations where UI and backend processes differ.

## Current implementation and evidence

| Location | Relevant behavior |
| --- | --- |
| [Demeter shortcut](../../crates/interface/src/native/demeter_castle.rs) — `Operation::Trail` | Creates a `TrailCastle` with the literal `institute/trails/health/Meal-preparation.lingua`. |
| [Demeter catalogue](../../crates/engine/src/demeter/catalog.rs) — `demeter_catalog` | Returns repository-relative `trails.entry`, `trails.needs`, and `trails.farming` values. Update these consistently with the shortcut. |
| [Trail Castle](../../crates/interface/src/native/trail_castle.rs) — `Operation::Preview` | Sends `castle.uri` through `engine::sources::Command::Preview`; import also uses that URI after preview. |
| [Engine sources](../../crates/engine/src/sources.rs) — `publication`, `source_command` | Loads the publication in the backend through the shared resource library. Local source files require the local owner session. |
| [Resource addresses](../../crates/resources/src/lib.rs) — `address` | Parses a URL or falls back to `std::path::absolute`, then constructs a file URL. Relative filesystem input therefore depends on process working directory. |
| [Publication loader](../../crates/resources/src/publication.rs) — `Publication::load` | Normalizes the entry address before loading. Dependencies and resource representations already resolve against each document's effective source base through `resolve`. |
| [Resource library](../../crates/resources/src/library.rs) | Retains content and cache entries by address/hash. Its managed storage root is not automatically the location of a supplied Trail content tree. |
| [Saved Trail Castle](../../crates/interface/src/native/trail_castle_persistence.rs) | Relevant to restoring the selected source after restart. |

The generic address converter's handling of an explicitly entered relative filesystem path can be legitimate. The defective assumption is in application-generated shortcuts that supply a repository-relative path without a base. Preserve document-relative imports, links, and media resolution.

Existing checks do not establish portability of this shortcut:

- [Demeter's complete-publication import test](../../crates/engine/tests/demeter.rs) builds an absolute development path using `CARGO_MANIFEST_DIR`.
- [Native Demeter interaction tests](../../crates/interface/src/native/demeter_castle/tests.rs) use prepared Records and exercise pricing, solving, and task acceptance.
- [Demeter visual example](../../crates/sandbox/examples/demeter_visual.rs) renders prepared fixtures. Its successful captures do not prove installed-content discovery.

## Remaining tasks

### 1. Establish the shared source-location contract

- [ ] Reconcile this fix with the current URI resolver, external-content, and distribution decisions in `Trail.md`; reuse general mechanisms already implemented by the Trail builder.
- [ ] Define how an application shortcut obtains an available source: an accepted source location, explicitly supplied adjacent content, a user-selected file/folder, or an ordinary HTTP(S) address. Follow the shared resolver's context and fallback rules.
- [ ] Define the content base explicitly. A development selection may point at `institute/trails/`; an installed distribution may expose a different directory tree. A shortcut must resolve through the supplied context.
- [ ] Decide where accepted locations are persisted using normal source/configuration state. Preserve the original source address and effective base where the shared resolver needs both.
- [ ] Keep the solution independent of the developer's checkout path, `CARGO_MANIFEST_DIR`, and changes to process working directory. Follow the external-content plan instead of embedding authored Trails or introducing a special Trail catalogue/manifest for this fix.

### 2. Resolve Nutrition-related entry points through that contract

- [ ] Replace the literal repository-relative URI in `Operation::Trail` with the shared source-opening path and its explicit source context.
- [ ] Update catalogue locations for Meal Preparation, Nutrition Needs, and Farming to use the same mechanism. Avoid separate UI and backend location rules.
- [ ] Normalize the selected entry before preview/import so both operations address the same source independently of the backend's working directory.
- [ ] Audit other application-generated Trail shortcuts for the same assumption and route affected callers through the shared mechanism. Distinguish executable shortcuts from authored document-relative links and intentional relative-path inputs.
- [ ] Preserve each dependency's declaring base, including nested Farming/Production paths and effective HTTP bases after redirects. Apply the current composition/import mechanism if `Trail.md` has replaced `lince.source.dependencies` by then.

### 3. Complete native selection, absence handling, and restoration

- [ ] When the shortcut's external content is unavailable, explain that a source location is needed and offer the ordinary Trail file/folder/URL selection flow.
- [ ] Remember the accepted location and reopen it after restarting from another directory. Restore saved Trail Castle state without reinstating the repository-relative assumption.
- [ ] Keep binary-only operation usable when optional learning content is absent. Selecting or importing content remains an ordinary user action.
- [ ] Preserve existing preview, accepted-update, and Karma activation boundaries. A location lookup must not silently import Records or activate rules.
- [ ] Preserve source access checks, local-owner restrictions, and normal error reporting for unavailable or unauthorized sources.

### 4. Verify external delivery and relative dependencies

- [ ] Supply the publication through the general optional adjacent-content or selected-folder workflow. Keep authored content outside the executable, following `Trail.md`.
- [ ] Exercise Meal Preparation together with Nutrients, Nutrition Needs, Farming, Production, and their required nested sources. Copy or distribute the necessary tree with its relative relationships intact.
- [ ] Confirm loading from a directory outside the checkout, including a path containing spaces. Selecting a different supported content root must establish that root's actual source context.
- [ ] Confirm accepted source identity, imported Records, and personal state remain stable when only the application's launch directory changes.

### 5. Add regression tests and complete validation

- [ ] Test the actual Demeter shortcut followed by preview/import with a backend working directory outside the repository. Isolate working-directory changes in a subprocess or equivalent fixture; `set_current_dir` is process-global and unsafe across parallel tests.
- [ ] Test selected external content with spaces in its path, complete nested dependency resolution, and reopening after restart from a different working directory.
- [ ] Test binary-only operation with content absent, the recovery/selection UI, and successful loading after selection.
- [ ] Retain coverage for valid absolute filesystem paths, file URLs, HTTP(S) sources, and document-relative dependencies/media using their effective source bases.
- [ ] Verify remote callers still cannot read local files and that source-location mappings do not bypass existing access checks.
- [ ] Run focused `lince-resources` tests with the `publication` feature, engine source/Demeter integration tests, and native Trail/Demeter interaction and persistence tests appropriate to the final change.
- [ ] Run `cargo check` for affected crates and the native interface through Nix, with compiler warnings denied. Check publication syntax if authored files change. Review the final diff and remove completed work from this plan.

## Working constraints

Follow repository `AGENTS.md`. Work on the current branch, preserve unrelated concurrent changes, use `cargo check` instead of `cargo build`, and run through Nix or `cargo xtask`. Keep scratch storage and `TMPDIR` under `/home/user/`; do not use `/tmp` or `/dev/shm`. Preserve past migrations, `AGENTS.md`, `README.md`, the unplugged web crate, and the owner's `institute/anicca/Lince.lingua`. If schema work proves necessary, use a new append-only migration and the required checksum/guard checks.
