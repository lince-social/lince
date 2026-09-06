# Native-first v1 delivery and build lanes

Part A — Dogfeeding delivers a real CEF-free native Interface client and new Sands connected live to a private Organ on a local Linux machine or VPS. The company is the acceptance case, not a separate application architecture. Neither the joined application with zero browser panes nor legacy desktop Sands meet that delivery. The lince-desktop wrapper is not required. The same shared native host serves product and acceptance runs. CEF stays behind cef-runtime until the end of v1. [Backend Part A](../backend-part-A.md) owns Role/Protein/property authority, private operation and remote proof; [Interface Part A](plans/part-a.md) owns the controls.

This is the delivery rule for the whole interface corpus. Earlier joined/CEF reports remain evidence for the revisions they measured. They neither certify the new native-only executable nor impose a CEF gate on Part A. Research documents describe possibilities, not additional prerequisites. The graph-work compilation notes describe the current joined dependency graph; the native default below is a planned change, and its actual build graph must be proved before it is called ready.

## One optional adapter, with no accidental entry path

`lince-interface/native-runtime` owns the common native host, domain transport, retained rendering, input, accessibility, configuration and native diagnostics. `lince-interface/cef-runtime` adds the CEF adapter and its browser-dependent registration, dependencies, subprocess handling and assets. `joined-runtime` can remain the explicit combination of native and CEF for the late-v1 lane; it is never selected by a normal Part A dependency or default command.

The required client product uses native-runtime; its company connection does not start a local company database. The retained desktop wrapper also follows the native/optional-CEF boundary when built, but is not Dogfeeding's required executable. Feature selection controls registration, imports, restored children and subprocess handling: a binary that still links or prepares CEF is not CEF-free.

Native launch and packaging must not fetch or copy CEF, set up its payload/cache/profile, require `CEF_PATH`, resolve its native libraries or start its processes. CEF declarations, lockfile entries, source, licenses and historical reports may remain in the repository. Audit the resolved target/feature graph and installed runtime closure, not the mere presence of a name in `Cargo.lock`. Shared graphics or system libraries are not evidence of CEF by themselves.

The current code has not reached this boundary: `crates/desktop/Cargo.toml` selects `joined-runtime`; `lib.rs` gates native modules and `run_native_interface` behind joined/CEF features; the common host is mixed into `cef_diagnostic.rs`; and the Nix desktop and interface shells prepare CEF. Bootstrap extracts the common native host and makes the default feature, shell, task and package path consistent. It does not create a second event loop or throw away the existing renderer.

## What is available in Part A

The source catalog still has 25 roots. Dogfeeding qualifies the reusable knowledge/work subset below, not all twenty non-browser roots. The administrator configures vocabulary, Roles, Protein policies and views manually; company starters and automatic board wiring are not gates. Other native workflows remain in [native follow-through](plans/native-follow-through.md); the five browser-backed roots remain in the final CEF lane. A native projection label alone does not prove an implementation.

| Dogfeeding surface | Implementation boundary |
| --- | --- |
| Edit controls, Zoom, Configuration | Native controls, stable editing, scopes, persistence and recovery |
| Record, Record Editor, Conversation | One native body editor, typed fields, real reads and acknowledged Actions |
| Table, Todo, Kanban | Shared native collection reads, row identity, configured filters/lanes and bulk failures |
| Vocabulary, Organ, Permissions | Reusable Concept/Assertion and People/Role controls; Protein-selected Record access, property-write grants and effective-access preview |
| Organ live connection and Record discussion | Authenticated remote native client and thread/message controls; no required project, calls or separate Communication Castle |
| Activity, Trash/Restore and protected operator information | Authorized history, recovery and company health; reuse ordinary Sands rather than requiring Archive or AI shells |

| Late-v1 CEF lane | What is retained but unavailable in Part A |
| --- | --- |
| `lince-website` | Embedded remote Websites and their CEF request contexts |
| `document-viewer` | The current PDF/EPUB/browser document renderer |
| `terminal` | The current browser Ghostty renderer and its task-bound pane |
| `freedoom-portal` | The current browser game renderer and controls |
| `lince-logo-led` | The current browser animation implementation |
| Installed HTML and mixed native/HTML Castles | Runtime HTML/JavaScript execution, browser previews and CEF-dependent portions of packages |

The native Record Markdown editor, ordinary supported images, file metadata/transfer controls, terminal-domain APIs and pure package/hash validation are not CEF features. Creating a new PDF, terminal, game or browser engine solely to keep the old catalog count in Part A is not planned. A separate future native implementation can be proposed explicitly; it is not a builder's automatic substitute for a disabled adapter.

Normal pickers and launch recipes offer available native projections only. Existing references to unavailable CEF children retain their ids, configuration and data bindings and show a non-executing unavailable placeholder when inspected or restored. They are not silently dropped, remapped or initialized. Native siblings may remain usable, but routes that require the unavailable child are explicitly unavailable and cannot acquire its authority. Pure metadata inspection never evaluates package code.

Exporting HTML or displaying a public Facade in somebody's browser does not itself require an embedded CEF runtime. Those keep their existing later delivery stages; deferring CEF neither cancels them nor makes a general browser client part of Part A. Building new file transport, rooms or a Fiote harness also remains a separate domain lane. Fiote's full session surface cannot claim its terminal pane is ready merely because native C5 passed.

## Which tool does which work

| Work | Lane | Evidence |
| --- | --- | --- |
| Reading, editing, formatting and static source checks | Any authorized worker; no compiler needed | Reviewed diff and narrow static checks |
| Native Rust type checks and deterministic unit/integration tests | Scoped Corgi in reusable builder checkouts | Actual native feature graph, nonzero test count and content/revision identity |
| Native Interface client and headless server package/production-entry checks | One warm verification checkout; Cargo/Nix, or a proved Corgi desktop root for its Rust portion | Real CEF-free desktop and no CEF payload requirement |
| GPU/input/accessibility, local server and lifecycle witnesses | Inspector only; a prepared native executable | Fresh execution against isolated fixtures, not cached test success |
| Frame timing and other machine measurements | Inspector's exclusive machine lease; canonical native release profile | Fresh warm-up and samples with build tool/toolchain, source and workload recorded |
| Anything enabling `cef-runtime` or `joined-runtime` | Late-v1 Cargo/CEF lane | Its own fresh security, integration, packaging and performance proof |

Every Corgi shell uses `CORGI_STORE=/home/user/git/lince-social/.corgi` on disk outside the checkouts and `CORGI_SYSTEM_READS=/nix/store:/run/current-system/sw/share/nix-ld`. The starting native check is `corgi check --root interface -p lince-interface --no-default-features --features native-runtime`. Named tests use the same root/features and a concrete `--test` target. Verify the installed CLI in bootstrap; a missing tool, unsupported option or absent test fails rather than silently choosing a weaker check.

Bootstrap adds and proves Corgi roots for the actual native client and headless server feature graphs before advertising commands for them. The existing interface root proves only its selected Rust target; it does not prove the server or remote connection. Until supported, product/package proofs use the one warm Cargo/Nix verification lane. No Dogfeeding proof requires launching lince-desktop or enabling joined-runtime.

Cargo is also allowed for a native proof Corgi cannot correctly execute. The planner records the exact reason and command and keeps that work in one warm verification checkout. It is not a reason to enable CEF, use `--all-features`, run an unscoped workspace build or create more Cargo targets in every checkout. There is no requirement to run every unit test under both compilers on every edit. The integrated proof uses the declared native lane and re-proves the accumulated affected work; final acceptance includes the canonical production package.

Use `cargo check`, never a bare `cargo build` as a proof. `cargo test` and release `cargo run` are permitted when the proof requires execution. Prepare the measured release executable before sampling and then run it directly. Cached deterministic test results may be reused only for the identical tested content/configuration; live UI, network-fixture, lifecycle and timing evidence always comes from a fresh run. If Corgi cannot guarantee an uncached runtime invocation, use the prepared Cargo executable for that witness.

Independent workers use separate reusable checkouts. Start with one compiler; the prior machine evidence supports three simultaneous compiler jobs including integration. Remeasure native check, test/link and release workloads with operating headroom and throughput before changing that budget. A fourth lightweight job is possible only if fresh evidence and actual session capacity support it. Agent slots, source checkouts and machine permits are different limits. A completed builder may submit a fixed candidate and reuse its checkout for another independent certified-ready node while verification runs; the [graph](graph.md#reusable-workers-and-the-review-queue) owns the bounded queue and permit priority.

During measurements, every other run-owned compiler, test and app is stopped or drained and acknowledged; the inspector verifies this before sampling and other local tool work remains paused until release. Unexpected outside load invalidates the run, and unrelated user processes are never killed to make room. Current agent capacity allows two builders plus one combined verification agent and the planner; more editors do not raise the machine budget.

## Native acceptance is a new report, not an edited old result

Part A keeps the laboratory's native frame, input, fixed-step, lifecycle, authority and accessibility thresholds. It runs with zero CEF processes and surfaces, using a native report profile distinct from the historical joined profile. Retain the 200-visible/1,000-active/10,000-resident regression workload where its native mechanics apply, and separately exercise ordinary stationary editing and real data updates. Automatic movement remains off in the everyday workspace.

The CEF count matrix, Installed-HTML live theme parity, browser-process recovery and browser security probes are **not applicable to Part A**, not passed or deleted. Their requirements are preserved in the late CEF plan. Native GPU recovery, native client and headless server starts, AccessKit/Orca, input, authenticated remote company Actions and measured resource use remain mandatory. Three 30-second warm-ups and 120-second samples still apply to the main benchmark. A new source fingerprint, enabled-feature list, machine/workload record and uncontaminated raw samples are required.
