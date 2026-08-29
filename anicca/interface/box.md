# Box, Protein areas, and spatial behavior

Purpose: Specify the v1 Box, current-Protein result templates, Areas,
editable topology, 2D/3D field views, interaction, navigation, and local
durability.

Owner source: [Interface](../Interface.lingua); no separate Sands Record
currently exists.

Status: Planned; implementation opens only after the Customization C5 gate.

Read when: working on the spatial product after composition foundations pass.

[Corpus map](README.md) · [Current context](current.md) · [Interface plan](plans/interface.md)

---

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

Protein admission has an explicit cadence and placement policy. The runtime
always materializes one complete semantic result group atomically before
physics can act on it; a half-built group never exposes children to different
forces or renders as though it were a valid result. Presentation may then use
one of three policies:

- **travel from spawn** places the completed group at the configured point or
  region, activates physics immediately, and reveals its route;
- **pre-settle then reveal** runs the same deterministic simulation out of
  view or as an explicit ghost until it settles or reaches a visible work
  budget, then reveals the result at that position; and
- **place directly** asks sorting/constraint areas for the deterministic
  destination and presents the group there without pretending an animation
  occurred.

The Protein area also controls whether rows enter together, in bounded
batches, or at a visible rows-per-second cadence. These choices change only
admission and presentation. They do not change the Protein order, duplicate
Records, or let rendering race semantic construction. A pre-settle budget
that expires reveals an honestly unsettled group and lets ordinary simulation
continue. Travel-from-spawn supports visible production lines; pre-settle or
direct placement supports a Kanban-like surface whose cards initially appear
in the correct columns.

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

### Topology editing and effective terrain

In the v1 Box, **topology** means an editable scalar height or potential field
over the logical plane. It is closer to sculpting literal sand than to the
mathematical study of connectivity. It gives the same force system a spatial
shape that can be manipulated and understood from above in 2D or from an
oblique camera in 3D.

Topology does not replace or silently change the four Area semantics. Protein
still selects data, force areas still combine forces, sorting areas still
order, mutation areas still request Actions, and immunity still decides which
effects are eligible. Topology adds two related things:

1. a common authored terrain field that may affect every eligible group; and
2. filtered **Topology Effects** whose potential applies only to groups that
   match their declared current-Protein filter.

For a group `g` at `(x, y)`, the runtime evaluates a common base field
`B(x, y)` and the matching effect field `P_g(x, y)`. The topology force is
the downhill gradient of their sum:

`F_g = -k_g ∇(B + P_g)`

A negative stamp therefore creates a pit that attracts matching groups. A
positive stamp creates a mountain whose slopes push them away. Strength,
height or depth, radius, steepness, falloff, and top flatness are separate
controls. Increasing flatness moves a smooth hill toward a plateau or
cylinder-like profile without requiring a person to type an equation.

The field is evaluated at the complete group's body. Children are not torn
out of a Castle or result template. Collision, settling, manual dragging,
immunity, and the existing deterministic Area order still apply after the
topology force is calculated.

An Area may opt into a topology visualization for its force. Radial attraction,
radial repulsion, and directional gravity have scalar potentials and can be
shown exactly as pits, hills, or slopes. That visualization does not apply a
second force: the Area remains the one behavioral source, and its potential is
an explanation of the force already being used. Sorting, mutation, hard
constraints, and future rotational or otherwise non-conservative forces cannot
always be represented honestly by one height field. They remain lanes, gates,
boundaries, arrows, or overlays unless the person separately adds a Topology
Effect.

Edit mode supplies topology brushes rather than formula entry in the first
version. Circle, square, ridge/line, flatten, smooth, raise, and lower tools
produce compact editable stamps. A stamp records its transform, extent,
signed height, steepness/falloff, plateau amount, blend strength, filter,
visual style, and ordering. The rendered mesh and pattern displacement are
derived artifacts, never the authoritative editable data. Stamps can be
moved, resized, reordered, copied, disabled, inspected, undone, and removed.

A world-anchored stamp remains at its Box coordinates. A Topology Effect may
instead be anchored to a Sand or group. Its local transform then follows that
anchor, so moving a Sand carrying a small hill pushes matching nearby groups
away and moving one carrying a pit attracts them. The anchor does not act on
itself unless that self-effect is explicitly enabled. Cycles, extreme
gradients, oscillation, and excessive affected-body counts use the same
bounded pause, explanation, and recovery rules as Areas.

There is no single universal visible surface when filters make different
groups experience different potential fields. Box therefore has explicit
field lenses:

- **base terrain** shows the common field;
- **selected Sand/group** shows the effective field that this particular body
  experiences;
- **selected filter/effect** shows the field for that cohort; and
- **neutral overview** keeps the base plane readable and shows filtered
  influences through contours, arrows, boundaries, color, and pattern
  distortion rather than pretending they affect everyone.

In a selected-field 3D view, matching groups can sit visually on their
effective surface, making a collected set appear inside a pit or on a plateau.
Nonmatching groups remain on the neutral/base plane or are visually subdued.
In 2D, the same state is seen from above: the groups have the same `(x, y)`
simulation positions, while contours, gradient arrows, color, and distorted
pattern communicate the field. Tilting between 2D and 3D is a camera and
explanation change; it never reruns the simulation under different rules or
persists a contradictory second position.

The Sand definition, instance, local state, ports, and Behavior are identical
in both views. A renderer projection may present an ordinary card as a crisp
screen-facing or gravity-upright surface in 3D so text stays readable, while
specialized Sands may deliberately lie on terrain or use a native 3D
projection. Changing projection never creates a second Sand instance. Pinned
viewport Sands remain HUD-like and do not fall into world terrain.

Pointer interaction in 3D raycasts into the selected plane/lens and resolves
back to the same logical `(x, y)` coordinates used in 2D. Dragging a Sand
makes its group body kinematic for the gesture; release restores its declared
forces and settling. Dragging a Sand-attached Topology Effect moves its field
with it and updates affected neighbors during the gesture. The topology brush
edits the hit plane, while ordinary select/move mode continues to manipulate
Sands, so one gesture cannot accidentally do both. Camera tilt/orbit preserves
selection, focus, active editor, and field lens.

Every Area and Topology Effect may customize its boundary, color, opacity,
contours, pattern, and how strongly it distorts the canvas pattern. Visual
style is independent of physical strength: making a pit darker does not make
it stronger. Edit mode can show the composed result as well as isolate each
contributor, and **Why is it here?** lists the sampled height, gradient,
matching filters, immunity decisions, direct Area forces, topology forces,
sorting constraints, and collisions that produced the current motion.

This allows a Box to become a spatial circuit. A Protein area can admit groups
at one point or across a region; a common slope can carry them forward;
filtered effects and force areas can bifurcate them; constraints can form
lanes; and terminal pits can collect different cohorts. The same circuit can
show movement from a central spawn, pre-settle groups before revealing them,
or directly place them into a flat Kanban-like arrangement.

For correctness, the semantic source is the ordered set of compact stamps,
filters, Area configurations, and fixed-step inputs. CPU evaluation first
samples the field and gradient for active bodies deterministically. For
performance, the renderer may cache affected tiles, generate the visible mesh,
contours, normals, shadows, and pattern displacement on the GPU, and update
only dirty tiles. GPU physics is not required until measurements identify a
specific field kernel that wins without readback or synchronization debt.

### Canvas base pattern

The canvas pattern exists on both a flat and a topologically deformed plane.
Its generated form is a recursive level-of-detail grid: zooming in reveals
finer repetitions of the pattern and zooming out removes detail before it
becomes visual noise. This is the familiar self-revealing canvas-grid effect,
not a requirement for Mandelbrot computation.

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
after these operations are stable. Topology editing uses its own visible
brush/lens mode so sculpting the plane cannot be confused with moving a Sand
or drawing ordinary Box content.

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
influence areas, topology stamps/effects, drawings, and other durable authored
entities. An instance
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
