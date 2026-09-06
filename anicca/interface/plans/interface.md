# Interface implementation plan

Purpose: Retain the landed foundation statement and detailed remaining Box, Area, persistence, and Facade work.

Owner source: [Interface in Lince](../../Lince.lingua).

Status: native foundation, customization, C1 semantic primitive-Sand, C2
recursive-composition and C3 Configuration/external-authoring runtimes
accepted; C4 official-Sand migration has begun with its Rust structure catalog
and native inspection surface.

Read when: closing the production report or implementing a Box-and-after cluster.

[Corpus map](../README.md) · [Current context](../current.md)

---

## What is left

### Interface — what is left

#### Landed native interface foundation

The following records the accepted joined prototype and its source revisions. The next production default is deliberately different: [Part A](part-a.md) extracts the same native host without CEF and earns new native-only evidence. Historical browser proofs and packaging instructions are retained for [the final CEF lane](cef.md), not repeated before Box.

Plan A is accepted and the completed task entry has been removed. The Linux
desktop now reaches the root-workspace `lince-interface` package directly,
owns one Wayland event loop and WGPU/Vulkan compositor, runs selected Bevy
render resources manually, projects retained text and controls through
Glyphon/cosmic-text and AccessKit, and imports accelerated CEF content without
a framebuffer CPU path. The same runtime supplies the eight human-visible
laboratory scenarios and machine-readable evidence. The production desktop
starts the real local Lince server before opening that host.

The accepted load is 200 visible interactive Sands, 1,000 continuously
eligible 120 Hz bodies and 10,000 resident lightweight nodes. On the owner's
Iris Xe Wayland session, the final-fingerprint joined 12-second run measured
10.181 ms frame p95, 12.433 ms p99, 4.458 ms CPU-frame p95, 0.680 ms
fixed-step p95, zero fixed-step backlog, 3.059 ms input-to-present-call p95
and 5.388 ms p99. It reached an interactive window in 901.018 ms and the first
CEF frame in 1,467.609 ms. These last input numbers are explicitly
present-call lower bounds; physical display timing was not measured.

The required three-repeat benchmark then ran 30 seconds of warm-up and 120
seconds of sampling per repeat with source fingerprint
`e2bc1f1bee10ac8a41aba7d0799d632f517a27eab4e7a98b9d91ca7bea4522bd`.
Frame p95 ranged from 10.423 to 10.444 ms and p99 from 12.016 to 12.167 ms;
CPU-frame p95 ranged from 4.642 to 4.719 ms, fixed-step p95 from 0.694 to
0.699 ms, and input-to-present-call p95 from 3.480 to 3.547 ms. All three
repeats had no frame above 50 ms. All three
reports used Wayland, Vulkan, Mailbox presentation, the 1366×740 surface and
the Intel Iris Xe/Mesa 26.1.2 stack in balanced power mode.

The current acceptance evidence also includes the CEF count matrix, live
off-camera behavior with suppressed copying, browser and read-only Facade
parity, AT-SPI traversal and activation through Orca, renderer and device-loss
recovery, and ten complete create/destroy cycles. The lifetime run left no CEF
process family alive and showed no monotonic RSS growth above five percent.
The dependency audit resolves one WGPU 29.0.4 family, no Git dependency family,
the selected licenses and the owned unsafe boundaries. CEF's authoritative
license and Chromium credits are bundled with its runtime evidence.

The final Nix package builds offline after its pinned CEF archive is fetched,
contains one canonical CEF payload plus its LICENSE and Chromium credits, and
occupies 765 MiB in the Nix store. Its selected Linux Cargo graph and runtime
closure contain no Tauri, Wry, WebKitGTK or xdotool runtime. CEF's prebuilt
binary still closes over its upstream GTK and X-family shared libraries even
though Lince exposes and launches only its Ozone Wayland path.

The production desktop release binary also builds, links and packages. The
native package is promoted at `crates/interface`; `mise run interface` favors
it, while `mise run interface-legacy` preserves the old Web UI as an explicit
browser surface. Both use the real Lince server. The native desktop prints the
legacy URL but does not place that privileged first-party page inside a
zero-authority Website Sand.

The exact pinned CEF distribution is materialized as a writable derived cache
inside `target/` for development because `cef-dll-sys` copies its files during
Cargo builds. The production package still bundles the immutable Nix payload.
The bundled consumer mints Lingua's optional declaration UIDs before
projection. The 2026-08-30 owner-authorized repair removed the six dangling
parent assertions while keeping the editable First Steps tutorial outside
Instinct, fixed Ailuros's required subject quantity, and left its unresolved
parent choice as a root rather than guessing. `lingua check anicca` validates
all 16 Records.

The first production rerun reached the window and wrote a passing report, then
revealed an AccessKit/zbus background panic caused by mixed Tokio and async-I/O
features in the combined desktop graph. The file picker now uses its
async-std/async-I/O portal backend, matching AccessKit and leaving only one
zbus executor family. This is a landed-foundation correctness repair; it does
not become C3 work. The corrected production report passed on Wayland at
1920×1052 with the accepted 200/1,000/10,000 workload and two accelerated CEF
surfaces: 10.861 ms frame p95, 11.967 ms p99, 1.098 ms fixed-step p95 and zero
backlog. No AccessKit/zbus panic recurred; the source fingerprint is
`1fd02aec9ca85626bc4cd33685c80b23da84f205bd23c0fe6c825022641ab97b`.

The same promoted release runtime passed its direct joined report on
2026-08-29 at 1920×1052. Under the accepted 200/1,000/10,000 workload and two
accelerated CEF surfaces it measured 10.958 ms frame p95, 12.448 ms p99,
1.172 ms fixed-step p95 and zero backlog. This closes the crate-promotion and
runtime seam; the production desktop report separately proves integration
with the current owner corpus.

The customization kernel is also landed. Its versioned typed contract resolves
92 canonical roles through seven inspectable scopes into native WGPU/retained
UI and Installed CEF CSS. The live Gallery proved workspace, group, instance,
partial-theme and mode changes without reloading Installed HTML or granting a
Website authority. The completed task is prose rather than a stale checklist;
its source inventory and evidence live in [Customization](../customization.md)
and [Visual inventory](../visual-inventory.md).

The semantic primitive-Sand kernel is landed as schema and ABI version 1. It
separates the renderer-neutral graph, persisted instance, projection manifest
and disposable runtime binding; validates graph, capabilities, bounded assets,
hashes, HTML node declarations and vendored notices; and generates package,
instance and message schemas plus valid/stale fixtures. The retained Gallery
and Installed CEF package expose the same 19 primitives. Native keyboard,
pointer and AccessKit actions, strict unknown-field refusal followed by a
Protein-shaped HTML mount, exact
`record-clicked` grant and zero-authority Website all passed in the joined
Wayland release report. The completed C1 entries and evidence are preserved in
[Customization plan](customization.md), not repeated as an active task here.

The C2 recursive-composition kernel is landed as composition schema version 1.
One Rust host consumes exact definition revisions, recursively applies
configuration and style patches, selects renderer adapters, wires exported
ports and owns disposable renderer and Behavior handles. Shared publication is
transactional through every exact ancestor; save, fork and last-known-good
lineage remain explicit. The F10 keyboard workbench exposes the Button and
twice-nested video-call fixture, all authoring mutations, visually distinct
Protein/Event/Action arrows and teardown. Native constructors, recursive Maud
and Installed HTML agree on the same normalized package. Release parity and
the joined Wayland report pass; the completed entries and exact evidence are
in [Customization plan](customization.md#landed-c2-recursive-composition-workbench).

The C3 Configuration/external-authoring kernel is landed as Configuration
artifact schema version 1. It persists theme/mode, workspace/group layers,
developer CSS and domain launch receipts with the exact composition artifact;
instance and definition edits therefore keep one source of truth. The
22-definition Configuration Sand exposes 16 F11 operations through keyboard,
pointer and AccessKit with live preview, origins, inherit, undo, safe reset,
refused states and atomic save/reopen. Invalid or unreadable persisted state
cannot block the complete default.

Theme selection uses uid plus manifest hash, the generated reference covers
all 92 typed roles, and one cascade projects to native, shared HTML, isolated
Installed HTML and browser roots. Scoped developer CSS refuses imports, URLs,
executable schemes, escaping selectors and host-security imitation. The parity
diagnostic publishes a seven-file ordinary HTML/CSS/JavaScript external-author
kit and unknown-version fixture. Renderer-neutral launch recipes create exact
placements, typed Record reads and Action exports idempotently; their durable
receipt focuses an existing domain group after reopen. The parity gate, joined
61-test suite and real Wayland/WGPU/CEF report pass. The 2026-09-01 report
applied and restored Configuration without reloading Installed CEF under the
accepted 200/1,000/10,000 workload; frame p95 was 10.879 ms, fixed-step p95
was 0.752 ms with no backlog, and its source fingerprint is
`577dc8b13f1e2b00822f0e97ecd1722fc0672b3edf0295f1f3a06532391c439c`.

#### Completed engine study and its place in the waterfall

The 30-entry Pulsar/Helio study is complete. It selected no GPUI, Pulsar,
Helio, SceneDB or WGPUI dependency and inserts no engine-adoption stage. The
canonical seven-point carry-forward boundary and implementation sourcing order
are in
[Runtime architecture](../architecture.md#completed-pulsarhelio-study-and-carry-forward-boundary).
The complete audits remain in [the research ledger](../links.md), not in this
checklist.

The accepted ideas attach to work that already exists:

| Existing stage | Study constraint carried into it |
| --- | --- |
| C4 official Sands | Stable semantic ids and renderer-neutral definitions remain above retained nodes, Bevy entities, DOM nodes, CEF ids and GPU handles. Focused UI crates or bounded techniques may assist projections without creating another application model. |
| C5 completion gate | Runtime health relates input and semantic revisions to native simulation and presentation work. Frame coordination, native recovery, accessibility and visual quality remain human-tested. CEF health is added only in its final v1 lane. |
| Box navigation | The Lince coordinator owns the frame; culling removes only extraction and drawing. Coordinate frames, displayed-revision picking and bounded adapter snapshots are correct before scale optimization. |
| Protein and Areas | Typed events and Actions cross ownership boundaries. Neither camera visibility nor renderer residency changes Protein, Behavior or physics meaning. |
| Topology and free space | Authoritative frames and potential fields are shared across physics, rendering, picking and persistence. Dirty tiles, dense GPU buffers and compute enter only from measured workload evidence. |
| Box durability and collaboration | Snapshots, journals and stable ids own truth; runtime slots, contacts, meshes, GPU buffers and browser handles are rebuilt. SceneDB remains repertoire, not storage or protocol. |
| Installed HTML, Websites and Facade | CEF/DOM state stays behind its adapter, receives transformed input for the displayed revision and never becomes Box authority. |

Correctness work ships with its owning stage. Dirty-range tuning, indirect
drawing, GPU compaction, generated detail, advanced pass fusion, lighting and
other renderer techniques remain optional measured work; the waterfall does
not claim them merely because the study found them interesting.

#### V1 master waterfall

Steps 1–2 are **Part A — Dogfeeding**: [general backend foundations and company acceptance](../../backend-part-A.md), [new native Sands](part-a.md), [build rule](../build.md). A private Linux server, on a local machine or VPS, and native live Interface support knowledge, teams, projects/tasks and threads/messages through ordinary Records and Assertions. Roles use Protein-selected Record permissions and property-write grants, including checks on proposed changes. Manual vocabulary, policy and view setup is sufficient; no company starter or required owning project is a gate. The former twenty-root graph is [stale pending re-cut](../graph.md). Additional [native migrations](native-follow-through.md), Karma/Transfer and separate Communication Castles are not required. The final CEF lane stays at step 11; later Box/time requirements remain without making all of v1 a prerequisite.

The current C4 boundary catalogs all 25 official roots as 72 validated
Rust-owned definitions and exposes them through F12, pointer and AccessKit.
Configuration is marked landed; edit controls, zoom controls, Record,
Conversation, Table, Todo and Kanban have native retained Behavior. The production runtime binds
Records, Conversation trees and private drafts through live Protein
subscriptions and sends Message/draft writes through acknowledged Actions; the
other 17 roots retain an explicit runtime-Behavior-pending state. The three
collection roots have core projection and Action behavior but retain explicit
C4 follow-up for inline editing, configured Protein/lane rules and bulk work. This does not
advance the master waterfall past step 1 because remaining workflows,
native workflows and their scoped legacy cleanup still remain. It proves recursive
retained projection, interaction and accessibility, fixes nested input
forwarding and establishes the renderer-neutral read/write seam before the
rest of behavior migration depends on it.

The owner-authorized Record repair has closed the former production-entry
gate. `First Steps.linguai` remains the editable future tutorial rather than an
ingested Record; the six chapters temporarily stand as roots until that draft
becomes owner-reviewed Instinct.

1. Extract the common CEF-free native host and deliver its real Interface client entry plus the headless server, without legacy desktop Sands or the lince-desktop wrapper. Complete backend identity and Role/Protein/property permissions, native Organ connection, manual Record/Assertion/Protein authoring, Table/Todo/Kanban, threads/messages and administration. Company workflows reuse these pieces rather than adding a starter or separate task/team model. Keep stable controls/editing and scope legacy retirement to certified replacements; no Karma, Transfer or call workflow is required.
2. Pass native visual, accessibility, theme, lifecycle and performance gates together with Backend Part A's company fixture, off-LAN live access, revocation, restart and backup restore. This closes Dogfeeding, not the full C4 catalog. Browser proofs remain in step 11.

3. Build a stable 2D Box: visible controls, navigation, layers, anchors,
   direct placement and the base pattern. Land the readable snapshot, operation
   journal, undo and restart recovery here. No customization below ships only
   in memory. Retain movement code without enabling it in this first delivery.
4. Build Protein result templates, individual appearance overrides, released
   children, copy-and-release, the Sand/Castle picker and presentation-switch
   preview. Add focused editing facets, min/max sizing and temporary focus.
   Extend the existing Sand contract where required; do not add another model.
5. Build stationary property-derived grouping/sorting Areas and the native
   Calendar, then the Clock's top and local spiral views. Recurrence comes from
   Karma. No terrain or Box free-space physics is a dependency of these views.
6. Pass the stable everyday-use gate below, including persistence, keyboard
   use, individual edits, changing presentation, released children and time.
7. Finish the [native follow-through surfaces](#native-follow-through-and-cross-feature-surfaces), live workspace collaboration and the read-only Facade. Independent follow-through tasks may start after Part A as their domain dependencies permit; workspace collaboration and Facade consume the stable Box gate. Their identity, permission and recovery gates still apply. HTML export and a Facade in an external browser do not require an embedded CEF runtime; a new surface or backend operation is never declared delivered by a related Part A migration.
8. Add moving Areas, mutation visits, immunity, topology and free-space work
   under the retained sections below. Extend durable recovery to spatial
   checkpoints. Compare Avian and a narrow solver on this workload. Movement
   stays an explicit choice and the stable default remains available.
9. Add bounded Sand visual effects and the supported WGSL editor. This is
   optional decoration after everyday usability, not a Calendar dependency.
10. Complete Action, Rule and Transfer simulation views when the corresponding
    domain simulation support is ready. This lane can start after step 6 and
    does not depend on motion, shaders or the Facade. Calendar and Clock remain
    useful without it.
11. Complete [the final v1 CEF lane](cef.md): the optional adapter and packaging, five deferred roots, Installed HTML, mixed compositions, embedded previews, administration and fresh browser-specific acceptance. The default native application remains independent of CEF when this lane lands.

The 2026-09-06 owner revision changes the old order: durable stable composition
and native time views precede automatic movement. Later motion code is kept.
The checklists below name their delivery stage; later work is not part of the
first stable gate. New contract extensions are deliberate work with tests,
not claims that all requested behavior already exists. A defect is repaired
at its owner. Detailed pre-Box work lives in
[Customization](customization.md) and [Sands](sands.md); the sections below own
Box and after.

Fiote is not another stage in this waterfall. Its Phase 0 cleanup can proceed
independently; its prototype lane opens after C5, consuming the domain-derived
compound recipe and Conversation foundation without becoming a prerequisite for Box. Its full terminal-bearing session surface additionally waits for the late-v1 terminal; C5 alone does not make that pane available. The shared requirements and the Fiote-only boundary are mapped in
[the Sand plan](sands.md#conversation-and-task-surfaces-carried-for-fiote) and
[Fiote build notes](../../Fiote.md#how-this-fits-the-interface-refactor).

#### Long-horizon world foundation — future, not in the current cluster

- [ ] Specify engine-neutral `World`, `Frame`, `Layer`, `Placement`,
  `Disclosure`, `ViewProxy`, `Scenario` and artifact-reference semantics before
  building a World Sand. Keep Bevy, GPUI, CEF, map-provider and scene-authoring
  runtime ids out of their persisted forms.
- [ ] Prove globe-to-desk precision with authoritative geodetic/ECEF placement,
  nested local frames and camera-relative rendering. Test both real Earth and
  an authored world with a deliberate Earth anchor.
- [ ] Prove an out-of-core globe adapter with terrain, imagery, attribution,
  offline/cache ceilings and open 3D Tiles input. Compare a pinned Cesium
  Native adapter with an all-Rust path; do not make a hosted map service
  mandatory.
- [ ] Prove the layer/branch model by combining base terrain, a private exact
  placement, a city-level disclosure, a live local proxy, a point/volumetric
  or mesh-based capture, a fantasy castle and a desired-future layer without
  changing their source identities.
- [ ] Specify `SpatialArtifact` and select scene-construction, structured
  geometry and reality-capture capability boundaries. Preserve authoritative
  source/operations and capture provenance while treating tessellations,
  voxel/SDF fields, collision proxies, captured-scene chunks and LODs as
  derived, content-addressed, rebuildable representations with named error
  bounds. Named programs and libraries remain evaluated examples, not schema.
- [ ] Specify world collaboration and time: durable layer operations and
  accepted Actions use sync; avatar/presence/media and preview simulation use
  bounded session channels; observed and desired worlds can be diffed without
  either being overwritten.
- [ ] Specify the capture-to-work review path: model output is a sourced,
  confidence-bearing proposal; human editing precedes conversion into Needs,
  Contributions, Transfers or Actions; no model receives ambient mutation
  authority.

#### Box canvas

Workspace-control visibility, folding, triangle style and keyboard recovery are owned by Part A A02.1. Reuse and regress those controls in Box; this is not a second implementation task.

- [ ] Replace the fixed 10,000×10,000 world with unbounded logical coordinates,
  viewport culling, and recoverable navigation.
- [ ] Add recenter, bring-selection-here, locate-by-name, and minimap surfaces
  with honest empty cases.
- [ ] Use stationary direct placement first. Preserve movement code and its
  tests without running automatic forces or settling in this first Box.

#### Protein-area result templates

- [ ] Define Protein-area state: referenced current Protein item, spawn point,
  boundary, locked result-template group, result identity, refresh/removal,
  and honest unconfigured, empty, loading, and error states.
- [ ] Expose the current Protein result shape visually, including available
  field paths, value types, optionality, collection/object boundaries, and the
  stable row key supplied by each supported source.
- [ ] Add edit-mode field-to-Sand wiring with arrows, typed inputs, visible
  compatibility errors, fan-out/multiple inputs, disconnection, and no
  implicit field assignment or write authority.
- [ ] Let a person compose and lock one result-template group containing bound
  and unbound Sands, then unlock and edit that shared template deliberately.
- [ ] Make field presentation ordinary composition: dragging a schema field
  onto the template may create or bind a Text, Quantity, Badge, Record link or
  another compatible Sand; hiding that field removes or disables its child
  presentation without deleting the field binding itself. Allow several
  presentations of one field and one child fed by several compatible fields.
- [ ] Reconcile live Protein results into one stable group instance per row,
  preserving row identity and local Box state without duplicating or deleting
  underlying Records; refuse persistent repetition when a source declares no
  stable row key.
- [ ] Show the Protein item hash and source area in every bound group's metadata and provide locate/highlight navigation in both directions. Build the first Why-is-it-here view now: source row, field binding, appearance/template scope, layout exception and current permission or broken-binding reason. Explanation must not wait for moving Areas; later stages extend this same inspector.

#### Individual edits and presentation changes

Stage 4 follows [Box ownership](../box.md#individual-appearances-and-released-children)
and [presentation mapping](../box.md#choosing-another-presentation).

- [ ] Select a loose Sand by uid or a result child by appearance, row and child
  identity. A source Record uid locates matching appearances and asks which
  one when ambiguous. Show the editing scope before a change.
- [ ] Persist instance overrides independently of the template. Prove that
  updating, renaming and reordering Apple retains its style while Pear and a
  second appearance of Apple are unchanged. Test loose decorative Sands too.
- [ ] Separate movement attachment from logical ownership in the existing
  composition model. Release one child or every child; move the remainder,
  reattach without a visual jump, and preserve bindings and event scope.
- [ ] Copy and release a property Sand, with a fresh child appearance and the
  same inputs. Both views update; sessions and grants are not cloned. Verify
  exactly one Action for a gesture on either permitted editor.
- [ ] Retire attached, released and copied appearances when their row vanishes.
  Restore their overrides and placements within the recovery policy, including
  after restart. Expose owner, locate, reattach and remove controls.
- [ ] Separate basic Sands and packaged Castles in the picker, including saved
  user compounds. Select a Protein once and preview another compatible
  presentation without widening its data scope silently.
- [ ] Implement common/missing/extra field mapping, filling or hiding choices,
  required-input refusals, local Protein forks, and explicit override/released
  child remapping. Cancel, failed validation and Undo preserve the old view;
  hiding presentation never deletes data. Test Sand-to-Castle and
  Castle-to-Castle switches with both missing and extra fields.

#### Editing facets, size and focus

Stage 4 follows [Box focus](../box.md#sizing-and-focus).

- [ ] Offer Appearance, Connections and Data facets separately and in
  combination. Hidden facets keep their settings. Highlight the affected scope
  and the route of a selected connection; keep full inspection available.
- [ ] Implement minimum and optional maximum sizes per axis, invalid-range
  feedback, content growth and internal scrolling at an imposed maximum.
  Prove a growing document and a capped one with the same data.
- [ ] Focus one Sand without changing its saved maximum or template. Use the
  outer focused view to navigate long content, retaining caret, drafts and
  selection. Back restores the workspace; live refresh, source retirement and
  nested child limits have visible, tested behavior.
- [ ] Open a property's source Record in focus through the normal authorized
  read. Handle missing or multiple sources; do not invent a Record for totals.

#### Stationary property Areas

Stage 5 follows [field-to-Area behavior](../box.md#turning-fields-into-areas).

- [ ] Turn a displayed property into grouping or ordering through an exact
  field/order preview. Keep stable value identities, missing-value groups,
  row-key tie breaking and explicit behavior for multi-value properties.
- [ ] Compose two axes: assignee groups down a strip and due dates leftwards
  inside each group. Permit a property to remain visible on cards as well as
  Area labels. Unsupported query fields report what must be supplied.
- [ ] Place results directly, showing layout priority and conflicts. Preserve
  user overrides and independent released children; never reattach by layout.
  Save, refresh, restart and undo the whole arrangement.
- [ ] Make manual placement an inspectable per-appearance exception with
  Follow layout to restore arrangement. Moving across a group label alone
  never edits its property; expose ordinary field Actions separately.

#### Calendar, Clock and simulation

Stage 5 implements [Time](../time.md). Simulation is its separate domain-backed
lane after the stable gate; specialized temporal rendering needs no Area clock.

- [ ] Add a calendar-entry input contract and native leaf inside a Castle of
  ordinary controls. Wire authorized Record/Promise dates, date-field mapping,
  stable occurrence identity, timezone, period navigation and selection.
- [ ] Ship agenda, day/week and linear timeline views with point events,
  intervals, all-day and unscheduled lanes, overlaps and bounded pagination.
  Open the correct source from each entry and retain selection across views.
- [ ] Supply missing bounded Karma occurrence reads and date Actions together
  with the UI. Test repeat occurrences, interval boundaries, timezone changes,
  daylight-saving ambiguity, and schedule updates. No frontend scheduler.
- [ ] Implement previewed drag/date edits and their keyboard equivalent. Show
  single-occurrence versus series scope only where supported; failed writes
  preserve drafts. Reading or navigating time never executes scheduled work.
- [ ] Build the Clock over the same entries: rolling next hour in top view,
  configurable finite horizon in a local perspective spiral, interval segments,
  recurrence, overlap selection and an equivalent agenda. Label top-view
  filtering and Held time; camera/view changes never pause Karma.
- [ ] For simulation, provide a permission-filtered proposed-operation response
  for an Action, a Rule and a Transfer, including unsupported/unknown effects.
  Existing quantity forecasts alone do not satisfy this dependency.
- [ ] Show Live versus scenario, starting revision, assumptions, before/after
  fields, dates and reasons. Reuse Sands; keep filtered-out changes findable.
  Prove previews produce no real Facts or external effects, stale proposals
  are rechecked, and applying returns through ordinary authorized Actions.

#### Stable everyday-use gate

- [ ] In the running native interface, use one Protein with Apple and Pear.
  Customize only Apple, copy and release its quantity, move its attached
  remainder, refresh, remove/restore the row, switch Castle with a field
  mismatch, focus and edit, then restart. Verify each stated scope and lifetime.
- [ ] Group tasks by assignee and due date; show the same timed work in Calendar
  and Clock, including an interval and recurrent occurrences. Prove that view
  navigation changes no schedule and missing capabilities are not called done.
- [ ] Perform the workflows through pointer and keyboard/AccessKit, in light
  and dark styles with restrained motion. Test empty/error/loading states,
  rejected writes, data visibility, undo and recovery at representative scale.
- [ ] Update the human tutorial from these real controls and exercised flows.
  This gate requires working backend bindings and the UI, not fixture-only
  demonstrations. Simulation, shaders and moving Areas have their own later gates.

#### Force, sorting, mutation, and immunity areas

Stage 8, after stable use. Stationary grouping/sorting above ships first.
Each movement body is an attached part or one explicitly released child;
row ownership and visibility always follow the original result bundle.

- [ ] Define common area geometry, current-Protein selection, overlap and
  evaluation order, entry/exit lifecycle, styling, persistence, and edit tools.
- [ ] Implement force areas and the optional workspace-centering force with
  controllable pull/push, direction, range, collision, settling, and
  reduced-motion behavior.
- [ ] Apply a matching force to the attached part or explicitly released body.
  Never release a child implicitly, and never let separate placement outlive
  the source row or widen event scope.
- [ ] Implement directional sorting areas using current Protein sort semantics
  and fixed, internally scrollable bounds.
- [ ] Implement mutation areas for quantity changes and Concept addition or
  removal through existing typed Actions, with one trigger per boundary visit.
- [ ] Share each mutation visit across matching attached/released/copied bodies
  of one result appearance: first entry opens it and last exit closes it.
  Show which bodies keep it open and prove duplicate views never multiply
  the Action, including retry and recovery.
- [ ] Implement immunity areas attached to one Protein area. Protect that
  source's spawned groups from workspace centering and external force, sorting,
  and mutation areas while preserving internal areas and manual interaction.
  Show the boundary, protected source, blocked influences, and effective
  evaluation in edit mode and Why-is-it-here.
- [ ] Add deterministic overlap ordering, serialized Actions, loop/resource
  ceilings, pause/recover controls, and honest partial-failure states.
- [ ] Extend the existing Why-is-it-here inspector with structured causal metadata from forces, sorting, pins, mutation Actions and blocked influences. Preserve the earlier source, field, grouping and override explanation rather than replacing it with a physics-only inspector.
- [ ] Add edit tools for drawing, resizing, copying, stacking, styling, and
  removing areas without silently changing the data they currently contain.

#### Topology editing, surface views and free space

Stage 8 retained work. Do not enable it as part of the first stable Box or
delete its existing implementation. Group-body operations apply to attached
parts; released children retain independent bodies and the same result owner.

- [ ] Give each workspace one explicit spatial mode, `surface` or `space`, and
  keep camera projection separate. Surface mode owns logical `(x, y)` body
  positions; space mode owns `(x, y, z)` body transforms. Never run or persist
  two contradictory active simulations for one Sand. Pinned viewport Sands
  remain outside both world simulations.
- [ ] Define the versioned topology document shape as ordered compact stamps
  and effects rather than a persisted mesh: stable uid, anchor, local
  transform, primitive profile, extent, signed height/depth, steepness,
  falloff, plateau/flatness, blend strength, current-Protein filter, visual
  style, enabled state, ordering, and optional Area or Sand attachment.
- [ ] Implement the common base field and per-group filtered potential field.
  Sample their gradient deterministically inside the same fixed step as other
  forces. Apply the result to each movement body, respect immunity and
  collision, and never derive different physics from the selected camera.
- [ ] Let conservative force areas expose an exact potential visualization
  without applying a duplicate force. Keep sorting, mutation, constraints, and
  non-conservative fields as lanes, gates, arrows, or overlays unless a
  separate Topology Effect is authored.
- [ ] Build topology edit mode with circle, square, ridge/line, flatten,
  smooth, raise, and lower brushes plus move, resize, reorder, copy,
  enable/disable, inspect, undo, and remove. Use preset profiles and sliders
  for height/depth, radius, steepness/falloff, and top flatness; formula entry
  remains deferred.
- [ ] Support world-anchored and Sand/group-anchored effects. Attached effects
  follow their anchor and do not act on it by default. Bound gradients,
  affected-body counts, oscillation, and cycles; show pause, failure, retry,
  and disable controls rather than silently dropping work.
- [ ] Add base-terrain, selected-group, selected-filter/effect, and neutral
  overview lenses. In 2D show contours, gradient arrows, boundaries, color,
  and pattern distortion. In 3D show the selected effective surface and
  matching groups without pretending nonmatching groups share it. Keep
  each matching group's support point and orientation glued to the sampled
  height and normal. Keep ordinary Sand text crisp through a reading face
  mounted on the terrain-bound body and leave viewport-anchored Sands outside
  terrain.
- [ ] In surface mode, keep each group's authoritative simulation position in
  one logical plane. Switching between Top and Perspective changes camera and
  explanation only; it cannot rerun, resettle, or persist a contradictory
  position. Raycast perspective input back into the same logical coordinates,
  preserve focus/selection across camera changes, make a dragged group
  kinematic only for the gesture, and move attached effects with their anchor.
- [ ] Build free-space mode with authoritative 3D transforms and no topology
  floor, contact, or gradient force. Project Protein, force, sorting, mutation
  and immunity Areas as declared volumes; give forces 3D vectors, sorting a
  local basis, and entry behavior volume crossings. Keep ordinary cards
  readable without forcing physical tumbling, while allowing specialized 3D
  Sand projections to expose orientation.
- [ ] Render the local `z = 0` collapse plane as a thin, nonphysical reference
  in space mode. Show Area footprints and optional selected-entity projection
  lines without turning the plane into a collider, support, topology force or
  second simulation.
- [ ] Implement expansion and collapse as explicit previewable, atomic and
  undoable Box operations. Surface-to-space preserves `(x, y)`, initializes
  Sand `z` from its effective terrain and extrudes Area footprints. Space-to-
  surface orthographically projects `(x, y, z)` to `(x, y)`, squashes Area
  volumes to declared footprints and places Sands on their effective terrain.
  Preview overlap and bounds consequences, retain exact source transforms in
  operation history for undo, and never silently translate dormant topology
  into a 3D force.
- [ ] Let every Area/effect configure boundary, color, opacity, contour,
  pattern, and pattern-distortion presentation independently of physical
  strength. Extend Why-is-it-here with sampled height, gradient,
  contributors, filters, immunity, direct forces, constraints, and
  collisions.
- [ ] Add Protein admission controls for all-at-once, bounded-batch, and
  rows-per-second cadence plus travel-from-spawn, pre-settle-then-reveal, and
  direct-placement policies. Materialize a complete result group atomically
  before physics admission; bound pre-settle work and reveal an honest
  unsettled state when its budget expires.
- [ ] Prove one surface circuit: a Protein source feeds a visible slope,
  filtered branches route groups through lanes, and terminal pits collect
  distinct cohorts. Show the same deterministic result from Top and
  Perspective cameras, then reproduce a flat Kanban-like direct-placement
  configuration from the same primitives.
- [ ] Expand that fixture into space, move Areas and groups above and below the
  collapse plane, cluster cohorts using only 3D force volumes, preview the
  projected footprints, collapse it deterministically to surface mode, and
  undo back to the exact 3D transforms. Verify Sand identity, bindings, state,
  focus, group integrity, Area order and Why-is-it-here explanations throughout.
- [ ] Establish scaling curves for stamp count, dirty field tiles, active
  bodies, 2D gradient sampling, 3D volume queries, mesh/pattern generation,
  camera changes and surface/space conversion. Start with deterministic CPU
  field sampling and use GPU tile/mesh work where it avoids readback; move
  physics kernels to GPU only after measurement.

#### Canvas base pattern

- [ ] Render the recursive dot-to-plus-to-mesh pattern in the shared world
  layer with stable zoom thresholds and without shimmer or pattern drift.
- [ ] Add the 0–100% arm-length control plus pattern scale, color, opacity, and
  level-of-detail controls.
- [ ] Add content-addressed raster and sanitized inert SVG wallpapers with
  fit/repeat, scale, position, opacity, inspect, replace, and remove surfaces.

#### Interaction and navigation

- [ ] Specify keyboard focus traversal, a command palette, remapping, visible
  shortcut discovery, and conflict handling with text editors and embedded Web
  pages.
- [ ] Dragging a supported file or URL asks the renderer registry which Sand
  can display it; an image or PDF can choose Document Viewer. A drop never
  silently creates Ledger data.
- [ ] Add performant drawing as compact Box data rather than bitmap snapshots.
  Its operation shape must participate in the live Box transaction stream
  without sending bitmap snapshots. Offline multi-writer drawing merge remains
  deferred.
  Benchmark complex diagrams and painting-like frames against the interaction
  and culling behavior people expect from tools such as Excalidraw.

#### Box-state persistence

Stage 3 establishes snapshots, operations, undo and ordinary restart recovery.
Stages 4–5 persist their additions as they land. Spatial checkpoints and
moving-body proofs below are stage 8 extensions, not prerequisites for saving
a stationary workspace.

- [ ] Replace copied Sand HTML in instances with content-addressed definition
  references plus small override patches.
- [ ] Store a Box-built group as a workspace-owned local Sand definition plus
  an instance placement. Its children, relative transforms, internal
  connections, Behaviors, and exports live in that definition; top-level
  connections and Protein bindings live in Box. Locking is persisted editor
  state, while save-as-Castle promotes the same shape into the reusable
  definition catalog.
- [ ] Make the Box operation model able to create, edit, validate, fork,
  promote, and instantiate that definition shape without copied HTML or group
  membership duplicated onto every child. Code-owned Maud definitions are
  override-or-fork and Box never attempts to rewrite their Rust source.
- [ ] Define the versioned Box document schema with stable uids for every
  authored entity; referenced Sand revisions; recursive group-local
  transforms; logical ownership separate from movement attachment;
  appearance/row/child override identities; released-child placements and
  local copies; world/viewport/group anchors; semantic layers and sibling order;
  workspace spatial mode and local collapse-plane frame; surface placements
  and free-space transforms; Protein references and field-to-port bindings;
  Area footprint, volume, projection and local-basis definitions; connections;
  override patches; topology stamps/effects and their anchors; persistent
  host-state allocation; and content-addressed assets.
- [ ] Keep a readable compact snapshot plus a typed operation journal. Validate
  both at runtime, batch one human gesture atomically, recover from an
  interrupted tail, compact without changing meaning, and retain the last
  known-good snapshot when an offline edit is invalid.
- [ ] Choose the snapshot grammar and extension explicitly. Reuse `.lingua`
  only if it is parsed, formatted, and checked as the same Lingua grammar;
  otherwise use a separate `.box`-style format. Publish a formatter, checker,
  structural diff, and concise author reference with it.
- [ ] Expose a typed Box-operation write API and read-only Box projection for
  the Web client, other programs, and agents. Live callers never modify the
  snapshot file behind the process; authorization, schema validation, limits,
  attribution, undo, and failure reporting apply equally to human and agent
  edits.
- [ ] Persist direct manipulation at semantic commit points rather than each
  pointer or physics frame. Journal drag/resize completion, anchors, groups,
  connections, configuration, topology stamps and Area edits. Harvest
  coalesced spatial checkpoints only at a completed fixed-step boundary when a
  body settles, reaches a stable effective destination, or exceeds a bounded
  maximum recovery age while continuously moving.
- [ ] Persist surface checkpoints as logical surface coordinates plus local
  orientation and space checkpoints as full free-space transforms. Ordinary
  work Sands reopen at rest; only an explicit continuous-simulation policy
  retains bounded velocity state. Never persist Area membership, solver
  caches, runtime handles, topology meshes, GPU buffers or visibility.
- [ ] Prove crash and clean-restart behavior with Sands spawned by Protein,
  transported by topology/Areas and settled in a destination. The last
  complete authored transaction always survives; documented recovery lag is
  bounded to the spatial-checkpoint policy.
- [ ] Separate local journal append/fsync, spatial checkpoint, snapshot
  compaction, File projection, and contact-delivery rates. Ordinary Lingua
  File Sync keeps its existing projection policy. A slower external delivery
  rate never weakens local durability or changes Box operation meaning. Expose the configured backup/checkpoint cadence, last durable revision, pending state and recovery lag through ordinary settings; a person need not edit a file to change the promised frequency.
- [ ] Measure actual write volume, recovery after interruption, compaction,
  state growth per Sand, spatial-checkpoint churn, sparse versus bulk encoding,
  and the cost of unbounded workspaces.
- [ ] Separate reusable workspace composition from personal view state now so
  synchronization does not need to unpick them. Sand placement, size,
  configuration, definitions, groups, connections, zones, and drawings are
  composition; camera, focus, selection, open panels, and temporary portals are
  personal view state.

#### Sand visual effects

Stage 9 follows [the effect contract](../shaders.md).

- [ ] Define the supported fragment-effect inputs, source profile and package
  assets; validate syntax, bindings and bounded work before running user WGSL.
  Start with presets and named settings, then expose the supported source editor.
- [ ] Support a bounded outward glow margin with correct clipping/culling and
  unchanged hit targets. Vertex/compute effects and scene lighting need their
  own later contracts, not hidden access through this surface effect.
- [ ] Keep last-good preview, readable content and external Reset/Disable
  controls. Test invalid source, reduced motion, device loss, persistence,
  one-appearance overrides, package credits and measured GPU cost. A successful
  shader compilation alone is not this feature's completion gate.

#### Live workspace collaboration

This is shared Box composition, camera/presence and host-ordered workspace state. Dogfeeding's authenticated live company Record/task editing is owned by [Backend Part A](../../backend-part-A.md) and already required before this stage. Do not defer remote task access to these Box protocol tasks or require shared layout merely to use a company Kanban.

- [ ] Add one Workspace lane beside Record/Organ sync and File projection in
  Protein's Synchronization surface. Share invitation, contact identity,
  owner/editor/viewer authority, status, cursors, recovery and error
  vocabulary without merging their typed operation schemas.
- [ ] Define a distinct versioned workspace protocol for hello, permission,
  snapshot hash/chunks, ordered transaction tail, editor intent, accepted
  canonical transaction or structured refusal, durable spatial-checkpoint
  batch, ephemeral preview/presence, gap recovery, access loss and host end.
  Unknown versions and operation kinds fail closed.
- [ ] Make one Cell the live session host and sole owner of the canonical Box
  revision, durable store and active physics simulation. Editors send typed
  intents with stable ids and base revisions; the host validates authority,
  limits and semantic preconditions before ordering and persisting them.
- [ ] Reuse existing authenticated contact transport, grants, durable delivery
  and health infrastructure, but do not add workspace variants to the Record
  operation schema and do not make a remote peer a filesystem provider.
- [ ] Keep cursor, camera, selection, media and high-rate transform previews
  ephemeral. Permit optimistic local interaction, then confirm or correct it
  from the host's canonical transaction. Persist and deliver coalesced spatial
  checkpoints through the same Box store used for restart recovery.
- [ ] Let a guest cache a verified snapshot for quick reconnect and an
  explicit read-only unavailable-host view. Do not permit offline edits,
  implicit host promotion, automatic failover or multi-writer replica merge in
  the first version. An explicit fork creates a new workspace lineage.
- [ ] Ship the human surface with invite, role review, connected/synchronizing/
  caught-up/offline/host-ended/access-lost states, pending and durable revision,
  checkpoint recovery lag, rejected-edit explanation, leave/revoke, limits and
  honest empty cases.
- [ ] Test concurrent intents, stale base revisions, duplicate/reordered and
  malformed frames, missing snapshot chunks/assets, host crash/restart,
  revocation, oversized topology/definition edits, guest reconnect, checkpoint
  convergence and zero Record/Action authority implied by Box edit rights.

#### Public Live Facade

- [ ] Define a versioned Facade publication manifest that references one
  validated Box composition revision, its content-addressed definitions and
  assets, token/style revision, and an allow-list of saved Protein projections,
  fields, limits, stable keys, and read-only Behaviors.
- [ ] Add a publish review showing the exact public Organ, Records/fields,
  Proteins, definitions, assets, external links, estimated size, and rejected
  write/network capabilities. Publishing and revoking have visible progress,
  success, stale, and failure states.
- [ ] Serve the public subset from a separate public Organ and a separate Web
  origin. Expose only static Facade assets and resumable read-only Protein
  streams; do not mount Action, arbitrary-query, administration, private media,
  terminal, filesystem, Sand-install, or Lince credential routes there.
- [ ] Make publication to the public Organ an explicit, field-narrowed
  replication policy with deletion/revocation propagation and visible lag.
  The Facade-serving Cell holds only that materialized public subset and has no
  capability to author data back into the private Organ.
- [ ] Compile view mode from the same compound Sand definitions used in Box.
  Strip edit mode, configuration, mutable ports, and mutation areas. Keep local
  navigation, Record selection, disclosure, page/chapter state, pan/zoom, and
  read-only sorting/force behavior.
- [ ] Keep visitor state in memory by default and optionally in a small
  Facade-uid-and-revision namespace in browser storage. Add inspect/reset and
  quota behavior; never upload it or interpret it as Ledger or shared Box
  state.
- [ ] Enforce the public runtime boundary with a separate origin, restrictive
  CSP, no ambient credentials or analytics, bounded stream messages and
  storage, sanitized Record rendering, safe external-link confirmation, and
  no Website or arbitrary network-capable Sand in the first version.
- [ ] Test that every declared Action, write port, mutation area, arbitrary
  Protein, private field, remote asset, Website Sand, unsafe CSS, unknown
  contract version, oversized stream item, and cross-origin credential attempt
  is rejected rather than hidden or partially applied.
- [ ] Keep Archive Facade and Live Facade as explicit delivery choices. The UI
  explains that a direct live server observes connection metadata while a
  content-addressed cached archive can avoid contacting the author; neither
  mode claims privacy it cannot provide.

#### Native follow-through and cross-feature surfaces

Stage 7 closes the remaining native surfaces, including [migrations excluded from Dogfeeding](native-follow-through.md). Independent tasks may start after Dogfeeding when their actual domain dependencies permit; they do not wait for unrelated Box, CEF or interfaceless work. Shared editing, Role/Protein/property authorization, native Organ live access, activity and Trash/Restore already belong to Dogfeeding and are reused here.

The [additional editor work](../product.md#collaboration-and-the-editor) retains its canonical tasks for recent displaced-edit visibility, additional scalar binding paths and their slug policy, the title-to-create Note lifecycle, and the shallow-snapshot optimization. Shared Record/embedded/Table editing already covered by Part A is reused and proved there; the remaining tasks must be checked against current source before implementation. Shallow snapshots are a separate storage optimization, not a prerequisite for a usable editor or Box. The eleven Ontology surfaces immediately below also belong to this native lane and include any named missing domain operation with their UI.

- [ ] Expose the Program-backed Karma workflow through native Sands: author and revise Programs and their Frequencies, activate/pause supported revisions, edit supported parameters, inspect runs/candidates, respond to a candidate and inspect/create/narrow/activate/revoke the required grants through the current Rust Actions. Show cycle/proof errors at the offending input and distinguish Recurrence polling from the director path. Preserve expected revisions, permission and admission refusals, and prove an isolated occurrence-to-candidate-to-review flow without claiming unsupported external execution. Scheduler migration, timezone artifacts and intent execution remain named Karma dependencies, not another frontend scheduler or a reason to hide already-supported operations.

[Communication](communication.md), [Fiote](../../Fiote.md), [Rooms](../../Rooms.md), [Files](../../Files.md), [Secrets](../../Secrets.md) and [Code](../../Code.md) own their additional feature work and usable native surfaces together. They are not silently imported into Part A; accepted behavior already present at its launch is preserved. Agent-only controls remain Fiote work, browser-hosted panes additionally wait for the CEF lane, and camera/call/media transport retains Communication's own gates. New primitive ideas in the product's software-archetype table are design probes, not automatic promises to implement a spreadsheet, chart system, IDE or game engine in v1.

### Ontology surfaces this interface owes

These are interface debts, not a claim that every domain operation is reachable. Dogfeeding now owns company access/administration and Trash/Restore through [Backend Part A](../../backend-part-A.md) and Interface A03.3/A06.3. The other device/contact/publication surfaces remain later unless an actual company dependency is explicitly cut; a live employee login does not by itself require company roster enrollment.

Two rules apply to all of them. **State the honest empty case**: a panel
showing nothing has several meanings ("none yet", "not switched on", "cannot
reach anyone") and must say which. And **absent is not blank**: render
`undefined` as "withheld" and `""` as empty, so a narrowed view never draws a
permission boundary as data.

- [ ] **A summary of what each contact can see of you.** The single most-wanted
  read here, and the one a per-contact panel can never answer, because each
  contact's sharing level is set one at a time in its own panel and nothing
  answers "who can see what of mine right now". A read-only aggregate over the
  existing `organ_contact` rows — direction, both scopes, hide-list count, and
  the unreadable-scope flag `store::organs` already derives. No new storage, no
  sync change. One screen, every contact side by side, each row saying
  everything / only these columns / nothing but which Record, with the broken
  ones called out; clicking a row opens the contact panel that owns the change.
- [ ] **Per-device sharing limits.** A device is all-or-nothing today: full
  access to your Organ, or logged out. Give roster entries the `scope_fields`
  column the per-contact scope already has (migration `0056_contact_scope`) and
  evaluate it in the same predicate so the two cannot drift. **A different axis
  from capabilities** — capabilities say what a device may DO, this says what it
  may SEE. One scope control per device, beside Remove.
- [ ] **A capability editor per Cell, so a stolen phone can be NARROWED rather
  than revoked.** The mechanism is signed into the roster and enforced —
  `CellEntry.capabilities` (`engine/src/roster.rs:105`) and `cell_may`
  (`roster.rs:690`), which `wire.rs:1828` already calls. No Action edits one
  Cell's capabilities; `roster-status` only reads them to decide whether to show
  a relay badge. An editor that republishes the roster, needing the root like
  every roster change, beside Remove. "This phone may no longer write, but stays
  in the roster" as one control.
- [ ] **The succession chain, rendered.** `store/src/roster.rs:245-304` stores
  and reads the `identity_succession` chain. Nothing shows it.
- [ ] **A reach control per contact.** `contact.mode` decides mailbox versus
  direct delivery and is load-bearing in `engine/src/wire.rs` — and NOTHING
  SETS IT: `organs::set_mode` has exactly one caller and it is a test. One
  control on the contact panel. Note the two enums no longer share a name: the
  contact-side one is `store::organs::Delivery`.
- [ ] **A Move button.** Move is built end to end — `Action::MoveRecordTo` and
  `Action::CancelRecordMove`, `store/src/record_move.rs`, and the exactly-once
  hand-over in `engine/src/share.rs`. The dangerous half is the half that
  exists; there is simply no way to reach it. "Place this project in the family
  Organ" as a consented offer the other side accepts.
- [ ] **The rate-limit line on the contact panel.** `store::contact_rate` backs
  off a contact that spends its hourly allowance of refused ops or whole-log
  serves. `states()` gives count, allowance, window and backoff per kind, and
  `clear()` lifts one by hand. The panel says "we are now answering them less
  often" WITH THE REASON. Empty is "nothing has counted against them", never a
  blank.
- [ ] **"No device list yet — reconnect to finish", per contact.** A contact
  paired before rosters travelled has `awaiting_roster_since` set (migration
  `0074`); after a seven-day grace their batches are refused. A state a person
  cannot act on is worse than no state, so this is a line plus a button that
  reconnects, not a passive label. Cleared automatically once a roster arrives.
- [ ] **One pending-offers list.** `store::offers::pending` returns all four
  handshakes in one vocabulary — kind, direction, subject, title, other party,
  when — over `invites`, `replica_grant`, `record_move` and
  `transfer_invitation`. Every kind side by side, accept and refuse on the row
  routed to that kind's own verb, one honest delivery state per row rather than
  receipts in one place and checkpoints in another. **A refusal must not look
  like an offer that was never answered**: three of the four kinds deliberately
  do not remember a refusal, and the thread invite's silence is a privacy
  decision (a sender must not be able to tell declined from ignored).
Trash/Restore is now owned by Backend Part A B16 and Interface A06.3. It reuses the newer-write undelete rule and retained Loro document rather than adding a second lifecycle layer; current access is checked on restoration.

- [ ] **The read-model health line.** `Engine::read_model_health()` returns when
  the read model was last checked against the log and what was found. **`None`
  means no pass has run yet on this Cell and must not be drawn as a tick** — it
  is the difference between "healthy" and "never looked".
