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
  the Record description in a kanban card and the same description in the
  Record Sand are the same live document, and any other field in any other Sand
  joins by naming a path rather than growing its own editor. Three editors that
  happen to agree is the failure this replaces. Tests: a bound column round-trips
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
  `field_policy` (`title_description` / `description_only` / future), inherited
  auth and session. Rules: embedded mode never creates records and never shows the
  record picker, editing only the concrete record it is given; ALL writes go
  through the binding; parents subscribe to editor events. Tests: an embedded
  editor cannot create or switch records; slash-command insertion and image
  rendering survive a concurrent remote edit.
- [ ] **`Note` sand, solo mode** (rename of the current markdown editor). A
  title-empty note is frontend-only, with no `record` row; entering a title
  creates the Record (`title` from the title input and `description` from the
  Markdown input) and hands editing off to
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
  on `record.<column>` or `<namespace>.<key>`; `title` and `description` prefer
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

#### Product horizons: v1 productivity and v2 world

The product has two explicit interface horizons. They are scopes, not two
unrelated applications and not permission to throw the first one away.

**Lince v1.0.0** is the productivity Box already being planned: composable
Sands and compound Sands/Castles, the current Protein capabilities, visual
field wiring, Areas of influence, direct manipulation, Why-is-it-here,
Customization, installed external HTML, Websites and Facade. Its default
experience is a clean orthographic desk with no required game mechanics or 3D
camera. It solves the immediate problem of having a usable Lince for work as
work is commonly organised now.

**Lince v2.0.0** is the long-horizon world interface: the same semantic Sands
and Actions can inhabit an Earth-scale globe, nested local places, authored
worlds, games, general scene and spatial construction, reality-derived
representations, avatars, time-aware scenarios and collaborative sessions. It
addresses how Lince may model Needs, Contributions and the organisation of
reality itself. Named authoring programs, geometry systems, interchange
formats and reconstruction techniques are examples to learn from or connect
through capability adapters; none is the definition of the feature or a
mandatory dependency. The detailed target is retained in
[Long-horizon world direction](#long-horizon-world-direction).

The existing names **Plan A** and **Plan B** remain runtime implementation
choices for v1, not aliases for v1 and v2. Plan A is the preferred GPU-first
native vertical slice. Plan B is the preserved Maud/HTML browser hybrid and
Facade path. V2 is neither a fallback nor a reason to implement the v1 world
features early.

V1 is a vertical slice of v2 in four permanent respects:

- it uses the final Sand definition, port, Action, Customization and external
  HTML contracts;
- a v1 workspace is a local spatial frame rather than an unrelated coordinate
  system that v2 must later translate;
- the native path exercises the intended single-device compositor, retained UI,
  world-runtime and CEF boundaries even while its world is only a simple desk;
  and
- v1 placements and renderer handles never become Ledger identity or engine
  save data, so a Sand can later appear on Earth or in an authored world without
  being redefined.

V1 does not need planet streaming, general scene-authoring tools, reality
reconstruction or a universal world format to satisfy these invariants. It
needs a small permanent kernel and an honest capability boundary, followed by
the simple human-usable Box.

The development course is therefore:

1. prove the permanent native ownership seams with one Lince-owned window,
   device and compositor, selected Bevy modules, retained UI and CEF, while
   keeping the GPUI-owned path as rejection/reference evidence;
2. complete Customization, the Lynx visual-character gate and the
   renderer-independent Sand/Castle composition workbench;
3. ship the human-usable v1 Box, current Protein wiring, Areas and external
   HTML on those foundations; and
4. keep v2 as structured research with explicit coordinate, planetary,
   artifact, scene-construction, capture and scenario proofs, promoting a
   result into product planning only after its semantics and human surface are
   understood.

Research proofs live behind development tooling and leave no dormant public
schema field, compatibility branch or half-supported button in v1. What v1
learns is written into the shared contracts and benchmarks rather than hidden
inside a disposable demo.

The runtime decision now has an ordered Plan A and Plan B. The normalized Sand
definition, Protein and Action boundaries, typed ports, capability model,
Customization cascade, Box document, and recursive composition semantics are
shared by both plans. A renderer may change without changing what a Sand means.
The browser remains a first-class client and public Facade surface; Plan A is a
native desktop/runtime decision, not permission to make Web data, composition,
or external HTML second-class.

#### V1 final product boundary

The accepted prototype is followed by one final v1 shape, not by an expanding
engine demonstration. A person opening v1 receives:

- a sharp, minimal native shell and an orthographic, conceptually unbounded 2D
  Box with pan, zoom, selection, minimap, locate and recenter controls;
- the complete token and configuration cascade, a human-visible Gallery and
  workbench, and the same resolved Customization meaning across native world
  primitives, retained UI, installed HTML, Websites and browser/Facade
  adapters;
- one recursive Sand definition and instance graph for primitive Sands,
  ordinary groups, locked groups, reusable compound Sands/Castles and Protein
  result templates, with typed ports, explicit Behavior, capabilities,
  lineage, stable child ids and inspectable overrides;
- current Protein as a visual data source: a spawn Area displays its result
  shape, arrows connect fields to Sand inputs, each stable result row creates
  one instance of the locked template, and every instance explains its Protein
  origin through **Why is it here?**;
- force Areas that attract or repel matching Sands, sorting Areas that arrange
  matching Sands in a declared direction, and mutation Areas that preview and
  request explicit Actions rather than changing Ledger truth from physics;
- group-level motion, immunity boundaries, a configurable weak recentering
  force, deterministic overlap order, scrollable constrained Areas, anchors,
  layers and pinned viewport Sands;
- lightweight native GPU Sands for large populations, retained rich native
  controls and editors, installed CEF HTML Sands with declared Protein/event/
  Action capabilities, and Website Sands with ordinary web networking and
  storage but no Lince authority;
- a human-readable runtime-health and resource surface that identifies the
  selected graphics backend, software rendering, heavy-Sand admission cost,
  denied starts, browser/GPU failures, recovery progress and the reason a Sand
  is unavailable; and
- a read-only Live Facade that consumes the same public definitions and
  Protein projection, permits only local visitor interaction state and never
  exposes Action or composition authority.

The base pattern is the recursive dot-to-plus-to-connected-mesh grid already
described below, with configurable density and optional image or SVG
wallpaper. Changing which Record fields are shown is a Protein/template choice,
not a separate semantic-zoom system. The default has no compulsory physics,
game, globe or 3D camera; motion and visual richness are opt-in while the
ordinary desk remains fast and calm.

The v1 exclusion line is equally concrete: no calendar generator, workspace
sharing, arbitrary Protein language beyond the current supported operations,
free-form force expressions, sculpted terrain/topology, planetary world,
general scene authoring, reality reconstruction, avatar/game product, or
collaborative world session is required for v1.0.0. A future capability may be
represented by a prototype fixture, stable id, port or adapter boundary only
when that seam is also needed by the v1 Box; it does not acquire a dormant
schema field or public button.

The native architecture is accepted on the owner's NixOS/Wayland machine.
Linux is Wayland-only: Winit is built without its X11 backend, the event loop is
forced to Wayland, CEF is forced through Ozone Wayland, and Linux has no Tauri,
WebKitGTK, XWayland or automatic fallback desktop. The prebuilt CEF shared
object still declares some X11-family system libraries as upstream binary
dependencies; those libraries are packaging baggage, not a Lince X11 code
path. A machine without a usable Wayland compositor reaches an honest launch
failure instead of negotiating down. V1 names any further supported operating
systems and graphics backends only after separate build, launch, input, CEF,
recovery and accessibility evidence; an untested Metal or Direct3D path is not
called supported merely because `wgpu` has that backend.

#### Plan A: GPU-first native prototype

Plan A is the accepted native runtime. A Lince-owned real-time compositor uses
Rust and `wgpu`; the currently accepted Linux path is Wayland plus Vulkan. A Lince-owned retained UI
layer supplies native application UI, text, editor surfaces, accessibility, and
reusable first-party controls over the same device and frame assembly. Its
first text path uses Glyphon/cosmic-text and its semantic projection uses
AccessKit; GPUI remains a behavior, visual-quality and implementation reference
rather than a production runtime dependency. A world renderer supplies the
retained 2D/3D scene, instanced Box material, maps, games, terrain, specialised
shaders, and future immersive visualisations. CEF supplies real Chromium HTML
as accelerated offscreen textures. These are projections behind the same Sand
graph rather than separate application models.

The native path begins with one candidate ownership constitution:

- the Lince shell owns the `winit` event loop, window lifecycle, `wgpu`
  instance/adapter/device/queue, final compositor and input router;
- the Bevy adapter runs without owning another window loop and receives the
  shared render resources through its supported manual-render initialization;
- the native UI projection receives normalized input, contributes display work
  to the host frame assembly, and publishes one AccessKit tree without owning a
  second window, surface, device, or queue submission;
- CEF remains in its required browser processes and exports accelerated
  offscreen surfaces to that same device topology; and
- the Lince frame coordinator defines when input, fixed simulation, semantic
  diffs, UI layout, world extraction, browser paint and composition occur.

The laboratory proved this constitution rather than treating it as true because
it was desirable. Its diagnostic baseline let the pinned GPUI renderer own a test
window and final submission while it composited one Lince/world texture. The
ownership comparison selected the Lince-owned candidate and rejected that GPUI source
as the v1 host or an embedded production dependency: its public seam points
from an external texture into a GPUI-owned window, while the required inverse
Linux GPU scene/command-buffer seam is absent. The GPUI-owned executable stays
only as reproducible input, IME, AccessKit, visual and lifecycle evidence. A
future exact source may reopen the dependency choice if it exposes the required
ownership without restoring a second production desktop.

Native UI nodes, Bevy ECS entities and CEF browser ids are local implementation
state. They do not point directly at one another or share arbitrary mutable
objects. Stable Lince ids and bounded snapshots, diffs and typed events cross
their adapters at declared frame boundaries. This permits several excellent
subsystems without creating several competing applications.

The durable Sand model is separated into three layers even when one generated
artifact carries all of them: a semantic graph owns identity, composition,
ports, Behavior, state planes, configuration and capabilities; a projection
manifest declares one or more compatible presentations such as retained native
UI, world GPU, installed HTML or browser DOM; and a runtime instance owns the
selected adapter and disposable local handles. Persisted definitions may name
a projection key and required capabilities, but never a Bevy entity, native UI
node, CEF browser id, GPU resource or an assumption that only one renderer can
present the Sand.

Runtime selection prioritizes capability, visual and interaction quality,
correctness, security, performance and architectural freedom over implementation
size or short-term convenience. A large refactor, pinned fork or substantial
native subsystem is acceptable when it protects those properties. Cost alone
does not select Plan B. An integration still needs a named owner, tests and an
upgrade path: accepting work is different from accepting unknowable behavior
or permanent accidental coupling.

The intended native architecture gives the Lince shell the final frame. Bevy
supplies world passes using the shared device, the Lince UI renderer supplies
native controls and editor work, and CEF supplies browser surfaces. This
follows the lesson from Pulsar's failed direct GPUI/game-loop integration
without making Bevy itself the outer application owner. The laboratory retained the
GPUI-owned diagnostic below as measured comparison evidence, not as an adapter
waiting to enter the product graph.

A GPUI-owned window that embeds one external world texture remains a useful
diagnostic and may serve a future application-only experiment, but it is not
the main Box architecture. The v1 prototype validates the Lince-owned path
with frame pacing, input, IME, accessibility, texture sharing, resize,
device-loss, and arbitrary Sand-transform evidence. The comparison found that making the
pinned source obey the inverse ownership would not be a small compositor hook:
renderer construction, Linux platform-window input, surface acquisition,
command handoff, presentation, AccessKit window integration and shared recovery
would all become maintained extraction seams. Lince therefore builds the
narrow retained UI it needs over its compositor and reuses only bounded,
licensed techniques whose lifecycle is separable.

The external-compositor work in the
[referenced GPUI fork](https://github.com/zed-industries/zed/compare/main...MSIsunny:zed:feat/external-compositor)
demonstrated an external `wgpu` texture on Linux/RADV without CPU readback and
later added Metal and DirectX bridges. It was credible prototype material, but
it did not expose the inverse ownership seam Lince needs. It remains evidence,
not a production dependency or an alternative Linux window path.

CEF remains native HTML rather than an HTML-to-GPUI translation. Blink lays out
the page, V8 runs its JavaScript, and Chromium owns Web APIs, media, storage,
networking, focus, and document semantics. Lince imports the accelerated CEF
surface into `wgpu`, composites it as a Sand, transforms pointer coordinates
back into its browser surface, and forwards keyboard, IME, focus, clipboard,
drag, popup, and accessibility information. Where a shared handle cannot be
held safely after the CEF callback, Lince makes a GPU-to-GPU copy into an owned
texture; it never makes a per-frame CPU screenshot the accepted path.

“One device” means one host rendering device, queue policy, frame coordinator
and final compositor for first-party Lince and world rendering. Chromium keeps
its required GPU process and may produce a surface from a different graphics
context or logical device. The CEF adapter must import or copy that surface
through explicit external-memory and synchronization rules, preserving format,
color space, alpha, damage, transform and lifetime without framebuffer CPU
readback. A producer process is not a competing application owner merely
because safe process isolation gives it its own graphics context.

Installed external HTML Sands receive the complete declared Sand bridge. They
may subscribe to mapped Protein inputs, emit typed outputs such as
`record-clicked`, receive Box events, request granted Actions, and participate
inside Castles. Every message is versioned, schema-checked, size- and
rate-bounded, attributed to its definition and instance, and capability-checked
by the host. An arbitrary Website remains different: it gets normal browser
network/storage behavior and host-owned navigation/focus/bounds ports, but no
Protein, Action, Lince identity, or ambient bridge unless it is deliberately
installed as a reviewed external Sand.

Camera visibility is presentation information only. An off-camera native Sand,
game, CEF Sand, video call, Protein subscription, Behavior, event route, Area
interaction, and physics body remain logically active exactly as if the camera
covered them. The native renderer culls their pixels and draw work, not their
runtime or simulation. CEF pages remain mounted and live even when Lince does
not composite their textures. The prototype must test CEF's client-controlled
begin-frame path to suppress off-camera browser painting without marking the
page hidden or throttling its script, media, network, layout semantics, or
events. If Chromium couples those concerns on a target platform, functional
activity wins and the remaining internal browser render cost is reported
honestly. Media capture, calls, audio, network sessions, timers, and game
simulation do not stop merely because the camera moved.

This does not prohibit a solver from sleeping a body that is genuinely settled
or a rule engine from evaluating only changed dependencies. Such optimisations
must be based on state and apply identically on- and off-camera; every relevant
Protein update, Area change, collision, connection, or event wakes or evaluates
the same work. There is no visibility-triggered suspension or unloading of a
runtime entity, CEF page, Behavior, media session, game, or physics semantics,
and no timer-throttling or reduced simulation tier. Replaceable visual cache
entries may follow the independent world-streaming policy. A world-scale
system therefore needs spatial indices, event-driven rules, parallel islands,
fixed subsystem rates, and GPU/CPU batching rather than hiding cost by
deactivating unseen data.

The real-time runtime owns one bounded compositor/device topology, not a device,
event loop, CEF process, or game engine per Sand. Lightweight native Sands are
instanced into retained buffers. Rich native editors remain retained UI nodes
in that same host. CEF surfaces are heavyweight and may be numerous only to the
degree measured resources allow; being off-camera removes composition cost but
deliberately does not remove their execution cost.

Physics and rendering remain separate. Native CPU ECS systems with a spatial
broad phase, parallel work and deterministic fixed steps are the first physics
path. GPU compute is used for measured large regular kernels, culling,
compaction, particles, height fields, splat sorting, or other work that can stay
on the GPU. It is not assumed to improve branch-heavy collision resolution when
upload, synchronization, or readback costs dominate.

#### Engine boundary: native UI, Bevy, GPUI research, and Pulsar

The Lince retained UI layer is the selected native application-UI path, not the
persistent Sand schema and never a second owner of the final frame. It must
earn GPUI's sharp feel through shared tokens, deterministic layout and paint,
high-quality text, immediate focus response and a complete AccessKit/IME path;
the decision does not lower that quality bar. The world engine sits behind a
narrow Lince-owned contract for scene entities, cameras, viewports, textures,
picking, input, frame timing, device recovery, and typed Sand events. This
boundary is further-looking than choosing one engine for the whole product:
Lince can improve or replace either projection without rewriting Protein
bindings, Castles, external HTML, or Box documents.

[Pulsar](https://pulsarnative.com/) is the closest architectural research
reference: GPUI editor surfaces, a separately scheduled game renderer, an ECS,
fixed-rate physics, and a final compositor. Its GPU-driven Helio ideas and
GPUI/WGPUI changes are valuable to study. Its maintainers describe the failed
direct-integration and final compositing direction in
[this discussion](https://github.com/orgs/Far-Beyond-Pulsar/discussions/40).
Pulsar also describes itself as early-stage and subject to heavy architectural
change, so Plan A must not make Lince's data or Sand model depend on Pulsar
formats, plugins, SceneDB, or editor lifecycle before a benchmark and
maintenance audit justify adoption.

[Bevy 0.19.1](https://bevy.org/news/bevy-0-19/) is the default world-runtime
candidate today: it has a much larger ecosystem, improved GPU-driven rendering,
composable scenes, mature ECS scheduling, custom render systems, render-device
recovery, Web targets, and current experiments for
[CEF surfaces](https://docs.rs/crate/bevy_cef_core/latest) and
[Gaussian splatting](https://github.com/mosure/bevy_gaussian_splatting). Its
cost is ownership and coupling: its world extraction, schedule, renderer, asset
model, release cadence, and window assumptions must fit the compositor rather
than becoming Lince's domain architecture. The first implementation therefore
uses a deliberately selected Bevy feature set behind a Lince adapter, not
unexamined `DefaultPlugins`. Bevy's window runner is optional, its high-level
2D/3D API is separable from render backends, and its renderer supports manual
initialization with an externally created device and queue. This makes the
single-owner constitution an intended integration path rather than a source
tree trick. Lince retains narrow custom `wgpu` passes as escape hatches and
comparisons instead of attempting to build a complete second engine.

Pulsar/Helio is adopted only if measured capability and maintainability beat
that path; marketing claims such as an O(1) CPU hot path are not performance
evidence because the corresponding culling and scene work still happens on the
GPU.

`winit` and `wgpu` are foundation pieces, not a world engine. Starting from
them alone would give Lince perfect ownership while also making it responsible
immediately for a render graph, batching, culling, materials, animation,
lighting, asset loading, picking, cameras, physics integration, profiling,
device recovery, and editor tooling. Lince uses `wgpu` directly where the
shared compositor or a specialised pass requires it, and keeps enough
engine-neutral tests to replace Bevy modules later. It does not prepay the cost
or delay user-facing capability by rebuilding a general-purpose subsystem
before an actual Bevy boundary fails on visual quality, correctness,
performance or freedom.

Pulling the pieces that serve Lince is acceptable and expected when done in
this order:

1. use a dependency through its public modular API when it already exposes the
   required ownership seam;
2. pin a small auditable fork when Lince needs a missing compositor, input or
   rendering hook, and maintain rebase and behavior tests for that fork;
3. upstream generally useful hooks where project direction and review allow;
4. vendor or copy only a bounded self-contained implementation whose license,
   credits, provenance and update ownership are explicit; and
5. replace a subsystem only after a representative Lince scene proves the
   existing one compromises capability, quality or performance.

Randomly copying internal types from several moving repositories would be a
mess. Reusing algorithms and crates behind one Lince-owned lifecycle is not.
The integration test is more important than whether all code originates in
one upstream repository.

The ownership boundary is the important final decision:

| Capability | Default owner | What must not own it |
| --- | --- | --- |
| Records, Protein, Actions, Sand/Castle graph, permissions | Lince semantic kernel | Bevy scenes, GPUI views, CEF DOM |
| Worlds, coordinate frames, layer/version graph, privacy and provenance | Lince spatial kernel | A game-engine save file or map provider |
| Native scene runtime, ECS scheduling, ordinary 2D/3D rendering | Bevy adapter | Persisted Lince truth |
| Window, event loop, shared GPU resources and final frame | Lince `winit`/`wgpu` shell and compositor | Bevy, GPUI or CEF independently |
| Cross-renderer texture composition | Lince compositor, with Bevy render systems supplying world passes | GPUI widget nesting or CPU screenshots |
| Inspectors, text-heavy editors, menus and accessible application chrome | Lince retained UI using shared text/input/AccessKit services | The 3D world renderer or a second application host |
| Genuine external HTML and Websites | CEF accelerated offscreen surfaces | HTML-to-native translation |
| Planetary/map data, authored geometry, captured representations and later simulation kernels | Specialised capability adapters | One universal engine abstraction |

This is not an anemic lowest-common-denominator renderer interface. Lince owns
small stable semantic contracts and capability discovery; a Bevy-backed world
may expose Bevy-specific advanced rendering internally. Persisted definitions
name the required capability and portable parameters, never a Bevy `Entity`,
component type, asset handle, schedule label, or Pulsar SceneDB object. Runtime
adapters maintain the temporary mapping from stable Lince ids to engine ids.

The intended Rust ecosystem shape is correspondingly explicit:

```text
lince-semantics      Records · Protein · Actions · Rules · Trust
lince-sands          definitions · ports · Castles · configuration
lince-spatial        worlds · frames · placement · layers · disclosure
lince-runtime        frame coordination · ids · diffs · capability routing
lince-render         winit/wgpu ownership · passes · composition · input
lince-ui             retained layout · text · focus · IME · AccessKit · controls
adapters/bevy        ECS/world projection · ordinary 2D/3D · physics
adapters/cef         installed HTML and Website surfaces
adapters/geospatial  planetary/tile selection · terrain · maps
adapters/scene       scene construction · geometry · derived render/collision forms
adapters/capture     images · video · spatial capture · reconstruction
```

These are ownership directions, not a requirement to create empty crates in
advance. A boundary earns a crate when its contract and independent tests are
real.

Pulsar can mature alongside Lince without becoming Lince's constitution.
Useful upstream research includes Linux shared-device composition,
GPUI-to-texture rendering, multiple viewports, frame pacing, device-loss
recovery, CEF texture surfaces, and Helio measurements. Lince would skew GPUI
badly by asking it to become a globe/game renderer and would skew Pulsar badly
by putting geospatial privacy, scenario history, Protein, or Sand persistence
inside its game schema. It uses GPUI as a measured application/editor reference
and Bevy substantially as intended as a game/world runtime; the unusual work
stays in Lince adapters and domain kernels.

#### Plan B: Maud/HTML-first hybrid

The existing Maud/HTML-first design is retained in full as Plan B, not erased.
Rust/Maud emits ordinary accessible HTML fragments paired with recursive Sand
nodes; native ES modules provide browser Behavior; one shared Rust/`wgpu`
WebAssembly surface supplies spatial rendering; and a Worker supplies batched
simulation. Plan A passed its prototype, so Plan B is the browser/Facade
projection and portable authoring path, not a Linux desktop fallback and not a
reason to carry a second WebView runtime.

Plan B keeps the same authoring constraints already documented: bare `Markup`
is not a composable child, Box and Maud produce the same normalized definition,
third-party HTML does not require Rust, and raw HTML remains supported. Its
known limit is that browser compositing and large populations of rich DOM nodes
cannot become a game-class world merely because a WebGPU canvas sits behind
them. Plan A is attempted first because Lince's intended maps, simulations,
large graphs, games, terrain and future spatial models make that ceiling
material.

[CanvasUI](https://canvasui.dev/) remains useful design research, but is not a
foundation choice. It does not remove the need for genuine browser semantics
for external HTML or the Lince-owned semantic Sand protocol.

#### Native Interface Laboratory

The Native Interface Laboratory was the first implementation cluster of the
v1 Interface refactor. It is a human-runnable, instrumented vertical slice,
not a visual mock or a second product. It decided that the intended native
parts can share ownership cleanly, preserve genuine HTML and meet the
representative v1 load on the owner's machine.

Its code is the root-workspace package `lince-interface` at
`crates/interface-prototype`; the directory name records its origin, not its
current status. It uses the root lockfile, contains no experimental Git
dependency, and is consumed by `lince-desktop` on Linux. Semantic crates do not
depend on it. On Linux the production desktop starts the real local Lince
server and then enters this same native Wayland runtime; there is no parallel
Tauri desktop or laboratory-only host to drift from production.

The cluster closed through five internal gates:

1. **P0a — ownership and dependency preflight.** Resolved one exact compatible
   source graph for `wgpu`, `wgpu-hal`, raw-window-handle types, Bevy, the GPUI
   candidate or fork, CEF and platform bindings. Run an empty Lince-owned
   compositor and the GPUI-owned diagnostic baseline, expose their ownership
   and adapter facts in a human-runnable panel, and export the first report.
2. **P0b — native render and input seam.** Added manual Bevy rendering, one
   narrow Lince `wgpu` pass and GPUI visual/input output. Compare frame
   ownership, scale, focus, IME, resize, teardown and recovery, then select the
   Lince-owned host and retained UI direction without a GPUI production fork.
3. **P0c — external HTML seam.** Added accelerated CEF surfaces, transformed
   input, Website/installed authority separation, storage/network/media work,
   external-memory synchronization, process teardown and renderer recovery.
4. **P0d — semantic and spatial seam.** Exercised the draft semantic graph and
   projection manifests, current Protein integration, token changes, Areas,
   the Avian-versus-specialized-solver comparison and camera invariance.
5. **P0e — joined decision.** Ran browser/Facade parity, accessibility,
   packaging, complete load and lifetime matrices and the final visual gate.
   Only the joined candidate can accept Plan A.

Every gate left a runnable scenario, machine-readable evidence and an honest
failure state. Defects found by later gates were repaired in the owning seam.

##### Starting tool set and decisions

| Part | Starting choice | Decision produced |
| --- | --- | --- |
| Outer application | Preferred Lince-owned `winit` event loop and `wgpu` instance, adapter, device, queue, swapchain and final compositor, measured against one GPUI-owned diagnostic window | Exact pins/backends, ownership, submission and device-loss policy |
| Retained native world | [Bevy 0.19.1](https://bevy.org/news/bevy-0-19/) with selected features, manual render resources and no Bevy `WinitPlugin`, Bevy UI, audio or unexamined `DefaultPlugins` | Exact feature list and any bounded Lince adapter/fork |
| V1 physics | [`avian2d` 0.7](https://github.com/avianphysics/avian) and a narrow SoA Lince field/constraint solver behind the same `PhysicsAdapter`, fixed 60 Hz with render interpolation and a measured 120 Hz comparison | Select from representative force, sort, group, boundary, collision, dirty-island and teardown evidence; no choice leaks into persisted Sand semantics |
| Native controls and editors | Lince-owned retained UI over the host WGPU frame assembly, Glyphon/cosmic-text, normalized input and AccessKit; the workspace's [GPUI 0.2.2](https://docs.rs/gpui/0.2.2/gpui/) and the exact external-compositor source remain measured quality and behavior references | Minimal retained layout/paint/focus/text/accessibility surface and the exact bounded techniques worth reusing without importing a second host |
| External HTML | Direct pinned [`cef-rs` 151.8.0+151.3.24](https://github.com/tauri-apps/cef-rs) accelerated offscreen rendering and CEF subprocesses | Exact Linux handle path, WGPU boundary, process/sandbox/package/update policy and measured admission budget |
| Accessibility | One Lince [AccessKit](https://github.com/AccessKit/accesskit) tree for native UI and custom world Sands; CEF retains Chromium semantics behind a bridged browser subtree | One inspectable, focusable composed route on Linux rather than unrelated trees |
| Coordinate seam | A small Lince-owned parent/local-frame fixture, with [Big Space's Bevy 0.19 branch](https://github.com/aevyrie/big_space/tree/bevy-0.19) as an optional measured comparison rather than a required dependency | Whether a library is useful enough to retain; authoritative coordinates remain Lince data |
| Browser/Facade projection | Existing Rust/Maud, ordinary HTML/CSS and pure JavaScript modules; no TypeScript or Datastar dependency | Proof that one shared definition fixture retains meaning outside the native renderer |

`bevy_cef_core`, Pulsar, the GPUI external-compositor work and other
integrations remained reference implementations during the laboratory. Direct `cef-rs` is
the baseline because Lince, not Bevy, owns browser-surface composition. A
dependency already appearing in `Cargo.toml` is not accepted merely by being
present: the report records exact versions/commits, enabled features, duplicate
runtime stacks, unsafe and platform code, licenses, notices, binary and compile
cost, release cadence, fork delta and named upgrade owner.

No Chromium demo flag that disables web security, ignores certificate errors
or weakens process isolation may enter an accepted fixture. Controlled network
tests use a valid local/test origin; installed packages and Websites retain
separate storage/permission partitions and all host authority still crosses the
declared bridge.

The ownership gate recorded a dependency-alignment table before visual integration began.
The table includes exact revisions, every duplicate `wgpu`/`wgpu-hal` and
raw-window-handle family, enabled Cargo features, backend APIs, external-memory
types, ownership of unsafe interop and whether a texture can cross the seam
without readback. An exact GPUI source commit replaces the crates.io baseline
when the required compositor or offscreen API cannot be applied to that
baseline; the plan does not preserve an old pin for compatibility.

The first ownership slice originally established a standalone laboratory host
in `crates/interface-prototype`; it is now the root-workspace `lince-interface`
package consumed by the desktop. `mise run interface-lab` enters a dedicated Nix
shell and opens a Lince-owned `winit` 0.30.12 / `wgpu` 29.0.4 window with a
Lynx-colored, Lato-rendered dependency and ownership panel. It records the
actual adapter, backend, driver, surface, scale and ownership candidates and
exports a versioned JSON report with `E`. Failure before GPU initialization is
also a first-class result: a missing platform library produces an unavailable
report and exits without a panic. The checked host graph resolves one aligned
`wgpu`/`wgpu-core`/`wgpu-hal` 29.0.4 family.
`mise run interface-lab-report` renders one frame, exports the same report and
exits for repeatable checks. The first successful NixOS run selected the Intel
Iris Xe integrated GPU through Vulkan and Mesa 26.1.5, used an sRGB BGRA surface
with FIFO presentation, and completed at the real 1366×740 window surface with
no initialization or render error. A later release run in the same environment
reported 1920×1052 despite the 1040×760 requested size, so initial surface size
is evidence to capture rather than an assumed constant. This is host proof, not
a frame-time or joined-compositor result.

That slice also disproved one assumption before integration work grew around
it: the workspace's crates.io GPUI 0.2.2 uses Blade Graphics 0.7, while the
external-compositor direction being evaluated is a newer `wgpu` implementation.
GPUI 0.2.2 remains only the existing compatibility baseline.

The exact GPUI-owned diagnostic input is now
[`MSIsunny/zed@bfa9c6c`](https://github.com/MSIsunny/zed/tree/bfa9c6c148f286fb4f645571ca08080c51cf0820),
pinned rather than followed by branch name. Its Linux platform path used
`gpui_wgpu` with `wgpu` 29.0.4 and raw-window-handle 0.6, so P0a aligned the
laboratory host to the same single 29.0.4 GPU family. In the diagnostic GPUI
owns the platform loop, rendering context, surface and final presentation. A
Lince compositor object receives GPUI's shared device, queue and frame-scoped
encoder, renders a live sRGB texture on the GPU and returns its view for GPUI
to sample; there is no framebuffer readback. The current fork submits that
external-compositor encoder before a separate main-scene encoder, so it proves
device-compatible texture composition but not the final submission policy.

The now-removed GPUI diagnostic opened that human-visible comparison, let the
window and live surface settle, exported its JSON report and exited. The first NixOS run selected the same Intel Iris Xe and Mesa
26.1.5 hardware without a software fallback, registered and presented the live
external texture, and reported no initialization error. The requested 920×640
window reported a 1920×1052 runtime viewport in this compositor session while
the external slot remained 420×420. Because a later Lince-owned host run also
reported the same full 1920×1052 surface instead of its requested size, P0b
classified shared compositor policy versus per-candidate initial-window
behavior and repeated it across bounded resize and scale evidence rather than
normalizing it away. That comparison is recorded below.
The cost is also now concrete: the original non-target-filtered comparison
added 463 normal dependency packages over the base laboratory. The reproducible
Linux-reachable audit counts 165 packages in the base profile and 541 in the
GPUI profile, a 376-package delta, with four pinned Git source families. That is
acceptable evidence infrastructure, not an automatic production dependency
decision. The completed P0b comparison rejected a GPUI production
extraction/fork for v1. At that stage GPUI-to-host composition, CEF runtime
integration, full joined output and live frame metrics were honestly absent;
the later joined gates supplied the accepted CEF and host evidence without
putting GPUI into production.

`mise run interface-lab-audit` now resolves five locked root-workspace profiles:
base, Bevy, physics, CEF and joined production. GPUI is absent from every
accepted profile.
It exports exact packages, activated features, source and license metadata,
duplicate critical packages, Git source families, assertions and source-review
findings. The same audit is reachable with `A` from the Lince-owned laboratory
panel. The earlier standalone report measured 303 packages for Bevy, 332 for
Bevy plus Avian, 256 for CEF after excluding its WGPU-30 convenience helper,
and 684 for all candidates. The promoted root-workspace report sees the full
771-package workspace metadata set in every profile, so package-count deltas
are no longer treated as feature reachability evidence. Its meaningful gates
are the selected feature graph, the single WGPU family, zero Git families,
license presence and the bounded source inventory.

The current audit scans the four retained ownership surfaces: `avian2d`,
`bevy_render`, `cef` and `cef-dll-sys`. Every selected package declares a
license and a license or notice file is present at or above its source root.
The packaged CEF runtime additionally carries CEF's authoritative license and
Chromium credits.

The source scan reports Rust-file counts, exact platform-name tokens, files
containing `unsafe` and lexical `unsafe` token counts. Its first run found 62
such tokens in Avian, 69 in `bevy_render`, 24 in GPUI, 30 in `gpui_wgpu`, none
in `gpui_platform`, 20,553 in `cef` and 11,702 in `cef-dll-sys`. The rejected
GPUI counts remain historical comparison evidence; the current source set
reports the Avian, Bevy and CEF values only. The CEF numbers
mostly identify its generated FFI binding surface and must not be read as a
like-for-like code-quality comparison. Lexical counts are navigation evidence,
not a semantic unsafe-code audit; each retained interop seam still needs a
named Lince owner and manual review. Likewise, platform tokens prove that a
branch exists, not that it compiles or runs. This remains a graph and source
surface, not runtime interoperability evidence.

The report assigns each retained surface rather than leaving dependency code
as nobody's responsibility:

| Resolved source | Lince owner and retention boundary |
| --- | --- |
| `avian2d` | Physics adapter; retain only if representative force and collision evidence wins |
| `bevy_render` | Native-world adapter; only manual rendering on host resources, never an engine-owned window or final compositor |
| `cef`, `cef-dll-sys` | Installed-HTML runtime adapter; CEF FFI, subprocess lifecycle and GPU-handle import stay in one fail-closed boundary |

The rejected GPUI sources have no row because they are not retained. The exact
single-WGPU graph, assigned source boundaries and machine-readable reports
closed the ownership decision; the later joined runtime accepted CEF.

Bevy 0.19.1 and Avian 0.7.0 compile in that exact graph. Bevy resolves the same
WGPU 29.0.4 family as the Lince and GPUI hosts, exposes
`RenderCreation::Manual` for caller-supplied render resources, and is included
without Bevy's window runner, UI, audio or default plugin blanket. Avian's
comparison profile keeps parallel f32 Parry collision while omitting its debug
renderer, scene and picking defaults.

`mise run interface-lab-bevy` now runs the next host fixture, and
`mise run interface-lab-bevy-report` renders, exports its ownership report and
exits. The Lince host creates the WGPU instance, adapter, device and queue, then
passes clones of those same handles through Bevy's `RenderCreation::Manual`.
Bevy has no Winit runner, primary Bevy window, schedule runner, Bevy UI, audio,
pipelined-render plugin or `DefaultPlugins`; the outer `winit` loop continues
to own application lifetime and final presentation.

The first runtime attempt exposed the implicit prerequisites hidden by the
usual Bevy plugin group. `RenderPlugin` schedules camera and render-asset work,
so omitting Bevy's window-message registration, `MeshPlugin` and base
`CameraPlugin` produced fail-fast missing-message/resource errors even in an
empty world. The corrected fixture installs an explicit minimal sequence:
task pools, frame count, time, transforms, a headless `WindowPlugin` with no
primary window or exit authority, assets, manual `RenderPlugin`, images,
meshes and the base camera resources. It does not solve this by adding the
default group. The successful NixOS report selected the same Intel Iris Xe
Vulkan device, recorded Mesa 26.2.1 in that run and completed two manually
driven empty-world updates before the host presented its panel.

The fixture now goes beyond initialization. A system in Bevy's render-graph
schedule clears a live 512×512 sRGB texture with an animated Lynx-dark field,
and the Lince compositor samples that texture behind the report text in its own
final pass. The path uses the shared WGPU device and queue, creates no Bevy
surface and performs no CPU readback. The exported report states that GPU
output is active.

The first implementation let Bevy submit the output before Lince submitted the
final compositor. Source inspection found that Bevy 0.19.1 exposes enough
public scheduling API to replace that ownership without a fork. The fixture
now removes only Bevy's stock `render_system`, retains its extraction,
preparation, pipeline-cache and render-graph work, and installs a
Lince-controlled render-graph runner after `RenderSystems::Render`. The output
system gives its completed command buffer to the outer host. Lince performs one
queue submission containing the Bevy output buffer followed by its final
compositor buffer, then presents the host surface. Bevy performs zero direct
queue submissions. This establishes the preferred submission shape through a
bounded adapter against public API; a fork is not justified at this seam yet.
The host represents that order with a small typed frame assembly: contributors
can append work, sealing consumes the assembly by appending the final Lince
compositor, and only the sealed result exposes command buffers for submission.
The report exports the ordered participant names, so a future native UI or CEF
adapter cannot become an invisible independent submit path.

Replacing the stock finalizer also means that Bevy's screenshot, GPU-readback
and Bevy-window presentation work is deliberately absent. Lince does not need
Bevy window presentation, but any accepted screenshot or readback capability
needs a host-owned equivalent and must not silently reintroduce a Bevy submit.
The semantic/spatial gate owns simulation behavior. The native input gate
exercised focus/IME, resize, presentation-surface recovery and deterministic
teardown; the joined gate subsequently exercised induced whole-device loss and
the complete accessibility route.

The version-7 ownership report separates redraw events, host presentations,
host queue submissions and submitted command-buffer count, Bevy updates,
Bevy-to-host command-buffer handoffs and direct Bevy submissions, GPUI view
renders/external compositions, keyboard input, resize and scale events, surface
recovery, CPU-side frame duration, normalized/refused input totals, the last
accepted input envelope, its target-local point, the most recent refusal,
bounded resize observations, capability registration, teardown state and the
selected ownership decision.
The latest coordinated one-frame Bevy exit recorded one host presentation, one
host queue submission containing two command buffers, one Bevy output handoff,
zero direct Bevy submissions, one initial resize event, one scale event and
19,900 microseconds from entering the redraw path through the CPU present call.
The earlier uncoordinated trace is retained as the reason this boundary
changed, not treated as the current shape. The corresponding 60-render GPUI
exit recorded 56 actual external compositions, making its registration/warm-up
gap visible instead of calling all view renders composed frames. Both automated
runs reported zero normalized and zero refused input because no person or input
driver operated either window; that empty state is distinct from an invalid
message. Neither sample is a performance result: the Bevy run has no warm-up
distribution or display-present timestamp, and the GPUI fork does not yet
expose equivalent CPU/GPU timing. They prove counter semantics and give P0b a
reproducible starting trace.

Both human-visible hosts now route observations through one renderer-independent
input envelope instead of treating an open window or unrelated counters as
input proof. Input contract version 1 carries a monotonic sequence, explicit
Winit/GPUI/replay source, physical surface size and scale, stable surface and
target semantic ids, adapter name, invertible surface-to-target transform,
target-local clip and an internally tagged event. Events cover pointer motion
and buttons, scroll units, touch phases, key press/release and repeat,
modifiers, Winit IME preedit/commit/enabled/disabled and focus. Pointer events
retain physical surface coordinates; the target transform derives local
logical coordinates, so renderer adapters do not each invent hit-space
semantics.

The boundary validates exact schema version, nonzero sequence and surface,
finite positive scale, finite invertible transforms, positive clips, finite
coordinates, bounded identifiers/key/text and valid UTF-8 byte ranges for IME
selection. Unknown event kinds and unknown fields fail deserialization;
unsupported versions and malformed payloads are refused without being counted
as accepted input. Tests cover coordinate mapping and each fail-closed case.
The version-7 report preserves the latest accepted envelope and refusal reason
so “nothing happened” cannot be confused with “input was rejected.”

The Lince-owned Winit adapter emits pointer, button, scroll, touch, physical and
logical key, modifier, focus and full IME events and records first-pending-input
to host queue-submit time separately from physical presentation latency. The
GPUI-owned diagnostic emits pointer, button, scroll, logical key, focus and IME
events into the same `interface-laboratory` target, including scale conversion
from GPUI logical pixels to physical surface coordinates. Its
`EntityInputHandler` keeps UTF-16 selection and marked ranges while converting
the normalized IME cursor to validated UTF-8 byte ranges. The target remains
tab-focusable, focuses on a left click and publishes an AccessKit `TextInput`
role, label and value. The panel and report expose accepted/refused and category
totals plus whether the IME handler, accessible node and context-recovery
callback were registered. GPUI does not expose a hardware key code through
this event surface, so that field is explicitly absent rather than fabricated.
Clipboard/drag, a joined accessible tree and a comparable GPUI event-to-submit
timestamp remain later-fixture work; the shared envelope closes the meaning and
validation seam, not every input feature.

Both running panels expose `E` as a human report action. The Winit host defers
that write until the input-triggered frame has reached the host queue submit,
so the exported report contains the key's normalized envelope and the resulting
event-to-submit sample. The GPUI panel exports on its next rendered view and
prints the path in the terminal; it does not claim a submit timestamp its
current renderer API cannot observe.

The pinned GPUI source is directional in a way the dependency graph alone did
not reveal. Its external-compositor API lets another renderer produce a texture
which a GPUI-owned window samples. It does not expose GPUI's scene as a GPU
texture or command buffer for a Lince-owned host to compose. `WgpuRenderer`
constructs a window surface, acquires its frame in `draw`, submits its external
encoder, separately submits its scene encoder and presents. GPUI's headless
scene interface is test-only, returns an RGBA image, and has a renderer only on
macOS in this source; Linux returns no headless renderer. It is therefore not a
zero-readback embedding route.

The bounded resize fixture requests five sizes and waits at most 30 rendered
frames for each. On the owner's Wayland session the Lince/Winit host remained
at 1366×740 for all 150 wait frames. This is an observed compositor-policy
result, not a hung resize: Winit 0.30.12 intentionally ignores client size
requests after a maximized, fullscreen or tiled Wayland configure. The pinned
GPUI source directly changes its Wayland surface geometry and internal drawable
size; it reported all five requested viewports in one or two frames. Those are
not equivalent policies, so the comparison did not label GPUI faster or Winit
broken. The joined Wayland fixture accepts compositor-driven resize. Both runs
used the actual 1.0 scale factor; renderer-independent tests replay physical-
to-logical mapping at 1.0, 1.25, 1.5 and 2.0. Further compositor-provided
physical scales are support-matrix evidence, not a reason to force a different
Linux backend.

The same stress run exercises lifecycle rather than terminating the processes
abruptly. The Lince host recreated and configured a new WGPU presentation
surface, presented once through it, waited for the device queue to become idle,
recorded zero teardown failures and then exited through Winit. The GPUI
diagnostic unregistered its external slot, records whether removal was
immediate or deferred until the last painted frame drained, and calls GPUI's
graceful quit instead of `process::exit`; the measured run removed immediately
with zero failures. GPUI does not expose a diagnostic queue-idle observation,
so that report value is honestly absent. The pinned renderer has a platform-
owned `device_lost`/`recover` path and notifies
`WgpuExternalCompositor::on_context_recreated`; the diagnostic registers that
callback, but no safe public hook can induce device loss. A successful
presentation-surface rebuild is not misreported as whole-device recovery.

The native seam therefore selected the Lince-owned Winit/WGPU outer host for the remaining
laboratory gates and did not retain GPUI in the v1 production dependency
graph. Putting the pinned GPUI underneath that host would require construction
from host resources, Linux offscreen scene output, command-buffer handoff,
embedded platform input/IME/AccessKit routing, and shared device-loss recovery;
these cross renderer, platform-window and accessibility ownership and are not a
small auditable fork. Choosing the GPUI-owned alternative would preserve its
sharp controls but give it the event loop, surface, final presentation and two
queue submissions, contradicting the measured one-owner frame path. Lince will
implement the narrower retained control/editor surface over its existing
frame, text, normalized-input and AccessKit services, using GPUI as a visual
and behavioral reference and reusing bounded licensed techniques only when
they do not import its application lifecycle. The version-7 reports preserve
the evidence; this document owns the architectural conclusion.

The external-HTML gate began from the Lince-owned host, frame assembly,
normalized input boundary and deterministic lifetime path established there;
defects were fixed in those owning seams rather than hidden inside the CEF
adapter.

The audit also found and removed a real CEF seam rather than hiding it. `cef`
151.8.0+151.3.24 exposes Linux accelerated DMA-BUF paint metadata and callbacks
without its optional `accelerated_osr` helper. That convenience helper resolves
WGPU 30.0.1 while the host graph is WGPU 29.0.4 and contains an automatic CPU
fallback. The laboratory therefore disables it. The CEF callback types still
compile, and every candidate profile now resolves only WGPU 29.0.4. The chosen
boundary is one small Lince-owned Linux importer that consumes CEF DMA-BUF
metadata through the host WGPU-29 Vulkan device. If GPU import is unavailable
it reaches a visible unavailable state rather than taking a CPU screenshot
path. A second application GPU context is no longer the default.

The laboratory proved that boundary on the owner's Linux/Wayland path with CEF
151.8.0+151.3.24. CEF's stable API version must be selected before the App is
created; leaving it at the wrapper's unselected value fails at process startup.
The accepted runtime uses Alloy runtime style, `ozone-platform=wayland` and
ANGLE's `gl-egl` backend. It does not disable web security, certificate checks,
the sandbox or process isolation. CEF produces single-plane BGRA DMA-BUFs; the
Lince adapter validates the plane, dimensions, stride, allocation, format and
DRM modifier, duplicates the callback-owned fd, imports it through Vulkan,
copies it into Lince-owned device memory before the callback returns, fences
that copy, destroys the temporary import and composes the owned image into the
WGPU swapchain. The current 12-second joined report observed 711
imported/copy-complete installed frames and two Website frames, with no CPU paint callback and no
framebuffer readback. CEF provides no separate sync-fd in this callback shape;
the callback readiness contract delimits producer completion and the host
fence delimits Lince's copy and fd lifetime. This is an explicit GPU copy, not
a zero-copy claim.

The fixture also found a content-boundary defect before it became an adapter
workaround: CEF's stream helper expects a bare MIME type. Advertising
`text/html; charset=utf-8` made Chromium display the installed source as a
document, so no script or bridge ran. `text/html` plus the document's UTF-8
metadata executes the genuine installed page. The installed and Website Sands
now use separate CEF request contexts. The installed custom origin has a
persistent profile and its title reported storage available with a monotonically
persisted sequence across process runs; the Website diagnostic uses an
ephemeral profile. Both exercised ordinary HTTPS requests. Only the installed
main-frame origin receives the versioned V8 bridge; a granted Protein request
and a deliberately unknown operation produced two allowed and one fail-closed
decision, while the Website reported the bridge absent. An undeclared media
request and a trusted-input popup attempt were refused. Browser close callbacks
completed for both surfaces before the automated report was written.

`mise run interface-lab-cef` is the human fixture and
`mise run interface-lab-cef-report` is its bounded machine-readable run. The
runtime copies CEF's authoritative `LICENSE.txt` and 19 MB Chromium
`CREDITS.html` into its notice evidence and refuses startup if either is
missing. The joined evidence closes the Linux accelerated surface, authority,
storage/network/media, CPU-fallback, notice, off-camera and orderly-lifetime
baseline. A deliberate renderer crash recovered, and an induced host-device
rebuild completed in 45.812 ms. Count runs with 0, 1, 4 and 12 admitted CEF
Sands all passed on Wayland; their frame p95 values were respectively 9.454,
9.878, 11.772 and 11.578 ms. Off-camera CEF copied no frames during its measured
interval while its bridge advanced four events, so presentation was culled
without suspending behavior.

The semantic and spatial gate also closed with renderer-independent graph and
projection fixtures, current Protein output, token diffs, Areas, fixed-step
physics comparison and camera invariance. Its 10,000-instance run measured the
specialized field solver at 3.648 ms p95 and the bounded Avian comparison at
1.077 ms p95. The joined runtime runs all 1,000 eligible bodies at 120 Hz,
carries rather than discards fixed-step backlog, and reached zero pending steps
at every accepted report boundary.

Big Space and physics are deliberately separate experiments. The laboratory proves one
high-precision parent frame, a camera-local render frame and one local physics
island near the floating origin; it does not claim that Avian bodies can move
unchanged across planetary grids or multiple floating origins. That broader
integration belongs to v2 research.

##### Host interfaces exercised by the prototype

These are narrow runtime seams, not a universal lowest-common-denominator
engine API and not yet persisted schema names:

| Seam | Minimum information crossing it |
| --- | --- |
| Sand semantic projection | Stable definition/instance/child ids, composition, resolved tokens, typed ports, Behavior, capabilities, state-plane snapshot and bounded semantic diff |
| Renderer projection manifest | Projection key, required renderer capabilities, presentation assets and compatible adapters without runtime handles or a claim that one backend is the Sand's meaning |
| Frame participant | Prepare, fixed-step update, extract/damage, encode, compose and deterministic teardown; the coordinator owns ordering and deadlines |
| GPU surface | Device-compatible texture, size, color/alpha space, damage, transform, clip, z-order, synchronization and explicit lifetime; never a CPU screenshot |
| Input target | Hit result, inverse coordinate transform, pointer/keyboard/IME/clipboard/drag data, focus and capture; adapters return handled state rather than reading global input |
| Physics adapter | Stable body/group ids, simple shapes, force/sort/constraint fields, fixed-step input, transform output and awake/dirty reasons independent of camera visibility |
| Installed-HTML bridge | Versioned instance identity, declared typed inputs/outputs, bounded Box events, local state and capability-checked Action requests |
| Accessibility projection | Stable accessible ids, roles, names, bounds, focus, values and actions for visible or logically focused native scene elements, joined with CEF focus routing |
| Measurement sink | Timestamped CPU/GPU phases, simulation counts, dirty/upload bytes, memory/process counts, latency markers, backend/driver/build identity and assertion failures |

The prototype uses draft Rust fixture types for these seams and rejects unknown
versions and messages. Only the Sand, Protein, Action, Customization and stable
identity meanings already required by v1 may be promoted into the production
contract; framebuffer handles, Bevy entities, native UI nodes, CEF ids,
physics handles and laboratory scenario fields remain runtime-local.

##### Human-visible acceptance fixtures

The in-app laboratory panel exposes these eight switchable scenarios, live
metrics and failures, a deterministic seed and machine-readable report export:

1. **Composition and character.** Render a standalone Button Sand and the same
   definition inside a locked Record-card Castle with text, panel and dropdown
   children. Read one actual current Protein stream, show its input shape,
   field-to-port arrows, one
   unbound field, a `record-clicked` event route, Why-is-it-here, edit/focus/
   invalid/empty states, light and dark themes and one instance override.
   Retained native chrome and world Sands must remain visually crisp and token-equivalent
   at 1.0, 1.25, 1.5 and 2.0 scale factors. Live-change a global palette,
   density and radius token and prove inheritance and the local override update
   without recreating a Sand or CEF browser.
2. **Joined compositor.** Place retained native Sands and continuously animated
   world material below Lince editing chrome, one installed CEF Sand and one
   Website Sand. Move, scale, clip, overlap, focus and reorder all surfaces.
   Resize repeatedly and exercise pointer capture, keyboard traversal, IME,
   clipboard, drag/drop, browser popup and clean teardown.
3. **External authority.** Give the installed Sand a mapped Protein input,
   typed Box input/output, local storage, controlled HTTPS request and one
   visibly granted Action request. Give the Website normal browsing, network
   and origin storage but prove every installed-Sand handshake and Lince call
   fails closed. Include animation, media and a loopback WebRTC workload.
4. **Box motion.** Use deterministic force, repel, sort, constraint and weak
   centre fields with ordinary Sands, locked groups and collisions. Let one
   mutation field preview and request a typed Action against laboratory data.
   The load
   ladder is 200 visible interactive Sands, 1,000 continuously eligible moving
   bodies and 10,000 resident lightweight nodes/connections, with a 100,000
   static/indexed-node stretch case. Exercise local drag, dense collision,
   global force, group movement and continuous motion.
5. **Camera invariance.** Run the identical seeded simulation, Protein/event/
   Action trace, timers, media and CEF work on-camera and off-camera. Only
   extraction, paint and composition counts may change. Settling based on
   physical state is allowed; visibility-based sleep, suspension, unloading,
   rate reduction or Behavior change is a failure.
6. **Coordinate and specialised-pass seam.** Render the 2D desk beside a small
   3D height field or scene, cross one high-precision cell/origin boundary,
   place a CEF surface in that scene and load one open scene/geometry
   interchange artifact, for example glTF. Produce one replaceable collision
   or distance-field proxy and send one picked interaction through a typed Sand
   event. The required fixture uses the Lince parent/local-frame contract;
   Big Space may run as a comparison but is not required to pass or remain a
   dependency. This proves representation and adapter boundaries, not a globe
   or scene editor.
7. **Definition/replay and browser parity.** Save and replay the normalized
   Button/Record-card graph and a short prototype-local operation trace, then
   render the same definition through the Maud/HTML/JavaScript path, compare
   identities, ports and behavior fixtures, and prove the read-only Facade
   projection omits Action and editor authority. The replay format is laboratory
   evidence, not the production Box document.
8. **Accessibility and recovery.** Inspect the composed tree with Linux AT-SPI
   tooling and a screen reader, operate the scenarios without a pointer, then
   exercise GPU device loss, CEF renderer loss, malformed bridge messages and
   restart without leaked processes or invisible focus.

CEF is a heavyweight renderer with an explicit admission budget, not the node
type used for thousands of markers. The laboratory measures 0, 1, 4 and 12 live CEF Sands.
V1 guarantees that every admitted instance stays functionally active when
off-camera; it does not promise an unbounded browser count. If 12 exceeds the
machine budget, the report defines the honest resource estimate and admission
surface rather than silently freezing older pages.

##### Reproducible benchmark contract

Every performance run records the git revision, dependency locks, release
profile, scenario/seed, backend, adapter/driver, CPU/GPU/RAM, power mode,
resolution/scale, visible/resident/awake counts and CEF count. It warms for 30
seconds, samples for 120 seconds, repeats at least three times and exports raw
samples plus p50/p95/p99 summaries. Shader and pipeline compilation happens in
warm-up or is reported as a separate cold-start measurement; it is not hidden
inside an average.

Each benchmark scenario also versions its workload morphology: Sand pixel
size, text and glyph density, connection degree, body shape and size
distribution, moving/settled ratio, Area coverage, local/global dirty set, CEF
surface dimensions and content workload, media state and deterministic input
script. Separate scaling curves identify the saturation point of the empty
shell, each adapter, the physics candidates and the joined workload before the
mixed hard gate is interpreted. A count without those facts is not comparable
evidence.

Correctness uses `cargo check` across the affected workspace targets with
warnings denied, plus focused unit/integration tests. Performance runs execute
the instrumented release binary directly (for example through `cargo run
--release`); they do not substitute debug results or a bare `cargo build` for a
human-runnable scenario.

The required machine is the owner's Vostro 3150 with an 11th-generation Intel
Core i7, Iris Xe integrated graphics, 16 GB RAM and NixOS. Native display
resolution is the hard gate. A 3840×2160 offscreen target and 120 Hz presentation
are recorded stretch gates until matching physical hardware is available.
An empty world-only pass and the rejected GPUI-owned experiment remain
comparison evidence; the accepted Linux desktop itself is the joined native
runtime, so Tauri is no longer a Linux baseline or fallback.

| Hard gate at native resolution | Acceptance threshold |
| --- | --- |
| Mixed Box load: 200 visible interactive Sands, 1,000 continuously eligible bodies, 10,000 resident light nodes | After warm-up, p95 frame time at or below 16.67 ms, p99 at or below 25 ms, and no more than one unexplained frame above 50 ms in a 120-second steady sample |
| Direct manipulation latency | Pointer/keyboard event to presented result p95 at or below 33.4 ms and p99 at or below 50 ms |
| Fixed simulation | 120 Hz remains independent of render rate; the representative 1,000-body step has p95 CPU time at or below 8 ms, drops no elapsed time and reports any carried backlog |
| Partial work | A local drag updates the affected island and damaged buffers/surfaces rather than uploading or laying out the complete 10,000-node world; global-field work is separately identified |
| Protein and configuration diffs | Changing 1% of a 10,000-row generated Protein fixture updates only affected bindings/instances; a global token change reaches the 200 visible Sands without CEF reload; each has p95 source-diff-to-present latency at or below 50 ms |
| GPU composition | Zero per-frame framebuffer readback; texture copies, synchronization waits and upload bytes are measured and bounded by changed content |
| Camera invariance | On/off-camera semantic event, Action, timer and fixed-step traces are identical for the same seed and inputs; only presentation traces differ |
| CEF | The 0/1/4 cases retain the frame and input-latency gates while all admitted pages remain live; the 12 case is a required characterization and resource-budget decision |
| Lifetime | After ten create/destroy/reset cycles, all browser processes, GPU objects, routes and subscriptions return to their expected counts, and RSS/VRAM shows no unexplained monotonic growth above a 5% post-warm plateau |
| Startup and recovery | Warm start reaches an interactive native window within 2 seconds and cold start within 4 seconds; lazy first-CEF readiness is reported separately; device/renderer recovery restores visible state and focus without semantic replay or data loss |
| Visual/accessibility/security | No clipping, blur or focus discontinuity at tested scales; keyboard, IME and AT-SPI/Orca paths complete the fixture; malformed/unknown bridge operations and all Website bridge attempts fail closed |

An average frame rate alone cannot pass the laboratory. The report includes hitch counts,
input-to-present latency, CPU and GPU phase time, fixed-step debt, dirty/awake
sets, buffer and texture traffic, process/RSS/VRAM growth, initialization time,
packaging size and power observations. Instrumentation overhead is measured
with the overlay hidden and shown.

Instrumented event-to-submit and compositor presentation timestamps are
reported separately. The accepted Wayland path does not expose a trustworthy
physical-display timestamp through WGPU, so the report calls its input-to-
present-call measurement a lower bound and never relabels it as what the person
saw. Camera-based input-to-visible validation remains a hardware/display
quality check; Lince will not add an X11 probe or fallback to manufacture that
number.

##### Acceptance and handoff

Plan A passed through the joined scenario—not separate native UI, Bevy or CEF
demos—across ownership, correctness, visual character, accessibility,
security and native-resolution hard gates. No GPUI production fork was
retained. Plan B remains a browser and Facade projection strategy, not a Linux
desktop fallback.

The exit artifact contains exact dependency and license inventory, architecture
decision records, raw and summarized benchmark results, known platform limits,
accepted adapter contracts and a disposition for every laboratory module. Only
Work now advances to Customization C0–C2, the C3 Sand/Castle composition
workbench, C4 official-Sand migration and C5 completion gate; then Box
foundations, Protein result templates, Areas and Actions, persistence, external
HTML hardening and the public Live Facade. No later cluster works around a
failed native foundation seam.

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

For example, a Record result can expose `title`, `description`, `quantity`, and
other fields. `title` may connect to a text Sand, `quantity` to a number Sand, and the
complete Record identity to a button that performs an Action. If five rows
arrive, Box produces five bound instances of the same locked group. The group
is referenced as a template rather than copied HTML, so editing its definition
updates every result instance while each instance retains its row binding and
Box position.

A mapped child receives only the field or object explicitly wired to its typed
input. Incompatible connections are rejected visibly; missing optional values
remain honest empty values. The template does not change automatically with
camera zoom. Whether it receives a title, full description, bounded
description excerpt, or another representation is determined by the configured
Protein output and the visible field mapping.

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

Result disappearance and shape drift are explicit states. When a stable row
stops arriving, the runtime removes its live projection but retains bounded
recoverable instance-local state by Protein identity and row key for a visible
grace policy; it never leaves an unexplained blank or deletes Ledger data.
When a result field is removed or changes to an incompatible type, the binding
becomes visibly broken, the last valid definition remains editable, and the
person can reconnect, clear or intentionally replace it. **Why is it here?**
shows whether an appearance is live, retired, restored or awaiting binding
repair.

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
- A **mutation area** previews declared typed Actions when a compatible bound
  group enters it. It begins disarmed and runs them only after the person gives
  that Area a durable, inspectable grant. The first mappings change quantity
  and add or remove Concepts.
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
Each attempted visit carries a stable idempotency key derived from the Area,
bound appearance and entry occurrence, so retries cannot duplicate a durable
Action. Edit and view mode show whether the Area is previewing, armed, paused
or failed, its grant and recent outcomes, and provide an immediate disarm
control.

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

### Long-horizon world direction

These ideas motivate Plan A's capability ceiling but are not part of the
current Box implementation plan. They remain one connected future direction
rather than being lost as unrelated feature phrases. The runtime boundary is
designed for them now so that the current Box does not become a disposable
shortcut, but their product and data semantics remain future work until their
own implementation clusters are written.

The capability is the intention here, not any named creative program. Lince
must eventually let a person construct and edit scenes and spatial objects,
draw or generate them from instructions, combine representations, derive useful
render/physics forms and turn a proposed world into organised work. Programs
such as Blender, CAD systems, game editors and geographic tools, and formats
such as glTF, are illustrative sources of workflows or artifacts. V2 may
integrate one, borrow an interaction, use a specialised library or implement a
Lince-native editor according to evidence available when that cluster begins.

#### Target experience

Lince may grow from a paper-like Box into an editable model of the world. One
continuous navigation language should cover a globe, country, city,
neighbourhood, house, room, floor, desk and constructed game world. Looking
straight down at a local plane can feel like today's 2D Box; pulling away can
reveal terrain, a globe and geospatial analysis. The transition must not imply
that all levels are one giant flat texture or one imprecise game scene.

People may model the parties and Organs they interact with, physical and
digital resources, Needs, Contributions, planned structures and the progress
of Transfers. They may replace the Earth presentation with a wholly invented
world, or layer a castle, spaceship, fully customisable avatar, game rules,
shaders and sprites over a real neighbourhood. Reality-derived captures,
ordinary meshes, structured geometry, map data, volumetric or point-based
representations and fantasy objects may coexist, alternate, mask or blend
without destroying one another. Private worlds and sessions may publish
selected submissions into a shared world while retaining their provenance and
withdrawal policy.

The long-term product is therefore not one game or one particular geographic
or scene-authoring program. It is a semantic world kernel with multiple
specialised projections.
The same Need can be inspected as a Record, a Sand card, a point on Earth, a
height field, a task in a construction scenario or an actor in a game without
duplicating or changing its identity.

#### Final architecture: a Lince kernel with replaceable organs

The architecture is deliberately federated:

```text
Ledger / Protein / Actions / Trust / Rules
                    │
        Lince semantic + spatial kernel
   ids · worlds · frames · layers · time · privacy
                    │
       runtime projection and capability ports
       ┌────────────┼──────────────┬─────────────┐
       │            │              │             │
  Bevy world   Lince native   CEF HTML      specialised
  projection   UI/editors     surfaces      geo/scene/capture
       └────────────┴──── Lince wgpu compositor ─┘
```

Lince owns durable meaning. The Bevy runtime holds a performance-oriented
projection of the active world; it may be rebuilt from Lince state and may
change across engine upgrades. Lince's retained UI owns application-grade
editing surfaces on the host device. CEF owns genuine HTML execution.
Geospatial streaming, scene/geometry construction and reality-capture
algorithms remain capability adapters instead of being reimplemented inside a
generic Sand ABI. The `wgpu` compositor combines their GPU results and routes
input without CPU screenshots.

This is further than adopting Pulsar: the advancement is not a larger engine
but a world model that can employ several engines without making any one of
them its database. Pulsar may supply or receive compositor, renderer and editor
work. Bevy may remain in the final product for ordinary world simulation and
rendering. A future GPUI source may be reconsidered for a separate tool or a
genuinely host-owned offscreen adapter, but the v1 pin is not retained. No
engine or UI toolkit is expected to express Lince's geospatial disclosure,
reality/fantasy branches, Protein bindings, or Transfer lifecycle.

#### Coordinate frames from Earth to a desk

One `f32` Cartesian scene cannot preserve useful precision from an entire
planet down to objects inside a house. Lince needs an explicit frame graph:

- Earth truth is stored in high-precision geodetic coordinates and/or
  Earth-centred Earth-fixed coordinates, with the coordinate reference system,
  datum, altitude reference, uncertainty, source and observation time named;
- a city, site, building, room or desk uses a nested local tangent or authored
  frame, with a high-precision transform back to its parent;
- the renderer converts the nearby frame into camera-relative `f32`
  coordinates or integer-grid cells so GPU transforms remain stable;
- an invented planet or game world has its own units, axes, gravity, bounds and
  frame tree, and may be attached to Earth by a deliberate anchor or portal;
- a 2D floor/desk view is an orthographic camera over a local frame, not a
  projection that flattens or rewrites the underlying Earth positions.

The engine adapter may initially use a Bevy floating-origin or nested-grid
plugin such as [Big Space](https://github.com/aevyrie/big_space), but the
authoritative frame graph remains Lince data and current Bevy-version support
must be verified rather than assumed. The selected path must round-trip across
cell boundaries, multiple cameras, selection, physics, imports and
collaboration before it is trusted. A renderer-local origin is a view
implementation detail and never appears as a person's actual location.

#### Worlds, layers, branches and time

A world is a versioned definition and layer stack, not a single mutable scene
file. At minimum the model must be able to distinguish:

- a base Earth/map/terrain layer;
- observations and reality captures, including imagery, scans and splats;
- private exact placements and personal arrangements;
- disclosed or community-published representations;
- fantasy/game geometry and rules;
- proposed, desired-future and construction-plan layers; and
- historical observations and alternate scenarios.

Each layer names its world and coordinate frame, owner, sources, license,
visibility and edit policy, time extent, version, provenance and blend or mask
relationship. Switching between real, fantasy and planned worlds is then a
change of view or layer stack, not a destructive replacement. A castle may
stand inside a captured street only in one branch; a later observation may
show how much of a planned structure now exists; both remain inspectable.

Large assets are content-addressed artifacts referenced by these layers.
Semantic operations, permissions, hashes, manifests and small editable
parameters remain human-inspectable Lince state; terrain tiles, video, dense
splats and mesh caches do not become enormous inline `.lingua` values.

#### True location, disclosed location and live proxies

Moving an Organ from elsewhere beside a local Need for analysis must not alter
the Organ's geographic claim. Lince distinguishes:

- the source entity and its authorized real or declared placement;
- the location disclosed to the current audience, which may be a city,
  region, cell, radius or other uncertainty envelope rather than a point; and
- one or more presentation proxies with local layout transforms, anchors and
  session state.

A proxy is a live view of the same source, not a copied Ledger Record. It may
be placed on a desk, inside a planning scene or beside a Need, while “Why is it
here?” explains the source, Protein, proxy owner, disclosed location and local
transform. Editing source data follows normal authority; moving the proxy only
changes the owning Box/world placement unless an explicit Action proposes a
real relocation.

Location privacy is enforced before data reaches a renderer. Lince must not
send an exact home coordinate and rely on a blurred marker or zoom restriction
to hide it. An audience-specific Protein or authorization boundary returns the
coarsened geometry that audience may know. Exact private indoor placement may
therefore coexist with a city-level public Organ location without leaking the
transform between them. Location claims include provenance, precision,
freshness and confidence so inferred, declared and measured places are not
silently equivalent.

#### Terrain, topology and editable worlds

The term topology covers several different kinds of data and they must remain
separable:

- geographic elevation or bathymetry is observed terrain with units and a
  source;
- a Lince semantic field visualises quantities such as Needs or Contributions
  above the terrain without pretending to be physical elevation;
- authored terrain deformations, buildings and fantasy objects belong to an
  editable layer; and
- structured geometry may retain an exact parametric, constructive or
  boundary-representation source while exposing derived meshes for rendering
  and collision.

A brush may raise, flatten, paint or mask an authored layer and shaders may
blend its result with the globe. Base map or captured reality is not silently
rewritten. An edit is a versioned operation with bounds, units, author and
target layer, so it can be previewed, undone, replayed, merged and compared
over time. The same source can produce a globe surface, local high-resolution
terrain, an orthographic 2D floor and simplified physics proxies.

A game engine is well suited to meshes, materials, lights, animation, avatars,
physics, shaders, particles and picking. It is not automatically a general
scene-construction system or an exact geometry kernel. Where an authored source
needs exact parametric or boundary-representation operations, a specialised
capability may use an audited library—for example
[Open CASCADE Technology](https://dev.opencascade.org/doc/overview/html/index.html)
or a mature Rust geometry project such as
[Truck](https://github.com/ricosjp/truck)—or a future Lince-native method.
Those names are examples, not selected dependencies. The authored document and
operations remain authoritative; generated render, collision and LOD forms are
replaceable caches consumed by the world runtime.

#### Spatial artifacts and an editable common world

The globe-to-desk gradient should be one native world runtime and compositor,
not a GIS WebView that hands off to a separate game window. It may contain
several specialised render passes, but one camera/frame graph, spatial identity
model, selection system, input router and Sand event system make the transition
continuous. Orthographic desk, local perspective, globe and avatar views are
camera and layer configurations over that runtime. Pinned Sands use
viewport-space anchors while ordinary Sands and geometry use world or local
frame anchors.

Lince should not prematurely convert every source into one supposedly universal
format. Exact structured solids, authored surface meshes, terrain height
fields, voxel or signed-distance fields, point/volumetric captures, images and
semantic Sands have different strengths. Converting all of them irreversibly
into one representation would lose exact dimensions, editable history,
appearance or capture information. Instead a **Spatial Artifact** keeps:

- one stable semantic identity and owning world/frame placement;
- the original or authoritative representation, units, axes, licenses,
  provenance and edit history;
- any number of content-addressed derived representations, such as an open
  interchange mesh, meshlets and LODs, voxel/SDF field, collision shape,
  navigation data, captured-scene chunks, thumbnail or low-cost proxy;
- the conversion recipe, tool/version, tolerance, error bounds and source hash
  for every derived representation; and
- Sand/Protein/Event/Action bindings that attach behavior to the semantic
  entity rather than to one mesh or voxel buffer.

An artifact from an external authoring system may therefore be rendered as a
mesh, participate in topology through a derived voxel/SDF or collision field,
remain editable through its authoritative representation or operation history,
and emit collisions or direct-manipulation events to nearby Sands. Changing
the source invalidates and rebuilds affected derived artifacts. Editing a lossy
derivative does not silently rewrite the exact source; it creates an authored
overlay, a new source revision or an explicit conversion result according to
the capability being used.

The research problem is not whether `wgpu` can draw all these representations;
it can host their passes. The hard work is robust conversion, multiresolution
editing, topology and boolean semantics, precision across frames, provenance,
collaborative operations and deciding which representation an edit is allowed
to change. V1 proves the identity, anchoring, renderer and event seams with a
small mesh/height-field artifact. V2 may mature the common editable world
without making v1 wait for a universal geometry theory.

#### Planet-scale streaming and activity

No engine keeps a whole high-resolution Earth resident or submits it every
frame. Globe content uses spatially indexed, out-of-core hierarchies with
frustum and horizon culling, screen-space error, progressive level of detail,
virtual or tiled textures, bounded CPU/GPU caches and cancellable streaming.
[OGC 3D Tiles](https://docs.ogc.org/cs/22-025r4/22-025r4.html) is a relevant
interchange and streaming standard.
[Cesium Native](https://cesium.com/learn/cesium-native/ref-doc/index.html) is a
strong candidate capability for WGS84 math, tile selection, cache management,
glTF decoding, terrain and raster overlays, even if using its C++ library
through a narrow Rust boundary is less comfortable than an all-Rust stack. It
is a geospatial organ, not Lince's world model or mandatory cloud service; its
pre-1.0 breaking-change policy also requires a pinned adapter and upgrade
tests.

This visual streaming does not violate camera-invariant behavior. Lince may
discard an invisible high-resolution texture, mesh, splat chunk or draw
instance because those are presentation caches. The entity, Protein binding,
game, media call, Area, physics semantics and events stay active at the cadence
their subsystem declares. Camera visibility never chooses that cadence. A
settled body may sleep because it is settled, a Rule may be dependency-driven,
and a global geospatial entity may update on events rather than at 60 Hz; each
decision must be identical whether the camera sees it. An off-camera game or
video call that declares continuous execution continues to execute.

Earth scale also requires different spatial indices for different work. Tile
selection, semantic location queries, broad-phase collisions, navigation,
network interest and private disclosure are related but not interchangeable.
They may share cell identifiers at boundaries, yet one universal octree must
not be forced to own every subsystem.

#### Sands and HTML inside worlds

The Sand graph remains above the renderer split. A native Sand, Castle, CEF
Sand and Website can be anchored to an Earth point, local frame, viewport,
avatar or another Sand. A CEF surface may be placed on a 2D Box plane, a panel
inside a 3D structure or a screen held by an avatar while retaining Chromium
HTML, JavaScript, media, storage and networking. Installed external Sands use
the same typed Protein inputs, Box events and Action requests already defined;
an arbitrary Website remains outside Lince authority.

CEF is not cheap enough to represent every map marker or lightweight node.
Thousands of ordinary Sands are instanced native scene data; rich editor nodes
use the retained Lince UI and a smaller measured population uses CEF where its
semantics justify the cost. This is a renderer choice for one Sand definition,
not a reduction in its composability. A Castle can contain native world
objects, retained editing surfaces and CEF-backed HTML children while its
persistent composition remains one Sand graph.

#### From captured reality to organised work

A video, scan or reconstructed spatial capture of land may seed a reality
layer. A future world model may propose geometry, a structure, detected resources, Needs,
risks, candidate Organs or a sequence of work. A person must be able to edit
the proposal spatially and semantically before accepting it. Distillation then
produces attributable Records, Needs, Contributions, Transfers and Actions;
the model does not directly convert pixels into unquestioned Ledger facts or
grant itself authority.

The desired world and observed world remain separate, time-aware branches.
Lince can display their diff, planned dependencies, responsible parties,
resource locations, Transfer progress and later observations so a person can
watch reality approach or diverge from the model. Physical inventory and
digital assets, including externally proven blockchain assets when useful, use
typed provenance and authority adapters. Merely drawing an asset in a scene
does not establish ownership, availability or custody.

Automation may sequence granted Actions and Transfers toward explicit
completion criteria, but it needs budgets, cancellation, review points,
attribution and an honest blocked state. A world model can propose the next
work; it cannot silently widen its authority because the observed world has
not yet matched the desired one.

Collaborative sessions separate durable semantic edits from ephemeral presence.
World/layer operations, Sand composition and accepted Actions use normal sync,
attribution and conflict semantics. Avatar pose, cursor, voice, transient game
state and preview physics use bounded live-session channels and only become
durable when an explicit feature says so. Public or community world
submissions require consent, license, moderation/trust, provenance, redaction,
version and withdrawal rules before they appear in another person's world.

**The Game of Life / Digital Real World Maps.** A World or Map Sand could show
the whole globe and move continuously into streets rendered as a plane of
lines, then into a person's authorised local frames. It could project people,
Organs, Records, Needs, Contributions, and Transfer Proposals into their real
or declared places.
Elevation and terrain data could provide the physical landscape. Lince data
could add a separate semantic height field: a concentration or quantity of
Needs may rise like a mountain, Contributions may answer or reshape it, and
the visual difference between geographic elevation and data-derived elevation
must remain inspectable. Transfer Proposal could show proximity, candidate
contributors, routes, hand-offs, progress, and delivery without confusing a
visual route with a promise or a completed transfer.

This future requires geospatial indexing, coordinate and projection choices,
source provenance, stale-location and privacy semantics, offline/streamed tile
budgets, terrain/DEM support, and license attribution for every map or imagery
source. A map renderer is a specialised Sand renderer behind the common ports,
not a new source of Ledger truth. Current Rust map renderers and MapLibre
texture-sharing work are research inputs; their missing features and licenses
must be audited when this work becomes planned.

**Captured streets and cities.** A point, street, building, or city could be
represented by a reconstructed spatial scene—Gaussian splatting is one current
example—and combined with ordinary geometry, terrain, map labels, and Lince
overlays. Progressive streaming, spatial
chunks, level of detail, GPU sorting, compression, provenance, capture consent,
redaction, storage size, and device budgets are prerequisites. The renderer may
show a low-cost proxy while data streams, but camera visibility must not alter
the underlying Record, game, Protein, or Area behavior. Future world models,
JEPA-like systems, or language models may propose classifications, Needs,
tasks, or responsible Organs from this material, but their outputs remain
attributed proposals requiring the applicable Trust, Karma, Rule, and Action
path; pixels never become unquestioned Ledger facts.

**Records as game material.** A game may derive a deterministic seed, terrain,
actors, resources, quests, enemies, or rules from Records and their quantities.
A larger quantity might become a larger mountain; a Need might become an
obstacle to resolve; Karma may parameterise rules, affordances, scoring, or the
consequences of choices. This is a projection of data into play, not permission
for a frame loop to mutate the Ledger. A game interaction emits a typed event
and, when durable change is intended, requests an attributed Action. Ephemeral
simulation state remains separate from Box host state and Ledger truth.

Karma and Rule evaluation should be dependency-driven and incremental rather
than blindly rerun every render frame. “As frequently as possible” means a
relevant change becomes visible with the lowest honest latency allowed by its
declared semantics; it does not mean spending GPU or CPU time reevaluating
unchanged truth. Visual motion may interpolate at display rate while durable
rules evaluate on changed inputs or an explicit fixed simulation cadence.

**Interface as direct manipulation of the model.** Box may eventually remove
many layers between database, driver, schema editor, application builder, and
runtime: editing a concept, field mapping, Protein, Action, Sand graph, or Rule
in the interface changes the corresponding semantic model immediately and the
same running binary reflects it. This must not mean altering SQLite's physical
schema whenever a Sand moves or inventing an unversioned database shape from
pixels. The interface emits the same validated, attributable schema and data
operations that any other client or agent would use, with preview, failure,
history, and recovery surfaces.

A distributable Lince artifact may embed its engine, migrations, built-in Sand
definitions, shaders, assets, default configuration, and licenses so it has no
runtime framework installation dependency. Mutable user data, secrets,
downloaded external packages, browser profiles, caches, and network content
cannot literally live forever inside an immutable executable. “The binary is
everything” is therefore pursued as one self-describing, portable runtime and
package contract, not as denial that durable mutable state has bytes and a
lifecycle outside the executable image.

Games are an optional interface expression, never the mandatory way to edit a
Record. The paper-like 2D Box and quick conventional controls remain available;
the same Sand graph may also become a playful 2D simulation, 3D world, map,
spreadsheet-scale GPU view, or immersive scene when that better communicates
the person's Need.

## What is left

### Interface — what is left

#### Landed native interface foundation

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
Iris Xe Wayland session, the current joined 12-second run measured 10.160 ms
frame p95, 12.083 ms p99, 4.045 ms CPU-frame p95, 0.524 ms fixed-step p95,
zero fixed-step backlog, 2.812 ms input-to-present-call p95 and 4.415 ms p99.
It reached an interactive window in 780.264 ms and the first CEF frame in
1,349.038 ms. These last input numbers are explicitly present-call lower
bounds; physical display timing was not measured.

The required three-repeat benchmark then ran 30 seconds of warm-up and 120
seconds of sampling per repeat with the same source fingerprint and no frame
above 50 ms. Frame p95 ranged from 10.124 to 10.836 ms and p99 from 12.618 to
14.232 ms; CPU-frame p95 ranged from 3.969 to 4.776 ms, fixed-step p95 from
0.491 to 0.517 ms, and input-to-present-call p95 from 2.764 to 3.702 ms. All
three reports used Wayland, Vulkan, Mailbox presentation, the 1920×1052 surface
and the Intel Iris Xe/Mesa 26.1.2 stack in balanced power mode.

The current acceptance evidence also includes the CEF count matrix, live
off-camera behavior with suppressed copying, browser and read-only Facade
parity, AT-SPI traversal and activation through Orca, renderer and device-loss
recovery, and ten complete create/destroy cycles. The lifetime run left no CEF
process family alive and showed no monotonic RSS growth above five percent.
The dependency audit resolves one WGPU 29.0.4 family, no Git dependency family,
the selected licenses and the owned unsafe boundaries. CEF's authoritative
license and Chromium credits are bundled with its runtime evidence.

V1 now advances in this order: Customization C0–C2 and its Gallery;
C3 recursive Sand/Castle composition workbench; C4 official-Sand migration; C5
completion gate; Box navigation, anchors and layers; current Protein result
templates and visible retirement/binding repair; force/sort/armed-mutation
Areas and Actions; Box durability; installed HTML/Website hardening and runtime
resource health; then the public read-only Live Facade. The detailed checklists below and in
[Customization](Customization.md) and [Sands](Sands.md) own those clusters.

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
