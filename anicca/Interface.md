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
worlds, games, CAD-like construction, reality captures, Gaussian splats,
avatars, time-aware scenarios and collaborative sessions. It addresses how
Lince may model Needs, Contributions and the organisation of reality itself.
The detailed target is retained in
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
- the native path exercises the intended single-device compositor, GPUI,
  world-runtime and CEF boundaries even while its world is only a simple desk;
  and
- v1 placements and renderer handles never become Ledger identity or engine
  save data, so a Sand can later appear on Earth or in an authored world without
  being redefined.

V1 does not need globe streaming, CAD editing, Gaussian capture or a universal
world format to satisfy these invariants. It needs a small permanent kernel and
an honest capability boundary, followed by the simple human-usable Box.

The development course is therefore:

1. prove the permanent native ownership seams with one Lince-owned window,
   device and compositor, selected Bevy modules, GPUI and CEF;
2. complete Customization, the Lynx visual-character gate and the
   renderer-independent Sand/Castle composition workbench;
3. ship the human-usable v1 Box, current Protein wiring, Areas and external
   HTML on those foundations; and
4. keep v2 as structured research with explicit coordinate, globe, artifact,
   CAD, capture and scenario proofs, promoting a result into product planning
   only after its semantics and human surface are understood.

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

#### Plan A: GPU-first native prototype

Plan A is a prototype-gated native runtime. A Lince-owned real-time compositor
uses Rust and `wgpu` over Vulkan, Metal, or Direct3D. GPUI supplies native
application UI, text, editor surfaces, accessibility, and reusable first-party
controls. A world renderer supplies the retained 2D/3D scene, instanced Box
material, maps, games, terrain, specialised shaders, and future immersive
visualisations. CEF supplies real Chromium HTML as accelerated offscreen
textures. All three are renderer adapters behind the same Sand graph rather
than three application models.

The native path has one ownership constitution:

- the Lince shell owns the `winit` event loop, window lifecycle, `wgpu`
  instance/adapter/device/queue, final compositor and input router;
- the Bevy adapter runs without owning another window loop and receives the
  shared render resources through its supported manual-render initialization;
- the pinned GPUI adapter receives normalized input and renders UI display work
  into a surface or texture the final compositor owns;
- CEF remains in its required browser processes and exports accelerated
  offscreen surfaces to that same device topology; and
- the Lince frame coordinator defines when input, fixed simulation, semantic
  diffs, UI layout, world extraction, browser paint and composition occur.

GPUI entities, Bevy ECS entities and CEF browser ids are local implementation
state. They do not point directly at one another or share arbitrary mutable
objects. Stable Lince ids and bounded snapshots, diffs and typed events cross
their adapters at declared frame boundaries. This permits several excellent
subsystems without creating several competing applications.

Runtime selection prioritizes capability, visual and interaction quality,
correctness, security, performance and architectural freedom over implementation
size or short-term convenience. A large refactor, pinned fork or substantial
native subsystem is acceptable when it protects those properties. Cost alone
does not select Plan B. An integration still needs a named owner, tests and an
upgrade path: accepting work is different from accepting unknowable behavior
or permanent accidental coupling.

The earlier GPUI-owned versus world-owned compositor question is resolved for
the intended native architecture. The Lince shell owns the final frame. Bevy
supplies world passes using the shared device, GPUI supplies UI surfaces or
display work, and CEF supplies browser surfaces. This follows the lesson from
Pulsar's failed direct GPUI/game-loop integration without making Bevy itself
the outer application owner.

A GPUI-owned window that embeds one external world texture remains a useful
diagnostic and may serve a future application-only window, but it is not the
main Box architecture. The v1 prototype validates the Lince-owned path with
frame pacing, input, IME, accessibility, texture sharing, resize, device-loss,
and arbitrary Sand-transform evidence. GPUI may be forked and pinned when the
required offscreen or input surface is not upstream, but the fork is then a
deliberate Lince dependency with rebase tests rather than an unrecorded patch.

The external-compositor work in the
[referenced GPUI fork](https://github.com/zed-industries/zed/compare/main...MSIsunny:zed:feat/external-compositor)
is not primarily a macOS/Windows experiment. Its original path created the
registry for Wayland and X11 and demonstrated an external `wgpu` texture on
Linux/RADV without CPU readback; later commits added the Metal and DirectX
bridges. It is credible prototype material, not yet a stable upstream API or
proof that transformed CEF, multiple surfaces, accessibility, and long-running
device recovery work for Lince.

CEF remains native HTML rather than an HTML-to-GPUI translation. Blink lays out
the page, V8 runs its JavaScript, and Chromium owns Web APIs, media, storage,
networking, focus, and document semantics. Lince imports the accelerated CEF
surface into `wgpu`, composites it as a Sand, transforms pointer coordinates
back into its browser surface, and forwards keyboard, IME, focus, clipboard,
drag, popup, and accessibility information. Where a shared handle cannot be
held safely after the CEF callback, Lince makes a GPU-to-GPU copy into an owned
texture; it never makes a per-frame CPU screenshot the accepted path.

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
instanced into retained buffers. GPUI surfaces are used where application-grade
semantics justify them. CEF surfaces are heavyweight and may be numerous only
to the degree measured resources allow; being off-camera removes composition
cost but deliberately does not remove their execution cost.

Physics and rendering remain separate. Native CPU ECS systems with a spatial
broad phase, parallel work and deterministic fixed steps are the first physics
path. GPU compute is used for measured large regular kernels, culling,
compaction, particles, height fields, splat sorting, or other work that can stay
on the GPU. It is not assumed to improve branch-heavy collision resolution when
upload, synchronization, or readback costs dominate.

#### Engine boundary: GPUI, Bevy, and Pulsar

GPUI is the preferred native application-UI layer, not the persistent Sand
schema and not automatically the owner of the final frame. The world engine
sits behind a narrow Lince-owned contract for scene entities, cameras,
viewports, textures, picking, input, frame timing, device recovery, and typed
Sand events. This boundary is further-looking than choosing one engine for the
whole product: Lince can improve or replace the world renderer without rewriting
Protein bindings, Castles, external HTML, or Box documents.

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

[Bevy 0.19](https://bevy.org/news/bevy-0-19/) is the default world-runtime
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
| Inspectors, text-heavy editors, menus and accessible application chrome | GPUI | The 3D world renderer |
| Genuine external HTML and Websites | CEF accelerated offscreen surfaces | HTML-to-native translation |
| Globe tiles, map data, CAD solids, splats and later simulation kernels | Specialised capability adapters | One universal engine abstraction |

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
adapters/bevy        ECS/world projection · ordinary 2D/3D · physics
adapters/gpui        native editor and sharp application/Sand surfaces
adapters/cef         installed HTML and Website surfaces
adapters/geospatial  globe/tile selection · terrain · maps
adapters/geometry    CAD · meshes · voxels/SDF · collision derivation
adapters/capture     images · video · splats · reconstruction
```

These are ownership directions, not a requirement to create empty crates in
advance. A boundary earns a crate when its contract and independent tests are
real.

Pulsar can mature alongside Lince without becoming Lince's constitution.
Useful upstream collaboration includes Linux shared-device composition,
GPUI-to-texture rendering, multiple viewports, frame pacing, device-loss
recovery, CEF texture surfaces, and Helio measurements. Lince would skew GPUI
badly by asking it to become a globe/game renderer and would skew Pulsar badly
by putting geospatial privacy, scenario history, Protein, or Sand persistence
inside its game schema. It uses GPUI exactly as an application/editor UI and
Bevy substantially as intended as a game/world runtime; the unusual work stays
in Lince plugins, adapters, and domain kernels.

#### Plan B: Maud/HTML-first hybrid

The existing Maud/HTML-first design is retained in full as Plan B, not erased.
Rust/Maud emits ordinary accessible HTML fragments paired with recursive Sand
nodes; native ES modules provide browser Behavior; one shared Rust/`wgpu`
WebAssembly surface supplies spatial rendering; and a Worker supplies batched
simulation. It remains the desktop fallback if Plan A fails its prototype and
the browser/Facade implementation path even when Plan A succeeds.

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

### Long-horizon world direction

These ideas motivate Plan A's capability ceiling but are not part of the
current Box implementation plan. They remain one connected future direction
rather than being lost as unrelated feature phrases. The runtime boundary is
designed for them now so that the current Box does not become a disposable
shortcut, but their product and data semantics remain future work until their
own implementation clusters are written.

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
shaders and sprites over a real neighbourhood. Reality captures, ordinary
meshes, CAD objects, map data, Gaussian splats and fantasy geometry may
coexist, alternate, mask or blend without destroying one another. Private
worlds and sessions may publish selected submissions into a shared world while
retaining their provenance and withdrawal policy.

The long-term product is therefore not one game, one GIS package or one CAD
program. It is a semantic world kernel with multiple specialised projections.
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
  Bevy world    GPUI tools    CEF HTML      specialised
  projection    and editors   surfaces      GIS/CAD/splat
       └────────────┴──── Lince wgpu compositor ─┘
```

Lince owns durable meaning. The Bevy runtime holds a performance-oriented
projection of the active world; it may be rebuilt from Lince state and may
change across engine upgrades. GPUI owns application-grade editing surfaces.
CEF owns genuine HTML execution. Geospatial streaming, CAD geometry and
reality-capture algorithms remain capability adapters instead of being
reimplemented inside a generic Sand ABI. The `wgpu` compositor combines their
GPU results and routes input without CPU screenshots.

This is further than adopting Pulsar: the advancement is not a larger engine
but a world model that can employ several engines without making any one of
them its database. Pulsar may supply or receive compositor, renderer and editor
work. Bevy may remain in the final product for ordinary world simulation and
rendering. GPUI may remain in the final product for tools. Neither is expected
to express Lince's geospatial disclosure, reality/fantasy branches, Protein
bindings, or Transfer lifecycle.

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
- exact CAD geometry retains its parametric or boundary-representation source
  while exposing tessellated meshes for rendering and collision.

A brush may raise, flatten, paint or mask an authored layer and shaders may
blend its result with the globe. Base map or captured reality is not silently
rewritten. An edit is a versioned operation with bounds, units, author and
target layer, so it can be previewed, undone, replayed, merged and compared
over time. The same source can produce a globe surface, local high-resolution
terrain, an orthographic 2D floor and simplified physics proxies.

A game engine is well suited to meshes, materials, lights, animation, avatars,
physics, shaders, particles and picking. It is not by itself a CAD kernel.
Lince should use a specialised B-Rep/parametric kernel such as audited
[Open CASCADE Technology](https://dev.opencascade.org/doc/overview/html/index.html)
or a sufficiently mature Rust alternative such as
[Truck](https://github.com/ricosjp/truck) behind a geometry capability. The CAD
document and operations are authoritative; generated render, collision and
LOD meshes are replaceable caches consumed by Bevy.

#### Spatial artifacts and an editable common world

The globe-to-desk gradient should be one native world runtime and compositor,
not a GIS WebView that hands off to a separate game window. It may contain
several specialised render passes, but one camera/frame graph, spatial identity
model, selection system, input router and Sand event system make the transition
continuous. Orthographic desk, local perspective, globe and avatar views are
camera and layer configurations over that runtime. Pinned Sands use
viewport-space anchors while ordinary Sands and geometry use world or local
frame anchors.

Lince should not prematurely voxelize every source into one supposedly
universal format. Exact CAD solids, Blender meshes, terrain height fields,
voxels or signed-distance fields, Gaussian splats, images and semantic Sands
have different strengths. Converting all of them irreversibly into one
representation would lose exact dimensions, editable history, appearance or
capture information. Instead a **Spatial Artifact** keeps:

- one stable semantic identity and owning world/frame placement;
- the original or authoritative representation, units, axes, licenses,
  provenance and edit history;
- any number of content-addressed derived representations, such as glTF mesh,
  meshlets and LODs, voxel/SDF field, collision shape, navigation data,
  Gaussian chunks, thumbnail or low-cost proxy;
- the conversion recipe, tool/version, tolerance, error bounds and source hash
  for every derived representation; and
- Sand/Protein/Event/Action bindings that attach behavior to the semantic
  entity rather than to one mesh or voxel buffer.

A CAD or Blender import may therefore be rendered as a mesh, participate in
topology through a derived voxel/SDF or collision field, remain editable in
its source representation, and emit collisions or direct-manipulation events
to nearby Sands. Changing the source invalidates and rebuilds affected derived
artifacts. Editing a lossy derivative does not silently rewrite the exact
source; it creates an authored overlay, a new source revision or an explicit
conversion result according to the tool being used.

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
Thousands of ordinary Sands are instanced native scene data; a smaller measured
population uses GPUI or CEF where their semantics justify the cost. This is a
renderer choice for one Sand definition, not a reduction in its composability.
A Castle can contain native world objects, GPUI-backed editing surfaces and
CEF-backed HTML children while its persistent composition remains one Sand
graph.

#### From captured reality to organised work

A video, scan or Gaussian capture of land may seed a reality layer. A future
world model may propose geometry, a structure, detected resources, Needs,
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

**Gaussian streets and cities.** A point, street, building, or city could be
represented by a Gaussian-splat scene and combined with ordinary geometry,
terrain, map labels, and Lince overlays. Progressive streaming, spatial
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

#### Entry gate

- [ ] Run and decide the Plan A GPU-first prototype before the broad Interface
  refactor. Once its compositor, GPUI, engine, CEF, portability, and maintenance
  gates choose Plan A or Plan B, complete every Customization C0–C5 gate and
  the recursive Sand composition workbench before changing the canvas model.
  Box work begins with versioned definitions, typed ports, compound Sands,
  configuration scopes, native and external Behavior adapters, and the chosen
  renderer contract already usable without the board.

#### Runtime and rendering

- [ ] Freeze the two product horizons and the permanent v1-to-v2 seams. V1
  implements the productivity Box only, but its workspace is one named local
  frame, its placements use stable semantic ids and anchors, and its runtime
  adapters remain rebuildable projections. Do not introduce globe, CAD or
  capture product scope through this foundation task.
- [ ] Build the smallest Plan A compositor proof on Linux/NixOS: one native
  window and event loop owned by Lince, one `wgpu` device/queue topology, a
  Bevy render app initialized from those manual resources without its own
  window runner, a GPUI offscreen surface, a continuously animated world pass,
  and an accelerated CEF surface. The Lince compositor owns the final frame.
  A GPUI-owned external-texture path is diagnostic evidence only. Prove zero
  CPU readback in the accepted path, arbitrary move/scale/clip/order, pointer
  coordinate mapping, keyboard, focus, IME, clipboard, popup handling, accessibility,
  resize/device scale, texture lifetime, device loss, and clean teardown.
- [ ] In that proof, implement one installed HTML Sand with mapped Protein
  input, a typed `record-clicked` output, a Box event input, local browser
  state, and one granted Action request. Implement one arbitrary Website beside
  it and prove the Website has normal HTTPS/storage behavior but cannot use the
  installed-Sand bridge or any Lince authority.
- [ ] Implement the world-engine boundary first with the default Bevy 0.19
  adapter, then implement one narrow custom `wgpu` reference pass that proves
  Lince still owns the compositor and can escape an engine limitation. Do not
  attempt two complete engines. Use Pulsar, Helio, WGPUI, the GPUI
  external-compositor fork, current CEF/Bevy integration,
  Gaussian-splatting, and map-renderer work as audited research or prototype
  code, not as silent commitments. Record dependency size, release churn,
  required forks, unsafe/platform code, licensing, Web/Facade viability, input
  ownership, and device-sharing cost as well as frames per second.
- [ ] Build the representative Plan A benchmark on the owner's Vostro 3150,
  11th-gen Intel Core i7, Iris Xe integrated graphics, 16 GB RAM, and NixOS. At
  native resolution target 60 FPS pan/zoom with 200 visible interactive Sands,
  at least 1,000 continuously eligible Sand/group bodies, and thousands of
  lightweight nodes/connections. Record median, p95 and p99 frame/simulation
  time, event latency, CPU and GPU utilization, upload/copy bytes, memory/VRAM,
  body and rule counts, driver/backend, resolution, and power mode. Run local
  drag, dense collision, global force, continuously moving, game, and camera
  traversal scenarios.
- [ ] Prove camera invariance explicitly. Run the same physics, Areas, Protein,
  events, timers, game logic, media call, and installed CEF Sand first on-camera
  and then off-camera. Only draw/composition work may fall. No body or Behavior
  may sleep because of visibility, no page may unload or suspend, and event and
  Action results must be identical. State-based equilibrium sleep is permitted
  only when the same entity would sleep on-camera and every relevant change
  wakes it.
- [ ] Benchmark CEF-backed Sands at 0, 1, 4, 12, and the largest useful measured
  count, both visible and off-camera. Include an internally animating page,
  video playback, a video call, storage, network, bridge traffic, and memory.
  Test external begin-frame control separately from CEF's hidden-page behavior.
  Off-camera Lince composition is culled while browser execution stays live;
  report any irreducible internal browser paint/execution cost honestly rather
  than hiding, freezing, throttling, or unloading the page to improve the
  result.
- [ ] Prototype a small 3D/map capability slice without turning the future
  vision into current product scope: 2D native Sands and active Areas beside a
  3D scene, terrain or height field, one Record-derived visual, and one CEF Sand
  on a transformed surface. Include one high-precision parent frame and a
  camera-local render frame, then cross their origin boundary without a visual,
  picking, physics or identity jump. Import one small glTF/Blender artifact,
  derive a collision or voxel/SDF proxy, and route one typed interaction to a
  Sand without treating the derivative as source truth. A Gaussian-splat
  sample is included only if its plugin/toolchain can be isolated cleanly. The
  proof is for compositor, coordinate, representation and engine capability,
  not a World Sand implementation.
- [ ] Accept Plan A only if the complete native path passes correctness,
  accessibility, security, performance, packaging, CEF update, GPUI-fork
  ownership, visual-character, and browser/Facade gates on the target machine.
  Implementation size and a maintained fork are acceptable; cost alone does
  not fail Plan A. If the path still cannot meet the gates, retain all evidence,
  choose the Maud/HTML-first Plan B, and remove production-facing
  half-integrations rather than serving two desktop runtimes indefinitely.

#### Long-horizon world foundation — future, not in the current cluster

- [ ] Specify engine-neutral `World`, `Frame`, `Layer`, `Placement`,
  `Disclosure`, `ViewProxy`, `Scenario` and artifact-reference semantics before
  building a World Sand. Keep Bevy, GPUI, CEF, Cesium and CAD runtime ids out of
  their persisted forms.
- [ ] Prove globe-to-desk precision with authoritative geodetic/ECEF placement,
  nested local frames and camera-relative rendering. Test both real Earth and
  an authored world with a deliberate Earth anchor.
- [ ] Prove an out-of-core globe adapter with terrain, imagery, attribution,
  offline/cache ceilings and open 3D Tiles input. Compare a pinned Cesium
  Native adapter with an all-Rust path; do not make a hosted map service
  mandatory.
- [ ] Prove the layer/branch model by combining base terrain, a private exact
  placement, a city-level disclosure, a live local proxy, a Gaussian or mesh
  capture, a fantasy castle and a desired-future layer without changing their
  source identities.
- [ ] Specify `SpatialArtifact` and select CAD and reality-capture capability
  boundaries. Preserve exact CAD source/operations and capture provenance while
  treating tessellations, voxel/SDF fields, collision proxies, splat chunks
  and LODs as derived, content-addressed, rebuildable representations with
  named error bounds.
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
