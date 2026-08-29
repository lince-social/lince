# Deferred v1-adjacent interface work

Purpose: Preserve workspace sync, richer Protein, portals, and topology
extensions beyond the current Box plan.

Owner source: [Interface](../Interface.lingua).

Status: Explicitly deferred and unplanned.

Read when: the owner reopens one of these subjects; do not load for ordinary v1 work.

[Corpus map](README.md) · [Current context](current.md)

---

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

### Deferred topology extensions

The v1 Box now includes one editable base height field, filtered potential
effects, fixed or Sand-attached brush stamps, Area-linked field visualization,
and 2D/3D lenses. Their canonical semantics live in
[Box topology editing](box.md#topology-editing-and-effective-terrain). The
following extensions remain deferred so that the first implementation has one
deterministic simulation and an understandable plane.

Several independently physical Sand planes may later coexist. A
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
