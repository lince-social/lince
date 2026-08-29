# Sand and Castle model

Purpose: Define recursive Sand identity, composition, state, ports, Behavior, renderer projections, and Box-built groups.

Owner source: no dedicated Sands Record currently exists;
[Interface](../Interface.lingua) governs shared interface decisions.

Preserved source metadata: `@sands`, order 1, `#chapter`, `#instinct`,
`#part-of @interface`, `#done`, uid
`r_14NKDPPS969TPSRGYJBJBJWEGR`.

Status: production schema, ABI, artifact validation, primitive Gallery and
recursive composition workbench landed; external authoring is next.

Read when: implementing reusable primitives, Castles, bindings, artifacts, or composition operations.

[Corpus map](README.md) · [Current context](current.md) · [Sand plan](plans/sands.md)

---

## Sand

### Landed version-1 contract

`crates/interface-prototype/src/sand.rs` is the executable Rust authority for
Sand schema and ABI version 1. It separates `SandDefinition` and
`DefinitionGraph`, serializable `SandInstance`, projection-specific
`ProjectionManifest`, and non-serializable `RuntimeSandInstance`. Persisted
data contains no DOM, CEF, Bevy, GPU or retained-tree handle. Exact revisions
are integers within the graph; the normalized graph and every packaged asset
also carry SHA-256 identity.

The contract includes typed inputs and outputs, stable local child uids,
transforms, configuration and style patches, connections and exported ports,
declarative or module Behavior, semantic element and accessibility roles,
capabilities, isolation and renderer requirements. Package validation checks
graph integrity, projection-node agreement, bounded local assets, hashes,
media types, a closed relative static-import graph, executable grants, and
vendored notices and credits. Installed
HTML structure declares its definition and every projected node and keeps CSS
and native JavaScript modules external. Website projections cannot receive
Lince authority.

The portable host boundary is a bounded, sequenced, versioned pair of
`SandInboundEnvelope` and `SandOutboundEnvelope`. Unknown versions, message
kinds and fields fail closed. JSON Schemas for package, persisted instance and
both envelopes, plus accepted and deliberately stale fixtures, are generated
under `target/interface-laboratory/sand-contract/` by the parity diagnostic.
That directory is evidence, not checked-in source of truth.

The 19-definition primitive package and its native state live in
`primitive_gallery.rs`. F5 switches native/HTML focus, Tab traverses, Enter or
Space activates and F9 cycles meaningful visual states. Installed CEF consumes
a Protein-shaped version-1 mount through a strict JavaScript decoder and emits
the granted `record-clicked` event. The live gate first proved that an unknown
field is refused; the Website companion retains network and its own storage
without bridge or style authority. The joined Wayland report passed. The C2
composition host consumes this contract rather than creating another model.

### Landed composition host

`crates/interface-prototype/src/composition.rs` is the executable authority for
composition artifact and catalog schema version 1. A catalog retains exact
definition revisions and one active revision per definition. A document owns
placements and typed Protein-read, Sand-event and Action-write bindings. A
runtime host recursively resolves the graph, merges definition style beneath
child or instance patches, validates configuration, selects a declared
projection adapter and allocates only disposable renderer and Behavior
handles. Stable semantic identities and scoped DOM identities are separate.

Shared definition publication is atomic. The changed definition and every
active exact ancestor receive new revisions together; an invalid candidate
does not enter the catalog. Saved-group and forked lineage are explicit. Save
and reopen serializes only semantic state. Lock changes editing affordance,
save creates a reusable definition and fork creates an independent definition;
all use the same recursive graph and exported-port rules.

The F10 workbench makes the model human-usable before Box. It places a Button
alone, nests the same Button inside `video-call`, nests that again inside
`video-call-room`, and mounts native and Installed HTML projections of the
room. Its ten keyboard operations exercise activation, lock, instance
override/reset, shared edit, invalid-edit refusal, save as definition,
save/reopen, fork and teardown/remount. The panel exposes the tree and separate
`READ`, `EVENT` and `WRITE` arrows. The Installed module scopes all cloned DOM
ids, repairs label and ARIA references, unregisters its event listener and
remounts without collisions.

Rust construction, recursive Maud output and workbench artifacts consume the
same 21-definition package. After the save and fork operations the catalog has
23 active definitions and five placements; the live joined report observed 17
mounted nodes, nine active Behavior handles and 43 retired handles. The
release parity and joined Wayland reports pass.

### Model and composition

A **Sand definition** is a reusable interface/Behavior description. A **Sand
instance** is one configured placement of that definition. Sand is recursive
at the model level; isolation is a runtime choice. Trusted first-party pieces
may share one composition root, while untrusted external code keeps a real
security boundary. A button can therefore be a Sand without paying for an
iframe per button.

The Sand store offers both UI and Behavior. Protein subscriptions are reads;
Actions are durable writes; host state stores presentation; lanes carry
ephemeral events and shared session state. A primitive button exposes a typed
press event. A named `Delete Record Button` may bundle that button with a
confirmation, permissions, and a typed delete Action. Workflow Sands such as
Kanban are borderless bundles of these pieces, not sealed windows: their
controls can be separated, moved elsewhere, resized, reconnected, and grouped
while continuing to interact.

**Castle** is an optional human word for a saved compound Sand: a group of
Sands prepackaged for reuse. It introduces no fourth object alongside Sand,
group, and definition. A Button Sand may be placed directly in Box or
referenced as a child of a Video Call Sand; the child is not copied or changed
into a special castle component. A local group can be locked for movement,
saved as a reusable compound definition, or forked without changing the
composition semantics. Protein result templates use the same recursive group
shape rather than a parallel template system.

Each child has a stable identity within its owning definition, local
coordinates, ordering, configuration overrides, and typed connections.
Connections name child ports rather than DOM selectors. A compound definition
may export selected child ports as its own interface; everything not exported
stays internal. This lets the same composition be embedded again without its
parent knowing its internal markup, and lets edit mode draw a complete route
through nested groups.

A Protein area's locked result template is one such compound group. Its child
inputs are wired visibly to fields of one Protein result, and Box repeats the
whole group once per row. Children without a field binding remain ordinary
presentation, controls, or Behavior inside that template.

Definitions are referenced rather than copied. Built-in and user-owned
definitions update their instances live, with instance changes represented as
override patches. Fork/detach explicitly creates an independent definition.
External executable Sands are content-hash pinned and never update silently.
An invalid or incompatible local definition update fails closed: existing
instances keep the last known-good revision and show the authoring error until
the definition is repaired. They never silently switch to broken content.

The stored contract has three distinct layers. The semantic graph owns stable
identity, recursive composition, ports, Behavior, configuration, capabilities
and state-plane meaning. Projection manifests describe compatible retained
native UI, world GPU, installed HTML, Website-wrapper or browser-DOM
presentations and their assets and renderer capabilities. A mounted runtime
instance selects one projection and owns disposable native UI nodes, world
handles, CEF browser ids, DOM roots and GPU resources. A package may carry all
three together, but a projection or runtime handle never becomes the Sand's
semantic identity.

#### One definition graph, several authoring paths

The GPU-first Plan A and Maud/HTML-first Plan B share one validated Sand
definition graph. Plan A pairs native Rust retained-UI or world-renderer
implementations with that graph and uses CEF for real external HTML. Plan B
pairs Rust/Maud fragments and native ES modules with the graph and uses one
shared Rust/`wgpu` WebAssembly spatial renderer. The ordered runtime decision
and HTML alternatives are recorded in
[Customization](research/runtime-alternatives.md#runtime-plans-and-plan-b-html-alternatives).
Maud does not become the stored Sand format and it does not run in the browser.
Native Rust, Rust/Maud, raw packaged HTML, and Box edit mode can all produce or
consume the same graph:

```text
Native Rust + retained UI/world renderer ─────────┐
Rust constructors + Maud ──> Sand artifact ───────┤
Raw HTML + declared metadata ─> Sand artifact ────┼─> semantic graph + projection set
Box edit operations ──────────────────────────────┘                    └─> composition host
                                                                           ├─> native UI adapter
                                                                           ├─> native world adapter
                                                                           ├─> installed CEF adapter
                                                                           ├─> Website CEF wrapper
                                                                           └─> Plan B DOM/wgpu adapters
```

#### Renderer roles across v1 and v2

V1 and v2 reuse the same Sand definitions but do not force every Sand through
one drawing implementation. The preferred native projection is:

- the Lince retained UI for sharp application chrome, inspectors, editors,
  focused rich Sands and viewport-pinned HUD surfaces;
- the shared Bevy/`wgpu` world for the desk, Areas, connections, large
  populations of lightweight Sands, ordinary 2D/3D objects and later globe or
  game content; and
- CEF for installed external HTML and zero-authority Websites.

The Lince retained component library owns the visual grammar; it does not adopt
Bevy's example UI or a generic game theme. Lightweight world Sands consume the
same semantic tokens in instanced rectangle, line, icon, image and shaped-text
primitives so density and hierarchy remain recognizably Lynx. A focused Sand
may expose a richer native editor without changing its definition or making
its idle representation a second Sand.

V1 places these projections in an orthographic local workspace. V2 may anchor
the same instance to Earth, an authored frame, an avatar, another artifact or
the viewport. The persistent definition declares semantic presentation and
required capabilities, not “is a native widget” or “is a Bevy entity.” Adapter
selection, effective device scale, visual LOD and cached runtime handles remain
runtime state. Visual LOD may simplify presentation but cannot remove declared
information, ports, Behavior or authority.

This split allows a clean low-cost desk and a dense world without making a
Chromium surface or separate GPU texture for every small button. It also keeps
external HTML genuine: CEF is an expensive, measured capability used where Web
semantics matter, not the universal native widget renderer.

Under Plan B and in browser/Facade adapters, the browser receives ordinary
HTML, CSS, and native JavaScript modules plus the shared `wgpu` WebAssembly
renderer when the world layer is present. Under Plan A, native built-ins use
the retained UI or the world renderer and external HTML runs unchanged in
Chromium/CEF. Third-party HTML never needs Rust, Maud, Wasm, a native UI
toolkit, or `wgpu`. Maud remains
valuable because a first-party author can build an HTML-backed button, panel,
dropdown, Kanban, or complete Video Call from Rust functions while the emitted
definition remains understandable and editable by Box. It is the Plan B
first-party structure and a Plan A HTML/Facade authoring path, not the primary
native renderer.

A Rust function returning only `maud::Markup` is not a composable Sand
constructor. Markup alone loses child identity, ports, Behavior, capabilities,
configuration, lineage, and the boundary that Box must reveal. The authoring
API therefore returns a paired value conceptually shaped like this:

```rust
struct AuthoredNode {
    node: SandNodeDefinition,
    markup: maud::Markup,
    assets: Vec<Asset>,
}

struct SandArtifact {
    definition: SandDefinition,
    fragments: Vec<RenderedFragment>,
    behavior_modules: Vec<BehaviorModule>,
    assets: Vec<Asset>,
}
```

These authoring-API names remain a C3 sketch, but the pairing may not. Primitive
constructors such as `button`, `panel`, `dropdown`, and `stack` produce both
their semantic node and their accessible Maud fragment. Compound constructors
compose those paired nodes, not bare HTML strings. The final artifact compiler
normalizes and validates the definition, renders its fragments, gathers native
ES modules and other assets, verifies declared licenses and credits, and
computes the revision/content hashes.

For example, first-party source should be able to read approximately like:

```rust
fn call_controls() -> SandArtifact {
    compound("lince.video-call.controls")
        .child(button("mute", "Mute"))
        .child(button("camera", "Camera"))
        .child(dropdown("device", "Microphone"))
        .behavior(module_behavior("media", "behavior/media-controls.js"))
        .connect(port("mute", "pressed"), behavior_port("media", "toggle_audio"))
        .connect(port("camera", "pressed"), behavior_port("media", "toggle_video"))
        .connect(port("device", "changed"), behavior_port("media", "select_device"))
        .export(port("media", "state"), "media_state")
        .build()
}
```

That syntax is an authoring convenience, not another runtime model. Its output
is the same logical definition that Box creates when a person places those
three Sands, connects their visible ports, groups them, and saves the group.
The examples below preserve the intended authoring experience and are
non-normative sketches, not serialized version-1 fixtures. Exact field
spelling, serialization and the current port vocabulary come from the
generated schemas and Rust authority above; C3 may add authoring conveniences
without creating a second runtime model.

```json
{
  "schemaVersion": 1,
  "uid": "lince.video-call.controls",
  "revision": "sha256:…",
  "capabilities": ["media.microphone", "media.camera"],
  "configurationSchema": {},
  "root": {
    "localUid": "controls",
    "renderer": { "uid": "lince.lynx.row", "revision": "sha256:…" },
    "layout": { "direction": "row", "gap": "space-1" },
    "children": [
      {
        "localUid": "mute",
        "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
        "overrides": { "label": "Mute" }
      },
      {
        "localUid": "camera",
        "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
        "overrides": { "label": "Camera" }
      },
      {
        "localUid": "device",
        "definition": { "uid": "lince.lynx.dropdown", "revision": "sha256:…" },
        "overrides": { "label": "Microphone" }
      }
    ]
  },
  "behaviors": [
    {
      "localUid": "media",
      "kind": "module",
      "module": { "asset": "behavior/media-controls.js", "hash": "sha256:…" },
      "ports": {
        "inputs": [
          { "name": "toggle_audio", "type": { "kind": "event", "payload": "none" } },
          { "name": "toggle_video", "type": { "kind": "event", "payload": "none" } },
          { "name": "select_device", "type": { "kind": "event", "payload": "device-ref" } }
        ],
        "outputs": [
          { "name": "state", "type": { "kind": "value", "schema": "lince.media-state/1" } }
        ]
      },
      "capabilities": ["media.microphone", "media.camera"]
    }
  ],
  "connections": [
    {
      "from": { "node": "mute", "port": "pressed" },
      "to": { "behavior": "media", "port": "toggle_audio" }
    },
    {
      "from": { "node": "camera", "port": "pressed" },
      "to": { "behavior": "media", "port": "toggle_video" }
    },
    {
      "from": { "node": "device", "port": "changed" },
      "to": { "behavior": "media", "port": "select_device" }
    }
  ],
  "exports": [
    {
      "name": "media_state",
      "from": { "behavior": "media", "port": "state" }
    }
  ],
  "assets": [
    {
      "path": "behavior/media-controls.js",
      "kind": "module",
      "hash": "sha256:…"
    }
  ]
}
```

The schema stores references and meaning, not a copied expansion of every
child's HTML. At render time the runtime resolves each exact definition
revision, instantiates its fragment in the appropriate trusted composition
root or isolated boundary, applies inherited configuration and instance
overrides, and mounts Behavior. Flattening into HTML may be a disposable render
optimization; it is never the persistent representation.

Paired constructors stamp fragment roots with their stable local node uid, and
the runtime builds an instance-scoped node map while mounting. Connections use
that map, never persisted CSS selectors. Repeated instances cannot ship fixed
global HTML `id` values: the runtime derives DOM ids from instance uid plus
local node uid and repairs associated `for`, `aria-controls`, `aria-labelledby`,
and similar references before interaction begins. The semantic local uid stays
stable even though its concrete DOM id differs in every instance.

This gives a Castle equivalent construction paths. A Maud-authored Castle
ships the graph above. A raw first-party or external package must declare that
graph beside its HTML rather than asking Box to infer it from tags. A
Box-authored Castle begins as a workspace-owned local definition with the same
child tree and connections. Locking changes only edit affordances. Saving
promotes that workspace-owned definition into the reusable definition catalog;
placing it creates an instance reference. There is no `castle` schema kind and
no Castle-specific runtime or rendering path.

The equivalence has an important limit: Box does not rewrite idiomatic Rust or
round-trip edits into Maud source. A code-owned first-party definition is
regenerated from Rust; Box may instantiate it, apply local overrides, or fork
it into a user-owned definition. After a fork, Box owns the graph and the Rust
definition continues on its own lineage. Conversely, a Box-authored Castle can
be exported as the normalized definition and used by Rust, but it does not
magically acquire handcrafted Maud source.

Decomposability is declared, not inferred from nested tags. When Maud calls a
paired Sand constructor, that child is visible in Box. When Maud emits an
ordinary private `div`, it remains an implementation detail of the nearest
declared node. A complex specialized renderer can therefore stay one leaf,
while its toolbar and surrounding controls are reusable Sands. Turning every
HTML element into a Sand would produce unusable authoring noise and is not the
goal.

#### Renderer and execution bindings

A Sand's meaning is independent of the technology that presents it. The
semantic graph declares typed ports, Behavior, capabilities, state ownership
and teardown. Its projection set declares presentation assets and compatible
adapter capabilities. The composition host then selects a projection and
runtime adapter:

- Plan A native application controls and rich editor surfaces use the Lince
  retained UI with Rust Behavior behind typed ports;
- Plan A Box material, lightweight native Sands, maps, games, terrain, graphs,
  splats, and specialised GPU leaves register entities or retained visual nodes
  in the shared native world renderer;
- installed external HTML runs as real Chromium content and communicates
  through a validated, size- and rate-bounded CEF process bridge;
- a Website's host-owned wrapper exposes only navigation/loading/focus/bounds
  facts while the remote CEF page receives no Lince bridge;
- Plan B ordinary first-party and raw HTML definitions render in a trusted DOM
  root and mount native ES-module Behavior, while spatial material registers in
  the shared Rust/`wgpu` WebAssembly renderer;
- an optional Wasm Behavior may implement the same logical lifecycle and ports
  through a generated component binding without receiving ambient DOM or host
  authority.

These are projections of one logical ABI, not one binary ABI or shared memory.
The portable contract includes versions; definition and instance identity;
mount, resize, camera-visibility hints, explicit user/system pause, and dispose;
typed input and output delivery; configuration and permitted state-plane
handles; bounds and device scale; capability handles; errors; and
host-validated Action requests. Camera visibility may suppress presentation
only and never pauses Behavior, physics, games, media, Protein, events, or CEF
execution. The authoritative Rust model generates the JavaScript and CEF/DOM
bridge validators, fixtures, and any future WIT projection. Raw DOM nodes,
functions, GPU handles, pointers, credentials, and the global Box store are
deliberately not portable values.

The shared world renderer is retained rather than rebuilt from native UI or the DOM.
A stable visual-node uid maps each GPU primitive to its Sand or private renderer
node; moving one instance updates only its transform and affected GPU buffer
range. Ordinary Sands do not allocate a GPU device, engine, browser process, or
animation loop apiece. A specialised GPU Sand uses the shared safe renderer
vocabulary where possible and receives a dedicated surface only when media,
isolation, or incompatible ownership genuinely requires one. GPU buffers,
compiled pipelines, collision caches, ECS caches, and Plan B Worker state are
disposable runtime resources and never enter the persisted Sand definition or
Box document.

Rendering does not grant Behavior authority. A GPU node emits a typed hit,
drag, selection, or value event into the same port graph as a DOM button. A DOM
or installed HTML Sand may answer that event, and only the host can convert a
declared route into a durable Action. Likewise, Protein values reach a GPU leaf
only after host validation and field binding; the renderer cannot query Lince
data merely because it draws the result.

Web Components are permitted inside trusted HTML renderer implementations, and
Maud may emit their custom elements, but Custom Elements or Shadow DOM do not
become the Sand schema or a security boundary. Their attributes and browser
events are adapted to typed Sand ports. An untrusted or cross-origin component
still uses the CEF boundary under Plan A or iframe/WebView boundary under Plan
B.

WebAssembly is an optional execution format, not a requirement placed on Sand
authors. Plan B uses it for the shared `wgpu` renderer and batched Worker
physics; Plan A may use it for portable untrusted Behavior or browser builds,
not as a tax on native Sands. WIT and the WebAssembly Component Model may later
provide generated bindings for portable installed Behaviors, but WIT does not
replace the Sand definition, package manifest, capability model, JavaScript or
CEF bridge, or Box editor. A browser toolchain or Component Model revision can
therefore change without changing what a Sand means.

#### Behavior modules and event composition

Plan A built-in Behavior is implemented by Rust systems attached to stable Sand
or Behavior uids. HTML Behavior is attached through the same graph, never
through inline `onclick` source, global DOM selectors, or an arbitrary script
string stored in Box. A JavaScript module is a content-addressed package asset
with declared typed inputs, outputs, configuration, state-plane access, and
capabilities. CEF or the Plan B browser runtime loads it once per revision and
mounts one scoped instance for each owning Sand instance.

The module lifecycle is deliberately small:

```javascript
export function mount(context) {
  const release = context.inputs.toggle_audio.subscribe(() => {
    context.media.toggleAudio();
  });

  const releaseState = context.media.subscribe((state) => {
    context.outputs.state.emit(state);
  });

  return () => {
    release();
    releaseState();
  };
}
```

The real `context` is runtime-validated and supplies only the instance
identity, resolved configuration, typed input/output handles, permitted state
plane handles, granted host capabilities, and an abort/teardown signal. A
renderer adapter may additionally receive its scoped root. Ordinary Behavior
does not receive the global Box store, unrelated Sand roots, raw credentials,
or authority merely because another child in its Castle has it.

Common wiring should remain declarative whenever possible. Emitting an event,
invoking one typed Action, toggling local state, selecting a value, or mapping
one typed field does not justify arbitrary JavaScript. These are inspectable
built-in Behavior nodes that Box can draw, validate, copy, publish safely, and
explain. A module Behavior is for a genuine transform, state machine, media
controller, specialized view adapter, or interaction that the declarative
registry cannot express cleanly. Its ports and capabilities stay visible even
when its internals are opaque to Box.

A button therefore owns a native `pressed` event port. It does not know whether
the press deletes a Record, changes a page, emits a board event, or toggles a
microphone. Connections decide that use:

- `pressed -> Action(record-delete)` performs a durable write after the host
  validates target identity, arguments, and authority;
- `pressed -> emit(record-selected)` sends a typed event through the current
  group and any explicitly exported parent port;
- `pressed -> local-state(toggle, panel-open)` changes only instance host
  presentation state;
- `pressed -> module(toggle_audio)` enters a scoped JavaScript Behavior;
- several connections may fan out from one port, with explicit deterministic
  ordering where order matters.

A Protein-bound Record control makes the separation concrete. Its compound
definition exports one `record` input, routes that value to two declarative
Behaviors, and exports the selection event needed by another Sand:

```json
{
  "uid": "lince.record-actions",
  "root": {
    "localUid": "record-actions",
    "renderer": { "uid": "lince.lynx.row", "revision": "sha256:…" },
    "children": [
      {
        "localUid": "open",
        "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
        "overrides": { "label": "Open" }
      },
      {
        "localUid": "delete",
        "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
        "overrides": { "label": "Delete" }
      }
    ]
  },
  "behaviors": [
    {
      "localUid": "select-record",
      "kind": "emit",
      "event": "record-selected",
      "inputs": ["trigger", "record"],
      "outputs": ["selected"]
    },
    {
      "localUid": "delete-record",
      "kind": "action",
      "action": "record-delete",
      "inputs": ["trigger", "record"]
    }
  ],
  "connections": [
    {
      "from": { "node": "open", "port": "pressed" },
      "to": { "behavior": "select-record", "port": "trigger" }
    },
    {
      "from": { "node": "delete", "port": "pressed" },
      "to": { "behavior": "delete-record", "port": "trigger" }
    }
  ],
  "exports": [
    {
      "name": "record",
      "direction": "input",
      "type": "record-ref",
      "to": [
        { "behavior": "select-record", "port": "record" },
        { "behavior": "delete-record", "port": "record" }
      ]
    },
    {
      "name": "selected",
      "direction": "output",
      "type": { "kind": "event", "payload": "record-ref" },
      "from": { "behavior": "select-record", "port": "selected" }
    }
  ]
}
```

Box then owns the bindings outside that definition:

```json
{
  "proteinBindings": [
    {
      "from": { "area": "records-area", "field": "record" },
      "to": { "instance": "record-actions-1", "port": "record" }
    }
  ],
  "connections": [
    {
      "from": { "instance": "record-actions-1", "port": "selected" },
      "to": { "instance": "record-view-1", "port": "record" }
    }
  ]
}
```

Pressing Open now sends the bound Record reference to the Record Sand; pressing
Delete requests the typed delete Action for that same reference. Neither
button contains workflow knowledge, and this case needs no custom JavaScript.
If a richer interaction later needs a module, it replaces one Behavior node
without changing the buttons, Protein binding, exported Castle interface, or
Box connection.

DOM bubbling is not the composition bus. A child event enters the typed graph,
stays within the owning group unless exported, and is attributed to the
definition, child, instance, and triggering input. Actions remain host/engine
operations; a JavaScript module can request one only through an explicitly
granted Action handle. This makes the event arrows seen in Box correspond to
the runtime route rather than merely documenting incidental DOM behavior.

#### How a Box-built group is stored

The Box document places instances of definitions. A hand-built group is a
workspace-local definition plus one placement of it, not membership copied
onto every child and not a stack of group ids:

```json
{
  "schemaVersion": 1,
  "workspaceUid": "workspace-main",
  "localDefinitions": {
    "workspace:call-controls": {
      "uid": "workspace:call-controls",
      "revision": "sha256:…",
      "root": {
        "localUid": "controls",
        "renderer": { "uid": "lince.lynx.row", "revision": "sha256:…" },
        "layout": { "direction": "row", "gap": "space-1" },
        "children": [
          {
            "localUid": "mute",
            "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
            "overrides": { "label": "Mute" }
          },
          {
            "localUid": "camera",
            "definition": { "uid": "lince.lynx.button", "revision": "sha256:…" },
            "overrides": { "label": "Camera" }
          }
        ]
      },
      "behaviors": [
        {
          "localUid": "media",
          "kind": "module",
          "module": { "asset": "behavior/media-controls.js", "hash": "sha256:…" },
          "capabilities": ["media.microphone", "media.camera"]
        }
      ],
      "connections": [
        {
          "from": { "node": "mute", "port": "pressed" },
          "to": { "behavior": "media", "port": "toggle_audio" }
        },
        {
          "from": { "node": "camera", "port": "pressed" },
          "to": { "behavior": "media", "port": "toggle_video" }
        }
      ],
      "exports": [
        {
          "name": "media_state",
          "from": { "behavior": "media", "port": "state" }
        }
      ],
      "assets": [
        {
          "path": "behavior/media-controls.js",
          "kind": "module",
          "hash": "sha256:…"
        }
      ]
    }
  },
  "instances": [
    {
      "uid": "call-controls-1",
      "definition": {
        "uid": "workspace:call-controls",
        "revision": "sha256:…"
      },
      "parent": null,
      "transform": { "x": 1240, "y": 680, "width": 420, "height": 40 },
      "anchor": "world",
      "layer": "content",
      "order": 12,
      "overrides": {},
      "hostStateUid": "host-state-call-controls-1"
    }
  ],
  "connections": [],
  "proteinBindings": [],
  "editor": {
    "lockedDefinitionInstances": ["call-controls-1"]
  }
}
```

Internal children and connections belong to the definition. Connections
between top-level instances and Protein field bindings belong to the Box
document. Per-instance differences are override patches. Local transforms of
children are relative to their definition root, so moving one Castle rewrites
one parent transform. Saving it for reuse changes ownership/lineage and its
catalog availability, not its composition semantics.

The same definition can later serve as a Protein result template. Box supplies
one row binding context per repeated instance and maps fields into its exported
or child inputs. The definition itself does not contain one copy per result.
Live Protein changes and interaction state remain JavaScript runtime concerns;
Maud renders the reusable starting structure, not every future record arriving
through a subscription.

The correctness price of this model is required work, not optional
optimization: paired Maud/schema constructors, stable local identities,
authoritative Rust schemas and JavaScript validators, exact definition
revisions, scoped module lifecycle and teardown, explicit state planes,
capability enforcement, definition/instance separation, and visible
fork/lineage behavior. Without these, Maud would only make source files
prettier while Box and first-party code continued to mean different things.

Performance work follows measurement: cache rendered fragments by definition
revision, import each behavior module once, delegate common native events per
composition root, camera-cull offscreen presentation without suspending its
instance, and virtualize only presentation for repeated Protein results. None
of those optimizations may stop off-camera Behavior, physics, games, media,
Protein, events, or CEF execution; copy the definition graph; erase Sand
identity; or merge security boundaries. Trusted first-party nodes may share a
root; an external or isolated Sand remains behind its CEF boundary under Plan A
or iframe/WebView boundary under Plan B and participates in a Castle only
through its wrapper's declared ports.

There are three state planes, never one ambiguous shared bag:

- durable shared truth is Ledger data reached through Protein and Actions;
- persistent interface configuration is Box host state;
- cursors, presence, transient events, media, and sessions are ephemeral lanes
  or explicit host capabilities.

Groups move together and shelter internal events. Crossing a group boundary
requires an explicitly exported typed port. Copying preserves definitions,
layout, configuration, durable bindings, and connections, but not live
sessions, presence, subscriptions in flight, or other ephemeral state.

#### Supercomponent test

The old Supercomponent theory remains a test of the composition model, not a
future monolith. Its useful axes are still:

- how much Record information is shown;
- how much interaction between Records is shown;
- whether spatial position carries meaning;
- which related Lince pillars, such as Karma, automation, and Transfer, appear
  and can be acted upon.

A traditional Kanban shows variable Record detail, makes column position
important, and usually shows little automation or Transfer information. A
Relation view shows less Record content, makes links central, and lets the
physics determine placement. A Karma view may show little Record detail while
making automation and Transfer consequences prominent.

Box reaches combinations between these presets by editing Sands, Behavior,
Protein field bindings, areas, and groups. The first implementation stays with
the shape already returned by one Protein item: a person may wire `title`,
`quantity`, and `description` into separate Sands, add an unbound button or
label, and lock them as the repeated result-template group. Moving or spatially
influencing any matching child moves the whole group. Pulling a Record because
of associated Karma, or composing Karma and Transfer projections into that
row automatically, is a preserved later expression rather than part of the
first area implementation. Configuration is preferred over generating another
bespoke component when one direct change can meet the Need.
