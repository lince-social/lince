# Native Interface Laboratory

Purpose: Preserve the acceptance fixtures, benchmark contract, evidence, and handoff for the selected runtime.

Owner source: [Interface in Lince](../Lince.lingua).

Status: Promoted native architecture, customization and primitive-Sand
evidence; the production live report awaits the owner reference repair
described in the plan.

Read when: validating runtime changes or deciding whether a prototype seam may become production contract.

[Corpus map](README.md) · [Current context](current.md) · [Interface plan](plans/interface.md)

---

#### Native Interface Laboratory

The Native Interface Laboratory was the first implementation cluster of the
v1 Interface refactor. It is a human-runnable, instrumented vertical slice,
not a visual mock or a second product. It decided that the intended native
parts can share ownership cleanly, preserve genuine HTML and meet the
representative v1 load on the owner's machine.

Its code is the root-workspace package `lince-interface` at
`crates/interface`. The laboratory was promoted in place, so its package is
now the favored native interface implementation rather than a parallel
prototype. It uses the root lockfile, contains no experimental Git
dependency, and is consumed by `lince-desktop` on Linux. Semantic crates do not
depend on it. On Linux the production desktop starts the real local Lince
server and then enters this same native Wayland runtime; there is no parallel
Tauri desktop or laboratory-only host to drift from production.

The cluster closed through five internal gates:

1. **P0a — ownership and dependency preflight.** Resolved one exact compatible
   source graph for `wgpu`, `wgpu-hal`, raw-window-handle types, Bevy, the GPUI
   candidate or fork, CEF and platform bindings. Run an empty Lince-owned
   compositor and the GPUI-owned diagnostic baseline, expose their ownership
   and adapter facts in a human-runnable panel, and export the first report.
2. **P0b — native render and input seam.** Added manual Bevy rendering, one
   narrow Lince `wgpu` pass and GPUI visual/input output. Compare frame
   ownership, scale, focus, IME, resize, teardown and recovery, then select the
   Lince-owned host and retained UI direction without a GPUI production fork.
3. **P0c — external HTML seam.** Added accelerated CEF surfaces, transformed
   input, Website/installed authority separation, storage/network/media work,
   external-memory synchronization, process teardown and renderer recovery.
4. **P0d — semantic and spatial seam.** Exercised the draft semantic graph and
   projection manifests, current Protein integration, token changes, Areas,
   the Avian-versus-specialized-solver comparison and camera invariance.
5. **P0e — joined decision.** Ran browser/Facade parity, accessibility,
   packaging, complete load and lifetime matrices and the final visual gate.
   Only the joined candidate can accept Plan A.

Every gate left a runnable scenario, machine-readable evidence and an honest
failure state. Defects found by later gates were repaired in the owning seam.

##### Starting tool set and decisions

| Part | Starting choice | Decision produced |
| --- | --- | --- |
| Outer application | Preferred Lince-owned `winit` event loop and `wgpu` instance, adapter, device, queue, swapchain and final compositor, measured against one GPUI-owned diagnostic window | Exact pins/backends, ownership, submission and device-loss policy |
| Retained native world | [Bevy 0.19.1](https://bevy.org/news/bevy-0-19/) with selected features, manual render resources and no Bevy `WinitPlugin`, Bevy UI, audio or unexamined `DefaultPlugins` | Exact feature list and any bounded Lince adapter/fork |
| V1 physics | [`avian2d` 0.7](https://github.com/avianphysics/avian) and a narrow SoA Lince field/constraint solver behind the same `PhysicsAdapter`, fixed 60 Hz with render interpolation and a measured 120 Hz comparison | Select from representative force, sort, group, boundary, collision, dirty-island and teardown evidence; no choice leaks into persisted Sand semantics |
| Native controls and editors | Lince-owned retained UI over the host WGPU frame assembly, Glyphon/cosmic-text, normalized input and AccessKit; the workspace's [GPUI 0.2.2](https://docs.rs/gpui/0.2.2/gpui/) and the exact external-compositor source remain measured quality and behavior references | Minimal retained layout/paint/focus/text/accessibility surface and the exact bounded techniques worth reusing without importing a second host |
| External HTML | Direct pinned [`cef-rs` 151.8.0+151.3.24](https://github.com/tauri-apps/cef-rs) accelerated offscreen rendering and CEF subprocesses | Exact Linux handle path, WGPU boundary, process/sandbox/package/update policy and measured admission budget |
| Accessibility | One Lince [AccessKit](https://github.com/AccessKit/accesskit) tree for native UI and custom world Sands; CEF retains Chromium semantics behind a bridged browser subtree | One inspectable, focusable composed route on Linux rather than unrelated trees |
| Coordinate seam | A small Lince-owned parent/local-frame fixture, with [Big Space's Bevy 0.19 branch](https://github.com/aevyrie/big_space/tree/bevy-0.19) as an optional measured comparison rather than a required dependency | Whether a library is useful enough to retain; authoritative coordinates remain Lince data |
| Browser/Facade projection | Existing Rust/Maud, ordinary HTML/CSS and pure JavaScript modules; no TypeScript or Datastar dependency | Proof that one shared definition fixture retains meaning outside the native renderer |

`bevy_cef_core`, Pulsar, the GPUI external-compositor work and other
integrations remained reference implementations during the laboratory. Direct `cef-rs` is
the baseline because Lince, not Bevy, owns browser-surface composition. A
dependency already appearing in `Cargo.toml` is not accepted merely by being
present: the report records exact versions/commits, enabled features, duplicate
runtime stacks, unsafe and platform code, licenses, notices, binary and compile
cost, release cadence, fork delta and named upgrade owner.

No Chromium demo flag that disables web security, ignores certificate errors
or weakens process isolation may enter an accepted fixture. Controlled network
tests use a valid local/test origin; installed packages and Websites retain
separate storage/permission partitions and all host authority still crosses the
declared bridge.

The ownership gate recorded a dependency-alignment table before visual integration began.
The table includes exact revisions, every duplicate `wgpu`/`wgpu-hal` and
raw-window-handle family, enabled Cargo features, backend APIs, external-memory
types, ownership of unsafe interop and whether a texture can cross the seam
without readback. An exact GPUI source commit replaces the crates.io baseline
when the required compositor or offscreen API cannot be applied to that
baseline; the plan does not preserve an old pin for compatibility.

The first ownership slice originally established a standalone laboratory host
in `crates/interface`; it is the root-workspace `lince-interface`
package consumed by the desktop. `mise run interface-lab` enters a dedicated Nix
shell and opens a Lince-owned `winit` 0.30.12 / `wgpu` 29.0.4 window with a
Lynx-colored, Lato-rendered dependency and ownership panel. It records the
actual adapter, backend, driver, surface, scale and ownership candidates and
exports a versioned JSON report with `E`. Failure before GPU initialization is
also a first-class result: a missing platform library produces an unavailable
report and exits without a panic. The checked host graph resolves one aligned
`wgpu`/`wgpu-core`/`wgpu-hal` 29.0.4 family.
`mise run interface-lab-report` renders one frame, exports the same report and
exits for repeatable checks. The first successful NixOS run selected the Intel
Iris Xe integrated GPU through Vulkan and Mesa 26.1.5, used an sRGB BGRA surface
with FIFO presentation, and completed at the real 1366×740 window surface with
no initialization or render error. A later release run in the same environment
reported 1920×1052 despite the 1040×760 requested size, so initial surface size
is evidence to capture rather than an assumed constant. This is host proof, not
a frame-time or joined-compositor result.

That slice also disproved one assumption before integration work grew around
it: the workspace's crates.io GPUI 0.2.2 uses Blade Graphics 0.7, while the
external-compositor direction being evaluated is a newer `wgpu` implementation.
GPUI 0.2.2 remains only the existing compatibility baseline.

The exact GPUI-owned diagnostic input is now
[`MSIsunny/zed@bfa9c6c`](https://github.com/MSIsunny/zed/tree/bfa9c6c148f286fb4f645571ca08080c51cf0820),
pinned rather than followed by branch name. Its Linux platform path used
`gpui_wgpu` with `wgpu` 29.0.4 and raw-window-handle 0.6, so P0a aligned the
laboratory host to the same single 29.0.4 GPU family. In the diagnostic GPUI
owns the platform loop, rendering context, surface and final presentation. A
Lince compositor object receives GPUI's shared device, queue and frame-scoped
encoder, renders a live sRGB texture on the GPU and returns its view for GPUI
to sample; there is no framebuffer readback. The current fork submits that
external-compositor encoder before a separate main-scene encoder, so it proves
device-compatible texture composition but not the final submission policy.

The now-removed GPUI diagnostic opened that human-visible comparison, let the
window and live surface settle, exported its JSON report and exited. The first NixOS run selected the same Intel Iris Xe and Mesa
26.1.5 hardware without a software fallback, registered and presented the live
external texture, and reported no initialization error. The requested 920×640
window reported a 1920×1052 runtime viewport in this compositor session while
the external slot remained 420×420. Because a later Lince-owned host run also
reported the same full 1920×1052 surface instead of its requested size, P0b
classified shared compositor policy versus per-candidate initial-window
behavior and repeated it across bounded resize and scale evidence rather than
normalizing it away. That comparison is recorded below.
The cost is also now concrete: the original non-target-filtered comparison
added 463 normal dependency packages over the base laboratory. The reproducible
Linux-reachable audit counts 165 packages in the base profile and 541 in the
GPUI profile, a 376-package delta, with four pinned Git source families. That is
acceptable evidence infrastructure, not an automatic production dependency
decision. The completed P0b comparison rejected a GPUI production
extraction/fork for v1. At that stage GPUI-to-host composition, CEF runtime
integration, full joined output and live frame metrics were honestly absent;
the later joined gates supplied the accepted CEF and host evidence without
putting GPUI into production.

`mise run interface-lab-audit` now resolves five locked root-workspace profiles:
base, Bevy, physics, CEF and joined production. GPUI is absent from every
accepted profile.
It exports exact packages, activated features, source and license metadata,
duplicate critical packages, Git source families, assertions and source-review
findings. The same audit is reachable with `A` from the Lince-owned laboratory
panel. The earlier standalone report measured 303 packages for Bevy, 332 for
Bevy plus Avian, 256 for CEF after excluding its WGPU-30 convenience helper,
and 684 for all candidates. The promoted root-workspace report sees the full
771-package workspace metadata set in every profile, so package-count deltas
are no longer treated as feature reachability evidence. Its meaningful gates
are the selected feature graph, the single WGPU family, zero Git families,
license presence and the bounded source inventory.

The current audit scans the four retained ownership surfaces: `avian2d`,
`bevy_render`, `cef` and `cef-dll-sys`. Every selected package declares a
license and a license or notice file is present at or above its source root.
The packaged CEF runtime additionally carries CEF's authoritative license and
Chromium credits.

The source scan reports Rust-file counts, exact platform-name tokens, files
containing `unsafe` and lexical `unsafe` token counts. Its first run found 62
such tokens in Avian, 69 in `bevy_render`, 24 in GPUI, 30 in `gpui_wgpu`, none
in `gpui_platform`, 20,553 in `cef` and 11,702 in `cef-dll-sys`. The rejected
GPUI counts remain historical comparison evidence; the current source set
reports the Avian, Bevy and CEF values only. The CEF numbers
mostly identify its generated FFI binding surface and must not be read as a
like-for-like code-quality comparison. Lexical counts are navigation evidence,
not a semantic unsafe-code audit; each retained interop seam still needs a
named Lince owner and manual review. Likewise, platform tokens prove that a
branch exists, not that it compiles or runs. This remains a graph and source
surface, not runtime interoperability evidence.

The report assigns each retained surface rather than leaving dependency code
as nobody's responsibility:

| Resolved source | Lince owner and retention boundary |
| --- | --- |
| `avian2d` | Physics adapter; retain only if representative force and collision evidence wins |
| `bevy_render` | Native-world adapter; only manual rendering on host resources, never an engine-owned window or final compositor |
| `cef`, `cef-dll-sys` | Installed-HTML runtime adapter; CEF FFI, subprocess lifecycle and GPU-handle import stay in one fail-closed boundary |

The rejected GPUI sources have no row because they are not retained. The exact
single-WGPU graph, assigned source boundaries and machine-readable reports
closed the ownership decision; the later joined runtime accepted CEF.

Bevy 0.19.1 and Avian 0.7.0 compile in that exact graph. Bevy resolves the same
WGPU 29.0.4 family as the Lince and GPUI hosts, exposes
`RenderCreation::Manual` for caller-supplied render resources, and is included
without Bevy's window runner, UI, audio or default plugin blanket. Avian's
comparison profile keeps parallel f32 Parry collision while omitting its debug
renderer, scene and picking defaults.

`mise run interface-lab-bevy` now runs the next host fixture, and
`mise run interface-lab-bevy-report` renders, exports its ownership report and
exits. The Lince host creates the WGPU instance, adapter, device and queue, then
passes clones of those same handles through Bevy's `RenderCreation::Manual`.
Bevy has no Winit runner, primary Bevy window, schedule runner, Bevy UI, audio,
pipelined-render plugin or `DefaultPlugins`; the outer `winit` loop continues
to own application lifetime and final presentation.

The first runtime attempt exposed the implicit prerequisites hidden by the
usual Bevy plugin group. `RenderPlugin` schedules camera and render-asset work,
so omitting Bevy's window-message registration, `MeshPlugin` and base
`CameraPlugin` produced fail-fast missing-message/resource errors even in an
empty world. The corrected fixture installs an explicit minimal sequence:
task pools, frame count, time, transforms, a headless `WindowPlugin` with no
primary window or exit authority, assets, manual `RenderPlugin`, images,
meshes and the base camera resources. It does not solve this by adding the
default group. The successful NixOS report selected the same Intel Iris Xe
Vulkan device, recorded Mesa 26.2.1 in that run and completed two manually
driven empty-world updates before the host presented its panel.

The fixture now goes beyond initialization. A system in Bevy's render-graph
schedule clears a live 512×512 sRGB texture with an animated Lynx-dark field,
and the Lince compositor samples that texture behind the report text in its own
final pass. The path uses the shared WGPU device and queue, creates no Bevy
surface and performs no CPU readback. The exported report states that GPU
output is active.

The first implementation let Bevy submit the output before Lince submitted the
final compositor. Source inspection found that Bevy 0.19.1 exposes enough
public scheduling API to replace that ownership without a fork. The fixture
now removes only Bevy's stock `render_system`, retains its extraction,
preparation, pipeline-cache and render-graph work, and installs a
Lince-controlled render-graph runner after `RenderSystems::Render`. The output
system gives its completed command buffer to the outer host. Lince performs one
queue submission containing the Bevy output buffer followed by its final
compositor buffer, then presents the host surface. Bevy performs zero direct
queue submissions. This establishes the preferred submission shape through a
bounded adapter against public API; a fork is not justified at this seam yet.
The host represents that order with a small typed frame assembly: contributors
can append work, sealing consumes the assembly by appending the final Lince
compositor, and only the sealed result exposes command buffers for submission.
The report exports the ordered participant names, so a future native UI or CEF
adapter cannot become an invisible independent submit path.

Replacing the stock finalizer also means that Bevy's screenshot, GPU-readback
and Bevy-window presentation work is deliberately absent. Lince does not need
Bevy window presentation, but any accepted screenshot or readback capability
needs a host-owned equivalent and must not silently reintroduce a Bevy submit.
The semantic/spatial gate owns simulation behavior. The native input gate
exercised focus/IME, resize, presentation-surface recovery and deterministic
teardown; the joined gate subsequently exercised induced whole-device loss and
the complete accessibility route.

The version-7 ownership report separates redraw events, host presentations,
host queue submissions and submitted command-buffer count, Bevy updates,
Bevy-to-host command-buffer handoffs and direct Bevy submissions, GPUI view
renders/external compositions, keyboard input, resize and scale events, surface
recovery, CPU-side frame duration, normalized/refused input totals, the last
accepted input envelope, its target-local point, the most recent refusal,
bounded resize observations, capability registration, teardown state and the
selected ownership decision.
The latest coordinated one-frame Bevy exit recorded one host presentation, one
host queue submission containing two command buffers, one Bevy output handoff,
zero direct Bevy submissions, one initial resize event, one scale event and
19,900 microseconds from entering the redraw path through the CPU present call.
The earlier uncoordinated trace is retained as the reason this boundary
changed, not treated as the current shape. The corresponding 60-render GPUI
exit recorded 56 actual external compositions, making its registration/warm-up
gap visible instead of calling all view renders composed frames. Both automated
runs reported zero normalized and zero refused input because no person or input
driver operated either window; that empty state is distinct from an invalid
message. Neither sample is a performance result: the Bevy run has no warm-up
distribution or display-present timestamp, and the GPUI fork does not yet
expose equivalent CPU/GPU timing. They prove counter semantics and give P0b a
reproducible starting trace.

Both human-visible hosts now route observations through one renderer-independent
input envelope instead of treating an open window or unrelated counters as
input proof. Input contract version 1 carries a monotonic sequence, explicit
Winit/GPUI/replay source, physical surface size and scale, stable surface and
target semantic ids, adapter name, invertible surface-to-target transform,
target-local clip and an internally tagged event. Events cover pointer motion
and buttons, scroll units, touch phases, key press/release and repeat,
modifiers, Winit IME preedit/commit/enabled/disabled and focus. Pointer events
retain physical surface coordinates; the target transform derives local
logical coordinates, so renderer adapters do not each invent hit-space
semantics.

The boundary validates exact schema version, nonzero sequence and surface,
finite positive scale, finite invertible transforms, positive clips, finite
coordinates, bounded identifiers/key/text and valid UTF-8 byte ranges for IME
selection. Unknown event kinds and unknown fields fail deserialization;
unsupported versions and malformed payloads are refused without being counted
as accepted input. Tests cover coordinate mapping and each fail-closed case.
The version-7 report preserves the latest accepted envelope and refusal reason
so “nothing happened” cannot be confused with “input was rejected.”

The Lince-owned Winit adapter emits pointer, button, scroll, touch, physical and
logical key, modifier, focus and full IME events and records first-pending-input
to host queue-submit time separately from physical presentation latency. The
GPUI-owned diagnostic emits pointer, button, scroll, logical key, focus and IME
events into the same `interface-laboratory` target, including scale conversion
from GPUI logical pixels to physical surface coordinates. Its
`EntityInputHandler` keeps UTF-16 selection and marked ranges while converting
the normalized IME cursor to validated UTF-8 byte ranges. The target remains
tab-focusable, focuses on a left click and publishes an AccessKit `TextInput`
role, label and value. The panel and report expose accepted/refused and category
totals plus whether the IME handler, accessible node and context-recovery
callback were registered. GPUI does not expose a hardware key code through
this event surface, so that field is explicitly absent rather than fabricated.
Clipboard/drag, a joined accessible tree and a comparable GPUI event-to-submit
timestamp remain later-fixture work; the shared envelope closes the meaning and
validation seam, not every input feature.

Both running panels expose `E` as a human report action. The Winit host defers
that write until the input-triggered frame has reached the host queue submit,
so the exported report contains the key's normalized envelope and the resulting
event-to-submit sample. The GPUI panel exports on its next rendered view and
prints the path in the terminal; it does not claim a submit timestamp its
current renderer API cannot observe.

The pinned GPUI source is directional in a way the dependency graph alone did
not reveal. Its external-compositor API lets another renderer produce a texture
which a GPUI-owned window samples. It does not expose GPUI's scene as a GPU
texture or command buffer for a Lince-owned host to compose. `WgpuRenderer`
constructs a window surface, acquires its frame in `draw`, submits its external
encoder, separately submits its scene encoder and presents. GPUI's headless
scene interface is test-only, returns an RGBA image, and has a renderer only on
macOS in this source; Linux returns no headless renderer. It is therefore not a
zero-readback embedding route.

The bounded resize fixture requests five sizes and waits at most 30 rendered
frames for each. On the owner's Wayland session the Lince/Winit host remained
at 1366×740 for all 150 wait frames. This is an observed compositor-policy
result, not a hung resize: Winit 0.30.12 intentionally ignores client size
requests after a maximized, fullscreen or tiled Wayland configure. The pinned
GPUI source directly changes its Wayland surface geometry and internal drawable
size; it reported all five requested viewports in one or two frames. Those are
not equivalent policies, so the comparison did not label GPUI faster or Winit
broken. The joined Wayland fixture accepts compositor-driven resize. Both runs
used the actual 1.0 scale factor; renderer-independent tests replay physical-
to-logical mapping at 1.0, 1.25, 1.5 and 2.0. Further compositor-provided
physical scales are support-matrix evidence, not a reason to force a different
Linux backend.

The same stress run exercises lifecycle rather than terminating the processes
abruptly. The Lince host recreated and configured a new WGPU presentation
surface, presented once through it, waited for the device queue to become idle,
recorded zero teardown failures and then exited through Winit. The GPUI
diagnostic unregistered its external slot, records whether removal was
immediate or deferred until the last painted frame drained, and calls GPUI's
graceful quit instead of `process::exit`; the measured run removed immediately
with zero failures. GPUI does not expose a diagnostic queue-idle observation,
so that report value is honestly absent. The pinned renderer has a platform-
owned `device_lost`/`recover` path and notifies
`WgpuExternalCompositor::on_context_recreated`; the diagnostic registers that
callback, but no safe public hook can induce device loss. A successful
presentation-surface rebuild is not misreported as whole-device recovery.

The native seam therefore selected the Lince-owned Winit/WGPU outer host for the remaining
laboratory gates and did not retain GPUI in the v1 production dependency
graph. Putting the pinned GPUI underneath that host would require construction
from host resources, Linux offscreen scene output, command-buffer handoff,
embedded platform input/IME/AccessKit routing, and shared device-loss recovery;
these cross renderer, platform-window and accessibility ownership and are not a
small auditable fork. Choosing the GPUI-owned alternative would preserve its
sharp controls but give it the event loop, surface, final presentation and two
queue submissions, contradicting the measured one-owner frame path. Lince will
implement the narrower retained control/editor surface over its existing
frame, text, normalized-input and AccessKit services, using GPUI as a visual
and behavioral reference and reusing bounded licensed techniques only when
they do not import its application lifecycle. The version-7 reports preserve
the evidence; this document owns the architectural conclusion.

The external-HTML gate began from the Lince-owned host, frame assembly,
normalized input boundary and deterministic lifetime path established there;
defects were fixed in those owning seams rather than hidden inside the CEF
adapter.

The audit also found and removed a real CEF seam rather than hiding it. `cef`
151.8.0+151.3.24 exposes Linux accelerated DMA-BUF paint metadata and callbacks
without its optional `accelerated_osr` helper. That convenience helper resolves
WGPU 30.0.1 while the host graph is WGPU 29.0.4 and contains an automatic CPU
fallback. The laboratory therefore disables it. The CEF callback types still
compile, and every candidate profile now resolves only WGPU 29.0.4. The chosen
boundary is one small Lince-owned Linux importer that consumes CEF DMA-BUF
metadata through the host WGPU-29 Vulkan device. If GPU import is unavailable
it reaches a visible unavailable state rather than taking a CPU screenshot
path. A second application GPU context is no longer the default.

The laboratory proved that boundary on the owner's Linux/Wayland path with CEF
151.8.0+151.3.24. CEF's stable API version must be selected before the App is
created; leaving it at the wrapper's unselected value fails at process startup.
The accepted runtime uses Alloy runtime style, `ozone-platform=wayland` and
ANGLE's `gl-egl` backend. It does not disable web security, certificate checks,
the sandbox or process isolation. CEF produces single-plane BGRA DMA-BUFs; the
Lince adapter validates the plane, dimensions, stride, allocation, format and
DRM modifier, duplicates the callback-owned fd, imports it through Vulkan,
copies it into Lince-owned device memory before the callback returns, fences
that copy, destroys the temporary import and composes the owned image into the
WGPU swapchain. The current 12-second joined report observed 711
imported/copy-complete installed frames and two Website frames, with no CPU paint callback and no
framebuffer readback. CEF provides no separate sync-fd in this callback shape;
the callback readiness contract delimits producer completion and the host
fence delimits Lince's copy and fd lifetime. This is an explicit GPU copy, not
a zero-copy claim.

The fixture also found a content-boundary defect before it became an adapter
workaround: CEF's stream helper expects a bare MIME type. Advertising
`text/html; charset=utf-8` made Chromium display the installed source as a
document, so no script or bridge ran. `text/html` plus the document's UTF-8
metadata executes the genuine installed page. The installed and Website Sands
now use separate CEF request contexts. The installed custom origin has a
persistent profile and its title reported storage available with a monotonically
persisted sequence across process runs; the Website diagnostic uses an
ephemeral profile. Both exercised ordinary HTTPS requests. Only the installed
main-frame origin receives the versioned V8 bridge; a granted Protein request
and a deliberately unknown operation produced two allowed and one fail-closed
decision, while the Website reported the bridge absent. An undeclared media
request and a trusted-input popup attempt were refused. Browser close callbacks
completed for both surfaces before the automated report was written.

`mise run interface-lab-cef` is the human fixture and
`mise run interface-lab-cef-report` is its bounded machine-readable run. The
runtime copies CEF's authoritative `LICENSE.txt` and 19 MB Chromium
`CREDITS.html` into its notice evidence and refuses startup if either is
missing. The joined evidence closes the Linux accelerated surface, authority,
storage/network/media, CPU-fallback, notice, off-camera and orderly-lifetime
baseline. A deliberate renderer crash recovered, and an induced host-device
rebuild completed in 45.812 ms. Count runs with 0, 1, 4 and 12 admitted CEF
Sands all passed on Wayland; their frame p95 values were respectively 9.454,
9.878, 11.772 and 11.578 ms. Off-camera CEF copied no frames during its measured
interval while its bridge advanced four events, so presentation was culled
without suspending behavior.

The semantic and spatial gate also closed with renderer-independent graph and
projection fixtures, current Protein output, token diffs, Areas, fixed-step
physics comparison and camera invariance. Its 10,000-instance run measured the
specialized field solver at 3.648 ms p95 and the bounded Avian comparison at
1.077 ms p95. The joined runtime runs all 1,000 eligible bodies at 120 Hz,
carries rather than discards fixed-step backlog, and reached zero pending steps
at every accepted report boundary.

Big Space and physics are deliberately separate experiments. The laboratory proves one
high-precision parent frame, a camera-local render frame and one local physics
island near the floating origin; it does not claim that Avian bodies can move
unchanged across planetary grids or multiple floating origins. That broader
integration belongs to v2 research.

##### Host interfaces exercised by the prototype

These are narrow runtime seams, not a universal lowest-common-denominator
engine API and not yet persisted schema names:

| Seam | Minimum information crossing it |
| --- | --- |
| Sand semantic projection | Stable definition/instance/child ids, composition, resolved tokens, typed ports, Behavior, capabilities, state-plane snapshot and bounded semantic diff |
| Renderer projection manifest | Projection key, required renderer capabilities, presentation assets and compatible adapters without runtime handles or a claim that one backend is the Sand's meaning |
| Frame participant | Prepare, fixed-step update, extract/damage, encode, compose and deterministic teardown; the coordinator owns ordering and deadlines |
| GPU surface | Device-compatible texture, size, color/alpha space, damage, transform, clip, z-order, synchronization and explicit lifetime; never a CPU screenshot |
| Input target | Hit result, inverse coordinate transform, pointer/keyboard/IME/clipboard/drag data, focus and capture; adapters return handled state rather than reading global input |
| Physics adapter | Stable body/group ids, simple shapes, force/sort/constraint fields, fixed-step input, transform output and awake/dirty reasons independent of camera visibility |
| Installed-HTML bridge | Versioned instance identity, declared typed inputs/outputs, bounded Box events, local state and capability-checked Action requests |
| Accessibility projection | Stable accessible ids, roles, names, bounds, focus, values and actions for visible or logically focused native scene elements, joined with CEF focus routing |
| Measurement sink | Timestamped CPU/GPU phases, simulation counts, dirty/upload bytes, memory/process counts, latency markers, backend/driver/build identity and assertion failures |

The prototype uses draft Rust fixture types for these seams and rejects unknown
versions and messages. Only the Sand, Protein, Action, Customization and stable
identity meanings already required by v1 may be promoted into the production
contract; framebuffer handles, Bevy entities, native UI nodes, CEF ids,
physics handles and laboratory scenario fields remain runtime-local.

##### Human-visible acceptance fixtures

The in-app laboratory panel exposes these eight switchable scenarios, live
metrics and failures, a deterministic seed and machine-readable report export:

1. **Composition and character.** Render a standalone Button Sand and the same
   definition inside a locked Record-card Castle with text, panel and dropdown
   children. Read one actual current Protein stream, show its input shape,
   field-to-port arrows, one
   unbound field, a `record-clicked` event route, Why-is-it-here, edit/focus/
   invalid/empty states, light and dark themes and one instance override.
   Retained native chrome and world Sands must remain visually crisp and token-equivalent
   at 1.0, 1.25, 1.5 and 2.0 scale factors. Live-change a global palette,
   density and radius token and prove inheritance and the local override update
   without recreating a Sand or CEF browser.
2. **Joined compositor.** Place retained native Sands and continuously animated
   world material below Lince editing chrome, one installed CEF Sand and one
   Website Sand. Move, scale, clip, overlap, focus and reorder all surfaces.
   Resize repeatedly and exercise pointer capture, keyboard traversal, IME,
   clipboard, drag/drop, browser popup and clean teardown.
3. **External authority.** Give the installed Sand a mapped Protein input,
   typed Box input/output, local storage, controlled HTTPS request and one
   visibly granted Action request. Give the Website normal browsing, network
   and origin storage but prove every installed-Sand handshake and Lince call
   fails closed. Include animation, media and a loopback WebRTC workload.
4. **Box motion.** Use deterministic force, repel, sort, constraint and weak
   centre fields with ordinary Sands, locked groups and collisions. Let one
   mutation field preview and request a typed Action against laboratory data.
   The load
   ladder is 200 visible interactive Sands, 1,000 continuously eligible moving
   bodies and 10,000 resident lightweight nodes/connections, with a 100,000
   static/indexed-node stretch case. Exercise local drag, dense collision,
   global force, group movement and continuous motion.
5. **Camera invariance.** Run the identical seeded simulation, Protein/event/
   Action trace, timers, media and CEF work on-camera and off-camera. Only
   extraction, paint and composition counts may change. Settling based on
   physical state is allowed; visibility-based sleep, suspension, unloading,
   rate reduction or Behavior change is a failure.
6. **Coordinate and specialised-pass seam.** Render the 2D desk beside a small
   3D height field or scene, cross one high-precision cell/origin boundary,
   place a CEF surface in that scene and load one open scene/geometry
   interchange artifact, for example glTF. Produce one replaceable collision
   or distance-field proxy and send one picked interaction through a typed Sand
   event. The required fixture uses the Lince parent/local-frame contract;
   Big Space may run as a comparison but is not required to pass or remain a
   dependency. This proves representation and adapter boundaries, not a globe
   or scene editor.
7. **Definition/replay and browser parity.** Save and replay the normalized
   Button/Record-card graph and a short prototype-local operation trace, then
   render the same definition through the Maud/HTML/JavaScript path, compare
   identities, ports and behavior fixtures, and prove the read-only Facade
   projection omits Action and editor authority. The replay format is laboratory
   evidence, not the production Box document.
8. **Accessibility and recovery.** Inspect the composed tree with Linux AT-SPI
   tooling and a screen reader, operate the scenarios without a pointer, then
   exercise GPU device loss, CEF renderer loss, malformed bridge messages and
   restart without leaked processes or invisible focus.

CEF is a heavyweight renderer with an explicit admission budget, not the node
type used for thousands of markers. The laboratory measures 0, 1, 4 and 12 live CEF Sands.
V1 guarantees that every admitted instance stays functionally active when
off-camera; it does not promise an unbounded browser count. If 12 exceeds the
machine budget, the report defines the honest resource estimate and admission
surface rather than silently freezing older pages.

##### Reproducible benchmark contract

Every performance run records the git revision, dependency locks, release
profile, scenario/seed, backend, adapter/driver, CPU/GPU/RAM, power mode,
resolution/scale, visible/resident/awake counts and CEF count. It warms for 30
seconds, samples for 120 seconds, repeats at least three times and exports raw
samples plus p50/p95/p99 summaries. Shader and pipeline compilation happens in
warm-up or is reported as a separate cold-start measurement; it is not hidden
inside an average.

Each benchmark scenario also versions its workload morphology: Sand pixel
size, text and glyph density, connection degree, body shape and size
distribution, moving/settled ratio, Area coverage, local/global dirty set, CEF
surface dimensions and content workload, media state and deterministic input
script. Separate scaling curves identify the saturation point of the empty
shell, each adapter, the physics candidates and the joined workload before the
mixed hard gate is interpreted. A count without those facts is not comparable
evidence.

Correctness uses `cargo check` across the affected workspace targets with
warnings denied, plus focused unit/integration tests. Performance runs execute
the instrumented release binary directly (for example through `cargo run
--release`); they do not substitute debug results or a bare `cargo build` for a
human-runnable scenario.

The required machine is the owner's Vostro 3150 with an 11th-generation Intel
Core i7, Iris Xe integrated graphics, 16 GB RAM and NixOS. Native display
resolution is the hard gate. A 120 Hz presentation target remains a recorded
stretch gate. Physical 4K validation waits for matching hardware and is not a
current gate; the renderer still must use dynamic surface size, device scale,
clipping, text rasterization, and quality selection so no fixed-resolution
assumption creates a known 4K ceiling.
An empty world-only pass and the rejected GPUI-owned experiment remain
comparison evidence; the accepted Linux desktop itself is the joined native
runtime, so Tauri is no longer a Linux baseline or fallback.

| Hard gate at native resolution | Acceptance threshold |
| --- | --- |
| Mixed Box load: 200 visible interactive Sands, 1,000 continuously eligible bodies, 10,000 resident light nodes | After warm-up, p95 frame time at or below 16.67 ms, p99 at or below 25 ms, and no more than one unexplained frame above 50 ms in a 120-second steady sample |
| Direct manipulation latency | Pointer/keyboard event to presented result p95 at or below 33.4 ms and p99 at or below 50 ms |
| Fixed simulation | 120 Hz remains independent of render rate; the representative 1,000-body step has p95 CPU time at or below 8 ms, drops no elapsed time and reports any carried backlog |
| Partial work | A local drag updates the affected island and damaged buffers/surfaces rather than uploading or laying out the complete 10,000-node world; global-field work is separately identified |
| Protein and configuration diffs | Changing 1% of a 10,000-row generated Protein fixture updates only affected bindings/instances; a global token change reaches the 200 visible Sands without CEF reload; each has p95 source-diff-to-present latency at or below 50 ms |
| GPU composition | Zero per-frame framebuffer readback; texture copies, synchronization waits and upload bytes are measured and bounded by changed content |
| Camera invariance | On/off-camera semantic event, Action, timer and fixed-step traces are identical for the same seed and inputs; only presentation traces differ |
| CEF | The 0/1/4 cases retain the frame and input-latency gates while all admitted pages remain live; the 12 case is a required characterization and resource-budget decision |
| Lifetime | After ten create/destroy/reset cycles, all browser processes, GPU objects, routes and subscriptions return to their expected counts, and RSS/VRAM shows no unexplained monotonic growth above a 5% post-warm plateau |
| Startup and recovery | Warm start reaches an interactive native window within 2 seconds and cold start within 4 seconds; lazy first-CEF readiness is reported separately; device/renderer recovery restores visible state and focus without semantic replay or data loss |
| Visual/accessibility/security | No clipping, blur or focus discontinuity at tested scales; keyboard, IME and AT-SPI/Orca paths complete the fixture; malformed/unknown bridge operations and all Website bridge attempts fail closed |

An average frame rate alone cannot pass the laboratory. The report includes hitch counts,
input-to-present latency, CPU and GPU phase time, fixed-step debt, dirty/awake
sets, buffer and texture traffic, process/RSS/VRAM growth, initialization time,
packaging size and power observations. Instrumentation overhead is measured
with the overlay hidden and shown.

Instrumented event-to-submit and compositor presentation timestamps are
reported separately. The accepted Wayland path does not expose a trustworthy
physical-display timestamp through WGPU, so the report calls its input-to-
present-call measurement a lower bound and never relabels it as what the person
saw. Camera-based input-to-visible validation remains a hardware/display
quality check; Lince will not add an X11 probe or fallback to manufacture that
number.

##### Acceptance and handoff

Plan A passed through the joined scenario—not separate native UI, Bevy or CEF
demos—across ownership, correctness, visual character, accessibility,
security and native-resolution hard gates. No GPUI production fork was
retained. Plan B remains a browser and Facade projection strategy, not a Linux
desktop fallback.

The first post-laboratory kernel now uses the same joined scenario as its human
and machine surface. `mise run interface-lab-joined` exposes F1 workspace
palette, F2 group density, F3 instance radius, F4 partial theme and F7 mode;
F8 exports the report. The 2026-08-27 release run resolved all 91 contract-v1
tokens, retained provenance from the seven scopes, updated native nodes,
borders, background and text, and sent two declaration updates to the
Installed CEF root. Its load count remained one before and after the probe.
Website still had no bridge, and the overall gate passed on Wayland/Vulkan at
1366×740 with frame p95 10.668 ms, frame p99 12.524 ms, fixed-step p95
1.107 ms and zero fixed-step backlog. Its page title acknowledged the final
workspace accent as `style v1:#3730A3`. This short run proves the style seam;
it does not replace the required long C5 visual and latency repetitions.

The second post-laboratory kernel uses the same joined surface for Sand schema
and ABI version 1. Its retained Gallery presents 19 primitive definitions,
switches native/HTML focus with F5, traverses with Tab, activates through
keyboard, pointer and AccessKit, and cycles its state matrix with F9. Installed
CEF serves the content-addressed package's accessible HTML, CSS and native
JavaScript modules. Its package admission resolves only declared relative
static module imports. At runtime its strict decoder refused one mount with an
unknown field, then accepted the valid Protein-shaped mount and emitted 15
granted `record-clicked` events. The Website made zero bridge calls. One popup
was refused, off-camera Behavior advanced, and no CPU paint occurred. The
current 2026-08-28 short release gate passed at 1366×740 with frame p95
10.265 ms and 120 Hz fixed-step p95 1.078 ms. The exact report, generated JSON Schemas and
accepted and stale fixtures live under
`target/interface-laboratory/sand-contract/`.

The third post-laboratory kernel adds composition artifact and catalog schema
version 1 to that joined surface. F10 opens the recursive workbench;
Tab/Shift-Tab and Enter operate nested activation, lock, instance
override/reset, transactional shared edit, invalid-edit refusal, save as
definition, save/reopen, fork and teardown/remount. Its visible tree contains a
standalone Button, the same definition inside `video-call`, that compound
inside `video-call-room`, and native and Installed HTML room projections.
Protein `READ`, Sand `EVENT` and Action `WRITE` arrows remain distinct.

The release parity report proved that Rust construction, recursive Maud output
and the workbench consume one 21-definition package. The joined run finished
with five placements, 17 recursive nodes, nine active and 43 retired Behavior
handles, one successful instance of every mutation and one refused invalid
publication. Installed CEF cloned two instances with eight collision-free
scoped DOM ids, repaired label/ARIA references and reported one explicit
teardown. The active catalog contained 23 definitions after save and fork;
the source package hash was
`sha256:b90813aac80ab7c0fb2da5ba2c198c142a252a7ca813ca3baa2025e3fc1bcafd`.

The production desktop release binary completed a cold build when the offline
desktop environment reused the already validated CEF distribution. After the
Interface material moved into `Lince.lingua`, the bundled Instinct consumer was
updated to mint omitted declaration UIDs before projection, matching Lingua's
current contract without writing owner files. A 2026-08-29 release run then
reached the current corpus and stopped before the window because the removed
First Steps root leaves six `@first-steps` references unresolved across Karma,
Lince and Ontology. `lingua check anicca` reports the same references. This is
an independent production-entry gate; no Markdown or interface adapter repairs
or bypasses owner Records.

The post-promotion joined release report passed independently at 1920×1052
with source fingerprint
`73f4855f4b38cc95ec8550cf1480a009f2680d8472a566168a19e622503b487f`.
Its 200 visible Sands, 1,000 continuously eligible 120 Hz bodies, 10,000
resident nodes and two CEF surfaces measured 10.958 ms frame p95, 12.448 ms
p99, 7.653 ms CPU-frame p95, 1.172 ms fixed-step p95 and zero backlog. Both
Installed and Website surfaces loaded through accelerated DMA-BUF copies with
zero CPU paints; the Website retained no bridge authority.

The exit artifact contains exact dependency and license inventory, architecture
decision records, raw and summarized benchmark results, known platform limits,
accepted adapter contracts and a disposition for every laboratory module. Work
now advances to Configuration/external authoring, official-Sand migration and
C5 completion gate; then Box foundations, Protein result
templates, Areas and Actions, persistence, external HTML hardening and the
public Live Facade. No later cluster works around a failed foundation seam.
