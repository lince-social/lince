# Interface v1 coverage

This is a reading map, not another task list. It accounts for the owner-authored Interface Record in [Lince](../Lince.lingua) and its cross-feature promises. [Dogfeeding backend](../backend-part-A.md) and [Interface Part A](plans/part-a.md) define the current company milestone; other native work remains scheduled in [native follow-through](plans/native-follow-through.md) and the master plan. Interfaceless is outside this review.

Read [Part A](plans/part-a.md) and the selected subject for implementation; the whole map and research ledger are not needed for every step. The [master waterfall](plans/interface.md#v1-master-waterfall) orders delivery and [build.md](build.md#no-embedded-browser) owns the no-embedded-browser rule. All mapped work remains subject to its real proof; this map certifies no implementation.

## Implementation base

All native work below assumes [Bevy ownership](architecture.md#bevy-native-interface),
not the earlier custom host. Use Bevy types directly and preserve translation
only at real domain, storage or external publication boundaries. First-party
Bevy UI/text, curves, retained gizmos and meshes are the defaults; AccessKit
support is accepted, Flair is preferred for CSS authoring over Bevy components,
and Avian 3D is selected for later contact motion. The scene is 3D-capable from
the start while Part A stays flat and stationary. Pinned facing with a Face
viewer toggle belongs to later spatial tools. Untrusted executable extensions
are deferred; specialized content engines remain low-priority end-of-v1 work.
Exceptions must meet a named Lince need and the same resource/idle tests.
Historical completion marks do not certify the Bevy replacement. The owner's
current machine is the [minimum target](build.md#minimum-machine-and-resource-baseline).

## Owner Interface points

| Point to preserve | Task owner and delivery |
| --- | --- |
| Black/white default, supporting purple, flat 2D surfaces and restrained differentiation | [Customization](customization.md); Part A A04.2 and A05.1–A05.7, including Dark/Light, exact roles, non-color meaning and visual review |
| Quick choices, full control, little permanent chrome | Part A A02.1 and A05.3; [editing facets](plans/interface.md#editing-facets-size-and-focus) at stage 4; minimal does not mean hidden or keyboard-inaccessible |
| Buttons, dropdowns, tooltips and reusable workflow pieces | [Sand composition](sand-model.md); Part A A01.2, A04.1–A04.4; grow primitives when a real workflow needs them |
| Ready-made Castles: Kanban, Table, relation graphs, Transfer and Karma | Dogfeeding A02.6–A02.7 covers task collections. Full Ontology/Relations, Transfer and Karma migrate in [native follow-through](plans/native-follow-through.md); Calendar/Clock remain later time work, and browser games need a native game surface first |
| Protein reads, typed interactions and Actions between pieces | Part A A01.3, A02.5 and [Box result templates](plans/interface.md#protein-area-result-templates), stage 4; reading never grants writing |
| Edit displayed properties, events, connections, tokens and advanced styling | [Sand authoring](plans/sands.md#sand), stage 4; [Customization](customization.md) scopes native tokens and bounded CSS to their supported projections |
| Spawn, attract, repel, sort and protect through Areas | [Stationary Areas](plans/interface.md#stationary-property-areas) first at stage 5; [moving/mutation/immunity Areas](plans/interface.md#force-sorting-mutation-and-immunity-areas) at stage 8 |
| Edit one circle or one appearance of Apple without changing its siblings | [Individual edits](plans/interface.md#individual-edits-and-presentation-changes), stage 4; appearance/row/child identity, not just Record uid |
| Separate basic Sands and packaged Castles in the picker | Same stage-4 checklist; native availability gate already applies in Part A A01.8 |
| Switch Sand/Castle presentation, handling common, missing and extra fields | Same stage-4 checklist; preview, explicit mapping and cancel/undo, without deleting Record data |
| Release one or all children while retaining group ownership | Same stage-4 checklist; independent movement, unchanged bindings/events/permissions and retirement with the row |
| Copy a property into another released Sand | Same stage-4 checklist; fresh appearance, shared source, no duplicated grant/session or doubled Action |
| Edit only Appearance, Connections, Data or a chosen combination | [Editing facets](plans/interface.md#editing-facets-size-and-focus), stage 4; hidden settings remain intact |
| Minimum size, optional maximum, content growth or internal scrolling | Same stage-4 checklist, including invalid bounds and long documents |
| Focus a Sand without changing its saved size, or focus its source Record | Same stage-4 checklist; restore workspace/draft/caret, handle missing or ambiguous sources; long content uses the outer reading view |
| Turn displayed properties into groups or ordered Areas, including two axes | [Stationary property Areas](plans/interface.md#stationary-property-areas), stage 5; retain optional redundant labels and manual exceptions |
| Native Calendar/timeline over time properties | [Time](time.md) and [time tasks](plans/interface.md#calendar-clock-and-simulation), stage 5; supported date Actions and bounded recurrence reads accompany the UI |
| Next-hour Clock, perspective spiral and configurable horizon | Same time tasks; top filtering is explicit, not an accidental overlap of later turns |
| Intervals occupy strips and recurrence appears repeatedly | Same time tasks; stable occurrence identities, timezone/admission, overlaps and accessible agenda; no frontend scheduler |
| Custom WGSL and possible outward glow | [Effects](plans/interface.md#sand-visual-effects), stage 9; bounded supported fragment effects first, not unrestricted compute/vertex access |
| Action, Rule and Transfer simulation views | Same time-task section, separate stage 10 after stable use; real domain preview, unknown effects, stale checks and no live side effects |
| Remove five-second source-to-rendered editor switching | Part A A02.2/G05; keep the current source block stable during thought, selection and IME |
| Show workspace controls by default; optional styled/transparent triangle | Part A A02.1/G04; keyboard restore and persistence. Box reuses it, not a second build |
| Stable placement first; keep movement code but do not use it by default | Part A decisions, stage-3 Box and [stable-use gate](plans/interface.md#stable-everyday-use-gate); retained moving workload is regression evidence only |
| LICENSE/credits with embedded dependencies and an easy-to-find public explanation | Part A A04.4/A05.6; package notices plus ordinary About/Credits, separate from executable bytes; browser dependencies remain in their deferred packages |
| Interface state in memory with configurable file backup | [Box persistence](plans/interface.md#box-state-persistence), stage 3 and extended with each feature; snapshot cadence, journal, undo, last durable revision and bounded recovery lag are distinct |
| Low attention and interoperability | [Interoperability](interoperability.md), Archive and later Facade/publication lanes; no general browser client or interfaceless work added to Part A |

## Promises outside that one Record

| Contract or remaining feature | Where it is accounted for |
| --- | --- |
| Every source root and ordinary failure state | Account for all 25, but qualify only Dogfeeding's company workflows now. Extra native roots stay in native follow-through; the five browser roots stay unavailable until each has a browserless design. Constructors are not behavior proofs |
| Native identity, Role/Protein/property policy, private drafts and revocation | Backend B02–B08/B14; Interface A01.3, A03.3 and A06.1. Real remote sessions, read/write and property checks against current/proposed state, protected policy dependencies and active access loss; no display-filter authority or test-only author |
| Collaborative Record/embedded/Table editing, acknowledgements and reconnect | Part A A02.2–A02.6; preserve the existing text contract without browser Wasm or whole-body overwrite substitutions |
| Recent displaced-edit visibility, extra scalar bindings, Note lifecycle, shallow snapshots | [Product/editor tasks](product.md#collaboration-and-the-editor), explicitly routed by [native follow-through](plans/interface.md#native-follow-through-and-cross-feature-surfaces); storage optimization is not a UI prerequisite |
| Program/director-backed Karma authoring, candidates and grants | Later native/Karma lane, including cycle refusal UI. No Karma or Transfer Castle is a Dogfeeding dependency |
| Contact visibility, device limits/capabilities, succession, reach, Move, backoff, reconnect, offers, Trash/Restore, read-model health | [Eleven Ontology surfaces](plans/interface.md#ontology-surfaces-this-interface-owes), native follow-through with any missing domain seams; not erased by A03.3's narrower migration |
| Record discussion and future multi-person rooms | Dogfeeding B13 and A02.4 qualify threads/messages without compulsory project membership. Cross-Organ membership and calls remain [Rooms](../Rooms.md)/Communication work, not live-access prerequisites |
| Files, password vaults, provider credentials, Code and File Sync | Their feature plans own mechanisms and native controls; [Part A boundaries](plans/part-a.md#related-feature-plans-do-not-become-hidden-prerequisites) preserve accepted concurrent behavior without importing entire unfinished features |
| Calls, media, session controls and agent work surfaces | [Communication plan](plans/communication.md) and [Fiote-facing Sand plan](plans/sands.md#conversation-and-task-surfaces-carried-for-fiote); terminal and browser panes wait for a browserless implementation, native human conversation does not |
| Stable Box, drawings/navigation/base pattern, topology and free space | Their named [master-plan](plans/interface.md) sections; stationary use precedes later optional spatial behavior |
| Live workspace sharing and public static/Live Facade | Master-plan collaboration/Facade sections, with [interoperability privacy conditions](interoperability.md); external browser viewing runs in the visitor's browser, not inside Lince |
| Source/package integrity, native quality and company acceptance | Part A A04–A05 and backend B17–B21; real client/server products, backup restore, remote access and fresh native evidence under the [build profile](build.md#native-acceptance-is-a-new-report-not-an-edited-old-result). G26–G29 need re-cut before use |
| Mobile, offline editable workspace replicas, portals, additional charts/trees/IDE archetypes and world research | Explicitly later or exploratory in [product](product.md), [deferred](deferred.md) and the future-Sand plans; not additional v1 or Part A acceptance requirements by example alone |

## Judgment calls to keep visible

Part A — Dogfeeding proves general Lince knowledge/work capabilities in a private company, not a special company/task product, full native catalog or complete user-composable Box. Backend B09–B10/B21 and Interface A02/A03/A06.2 cover manual vocabulary, Role/Protein/property policies and view setup from an empty Organ; no starter, wizard, fixed project ownership or legacy desktop Sands are required. The backend and live connection are real work, and the old graph remains stale until re-cut. Later native migration, Box and Calendar work stay planned.

Why-is-it-here begins with source, field, ownership, layout and permission explanations in the first result-template stage. Physics later extends it. A stationary view can already confuse someone, so explanation cannot wait for moving Areas.

The direct Live Facade and privacy-preserving cached public generations have different network guarantees. The interoperability plan retains an explicit reconciliation gate for public modes; do not collapse them into one promise or invent an owner decision from an older missing Record link. This does not block native Part A.

Native CSS is not a browser engine, a focused document cannot make arbitrarily long content fit a finite display, and compiling a shader does not bound its cost. The existing plans preserve the intended control through supported styling, an outer focused reading view and bounded effects rather than claiming those limits disappear.
