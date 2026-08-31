# Interface working corpus

This directory is the agent-owned technical desk for Lince interface work. It
separates the current v1 implementation from accepted runtime evidence,
subject specifications, deferred work, historical alternatives, and the v2
world direction.

The owner-authored [Interface section in Lince](../Lince.lingua) remains
authoritative for interface decisions. [Ontology](../Ontology.lingua) and
[Karma](../Karma.lingua) remain authoritative when a subject here links to
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

[current.md](current.md) is the compact continuation context. The promoted
native foundation is described in [architecture.md](architecture.md), while
its evidence and current production-launch blocker live in
[laboratory.md](laboratory.md) and
[plans/interface.md](plans/interface.md).

The typed Customization, semantic primitive-Sand and recursive-composition
kernels are landed. The ordered 30-entry Pulsar/Helio study in
[links.md](links.md) is complete. It selected no engine, renderer, database or
GPUI dependency; its narrow carry-forward set is canonical in
[architecture.md](architecture.md#completed-pulsarhelio-study-and-carry-forward-boundary).
Implementation continues with Configuration and external authoring,
official-Sand migration, the completion gate, and only then Box. The completed
SceneDB/EngineFS cluster informs Box durability and live collaboration without
owning either design.

## Subject map

| Document | Canonical responsibility | Status |
| --- | --- | --- |
| [First Steps.linguai](First%20Steps.linguai) | Agent-maintained future user tutorial that will become owner-reviewed Instinct after v1 settles | Draft; legacy instructions plus clearly marked native plan |
| [product.md](product.md) | Product promises, UI guidelines, collaboration/editor surface | Active product boundary |
| [architecture.md](architecture.md) | V1 runtime ownership, Plan A/Plan B disposition, engine seams | Plan A accepted |
| [laboratory.md](laboratory.md) | Native fixtures, benchmarks, acceptance evidence and handoff | Promoted through C2; production launch awaits valid owner references |
| [customization.md](customization.md) | Tokens, themes, visual character and design-system sequence | Composition landed; Configuration next |
| [visual-inventory.md](visual-inventory.md) | First-party visual source boundary and token migration classification | Landed inventory; migration pending |
| [legacy-web-ui.md](legacy-web-ui.md) | Existing Web component/API inventory | Migration input only |
| [behavior.md](behavior.md) | JavaScript/Rust Behavior boundary and shipped source rules | Accepted boundary |
| [sand-model.md](sand-model.md) | Recursive Sand/Castle model, ports, state and renderer projections | Recursive host and workbench landed |
| [html-and-websites.md](html-and-websites.md) | Installed HTML, Website authority, packages and isolation | Prototype proven; product surface pending |
| [official-sands.md](official-sands.md) | Existing first-party Sand behavior that migration must preserve | Migration input |
| [box.md](box.md) | Box, Protein result templates, Areas, surface topology, free space, projection and persistence | Opens after customization gate |
| [facade.md](facade.md) | Public read-only Live Facade | Late v1 |
| [interoperability.md](interoperability.md) | Interface-facing Facade, Blood and portable-canvas boundaries | Partly deferred |
| [communication.md](communication.md) | Communication Sand domain and staged implementation | Separate Sand backlog |
| [future-sands.md](future-sands.md) | Unplanned 2D Map and Ergon ideas | Future |
| [deferred.md](deferred.md) | Offline workspace replicas/failover, richer Protein, portals and advanced topology extensions | Explicitly deferred |
| [v2-world.md](v2-world.md) | Globe-to-desk worlds, terrain, scenes, time and capture-to-work | V2 research |
| [research/runtime-alternatives.md](research/runtime-alternatives.md) | Maud, raw HTML, Datastar and renderer alternatives | Reference only |
| [research/scenedb.md](research/scenedb.md) | SceneDB runtime, Box durability, EngineFS, sync lanes and live collaboration review | Accepted research direction |
| [links.md](links.md) | Complete Pulsar/Helio research ledger, relevance judgments and focused source snapshots | 30 reviews complete; reference only |

## Plans

- [plans/interface.md](plans/interface.md): landed foundation statement and
  remaining Box, Areas, persistence and Facade work.
- [plans/customization.md](plans/customization.md): landed style, Sand and
  composition kernels and remaining C3 through C5 waterfall.
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
