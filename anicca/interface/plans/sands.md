# Sand implementation plan

Implementation decision, 2026-09-07: new native Sands use Bevy directly,
including scenes, components, widgets, text, observers, picking and rendering.
The landed sections below are prototype evidence, not a frozen native
projection ABI. Existing backend/data boundaries may be translated; existing
interface code may be rewritten. No paired HTML constructor, generic renderer
interface or separate retained view tree is required. The [architecture](../architecture.md#bevy-native-interface)
defines scoped plugin/crate/WGPU exceptions.

Purpose: Define production Sand schema, authoring, composition, package, HTML, Website, and migration work.

Owner source: no dedicated Sands Record currently exists;
[Interface in Lince](../../Lince.lingua) governs shared interface decisions.

Status: Coordinated with Customization C4-C5; semantic kernel, primitive
Gallery, recursive composition and C3 external-authoring layer landed, and Box
follows the gate.

Read when: implementing the Sand contract or external runtime surface.

[Corpus map](../README.md) · [Current context](../current.md)

---

## Landed C1 boundary

Sand schema and ABI version 1, persisted/runtime separation, projection
manifests, package/hash/license validation, generated JSON Schemas and
valid/stale fixtures have landed. The 19-definition retained Gallery and its
Installed HTML/CSS/ES-module projection use that contract; the Website
companion retains no Lince authority. Keyboard, pointer and AccessKit actions
reach the retained state, a Protein-shaped mount reaches Installed HTML and
`record-clicked` returns through its exact event grant. Release parity and the
joined Wayland report pass. Package admission resolves only declared relative
static module imports, and the live Installed decoder proves malformed
unknown-field refusal before accepting a valid mount.

## Landed C2 boundary

Composition schema version 1 adds the exact-revision catalog, strict artifact,
placement, typed binding and lineage records above the Sand graph. One
Rust-owned recursive host mounts definition, child and instance patches,
chooses every declared adapter kind and owns all renderer and Behavior
handles. Shared edits propagate transactionally through exact ancestors;
invalid publications preserve the last-good catalog. Save as definition,
save/reopen, lock, override/reset and fork/detach are normalized operations,
not renderer-specific shortcuts.

The F10 workbench and its AccessKit nested Button are the human surface. A
standalone Button and twice-nested video-call compound expose the same typed
identity through native retained and Installed HTML projections. Protein
reads, Sand events and Action writes have visibly different arrows. Installed
HTML creates collision-free instance DOM ids, repairs label and ARIA
references and explicitly tears down its listener. Rust construction,
recursive Maud output and workbench artifacts agree in the release parity
report, and the joined Wayland report passes. Castle remains only a human name
for a saved compound.

## Landed C3 boundary

Configuration schema version 1 wraps the exact composition artifact without
creating another definition graph. The 22-definition Configuration Sand uses
the same primitives and exposes global/group/instance style, definition,
Behavior, port, isolation/capability, developer-CSS, undo, inherit, launch and
persistence operations through F11, pointer, keyboard and AccessKit. One
resolved cascade reaches native, Installed HTML, shared HTML and browser roots.

The versioned external-author manifest pins package uid, normalized graph hash
and exact roots. The generated seven-file kit includes schemas, a valid
ordinary HTML/CSS/ES-module package, an unknown-version refusal, token
reference, launch recipe and guide. The renderer-neutral recipe materializes
ordinary placements, typed Record reads and Action exports and persists a
domain receipt so reopen focuses instead of cloning. These completed entries
are prose and generated evidence rather than remaining tasks.

## What is left

### Landed first C4 structure boundary

The official migration catalog now validates 25 root Sands and 72 total
definitions built in Rust. It decomposes common workflow structure into
placeable primitives and compounds and makes the remaining specialized
renderer boundary explicit. F12 exposes root selection, typed ports, legacy
source, readiness and the recursive tree through keyboard, pointer and
AccessKit. Configuration reports landed. Edit controls, zoom controls, Record,
Conversation, Table, Todo and Kanban report native retained Behavior. An isolated laboratory run
uses deliberately representative Protein-shaped input, while the production
desktop binds Record, Conversation and private-draft results through three live
Protein subscriptions. Message, draft and Record-quantity writes use
acknowledged Actions; the other 17 roots truthfully keep runtime Behavior
pending. The collection roots remain active C4 work beyond their landed core.

The first operational slice projects the exact recursive definitions into a
renderer-neutral retained scene with stable semantic paths, canonical style
roles, Glyphon text, WGPU rectangles, hit testing and a dynamic AccessKit
subtree. F12 then Enter/Space opens the seven available roots. Their pointer,
keyboard and accessibility actions operate edit/group/Castle state, zoom and
recenter state, Record property presentation, local draft text, message-send requests
and a typed `record-clicked` identity. Generic toolbar actions use a generic
Boolean trigger; only the deliberately Record-bearing action can emit the
Record event. Domain failure is visible and reconnecting. Conversation keeps
author/operator and lifecycle state, coalesces durable private-draft edits and
does not consume draft state before a successful Action reply.

The first collection slice adds a renderer-neutral repeated-placement seam:
each stable Protein uid creates an instance of the same Table-row or
Record-summary definition. Table paging and creation, Todo creation/completion
and default Kanban quantity-lane movement are operable through the same scene,
focus, AccessKit and Action boundaries. Table inline editing/deletion,
user-selected Protein, saved Kanban lane/concept presets, selection/bulk work
and swimlanes remain explicit work in this stage.

The composition host was repaired before this catalog could rely on it.
Protein or fixed values bound to a public compound input now flow through
nested input exports to the eventual child, declared defaults participate in
mounting, and two routes to the same terminal input are refused. This is C2
correctness, not a C4 workaround.

### Sand

First-party Bevy is the baseline, not every community crate named `bevy_*`.
No Lyon, Vello or parallel text/layout toolkit is selected by default. Flair
is the preferred CSS-authoring integration on Bevy components; it does not
replace Bevy UI or require a portable Sand constructor.
Use matching AccessKit types for custom accessible roles and necessary native
platform libraries without creating another interface host. Build spatial
Sands on the 3D-capable scene model; Part A remains planar and stationary.
Avian 3D belongs to later moving Box work. Pinned/Face viewer orientation is
specified in [Box](../box.md#sand-facing-in-3d).

Current extensions expose registered editable components and named effects,
plus trusted Rust plugins. Untrusted executable installation and sandbox work
are deferred until an explicit later decision, not needed for native authoring.

The active native migration, constructor/package conformance and author guidance are owned once by [Part A](part-a.md). [The build rule](../build.md#no-embedded-browser) makes the native client/server products free of any embedded browser. Part A is now Dogfeeding's company-workflow subset with [backend foundations](../../backend-part-A.md); other roots remain in [native follow-through](native-follow-through.md). Pure Maud/HTML export and metadata validation never needed a browser runtime. Installed HTML execution, Website and in-desktop previews do not wait for a lane: they each need a way to run without embedding a browser in Lince. The boxes below preserve later Box and external-authoring work, not a second C4 checklist. Correctness needed by an existing Part A workflow is repaired in its owning node rather than postponed under this heading. External package boundaries never make native Bevy widgets wait for a browser runtime or portable ABI.

- [ ] Define the Bevy-native authoring and saved-composition path using
  components, scenes and stable Sand ids directly. Register editable fields,
  typed ports and named effects; validate allowed persisted values, exact
  definition references, assets and licenses, and remap entity references on
  load. Private helper entities and ordinary Rust callbacks need no portable
  schema. Box and code-built Sands share the same composition, not two trees.
- [ ] Keep translation and generated validators at actual existing backend,
  storage, network or external-package boundaries. Do not generate native UI
  and world adapters merely to normalize Bevy. Preserve ownership, exported
  scope, permission checks, last-good state and teardown in direct Bevy code.
- [ ] Use Bevy UI/text, retained gizmos and curves, meshes/materials and assets
  for patterns, zones, connections, drawings, selection and native leaves.
  Feed custom line/area hit tests into Bevy picking. Add a custom plugin,
  internal/external crate or pure WGPU pass only for a named need, normally
  sharing Bevy's device, resource lifetime and presentation.
- [ ] Preserve the bounded shader-authoring and external-code security rules.
  A native plugin is trusted process code, not a sandbox. Validate resource
  budgets, executable hashes/capabilities and LICENSE/NOTICE/credits at the
  actual external boundary, not through per-widget native ABI envelopes.
- [ ] After the Customization completion gate and official-Sand migration,
  prove the Box model vertically with one current Protein item, visual result
  fields, a mixed bound/unbound result-template group, repeated row instances,
  and stationary grouping/sorting areas. Released children, appearance edits,
  presentation switching and focus follow the
  [Interface plan](interface.md#individual-edits-and-presentation-changes).
  Do not use this proof to finish the
  component or composition foundations underneath it.
- [ ] In that vertical proof, remove and restore one stable Protein row and
  change one result field incompatibly. Show live, retired, restored and broken
  binding states, retain bounded recoverable instance-local state, and provide
  visible reconnect/clear/replace operations through **Why is it here?**.
- [ ] Expose the landed instantiate/override/reset/save/fork operations in Box
  edit mode. A code-owned native or Maud definition is never rewritten; Box
  edits a visibly forked user definition and preserves lineage and revision
  inspection.
- [ ] In the same edit mode, let a person inspect the selected Sand's typed
  inputs, outputs, configuration and child tree; attach a Protein field or a
  fixed value to an input; add an independently placeable presentation Sand
  for a property; remove or disable that child to stop showing the property;
  and reconnect emitted events to declared Box events or typed Actions.
  Property visibility is semantic composition, never CSS-only hiding that
  leaves an interactive or accessible node alive.
- [ ] Let a person select arbitrary Sand instances, preserve their relative
  transforms and connections, glue them into one locally locked compound
  group, unlock and rearrange its children, expose selected child ports, and
  save or fork the exact group as a reusable Castle. Grouping creates one
  recursive definition and one placement rather than copying HTML or storing
  group membership redundantly on every child.
- [ ] Extend this same composition model for logical ownership separate from
  movement attachment. Release and copy-and-release preserve the owning
  Protein row, input/event scope and teardown. Do not confuse these operations
  with forking a definition. Prove them through the stage-4 Interface checklist,
  including refresh and restart, before declaring the extension complete.
- [ ] Add the basic-Sand and packaged-Castle picker sections, compatible field
  mapping and appearance/template scope choices to the shared authoring
  surface. The calendar/timeline native leaf and Clock are new v1 catalog
  additions described in [Time](../time.md), after the existing C4 migration.
- [ ] Implement custom surface effects through [Shaders](../shaders.md) in the
  later v1 effect stage. The existing GPU capability/package checks are a
  foundation, not a completed WGSL editor or a guarantee of bounded GPU work.

### End-of-v1 external-content design and retained requirements

At the end of v1, choose browserless designs or explicit system-browser handoff for specialized content. This does not schedule an untrusted executable plugin system; the owner deferred that separate decision. Its historical package/execution tasks below are retained research requirements, not work to perform automatically at the end of v1. The detailed package, permission and browser cases below are retained requirement/research material, not an instruction to rebuild their former execution machinery. The embedded browser they assumed is gone, so none of them can be built until installed HTML and Website each have a way to run without embedding a browser in Lince; see [the build rule](../build.md#no-embedded-browser). They are kept because whatever replaces those surfaces inherits the same obligations. The native schema and package inspection already needed by Part A stay in Part A. Browser/Plan B cases here test portable projections; they do not authorize a general browser client for a Cell.

- [ ] Define and version the Sand manifest, bridge handshake, typed ports,
  capability vocabulary, provenance record, resource limits, CSP, and package
  signature/integrity rules together.
- [ ] Prove the installed external path with one installed HTML Sand that receives a
  mapped Protein Record summary, emits `record-clicked`, consumes a Box event,
  keeps local browser state, and requests one granted typed Action. Run the same
  semantic fixture through Plan B `MessagePort`. Reject undeclared ports,
  malformed values, excessive size/rate, spoofed instance identity, and the
  same messages from an arbitrary Website.
- [ ] Replace the current broad iframe grant with the trust tiers above and
  prove that a denied Sand cannot reach Actions through another Sand or leak
  data through lanes, navigation, popups, downloads, or network requests.
- [ ] Add import, inspect-before-run, permission review, update review,
  revoke, disable, and delete surfaces with honest failure and empty states.
- [ ] Add a runtime-health and admission surface for heavyweight Sands. Show
  process and texture cost, configured budget, denied starts, renderer crashes,
  recovery attempts and the exact unavailable reason; keep retry, disable and
  clear-storage controls reachable without opening developer tools.
- [ ] Build the Website Sand without embedding a browser in Lince, and a sandboxed iframe in browsers. Keep origin/security chrome above
  remote pixels and prove that Website content cannot invoke Lince native
  APIs, overlap system chrome, or receive a privileged parent message. Moving
  it off-camera culls composition only and does not unload, suspend, or throttle
  its browser execution.
- [ ] Enforce HTTPS navigation; deny custom protocols, filesystem access, and
  all Lince native capabilities; and harden every local HTTP/WebSocket
  endpoint against foreign origins, unauthenticated requests, and CSRF. Where an
  engine or dedicated WebView allows it, additionally intercept requests to block loopback,
  link-local, private-network destinations, unsafe redirects, and DNS
  rebinding. In a browser iframe, disclose that broader private-network egress
  cannot be guaranteed rather than presenting it as enforced.
- [ ] Add the isolated per-origin Website profile, explicit persistent/private
  modes, storage quotas, usage inspection, clear-data controls, and tests for
  cookies, local storage, IndexedDB, Cache Storage, service workers, restart,
  and cross-instance sharing.
- [ ] Add per-origin permission and activity surfaces for downloads, popups,
  external protocols, clipboard, camera, microphone, location, notifications,
  screen capture, and storage-access requests. Denial and platform limitations
  have honest in-Sand explanations.
- [ ] Test malicious Websites for local-network requests, CSRF against Lince,
  navigation spoofing, popup escape, downloads, resource exhaustion, tracking
  identifiers crossing profiles, and frame/IPC confusion. Keep any WebView or
  browser runtime patched; sandboxing does not eliminate engine exploits.
- [ ] In browser/Plan B iframe mode, detect sites that prohibit framing and
  offer an explicit open-in-browser fallback. Never strip or proxy around
  `frame-ancestors` or `X-Frame-Options`. In any Plan A surface, detect sites,
  authentication, protected media, or policies that reject the runtime and
  offer the same fallback.
- [ ] Remove the legacy nested-payload frame and old Lynx component API during
  the rebuild. Route `.lince` imports by inspected content rather than a
  legacy filename suffix; unknown shapes fail closed.
### Later package distribution

Package distribution is independent of whether the recipient can execute an embedded browser projection. Disabled projections remain inspectable metadata, not runnable content.

- [ ] Federate published Sand packages between connected Organs while
  preserving content hash, lineage, author, capabilities, and licenses. The
  contract must not depend on whether bytes live on disk or in a future object
  store, and it must not require a central registry.

### Later data-authoring exploration

The [MorphoHDL example](https://github.com/paradigms-of-intelligence/morpho) combines a small circuit-description language, recursive graph rewriting and a live viewer. For Lince, the useful question is whether a person can describe a structure, inspect its growth and understand what would be created. This is an exploratory use of Records, Trails, Sands and explicit Actions, not an accepted new language, circuit engine or Part A dependency.

- [ ] Explore a bounded data-creation preview beside an editable Lingua description or an Ontology Trail, using the Morpho example as a comparison. Show proposed Records/assertions and intermediate steps before deliberate application through ordinary authorized Actions; test whether the existing language and primitives suffice before proposing another DSL or runtime. Keep the demonstration's transient graph distinct from committed Records and include notices if code is reused.

## Conversation and task surfaces carried for Fiote

Owner ask, 2026-08-29: "we should be able to have in the end a libghostty or
equivalent sand so we can attach properties to that specific sand which can be
the record of the task we are trying to do in such terminal."

The design reasoning lives in [Fiote's session model](../../Fiote.md#the-session-is-a-thread), [tasks and orchestration](../../Fiote.md#tasks-and-orchestration), [wake behavior](../../Fiote.md#being-woken) and [message queue](../../Fiote.md#the-message-queue); only the shared interface consequence belongs here. Phase 1 is
split across the existing pre-Box waterfall rather than inserted as another
framework or milestone:

- the historical C3 recipes establish launch identity and typed Record bindings; new native work may instantiate Bevy composition directly without preserving their projection layer;
- C4 owns Conversation authorship, live message state, private drafts and the native Conversation/Record projections;
- C5 proves those native surfaces through keyboard, pointer and AccessKit;
- the terminal pane needs a native terminal renderer before it returns, since no embedded browser will supply one; it is not a Part A prerequisite;
- Fiote Phase 2 then adds the agent-specific session permission, tool timeline
  and session-control Sands without blocking Box.

Fiote's natural-language and later speech surface is an additional route to
the same authorized Actions. It does not replace keyboard navigation,
semantics or assistive-technology support in the native Sands.

### Task-bound terminal Sand

The requirements below are retained for the low-priority end-of-v1 terminal work. Select a terminal emulation/PTY crate as needed, with Bevy handling presentation and surrounding controls; no library is selected merely by this plan. Its old browser/Wasm renderer is gone with the embedded browser, and a native terminal renderer must be chosen and built before the pane returns; see [the build rule](../build.md#no-embedded-browser). The typed task binding and host-owned identity remain independent of terminal emulation details. A future Bevy terminal integration can be chosen separately, but neither C4/C5 nor Fiote may assume one has already been selected or built.

The legacy `BoardCard.widget_state`, `groupId` and JavaScript grouping path is
not part of the native interface foundation. The landed contract already has the correct primitive: a
`SandInstance` or `CompositionPlacement` receives a typed `Record` input and
the composition document persists the binding, while the Record remains the
truth. C3 added the launch recipe and provenance around that primitive; it does
not add an arbitrary shared state bag.

- [ ] Let the Terminal definition expose a typed task-Record input,
  shown in the Sand's own chrome so several terminals remain distinguishable.
  The honest empty cases are separate: not bound, Record deleted, and not
  permitted to read it.
- [ ] Open a bound terminal *from* a Record — the inverse direction is what
  makes the binding worth having, because a person starts from the task, not
  from a pane.
- [ ] Let the bound Record be changed or cleared on a live pane without
  killing the session. The PTY and the binding have different lifetimes, and
  today the PTY's is the shorter one
  (`crates/transport/src/terminal.rs` owns sessions per websocket connection —
  see [Fiote's harness](../../Fiote.md#the-harness-is-ours), which moves agent sessions off that ownership).
- [ ] Show the same binding on whatever pane a Fiote cub runs in, so the
  terminal view and the Fiote view are two projections of one task rather than
  two unrelated surfaces.
- [ ] Keep the placement and its binding in Box host state. Exporting or
  sharing a workspace preserves the typed reference and provenance, but a
  recipient who cannot resolve or read it gets the honest unavailable state
  rather than a dead uid or copied Record.

### Fiote sessions stream into the interface

Owner, 2026-08-30, marked high priority for the interface refactor: whatever
Fiote and its cubs are doing must be watchable here, live.

This reuses the same typed Record binding and Terminal renderer as the
task-bound terminal, but it is a compound rather than one overloaded pane. The
harness design lives in [Fiote build notes](../../Fiote.md); only the surface
consequence belongs here.

Native Conversation and session controls may proceed after native C5. The full compound described here, including a real terminal and the reusable VT view, additionally depends on the late-v1 Terminal work. A text-only or headless intermediate is not completion of that full user surface.

- [ ] Stream a Fiote or cub session into a pane as it runs: assistant text,
  thinking, tool calls and their results, and the bytes of any command it ran.
  The stream is host-owned and the pane attaches to it, so closing the pane
  does not end the session and reopening replays the backlog.
- [ ] Show per-session state beside the stream — model, tokens used, context
  percentage, cost, and whether it is streaming, waiting on a tool, waiting on
  a person, or finished. A pane with no state readout cannot be told apart
  from a stalled one.
- [ ] Let a person type into a running session (steering) and have it land
  between tool calls rather than mid-stream.
- [ ] Several sessions on one board at once, each labelled by which Fiote owns
  it and which task it is on, since a board of unlabelled panes stops being
  readable at about four.
- [ ] Render command bytes through the Terminal Sand's reusable VT renderer
  inside the read-only tool-timeline view. The person's real Terminal remains
  a separate interactive Sand; renderer reuse does not merge their authority.

### Pinned and queued messages, one list

The reasoning is in [Fiote's message queue](../../Fiote.md#the-message-queue); this is the surface.

A draft message is a Record only its author can see. The landed C4 storage
keeps Conversation/thread routing, `pinned`, timing and position together in
the private `lince.message-draft` extension rather than creating public
ontology assertions for queue mechanics. `pinned` is a preset copied on send;
`next_safe_point` delivers when the current tool returns; `after_turn` delivers
when the turn finishes. A queued draft without `pinned` is consumed only after
an acknowledged send. One list holds both, and the controls say what each entry
will do. The native runtime debounces edits for 250 ms, preserves edits made
while an older revision is in flight and restores Protein-materialized drafts
after restart. Actual next-safe-point/after-turn delivery begins when Fiote
attaches its running-turn state; without one, the same control says and behaves
as “send now · no turn running.”

- [ ] One drafts list per conversation, private to its author, holding presets
  and queued messages together. Reorder by dragging; promote an entry to go
  next; edit or delete in place.
- [ ] The send control shows what pressing it will do, because that changes
  with what is running: send now, deliver at the next safe point, or queue
  behind what is already queued. Never learned by surprise.
- [ ] Delivery controls appear only while a turn is in flight, and when inert
  they are still reachable and say why — "no turn running: this sends now".
  Presets are unconditional; a canned reply is useful in a conversation between
  people too.
- [ ] Show a queued entry's age. One queued an hour ago and fired unattended is
  a stale intent, not an instruction.
- [ ] "Send next" must never be labelled "now" while a tool is running: the
  honest maximum is when that tool returns. Abort-and-send is a separate
  control and looks destructive.

### Session input boundaries

- [ ] Keep three distinct paths: durable thread Messages, ephemeral read-only
  tool output, and the person's interactive shell. If intervention in the
  agent's command stream is ever allowed, it visibly bypasses the model and
  writes that fact into the thread; it never masquerades as steering.
- [ ] Ctrl-C in the terminal stops the running command; a stop control on the
  session cancels the whole turn. An interrupted turn must be marked as
  interrupted where the model can see it, or it reads its half-finished work as
  finished.
- [ ] The terminal owns its keys entirely (it already has its own keymap), so
  composer shortcuts such as Tab-to-queue apply only in the composer. No
  keyboard trap in either pane.

### Three views of a session, and live messages

The reasoning is in [Fiote's session model](../../Fiote.md#the-session-is-a-thread) and [message queue](../../Fiote.md#the-message-queue).

Agreed and settled: a queued draft is consumed on send and a pinned preset is
copied; delivery aspects are assertions on the draft; delivery controls appear
only while a turn is in flight and say honestly what they would do when inert;
a queued entry shows its age; "send next" is never labelled "now" while a tool
is running; queued messages are editable; when a turn ends with several queued,
they are delivered one at a time by default and the setting can be flipped.

- [ ] A session pane offers three views, and switching never restarts or
  interrupts anything: **the thread** (messages, durable, what other people
  see), **what the agent ran** (its commands and their output, rendered as a
  terminal because that is what they are, read-only), and **a real terminal for
  the person** (their own shell in the session's working directory, fully
  interactive — theirs, not the agent's).
- [ ] Match what Pi's own interface shows, and list the gaps rather than
  excusing them: tool calls with arguments, results, diffs, token and context
  counts, queue state, compaction. All of it arrives over the protocol; where
  our rendering is thinner, that is work, not a missing capability.
- [ ] Never let the agent's command view be typed into as if it were a shell.
  Input there bypasses the model and the model does not know it happened.
  Either keep it read-only or make the intervention visibly exceptional and
  record it in the thread.
- [ ] Render an assistant message **live, as it is written** — the Record's
  body grows and the thread shows it growing, including for a person on another
  Organ watching the same thread.
- [ ] Show a message's state: still writing, finished, or interrupted. A reader
  who cannot tell a live message from a stalled one does not know whether to
  wait.
- [ ] A streaming message is read-only until it finishes; editing arrives after.

### A session is a Sand group

Owner, 2026-08-30: when we have a session with an agent we have a Sand group —
the thread, what the agent ran, and possibly the terminal — reusing existing
Sands and inventing the few that are missing.

The landed C2 mechanism is a compound Sand definition plus a
`CompositionDocument`; children are not cards carrying copied membership or a
stack of group ids. C3 added the general mechanism: a versioned domain
launch recipe instantiates that definition with stable placements, typed
Record inputs, provenance and undo. Reopening is idempotent, and user layout
changes remain Box placement overrides rather than mutations to the session or
the code-owned definition.

**What is reused, and what is actually new.**

| View | Sand |
| --- | --- |
| The thread | **`conversation`**, rebuilt in C4. It reads Conversation → Thread → Message over Protein and sends through ordinary Actions. A session thread is an ordinary thread. |
| The task | **`record`**, rebuilt in C4 and bound through its typed Record input. |
| A real terminal | **`terminal`**, blocked on a native terminal renderer with its typed task input. The old browser Ghostty implementation is gone with the embedded browser. |
| What the agent ran | **New.** A tool timeline: each call with its arguments, its result, diffs it produced, folded by default. Command output renders through the same VT path the terminal Sand already uses, read-only. |
| Session control | **New.** The cub tree, spawn and stop, model and thinking level, tokens / context / cost, compaction. |
| Drafts and queue | Part of the composer, not its own Sand. |

- [ ] Give the root compound typed Fiote and active-session Record inputs;
  route those inputs only to children that declare them. Give the group one
  ephemeral lane room for live coordination without turning lane traffic into
  Ledger data.
- [ ] Opening a session reconstructs its group; closing it destroys nothing,
  because the session is host-owned and outlives every view of it ([Fiote's harness](../../Fiote.md#the-harness-is-ours)).
  The earlier Web-era note that layout had to wait for v2 is superseded by the
  C2 composition artifact: reusable layout lives in the compound definition,
  its placement and overrides live in Box state, and the domain launch recipe
  only joins them idempotently.
- [ ] **One group per Fiote, not per cub.** Three or four Sands per session
  multiplied by several cubs is a board nobody can read — the same failure as
  unlabelled panes. Cubs are rows in the session-control Sand, and any one of
  them can be *promoted* into its own group on demand. Nested groups already
  support that; auto-spawning them does not.
- [ ] A new Sand needs its permission in the manifest, the way the terminal
  Sand declares `terminal_session`. The session Sands need one of their own
  rather than borrowing `act`.
- [ ] Keep the operator's view and the shared artifact separate: **another
  person opens the same thread with only the `conversation` Sand** and sees
  the conversation, because the thread is the shared thing and the group is
  one person's way of working on it. Nothing about the group should be
  required to read what happened.

### The two new session Sands, in tiers

Owner, 2026-08-30: face the feature increase — write down the full version and
build it piece by piece. First cut is what the Fiote prototype needs
(Fiote.md, "The first Fiote a person can use"); full is what it becomes.

**Session control.**

- [ ] *First cut:* the roster — this Fiote and its cubs, each with a state
  (idle, streaming, running a tool, waiting on a person, finished) and the task
  it is on; tokens used and context percentage per session; stop.
- [ ] Model and thinking level per session, changeable mid-session.
- [ ] Budget: a ceiling per session and per Fiote, spend so far, and what
  happens when it is reached — stopping, not warning.
- [ ] Tool policy: which MCP servers and which tools this agent may use, each
  allow / ask / deny.
- [ ] Which Agent Record the session's prompt came from **and which revision**,
  so "why did these two behave differently" has an answer.
- [ ] Traversal policy: the link Concepts followed, direction, depth, and
  whether each is glanced at, summarised or read in full.
- [ ] The draft queue with ages, reorder and promote (shares the composer's
  list).
- [ ] Compaction: current context use, the compact gesture, and what a past
  compaction dropped.
- [ ] Provider trouble: the last retry, the last error, and whether it is
  retrying now.
- [ ] Spawn a cub, fork a session, rename a session.

**Tool timeline.**

- [ ] *First cut:* a folded chronological list — tool name, what it acted on,
  ok or error, duration. Nothing is persisted by default ([Fiote's session model](../../Fiote.md#the-session-is-a-thread)): the timeline is
  live while the session runs and empty afterwards, and "output not kept" is
  stated rather than looking like "no output".
- [ ] Render a payload by what it is: text, VT for a command, a diff for an
  edit, an image for an image.
- [ ] Promote a payload into the thread — the one way anything about a tool
  call becomes durable, since nothing is kept by default ([Fiote's session model](../../Fiote.md#the-session-is-a-thread)). Copying
  it into a Message is an ordinary write and needs no persistence layer.
- [ ] Filter by tool and by status, and search within outputs.
- [ ] Jump from a call to the turn it belongs to, and back.
- [ ] Copy the command.
- [ ] **Not to be built casually: re-run a call.** Replaying a side effect
  outside the context that produced it is a different act from repeating a
  query, and the timeline should not make them look alike.

### Fiote arranging the board

- [ ] Once a group is derived rather than hand-placed, a Fiote adding or moving
  a Sand is an ordinary write and needs no privileged path — which is what
  [Karma](../../Lince.lingua) imagined. It must be **visible and undoable**,
  or a board that rearranges itself reads as haunted rather than helpful.

### The agent work board

- [ ] Task Records assigned to agents form a `part-of` tree, which `kanban` and
  `relations` already render. "What are my agents working on" is likely an
  existing Sand with a filter rather than a new one — check before building.
