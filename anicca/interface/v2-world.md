# V2 world direction

Purpose: Preserve the globe-to-desk, authored-world, terrain, scene, time, disclosure, and capture-to-work direction.

Owner source: [Interface in Lince](../Lince.lingua).

Status: Capability horizon and future research, not a v1 product checklist.

Read when: evaluating permanent seams or beginning an explicitly authorized v2 cluster.

[Corpus map](README.md) · [Current context](current.md)

---

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
