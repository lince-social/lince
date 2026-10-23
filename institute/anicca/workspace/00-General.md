# Workspace — stage guide

Build a useful native workspace from precise model editing through sparse world navigation and collaboration, stabilize it, then add physical-material and scientific simulation. Each numbered file owns its implementation tasks, required workflows and evidence; the stage registry below is the single delivery order.

## Next action and context budget

- [ ] Start with the first stage whose registry status is not verified; read that stage's opening milestones, entry/scope, ordered tasks and advance gate, then open only the requirement sections needed by the current task. No capability is marked verified by this reorganization; core stages start Pending and numerical stages remain Deferred.

- [ ] Before coding, check existing implementation authorization and summarize the current concrete slice for owner refinement when approval is still needed. This document reorganization is authorized; application implementation still needs its agreed scope. Reuse approvals already present.

- [ ] Use the named predecessor completion summaries to recover APIs/decisions/proofs; open only a needed older contract/fixture section. Do not preload every file, the CAD research catalogue or future numerical sciences.

- [ ] Implement the stage through the shared native/backend/persistence/authority path. Advance only after its user workflows, applicable correctness/security/resource tests and native interaction work together; a helper/API checkbox, screenshot or 30,000-square draw test alone is insufficient.

- [ ] Write the short factual stage handoff, retain reproducible evidence and unresolved scope, and update its registry status only after its gate passes. Remove only actually completed task detail after implementation; keep the handoff and deferred/blocking requirements.

- [ ] Create one current-task context card from the registry using the template below; fill only the selected milestone/task and needed contract/summary links. Empty evidence fields remain pending and cannot advance a stage.

  ```yaml
  stage: NN-stage.md
  milestone: NN.Mxx
  task: NN.Txx
  required_contracts: []
  predecessor_summaries: []
  native_workflow_evidence: pending
  measured_budget_evidence: pending
  blockers: []
  ```

## Stage registry

- [ ] Follow the numbered files in order; the Outcome column states the user-visible gate and every completion link currently contains a pending template.

| Stage | Required outcome before advancing | Status | Past-context summary |
| --- | --- | --- | --- |
| [01 — Fixtures, budgets and delivery contracts](01-stage.md) | Freeze actual baseline, bounded fixture/replay data and budgets. | Pending | [Summary](01-stage.md#completion-summary) |
| [02 — Durable models, accepted edits and basic workspace entry](02-stage.md) | Create/edit/undo/save repeated objects; scoped editor/guest entry. | Pending | [Summary](02-stage.md#completion-summary) |
| [03 — Frames, local precision and independent views](03-stage.md) | Edit tiny objects in distant frames and independent precise views. | Pending | [Summary](03-stage.md#completion-summary) |
| [04 — Selections, shared regions, mounts and Areas](04-stage.md) | Use shared selection, 3D Areas/targets, bindings, immunity and mounts. | Pending | [Summary](04-stage.md#completion-summary) |
| [05 — Continuous sparse scale, navigation and hosted places](05-stage.md) | Ball ↔ building ↔ street ↔ globe/outer context; walk and host it. | Pending | [Summary](05-stage.md#completion-summary) |
| [06 — Offline city coverage, package closure and bounded streaming](06-stage.md) | Save/search/revisit a bounded offline city and favorite places. | Pending | [Summary](06-stage.md#completion-summary) |
| [07 — Native room authoring, multi-copy editing and mounted applications](07-stage.md) | Author/furnish a room or castle; batch-edit copies and mount apps. | Pending | [Summary](07-stage.md#completion-summary) |
| [08 — Caves, selective low-poly optimization and safe support](08-stage.md) | Navigate and simplify room/cave geometry without losing source/support. | Pending | [Summary](08-stage.md#completion-summary) |
| [09 — Imported bases, authored overlays and physical comparison](09-stage.md) | Compare authored overlays and reimport sources at declared 1:1 scale. | Pending | [Summary](09-stage.md#completion-summary) |
| [10 — Dimensioned sketches and exact CAD](10-stage.md) | Design/revise a dimensioned CAD part with honest exactness/history. | Pending | [Summary](10-stage.md#completion-summary) |
| [11 — Assemblies, building systems, drawings and fabrication data](11-stage.md) | Assemble buildings/parts and generate coherent drawings/takeoff/exchange. | Pending | [Summary](11-stage.md#completion-summary) |
| [12 — Structural molecule, crystal and protein editing](12-stage.md) | Author molecules/crystals/proteins as independent structural roots. | Pending | [Summary](12-stage.md#completion-summary) |
| [13 — Production, stock, procurement and civic workflows](13-stage.md) | Receive/reserve/produce/deliver and review civic evidence safely. | Pending | [Summary](13-stage.md#completion-summary) |
| [14 — Independently governed infrastructure and composed workflows](14-stage.md) | Compose sites/operations/public views across independent Organs. | Pending | [Summary](14-stage.md#completion-summary) |
| [15 — Core stabilization and workflow acceptance gate](15-stage.md) | Pass the complete personal/offline/hosted workflow stabilization gate. | Pending | [Summary](15-stage.md#completion-summary) |
| [16 — Material properties, conditions and offline knowledge — deferred](16-stage.md) | Choose physical materials and inspect applicable evidence offline. | Deferred | [Summary](16-stage.md#completion-summary) |
| [17 — Micro-derived models and independent laboratory experiments — deferred](17-stage.md) | Adopt a checked micro-derived model and run an independent protein study. | Deferred | [Summary](17-stage.md#completion-summary) |
| [18 — Additional rigid dynamics, joints and authored forces — deferred](18-stage.md) | Interact with admitted rigid/joint/force domains with one writer. | Deferred | [Summary](18-stage.md#completion-summary) |
| [19 — Verified elasticity, flexible bodies and heating — deferred](19-stage.md) | Inspect verified elasticity, conduction and transient heating. | Deferred | [Summary](19-stage.md#completion-summary) |
| [20 — Optional scoped recording and playback — deferred](20-stage.md) | Record/replay a bounded selected activity without live side effects. | Deferred | [Summary](20-stage.md#completion-summary) |
| [21 — Specialist physical models and coupled studies — deferred](21-stage.md) | Deliver each validated specialist/coupled science slice separately. | Deferred | [Summary](21-stage.md#completion-summary) |

## Shared decisions to preserve

- [ ] Keep the kernel domain-neutral: identities/occurrences/revisions; units/frames/clocks; supports/selections; typed values/evidence/unknowns; relations/constraints; operators/dependencies; purpose-specific representations; explicit runtime state. Workflows reuse these primitives rather than owning parallel model/save/query/undo systems.

- [ ] Keep ownership small: proposed pure `workspace_model` may depend on pure nucleus types; nucleus never imports it. Engine/store own authorized acceptance/persistence, native interface owns rendering/tools, map supplies geographic data, transport/Cell/Organ own scoped delivery, Protein retains semantic/source sessions, and specialist workers appear only when a stage needs them. Keep scientific kernels out of ordinary viewers.

- [ ] Make scale scene-independent. The ball/Earth/solar route and independent invented scene are required examples, never fixed engine endpoints/hierarchy/zoom bands. Preserve local physical dimensions, exact authored distinctions, navigation up distinct from gravity, and sparse active cost independent of empty extent.

- [ ] Select Orbit/Fly/Walk explicitly; preserve supported pose/focus and horizon/street/interior use. Ground has no local relief and absent building/interior geometry stays absent unless supplied/authored. The globe remains curved; blank roots need no Earth. Map/overview proxies and street geometry identify the same user-authored place.

- [ ] Release unneeded RAM/GPU after bounded demand/fences; retain durable edits, bounded compressed disk cache and explicit offline packs. Fetch configured detail on demand online within persistent budgets; full workspace-offline starts no remote/source/heartbeat/reconnect request.

- [ ] Automatically apply ready applicable editor output to the captured selected local scope on exit; preserve pending/invalid/stale state and previous accepted output otherwise. Keep ordinary multi-copy editing simple, source lineage retained and deliberate shared-definition updates explicit. Exceptional unsupported continuation retains last usable output and pauses only affected evolution before a visible refresh/restart.

- [ ] Preserve scoped identity/rights: private invites plus optional explicit public visitors, identified authorized editors, optional nonphysical avatars/cursors and designated runtime authority. Entry cost/preflight is bounded and continuing limits are enforced. No invite lends another Organ's login, private source, execution, desktop/pixels/input or worker budget.

- [ ] Keep permitted cached viewing and drafts after host loss; revalidate/reconcile with the same authority and recorded request outcomes. Do not automatically replay mutations, elect a physics writer or donate participant compute. Distinguish accepted geometry, observations, operational facts and conditional results.

- [ ] Preserve direct empirical material choice without microscopic setup, independent specimen/laboratory roots and amount-versus-sample separation. Structural editing is stage 12; new property derivation, experiments and physical behavior wait for the deferred tail. Atoms/proteins do not replicate by moles to simulate a bulk object.

## Deferred simulation

- [ ] Keep stages 16–21 deferred until stage 15 stabilization passes and the relevant numerical slice is authorized. Retain all material-property resolution, thermal/heating/thermodynamic cooling, additional rigid/joint/flexible/continuum behavior, laboratory experiments, optional scoped recording and specialist science requirements.

- [ ] Integrate and stabilize existing Areas/influence, Sand gravity, motion cooling/sleep and support throughout stages 01–15. Existing motion cooling reduces dynamics work; it does not calculate temperature. Core geometry/CAD constraints, visual appearance, passive definitions, Walk support and safe existing collider replacement remain core modeling/navigation work.

- [ ] Do not prebuild scientific datasets/workers/schema families because an earlier contract mentions future properties. Recorded/imported definitions and evidence remain usable with honest unsupported status. A mole never allocates an atom population; new numerical fidelity and sample/replica/output counts are separately admitted.

## Find only the required contract

- [ ] Locate full normative task detail by the owning stage below; use its completion summary/code after that stage is verified rather than rereading all implementation history.

| Needed context | Owning stage/section |
| --- | --- |
| Fixtures, core device budgets, adapter evidence and test discipline | [01 requirements](01-stage.md#architecture-and-task-requirements) |
| K1/K4/K6, pure crate boundaries, document/save/import, E1/E2/E3/E7, I4, H1/H2/H6/H9/H10 | [02 requirements](02-stage.md#architecture-and-task-requirements) |
| K2, I1, view precision and presence | [03 requirements](03-stage.md#architecture-and-task-requirements) |
| K3/K5, I3/I5, E4, H5, T4–T6 and indexed selection | [04 requirements](04-stage.md#architecture-and-task-requirements) |
| K7/I6/E6/E8, H3/H4/H8, scale/globe/controllers/demand and hosted starter budgets | [05 requirements](05-stage.md#architecture-and-task-requirements) |
| City coverage/offline packs/portable encoding and F9 | [06 requirements](06-stage.md#architecture-and-task-requirements) |
| I2, T1/T2, previews, batch variants and Park adapters | [07 requirements](07-stage.md#architecture-and-task-requirements) |
| T3, cave/simplification and safe derived support | [08 requirements](08-stage.md#architecture-and-task-requirements) |
| Reimport, authored placement and measured registration | [09 requirements](09-stage.md#architecture-and-task-requirements) |
| Exact CAD, Rust-first/kernel decision and CADCraft/microCAD/SolidCraft research | [10 requirements](10-stage.md#architecture-and-task-requirements) |
| Assemblies/building/fabrication geometry and IFC/BCF | [11 requirements](11-stage.md#architecture-and-task-requirements) |
| Molecule/crystal/protein structural representation/editing | [12 requirements](12-stage.md#architecture-and-task-requirements) |
| F1–F7, operational accounting/evidence/processes and civic forms | [13 requirements](13-stage.md#architecture-and-task-requirements) |
| F8/F10, independent infrastructure/observations/public governance | [14 requirements](14-stage.md#architecture-and-task-requirements) |
| Core/deferred phase boundary and mixed workflow gate | [15 gate](15-stage.md#advance-gate) |
| Physical-material properties/knowledge/applicability | [16 requirements](16-stage.md#architecture-and-task-requirements) |
| Model adoption, protocol/job/results, H7 and independent experiments | [17 requirements](17-stage.md#architecture-and-task-requirements) |
| K8/E5, additional rigid domains/authority/state transition | [18 requirements](18-stage.md#architecture-and-task-requirements) |
| Elastic/thermal discretization, reference equations and validation | [19 requirements](19-stage.md#architecture-and-task-requirements) |
| Optional capture/retention/playback policy | [20 tasks](20-stage.md#ordered-implementation-tasks) |
| Molecular/protein dynamics, nonlinear/coupled/device/living/nuclear/astronomical science | [21 tasks](21-stage.md#ordered-implementation-tasks) |

## Fixture ownership

- [ ] Open a fixture's canonical specification only when the current stage calls for its supported subset; core metadata/structure coverage never completes deferred physical-science proofs.

| Canonical fixture specifications | Owner |
| --- | --- |
| B01, B15, B20, B24, B34, B35, B39 | [02](02-stage.md#fixture-specifications-owned-here) |
| B13, B23, B36 | [03](03-stage.md#fixture-specifications-owned-here) |
| B05, B19, B22 | [04](04-stage.md#fixture-specifications-owned-here) |
| B25, B26, B27, B28, B30, B33, B38 | [05](05-stage.md#fixture-specifications-owned-here) |
| B02 | [06](06-stage.md#fixture-specifications-owned-here) |
| B03, B31, B40 | [07](07-stage.md#fixture-specifications-owned-here) |
| B04 | [08](08-stage.md#fixture-specifications-owned-here) |
| B06 | [09](09-stage.md#fixture-specifications-owned-here) |
| B10, B29, B32 | [10](10-stage.md#fixture-specifications-owned-here) |
| B08 | [12](12-stage.md#fixture-specifications-owned-here) |
| B14 | [13](13-stage.md#fixture-specifications-owned-here) |
| B07, B16 | [16](16-stage.md#fixture-specifications-owned-here) |
| B09, B17, B18, B37 | [17](17-stage.md#fixture-specifications-owned-here) |
| B11, B21 | [18](18-stage.md#fixture-specifications-owned-here) |
| B12 | [19](19-stage.md#fixture-specifications-owned-here) |

## Completion rules

- [ ] Preserve source revisions, units/frame/clock, selection/scope/base/actor, output IDs/receipts, evidence and resource owner/payer in every applicable work card. Capture exact test/replay commands and measured limits before completing a milestone.

- [ ] Re-audit current source paths/pins/limits before coding; research links are prior art, not selected dependencies or vendor performance promises. Embedded dependencies need LICENSE/credits. Use native Rust frontend, `cargo check`, required tests and cargo xtask/Nix workflows; warnings are errors. Do not edit AGENTS/README/past migrations or the unplugged web frontend.

- [ ] Keep every future stage/summary pending until implemented and verified. Planning examples compile independently but do not establish production APIs, scientific correctness, usable UI or measured application performance.
