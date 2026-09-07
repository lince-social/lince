# Bevy-native v1 delivery and build lanes

Part A — Dogfeeding delivers a real native Interface client with no embedded browser, and new Sands connected live to a private Organ on a local Linux machine or VPS. The company is the acceptance case, not a separate application architecture. Neither the joined application with zero browser panes nor legacy desktop Sands meet that delivery. The lince-desktop wrapper is not required. The same Bevy application and Lince plugins serve product and acceptance runs. The old native host may be rewritten; it is not a framework to preserve. [Backend Part A](../backend-part-A.md) owns Role/Protein/property authority, private operation and remote proof; [Interface Part A](plans/part-a.md) owns the controls.

This is the delivery rule for the whole interface corpus. Earlier joined reports remain evidence for the revisions they measured. They neither certify the new native-only executable nor impose a browser gate on Part A. Research documents describe possibilities, not additional prerequisites. The native default below is a planned change, and its actual build graph must be proved before it is called ready.

## No embedded browser

Lince does not embed a browser. CEF was removed from the repository on 2026-09-07: the adapter, its features, its dependencies, its Nix runtime fetch, its diagnostics and its tasks are all gone, and nothing is retained behind a feature flag waiting for a later lane.

CEF was a bad idea and is not coming back. It made a Chromium tree a build input, put a second process family, a second GPU path and a second accessibility tree inside the product, and made every downstream feature wait on a lane that never arrived. An embedded browser is not a way to finish a surface; it is a way to defer finishing it.

So the rule for anything that used to depend on it: **it needs a way to run without embedding a browser in Lince.** Until such a way exists and is designed, that surface is unavailable; the replacement design is low-priority end-of-v1 work, not a hidden browser lane. Naming a native projection does not prove an implementation, and a new PDF, terminal, game or animation engine written only to keep the old catalog count is not planned either. What is planned is that each of these gets a real browserless design before it returns.

`lince-interface/native-runtime` builds one Bevy application with domain transport, Bevy UI/text/rendering/input/accessibility, Configuration and native diagnostics. New interface code uses Bevy directly; no renderer-neutral UI tree, generic world adapter or separate Winit/WGPU owner is required. Translate existing backend/data boundaries only where needed.

## Bevy base and scoped exceptions

The owner selected Bevy on 2026-09-07. Use first-party Bevy crates for ordinary UI, text, layout, scenes, lines/curves, picking, assets and animation. Keep a focused feature set rather than enabling every engine feature. The internal WGPU, Taffy and text dependencies remain Bevy's integration; direct Glyphon or Taffy paths are not retained by default.

Implement Lince behavior as Bevy plugins. A custom plugin, internal/external crate or pure WGPU pass is allowed for a named Lince need; it normally shares Bevy's lifecycle, device and final presentation. AccessKit types matching Bevy and narrow platform-service libraries are accepted supporting dependencies. Flair is selected for CSS styling over Bevy components, with its integration and idle costs to be proved. Avian 3D is selected for later collision/settling work under [the architecture](architecture.md#physics-application-rules-and-collision-solving); no general physics solver runs in the stationary Part A workspace.

Do not preserve prototype ABI/projection layers just to make future interface code renderer-independent. Existing interface code may be replaced wholesale while retaining user-visible behavior, data meaning and backend authority. No source implementation is claimed migrated by this documentation change.

Native launch and packaging must not fetch or copy a browser runtime, set up its payload/cache/profile, resolve its native libraries or start its processes. Audit the resolved target/feature graph and installed runtime closure, not the mere presence of a name in `Cargo.lock`. Shared graphics or system libraries are not evidence of an embedded browser by themselves.

The desktop binary on Linux is the native interface and nothing else: `lince-web` is not compiled into it, so no HTML asset, board route or template renderer reaches that build. A Cell server is a separate process — `lince --server` — and the interface reaches it over the transport socket.

## What is available in Part A

Dogfeeding qualifies the reusable knowledge/work subset below. The administrator configures vocabulary, Roles, Protein policies and views manually; company starters and automatic board wiring are not gates. Other native workflows remain in [native follow-through](plans/native-follow-through.md). A native projection label alone does not prove an implementation.

| Dogfeeding surface | Implementation boundary |
| --- | --- |
| Edit controls, Zoom, Configuration | Native controls, stable editing, scopes, persistence and recovery |
| Record, Record Editor, Conversation | One native body editor, typed fields, real reads and acknowledged Actions |
| Table, Todo, Kanban | Shared native collection reads, row identity, configured filters/lanes and bulk failures |
| Vocabulary, Organ, Permissions | Reusable Concept/Assertion and People/Role controls; Protein-selected Record access, property-write grants and effective-access preview |
| Organ live connection and Record discussion | Authenticated remote native client and thread/message controls; no required project, calls or separate Communication Castle |
| Activity, Trash/Restore and protected operator information | Authorized history, recovery and company health; reuse ordinary Sands rather than requiring Archive or AI shells |


| Surface with no browserless design yet | What it needs before it can return |
| --- | --- |
| `lince-website` | A way to show a remote Website without embedding a browser in Lince, or an explicit decision that Lince opens the system browser instead |
| `document-viewer` | End-of-v1 browserless PDF/EPUB interpretation; display and controls integrate with Bevy |
| `terminal` | End-of-v1 terminal emulation/PTY integration and a Bevy presentation for the task-bound pane |
| `freedoom-portal` | End-of-v1 native game integration with Bevy; any external game engine is a scoped exception |
| `lince-logo-led` | End-of-v1 animation using Bevy materials/animation by default |
| Installed HTML and mixed native/HTML Castles | A packaging and execution story that does not run HTML or JavaScript inside Lince |

Specialized content-engine selection, including video decoding, is low-priority work at the end of v1, after the ordinary native interface. It may use internal/external crates without adding another general UI runtime. This does not postpone the Record Markdown editor, Bevy-supported images/audio, file metadata/transfer controls, terminal-domain APIs or pure package/hash validation. It does not schedule untrusted executable installation or a sandbox: current extensions are registered editable components/effects and trusted Rust plugins; untrusted execution awaits a separate later decision.

Normal pickers and launch recipes offer available native projections only. Existing references to the unavailable children above retain their ids, configuration and data bindings and show a non-executing unavailable placeholder when inspected or restored. They are not silently dropped, remapped or initialized. Native siblings may remain usable, but routes that require the unavailable child are explicitly unavailable and cannot acquire its authority. Pure metadata inspection never evaluates package code.

Exporting HTML or displaying a public Facade in somebody's browser is a different thing entirely: it runs in the visitor's browser, not inside Lince, and nothing above constrains it. Building new file transport, rooms or a Fiote harness also remains a separate domain lane. Fiote's full session surface cannot claim its terminal pane is ready merely because native C5 passed.

## Which tool does which work

| Work | Lane | Evidence |
| --- | --- | --- |
| Reading, editing, formatting and static source checks | Current checkout | Reviewed diff and narrow static checks |
| Native Rust type checks and deterministic unit/integration tests | Focused Cargo commands in one reused target | The named affected target and nonzero focused tests |
| Native Interface client and headless server package/production-entry checks | Cargo/Nix production lane | A real client/server with no browser runtime in its closure |
| GPU/input/accessibility, local server and lifecycle witnesses | Prepared native executable | Fresh execution against isolated fixtures, not cached test success |
| Frame timing and other machine measurements | Quiet machine; canonical native release profile | Fresh warm-up and samples with toolchain, source and workload recorded |

Use one current checkout and one reused Cargo target. Start with the narrowest test that exercises the changed feature. A change to a shared contract runs the directly affected focused targets; it does not automatically rerun every backend or Interface test. Release acceptance separately runs the production client/server package and end-to-end journey.

Use `cargo check`, never a bare `cargo build`. Use `cargo test` when behavior must execute, and `cargo run` only for an actual application or release witness.

During measurements, stop other project compilers, tests and applications before sampling. Unexpected outside load invalidates the run, and unrelated user processes are never killed to make room.

## Minimum machine and resource baseline

The owner selected their current machine as the minimum target on 2026-09-07.
Read-only inspection of the working host found:

| Item | Observed baseline |
| --- | --- |
| CPU | Intel Core i7-1165G7, 4 cores / 8 logical CPUs |
| System memory | 16 GB class; Linux reports 16,116,788 KiB total, about 15.37 GiB |
| Graphics | Intel integrated GPU, PCI `8086:9A49`, Linux `i915` driver |
| System | NixOS 26.11; retain the Wayland/Vulkan acceptance target |

These are inventory facts, not fresh performance results. Each acceptance run
records the actual selected graphics adapter, driver/Mesa version, compositor,
window dimensions/scale, power mode, exact source and enabled features. Measure
at the owner's normal native resolution; do not silently lower resolution or
use a different GPU to pass. Account for graphics allocations and process
memory without pretending this integrated GPU has a separate fixed VRAM pool.
The machine's total RAM is not Lince's permitted allocation: retain bounded
caches, memory headroom and repeated-open/close checks, then record explicit
working-set limits from the real workloads. No benchmark is run by this plan edit.

## Native acceptance is a new report, not an edited old result

Part A earns new Bevy-native evidence against the applicable laboratory frame, input, fixed-step, lifecycle, authority and accessibility thresholds; old custom-host results do not certify Bevy UI or the Bevy application runner. It runs with zero browser processes and surfaces, using a native report profile distinct from the historical joined profile. Retain the 200-visible/1,000-active/10,000-resident regression workload where its native mechanics apply, and separately exercise ordinary stationary editing and real data updates. Automatic movement remains off in the everyday workspace.

The browser-count matrix, Installed-HTML live theme parity, browser-process recovery and browser security probes are **gone**, not passed and not pending. Whatever replaces a surface above brings its own acceptance evidence, designed for that implementation. Native GPU recovery, native client and headless server starts, AccessKit/Orca, input, authenticated remote company Actions and measured resource use remain mandatory. Three 30-second warm-ups and 120-second samples still apply to the main benchmark. A new source fingerprint, enabled-feature list, machine/workload record and uncontaminated raw samples are required.

Measure truly idle focused/unfocused and minimized windows, one active Sand among many static Sands, targeted Protein changes, and repeated open/close separately. Record CPU/GPU work, redraws/submissions, wakeups, input latency, RAM and VRAM after warm-up. A static workspace must not continuously render or run a 120 Hz solver; retained state does not mean zero memory, and change detection does not prove only changed nodes were visited. Wake Bevy for real deadlines and backend changes without tying backend liveness to camera visibility.
