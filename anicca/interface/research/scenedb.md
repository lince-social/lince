# SceneDB 2.0 and EngineFS review

Purpose: Evaluate SceneDB as a runtime and persistence foundation for Lince's
2D/3D Box, and evaluate EngineFS as inspiration for local saving, File Sync,
contact sync, and live workspace collaboration.

Status: Research conclusion accepted for the agent-owned interface plan. This
review does not adopt either project as a dependency.

Read when: designing Box persistence, spatial runtime storage, workspace
collaboration, scene streaming, or synchronization APIs.

[Corpus map](../README.md) · [Box](../box.md) ·
[Runtime architecture](../architecture.md) ·
[Interoperability](../interoperability.md)

---

## Sources and scope

Read together on 2026-08-29:

- [SceneDB 2.0: The Cross-Device Spatial Database](https://pulsarnative.com/blog/2026-07-17-scenedb20-cross-device-spatial-database),
  whose page reports publication on 2026-07-23 despite the 2026-07-17 URL;
- [SceneDB 2.0 & Helio — Unified Engine Specification](https://pulsarnative.com/Research/doc/?section=drafts&slug=scenedb20),
  revision 2.1, May 2026;
- [There Is Only One Copy: Why SceneDB's ECS Looks Nothing Like an ECS](https://pulsarnative.com/blog/2026-08-26-state-of-scenedb),
  whose page reports publication on 2026-08-27 despite the 2026-08-26 URL;
- [EngineFS: Pulsar's Virtual Filesystem Layer](https://pulsarnative.com/blog/2026-06-26-pulsar-engine-fs),
  published 2026-06-26; and
- the public [SceneDB repository](https://github.com/Far-Beyond-Pulsar/SceneDB),
  inspected at commit `cebb1372592b229d48aadcddd6cb3a19dafd6530`
  from 2026-08-25.

The articles and specification are first-party architectural explanations, not
an independent audit. The repository is the strongest evidence for what can
actually be reused today, but it is young and changes faster than the prose.
Claims here distinguish the described design, the inspected implementation,
and the conclusions Lince should draw from them.

## Executive judgment

SceneDB is a strong source of runtime techniques for keeping many spatial
objects dense, queryable, synchronizable to a GPU, and safe under relocation.
It is not a durable Box database, a physics engine, a collaboration system, or
a general scene-authoring model. Lince should borrow its ownership law, stable
runtime indirection, structure-of-arrays layout, dirty tracking, explicit
frame boundaries, and sparse/full synchronization crossover. Lince should not
make persisted Sand identity, Box documents, collaboration, or authority
depend on SceneDB handles or snapshots.

EngineFS demonstrates the ergonomic value of one caller-facing storage API and
replaceable backends. Its file-operation abstraction is too weak to be the
common semantic protocol for Lince File Sync, Organ sync, and collaborative
Box editing. Lince should provide one Synchronization product surface and a
shared service vocabulary for status, invitations, permissions, cursors, and
recovery, while retaining distinct typed lanes for Records, workspaces, and
file projections.

The first collaborative Box should be **live-only and host-authoritative**.
The host owns the durable Box store and the one active physics result. Guests
send editing intents; the host validates and orders them, persists canonical
operations and spatial checkpoints, and streams the accepted result. Offline
multi-writer replicas are deliberately excluded from the first collaboration
version. This avoids pretending that nondeterministic physics, topology brush
strokes, definition edits, and ordinary record merge all have the same
conflict rule.

## What SceneDB actually is

SceneDB's central rule is that scene data which must survive a frame belongs to
SceneDB. A renderer such as Helio owns only resources derived from that scene
state: pipelines, bind groups, visibility results, indirect commands, and
other replaceable GPU products. This is best understood as one **logical
mutation authority**, not literally one physical copy. CPU columns, GPU
buffers, replication deltas, caches, and renderer products still exist, but
only one layer is allowed to define the persistent scene meaning.

The runtime packs a slot and generation into a 64-bit handle. A slot registry
maps that stable runtime handle to the current dense row. When a row moves
during swap-and-pop compaction, the registry changes while the handle remains
usable. Generation checking prevents a retired handle from accidentally
addressing a later object that reused its slot. CPU and GPU use the same slot
indexing convention, which makes validation cheap in shaders.

Data is stored in aligned structure-of-arrays pages. Hot numeric columns are
suited to SIMD queries and GPU transfer; generic heap-backed columns remain
CPU-only. Dirty masks and dirty ranges let the GPU receive only changed fields
and rows. The implementation can choose sparse range upload for ordinary
changes and a full upload when mutation becomes dense enough that range
bookkeeping loses.

Mutation and relocation are organized around explicit phases. Simulation may
change fields, then a frame boundary retires dead handles, compacts rows,
repairs mappings, harvests changes, and synchronizes derived resources. GPU
resources retire by completed submission serial rather than a guessed number
of frames. Query results are per-frame row tokens and are invalidated by the
next relocation boundary.

Spatial storage adds cells, broad AABB/frustum scans, SIMD paths, multiple
observers, hysteresis, pins, and nested levels of detail. The later source also
describes a warm in-memory tier between visible/active data and cold storage.
This is scene residency and render preparation, not a collision broad phase or
a durable geographic database.

## How the three SceneDB texts evolve

The formal May specification is primarily a storage/render contract. It
defines handle safety, pages, mutations, frame phases, synchronization, spatial
queries, streaming, and Helio ownership. It explicitly does not specify a
durable file, crash recovery, collaboration, or a network protocol.

The July article explains those mechanisms more expansively and reports large
advantages when only a small part of a scene changes. Its illustrative
100,000-object, 0.1%-mutation case reports roughly a thousandfold reduction in
uploaded bytes and about a sixfold CPU improvement compared with uploading the
whole scene. The same measurements show the important boundary: at 100%
mutation the delta path loses, so Lince must measure and switch strategies
rather than declaring sparse synchronization universally faster. The article
also reports SIMD query improvements that vary substantially by query and
platform.

The August state article adds a replication-oriented vocabulary around the
runtime. Fields can independently opt into GPU transfer and replication.
Writes feed a change tracker, which can produce deltas for relevance sets,
authority partitions, event batches, snapshots, reconciliation, and
subscriptions. The article is careful about what remains outside SceneDB:
transport, encryption, authentication, connection lifecycle, asset streaming,
anti-cheat, editor operational transformation or CRDT behavior, locks, and
undo history.

The repository confirms useful pieces of this later design, including bounded
binary encoding for deltas, tests, and snapshot types. It also exposes the
current persistence limit. A spatial snapshot records cell indices and live
row contents, then allocates new handles when restored; the original spatial
handle bits are discarded. Snapshot values do not currently have the same
wire/disk encoding API as deltas, and there is no atomic durable store,
journal replay, checksum chain, or crash-recovery protocol. SceneDB snapshots
are useful replication/runtime captures, not stable Lince workspace files.

The repository and prose differ in smaller evolving details such as tiers and
buffer behavior. That is normal for active research, but it means Lince cannot
freeze its public semantics around this version of SceneDB.

## What SceneDB gets right for Lince

### Runtime ownership

Lince should keep the same one-authority discipline across three state planes:

| State plane | Authority | Examples |
| --- | --- | --- |
| Durable semantic workspace | Lince Box store | Sand and Area uids, definitions, bindings, topology operations, durable placement checkpoints |
| Active simulation | Lince spatial runtime behind its adapter | Current transforms, velocities, contacts, forces, solver state |
| Derived presentation | WGPU/Bevy/CEF adapters | Dense handles, GPU buffers, visibility, terrain mesh, bind groups, textures, draw commands |

The active runtime is authoritative for the current in-session physical state.
The durable checkpoint is authoritative only for recovery and collaboration at
its committed revision. The renderer never becomes authoritative. This is
more precise than saying that the same truth exists simultaneously in all
three layers.

### Stable semantic ids over disposable dense handles

Every durable workspace, Sand, group, Area, topology operation, definition,
asset, connection, and Protein binding receives a stable Lince uid. The hot
runtime maps those ids to generation-checked dense handles. A renderer or
physics adapter may compact, rebuild, or replace its handles without changing
Box meaning. Neither a Bevy entity nor a SceneDB slot may cross the Box file or
collaboration boundary.

### Hot and cold representations

The authored topology is an ordered set of compact stamps and effects. A
tessellated surface, normal field, collision proxy, potential tile, GPU
texture, and culling structure are derived caches. They can be partitioned and
rebuilt by dirty tile. The same rule applies to Sands: definitions and stable
placements are durable; instance buffers and visibility lists are derived.

The v1 Box does not need planet-scale streaming, but its local spatial frame
should not prohibit later cell/chunk residency. SceneDB's observer union,
hysteresis, pins, and proxy tiers are useful v2 repertoire once a real globe
workload exists.

### Explicit phase boundaries

Lince already needs a fixed simulation step, input coordination, topology
updates, CEF texture import, native layout, GPU submission, and retirement.
SceneDB reinforces the value of declaring when each kind of mutation may
occur. A Box persistence boundary can harvest a stable spatial checkpoint only
after a completed simulation step; it must not sample half an Area update or
half a topology edit.

### Sparse work must stay sparse

Moving one Sand should dirty that Sand's transform, affected spatial cells,
and relevant topology tiles, not cause a full scene upload or a full Box file
rewrite. Conversely, loading a workspace or changing nearly everything should
use a bulk path. The crossover is measured independently for CPU-to-GPU
upload, durable checkpoint encoding, and network delivery.

## What Lince must not inherit

SceneDB's shared-authority conflict rule chooses the higher client id for a
contested field in a frame. That is simple and deterministic, but arbitrary for
human composition. It cannot decide whether two topology brush strokes should
merge, whether a group edit invalidates a child drag, whether an Area deletion
cancels a physical checkpoint, or whether changing a Sand definition should
rewrite every instance.

Delta application in the inspected implementation does not itself reject an
older frame. A host must track ordering and refuse stale or duplicated input.
Compressed delta caches also assume reliable in-order acknowledgement; a gap
requires snapshot recovery. Lince must place revision, ordering, validation,
and recovery in its own protocol rather than relying on an implied transport.

SceneDB is not a physics engine. Its spatial cells and scans must not be
confused with Avian collision detection, force integration, constraint solving,
or Lince's per-group effective topology. Adopting SceneDB beside Bevy and Avian
without one explicit owner could produce three partly duplicated worlds and
undo its main benefit.

SceneDB also does not provide Box schema validation, access control, undo,
assets, external HTML authority, text/accessibility, Protein reconciliation,
or crash durability. Those are not small integration details; they are most
of the Lince product boundary.

The immediate Lince workload is also not yet evidence for replacing its
current runtime storage. The correct approach is to build the Box workload,
measure where Bevy/Avian extraction and storage cost lands, then adopt a
SceneDB-like hot store only if it removes a demonstrated copy or traversal
bottleneck.

## Durable Box state

The user-visible requirement is stronger than the earlier optional resting
hint: when a Sand travels away from its Protein spawn Area and settles in an
influence Area, reopening Lince must restore it there. Its last durable
spatial checkpoint is therefore part of recoverable Box state.

### What is stored

The durable Box document stores:

- stable ids and revisions for the workspace, Sand definitions and instances,
  groups, connections, Areas, Protein bindings, topology operations, and
  content-addressed assets;
- the active spatial mode and collapse-plane frame;
- surface placements as logical surface coordinates and local orientation, or
  free-space placements as full transforms;
- authored anchors, direct-manipulation results, layers, ordering, sizes,
  configuration, locks, and relative child transforms;
- ordered topology stamps/effects and Area geometry, filters, force/sorting/
  mutation policy, projection, appearance, and evaluation order;
- coalesced spatial checkpoint batches tied to the exact Box revision and
  simulation mode from which they were harvested; and
- an explicit resume policy when velocity or other continuation state is
  intentionally durable.

Area membership, height-field meshes, world-space positions derived from a
surface coordinate, normals, broad-phase structures, GPU handles, visibility,
solver contacts, bind groups, pipelines, and CEF textures are rebuilt. A
restored surface Sand first receives its durable logical surface position,
then the current topology derives its 3D contact position. This prevents a
baked world-space coordinate from disagreeing with edited topology.

Most work-oriented Sands resume at rest. A game or explicitly continuous
simulation may opt into a bounded continuation checkpoint containing velocity
and angular velocity. Solver caches are never persisted; continuation is an
approximation from a verified step, not a promise of bit-identical replay.

### Snapshot and journal

The Box store uses a compact readable snapshot and a typed append journal. A
snapshot names its schema version, workspace uid, canonical revision, content
hash, definition/assets manifest, and latest durable spatial checkpoints. The
journal records atomic semantic transactions after that revision. Startup
loads the last verified snapshot, replays complete valid transactions, ignores
an interrupted tail, and fails closed on an unknown operation or schema.

The journal has two explicit categories:

- **authored operations** preserve intent and undo, such as moving a Sand,
  changing an Area, applying a topology brush, grouping, binding Protein, or
  changing a definition; and
- **spatial checkpoints** replace older recovery state for a bounded set of
  bodies after simulation, without pretending every physics tick is an
  authored command.

Both use stable ids and canonical revisions, but checkpoint compaction may
drop superseded transform batches without deleting authored history. A
checkpoint derived from an older topology/Area revision can never outrank a
later authored operation.

A conceptual transaction envelope contains:

```text
workspace_uid
transaction_uid
actor_cell_uid
actor_organ_uid
base_revision
canonical_revision
hybrid_logical_time
cause
operations[]
```

This is vocabulary, not a frozen serialization. The Box grammar and binary
wire framing remain separate decisions.

### Commit policy

Lince does not write at display or physics frequency. It commits direct
manipulation when the gesture completes, topology and Area edits as atomic
semantic transactions, and simulation state as coalesced batches. A body is
eligible for a durable checkpoint when it settles, crosses into a stable
effective destination, or has remained continuously active beyond a bounded
maximum recovery age. Clean shutdown performs a final bounded flush; crash
recovery may lose only the visibly stated checkpoint window, never the last
completed authored transaction.

The actual settle interval, movement threshold, maximum checkpoint age,
batch size, fsync cadence, and compaction threshold are benchmark parameters,
not architectural constants. The runtime-health surface must report pending
durability and the possible recovery lag.

## EngineFS review

EngineFS presents one global, replaceable filesystem provider with familiar
read, write, create, delete, rename, list, directory, existence, metadata, and
manifest operations. Local, HTTP, and peer providers can occupy that slot.
Callers do not know which storage backend is active. Writes emit events with a
local/remote source marker to avoid simple echo loops, and a remote manifest
can reduce repeated network listings.

That is attractive for development: one API, mockable providers, centralized
path checks, consistent events, and backend selection at project open. It is
also too file-shaped and too global for Lince's complete problem:

- remote calls are synchronous and may block the caller;
- the one-provider model does not naturally express several open workspaces,
  local durability plus peer delivery, or different asset backends at once;
- file overwrite and rename do not carry semantic transaction identity,
  preconditions, authority, atomic multi-entity intent, conflict meaning,
  ordered replay, tombstones, or snapshot-gap recovery; and
- treating a peer as the filesystem provider confuses where bytes are stored
  with who is allowed to define workspace truth.

Lince's current File Sync is already semantically different. It projects
Record files in a chosen directory into Actions/Ledger changes and projects
the Ledger back to disk. Disk wins for that projection and deletion is
debounced. Organ synchronization uses typed field operations, authenticated
peers, authority, durable delivery, and idempotent reconciliation. Neither is
well represented as the other's filesystem backend.

## One synchronization experience, separate lanes

The user should find every kind of synchronization in Protein's
Synchronization area. That surface can list Record/Organ synchronization,
live Workspace collaboration, and File projection together; invite a contact;
show owner/editor/viewer authority; expose freshness, recovery lag, errors and
limits; and answer whether a change is local, delivered, or waiting.

Underneath, three protocols retain their real semantics:

| Lane | Canonical unit | Authority and merge |
| --- | --- | --- |
| Record/Organ sync | Existing typed record field operations | Existing Organ/cell grants and field reconciliation |
| Workspace live collaboration | Box transactions, spatial checkpoints and snapshots | One authoritative live host validates and orders edits |
| File projection | User-selected files mapped to/from typed domain changes | Local projection policy; never a peer collaboration protocol |

A shared instance-scoped synchronization service may expose concepts such as
source registration, status, invitations, permissions, subscriptions,
delivery cursors, snapshot requests, content-addressed blobs, cancellation and
health. It must not erase the lane-specific operation schema. This is the
useful part of EngineFS's uniformity applied one level above raw filesystem
verbs.

The existing authenticated contact transport, identity, grants, delivery
queue, and status infrastructure should be reused. Workspace messages should
use a distinct versioned protocol/ALPN or an equally explicit tagged channel,
not variants smuggled into the current Record operation enum. Unknown message
or operation versions fail closed; Lince does not negotiate down.

File Sync remains a projection adapter. If a person exports or watches a Box
file, it validates whole replacement transactions, reports structural diffs,
and never publishes half-written journal bytes. Contact delivery replicates
accepted Box transactions and referenced assets, not arbitrary paths from the
host filesystem.

## V1 live collaboration

### Authority model

One Cell opens a live session as host. It owns the canonical Box revision,
durable store, permission table, and physics simulation. A named contact joins
as viewer or editor through existing authenticated identity. Editors send
intents such as drag, group, connect, topology stroke, Area edit, definition
edit, or mode conversion. The host validates capability, limits, target
revision, and current semantic preconditions, then commits one canonical
transaction or a structured refusal.

The host streams reliable ordered transactions and durable checkpoint batches.
It may additionally stream high-rate transform previews over a replaceable
ephemeral channel. Guests can render optimistic local drag/topology previews,
but the accepted host transaction corrects or confirms them. Presence,
cursors, cameras, selections, voice/video media, hover, and in-progress
previews never enter the durable journal.

On join or after a detected gap, a guest receives a verified snapshot and the
ordered tail after its revision. A guest may cache that snapshot for quick
reconnect and an explicit read-only unavailable-host view. The cache is not an
editable offline replica and cannot become a new host implicitly.

A protocol needs equivalents of:

- hello with workspace uid, schema, known revision, snapshot hash and grant;
- chunked verified snapshot and referenced asset transfer;
- intent with stable intent uid, actor, base revision and typed payload;
- accepted transaction with canonical revision, actor attribution and typed
  operations, or a structured refusal;
- durable spatial checkpoint batches tied to a canonical revision;
- ephemeral state previews and presence with no durability promise; and
- gap, resnapshot, access-lost, host-ended and resource-limit states.

### Why live-only first

Benefits:

- one solver produces one position for Areas, slopes, collisions and grouped
  Sands;
- topology and composition conflicts are validated against a current document
  instead of assigned an arbitrary last-writer rule;
- permissions, undo attribution, mutation Actions, resource admission and CEF
  capability decisions have one enforcement point;
- the local durable path and collaborative durable path exercise the same Box
  transaction API; and
- the UI can explain exactly which host owns the current session and whether a
  checkpoint is durable.

Costs:

- collaboration depends on host availability;
- guest input includes network latency and may require optimistic preview;
- there is no offline co-authoring, automatic failover, or transparent device
  replica; and
- a host must budget simulation, CEF surfaces, assets and guest traffic.

These costs are honest and bounded. Offline multi-writer collaboration would
add host election, causal merge, definition and topology conflict semantics,
physics reconciliation, encrypted replica membership, revocation, tombstones,
garbage collection, and asset availability. It should follow evidence from
the live protocol rather than being approximated by syncing whole Box files.
An explicit export or fork remains possible, but it creates a new workspace
lineage rather than silently becoming a peer replica.

The stored operation model must not bake in a permanent single-host id. Stable
operation ids, actor attribution, revisions, snapshots, and typed semantics
leave a future replicated protocol possible without pretending it exists now.

## Security and resource boundaries

Workspace sharing reuses authenticated and encrypted contact sessions, but
authentication alone is insufficient. Every transaction is authorized for
workspace and operation scope; definition edits are distinct from instance
edits; Record Actions retain their own authority and are never implied by Box
editor status; and Installed HTML/Website capability boundaries remain intact.

The host validates operation count, nesting depth, string and asset sizes,
topology stamp complexity, Area count, physics admission, CEF surface budgets,
and content-addressed asset hashes before commit. A guest cannot name host
filesystem paths, inject an unreviewed shader/Behavior/package, publish a
Facade, broaden a Protein query, or invoke a Ledger Action through a generic
workspace edit.

Snapshots and journals are checksummed and versioned. Interrupted local tails,
malformed remote frames, unknown verbs, stale base revisions, oversized
transactions, missing assets, and revoked access all fail visibly and closed.
Accepted transactions retain actor attribution for inspection and undo.

## Needed for correctness and needed for speed

Needed for the feature to work:

- stable Lince semantic ids independent of runtime handles;
- one durable Box store with atomic snapshots, typed journal replay, revision
  validation, checksums, recovery, and explicit commit boundaries;
- durable coalesced spatial checkpoints so physically moved Sands restore at
  their last committed locations;
- explicit separation of authored state, durable simulation checkpoints,
  personal view state, and ephemeral session state;
- host validation, permission checks, ordered delivery, gap detection,
  snapshot recovery, resource limits, and honest session UI; and
- separate Record, Workspace, and File-projection operation semantics behind
  one Synchronization experience.

Needed only if measurement shows it makes the accepted workload faster:

- SceneDB-like structure-of-arrays pages and generation handles;
- SIMD cell scans, warm/cold residency, HLOD, and world streaming;
- GPU visibility compaction and indirect drawing;
- sparse dirty-range upload versus full-buffer crossover;
- dirty topology tiles and derived-cache persistence;
- checkpoint debounce, transform quantization, binary delta compression, and
  content-addressed asset chunking; and
- interest-managed preview streams for large collaborative workspaces.

No unmeasured storage or replication optimization blocks the human-usable Box.
The schema must permit measured improvements without exposing their runtime
handles or cache shapes.

## Decision and implementation order

1. Keep the existing Lince-owned Winit/WGPU compositor and selected
   Bevy/Avian adapters. Do not adopt SceneDB as the Box database.
2. Define stable spatial ids, coordinate forms, Box transactions, canonical
   revisions, authored operations, spatial checkpoints, and ephemeral state
   before implementing disk encoding.
3. Build the local snapshot/journal store with crash and invalid-tail tests,
   then expose its human-visible saved/saving/recovery state.
4. Make Area/topology physics harvest coalesced checkpoints at fixed-step
   boundaries and prove that a Sand which settles in an influence Area returns
   there after process restart.
5. Measure runtime copies, upload ranges, query cost, topology dirty tiles, and
   checkpoint volume at the accepted Box workload. Introduce SceneDB-like hot
   storage only where those measurements justify it.
6. Add the unified Protein Synchronization surface with distinct Record,
   Workspace, and File Projection lanes.
7. Add live-only host-authoritative workspace sessions over the existing
   authenticated contact infrastructure, including optimistic previews,
   durable transactions, checkpoint delivery, snapshot recovery, permission
   refusal and honest host-loss behavior.
8. Revisit offline replicas only after live collaboration has exposed real
   conflict, authority, asset, topology, and physics requirements.

SceneDB remains a valuable implementation repertoire and a possible bounded
runtime dependency after measurement. EngineFS remains useful API-design
inspiration. Neither becomes Lince's semantic constitution or durable storage
format.
