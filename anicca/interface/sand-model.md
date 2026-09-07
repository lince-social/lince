# Sand and Castle model

Purpose: Define Bevy-native Sands, recursive composition, state, ports, Behavior and Box-built groups.

Owner source: no dedicated Sands Record currently exists;
[Interface in Lince](../Lince.lingua) governs shared interface decisions.

Preserved source metadata: `@sands`, order 1, `#chapter`, `#instinct`,
`#part-of @interface`, `#done`, uid
`r_14NKDPPS969TPSRGYJBJBJWEGR`.

Status: prototype schema and composition evidence is retained. The owner
selected Bevy-native interface code on 2026-09-07; its runtime and authoring API
may replace the prototype without preserving a renderer-neutral layer.

Read when: implementing reusable primitives, Castles, bindings, artifacts, or composition operations.

[Corpus map](README.md) · [Current context](current.md) · [Sand plan](plans/sands.md)

---

## Sand

Projection availability is distinct from definition validity. [The native-first build rule](build.md) controls which adapters the executable contains. A valid installed HTML definition can remain stored and inspectable while unavailable to run; native siblings retain their own bindings and authority. Part A proves the native contract and fail-closed unavailable projections, not live HTML execution or browser parity. The landed joined examples below are historical evidence for the shared model; the embedded browser they ran in is gone, and installed HTML needs a way to run without embedding a browser in Lince before any of it applies again. See [the build rule](build.md#no-embedded-browser).

### Native implementation rule

New Sands use Bevy components, relationships, scenes, systems and observers
directly. A small Sand API may add stable identity, editable metadata, bindings
and effects; it must not duplicate Bevy's widget tree or hide Bevy behind a
generic renderer ABI. Translate existing backend/data formats at their real
boundary. No native Sand must emit Maud, HTML, JSON messages or a projection
manifest merely to call another native Sand.

Box and Rust author the same editable Bevy-native composition. Keep stable
Sand/definition/appearance ids, typed exported ports, overrides, permissions
and persistence because the product needs them, not for hypothetical engine
replacement. Bevy entity references are fine in memory; saved references must
be validated and remapped. Private Bevy helper entities need not become Sands.

The landed sections below describe the previous implementation. They preserve
useful behavior and external-data evidence, not a frozen native API or proof
that the Bevy rebuild is complete. [Architecture](architecture.md#bevy-native-interface)
owns the new base and scoped exceptions.

### Landed version-1 contract

`crates/interface/src/sand.rs` is the executable Rust authority for
Sand schema and ABI version 1. It separates `SandDefinition` and
`DefinitionGraph`, serializable `SandInstance`, projection-specific
`ProjectionManifest`, and non-serializable `RuntimeSandInstance`. Persisted
data contains no DOM, browser, Bevy, GPU or retained-tree handle. Exact revisions
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
Space activates and F9 cycles meaningful visual states. Installed HTML consumes
a Protein-shaped version-1 mount through a strict JavaScript decoder and emits
the granted `record-clicked` event. The live gate first proved that an unknown
field is refused; the Website companion retains network and its own storage
without bridge or style authority. The joined Wayland report passed. The C2
composition host consumes this contract rather than creating another model.

### Landed composition host

`crates/interface/src/composition.rs` is the executable authority for
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

### Landed Configuration, external-author and domain-launch layer

`configuration.rs` layers one versioned Configuration artifact over the exact
composition artifact. It does not add a competing Sand graph. The
Configuration definition is itself a compound of the same Panel, Title and
Button definitions; its 16 exported operation ports reach one workbench state
through keyboard, pointer and AccessKit.

Definition-default, port, Behavior, isolation and capability edits publish new
exact revisions through the same transactional catalog as F10. Workspace and
group style stay in Configuration; instance style stays on the composition
placement. Undo and atomic persistence snapshot both together, so reopening
cannot combine a new theme choice with stale definition or placement state.

The external-author manifest pins a Sand package uid, normalized graph hash
and exact root revisions. Its generated kit carries the Sand/package/message,
Configuration and launch schemas, one accepted ordinary HTML/CSS/JavaScript
package and a deliberately unknown-version manifest. External authors never
need Maud. The host validates the existing package graph, closed module/assets,
capabilities and licenses; the small external manifest selects exact roots and
fails closed rather than negotiating another contract.

A domain launch recipe is a renderer-neutral authoring input, not another
compound kind. It names exact definitions, placements, typed Record reads and
exported Action writes. Materialization creates ordinary composition entries
plus a durable receipt keyed by domain kind and uid. A repeat launch focuses
the receipt's existing placements, so Conversation, Transfer and later Fiote
can open workrooms without bespoke UI constructors or duplicate groups.

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

**Castle** is the picker category for a packaged workflow made of Sands. Basic
Sands and Castles have separate picker sections. A Castle remains a saved
compound Sand; it introduces no fourth object alongside Sand,
group, and definition. A Button Sand may be placed directly in Box or
referenced as a child of a Video Call Sand; the child is not copied or changed
into a special castle component. A local group can be locked for movement,
saved as a reusable compound definition, or forked without changing the
composition semantics. Protein result templates use the same recursive group
shape rather than a parallel template system.

The 2026-09-06 Interface Record extends movement: a child may be released from
the group's movement while remaining its logical child. Copy and release adds
one appearance of the same bound property. Both disappear with the owning
Protein row, even when placed elsewhere. Release, editor unlock and definition
fork are separate operations. The full behavior is in
[Box](box.md#individual-appearances-and-released-children).

Keep one canonical owner for each child appearance. Movement attachment is
independent of ownership: inherited children retain stable local uids, and an
appearance override records release and placement. Locally added copies have
their own stable child identities. Input routing, event scope, teardown and
permissions follow ownership; coordinates follow attachment. Do not infer
ownership from which rectangle contains a child.

An override targets an exact loose Sand or a child in a stable result
appearance. A Record uid locates candidates but never selects all appearances
implicitly. Protein refresh merges new data with those overrides; it does not
replace the appearance with an uncustomized template. Presentation switches
reconcile bindings and overrides through the
[mapping preview](box.md#choosing-another-presentation).

Each child has a stable identity within its owning definition, local
coordinates, ordering, configuration overrides, and typed connections.
Connections name child ports rather than DOM selectors. A compound definition
may export selected child ports as its own interface; everything not exported
stays internal. This lets the same composition be embedded again without its
parent knowing its internal markup, and lets edit mode draw a complete route
through nested groups.

Input exports are executable runtime routes, not schema-only annotations. A
value bound to a compound's public input is forwarded at each recursive level
until it reaches the declared child input. Defaults are resolved before the
child mounts. Validation follows those aliases to the terminal input and
refuses a second direct or exported binding to the same destination. The C2
video-call fixture proves one Protein value crossing two compound boundaries.

A Protein area's locked result template is one such compound group. Its child
inputs are wired visibly to fields of one Protein result, and Box repeats the
whole group once per row. Children without a field binding remain ordinary
presentation, controls, or Behavior inside that template.

Definitions are referenced rather than copied. Built-in and user-owned
definitions update their instances live, with instance changes represented as
override patches. Fork explicitly creates an independent definition. The UI
calls movement detachment Release from group, never Fork.
External executable Sands are content-hash pinned and never update silently.
An invalid or incompatible local definition update fails closed: existing
instances keep the last known-good revision and show the authoring error until
the definition is repaired. They never silently switch to broken content.

#### One composition, direct Bevy authoring

Code-built Sands use ordinary Rust and Bevy scenes, including `bsn!` where
useful. A Button is a Bevy control with Sand identity and typed effects when
it is independently composable. A Castle combines the same controls,
relationships, configuration and exported ports used in Box.

For example, call controls combine mute and camera buttons, a device picker
and scoped media state. Their Bevy observers invoke the same declared effects
that Box can connect visually. The controls do not require HTML fragments,
JavaScript modules, a portable view tree or per-widget host messages. Media
implementation remains a separate, late content capability.

A code-owned definition is not rewritten into Rust when edited in Box.
Instances may carry overrides, and Fork creates a user-owned definition with
its own lineage. Saving a group makes its editable component data,
relationships and bindings reusable; it does not create a second Castle
runtime. New native definition and package formats may be Bevy-specific.

Persist authored values and stable references, not subscriptions, closures,
live Bevy entity ids or GPU allocations. Registered effects identify trusted
implementations; a saved callback is not serialized machine code. Validate
loadable components and remap references on restore. A reusable native scene
and an external executable package have different trust requirements.

#### Native presentation

Bevy UI supplies ordinary layout, widgets, text, focus and editor surfaces.
Bevy cameras, meshes, materials, retained gizmos and rendering systems supply
Box geometry, connections, Clock spirals and specialized views. They share one
Bevy application rather than separate native UI and world adapters.

Lince's tokens and style scopes drive those components and materials. A
focused Sand may expose a richer view while preserving identity, bindings and
editing state. Effective device scale, clipping and text quality remain
requirements; a texture per Sand is not assumed.

Logical ownership is separate from movement attachment. A released child can
move independently while its Protein row still owns its identity and
lifetime. Implement this in Bevy relationships and systems; the transform
hierarchy must not accidentally become the ownership or event-authority model.

#### Existing external authoring and publication

Existing HTML, Maud and package formats can have a translator at import,
export or publication. Their stored child ids, bindings, capabilities and
lineage remain meaningful where retained. Native constructors do not have to
produce a parallel external projection.

For a chosen external-browser format, map stable child ids to instance-local
DOM ids, repair label/ARIA references, validate messages and isolate untrusted
code. Ordinary private markup is not a Sand. These are requirements on that
boundary, not on first-party Bevy components.

Installed HTML has no execution path inside Lince. Preserve inspected
metadata and honest unavailable references. End-of-v1 browserless designs or
explicit system-browser handoff are separate from native authoring. Public
Facade admits an explicitly supported read-only subset rather than demanding
universal native/HTML parity.

#### Renderer and execution bindings

Native Sands use Bevy events, messages, observers, systems, assets and
resources for in-process work. Do not create portable mount/resize/dispose
messages or adapter selection for an ordinary widget. Retire subscriptions
and owned resources through the actual Sand/plugin lifetime.

Use Bevy's first-party line/curve, text, layout and material paths first.
Custom Bevy plugins, internal/external crates or pure WGPU passes are allowed
for a specific Lince need. A WGPU pass normally shares Bevy's device,
rendering lifecycle and final frame. No Sand owns an independent application
loop or GPU engine by default.

Runtime meshes, font caches, contacts and GPU buffers are disposable; authored
identity, relationships, overrides and bindings survive their replacement.
Virtualization may retire visual detail without stopping Protein, events,
media, Area behavior or admitted simulation.

A serialization/ABI boundary is needed for the existing backend, saved files,
networked workspaces and genuinely external code, not between every native
component. Validate versions, identity, bounded payloads, ports, capabilities,
lifecycle and Action requests there. Never pass raw pointers, credentials,
unrestricted World access or device handles to untrusted code.

First-party Rust has process authority and is not sandboxed by Bevy plugin
registration. Backend permissions remain the enforcement point. Untrusted
installed behavior and its isolation design are explicitly deferred. Current
native extension work is editable composition of registered components/effects
and trusted Rust plugins. A manifest cannot make native-library loading safe.
Wasm is a possible later execution choice, not a Sand-authoring dependency.

#### Behavior modules and event composition

Built-in Behavior uses Bevy systems and observers. Common effects are
registered typed operations that Box can inspect, connect and persist;
richer trusted behavior may be ordinary Rust. Native calls do not need a
generic module lifecycle.

Existing JavaScript is relevant only to a selected external-browser/export
boundary, with content hashes, declared ports, state and capabilities, scoped
mounting and teardown. It is not the language of new native Sands.

Common wiring should remain declarative whenever possible. Emitting an event,
invoking one typed Action, toggling local state, selecting a value, or mapping
one typed field needs no new interpreter or arbitrary executable source. These are inspectable
built-in Behavior nodes that Box can draw, validate, copy, publish safely, and
explain. A custom Bevy system or observer is for a genuine transform, state machine,
media controller or interaction that the effect registry cannot express cleanly. Its ports and capabilities stay visible even
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
- `pressed -> effect(toggle_audio)` invokes the registered native media behavior;
- several connections may fan out from one port, with explicit deterministic
  ordering where order matters.

The following JSON sketches explain persistent binding meaning only; they
are not a required new native ABI or exact Bevy scene format.

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

Neither DOM nor Bevy pointer bubbling is the composition bus. A child event enters the declared route,
stays within the owning group unless exported, and is attributed to the
definition, child, instance, and triggering input. Actions remain host/engine
operations; external code can request one only through its explicitly
granted Action handle, and native requests still undergo backend checks. This makes the event arrows seen in Box correspond to
the runtime route rather than merely documenting incidental hierarchy bubbling.

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
          "implementation": "lince.media-controls",
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
      "assets": []
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
Live Protein changes update the affected Bevy components and appearance state.
One changed row must not rebuild every result or erase local overrides.

Correctness requires stable ids, explicit ownership and movement attachment,
validated persistence, exact reusable-definition references, scoped effects,
teardown, permission checks and visible fork/override behavior. It does not
require paired Maud/schema constructors or JavaScript validators for
in-process Bevy calls.

Start with Bevy's retained assets and ordinary systems. Measure before adding
specialized batching, dirty buffers or visual caches. Virtualize presentation
without stopping off-camera behavior or erasing Sand identity. Untrusted
external content keeps its security boundary and declared ports; a shared
Bevy World is not an isolation mechanism.

There are three state planes, never one ambiguous shared bag:

- durable shared truth is Ledger data reached through Protein and Actions;
- persistent interface configuration is Box host state;
- cursors, presence, transient events, media, and sessions are ephemeral lanes
  or explicit host capabilities.

Attached children move together; all logical children shelter their internal
events even when released. Crossing an ownership boundary requires an
explicitly exported typed port. Copying preserves definitions,
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
label, and save them as the repeated result-template group. Attached children
move together; an explicitly released child moves independently while keeping
its result ownership. Later forces use that same attachment boundary. Pulling a Record because
of associated Karma, or composing Karma and Transfer projections into that
row automatically, is a preserved later expression rather than part of the
first area implementation. Configuration is preferred over generating another
bespoke component when one direct change can meet the Need.

#### A Sand that restates a kernel type, and why its JavaScript is untested

Recorded 2026-08-31, while adding the `set-quantity-where` consequence.

The Karma sand's rule builder does not merely display a rule — it *composes*
one. `consequencesFrom` in `app/recurrence.js` and `consequenceFrom` in
`app/builder.js` turn form fields into the tagged JSON that
`nucleus::karma::Consequence` deserializes: which kinds exist, which fields
each kind takes, which of them are required, and what a blank amount means.
Every one of those facts is already stated in Rust, in the enum itself and in
`Consequence::validate`. The sand states them a second time, in another
language, with no link between the two.

That duplication is what made a one-variant change touch five files. It is
also what produced `crates/web/tests/karma_sand_js.{rs,mjs}` — a Rust test
that shelled out to `node` because the composition logic it needed to check
was unreachable from Rust. Those two files are deleted. Keeping a Node process
in the test suite to check a mapping that should not exist was paying twice for
one mistake, and the suite passed silently when `node` was absent anyway.

The deletion cost more than the consequence mapping, and the rest of it was
collateral rather than intended. The same file was the only cover for
`blocksIn`, `activeQuery`, `rankBlocks`, `insertAtCaret`, `completionFor` and
the canvas card selection — the caret-and-autocomplete machinery in
`app/blocks.js`, which is genuinely interface logic with no backend it could
move to and no Rust equivalent to check it against. Around 570 lines of checks
went with the ~120 that were about consequences. That machinery is now
unchecked and will stay unchecked; it is the price of not running a Node
process in the suite, and it should be spent knowingly rather than
rediscovered.

So the mapping is now unchecked, deliberately. What still holds it honest:

- The sand's Rust tests assert the rendered markup offers a control for every
  consequence kind (`every_consequence_a_rule_can_carry_is_authorable`), so a
  kind with no way to author it is still caught.
- The engine refuses a malformed consequence at the Action boundary. A wrong
  shape from the sand fails loudly at save time, on the person's screen, rather
  than being written and discovered later.

That historical JavaScript repair is superseded for native work: the Bevy
form uses the existing Rust domain types/validation or backend-provided
schema at the real transport boundary. It does not recreate them in another
language or add Wasm merely to call Rust.

The earlier alternatives below remain historical, neither attempted there:

- **The backend serves the consequence schema** — kinds, fields, required-ness,
  and what an omitted field means — and the form renders from it. Adding a
  variant then reaches the UI with no JavaScript edit, and there is nothing
  left in JS to test.
- **Composition moves into WASM**, so the form calls the same Rust the engine
  validates with.

Those alternatives explain the old Web maintenance problem, not a task to
edit the museum crate. New Bevy controls require focused Rust behavior tests
and real backend refusals; the unchecked historical JavaScript is not a new
native testing policy.
