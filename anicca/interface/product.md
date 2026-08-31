# Product boundary and interface guidelines

Purpose: Collect the interface promise, UI guidelines, and collaboration/editor expectations.

Owner source: [Interface in Lince](../Lince.lingua); no separate Customization,
Sands, or Interoperability Record currently exists.

Status: Active product boundary; detailed implementation lives in subject documents.

Read when: changing what v1 exposes to a person or how the base interface should behave.

[Corpus map](README.md) · [Current context](current.md)

---

## Interface scope notes

- [ ] [Sand](sand-model.md#sand): Sand is the recursively composable unit of interface.
  A button, form, Record view, graph, game, or complete workflow may all be
  Sands. Small Sands combine into larger ones without creating a conceptual
  boundary between primitive controls, the Sand store, and workflows. A Sand may be dead
  presentation, carry data and Behavior, or expose typed connections to other
  Sands. Ready-made Sands are bundles, not sealed applications.
  - [ ] Sands are references to definitions, not copied HTML. Changes to a
    built-in or user-owned definition reach its existing instances live while
    preserving instance overrides. External executable definitions are pinned
    and never change silently.
  - [ ] The Sand store grows on demand from ordinary design-system controls to
    specialised workflow bundles; it need not build every generic control
    before a workflow first needs it.
- [ ] [Box: Base Capabilities](box.md#box-workspace-and-canvas): Box is the application host and the
  spatial playground for building, connecting, and using Sands. It is a
  holistic environment rather than a sidebar of separate applications. Its
  canvas, recursive base pattern, Protein areas, spatial behaviors, topology
  editing, surface Top/Perspective views, free-space mode, collapse projection,
  and edit tools make it feel alive while the default remains minimal and
  paper-like.
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
    primitive/compound Sand composition, saved compound Sands, and the non-spatial
    composition workbench are complete before canvas, Protein-area, or
    influence-area work begins.
  - [ ] Workspace state is locally durable before it is collaborative. The
    current Interface completion then adds live-only, host-authoritative
    workspace sharing in Protein's Synchronization surface. Record/Organ sync,
    Workspace transactions and File projection share one human-facing place
    and infrastructure where appropriate, but retain distinct typed semantics.
    Offline editable replicas and automatic host failover remain deferred.
  - [ ] Human-readable authoring documentation must let a non-technical person
    build from existing Sands, query through Protein, invoke Actions, and
    connect behaviors without first learning the Rust implementation.
- [ ] [Highly customizable](customization.md#customization): a person can ergonomically and
  live-edit the appearance, layout, information density, behavior, and
  connections of the Box and its Sands. Simple controls cover padding, gaps,
  thickness, radius, typography, colorschemes, and other common properties;
  an explicit advanced developer mode permits deeper Sand- and workspace-level
  freedom.
- [ ] [Interoperability](interoperability.md#interoperability): Lince coexists with other systems
  through [Blood](../Ontology.lingua#9-federation-and-blood-talking-to-other-systems),
  external Sands, and portable representations. A future open canvas format may
  be adopted only after Lince's own canvas schema and capabilities are stable;
  no external specification may constrain Box features.
  - [ ] Public Facades make genuine data inspection cheap without exposing the
    private publishing Cell to viewer traffic or abuse. Content-addressed
    archive delivery avoids an origin read receipt; a Live Facade deliberately
    trades that stronger network privacy for a real-time scoped Protein stream
    and states the server-visible metadata honestly.
- [ ] **Mobile:** mobile work begins only after the complete desktop
  Sandbox is implemented. Until then, this goal is deliberately not allowed to
  shape or delay the desktop implementation.

## Software archetypes composed from Box

These are design probes, not a promise to turn each application into a sealed
built-in Sand. Lince should reproduce their useful interaction grammar from a
small set of reusable Sands, typed ports, Protein projections, Actions, Areas
and spatial modes. A new primitive earns its place when several archetypes need
it; an application-specific exception does not.

| Software shape | Composition from Lince primitives | Reusable pressure it reveals |
| --- | --- | --- |
| Kanban and task tracker | Protein result groups become cards; sorting Areas form columns; mutation Areas request status Actions; immunity and fixed bounds keep columns stable. | Card template, lane/shelf, status control, assignee display, internal scrolling. |
| Spreadsheet and financial dashboard | Dense table Sands bind Record fields; aggregate Proteins feed totals and charts; Actions edit attributable values; groups package reusable reports. | Virtualized grid, number/date editors, chart primitives, selection ranges and an eventual safe formula capability. |
| CRM and case pipeline | Person, Organ and Record cards share relation ports; force/sorting Areas cluster accounts and stages; a detail Sand follows `record-clicked`. | Relation picker, activity trail, master/detail selection and reusable pipeline presets. |
| Inventory, warehouse and production line | Quantity-bearing groups enter through Protein Areas, travel through slopes or force lanes, branch by filters and request quantity/concept changes at guarded mutation Areas. | Quantity/unit controls, batch admission, route explanation, capacity limits and map/layout views. |
| Node automation and rule editor | Typed Sand ports expose events and Actions; wires and Areas create visible flow; Karma remains the durable rule mechanism rather than hidden canvas JavaScript. | Port inspector, event trace, Action preview/grant, cycle diagnosis and reusable subgraphs. |
| Inbox, help desk and communication client | Protein streams feed thread/message groups; sorting Areas express priority and assignment; a thread Castle combines list, reader, composer and call Sands. | Virtualized feed, thread/message primitives, unread state, media/session adapters and notification policy. |
| Wiki, research notebook and learning trail | Record/text Sands, relation graphs and reader Sands compose knowledge views; Protein chooses chapters while local or attributed Actions record reading progress. | Rich document projection, outline/tree, citations, backlinks, search and progress controls. |
| IDE, terminal and operations console | File, editor, terminal, diagnostics and command Sands exchange typed selections and events inside a Castle; external tools receive narrowly granted capabilities. | High-quality text editor, tree, terminal surface, command palette, diagnostics and explicit process/file authority. |
| Monitoring and control room | Streaming Protein or external adapters feed gauges, charts, logs and topology-aware status groups; Actions remain guarded controls rather than clickable telemetry. | Time-series chart, bounded stream/log view, thresholds, alert explanation and rate/backpressure controls. |
| Presentation, storyboard and document reader | Saved groups form scenes or pages; camera bookmarks and selection events move between them; pinned Sands provide navigation while reader state stays local when appropriate. | Camera bookmarks, ordered page/scene primitive, presenter controls and optional timeline. |
| 3D scene planning and spatial whiteboard | Space mode provides free transforms, volume Areas and floating native/HTML Sands; collapse previews how the composition becomes a surface workspace. | 3D transform gizmos, local frames, volume shapes, picking, snapping and spatial artifact adapters. |
| Simulation and game | Records project as actors; Behavior and fixed-step systems update them; force volumes, collisions and Actions connect simulation consequences back to Lince authority. | Deterministic simulation boundary, input mapping, sprite/mesh/audio Sands, replay and capability-limited game rules. |
| Map, logistics and geographic planning | Location-aware Records, routes, layers and Area filters eventually project through the world frame while ordinary Sands remain usable as map annotations and HUD. | V2 globe/map streaming, disclosure-aware location, route primitives and multiscale spatial indexing. |
| Calendar, Gantt and media timeline | Protein can select dated Records and groups can render entries, but pagination, recurrence and a shared time-axis interaction remain deliberately undecided. | A future bounded timeline/time-ruler primitive; these applications must not smuggle calendar semantics into v1 Areas. |

The strongest near-term primitives across this matrix are a virtualized table,
tree/outline, chart, master/detail selection, camera bookmark, 3D transform
gizmo, Area shelf/lane and an inspectable stream/log. Calendar recurrence,
planetary maps, exact geometry and a general media timeline remain separate
future semantics even though the Box should leave room for them.

## UI guidelines

- Honesty over decoration. "A number on a chart that nobody can explain is worse than no number."
- Surfaces are opaque; line styles carry truth (settled vs. declared); color
  never carries meaning alone. A connection state, for example, has an icon or
  label that remains meaningful when its color is changed.
- A whiteboard, not a cockpit. Sand are lego blocks on a blank canvas — dots, strokes, hand-drawn arrows, blocks the user arranges.
- Familiar, paper-like, user-owned. When the user makes their lince, it feels like they are creating an art piece, the built-in ui should be minimalist to not carry the composition away from the user's intention.
- When a past implementation conflicts with the organised contract, choose the
  clean, elegant, surgical minimum. Legacy behavior has no independent claim
  to survive; preserve the information and user Need, not accidental machinery.


## Collaboration and the editor

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
