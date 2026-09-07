# Runtime architecture

Purpose: Define the Bevy-native interface, its v1/v2 scope, and the few places where Lince needs an extension.

Owner source: [Interface in Lince](../Lince.lingua).

Status: the owner selected Bevy as the interface base on 2026-09-07. This replaces the prototype's custom Winit/WGPU host, subordinate Bevy adapter and separate retained UI. The migration is planned, not certified by the old reports. The no-embedded-browser decision remains; see [the build rule](build.md#no-embedded-browser).

Read when: changing window, renderer, engine, physics, HTML-compositor, or platform ownership.

[Corpus map](README.md) · [Current context](current.md) · [Laboratory](laboratory.md)

---

## Preserved implementation specification

The current milestone is [Part A — Dogfeeding](plans/part-a.md) with [backend foundations](../backend-part-A.md): new native Interface Sands entering a private Organ live on a headless Linux server, locally or on a VPS. Role-associated Protein Record selection and property-write permissions govern knowledge/work Records; the company is the first acceptance case, not a second project/team or permission model. Manual configuration is sufficient. This does not require legacy desktop Sands, lince-desktop, a browser client, database replication onto employee devices or shared live Box layout. The [build rule](build.md#no-embedded-browser) forbids an embedded browser; use one Bevy application for production and diagnostics. Existing backend and data boundaries may be translated; new interface code uses Bevy directly, and existing interface code may be rewritten rather than wrapped. Later Box/time, additional native roots and the browserless replacements retain separate delivery.

## Base

### Runtime and rendering

#### Product horizons: v1 productivity and v2 world

The product has two explicit interface horizons. They are scopes, not two
unrelated applications. Product meaning and user data survive implementation
changes; preserving the prototype's interface framework is not a requirement.

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

**Bevy is the native interface base**, not one candidate behind an engine
adapter. Plan A and Plan B name earlier research and prototype designs only.
External HTML export and Facade remain separate delivery requirements; they
do not require another native renderer or paired HTML output from every new
Sand. V2 is not a reason to build its world features early.

V1 is a vertical slice of v2 in four permanent respects:

- it preserves Sand identity, ports, Actions, Customization and explicit
  external-data authority, without freezing the prototype's Rust API or ABI;
- a v1 workspace is a local spatial frame rather than an unrelated coordinate
  system that v2 must later translate;
- one Bevy application owns UI, scene rendering and presentation, with scoped
  integration for genuinely external producers even while the world is a simple desk;
  and
- runtime entity and renderer handles never become Ledger identity. Authored
  placements may be saved Bevy component data with stable Sand references, so
  a Sand can later appear on Earth or in an authored world without being
  redefined.

V1 does not need planet streaming, general scene-authoring tools, reality
reconstruction or a universal world format to satisfy these invariants. It
needs durable product meaning and a simple human-usable Box, not an
engine-neutral kernel built in anticipation of replacing Bevy.

The prototype ownership proof and initial Customization/composition kernels have landed; they are historical evidence, not acceptance of the new Bevy host. The development course now follows the [master waterfall](plans/interface.md#v1-master-waterfall): browserless production bootstrap and native C4/C5; stationary Box, durable composition and native time; the other planned native v1 work; then browserless designs for the surfaces that used to need an embedded browser. V2 remains research, and the rejected GPUI path remains reference evidence. Preserving an HTML contract does not require running its adapter during native work.

Research proofs live behind development tooling and leave no dormant public
schema field, compatibility branch or half-supported button in v1. What v1
learns is written into the shared contracts and benchmarks rather than hidden
inside a disposable demo.

New interface work uses Bevy components, resources, relationships, scenes,
systems and observers directly. Sand identities, Protein/Action authority,
customization scopes and recursive composition are product requirements, not
a reason to route native calls through a portable rendering ABI. Existing
backend, saved-data and external publication boundaries may have translators.
Web data and publication remain wanted without dictating native implementation.

#### Reaching this Cell's own interface through a browser: NOT PLANNED

Decided 2026-09-01, and indefinite rather than deferred to a date. **A person
opening a web browser and pointing it at a Lince Cell to use that Cell's
interface is not planned work, and no task may depend on it, until the owner
says otherwise.** The old model below is retained as research. Reopening it
would require review against the Bevy-native design and the later
no-embedded-browser decision, not an unchanged implementation.

What this decision does NOT touch, because none of it is a browser reaching a
Cell's interface:

- **HTML package preservation.** Local HTML and Sand package metadata can be
  inspected and preserved without execution. Running them inside Lince is
  unavailable without a browserless design; ordinary native Sands need no HTML.
- **Website Sands.** Showing a remote site as a deliberately untrusted surface
  remains wanted, but the embedded browser that carried it is gone. The
  authority and composition rules survive as requirements on whatever renders a
  remote site without embedding a browser in Lince — including the option that
  Lince simply opens the system browser.
- **The public Facade.** A Facade is a published, read-only PROJECTION that a
  stranger looks at. It is not someone using this Cell, and it stays planned.
- **The exported archive.** A workspace exported as one self-contained file is
  opened in a browser by design and is untouched.
- **Existing HTML authoring and export data.** Translate these at their real
  import/export boundary; they do not constrain new Bevy Sand constructors.

What it does gate, until the owner lifts it: browser-based login and session to
a Cell you are using as a client; the ordinary HTTPS deployment whose purpose
is that login; and any surface, protocol or capability whose only justification
is a browser acting as a Lince client.

The earlier browser-client model remains historical context in
`html-and-websites.md`. Its resumption would need a fresh design; new native
Sands do not maintain an unused DOM projection in preparation for it.

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
  controls and editors, installed HTML Sands with declared Protein/event/
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

The previous native prototype was accepted on the owner's NixOS/Wayland
machine; the Bevy replacement must earn fresh acceptance on that target.
Linux is Wayland-only: Bevy's Winit integration is configured without its X11 backend, the event loop is
forced to Wayland, and Linux has no Tauri, WebKitGTK, XWayland or automatic fallback desktop. The removed browser adapter used to force Ozone Wayland while its prebuilt shared object still declared X11-family system libraries as upstream binary dependencies — one of the concrete costs that decided against it. A machine without a usable Wayland compositor reaches an honest launch
failure instead of negotiating down. V1 names any further supported operating
systems and graphics backends only after separate native build, launch, input, recovery and accessibility evidence. An untested Metal or Direct3D path is not
called supported merely because `wgpu` has that backend.

#### Bevy-native interface

Bevy owns the application, window lifecycle and event loop, input, ECS
schedules, assets, UI, text, scene rendering, GPU resources and final
presentation. Production and the laboratory use the same Bevy plugins.
There is no Lince-owned Winit/WGPU shell around a subordinate Bevy application,
no separate Glyphon UI renderer, and no generic engine or renderer adapter
for new work.

Build on a 3D-capable Bevy scene and transform model from the foundation.
Part A v1.0.0 remains a flat, stationary software-house workspace, using
planar placement and an orthographic view rather than a separate 2D-only
engine. This does not bring terrain, orbit tools or moving bodies into Part A.
Ordinary Bevy UI and overlay controls remain appropriate; not every label
needs a 3D mesh or physics body.

Use Bevy's first-party features with the selected scoped integrations:

| Work | Base |
| --- | --- |
| Layout and controls | Bevy UI, core widgets and suitable Feathers pieces; Flair for stylesheet authoring over Bevy components |
| Native authoring | Bevy scenes and `bsn!`, ordinary components and systems; small Sand conveniences only where they remove repetition |
| Text, selection and IME | Bevy text and `EditableText`, extended for the shared Record editor |
| Click, hover, drag and focus | Bevy picking, input focus and observers |
| Box and Clock geometry | Bevy cameras, transforms, meshes and materials |
| Connections and outlines | Bevy curves and retained `GizmoAsset`/`Gizmo`; a custom Bevy mesh/material if required |
| Images and ordinary sound | Bevy assets and audio |
| Visual effects | Bevy materials, shaders and custom rendering systems |

Lince's black/white/purple design, sharp text, accessibility and restrained
motion remain requirements. Bevy's example styling is not a product design.
Bevy UI already uses Taffy and its text stack uses Parley; these internal
dependencies do not justify separate Lince integrations.

Select the Bevy features and plugins the product actually uses. Adopting Bevy
does not mean enabling every 3D, audio, asset-format or development feature.
Do not introduce Lyon, Vello or a second UI toolkit merely to duplicate Bevy.
Flair is the selected CSS-authoring integration, subject to the same correctness
and resource gates as other dependencies. It styles Bevy components; it does
not replace Bevy's layout/rendering or prescribe Sand constructors. See
[Customization](customization.md#flair-as-the-styling-integration).

#### New code and existing boundaries

New native Sands may use Bevy types directly, share components and resources,
and compose through ordinary Bevy relationships and scenes. Do not insert
snapshot handoffs, runtime traits, renderer-neutral view trees or projection
manifests between Lince code and Bevy solely to preserve hypothetical engine
replacement. Rewriting existing interface code is allowed when simpler than
carrying its prototype framework forward. No old-version compatibility work
is required.

The Sand model still needs stable definition, instance and child identities,
editable composition, ports, overrides and declared effects. Box edits the
same authored components and relationships used by code-built Sands, not a
second UI tree. Movement attachment and logical ownership remain separate;
Bevy parentage or pointer bubbling alone must not decide a Castle's exported
event scope or a released child's lifetime.

Persist selected authored data and stable references, not raw runtime entity
ids, GPU handles, subscriptions, closures or credentials. Bevy-authored
components and scene data are allowed in that representation. Saving and
loading require validation and entity-reference remapping; they do not
require a portable renderer schema.

Existing Protein, Actions, authentication, collaborative text, storage and
transport contracts stay owned by their backend modules. A narrow translator
at these real boundaries is appropriate. First-party Bevy code is trusted
application code, not a security sandbox: permissions are still enforced by
the backend. The current extension model is editable composition of registered components
and named effects plus trusted Rust plugins. Untrusted executable installation
and its sandbox are deferred until an explicit later decision. Preserving
package metadata does not execute it; no package receives unrestricted ECS
World or device access merely by being called a plugin.

HTML export and public Facade consume a deliberate public subset through
their own boundary. They need not mirror every native component or influence
new Sand constructors. Unsupported publication must explain the missing
capability rather than silently dropping content or authority checks.

#### Extensions and exceptions

The normal extension is a Lince Bevy plugin, not another runtime. It supplies
Sand/Protein bindings, scoped effects, Box operations, Areas, styling, editor
features and resource policy using Bevy's existing systems.

An internal or external crate, bounded fork or pure WGPU pass may replace or
extend a specific Bevy subsystem when Lince's required correctness, usability,
accessibility, quality or measured resource use needs it. Record the reason,
scope, license/credits, maintenance owner and acceptance test. Prefer the
smallest change that solves the actual need; do not add a generic replacement
framework around it.

Pure WGPU work belongs inside Bevy's rendering lifecycle by default, sharing
its device, queue, resources and presentation. Do not start an independent
window loop, GPU device or compositor for each Sand. An external producer with
an unavoidable separate device/process needs an explicit synchronization,
lifetime, color/alpha, recovery and resource-budget contract; it does not
become the interface owner.

Accepted supporting exceptions:

- Use Flair for CSS authoring and style resolution on Bevy components. Keep
  the Lynx scope rules, editor overrides and asset restrictions explicit;
  measure static and animated workloads before certifying its integration.

- Use the AccessKit version matching Bevy where custom Sand roles and actions
  need its types; keep Bevy's platform accessibility integration and one
  coordinated accessibility tree.
- Use narrow native platform libraries for required dialogs, portals,
  clipboard gaps or system integration; they do not own another application.
- Put terminal emulation, PDF/EPUB interpretation, video decoding and other
  specialized content engines at the end of v1. Choose a browserless library
  per real Sand need; Bevy still owns the surrounding UI and composition.
  This is not a prerequisite for Dogfeeding, stable Box or native time.
- Use Avian 3D for the later collision/settling stage below. The owner accepted
  this choice on 2026-09-07; it is not a dependency of every widget or a reason
  to run physics in the stationary Part A workspace.

The embedded-browser rejection remains absolute. External content that has no
browserless design stays unavailable, or is explicitly opened in the system
browser. No new exception here restores CEF or an embedded WebView.

#### Idle work and large Boxes

The owner's current machine is the minimum acceptance target for now, as
selected on 2026-09-07. [Build](build.md#minimum-machine-and-resource-baseline)
records its observed hardware and how reports must identify the actual runtime
and graphics device. This is a target, not a new benchmark pass or permission
to consume all available memory.

The default stationary workspace must not run a continuous game/render loop.
Configure Bevy's reactive event loop and wake it for actual input, a completed
background task, a Protein change, a media frame or the next due timer.
Animations request frames only while active. A cursor blink has a deadline;
it does not require all Sands to update at display rate.

Measure two different cases: a completely idle application, and one active
Sand among many unchanged Sands. Bevy change detection can avoid rebuilding
content without avoiding every entity scan; UI layout can traverse unchanged
nodes when the schedule runs. Retained geometry likewise does not promise
zero draw work. Add scoped dirty work, view reuse or selective custom passes
only where representative measurements show the need.

Keep fully idle, focused/unfocused, minimized, live-data, one-animation and
repeated open/close measurements. Record CPU and GPU work, submissions,
wakeups, input latency, RAM and VRAM separately. Sleeping is not deallocation.
Fonts, textures and view caches need explicit bounds. Reuse the historical
200-visible/1,000-active/10,000-resident stress shape where applicable, but
earn new Bevy-native results and do not make continuous 120 Hz simulation the
ordinary idle workload.

Visibility affects presentation only. Off-camera Protein, Behavior, events,
media, games, Areas and admitted physics keep their intended semantics.
Visual entities and caches may be virtualized independently of stable Sand
identity, ownership and active behavior. Never use camera culling as a hidden
pause, timer-throttling policy or way to discard a dirty editor.

A genuinely settled body may sleep, and unchanged dependencies need not be
reevaluated. Relevant Area, Protein, contact or input changes must wake the
same work on- and off-camera. Backend activity continues independently of
whether the interface needs to present another frame.

#### Physics: application rules and collision solving

The first stationary Box, property grouping, Calendar and Clock need no
general physics solver. Implement their placement and ordering as Bevy
systems. Preserve existing movement code as regression material without
running it continuously in the ordinary workspace.

The turning point is the later motion contract: many bodies must collide and
settle together, dragging switches bodies to kinematic motion, fast movement
must not pass through obstacles, and free-space mode adds 3D contact behavior.
That is contact solving, not just moving a transform toward a target.

Selected for that stage: Avian 3D for rigid bodies, contacts, damping,
sleep/wake and collision queries instead of a new general solver. Use the
same 3D simulation for surface-constrained and later free-space motion, one
active mode at a time. Flat mode constrains movement to its plane; terrain
mode needs Lince's effective-surface constraint, not a competing 2D solver.
The earlier optional Avian 2D preflight is historical code, not the chosen
product dimension. Preserve useful regression evidence without wrapping both
solvers into a new portability layer. Avian 0.7 supports the current Bevy 0.19
family; verify the selected versions again when implementing the motion stage.
[Avian compatibility](https://github.com/avianphysics/avian).

In 3D, a Sand's visible face is pinned to its authored direction by default.
A per-Sand facing-mode toggle can make that face follow the viewer instead.
Save the pinned orientation and chosen mode; do not save a camera-derived
rotation every frame. Facing changes presentation, not the collider, terrain
support, group ownership or Area forces. The detailed rule lives in
[Box](box.md#sand-facing-in-3d).

Keep Protein selection, sorting, mutation visits, immunity, filtered
potentials, terrain stamps and Why-is-it-here in Lince Bevy systems. Avian
does not know those rules. Per-group effective terrain is not one universal
heightfield collider; evaluate and explain that constraint in Lince, and test
how it interacts with contacts before claiming topology support.

One workspace has one active simulation authority and spatial mode. Do not
run separate 2D and 3D solvers that both write the same Sand. Fixed-step
ordering, bounded catch-up, deliberate sleep/wake and server-authoritative
collaboration remain explicit. Fixed steps alone do not promise bitwise
determinism across machines. Replays must state the tested platform,
configuration and tolerances.

Bevy's [physics ecosystem](https://bevy.org/assets/#physics) lists Avian and
Rapier; it does not make Avian the engine's compulsory or exclusive solver.
Its [Breakout example](https://bevy.org/examples/games/breakout/) demonstrates
small custom collision logic. [Avian's solver plugins](https://docs.rs/avian2d/latest/avian2d/dynamics/solver/struct.SolverPlugins.html)
covers the contact and sleeping machinery that the later Box would otherwise
need to build. These are the reasons for the recommendation, not a benchmark
claim that Avian is always faster.

#### Completed Pulsar/Helio study and carry-forward boundary

The dated studies in [links.md](links.md) remain historical research, not
instructions to retain the old Lince compositor or an engine-neutral layer.
Their useful lessons apply inside Bevy: stable user identities, ordered
schedules, targeted changes, presentation-only culling, understandable runtime
health, consistent coordinate frames and measured GPU optimization.

No GPUI, Pulsar, Helio or SceneDB adoption is implied. Study or reuse a bounded
technique only for a demonstrated Lince need, retaining licenses, provenance
and focused correctness/performance evidence. Prefer Bevy's public APIs;
a custom plugin, focused crate, fork or WGPU pass is an exception with a
specific owner, not an alternate interface architecture.

#### External publication and the former Plan B

Plan B is historical browser research, not a parallel native authoring or
runtime obligation. Existing Maud/HTML data and export code may have a
translation boundary. Public Facade remains read-only and static archives
remain externally viewable; their security and privacy rules remain in
[Facade](facade.md) and [Interoperability](interoperability.md).

Choose the external projection needed by that publication feature when it is
built. A Bevy web target or a limited HTML exporter can be evaluated there;
neither a general browser client nor automatic HTML parity for every Bevy
Sand is assumed. The native API is allowed to be fully Bevy-specific.
