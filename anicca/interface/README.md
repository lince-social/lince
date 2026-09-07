# Interface working corpus

This directory is the agent-owned technical desk for Lince interface work. It
separates the current v1 implementation from accepted runtime evidence,
subject specifications, deferred work, historical alternatives, and the v2
world direction.

The owner-authored [Interface section in Lince](../Lince.lingua) remains
authoritative for interface decisions. [Ontology](../Lince.lingua) and
[Karma](../Lince.lingua) remain authoritative when a subject here links to
them. There are currently no separate Interface, Customization, Sands, or
Interoperability `.lingua` files. If one is created, it immediately becomes
the higher source of truth for its adjacent subject.

Markdown here explains consequences, records implementation reasoning, and
breaks work into verifiable parts. It never silently overrides a Record. When
a Record and this corpus disagree, preserve the Record decision, identify the
conflict, and wait for the owner to decide whether the Markdown should change.

## Reading protocol

1. Read this file.
2. Read [current.md](current.md).
3. Read only the subject document for the work being done.
4. Open the corresponding file under [plans/](plans/) only when implementing
   that cluster.
5. Read [research/](research/), [deferred.md](deferred.md), or
   [v2-world.md](v2-world.md) only when the current decision actually depends
   on them.

For a completeness review, [coverage.md](coverage.md) maps Interface ideas and cross-feature surfaces to their task owners. It is not another checklist or a required full read for each builder. Graph-work's per-role packets keep the active assignment small; current source and exact evidence, not repeated summaries, settle implementation state.

Do not load the complete directory by default. Do not repeat a settled
decision in several files; link to its canonical home. Git is the history, so
superseded working documents and completed task entries should be deleted
after their durable conclusions reach the appropriate prose.

## Preservation

The 2026-08-26 split was verified before editorial normalization by
concatenating each new section set in source order. All four results matched
the SHA-256 hash of the corresponding moved monolith exactly. Subsequent
changes only added routing metadata, repaired relative links, and expressed
the preserved pseudo-Record wrappers as readable source metadata. No
substantive source passage was dropped.

## Current checkpoint

The next milestone is [Part A — Dogfeeding](plans/part-a.md): general Lince knowledge/work capabilities used by a private company through new native Sands connected live to a local Linux machine or VPS. [Backend Part A](../backend-part-A.md) owns identity, Role-associated Protein Record access, property-write checks, reliable remote work and recovery; the Interface checklist owns reusable controls. The administrator can set vocabulary, Roles, policies and views manually, with no company starter or required owning project. All twenty native roots are not required: Karma, Transfer, separate Communication and other nonessential migrations remain in [native follow-through](plans/native-follow-through.md); CEF remains last.

Dogfeeding is not the already-accepted runtime Plan A, full C4 catalog completion or shared live Box collaboration. The [build rule](build.md) keeps the actual client/server path CEF-free without making lince-desktop a client dependency.

The 2026-09-06 Interface Record revises planned v1 behavior: stable placement
comes first, with per-appearance edits, children released from group movement,
presentation switching, focused editing and native Calendar/Clock. Movement
code stays intact but inactive in the first delivery. The
[master plan](plans/interface.md#v1-master-waterfall) is the execution order;
[Box](box.md), [Time](time.md) and [Shaders](shaders.md) define the changes.
The evidence below describes what was already built, not these new features.

[current.md](current.md) is the compact continuation context. The promoted
native foundation is described in [architecture.md](architecture.md), while
its evidence and production launch proof live in [laboratory.md](laboratory.md)
and [plans/interface.md](plans/interface.md).

The typed Customization, semantic primitive-Sand, recursive-composition and C3
Configuration/external-authoring kernels are landed. C4 has a 25-root,
72-definition Rust catalog and native F12 inspector. Edit controls, zoom
controls, Record, Conversation, Table, Todo and Kanban now have operational
retained projections.
The production desktop binds Record, Conversation and private-draft data
through live Protein subscriptions; acknowledged Message lifecycle and durable
draft and Record-quantity Actions cross the same domain boundary. Seventeen
runtime migrations were still pending at that checkpoint; their native/late-CEF split is now in the build rule. The three collection roots still carry
explicit C4 follow-up for richer configuration and editing.
The ordered 30-entry Pulsar/Helio study in
[links.md](links.md) is complete. It selected no engine, renderer, database or
GPUI dependency; its narrow carry-forward set is canonical in
[architecture.md](architecture.md#completed-pulsarhelio-study-and-carry-forward-boundary).
Implementation continues with official-Sand runtime migration, the completion gate,
and only then Box. The completed
SceneDB/EngineFS cluster informs Box durability and live collaboration without
owning either design.

## Subject map

| Document | Canonical responsibility | Status |
| --- | --- | --- |
| [First Steps.linguai](First%20Steps.linguai) | Agent-maintained future user tutorial that will become owner-reviewed Instinct after v1 settles | Draft; legacy instructions plus clearly marked native plan |
| [product.md](product.md) | Product promises, UI guidelines, collaboration/editor surface | Active product boundary |
| [architecture.md](architecture.md) | V1 runtime ownership, Plan A/Plan B disposition, engine seams, browser-as-client gate | Plan A accepted; browser client NOT PLANNED (2026-09-01) |
| [build.md](build.md) | Native default, optional CEF boundary, catalog availability, tool lanes and proof profiles | Binding delivery rule; implementation begins in Part A |
| [coverage.md](coverage.md) | Owner Interface points and cross-feature UI promises mapped to one execution home | Coverage index; no implementation certification |
| [laboratory.md](laboratory.md) | Native fixtures, benchmarks, acceptance evidence and handoff | Promoted; first C4 retained runtime and live Protein seam proven |
| [customization.md](customization.md) | Tokens, themes, Configuration, visual character and design-system sequence | C3 landed; C4 retained-runtime migration active |
| [visual-inventory.md](visual-inventory.md) | First-party visual source boundary and token migration classification | Landed inventory; migration pending |
| [legacy-web-ui.md](legacy-web-ui.md) | Existing Web component/API inventory | Migration input only |
| [behavior.md](behavior.md) | JavaScript/Rust Behavior boundary and shipped source rules | Accepted boundary |
| [sand-model.md](sand-model.md) | Recursive Sand/Castle model, ports, state and renderer projections | Recursive host and workbench landed |
| [html-and-websites.md](html-and-websites.md) | Installed HTML, Website authority, packages and isolation | Prototype evidence retained; embedded runtime deferred to final v1 lane |
| [official-sands.md](official-sands.md) | Existing first-party Sand behavior that migration must preserve | Rust catalog plus first retained-runtime slice landed; migration active |
| [box.md](box.md) | Box, Protein result templates, Areas, surface topology, free space, projection and persistence | Opens after customization gate |
| [time.md](time.md) | Native Calendar/timeline, next-hour Clock, spiral and simulation views | Planned v1; domain dependencies included |
| [shaders.md](shaders.md) | Presets, supported WGSL effects, glow bounds and authoring/recovery | Later v1, after stable use |
| [facade.md](facade.md) | Public read-only Live Facade | Late v1 |
| [interoperability.md](interoperability.md) | Interface-facing Facade, Blood and portable-canvas boundaries | Partly deferred |
| [communication.md](communication.md) | Communication Sand domain and staged implementation | Separate Sand backlog |
| [future-sands.md](future-sands.md) | Unplanned 2D Map and Ergon ideas | Future |
| [deferred.md](deferred.md) | Offline workspace replicas/failover, richer Protein, portals and advanced topology extensions | Explicitly deferred |
| [research/runtime-alternatives.md](research/runtime-alternatives.md) | Maud, raw HTML, Datastar and renderer alternatives | Reference only |
| [research/scenedb.md](research/scenedb.md) | SceneDB runtime, Box durability, EngineFS, sync lanes and live collaboration review | Accepted research direction |
| [links.md](links.md) | Complete Pulsar/Helio research ledger, relevance judgments and focused source snapshots | 30 reviews complete; reference only |

## Plans

- [../backend-part-A.md](../backend-part-A.md): Dogfeeding's backend foundations, access model, company fixture and operational acceptance.
- [plans/part-a.md](plans/part-a.md): Dogfeeding's native controls and quality gates.
- [plans/native-follow-through.md](plans/native-follow-through.md): native migrations excluded from Dogfeeding, preserved as later v1 work.
- [plans/cef.md](plans/cef.md): the final v1 CEF lane, including the five deferred roots and fresh browser-specific proofs.
- [plans/interface.md](plans/interface.md): landed foundation statement and
  stable Box, individual composition, native time, later motion/effects,
  simulation dependencies, persistence and Facade work.
- [plans/customization.md](plans/customization.md): landed style, Sand,
  composition and Configuration kernels and remaining C4-C5 waterfall.
- [plans/sands.md](plans/sands.md): landed composition boundary, packages and
  external runtime hardening.
- [plans/communication.md](plans/communication.md): Communication stages.
- [plans/future-sands.md](plans/future-sands.md): explicitly unplanned
  World/Map and production-coordination ideas.

## Scope discipline

What is required for a capability to work belongs in its subject
specification and completion gate. What might make it faster belongs in
measured runtime work; an unmeasured optimization does not become a
prerequisite.

V1 is the productivity interface. V2 motivates permanent seams but does not
expand the current cluster. Browser/Facade projection is retained, but Linux
desktop development follows the accepted Wayland-native Plan A rather than
maintaining a second desktop implementation.

The 2026-09-06 reference sweep covered the owner Interface notes, this corpus and its plans, the draft tutorial, and cross-feature surface promises in Ontology, Karma, Fiote, Files, Rooms, Secrets, Code and File Sync. [Part A](plans/part-a.md#related-feature-plans-do-not-become-hidden-prerequisites) records the domain boundaries. Research ledgers, historical joined measurements and v2 ideas remain reference material rather than new prerequisites. The owner's `.lingua` was read, not changed; the current CEF instruction is recorded here and in the build rule.
