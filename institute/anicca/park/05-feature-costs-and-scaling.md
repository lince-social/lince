# 5. Scale every feature through shared interface families

[Organization and execution order](Park.md). Targets are in [benchmarks](08-benchmarks.md); interaction coverage is in [workflows](09-workflows.md).

Cover the active feature set through shared cost families. P124 makes the coverage concrete and extends the seed inventory in 09. Then choose experiments by measured cost, not by the number of Castles they touch. Normal functionality while hidden is separate from presentation demand. Existing plugin registration, asynchronous workers and cache presence alone prove neither waste nor efficiency.

These changes reuse shared controls, row identities, assets, source deadlines and bounded workers. Each task names one actual costly path before coding and includes input/visible-output checks; do not implement speculative per-feature infrastructure merely to fill the inventory.

- [ ] **P124 — Complete the source-to-workflow inventory for active interface features.**

    **Why:** a rendering optimization cannot be called holistic while unexamined feature families keep doing work or lose behavior. **Scope:** reconcile the explicit family table in 09 against native/app.rs, the Laboratory catalogue, actions/openers and current shipping feature flags. For each reachable feature record its input/control family, data source, presentation update, worker/timer/subscription, persistence, hidden semantics and existing correctness/performance selector. Include feature-gated active Sands; mark absent coverage and optional external services separately. **Close with:** a source-stamped inventory with every active registration assigned and one useful representative per distinct cost path, plus a ranked shortlist of actual unexplained work. This is an executable ownership/coverage inventory, not a requirement to benchmark every label or every permutation. P86 separately ranks removable bloat.

- [ ] **P77 — Gate presentation work for one feature family with no live views.**

    **Why:** registering many UI plugins can leave work running when that presentation has no consumers. **Scope:** measure one absent-view family in native/app.rs and its systems, then gate only its visual derivation or UI subscriptions through explicit demand. Registration alone is not proof of waste. **Verify:** zero unnecessary idle work, first open, close/reopen, multiple views and backend events while hidden. Preserve notifications, commands, recording and other semantic activity; avoid a second scheduler/cache per Castle.

- [ ] **P49 — Virtualize one measured large Record-result list.**

    **Why:** layout/entity cost should follow visible rows rather than all loaded Records. **Scope:** visible rows plus a bounded margin, stable UID identity, variable-height bookkeeping and retained edit/focus state. **Verify:** scroll/reveal, sorting/filtering, live inserts/deletes, focused or dirty rows, viewport resizing and source replacement. Row virtualization must not delete Records or lose semantic subscriptions. Reuse appropriate existing scroll infrastructure; prove smaller presentation work under the real large-list workload before generalizing.

- [ ] **P125 — Retain one costly settings or property panel across ordinary edits.**

    **Why:** rebuilding a panel can churn entities, text/layout, focus and style for each slider tick or response. **Scope:** identify one P66/P124-proven panel rebuild and update its existing controls by stable model identity, rebuilding only changed structure. Reuse native controls, dropdowns, accordion sections and item/property families; keep pending drafts distinct from refreshed model values. **Verify:** typing/selection/IME, slider drag, expand/collapse, search, validation errors, source replacement, accessibility and close/reopen. Count despawns/spawns, layout/style work and input latency, including small panels. Do not retain invalid control actions or hide backend errors for the sake of stable entities. **One demonstrated panel first;** generalize only its genuinely shared mechanism.

- [ ] **P128 — Keep terminal or Command output from monopolizing interface work.**

    **Why:** a fast local output stream can make unrelated text input late through repeated decoding, row updates, history scans or wakes. **Scope:** select the measured terminal/Command publication bottleneck; process output in bounded ordered batches with incremental retained presentation and producer-driven readiness. Keep command state/history semantics and full data retention according to current limits; permit presentation to skip obsolete intermediate paints, not output bytes. **Verify:** Unicode, long lines, scrolling, interactive stdin, exit/status, cancellation, multiple instances and another editor under the same stream. Measure input age, queue memory and CPU plus final output equality. Command history notifications already exist; do not replace them with polling or a second terminal implementation.

- [ ] **P127 — Remove one blocking preparation step from opening a native view.**

    **Why:** opening a document, file, image, source view or full Record should not freeze typing elsewhere while it reads, parses or decodes. **Scope:** choose one measured cold/warm opener from the inventory, reuse its existing worker where present, and move the remaining expensive pure preparation to bounded revision-stamped work. Publish a stable truthful loading/error shell and cancel or ignore obsolete results after close/source replacement. Include initialization, font/asset setup and copying in the comparison. **Verify:** first usable feedback, final content, close/reopen, simultaneous opens, failures, data ownership and normal input on both outputs. Do not add another worker thread per view or treat a quick empty shell as complete open latency. **Existing document/file workers are the baseline, not missing features.**

- [ ] **P129 — Tie one media producer to real consumer presentation demand.**

    **Why:** decoded video, camera, shader, animation or audio-visualization activity can consume work after its visual consumers disappear, or multiply work for several views. **Scope:** choose one measured producer and separate necessary semantic/audio/recording progress from skippable visual decode/upload/paint. Share compatible ready content among consumers, account for every output/overview/capture demand, and use actual readiness/deadlines. **Verify:** hide/reveal, multi-view, paused/ended sources, recording while hidden, capture, errors and source retirement; compare idle/activity cost and responsiveness elsewhere. Do not stop recording/audio or change playback speed/quality to obtain a win. Distinguish display frame pacing from the media clock. **Needs:** P124 plus a real residual cost; existing worker/cadence mechanisms remain in place.

- [ ] **P130 — Make one Area or physics recomputation depend on changed participants.**

    **Why:** moving one item or evaluating a rule may scan all Areas/relations and repeat derived styles/layout every tick. **Scope:** use the measured Area/physics path to retain only valid unchanged membership/force/effect derivation, keyed by source, workspace, shape/reach, transform and participant revisions. Reuse the P50 index only if its maintenance earns the cost; integrate actual physics deadlines. **Verify:** enter/leave, overlapping Areas, immunity/enabled state, source changes, pinned/grouped items, 2D/3D navigation and deterministic effects. Keep semantic rules/automation active where required and do not cap participant counts to improve the benchmark. Compare moving and quiet scenes, index/dirty maintenance, CPU and local input tails. **One concrete expensive derivation, not a physics-engine rewrite by assumption.**

- [ ] **P50 — Compare an incremental spatial index with one expensive canvas scan.**

    **Why:** a local hit/visibility/relation query may currently scale with the entire canvas. **Scope:** one P66-measured query; include insertion, movement, resizing, removal and origin-shift maintenance. **Verify:** indexed versus scan answers at large coordinates, overlap/clipping, multiple workspaces and outputs, plus total CPU and memory for small and moving scenes. Retain the scan when maintenance costs more. Index candidates are not cached authorization results or a proof of full occlusion.

- [ ] **P85 — Reuse one unchanged pointer-geometry derivation safely.**

    **Why:** rebuilding large input snapshots may delay every native frame. **Scope:** retain only measured pure geometry, keyed by real node/clip/transform/output changes; read action versions, activity, modal and current owner eligibility at the required boundary. **Verify:** stacked/nonblocking recipients, rounded clips, moves, replaced actions, reparenting, focus/IME/lock and queued presses. Include small controls and dense scenes. Previous camera/query and full projection-map experiments were rejected; a distinct measured invalidation strategy is required before revisiting this area.

- [ ] **P84 — Apply the shared list approach to one other costly native list.**

    **Why:** the same excessive materialization may affect the store, overview or settings lists. **Scope:** choose exactly one list demonstrated costly after P49 or an equivalent existing abstraction is assessed. Preserve keyboard navigation, search, selection and the ability to reveal off-screen controls. **Verify:** node/layout cost versus visible rows, variable content and narrow/wide viewports. Do not virtualize a small list for uniformity or create another competing list framework; name the selected list before implementation.

- [ ] **P131 — Publish accessibility changes incrementally for a stable interface.**

    **Why:** full accessibility-tree/text/geometry reconstruction may amplify ordinary input if it is currently on the hot path. **Scope:** first attribute the cost, then retain stable node identity and publish the affected text/focus/state/geometry subtree through the existing accessibility family. An absent accessibility consumer may avoid presentation-only export if the API permits it, without disabling keyboard/focus semantics or future attachment. **Verify:** attach/detach, typing/selection, scroll/reparent, focus changes, hidden/modal views, password protection and stale references. Measure dirty bookkeeping and full versus incremental publication. **Conditional:** close with a measured rejection if the current path already costs negligibly or demand gating is already correct; no blanket disabling of accessibility.

- [ ] **P132 — Retain one unchanged capture-contributor derivation.**

    **Why:** capture ownership/source normalization may repeat large scene walks even while content and capture demand are unchanged. **Scope:** select P66/P124's measured derivation in native capture/topology, reuse explicit owner/UID/placement and source revisions, and separate stable contributor structure from changing media pixels. Existing shared capture extraction is the baseline. **Verify:** sibling metadata/image aliases, private/public changes, window/source replacement, removal, lock/output changes and simultaneous consumers; invalid owner data remains conservative. Include cache validation/retirement, memory and actual foreground cost. Never cache an access decision beyond its valid epoch or expose a previously valid private frame while revalidating. **Conditional on:** a remaining expensive walk, separate from GPU pass removal P55.

- [ ] **P133 — Share one repeatedly created immutable asset family.**

    **Why:** opening many items may recreate identical font, icon, markup image or pipeline input assets and make both startup and steady work more expensive. **Scope:** from P124/P86 evidence, choose one immutable value family with identical lifetime/usage semantics and reuse its canonical existing asset handle or interning path. Account for load/decode/upload and reference retention, not just allocation counts. **Verify:** multiple instances, live replacement, theme/density changes, close/reopen, device retirement where relevant, license/credits and memory returning to its documented bound. Do not share mutable editor state or private content across owners. **Keep only:** demonstrated startup/input/memory benefit; a global unbounded asset cache is not the objective.
