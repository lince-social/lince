Sands (@sands: 1, is #chapter, #instinct, #part-of @interface, #done) { r_14NKDPPS969TPSRGYJBJBJWEGR

## Sand

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

#### One definition graph, several authoring paths

The GPU-first Plan A and Maud/HTML-first Plan B share one validated Sand
definition graph. Plan A pairs native Rust/GPUI or world-renderer
implementations with that graph and uses CEF for real external HTML. Plan B
pairs Rust/Maud fragments and native ES modules with the graph and uses one
shared Rust/`wgpu` WebAssembly spatial renderer. The ordered runtime decision
and HTML alternatives are recorded in
[Customization](Customization.md#runtime-plans-and-plan-b-html-alternatives).
Maud does not become the stored Sand format and it does not run in the browser.
Native Rust, Rust/Maud, raw packaged HTML, and Box edit mode can all produce or
consume the same graph:

```text
Native Rust + GPUI/world renderer ────────────────┐
Rust constructors + Maud ──> Sand artifact ───────┤
Raw HTML + declared metadata ─> Sand artifact ────┼─> Sand definition ─> composition host
Box edit operations ──────────────────────────────┘                         ├─> GPUI adapter
                                                                           ├─> native world adapter
                                                                           ├─> installed CEF adapter
                                                                           ├─> Website CEF wrapper
                                                                           └─> Plan B DOM/wgpu adapters
```

#### Renderer roles across v1 and v2

V1 and v2 reuse the same Sand definitions but do not force every Sand through
one drawing implementation. The preferred native projection is:

- GPUI for sharp application chrome, inspectors, editors, focused rich Sands
  and viewport-pinned HUD surfaces;
- the shared Bevy/`wgpu` world for the desk, Areas, connections, large
  populations of lightweight Sands, ordinary 2D/3D objects and later globe or
  game content; and
- CEF for installed external HTML and zero-authority Websites.

The Lince component library on GPUI owns the visual grammar; it does not adopt
Bevy's example UI or a generic game theme. Lightweight world Sands consume the
same semantic tokens in instanced rectangle, line, icon, image and shaped-text
primitives so density and hierarchy remain recognizably Lynx. A focused Sand
may expose a richer GPUI editor without changing its definition or making its
idle representation a second Sand.

V1 places these projections in an orthographic local workspace. V2 may anchor
the same instance to Earth, an authored frame, an avatar, another artifact or
the viewport. The persistent definition declares semantic presentation and
required capabilities, not “is a GPUI widget” or “is a Bevy entity.” Adapter
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
GPUI or the world renderer and external HTML runs unchanged in Chromium/CEF.
Third-party HTML never needs Rust, Maud, Wasm, GPUI, or `wgpu`. Maud remains
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

The exact Rust names may change in C0, but the pairing may not. Primitive
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
The examples below establish the semantic shape. C0 freezes exact field spelling,
serialization, and the complete port-type vocabulary against shared fixtures;
those details may change together before implementation begins.

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
definition graph declares a renderer reference, typed ports, Behavior,
capabilities, state ownership, assets, and teardown. The composition host then
selects a runtime adapter:

- Plan A native application controls and rich editor surfaces use GPUI with
  Rust Behavior behind typed ports;
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

The shared world renderer is retained rather than rebuilt from GPUI or the DOM.
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
the shape already returned by one Protein item: a person may wire `head`,
`quantity`, and `body` into separate Sands, add an unbound button or label, and
lock them as the repeated result-template group. Moving or spatially
influencing any matching child moves the whole group. Pulling a Record because
of associated Karma, or composing Karma and Transfer projections into that
row automatically, is a preserved later expression rather than part of the
first area implementation. Configuration is preferred over generating another
bespoke component when one direct change can meet the Need.

### External HTML and Sand packages

External HTML is a first-class Sand source, but visual integration and trust
are separate concerns. Imported content can look and behave like it has always
belonged to Lince while still crossing an explicit capability boundary. There
are four entry forms:

- a local HTML file becomes a content-hash-pinned Sand definition;
- a Sand package carries HTML, assets, Behavior metadata, its capability
  manifest, lineage, version, licenses, and credits;
- a `.lince` package carries a workspace or group of referenced Sand
  definitions and placements;
- a remote URL becomes a Website Sand rather than silently copying or
  modifying the remote page.

#### Website Sand

A Website Sand is a deliberately untrusted browser surface. It is fully
interactable and can be moved, resized, grouped, connected at its wrapper, and
influenced like another Sand. Box owns those wrapper capabilities. The remote
page receives no Lince identity, credential, Protein, Action, lane, host state,
native IPC, or ambient bridge and cannot inspect its parent composition. Lince
cannot inspect or restyle the cross-origin page or read its private state.

Website mode permits ordinary user-directed HTTPS navigation and the remote
services, scripts, forms, and subresources that the site itself needs. This is
possible with the same fundamental risk as opening a browser tab: the site may
track, deceive, fingerprint, waste resources, or contain a browser/WebView
exploit. Lince can protect its own authority and local data; it cannot certify
arbitrary Internet code as harmless. A persistent, non-spoofable origin label
and security menu therefore remain visible even when ordinary Sand chrome is
borderless.

Adding a URL does not execute it immediately. The person first sees the origin
and chooses to enable Website mode after an honest explanation that the site
can contact the Internet and store site data like a browser tab. This grant is
to run as a Website, never a grant of Lince authority, and can be revoked by
unloading the page and clearing or retaining its isolated site data as chosen.

Static HTML and Lince integration are different modes, not permission toggles
inside a live Website:

- **Static HTML** is pinned/imported content with no network, remote storage,
  or ambient script authority. It is the safest mode for archives and Facades.
- **Website** runs the remote origin like a normal Web page but has zero Lince
  authority. It exposes only host-owned wrapper facts such as URL, title,
  loading state, focus, and bounds.
- **Installed external Sand** is content-hash-pinned code with a reviewed,
  versioned manifest. This is how a third party combines external network
  access with Protein, Actions, or typed Sand ports. An arbitrary live Website
  never upgrades itself into an integrated Sand.

An installed external Sand may therefore declare an input such as
`record: RecordSummary`, receive it from a visible Protein field mapping, and
emit `record-clicked: RecordRef` into Box. Another Sand or Castle may consume
that event, and an explicitly connected route may request a typed Action. CEF
IPC or `postMessage` is only transport: the host validates the port, value,
instance, rate, size, capability, actor, and Action request before delivery.
The event name does not grant access to the Record table, and the external Sand
cannot subscribe to a Protein or invoke an Action that its definition and Box
connections did not expose.

Under Plan A, Website is a separate CEF browser surface and request context
whose accelerated texture is composited by the native runtime. It remains
mounted and executing when outside the camera even though Lince does not draw
its texture. Because the CEF page is a browser surface rather than a child
iframe, framing headers do not apply in the same way; sites may still reject
embedded browsers, protected media, authentication, automation, or unsupported
Chromium builds, and Lince offers an explicit open-in-browser fallback.

In the browser client and Plan B, Website uses a
[sandboxed cross-origin iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe).
A site may refuse embedding with
[`frame-ancestors`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors)
or `X-Frame-Options`; Lince respects that
decision and offers to open it externally rather than proxying the page or
stripping its protection. Services such as YouTube work only through their
supported embed URLs. If Plan B uses Tauri, Website uses a dedicated WebView
with no [`Tauri capability`](https://v2.tauri.app/security/capabilities/)
instead of sharing a privileged WebView boundary. This is especially important
where the platform cannot reliably attribute iframe IPC to the iframe rather
than its containing WebView.

Website storage is useful and permitted, but is not Lince host state. Cookies,
local storage, IndexedDB, Cache Storage, and service-worker data live in a
separate Website profile, partitioned by origin and isolated from Lince and the
person's normal browser profile. Same-origin Website Sands may share their site
session; a private Website instance uses an ephemeral profile. Configuration
shows per-origin use and quota and can clear one origin or the complete Website
profile. Browser iframe privacy rules may still make an embedded login behave
differently from a top-level tab, and Lince reports that incompatibility rather
than weakening storage isolation silently.

Normal Website navigation is HTTPS-only by default. A Website must never reach
Lince native/Tauri capabilities, custom protocols, `file:` URLs, or
authenticated Lince host endpoints. Every local HTTP and WebSocket endpoint
also rejects a
foreign `Origin` and requires an unguessable, scoped credential for privileged
requests; CORS and the browser's
[same-origin policy](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Same-origin_policy)
are not treated as CSRF protection.

Where CEF or a dedicated WebView exposes reliable request interception, Website mode
also blocks loopback, link-local, and private-network destinations and
rechecks redirects and DNS resolution against rebinding. A normal browser
iframe does not give its parent complete control over the destinations its
cross-origin child requests. The browser client therefore states the same
local-network risk as an ordinary browser tab instead of claiming containment
it cannot enforce; its hard boundary is that no such request carries Lince
authority and no Lince endpoint accepts it. Downloads, popups,
external-protocol links, clipboard, camera, microphone, geolocation,
notifications, screen capture, and unpartitioned storage begin denied and
require a visible per-origin decision where the platform can enforce it. A
download is host-mediated and never gives the page a filesystem path.

Trust is provenance-based:

- official Sands are trusted and automatically receive only the capabilities
  declared in their signed first-party manifest, without repeated prompts;
- locally authored Sands run in developer mode and make their requested
  capabilities visible;
- installed third-party packages are pinned, reviewed by manifest, and
  granted capabilities explicitly;
- raw local content is isolated, receives no Lince credentials, and has no
  Protein, Action, filesystem, terminal, clipboard, media, or network
  capability by default. A Website Sand has only its separate browser profile
  and host-owned wrapper described above.

Capability checks are enforced by the host and engine, not by hiding UI or
trusting iframe JavaScript. Network access is itself a declared capability,
scoped by destination and method for installed Sands. A Website's ordinary Web
traffic is not a Lince capability and never carries Lince data. Every installed
Sand bridge message is schema-checked, size-bounded, and attributed to the Sand
definition and instance that caused it. Unknown API versions and unknown verbs
fail closed; there is no compatibility path for obsolete Sand APIs.

Official and user-owned definitions update live because their identity and
lineage are local and controlled. Installed external executable packages do
not: an update produces a new content hash, shows the capability and license
diff, and requires deliberate adoption. Any vendored embedded library ships
its required LICENSE, NOTICE, and credits inside the Sand package.

The [runtime facts appendix](#runtime-facts-appendix) records what the current
Web implementation already does. Statements there are evidence, not a promise
to retain its legacy component or frame shapes.

### Record

The reusable Record body editor keeps canonical Markdown while making it easy
to insert and render blocks. Typing the Markdown remains equivalent to choosing
from the searchable slash palette: `#` through `#######` create headings,
`![](url)` embeds an external image, the image picker stores an allowed local
image under an opaque media name, and `- [ ]` creates an interactive checkbox.
`@slug` references another Record in bodies and thread messages.

The Record sand presents that one canonical body in three modes: **Raw** is a
plain editable Markdown textarea, **Pretty** is a read-only rendering, and
**Pragmatic** is the default rendered editor. In Pragmatic mode the line under
the caret becomes source; fenced structures such as Mermaid become source as
a complete block. A line means source text ending at a real newline, never one
visual row produced by wrapping inside a narrow Sand. After five seconds
without caret activity it renders again and uses the browser's native caret in
that one rendered surface; moving or typing turns the line or block reached by
the caret back into borderless raw source. There is no synthetic caret or
hidden input to disturb the Sand's layout or scroll position. The body itself
has no border; the border belongs to the complete Record surface. Its
property accordion is a divided horizontal rule, not a box: the down-triangle
half reveals every section and the up-triangle half hides them all. It normally
shows only filled sections and always opens a new Record with History closed.
Head, slug, quantity, the accordion control, and body remain visible in that
order. Rendered checkboxes use their own block row so their control cannot
collapse surrounding text into one inline run.

- [x] **Record** (formerly "record_info" — the sole markdown editor, viewer, and creator for a record, and the home for every other per-record concern) — the get view IS the edit view (head/slug/quantity/ body writable, Save writes only what changed, a dirty form is never clobbered by live updates); Zero (`deactivate`) and Delete (`delete-record`, permission-gated) are separate buttons; creation mode shows the same fields empty, Create + focuses the new record; carries the shared slash-block editor (headings/images/checkboxes/`@slug`, the same palette everywhere in a body); collapsible sections for **Work** (start/due dates, estimate, worklogs with play/pause, on the `work` record extension, offline-queued writes), **Assignees** (`assigned-to` assertions), **Relations** (every hop-1 binary assertion in either direction, predicate+object inputs, both autocompleted — a document/URL is an asserted relationship or inline media in the body, with no separate resource/attachment model), and **Threads** (a real multi-thread system — a tab per thread, search filters which tabs list without hiding messages; chat-style runs (2026-08-07) show the sender name — `user@organ` when the message's origin-organ name differs from the sender's, just `user` when they match (the common single-user-organ case) — only on the first message of an unbroken run from the same `created_by`, every message keeps its own bottom-right timestamp and edit/delete controls, and editing an existing message now uses the same shared slash-block editor as composing one; `@slug` in a post becomes a Record reference, delete controls per permission). Reusable — any sand drives it via a scoped `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel. Full real-time collaborative editing is blocked on the CRDT text relay in [Synchronization](Ontology.md#11-sync-one-channel-two-persistence-modes).

#### Drawing in a body (future feature, moved here from Ontology 2026-08-16)

Wanted, not scheduled: draw on a canvas, scale the result down, and place it in
part of a Record body the way an image is placed — including replacing the body
art of a Record that arrived from a shipped bundle. Two things about it are
already settled and must survive the wait, because both were decided against a
constraint rather than a preference:

- **The body holds a REFERENCE to a content-addressed asset, never inline
  bytes.** Body text rides the op log to every peer, and inlining an image
  there undoes the O(live state) property the log work bought. Store the
  drawing in the media store addressed by its hash and reference it — which
  also deduplicates the same drawing across Records for free.
- **Raster, not SVG.** `media_assets` sniffs magic bytes and refuses SVG
  outright (`evil.svg` is a test case), because an SVG is a document that can
  carry script. PNG or WebP off the canvas. Vector strokes would need their own
  sanitised path — the same threat model as rendering a stranger's Facade — and
  that is separate work nobody has asked for yet.

### Kanban

The Kanban sand when ready will be able to provide teams the organization necessary to tackle projects together in a classic way. The data they CRUD in Kanban is accessible in other sands to fit greater workflows though.

- [x] Have a way to create a new Record.
- [x] Moving one card from one column to the other issues an update on the quantity of the Record.
- [x] Cards can be shown minimally or with a lot of information about them displayed.
- [x] The columns of the Kanban dictate what quantity the Records have underneath. The user doesnt have to know that column Done is for quantities 1 by default. But they have to know if they want to change it. I have a place to configure my columns, in that case i need to select one quantity for the the column, so Records with that quantity are shown in the respective column (would be cool to select a range, like from 1 to 2, 3 to 10). I must be able to sort them with drag and drop to say that column X is to be -1 and move it to -2. I must be able to click buttons to create new columns, give a name and type a quantity. Maybe have a tooltip to signal the reason behind using quantity.
  - [x] Have a way to CRUD column presets, like instead of Todo, WIP, Done its Backlog, Next, WIP, Finished. And i can apply one to this Kanban.
- [x] Having a small indicative that the connection with the backend is ok, can be used to signal that an update is taking place and when it is finished (maybe a cute little ball with different colors for the states - duds).
- [x] Be able to select one or more Records, to execute possible actions: move to another column, delete.
- [x] Currently, metadata of Records is only visible and editable in the Record sand, a reusable sand for editing in-depth info about Records. We must be able to see such metadata, even if we can only interact with it through Record sand. Either way, here are the tasks for metadata control someway:
  - [x] Date for the supposed start and end of the task.
  - [x] Time estimate, how much time do i think this is going to take, in hours and minutes (the data saved is in minutes).
  - [x] Play/Pause button to log time spent in the task. Play starts a work log, Pause ends one, time is added on Pause. Also we need to be able to full CRUD this so that if i spent some time before I can add it, if I inputted something wrong i can update the existing or delete it.
  - [x] Assign the task to someone, by their name or username.
  - [x] Be able to set the parent/children of this task.
  - [x] Being able to CRUD threads and messages as links of records (that belong to a record) that can have the same complexity of body content: text, images...
  - [x] The interaction with the body of Record must be able to have slash '/' commands to put add content in an easy way: typing /h3 will give you ### which is the end result that remains in the body (###). If we can make the body of a kanban have text, why not make it have the full editing and visualization that the 'Record' sand has for the body of the Record? We implemented the same checkbox clicking in the body of the record in kanban, why not put the body of the Record of the 'Record' sand?
    - [x] Changing the body of Records in Kanban cards by clicking on it to write in it or to check a box (slash commands only in Record sand? too hard to implement such feature twice? gotta be a way)
- [x] Filter and search cards through Protein by assignee, work date, assertion predicate, directional binary assertion such as `@parent`/`@child`, quantity, or text. Filters can be nested in AND/OR groups up to 10 levels. See [Ontology](Ontology.md).
- [x] Optionally group cards in swimlanes by assignee, parent, concept, or a Protein grouping key.
- [x] Order based on several important fields, from head of record, to quantity, @concept and links. 

### Relation

Relation is a graph projection of **binary Record assertions**. It does not own
a separate relation or link model; the shared data semantics, CRUD operations,
hierarchy widening, and Protein behavior live in [Ontology](Ontology.md).

- [x] **Relations** — d3 force graph with physics sliders, golden-angle layout,
  zoom/pan/fit, directed arrows, and assertion-predicate labels. Shift+drag
  asserts a directed binary assertion optimistically; edge-click selects it;
  the header chip retracts it. Predicate inputs autocomplete from Concepts.
- [x] **Trail mode** — projects a selected predicate's forward assertion graph
  topologically, with a Done/Undo promotion cascade over shared status
  predicates (quantity or Concept buckets such as
  `@todo/@next/@wip/@done`). Delete retracts the selected assertion; Ctrl+Z
  undoes the local session action.
- [x] **Protein trails and Focus** — consumes a directed assertion-order item
  from Record Protein. The returned ordering supplies traversal and the
  earliest root; Focus advances through matching Record states.
### Table

Table is a simple tabular projection of Protein results. It is a useful
baseline renderer and composition primitive, but it does not imply that Ledger
storage is one generic table or make the Table Sand the base class of other
Sands.

### Communication

The Communication sand is a messaging-app surface for Lince. Every
conversation — with one user of your organ, users of other organs, or a mixed
group — is a normal Record with the full thread/message system, and any
conversation can additionally carry an audio and/or video room
(Discord-style: a room you activate and join, not a phone call you dial).

Core stance: **this is mainly a Record with audio/video room capabilities.**
The Communication Sand never invents a chat system. It owns the conversation
list and room surface; the one pinned Record Sand owns threads, messages, and
`@slug` references. Communication ships ungrouped and drives that Record Sand
through board-scoped `recordClicked` and `recordCreate` events. This replaces
the earlier design that bundled another Record Sand into each workflow.

A conversation access group may contain several People from several Organs.
Its durable conversation/thread/message content and its live-room key material
are encrypted to the authorized group. A room can be activated for the whole
conversation or for one thread. A thread room remains inside its conversation,
inherits or narrows the thread's access set, and cannot widen access beyond the
conversation merely by becoming a call.


Everything below is the implementation order. A stage's title checkbox is
ticked only when all its inner checkboxes are ticked and its selftest passes.

#### [ ] S0 — Standing agreements (respected by every stage)

#### [ ] S1 — Vocabulary and model definition (docs before schema)

#### [ ] S2 — Store layer

#### [ ] S3 — Actions and authorization (nucleus/engine)

##### S3 landed code

The handler logic already lives in `crates/engine/src/communication.rs`
(an `impl Engine` block: `communication_create`, `communication_bind_controller`,
`communication_join`, `communication_leave`, `communication_close`,
`communication_recording`, plus `require_communication_participant` and the
local `comm_resolve` / `comm_annotate` helpers). Only the wiring below belongs
in `actions.rs`, which is being refactored by the transfer work — paste it when
that file is green again.

#### [ ] S4 — Sand surface scaffold

Landed code (client written; server verification blocked on the engine build):
`crates/web/src/sand/communication/mod.rs` (package builder) +
`communication.html` (the Sand), registered in `crates/web/src/sand/mod.rs`.
The earlier grouped package builder was removed: the official catalog entry is
now a single ungrouped Sand so it can reach the pinned Record surface. The list
Protein needs no new server source — it is
`{ source: "record", where: [{ assertion: { predicate: "tagged", direction: "out", object: <record> } }],
include: { assertions: { predicates: ["participant"] }, threads: { messages_limit: 1 },
extension: { namespace: "communication.v1" } } }`. The chromium selftest and
`cargo test -p web` stay unticked until the engine compiles (web → engine).

- [x] Official `communication` sand on the table template; registered like
      the other official sands.
- [x] Imports as one ungrouped Communication Sand. Board-scoped events drive
      the one pinned Record Sand, which handles threads/messages for whichever
      conversation is selected.
- [x] One WS per board; Communication subscribes via Protein for list
      updates, room state, occupants, and new-message signals (one
      `subscribeProtein("communication", …)` over the shared bridge; recording
      state rides the same rows via the `communication.v1` extension). Pending
      call intents belong to F2.
- [x] Chrome/host state persists: chosen tag, selected conversation, view
      mode (`patchCardState({ communication: { tag, selected, view } })`,
      re-adopted via `onCardState`). No credentials ever in host state.
#### [ ] S5 — List view (no media)

Landed in `communication.html` (needs the engine build + a driven page to tick
the selftest). Participant remote-vs-local badge and the unread indicator have
placeholder wiring (`to_remote` flag, `unread:false`) pending the settled
private per-Person last-read state and Protein exposing a participant's origin
Organ.

- [x] Tag selector: defaults to `@communication`, switchable to any `@slug`;
      list = Records carrying that tag, newest activity first (sorted client
      side by newest message/created time).
- [x] Row contents: head, participant names, last-message preview, unread
      indicator (local-vs-remote badge + unread are placeholder until origin
      Organ is on the row and private last-read state is implemented).
- [x] Row click does BOTH: emits `recordClicked` so the pinned Record Sand
      opens the conversation's threads/compose, AND switches the
      Communication sand itself into the deeper room mode (S6) for that
      Record.
- [x] "New conversation" flow: `conversation-create` action → opens it. (v0
      creates an empty tagged conversation; the participant/group picker UI is
      a follow-up — the action already accepts `participants`/`groups`.)
#### [ ] S6 — Room mode, call view shell (no media yet)

Landed in `communication.html`; the S9 view-switch/call-bar/idle-auto-hide
skeleton also landed early since it is the same UI surface (see S9). Server
verification waits on the engine build.

- [x] Deeper mode of the Communication sand per conversation: the **call
      view**. Back affordance returns to list view.
- [x] Room configuration controls in call view: media mode
      (audio / audio+video), provider, recording policy, controller
      binding — controller binding writes `communication.v1` via
      `room-bind-controller` on open; media/provider/recording selects are
      wired to state (persisting them through an action is a small follow-up).
- [x] Room state plumbing end to end: `room-join` / `room-leave` /
      `room-close` call the S3 actions (which mutate the extension + session
      Records); live occupant strip renders from the Protein rows. A "room"
      you can be in silently — no media. (`room-open` folded into first join.)
- [x] List view shows a live-room strip on active rows: who is in, audio or
      video, with **Join audio** / **Join video** buttons directly on the
      row (join without leaving list view).
#### [ ] S7 — Media v0: one-to-one audio (pure JS + Rust signaling)

#### [ ] S8 — Video and the participant grid

#### [ ] S9 — Call-mode UX: two views, one call

The UX shell landed early with S6 (same surface). The `media` seam keeps the
call alive independent of the view, so these hold once S7 fills real tracks;
"audio plays / video paused in list view" needs the real track handles from S7
to fully honor. Selftest waits on the engine build + a driven page.

- [x] View toggle while a call is live: **call view** (grid + controls) ⇄
      **list view** (the conversation list). The call persists across the
      toggle — the `media` object is view-independent, switching never
      disconnects.
- [x] In list view during a call: a persistent slim **call bar** shows the
      active room, mute, and a jump-back-to-call-view button (`#call-bar`,
      shown via `body.in-call.view-list`). Audio-keeps-playing /
      video-paused is enforced once S7 owns the real tracks.
- [x] In call view: after a period of no interaction (mouse/keyboard/touch),
      **all controls auto-hide to maximize screen space** (`controls-hidden`
      class, `IDLE_MS` timer); any interaction brings them back. Auto-hide
      only in call mode + call view.
- [x] `recordClicked` still works during a call: browsing other
      conversations in list view drives the Record sand without touching
      the live call (`openConversation` emits `recordClicked`; the `media`
      object is untouched by view/selection changes).
#### [ ] S10 — Screen sharing

#### [ ] S11 — Recording (prototype) and artifacts

#### Far-future communication work

Everything in this part is deferred until the near-term stages (S0–S11) are
landed and proven. Karma-driven interactions are the very last thing built,
after the provider/scale decision.

##### [ ] F1 — Group scale and provider decision

##### [ ] F2 — Karma call intents (last of all)

##### Settled call decisions

For a mixed-Organ room, the conversation Record's origin Organ is the
signaling authority. Recording bytes remain at the producing storage owner;
only access-controlled metadata and resource references replicate. Starting a
screen share does not consent to recording it: capturing the share requires a
separate visible consent each time. A recording cannot begin until its
inherited or explicit retention policy is shown, and a missing policy does not
silently mean permanent retention.

Unread/last-read position is durable private per-person state that can follow
the person across their devices; it is not shared conversation truth and not
Cell-local Box state. Call controls auto-hide after three idle seconds by
default, the value is user-configurable, and reduced-motion settings do not
disable the hide itself because it is an instantaneous visibility change.

#### References

- Canonical Karma architecture: [Karma](Karma.md)
- Shared data and federation model: [Ontology](Ontology.md)
- Local: `crates/web/src/sand/record/record.html` (thread/message surface)
- Local: `crates/store/src/action_intents.rs` (intent/lease machinery)
- MDN: `RTCPeerConnection`, `MediaRecorder`, `getDisplayMedia`
- LiveKit docs: Egress overview and screen sharing; LiveKit / Jitsi+Jibri:
  Apache-2.0; mediasoup: ISC; Janus: GPL-3.0

### 2D Map

### Ergon

Ergon is the enduring, physical manifestation of our conscious actions that shapes both our world and our own evolution. Born from the Proto-Indo-European root *wérǵom, it represents the primordial energy of bringing reality into being through purposeful creation. Though long degraded by ruling elites as the mindless toil of the unfree, it is truly the highest form of conscious practice—a liberating force that allows a species to reclaim its creative output and consciously co-create its destiny with nature. - Gemini.

The coordination of production for our Needs requires specific interfaces? We will know when time comes. Possible future Needs not covered by future sands are:

## What is left

### Sand

- [ ] Define the recursive Sand schema: definition identity, composition tree,
  renderer reference and binding kind, typed ports, Behavior bindings, required
  capabilities, default state, overrides, lineage, content hash, assets,
  licenses, and credits.
- [ ] After the Plan A prototype selects the runtime, make native Rust/GPUI or
  world-renderer constructors the first-party Plan A path, paired with the exact
  node/port/configuration metadata Box needs. Preserve Maud as the standard
  Plan B and HTML-backed authoring path without changing HTML packages into a
  Rust-only format. Its paired constructors produce accessible `Markup` and the
  same metadata; reject naked markup as a declared child boundary.
- [ ] Define the Sand artifact compiler that normalizes the selected authored
  graph (Rust/Maud or declared raw HTML metadata),
  validates it through the authoritative Rust schema, renders and hashes
  fragments, gathers native JavaScript modules, Wasm modules, generated loader
  glue, shaders and other assets, and enforces manifest, capability, source to
  artifact hash, LICENSE, NOTICE, and credit completeness.
- [ ] Define declarative Behavior kinds for common event, local-state, field,
  and typed-Action routes, plus the content-addressed ES-module Behavior ABI:
  typed ports, runtime-validated context, explicit capabilities and state
  planes, scoped roots only for renderer adapters, deterministic ordering, and
  mandatory teardown.
- [ ] Define the logical Sand runtime ABI once and generate its adapter
  projections: native Rust/GPUI calls, retained world-scene handles, validated
  size- and rate-bounded CEF messages for installed external HTML,
  wrapper-only Website ports, Plan B scoped DOM/`MessagePort` calls, and an
  optional WIT projection for Wasm Behavior. Prove the adapters agree on
  lifecycle, typed ports, attribution, capabilities, state planes, Action
  requests, errors, camera-only presentation culling, and teardown while
  passing no raw DOM, GPU object, pointer, credential, or global Box store
  through the portable boundary.
- [ ] Define the GPU renderer vocabulary and package rules for built-in
  patterns, sprites/glyphs, zones, connections, drawings, selection, and
  specialized leaves. Use one shared device and retained scene with stable
  visual-node uids and partial buffer updates. An arbitrary shader is installed
  executable content with an exact hash, declared GPU capability, resource
  budget, validation, license, credits, and deterministic disposal.
- [ ] Use one group representation for an ordinary local group, a locked
  group, a saved compound Sand/Castle, and a Protein result template. Locking
  is editor state; saving creates a reusable definition; neither creates a new
  component kind or execution path.
- [ ] Give definition children stable local uids, local transforms and explicit
  order, referenced definition revisions, override patches, and connections
  between typed ports. Exported ports preserve their identity through nesting
  and unexported ports remain sheltered.
- [ ] Build composition into Box edit mode; there is no separate Sandbox Sand
  or Sand Editor product.
- [ ] Rebuild official workflow Sands into referenced LynxUI + Behavior pieces
  after the contract is proven. Do not preserve the legacy component API or
  old board state merely to avoid rebuilding.
- [ ] After the Customization completion gate and official-Sand migration,
  prove the Box model vertically with one current Protein item, visual result
  fields, a mixed bound/unbound result-template group, repeated row instances,
  and force/sort/mutation areas. Do not use this spatial proof to finish the
  component or composition foundations underneath it.
- [ ] Before that spatial proof, prove reuse in the composition workbench with
  a standalone Button Sand and the same definition nested inside a Video Call
  compound Sand, then nest that compound again. Editing the shared definition
  updates every instance; instance overrides remain local; fork/detach is
  explicit.
- [ ] Build that fixture three ways: from the selected Plan A native paired
  constructors, from Plan B Rust/Maud paired constructors, and through Box
  operations. Normalize all into the same definition graph and prove equivalent
  identity, ports, connections, Behavior meaning, overrides, save/reload, and
  teardown through their renderer adapters. Do not require Box to generate or
  rewrite Rust or Maud source.
- [ ] Treat every code-owned native or Maud definition as
  instantiate/override/fork in Box. Box edits a forked user definition rather
  than creating a second source of truth for Rust; lineage and revision changes
  remain visible.
- [ ] Write concise author documentation that starts with composing existing
  pieces and progresses to HTML, Protein, Actions, ports, permissions, and
  packaged assets.

- [ ] Define and version the Sand manifest, bridge handshake, typed ports,
  capability vocabulary, provenance record, resource limits, CSP, and package
  signature/integrity rules together.
- [ ] Prove the installed external path with one CEF HTML Sand that receives a
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
- [ ] Build the Website Sand with an isolated CEF browser surface and request
  context under Plan A, a sandboxed iframe in browsers, and a zero-capability
  dedicated WebView if Plan B uses Tauri. Keep origin/security chrome above
  remote pixels and prove that Website content cannot invoke Lince native/Tauri
  APIs, overlap system chrome, or receive a privileged parent message. Moving
  it off-camera culls composition only and does not unload, suspend, or throttle
  its browser execution.
- [ ] Enforce HTTPS navigation; deny custom protocols, filesystem access, and
  all Lince native/Tauri capabilities; and harden every local HTTP/WebSocket
  endpoint against foreign origins, unauthenticated requests, and CSRF. In CEF
  or dedicated WebViews, additionally intercept requests to block loopback,
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
  identifiers crossing profiles, and frame/IPC confusion. Keep CEF, WebView,
  and browser runtimes patched; sandboxing does not eliminate engine exploits.
- [ ] In browser/Plan B iframe mode, detect sites that prohibit framing and
  offer an explicit open-in-browser fallback. Never strip or proxy around
  `frame-ancestors` or `X-Frame-Options`. In Plan A CEF mode, detect sites,
  authentication, protected media, or browser policies that still reject the
  embedded runtime and offer the same fallback.
- [ ] Package the authoring documentation and a minimal bridge test kit so
  external HTML can integrate without copying an official Sand as folklore.
- [ ] Remove the legacy nested-payload frame and old Lynx component API during
  the rebuild. Route `.lince` imports by inspected content rather than a
  legacy filename suffix; unknown shapes fail closed.
- [ ] Federate published Sand packages between connected Organs while
  preserving content hash, lineage, author, capabilities, and licenses. The
  contract must not depend on whether bytes live on disk or in a future object
  store, and it must not require a central registry.

- [ ] Later on, some form of creation of data, similar to ontology's trail should exist and be able to see it in this sand, to input in some dsl or lingua the creation of data to make this demo of paradigms of intelligence: https://paradigms-of-intelligence.github.io/morpho/.

- [ ] A conversation is a normal Lince Record; it is the durable
      communication object (head, body, participants, group links, threads,
      messages, call sessions, recording refs, transcript refs).
- [ ] Conversations are discovered by tag (`@communication` by default, any
      chosen `@slug`); no special conversation table.
- [ ] Audio/video is an activation mode of a conversation or one of its
      threads, never a separate product; a call never exists without a parent
      conversation Record.
- [ ] Room semantics are Discord-like: the conversation/thread Record is the
      durable room scope and a session is one occupancy stored as a child
      Record.
- [ ] Conversation, thread, message, signaling, and media access is encrypted
      to authorized participants across Organs. Relays route ciphertext and do
      not become participants merely by transporting it.
- [ ] Official sand, built on the table template, Protein/Actions only, one
      WS per board, chrome state lives in host state.
- [ ] Rust owns the durable control plane (model, authorization, room state,
      Karma consequences, leases, recording metadata, message creation,
      audit, provider tokens). Browser JS owns browser-only edges (device
      permissions, WebRTC/provider SDK, local device state, media layout).
- [ ] Karma never broadcasts vague commands; it creates typed, targetable,
      claimable call intents.
- [ ] Recording is explicit and visible; silent automatic recording never.
- [ ] Each UI/backend stage lands with a driven chromium selftest before the
      next stage starts.

- [ ] Define the conversation Record shape in docs: an ordinary or identity
      `@communication` assertion first; do not introduce a dedicated
      `conversation` kind merely for filtering.
- [ ] Define the conversation assertion convention: a unary identity or
      ordinary assertion using `@communication`; any Record can be promoted to
      a conversation by asserting it.
- [ ] Define assertion predicates: `@participant` (→ person/user Records, local or
      remote organ), `group-of` (→ group Record), `call-session-of`
      (session → conversation or thread room scope), `call-recording` / `call-transcript`
      (session → media/transcript refs). Threads/messages stay exactly as
      the Record sand already does them.
- [ ] Define the `communication.v1` record extension:

      ```json
      {
        "namespace": "communication.v1",
        "provider": "native-webrtc" | "livekit" | "jitsi" | "mediasoup",
        "room_id": "stable room identifier",
        "conversation_record_id": "root conversation Record",
        "scope_record_id": "conversation or thread Record",
        "room": {
          "state": "idle" | "active",
          "media": "audio" | "audio+video",
          "occupants": ["advisory mirror of live state"],
          "session_record_id": null
        },
        "controller_sand_uid": "widget instance bound as room controller",
        "recording_policy": "manual" | "karma" | "disabled"
      }
      ```

- [ ] Define the `call_session` child Record:
      `head = "Call · <timestamp>"`, sidecar `{ started_at, ended_at, media,
      peak_participants, recording: { state: idle|recording|processing|
      available|failed } }`, linked `call-session-of` → its conversation or
      thread room scope.
- [ ] Define the participant-access and group-key envelope for encrypted
      conversation, thread, message, signaling, media, recording, and
      transcript data. Thread-scoped rooms inherit or narrow access and key
      rotation follows membership changes.
- [ ] Call intents (Karma-driven room control) are deferred entirely to the
      Far-Future part; their shape and machinery live in F2, not here. Near-
      term stages carry zero intent plumbing.
- [ ] Rule: recording files are resource references (object storage or local
      `/host/media`), never blobs in `record.body`; Lince stores metadata,
      hash, duration, owner, retention, access policy.

- [ ] Read/write helpers for the `communication.v1` extension.
- [ ] Query: Records carrying a given tag, ordered by newest activity
      (last message / last session), with last-message preview and
      participant links resolved in one shot for the list view.
- [ ] Create/close `call_session` Records and their links atomically with
      room state transitions.
- [ ] Per-crate store tests for tag query and session lifecycle. (Call-intent
      claim races belong to F2, not this stage.)

- [ ] Typed actions: `conversation-create` (participants from this organ +
      connected organs + groups; creates Record, tags `@communication`,
      links participants), `room-bind-controller`, `room-open`, `room-join`,
      `room-leave`, `room-close`, `recording-start`, `recording-stop`.
      (`call-claim-intent` is Karma machinery — deferred to F2.)
- [ ] Every room action carries the root conversation and its conversation-or-
      thread scope. The currently landed conversation-only signatures below
      must be replaced before S3 can complete.
- [ ] Message sending is NOT duplicated here — it stays the Record-sand
      message action.
- [ ] Participant authorization: only members authorized for the selected
      conversation/thread scope may join or receive its keys; cross-Organ
      identity is resolved through the Organ network.
- [ ] Room state machine enforced server-side: idle → active on first join
      (opens a session Record), active → idle on last leave/close (closes
      the session Record).
- [ ] Provider tokens (when a provider exists) are minted host-side; never
      stored in `widgetState`.

- [ ] `Action` enum variants (in the `#[serde(tag="action", rename_all="kebab-case")]`
      enum near the top of `actions.rs`):

      ```rust
      ConversationCreate {
          head: String,
          #[serde(default = "default_communication_tag")]
          tag: String,
          #[serde(default)]
          participants: Vec<crate::communication::ParticipantRef>,
          #[serde(default)]
          groups: Vec<String>,
      },
      RoomBindController { conversation: String, controller_sand_uid: String },
      RoomJoin {
          conversation: String,
          #[serde(default = "default_room_media")]
          media: String,
      },
      RoomLeave { conversation: String },
      RoomClose { conversation: String },
      RecordingStart { conversation: String },
      RecordingStop { conversation: String },
      ```

      with the two serde defaults as free fns in `actions.rs`:

      ```rust
      fn default_communication_tag() -> String { "communication".into() }
      fn default_room_media() -> String { "audio".into() }
      ```

      (`room-open` is folded into `room-join`: the first join opens the session.
      A distinct `RoomOpen` variant delegating to `communication_join` can be
      added if the sand needs to pre-open a room without joining.)

- [ ] Dispatch arms (in the `match action { … }` in `act_at`):

      ```rust
      Action::ConversationCreate { head, tag, participants, groups } => {
          self.communication_create(&head, &tag, &participants, &groups, actor, now, &mut outcome).await?;
      }
      Action::RoomBindController { conversation, controller_sand_uid } => {
          self.communication_bind_controller(&conversation, &controller_sand_uid, actor, now, &mut outcome).await?;
      }
      Action::RoomJoin { conversation, media } => {
          self.communication_join(&conversation, &media, actor, now, &mut outcome).await?;
      }
      Action::RoomLeave { conversation } => {
          self.communication_leave(&conversation, actor, now, &mut outcome).await?;
      }
      Action::RoomClose { conversation } => {
          self.communication_close(&conversation, actor, now, &mut outcome).await?;
      }
      Action::RecordingStart { conversation } => {
          self.communication_recording(&conversation, true, actor, now, &mut outcome).await?;
      }
      Action::RecordingStop { conversation } => {
          self.communication_recording(&conversation, false, actor, now, &mut outcome).await?;
      }
      ```

- [ ] Engine test (new `crates/engine/tests/communication.rs`): create a
      conversation, bind a controller, join twice (room goes active, one
      session, peak = 2), leave twice (last leave closes the session, room
      idle), and assert a non-participant actor is rejected by
      `require_communication_participant`.

- [ ] `GET contract` returns tag filter, conversation list, provider,
      allowed actions, participant lists, controller binding, recording
      policy. (Client reads list/room from the Protein rows already; a distinct
      contract endpoint for provider/ICE is only needed at S7.)
- [ ] Selftest: Sand imports, contract loads, and the honest empty-list state
      renders.

- [ ] Selftest: tag listing, row click drives the Record Sand + deep mode,
      conversation creation.

- [ ] Let the call view activate the whole conversation or a selected thread,
      show the effective access group before joining, and prevent a thread
      room from widening its inherited authorization.
- [ ] Record sand (later, optional): small "call active — join" banner when
      the viewed Record has an active room.
- [ ] Selftest: open/join/leave/close transitions, session Record writes,
      occupant strip updates, list-row join buttons.

- [ ] WebRTC signaling messages ride the existing board WS (transport
      crate); Rust relays offers/answers/ICE between authorized occupants
      only.
- [ ] Use end-to-end media encryption for every cross-Organ room, including
      provider/SFU paths. Transport encryption alone is insufficient when an
      intermediary can terminate it; a recorder is an explicit authorized
      participant with separately visible consent.
- [ ] `getUserMedia` audio; 1:1 `RTCPeerConnection`; mute/unmute; device
      picker. Local ephemeral UI state (muted, device, levels) stays in the
      browser.
- [ ] STUN/TURN configuration surface (usually `coturn`) host-side; sand
      receives ICE servers from the contract, never hardcodes them.
- [ ] Joining audio from the list row (S6 buttons) actually connects.
- [ ] Selftest: two driven pages join the same room and exchange a
      connected peer state (media-level assertions as far as headless
      allows).

- [ ] `getUserMedia` video; camera toggle independent from audio.
- [ ] **Participant grid**: one tile per participant; a participant's
      camera and (later) screenshare are tiles within the grid, divided per
      participant. Responsive layout, active-speaker highlight, local
      preview tile.
"- [ ] Small-mesh support" (2–4 peers) with the honest limits documented:
      mesh scales poorly; group scale waits for F1.
- [ ] Selftest: grid renders N tiles for N occupants; camera toggle updates
      tile state.

- [ ] Selftest: toggle during simulated call keeps room membership; controls
      hide on idle timer and return on interaction.

- [ ] `getDisplayMedia` share; the screenshare enters the grid as another
      tile of that participant (per S8's per-participant division).
- [ ] Share always needs explicit user activation; stop-share affordance
      always visible even when controls are auto-hidden (or shown on the
      call bar).
- [ ] Selftest: share tile appears/disappears in the grid.

- [ ] Manual local recording via `MediaRecorder` as the prototype path;
      explicit start/stop, visible recording indicator to all occupants.
- [ ] Artifact flow: session Record → `recording.state` transitions →
      recording Record/resource ref created → linked `call-recording` → a
      message lands in the conversation's thread (available / failed /
      deleted), so room history reads like any conversation history.
- [ ] Encrypt recording and transcript artifacts to their authorized access
      group and rotate future artifact keys when that group changes.
- [ ] Transcripts, summaries, decisions, action items are messages or
      linked Records, never hidden provider state.
- [ ] Selftest: recording lifecycle writes the session sidecar, link, and
      thread message.

- [ ] Decide the production media path once mesh limits bite: continue
      Rust-native (`webrtc-rs` / `str0m` / mediasoup-rust — most aligned,
      but an SFU + recording pipeline is a large subsystem; do not
      hand-roll a production group recorder), or embed **LiveKit**
      (pragmatic first candidate: Apache-2.0 SFU, self-host, tokens, SDKs,
      screen share, Egress recording; Go server), or Jitsi/Jibri
      (Apache-2.0, heavier), or mediasoup (ISC, low-level). Janus is
      GPL-3.0 — only with explicit license acceptance.
- [ ] If a provider is embedded: server-side recording replaces the S11
      prototype path; host mints provider tokens; the S8 grid and S9 UX are
      preserved on top of the provider SDK.
- [ ] Reject a provider path that cannot preserve end-to-end media encryption
      for group calls. Server-side recording is possible only by admitting a
      visible, explicitly authorized recorder participant for that session.
- [ ] License rule: vendored client/embedded assets keep their
      LICENSE/NOTICE/credit files beside the sand package, bundled with the
      widget assets.

- [ ] Define the call intent shape (rides the existing `action_intents`
      machinery): `conversation_record_id`, `scope_record_id`,
      `target_controller_sand_uid`,
      `action` (`room.open` | `room.close` | `recording.start` | …),
      `source`, `status`, `nonce`, lease fields, payload. No new intent table.
- [ ] Store: reuse `action_intents` for call intents (target matching, atomic
      claim + lease, idempotence by nonce, completed/failed writes) with a
      store test for claim races.
- [ ] Action: `call-claim-intent` (a sand claims a pending intent for its
      bound conversation/scope + `controller_sand_uid`).
- [ ] Karma consequences create call intents instead of direct UI commands:
      e.g. "at 10:00 create `room.open` for controller X", "when Transfer Y
      reaches agreed, create `room.close` for its room".
- [ ] Claim rules enforced: a sand claims only intents for its bound
      conversation/scope and its `controller_sand_uid`; atomic claim + lease;
      one winner; idempotent by nonce; completion writes completed/failed.
- [ ] Selftest: two controller instances race a claim; loser observes
      claimed state; intent completes.

- [ ] **Far-future, unplanned World/Map Sand:** project consented locations of
      Records and/or people into a real-time 2D map synced between Organs, then
      potentially add streets, terrain/elevation, Needs and Contributions as a
      distinct semantic height field, Transfer Proposal routes/proximity, 3D
      scenes, and Gaussian-splat places. Preserve provenance, privacy, map-data
      licensing, the distinction between geographic and data-derived height,
      and the game/world rules retained in
      [Interface](Interface.md#long-horizon-world-direction). This is
      capability motivation for Plan A, not current implementation work.

- [ ] Transparent stock control
- [ ] Logistic distribution and instant correction from a flicker of operational change of the brute mineral extractor to the chip manufacturer.
- [ ] Order management, how much requests affect production.
} r_14NKDPPS969TPSRGYJBJBJWEGR
