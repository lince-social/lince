# Interface - AI notes

- [ ] [Sand](Sands.lingua#sand): Sand is the recursively composable unit of interface.
  A button, form, Record view, graph, game, or complete workflow may all be
  Sands. Small Sands combine into larger ones without creating a conceptual
  boundary between LynxUI, the Sand store, and workflows. A Sand may be dead
  presentation, carry data and Behavior, or expose typed connections to other
  Sands. Ready-made Sands are bundles, not sealed applications.
  - [ ] Sands are references to definitions, not copied HTML. Changes to a
    built-in or user-owned definition reach its existing instances live while
    preserving instance overrides. External executable definitions are pinned
    and never change silently.
  - [ ] The Sand store grows on demand from ordinary design-system controls to
    specialised workflow bundles; it need not build every generic control
    before a workflow first needs it.
- [ ] [Box: Base Capabilities](#-base): Box is the application host and the
  spatial playground for building, connecting, and using Sands. It is a
  holistic environment rather than a sidebar of separate applications. Its
  canvas, recursive base pattern, Protein areas, spatial behaviors, and edit
  tools make it feel alive while the default remains minimal and paper-like.
  - [ ] Add built-in and external Sands at every scale.
  - [ ] Link Sands through typed interactions, data, events, and explicit state
    planes. Groups move as one and shelter their internal events, except for
    ports deliberately exposed across the boundary.
  - [ ] Box edit mode is the Sand composer. It reveals subscriptions, Actions,
    state, result fields, typed inputs and outputs, mapping arrows, spatial
    areas, connections, and inherited definitions, and copies these semantics
    together with appearance.
  - [ ] Box consumes the completed Customization and recursive Sand contracts.
    Token architecture, Configuration, strict schema-validated boundaries,
    LynxUI/Sand composition, saved compound Sands, and the non-spatial
    composition workbench are complete before canvas, Protein-area, or
    influence-area work begins.
  - [ ] Workspace state is local for the current Interface completion
    contract. Its model must remain suitable for a later Protein-owned sync
    surface, but device sync and collaborative workspace sharing are deferred
    and do not block Base.
  - [ ] Human-readable authoring documentation must let a non-technical person
    build from existing Sands, query through Protein, invoke Actions, and
    connect behaviors without first learning the Rust implementation.
- [ ] [Highly customizable](Customization.lingua#customization): a person can ergonomically and
  live-edit the appearance, layout, information density, behavior, and
  connections of the Box and its Sands. Simple controls cover padding, gaps,
  thickness, radius, typography, colorschemes, and other common properties;
  an explicit advanced developer mode permits deeper Sand- and workspace-level
  freedom.
- [ ] [Interoperability](Interoperability.lingua#interoperability): Lince coexists with other systems
  through [Blood](Ontology.md#9-federation-and-blood-talking-to-other-systems),
  external Sands, and portable representations. A future open canvas format may
  be adopted only after Lince's own canvas schema and capabilities are stable;
  no external specification may constrain Box features.
  - [ ] Public Facades make genuine data inspection cheap without exposing the
    private publishing Cell to viewer traffic or abuse. Content-addressed
    archive delivery avoids an origin read receipt; a Live Facade deliberately
    trades that stronger network privacy for a real-time scoped Protein stream
    and states the server-visible metadata honestly.
- [ ] [Mobile](Mobile.lingua#mobile): mobile work begins only after the complete desktop
  Sandbox is implemented. Until then, this goal is deliberately not allowed to
  shape or delay the desktop implementation.

# UI Guidelines

- Honesty over decoration. "A number on a chart that nobody can explain is worse than no number."
- Surfaces are opaque; line styles carry truth (settled vs. declared); color
  never carries meaning alone. A connection state, for example, has an icon or
  label that remains meaningful when its color is changed.
- A whiteboard, not a cockpit. Sand are lego blocks on a blank canvas — dots, strokes, hand-drawn arrows, blocks the user arranges.
- Familiar, paper-like, user-owned. When the user makes their lince, it feels like they are creating an art piece, the built-in ui should be minimalist to not carry the composition away from the user's intention.
- When a past implementation conflicts with the organised contract, choose the
  clean, elegant, surgical minimum. Legacy behavior has no independent claim
  to survive; preserve the information and user Need, not accidental machinery.


# Collab and the editor

Moved here from Ontology on 2026-08-21, replacing the coarser copy that used to
sit at the end of this file: the binding, the editor sand and every place a
field is edited are Interface work, and Interface is what comes next. These
fold into Interface's own plan when that plan is made. Nothing here has
dependents outside itself.

- [ ] **Somewhere to SEE the recent-changes diff.** Ontology built the log and
  stopped at the store: `record_change` fills correctly and
  `store::record_changes::recent` reads it, but nothing renders it, so a person
  still cannot find out that their edit lost. The whole point of the box was
  legibility, and a log nobody is shown is not legible — the backend half is
  finished and worth nothing on its own. What an entry already carries decides
  most of the design: the field, whether the change was local or arrived, which
  Organ won, and the displaced value. That last one is what makes the surface
  useful rather than merely informative — the old text is right there, so
  recovering a lost edit is retyping what is on screen rather than digging
  through an op log. Entries expire, so this is a "recently" strip near the
  Record and never a history tab. Only mark entries whose `displaced_local` is
  set as something that happened TO the person; the rest are ordinary sync and
  saying otherwise would cry wolf on every arriving op.
- [ ] **Compaction exports a SHALLOW Loro snapshot.** Everything around this
  landed in Ontology's C2 — compaction is triggered by update count or byte
  threshold, stores one snapshot, logs it as an op, prunes the `crdt` ops below
  it under the normal checkpoint-gated retention, and a doc load is snapshot +
  tail rather than a history replay. What is deferred is the SHALLOW part:
  `compact_doc` exports a FULL snapshot, because a peer compacting at a
  divergent frontier could produce unimportable tails and that needed more
  thought than that work had room for. Cost is already O(current state) rather
  than O(edit history); shallow makes the constant smaller. This matters only
  for genuinely long-lived collab documents, where human authorship bounds the
  size anyway — a thread is rows, not a document. Tests: materialized text is
  identical before and after; a doc load never replays full history.
- [ ] **`record.<column>` as a bindable path.** `attachField` covers text and
  `<namespace>.<key>` (Ontology §11a); a scalar COLUMN — `slug`, `place_uid` —
  has no single-field action to drive, so it is the one path still unbound. It
  wants the same treatment as an extension key (per-field LWW through an
  ordinary action), not a Loro container — see Ontology §11b. Fact-backed
  values (quantity) stay structurally excluded; quantity displays update live
  because fact ops arrive on the same channel, not because the number is a CRDT. `slug` is a
  hazard as a freely-bindable LWW column — slugs are identifiers, and
  last-write-wins across two Cells silently breaks every link that used the
  old one — so it is either excluded from binding or routed through a
  uniqueness check, and that decision lands with this box. What this is FOR,
  confirmed 2026-08-09: one binding serving every place a field is edited —
  the record body in a kanban card and the same body in the Record sand are
  the same live document, and any other field in any other sand joins by
  naming a path rather than growing its own editor. Three editors that happen
  to agree is the failure this replaces. Tests: a bound column round-trips
  through the ordinary action; quantity is refused; the chosen `slug` rule
  holds under a concurrent rename.
- [ ] **`record_editor` sand** — the rich UI on top of the binding, in
  standalone and embedded modes. Rich editing lives HERE, above the CRDT: the
  doc stores plain markdown text; slash commands are input affordances that
  insert markdown or block syntax at the caret (and `/slash` blocks stay a
  record_info product, K-plan unchanged); images are markdown links rendered
  at display time through the local `/host/media` pipeline; preview and
  rendering never write. Because rich features are a layer over plain text
  they need zero CRDT awareness, and a remote edit can never corrupt a block —
  worst case is concurrent text inside one block, which Loro text merges
  character-wise. Inputs: `record_id`, `owner_organ_id`, `mode`,
  `field_policy` (`head_body` / `body_only` / future), inherited auth and
  session. Rules: embedded mode never creates records and never shows the
  record picker, editing only the concrete record it is given; ALL writes go
  through the binding; parents subscribe to editor events. Tests: an embedded
  editor cannot create or switch records; slash-command insertion and image
  rendering survive a concurrent remote edit.
- [ ] **`Note` sand, solo mode** (rename of the current markdown editor). A
  title-empty note is frontend-only, with no `record` row; entering a title
  creates the record (title→`head`, markdown→`body`) and hands editing off to
  `record_editor`; a green status-ball picker in the top right, following the
  document-reader pattern, binds to an existing record instead. The
  user-facing name stays `Note` — `record_editor` and the binding are
  internal, and normal UI labels never say "CRDT" or "Loro". Tests: a draft
  creates no record before a title; a title creates the record and its doc;
  the picker binds to an existing record.
- [ ] **`Note` sand, embedded mode**: no status ball, no creation, no search
  — the parent passes record context and Note renders `record_editor` for it.
  Tests: embedded Note offers no creation path.
- [ ] **Embed the binding into Table**: scalar cells may use the bare binding
  on `record.<column>` or `<namespace>.<key>`; `head` and `body` prefer
  embedding `record_editor`. Tests: a scalar cell edit lands through the
  binding.
- [ ] **A tombstone freezes its doc.** New `crdt` ops against a deleted
  record are rejected; a title-less Note draft has no doc; a new record's doc
  initializes from its materialized columns. Tests: a deleted record rejects
  `crdt` ops; a fresh record's doc matches its columns.
- [ ] **The cross-cutting binding tests** that belong to no single sand: two
  bindings on the same record (Record sand and kanban card) converge both
  ways; concurrent edits to different fields and keys both survive; a local
  edit appends `crdt` ops; applying a remote op materializes SQLite and never
  re-enqueues a loop; duplicate op identity is a no-op; socket subscribers
  receive local updates; the vendored `loro-wasm` asset ships its LICENSE and
  notice files and both pins are the same version.


# Implementation

## Base

### Runtime and rendering

The browser remains a first-class, fully capable Lince client. The desktop app
hosts the same interface with Tauri. HTML remains the proven surface for text,
forms, accessibility, CSS, and imported Web content; choosing it does not mean
that every spatial object must be a DOM node or that every visual operation
must run on the CPU.

The accepted default authors first-party Sand structure Maud-first in Rust and
emits ordinary HTML fragments and packages. Native ES modules remain the
ordinary browser Behavior runtime, and a shared `wgpu` renderer compiled to
WebAssembly owns Box's spatial GPU layer. Customization C0 freezes the exact
artifact and ABI details against the documented raw-HTML and Datastar
alternatives rather than reopening this direction without new evidence. Maud
constructors are paired with the
recursive Sand nodes, stable child identities, ports, configuration, Behavior,
assets, and capabilities they render; bare `Markup` is not enough to declare a
composable child. Box edit mode and raw packages produce the same normalized
definition graph, and the browser composition runtime consumes that graph
regardless of authoring origin. Maud performs no browser rendering and external
HTML remains independent of it.

The implementation choice for the current Interface work is a hybrid. DOM
composition roots render interactive Sands while one shared `wgpu` world
renderer draws spatial material. In the browser and Tauri WebView that renderer
runs as a content-addressed Rust/WebAssembly artifact over WebGPU, with wgpu's
WebGL2 backend as the rendering fallback where required. The same renderer code
can later target native Vulkan, Metal, Direct3D, or OpenGL without changing the
Sand, Protein, Action, or Box-state contracts. GPUI/WGPUI, Bevy, and Pulsar are
future ideas, not competing foundations during this implementation.

Here, **hybrid** means one Web interface with two cooperating presentation
layers, not two products and not a WebView per component. Ordinary interactive
Sands stay in a small number of DOM composition roots. A transparent GPU
surface behind or beside them owns the world-scale visuals and receives the
same camera transform. Box performs hit testing and routes input to the owning
layer. The renderer owns a bounded number of shared surfaces and one device,
not a canvas, GPU device, or Wasm runtime per Sand.

The shared `wgpu` renderer accelerates the recursive base pattern, zone masks
and force indicators, connections and arrows, drawing strokes, selection
overlays, and thousands of lightweight sprites or glyph-like nodes. It keeps a
retained scene and GPU instance buffers keyed by stable Sand or visual-node uid.
Moving one Sand updates that instance's transform and the spatial index; it
does not rebuild every vertex, rerender unchanged DOM, or ask every other Sand
to run. HTML continues to render Sand text, forms, editors, accessibility
trees, and embedded pages; `wgpu` does not turn those into textures.

Physics is a separate responsibility from rendering. The first implementation
runs force, collision, and spatial-index work as batched Rust/WebAssembly in a
front-end Worker so it cannot block input. It uses a broad-phase spatial index
and a dirty/awake set: only moved bodies, nearby collision candidates, affected
groups, and areas whose influence intersects them enter a step. Offscreen and
settled bodies sleep until a relevant change wakes them. Groups may submit one
coarse body before detailed child collision is justified. The simulation and
render clocks are decoupled, and the main thread receives bounded transform
diffs rather than a full-world snapshot every frame.

A global force that genuinely moves every body necessarily performs at least
linear work over the awake set; the architecture cannot make meaningful work
free. Its guarantee is that one local drag does not become an all-world scan
and that collision candidates do not become an all-pairs comparison. One
thousand simple bodies is a modest batched Rust/Wasm and instanced-GPU workload;
one thousand simultaneously visible rich DOM editors or live Websites is not,
so visibility and runtime kind remain part of the budget.

WebGPU compute is a later acceleration path for large, regular kernels after
profiling proves the Worker calculation is the bottleneck. Rendering on the GPU
does not automatically make collision or force code GPU-powered, and moving
small active sets is normally cheaper on the Worker than uploading and reading
back an entire world. The baseline does not require Wasm threads,
`SharedArrayBuffer`, or cross-origin isolation, because those requirements can
interfere with external Web content; transferable or copied batches are tested
first. No back-end GPU is needed for local compositing.

External HTML remains real DOM, and an untrusted Website remains a sandboxed
iframe or dedicated WebView. An installed external HTML Sand and a GPU Sand can
exchange typed events, Protein values, local state, and Action requests through
the same logical Sand ports, but through different adapters: scoped JavaScript
calls for trusted DOM, validated `MessagePort` messages for an iframe, and the
renderer/Wasm binding for `wgpu`. A live Website receives only wrapper ports and
never Lince authority. Cross-origin HTML cannot be captured into a GPU texture
or inspected by `wgpu`; the browser compositor keeps it interactive above the
world surface.

[CanvasUI](https://canvasui.dev/) remains useful design research, but is not a
foundation choice: imported HTML, rich text editing, accessibility, and
cross-origin Web content still need real browser semantics.

#### Future native engines

As assessed on 2026-08-15, a native-first rewrite would trade away the Web
client and make HTML Sands a second embedded system before performance has
shown that cost is necessary. [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)
is pre-1.0 and officially targets macOS and Linux;
[WGPUI](https://docs.rs/wgpui/latest/wgpui/) is a cross-platform fork rather
than the canonical Zed framework; [Pulsar](https://pulsarnative.com/) is an
early game-engine/editor stack built around GPUI; and
[Bevy 0.19](https://bevy.org/news/bevy-0-19/) has much better UI widgets but is
still a game engine, not a browser-compatible document and embedding model.
These remain credible ideas for a future native Box after the current work,
but none currently repays rebuilding Lince's text, accessibility, CSS, and
external-Web integration.

### Box, workspace, and canvas

**Box** is the application host and composition environment. A **workspace**
is one persisted spatial document. Its **canvas** is conceptually unbounded and
uses spatial indexing and viewport virtualisation rather than a fixed world
rectangle. **Sandbox** is the metaphor for the complete environment; `board`
is legacy implementation terminology.

The default view is an orthographic, paper-like 2D workspace. Spatial physics
may feel alive, but a person is never forced into a game camera merely to edit
a Record. Controls must recenter the view, bring a chosen Sand or selection to
the user, and expose a minimap Sand so an unbounded workspace remains
navigable.

### Protein areas and result-template groups

A **Protein area** gives data a place to enter Box. A person places its spawn
point and boundary, then selects one already-supported Protein item. That item
continues to own its source, filters, includes, sort, and limit. The area does
not add new query semantics and Protein remains a read: it neither creates nor
copies Ledger data by showing a result.

The result-template pipeline is deliberately linear:

1. Protein produces ordered result rows or objects.
2. Edit mode shows the fields and types present in that result shape.
3. The person composes one group from any number of Sands.
4. Arrows connect result fields to compatible data inputs on those Sands.
5. Unconnected Sands may remain in the group as labels, controls, decoration,
   or Behavior.
6. Locking the group makes it the area's result template.
7. Every Protein row fills one instance of that complete group.

For example, a Record result can expose `head`, `body`, `quantity`, and other
fields. `head` may connect to a text Sand, `quantity` to a number Sand, and the
complete Record identity to a button that performs an Action. If five rows
arrive, Box produces five bound instances of the same locked group. The group
is referenced as a template rather than copied HTML, so editing its definition
updates every result instance while each instance retains its row binding and
Box position.

A mapped child receives only the field or object explicitly wired to its typed
input. Incompatible connections are rejected visibly; missing optional values
remain honest empty values. The template does not change automatically with
camera zoom. Whether it receives a head, full body, bounded body excerpt, or
another representation is determined by the configured Protein output and
the visible field mapping.

One field may feed several Sands, and a Sand with several typed inputs may
receive several fields. An arrow carries read data; it does not grant write
authority. A Sand that edits a value must expose a separate typed write binding
or Action, attributed to the bound result identity.

Every repeated row also needs a stable identity supplied by its current
Protein source: a Record uses its uid, while an aggregate uses its canonical
grouping key. Box does not invent identity from mutable display text. A result
shape without a stable key cannot be used as a persistent repeated template
until its source declares one.

One Record may appear more than once when different Protein items call it.
Those appearances share Ledger identity but have independent Box state. Each
result group shows the stable hash/identity of its originating Protein item in
metadata, and that metadata can locate and highlight the source area. When a
row stops arriving, Box retires only that result-group appearance; it never
deletes the underlying Record.

### Spatial areas and production-line behavior

The Supercomponent is a set of Box capabilities, not one enormous Sand.
Beyond supplying data, its areas can act on bound Sands. The first version has
four single-purpose area semantics. Each uses the same filters and field semantics
already available to Protein; arbitrary formulas, Karma-aware traversal, and
new query languages are outside this plan. Only a Protein area spawns result
groups. The areas below merely test the Protein-bound row already carried by a
group and then act on that group.

- A **force area** pulls matching Sands toward itself or pushes them away.
  Strength, direction, range, collision, and settling are visible controls.
- A **sorting area** selects bound groups inside its boundary, orders them with
  a configured Protein sort, and lays them along a chosen direction. Fixed
  bounds remain fixed and use internal scrolling when results do not fit.
- A **mutation area** runs declared typed Actions when a compatible bound group
  enters it. The first mappings change quantity and add or remove Concepts.
- An **immunity area** belongs to a Protein area and protects the groups spawned
  by that source from the workspace-centering force and from force, sorting, or
  mutation areas whose effective area lies outside the immunity boundary.
  Areas inside the boundary remain valid. Immunity changes spatial/Behavior
  eligibility only; it does not hide data, deny manual editing, or grant Action
  authority.

Force and sorting areas may act on read-only or aggregate rows. Mutation areas
require a concrete writable target identity and an Action compatible with that
target; an aggregate or summary row cannot be mutated as though it were a
Record.

Areas may overlap. Their persistent evaluation order is visible in edit mode,
so a force area can feed groups through sorting and mutation areas like a
production line. A mutation can change which filters match next, causing the
group to move onward or disappear from its original Protein area when the
source query no longer returns it. That is a consequence of the committed
Action and subsequent Protein refresh, not hidden direct manipulation of the
Record.

Immunity is evaluated before external area effects. It follows the originating
Protein identity carried by the spawned group, not whichever rectangle the
group happens to overlap later. A group spawned by another Protein area does
not inherit immunity merely by entering the boundary. Edit mode shows the
protected source, boundary, currently blocked external areas, and permitted
internal areas so immunity never reads as broken physics.

Entry caused by physics is meaningful and may trigger a mutation. It fires
once for each outside-to-inside visit, not once per animation frame. Actions
are serialized in the displayed area order, failures remain visible, and a
bounded cycle detector pauses a projection whose mutations and forces loop
without settling. Leaving and deliberately re-entering begins a new visit.

Groups always move as one body. If a filter or force matches a data-bound
child, the complete result-template group is pulled or pushed; the child is
never torn out of the locked group. Bare Sands and ordinary hand-made groups
may coexist on the canvas, but a Protein filter cannot match data they do not
carry. When several children match different forces, those forces combine at
the group transform and the group remains intact.

The workspace also has an optional weak centering force, similar to the current
Relation physics, so unattended Sands can slowly return toward a recoverable
region. Areas expose shape, pull/repulsion strength, color, opacity, and border
controls. Color is never their only label.

Edit mode provides a **Why is it here?** explanation for every bound group. It
shows the source Protein item and row, field mappings, group template, matching
force and sorting areas, current order, manual pin/offset, and the
mutation-area entries and Actions that affected it. Every reason can highlight
its source on the canvas. Box movement and mutation mechanisms must therefore
emit structured reasons rather than setting positions or data anonymously.

### Canvas base pattern

The current canvas pattern is deliberately simpler than topography. Its
generated form is a recursive level-of-detail grid: zooming in reveals finer
repetitions of the pattern and zooming out removes detail before it becomes
visual noise. This is the familiar self-revealing canvas-grid effect, not a
requirement for Mandelbrot computation.

The default pattern interpolates with one percentage slider. At 0% every grid
intersection is a dot. As the percentage rises, four arms extend from each dot
to form a `+`; at 100% the arms meet their neighbours and become a continuous
orthogonal mesh. Scale, color, opacity, and the zoom levels at which each
recursion appears remain configurable.

A person may instead use a pinned raster image or safe SVG asset as a canvas
wallpaper, with fit/repeat, scale, position, and opacity controls. An imported
SVG wallpaper is inert presentation: scripts and remote resource loads are not
executed merely because it is used as a pattern.

### Interaction and navigation

View mode is for using the composition; edit mode reveals placement,
subscriptions, Actions, Behavior, state, ports, event paths, groups, inherited
definitions, and influence zones. It supports pan, zoom, select, marquee,
move, resize, group, connect, copy/paste, and drag/drop. Drawing follows only
after these operations are stable; terrain sculpting belongs to deferred
topology research.

Keyboard use is first-class. Vim-like spatial motions can move focus between
Records/Sands; Enter opens the focused Record in the configured Record view;
Ctrl+N creates; configurable action keys can change quantity or perform the
same focused operations as Relation Trail mode. Shortcuts act on the current
selection and context, never on a hidden arbitrary Record.

### Box-state persistence

Box state is readable presentation state, separate from the Ledger. The
current implementation serializes the complete pretty-printed
`board-state.json` to a temporary file and atomically renames it on every
persisting commit. Camera movement is debounced, and drag/resize previews avoid
writes until completion, but many Sand preference changes write a full
snapshot. Cards may also carry copied HTML, so large workspaces can amplify
writes and file size.

The replacement is a versioned **Box document**, not a dump of DOM or
JavaScript state. It has stable uids for the workspace, referenced Sand
definitions, instances, groups, connections, Protein areas, field bindings,
influence areas, drawings, and other durable authored entities. An instance
records its definition revision, parent group, local transform, anchor space,
layer, sibling order, override patch, exported bindings, and its persistent
host-state allocation. Child position is relative to its group; moving the
group therefore never rewrites every child. Persisted ordering is semantic
layer and sibling order, not a leaked CSS `z-index` implementation detail.

Pinning is not one ambiguous boolean. An anchor declares whether coordinates
belong to the world, the viewport, or a parent group. Changing that anchor is
an authored operation which converts coordinates visibly. Camera, focus,
selection, open panels, hover, drag previews, media sessions, presence, and
the current numerical position of a force simulation are personal view or
ephemeral runtime state, not shared composition.

The Box document keeps a compact human-readable snapshot plus a typed
operation journal. The snapshot is the inspectable and editable interchange
form; the journal provides crash recovery, small writes, undo, agent control,
and the future synchronization seam. Each operation has its own uid and names
stable target uids; unknown document or operation versions fail closed. An
atomic batch represents one human gesture such as grouping, reconnecting, or
dropping a result-template definition.

Physics does not emit persistence on animation frames. Box persists authored
constraints and changes: drag/resize completion, pin/unpin, group edits,
configuration commits, connections, and area edits. A settled position may be
checkpointed as a recoverability hint at a bounded configurable interval, but
it is derived state and cannot overwhelm or outrank the authored operation
that produced it. Append, fsync, snapshot compaction, File Sync publication,
and contact synchronization are separate rates; making an external sync rate
slower must not make the local document unsafe.

The text format must be honest about its grammar. If the Box snapshot uses the
same Lingua grammar and tooling, it may be a Lingua declaration. If spatial
composition needs a different grammar, it uses a distinct extension such as
`.box`, even when its vocabulary is Lingua-inspired. Two incompatible syntaxes
must never share `.lingua`. A Lingua Record may reference a Box document
without turning thousands of spatial operations into Ledger Records.

Other programs and agents interact with a running Box through the same typed
operation API and read-only Box projection used by the interface, rather than
editing the snapshot behind Lince's back. Offline tools may edit the snapshot
atomically; Lince validates the whole replacement, shows a structural diff,
and retains the last known-good document if it is invalid. External canvas
formats such as OCIF are examples to look at while explaining why Lince needs
stable node identity and a readable graph. They create no import, export,
adapter, compatibility, or evaluation obligation and do not define or limit
Lince's native schema, typed ports, Protein bindings, Behaviors, capabilities,
or spatial areas.

### Public Facade

A **Live Facade** is a published, read-only rendering of one Box composition at
a public URL. Caddy and DNS may terminate and route the public origin, but
Lince still owns the publication manifest, public assets, read-only data
contract, and safe browser runtime. Publishing freezes the available Sand
definitions, layout, areas, connections, and configuration until the owner
publishes a new revision. A visitor receives no edit mode, Sand store, add or
remove operation, Action bridge, terminal, filesystem authority, identity
credential, or private Lince endpoint.

The composition can remain alive without becoming writable. It subscribes to
predeclared, read-only Protein projections and updates them in real time. A
visitor cannot submit an arbitrary Protein query: publication names the saved
Protein items, allowed fields, limits, and stable result keys, and the public
service exposes only those projections. The Action route is absent, not merely
hidden. The stream has revision/resume information, bounded messages,
backpressure, reconnect behavior, and honest stale/offline/removed states.

Interaction that changes only the visitor's browser remains available:
opening a Record selected from a Kanban-like composition, changing the current
Instinct page or chapter, expanding sections, filtering an already-delivered
projection, panning, zooming, and running read-only force or sorting areas.
This state begins in memory. A Facade may opt into namespaced browser storage
for preferences such as the last page, with a visible reset and a small quota;
it never sends that state back as an Action. Mutation areas, write ports, and
Behaviors requiring durable authority make publication validation fail rather
than quietly becoming inert.

A ready-made Kanban Facade is therefore the same saved compound Sand a person
could open and decompose in Box: Protein area, repeated card group, field
arrows, grouping/sorting areas, and Record detail composition. It remains as a
convenient Sand-store entry, but it is not a separately implemented widget.
The Facade renderer consumes the same definitions and token cascade and simply
removes authoring and mutation authority.

The first Live Facade admits official/read-only compound Sands and inert,
sanitized assets. It does not admit Website Sands, arbitrary remote resources,
installed network-capable Sands, or raw advanced CSS that can impersonate
Lince chrome or escape its bounds. It runs on an origin separated from private
Lince administration, with a restrictive CSP whose only connection is the
scoped public Protein stream, no ambient cookies or Lince credentials, bounded
storage, and sanitization of rendered Record content.

Public data must be projected into a separate public Organ before serving it.
That Organ contains only the published subset; a bug or compromise must not
turn a field filter into access to the private Cell. Publication shows the
selected Records, fields, definitions, assets, and estimated size before the
owner confirms it, and revocation closes the stream and removes future
availability without pretending already downloaded public data can be erased.

This Live Facade and the existing content-addressed archive Facade are two
delivery modes. The archive has no live stream and can be fetched privately by
hash. A direct public URL and WebSocket necessarily reveal network metadata
such as visitor IP and timing to Caddy or whichever service answers it; Lince
can avoid accounts, cookies, analytics, and application-level viewer ids, but
cannot truthfully promise that a directly contacted server learns nothing
about the request. Use the archive/relay path when that stronger privacy
property matters.

### Deferred workspace synchronization and sharing

Workspace device sync and collaborative sharing are preserved future work and
do not gate any current Interface checkbox. When resumed, a workspace may sync
among one Person's devices, be shared with named People across Organs, or be
owned by an Organ. Authorized editors may concurrently change composition;
viewers receive it without write authority; replica members may work offline
and later converge.

The public interface for all of that synchronization remains Protein. Protein's
Sync surface will gain a separate Workspace section beside its Ledger-data
sources: the same place will invite a friend, choose live/replica behavior, set
limits, report status, and subscribe to a `workspace` projection. This does not
require Box state to pretend to be Records.

The implementation beneath Protein may use a separate schema, storage, signed
`WorkspaceOp` stream, and compacted snapshots. Protein remains the unified read
and sync surface; typed Actions/Box edit operations remain the write surface.
The local Box document deliberately establishes stable entity ids, operation
semantics, atomic batches, snapshots, and compaction now so later sync does not
have to reverse-engineer whole-file diffs. That does not make the local journal
a collaboration protocol by itself: actor identity, authorization, causal
dependencies, deterministic merge, tombstones, revocation, encryption, and
resource limits still belong to the future transport envelope. Disk and wire
may encode the same semantic operation differently.
Future requirements retained for that work are:

- versioned granular operations for instances, overrides, groups, ports,
  zones, drawings, portals, and definition references, with unknown versions
  failing closed;
- viewer/editor/owner roles, invitation, leave, revocation, and a separate
  permission for editing a reusable Sand definition whose other instances may
  live outside the shared workspace;
- encryption of composition and assets to current members, key rotation for
  future state after membership changes, and no promise to erase replicas a
  former member already received;
- deterministic merge, atomic batches, dependencies, tombstones, offline
  replay, compaction, resource ceilings, and missing-definition recovery;
- ephemeral presence and cursors outside the durable operation log;
- honest empty, offline, conflict, and access-lost UI alongside the mechanism;
- cross-Organ convergence, revocation, malformed/oversized operation, snapshot
  recovery, and Protein-projection parity tests.

### Deferred Protein and Box extensions

The following ideas are preserved but unplanned. They carry no current
checkbox and do not expand the first Protein-area or spatial-area contract.

**Calendar composition.** A future Calendar may be a paginated stack of day
Protein areas rather than a separate calendar data model. Each day would show
Record or recurrence projections valid on that date; moving between months
would hide one page and reveal another without deleting or recreating Records.
One recurring Record could produce several occurrence appearances tied to the
same source unless an explicit Action materialized an occurrence as its own
Record. This needs more design before it becomes implementation work.

**Richer Protein selection.** Exact and bounded-regex selection by Record slug
or Concept, and filters that pull a Record because of associated Karma or other
nested data, remain possible future Protein work. The current area plan uses
only capabilities Protein already exposes. Any future regex syntax must bound
pattern size, result count, and execution cost and report invalid expressions
visibly.

**Stacked workspaces and portals.** Workspaces may eventually behave as stacked
surfaces. A person could open a bounded hole into the workspace beneath it,
interact through that portal, and heal it without merging workspaces or leaking
events. Ordering, coordinates, input routing, focus, event scope, and saved
portal semantics remain undecided.

### Deferred topology and layered-physics research

Full topology is deliberately the last Base idea, after the simple recursive
pattern, Protein areas, and current zones of influence are complete. It is not
part of the current implementation plan. When revisited, topology may describe
several Sand planes rather than one universal terrain.

A viewport-attached hover plane could behave like hockey pucks on an air table:
Sands drift and bounce as an optional screensaver-like mode, react when the
plane is shaken, and follow a gentle hover-plane gravity toward a corner. The
ground plane could instead carry sculpted terrain. Adjustable circle and square
tools would push a mountain up from below, flatten its top, dig or reshape a
region, and change size in edit mode.

A Sand could be glued to a point of that topology. Moving the Sand would move
the associated topology relative to it, while sculpting the terrain would move
the pinned Sand. Direct manipulation and formula editing would update the same
topology in real time rather than becoming two unrelated representations.

The canvas pattern would respond to terrain: a line grid, wallpaper, future
shader, or mandala could compress and darken in valleys, deform across slopes,
and let raised terrain cast a shadow onto material at the floor level. Zones
could remain flat above the terrain or be grounded and deformed by it; the
Sands themselves must remain readable.

Mathematical editing would use ordinary notation and presets rather than a
Lince-specific programming language. Its stored semantic form would be a
bounded profile of W3C
[Content MathML](https://www.w3.org/TR/mathml4/#contm) and
[OpenMath](https://openmath.org/standard/om20-2019-07-01/), safe when stored and
rendered clearly when seen. Expressions could not perform I/O, invoke
JavaScript, or allocate unbounded work. A future visual editor, field/force
explanation, deterministic settling model, and CPU/GPU evaluation limits must
be designed before enabling raw expressions. Any vendored evaluator or editor
must carry its license and credits with the owning Sand.

The terrain direction remains inspired in part by
[this topographic-canvas demonstration](https://youtu.be/-IOLRcFC6OY?si=WW1tMdMmo0NYxcu_),
without requiring Lince to reproduce that particular effect.

## What is left

### Interface — what is left

#### Entry gate

- [ ] Complete every Customization C0–C5 gate and the recursive Sand
  composition workbench before changing the canvas model. Box work begins
  with versioned, validated definitions, typed ports, compound Sands,
  configuration scopes, schema-validated native JavaScript Behavior, and the
  renderer adapter contract already usable without the board.

#### Runtime and rendering

- [ ] Build a representative benchmark on the owner's Vostro 3150, 11th-gen
  Intel Core i7, Iris Xe integrated graphics, 16 GB RAM, and NixOS. At the
  machine's native display resolution it must sustain 60 FPS pan/zoom with 200
  visible interactive Sands and at least 1,000 placed Sand/group bodies,
  thousands of lightweight nodes/connections, offscreen suspension, and no
  frame-by-frame rerender of unchanged DOM. Moving one settled Sand through a
  populated region must update only its dirty neighborhood rather than scan or
  rewrite the whole workspace. Record frame time, Worker step time, main-thread
  blocking time, GPU upload bytes, memory, active/sleeping body counts, exact
  CPU, resolution, browser/WebView, and power mode. Test a small number of
  composition roots and one shared GPU device rather than per-Sand canvases,
  runtimes, WebViews, or iframes. Run separate settled-local-drag, dense
  collision-pile, and global-force scenarios so a favorable sleeping case does
  not conceal the real cost of waking the world.
- [ ] Benchmark Website Sands separately at 0, 1, 4, and 12 simultaneously
  visible sites, including video playback, suspension, storage, and memory.
  The 200-Sand target does not mean 200 live Websites; offscreen Websites are
  frozen or unloaded under an explicit session policy.
- [ ] Build the accepted shared `wgpu` world renderer as a content-addressed
  Rust/WebAssembly front-end artifact. Exercise WebGPU and the supported WebGL2
  fallback, device loss, resize/device-scale changes, suspension, context
  recovery, and accessible DOM counterparts. Record download, compile, shader
  warm-up, frame time, memory, and startup alongside text-input and Website
  behavior. Back-end GPU work cannot accelerate browser compositing.
- [ ] Build the Worker physics proof with a spatial broad phase, dirty/awake
  sets, sleeping, group-level coarse bodies, bounded transform diffs, and
  deterministic teardown. Compare JavaScript and Rust/Wasm only where the same
  kernel supplies useful evidence. Add WebGPU compute to a separate spike only
  if measured Worker time, rather than DOM or rendering time, misses the budget.
- [ ] Treat the benchmark as validation and tuning of the accepted hybrid, not
  as permission to omit its human surface. Replace `wgpu` only if evidence shows
  it cannot meet the contract; prefer the smallest repair before reconsidering
  a full game engine or native rewrite.

#### Box canvas

- [ ] Replace the fixed 10,000×10,000 world with unbounded logical coordinates,
  viewport culling, and recoverable navigation.
- [ ] Add recenter, bring-selection-here, locate-by-name, and minimap surfaces
  with honest empty cases.
- [ ] Keep Box chrome minimal and outside the user composition. The base Web
  surface uses the folded top-right corner for system controls; Sand controls
  belong to the focused Sand's bottom-right page corner.

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
- [ ] Reconcile live Protein results into one stable group instance per row,
  preserving row identity and local Box state without duplicating or deleting
  underlying Records; refuse persistent repetition when a source declares no
  stable row key.
- [ ] Show the Protein item hash and source area in every bound group's
  metadata and provide locate/highlight navigation in both directions.

#### Force, sorting, mutation, and immunity areas

- [ ] Define common area geometry, current-Protein selection, overlap and
  evaluation order, entry/exit lifecycle, styling, persistence, and edit tools.
- [ ] Implement force areas and the optional workspace-centering force with
  controllable pull/push, direction, range, collision, settling, and
  reduced-motion behavior.
- [ ] Apply a matching force to the complete group rather than pulling a bound
  child out of its result template.
- [ ] Implement directional sorting areas using current Protein sort semantics
  and fixed, internally scrollable bounds.
- [ ] Implement mutation areas for quantity changes and Concept addition or
  removal through existing typed Actions, with one trigger per boundary visit.
- [ ] Implement immunity areas attached to one Protein area. Protect that
  source's spawned groups from workspace centering and external force, sorting,
  and mutation areas while preserving internal areas and manual interaction.
  Show the boundary, protected source, blocked influences, and effective
  evaluation in edit mode and Why-is-it-here.
- [ ] Add deterministic overlap ordering, serialized Actions, loop/resource
  ceilings, pause/recover controls, and honest partial-failure states.
- [ ] Build the Why-is-it-here inspector and require structured causal metadata
  from Protein spawning, field mapping, grouping, forces, sorting, pins, and
  mutation Actions.
- [ ] Add edit tools for drawing, resizing, copying, stacking, styling, and
  removing areas without silently changing the data they currently contain.

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
  Its operation shape must remain suitable for later merging and workspace
  sync without making that deferred work part of the current implementation.
  Benchmark complex diagrams and painting-like frames against the interaction
  and culling behavior people expect from tools such as Excalidraw.

#### Box-state persistence

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
  transforms; world/viewport/group anchors; semantic layers and sibling order;
  Protein references and field-to-port bindings; areas; connections; override
  patches; persistent host-state allocation; and content-addressed assets.
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
  connections, configuration, and area edits; treat simulated positions as
  derived checkpoints with a bounded configurable cadence.
- [ ] Separate local journal durability, fsync cadence, snapshot compaction,
  File Sync publication cadence, and later contact delivery. Ordinary Lingua
  file synchronization keeps its current/immediate default; Box File Sync
  exposes a rate policy and its possible recovery lag without changing the
  meaning of the Box operations.
- [ ] Measure actual write volume, recovery after interruption, compaction,
  state growth per Sand, and the cost of unbounded workspaces.
- [ ] Separate reusable workspace composition from personal view state now so
  future synchronization does not need to unpick them. Sand placement, size,
  configuration, definitions, groups, connections, zones, and drawings are
  composition; camera, focus, selection, open panels, and temporary portals are
  personal view state.
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
