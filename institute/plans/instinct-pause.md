# Instinct development pause

Paused at the owner's request on 2026-10-04. The requested handbook is partially implemented. This file records the work in the shared working tree and the remaining work; it does not mark the whole proposal complete.

The task was originally to implement all of [Instinct.md](../anicca/Instinct.md). The owner subsequently authorized additions to the exact canonical file, institute/anicca/Lince.lingua, while retaining all original text. Development is now paused. Resume only when the owner requests it.

## Running the application

Ordinary startup keeps the Cargo instinct feature off. The existing cargo xtask dev command runs the native interface and enters the Nix interface shell when Nix is available. The changes remain on the current branch.

For ordinary startup without the handbook, with a separate development database:

~~~sh
LINCE_DATA_DIR=.lince-dev LINCE_PORT=6176 cargo xtask dev
~~~

mise dev now delegates to that command, with .lince-dev and 6176 as its defaults. mise dev-instinct remains a separate opt-in task for the unfinished handbook; its direct Cargo command requires the interface development dependencies.

The instinct feature remains enabled in the added handbook CI checks and the modified release/Nix package configuration. Those paths validate and embed the canonical file. Opting into the flag does not mean all proposed practices have been completed.

## Work implemented so far

- Added the ordered 65-page handbook manifest and 13 chapters. The hub uses the suggested basics route, provides direct page starts in Free and Assisted modes, and has page search and related reference navigation. Later pages currently provide reading rather than the required feature exercises.
- Added canonical explanations and step Records to Lince.lingua, with explicit UIDs, slugs, #instinct and subject/chapter relationships. Filled the empty Sync explanation by adding text. Original content was retained. The new wording is an agent-written draft authorized by the owner; it still needs human review.
- Added a shared typed practice model in crates/interface/src/practice.rs: operations, steps, observations, scoped semantic targets, progress distinctions and a ticketed runner. Next observes before acting, Skip does not execute the sample action, pending actions are correlated, and retries retain their ticket.
- Replaced the old data-writing tutorial runner with the new native practice adapter and retained the highlight geometry code. Added Close, Keep/Discard, mode changes, a chapter picker and the independent Ctrl Shift Escape path.
- Added isolated in-memory practice Cells and scoped source routing. Protein Areas, property transitions, full Record views, Todo and Kanban practice requests can use the sample Cell rather than the personal Cell. Practice property Areas reject personal Record UIDs.
- Implemented sample adapters for opening Edit mode, placing/moving/pinning Sands, composing and separating a Castle, workspace/canvas navigation, text, sample appearance, ordinary Areas, attraction/repulsion, size effects and inspection.
- Implemented sample Record/Assertion/Vocabulary views, live Protein Areas and property presentation, and confirmed Record changes on Area entry/exit. These adapters still need broader behavioral review.
- Added work chapter adapters for completion/undo, a standalone stopwatch, dated Records, an Operation quantity change and notification inspection/dismissal. These were the latest feature additions and are not complete teaching coverage for T25–T30.
- Added separate learning-progress persistence. Keep retains an isolated sample workspace/Cell for the current application run; it is not a durable retained Cell across restarts. Practice-created property actions are disabled on finish.
- Added a default-off Cargo flag forwarded through Lince, Cell and desktop to the engine. Disabled builds emit an empty bundle and do not automatically seed an empty reader. Enabled builds read the canonical file, use Anicca's actual grammar, validate required content, and embed the approved manifest plus referenced Records.
- Added structural validation for missing content, duplicate slugs/UIDs, tags, subject relationships, references, invalid chapter quantities, duplicate chapter order and cycles. Full manifest-order/reference diagnostics remain unfinished.
- Replaced handbook import with read-only preview and atomic commit. Preview reports exact Records, new/reused counts, conflicts, dedicated Vocabulary/concept identities and a fingerprint. Commit revalidates in its transaction, refuses conflicts and permission failures, preserves canonical Record properties/Assertions, and does not execute imported tutorial text.
- Added import rollback/conflict/repeat tests, shared runner tests, native reader/practice tests and Laboratory registrations. Added mise tasks and CI feature checks.
- During pause stabilization, fixed the search editor's duplicate Node bundle, which could panic when opening the reader. Corrected native test fixture initialization and updated the test for the newly added movement step. Ordinary mise dev no longer enables the unfinished feature automatically.

## Coverage and remaining limitations

| Proposal group | State at pause |
| --- | --- |
| T01–T03 | Explanations and independent handbook controls are present. The complete first-use/help exercise has not been reviewed. |
| T04–T11 | Native sample adapters are present. Manual store additions, keyboard-only activation and all layout/removal choices need further contract checks. |
| T12–T15 | Ordinary Area adapters are present. Force direction, visible motion, exclusions and size effects need wider native verification. |
| T16–T20 | Isolated sample creation and views are present. Unit examples, aliases/ambiguity, relation following and work-history presentation are incomplete. |
| T21–T24 | Live sample Protein and entry/exit adapters are present. Saving a query, full presentation choices and arrangement/motion coverage are incomplete. |
| T25–T30 | Work examples were added. Kanban's Next adapter currently applies saved state directly before presenting the board; it still needs the same ordinary drag/transition path as manual movement. The Operation lesson also needs its configured ordinary control. |
| T31–T65 | Navigable explanations exist. The requested prepared automation, peers/social flows, Transfers, file sync/blob transfer, file/document/audio/spatial examples and maintenance exercises are not implemented. |
| P02/P03 acceptance | Runner/input scaffolding exists. The complete action-equivalence, lifecycle, input and failure matrix is unfinished. |
| B01–B05 | Flag, embedding, validation and initial seeding changes exist. Package/source-without-canonical checks, precise manifest order and all seeding cases need final verification. |
| I01–I04 | Atomic engine import and UI preview exist. Import UI timeouts, a way to inspect the committed Records, clearer concept/Assertion rendering and cancellation during commit need work. |
| P09/P10 | Full route exercises and a finalized maintenance workflow remain unfinished. This pause note is a handoff, not their acceptance. |

Assisted mode uses action policy, pointer/tab disabling, focus release, selection/gesture checks and scoped targets. Open Edit resolves its actual feature control; several other operations resolve sample owners; some still resolve the tutorial's action button. This is not yet a complete semantic-control contract shared by every feature, inspection and Laboratory.

Pending-step timers are aborted on completion/Skip/Close, and dropping the practice component aborts its tracked tasks. Setup waits and asynchronous example waits still need independent bounds. Removal of an owner outside ordinary Close still needs complete cleanup of restrictions and sample resources. Keep needs durable ownership/persistence, and settings restoration needs more lifecycle tests.

Inactive practice and import systems have run conditions. Baseline measurements, active-idle acceptance budgets, all input methods, 2D/3D highlight relocation, stale/disconnected responses and hardware/provider failures have not been comprehensively measured or exercised. Human readability review has not occurred.

## Validation

The engine import integration suite passed all three cases after the work chapter additions: preview/cancel/repeat/content conflicts, slug races/permission denial, and injected failures during Record/Assertion creation with complete rollback.

During pause stabilization, nix develop .#interface --command cargo check --locked -p lince passed with warnings treated as errors. This checks the ordinary application, including its native interface and media dependencies. A graphical startup has not been verified in this handoff.

The complete application also passed nix develop .#interface --command cargo check --locked -p lince --features instinct after the latest adapters and reader fix. This verifies compilation and canonical-content validation, not complete tutorial behavior.

The existing run helper passed nix develop .#interface --command cargo check --locked -p xtask. mise.toml also parsed successfully, and its ordinary task delegates to cargo xtask dev without enabling Instinct.

The current Anicca validation suite passed all three tests, and the shared practice runner suite passed all six tests. The command was nix develop .#interface --command cargo test --locked -p anicca --test instinct -p lince-interface --no-default-features --test practice.

The first complete native Instinct test run found the reader bundle panic and missing runtime resources in the test fixture. Those failures were addressed during pause stabilization. The fixture also now uses Bevy's current insert_non_send API so deprecation warnings do not fail the test build.

The final native run was nix develop .#interface --command cargo test --locked -p lince-desktop --features instinct --lib instinct:: -- --test-threads=1. It passed 10 tests and failed two. Reader navigation, restoration, error handling, habit controls, independent page starts and Assisted recovery checks passed. The remaining failures are:

- user_action_and_next_share_observation_without_duplicate_samples: the movement step stays at step 2 instead of advancing to step 3.
- record_changes_use_an_isolated_cell_and_confirm_entry_and_exit: sample Records are created, but the Area-entry action reports that its sample Sand is missing, so entry/exit confirmation times out.

These opt-in practice behavior gaps remain unfinished at the owner's pause request. The native Instinct test suite is not green. No further feature adapters were added during stabilization. Ordinary startup leaves the feature off; both application configurations compile, but neither a successful compile nor the passing reader tests establish complete practice coverage.

git diff --check passed. The canonical Lince.lingua diff contains 473 added lines and zero removed lines. Original proposal text was retained, with a pause note appended to Instinct.md.

## Resume sequence

1. Read this note and the existing diff before changing the shared working tree. Preserve unrelated work, AGENTS.md, README.md and original canonical text.
2. Re-run the documented checks. Finish the current native reader/practice failures before adding another feature adapter.
3. Finish P02–P04 recovery, ownership, semantic targets, input coverage, build/import diagnostics and measurements.
4. Complete and verify T04–T24, then correct and finish T25–T30.
5. Implement T31–T65 in the proposal's listed order using actual ordinary feature actions and isolated prepared examples. Reading-only pages do not satisfy tasks that request a real safe practice outcome.
6. Exercise the full route and every independent start in both modes. Arrange human review of the explanations and instructions.
7. Finalize the tutorial maintenance workflow outside AGENTS.md unless the owner separately authorizes that file.
