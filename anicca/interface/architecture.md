# Runtime architecture

Purpose: Define v1/v2 horizons, Plan A ownership, retained alternatives, and the engine boundary.

Owner source: [Interface in Lince](../Lince.lingua).

Status: Plan A ownership accepted through historical joined evidence. The next delivery makes its common native host CEF-free by default; the optional browser adapter returns only at the end of v1.

Read when: changing window, renderer, engine, physics, HTML-compositor, or platform ownership.

[Corpus map](README.md) · [Current context](current.md) · [Laboratory](laboratory.md)

---

## Preserved implementation specification

The current milestone is [Part A — Dogfeeding](plans/part-a.md) with [backend foundations](../backend-part-A.md): new native Interface Sands entering a private Organ live on a headless Linux server, locally or on a VPS. Role-associated Protein Record selection and property-write permissions govern knowledge/work Records; the company is the first acceptance case, not a second project/team or permission model. Manual configuration is sufficient. This does not require legacy desktop Sands, lince-desktop, a browser client, database replication onto employee devices or shared live Box layout. The [build rule](build.md) keeps CEF optional and late; reuse the existing common native host, not a second runtime. Later Box/time, additional native roots and CEF retain separate delivery.

## Base

### Runtime and rendering

#### Product horizons: v1 productivity and v2 world

The product has two explicit interface horizons. They are scopes, not two
unrelated applications and not permission to throw the first one away.

**Lince v1.0.0** is the productivity Box already being planned: composable
Sands and compound Sands/Castles, the current Protein capabilities, visual
field wiring, Areas of influence, bounded topology editing, surface-bound
top/perspective views and an explicit free-space mode, direct manipulation,
Why-is-it-here, Customization, installed external HTML, Websites and Facade. Its default
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
[Long-horizon world direction](v2-world.md#long-horizon-world-direction).

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

The ownership proof and initial Customization/composition kernels have landed. The development course now follows the [master waterfall](plans/interface.md#v1-master-waterfall): CEF-free production bootstrap and native C4/C5; stationary Box, durable composition and native time; the other planned native v1 work; then optional CEF and its dependent features. V2 remains research, and the rejected GPUI path remains reference evidence. Preserving an HTML contract does not require running its adapter during native work.

Research proofs live behind development tooling and leave no dormant public
schema field, compatibility branch or half-supported button in v1. What v1
learns is written into the shared contracts and benchmarks rather than hidden
inside a disposable demo.

The runtime decision now has an ordered Plan A and Plan B. The normalized Sand
definition, Protein and Action boundaries, typed ports, capability model,
Customization cascade, Box document, and recursive composition semantics are
shared by both plans. A renderer may change without changing what a Sand means.
The Web remains first-class as DATA, COMPOSITION and PROJECTION; Plan A is a
native desktop/runtime decision, not permission to make Web data, composition,
or external HTML second-class.

#### Reaching this Cell's own interface through a browser: NOT PLANNED

Decided 2026-09-01, and indefinite rather than deferred to a date. **A person
opening a web browser and pointing it at a Lince Cell to use that Cell's
interface is not planned work, and no task may depend on it, until the owner
says otherwise.** It is not cancelled and not deleted: the model below stays
written down, stays consistent with the shared contracts, and can be picked up
unchanged on the day it is wanted.

What this decision does NOT touch, because none of it is a browser reaching a
Cell's interface:

- **HTML as a Sand source.** Local HTML files, Sand packages and `.lince`
  packages remain first-class, and raw HTML remains supported.
- **Website Sands and CEF.** Embedding a remote site as a deliberately
  untrusted surface is unaffected, as is the joined CEF authority and
  composition that carries it.
- **The public Facade.** A Facade is a published, read-only PROJECTION that a
  stranger looks at. It is not someone using this Cell, and it stays planned.
- **The exported archive.** A workspace exported as one self-contained file is
  opened in a browser by design and is untouched.
- **Maud/HTML authoring and the shared normalized definition.** Plan B's
  authoring constraints continue to shape what a Sand IS.

What it does gate, until the owner lifts it: browser-based login and session to
a Cell you are using as a client; the ordinary HTTPS deployment whose purpose
is that login; and any surface, protocol or capability whose only justification
is a browser acting as a Lince client.

**The model is kept, not erased.** Plan B below, `html-and-websites.md` and
`facade.md` continue to describe how a browser client would work if it returned.
Nothing here removes a contract; it removes a commitment.

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
- one surface mode in which Sands remain in contact with filtered effective
  terrain, one free-space mode in which 3D Area volumes cluster floating Sands,
  separate Top/Perspective camera controls, and a visible, previewable,
  undoable projection between the modes;
- lightweight native GPU Sands for large populations, retained rich native
  controls and editors, installed CEF HTML Sands with declared Protein/event/
  Action capabilities, and Website Sands with ordinary web networking and
  storage but no Lince authority;
- a human-readable runtime-health and resource surface that identifies the
  selected graphics backend, software rendering, heavy-Sand admission cost,
  denied starts, browser/GPU failures, recovery progress and the reason a Sand
  is unavailable;
- a durable Box snapshot/journal whose coalesced spatial checkpoints restore
  Sands after Area/topology motion, plus live-only host-authoritative workspace
  collaboration in the same Protein Synchronization area as Record sync and
  File projection; and
- a read-only Live Facade that consumes the same public definitions and
  Protein projection, permits only local visitor interaction state and never
  exposes Action or composition authority.

The base pattern is the recursive dot-to-plus-to-connected-mesh grid already
described below, with configurable density and optional image or SVG
wallpaper. Changing which Record fields are shown is a Protein/template choice,
not a separate semantic-zoom system. The default has no compulsory physics,
game, globe or 3D camera; motion and visual richness are opt-in while the
ordinary desk remains fast and calm.

The 2026-09-06 owner revision puts stable placement, durable individual edits
and native Calendar/Clock before automatic movement. Preserve existing motion
code without enabling it in the first Box. The [master plan](plans/interface.md#v1-master-waterfall)
owns delivery order; the [time surfaces](time.md) use a specialized native leaf,
not time-driven Areas or a second recurrence engine.

The later v1 motion stage includes only the Box topology specified in
[Topology editing and effective terrain](box.md#topology-editing-and-effective-terrain):
compact brush stamps, filtered scalar potentials, Sand-attached effects, and
consistent Top/Perspective explanation of the surface simulation. It also
includes one workspace-level free-space simulation with a nonphysical collapse
plane and an explicit projection back to surface mode. The two modes reuse the
same Sand identities, Protein bindings and Areas but never run as contradictory
live positions.

The v1 exclusion line remains concrete: no separate frontend recurrence
scheduler, offline multi-writer workspace replicas, automatic collaborative-host failover,
arbitrary Protein language beyond the current supported operations, free-form
force expressions, multiple independently physical Sand planes, planetary
terrain/world, general scene authoring, reality reconstruction, avatar/game
product, or collaborative world session is required for v1.0.0. The included
workspace session is bounded live composition around the v1 Box, not the v2
shared-world product. A future capability may be represented by a prototype
fixture, stable id, port or adapter boundary only when that seam is also needed
by the v1 Box; it does not acquire a dormant schema field or public button.

The native architecture is accepted on the owner's NixOS/Wayland machine.
Linux is Wayland-only: Winit is built without its X11 backend, the event loop is
forced to Wayland, and Linux has no Tauri, WebKitGTK, XWayland or automatic fallback desktop. When the optional CEF adapter is enabled, it is forced through Ozone Wayland; its prebuilt shared object declares some X11-family system libraries as upstream binary dependencies. Those libraries belong to the optional package, not a Lince X11 code path or the native default. A machine without a usable Wayland compositor reaches an honest launch
failure instead of negotiating down. V1 names any further supported operating
systems and graphics backends only after separate native build, launch, input, recovery and accessibility evidence, with additional CEF evidence before that optional adapter is offered there. An untested Metal or Direct3D path is not
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
shaders, and future immersive visualisations. When enabled, CEF supplies real Chromium HTML
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
- the optional CEF adapter remains in its required browser processes and exports accelerated
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
native controls and editor work, and optional CEF supplies browser surfaces. This
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

Feature unavailability is not camera culling. A disabled CEF projection is never admitted or executed; the following liveness rule applies to admitted, enabled runtimes.

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

#### Engine boundary: native UI, Bevy, and completed GPUI/Pulsar research

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

[Pulsar](https://pulsarnative.com/) was the closest architectural research
reference: GPUI editor surfaces, a separately scheduled game renderer, an ECS,
fixed-rate physics, and a final compositor. The completed source-audited study
is retained in [links.md](links.md). It did not select Pulsar, Helio, SceneDB,
GPUI, WGPUI or their formats, plugins and editor lifecycle as production
dependencies. Their value is bounded repertoire and negative evidence, not a
second engine plan.

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

No v1 stage waits for Pulsar or Helio adoption. An individual bounded technique
may be reimplemented or reused only after the existing Bevy, Avian, focused UI
crate or direct WGPU path exposes a representative measured gap. Marketing
claims such as an O(1) CPU hot path are not performance evidence because the
corresponding culling and scene work still happens on the GPU.

The completed [SceneDB 2.0 and EngineFS review](research/scenedb.md) narrows
that decision. SceneDB is useful repertoire for generation-checked dense
handles, structure-of-arrays pages, explicit relocation boundaries, dirty
ranges and spatial residency. It is not the durable Box database: the formal
specification omits crash persistence, current spatial snapshot restore
allocates new handles, and collaboration authority/transport remain outside
the crate. Lince retains stable semantic ids, its own snapshot/journal and one
authoritative live simulation, then considers SceneDB-like hot storage only
after the real Box workload exposes a measured bottleneck.

#### Completed Pulsar/Helio study and carry-forward boundary

The 30 reviews do not create 30 implementation commitments. They leave seven
ideas in active interface development:

1. Stable Lince identities remain above disposable Bevy entities, CEF browser
   ids, dense runtime slots and GPU handles.
2. One Lince frame coordinator owns ordered boundaries between input, fixed
   simulation, semantic revisions, retained UI, world extraction, browser
   paint, GPU work and presentation.
3. Adapters exchange bounded revisioned snapshots, typed events and dirty
   changes instead of sharing arbitrary mutable state or rebuilding everything
   unconditionally.
4. Visibility removes presentation work only. Off-camera Protein, Behavior,
   media, browser execution, Areas and physics retain the same semantics.
5. Runtime health attributes a displayed result to the responsible input,
   semantic revision, simulation work, browser copy and render work in language
   a person can act on.
6. Hierarchical coordinate frames are authoritative Box/spatial data shared by
   rendering, physics, picking, accessibility, CEF input mapping, persistence
   and collaboration; renderer-only sublevels are insufficient.
7. Dense GPU storage, compaction, indirect work, dirty ranges and generated
   detail are performance options for disposable projections. They enter only
   after the accepted Box workload identifies the bottleneck and never move
   semantic authority onto the GPU.

Stable identity, authoritative ownership, ordered frame handoff, off-camera
semantics, causal status and shared coordinate meaning are correctness or
explanation constraints where their owning feature appears. Revision matching
is also correctness; dirty-range coalescing and the seventh idea are measured
performance work. A simple typed frame schedule is sufficient until resource
dependencies require a compiled graph; a CPU solver is sufficient until
Area/topology measurements justify a different kernel. The research therefore
strengthens boundaries without prepaying for Helio's renderer, SceneDB's hot
store, GPUI's application model, Fusor, Corona, probe lighting, foliage,
portals, XR or a Behavior compiler.

Implementation sourcing remains ordered:

1. Keep Protein, Sands, Actions, Box transactions, permissions, durable ids and
   coordinate meaning in Lince.
2. Use selected Bevy and Avian public modules for ordinary world rendering,
   scheduling, collision and spatial queries where they satisfy the owned
   adapter.
3. Use focused crates for text, accessibility, layout or rendering primitives
   when they fit the Lince-owned frame and Sand model more cleanly than a full
   UI framework.
4. Use direct WGPU for final composition, CEF interop and specialized measured
   passes that belong to Lince's host.
5. Reimplement, fork or vendor a bounded studied technique only when the prior
   routes fail a named correctness, capability, quality or performance gate;
   retain license, provenance, behavior tests and update ownership.

This boundary means inspiration never silently becomes GPUI ownership or a
Pulsar dependency. The active waterfall maps these ideas to existing stages;
the detailed article findings remain reference material and not active tasks.

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
| Box snapshots, operation journal, canonical revisions and spatial recovery checkpoints | Lince Box store | SceneDB handles, Bevy entities, renderer buffers or a remote filesystem provider |
| Current in-session Sand transforms, velocities, contacts and Area/topology physics | Lince spatial runtime behind its adapter | The renderer or an independently simulating collaboration guest |
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
Lince may compare later work with its shared-device composition, multiple
viewports, frame pacing, recovery, CEF surfaces and GPU measurements, but a
comparison does not reopen the host decision. Lince would skew GPUI badly by
asking it to become a globe/game renderer and would skew Pulsar badly by
putting geospatial privacy, scenario history, Protein or Sand persistence
inside its game schema. GPUI remains a measured application/editor reference,
Bevy is used substantially as intended as a game/world runtime, and the
unusual work stays in Lince adapters and domain kernels.

#### Plan B: Maud/HTML-first hybrid

**Gated by the decision above.** Plan B's Facade and portable-authoring halves
stay planned; its browser-as-client half is modelled here and not planned. The
description is kept in full so it can be resumed unchanged.

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
