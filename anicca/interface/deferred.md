# Deferred v1-adjacent interface work

Purpose: Preserve offline workspace replication, richer Protein, portals, and
topology extensions beyond the current Box plan.

Owner source: [Interface in Lince](../Lince.lingua).

Status: Explicitly deferred and unplanned.

Read when: the owner reopens one of these subjects; do not load for ordinary v1 work.

[Corpus map](README.md) · [Current context](current.md)

---

### Deferred workspace replicas and failover

The first live collaborative workspace is no longer deferred. Its canonical
host, durable Box transactions, spatial checkpoints and one Protein
Synchronization surface are specified in
[Box](box.md#live-workspace-collaboration). This section retains only what that
bounded live mode deliberately does not claim.

Future workspace replication may synchronize one Person's devices, let named
People across Organs edit while the host is unavailable, or let an Organ own a
replicated workspace. Replica members could work offline and later converge;
a surviving authorized member could explicitly replace a lost host. This is
not whole-file synchronization and is not implicit promotion of a guest cache.

The public surface still belongs beside live workspace sessions, Record/Organ
sync and File projection in Protein's Synchronization area. It may offer live,
replica and fork as visibly different policies. Box state does not become
Records, and disk and wire may encode the same typed semantic operation
differently.

The live protocol already establishes stable ids, actor attribution, atomic
transactions, canonical revisions, snapshots, ordered tails, permissions,
limits and recovery. Offline replication additionally requires:

- a defined causal and deterministic merge for instances, overrides, groups,
  ports, zones, drawings, portals, topology strokes, spatial checkpoints and
  definition references, with unknown versions failing closed;
- an explicit owner/editor/viewer replica grant, leave, revocation, host
  replacement and a separate permission for editing a reusable Sand
  definition whose other instances may live outside the shared workspace;
- encryption of composition and assets to current members, key rotation for
  future state after membership changes, and no promise to erase replicas a
  former member already received;
- semantic handling for simultaneous physics, topology, definition and mode
  conversion rather than arbitrary field-level last-writer wins;
- dependencies, tombstones, offline replay, compaction, garbage collection,
  resource ceilings, missing-definition/asset recovery, and replica fork
  lineage;
- ephemeral presence and cursors outside the durable operation log;
- honest offline, divergent, merging, conflict, host-replacement and
  access-lost UI alongside the mechanism; and
- cross-Organ convergence, revocation, malformed/oversized operation,
  snapshot recovery and Protein-projection parity tests.

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

### Deferred topology extensions

The v1 Box now includes one editable base height field, filtered potential
effects, fixed or Sand-attached brush stamps, Area-linked field visualization,
surface Top/Perspective views, one free-space mode and an explicit collapse
projection between them. Their canonical semantics live in
[Box topology editing](box.md#topology-editing-and-effective-terrain). The
following extensions remain deferred so that the first implementation has one
active deterministic simulation at a time and an understandable reference
plane.

The included free-space mode is workspace-wide and does not create several
simultaneous floors or preserve a hidden second physical position. Several
independently physical Sand planes may later coexist. A
viewport-attached hover plane could behave like hockey pucks on an air table:
Sands drift and bounce as an optional screensaver-like mode, react when the
plane is shaken, and follow a gentle hover-plane gravity toward a corner while
the ground plane carries different terrain. Cross-plane collision, portals,
attachment, focus, ordering, and event routing need their own model before
this can be more than an effect.

Advanced rendering may let a wallpaper, shader, or mandala compress and darken
in valleys, deform across slopes, and let raised common terrain cast shadows
onto material at floor level. The current Box already permits bounded pattern
distortion and color per Area/effect; general programmable materials,
arbitrary shaders, deep displacement, self-shadowing, and deformation of rich
Sand content remain later capability work. Sands must stay legible.

Formula editing also remains deferred. When introduced, it should use ordinary
notation and presets rather than a Lince-specific programming language. Its
stored semantic form would be a bounded profile of W3C
[Content MathML](https://www.w3.org/TR/mathml4/#contm) and
[OpenMath](https://openmath.org/standard/om20-2019-07-01/), safe when stored and
rendered clearly when seen. Expressions could not perform I/O, invoke
JavaScript, or allocate unbounded work. A visual editor, deterministic
evaluation limits, field explanations, and CPU/GPU cost gates must precede raw
expressions. Any vendored evaluator or editor carries its license and credits
with the owning Sand.

The terrain direction remains inspired in part by
[this topographic-canvas demonstration](https://youtu.be/-IOLRcFC6OY?si=WW1tMdMmo0NYxcu_),
without requiring Lince to reproduce that particular effect.
