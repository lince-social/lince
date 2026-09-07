# Pulsar and Helio research ledger

Completed 2026-08-30 with 30 contiguous source-audited reviews. Each entry
keeps a technical reading, Lince relevance judgment, possible experiments and
the limits that prevent an article from becoming an architectural decision by
itself. A focused local HTML snapshot is retained when the study explicitly
calls for one and the source permits it; the review always links the live
original. This file is evidence and repertoire, not an implementation queue.

## Carry-forward decision

The owner's 2026-09-07 Bevy decision supersedes the implementation assumptions
inside these dated reviews. Bevy now owns the application and renderer; new
interface code uses it directly. References below to an accepted Lince-owned
compositor, subordinate Bevy, renderer-neutral inspector/pass APIs or separate
retained UI describe the old research context, not requirements. Do not
implement them unless a current scoped need independently justifies the work.
Custom Bevy plugins, focused internal/external crates and pure WGPU passes
remain possible exceptions under the current architecture. The source audits
and historical verdicts below are preserved rather than rewritten as new
measurements.

The embedded browser these readings assume is gone. CEF was removed from the
repository on 2026-09-07 and rejected as an approach; [the build
rule](build.md#no-embedded-browser) records why. Every technique below that
crosses an "embedded browser" boundary is retained as a constraint on whatever
renders external HTML without embedding a browser in Lince, not as a plan to
embed one again.

The study selected no Pulsar, Helio, SceneDB, GPUI or WGPUI production
dependency. Active interface development carries only stable semantic identity
above disposable handles, ordered Bevy schedules, targeted changes at real
backend/external boundaries, presentation-only culling, human-readable
causal measurements, authoritative shared coordinate frames and measured-only
GPU acceleration of disposable projections. The canonical architectural
meaning and sourcing order live in
[architecture.md](architecture.md#completed-pulsarhelio-study-and-carry-forward-boundary),
and their locations in the existing waterfall live in
[plans/interface.md](plans/interface.md#completed-engine-study-and-its-place-in-the-waterfall).

The remaining findings stay here so future work can recover both the useful
techniques and the reasons particular implementations were rejected. They do
not reopen GPUI ownership, introduce a second engine plan, or add v1 work for
Fusor, Corona, probe lighting, foliage, portals, XR, SceneDB persistence or a
Pulsar Behavior compiler.

https://tridentforu.com/blog/posts/Helio-Renderer

### 1. Building a Renderer From the Ground Up: How Helio Works

Read 2026-08-29. Published 2026-04-08; the source page reports a 2026-08-21
modification. Focused article-body snapshot:
[helio-renderer.html](research/sources/helio-renderer.html), SHA-256
`ab5f471629e28114b34ae9f2473f2967d5d62c260364edcea7cef26ec41c68b1`.
The snapshot removes site navigation, footer, stylesheet/script bundles and
analytics while retaining the title, attribution, canonical URL and article
body. It is research material; copyright remains with Tristan Poland.

**What the article explains.** Helio is presented as a Rust/`wgpu` deferred
renderer organized as an ordered graph of small passes rather than one large
render function. CPU `prepare` work updates resources and GPU `execute` work
records into a shared command encoder. The described default path performs
shadow matrix and atlas work, sky lookup/rendering, debug drawing, temporal
Hi-Z occlusion culling, a depth prepass, tiled light culling, G-buffer
generation, virtual geometry, deferred lighting, billboards, water simulation,
TAA/upscaling, profiling and final debug composition.

Scene objects use typed integer handles into GPU-resident arrays. Growable
buffers carry dirty and generation state: ordinary changes upload only dirty
data, while a reallocation increments the generation so dependent bind groups
rebuild only when their underlying allocation changed. Materials use indexed
texture arrays. Visibility compute writes indirect-draw arguments, allowing
the CPU to submit batches instead of walking visible objects and issuing one
draw per object.

The renderer distinguishes internal render resolution from output resolution,
supports pre-baked data for static scenes, exposes per-pass GPU profiling, and
allows a simple graph, inserted custom passes or a fully custom graph rebuilt
on resize. Optional adapter features are queried at startup. The article's
radiance-cascade path still depends on experimental ray queries and is
described as a zero-contribution fallback on the `wgpu` version discussed.

**What helps Lince.** The strongest lesson is architectural rather than visual:

- A Lince-owned frame graph can make resource reads, writes, order, lifetime
  and profiling explicit across topology mesh generation, native Sand
  instances, world rendering, embedded browser texture import/copy, retained UI, selection
  overlays and final composition.
- Typed runtime handles and generation counters fit Lince's existing rule that
  renderer ids are disposable projections. Sand definition identity stays in
  Lince while dense GPU slots can move or reallocate freely.
- Dirty ranges and dirty topology tiles are more relevant than blindly
  uploading every Sand, Area, transform or pattern every frame.
- Indirect drawing and GPU visibility/compaction are promising for thousands
  of lightweight native Sands. Visibility may zero draw work exactly as Helio
  describes while Protein, Behavior, Areas, media and physics remain active;
  it must never become Lince's forbidden camera-based behavior suspension.
- Per-pass timestamp telemetry should feed Lince's human-readable runtime
  health Sand. A frame spike should identify the responsible pass and workload
  instead of merely reporting low FPS.
- Separate internal resolution is useful for terrain, shadows and expensive
  world effects, but native text, retained controls and embedded browser composition should
  remain at output/device resolution unless their own measured policy says
  otherwise.
- A small/default/custom graph is a useful capability pattern. Lince can keep a
  calm desk graph and enable expensive world passes when a workspace actually
  needs them without changing Sand semantics.

**What it does not establish.** This is an explanatory author blog post, not a
reproducible benchmark or a Lince integration audit. The stated “O(1) CPU frame
cost” describes constant CPU command submission for selected steady-state
passes; GPU shader work still scales with objects, pixels, lights and shadow
faces, and CPU scene updates still scale with changes. It should not become a
literal performance guarantee in Lince.

The fixed roughly 256 MiB shadow atlas favors predictable renderer timing but
is inappropriate as an unconditional cost for Lince's minimal desk. Deferred
lighting, a G-buffer, water, atmospheric sky, TAA and global illumination are
not prerequisites for crisp cards and may cost more bandwidth than a simpler
UI/world path. The article does not cover embedded-browser external-memory synchronization,
text/IME, accessibility, topology or Area physics, Protein, Sand composition,
device loss, or Lince's authority boundaries. Its multi-backend and GI details
are tied to the versions discussed; the black GI fallback is an interface slot,
not evidence that the feature works.

**Ideas to test, not decisions yet.** When rendering work resumes, define a
narrow Lince frame-graph experiment with declared resources and timestamps;
add allocation generations and dirty ranges to instanced Sand and topology
buffers; compare CPU-built visibility with indirect GPU compaction at the
accepted 10,000-node workload; keep a full-resolution text/browser/UI pass over a
scalable world pass; and expose adapter capabilities and disabled quality
features honestly in runtime health. Study Helio's implementation before
copying any mechanism and benchmark the calm 2D desk separately from the
surface-perspective and free-space workloads.

**Verdict:** highly useful repertoire for Lince's renderer organization,
resource invalidation, batching and observability; insufficient reason to
adopt Helio wholesale or to replace the accepted Lince-owned compositor.

[EngineFS: Pulsar's Virtual Filesystem Layer](https://pulsarnative.com/blog/2026-06-26-pulsar-engine-fs)

[SceneDB 2.0: The Cross-Device Spatial Database](https://pulsarnative.com/blog/2026-07-17-scenedb20-cross-device-spatial-database)

[SceneDB 2.0 & Helio — Unified Engine Specification](https://pulsarnative.com/Research/doc/?section=drafts&slug=scenedb20)

[There Is Only One Copy: Why SceneDB's ECS Looks Nothing Like an ECS](https://pulsarnative.com/blog/2026-08-26-state-of-scenedb)

### 2. SceneDB 2.0, state ownership, persistence, and EngineFS

Read together 2026-08-29. EngineFS reports publication on 2026-06-26. The
SceneDB specification is revision 2.1 from May 2026. The SceneDB 2.0 and state
pages report 2026-07-23 and 2026-08-27 respectively, one later than the dates
embedded in their URLs. The public
[SceneDB repository](https://github.com/Far-Beyond-Pulsar/SceneDB) was also
inspected at commit `cebb1372592b229d48aadcddd6cb3a19dafd6530` from
2026-08-25. The complete combined analysis is
[SceneDB 2.0 and EngineFS review](research/scenedb.md).

**What the SceneDB sources explain.** SceneDB owns scene data which must
survive a frame; Helio owns only disposable products derived from it. Packed
slot/generation handles remain stable while dense structure-of-arrays rows
move. Explicit mutation/relocation boundaries repair mappings, harvest dirty
fields, compact pages, synchronize sparse changes to the GPU and retire GPU
resources by completed submission serial. Spatial cells add SIMD AABB/frustum
queries, multiple observers, hysteresis, pins and residency/LOD tiers.

The July article's sparse-mutation measurements are promising, but also show
that delta processing loses when almost everything changes. Lince should
measure the sparse/full crossover for its own transforms, topology tiles and
GPU buffers. The August article adds independent GPU/replication field
annotations, change tracking, deltas, relevance, authority, events, snapshots,
reconciliation and subscriptions. It explicitly leaves transport, encryption,
authentication, asset streaming, undo, locks and editor CRDT/OT outside
SceneDB.

**Repository reality.** The crate implements useful bounded delta encoding and
runtime snapshots, but it is not a crash-safe spatial database. A spatial
snapshot restores rows with newly allocated handles and discards the original
handle bits. Snapshots do not yet have the delta wire/disk framing, and there
is no atomic durable snapshot plus journal-replay protocol. SceneDB handles
therefore cannot be Lince workspace ids. Its simple shared conflict policy is
also not a semantic answer for simultaneous topology, grouping, definition or
physics edits.

**What helps Lince.** Borrow the one-logical-authority rule,
generation-checked dense runtime indirection, hot SoA columns, dirty masks and
ranges, explicit fixed-step/relocation boundaries, sparse/full upload choice,
and later cell residency. Keep stable Lince uids above every Bevy entity,
SceneDB slot or GPU index. The durable Box store owns semantic entities and
recovery checkpoints; the spatial runtime owns current in-session simulation;
the renderer owns only disposable mirrors and caches. Adopt a SceneDB-like hot
store only after the real Box workload identifies a measured bottleneck.

**What Box saves.** A readable versioned snapshot plus typed append journal
stores Sand/Area/topology definitions, stable ids, bindings, authored
transactions and coalesced spatial checkpoints. It never writes each physics
frame. A Sand becomes checkpoint-eligible when it settles, reaches a stable
effective destination or exceeds a bounded maximum recovery age. Surface mode
saves logical surface coordinates and derives its world height from restored
topology; free space saves a full transform. Runtime handles, meshes, contacts,
solver caches and GPU resources are rebuilt. This makes a Sand that travelled
from Protein into an influence Area reopen where it last durably settled.

**What EngineFS contributes and where it stops.** One replaceable filesystem
provider gives callers a pleasant uniform API, centralized path handling,
events and testability. File verbs are not enough for Lince collaboration:
they lack semantic transactions, revisions, authorization, atomic multi-entity
intent, merge meaning, ordered replay and snapshot-gap recovery. A global
provider also conflates the local authority with a storage location and makes
remote blocking calls part of ordinary filesystem access.

Lince keeps one Protein Synchronization experience with three explicit lanes:
existing Record/Organ operations, live Workspace Box transactions, and File
projection. They reuse contact identity, grants, invitations, delivery,
status, assets and recovery where appropriate, but not one operation enum or
filesystem-provider abstraction.

**Collaboration decision.** The first workspace collaboration is live-only and
host-authoritative. One Cell owns the durable store, canonical revision and
physics result. Guests send typed intents; the host validates and persists a
canonical transaction or refuses it. Reliable transactions, snapshots and
coalesced checkpoints are durable; cursor, camera, presence, media and
high-rate movement previews are ephemeral. A guest cache supports reconnect
or a clearly read-only host-unavailable view, not offline edits or implicit
host replacement. This yields one physics outcome and understandable conflict
semantics at the price of host availability and network-latency handling.
Offline replicas and failover remain a later, explicit protocol.

**Verdict:** use SceneDB as runtime repertoire, not persisted identity or Box
storage; use EngineFS as inspiration for service ergonomics, not as the common
wire/storage protocol. Build Lince's local snapshot/journal and durable spatial
checkpoint path first, then live collaboration over the same typed Box commit
API and existing authenticated contact infrastructure.

[Building a Production-Grade 3D Viewport: Zero-Copy Rendering Between Bevy and GPUI](https://tridentforu.com/blog/posts/gpui-viewport)

### 3. Bevy-to-GPUI viewport and cross-renderer composition

Read 2026-08-29. Published 2025-10-25; the page reports a 2026-08-21
modification. The article was checked against the public Pulsar source at the
publication-day commit
[`117ad4e`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/117ad4e2ef4670aa471c041894124bdb603a982c)
and against current main at
[`0f4ee79`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/0f4ee7961addc0084ebec427542c26773c7ea0ac).

**What the article explains.** This is a historical Windows viewport design in
which Bevy renders through Direct3D 12 into two shareable BGRA textures and a
GPUI-owned Direct3D 11 compositor opens the same allocations through DXGI NT
handles. A dedicated thread polls keyboard and mouse state at 120 Hz, writes
small values and accumulated deltas into atomics, and opportunistically sends
them to the render state. Two atomic indices nominate a render texture and a
display texture. GPUI continuously requests frames and samples the nominated
display texture. Selection converts viewport coordinates through NDC and
camera space to a world ray, then tests scene-object bounding spheres.

The useful architectural shape is three independently paced responsibilities:
input collection, world update/rendering, and final UI presentation. A texture
or command product crosses the rendering seam rather than a CPU framebuffer,
and camera input does not wait for unrelated UI layout. Coordinate transforms,
adapter selection, exact texture format, resource lifetime and frame ownership
are recognized as first-class integration concerns rather than incidental
plumbing.

**Needed for such a seam to work correctly.** A CPU atomic saying “buffer 1”
does not prove that GPU writes to buffer 1 have completed. The producer must
publish only after the relevant submission, the consumer must wait on an
explicit GPU completion primitive or a rigorously documented shared-surface
contract, and the old display buffer must not be reused until the consumer is
finished. Format, color space, alpha, dimensions, adapter identity, device-loss
recovery and handle ownership must also agree. A pair of separate atomic stores
is not an atomic pair; acquire/release ordering does not by itself make two
indices a coherent ownership protocol. The publication-day implementation has
no shared DX11/DX12 fence or keyed synchronization around the sampled texture,
and `std::mem::forget` makes the allocations process-lifetime leaks instead of
giving them an explicit cross-subsystem owner. Those excerpts are a prototype,
not a safe resource-lifetime recipe.

Microsoft's
[D3D12 simultaneous-access contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_resource_flags)
allows multiple readers and one writer only when the writer is not modifying
texels concurrently read by another participant; the
[shared-handle documentation](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createsharedhandle)
explicitly points DX11/DX12 interoperability to shared fences. The article's
buffer nomination is still useful, but it needs a real buffer state machine
such as `free -> rendering -> ready -> displaying -> free`, submission/fence
values, and owned teardown. Two buffers may then be enough; a third can keep a
producer moving when presentation is late, at the cost of memory and potentially
one more queued frame of latency.

**Needed only to make it fast.** Avoiding GPU-to-CPU-to-GPU framebuffer
readback, separating producer and presentation rates, accumulating high-rate
pointer deltas without taking a contended lock, and repainting the UI only when
a new world frame or UI change exists are promising optimizations. Whether a
dedicated input thread, lock-free fields, double versus triple buffering, or a
cross-context zero-copy surface wins must be measured on the actual Lince path.
Atomics are not automatically faster at the system level: cache contention,
polling wakeups, frame queue depth and synchronization stalls can dominate
their nanosecond instruction cost.

**Wayland and Lince fit.** DXGI, NT handles, D3D11 cursor warping and global
device polling are not a Linux/Wayland design. Lince must receive focused input
through its owned Winit/Wayland seat path and use compositor-supported relative
pointer and pointer-constraint behavior for captured 3D navigation. A future
sampling thread may consume that authorized event stream if measurement shows
it helps; it must not bypass focus or depend on global hardware polling. On the
graphics side, Linux external surfaces use DMA-BUF/Vulkan external-memory
rules, DRM format modifiers and explicit or callback-defined synchronization,
not a translated assumption that an integer handle is sufficient.

The ownership direction is also the inverse of accepted Lince Plan A. The
article makes GPUI own the window and final presentation while sampling an
external world texture. Lince already measured that direction and selected its
own Winit/WGPU host so native UI, Bevy world passes and imported embedded browser surfaces
share one Lince-owned frame policy. Native world/UI work on the same WGPU
device should hand over texture views and command work directly; the embedded browser remained a
separate producer and follows the already-proven DMA-BUF import, GPU copy and
fence boundary. This article gives no reason to put GPUI back into the
production dependency graph.

**Source and evidence limits.** The page reports a four-object scene on one
RTX 3060/Ryzen 5600X machine, but supplies no benchmark harness, traces,
percentile distributions or workload scaling. Its “2-5 ms input latency” ends
at a GPU-state update, while its own input-to-visible-frame walkthrough totals
about 35 ms; 300+ UI FPS and 120+ renderer FPS are independent rates rather
than an end-to-end guarantee. The shown object selection loop is CPU-side
bounding-sphere testing despite being called GPU-side raycasting. Multiple
viewports require additional scene rendering or reuse of an already-rendered
view and therefore are not nearly free. Kernel GPU handles also cannot be sent
over a network as a remote-rendering transport; remote display needs capture,
encoding, transport, decoding and pacing.

Pulsar's current main no longer contains the cited Bevy/DX12-to-GPUI/DX11
paths. It uses Helio with a WGPUI `WgpuSurfaceHandle`, a background renderer,
triple buffering and a published-frame counter. That evolution reinforces the
value of same-device composition and repaint-on-publication, but it also means
the article is not current Pulsar architecture documentation. Current main's
Linux viewport cursor implementation remains X11/XWayland-oriented and says
the operations silently do nothing on Wayland, so it is not reusable evidence
for Lince's Wayland input seam.

**Ideas to carry forward.** Keep one explicit frame coordinator; model every
producer/consumer texture with owned states and GPU completion values; compare
two and three buffers using input-to-present latency rather than producer FPS;
drive world repaint from published work instead of unconditional UI animation
requests; test coordinate mapping under resize and scale; and expose queue
depth, dropped/overwritten frames, input-event-to-submit and submit-to-present
separately in runtime health. Treat relative-pointer correctness as a required
Wayland capability and high-rate input sampling as an optional measured
optimization.

**Verdict:** strong historical evidence for GPU-resident composition,
independent scheduling and explicit viewport coordinate boundaries; unsafe as
a synchronization/lifetime template, Windows-specific in its central bridge,
and supportive of Lince's already-selected owned WGPU compositor rather than a
reason to restore GPUI.

[Introducing Helio: A GPU-Driven Renderer Built in Rust](https://pulsarnative.com/blog/2024-06-01-introducing-helio)

### 4. Helio's public scene and GPU-driven rendering model

Read 2026-08-29. Despite the date in its URL, the page declares publication
and modification on 2026-06-06. This entry does not repeat the pass-by-pass
renderer notes in entry 1. It concentrates on the public scene model and checks
the claims against the Helio repository at
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).

**What the article adds.** Helio is a Rust/`wgpu` deferred renderer whose CPU
scene uses generational handles for meshes, materials, objects and lights.
Scene mutations update dirty CPU mirrors; `flush()` rebuilds or patches the
corresponding GPU buffers. Packed vertices, instance records and material
indices make scene data directly consumable by shaders. Compute passes perform
frustum and optional Hi-Z culling, compact indirect draws, and choose meshlet
LOD. A render graph orders shadow, depth, G-buffer, lighting and optional world
passes. The article also describes bindless materials, physical light units,
meshlet virtual geometry, water heightfields, scene import, 64 visibility
groups, static/stationary/movable caching and automatic CPU/GPU pass timing.

This gives a useful separation between an application's semantic scene, a
dense runtime scene, and disposable GPU projections. A rigid object's mesh can
remain upload-once while only its instance transform changes. A high-polygon
world object can use meshlets without forcing lightweight cards, text or browser surfaces
surfaces through that representation. Optional passes can be absent from the
calm desk graph and present in a world graph without changing the Box document.

**Needed for Lince to work.** Persistent Sand, Area, topology, binding and
Castle identity stays in Lince stable uids. Helio-style slot/generation handles
are safe only inside a rebuilt runtime; they do not replace durable ids or the
snapshot/journal design. A render graph must declare resource ownership,
formats, dependencies, resize and device-loss reconstruction. Every adapter
capability must produce an explicit supported path or an honest unavailable
state. Renderer culling and visibility masks may remove pixels and draw work,
never Protein, Behavior, Areas, physics, calls or off-camera Website/Installed
Sand execution.

Lince also cannot treat `Static` as permission to discard an edit. Current
Helio returns success after warning and doing nothing when a static object's
transform is changed. That is acceptable only as engine-specific policy, not
as a Box transaction contract. Lince should derive cache eligibility from
settled/change state where possible; if an authored classification really
forbids mutation, the attempted transaction must fail visibly. A Sand moved by
physics, Protein or a person remains semantically movable even when its mesh
geometry is immutable.

**Needed only to make it fast.** Dirty ranges, dense GPU columns, indirect
drawing, compact instance layouts, meshlet culling, bindless materials,
upload-once geometry, cached shadows and asynchronous timestamp readback are
optimizations to benchmark per workload. Lightweight native Sands are the
first candidate for instancing and indirect draws. Meshlets and virtual
geometry become relevant for dense imported/custom world scenes; they do not
help ordinary rectangular cards. The topology plane may benefit from tiled
dirty geometry or a GPU height field before it benefits from a general virtual-
geometry system. GPU water is repertoire for later field simulation, not an
implementation of Box topology or Area forces.

**Corrections from the source audit.** Several statements are directional, not
literal contracts:

- “O(1) CPU per frame” means a steady scene can submit a bounded set of passes
  without a CPU draw loop. GPU culling still scales with candidates and
  meshlets; CPU work scales with changes, graph work, assets and readback. It is
  not a frame-complexity guarantee.
- The listed `PackedVertex` fields total 40 bytes, not 43, and current
  `GpuInstanceData` is already 208 bytes because it also carries the previous
  transform. These layouts are evolving implementation details, not an ABI for
  Lince Sand packages or persisted Box state.
- Current main requires only `INDIRECT_FIRST_INSTANCE`. Indirect-count draws,
  timestamps, writable vertex storage, bindless material features and
  experimental ray queries are requested only when the adapter supports them.
  This current capability negotiation is healthier than the article's claim
  that several advanced features are unconditionally required.
- Hiding or showing an already-set visibility group is O(1), but changing a
  group currently reevaluates objects and updates visibility slots in O(N).
  The bit test is cheap; mass visibility changes are not “free.” Lince groups
  also need more than 64 semantic categories, so this mask can only be a
  renderer cache derived from richer layers, filters and Castle membership.
- The current radiance-cascade pass uses experimental ray queries when
  available and otherwise runs an ambient/scene-color fallback. The article's
  multi-bounce description does not establish equivalent GI on every target.
- `wgpu` portability is not application support by itself. The repository's
  main CI workflow builds and tests only Ubuntu; the article provides no
  platform matrix proving Vulkan, DX12, Metal, Android and WebGPU output,
  input, recovery and feature parity.

**What to borrow for Box.** Keep the Lince-owned semantic/runtime/renderer
three-layer split. Project thousands of simple native Sands into dense
instance columns with dirty spatial ranges. Let richer native UI remain
retained nodes and the embedded browser remain an imported browser surface. Use independent
presentation masks for editor chrome, 2D surface, 3D free-space, selection and
debugging, but never confuse them with Castle grouping or Protein filtering.
Give topology tiles, native Sand instances and world geometry separate update
and render paths behind the same frame coordinator. Expose pass CPU time, GPU
time, availability and delayed-sample age in runtime health rather than
silently presenting CPU timing as GPU timing when timestamps are absent.

The asset bridge's intermediate converted scene is a useful boundary: Lince
can accept multiple scene-authoring and interchange formats through adapters,
normalize them into an owned scene representation, retain provenance and
licenses, then build runtime meshes. It should not make Solid3D, FBX, USD or
any other example format the Box schema. Sectioned meshes behaving as one
logical object also resemble a Castle's “many projections, one semantic unit,”
but Castle composition remains Sand/event/binding composition rather than a
renderer material-section primitive.

**Ideas to test.** Compare the existing Bevy/native-WGPU path with a narrow
Helio scene adapter using the same 10,000-Sand and topology fixtures; measure
steady, 1% dirty, high-churn and visibility-change cases separately. Record
CPU source-diff cost, upload bytes, compute-cull time, draw time and memory.
Exercise missing indirect-count, bindless and timestamp capabilities on
purpose. Only consider adopting a Helio subsystem if it beats the existing
path and its ownership, unsafe surface, maintenance and feature fallback remain
smaller than implementing the bounded mechanism directly.

**Verdict:** valuable confirmation of generation-safe runtime handles, dirty
GPU mirrors, explicit render graphs and specialized GPU scene paths. It does
not justify replacing Lince's semantic model or accepted compositor, and its
performance/platform language must be converted into Lince-specific measured
gates before adoption.

[Pulsar's Subsystem Architecture: How the Engine Core Ends Up Knowing Nothing](https://pulsarnative.com/blog/2026-06-26-pulsar-subsystems)

### 5. Subsystems, capability injection and the limits of decoupling

Read 2026-08-29. The architectural claims were checked against the Pulsar
source from the article's publication day at
[`85ca40c`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/85ca40c7c94150e862efabe3b3f96d0a986f9074)
and current main at
[`0f4ee79`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/0f4ee7961addc0084ebec427542c26773c7ea0ac).

**What it proposes.** Pulsar puts renderer, physics and other services behind a
`Subsystem` trait and stores erased implementations in a registry. Each
subsystem declares string-identified dependencies, and the registry contains a
topological initializer, per-frame callback and reverse-order shutdown. A
composition root loads built-ins and plugin dynamic libraries, then injects
their subsystem objects into an allegedly implementation-agnostic backend.
Reflection-registered component behaviors retrieve concrete services from a
second type-indexed context and project component data into renderer, physics
and cache state. Additional inventory registries associate class-name strings
with runtime behavior and scene-property projection functions.

The healthy core is dependency inversion, not the claim that the engine knows
literally nothing. A small semantic kernel can know service contracts and
orchestration rules while a composition root chooses implementations. That can
make a headless Lince, renderer tests and alternative capability sets possible
without making every Sand aware of WGPU, the embedded browser, persistence or networking.

**Needed for Lince to work.** There must be one authoritative composition
phase that gathers every built-in and extension declaration, rejects duplicate
identities, validates missing dependencies and cycles, and only then performs
side effects in dependency order. A failed initialization must roll back what
was already initialized; shutdown must attempt every initialized service and
report all failures rather than abandoning the remainder at the first error.
The resulting runtime must expose capability availability and subsystem health
to the person using Lince. A Sand whose required capability is absent should
be visibly unavailable with a reason, not panic, disappear or half-project.

Capabilities given to Sand behaviors must be narrow, typed and mediated. A
native Sand can request, for example, record selection, Box event publication,
local presentation state or an approved external-call channel. It should not
receive a raw renderer, database, filesystem, network client or mutable bag of
all services. Website Sands cross a process and browser boundary and receive the same
semantic ports through a versioned message protocol. Rust `Any`, `TypeId` and
trait objects are suitable conveniences only inside the same compiled Lince;
they are not a stable ABI for external Sands or independently built dynamic
libraries.

The Box transaction remains authoritative. If one operation changes a Record,
a Sand placement and an Area membership, those semantic changes must validate
and commit atomically before adapters derive physics, render and cache state.
Sequentially borrowing a renderer, mesh cache and scene cache prevents several
Rust aliasing mistakes, but it does not make a multi-service update atomic.
Derived projections must be disposable and reconstructible after a partial
adapter failure.

**Needed only to make it fast or pleasant to develop.** O(1) type or id lookup,
link-time registration, cached dependency plans, dirty component queues and
independent subsystem rates can reduce boilerplate or runtime work, but they
are not prerequisites for the architecture. Lince should first establish
typed ownership and correct lifecycle behavior. It can then replace full scene
scans with Record/Box change subscriptions, batch projection work, and schedule
fixed simulation, asynchronous external work and rendering at distinct rates.
Off-camera Sands continue executing; only their presentation work is culled.

**Source-audit problems.** The article describes safeguards that the actual
integration path does not receive:

- Both at publication and on current main, `inject_plugin_subsystems()`
  registers each supplied object and immediately calls its `init()` in the
  supplied order. It does not call the registry's dependency resolver, record
  `init_order` or set `initialized`. The backend later calls `shutdown_all()`,
  but that method returns immediately while `initialized` is false. The
  injected services therefore bypass the advertised ordered initialization and
  reverse teardown. Searches of the current production crates find the
  registry's `init_all()` and `update_all()` only in definitions and tests.
- Registration occurs before initialization. If an initializer fails, the
  failed object remains registered and earlier objects are not rolled back.
  Even the unused `init_all()` stops at the first initialization failure
  without rolling back prior services; `shutdown_all()` similarly stops at its
  first shutdown error. A dependency graph on paper is not a lifecycle
  transaction.
- The article says the backend crate does not list or import Helio, Rapier or
  the old rendering crate. Its publication-day manifest directly listed those
  dependencies, and current `engine_backend` still contains optional Helio,
  WGPU and Rapier dependencies, direct renderer/physics modules and Helio
  re-exports. Features make coupling conditional; they do not make the claimed
  compiler boundary true.
- Link-time `inventory` is not entirely self-assembling. Current source has
  explicit force-link imports to stop component registrations from being
  discarded. This can still be useful for same-build modules, but the
  composition root must own registration visibility and test the final linked
  set rather than assuming every crate was discovered.
- A separate runtime service bag stores borrowed mutable services as raw
  pointers and reconstructs `&mut T` through `unsafe`; its convenience macro
  panics when a service is absent. The lifetime and uniqueness contract rests
  on callers and is not represented by the type. That is too much ambient
  authority for Sand behavior and too fragile for extension isolation.
- Identity is split among subsystem string ids, concrete `TypeId`s, reflected
  class-name strings and several inventory registries. Duplicate behavior uses
  inconsistent policy: direct registration errors, registry merge silently
  keeps the first, and inventory iteration returns the first matching class.
  Silent first-wins makes load order an accidental policy and hides conflicts.
- Scene-property projection writes into a generic string-to-value map. It is a
  convenient escape hatch, not a schema: ownership, collision handling,
  provenance and allowed readers are unclear. The runtime sync then scans
  objects/components and linearly searches inventory registrations by class
  name, which should not become Lince's change-propagation model.

The fact that native plugins are never unloaded reduces one lifetime hazard,
but it does not turn Rust trait objects into an external ABI. Compiler and
dependency versions, trait layout, panic behavior, allocator ownership and
duplicate crate instances still have to agree. Lince does not need this risk:
same-build built-ins can use Rust traits, while independently supplied code can
use a stable protocol and process isolation.

**Concrete Lince shape.** Give every capability and Sand definition a
namespaced stable id and a typed declaration. A definition owns its data
schema, editor/inspector surface, native or browser presentation adapter, event
ports, Protein bindings, behavior requirements, permissions, persistence
projection and runtime-health description. “Castle” remains composition of
those definitions rather than a privileged subsystem kind. At startup, a
composition manifest resolves all providers and consumers into an immutable
validated graph. During execution, explicit phases run input, semantic Box
transactions, fixed-step physics/Areas, derived projections, persistence and
render preparation without pretending every service has one useful
`on_frame(delta)` cadence.

Projection ports should be typed and namespaced rather than one shared
property map. The Sand definition that owns a projection declares its source
schema and target capability; the runtime compiles that into direct dispatch
and change subscriptions. Unknown definitions, versions and ports fail closed
with an explanation. Conflicting providers fail startup or leave only the
affected definition unavailable; they never resolve by registration order.

**Ideas to test.** Build lifecycle conformance tests that deliberately create a
missing dependency, cycle, duplicate provider, middle-of-chain init failure
and several shutdown failures. Verify rollback, complete teardown and visible
health. Build one native Sand and one embedded browser Sand against equivalent Record/event
ports without exposing internal services. Benchmark a full component scan
against uid-keyed dirty dispatch only after correctness is established, using
the same high-churn Protein and physics fixtures planned for Box.

**Verdict:** adopt the composition-root, dependency-graph and narrow-capability
direction for Lince's internal architecture. Do not copy Pulsar's concrete
lifecycle path, ambient mutable type bag, silent conflict handling,
string-property projection or Rust-DLL boundary. The useful outcome is a thin
semantic kernel with explicit authority and a replaceable implementation edge,
not an engine that claims to know nothing while still importing its concrete
systems.

[Pulsar's Reflection System: From Macro to Properties Panel](https://pulsarnative.com/blog/2026-06-26-pulsar-reflection-system)

### 6. Reflection-derived inspectors without a second source of truth

Read 2026-08-29. The described implementation was checked at the article-day
Pulsar commit
[`85ca40c`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/85ca40c7c94150e862efabe3b3f96d0a986f9074).
Pulsar has since split reflection into its own repository; the revision used by
current Pulsar main is
[`745ee78`](https://github.com/Far-Beyond-Pulsar/Pulsar-Reflection/tree/745ee787cc63288463c170aafb778672db8e85ac).
The changes between those versions are as informative as the article.

**What it proposes.** Proc macros derive runtime descriptions from Rust
structs and enums. An engine class exposes selected properties, nested
“sub-properties,” categories, constructors, getter/setter closures and methods.
A second `Reflectable` description records a field's concrete `TypeId`, type
name, size, alignment and recursive structural kind. Link-time `inventory`
registrations collect classes, types, runtime behaviors, methods and
type-specific GPUI property editors. The properties panel selects a widget by
field type, otherwise falls back to structural rendering, and writes edits
back to per-component JSON. The intended result is an inspector and Blueprint
surface that need no component-specific central switch statement.

The strongest reusable insight is that a value schema and its editor are
related but separate. Structural metadata can choose a safe default editor;
semantic metadata can refine it into a length, duration, color, force curve,
Record reference, Protein field mapping or bounded choice. The editor can
compose sections from those descriptors while specialized field editors own
their interaction state. This is useful for Sand edit mode, Area/topology
inspectors, typed wire creation and “Why is it here?” explanations.

**Needed for Lince to work.** Reflection must always be derived evidence, never
a new authority. Owner decisions remain in the relevant `.lingua` Record, and
the implemented persistent/wire authority remains Lince's explicit versioned
Rust schemas and their generated artifacts. Record and Protein shapes are data
schemas, not Rust memory layouts. A proc macro may derive an inspector
descriptor from an internal Rust type, but that descriptor cannot silently
define durable Sand, Box, Protein or host-message meaning.

Every editable field needs a stable namespaced field id and a full nested path,
separate label, value schema, constraints, unit, default/absence meaning,
read/write authority and mutation route. Presentation flattening may make a
panel calmer, but it must not erase nesting or make two identically named child
fields collide. Editor state is keyed by Sand instance uid, stable child uid
and field path, not by class and property names. Unknown types and missing
editors render a legible read-only value plus the reason editing is unavailable.

An edit does not call an arbitrary generated setter. It produces a validated
Box/configuration transaction with actor, origin and undo information. An
exposed behavior method likewise has a stable command/port id, typed inputs and
outputs, declared effects, capability requirements, permission checks and a
fallible result. Do not automatically turn every readable property into a
public mutating behavior method: that bypasses invariants and would let a
visual connection acquire authority merely because a Rust field exists.

Use two descriptor sources behind one inspector interface:

- internal fixed runtime types may derive descriptors at compile time, with
  compile-fail and round-trip tests for every supported structural kind;
- configurable Sand definitions, Protein-bound values and external Sand ports
  use their explicit versioned data schemas, from which Lince creates the same
  inspector descriptor without requiring a corresponding Rust struct.

The shared descriptor points to semantic editor kinds and customization tokens,
not GPUI types, an embedded browser DOM, WGPU handles or `TypeId`. Native retained editors then
implement those kinds with Lince's own sharp UI; installed HTML may provide a
package-local editor only through the same capability and validation boundary.

**Needed only to make it fast or improve DX.** Proc-macro generation, direct
typed getters, cached descriptor graphs, O(1) editor lookup, cached widget
instances and link-time registration reduce repeated code or allocation. They
do not establish correctness. After the schema and transaction boundary work,
Lince can update only dirty fields, retain focus/caret/popover state, avoid a
JSON serialize/deserialize round trip during every edit, batch durable writes
and virtualize very large inspectors. These are measured optimizations; a
`HashMap<TypeId, fn>` is not itself evidence of a fast editor.

**Source-audit corrections.** The article's “zero handwritten widget code” and
“marginal cost approaches zero” claims need substantial qualification:

- At the publication commit, deriving `Reflectable` for a normal named-field
  struct generated a static initializer that called non-const
  `Reflectable::type_info()` methods. Rust rejects that shape even for a plain
  `f32` field. Current reflection fixed it with lazy `OnceLock` construction on
  2026-08-21 and added a two-`f32` compile/round-trip test. Thus one of the
  article's basic advertised paths did not compile in the audited publication
  source.
- Current derived struct deserialization still obtains each erased field as a
  shared reference and moves it with `*downcast_ref`. That generated operation
  only works for `Copy` fields even though `Reflectable` does not require
  `Copy`; the current test covers only `f32`. `String`, `Vec`, nested
  configuration and other ordinary non-`Copy` fields need explicit tests and a
  corrected owned-value/clone contract before this is a general recursive
  reflection system.
- Unit enums are serialized as ordinal indices, and variants carrying data are
  rejected. Reordering variants can therefore reinterpret persisted data. A
  durable Lince schema uses stable variant ids and explicit payload schemas;
  display order is presentation only.
- `TypeId`, Rust type names, sizes, alignments and field offsets describe one
  compiled program. Rust documents type names as diagnostic output rather than
  a unique stable identity, while `TypeId` hash/order vary between releases.
  None can identify persisted values, ports, fields or an external ABI. Lince
  stable schema ids sit above them.
- Class, runtime-type and property-editor registries insert registrations into
  hash maps without rejecting duplicates; a later entry replaces an earlier
  one according to inventory iteration. Method lookup similarly returns the
  first name match. Current Pulsar added some compile/test-time method-collision
  audits, but the runtime registries still do not provide a single fail-closed
  conflict policy.
- Reflected method callers panic on a missing argument, wrong argument type or
  object downcast and ignore surplus arguments. That is unsuitable for a Box
  event, behavior graph or external message. Invocation must validate exact
  arity/types and return a structured error without unwinding the runtime.
- The property-editor registry erases a typed function pointer to `fn()` and
  restores it with `unsafe transmute`. A typed helper narrows the manual safety
  contract, but the registry still cannot prove it at the use site. Lince can
  keep its UI-owned registry typed and keep schema descriptors UI-agnostic,
  avoiding this unsafe bridge.
- “Zero runtime cost” applies only to placing registration records in the
  linked image. Startup still iterates them and builds several hash maps;
  `get_properties()` allocates a vector and boxed getter/setter closures; the
  article-era panel built type-erased widget maps and moved values through JSON.
  None is automatically bad, but none is zero.
- The article-era widget state key was only `(class_name, prop_name)`. Two
  instances of the same component could therefore share one editor's state.
  Current Pulsar explicitly fixed this by requiring an instance-specific
  `editor_key` and replaced the JSON/widget-map contract with retained
  `BoundPropertyEditor`s receiving typed `Any` values. That evolution supports
  Lince's stable instance/child/field key requirement.
- A component is not fully editable merely because it is registered. At
  publication, an unknown primitive showed “no editor,” wrappers and structs
  were read-only labels, and only unit enums received a generic dropdown.
  Current panel is even more honest and shows `(nyi)` when no factory exists.
  New components also still need semantics, validation, behavior, permissions,
  persistence, projection, accessibility and tests.

**Concrete Lince shape.** Define a renderer-neutral `InspectorDescriptor`
compiled from a versioned schema. A field descriptor resembles:
`field_uid`, `path`, `label`, `value_schema`, `editor_kind`, `constraints`,
`tokens`, `read_source`, `write_action`, `visibility` and `explanation_source`.
An editor registry maps the semantic `(value_schema, editor_kind)` pair to a
native retained Sand definition. Primitive editor Sands—number, text, toggle,
color, enum, Record reference, Protein field, vector and curve—compose into
section/group Sands using the same recursive composition mechanism as every
Castle. Unsupported structure stays visible and cannot be edited.

This makes reflection serve composition rather than define it. Selecting a
Topology Effect can generate controls for height, steepness, shoulder/top
flatness, polarity, falloff and visualization tokens. Selecting a Protein
result template can generate a source schema view whose output ports wire to
Sand inputs. Both use the same inspector primitives, stable paths and
transaction pipeline, while their domain rules remain owned by Box and
Protein.

**Ideas to test.** Add compile-fail and round-trip fixtures for optional,
vector, nested, non-`Copy`, payload-enum and renamed/reordered fields. Deliberately
register duplicate type, field, editor and method ids and require a diagnostic.
Mount two copies of the same Sand definition and prove focus, undo and local
editor state never cross. Invoke a behavior with missing, extra and malformed
arguments and prove it returns a visible error without mutation. Finally,
generate the same inspector once from a native Rust-backed Area schema and once
from an external Sand port schema to show that `TypeId` is only an internal
acceleration key.

**Verdict:** borrow descriptor-driven inspectors, structural fallbacks,
specialized editor registration and self-owned retained editor state. Do not
borrow Rust layout as schema, ordinal persistence, ambient setters, panic-based
method dispatch, silent inventory conflicts or JSON as the live editing
currency. For Lince, reflection is a compiler from authoritative schemas into
composable inspector Sands—not the database, wire protocol or source of truth.

[Corona: Building a GPU-Native Particle System](https://pulsarnative.com/blog/2026-06-26-corona-gpu-particles)

### 7. GPU particles as a disposable Box visualization

Read 2026-08-29. The implementation was checked in Helio at the last article-
day commit
[`a1f7243`](https://github.com/Far-Beyond-Pulsar/Helio/tree/a1f7243c5b1db282d27f5a3869483f3dadd179ac)
and current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The central simulation and the issues below remain present on current main.

**What it builds.** Corona allocates all particle state in GPU buffers. A
compute pass advances live slots, a one-thread-per-emitter pass writes new
particles through ring-buffer cursors, and a three-stage prefix scan compacts
live indices into emitter-local contiguous ranges. Another compute pass writes
per-emitter indirect draw arguments. Optional bitonic passes sort each
emitter's compact range by camera depth; billboard rendering samples one cell
of a procedural sprite atlas. The CPU uploads uniforms and changed emitter
descriptors and records the pass sequence, without reading particle positions
back.

The article is unusually useful about failed iterations. It shows why a
GPU-written cursor was repeatedly reset by CPU emitter uploads, how a Rust/WGSL
camera-layout mismatch produced plausible garbage, why rasterizing every dead
slot wastes vertex work, and how hundreds of tiny sort dispatches changed a
reported 2–3 ms frame into roughly 34 ms. The larger lesson is that data
ownership, binary layout and dispatch count matter more than calling work
“GPU-driven.”

**Where it helps Lince.** A particle adapter could make Protein admission,
Area attraction/repulsion, topology flow, connection activity, selection,
collisions and Action results easier to see. An Area style might emit a gentle
stream along its force field; a topology slope might show moving grains; a
newly spawned result group might leave a short trail. Sprite atlases and GPU
indirect draws are appropriate for thousands of such tiny repeated visuals.

Particle instances remain local, reconstructible presentation. They are not
Sands, Records, Box actors, topology samples, Area physics bodies or the saved
explanation of why something moved. The semantic source event, emitter style,
seed policy and customization tokens may be retained; individual particle
positions normally are not persisted or synchronized. A particle that must be
picked, carry a Record, affect an Area, survive restart or agree in a shared
session is instead a stable Box actor using the authoritative simulation path.

This separation satisfies the off-camera rule. Protein, Behavior, games,
media, Areas and semantic physics continue normally outside the camera. A
purely decorative particle projection may stop rasterizing and may advance by
age/seed reconstruction rather than executing every invisible integration
step. That is presentation culling, not sleeping behavior. If an effect is
declared interactive, it has crossed into the stable actor path and cannot use
that shortcut.

**Needed for the capability to work.** Lince must validate emitter identity,
pool ranges, aligned capacity, cumulative total, rates, lifetimes, finite
numbers and renderer limits before any GPU write. Adding or removing an emitter
must define whether its old slots expire, clear or transfer; load order cannot
reassign live particles accidentally. One side owns each evolving cursor and
fractional emission accumulator. A changed descriptor cannot overwrite a stale
prediction of GPU state.

Rust and WGSL layouts need one generated or mechanically verified source with
size, alignment, offset and shader-interface tests. Development builds retain
WGPU/backend validation. A Rust field-order mismatch is not something native
API validation can generally infer from raw bytes, so disabling validation is
not its cause or an acceptable debugging strategy. Device loss, buffer
recreation and adapter capability failure clear/reconstruct the effect without
touching Box truth.

Blend and ordering policy belongs to each effect/material family. Additive
effects need no depth order; alpha effects need a genuinely correct declared
strategy such as global sorting for the affected set, bounded depth bins or a
measured order-independent technique. Per-emitter sorting is not enough when
separate emitters overlap. Reduced-motion, effect-intensity and no-effect
customization are first-class, and an unavailable particle adapter leaves the
underlying interaction fully legible.

**Needed only to make it fast.** Keeping transient state GPU-resident,
compacting sparse pools, indirect draws, atlases, dense 64-byte particle
records, prefix sums and cached bind groups are performance choices. So are
workgroup size, maximum emitters, sorting algorithm, dispatch fusion,
subgroups, push constants and whether an off-camera effect is simulated or
analytically reconstructed. Choose them from Lince measurements across alive
fractions and effects; none belongs in the durable Sand schema.

A fixed full-million scan is unlikely to be the right allocator for every Box.
Pool pages can be sized by effect class, and dense effects may skip compaction
while sparse ones use it. Owner lookup should not linearly scan as many as 64
emitters in every particle thread if a compact owner index or page-level
metadata measures better. High-rate spawning should not run an unbounded
serial loop in one GPU invocation. These are later optimizations after bounds
and state ownership are correct.

**Source-audit corrections and limitations.** The article's headline and some
generalizations are broader than its evidence:

- The demo reserves four pools totalling 589,824 slots, while the pass still
  allocates and scans buffers for 1,048,576 slots. Its configured rates and
  base lifetimes imply roughly 77,000 simultaneously live particles at steady
  state before variation. It does not demonstrate one million simultaneously
  live, sorted and rasterized particles.
- The reported 2–3 ms unsorted and 34 ms fully sorted figures name neither GPU
  nor measurement method. The demo source uses a 1600×900 window and Corona's
  pass descriptors contain no timestamp writes. Those figures explain one
  investigation but are not portable performance gates for Lince.
- Each frame computes `u32(emit_rate * dt)` and discards the fractional part.
  At 60 Hz, 100 particles/second becomes one per frame—about 60/second—and any
  rate below the frame frequency can become zero forever. A residual
  accumulator or event/absolute-time formulation is required; variable render
  cadence must not silently change the configured rate.
- Cursor state is simultaneously advanced by the GPU and predicted on the
  CPU. The CPU mirror advances only when emitter `generation` changes. A
  never-changing emitter works because its GPU cursor remains untouched, and
  the demo works because it uploads every frame, but changing a formerly static
  emitter can restore a stale cursor. This is dual ownership rather than a
  general fix.
- Counts are rounded to multiples of 256 so prefix-scan blocks do not cross
  emitter ranges, but the cumulative aligned range is not checked against the
  million-slot buffer. The declared 262,144 per-emitter limit is not used by
  the pass. Sixty-four individually valid descriptors can therefore describe
  ranges far beyond the allocation.
- Bitonic sorting requires a power-of-two network or explicit sentinel padding
  to the next power of two. The implementation accepts any multiple of 256 and
  stops stages at `k <= n`; a 768-slot emitter is not globally sorted. The
  power-of-two demo pools conceal this additional invariant.
- Sorting is per emitter and draws emitters sequentially. Particles are ordered
  within each pool but not against particles in another overlapping pool, so
  the implementation cannot eliminate the cross-emitter alpha errors the
  article uses to motivate sorting. It also sorts the full reserved pool,
  including dead sentinels, rather than the live count.
- Ring emission overwrites slots whether their old particles are dead or not.
  That may be a deliberate visual budget, but overload/drop policy must be
  explicit and observable. Removing emitters or changing ranges does not clear
  old slots, allowing previous live data to freeze or be interpreted under a
  new emitter layout.
- The Rust emitter described as 256 bytes is 240 bytes from the listed fields
  and padding. The buffers use `size_of`, so this prose error does not by itself
  corrupt the implementation, but it reinforces the need for executable
  layout assertions rather than hand-counted ABI documentation.
- The staging-to-indirect copy is a valid solution to Corona's conflict, not a
  universal law for GPU-generated draws. Its single render bind group also
  includes the storage-bound argument resource; then using that resource as an
  indirect input in the same render-pass usage scope conflicts. The
  [WebGPU usage-scope rules](https://gpuweb.github.io/gpuweb/#usage-scopes)
  make separate compute dispatches and a render pass distinct scopes. Separate
  per-pass layouts/access declarations can permit one resource with an
  appropriate transition on capable APIs; Lince should let its render graph
  choose rather than institutionalize an extra copy.
- Prefix scatter gives a stable destination for a given alive bitmap. It does
  not make variable-`dt` floating-point simulation and frame-number random
  seeding deterministic across devices, restarts or collaborators.

**Concrete Lince adapter.** Define a `VisualEffect` projection whose semantic
input is a bounded stream of attributed Box events and whose output is pixels
only. An emitter descriptor carries a stable local source id, effect style,
seed/age rule, transform source, finite bounds, blend family, pool budget,
sprite/material reference and customization tokens. The runtime resolves it to
GPU pages and disposable handles. The inspector uses the reflection-derived
Sand controls from entry 6 but writes a validated style transaction, never a
raw `GpuCoronaEmitter`.

The frame graph gives simulation, compaction, optional ordering and drawing
separate resource declarations. It can omit draw work when an effect is outside
the camera while preserving or reconstructing its visual age. Runtime health
shows allocated slots, live estimate/count, bytes, spawn drops, compute time,
draw time, sorting strategy and unavailable capabilities. Effects never
consume the frame budget invisibly merely because their emitter exists.

**Ideas to test.** Benchmark 10k, 100k and 1m allocated slots separately from
1%, 25%, 75% and 100% alive ratios; vary emitter counts, changes, overlap,
off-camera state and additive/alpha modes. Report CPU encode time, GPU time per
phase, memory and source-event-to-present latency on named hardware at the
current development resolution. Add correctness fixtures for fractional rates,
large `dt`, pool overflow, non-power-of-two counts, descriptor mutation,
emitter removal, device rebuild and overlapping transparent emitters. Run the
same effect alongside 10,000 native Sand actors and Area physics so a visually
impressive isolated demo cannot hide contention with Box's actual workload.

**Verdict:** keep Corona's GPU-resident state, sparse compaction, indirect draw,
atlas and debugging lessons in the rendering repertoire. Do not adopt Corona
itself as Box simulation or infer a million-Sand capability from its demo.
Lince may build or adapt a particle projection after its semantic actors are
correct, with explicit pool ownership, honest ordering, bounded costs and a
presentation-only contract.

[Tiled Light Culling: From Linear Scan to Forward+](https://pulsarnative.com/blog/2026-06-29-tiled-light-culling)

### 8. Screen-tiled lighting as renderer-private spatial binning

Read 2026-08-29. The article-day implementation was audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The central culling algorithm, fixed list bound and cache behavior remain the
same on current main.

**What it builds.** The deferred-lighting shader previously visited every
movable light for every shaded pixel. The new compute pass divides the render
target into 16×16-pixel tiles, constructs four lateral view-frustum planes per
tile, tests each movable light's bounding sphere against those planes, and
writes at most 64 light indices for the tile. The deferred pass then evaluates
only those indices. Directional lights are inserted into every tile; static
and stationary lighting is expected to have been baked instead of entering
this list.

The implementation dispatches one shader invocation per tile, with 256
invocations per workgroup. At 1920×1080 this is 8,160 tiles and an approximately
2 MiB index buffer; at 3840×2160 it is 32,400 tiles and approximately 7.9 MiB.
Every tile invocation still scans every movable light, making culling
`O(tile_count × light_count)`, but the expensive material and lighting
calculations run only for the smaller resulting lists. The article reports on
an RTX 4070 at 1920×1080 with 200 dynamic lights: roughly 1.8 ms for the old
linear deferred loop versus 0.12 ms culling plus 0.35 ms deferred lighting.

**Where it helps Lince.** Screen-tiled lists are a good private acceleration
structure for lights and other presentation work whose answer is inherently
camera-dependent. The renderer could use similar bins for visible Sand
decals, labels, selection outlines, effect emitters, topology/Area debugging
overlays and picking candidates. It is especially relevant once Box can move
between the planar view and a lit 3D view: one large light/effect collection
need not make every visible pixel inspect every item.

It is not an Area-of-Influence index, Box physics partition or source of saved
membership. Moving the camera changes screen tiles; off-camera actors have no
tile; a bounded render list may conservatively overinclude or deliberately
drop presentation work. None of those properties is allowed to change which
Records an Area attracts, where a Sand moves, or what collaborators persist.
Semantic broad phase stays in Box/world coordinates, remains active off
camera, and must produce a complete explainable result. A camera projection
may consume that truth and build disposable tile lists from it.

The reusable idea is therefore not “use the light culler for Areas.” It is to
give each projection an explicit dependency set and epoch, recompute only when
those inputs change, and keep its caches reconstructible. That can serve
lighting today and later serve world-space grids, BVHs or GPU broad phases,
but each structure has its own coordinate system and correctness contract.

**Needed for the capability to work.** A tile list must either contain every
light that can affect the tile or make overflow explicit and select a correct
fallback. Silently retaining the first 64 lights changes the picture according
to dense-buffer order. Lince needs an overflow counter visible in renderer
health, a diagnostic heatmap, and a defined policy: grow/reallocate within a
budget, run an overflow path, select by a declared contribution bound, or
refuse the configuration. Merely truncating is not a policy.

Frusta must be derived from the actual projection for the relevant view. This
includes projection jitter, off-axis projections, reversed depth conventions
and separate stereo eyes. Resource bindings and cache keys need explicit
resource identities/generations rather than host wrapper addresses. Insert,
remove, swap-remove, buffer replacement, camera change, render-resolution
change and device restoration must all invalidate exactly the projections
they affect. Debug UI must show list capacity, maximum/average occupancy,
overflow count, memory and whether the cull ran or was reused; an empty list
must distinguish “there are no movable lights” from “the pass is unavailable.”

The semantic/rendering boundary is also a correctness requirement. Screen
tiles may cull rasterization and make picking's first candidate set smaller.
They may not sleep Sand behavior, omit off-camera Area physics, decide persisted
positions, or become collaboration state. A hit-test refines a tile candidate
against the authoritative geometry before emitting a Box event.

**Needed only to make it fast.** The 16×16 tile size, 64-entry capacity,
single-thread-per-tile organization, cache, bind-group reuse and packed flat
buffers are performance decisions. So are per-tile depth bounds, a two-stage
light-centric binning pass, prefix allocation instead of fixed slots, clustered
3D cells and separate handling for large/global lights. Lince should choose
these from its real distributions, not encode them in Sand or Box schemas.

The present algorithm is attractive for tens or a few hundreds of movable
lights. Because every tile still scans all of them, it eventually only moves
the linear problem into compute. Depth-aware tiles reduce false positives
along a view ray; clustered shading extends the partition into depth; a
light-centric or hierarchical first phase avoids testing every light in every
tile. Those are scaling paths after a correct bounded implementation is
measured.

**Source-audit corrections and limitations.** The implementation narrows
several claims made in the article:

- The advertised static-scene cache does not skip normal rendered frames in
  the audited integration. `Renderer::render` calls `update_camera` every
  frame, and `update_camera` unconditionally increments `camera_generation`.
  The cull cache key includes that generation, so it misses even when the
  user-visible camera is static. Temporal jitter would also make the actual
  projection change. The benchmark table labels both a cache hit and miss as
  0.12 ms of compute; a true hit returns before dispatch and cannot have the
  same cull cost. The “90% of editor frames” claim is consequently not
  demonstrated by this path.
- The shader says it unprojects tile edges, but it divides NDC coordinates
  only by the projection matrix's two diagonal scale values. It does not use
  an inverse projection and ignores projection-center offsets. This works as a
  shortcut for a symmetric perspective camera, not for arbitrary off-axis,
  jittered or stereo projections. Current main stores multiple cameras but the
  culler and deferred shader still use `cameras[0]`; a second eye cannot simply
  reuse these lists as though its frustum were identical.
- A tile silently stops accepting indices at 64, including directional lights
  that occupy the same budget. There is no overflow flag, metric or fallback.
  Visual correctness can therefore fail precisely in the dense-light case the
  feature is intended to support.
- The four side planes pass through the camera and there is no near/far plane
  or tile depth range. The article acknowledges behind-camera false positives;
  lights elsewhere along the same view ray are also conservatively admitted.
  A spot light is represented by its bounding sphere, so narrow cones can
  occupy many unnecessary tiles. These make lists larger but are safe until
  the fixed 64-entry cap turns overinclusion into dropped real contributors.
- `movable_lights_generation` changes for every movable-light update, including
  color/intensity edits that do not necessarily alter tile membership. In the
  article-day code it does not change on insertion or removal; count catches a
  simple single edit but not every same-count remove/insert/reordering sequence.
  Current main marks its light buffer dirty for membership edits but retains
  the same cull cache key. Cache dependencies should describe culling data,
  not be both broader and less complete than it.
- The bind-group cache compares the addresses of Rust buffer-wrapper objects.
  A wrapper can keep its address while replacing its internal GPU buffer, so a
  pointer is not resource identity. Reallocation needs a generation carried by
  the resource owner.
- The 500-light and 4K figures are forecasts in the prose rather than rows in
  the reported measurement. The named RTX 4070, resolution and separate pass
  figures make the 200-light result more useful than an anonymous demo, but
  the article does not state its timing method, scene distribution, warm-up or
  variance. The culling pass itself requests no timestamp writes; an outer
  graph profiler may still have measured it, but that method is not shown.

**Concrete Lince integration.** Define a renderer-owned `ScreenBinProjection`
whose inputs are a view id, exact view/projection state, render extent, bounded
presentation-object snapshot and resource generations. Its output is a
disposable GPU list plus health counters. Lighting, decals, visible labels,
effect rendering and coarse pointer candidates may each specialize this
pattern rather than share a falsely universal list.

Box separately maintains a camera-independent world spatial index for Sands,
Areas and topology effects. That index participates in fixed-step simulation
and explanation whether or not an actor is visible. A rendering adapter reads
the resulting transforms and bounds, culls presentation to the camera, and
may then tile the visible candidates. In 3D, clustered world/view-space bins
may eventually fit lighting better; in 2D, screen tiles may be sufficient for
decals and picking. Neither representation crosses the persistence or external
Sand ABI.

**Ideas to test.** Build a correctness oracle that compares tiled output with
an unbounded linear-light render for symmetric, jittered, off-axis and stereo
cameras. Exercise tile edges, the near plane, behind-camera lights, huge
ranges, narrow spots, more than 64 contributors, resize and device rebuild.
Remove and insert lights without changing total count and prove cache
invalidation. A debug view must color occupancy and overflow so the owner can
inspect the feature in the running interface.

Benchmark the linear and tiled paths with 0, 50, 200 and 1,000 movable lights
in uniform, tightly clustered, screen-filling and behind-camera arrangements.
Record CPU encoding, cull GPU time, deferred-light time, list memory,
occupancy/overflow and cache hit rate on named hardware at the current
development resolution, both with moving and static camera/lights. Run the
test beside 10,000 native Sand actors and active Area physics. A 4K benchmark
is not a present gate, but buffer sizing remains resolution-derived so the
architecture does not assume that today's resolution is permanent.

**Verdict:** borrow GPU-built bounded candidate lists, explicit dependency
epochs, occupancy diagnostics and the separation between culling and costly
shading. Correct the projection, cache and overflow model before adopting the
implementation. Most importantly, keep tiled culling a disposable camera-side
acceleration; Box's Areas, topology and off-camera behavior require a distinct
complete world-space simulation structure.

[Designing a Render Graph That Doesn't Get in the Way](https://pulsarnative.com/blog/2026-06-29-render-graph-design)

### 9. Compile a typed frame plan; do not confuse declarations with a graph

Read 2026-08-29. The article's `helio-v3` graph was audited at the article-day
commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and its replacement in `helio-core` was inspected on current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The current code has materially changed, but several capabilities described by
the article are still scaffolding rather than working resource-graph features.

**What it proposes.** Helio replaces a hard-coded render sequence with native
Rust pass objects. Each `RenderPass` declares named reads/writes, prepares CPU
uploads, records GPU commands, and publishes references for later passes. A
`FrameResources` struct carries these references in `Tracked<T>` slots. The
graph creates inter-pass textures at resize, caches texture views, detects
missing prior writers, keeps a `TypeId` index for configuring concrete passes,
and reports resource/pass data to a debug overlay.

The article also describes three optimizations: rebuilding bind groups only
when growable buffers are replaced, aliasing transient textures whose lifetimes
do not overlap, and fusing adjacent passes so tile GPUs can retain attachments
on chip. A wrapper pass illustrates cross-cutting instrumentation by enclosing
another pass with pre/post GPU analysis.

**What should shape Lince.** A compiled frame plan is the right foundation for
native Sands, topology, the 2D/3D Box views, embedded browser surfaces, picking, lighting and
diagnostics. Rendering dependencies should be stated once and validated before
the first frame. Optional capabilities should compile into a valid variant of
the plan rather than leave every pass guessing whether an input happens to be
present. The same plan should expose enough health data that the owner can see
which projection failed or consumed the frame.

Native passes should be typed Rust code. Recompilation is acceptable for this
project, and an external HTML Sand does not imply an externally authored GPU
pass: the embedded browser produces an imported/composited surface that enters through one
mediated adapter. Sand definitions, customization and Box configuration must
not serialize renderer implementation order. A future trusted shader/plugin
system can compile a restricted declaration into the internal plan; it should
not make an arbitrary JSON graph the renderer's source of truth.

The useful wrapper idea generalizes to pass instrumentation, but composition
should occur at plan compilation rather than nesting opaque objects blindly.
Timestamping, debug markers, validation scopes, overdraw capture and output
inspection are graph policies applied uniformly. They can be enabled without
changing each pass and without adding two hidden dispatches around work whose
resource requirements the scheduler cannot see.

**Needed for the capability to work.** Every pass and logical resource needs a
stable typed/generational handle. A resource descriptor includes kind, format,
extent policy, layers, mip/sample count, usages, initialization/load policy,
history/persistence class and import/export ownership. An access declares
read/write mode and pipeline stage, not just a string. The compiler rejects an
unknown handle, read-before-write, incompatible multiple writers, descriptor
disagreement, uninitialized load, usage/format mismatch, cycle, absent required
output and a resource used after its lifetime. Optional inputs state an honest
fallback at plan construction.

The graph is locked only after mandatory validation succeeds. Registration
order may break ties but does not define dependencies: the compiler constructs
a DAG, derives a deterministic topological order, and shows that order in the
running diagnostic Sand. Structural changes produce a new immutable plan and
swap it at a frame boundary. Per-frame values—camera, transforms, embedded browser frame,
selection and customization—update resources without rebuilding the graph.

Start with one command timeline/encoder whose recorded order is the compiled
order. Multiple encoders, queues or asynchronous compute require the scheduler
to derive explicit submission dependencies; simply recording compute and
render commands separately and submitting all compute first can reverse a
render-to-compute dependency. Pass code does not choose an escape-hatch encoder
that the graph cannot reason about.

Imported browser and eventual native external-memory surfaces declare acquire,
usable extent/format/color space, last-use and release. A failed or late embedded browser
frame reuses the last accepted surface or shows an explicit placeholder; it
does not stall Box semantics. Swapchain acquisition, minimized/zero extent,
resize, device loss and adapter-capability changes have similarly explicit
states. If command recording fails, abandon that disposable frame, retain the
last presentable result where possible, surface the failing pass, and rebuild
from authoritative Box state. Rendering rollback is not allowed to roll back
Records or physics.

Resource identity is an owner-issued id plus allocation generation. Bind groups
cache against every bound resource generation, view descriptor, sampler and
layout generation. A Rust wrapper address is not a GPU-resource identity. The
frame plan owns view creation and recreates dependent objects after resize or
device restoration.

**Needed only to make it fast.** Eager allocation at plan/resize time, cached
views, bind-group caches and zero hot-path string lookup are sensible starting
choices. Physical-memory aliasing, render-pass merging, render bundles,
parallel command recording, asynchronous compute, dynamic-resolution resource
variants and inactive-history eviction are optimizations. Each comes after the
compiler can prove its lifetime/order constraints and a measurement shows its
value.

Logical transient aliasing can reuse one compatible texture for disjoint
lifetimes, but compatibility includes dimensions, format, sample/mip/layer
shape and the union of usages. The allocator maps several logical handles to a
physical allocation; an “alias group” label is not evidence that sharing
occurred. The diagnostic view reports logical bytes, physical bytes and actual
savings separately.

Likewise, WebGPU/wgpu does not offer the article's proposed
`next_subpass()`/input-attachment API. Lince may keep a single render pass open
across several draw producers only when they use the same attachments and
compatible load/store semantics. That is useful for layers of native Sand
geometry or several world-geometry producers. It is not GBuffer-to-deferred
subpass fusion: sampling a GBuffer while it remains a render attachment needs
a backend capability/model WebGPU does not expose. Treat backend-specific
Vulkan render-pass/subpass work as a later measured native specialization, not
as the portable graph contract.

**Source-audit corrections and limitations.** The article-day implementation
does not substantiate several of its central claims:

- `GraphTexturePool::allocate` always creates and pushes a new texture. It does
  not search or reuse an alias allocation. `ResourceDecl` has no alias field,
  every collected lifetime receives `alias_group: None`, and `release()` is not
  called. The claimed SSAO/Hi-Z sharing and greedy interval packing therefore
  do not happen. Current main assigns some alias-group strings but the pool
  still allocates one texture per logical resource.
- Current main groups “chain-local” resources by their first writer and states
  that resources written by the same pass are never simultaneously alive.
  Multiple color attachments written by one pass are necessarily concurrent.
  The dormant allocator hides this error; implementing reuse literally would
  alias live attachments and be incorrect.
- Article-day subpass detection only checks whether adjacent string sets
  intersect, stores the ranges for display, and never uses them in execution.
  Every pass descriptor opens a separate wgpu render pass. The article itself
  later concedes this, contradicting its earlier description of a shared pass.
  Current main can keep one wgpu render pass open, but restricts the chain to
  identical attachment-view signatures; it still is not input-attachment
  subpass fusion and cannot realize the stated GBuffer → deferred example.
- `validate_dependencies()` exists but is not called by construction, lock or
  execution in the audited code. It examines only legacy `reads()`/`writes()`,
  not the builder declarations used for allocation. It does not check types,
  formats, access modes, multiple writers or cycles, and pass order remains
  registration order on current main.
- Duplicate resource writes are folded through `HashMap::entry(...).or_insert`,
  silently keeping the first descriptor. Unknown names in the hard-coded
  routing function silently do nothing. The article says a unit test verifies
  every publication/slot signature, but no such test is present at the audited
  commit or current main.
- `Tracked<T>` is a useful local assertion, not a resource contract. In release
  it is an `Option`; callers may use untracked `get()`. In debug, `read()` only
  detects an empty, never-written slot. It cannot prove that the value has the
  right logical identity, dimensions or generation. `FrameResources` is newly
  created by the high-level renderer each frame, and the described
  `reset_tracking()` “one-frame grace” is never called. Its implementation
  actually re-marks populated fields as written rather than invalidating them.
- A raw pointer tuple can notice replacement of the Rust `wgpu::Buffer` value,
  but it is a fragile, incomplete cache key and must be updated manually for
  every binding. The preceding tiled-light audit shows a wrapper address can
  also remain stable while its owned GPU allocation changes.
- `TypeId` lookup is a runtime hash lookup—average, not guaranteed, O(1)—and
  `add_pass` indexes only the first pass of a duplicate concrete type. The
  generic caller gets type checking, but a renderer that repeatedly reaches
  inside concrete passes is tightly coupled. Lince should expose typed control
  capabilities/handles rather than make higher layers know pass structs.
- The two lifecycle phases are interleaved per pass (`prepare A`, `execute A`,
  `prepare B`, `execute B`), not two global phases. This can intentionally let
  B prepare after A publishes, but prevents preparation from being parallelized
  as a phase and makes queue-write/command-order semantics part of the hidden
  contract.
- The graph creates both render and compute encoders, while individual passes
  decide which raw pointer to use. Current `PassContext::begin_compute_pass`
  records into the separate compute encoder and submission always places the
  entire compute encoder before the render encoder. The declaration graph does
  not enforce that all such compute work is independent of earlier render
  results. This is manual scheduling behind a graph-shaped API.
- The debug VRAM estimate multiplies width, height, layers and nominal bits per
  pixel. It omits mips, samples, padding and actual alias allocation, so it is a
  useful rough logical estimate rather than authoritative physical VRAM or
  savings. The article's 150–200 MB pool figure is not accompanied by a
  captured resource table or measurement.

**Concrete Lince frame plan.** Compile a `NativeFramePlan` from a fixed registry
of native pass factories and current adapter capabilities. It contains typed
nodes for world simulation upload, native Sand geometry, terrain/topology,
opaque/transparent 3D, embedded browser surface import/upload, effects, picking ids,
selection/debug overlays, color composition and presentation. The exact pass
set may differ between the planar, 3D and mixed view, but their outputs meet a
common compositor contract and swaps occur only at frame boundaries.

Box simulation remains outside this graph. It advances its complete fixed-step
world and publishes an immutable render snapshot. The graph can omit
off-camera draws, but it cannot decide whether behavior advances. A browser surface similarly
continues according to the Sand lifecycle; its latest presentable surface is
only one graph input. This keeps frame-graph failure, aliasing and renderer
reconfiguration from becoming database or collaboration behavior.

Ship a `Renderer Health` Sand with the mechanism. It shows the compiled pass
order, required/optional edges, resource descriptors, logical/physical bytes,
allocation generations, view and browser imports, cache rebuilds, CPU/GPU timings and
the last validation/runtime error. It can select a pass output for inspection
and states honest empty cases such as “3D lighting is disabled in this plan” or
“website has not produced its first frame.”

**Ideas to test.** Unit-test graph compilation with missing writers, cycles,
duplicate writers, conflicting descriptors, wrong usages, invalid loads,
optional branches and deterministic ordering. Property-test compatible
interval allocation against overlapping lifetimes and require distinct
physical ids for every live overlap. Snapshot the compiled plan so a pass edit
cannot silently reorder the frame.

Run wgpu validation tests for minimized/restored windows, repeated resize,
render-scale changes, embedded browser frame loss/replacement, device recreation and each
adapter capability profile. Inject a recording failure in every pass and prove
the frame is abandoned with a visible diagnostic while Box continues. Compare
one-encoder correctness with any proposed multi-encoder schedule before
accepting its speedup. Benchmark eager versus aliased allocation and pass
merging at the current development resolution; retain resolution-derived
descriptors so higher resolutions remain possible without making 4K a present
gate.

**Verdict:** adopt typed in-tree passes, a prepare/record lifecycle, retained
resource ownership, generation-based cache rebuilding and a first-class graph
inspector. Do not adopt Helio's string routing, `TypeId` control coupling,
manual encoder selection, uncalled validation or advertised alias/subpass
features. Lince needs a real graph compiler whose validated immutable frame
plan owns resource identity, order, synchronization and recovery.

[Real-Time Global Illumination with Radiance Cascades](https://pulsarnative.com/blog/2026-06-29-radiance-cascades-gi)

### 10. Directional probe fields are future presentation, not v1 truth

Read 2026-08-29. The article-day pass and its dormant ray-query shader were
audited at
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747),
then compared with the partially activated implementation on current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The article is commendably explicit that its published GI does not ship, but
the proposed shader is much farther from a correct switch-over than a wgpu
upgrade.

**What it proposes.** The design places 8×8×8 probes around the camera and
stores 16 octahedrally encoded directions per probe. All 8,192
probe/direction samples fit in a 32×256 `Rgba16Float` texture: 64 KiB. A
deferred shader reconstructs a surface's indirect diffuse light by trilinearly
interpolating eight probes and cosine-weighting their direction bins. Outside
the probe volume, hemisphere ambient remains available; baked lightmaps take
priority on static geometry.

The intended producer uses hardware ray queries. Each probe-direction ray
finds geometry, evaluates direct lights at the hit, and stores that radiance.
The proposed shader also contains parent-cascade and temporal-history inputs.
The article-day Rust pass cannot compile that shader on its wgpu version, so it
instead dispatches a shader that writes five percent of the sky color into
every atlas texel. The author correctly says this is an initialized interface
and ambient fallback, not working global illumination.

**Where the ideas help Lince.** Lince's future world view will have dynamic
topology, user-built scenes, moving Sands and imported scene representations.
A camera-local directional light field could make those changes feel grounded
without baking every arrangement. Compact atlases, explicit spatial coverage,
capability-driven quality tiers and a smooth visual fallback are useful
rendering patterns. The atlas and all temporal samples remain disposable
projections; they are never Records, topology, physics, Area membership or
collaboration state.

The material boundary matters more than GI ambition. Pinned/HUD Sands and embedded browser
web surfaces should be color-managed, sharp and normally unlit/emissive so a
website or control does not change contrast as a virtual lamp moves. Native
Sands placed on the topology may opt into a world-lit shell, cast/receive
shadows, or remain unlit. World geometry, avatars and decorative/game objects
are the natural GI receivers. Area colors, selection, arrows and “Why is it
here?” explanations stay legible in a flat/unlit diagnostic mode.

This is not required for the v1 interface. Direct lighting, ambient/environment
light, shadows and careful material/color treatment are sufficient to validate
the planar-to-3D Box. The frame plan from entry 9 should reserve an optional
indirect-light capability and fallback edge; building a ray-query GI system
before native Sands, topology and embedded browser composition are correct would validate
the wrong risk.

**Needed for an eventual GI capability to work.** The producer and consumer
must share the exact probe origin, spacing, orientation, cascade count and
atlas layout. A camera-local grid either snaps to world-cell increments and
scrolls/reuses overlapping history, or reprojects history into the new grid.
Newly exposed cells, camera cuts, teleports, topology changes and large Sand
motion need explicit invalidation; blending unrelated old world positions for
several frames is not recovery.

Each texel needs separate radiance and confidence/validity information. A dark
but valid room must remain dark rather than be replaced by a brighter
hemisphere merely because radiance magnitude is low. Probe visibility,
distance moments, relocation/classification or another tested leakage control
is needed so trilinear interpolation does not carry light through walls or
across thin topology. Material albedo, emissive contribution and a defensible
surface normal/BRDF are required at a ray hit if the result is to represent
bounced light rather than direct-light samples in empty space.

The capability contract includes BLAS/TLAS construction, stable geometry and
material ids, incremental rebuild/refit policy, masks, dynamic-object updates,
feature/backend detection, device restoration and a non-RT plan. Unsupported
hardware never creates a nominal “GI pass” that silently does nothing. The
Renderer Health Sand states `off`, `ambient`, `screen-space`, `probe RT` or a
future technique by its honest name and exposes probe bounds/history age.

History textures must genuinely ping-pong or use disjoint subresources/passes;
the shader cannot sample and storage-write the same texture in one usage scope.
The first valid sample should not be attenuated as though a black history were
real data. Temporal alpha should be time-based or defined against a fixed GI
update cadence, and confidence/reset masks determine where accumulation is
allowed. GI updates may be camera-local and presentation-culled without
sleeping off-camera Box behavior.

**Needed only to make it fast.** Probe count, directions, cascade count,
update fraction, ray budget, atlas format, workgroup size, history alpha,
screen-space reuse and TLAS refit cadence are performance/quality choices.
Sparse or clipmapped probes, priority updates near changes, ray rotation,
denoising, reservoir reuse and hardware RT are later options. The 64 KiB base
atlas is cheap; ray traversal, hit shading, acceleration-structure maintenance
and downstream sampling are the actual costs.

Evaluating every scene light at every primary hit and launching up to four
additional shadow rays per point/spot light scales far beyond the advertised
8,192 primary rays. Lince should reuse a bounded light-selection structure or
sample lights stochastically rather than make GI cost
`probe rays × all lights × shadow samples`. A single dispatch and low CPU cost
do not imply low GPU cost.

**Source-audit corrections and limitations.** The code contradicts or narrows
several technical descriptions:

- The article-day runtime contains one 32×256 2D texture and no array layers,
  parent atlas, history textures, static cascade uniform, TLAS or light binding.
  It implements one ambient-writing level, not multiple cascade slices,
  hierarchy merging, temporal accumulation or multi-bounce GI. The 64 KiB
  calculation is for the entire declared texture, not “per slice.”
- The runtime producer uses fixed bounds `[-10, -1, -10]` to `[10, 10, 10]`.
  The deferred consumer independently treats the same atlas as covering the
  camera-centered `GiConfig::rc_radius`, which defaults to ±80. Activating the
  full producer unchanged would write and sample different world volumes.
  Current main retains the fixed producer bounds.
- `rc_fade_margin` is declared and given defaults but never read. The deferred
  shader instead hard-codes a normalized five-percent boundary fade. The
  advertised configurable 20-world-unit transition does not exist.
- The dormant WGSL `GpuLight` does not match the article-day Rust light buffer.
  It reads Rust's range as `light_type`, outer spot cosine as `range`, shadow
  index as a float inner cosine, and type bits as an outer cosine. Current
  `libhelio` explicitly notes that this mirror has diverged, yet current main
  can select and compile it when ray queries are enabled.
- The shader's top-level contract says sky misses carry throughput 1, but both
  hit and miss branches assign throughput 0. Parent radiance is multiplied by
  that value and can never contribute. Even if corrected, extending a ray
  through empty space/parent intervals is not multiple diffuse bounces. Every
  geometry hit terminates and samples direct lights; there is no recursive
  reflected transport. Temporal averaging reduces noise but does not create
  additional light bounces.
- Ray hits use `±ray_direction` as their surface normal and do not fetch
  geometry normals, material albedo or emissive data. This is not a small
  quality refinement: it changes which lights illuminate the hit and omits the
  surface response that turns incident light into bounced radiance.
- The deferred pass treats `clamp(length(rc_irr) * 4)` as RC validity. A valid
  low-energy probe field is assigned low confidence and replaced by ambient;
  bright data is considered valid. Availability and radiance are different
  quantities and cannot share this heuristic.
- With EMA alpha 0.15, six frames retain about 38 percent of the old sample;
  they are not “fully converged.” About 18 frames are needed for 95 percent and
  29 for 99 percent after a step. The article later gives the more honest
  roughly-30-frame figure. Camera cuts are therefore not handled merely by
  waiting “a few” frames, especially because history is not spatially
  reprojected.
- The current fallback is no longer five-percent sky; it screen-space marches
  Hi-Z and `pre_aa`. In every inspected default graph, however, the RC pass is
  registered before geometry/deferred lighting creates `pre_aa`. Its fallback
  returns early when that view is absent, so it performs no dispatch. This is
  exactly the read-before-write error mandatory graph validation in entry 9
  must catch.
- Current RT setup binds `rc_cascades` simultaneously as storage output and
  sampled parent, and binds the single `rc_history` texture simultaneously as
  sampled history and storage history output. Those are not ping-pong buffers
  and conflict in a WebGPU usage scope. It also creates the RT views and bind
  group every frame. The portability tests request no ray-query feature and
  exercise only the fallback, leaving this path unvalidated.
- The claimed “swap the shader and bind a TLAS” understates the work. A working
  path also needs correct layouts, acceleration-structure population/update,
  material lookup, cascade resources/order, non-aliasing history, grid
  movement, invalidation, synchronization, capability tests and visual
  correctness tests. Current Helio's later additions confirm that this is an
  architecture, not a dependency bump.

**Concrete Lince direction.** For v1, define a `WorldLightingProjection` output
in the native frame plan with an explicit implementation enum and validity
mask. Initially provide `Unlit` and a predictable ambient/direct/shadow path.
Keep Sand UI composition independent of that path. When a real dynamic GI
experiment begins, add it as a capability-selected producer of the same
indirect-diffuse contract, with its own inspector and a one-click comparison
against ambient.

Use topology and Sand transforms only through the immutable render snapshot.
Geometry changes mark affected lighting regions dirty, but GI never writes
back into Box. A topology valley can look darker because presentation computes
occlusion; its Area attraction and persisted shape remain identical with GI
disabled. embedded browser textures normally composite after world lighting. An explicitly
world-integrated website panel may light only its frame/backing mesh while the
HTML pixels remain color-correct emissive content.

**Ideas to test.** Begin with analytic scenes: an enclosed dark room, a window
lit room, two colored diffuse walls, a thin separator, moving occluder, moving
emissive object and a camera teleport. Compare probe output and final irradiance
against a slow reference or tightly bounded expected behavior. Prove that
valid black differs from unavailable, no light crosses the separator beyond a
defined tolerance, history resets/reprojects correctly, and producer/consumer
bounds match exactly.

On each supported named adapter/backend, validate the feature-selected graph,
shader layouts, TLAS update, resize/device restoration and history ping-pong.
Measure TLAS CPU/GPU cost, primary/shadow ray counts, GI pass time, deferred
sampling time, convergence/error and memory while native Sands and Area physics
run concurrently at the current development resolution. Vary dynamic geometry
and light counts rather than presenting one empty-atlas dispatch as GI
performance. The owner-facing inspector must show probe cells, confidence,
radiance directions, dirty/history state and active fallback.

**Verdict:** preserve compact directional probe fields, explicit coverage,
capability tiers and graceful presentation fallback as v2 research. Do not put
this implementation on the v1 critical path or call a wired atlas “GI.” A
future Lince implementation needs real material-aware transport, coherent
world bounds, validated history/cascade resources, leakage controls and honest
visual diagnostics before it can replace ambient lighting.

[Building a Zero-Instrumentation GPU Profiler in wgpu](https://pulsarnative.com/blog/2026-06-29-gpu-profiler)

### 11. Measurement is a human-facing capability, and attribution must be real

Read 2026-08-29. The profiler, frame-graph integration, culling readback and
portal were audited at the article-day Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747),
then compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
Current main substantially improves bounded asynchronous readback and exposes
its health, but its pass-level GPU markers still do not bracket most pass work.

**What it builds.** When supported, Helio allocates a 256-entry timestamp query
set, a `QUERY_RESOLVE | COPY_SRC` GPU buffer and a
`COPY_DST | MAP_READ` staging buffer. Every graph pass is assigned two query
indices. The executor writes timestamps around the pass, resolves the used
range, copies it for CPU mapping, multiplies the tick delta by the queue's
timestamp period and associates it with the pass name. Unsupported adapters
fall back to CPU timing.

CPU preparation is timed with an RAII guard. Separate GPU atomic counters
report culling outcomes. The results can be drawn into an in-frame character
grid or sent to a loopback web portal with charts, graph views and recording.
The article also describes the ownership lesson that an embedded renderer
should not independently drive `Device::poll` when the host owns the device.

The three-stage timestamp readback pattern is sound. A query set is not a
buffer, the resolve destination needs `QUERY_RESOLVE`, and WebGPU-compatible
mapping usage requires a separate `MAP_READ | COPY_DST` staging resource. The
important optimization is not removing one of these resources; it is putting
several staging slots in flight and consuming an older completed frame without
stalling the current one.

**Where it helps Lince.** Smoothness is a product capability, not a private
developer intuition. The `Renderer Health` Sand proposed in entries 8–10
should ship with the native interface and combine render-plan inspection with
timings, budgets, queue/readback state and failure explanations. It is how the
owner tells whether moving one Sand is expensive because of Box simulation,
snapshot extraction, native UI tessellation, browser upload/composition, culling,
topology, world rendering, GPU submission, presentation or synchronization.

Automatic graph instrumentation is the right default, but “zero
instrumentation” should mean passes inherit correct coarse scopes. It does not
mean a graph can infer meaningful nested work after passes escape into raw
encoders. Every measured command must occur between markers on the same GPU
timeline, or the displayed number is worse than absent: it confidently directs
optimization toward the wrong subsystem.

**Needed for the capability to work.** The compiled frame plan assigns stable
unique scope ids, labels and parent relationships. Names are display text, not
identity; several instances of the same pass or Sand projection remain
distinct. CPU spans separately measure simulation, render-snapshot extraction,
prepare/upload, command recording, submit/present and browser work. Per-pass CPU
recording is not reported as total frame CPU, and `Instant` scopes are never
labelled GPU time.

GPU begin/end markers are encoded on the command encoder and queue timeline
that contains the measured commands. For a render or compute pass, use the
descriptor's pass timestamp writes when supported, or same-encoder markers
immediately outside the pass. A pass that creates multiple GPU passes exposes
nested scopes through the frame-plan recorder. Multiple command encoders or
queues carry explicit submission order and scope attribution; a marker pair
on an unrelated encoder is rejected by the profiler API.

Readback uses a bounded ring. Each slot retains the exact frame id and scope
table whose queries it contains. If every slot is busy, the profiler drops the
new sample and increments a visible counter rather than stalling or overwriting
in-flight data. Query-capacity overflow similarly remains visible and does not
emit an out-of-range command. The UI reports whether GPU timing is disabled,
unsupported, pending, available, stale/backpressured or failed, along with the
age of the shown GPU frame.

Device maintenance belongs to one explicit owner. Lince's native runtime plans
to own wgpu, so its event loop can service mapping callbacks without a blocking
wait. If rendering is embedded later, the host contract provides a maintenance
cadence/completion channel; the child never assumes that `Device::poll` is
universally unsafe or universally its right. Device loss cancels mappings,
recreates query resources and leaves the last sample visibly stale.

GPU counters need a versioned shared Rust/WGSL layout and exact definitions.
“Total,” “submitted,” “tested,” “visible,” “culled” and “drawn” cannot be
interchanged. Reset, production, copy and consumption are ordered in the frame
plan. Counter readback uses the same bounded asynchronous mechanism as timing.
Instrumentation can be sampled or disabled, but absence is shown honestly.

The shipped Health Sand provides the human surface in the same task. Its empty
states say why there are no measurements. It can freeze/capture a bounded
window and export it only on request with adapter/backend, driver, resolution,
quality configuration, build revision, scope schema, frame ids and profiler
overhead mode. Record values, website contents and private Box data are not
telemetry payloads.

**Needed only to make it fast.** Query/ring capacities, readback delay, sample
frequency, counter sampling, history length, percentile windows and overlay
refresh rate are performance choices. Reusing vectors/strings, avoiding a
per-frame JSON serialization, downsampling graphs and calculating summaries on
a worker all help after attribution is correct. Profiling overhead is measured
by alternating equivalent instrumented/uninstrumented captures, not assumed
from a per-command nanosecond estimate.

The overlay need not redraw at render frequency. Timings may arrive every frame
while charts update at a readable bounded rate. Likewise, expensive overdraw,
shader-complexity and texture inspection modes are explicit captures, not
always-on wrappers silently adding work to every pass. Normal health counters
remain cheap and bounded.

**Source-audit corrections and limitations.** Several headline claims do not
match the article-day implementation:

- The graph writes every pass's GPU start and end timestamps into
  `compute_encoder`, while most render and many compute commands are recorded
  into a different `encoder`. At submission it sends the entire compute command
  buffer before the render command buffer. The marker pair therefore brackets
  no render-pass work and often only two adjacent timestamp writes. It cannot
  produce the advertised per-pass GPU duration. Current main retains the same
  placement while adding more render-pass chaining.
- CPU RAII scope creation occurs only around `pass.prepare()` and is dropped
  before texture routing and `pass.execute()`. The result is preparation CPU
  time, not “prepare + any CPU-side work” or per-pass CPU total. Timings are
  stored in a `HashMap` keyed by static name, so repeated names overwrite one
  another; the documented stack/timing tree is not implemented.
- The article-day capacity has no bounds check. More than 128 measured pass
  instances writes beyond the 256-query set. Current main now refuses the
  excess and counts `query_overflows`, which is the right pattern, though those
  scopes still need to appear as unavailable rather than vanish.
- The article-day external-device readback function performs no mapping and
  always clears pending metadata. It does not lag by one or two frames; it can
  never produce a new GPU sample. Current main fixes this with three staging
  slots, asynchronous mapping driven by the external owner, frame-age metadata,
  drop counts and bounded backpressure. This current design is worth borrowing.
- Owned-device mode maps the just-submitted frame and calls
  `poll(wait_indefinitely)` every frame. In a normal CPU-ahead render loop this
  can wait for the remaining GPU frame, serialize frames in flight and cost far
  more than “under 100 µs.” The 0.2 ms profiler and 0.3 ms total-overhead figures
  have no shown methodology and cannot be assumed. Current main keeps this
  blocking owned-device path even though it has a ring.
- The warning that any poll by an external renderer universally corrupts driver
  state is too broad. Concurrent maintenance without an ownership/scheduling
  contract can certainly be wrong, and the host must own it, but wgpu's correct
  integration is not “callbacks never progress.” The current ring demonstrates
  the better contract: enqueue mappings, let the owner maintain the device, and
  consume completed slots later.
- The live portal is not a plain dependency-free HTML/JS fetch loop. The audited
  page loads vendored React/ReactDOM/ReactFlow, a Google-hosted font and Lucide
  from `unpkg.com`, and connects by WebSocket. `live-portal` is optional and is
  not in Helio's default feature list, contrary to the claim that it starts by
  default. Lince should not add automatic network fetches or a listening
  diagnostic server to obtain local renderer health.
- The portal's bridge begins with an unbounded standard channel and serializes
  every published snapshot. If consumers or serialization fall behind, memory
  and CPU work are not bounded by the WebSocket broadcast capacity. A latest-
  value channel or fixed capture ring better matches live telemetry.
- Culling stats are copied in another submission and then synchronously mapped
  and polled every owned frame in the article-day renderer, adding a second
  stall. Current main has replaced this with callback-driven non-blocking state,
  another improvement to retain.
- “Eight atomic increments per draw” is not the source's semantic cost model;
  different shaders increment different counters as candidates traverse
  stages. Contention, invocation count and GPU architecture determine cost.
  Multiplying an assumed 50–100 cycles by 10,000 and converting it directly to
  0.05–0.1 ms is not a benchmark.
- The portal/overlay totals sum per-pass values by name. Even after marker
  placement is fixed, total GPU frame duration should come from a frame scope;
  summing children is misleading when queues overlap or scopes nest. CPU frame
  duration likewise needs its own root span.
- Profiling is not literally zero initialization cost when its Cargo feature is
  disabled: `Profiler::new` still constructs `GpuProfiler`, which allocates
  query/readback resources when device features allow them, while runtime calls
  are gated by `enabled`. This is minor, but it exemplifies why overhead is
  measured rather than inferred from `cfg!` prose.

**Concrete Lince telemetry model.** Let the `NativeFramePlan` emit a bounded
`InterfaceTelemetrySnapshot` after submission. It contains schema/build id,
CPU/GPU frame ids, availability/lag, root frame/present spans, stable scope
records, resource/candidate counters, readback health and current renderer
configuration. A latest-value slot feeds Renderer Health without blocking the
render thread; an explicit recording action copies selected snapshots into a
bounded capture.

Renderer Health has three levels. The default shows frame pacing, input-to-
present latency, CPU/GPU root time, Box fixed-step backlog, embedded browser frame age and
the dominant scope. Expanding shows the frame-plan tree and percentile/history
charts. Capture mode enables heavier views—overdraw, tile occupancy, topology
field, allocation map, picking and pass outputs—with their own measured cost.
This is composed from native Sands using the same customization tokens, not a
separate aesthetic/debug UI system.

Keep a small offline report viewer possible by exporting a documented capture
file, but make it opt-in and self-contained. A future remote session requires
explicit authority, authentication and redaction. Renderer profiling is not a
reason for Facade viewers, collaborators or external Sands to learn private
Record contents or control the graph.

**Ideas to test.** Create deterministic GPU workloads of known increasing
duration and prove reported scope order and ratios. Put commands on the wrong
encoder intentionally and require plan validation to reject the scope. Test
duplicate names with distinct ids, nested scopes, 129+ passes, unsupported
features, mapping failure, three occupied slots, device loss and frame-id wrap.
Ensure stale GPU data is visibly paired with its original CPU/config frame.

Measure profiler overhead with timestamps/counters/overlay independently
enabled and disabled over long alternating runs on named hardware. Report
median, p95 and p99 frame pacing, not one frame. Saturate the render thread and
GPU separately to show non-blocking readback does not serialize them. Verify
the health UI remains responsive and bounded when the embedded browser stops producing frames,
the window is minimized, Box contains 10,000 actors, and a heavy capture mode
is activated.

**Verdict:** make correct automatic measurement and its in-app Health Sand part
of the v1 foundation. Borrow timestamp-query readback, bounded current-main
rings, explicit lag/drop/availability, shared shader counters and capture
views. Reject unrelated-encoder timestamps, blocking per-frame readback,
name-only identity, unbounded telemetry and unsupported performance constants.
No later Lince performance claim is accepted until this instrumentation can
show what was measured on the actual frame timeline.

[Editor Gizmos: 3D Transform Manipulation in Rust](https://pulsarnative.com/blog/2026-06-29-editor-gizmos)

### 12. Manipulation is a transaction, not direct mutation from mouse motion

Read 2026-08-29. The article-day implementation was audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The current source has been split into smaller modules, adds a visual shadow to
the handles and cancels latched input on focus/cursor loss in the demo, but the
underlying single-selection transform model is substantially unchanged.

**What it builds.** Helio provides translate, rotate and scale gizmos for a
selected scene actor. Three analytic handle shapes are drawn through the debug
batch: arrow shafts and cones for translation, annuli for rotation, and shafts
with cube ends for scale. Their world size is recalculated from an 80-pixel
target so they remain approximately constant on screen.

Hover does not wait for a GPU pick-buffer readback. A cursor position is
unprojected into a world ray and tested against a line-segment approximation
for translate/scale handles or a ray-plane/radius approximation for rotation
rings. Starting a drag freezes the actor transform, pivot, local axes and the
initial constraint parameter. Every later cursor event recomputes a transform
from that baseline, avoiding accumulated frame-to-frame floating-point error.

The selected actor is still mutated on every drag update. An object or
sectioned object can use all three modes; a light uses world-axis translation.
The scene transform update refreshes CPU-side bounds and, when the GPU layout
is stable, patches the affected instance/AABB slots instead of rebuilding the
whole scene. The debug pass batches line and triangle vertices into two draw
calls.

**Where it helps Lince.** We should own the feel of editing rather than bend
Box manipulation around an engine editor widget. Analytic handles are a good
fit because their hover cost depends on the small number of active handles,
not on the number of Sands. Fixed apparent size, one frozen drag baseline and
the same camera/projection contract for drawing and hit testing are all useful
foundations.

The reusable part is not an XYZ-gizmo type. It is a manipulation protocol that
can give different tools the same lifecycle and interaction quality. Native
Sands, Castles, Areas, Protein spawn regions, topology effects, topology-brush
control points, lights and future world objects need different handles but the
same hover, arm, preview, commit, cancel, undo and collaboration semantics.

This also connects the previously planned 2D/3D modes. A Sand glued to a
deformed topology surface is not freely translated through XYZ: its primary
motion is in the local tangent plane, its elevation follows the surface and
its displayed tilt follows the topology normal. A floating Sand has true 3D
translation. In collapsed 2D, the same object exposes plane translation,
resize and Z-order/elevation controls rather than pretending that an
edge-on Z arrow is usable.

**Needed for the capability to work.** Input first passes through one routing
and capture system. Native Sand controls, embedded browser surfaces, world selection,
gizmos and camera navigation receive explicit priority. Pressing an active
handle captures that pointer until commit/cancel/focus loss; moving outside
the window cannot leave a drag latched. Mouse, touch and pen share pointer
identity, while keyboard commands and numeric entry remain discoverable in a
visible tool surface rather than existing only as G/R/S shortcuts.

A `ManipulationSession` records a stable target id, tool and handle id,
coordinate space, pointer id, starting workspace revision, starting semantic
state, pivot/basis, camera/projection snapshot, constraints and current draft.
Its states are `hover`, `armed`, `dragging`, `committing`, `cancelled` and
`conflicted`; idle is the absence of a session. Recalculation always starts
from the captured baseline. Escape restores the baseline, release commits,
and focus/device loss follows an explicit cancel policy rather than silently
accepting a partial edit.

The coordinate-space choice is explicit: Box/world, actor-local, camera/view,
2D Box plane, deformed-surface tangent or screen/pinned-HUD. Transform storage
does not infer one from the current camera. A surface-bound Sand stores its
semantic plane coordinates and surface attachment separately from its derived
3D presentation transform. Switching between top-down 2D and topology-visible
3D therefore does not rewrite saved positions.

Dragging is temporary authority, not an uncontrolled fight with physics. For
a physics-controlled Sand, the session can install a kinematic target or a
strong pointer constraint while the solver continues to run for the rest of
the Box. Area forces, collision and topology responses remain observable but
do not overwrite the pointer each frame. The chosen policy is visible and can
be configured per kind of Sand; releasing hands the final state back to the
solver without an impulse caused by stale velocity.

The same rule applies to collaboration. A local drag publishes bounded,
ephemeral preview poses if live collaborators are meant to see it, but it
persists one coalesced semantic operation on commit. It never writes every
pointer sample as durable workspace history. Revision/precondition checks
detect a concurrent edit to the same semantic property. Lince then applies a
declared ownership/conflict rule instead of Helio's behavior of repeatedly
reconstructing the old snapshot and silently overwriting external changes.

Every committed edit is an undoable workspace operation. Transforming a
Castle/group preserves member-relative transforms and records the group pivot
and membership revision used by the operation. Moving an Area, resizing a
Protein spawn region, changing a topology-effect profile and painting topology
are distinct operation kinds; they are not all flattened into matrices. A
topology stroke stores a deterministic brush path plus parameters or a
deterministic resulting patch, according to the persistence model selected by
the spatial-state work.

Handle geometry and behavior are separate. Geometry produces visual
primitives and analytic hit shapes; behavior maps a constrained pointer state
to a semantic draft. This lets translation arrows, planar squares, rotation
rings, resize corners, falloff radii, slope/height handles and topology brushes
reuse the session machinery. Styling uses interface tokens for size, contrast,
hover, active, disabled, conflict and occluded states, with a high-contrast
alternative that does not depend on red/green/blue alone.

Snapping is a first-class constraint chain: grid/unit, angle, surface, nearby
edge/center, Area boundary, topology contour and semantic alignment can each
produce a candidate and explanation. Modifier keys temporarily select or
bypass candidates. The active snap target, numerical delta and coordinate
space are shown while dragging. Exact values can be entered without requiring
pixel-perfect pointing.

Selection and manipulation remain separate. Scene/Sand picking returns stable
workspace identities; the active tool generates only its few handle tests.
the embedded browser receives pointer input only when its external Sand is the routed target,
and an untrusted page cannot synthesize a privileged Box manipulation. The
`Why is it here?` surface can explain a selected Sand's saved placement,
surface attachment, active Area forces and current manipulation override.

**Needed only to make it fast.** CPU analytic hit testing for a handful of
handles should be the initial path. Prebuilt unit meshes, instancing, a small
dynamic transform buffer, cached tessellation and only updating changed visual
state can reduce the article's per-frame CPU triangulation/upload, but none is
required before measurement shows the bounded debug geometry matters.

Picking thousands of scene actors may use a CPU spatial index, renderer ID
buffer or hybrid candidate system; that is separate from testing three active
handles. GPU readback must not enter the immediate pointer feedback loop unless
its latency is hidden behind prediction and a CPU confirmation path. Hover can
be sampled at input/display cadence while physics remains fixed-step.

Preview traffic is latest-value and bounded. Durable operations are coalesced
at commit. Topology brush samples can be resampled into stable spacing before
evaluation, and large affected regions can update through dirty tiles/chunks.
Those are throughput choices after the semantic stroke and cancellation rules
are correct.

**Source-audit corrections and limitations.** The implementation is a useful
prototype, but several statements need narrower interpretation:

- The apparent-size formula uses Euclidean camera-to-pivot distance. A
  perspective projection is governed by view-space depth, so an object far off
  axis is oversized relative to an equally deep centered object. Orthographic
  projection, points behind the camera, zero-height viewports and per-view
  scale are not handled. Lince should derive visual and hit geometry from one
  actual viewport projection helper and test off-axis views.
- The near-parallel fallback in `ray_to_segment_dist` does not clamp its
  segment parameter to `[0, 1]`, despite the article saying the segment is
  clamped. Handle overlap near the common origin is resolved only by smallest
  world distance and iteration order; ray depth and screen-space precedence
  are not returned. These are feel/correctness issues, not reasons to move
  three analytic tests to the GPU.
- Local axes are normalized transform columns. This is suitable for ordinary
  translate/rotate/scale transforms, but shear or a degenerate scale can make
  the basis non-orthogonal or zero. There is no world/local toggle: objects are
  always local and lights are always world-aligned, despite the prose invoking
  world-versus-local consistency.
- Freezing the initial transform prevents accumulated numerical drift, but it
  does not preserve an external transform modification during the drag. The
  next cursor event reconstructs the result from the old snapshot and
  overwrites the competing change. This is particularly unsafe for Lince's
  physics and collaborative workspace without an authority/conflict contract.
- Rotation ring picking is the acknowledged ray-plane/radius approximation and
  fails at grazing angles. Axis translation similarly returns no update when
  the view ray is nearly parallel to the chosen axis. Screen-space fallback
  constraints or alternate handles are required; widening an axis that
  projects to a point does not by itself define meaningful drag motion.
- The angle is an `atan2` delta from the start. It represents orientation but
  cannot track intentional multiple revolutions, and crossing the branch cut
  becomes relevant once continuous numerical feedback or snapping is added.
  Scale is clamped positive, so mirroring is unsupported and should be an
  explicit operation rather than an accidental zero crossing.
- The article says the debug path draws handles “on top” while also saying it
  depth-tests them. With `LessEqual`, a depth-tested handle can be hidden by
  scene geometry; without depth testing it draws through everything. Pass
  order and disabled depth writes do not solve that choice or z-fighting.
  Lince needs deliberate visible, occluded and X-ray styling, not one global
  debug-depth flag.
- The article-day demo has no pointer capture/focus-loss recovery, cancel-to-
  baseline, undo transaction, snapping, numeric entry, plane handles,
  multi-selection or collaboration. Current demo does end transient input on
  focus/cursor loss, which is an improvement, but `end_drag` still merely drops
  state and cannot revert or produce a semantic command.
- Transform-update errors are ignored. Static-object updates can no-op while
  the manipulation continues to look active. A Lince handle must be disabled
  with a reason or surface the failed commit; a visual affordance must never
  promise authority the target does not grant.
- The “well under 0.01 ms” CPU and “0.001% fill rate” GPU values have no shown
  benchmark. The implementation regenerates immediate-mode vertices and
  uploads changed debug buffers. The cost is plausibly small and bounded, but
  entry 11's profiler—not arithmetic from triangle count—decides whether it is
  free in Lince's full frame.

**Concrete first slice.** Implement the common manipulation-session state
machine with translation in two coordinate spaces: Box plane/surface tangent
for a topology-bound native Sand and world XYZ for a floating native Sand.
Ship visible pointer capture, cancel, commit, undo and an inspector that shows
the current space and numerical delta. Run the draft through the existing
fixed-step simulation override, persist only the commit, then reload and prove
the semantic position reconstructs the same 2D and 3D views.

Next add an Area move/resize tool and one topology height/falloff handle using
the same protocol. That demonstrates that manipulation composes across data
Sands, physics fields and the deformed plane without making “gizmo” a hard
object-transform abstraction. Only after these work should rotation, group
pivots, rich snapping and topology brushes expand the handle set.

**Ideas to test.** Exercise centered/off-axis perspective and orthographic
views, near/far clips, camera crossing the target, zero/minimized viewport,
non-uniform/zero/sheared transforms and an axis aligned with the camera. Test
overlapping handles, obscured handles, selection behind a browser surface, high-DPI scale,
multi-pointer input, cursor/focus loss, Escape and device loss.

During a drag, inject Area forces, a topology update, actor deletion, permission
loss and a conflicting remote transform. Verify the chosen authority rule,
cancel restoration, one-operation commit, undo/redo and reload. Replay the same
topology stroke at different input event rates and require the same saved
result. Measure input-to-visible-preview latency and frame pacing while the Box
contains 10,000 active off-camera actors; do not use an unmeasured triangle
count as the performance result.

**Verdict:** borrow the analytic hit shapes, stable screen-space intent,
baseline-relative math, local GPU patching and small batched visuals. Build a
Lince-owned, transactional manipulation protocol around them before using the
math in Box. Direct per-pointer mutation is acceptable for an isolated editor
demo; it is the wrong persistence, undo, physics and collaboration boundary for
Sands, Areas and topology.

[Culling Done Right: Fixing What Everyone Gets Wrong About GPU Culling](https://pulsarnative.com/blog/2026-06-29-culling-system)

### 13. Render less without putting the world to sleep

Read 2026-08-29. The article-day culling, Hi-Z, virtual-geometry and shadow
passes were audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
Current main has fixed the largest batching error by culling and compacting
instances within each draw group, improves meshlet Hi-Z and exposes overflow,
but some article claims still describe a target design rather than the source.

**What it describes.** The proposed GPU-driven pipeline extracts and normalizes
six frustum planes, rejects sub-pixel objects, tests projected bounds against a
max-depth Hi-Z pyramid, compacts surviving indirect draw commands, chooses
meshlet LODs, culls shadow casters for each dirty light face and accumulates
eight diagnostic counters. A pre-baked potentially-visible set is presented as
an additional conservative rejection stage for static geometry.

The post is framed as a corrective history. It identifies unnormalized
frustum planes, indexing an indirect-command buffer by instance rather than
draw, an incorrect sphere near-depth expression, one-point Hi-Z sampling,
object-level bounds used for meshlet LOD and colliding statistics indices. Its
strongest general lesson is correct: a culler has no visual recovery path. A
false positive wastes work; a false negative makes content disappear, so the
system must be conservative and observable.

**Where it helps Lince.** The owner has set a precise invariant: a Sand outside
the camera must not be rendered, but its behavior must remain normally active.
Culling therefore belongs only to presentation extraction. It may suppress
native vertices, text, shadows, topology fragments, embedded browser texture upload and
composition, but it cannot remove the corresponding workspace actor from
physics, Area influence, Protein evaluation, event delivery, collaboration,
persistence, website execution, call/audio lifetime or game logic.

That separation is the foundation of scale. The authoritative Box contains all
active actors. At render-snapshot time each view produces candidates and then a
render-eligibility result. Camera culling changes neither the actor nor the
saved spatial state. Returning the camera to an actor simply derives a fresh
presentation from the state that continued evolving off screen.

the embedded browser needs the same split at a different boundary. An off-camera external Sand
keeps its browser context, JavaScript, permitted network/local storage, events,
audio and call participation active. Lince may stop importing or uploading new
video frames and omit its textured quad from composition. A video decoder may
drop disposable visual frames while audio and protocol state continue. This is
resource throttling at the presentation sink, not page suspension.

**Needed for the capability to work.** Give every view its own immutable render
snapshot and `VisibilityResult`. The result retains actor id, view id, bounds
revision, visible/culled decision and typed reason bits such as layer-policy,
outside-frustum, behind-camera, too-small, occluded, invalid-bounds and
capacity-overflow. “Not drawn” is never overloaded to mean hidden by the user,
deleted, sleeping or absent from simulation.

Bounds are authored and updated as part of each presentation kind. A native
Sand has its actual visual rectangle/rounded volume and elevation; a Castle can
use a conservative union bound only when all children share the same render
policy; an Area includes its visible field/outline while editing; topology uses
spatial tiles; a embedded browser surface uses its composed quad; a pinned Sand belongs to
screen space. Selected, manipulated and diagnostic actors can opt into an
explicit X-ray/editor pass without lying to normal visibility.

The first correct culling sequence is per-view layer eligibility, coarse Box
chunk/region selection, frustum or 2D clip intersection and exact projected
bounds. In top-down 2D, rectangle/shape clipping is sufficient. In 3D, optional
conservative depth occlusion follows only after its temporal validity rules
exist. Pinned/HUD Sands bypass world frustum and topology occlusion but remain
clipped to their viewport.

All rejection stages fail open. A missing or non-finite bound, stale depth
pyramid, camera discontinuity, resized surface, changed projection, newly
spawned/moved actor, changed topology occluder, overflowed output buffer or
unsupported GPU feature renders the uncertain candidate rather than dropping
it. The Health Sand records the fallback and its cost. Correct overdraw is
preferable to an unexplained disappearing Record.

Temporal Hi-Z carries exact provenance: producing view, viewport/projection,
camera transform, depth convention, frame id, topology/occluder generation and
resolution. A previous-frame pyramid is usable only under a conservative
motion policy, reprojection/expansion scheme or explicit validation. Camera
teleports and newly revealed regions invalidate it. Dynamic occluder motion
also matters when the camera is static; camera generation alone is not a valid
depth-generation key.

Render compaction preserves the ordering semantics of its consumer. Atomic
append order is acceptable for opaque geometry whose result is order
independent. It is not silently reused for transparent Sands, 2D z-order,
keyboard traversal, picking or Castle child order. Those paths use a stable
compaction/sort key or a separately ordered candidate list. A render index is
never a workspace identity.

Sub-pixel rejection is derived from the actual viewport and conservative
projected extent, not a fixed NDC constant. It is a rendering threshold, not
the removed “semantic zoom” concept: Protein still decides which Record fields
exist in a Sand. When an entire cluster is smaller than a pixel, Lince may omit
it or draw an explicit aggregate marker according to the view design, but the
underlying actors and Protein results remain unchanged. Selected/active objects
and interaction handles have explicit minimum presentation rules.

The renderer and picker consume compatible visibility snapshots. By default a
world object occluded by topology is not front-clickable through it; an
editor/X-ray selection mode can intentionally broaden candidates. A embedded browser Sand
cannot intercept input when its composed surface is not the routed visible hit.
This avoids a hidden web surface capturing clicks while preserving its off-
camera runtime.

Culling counters use a generated/versioned Rust/WGSL layout rather than
independent integer literals. Each stage defines its input population, survivor
population, rejection population, unit (`actor`, `draw group`, `instance`,
`meshlet`, `topology tile`, `embedded browser surface`) and whether categories partition the
input. The invariant is tested before percentages are shown. Entry 11's bounded
asynchronous telemetry carries the counters to Renderer Health without a
per-frame map-and-wait.

Renderer Health provides a culling inspector in the same capability. It can
freeze a frame, color candidates by final reason, show bounds and Hi-Z tiles,
disable one stage, report stale/fail-open/overflow state and identify the exact
actor hidden by a decision. Its empty case distinguishes “nothing was
submitted,” “stage disabled,” “unsupported” and “data pending.” This is needed
to make a disappearing Sand diagnosable by a human.

For the future globe, culling starts in a high-precision, hierarchical spatial
frame. Globe cells/regions produce camera-relative candidates before f32 GPU
bounds are evaluated. A city, imported scene or Gaussian representation can
have nested bounds and presentation-specific LOD, but semantic actors remain
addressable and active outside selected/rendered cells. Pre-baked visibility is
an optional optimization for sufficiently static worlds, never the source of
truth for an editable topology.

**Needed only to make it fast.** GPU instance compaction, hierarchical Box
chunks, max-depth pyramids, occlusion history, mesh/meshlet LOD, per-light-face
shadow lists, pre-baked PVS, camera-relative globe cells and indirect-count
drawing are optimizations. They are valuable only past measured break-even
points. A few hundred simple Sands may be cheaper to clip while building a CPU
render list than to dispatch several GPU passes.

Use a coarse-to-fine hierarchy so expensive tests see fewer candidates. Reuse
stable static bounds, update only dirty spatial chunks, keep opaque and ordered
presentation paths separate, and use bounded capacity with a visible overflow
fallback. embedded browser frame dropping should happen before copy/upload where possible.
None of these optimizations authorizes sleeping behavior.

Benchmark at the current development resolution with representative native
Sands, text, topology and embedded browser surfaces. Vary candidate count, visible ratio,
overdraw, camera motion and bound quality; compare culling cost against work
actually avoided. The math and resource sizing must remain resolution-derived
so the architecture is not limited to the current monitor, but a 4K performance
target is not required now.

**Source-audit corrections and limitations.** The article correctly fixes
plane normalization, but its “done right” result still had fundamental gaps at
publication:

- The article-day frustum and occlusion passes execute one thread per draw
  group and test only `instances[draw.first_instance]`. Optimized groups batch
  spatially unrelated instances by mesh/material, so an off-screen
  representative can remove visible siblings and a visible representative can
  draw all hidden siblings. The stated instance-index out-of-bounds bug was
  replaced by group-level incorrectness, not by per-instance culling. Current
  main now cooperatively tests and compacts each group's instances, which is
  the important fix to borrow.
- The supposed tighter AABB is produced by `sphere_to_aabb`: it is the cube
  enclosing the already available sphere, not a mesh-derived local AABB. It is
  looser, especially at corners. The shader chooses that AABB result whenever
  it is non-degenerate instead of requiring both a sphere coarse test and a
  genuinely tighter box test. The article's elongated-object and transformed-
  mesh-AABB description does not match the source, and current main retains
  the sphere-derived cube.
- Publication sub-pixel culling compares radius in NDC to the constant `0.001`.
  That is not one pixel and changes meaning with viewport height. Current main
  retains the constant and makes the decision at draw-group level: if one
  frustum survivor is large enough, all compacted survivors are drawn; if none
  is, the group is removed. Its counters count draw groups despite prose/table
  labels that call them instances.
- The post's sphere “nearest point” is nearest in Euclidean distance to the
  camera, not necessarily nearest along camera/view depth for an off-axis
  sphere. Projecting it can overestimate the nearest depth and create false
  occlusion. Exact conservative projected sphere/box bounds and view-axis near
  depth need reference tests across the frustum, especially near the camera.
- The 4-corner strategy can be conservative with a nearest-sampled max pyramid
  only when the chosen mip and footprint prove the touched texels cover the
  whole projected bound. That proof depends on exact bounds, texel alignment,
  reduction direction and sampler. The publication instance pass uses a
  nearest sampler, but its virtual-geometry pass still samples only the center,
  contrary to the article's claim that per-meshlet culling runs the same
  four-tap test. Current main has added four meshlet taps and on-screen/validity
  guards.
- The Hi-Z pyramid is previous-frame depth. Publication skips only frame zero;
  it does not invalidate or conservatively reproject on camera motion. Worse,
  the max pyramid stops rebuilding whenever `camera_generation` is unchanged,
  even if movable geometry, Sand placement or topology changed. Current main
  explicitly rebuilds its min pyramid for moving content but still keeps the
  camera-only early-out for the max occlusion pyramid. That can freeze
  occluders under a static camera.
- The article describes a CPU PVS bitmask indexed by the camera's voxel. The
  publication shader instead samples a GPU 3D directional distance texture at
  the object's clamped position and compares camera-to-object distance. This is
  neither the described data structure nor zero GPU cost. Current main now
  publishes a separate `BakedPvsRef` bitfield, but no audited render pass reads
  it; the occlusion shader still uses the older directional texture while
  calling it PVS. Out-of-bounds positions are clamped rather than failing open.
- The shadow pseudocode says one workgroup per face. The source dispatches one
  thread per movable draw and loops over all 256 possible faces, skipping clean
  ones. Article-day indexing assumes the instance and source-indirect slots
  align; current main at least reads the instance through each draw's
  `first_instance`. The advertised five-to-six-times result and microsecond
  pass cost have no shown benchmark or scene definition.
- The source reserves shared counter slots and resets the buffer, which is a
  useful fix for the reported collision. But article-day owned rendering then
  copies, maps and waits for the stats every frame, adding the stall already
  identified in entry 11. Current main makes that readback callback-driven,
  though the stage/unit semantics are still comments and labels rather than a
  typed schema.
- Most tests are CPU helper replicas, structure/layout checks or WGSL parse
  tests. They do not render adversarial GPU scenes and assert that no visible
  object was falsely culled. The 0.3–0.8 ms and 50–70% savings figures have no
  reproducible hardware, resolution, scene, comparison mode or percentile
  method in the article.

**Concrete Lince visibility model.** Keep `WorldActorState` authoritative and
produce a per-view `RenderCandidate` stream containing stable id, presentation
kind, conservative bounds, layer/order key, bounds generation and resource
references. A `VisibilityPipeline` returns stable visible lists for ordered
native Sands, opaque world geometry, transparent world geometry, topology,
embedded browser surfaces, shadows and diagnostic overlays. Each output retains rejection
telemetry without mutating the inputs.

The v1 slice needs CPU/spatial-index 2D clipping and conservative 3D frustum
culling for native Sands/topology, plus composition culling for browser surfaces. Demonstrate
an off-camera Sand continuing fixed-step Area motion and re-entering at its
evolved position; an off-camera external Sand continuing a timer/event/audio
test while its texture-import counter stops; and a pinned Sand remaining in the
HUD. Ship these scenarios in Renderer Health before adding Hi-Z.

Hi-Z becomes a later v1 optimization only after a depth-provenance key,
camera/topology invalidation, fail-open path and false-negative oracle exist.
Static PVS belongs to v2 world-scale research because editable topology and
user-made worlds invalidate baked visibility frequently. Meshlet/primitive LOD
is likewise presentation-specific and should not complicate the initial Sand
renderer.

**Ideas to test.** Use a CPU double-precision oracle to generate random cameras,
projections and conservative bounds, then compare GPU results and require zero
false negatives. Permute mesh/material batching and require the same visible
actor identities. Cover elongated meshes, sphere-derived cubes, non-uniform
scale, deformed topology, near-plane intersections, off-axis spheres, actors
larger than the frustum and empty/non-finite bounds.

Test camera teleports, resize, FOV change, stationary camera with moving
occluders, newly spawned actors behind stale depth, topology deformation,
history frame loss and capacity overflow. Run the same scene at several small
test resolutions to verify the one-pixel threshold changes correctly; this is
a correctness invariant, not a 4K performance benchmark. Compare stable
ordering before/after compaction for transparent and 2D Sands.

Finally, simulate 10,000 actors with only a small visible fraction and prove
off-camera counters for physics steps, Protein/event processing and
collaboration continue unchanged while render submissions fall. Measure
median/p95/p99 frame pacing with each stage independently disabled, using the
same scene and Renderer Health capture.

**Verdict:** adopt the conservative GPU-driven direction, normalized planes,
per-instance compaction, typed counter ranges and visible culling diagnostics.
Do not inherit article-day batch representatives, sphere-derived “tight”
AABBs, fixed NDC thresholds, camera-only Hi-Z reuse or the unimplemented PVS
story. Most importantly, make culling a per-view rendering decision. Lince's
world continues living outside the camera.

[Cross-Platform GPU Programming Without Losing Your Mind](https://pulsarnative.com/blog/2026-06-29-cross-platform-gpu)

### 14. Portability is an explicit renderer contract, not a collection of platform guesses

Read 2026-08-29. The article-day implementation was audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747),
which used `wgpu` 23.0.1, and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be),
which uses `wgpu` 30.0.1. Lince currently pins `wgpu` 29.0.4. The post has a
valuable architectural thesis, but many of its API examples, limits and claims
about Helio do not match either audited source tree. It should be used as a
prompt to verify capabilities, not as a compatibility reference.

**What it argues.** `wgpu` provides one safe command/resource model over
Vulkan, Direct3D 12, Metal and browser WebGPU, but it cannot make dissimilar
hardware expose identical optional features and limits. The post surveys
binding arrays, GPU-counted indirect draws, WGSL language restrictions,
texture and depth formats, host/shader structure layout, timestamps and
primitive indices. Its proposed answer is to discover capabilities after
adapter selection and route every renderer subsystem through an explicit
primary or fallback implementation.

That answer is sound. An operating-system `cfg` is generally the wrong way to
choose GPU behavior because backend, adapter, driver, limits and per-format
features are runtime facts. A fallback is also not correct merely because it
draws something: it must preserve the same semantic result, ordering and
authority while making any visual or performance degradation visible.

**Where it helps Lince.** Lince has deliberately chosen a narrower production
target than the post: a Wayland/Ozone, Vulkan-first Linux host. It should not
add X11, Metal, Direct3D or browser-WebGPU branches to Plan A in pursuit of an
abstract portability score. Nevertheless, Linux is not one GPU. Mesa AMD,
Mesa Intel, NVIDIA, software Vulkan, driver versions and device limits still
differ, and embedded browser DMA-BUF interoperability adds another capability dimension
that `wgpu::Features` does not describe.

The useful result is a first-class `RendererPlatformReport`, built once from
the selected adapter before device creation and completed after surface and
embedded-browser-interoperability probing. It records backend, adapter/driver identity,
limits, surface formats and present modes, per-format usage features, enabled
WGSL extensions, selected implementations, unavailable enhancements and
reasons. The report is immutable for a device generation and is replaced
atomically after device recreation.

This hardware report must remain distinct from the existing semantic
`RendererCapability` set used to select a Sand projection. `ExternalHtml`,
`InstancedNodes` and `ThreeDimensional` describe what an adapter can present;
`MULTI_DRAW_INDIRECT_COUNT`, a texture limit or a DMA-BUF modifier describes
how the host can implement it. Sand definitions should not become coupled to
vendor features. The host either supplies the promised projection, selects a
semantically equivalent projection, or presents an explicit unavailable state.

The Renderer Health Sand exposes the report in human language: selected GPU
and driver, Vulkan/device generation, native and browser status, active rendering
paths, measured degradation and a copyable diagnostic. Exact low-level fields
can expand on demand. A Website receives none of this through the Lince bridge;
Installed HTML receives only normalized host capabilities it was granted, not
an accidental adapter fingerprint or raw GPU authority.

**Needed for the capability to work.** Define one reviewed baseline from the
actual pipelines Lince ships. Request exactly its required features and limits;
device creation must fail with a specific diagnostic when the adapter cannot
satisfy them. Optional accelerators are intersected with adapter support before
the device request and the resulting device features, not adapter promises,
select implementations. Do not request all adapter limits: every requested
limit becomes a contract, and asking for values the renderer never needs makes
device creation and validation unnecessarily brittle.

Compile-time platform selection remains only at real native integration
boundaries such as Wayland handles and Linux embedded browser/Vulkan interop. Rendering
algorithms use runtime capabilities. Each chosen path is represented by a
typed enum or prepared strategy, so passes do not scatter feature probes and
conditional assumptions through command recording. Pipeline compilation,
format selection and buffer-layout validation happen before the path is
admitted into a frame plan.

the embedded browser requires a sibling interoperability report: accelerated-paint support,
DMA-BUF fourcc/modifier, plane layout, importable Vulkan format, external-memory
handle support, explicit synchronization path, producer/consumer device match,
and the last import failure. These cannot be inferred from `wgpu` feature bits.
Native Lince may continue when external composition is unavailable, but an
Installed HTML or Website projection must show a precise unavailable surface;
it must not silently become a privileged DOM implementation or an unmeasured
CPU-paint path.

Capability tiers never change durable Box semantics. They cannot change Sand
identity, Protein results, Area filtering, physics, saved topology,
collaboration operations, event order or whether an off-camera actor remains
alive. They may change draw submission, texture binding representation,
anti-aliasing, shadow/GI quality or diagnostic precision. If two paths cannot
produce the same ordered visual actor set for the same render snapshot, they
are different behavior and one is not a valid fallback.

The Rust/WGSL ABI needs its own source of truth. Generate or validate host
offsets, sizes, alignments and shader declarations together; require Naga/
`wgpu` validation for every generated shader variant; and give GPU records
explicit versioned layouts. `bytemuck::Pod` is useful for ruling out
uninitialized padding in a Rust type, but it cannot prove that an independently
written WGSL type has the same offsets.

Device loss and surface loss are capability transitions, not panics. Stop
submitting the affected generation, retain semantic, physics and browser lifecycle
state, recreate the adapter/device resources, build a new report and resume
presentation. Renderer Health distinguishes unsupported, temporarily lost,
degraded and deliberately disabled states.

**Needed only to make it fast or richer.** GPU-counted multi-draw, binding
arrays/non-uniform indexing, timestamp queries, primitive indices, filterable
32-bit float textures, subgroups, push/immediate data and future mesh/ray
features are accelerators or presentation enhancements unless a measured
Lince pipeline proves otherwise. Their absence must not be disguised as a
correctness failure.

The high-throughput node path can compact on the GPU and use
`multi_draw_*_indirect_count` when supported. Without it, a fixed-capacity
indirect range filled with zero-count commands, an ordinary multi-draw call,
or a differently batched instanced draw can preserve the same visible set.
Which alternative wins is measured on Lince's representative 200-visible,
1,000-active and 10,000-resident workload. Reading the visible count back to
the CPU in the same frame is not an acceptable default because it can
serialize the GPU and CPU.

Binding arrays likewise remain an implementation choice. A fixed expanded
binding table, atlas/array texture, material batching or descriptor-indexed
table can all back the same Sand projection. The choice depends on queried
limits and measured update/draw cost, not an assumed “Metal equals 16” rule.
Changing the binding strategy cannot change which theme asset or Record image
a Sand displays.

Timestamp queries are optional to rendering but necessary for accurate GPU
attribution when available. Entry 11's bounded asynchronous readback and
availability/lag/drop reporting applies. CPU wall time is not presented as GPU
time when timestamps are absent. Primitive index is only enabled for a view or
debug pipeline that uses `@builtin(primitive_index)` and tolerates its possible
geometry-processing cost.

**Source-audit corrections.** Several statements in the article are materially
wrong or obsolete:

- `DeviceDescriptor` has required features, not separate `required_features`
  and `optional_features` sets. Helio both then and now constructs its requested
  set by intersecting optional bits with `adapter.features()` and passes that
  result as required for the created device. Unsupported requested features
  return an error; checked `wgpu` calls do not have a legitimate
  “undefined-behavior if used without checking” mode.
- `MULTI_DRAW_INDIRECT_COUNT` is a native-only `wgpu` feature for Vulkan and
  Direct3D 12. It is not supplied by WebGPU's `indirect-first-instance` feature;
  the latter only permits a non-zero `first_instance` field. Current Helio
  correctly distinguishes the two and uses ordinary multi-draw or individual
  indirect calls when the count feature is absent.
- The post's Metal sampler-array prohibition is too broad. Current `wgpu`
  advertises texture/sampler binding arrays on supported Metal versions and
  separately gates non-uniform indexing. Limits and the complete required
  feature set still have to be queried. Current Helio can choose binding arrays
  or rewrite them to expanded bindings, although its top-level maximum remains
  partly selected by target `cfg` and therefore does not yet embody the post's
  purely runtime ideal.
- Fixed-size arrays are constructible WGSL types and may be function return
  types. Runtime-sized arrays cannot. WGSL does lack closures/function pointers
  and `if` expressions, but the article's mandatory output-pointer workaround
  for all arrays is false.
- WGSL host-shareable layout is specified, not backend-dependent. A
  `vec3<f32>` has 16-byte alignment and 12-byte size; a following `f32` may
  validly occupy offset 12, making that particular pair 16 bytes. Members do
  not each require 16-byte alignment. Lince should still prefer explicit,
  mechanically checked layouts because Rust type choices can disagree with
  WGSL even though WGSL itself is deterministic.
- `wgpu::BufferDescriptor` has no `alignment` field. In the audited `wgpu`
  30 types, storage binding sizes are multiples of four; dynamic binding
  offsets obey the adapter's `min_storage_buffer_offset_alignment`, commonly
  256 in default limits. Mapped-at-creation and copy ranges have their own
  four-byte rule. Those are separate constraints, not one universal 16-byte
  buffer rule.
- `R32Float` is unfilterable in WebGPU's baseline unless
  `FLOAT32_FILTERABLE` is enabled; support is a queried feature across native
  adapters as well. `R32Uint` is an integer format and is not “universally
  filterable.” More importantly, Helio used `R32Float` Hi-Z with four explicit
  `textureLoad` operations and max reduction at the publication commit and
  still does so today. The claimed conversion to packed `R32Uint` is not in
  either source tree and is unnecessary for explicit-load reduction.
- `Depth24Plus` is intentionally an abstract WebGPU format whose depth aspect
  may be backed by 24-bit depth or `Depth32Float`; that implementation choice
  is not a request silently changing into a different API format. Explicit
  `Depth32Float` is still a sensible Lince choice when its precision and memory
  cost are what the pipeline actually wants. Combined depth/stencil remains a
  separately queried feature.
- Primitive ID is `@builtin(primitive_index)`, not `sample_index`. In current
  `wgpu` the feature is named `PRIMITIVE_INDEX`; the older
  `SHADER_PRIMITIVE_INDEX` spelling is only historical compatibility. These
  built-ins represent different facts.
- The article's timestamp pseudocode does not match the `wgpu` API. Encoder
  timestamps require the appropriate enabled feature set and call
  `encoder.write_timestamp(query_set, index)`. Current Helio performs that
  check, whereas the article's described `optional_features` mechanism does
  not exist.
- No `GpuCapabilities` table matching the article was present at the
  publication commit or on audited current main. The publication source did
  contain repeated feature checks and `cfg`-selected texture counts. Current
  main has improved a number of individual fallbacks and centralized material
  binding configuration, but the post presents a cleaner architecture than
  the repository demonstrates.

The quoted Metal degenerate-draw costs and 4 ms scene estimate have no hardware,
driver, command shape, benchmark source, distribution or reproducible method.
They are anecdotes, not planning constants. The claims that Vulkan universally
mandates linear filtering for `R32Float` and that bindless arrays are always
256 there are likewise unsafe substitutes for adapter and format queries.

**Concrete v1 contract.** Add a capability fixture format used only by tests
and diagnostics, not durable Box state. A fixture contains adapter features,
limits, format features, surface capabilities and embedded browser interop outcomes. Feed
it through one pure planner that produces a required-device request, chosen
surface/depth formats, node submission path, material binding path, profiler
availability and external-surface path. The same input must always produce the
same plan and typed rejection reasons.

Use at least these profiles: the actual development Vulkan adapter; a reduced
but valid baseline with no optional features; no GPU-counted multi-draw; low
sampled-texture/sampler limits; unfilterable `R32Float`; no timestamps; the embedded browser
DMA-BUF import unavailable; and an invalid baseline. The reduced valid profiles
must render the same stable actor ids and picking ids. The invalid profile must
name the exact missing requirement before frame construction.

On real Wayland Vulkan runs, capture adapter/driver and the selected plan with
the existing joined report. Exercise Mesa AMD/Intel and NVIDIA when that
hardware becomes available; use software Vulkan for validation coverage, not
as performance evidence. Shader variants compile through Naga and `wgpu`
validation. ABI tests compare Rust `size_of`/offsets with generated layout
metadata. Device/surface loss tests verify that simulation, Protein, events,
physics and browser lifetimes continue while only presentation is rebuilt.

Do not add a physical 4K benchmark gate without the hardware. Do keep all
extent, scale, clip, atlas and allocation decisions dynamic and run synthetic
large-extents correctness tests, so no known resolution ceiling is designed
into a supposedly portable path.

**Verdict:** adopt runtime negotiation, explicit mandatory versus optional
features, typed selected paths, semantic-equivalence tests and a visible
renderer/browser capability report. Keep Plan A Wayland/Vulkan-focused instead of
maintaining unrelated platform branches. Reject the article's guessed limits,
invalid API shapes and undocumented performance constants. For Lince, the
fallback is part of the product only when it preserves the same Sand/Box
meaning; otherwise the honest product behavior is a precise unavailable state.

[Why Compute Shaders Beat CPU Loops: Helio's O(1) Frame Cost Philosophy](https://pulsarnative.com/blog/2026-06-29-compute-over-cpu)

### 15. Keep spatial work resident; do not confuse fixed submission cost with fixed work

Read 2026-08-29. The article-day implementation was audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
This post overlaps the culling article reviewed in entry 13, but asks the more
important architectural question for Lince: which work should remain on the
CPU, and which hot numeric state should live and evolve on the GPU?

**What it proposes.** Keep flat scene arrays resident in GPU buffers, retain a
CPU mirror, upload only changed ranges, run frustum/sub-pixel/Hi-Z/meshlet and
shadow culling in compute shaders, then consume GPU-written indirect draws.
The host records a bounded set of dispatch and draw commands instead of
iterating every render object. The post calls this an “O(1) frame cost”
philosophy and reports 397 microseconds of GPU culling for 12,000 objects plus
10,000 meshlets on an RTX 4070 at 1920×1080.

The durable lesson is data residency, not the asymptotic slogan. A dispatch is
one host API call, so command-recording work can be constant in object count.
The dispatched GPU work remains O(n), consumes memory bandwidth, crosses a
finite number of SIMD lanes in successive waves and competes with rendering on
the same device. Dirty uploads are O(changed), which becomes O(n) when every
body moves. A GPU makes suitably parallel work much wider; it does not make
unbounded work constant.

CPU culling is not inherently a full linear scan either. A hierarchy, spatial
grid or change-driven index can visit regions and visible candidates rather
than every object. GPU-driven, CPU-driven and hybrid designs therefore compete
on an actual workload. The right objective for Lince is bounded host overhead,
incremental transfer, stable p95/p99 frame pacing and a measured total CPU/GPU
budget—not ideological elimination of CPU loops.

**Where it helps Lince now.** The promoted prototype proved 1,000 continuously
eligible CPU-simulated bodies at a 120 Hz fixed step, 10,000 resident light
nodes and 200 visible interactive Sands. It did not prove 10,000 simultaneously
moving and visible Sands. The laboratory `FieldSolver` still loops bodies
against Areas for force/mutation work and uses a CPU spatial grid for
collisions; its `PhysicsAdapter` seam is the correct place to compare a later
GPU solver. The laboratory `NodeLayer` is a 256-instance presentation fixture
that rebuilds and uploads its displayed instances. Neither detail is the final
Box architecture.

For v1, semantic and continuous spatial work need different ownership:

- Protein selection, Record fields, regex/concept matching, Sand/Castle
  identity, Area definitions, mutation Actions, permissions, persistence and
  collaboration remain authoritative Rust state. They change because data or
  an operation changed, not because a render frame elapsed.
- Position, velocity, topology samples, force accumulation, collision working
  sets and presentation transforms are dense numeric state. They may remain in
  the CPU SoA solver or move behind the same adapter into persistent GPU
  buffers when measurement shows a real win.
- Renderer visibility is derived from the spatial snapshot. Culling never
  controls whether an off-camera body is simulated, as established in entry
  13.

This division lets GPU compute accelerate the literal-sand behavior without
making the GPU the database. A body still has a stable Lince id and generation;
its transient array slot and device buffer offset are replaceable runtime
handles. A device loss cannot erase a Sand, an Area or its last durable spatial
checkpoint.

**Needed for the Box capability to work.** Retain one fixed-step spatial
contract independent of renderer cadence and solver implementation. It takes
an immutable semantic/spatial input revision and produces positions,
velocities, contacts, Area crossings, ordering state, overflow facts and a
completed tick id. Pointer manipulation, topology editing and Actions enter at
defined tick boundaries. The same body is never simultaneously authoritative
in two solvers.

Protein filters are evaluated when their inputs change and compiled into
stable body membership/mask data. A numeric solver does not parse slugs,
concepts or regular expressions every 120 Hz tick. Mutation Areas never let a
GPU shader edit Records directly: a solver emits bounded typed candidates,
Rust verifies current identity, membership, immunity and authority, orders
them by stable keys, then performs each Action exactly once.

Persist spatial state through the operation/snapshot design already planned
for Box. Coalesced checkpoints contain stable ids, tick/revision, transform,
velocity where meaningful, Area/topology revisions and solver metadata—not
GPU slots. A bounded asynchronous path may make a recent GPU transform snapshot
available for disk and live host-authoritative collaboration. The UI states
the last durable checkpoint and any save lag. Device recovery starts from the
last acknowledged state and reapplies later semantic operations; it never
pretends an unavailable GPU buffer is durable truth.

Live collaboration does not require every peer's GPU to reproduce identical
floating-point atomics. The workspace host owns the simulation tick and sends
coalesced authoritative spatial state/operations. Peers interpolate for
presentation and submit intents. Any GPU-produced mutation candidates are
canonically ordered and committed by Rust before they become shared facts.

The 2D surface-bound and 3D floating modes share this ownership model. A
surface-bound body receives its topology height/normal and force field in the
solver, while a free-space body receives volumetric forces. Switching view or
projection does not copy semantic state. GPU transforms can feed instanced
native Sands, topology and selection/picking directly; browser quads use the same
resolved transform while browser execution remains independent.

Every variable-size output has an explicit capacity, count, overflow flag and
fail-safe behavior. Overflow cannot silently discard a visible Sand, collision
or mutation. Correctness-sensitive outputs either grow/retry outside the hot
tick, fall back to the CPU path, or decline the tick with a visible diagnostic.

**A candidate GPU spatial kernel, only if measurement earns it.** Keep
ping-pong position/velocity buffers, stable slot generations, Area parameter
tables, precompiled membership masks, topology tiles and a spatial hash/grid
resident on the device. A fixed-step frame plan performs field accumulation,
sorting constraints, broad phase, contact solve and integration in declared
passes. The final transform buffer is consumed by native rendering and GPU
picking without a whole-scene CPU round trip.

Compact only exceptional results back toward Rust: Area entry/exit candidates,
mutation candidates, collision events requested by Behavior, overflow and a
checkpoint snapshot at its configured cadence. Use bounded staging rings and
associate every readback with device generation, simulation tick and semantic
revision. A late result from a replaced revision is discarded rather than
applied to a different Record.

Interaction needs an explicit authority override. While a person drags a Sand,
the captured transaction supplies the body's target/constraint to the next
solver tick; physics does not race the pointer. Selected or browser-backed surfaces
that require immediate CPU hit-coordinate mapping retain a small synchronized
interaction record rather than forcing a readback of all bodies.

This design is plausible, but not automatically v1's best solver. Sort Areas,
groups that act as one, collision constraints, immunity, topology deformation,
event extraction, device recovery and checkpointing make a correct GPU solver
substantially harder than GPU culling. The existing CPU SoA plus spatial
indexing remains the baseline until the representative topology workload shows
that it misses the fixed-step or frame-pacing gate.

**Needed only to make it fast.** Persistent GPU transforms, indirect draws,
GPU broad phase, workgroup scans, subgroup prefix sums, sparse upload gathers,
multi-rate simulation and GPU picking are optimizations. None is needed to
express Protein spawning, Areas, topology or 2D/3D navigation correctly.

On the CPU path, first remove avoidable work: compile filter membership,
spatially index Areas, skip recomputation for unchanged static state, keep SoA
data contiguous, batch changed ranges and render only extracted visible
instances. One coalesced dirty interval is simple but may upload a large middle
range when two distant slots change; sparse spans or a staging gather are worth
adding only after transfer metrics show that pattern.

On a GPU path, a global atomic per survivor is a starting point, not a timeless
design. Per-workgroup scans and subgroup operations can reduce contention when
the selected adapter supports them, but introduce feature variants and
ordering complexity. They are accepted only with the capability planning from
entry 14 and identical result-set tests. Compute and graphics on one `wgpu`
queue are not assumed to overlap; async execution is evidence to measure, not
free time credited to the design.

**Source-audit corrections.** The publication source differs sharply from the
post's concrete pipeline:

- `IndirectDispatchPass` dispatched one invocation per draw-call group, not per
  object. It selected only the group's first instance as representative and
  wrote a non-compacted indirect slot with `instance_count = 0` when culled.
  There was no final visible-count atomic, `mesh_buf`, `draw_id_buffer` or
  `instance_visible` boolean array in that pass.
- The shader did not perform a cheap sphere test followed by an eight-corner
  AABB test. It used a positive-vertex plane test for the stored AABB and used
  the sphere only when the AABB was degenerate. The article's rule “at least
  one AABB corner is inside the frustum” is not a generally correct
  intersection test; a large box can contain the frustum while none of its
  corners is inside.
- The sub-pixel threshold was a fixed NDC value of `0.001`, not one pixel and
  not derived from render dimensions. Entry 13 records the resulting
  correctness issue.
- Publication-day Hi-Z occlusion sampled four rectangle corners, not the one
  texel described in the post. The source used representative spheres rather
  than projected eight-corner AABB rectangles and retained the temporal
  false-negative risks already reviewed in entry 13.
- Hi-Z construction used a depth-copy compute pass and then one dispatch per
  mip inside another compute pass. Its work scales with render area and mip
  count even though the host loop is logarithmic and small.
- `GpuScene` did have real dirty-range tracking, but no type named
  `DirtyTracker`. A single dirty interval expands from its lowest to highest
  changed slot, so “only the slots that changed” can include unchanged slots
  between two edits. Structural scene changes also trigger broader rebuilds.
- The claimed explicit `wgpu` buffer barrier is not visible in the audited
  source and there is no public encoder buffer-barrier call in the relevant
  `wgpu` API. Lince must express producer/consumer order through separate pass
  scopes and frame-plan resource dependencies, allowing `wgpu` to emit native
  synchronization, rather than relying on an imaginary manual call.
- The described main/shadow atomic race is not demonstrated by the publication
  source. Main culling used a shared statistics buffer at distinct indices;
  shadow culling already owned per-face count and indirect buffers. Atomic
  operations at different addresses do not mix their values merely because
  execution overlaps. Separate ownership can simplify dependencies and reduce
  contention, but the explanation given is not a valid race mechanism.
- VirtualGeometry did perform one-thread-per-meshlet compaction, but it did not
  consume the article's claimed `instance_visible` output. It independently
  read instance data and performed its own frustum, cone, Hi-Z and LOD tests.
  Its article-day Hi-Z test sampled the center rather than inheriting the
  four-tap object pass.
- Current Helio has replaced the largest representative-instance error with
  per-instance compaction inside draw groups and a second post-occlusion
  compacted-index buffer. That is a meaningful improvement, not evidence that
  GPU cost is constant.
- The repository paths printed at the end of the post (`crates/helium/shaders`,
  `crates/passes/indirect_dispatch`, `crates/passes/hiz`) do not match the
  article-day tree.

The reported performance table supplies a GPU model and resolution but no
driver, clocks/power state, scene generator, visibility ratio, command capture,
warm-up, sample count, distribution or benchmark source. The CPU comparison is
an arithmetic estimate, not a measured implementation. Moreover, publication-
day profiling has the attribution limitations documented in entry 11, and the
same queue does not make culling execute “in parallel with other work” for
free. Treat every number as an anecdote until reproduced by Renderer Health.

**Ideas to test.** Build one deterministic topology/Area corpus and run it
through the CPU SoA baseline and any GPU adapter. Scale active bodies through
1,000, 10,000 and 100,000; vary Areas, filter density, groups, collision
density, topology tile updates, visible ratio and mutation crossings. Include
all-moving and mostly-static cases. Report fixed-step and frame median/p95/p99,
backlog, GPU pass time, CPU preparation time, bytes uploaded/read back, event
latency, memory and device-recovery time.

Compare stable ids, membership, crossings and committed Actions exactly;
compare numeric transforms within a declared tolerance after every tick.
Randomize insertion order and force high contention. Saturate every compacted
output and prove the overflow policy. Interrupt the GPU between semantic
revision, tick, render and checkpoint; recovery must not apply a stale event or
lose the last acknowledged durable state.

Keep all off-camera bodies active in every run and verify their tick counters
and results. Measure 2D surface-bound topology, Perspective view and free-space
3D separately. Include moving embedded browser surfaces without making browser execution
part of the solver. Physical 4K remains outside the present gate, while render
extent stays dynamic and large synthetic extents remain correctness tests.

**Verdict:** adopt persistent dense buffers, changed-range uploads, bounded GPU
submission and direct compute-to-render data flow as architectural options.
Reject “O(1) frame cost” as a performance claim and do not move Lince semantics
onto the GPU. Keep the CPU SoA solver as the v1 correctness baseline, preserve
the `PhysicsAdapter` seam, and build a GPU spatial kernel only when the actual
Area/topology/all-moving benchmark proves that its added synchronization,
persistence and interaction complexity buys materially better user-visible
smoothness.

[Cascaded Shadow Maps: From Matrix Compute to PCSS Filtering](https://pulsarnative.com/blog/2026-06-29-cascaded-shadow-maps)

### 16. Shadows are a depth cue, not the foundation of Lince's spatial model

Read 2026-08-29. The article-day implementation was audited at Helio commit
[`b88e366`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
and compared with current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
This is useful renderer repertoire for Lince's Perspective view and future
world scenes, but it is not a reason to put an AAA-sized shadow system in front
of the topology and Sand contracts that shadows would merely depict.

**What it proposes.** Helio assigns shadow-casting lights consecutive layers
in two `Depth32Float` texture arrays, one for static casters and one for movable
casters. A compute pass writes point-light cube matrices, spot-light perspective
matrices and four sphere-fit, texel-snapped directional cascades. Matrix hashes
and object movement select dirty light faces; another compute pass frustum-culls
movable draws into a compact indirect list for each dirty face. Static depth is
cached separately. A GPU-counted clear triangle and indirect geometry draws let
clean dynamic faces retain their old depth without CPU readback. Devices without
GPU-counted multi-draw take a clear-and-draw fallback.

Deferred lighting samples both atlases and treats a fragment as lit only when
neither atlas occludes it. Low and Medium use fixed-radius percentage-closer
filtering. High and Ultra add blocker search and a variable PCF kernel for the
four directional cascades, with Vogel-disk sample locations rotated by a
screen-pixel hash. The post's architectural theme is stable memory and bounded
host submission: allocate a maximum once, cache unchanged faces and keep cull
results on the GPU.

**What helps Lince.** Shadow maps can make a deformed topology immediately
legible in Perspective view. A raised brush stroke can cast onto a lower part
of the plane; a Sand glued to a slope can have a small contact shadow; a pit,
ridge or topology effect can read as geometry rather than only as color and
pattern distortion. The static/dynamic separation is also a useful concept:
unchanged terrain tiles and authored scenery should not be redrawn because one
Sand moved.

The important boundary is that lighting remains a disposable presentation of
the resolved spatial state. It never supplies height, collision, Area force,
membership or persistence. A shadow disappearing cannot change where a Sand
is, and a device without the selected shadow quality still has exactly the same
Box. This is especially important for the 2D and 3D relationship already chosen:
Top view and Perspective view observe the same surface-bound bodies and
topology; free-space mode removes the floor constraint, but does not acquire a
new semantic model merely because it uses different depth cues.

Lince should also distinguish world geometry from readable interfaces. The
topology and ordinary scene objects may receive world lighting. A native Sand
surface should use a hybrid treatment: its transform and occlusion belong to
the world, while text, controls and important chrome remain unlit or
display-referred so the configured colors preserve contrast. A subtle backing
or contact shadow can anchor it to the surface. A embedded browser texture follows the same
rule; the composited HTML must not become dim, color-shifted or illegible under
a world light. Pinned Sands and the HUD are composited after world lighting and
never participate in the shadow atlas.

**Needed for the spatial interface to work.** Perspective view needs correct
depth, topology normals, stable surface transforms and clear occlusion. It does
not need cascaded shadow maps. A directional key plus ambient/fill lighting and
normal-based topology shading can accurately reveal slopes while the topology
brush, Area visualization, selection and Sand composition are being built.
Top view must remain understandable with shadows disabled.

When shadows are added, their contract should be expressed in Lince terms:

- The renderer plan selects an actual memory budget, face capacity, resolution,
  supported filtering path and unavailable reason from adapter capabilities. A
  fixed 256-face allocation is not a semantic requirement.
- A light owns explicit stable shadow-face handles. Point, spot and directional
  lights request the number of faces they really use; storage is not inferred
  from `light_index * 6` forever.
- Each cached face records the light revision, camera/cascade revision, caster
  set revision, topology-tile revisions and renderer-device generation that
  produced it. A cache entry with a stale dependency is invalid, not “probably
  close enough.”
- Moving geometry dirties faces intersected by both its previous and current
  bounds. Rotation, scale, mesh/bounds change, insertion and removal are part of
  invalidation. A topology brush dirties only shadow regions influenced by the
  changed tiles when that optimization exists; until then, conservatively
  redrawing the affected light is correct.
- Shadow quality is a presentation/customization policy with honest modes such
  as off, crisp, soft and automatic. Contrast/accessibility policy may suppress
  noisy or strong world shadows without changing spatial meaning. Per-Sand
  cast/receive policy is explicit, with safe defaults for native and browser
  interface surfaces.
- Every capacity has an overflow report and defined behavior. Exceeding the
  local-light budget must not index outside a buffer or silently map two lights
  to the same layer; affected lights become visibly unshadowed or the renderer
  reallocates at a safe boundary.

Desk-to-globe scale prevents four fixed metre distances from becoming Lince's
long-term shadow ABI. Cascade or clipmap coverage must be derived from the
active spatial scale, camera projection and a declared maximum shadow distance.
The calm desk can use a small near field; a city or globe view may require
camera-relative clipmaps, virtualized pages or another large-world technique.
Those are interchangeable renderer strategies behind the same lighting
contract, not persisted properties of Records or topology.

**Needed only to make it fast or richer.** GPU matrix generation, per-face
dirty compaction, two cached atlases, GPU-counted indirect draws, cascade
blending, PCSS, temporal denoising and virtual shadow pages are optimizations or
fidelity features. None is necessary to prove surface-bound Sands, topology
editing or the Top/Perspective/free-space transitions.

Start with one directional light and a deliberately small, demand-sized shadow
resource only after normal shading is insufficient. Measure topology-edit
latency and moving-Sand frame pacing. Add cached static terrain, local-light
faces and softer filtering independently when a representative scene earns
them. The Box's full physics set stays active off camera, while shadow work is
allowed to follow rendered visibility because it has no behavioral authority.

A constant maximum allocation is not automatically a performance virtue. It
can remove allocator churn while wasting enough VRAM to evict more useful
resources. Demand-sized tier allocations, a page pool or explicit scene budget
can still be stable during a frame and rebuild only at declared boundaries.
Likewise, a CPU loop bounded by a compile-time 256 faces is technically constant
in scene size but can still record hundreds of render passes; frame time, not
Big-O rhetoric, decides whether it is acceptable.

**Source-audit corrections and risks.** The publication source and article do
not support several of the post's concrete claims:

- The post says the current face resolution is 512, while the article-day
  `RendererConfig` default was 1024. Both 256-layer atlases were always
  allocated. One 1024² `Depth32Float` atlas is 1 GiB, so the actual default
  reservation was 2 GiB, not the table's “~256 MB at 1024 px” and not the
  512 MiB total justified later in the post. Current Helio defaults to 32
  layers per atlas, reducing that 1024 default to 256 MiB total; making layer
  capacity configurable was the right correction.
- Every selected light reserved six consecutive slots in the publication
  scene code. Directional lights used four and filled two with identity; spot
  lights used one and wasted five. The stated capacity “42 point lights × 6
  plus 4 CSM cascades = 256” therefore was not the implemented packing. It was
  at most 42 shadow-casting lights total, with 252 allocated face slots. A unit
  test repeated the article's arithmetic without testing the allocator, so it
  passed while documenting a layout the runtime did not have.
- The four split distances were duplicated constants. `pssm_splits` existed but
  was unused, and the matrix shader did not calculate splits with lambda 0.5.
  The lighting uniform also exposed configurable split distances while matrix
  construction retained its constants. Changing one side could select a
  cascade whose matrix covered a different interval. Lince should generate or
  upload one authoritative cascade description and use it in every pass.
- Publication-day texel snapping hard-coded 2048 texels despite the actual 1024
  default and configurable atlas size. Current matrix generation receives the
  configured size, but current deferred filtering still hard-codes
  `ATLAS_SIZE = 1024`; non-1024 settings therefore remain internally
  inconsistent. Texture dimensions should come from the resource or one shared
  generated uniform, never parallel constants.
- The point-light far plane is `range * 2.5` on the rationale that cube corners
  need extra radial coverage. Six 90-degree projections already partition
  directions; extending radial distance does not fill angular seams and spends
  depth precision beyond the declared light range. The far plane should follow
  the light's actual influence/caster policy, with seam handling tested
  separately.
- Publication matrix-change flags and hashes were only 64-byte buffers—16
  entries despite a claimed 42-light budget—and the matrix dirty buffer was not
  connected to `ShadowDirtyPass`. Current Helio sizes them for 42 casters and
  consumes matrix dirtiness, a substantive fix.
- Publication `ShadowDirtyPass` let invocation zero clear shared dirty arrays
  and used `storageBarrier`, which synchronizes only a workgroup. With more than
  64 movable draws, other workgroups could mark a face before workgroup zero
  erased it. Current Helio correctly performs encoder buffer clears before the
  dispatch.
- The movement detector still stores only 256 previous positions even though
  the cull path admits up to 4,096 draws. It compares translation only and tests
  only the current bound. Rotation or scale in place can be missed, and an
  object moving out of a face can leave its old shadow cached because that old
  face is never dirtied. The previous and current swept bounds plus a full
  caster revision are required. Capacity must be tied to draw capacity rather
  than accidentally borrowed from the face constant.
- Publication shadow culling indexed `instances[draw_index]` instead of reading
  the indirect draw's `first_instance`; current main corrected that mapping.
  Both versions loop the fixed 256-face maximum in every cull invocation and
  use atomics into a 4,096-draw-per-face allocation, so the work and roughly
  20 MiB indirect buffer are material even when host submission is bounded.
- The PCSS code is directional-cascade-only and works in normalized
  orthographic depth, but fields labelled as metre-sized lights are divided
  directly by a pixel dimension without the cascade's world-to-UV scale. The
  resulting softness is an artistic texel policy, not the physical size model
  the names imply. The “TAA-friendly” rotation is stable at a screen pixel, not
  at a world point; camera movement changes the hash seen by a surface and can
  shimmer. This needs motion captures and temporal metrics rather than prose.
- Front-face culling and slope bias are useful acne controls, not universal
  correctness. Thin/open meshes can leak or lose shadows. `Depth24PlusStencil8`
  also does not promise a 25 percent memory saving merely because its depth
  component is named 24: it includes stencil and its physical representation is
  backend-defined.

The source tests mostly check copied constants, struct sizes and dispatch
division. They do not render the shader, compare CPU/GPU matrices, test cache
invalidation, measure shimmer or enforce the real atlas allocation. The post
offers no reproducible frame timing, memory capture or visual-error corpus.

**Ideas to test.** First build a small deterministic scene containing a sloped
topology tile, a pit and ridge, one surface-bound native Sand, one embedded browser Sand, one
floating Sand and one directional light. Capture Top and Perspective views with
shadows off and on. Native text and HTML colors must be unchanged; world
placement, picking and occlusion must agree; a topology edit must be visible in
the same interaction without a stale shadow.

Then exercise every invalidation edge: translation across two faces, rotation,
scale, mesh/bounds change, add, delete, static-to-movable change, camera motion,
light motion, cascade crossing, topology-tile edit, atlas resize and device
recovery. Hold the camera just inside each cascade blend and measure luminance
discontinuity and temporal variance. Sweep active faces, movable casters and
resolution while recording actual allocated bytes, CPU render-pass recording,
GPU matrix/cull/depth/filter time and p95/p99 frame pacing. Saturate face and
draw capacities and verify the visible diagnostic and safe result.

No physical 4K gate is required now. Shadow resources are independent of output
extent, while full-screen filtering cost is not; keep dimensions dynamic and
use synthetic large extents for correctness until representative hardware is
available.

**Verdict:** carry forward demand-budgeted shadow resources, explicit cache
dependencies, static/dynamic invalidation and GPU-resident cull-to-draw flow.
For v1, build topology normals and readable hybrid Sand composition first, then
add a modest directional shadow path only if it materially improves spatial
reading. Reject the fixed 256-layer design, duplicated constants, six-slots-per-
light ABI and “constant cost” framing. Full cascades, local-light atlases and
PCSS are measured quality tiers; the desk-to-globe renderer must remain free to
replace them without changing the Box or its data.

[Making GPUI Fast: A Compositor Thread, an Overscroll Buffer, and Why Browsers Don't Re-Render on Scroll](https://pulsarnative.com/blog/2026-06-30-gpui-compositor)

### 17. Keep the platform responsive with revisioned presentation, not a giant cached window

Read 2026-08-29. The article is explicitly a proposed design. Its publication-
day WGPUI tree was inspected at commit
[`31fbf35`](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/31fbf35800b4cfc2ddf202a1c16a2971251ca84c),
and its later direction was compared at Pulsar Native's current pinned WGPUI
commit
[`f9c3abb`](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/f9c3abb4aa5317e85cebb4fe3222a52da38b746d).
The separate GPUI external-compositor experiment previously raised by the owner
was also checked at
[`bfa9c6c`](https://github.com/MSIsunny/zed/tree/bfa9c6c148f286fb4f645571ca08080c51cf0820).
It solves texture-slot composition, not the compositor-thread proposal in this
post.

**What it proposes.** Split GPUI's frame into a main/event-thread producer and
a dedicated compositor-thread consumer. The main thread handles the window,
input, element-tree work and scene construction, then offers a `Scene` through
a capacity-one channel. The compositor owns scene finishing, previous-scene
damage comparison, GPU buffers, atlas uploads, pipeline objects, command
encoding and an offscreen “pipeline texture.” It submits that work and sends a
completion signal. The main thread then acquires the swapchain image, copies
the appropriate region from the pipeline texture, submits the copy and
presents it.

Capacity one intentionally drops an intermediate visual scene when rendering
falls behind instead of building latency. The next scene is expected to contain
all accumulated UI state. A synchronous path remains for tests, WebGPU,
debugging and compositor startup failure.

The second proposal is a 3× viewport texture in each dimension. During a pure
scroll, the visible viewport becomes another crop from already-rendered pixels,
so GPUI skips element traversal, layout, scene construction, upload and
rasterization. Near a cache edge, it renders newly exposed content; near the
texture edge, it proposes copying valid pixels to a recentered position and
filling the new edges. Scroll-only hit testing applies the same translation to
old hitboxes. Layout-, paint- and scroll-only notification classes decide when
this path is safe. The post admits open questions around variable-height
content, opt-in, nested behavior and position-affecting animations.

**What helps Lince.** The most valuable idea is not GPUI or overscroll. It is
that three kinds of traffic have different loss and ordering rules:

- Semantic input, Protein results, Actions, fixed physics ticks, Area crossings,
  collaboration operations and persistence acknowledgements are non-droppable.
  They progress independently of whether a frame is presented.
- A presentation snapshot is latest-wins. If revision 42 has not started and
  revision 43 supersedes it, rendering 42 only increases input-to-pixel latency.
  The renderer may skip it as long as 43 is a complete projection of all
  accumulated authoritative state.
- GPU/resource lifecycle traffic is ordered and non-droppable. Texture import,
  glyph or image readiness, embedded browser frame release, device-generation change,
  submission completion and resource retirement cannot disappear merely
  because the visual snapshot which first referenced them was replaced.

That separation fits the Lince-owned compositor better than grafting GPUI into
the runtime. The Wayland/winit platform loop can remain responsive to input,
IME, configure, scale and lifecycle events while a render coordinator owns the
frame graph and expensive preparation. A bounded latest-snapshot mailbox
prevents render backlog. One explicit submission coordinator establishes the
order among world rendering, native Sand rendering, browser import/copy, overlays,
swapchain composition and retirement. “`wgpu` is thread-safe” is not a frame
protocol.

Lince should retain independent presentation layers rather than one finished
window image:

- topology/world and native spatial instances;
- native Sand surfaces and Castle chrome;
- each visible browser external surface by imported-frame generation;
- selection, edit arrows, Area/topology tools and diagnostics;
- pinned HUD and accessibility overlays.

Each layer needs stable identity plus content, transform, clip, texture and
device generations. A transform-only change may reuse primitives or a local
texture. A content change damages only its layer. A device change invalidates
every device-owned product without changing Sand or Box identity. This is the
same family of idea as current WGPUI's retained layers and per-layer slabs, but
Lince can design it into its renderer instead of inheriting GPUI's element-tree
and cache history.

**Needed for the interface to work.** Define one immutable
`PresentationSnapshot` contract. It carries a monotonically increasing
presentation revision, semantic/spatial tick, device generation, camera and
viewport, stable Sand ids/generations, resolved visible transforms, layer
revisions, picking data, topology tiles, and external-surface frame handles.
It contains no borrowed application state. A renderer either presents that
revision or supersedes it; it never publishes half of two revisions.

The displayed revision is reported back. Picking and pointer capture resolve
against what the person can see: a picking result contains the displayed
revision and stable Sand generation, then authoritative Rust validates that
identity before applying an intent to current state. This prevents a dropped
frame from making the user click the new invisible position of an object while
its old image is still on screen.

Use separate bounded paths for snapshot replacement and lifecycle completion.
A replaced snapshot releases every embedded browser frame/import lease and retained resource
reference it held. An off-camera Website Sand remains normally executing, as
required, but the renderer does not import/sample/draw a new browser frame for
it. The browser producer coalesces to its latest frame and receives all required
release acknowledgements. Native physics and Behavior also continue off camera;
only presentation extraction and drawing are culled.

Wayland is the product target. Do not encode the article's assumed rule that a
`wgpu::Surface` must stay on the creating/main thread. Keep platform events on
the winit loop and prove the chosen Vulkan surface acquire/present ownership on
the actual stack. Prefer one thread to own device polling, queue submission and
presentation if it works cleanly; otherwise keep acquire/present on the
platform thread and use explicit submission serials and wakeups. Device loss,
resize and scale change cross a generation boundary and cannot race an external
producer or an old snapshot.

Do not use `ControlFlow::Poll` as a substitute for notification. Wayland frame
callbacks, event-loop proxies and compositor completion wakeups should leave an
unchanged, non-animating Lince asleep. Input, physics or media schedules work at
its real cadence; idle presentation consumes no core.

**Needed only to make it fast.** A dedicated render thread, parallel scene
finishing, persistent GPU slabs, damage rectangles, transform-only layer
composition, indirect drawing, local texture retention and latest-wins frame
dropping are optimizations. The same snapshot and revision rules can first run
synchronously, which supplies the correctness oracle for the threaded path.

For Box camera movement, persistent geometry plus a changed camera uniform is
usually better than a pixel overscroll buffer. Orthographic pan, Perspective
orbit, zoom, topology deformation, lighting, moving Sands, edit overlays and
browser frames invalidate different portions of a finished image. Re-rendering the
currently visible native instances can be cheap while preserving sharp text and
correct depth. Pixel retention is most plausible inside a stable 2D Sand with a
large document or list; there it should be local, tiled and combined with data
virtualization.

The owner requires that off-camera content not be rendered. A 3×-by-3× window
buffer proactively rasterizes eight viewports of content outside the camera and
therefore is not Lince's Box strategy. Previously visible pixels may remain in a
budgeted cache, but cache extension never renders beyond the active clip. The
renderer can reuse what happens to remain valid or draw the newly visible
region after a pan. This preserves the behavior/render distinction without
quietly turning “not rendered” into “rendered just in case.”

**Source and design audit.** The post is an architecture sketch, not a report of
shipping code:

- None of its named `CompositorJob`, `OverscrollBuffer`, environment-variable
  or previous-scene finishing symbols existed in the publication-day WGPUI
  source. The article supplies pseudocode, no patch, benchmark harness, trace or
  before/after result.
- Current WGPUI describes itself as an immediate-mode renderer with a view
  cache, contradicting the post's simple “retained-mode framework” premise. It
  later pursued keyed retained layers, retained element instances, persistent
  Taffy layout, per-layer GPU slabs, layer-local occlusion and localized
  overscroll margins. Its current design document reports these as shipped but
  still retains an older range-replay cache, so even that migration is not a
  clean finished state.
- Current WGPUI's own audit found that the scroll fast path required explicit
  keyed-layer policy and was absent from 32 of 37 ordinary product scroll
  containers. It reports stale hover styling between buffer refills and that
  view `render` still runs. Its 10,000-row plain-list investigation measured a
  21.17-second first paint, a 4.3-second blocking resize and an unthrottled idle
  present loop before further fixes. These are valuable findings, but they
  refute the post's implication that moving finish/upload alone makes a complex
  UI or large list effectively solved.
- Current WGPUI chose local layer textures and list-aware refill rather than the
  article's single 3× window texture. It also learned that a non-virtualized
  list still lays out all children on a refill; cached pixels do not make
  unbounded content construction free. Later layout containment improves
  declared-size children, with honest fallback for unknown sizes.
- The proposed recenter calls `copy_texture_to_texture` from one region of a
  texture to another region of the same mip/layer. WebGPU and current `wgpu`
  require source and destination subresource sets of a same-texture copy to be
  disjoint; changing only X/Y origins does not satisfy that rule. A temporary
  texture, double buffer, tiled ring or remapped tile table is required. It is
  not automatically a same-texture DMA operation.
- The article labels `wgpu::Surface` non-`Send`/non-`Sync` and
  `CommandEncoder` uncertain. Current native `wgpu` 30 statically asserts both
  `Surface` and `CommandEncoder` are `Send + Sync`. Platform APIs may impose
  narrower operational rules, but they must be verified for the selected
  version/backend rather than inferred from an unsourced table.
- A capacity-one visual channel is safe only for a complete derived snapshot.
  GPUI atlas allocation/upload, async image readiness, glyph creation, external
  frame ownership and callbacks have independent effects. If those effects are
  stored only inside the dropped scene, “the next draw accumulates everything”
  is false. Lince's separate lifecycle stream closes that hole.
- Copying a viewport-sized texture into the swapchain consumes full-screen
  bandwidth and adds a stage on every presented frame. It may still buy
  decoupling, but it is not near-zero work. The stated memory totals count only
  one RGBA8 image: a 3× factor in each dimension is nine times viewport pixels,
  before HDR, depth, retained layers, embedded browser surfaces, buffering and the rest of
  the renderer budget. “Fine on any 4 GB GPU” is not a capability decision.
- Screen-space hitbox translation is valid only for a proven pure translation.
  Nested scrollers, sticky/fixed content, hover changes, transforms, scale,
  zoom, wrapping, animations and asynchronously changing media require their
  own invalidation. Automatic scroll-only detection that is occasionally wrong
  produces stale pixels and wrong clicks, a worse failure than a slow frame.
- Running tests only through the synchronous fallback verifies the oracle but
  leaves the threaded protocol, dropped-frame cleanup, resize race, device loss
  and submission ordering untested. Both paths need the same conformance corpus,
  with controlled stalls and reordered completion.

The later external-compositor branch provides a separate useful example. It
registers generation-checked texture slots with device-pixel dimensions,
format/alpha metadata, compose-once-per-distinct-slot behavior, resize, stale
handle protection, device-recreation notification and GPU-lifetime retention.
Its Wayland path uses the wgpu renderer directly; macOS and Windows require
backend-specific sharing work. These details support Lince's external-surface
ABI thinking, but the GPUI trait is wgpu-producer-specific and event-thread-
resident. embedded browser DMA-BUF import, permissions, input, audio/video lifetime and Box
events still require the broader Lince-owned Website Sand contract.

**Ideas to test.** First make synchronous and threaded render coordinators
consume the same recorded snapshots. Randomly delay scene preparation, GPU
submission, embedded browser frame readiness and presentation; the displayed revision must
never go backward, no stable handle may resolve to a newer occupant, and every
dropped frame lease must retire exactly once. Resize, scale change and device
loss while a frame is in flight. The platform loop must continue receiving
input and IME throughout.

Measure platform-event latency, input-to-present latency, dropped snapshots,
CPU preparation, queue depth, upload bytes, full-screen-copy time, retained
texture bytes and p95/p99 frame pacing for the accepted 1,000-active/10,000-
resident/200-visible workload. Include all-moving physics, calm desk, topology
editing, Perspective navigation, browser video and a large text/list Sand. Verify
that off-camera tick/media counters advance while their draw/import counts do
not. Leave the window idle and require zero redraw/present spin.

For local document retention, compare no cache, retained primitives, visible-
only tiles and a bounded pixel cache. Pan forward and backward, mutate an old
row, change font scale, hover while scrolling and insert variable-height
content. Differentially compare pixels and hit results against a full fresh
render. No physical 4K gate is needed; keep extents dynamic and record actual
allocation arithmetic at synthetic large sizes.

**Verdict:** adopt a responsive platform/render split, bounded latest-wins
presentation snapshots, non-droppable lifecycle acknowledgements, displayed-
revision picking and independently retained layers. Do not adopt GPUI or the
article's one-texture overscroll architecture. Use persistent scene data and
local virtualization first; allow pixel retention only for a proven 2D layer,
within the camera and memory budget. Current WGPUI and the external-compositor
fork are implementation repertoire to study, not dependencies or evidence
that Lince should reverse its owned-compositor decision.

[Post Processing and Custom Shader Injection in Helio](https://pulsarnative.com/blog/2026-07-07-post-processing-shader-injection)

### 18. Compile a typed visual-effects graph; never make arbitrary WGSL a Sand capability

Read 2026-08-29. The article was compared with Helio at its publication-day
commit
[`d04034b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/d04034be77eb25409e31827b62018bf3647fe340)
and current commit
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
This matters because several headline claims do not describe the published
implementation, while later changes expose a useful render-graph lesson.

**What it presents.** Helio collects post-processing into an “uber” fragment
shader. A frame first runs compute work for spatial post-process-volume
blending, auto-exposure and a five-level bloom pyramid. One full-screen draw
then reads the HDR and depth buffers and evaluates a fixed sequence: exposure,
bloom, grading, white balance, tonemapping, vignette, chromatic aberration,
grain, depth of field and motion blur. Disabled effects branch to an identity
result.

Four textual markers permit custom WGSL before the built-in chain, after
tonemapping, after grain or at the end. One API accepts complete function
definitions; another wraps a supplied expression in a generated function.
Sixty-four unnamed `vec4` values form a generic parameter buffer. Changing an
effect regenerates the WGSL module and render pipeline. The article describes
post-process volumes as GPU-blended AABBs with priority, proximity and unbound
variants, and describes the resulting settings as a spatially varying visual
style.

**The useful architectural core.** A single full-screen pass can reduce
intermediate render targets and memory traffic when several operations are
pure functions of the same pixel in the same color space. Lince should preserve
that opportunity without making the uber-shader its authoring model. Define a
typed visual-effects graph, validate it, then let the renderer fuse compatible
adjacent nodes into one generated shader. Exposure compensation, grading,
white balance, tonemapping and perhaps vignette can often fuse. Neighborhood,
history and multi-resolution operations such as bloom, depth of field, motion
blur and temporal effects declare separate inputs, outputs and passes.

Every node needs an input and output color-space contract, execution domain,
resource requirements, parameter schema, deterministic order and accessible
fallback. The compiler, not a Sand, chooses fusion boundaries. That gives Lince
both composition and performance: customization can construct named effects
and presets while the runtime remains free to lower them differently on each
adapter.

The layer boundary is important. World/topology rendering may receive a world
post-process before native readable Sands, embedded browser surfaces, edit controls, pinned
HUD and accessibility overlays are composed. A stylized Sand or Castle may opt
into a scoped effect layer, but a global world effect must not silently change
browser pixels, text contrast, focus indication or selection colors. An
accessibility override must be able to disable motion blur, grain, chromatic
aberration, flashes and contrast-reducing effects independently of a scene's
art direction.

Post-process volumes are presentation regions, not Protein Areas of Influence.
They may make the world warmer, foggier or more exposed as the camera enters a
region; they never move a Record, change data or acquire semantic authority.
Likewise, a topology material's pattern distortion, contour color or shader is
usually a surface/material graph evaluated with topology data, not a
screen-space post-process. Keeping those three concepts distinct prevents a
visual preset from becoming an accidental Behavior.

**Needed for it to work.** The minimum correct contract is a linear/HDR world
color space, an explicit display transform and output format, stable
world/native/browser/overlay layer ordering, typed effect parameters, deterministic
blend and order rules, visible compilation diagnostics, an unchanged previous
pipeline on compile failure, and recovery after device loss. A calm 2D desk
can initially use only the correct display transform; bloom, grain and cinematic
effects are not prerequisites for Sands, Areas or topology.

Full user-authored WGSL is privileged executable code, not configuration. The
published builder inserts text into the same shader module as every binding.
Although its advertised function takes only color, UV and dimensions, injected
code can name the HDR and depth textures, bloom and noise textures, camera and
volume buffers, custom parameters, and read-write exposure and blend buffers.
WebGPU validation supplies bounds and resource safety; it does not make an
expensive or non-terminating shader trustworthy or prevent a GPU/device denial
of service. A Website Sand or other external HTML must never receive this
capability.

The ordinary authoring surface should therefore be a safe declarative graph or
expression language with bounded node types, no arbitrary loops, no storage
writes and only declared inputs. Lince compiles it to WGSL. A separate local
developer mode may accept raw WGSL as explicitly trusted code, with adapter
limits, compilation diagnostics, time/cost guards where feasible, old-pipeline
fallback and device recovery. It is not synced or activated for a collaborator
as inert-looking Box data. The permission boundary is based on behavior, not on
whether the text arrived through Maud, JavaScript, a native Sand or a file.

Effect parameters should be named and typed—units, range, default, animation
policy, color space and accessibility effect—not anonymous array offsets.
Customization tokens may bind to that schema, and the generated host layout and
WGSL layout should come from the same description. This avoids the article's
64-`vec4` buffer, whose setter has no capacity check, whose shorter writes leave
an old tail resident and whose callers must privately agree that an index means
time, intensity or pointer position.

**Needed only to make it fast or richer.** Shader fusion, asynchronous pipeline
compilation, pipeline caching, GPU-side region blending, bloom pyramids,
histogram exposure, LUTs, temporal adaptation and effect-quality tiers are
optimizations or presentation features. They follow measurement. Rebuilding in
`prepare()` at the beginning of the next frame merely relocates synchronous
module/pipeline creation; it does not eliminate the frame spike. One published
API also rebuilds synchronously at the call site. A real replacement is
prepared away from the presentation critical path where the backend permits,
then atomically becomes current; the last good pipeline remains usable.

**Source audit.** The publication implementation is a valuable warning against
treating an ambitious shader and prose as a validated subsystem:

- “One pass” means one final draw, not one post-process pass. Volume blending,
  exposure, bloom extraction and four bloom downsamples precede it. Fusion saves
  bandwidth only for operations that can actually share the final pixel pass.
- The claimed 128-bin, outlier-resistant histogram does not exist. The compute
  shader calculates subsampled average log luminance, with no bins, percentile
  rejection or nit-range mapping. Every 16×16 workgroup writes the same
  `avg_luminance[0]`, so multiple workgroups race and the winner is
  nondeterministic. Its dispatch and shader strides also make workgroups sample
  overlapping image sets rather than independent tiles.
- More decisively, the final shader never reads `avg_luminance`. It multiplies
  color only by fixed exposure compensation. Exposure mode is never consulted,
  and the exposure compute runs in manual mode too. The CPU settings contain
  bright/dark adaptation speeds, but those fields never reach the GPU uniform.
  The advertised auto exposure and temporal adaptation therefore do not
  function in this source.
- Volume blending scans a fixed 256 slots in one thread and performs an
  insertion sort, despite the early “arbitrary volume counts” claim. It ignores
  the actual count. For a bounded volume it first rejects cameras outside the
  AABB, then clamps an already-inside camera to that AABB; distance is therefore
  always zero. `blend_radius` cannot create the described boundary falloff.
- Dense removal does not clear the vacated GPU slot. Because the shader scans
  all 256 entries for positive weight, a removed last entry can survive as a
  stale or duplicated volume. The retained CPU reference implementation also
  uses different outside-AABB falloff semantics, so it is not yet an oracle.
- Bloom is five separate compute dispatches followed by five texture samples,
  not just a cheap branch in the uber-shader. If any volume exists, the host
  conservatively runs bloom because the authoritative blended setting is not
  known there. `bloom_radius` is blended and stored but never affects extraction,
  downsampling or composition. The article's reported desktop time has no
  benchmark setup or trace.
- Depth of field performs up to a 15×15 neighborhood inside the fragment pass,
  and motion blur samples a constant horizontal direction rather than a motion
  vector. Their placement after tonemapping also means they resample the
  original HDR input while mixing it with already-tonemapped `color`, violating
  a coherent color-space contract. An identity branch is not evidence that all
  enabled effects belong in one pass.
- `set_user_shader(None)` is documented as restoring the default, but `None`
  also means “no pending update,” so `prepare()` never observes the reset.
  Compilation has no error scope or recoverable typed failure around pipeline
  replacement. Injected complete functions all assume the same
  `user_effects` name and can collide.
- The pass has no dedicated tests. Neighboring pass tests largely check layout
  and arithmetic; there are no shader-output goldens, CPU/GPU volume
  differential tests, malformed-injection tests, exposure determinism tests or
  pipeline-failure tests.

Current Helio later moved volume blending into an earlier graph pass after
finding that volumetric fog consumed camera defaults while the late
post-process pass produced the blend. That is exactly why Lince's derived
visual settings should be an explicit typed producer before their first
consumer. Current Helio also added 3D-LUT support, showing that the article's
categorical rejection was a temporary product choice. Interpolating LUTs is a
standard artistic transition even when it is not algebraically identical to
interpolating the source parameters.

**Ideas to test.** Build a CPU reference for the typed graph and compare fused
and deliberately unfused GPU lowering for representative pure nodes, including
linear/HDR boundaries, alpha and final display encoding. Golden captures must
show that native text, focus, browser pixels and accessibility overlays are
unchanged by world effects. Malformed and oversized graphs, raw shader compile
failure, an intentionally excessive shader and device reset must retain a
usable previous presentation and expose a human-readable diagnostic.

For spatial presentation regions, test inside, outside, every boundary,
overlap, equal and unequal priority, insertion, update, removal and capacity.
Compare CPU and GPU results bit-tolerantly. For future exposure, test black,
white, a bright outlier, resolution changes, multiple dispatch shapes and
bright/dark adaptation curves; repeated identical inputs must not change with
workgroup scheduling. Bloom disabled should allocate and dispatch no transient
work in the selected implementation, and a region with unrelated settings must
not force it on.

Profile the actual lowered graph with representative desk, topology and 3D
world scenes. Record transient bytes, texture reads/writes, pipeline-compile
latency, CPU encoding, individual GPU pass time and p95/p99 frame pacing. Keep
extent handling dynamic and test large synthetic dimensions, but no physical
4K gate is needed now.

**Verdict:** keep a typed effect/material graph, explicit color spaces and a
compiler that opportunistically fuses compatible nodes. Keep post-process
regions presentation-only and apply global world styling below readable native
and HTML layers. Reject arbitrary WGSL as a Sand or Website capability, the
anonymous custom buffer, fixed textual injection ABI and the publication's
volume/exposure implementation. Raw WGSL can exist only as privileged local
developer code. For v1, correct layer composition and display color are the
requirement; richer effects earn their place through the same representative
benchmarks as the rest of the renderer.
[Helio Fusor: How the Executor Took Ownership of the Render Pass and Why Nobody Noticed](https://pulsarnative.com/blog/2026-07-07-helio-fusor)

### 19. Let the frame-graph compiler own pass boundaries, but keep graph order true

Read 2026-08-29. The article was checked against its publication-day Helio
commit
[`d04034b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/d04034be77eb25409e31827b62018bf3647fe340)
and current commit
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The central ownership decision is useful for Lince. Most of the claimed
bandwidth, aliasing and backend behavior is not implemented by the inspected
source.

**What it proposes.** Helio's “Fusor” moves `wgpu::RenderPass` creation and
closure from individual pass crates into the render-graph executor. Passes
describe their color/depth attachments and record draws into a render pass the
executor supplies. At graph lock time, a greedy scan finds consecutive passes
whose declared write/read sets overlap and whose probed attachment-view
pointers match. The executor then opens one render pass for the range instead
of one per pass.

A `chain_transparent` pass may sit inside the range while recording only to a
separate compute encoder. The post calls this a bridge. It also describes
resources whose complete lifetime falls inside a chain as `chain_local`, says
their final store is changed to `Discard`, and says non-overlapping resources
share allocations. A debug snapshot exposes chains, local resources and
claimed saved memory.

The concrete shipping example is much narrower than a G-buffer subpass chain:
Billboard and Corona both append blended draws to the same `pre_aa` color and
depth attachments. Continuing one ordinary render pass across those overlay
draws can avoid an end/begin pair. The article separately admits that `wgpu`
does not expose Vulkan subpasses or their input-attachment dependencies.

**What helps Lince.** The frame graph, not every Sand renderer or visual
feature, should own scheduling, pass boundaries, temporary resources,
load/store decisions, profiling scopes and submission order. A render feature
declares what it reads and writes and supplies safe recording work. It must not
dictate that its own draw is a standalone GPU pass. This is how native Sand
primitives, topology, Areas, selection overlays and the 3D world can later be
batched or fused without changing their semantic APIs.

This does not mean one pass per Sand. Lince should first extract visible native
Sands into persistent instance buffers and sort/batch by render phase,
material, clip/depth policy and pipeline. Thousands of similar cards, labels or
topology marks then become a small number of draws inside a few intentional
passes. Each visible embedded browser surface contributes a composited external texture;
external HTML never contributes GPU passes or frame-graph declarations. The
world, native readable Sand, the embedded browser, edit-overlay and pinned-HUD layer boundary
from the compositor plan remains explicit.

The graph compiler may later combine adjacent compatible render nodes, fuse
pure post-process nodes, reuse transient textures and choose a platform-specific
local-rendering path. Those are lowering decisions. The authored graph remains
an ordered, backend-independent statement of data dependencies and observable
results. A fused and unfused lowering must produce the same pixels and the same
displayed revision.

**Needed for it to work.** Every graph resource needs a stable logical id,
format, extent, sample count, mip/layer range, usage, initialization state and
temporal generation. Every node declares exact read/write access, pipeline
domain, and whether a read consumes this frame or an earlier history revision.
The compiler validates the DAG, establishes the real compute/render/copy order,
and fails with a visible diagnostic for a cycle, uninitialized read, incompatible
usage or exceeded adapter limit.

Pass compatibility is the complete attachment contract, not pointer equality:
ordered color slots, exact subresources, resolve targets, depth/stencil aspects
and read-only state, load/clear/store operations, sample count, extent,
multiview, query scopes and every resource bound during the render-pass usage
scope. A later node that requires a clear, samples a writable attachment,
changes the attachment set or depends on intervening compute creates a boundary.
The [WebGPU usage-scope rules](https://gpuweb.github.io/gpuweb/#resource-usages)
make one entire render pass a usage scope and forbid combining a writable
attachment with an ordinary sampled binding of that subresource. General
G-buffer consumption therefore cannot be manufactured by merely keeping an
ordinary `wgpu::RenderPass` open.

Commands must execute in graph order. Use one command encoder for sequential
render/compute/copy work where possible, or split at deliberate boundaries and
submit command buffers in dependency order. Parallel CPU recording is allowed
only after the graph proves independence; it never changes GPU happens-before.
Timestamp queries surround the actual commands they name and resolve only after
those commands.

The executor should expose a safe recording interface. Lince does not need
`*mut RenderPass<'static>`, `transmute` or a trait that encourages passes to
retain a forged pointer. An owned, renderer-neutral pass description can be
lowered to a stack-local `wgpu` descriptor, and safe closures/recorders or
compatible render bundles can receive a short-lived render-pass borrow. A
feature cannot keep that borrow after recording.

`StoreOp::Discard` is valid only when the attachment's result is not read after
the render pass; the official [`wgpu` contract](https://docs.rs/wgpu/latest/wgpu/enum.StoreOp.html)
says it becomes uninitialized. Tile-local intermediate data requires a real
backend facility—subpass input/local read, transient attachment or another
supported mechanism—with feature detection and an unfused fallback. The
[Vulkan tile-rendering guidance](https://docs.vulkan.org/guide/latest/tile_based_rendering_best_practices.html)
also treats merging as implementation-dependent and distinguishes neighborhood
sampling, which still needs another pass, from current-pixel local reads.

**Needed only to make it fast.** Continuing compatible overlay draws in one
pass, transient-resource reuse, aliasing, render bundles, CPU-parallel
recording, fused post-processing, backend local-read paths and discard/store
selection are optimizations. Lince first needs a correct graph and representative
captures. A tens-of-microseconds claim is much smaller than one missed frame at
60 or 120 Hz and cannot justify an unsafe or false scheduler.

Physical allocation reuse must be real. Logical resources with non-overlapping
lifetimes and descriptor-compatible storage may map to the same pooled texture
at different phases, but resources alive at the same time never alias. In
particular, multiple attachments first written by one G-buffer pass are
concurrent by definition. Peak-byte accounting must be derived from actual
physical allocations, not alias-group labels.

**Source audit.** The publication source contradicts the post in several
load-bearing places, and current Helio retains the same core behavior:

- The only implemented fusion is multiple draw sequences in one ordinary
  single-subpass `wgpu::RenderPass`. There is no transition to another Vulkan
  subpass and no input-attachment/local-read mechanism. The article alternately
  calls this subpass fusion, admits everything remains subpass 0, and says
  WebGPU keeps separate render passes, although the inspected executor follows
  the same shared-`RenderPass` path rather than a WebGPU-specific standalone
  path.
- Attachment probing compares only a vector of color-view addresses plus one
  depth-view address. It ignores resolve views, load/store and clear values,
  depth/stencil operations and read-only state, query sets and the rest of the
  compatibility contract. The executor opens the first descriptor and never
  applies later descriptors, so a later pass's requested clear or different
  store/depth policy silently disappears.
- A write/read name intersection does not establish a legal in-pass dependency.
  If the second draw truly samples the color attachment the pass writes, WebGPU
  usage validation rejects it. Billboard and Corona work only because “read
  `pre_aa`” means load/blend onto the same attachment; neither uses `pre_aa` as
  the advertised intermediate sampled texture. This continuation cannot keep
  a deferred G-buffer texture locally available to a lighting shader.
- `CachedPass.store_ops` is initialized entirely to `None` and no publication
  code ever sets an entry to `Discard`. `chain_local` affects diagnostics and
  alias-group names, not attachment store operations. The claimed tile-memory
  write-back elimination is absent.
- The texture pool always calls `device.create_texture` for every logical
  resource. It records `alias_group` and reference counts but never reuses an
  allocation, and no graph code calls its `release` method. Thus the claimed
  SSAO/Hi-Z aliasing, lifetime reuse and saved VRAM are absent too. The comment
  that resources with the same first writer “are never alive concurrently” is
  backwards; outputs of the same pass commonly are concurrent attachments.
- The executor records all compute commands into one encoder and all render
  commands into another, then submits the entire compute command buffer before
  the entire render command buffer. This does not preserve the pass loop's
  order. A supposedly transparent compute pass between two render passes runs
  before both at the GPU. The canonical performance analyzer samples and copies
  `pre_aa` this way, so it cannot observe the intended current-frame point
  between the draws. Any compute that depends on an earlier render result is
  likewise wrong.
- GPU “start” and “end” timestamps for every logical pass are written to that
  compute encoder around CPU recording. Render work lives in the later render
  command buffer, outside those timestamps. These values cannot measure the
  named render passes or substantiate the fusion timing claims.
- Sixteen pass implementations allocate their attachment slices with
  `Box::leak`; frequently executed descriptor calls leak memory every frame.
  The executor then uses `ManuallyDrop`, lifetime transmutation and raw static
  render-pass pointers. Debug-only checks catch two calls by a transparent
  pass, but cannot make a stored pointer or broader aliasing mistake sound.
- Unit tests exercise only the greedy range calculation with synthetic names
  and integer attachment signatures. They do not render fused/unfused images,
  validate descriptors, test compute ordering, verify a discard, inspect an
  allocation reuse, capture backend commands or benchmark either path.
- The article gives no harness, raw trace, GPU capture or reproducible settings
  for its M1 Max, RTX 4090 and Adreno figures. It even calls the M1 Max an
  eight-core GPU; [Apple specifies up to 32 GPU cores](https://www.apple.com/newsroom/2021/10/introducing-m1-pro-and-m1-max-the-most-powerful-chips-apple-has-ever-built/).
  The numerical claims should not enter a Lince budget.

**Ideas to test.** Construct a tiny typed graph corpus with overlay draws,
G-buffer-to-lighting, compute between draws, render-to-compute-to-render,
history reads, clear/load changes, depth read-only changes, resolve targets,
multiple mip/layer subresources and a deliberately illegal feedback loop. Run
the unfused oracle and each optimized lowering under validation, then compare
pixels, buffers and timestamp order. Randomize independent DAGs and verify that
every execution is a topological ordering.

Instrument logical and physical resource ids and require allocation overlap to
match the lifetime proof. Poison a released transient before reuse. Assert that
discarded content is never subsequently read. Force adapter feature changes,
resize and device recovery, and make the graph compiler regenerate safely.

For performance, capture the actual Vulkan command stream on the Wayland target
and use GPU timestamps around real draw/compute ranges. Compare intentional
pass continuation on/off for native Sand overlays, topology/world geometry and
edit layers. Record pass count, barriers, attachment load/store bytes, transient
peak bytes, CPU encoding, GPU time and p95/p99 frame pacing. Test calm and
all-moving 1,000-active/10,000-resident scenes; no physical 4K gate is needed,
but dimensions remain dynamic.

**Verdict:** adopt executor-owned pass lifecycles and a typed, inspectable frame
graph. First batch native Sands by render phase and preserve exact graph order.
Treat compatible-pass continuation, local reads, discard and resource reuse as
separate proven lowerings with an unfused oracle. Do not copy Helio's raw-pointer
ABI, separate all-compute-first submission, pointer-only compatibility,
`chain_transparent` bridge or unimplemented alias/store accounting. The article
is repertoire for the ownership direction and for failure modes, not a Fusor
implementation Lince should import.
[The AI in the Machine: AI Tools and Context at Scale](https://pulsarnative.com/blog/2026-07-11-pulsar-ai-tools)

### 20. Give agents the same typed, permissioned command bus as people—not native ambient authority

Read 2026-08-30. The article was checked against publication-day Pulsar commit
[`d2bc125`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/d2bc125a0080615aec77f75021594c8fe19e6ace),
current commit
[`0f4ee79`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/0f4ee7961addc0084ebec427542c26773c7ea0ac),
and the ToolbeltRS commit pinned by both,
[`116b902`](https://github.com/Far-Beyond-Pulsar/ToolbeltRS/tree/116b9028bad6b9467073de2534be60ef4df9b279).
The command-surface direction is valuable. The post's claims about tool
scoping, context size, filesystem isolation, undo, provider-secret storage,
skills, MCP and latency are substantially broader than the inspected system.

**What it proposes.** Pulsar puts the agent inside the same native process as
its editor and permanently loaded plugin libraries. Plugin tools are ordinary
Rust functions; a `#[tool]` macro extracts their signatures and documentation,
generates JSON-facing wrappers and registers them through `inventory`. The
article presents this as a direct, serialization-free route from an LLM tool
call to live editor state.

Tools are described as belonging to the plugin and file type currently in
scope. Instead of sending every schema to the model, the post says the system
prompt exposes only `list_tools(path)`, `describe_tool(path)` and
`call_tool(path, args)`, allowing the model to discover a hierarchical tool
tree on demand. Reflection can supposedly generate a universal property-edit
tool. EngineFS gives the agent a provider-independent workspace view while
editor-specific commands mutate open state rather than bytes, preserving
syntax, immediate UI feedback and undo.

The same provider trait is said to cover fifteen OpenAI-compatible services,
live model discovery, validation before encrypted storage and later
revalidation. The post also describes provider-independent skills, a text
fallback for models without native tool calls, optional MCP sources, and
collaborative agents sharing filesystem and undo streams. Native calls are
positioned as the necessary low-latency path for interactive mutation, with
process isolation deferred only for community plugins.

**What helps Lince.** Human gestures, an agent request, an external Sand event
and a collaborator's operation should converge on the same authoritative
domain commands. A Sand dragged by a person and a Sand placed by an agent must
not have two mutation implementations. Both request an operation such as
`box.sand.place`; Rust validates it, commits it at a defined simulation/state
boundary, persists the semantic result and publishes the resulting revision to
rendering, collaboration and the visible history.

That command surface should cover the language of Lince rather than expose
memory-shaped setters. Representative families are:

- inspect Box selection, Sand definitions and instances, Protein provenance,
  Areas, groups, topology, schema and current revisions;
- create, compose, connect, place, group and remove Sands; connect Protein
  output fields to Sand inputs; configure Areas and topology effects;
- preview and commit a bulk layout, topology stroke or data mapping;
- invoke an allowed Action or emit a named Box event; and
- inspect a transaction, explain “Why is it here?”, undo an eligible local
  transaction and resolve an explicit revision conflict.

A command descriptor needs a stable namespaced id and version, exact input and
result schemas, declared read/write effects, required authority, confirmation
policy, concurrency policy, idempotency/retry behavior, deadline, output bound
and collaboration/offline policy. An invocation carries the principal, Box and
target identities, expected base revision, idempotency key and trace id. A
successful commit returns before/after revisions, affected stable ids, the
semantic operations, an inverse when one exists, diagnostics and provenance.
Those are not LLM details; they are the same transaction contract the edit UI,
Behavior pieces, collaboration and persistence need.

The agent must issue intentions, not participate in the render or physics
loop. For example, it may place an Area, change its filter or request a Sand
position. It does not write a physics body's coordinates from an arbitrary
worker thread each frame. The authority loop accepts the command at a fixed
boundary; settled spatial state and meaningful user operations enter the Box
history according to the persistence design. GPU instance ids and transient
physics handles remain disposable projections.

Demand-paged discovery is also worth keeping, but Lince's scope is richer than
an open file extension. Available operations depend on the principal, active
Box, selected Sand/group/Area, edit or use mode, Protein grants, connection
state and whether a destructive preview has been approved. Discovery returns
only commands the principal could meaningfully request in that context. It
must never imply that seeing a command grants permission to execute it; the
authoritative handler checks again at invocation and commit.

This preserves the external-HTML boundary. A Website Sand runs in its browser
security context and talks to a narrow broker using its Sand-instance identity.
It may subscribe to granted Protein projections, receive permitted Box events,
hold explicitly granted web storage and network capabilities, and request
allow-listed domain commands. It cannot load a native plugin, see the native
registry, obtain arbitrary filesystem paths or turn HTML content into
authority. Built-in Sands can use an in-process adapter to the same bus, but a
faster adapter does not change command semantics or permissions.

The human surface is part of the mechanism. Edit mode needs an activity Sand
that shows the actor, requested change, targets, pending confirmation, current
state, result, revision and undo availability. Bulk or destructive commands
need a visible preview. Empty states must distinguish no agent configured, no
authority granted, no relevant commands and no recent activity. A user must be
able to inspect and stop work without reading Rust or a chat transcript.

**Needed for it to work.** The registry must reject duplicate canonical ids,
invalid schemas and incompatible versions at startup. It should validate the
complete JSON schema before dispatch, not merely deserialize whatever fields a
wrapper happens to read. Domain invariants, authorization, expected revisions
and target existence are checked again in the authoritative transaction. An
unknown command or version fails closed.

Mutating commands are serialized or proven commutative. Parallel read-only
queries may share a revision; arbitrary tool calls are not launched together
merely because one model returned them in one response. Multi-operation
commands commit atomically or report an unapplied failure. Cancellation and
deadlines have defined semantics: cancelling the wait does not pretend an
already committed side effect disappeared, and a timeout cannot leave an
unreported background mutation.

Filesystem authority should use rooted capability handles rather than trusting
user/model path strings. Resolution must remain anchored through opens and
renames, including symlinks and non-existent write targets. Read, create,
modify, delete, network and process execution are separate grants. Provider
secrets belong in an operating-system secret store or an equivalently designed
encrypted store whose key is not stored beside the ciphertext; they never
enter ordinary Box data, prompts, logs, collaboration or Facade output.

Collaboration requires more than swapping a local `FsProvider` for a peer
provider. The shared unit is the validated semantic operation with stable ids,
actor provenance, causal/base revision, idempotency and conflict behavior. Each
peer must not independently accept the same conflicting command against
different state and call the resulting byte synchronization collaboration.
Undo is scoped to an operation and actor/context; a person's Ctrl+Z must not
silently reverse an unrelated collaborator or agent transaction.

Agent input and retrieved content remain untrusted. A Record body, Website,
remote response or collaborator message can contain an instruction-shaped
string but cannot widen the current principal's capabilities. Sensitive
Protein projections require explicit egress policy and visible disclosure
before they are sent to a cloud provider. Tool outputs need size limits and
structured truncation so a directory, web response or record set cannot consume
the conversation or application memory without bound.

Reflection is suitable for deriving inspection schemas and eliminating
duplicated metadata. It is not a universal authorization or validation layer.
A generic field write can bypass cross-field constraints, derived state,
Protein provenance and transaction meaning. Lince should expose domain
commands for consequential changes; a reflected property editor is acceptable
only where the same constraints, authority and inverse operation are known.

**Needed only to make it fast.** An in-process Rust adapter, lazy schema
discovery, cached command descriptions, cached provider model catalogs,
batched queries, parallel read-only work, derived reflection metadata and
compact result projections can reduce overhead. None establishes correctness.
An LLM/network turn usually dominates a microsecond-scale function dispatch,
and ordinary interactive state can cross a local protocol boundary without
missing a frame if the authority and presentation paths are designed well.
Provider networking and model streaming should stay off the platform/render
loop; committed state reaches presentation through the same revisioned
snapshot path as human edits.

MCP is neither required for Lince's internal command bus nor inherently a
remote HTTP service. The official
[MCP transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
defines local child-process `stdio` as well as Streamable HTTP and permits
custom transports. Direct calls and MCP are adapters at different trust and
interoperability boundaries, not competing sources of domain truth. Lince can
keep the internal typed bus independent and add a protocol adapter later if a
measured interoperability need warrants it.

**Source and security audit.** The publication source contains a useful start,
but not the architecture the prose says is running:

- Toolbelt's `#[tool]` does register functions and generate wrappers. Its
  generated schema recognizes primitives and `Option`; all other parameter
  types become an undifferentiated `object`. It does not derive nested fields,
  enums, array constraints, field descriptions, `serde` rules or a result
  schema. Its `timeout_ms` is explicitly stored as an unenforced hint.
- `ToolRegistry` keys tools only by their short name and deliberately
  overwrites duplicates. The plugin bridge does the same, so two providers can
  silently replace one another. Inventory collection uses textual
  `starts_with` matching on a Rust module prefix rather than an exact namespace
  boundary. A Lince registry cannot use any of these as canonical identity.
- Neither the publication tree nor current Pulsar contains the advertised
  `list_tools`, `describe_tool` or `call_tool` navigation tools in the inspected
  agent/plugin subsystem. The chat panel collects `definitions()` for the
  complete default registry and sends all of them with every provider request;
  current code retains that behavior. The article's three-tool system prompt
  is therefore a design description, not the implementation.
- Pulsar does have a file-specific bridge builder that asks each plugin for
  capabilities. The chat panel builds the all-plugin bridge instead. Cached
  `AvailableTool.file_types` values are always empty, making
  `tools_for_file()` treat every cached tool as universal. The map is again
  keyed only by tool name. File scoping can be implemented through the
  capability query, but the claimed “tools literally do not exist outside the
  open file” invariant is absent.
- The plugin execution trait receives a path, tool name and JSON value; it does
  not receive an editor-instance handle, transaction, authority, revision or
  cancellation context. The level-editor implementation is a positive local
  example because it looks up open state and routes many edits through a shared
  `SceneCommand`. That executor only bumps a revision and says undo/redo is
  ready to be layered on top. No inverse/transaction history is implemented
  there, several paths still mutate directly, and the generic plugin API cannot
  enforce this discipline. The article's universal Ctrl+Z guarantee is false.
- No `edit_property` agent tool, registered skill mechanism or MCP integration
  appears in the inspected agent/plugin subsystem at the publication commit.
  The reflection library and editor property machinery exist, but the article
  extrapolates them into agent capabilities that are not wired up.
- EngineFS's registered agent tools are `cd`, `list_files` and `tree`; the
  described demand-loaded file-reading tool is absent from that registry.
  EngineFS itself exposes byte reads/writes/deletes/renames, uses one
  process-global mutable provider, and defaults/reset to an unrestricted local
  provider. Its tooling state is also global, so concurrent workspaces or
  agents can replace one another's root/current directory.
- The bridge installs an `unrestricted` filesystem context and its only path
  check is lexical `starts_with`. Its create/modify/delete flags are never
  consulted, and its own source acknowledges that a native plugin can call
  `std::fs` directly. A separately constructed rooted local provider does
  canonicalize existing paths and ancestors, but that stronger provider is not
  the default and does not turn unrestricted native code into a sandbox.
- The main loop launches every tool call in a response on a separate native
  thread, including mutations, then joins them. Tool functions also recover
  request state from process-global stores instead of consistently using the
  per-call `ToolContext`. Multiple calls, chats or workspaces can therefore
  race over shared context. Cancellation is checked between model iterations,
  not while a tool blocks, and the macro timeout is not enforced.
- The provider crate contains sixteen entries at the audited publication-day
  commit, including a custom OpenAI-compatible entry, while the article lists
  fifteen. Live model fetching and an in-memory catalog do exist. Config values
  are validated and retained only inside the newly created in-memory provider;
  no provider-config persistence or encryption exists in the chat panel. The
  general OpenAI validation treats connection failure and every HTTP status
  except 401/403 as valid, so it is not “verify before storing” in the strong
  sense presented.
- Model descriptors carry `supports_tools`, but the main request enables tools
  whenever the registry is non-empty and sends them all. No inspected code
  implements the claimed structured-text tool fallback. Tool JSON crosses the
  provider API, is parsed into `serde_json::Value`, and crosses the plugin
  bridge in JSON form, so “no serialization” is not true of the agent path even
  when the final native dispatch is an in-process function call.
- Native plugins have the process's memory, GPU, filesystem and network
  authority. A SHA-256 manifest proves that a binary matches a manifest; it
  does not authenticate an author, grant capabilities or isolate faults, and
  the loader supports an allow-unlisted switch. Treating marketplace code as
  “the same risk as native” merely names full code execution. Trusted built-ins
  may be in-process; community code needs a real isolation and permission
  design. Website Sands remain in the browser and never graduate to native trust for
  speed.
- Toolbelt has broad unit coverage for registry and macro mechanics, but the
  Pulsar integration tests found here do not exercise duplicate-provider
  resolution, file-scope exclusion, symlink escapes, concurrent chats, global
  context races, transaction rollback, cancellation during mutation,
  permissions, prompt injection, secret persistence or the claimed lazy
  discovery loop. There is no published latency harness supporting the
  microsecond-versus-hundreds-of-milliseconds comparison.

**Ideas to test.** Build a small real command bus before exposing it to a model.
Drive the same Sand/Area mutation through the visible edit UI and a deterministic
test principal, then require identical validation, transaction, persisted
operation, renderer revision and undo result. Attempt stale revisions,
duplicate idempotency keys, unknown versions, invalid nested values, missing
targets, mid-batch failures and cancellation before, during and after commit.

Exercise a principal matrix covering local human, local agent, collaborator,
built-in Sand, Website Sand and read-only Facade. Feed instruction-shaped text
through Records, Protein, Website content and tool results; it must never
change the matrix. Test rooted filesystem handles with `..`, absolute paths,
symlinks, renamed ancestors and non-existent write targets. Verify secret
redaction in logs, crash reports, Box state, exports and collaboration.

Run simultaneous read-only queries and conflicting mutations from two chats
and two peers. The former may execute concurrently against an identified
revision; the latter must serialize, reject stale input or use an explicitly
defined commutative operation. Undo each accepted transaction independently.
Crash or terminate an isolated external provider during a call and keep Box,
rendering and the previous committed state usable.

Only after those tests pass, benchmark in-process and isolated adapters with
the same no-op, small state edit, bulk Sand placement and remote model call.
Measure dispatch, validation, commit-to-visible latency and p95/p99 frame
pacing separately. If validation and a local process hop are invisible beside
model latency, preserve the stronger boundary. Optimize schema paging and
batching based on observed prompt size and interaction latency rather than the
article's uncited numbers.

**Verdict:** adopt one typed domain command bus shared by human UI, agents,
Sand Behaviors, collaboration and persistence, with demand-paged discovery and
immediate visible results. Keep native in-process dispatch as an optional
adapter for trusted built-ins, not as the authority model. Do not copy Pulsar's
short-name registry, global context/provider state, parallel mutation threads,
lexical filesystem check, ambient native plugin trust or claimed universal
reflection setter. For v1 the requirement is a human-visible, permissioned,
revisioned and undoable command path; lazy schemas and microsecond dispatch are
optimizations after that path is correct.

### 21. Make Sand Behavior one typed, verified graph; native lowering is an optimization

Read 2026-08-30. [The article](https://pulsarnative.com/blog/2026-07-14-pulsar-blueprint-executor)
was checked against publication-day Pulsar commit
[`4198dae`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/4198dae5cb1686677b5ec9e1491d88f89dda6266),
its pinned PBGC compiler/runtime
[`89f42d5`](https://github.com/Far-Beyond-Pulsar/PBGC/tree/89f42d511ea480f71d5102532ba2abe8e965da0d)
and Blueprint editor
[`f9edb82`](https://github.com/Far-Beyond-Pulsar/Plugin_Blueprints/tree/f9edb82ab641dcd38f28f527eed3cb88b4187124),
then against current Pulsar commit
[`0f4ee79`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/0f4ee7961addc0084ebec427542c26773c7ea0ac)
and its newer PBGC
[`8739ed4`](https://github.com/Far-Beyond-Pulsar/PBGC/tree/8739ed4b9d4acc362887a3d6414d3ff96c880f4f)
and editor
[`fbb0bdb`](https://github.com/Far-Beyond-Pulsar/Plugin_Blueprints/tree/fbb0bdb97a283acd89933ab1747ac5ec38a7bc0f).
The reusable typed-graph idea fits Lince very well. The published byte-arena
executor does not: it treats compiler output as trusted native memory layout,
while Lince needs an authored, collaborative and capability-bearing behavior
format that remains safe even when a file, Website Sand or peer is hostile.

**What it proposes.** A Rust `#[blueprint]` macro turns ordinary functions into
visual nodes. It emits metadata for pins, types, source, documentation and
execution routes; an `unsafe extern "C"` shim that reads and writes a byte
arena; and link-time registration. Four node classes distinguish pure data
operations, ordered side effects, branching control flow and event entry
points. More than two hundred standard-library nodes are advertised across
math, logic, flow, collections, filesystem, network, process and concurrency
categories.

One authored graph is intended to have two execution paths. During development
it becomes JSON bytecode, is patched with native function pointers from an
embedded `pulsar_std` library and runs from a persistent per-instance arena for
fast graph reload. For shipping it is transpiled to Rust so ordinary compiler
optimization can inline the graph. The post also describes component property
and method nodes derived from reflection, per-frame lifecycle events, stateful
variables, breakpoints and native hot swapping. It openly says that scene
access, entity references and the component bridge were unfinished at
publication.

**What helps Lince.** Sand and Castle composition needs an authored Behavior
model independent of whether the same composition was made visually in Box or
declared with Rust helpers. A button Sand, a Record card and a larger video-call
Castle can all expose typed ports and reuse the same Behavior pieces. Grouping
does not create a privileged Castle runtime; it composes Sand definitions,
ports, state and Behaviors under another stable definition.

The four-way distinction is a useful starting vocabulary, adjusted to Lince's
domain:

- pure nodes transform explicit inputs such as Protein fields, constants and
  immutable event data without observing ambient process state;
- query nodes read an identified Box/Protein snapshot and therefore carry that
  revision as an input rather than pretending to be timelessly pure;
- effect nodes request typed domain commands through the command bus defined in
  the preceding review, never mutate Box, disk, network or process state by
  ambient Rust access;
- control nodes route bounded execution, while events enter through named,
  typed contracts such as Sand activated, Record selected, Protein item
  spawned, Area entered or left, group settled and an explicitly configured
  timer.

The same graph can connect a Protein output to Sand presentation, map an
interaction to `record.selected`, update local Sand state, request an Action or
change an Area through an authorized command. Each node and port needs a stable
namespaced id, schema version, exact input/output types, effect declaration,
authority requirement and deterministic error contract. A wire connects port
identity, not a Rust offset or a string that happens to resemble a type.

Pure classification must be enforced rather than decorative. A value that
reads the clock, random source, environment, Box revision, Protein stream or
physics state is not a cacheable pure expression unless that changing value is
an explicit input. An effect node returns the accepted command/result and its
revision; it does not smuggle an immediate mutation into a function labeled as
pure. This makes reordering, common-subexpression elimination and replay valid
when they are eventually worth doing.

Behavior runs at the authority boundary, not inside rendering and not as an
unbounded callback in the physics solver. Input events are stamped and queued;
the simulation/state owner processes a bounded batch at a defined boundary;
accepted effects become semantic transactions; rendering consumes the later
revision. Continuous Sand motion and topology forces remain the spatial
runtime's responsibility. A Behavior may configure a force, create an Area or
react to an Area event, but it does not execute arbitrary graph code once per
particle pair.

The authored graph is durable truth. It contains stable node/port ids,
configuration, declared local state, event bindings and provenance. A verified
execution plan, native library, shader, bytecode or cache is a disposable
projection identified by the graph hash, node-registry version and compiler
version. Collaboration synchronizes semantic graph edits and Behavior state
operations, never raw function pointers, byte-arena contents or generated Rust.
Unknown node or schema versions fail closed.

Persistent state and scratch values must be separate. Every Sand or Castle
instance owns state according to its Behavior-state schema; temporary event
values live only for that invocation. Reload compiles and validates a candidate
off to the side, computes an explicit state migration, swaps it at a safe
boundary and retains the last known-good plan if any stage fails. Copying a
matching Rust offset is not a migration rule. A renamed, removed or changed
field needs a declared reset/conversion and a visible diagnostic.

Website Sands stay on the browser side of the broker. They may emit and subscribe
to granted typed events and commands, but cannot register a native function,
provide a pointer shim or extend the in-process node library. Trusted built-in
Rust can register node descriptors through a safe adapter to the same Behavior
contract. Native location changes cost, not authority.

The human surface is part of this feature. Box edit mode should show the
Protein-to-Sand mapping arrows already planned, then allow a Sand or group to
open its Behavior graph with typed pins, event entry points and reusable
subgraphs. Compile errors must point to the responsible node and wire. A run
surface needs current plan revision, active event, bounded trace, local state,
requested effects, denied authority and pause/stop controls. Empty states must
say whether there is no Behavior, it has not compiled, it is disabled, or it
has no authority. A user should be able to build and try a button-to-Record
interaction without reading Rust.

**Needed for it to work.** Define the Behavior document and node-registry
schemas before an executor ABI. Validate ids, versions, port cardinality,
connections, types, required inputs, event signatures, effect declarations,
state schema, control-flow targets and resource limits as one operation. Reject
integer overflow, out-of-range indices, duplicate labels, unreachable required
work, illegal cycles and a graph whose worst allowed execution cannot be
bounded.

Use a memory-safe verified interpreter or execution plan first. Values can be
an owned typed enum plus registered domain handles, or another representation
whose clone, move and drop semantics are explicit and testable. They cannot be
arbitrary bytes asserted to have a Rust type. Built-in functions receive typed
values and a narrow execution context; effectful functions can only enqueue
declared commands. The runtime must have event-step, recursion, memory, output,
timer and external-call budgets, plus cancellation and deadline semantics. A
bad Behavior fails its invocation without hanging or corrupting Lince.

Events need one canonical name and complete parameter schema. Per-frame work
must receive the actual time value if promised; input and Area events need the
right actor, target, revision and provenance. Ordering between multiple
Behaviors, reentrant events and effects produced during an event must be
defined. Deterministic inputs should produce the same state and command stream
on every peer; inherently external results become recorded inputs rather than
secret ambient observations.

Capability analysis is part of plan verification. The compiler derives the
minimal read/write/command set from reachable effect nodes and compares it with
the Sand instance's grants. Filesystem, network, process, Protein, Box mutation
and Facade publication are distinct capabilities. The handler rechecks at
execution and commit. A hand-maintained function-name whitelist and a trusted
dynamic-library hash do not express this authority.

Hot reload must be transactional and instance-aware. Candidate preparation
cannot remove the running plan first. Existing invocations either finish on
their old immutable plan or are cancelled under an explicit rule; new events
start only after the swap. Every affected instance is migrated atomically or
the whole change stays unapplied. Compilation, validation, migration and swap
diagnostics must be visible in the same Sand editor surface.

If two lowerings eventually exist, they need one normative semantics and a
differential oracle. The interpreter and optimized path must produce identical
event results, state transitions, command requests, errors and ordering for a
large generated corpus. “Equivalent Rust” is not sufficient when one path has
per-instance state and another has thread-local state, or when one copies bytes
and another clones values.

**Needed only to make it fast.** Compact opcodes, pre-sized value storage,
specialized primitive operations, cached verified plans, pure-subgraph folding,
direct built-in dispatch, parallel evaluation of proven independent pure
regions and ahead-of-time Rust/Wasm lowering are optimizations. So is avoiding
JSON inside a trusted local adapter. None should precede a correct ownership,
authority and reload model.

Behavior execution may not be the hot part of a thousand-Sand scene. Physics,
visibility, text and GPU submission have their own measured budgets. First
measure events per tick, nodes per event, state size, command latency and p95/p99
frame impact on the Wayland target. Add an optimized lowering only when a real
Behavior workload exceeds its budget. An unsafe arena is not justified by an
article's general statement that game logic is a small part of frame time.

**Source and security audit.** The inspected implementation establishes useful
mechanics, but contradicts several of the article's stronger guarantees:

- The publication tree contains 429 `#[blueprint]` annotations, so the claimed
  breadth is real in source count. Breadth is not completion: HTTP and shell
  modules shown in that source contain placeholder functions, while other
  registered nodes expose real environment, filesystem and process behavior.
- The macro computes lowercase node kinds (`pure`, `fn_`, `control_flow`,
  `event`) but tests execution inputs against capitalized `Pure` and `Event`.
  Consequently its own metadata gives every pure and event node an `exec`
  input, contrary to the article. The bug and absence of a matching metadata
  test remain in the current commit.
- `BpProgram` is ordinary deserializable JSON. The VM checks only that the
  supplied arena is at least `program.arena_size`; it never proves that an
  instruction's offset, byte range, alignment, type-slot location, output or
  condition lies within that arena. Every raw pointer operation instead says
  codegen guarantees it. An edited file can therefore drive out-of-bounds or
  misaligned access before any typed runtime error.
- `Jump` can loop forever and calls can block indefinitely. There is no fuel,
  instruction limit, deadline or cancellation check. Label collection also
  silently lets a duplicate id replace an earlier destination. Preparation
  resolves allowed function names but does not verify the program layout or
  control-flow graph.
- `LoadVar` and `StoreVar` perform byte copies. Dispatch shims use `ptr::read`
  for owned arguments and `ptr::write` for results. The arena never runs
  destructors for live values. Fan-out, repeated reads, overwrites and
  persistent `String`, `Vec` or another owning type can therefore duplicate a
  pointer-owning header, move the same value more than once, leak it or leave a
  dangling copy. One test explicitly says its arena copy becomes dangling and
  is safe only because that particular graph never reads it again; that is not
  an ownership model.
- Generic dispatch substitutes a bare `T` with `[u8; N]` selected only by
  size. The article says this has the same size and alignment as any `N`-byte
  type, but every byte array has alignment 1. It also permits semantic traits
  such as `PartialEq` and `Ord`, whose byte-array behavior is not the concrete
  type's behavior, especially with padding. Reinterpreting `Vec<T>` as
  `Vec<[u8; N]>` also changes allocation layout and drop semantics. Unsupported
  sizes panic from an `extern "C"` shim rather than yielding a graph error.
- The state arena's base alignment is fixed at 8 even though descriptors carry
  arbitrary alignments. Descriptor ranges, defaults and alignment are not
  validated when `CompiledBytecode` loads. Its format `version` is serialized
  but never checked; the later source changes the constructor from version 1
  to 2 without adding a rejecting loader.
- The advertised event contract and runtime differ. The standard node is
  `on_tick(delta_time)`, while the publication dispatcher requests event key
  `tick` and deliberately discards `delta_time`. Missing events are execution
  errors logged by the lifecycle loop rather than the empty no-op described in
  the walkthrough.
- Rust code generation exists, but the claimed dual execution runtime does
  not. `ExecutionMode::Native` is an enum value with a setter; neither the
  publication nor current dispatcher branches on it. Every registered instance
  owns a bytecode arena and every event takes the VM path. Generated variables
  are process/thread-local statics, so multiple instances on one thread would
  share them rather than mirror the VM's per-instance state.
- Publication hot reload removes the old class before preparing the new one
  and does not migrate existing arenas. A failed preparation loses the
  last-known-good plan; a successful layout change leaves instances with stale
  storage. Current source adds component/entity bindings and exact-layout arena
  rebuilding, which is real progress, but the executor still removes the old
  plan first and migration still copies raw bytes. It is not an atomic semantic
  migration.
- The executor's symbol whitelist admits wildcard families including
  `file_*`, `http_*`, `process_*`, `shell_*`, `get_*` and `set_*`. Publication
  `process_exit` and `process_abort` terminate the host and are admitted by
  `process_*`; they are not protected by the separate shell-execution flag.
  `get_env` is incorrectly declared pure and `set_env` mutates the host
  environment. A prefix whitelist can also grant a newly added function merely
  because its name happens to match. This is ambient host authority, not a
  sandbox or per-graph capability set.
- Hash verification proves only which `pulsar_std` bytes were read. The
  Blueprint executor hashes with `std::fs::read` and then separately calls
  `Library::new(path)`, leaving the substitution interval it says it prevents.
  `PermanentLibrary` exists in the plugin manager but is not used by this
  executor. The graph JSON is separate and not bound by the library digest, so
  the chain cannot prove that native code “matches the graph the user built.”
- `linkme` gathers entries from the dependency graph into a section of the
  final linked binary, as its
  [own documentation](https://docs.rs/linkme/latest/linkme/) says. It does not
  make a separately loaded plugin DLL's section appear automatically in the
  host's slice. Pulsar's own cdylib configuration disables that registry and
  returns an empty node slice, then resolves explicitly named symbols instead.
  The article conflates static/link-time collection with runtime plugin
  discovery.
- Executor tests cover arithmetic, branch behavior, serde round trips, timing
  loops and several generic examples. They do not feed malicious bytecode,
  validate every offset/alignment, exercise execution budgets, fan out owning
  values, compare multiple instances across both lowerings, prove transactional
  reload, or test the dangerous-node authority boundary. The editor's
  breakpoint UI says it is driven by a fake simulator with fabricated pin
  values, “or eventually the real VM”; the article presents that future
  debugger as current behavior.
- Later current source does implement much of the admitted component gap:
  component-operation trampolines, SceneDB entity binding, cross-object lookup,
  broader value marshalling and instance rebuilding appear after publication.
  PBGC's VM file is nevertheless byte-for-byte identical between the audited
  publication and current pins. Feature reach grew; the unsafe trust boundary,
  ownership defects and missing budgets did not shrink.

**Ideas to test.** Build the safe interpreter and validator together. Fuzz the
serialized graph and verified-plan decoder with overflowing offsets, huge
counts, wrong port types, duplicate ids/labels, illegal cycles, missing
targets, unknown node versions and adversarial nested values. Every rejection
must be a bounded diagnostic; no input may reach unchecked pointer arithmetic.

Create a semantic corpus using primitives, strings, vectors, a 16-byte-aligned
type, a custom clone/drop counter, fan-out, repeated state reads, overwrites,
branch loops and multiple instances of one definition. Verify exact values and
drop counts, instance isolation, deterministic event ordering and step/memory
limits. Generate valid random graphs and compare interpreter results with any
future native/Wasm lowering, including errors and the ordered domain-command
stream.

Exercise the authority matrix for a local human, built-in Sand, Website Sand,
agent, collaborator and read-only Facade. Put file, network, process and Box
effects behind distinct nodes; compile and run every allowed/denied
combination. Renaming a node to an allowed prefix must never grant authority.
Instruction-shaped Protein or Website data must remain data.

Hot-reload a running group across added, removed, renamed and changed state
fields while events are queued and an invocation is active. Inject compiler,
validation and migration failures at every stage. The visible Sand must keep
running the previous revision until one atomic swap succeeds, and peers must
converge on the same authored graph and migration result without syncing an
executable artifact.

For performance, benchmark a representative quiet board, bursty Protein spawn,
button/event-heavy Castle and continuously active Area automation at 1,000
active and 10,000 resident Sands. Record graph validations, events and nodes
per tick, allocations, command-commit latency, CPU time and p95/p99 frame
pacing separately from physics and rendering. Compare the safe baseline with
one optimization at a time; do not infer a native-code requirement from node
count alone.

**Verdict:** adopt a reusable typed Behavior graph shared by visual Box editing
and Rust-defined built-ins, with explicit pure/query/effect/event semantics and
the same permissioned command bus used elsewhere in Lince. Persist and
collaborate on the authored graph; execute a fully validated, memory-safe,
budgeted plan; make compilation and traces human-visible. Do not adopt Pulsar's
raw byte arena, size-only generic ABI, wildcard host-function whitelist,
thread-local generated state, hash-as-sandbox claim or nominal native mode.
Keep AOT Rust/Wasm as a later measured lowering required to prove exact
equivalence to the safe interpreter. The article contributes the composition
model and a valuable map of implementation hazards, not Lince's executor.

### 22. Compile Behavior through one fail-closed typed IR, not two loosely related generators

Read 2026-08-30. [The article](https://pulsarnative.com/blog/2026-07-16-pulsar-blueprint-compiler)
was checked against publication-day Pulsar commit
[`91fea16`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/91fea1682471579ebdcde144961d60d4009bc11b),
its Graphy
[`13f874b`](https://github.com/Far-Beyond-Pulsar/Graphy/tree/13f874bd1cdb4c0f3dea7de74f39f39d756f8d50),
PBGC
[`89f42d5`](https://github.com/Far-Beyond-Pulsar/PBGC/tree/89f42d511ea480f71d5102532ba2abe8e965da0d)
and Blueprint editor
[`f9edb82`](https://github.com/Far-Beyond-Pulsar/Plugin_Blueprints/tree/f9edb82ab641dcd38f28f527eed3cb88b4187124),
then against the current pins linked in the preceding review. The phase-oriented
compiler is the right shape for Lince. The inspected compiler does not implement
the validation assembly line claimed by the prose, and its two outputs do not
share a sufficiently precise intermediate semantics to validate one another.

**What it proposes.** A visual graph is stripped of editor-only state and sent
through subgraph expansion, type checking and coercion, pure-data dependency
analysis, execution routing and either bytecode or Rust generation. Graphy
stores nodes by id and connections as a flat list. Persistent type strings are
parsed into a structural type algebra; runtime reflection metadata supplies
layout when code generation needs it.

Pure nodes are topologically ordered with Kahn's algorithm. Execution wires
become a routing table. Reusable macro graphs are recursively inlined with
prefixed ids and recursion detection. Bytecode gets fixed arena slots; Rust
generation emits expressions and uses `syn` to replace `exec_output!()` markers
inside trusted control-flow templates. Structured diagnostics are presented as
continuing across phases and pointing back to graph, node and pin. The article
calls the two generated paths a mutual correctness check.

**What helps Lince.** Behavior should have one compiler front end regardless of
whether the author used Box, a Rust definition helper or an imported trusted
package. The normalized result—not the visual layout and not generated Rust—is
the semantic boundary. A suitable pipeline is:

1. resolve every Behavior, node, port, state and event schema by stable id and
   exact version;
2. expand reusable Sand/Castle subgraphs hygienically while retaining a source
   map back through every instance;
3. validate structure, port direction and cardinality, required values,
   authority, state ownership and event signatures;
4. solve types and materialize every conversion as an explicit typed operation;
5. classify pure snapshot queries and effects, then form a deterministic
   control/data-flow IR;
6. prove budgets, legal cycles, state layout/migration and command effects; and
7. emit a verified interpreter plan plus diagnostics and an artifact identity.

The source map must survive expansion and lowering. An error inside a reusable
Record-card Behavior should identify both the reusable definition and the
particular Sand/group instance through which it was reached. Generated ids use
opaque stable identities, not sanitized display names. Presentation positions,
selection and animation stay in the authoring document but do not influence the
compiled result or its hash.

Lince should not treat Rust spelling as its durable type identity. A port type
has a canonical schema id and version with explicit shape, constraints and
domain meaning. Two records with the same fields may still be distinct types;
two quantities with the same number representation may have different units.
Wrappers and collections retain their element schemas. An unknown type is a
compile error, never an automatic wildcard.

Conversions should be visible nodes or compiler-inserted operations shown on
the wire. Even a lossless numeric widening changes runtime representation and
must be emitted in both lowerings. Domain conversions—quantity units, concept
resolution, Record projections—are never inferred from similarly shaped data.
Lossy conversions require an explicit node and policy, not a warning that still
quietly changes values.

Diagnostics may accumulate so a person can fix several problems together, but
an artifact with any error is not executable. The running Sand retains its last
known-good plan while the editor overlays all candidate errors. Warnings can
produce a candidate only when they do not change authority or semantics.
Diagnostics need stable codes, severity, source path, explanation and a direct
action in the Sand editor; they must not exist only in a compiler log.

The normalized IR should make effects and control explicit rather than splice
arbitrary source text as its meaning. Trusted Rust node authors can use macros
or helpers, but those helpers describe a control operation that lowers to the
same IR as a visually authored branch. Rust/Wasm emission later consumes that
IR. This gives the safe interpreter, optimizer and human trace one semantic
object instead of asking source-template substitution and bytecode walking to
coincidentally agree.

Compilation must be deterministic. Sort independent nodes, registry entries,
events, diagnostics and effect sets by stable identity. The same graph,
registry and compiler version must yield the same normalized IR and artifact
hash on each peer. Collaboration sends authored semantic edits; peers can
compile locally and compare hashes without synchronizing generated native code.

**Needed for it to work.** The structural validator must reject missing nodes
or ports, wrong source/target direction, execution-to-data edges, duplicate
connections where a port is singular, incompatible fan-in/fan-out, duplicate
stable ids, key/id disagreement, namespace collisions, unknown metadata,
unreachable required state and unsupported versions. Control-flow cycles need
an explicit bounded-loop construct; an arbitrary execution back-edge is not a
loop definition.

Type solving and conversion lowering are one transaction. A checker cannot
merely declare `i32` compatible with `i64` and let a code generator pass the
same bytes or expression unchanged. Each inserted conversion has a registered
implementation, authority/effect classification, failure behavior and source
annotation. Generic constraints are solved by type identity and traits the
runtime actually implements, not a wildcard that accepts anything.

Subgraph expansion needs hygienic identity allocation, declared interface
mapping, recursion/depth/node limits and deterministic traversal. It must not
overwrite a parent node whose id happens to equal a generated prefix. The
expanded IR retains definition/instance provenance, while debugging can still
enter the reusable abstraction rather than showing only a huge flattened
graph.

Pure analysis may reorder only operations proven pure against all ambient
state. Snapshot queries include their revision input. The compiler must reject
pure cycles and separately validate the effect/control graph, including event
reentrancy and loops. Multiple outgoing edges from a sequential pin need a
defined fork/order operation; vector insertion order is not a language rule.

Before execution, verify the normalized IR independently of the authoring
compiler. That verifier checks types, control targets, budgets, capabilities
and plan integrity without trusting the editor. A later native/Wasm backend is
accepted only after compiling successfully and carrying the exact source IR
identity and declared effects. The executable cache is disposable.

**Needed only to make it fast.** Incremental recompilation, cached registry and
type parsing, parallel scanning, pure-node folding, dead-node elimination,
compact source maps, subgraph specialization and parallel independent pure
regions can improve editing or execution. Ahead-of-time emission remains an
optional backend. First measure graph normalization and validation on realistic
Sand compositions; parallelizing a thousand-edge scan is not valuable if
rendering, physics or a network Action dominates the interaction.

**Source audit.** The publication source contains most named building blocks,
but not their advertised composition:

- Graphy has a `TypeChecker`, coercion table and structured diagnostic types.
  PBGC's public Rust and bytecode compile entry points never instantiate the
  checker. They load metadata, build `DataResolver`, build
  `ExecutionRouting`, then generate output. The publication editor likewise
  does not call it. A type-mismatched graph therefore is not stopped by the
  phase the article says runs first.
- PBGC returns `Result<_, GraphyError>`, not the described `CompileResult` with
  diagnostics accumulated across phases. The editor turns one error into a
  status/history string. The claimed inline pin diagnostics and continuation
  through later passes are not connected to this compile path.
- The type checker itself validates only existence of endpoint nodes for an
  execution edge; it does not verify those pins exist, their directions or
  that they are execution pins. For data edges, an unresolved pin or type is
  silently skipped, and wildcard matches everything. Those are inappropriate
  fail-open rules for an executable Behavior format.
- Publication coercions are an ordinary local vector, not an `inventory`
  plugin registry. The source contains integer-to-integer widening, `f32` to
  `f64` and `&str` to `String`, but not the integer-to-float rules the article
  lists. More importantly, the checker only says a coercion is allowed; neither
  generator inserts a conversion instruction or expression. Permitting a wire
  without lowering the representation cannot be correct.
- Current Graphy adds a registry for explicit conversion-node paths, while
  current PBGC gains component and multi-output lowering. Its public compiler
  still does not call `TypeChecker` or the conversion registry. The central
  validation gap remains after the additional machinery.
- PBGC's library-manager parameter is an unused `Option<()>` and Phase 0 is a
  commented TODO. The publication editor does call Graphy's flat expander
  before PBGC, so local editor macros can work, but headless/public compiler
  behavior differs from the article's standalone pipeline.
- The expander's prose says entry/exit nodes are excluded, but its code inserts
  every prefixed node, including entry and exit, then routes parent edges to
  them. It uses `HashMap::insert` without checking whether a prefixed id already
  exists, so a generated name can replace parent content. Expansion limits
  beyond recursive graph-id detection are absent.
- Data connection mapping inserts into one `(target node, target pin)` key; a
  later connection silently replaces an earlier source. The parallel variant
  collects the same duplicates without a language-level winner. Execution
  routing permits an arbitrary vector of targets per output and preserves file
  insertion order as behavior without validating whether the pin is a fork.
- Pure topological sorting correctly detects a pure-data cycle, but independent
  ready nodes originate from hash collections and are not stably ordered.
  Event nodes and other generated collections also originate from hash-map
  iteration. Output and hashes can vary even where semantics should not.
- `DataResolver` claims unique result names but directly sanitizes each node id.
  Distinct ids such as `a-b` and `a_b` produce the same Rust identifier. A
  separate collision-aware `VariableNameGenerator` exists but this compile
  path does not use it.
- The timing test cited as dual-path equivalence only measures how long each
  generator takes and prints a ratio. It never compiles or executes the
  generated Rust, and never compares its state/effects with VM execution. The
  claim that the test suite establishes equivalent behavior is unsupported.
- The fixed byte-layout, ownership and native-state defects recorded in review
  21 flow directly from this compiler. Up-front layout calculation cannot catch
  conflicts the compiler never validates, and a source-template backend cannot
  serve as an oracle while its state and event contracts differ from the VM.

**Ideas to test.** Create a conformance corpus with missing pins, reversed
edges, duplicate writers, wrong edge kinds, unknown nodes/types, wildcard
attempts, numeric and domain conversions, sanitized-id collisions, subgraph id
collisions, recursive/deep expansion, pure and execution cycles, multiple
events and disconnected effects. Assert exact diagnostics, stable source paths
and no executable candidate on any error.

Compile identical authored graphs after randomizing map insertion, connection
order and registry discovery order. Normalized IR, diagnostics, capability set
and artifact hash must remain identical. Property-test hygienic expansion by
nesting reused definitions and checking every expanded identity and source-map
round trip.

Build a small reference interpreter for the normalized IR and generate random
well-typed bounded graphs. Every future optimized backend must match its output,
state transition, ordered command stream, failure and budget exhaustion. Include
multiple Sand instances, owning values and event reentrancy so superficial
expression equivalence cannot pass.

Measure full edit-to-visible latency separately: graph normalization, type
solving, expansion, verification, plan swap and first resulting frame. Test the
normal button/card/Castle graphs before synthetic 1,000- and 10,000-node graphs.
Only parallelize or incrementally cache the phase that a profile shows matters.

**Verdict:** keep the article's staged compiler, structural diagnostics,
source-mapped reusable graphs and one authored graph with optional backends.
Make one deterministic, capability-aware normalized Behavior IR the semantic
truth, and execute only after a separate fail-closed verifier accepts it. Do
not copy Graphy's permissive unknown/wildcard behavior, implicit coercions
without emitted conversions, prefix-based expansion, hash-order output or
PBGC's unvalidated direct path. Dual backends are not mutual proof; a safe
reference semantics plus differential tests are.

### 23. Tier customization by cost, but compile only bounded material contracts

Read 2026-08-30. [The article](https://pulsarnative.com/blog/2026-07-18-helio-radiant)
was checked against publication-day Pulsar commit
[`db396ee`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/db396ee0c804c48e6cf7cdc43fe5c4326049627f)
and its Helio
[`8f73203`](https://github.com/Far-Beyond-Pulsar/Helio/tree/8f73203a9976bfb7bb3af5a8e9f43e3e5f23c7f7),
then against current Helio
[`6fb248c`](https://github.com/Far-Beyond-Pulsar/Helio/tree/6fb248c921fdce8adbc01d0eef8c18a9a98f2308).
The useful idea is to make common appearance changes cheap while retaining a
more expressive compiled tier. The inspected implementation is not a safe
material language or a content-correct cache, and “zero-cost” is not an
accurate contract.

**What it proposes.** Radiant divides materials into three tiers. Ordinary
properties and finite features use one deferred-PBR shader with runtime flags.
Reusable material classes use WGSL templates. A visual graph can emit a WGSL
snippet that replaces a marked region in a template. A completely custom WGSL
shader is the final escape hatch. `GpuMaterial` adds a class and class-specific
parameters, and shader modules and pipelines are cached by template id, graph
hash and feature flags.

Visible work is described as being sorted by material class and graph hash,
forming contiguous draw ranges with one pipeline selection per range. The
article presents branches on a material flag as effectively uniform within a
draw and therefore negligible, while classes avoid forcing every possible
feature into one increasingly large shader. It also presents live template
replacement, lazy first-use compilation and future persistent shader caching.

**What helps Lince.** The tier distinction maps well to the intended
customization system, provided it is a product contract rather than permission
to inject source text:

1. almost all Sand customization should remain tokens, geometry parameters,
   atlas selections and finite instance flags in existing pipelines;
2. a typed material/effect graph may select bounded operations and produce an
   engine-owned material IR for Box terrain, Area visualization, native Sands
   and world objects;
3. trusted Lince development may add a new render contract, template or pass
   when the existing domains cannot express an effect; and
4. arbitrary WGSL is developer tooling, never an authority granted to a
   downloaded Castle, Website Sand, Record or collaborator.

Each render domain needs an explicit output contract. A native UI Sand may
produce coverage, color and optional picking data. Box topology may produce a
surface material plus a separately defined displacement visualization. An
opaque world surface targets the deferred geometry contract, while transparent
objects, sprites, video textures, pinned HUD content and embedded browser surfaces have
different ordering and blending rules. A graph valid for one contract is not
silently accepted by another merely because both eventually emit pixels.

This is particularly important for topology. Area color, pattern distortion,
contours, shadows and a visible valley or mountain can be material/effect
inputs. The authoritative topology and force functions remain typed simulation
data. A shader may visualize them but must not become the hidden source of the
physics. The same state can then be shown from the 2D overhead view and the 3D
surface view without allowing a visual customization to change which Sands
move.

Material identity should be derived by Lince from canonical, validated input:
render-contract version, normalized graph IR, template/pass revision, finite
features, resource layout, target formats, sample count and relevant device
capabilities. Use a collision-resistant digest rather than a caller-supplied
`u64`. A display id can remain small, but it refers to the full artifact
identity. Changing a template, graph or contract creates a new revision and
cannot accidentally retrieve a pipeline compiled from older source.

Compilation should be transactional. The editor validates and previews a
candidate, captures structured WGSL/pipeline diagnostics behind an error scope,
and swaps the artifact only after it is ready. On failure, the running scene
keeps the last known-good material and visibly marks the candidate as broken.
Missing assets and unsupported device features produce a human-readable Sand
status; they neither panic the renderer nor silently look like the default
material.

Draw grouping is a derived render plan, not persisted scene meaning. Opaque
world objects may be regrouped aggressively. UI and transparent domains retain
their semantic z/depth order, and Website/video surfaces retain lifecycle and
compositing constraints. Sorting for fewer pipeline changes must never reorder
Box hit testing, accessibility order, Sand grouping or Behavior delivery.

**Needed for it to work.** The material graph needs a typed node registry,
fixed inputs and outputs per render contract, exact type and unit checking,
bounded texture/sampler/storage access, loop and instruction budgets, and a
verifier independent of the editor. Generated WGSL is an internal artifact.
The verifier rejects unknown nodes, unavailable capabilities, out-of-contract
bindings and dynamic resource access it cannot bound.

The cache key needs every input that can change module or pipeline behavior,
and registries need revision-aware invalidation. Cache entries need ownership,
memory budgets, eviction and device-loss rebuilding. Registration must reject a
digest/content mismatch rather than overwrite an existing identity. Removing a
graph must either retain an immutable referenced artifact until unused or
invalidate every dependent pipeline deterministically.

External HTML remains behind browser texture/event boundary. It can request a
declared visual effect through the same capability-checked Box protocol, but it
cannot supply WGSL, choose bindings, read renderer resources or share native
pipeline authority. Trusted imported material packages carry provenance,
license, contract version and declared limits and are validated like locally
authored graphs.

The renderer also needs an honest fallback for each domain, a preflight path
against the selected adapter's features and limits, and clear behavior when a
material exceeds them. Runtime editing must not block the render thread on an
unbounded compile. Queue compilation and pipeline preparation, show pending
state, and publish the result at a frame boundary.

**Needed only to make it fast.** Runtime flags, instance packing, draw-range
grouping, module/pipeline caches, compile prewarming, asynchronous preparation,
disk caches, graph specialization and shader dead-code elimination are
optimizations. Whether a flag is coherent across a GPU subgroup is empirical:
pixels from different primitives and materials can share execution, so sorting
draws does not prove branch uniformity on screen. Measure real Lince scenes.

Most native Sands should share a small number of pipelines and put variation
in instance buffers. Thousands of independently compiled Sand shaders would
trade one problem for pipeline churn. Profile CPU preparation, shader and
pipeline creation, bind changes, GPU duration and cache memory separately on
the supported Wayland adapters. Optimize the topology/world material domains
only after the basic UI path remains sharp.

**Source audit.** The publication code contains the advertised names but not
the safety, cache coherence or measurements implied by the article:

- Template composition is string replacement between two marker comments.
  Missing or malformed markers return the source unchanged. Graph snippets are
  unparsed WGSL strings, so their only effective validation is whatever error
  later emerges from shader-module or pipeline creation.
- The graph registry accepts a caller-supplied `u64` and a string, checks
  neither content identity nor collision, and replaces an existing entry with
  the same key. The shader cache is keyed by that number, so changed source can
  continue using the old compiled module. Unregistering a graph does not evict
  dependent cache entries.
- A template can likewise be replaced under the same numeric class while the
  cache key contains no template content or revision. This contradicts the
  claim that templates can be updated at any time: an already cached key need
  not observe the new source.
- Publication rendering does not sort the post-cull visible set each frame as
  described. Persistent scene rebuilding sorts the CPU object's dense set by
  `(material_class, graph_hash)` when object state is rebuilt, before GPU
  culling. The distinction changes both the claimed cost basis and ordering
  semantics.
- The render path creates the cache key with `feature_flags: 0`; material
  feature flags are runtime buffer values rather than compiled variants.
  Including flags in the key therefore does not implement a second variant
  dimension in this path.
- A missing graph snippet becomes an empty string and therefore the unmodified
  template. The publication source logs and falls back to class zero for most
  missing templates rather than always panicking as the article says. Silent
  visual fallback and host panic are both inferior to a visible, last-good
  error state.
- Runtime template strings are converted to `&'static str` with `Box::leak`.
  Current source introduces a shared registry because prior deep clones leaked
  these strings repeatedly; `RadiantTemplate::clone` itself still leaks. The
  current partial-template composer counts braces in raw text rather than
  parsing WGSL, so braces in lexical constructs can defeat replacement.
- Current source adds built-in classes, transparent composition and baseline
  WebGPU binding adaptation, but retains the caller-provided graph hash and
  unchanged `(template_id, graph_hash, feature_flags)` shader key. The core
  revision/invalidation problem remains.
- The cited microsecond sort, millisecond pipeline, bind-cost, PSO-memory and
  shader-count figures have no raw capture or reproducible benchmark harness
  in the inspected source. G-buffer tests mostly assert constants and default
  layouts; they do not compile every variant, render comparison images, test
  invalid source, collide hashes, replace live templates or prove frame-time
  behavior.
- Saving WGSL source to disk is not by itself a compiled shader cache—the
  source already exists and still needs translation and pipeline creation.
  Likewise, backend driver deduplication of separately created equivalent
  modules is not a portable guarantee.
- The added material fields, runtime branches, sorting, extra ranges, pipeline
  binds, compilation and cache memory are real costs. They may be good costs,
  but the phrase “zero-cost” hides the measurements Lince would need.

**Ideas to test.** Build a conformance suite for every material node and render
contract. Include missing markers, malformed generated code, unavailable
features, excessive resources, graph cycles, digest collision attempts,
template and graph replacement, deletion while referenced, device loss and a
compile failure after a known-good version. Assert exact human diagnostics,
artifact identity and uninterrupted last-good frames.

Randomize graph serialization and registry insertion order and require the
same canonical IR, digest and WGSL. Change each cache-relevant input one at a
time and require a distinct artifact; change presentation-only metadata and
require reuse. Exercise cache budgets and confirm retired materials release
all CPU and GPU resources.

Render reference scenes for native UI, opaque topology/world geometry,
transparent objects, sprites and Website textures. Verify pixels, picking,
z-order and accessibility/Behavior order before and after batching. Benchmark
common token changes separately from graph recompilation, and compare one
shared pipeline, finite flag variants and class-specialized pipelines on the
actual Lince workload.

**Verdict:** keep the three levels as an authoring and cost model, but collapse
their trust boundary: people configure finite data or a typed, bounded graph;
only trusted engine code defines raw render contracts and WGSL. Derive immutable
artifact identities from validated content, compile transactionally, retain a
visible last-known-good result and group draws only where domain ordering
permits. Do not adopt source-marker injection, caller-owned hashes, mutable
unrevisioned registries, silent fallback or unverified performance claims.

### 24. Select retained property-editor Sands by durable schema, not widget special cases

Read 2026-08-30. [The article](https://pulsarnative.com/blog/2026-07-23-type-agnostic-reflection)
was checked against publication-day Pulsar commit
[`40cd8f1`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/40cd8f1be262c80a4544219ac3703bbc62989bb8)
and Pulsar Reflection
[`9b887f1`](https://github.com/Far-Beyond-Pulsar/Pulsar-Reflection/tree/9b887f1ed327b5e3e2b6ba9066679469520cb446),
then against current Reflection
[`745ee78`](https://github.com/Far-Beyond-Pulsar/Pulsar-Reflection/tree/745ee787cc63288463c170aafb778672db8e85ac).
Separating a generic inspector from retained, type-specific editors is directly
useful for Lince. Rust `TypeId` and `dyn Any` are suitable implementation tools
inside one process, not the schema or collaboration contract.

**What it proposes.** Pulsar's old property editor accepted both JSON and a
typed value, returned a temporary GPUI element, and grew type-specific callback
slots. Stateful widgets required a separate registration and initialization
path. The refactor removes the dead JSON input, makes every editor a retained
GPUI entity, and returns a `BoundPropertyEditor`: an erased view handle plus a
closure that pushes later values into the concrete editor.

A generic panel caches one bound editor per class/property key. It finds a
factory by Rust `TypeId`, constructs the editor once, supplies subsequent
values as `&dyn Any`, and returns its view. Each editor owns its child widgets,
subscriptions, focus and write callback. A derive macro registers a generic
dropdown factory for reflected enums, and a missing `f64` primitive/editor was
added when an import schema exposed the registry gap.

**What helps Lince.** Box needs the same separation at a richer level. The
property-panel Castle should know how to enumerate an editable schema, address
a value, place rows, show validation and submit an edit transaction. It should
not branch on booleans, quantities, colors, references or topology profiles.
An editor registry chooses a retained Sand or Castle factory from a durable
schema id, constraints, editor role and capabilities.

The editor instance owns its local interaction state: focus, caret, open
dropdown, drag origin, provisional text, color-picker state and subscriptions.
It receives authoritative snapshots and emits typed edit intents. Reusing the
same editor across frames prevents a render from resetting interaction, and
the same editor Sand can be composed inside a record inspector, Sand builder,
Area editor, topology brush, material graph, Castle configuration or a compact
inline control.

This is a concrete example of Sands replacing a separate LynxUI vocabulary. A
quantity editor may itself be a Castle of number field, unit selector, slider
and validation message. A Record-reference editor may combine a search field,
selected-record chip and “Why is it here?” affordance. The inspector treats
both as one property-editor Sand. The same primitives can be used directly in
another Castle without requiring a privileged widget framework.

Selection must use persistent Lince identities rather than Rust spelling. A
property address includes the object/entity id, component or Sand role,
property schema id and schema revision. The schema describes value kind,
concept or unit identity, cardinality, constraints, nullability, access policy,
display hints and suitable editor roles. A local registry may index resolved
Rust handlers by `TypeId`, but that key never goes to Scene/Box storage, Sync,
Behavior, `.lingua` data or an external Sand.

Write-back is a typed command, not an arbitrary boxed replacement. It includes
the property address, base revision, candidate value, edit phase and author.
Text drafts and continuous slider/drag previews remain local until valid;
commits enter the same undo, collaboration, authority and persistence path as
other Box edits. The editor receives accepted, rejected or rebased outcomes so
it does not mistake its own optimistic display for authoritative state.

**Needed for it to work.** Registration must fail on duplicate schema/editor
claims unless an explicit, deterministic priority or override has been
declared. Factory selection must report missing editors, ambiguous matches and
unsupported constraints visibly. “No editor” should say whether a property is
read-only, unavailable in this build, malformed or simply lacks a suitable
editor; a bare `(nyi)` looks broken and cannot guide a person.

The cache key must include stable object/property identity, schema revision,
editor role and relevant mode. A selection change, property removal, type
change, permission change or registry reload retires the old instance and its
subscriptions. A type mismatch while updating a retained editor is an
invariant error with diagnostics, not a silent no-op. Lifetimes and cleanup
must be exercised because hidden and virtualized inspectors should retain only
the state the interaction contract requires.

Schema codecs must be total for every accepted value or return structured
errors. Generic enum editing applies only to the enum shapes the schema can
construct. Tagged/data-carrying variants need an editor for their payload, and
unknown or removed variants need an explicit migration/error representation.
Variant identity is a stable id rather than an array index, so reordering a
display list does not mutate stored meaning.

No unsafe cast may assert that an erased value is `Send`. The erased envelope
is created with the required bounds at the point where the concrete type is
known, or the command remains on its owning thread. External and data-defined
editors communicate through the serializable schema/value protocol and
capability bus; they never receive Rust `Any`, process pointers or direct Box
mutation authority.

Editor destruction must cancel subscriptions, popovers, pending validation and
previews. Feedback loops also need a rule: applying a remote or accepted value
updates display without emitting a new write, while a person-originated change
emits exactly one intent. Stale base revisions and simultaneous edits must be
surfaced rather than resolved by last render order.

**Needed only to make it fast.** Retaining editor instances, virtualizing long
inspectors, incremental schema resolution, keyed row reuse, coalescing preview
events, caching layouts and using native typed values rather than JSON in the
hot path can improve latency. First profile realistic inspectors, mixed
selection and continuous topology/material controls. A small form does not
need an elaborate cache; the retained model is primarily needed for correct
lifecycle and interaction.

**Source audit.** The implementation supports the central lifecycle claim, but
its erased boundaries are weaker than the article presents:

- `BoundPropertyEditor::new` downcasts an incoming value and silently returns
  on mismatch. The panel cache key is only `(class_name, prop_name)` and relies
  on every owner calling `clear` when context changes. A stale editor or key
  collision can therefore suppress updates without a diagnostic.
- Registry population inserts inventory entries into a `HashMap<TypeId, _>`.
  A duplicate `TypeId` replaces the earlier factory according to inventory
  iteration order rather than failing as an ambiguous registration.
- `TypeId` is appropriate for local dispatch but is not a stable type name,
  schema version or persisted/network identity. The article's extensibility
  conclusion does not by itself solve plugins compiled separately, stored
  scenes or collaboration.
- Factories are erased from their real signature to `fn()` and restored with
  `transmute`. Registration helpers constrain ordinary call sites, but the
  public hint structure itself can still be constructed incorrectly. A typed
  registry assembled where GPUI is available would avoid making this an unsafe
  invariant.
- Runtime deserialization returns `Box<dyn Any>`. The enum editor converts it
  to `Box<dyn Any + Send>` with `transmute`, justified by a prose assertion
  that unit enums are `Send`; the type system does not establish that bound at
  this conversion. Current source retains this unsafe cast.
- The derive emits the enum editor registration for every reflected enum, while
  serialization/deserialization reject variants with fields. Errors when
  reading the current variant fall back to index zero; errors constructing a
  selected variant are ignored. A data-carrying or codec-broken enum can thus
  display a plausible but false first choice.
- The generic enum editor does use JSON serialization and deserialization for
  every refresh and selection. That is compatible with removing JSON from the
  panel's public input, but contradicts the closing claim of “No JSON path” and
  makes codec failure part of ordinary UI behavior.
- The new `f64` registration has useful codec tests, but there is no automated
  completeness rule saying every schema kind used by an import/component has a
  value codec and editor. The panic that motivated the change is evidence that
  handwritten primitive coverage is not self-proving.
- The inspected tests cover many primitive codecs, but not retained editor
  identity, selection/cache invalidation, duplicate factory registration,
  downcast mismatch, enum dropdown interaction or the unsafe send boundary.

**Ideas to test.** Generate a registry conformance test from the schemas used
by Lince. For each editable kind, construct minimum, maximum, invalid, null and
round-trip values; require exactly one suitable editor or an intentional
read-only explanation. Exercise tagged enums, units, references, lists,
records, unknown variants and schema upgrades.

Switch one panel rapidly between objects whose visible property names match but
whose identities, types, permissions and schema revisions differ. Assert the
right retained state, subscription disposal, focus policy and a loud invariant
error for any wrong value envelope. Test remote edits, undo, rejected stale
writes, continuous local previews and removal while focused.

Use the same editor factory in several Castles and in a Box overlay, then
compare accessibility tree, keyboard navigation, theme tokens and edit command
semantics. Benchmark a normal inspector and a large multi-selection inspector
separately; record creation, update and destruction counts so caching does not
hide leaks.

**Verdict:** adopt the generic inspector plus retained, self-owning editor
entity pattern, expressed in Lince as composable Sands and Castles. Replace
process-local type identity at every durable boundary with versioned schemas,
and replace erased write callbacks with checked edit transactions. Do not copy
silent downcast failure, manual cache invalidation, duplicate last-writer wins,
index-identified enums, unsafe `Send` transmutation or an opaque missing-editor
placeholder.

### 25. Make the interface explain a slow or wrong frame from event to presentation

Read 2026-08-30. [The article](https://pulsarnative.com/blog/2026-07-26-ui-flamegraph-profiler)
was checked against publication-day Pulsar commit
[`7724916`](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/772491663469f7af963a2fe6590378d2627a1c5f),
WGPUI
[`76ec65d`](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/76ec65d7f774ba28221b928b579dea37c6d7f09c)
and WGPUI Component
[`14e99ad`](https://github.com/Far-Beyond-Pulsar/WGPUI-Component/tree/14e99adacbb004f6ebc880a86ba3014c3bdf163c),
then against the current WGPUI profiler sources. Lince should build equivalent
observability into its own runtime rather than adopt GPUI to obtain it. A
timeline and inspectable frame artifact are essential to judging whether the
prototype and v1 feel sharp.

**What it proposes.** Feature-gated instrumentation records nested CPU spans,
background spans, GPU timestamp-query spans, per-frame counters and memory
estimates. A frame carries CPU begin/end, CPU-observed submission, calibrated
GPU begin/end and CPU-observed readback completion on a nominally shared
timeline. Frames are retained in a bounded ring, and a binary format streams
length-prefixed frame records.

Separate one-shot captures record draw-call descriptions, fixed renderer
buffers, atlas and surface pixels, a depth-annotated element traversal,
resolved layout/style subsets and a copy of scene primitives. An in-process
replay reconstructs the element tree and reissues supported draw kinds into an
offscreen target. A profiler tab exposes frame selection, counters, memory,
tree and replay. Its dense flame-chart bars are a custom instanced wgpu surface
with CPU hit testing rather than hundreds of ordinary UI elements.

**What helps Lince.** Instrumentation belongs at the boundaries of the
architecture we control. One interaction should be traceable through Wayland
event receipt, hit testing, focus/gesture resolution, Behavior dispatch,
Protein query or update, scene-state commit, physics/topology step, visibility
and draw-plan construction, embedded browser texture acquisition, GPU upload/passes,
submission and presentation. Stable Sand, Castle, Area, Protein and scene
entity ids let a person answer both “which stage was slow?” and “why was this
Sand involved?”

The profiler itself should be a first-party diagnostic Castle available from
the running interface, not output that requires reading Rust or a log. It needs
an honest no-capture state, start/stop controls, capture scope, frame and event
selection, a cross-lane timeline, counters, resource budgets and links back to
the affected Sand/Area/Behavior. A dense timeline is a good use of a dedicated
instanced native primitive while controls and detail panes reuse normal Sands.

Use two capture levels. A bounded timeline stores cheap spans, counters and
artifact identities. An explicitly armed frame snapshot may copy selected
scene/draw resources for visual diagnosis. The second is expensive and
privacy-sensitive, especially because the embedded browser, video, maps and Record content can
appear in textures. It is local-only by default, says exactly what it will
capture, supports redaction/exclusion by Sand capability and never enters
Workspace Sync or a bug report without deliberate confirmation.

“Replay” should mean one of two precise things. A semantic replay stores input,
accepted Box transactions, deterministic simulation inputs and artifact
versions so Lince can reproduce why state changed. A render replay stores the
complete immutable render packet and the exact shader/pipeline/resource
contracts needed to reproduce pixels. A partial diagnostic viewer may still
be valuable, but it must label missing state and must not call reconstructed
draws a faithful replay.

The profiler also supplies the benchmark evidence the interface plan needs.
Record frame-time percentiles and worst frames, not only a mean and maximum.
Separate event-to-visible latency, active simulation time, scene extraction,
CPU render preparation, queue latency, GPU execution and presentation. Tag the
adapter, driver, resolution, scale factor, present mode, power state, scene
fixture and build identity so results can be compared honestly. We do not need
a 4K gate now, but the workload model and pixel-dependent passes must remain
visible so nothing assumes a fixed small output.

**Needed for it to work.** Every capture has an id, target window/view, start
and stop reason, monotonic sequence, build/schema versions, completeness flags
and explicit clock-domain metadata. CPU and GPU clocks cannot be declared the
same merely by pairing one CPU instant with one GPU timestamp. Calibration
needs bracketing observations, an uncertainty interval, periodic drift checks
where supported and a visible “uncorrelated” state where not. GPU durations
within one timestamp domain remain useful even without cross-clock placement.

Frame correlation is explicit rather than inferred from “most recently
opened.” Multiple windows, browser producers and overlapping background tasks need
their own frame/task ids. A span can cross several frames and is shown as such;
it is not assigned solely by its start time to whichever frame closes after it
finishes. Buffers have bounded event counts and report dropped/truncated data,
including long tasks that complete after the originating frame was evicted.

The frame artifact must identify pipelines and resources by immutable artifact
digest and revision, capture used byte ranges rather than entire reusable
allocations, and either zero unused storage or never export it. Record actual
bind/resource identities, dynamic offsets, render targets, viewport/scissor,
blend/depth state, push/uniform data, draw/dispatch parameters and pass order
for any path claiming render replay. Dynamic material and embedded browser surfaces need
domain-specific capture rules.

A trace reader treats the file as hostile: bounded lengths/counts, checked
arithmetic, versioned schemas, checksums and no process pointers or assumed
`'static` strings. Capture budgets cover CPU heap, staging buffers, texture
readback and viewer GPU memory. Device loss, unsupported timestamp features,
mapping failure and a dropped capture UI all terminate cleanly and visibly.

**Needed only to make it fast.** Compile-time instrumentation removal, one
relaxed atomic check while idle, per-thread rings, string interning, sampling,
GPU query rings, deferred readback, instanced timeline bars, viewport culling
and incremental frame diffs reduce overhead or viewer cost. Which should ship
in normal builds is a measured decision: compiling everything out makes a
field-only failure impossible to inspect, while always-on rich attribution may
alter the frame being studied. Maintain explicit diagnostic build/capture
levels and measure their idle and active perturbation.

**Source audit.** The source is unusually candid about several limitations and
has real CPU, GPU and replay tests. It nevertheless supports a narrower claim
than “a frame's entire GPU state”:

- Feature gating is real at the profiled call sites. With the feature present
  but idle, most recorders first read a relaxed atomic. No measurement in the
  article or inspected tests establishes the total idle cost across all call
  sites, so “for free” remains a design goal rather than evidence.
- GPU calibration records one CPU instant before submitting two adjacent GPU
  timestamps and equates that instant with the first GPU tick. CPU encoding,
  submission and queue delay lie between them. This cannot accurately derive
  queue backlog; it bakes an unknown offset into the supposedly shared
  timeline and later saturating subtraction can hide its sign.
- Three query generations are forcibly reset if readback lags. The displaced
  frame remains `gpu_spans_finalized: false` forever. That is a reasonable
  nonblocking policy only if the viewer clearly reports dropped timing rather
  than treating an empty lane as no GPU work.
- Background spans are drained when a foreground frame closes and selected by
  start time. A long task that finishes after its start frame closed has a
  start earlier than every later window and remains in the pending vector;
  overlapping windows can also claim work by close order rather than causal
  ownership. The pending vector is not bounded by the frame ring.
- Dropping `CaptureHandle` without calling `stop` intentionally leaves the
  process-wide capture running. The result is no longer reachable through the
  handle and subsequent starts fail, an unsafe lifecycle for a UI control or
  panic path.
- Deep capture records draw kind, labels, ranges, bind-group count and selected
  buffers/textures. It does not record complete pipeline descriptors, shader
  binaries/source identity, actual bind groups and dynamic offsets, all
  uniforms, viewport/scissor, attachment contents or every per-call resource.
  Replay rebuilds pipelines from the viewer's currently compiled production
  shaders. It is reconstruction, not freezing the original GPU state.
- Surfaces have pixels but no placement parameters and cannot replay. Backdrop
  filters lack the sampled content and use a placeholder. A global one-shot
  deep-capture request and the independent global UI-tree request can be
  consumed by different frames or windows, yet their results have no shared
  capture/frame identity proving they belong together.
- Fixed buffers are copied through their full allocation size, including
  unwritten tails from reusable buffers, rather than just live ranges. Besides
  potentially large readback, an exported snapshot could retain stale bytes
  from earlier content.
- The UI “scene snapshot” deliberately drops path vertices, filter-boundary
  markers and multiple primitive/style details. Styles join to nodes through a
  64-bit hash only; un-id'd nodes cannot receive styles and collisions are not
  detected. These are useful summaries, not a full element/GPU explanation.
- Live trace types are serialization-only and replay is in-process. The test
  reader uses separate owned mirror types and validates the happy round trip,
  but there is no public hardened reader, out-of-process artifact, forward
  compatibility policy or corruption/fuzz suite.
- CPU span duration is clamped into `u32` nanoseconds at about 4.29 seconds.
  That avoids wraparound but makes exceptionally long hangs indistinguishable
  above the ceiling unless truncation is presented.
- Current WGPUI changes the clock wrapper and removes unsafe raw test-struct
  byte casts, but leaves the profiler architecture and the limitations above
  substantially unchanged.

**Ideas to test.** Capture known synthetic CPU/GPU schedules with injected
queue delay, readback delay, long cross-frame tasks and two interleaved
windows. Assert causal ids, uncertainty, truncation and dropped-generation
states rather than exact cross-domain times the API cannot prove. Compare GPU
duration queries against an external profiler on supported adapters.

Run Lince fixtures for 1, 100, 1,000 and several thousand native Sands with
stationary and active physics; topology editing; large Protein result arrival;
mixed native and browser Sands; camera culling; 2D/3D transition; and off-camera
active behavior. Record p50/p95/p99/worst event-to-visible latency, CPU/GPU
stages, allocations, draw counts and resource memory. Repeat with capture
compiled out, idle and active to quantify observer effect.

Round-trip and fuzz the trace reader with truncated lengths, huge counts,
unknown kinds and incompatible versions. Capture a frame, mutate current
shaders/assets, and prove that semantic replay either pins the old artifact or
refuses; it must not silently render with the new one. Fill reusable buffers
with recognizable prior data and verify a snapshot exports no inactive bytes.

Visually test the profiler Castle itself: an empty state explains how to start,
a partial capture labels every missing lane/resource, keyboard navigation can
reach frames and details, and selecting a span highlights the corresponding
Sand/Area without changing the captured scene.

**Verdict:** build this class of profiler into Lince's winit/wgpu runtime and
use it to validate the prototype and every later optimization. Keep cheap
bounded timelines, explicit deep captures, instanced visualization and honest
partial-resource statuses. Strengthen them with causal frame ids, clock
uncertainty, privacy/redaction, exact artifact identities and a hardened
portable format. Do not describe labels plus selected buffers as complete GPU
state, or reconstructed current-shader drawing as faithful replay.

### 26. Treat evaluation points as typed render contracts, not impossible per-pixel pipeline dispatch

Read 2026-08-30. [The Radiant 2.0 draft](https://pulsarnative.com/Research/doc/?section=drafts&slug=radiant-20)
was read from the portal's generated 2026-07-29 content index and compared
with current [Helio](https://github.com/Far-Beyond-Pulsar/Helio) and
[Pulsar Native](https://github.com/Far-Beyond-Pulsar/Pulsar-Native) source.
No implementation of its evaluation-point registry, template dispatch table
or material-visibility hierarchy was found in those current trees. The draft's
best abstraction strengthens review 23, but its central single-dispatch design
is internally contradictory and should not be prototyped unchanged.

**What it proposes.** A render pass declares named evaluation points such as
G-buffer, transparency, shadow, velocity or SSR, each with a shader function
signature, base source, pipeline layout, fixed-function state and fragment or
compute execution model. A material template targets one or more points and
provides WGSL functions and a parameter schema. Material instances contain
only template id, parameters and textures.

The engine supposedly compiles one pipeline per `(template, evaluation point)`
but submits all instances with one multi-draw per pass. A GPU table maps each
template id to a bitmask of participating evaluation points. Fragment shaders
load the mask and discard nonparticipants; compute passes consult a pyramid
whose texels OR the masks of visible template ids. The draft claims material
count no longer affects CPU dispatch, common materials add no per-pixel cost,
and custom templates have an invariant six-to-ten-cycle check independent of
table length.

**What helps Lince.** Evaluation point is a useful name for a versioned render
contract. Native Sand surface, glyph/icon, Box plane/topology, opaque world,
transparent world, shadow/depth, picking, video and browser composite and post-effect
are separate domains with declared inputs, outputs, ordering, resources and
device requirements. A material/effect graph targets a contract; it does not
splice itself into an arbitrary pass or infer compatibility from function
spelling.

Template versus instance is also the right authoring boundary. Thousands of
Sands can share a small set of renderer programs while tokens and parameters
vary per instance. A template has an immutable artifact digest, typed parameter
layout, allowed textures/samplers and supported render domains. An instance
stores that template identity plus validated parameter/resource references.
That enables editor controls, cache reuse and exact Scene/Box persistence
without treating every color change as shader compilation.

There are only three technically coherent dispatch strategies, and Lince can
choose per render domain after profiling:

1. group indirect draws by compatible pipeline/template and bind a pipeline
   per group;
2. compile a bounded uber-shader containing a finite set of implementations
   and select code inside that one shader; or
3. use separate passes/compute work queues produced by GPU classification.

The first retains independent compiled templates and usually fits opaque
world objects. The second can fit a deliberately small, stable native-Sand
feature set but grows code and divergence. The third may fit expensive
screen-space effects when classification saves enough work. A `template_id`
can select data or a branch already compiled into the currently bound shader;
it cannot switch a draw to another wgpu pipeline or call WGSL absent from that
pipeline.

The render-contract registry is frozen into a deterministic graph artifact.
Names are display metadata; stable ids and exact versions define persistence
and plugin/package negotiation. Duplicate contracts, incompatible layouts,
unsupported device features or dependency cycles produce build/editor
diagnostics before a frame uses them. Adding a trusted pass produces a new
graph revision and validates every material that targets it.

For Lince's user-facing customization, the “template source” is the verified
material IR from review 23. The engine generates WGSL and exact parameter
layouts. Trusted renderer development may supply raw pass code, while
downloaded Castles, Website Sands and collaborators remain incapable of
registering source, bind groups or fixed-function state. A material editor
shows which contracts a graph targets and why a target is unavailable.

**Needed for it to work.** Pipeline grouping or in-shader selection must be
specified honestly before data layout. Each render domain defines ordering:
opaque objects may be regrouped; transparent content needs a sorting or
order-independent-transparency policy; native UI and pinned Sands retain
semantic z order. A single unsorted multi-draw is not a universal replacement
for ranges.

Parameter schemas compile to an explicit aligned GPU layout with offsets,
sizes, defaults, texture classes and device-limit validation. Host and WGSL
array stride are tested byte for byte. Different templates either share a
declared common material envelope or use checked offsets into typed arenas;
the phrase “params + textures” is not enough to make independently authored
WGSL share a pipeline layout safely.

Contract/template registration is transactional and immutable by revision.
Hot reload compiles a new artifact, invalidates every dependent pipeline and
swaps at a frame boundary only after success. Stable table slots use
generation-checked handles and GPU-lifetime retirement. Unknown ids and
out-of-bounds table access fail before submission and have a visible fallback.

Any visibility/classification hierarchy must define its actual construction
algorithm, synchronization, texture/buffer representation and worst case. A
coarse check still launches workgroups; it saves the expensive body, not the
dispatch itself. It is optional unless profiles show that classification plus
memory traffic beats direct execution for Lince's scenes.

**Needed only to make it fast.** Pipeline grouping, indirect command
compaction, material classification, visibility pyramids, finite uber-shader
flags, bindless parameter access, incremental updates and prewarmed pipeline
caches are optimizations. Template/instance separation and typed contracts are
correctness and composition architecture; the particular GPU dispatch scheme
is not. Use the profiler from review 25 to compare them on native Sands,
topology/world content and mixed embedded browser composition.

**Specification audit.** Several claims conflict with GPU pipeline semantics,
the draft's own structures or basic arithmetic:

- It compiles a distinct shader/pipeline for every template/evaluation-point
  pair, then says one multi-draw renders instances from all templates. A render
  pipeline is bound for a draw/pass; a per-fragment integer cannot transfer
  execution to another compiled pipeline. The shown fragment shader contains
  one `eval_*` implementation, not a dispatch over all implementations.
- Consequently the mask only decides whether the currently bound shader runs.
  It cannot make one glass pixel execute the glass pipeline and the next PBR
  pixel execute a separately compiled PBR pipeline. The claimed removal of
  template ranges has removed the mechanism that selected the right code.
- Early depth testing does not make an all-instance transparent draw free.
  Depth comparison, whether transparent objects joined a prepass, equal-depth
  behavior, blending, `discard` and hardware early/late-test eligibility all
  matter. Opaque geometry submitted to the pass still needs exclusion. Shadow
  and depth participation likewise require command classification or shader
  work; “no coverage” is not a dispatch design.
- The draft calls a load, address calculation, bit test and branch zero cycles
  in its Tier 1 tables, then later prices the same sequence at six to ten
  cycles. Issued instructions and memory traffic are not free merely because
  a branch is uniform and not taken.
- Constant instruction count is not constant system cost. A larger, less
  local table changes cache behavior, while divergent ids change memory
  transactions. The appendix pads every dispatch entry to sixteen bytes even
  though the memory analysis repeatedly prices it as four, so 10,000 entries
  are 160 KB under the shown structure, not a tiny pinned table.
- A 32-bit evaluation mask limits evaluation points to 32, not templates. The
  text incorrectly says it bounds the maximum template count. Runtime
  registration-order bit indices are also unsuitable as durable identities.
- The visibility shader performs the “coarse” texture load in every one of 256
  invocations, not once per workgroup as claimed. In the worst case it adds
  hierarchy construction and the extra load, so it can be worse than direct
  per-pixel checking. Uniform return saves the expensive body but is not zero
  executed work.
- Building dependent mip levels needs a real synchronization/multi-dispatch or
  specialized construction algorithm. Calling the complete chain one standard
  reduction dispatch with atomic OR does not specify how later levels wait for
  earlier writes, and the draft supplies no implementation or test.
- The host-layout calculation is wrong. Four trailing `u32`s produce 144
  bytes, already divisible by sixteen. A fifth produces 148 bytes and a WGSL
  array stride of 160. Plain Rust `repr(C)` remains four-aligned/148 bytes
  unless explicit layout changes it. The new field does not simply consume
  pre-existing padding, and the shown host/shader arrays would disagree.
- String/brace scanning is reused as shader composition. It is not a WGSL
  parser, does not verify the declared function signature or bindings, and the
  claim that arbitrary helper functions “survive” is not established by the
  shown replacement algorithm.
- Hot reload re-registers the same template id, while the cache key contains
  id, graph hash and flags but no source/template revision. This repeats
  Radiant v1's stale-pipeline defect.
- A fixed blend/depth state belongs to the evaluation point, limiting what
  templates can mean. Transparent materials still require an ordering policy;
  alpha blending is not made correct by sharing one pass.
- The performance table labels 120 FPS as a 16.67 ms budget; 120 FPS is about
  8.33 ms. Its 0.05/0.1/0.15 ms and cache-cycle figures have no implementation,
  captured benchmark or adapter data behind them. “Final specification” and
  “formal analysis” overstate a draft whose central mechanism is not present
  in current source.
- The comparisons to other engines are uncited and categorical. In particular,
  material instances commonly share parent shader code, and multi-pass effects
  are not generally “impossible.” They are not evidence for Lince's design.

**Ideas to test.** First make three minimal renderers for the same fixture:
CPU/GPU-compacted pipeline ranges, a finite uber-shader and GPU-classified work
queues. Use at least opaque, alpha-tested, transparent and UI-ordered domains.
Vary instance count, visible template count, screen-space mixing and parameter
count; inspect correctness images and p50/p95/p99 CPU/GPU time, pipeline binds,
cache behavior and memory.

Generate host and WGSL layout assertions for every parameter and instance
type, including arrays and dynamic offsets. Mutate a template revision under
load, force compilation failure and device loss, and require last-known-good
pixels plus complete cache retirement. Randomize registration order and demand
the same durable graph/artifact ids.

If a visibility hierarchy later proves useful, implement it as an isolated
experiment. Measure pyramid construction, bytes moved, workgroups launched and
expensive invocations avoided at multiple resolutions and coverage patterns.
Compare against one mask check and against compacted dispatch lists; reject it
if the worst/common Lince scenes do not pay back its 11/44 MB-scale resources.

**Verdict:** keep evaluation points as versioned, typed render-domain
contracts and keep template instances as cheap parameterized data. Reject the
draft's asserted combination of independently compiled template pipelines and
one all-template draw; choose a coherent grouping, uber-shader or classified
work strategy per domain after measurement. The material IR, security,
revision and last-good rules from review 23 remain the authority.

[Rendering a Million Blades of Grass: Helio's GPU-Driven Foliage System](https://pulsarnative.com/blog/2026-08-02-helio-foliage-system)

### 27. Tile derived world detail, never the authoritative Box simulation

Read 2026-08-30. The article was checked against the Helio source at its
publication-day commit
[`7b5f4b8`](https://github.com/Far-Beyond-Pulsar/Helio/tree/7b5f4b82fca0a8074a426f40674321c86dcdc961)
and current main at
[`6fb248c`](https://github.com/Far-Beyond-Pulsar/Helio/tree/6fb248c921fdce8adbc01d0eef8c18a9a98f2308).
Current main contains no later foliage completion that materially changes the
audit below.

**What it proposes.** The world is divided into eight-metre tiles and only a
camera-centred ring has generated foliage resident on the GPU. Moving the ring
queues new tiles, bounds placement work per frame and evicts old tiles by LRU.
Each tile owns a fixed-capacity blade slab. A compute pass deterministically
derives candidate positions from tile coordinates, lane, generation and seed,
then accepts them according to terrain and foliage rules. Tile and 4-by-4
cluster culling compact visible instances into four LOD streams, followed by
four indirect draws. The representation progresses from segmented geometry to
cards and distant terrain perturbation.

The proposed complete system also includes wind with current and previous
sample times, deformation-aware culling bounds, baked lit impostors, a
camera-relative interaction field, progressive recovery after teleports and
hard performance budgets. This is a good example of separating compact
authored rules from a much larger disposable rendered population.

**What helps Lince.** Lince needs this distinction more than it needs a grass
renderer. A topology/world decoration population can be stored as generator
identity, stable seed, parameters, tile coordinates and topology revision,
then regenerated near a view. Grid detail, surface tessellation, particles,
map decoration and high-density visual proxies can follow the same model.
Tile-local coordinates also reduce precision loss as Box eventually spans a
desk, city and globe.

That rule stops at the semantic boundary. Records, Sands, Areas, topology
edits, Protein bindings, group membership and settled positions are
authoritative Box state. They cannot disappear with a camera ring or be
rerolled from a random seed. Off-camera Sand behavior, media, physics and Area
influence remain active as the owner required; only presentation work and
regenerable decoration are culled. A Record may have disposable visual proxies
at several detail levels, but its identity, interaction ports and Protein-
chosen contents remain unchanged. This is technical render LOD, not the
discarded semantic-zoom feature.

A camera-relative interaction texture is similarly only a transient visual
cache. Lince's topology and Areas affect off-camera Sands and survive restart,
so their authoritative fields must use persistent world/Box coordinates and
analytical or tiled definitions. A visible-region texture may be derived from
those definitions for rendering. Conservative displaced bounds are valuable
for topology, animated Sands and Area effects, but visual bounds never replace
the full simulation's force calculation.

**Needed for it to work.** Authored and derived data need different ids,
lifetime rules and persistence. A derived tile key includes the generator
artifact digest, stable layout seed, source-data revision and spatial cell;
changing an unrelated property must not reshuffle the world. If an edit is
intended to preserve existing placements, its content revision and layout seed
must be separate rather than using one generation counter that rerolls every
candidate.

The placement and representation ladder must publish an honest readiness
state. A teleport or large edit may progressively fill visual detail, but the
canonical Box state remains immediately queryable and selectable. Visible and
interaction-near work has priority; fixed per-frame budgets prevent generation
from taking over the frame. Generated host/GPU structures require offset,
alignment, array-stride and shader-conformance tests, not only total-size
assertions.

Collaboration cannot rely on byte-identical floating-point GPU placement for
authoritative outcomes. Persist or synchronize semantic results, or use a
fully specified deterministic CPU/fixed-point path where identical results
are required. Harmless decoration may differ slightly between adapters. A
fixed simulation clock and collaboration revision are also distinct from the
visual wind clock used to produce current/previous positions for motion.

**Needed only to make it fast.** Camera rings, fixed slabs, GPU placement,
cluster culling, Hi-Z, indirect compaction, generated vertices, impostors and
interaction textures are optimizations. None is required to establish the
authored/derived boundary. Their capacities, tile sizes, LOD distances and
work budgets must come from Lince scenes and adapters rather than from the
article's grass presets.

**Source audit.** The repository implements an interesting phase-two skeleton,
not the system the article describes as shipped:

- Placement currently samples a temporary flat plane. There is no terrain
  capture pass, so terrain height, slope and density placement and the distant
  terrain-material ring are absent.
- The interaction binding is a one-pixel placeholder. There is no interaction
  pass, scrolling camera-relative field, recovery simulation or previous-frame
  interaction history.
- The G-buffer shader explicitly uses a procedural material without the
  advertised texture/material table. Cards and impostors remain later-phase
  work, as does per-foliage-type deformation extent.
- CPU reference tests repeat the CPU function. No test was found that compares
  GPU output with CPU output or two GPU implementations byte for byte. Integer
  hashes and fixed scans help reproducibility, but they do not prove the
  article's cross-GPU floating-point guarantee.
- The stated 2.70 ms total and one-million-blade CI gate appear as planning
  targets. The corresponding tests assert capacity, not measured GPU time.
- Quality tables contradict each other. The article first gives Medium a
  64-metre ring and 24 MiB, then later gives it 128 metres and 64 MiB. Source
  methods use the latter values, while source tests still expect Low at 4 MiB
  and Medium at 24 MiB even though the methods return 8 and 64 MiB. The claimed
  presets are not internally settled.
- A 4-by-4 cluster takes its LOD ladder from the first blade even though
  placement can assign different foliage types within that cluster. The
  shader acknowledges that classification can therefore be wrong for mixed
  types.
- The G-buffer uses eight render targets totaling 48 bytes per sample. Source
  tests acknowledge that this exceeds the baseline 32-byte color-attachment
  limit, so the pipeline is unavailable on such adapters. A quality dropdown
  cannot repair an unsupported render-pass layout; adapter negotiation needs
  an alternate G-buffer or a clearly unavailable feature.

The equal-size tile slab makes allocation and indirect addressing simple but
also reserves near-tile capacity for sparse far tiles. That trade can be
measured later; it is not a reason to adopt an elaborate allocator before a
real Lince population requires it.

**Ideas to test.** Build one disposable topology-decoration generator over a
small persistent height field. Move the camera, edit one tile, restart and
verify that authored topology and semantic Sands restore exactly while derived
detail regenerates. Keep an off-camera Sand under Area influence throughout
and prove its simulation did not sleep. Compare CPU generation with GPU
generation, then add tile/cluster culling only after profiler evidence.

Test representation changes without changing picking, event routing or Sand
identity. Exercise a topology displacement larger than its declared bound and
make the runtime report the invalid contract. Finally, force a baseline-limit
adapter profile and require a valid reduced render graph or an explicit
human-readable refusal rather than pipeline creation failure.

**Verdict:** adopt tiled deterministic regeneration for disposable world
detail, local-coordinate precision and bounded progressive work. Reject
camera-residency as a semantic lifecycle, GPU-float determinism as a
collaboration guarantee, and the article's unimplemented performance and
feature claims. Lince's Box store and always-active simulation remain the
authority; the renderer may cheaply rebuild what is genuinely derived.

[Helio VR: OpenXR Integration Through wgpu's Vulkan Escape Hatch](https://pulsarnative.com/blog/2026-08-03-helio-vr-openxr)

### 28. Make spatial presentation a view-family capability, not a second Box

Read 2026-08-30. The article and XR implementation were checked at Helio's
article-day commit
[`b8cdb87`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b8cdb87f74ba142caa3618d847ff3b79730db8f7)
and current main at
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).

**What it proposes.** OpenXR, rather than ordinary wgpu startup, creates the
Vulkan instance and device and selects the physical device connected to the
headset. Helio obtains wgpu-hal's required extensions, asks the OpenXR Vulkan
extension to create the raw objects, then unsafely wraps the resulting Vulkan
instance, adapter, device, queue and swapchain images back into wgpu. OpenXR
continues to own the swapchain images; no-op destruction callbacks prevent the
wgpu wrappers from freeing them.

The runtime recommends per-eye resolution and supplies predicted display time,
asymmetric eye poses and frustums. A strict frame loop polls session state,
waits, begins, locates views, acquires/waits for an image, renders, releases the
image and ends the frame with two projection views. Semantic input actions are
suggested for several controller profiles. A world-from-stage transform moves
the user without rewriting scene content, and an optional desktop mirror
samples the two eye layers side by side.

The article describes a current dual-pass mode and a future single-pass
multiview mode. The latter needs array render targets, a multiview render-pass
mask and shaders indexing a two-camera buffer through the view index. It also
documents an asymmetric projection bug caused by using the wrong clip-depth
convention and swapping two column-major matrix elements; three CPU tests pin
the corrected math.

**What helps Lince.** XR is not a separate persistence model, Sand kind or
world. The same Box snapshot, topology, Areas, Sands, permissions and events
can be presented through a view family containing one desktop view, two XR eye
views, a mirror and eventually other observers. Each view declares pose,
projection, viewport/layers, visibility policy, output target, pixel density
and timing. Derived culling and render products may differ per view while
semantic simulation remains singular and active.

The world-from-stage boundary is especially useful for Lince's surface/free-
space modes. Box/world coordinates remain stable and persisted; a user's room
origin, recentering, head and controller poses are session-local inputs. Live
collaboration transmits an intentional avatar/presence projection at its own
rate and disclosure level, not raw tracking as durable Box state. Locomotion
changes the viewer transform unless an explicit edit moves a Sand or world
object.

Website Sands and native Sands keep the same event and Protein contracts in
XR. An embedded browser still produced an HTML texture; a world or HUD Sand presents that
texture on a surface and maps an authorized ray hit to local CSS pixels.
Pointer focus, capture, scrolling, text input, keyboard ownership and surface
resolution must be explicit. XR does not make arbitrary HTML native 3D, and a
desktop DOM cannot simply be handed controller poses. A Sand may instead have
a native spatial presentation while retaining the same semantic definition.

**Needed for it to work.** If OpenXR owns adapter/device creation, it is an
early bootstrap branch of Lince's graphics host. The renderer cannot first
select a desktop adapter and later assume the headset compositor uses the same
one. Required features, limits, queue families, formats and extensions come
from one shared Lince device contract used by desktop and XR startup; the XR
path may satisfy or refuse that contract but must not maintain a drifting copy.
The unsafe bridge is kept in one small crate pinned to exact wgpu/OpenXR/Vulkan
versions and covered by loader/runtime integration tests.

Dual pass and multiview are distinct coherent render modes. Dual pass binds a
single-layer target, single-view pipeline and the current eye's camera for each
submission. Multiview binds array targets, a matching mask and view-indexed
shader data in one submission. The render graph validates every pass,
attachment, sampled texture and shader for the selected mode before entering
the headset. Unsupported passes need an explicit substitute or are omitted
with a visible capability explanation; blindly injecting a mask is not a
conversion.

The frame transaction needs scope guards or an equivalent state machine. Once
`wait`/`begin` succeeds, every later error path still ends the frame; once an
image is acquired, every later error path releases it after submitted work is
safe. Session loss, device loss, format/size changes and runtime restart tear
down objects in the required ownership order. Reference-space support is
queried, STAGE falls back deliberately when unavailable, and recenter/reference
space changes enter the coordinate mapping rather than silently moving data.

Controller actions belong to a versioned Lince input vocabulary such as
select, manipulate, navigate, menu and text focus. Runtime profile bindings are
replaceable mappings. An inactive or untracked action clears transient motion
without snapping authored objects; a grabbed Sand needs explicit capture and
release semantics and the same permission checks as mouse manipulation.

**Needed only to make it fast.** Single-pass multiview, union-frustum culling,
foveated rendering, late-latched poses, hidden-area meshes, resolution scaling,
cached mirror resources and avoiding the desktop mirror are optimizations.
Dual pass is a valid first implementation only if measurement shows the full
per-eye graph meets the headset's frame deadline. VR performance is about
deadline misses and motion-to-photon behavior, not a desktop average-FPS
counter.

**Source audit.** The useful bridge exists, but the shipped-status conclusion
is not supported by a coherent render path:

- The article first says XR mode forces `multiview_mask = 0b11` and every
  shader works unchanged, then says active rendering is dual pass because the
  shaders use `cameras[0]`. Source contains both designs at once. Building the
  graph with XR enabled forces a multiview mask on every graph render pass and
  creates array transients. `render_xr()`, however, submits each eye to a
  single-layer `D2` view with its `multiview` argument false. A multiview pass
  cannot use attachments that do not provide the masked layers. The comments
  alternately call this single-pass and dual-pass, but the contracts do not
  compose into either.
- The one-layer swapchain fallback is not a working side-by-side fallback.
  The render loop still asks `layer_view(image_index, eye)` for eye one. With
  one layer the flattened index can select the next, unacquired swapchain image
  or run out of bounds. Both full-eye renders also lack side-by-side viewport
  regions, and the mirror shader samples array layer one even though it does
  not exist.
- After `begin_frame`, errors from locating views, acquiring/waiting, either
  render, mirror composition, release or end return directly. There is no
  guard that completes the required frame/image lifecycle, so one ordinary
  rendering error can poison subsequent runtime calls.
- In the STOPPING state, event handling calls `request_exit` rather than
  ending the begun session. Teardown is best-effort and state-selective. This
  needs conformance against the OpenXR session-state rules rather than relying
  on the happy path.
- Device limits and features are reconstructed separately from normal Helio
  startup. The code raises `max_multiview_view_count` to two instead of treating
  the adapter's reported limit as a capability to validate, masks requested
  features down to available ones, and duplicates the texture/buffer caps. The
  source already records a panic caused by that duplication omitting the
  desktop path's buffer-size clamp.
- The projection tests are real and valuable. They are the only tests in the
  XR crate. There are no automated tests for frame sequencing, swapchain
  ownership, one-layer fallback, graph compatibility, input profiles, runtime
  loss or a rendered stereo result.
- “Renders through a headset at 90 Hz” has no captured hardware, timing trace,
  percentile/deadline report or reproducible harness. The article itself says
  the unsafe boundary cannot be tested without a headset. It is a report of a
  local experiment, not performance or portability evidence.

The bridge is Vulkan-only and tightly coupled to wgpu-hal 30 internals. That is
an acceptable contained platform adapter if Lince later chooses it, not a
portable renderer abstraction and not v1 launch scope.

**Ideas to test.** Before integrating a headset, implement a fake XR runtime or
frame-lifecycle model that injects failure after every transition and proves
balanced begin/end and acquire/release. Validate a two-eye color-coded scene
offscreen in true dual-pass and true multiview configurations. Make a one-layer
fallback either correct in layout, rendering and mirror composition or remove
it and fail clearly.

On hardware, record missed compositor deadlines, CPU/GPU pass timings,
motion-to-visible latency and device/extension diagnostics. Test asymmetric
frustums, stage absence, recentering, session focus/loss and adapter mismatch.
Then place one native Sand and one browser Website Sand in space and verify ray
focus, click, scroll, text input, event delivery and permissions without
changing their Box identity.

**Verdict:** retain OpenXR as a future presentation/input adapter over one
authoritative Box and design the renderer around explicit view families now.
The Vulkan escape hatch is plausible when isolated, but Helio's current
dual-pass/multiview combination and fallback are not a reusable implementation.
Lince should first finish its desktop surface/free-space view contract; XR can
then prove that contract without creating a second engine or semantic model.

[Ten Million Sprites: Helio's GPU-Driven 2D Pipeline](https://pulsarnative.com/blog/2026-08-03-helio-2d-gpu-sprites)

### 29. Let the GPU own dense hot projections without hiding semantic results

Read 2026-08-30. The implementation was inspected at the article-day Helio
commit
[`b8cdb87`](https://github.com/Far-Beyond-Pulsar/Helio/tree/b8cdb87f74ba142caa3618d847ff3b79730db8f7)
and current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The relevant passes changed substantially between them; the audit describes
the stronger current implementation while retaining the article's claims as
the object under review.

**What it proposes.** A persistent CPU/GPU sprite pool assigns each sprite a
slot handle and uploads a dirty byte span only after insert, update or removal.
The stable instance pool is separate from a per-frame draw-order buffer. A
vertex-pulling shader reads a sorted slot index for each indirect instance,
then fetches the corresponding transform, atlas rectangle and tint. This lets
draw order change without physically reordering the authoritative instance
array.

GPU compute first simulates every alive slot, then culls the fixed-capacity
pool into compact visible indices and floating-point depth keys. A prepare
kernel converts the GPU-visible count into indirect sort dispatch arguments.
Thirty-two one-bit LSD radix stages each run histogram, serial block scan and
stable scatter kernels. The final visible count is also the instance count of
one indirect indexed draw, so it never needs a CPU readback. A separate
low-resolution lighting path builds an occupancy/emitter texture, jump-flood
distance field and radiance-cascade textures before multiplying the result
over the sprite image.

The important technique is not “everything is a sprite.” It is the separation
of stable storage, disposable visibility/order, and GPU-written indirect work.
The CPU command shape can remain bounded while GPU work scales with pool and
visible counts.

**What helps Lince.** This is a strong candidate for dense native Box
projections: background pattern elements, ports, arrows, particles, selection
marks, map symbols, simplified distant Sand proxies, repeated decorations and
possibly native card shells that share one material family. Stable renderer
slots plus a disposable draw-order list fit the existing rule that Box uids
are durable while GPU indices are not. Culling presentation while compute
continues to simulate all slots also demonstrates the owner's required
off-camera distinction: invisible does not mean asleep.

It is not a universal Sand renderer. Text glyphs require their own atlases and
clipping; native controls need retained interaction and accessibility; the embedded browser
Website Sands are independently produced textures with lifecycle and
synchronization; video has color-space and update-rate requirements; arbitrary
materials and transparent overlaps require compatible batching/order rules.
These sources can meet in Lince's compositor without being forced into one
sprite array or draw call.

GPU physics is useful only if Lince can observe its semantic consequences.
Areas of Influence, topology, picking, “Why is it here?”, persistence,
collaboration and Sand events need positions, effective forces and transitions.
Two plausible prototypes remain: a fixed-step host-authoritative simulation
that uploads transforms, or a GPU hot simulation that emits compact events and
asynchronously returns coalesced checkpoint state to the host-authoritative Box
commit path. The article's rule that the CPU never sees positions is suitable
for decorative bouncing dots, not for Lince Records.

**Needed for it to work.** Each hot field has one declared writer per phase.
If compute owns position and velocity while Box edits tint or binding, those
fields should use separate buffers or a merge protocol; uploading an 80-byte
stale CPU mirror must not erase GPU motion. Transfers of ownership occur at a
frame/fixed-step boundary and carry revisions. Device loss rebuilds a GPU
projection from the last authoritative checkpoint rather than making the
render buffer the only copy of user data.

Runtime handles use slot plus generation. A stale handle after delete/reuse
must refuse instead of modifying the new occupant. Box uid maps to that
ephemeral handle and survives compaction, rebuild and device loss. Shared
host/WGSL layouts come from one versioned schema or generated protocol crate,
with size, alignment, field-offset and array-stride checks in every consuming
pass. Passing raw buffers between crates does not remove their dependency on
the byte contract.

Sort order must encode Lince semantics. Equal-depth native Sands need stable
z-order and group/clip boundaries; transparent browser and video surfaces may need
different compositor batches. A key can combine layer, group, z order and a
stable tie-break rather than treating arbitrary float depth as the whole
contract. Surface-perspective mode may depth-test opaque elements while
retaining explicit ordering for translucent/UI content.

Capacity and memory are negotiated before allocation and surfaced in runtime
health. Overflow clamps the indirect count or fails the frame safely; it never
lets the draw read beyond a compacted list. The frame graph declares simulation
writes, cull reads, order writes and render reads, so ordering is validated
rather than maintained by “add these passes in this sequence.”

**Needed only to make it fast.** GPU culling, indirect draw counts,
vertex-pulling, radix sorting, fixed-capacity arenas, dirty uploads, atlas
arrays, low-resolution radiance and a GPU simulation are optimizations.
Correct persistent identity, field ownership, ordering, overflow handling and
semantic checkpoints come first. Lince should compare a CPU-built visible
list, GPU compaction without sorting, bucketed pipeline/z ranges and full GPU
sorting against its actual mostly-stationary and high-churn Box fixtures.

**Source audit.** The implementation contains useful working pieces and one
real GPU-versus-CPU cull/sort test, but several article statements are too
broad:

- “One million costs the same CPU work as ten” means the CPU records the same
  shaped command sequence after startup. GPU simulation and culling still run
  across every reserved slot. The sorter records one prepare plus three
  kernels for each of 32 bits; with simulation and culling that is 99 compute
  dispatches before the draw, not “three compute dispatches.” The one-thread
  block scan also loops serially over all visible blocks 32 times.
- The ten-million demo's 800 MiB figure counts only the instance buffer. Its
  GPU allocations also include roughly 40 MiB of alive flags, 80 MiB of
  velocities and about 160 MiB for two key and two index buffers: around
  1.08 GB before atlases, other resources and driver overhead. The retained
  CPU slot and alive arrays add roughly another 840 MiB. There is no memory-
  budget preflight or graceful reduced configuration; allocation uses
  `expect` after requesting the adapter's limits.
- Dirty tracking stores one minimum-to-maximum span. Updating slots 0 and
  9,999,999 uploads every slot between them, and both instance and alive
  slices are uploaded. Thirty scattered edits therefore do not necessarily
  “pay for exactly 30 sprites,” and removal does not upload only its alive
  word as the prose says.
- `SpriteHandle` is only a slot `u32`. Removing a sprite and reusing its slot
  makes an old handle silently address the new sprite. Update and removal also
  index without validation.
- Compute-written transforms and the CPU mirror deliberately diverge. The
  “do not call update afterward” rule is documentation only, applies to the
  whole instance rather than individual fields and is not enforced by the
  type or API. It cannot support interactive/persistent Lince data as written.
- The batch, cull and simulation crates duplicate the sprite layout in Rust
  and WGSL while claiming to be independent. No compile-time size assertion,
  field-offset conformance or cross-crate schema test was found. Compilation
  does not catch one pass changing an offset that another hardcodes.
- Atomic cull compaction gives equal-depth items an arbitrary initial order.
  The radix stages are stable relative to that order, but the final tie order
  is not deterministic. The test uses random floating depths, permits ties to
  differ and skips successfully when no adapter exists.
- `max_visible` sizes output buffers, but the cull atomic increments the
  indirect draw count even after that limit. If more sprites are visible than
  the estimate, writes stop while the draw count keeps growing, so the draw
  indexes beyond the valid compacted order. The demos avoid it by allocating
  visible capacity for the full pool; the public contract does not enforce it.
- The demos print recent FPS but provide no captured adapter, GPU timestamps,
  workload trace, percentile results or reproducible claim that ten million
  is smooth. None of the sprite/radiance passes attaches timestamp writes.
- Radiance cascades has no correctness images or tests. It rebuilds its whole
  scene, jump-flood field and every cascade each execution even when only the
  uniform's dirty flag is clean. Emitter count is not clamped to the configured
  buffer capacity. Its “bounces around corners” wording is not evidence of a
  general multi-bounce lighting solution.
- Both sprite and radiance composite descriptors use `Box::leak` for an
  attachment array every frame while saying the executor drops it. A leaked
  allocation cannot be dropped; a long-running Lince interface must use an
  owned/safely scoped descriptor path.

**Ideas to test.** Start with 10,000 lightweight native Sand proxies carrying
durable Box uids and generation-checked render handles. Exercise calm, sparse-
edit, all-moving, all-visible and mostly-offscreen scenes. Record CPU prepare,
command encoding, transfer bytes, every GPU pass and present percentiles. Add
sparse edits at opposite ends of the pool to expose range amplification.

Compare no sort, stable integer-key sort, pipeline buckets and the 32-bit
radix path. Force capacity overflow, stale handles and a device rebuild. For a
GPU-simulation variant, move a Sand through an Area off-camera, emit its
semantic transition, checkpoint it, restart and verify the restored state and
“Why is it here?” chain. Repeat with a host simulation to learn whether GPU
authority is needed rather than assuming it.

Finally composite one native proxy population, real output-resolution text
and several browser and video surfaces. This verifies that batching dense native work
improves the whole interface without sacrificing external Sand correctness or
turning the single-draw count into a design objective.

**Verdict:** adopt stable-instance/disposable-order separation, GPU-generated
visible work and continued off-camera compute as important performance
repertoire. Do not adopt slot-only handles, convention-based field ownership,
hidden byte-layout coupling, unbounded indirect counts or GPU-only semantic
positions. The technique should accelerate selected projections inside
Lince's retained compositor; it should not redefine every Sand as an 80-byte
sprite.

[Sublevels: One Coordinate Space Behind Moving Chunks, Streamed Worlds, and Portals](https://pulsarnative.com/blog/2026-08-07-sublevels)

### 30. Use coordinate frames for Castles and worlds, not renderer-only sublevels

Read 2026-08-30. The implementation was inspected at the article-day Helio
commit
[`50f0b1d`](https://github.com/Far-Beyond-Pulsar/Helio/tree/50f0b1d461823cb5b7acfb9d58d3c71b350e86bd)
and current main
[`4f9c85b`](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be).
The article combines a shipped coordinate-space mechanism, an experimental
portal renderer and a streaming design that it explicitly says is not yet
built. Those have different confidence levels and should not be accepted as
one finished feature.

**What it proposes.** Every render instance keeps its ordinary local transform
and an eight-bit coordinate-space index. Index zero is the world identity; the
other indices select a current and previous 4x4 transform from a small shared
GPU table. A sublevel captures the instances in a group, assigns them one
coordinate-space slot and changes the whole group by updating that one parent
matrix. The shader composes parent and local transforms during culling and
drawing. The previous table provides motion-vector history without rewriting
each child.

The proposed streaming layer would divide a world into independently resident
sublevels, load them inside a radius, unload them beyond another radius and
use hysteresis and budgets to avoid oscillation. Nonresident objects would not
exist in the live scene. This is a design in the article, not an implemented
Helio facility.

Portals reuse the same coordinate-space table. A portal pair contributes a
mapping from one opening into the other, and chains compose those mappings up
to a bounded depth. An extra GPU cull duplicates visible scene instances for
eligible chains. A mask pass stamps the opening, resets depth inside it, and a
second G-buffer pass draws the remapped geometry through the main camera. It
avoids a texture and separate recursive camera for every portal, but it does
not avoid additional visibility, geometry, mask or fill work.

**What helps Lince.** Parent coordinate frames are a good primitive for a
Castle or locked group of Sands. Each member can retain a local pose while one
group pose moves, rotates or animates the composition. An Area that attracts
one member of a group can move the group root, matching the decision that a
group acts as one. The same mechanism can cheaply place repeated scene chunks,
map tiles, imported constructions and dense visual proxies.

Castle must remain a composition abstraction rather than a special renderer
object. Box persists the Castle uid, membership, member-local poses, parent
relation and group pose. GPU table slots are disposable projections rebuilt
from that state. The Box editor and a compiled built-in Castle should produce
the same composition model, so a group made visually can later be reused just
as one assembled in Rust.

Coordinate frames also clarify the relation between Lince's views. In surface
mode, a Sand has a coordinate on the Box plane and a pose derived from the
topology at that coordinate; the card remains glued to the deformed surface
and follows its slope. Perspective 3D shows that same state from an oblique
camera. Free-space mode gives the Sand a three-dimensional pose and shows the
surface as a thin reference plane; collapsing back projects it onto the
plane's x/y position according to the documented collapse policy. These are
different projections or motion constraints over one Box, not independent
copies of its Records.

The portal technique may eventually support linked views between Box regions,
Castles or world spaces. It is not required for the v1 Box, and it is not a
general answer for showing another world. Duplicate geometry assumes that both
sides share one compatible scene and render domain. Website Sands, video,
transparency, per-view exposure, different post-processing and independently
configured worlds may require a view-family render or composited surface
instead.

**Needed for it to work.** Lince needs an authoritative transform graph with
stable uids. Nodes carry typed local poses, parent links and enough revision
information for persistence and collaboration. Parenting is cycle-checked,
nested Castles compose ancestors deterministically, and reparenting explicitly
chooses whether to preserve local or world pose. Membership changes are
transactions: adding, removing, dissolving and overlapping groups cannot leave
a child silently attached to a recycled renderer slot.

All semantic consumers derive the same world pose from that graph. Rendering,
culling, picking, focus, accessibility geometry, pointer routing, embedded browser surface
composition, physics, collisions, Areas of Influence, Protein spawn placement,
topology adhesion, event coordinates, saving and collaboration cannot each
invent a nearby transform calculation. Current and previous poses advance at
a defined simulation or presentation boundary so motion history is meaningful.
“Why is it here?” records parent movement, Area forces and topology constraints
rather than only the final matrix.

Rigid frames and topology are separate primitives. A rigid pose should not
accept arbitrary scale or shear while its bounds and normals still assume a
rotation and translation. A Sand glued to topology stores a surface anchor and
derives position and tangent frame from the field. A Castle spanning a curved
surface may need individual member anchors rather than one rigid matrix. A
movable Topology Effect changes the field felt by eligible Sands; it is not
equivalent to moving every affected object under one parent.

For the future globe, a flat table of `f32` matrices is not the authoritative
location system. Planetary and city-to-desk scales need durable geographic or
cell-local coordinates with sufficient CPU-side precision, then camera-relative
`f32` transforms for the GPU. Coordinate frames remain useful at each cell,
tile, scene and construction boundary, but their render slots and origins are
derived and replaceable.

Streaming must distinguish visual residency from semantic activity. Lince's
off-camera Sands, games, Protein bindings, calls and Areas remain active by
owner decision. Distance-based unloading can apply to regenerable meshes,
textures, map detail, Gaussian data and other presentation resources. It
cannot delete a semantic Sand or suspend its behaviour merely because the
camera moved away. If a heavy external Website or video requires a resource
policy, that policy must be explicit and must preserve the requested live
semantics rather than borrowing the article's object-removal rule.

**Needed only to make it fast.** A GPU transform table, packed space index,
dirty-range uploads, indirect duplicate draws, portal-chain culling, masks,
residency radii, budgets and hysteresis are optimizations. The transform graph,
valid group membership, stable identities, shared world-pose semantics,
surface anchors and persistent state are correctness. Start with a host-side
graph and measured uploads; move hot derivation or visibility work to compute
only when the profile justifies it.

The GPU table should eventually be growable or capacity-negotiated, use
generation-checked handles and upload only changed ranges. A fixed 32-slot
table is useful evidence for a prototype, not a Box limit. Previous transforms
can be copied only at the boundary and only for live changes. Portal work needs
visible overflow diagnostics, spatially relevant prioritization and GPU timing
before its caps or chain depth can be considered a performance policy.

**Source audit.** The implementation proves that shared parent transforms can
be threaded through several rendering paths, but it does not yet prove the
article's broader moving-world claims:

- Updating one coordinate space changes one CPU array entry, but `flush`
  uploads the entire 32-matrix table. The previous table is copied and marked
  dirty every frame, so it is uploaded every frame even when no space moved.
  This is a small fixed cost, but not literally one matrix upload.
- An instance has only one packed space id. Overlapping sublevels are therefore
  last-writer-wins. Refresh assigns current group members but does not restore
  members removed from the group. Removing the sublevel scans only the group's
  current members, so an earlier removed child can retain a freed id; reusing
  that slot can make the stale child move with an unrelated space. Removing
  one overlapping sublevel also resets world identity rather than recovering
  another parent.
- The public API accepts any `Mat4` although the article requires a rigid
  transform. Culling preserves sphere radius and the shaders transform normals
  as though scale and shear are absent. Nothing enforces that precondition.
- Coordinate spaces are applied in rendering, culling and shadow shaders, not
  automatically in physics, Areas, behaviours, ordinary CPU picking or light
  placement. A helper exists for CPU composition, but every caller must opt in.
  The demo's point light is inserted independently and does not ride with the
  sublevel despite the surrounding presentation. This is render indirection,
  not yet an authoritative moving-world system.
- The current sublevel demo builds a 128 by 128 grid, roughly 16,384 studs,
  rather than the article's stated population above 65,000. No captured GPU,
  adapter, frame-time percentiles or timestamp measurements support “frame
  time does not move.” No sublevel lifecycle tests were found.
- Portal pose composition is constant work per mapping, but portal rendering
  is not the cost of moving a platform. The portal cull scans draw groups for
  chains, then the mask stamps openings, depth is reset and duplicate geometry
  is drawn. Work grows with chains, visible instances and covered pixels. It
  uses one main camera, but it still adds another cull, so “one camera, one
  cull” is inaccurate.
- Portal chains are bounded, unrolled recursive visibility rather than an
  absence of recursion. Current source added position-based reachability
  pruning after all-sequence generation produced ghost content. The heuristic
  ignores orientation, and a 300-chain cap truncates traversal order rather
  than selecting the most relevant views with a documented policy.
- Current portal buffers cap draws at 512 and group-chain pairs at 1,024. Extra
  work is silently dropped instead of being surfaced as an incomplete view.
  Diagnostic readback calls an indefinite device poll on the render path every
  60 frames when pending, creating a periodic synchronization stall.
- The default graph places `PortalMaskPass` between the ordinary G-buffer and
  `PortalInstancePass`. Because the mask opens its own passes, the later
  duplicate draw does not fuse into the same physical G-buffer pass despite
  source prose claiming adjacency-based fusion. The portal instance descriptor
  also leaks a newly boxed attachment slice every frame.
- Duplicate portal content uses the default opaque G-buffer material rather
  than every source material family. It has no general path for transparent
  content, browser Website Sands or video. Portal duplicates do not cast portal
  shadows, and the article correctly identifies both that and missing portal
  Hi-Z as limitations. VR was also explicitly untested.
- Streaming remains a proposed policy. It has no source implementation to
  validate, and applying its nonresident-object deletion directly to Lince
  would violate the rule that off-camera semantics remain alive.

The article's rejection of render-to-texture is also too absolute. A portal
surface need not reserve a fixed full-resolution target permanently or display
the previous frame: transient atlases, scaled/scissored targets, visible-only
rendering and same-frame scheduling are possible. They are still expensive and
recursive composition remains difficult. Conversely, remapping geometry
through one camera is elegant but cannot provide arbitrary per-view worlds,
render settings or HTML content. Lince should retain both as possible
presentation strategies and choose from the view contract.

**Ideas to test.** Build one Castle from native Sands and one from a Box-edited
group. Nest them, reparent while preserving world pose, dissolve a group and
restart Lince. Move the root through an Area and verify collision, picking,
browser pointer coordinates, saved positions, collaboration operations and “Why
is it here?” all agree. Remove a member immediately before destroying a group,
reuse every renderer slot and prove no stale child moves.

Place individual Sands on a curved topology field, then compare surface,
perspective and free-space views. The cards should follow position and tangent
in surface modes, preserve the documented x/y projection through a free-space
round trip, and continue simulation outside the camera. Repeat at globe, city,
room and desk scales with camera-relative rendering to expose precision loss.

Benchmark 1, 32, 1,000 and deeply nested frames under sparse and all-moving
loads. Measure transform resolution, upload bytes, culling, picking and frame
percentiles separately. Then prototype one portal between two Box regions with
native opaque Sands, transparent content and a embedded browser surface. Compare remapped
geometry and a composited view, force every capacity limit and make incomplete
presentation visible rather than silently dropping it.

**Verdict:** adopt hierarchical coordinate frames as an authoritative Box
primitive and use GPU transform tables as their fast projection. This directly
supports reusable Castles, topology-aware Sands, repeated world pieces and the
surface/free-space continuum. Do not adopt renderer-only membership, ephemeral
space ids as saved state, semantic distance unloading, or the portal path as a
universal alternate-view system. Sublevels supply a valuable rendering
technique; Lince must complete it into one transform model shared by every
subsystem.
