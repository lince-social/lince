# Unified plugin system and Blood implementation plan

Lince should discover installed plugins at startup and make their contributions available to the features that use them. One plugin can extend several parts of Lince: a Nostr package could supply a Sync provider, Record mappings, Actions, and an optional feed presentation. Each feature receives its contributions through a defined registry and keeps responsibility for its own behavior.

Sync owns Blood's protocol connections and synchronization with IRC, ActivityPub, Nostr, and other systems. Protein selects the data involved. Imported content becomes ordinary Lince Records when retained, so existing chats, feeds, searches, and automations can use it.

## Agreed direction

1. There is one plugin package and lifecycle across Lince. A package may contribute to several extensible features.
2. Lince reads installed plugin definitions at startup and exposes their available contributions to the relevant features.
3. Features that accept plugins are refactored around explicit extension interfaces. Adding another implementation should not require another protocol branch in the core or its interface.
4. Sync is responsible for Blood's interactions with external protocols. Nostr, IRC, and ActivityPub are Sync options supplied by plugins.
5. Sync bindings use Protein to select data and can be incoming, outgoing, or bidirectional when supported and enabled by the user.
6. External content can be translated into ordinary Records, Threads, and Messages. A specialized Castle is optional; existing presentations should work with the resulting data.
7. The plugin system also extends features unrelated to Sync. The extension map below identifies useful candidates without committing to implementing all of them immediately.
8. Schedule Proteins select simulated Karma occurrences and a fixed or rolling window. Time Castle and export workers reuse the same schedule and allocation behavior.
9. Each Sync has independent selection, settings, interval and destination. Document format plugins convert snapshots; shared Sync infrastructure writes the configured file. Calendar exports are authoritative and outgoing only.

The implementation uses locally installed native executables and a versioned JSON protocol. Its registries accept Sync providers, Actions, and Record schemas. Packages implement Nostr public notes and `.ics`/Markdown document exports. See [the implementation guide](plugin-system-implementation.md) and [calendar exports](ics-sync.md) for installation, contracts, and limits. The remaining extension families below are still proposals.

## Where plugins would be useful

| Feature | What a plugin contributes | Example | Feature responsibility |
| --- | --- | --- | --- |
| Sync and Blood | Protocol providers, account setup, subscription and publishing operations, mapping and reconciliation | Nostr relays, IRC channels, ActivityPub accounts | Connections, direction, Protein selection, queues, conflicts, and status |
| File formats and archives | Readers, writers, mappings, and supported operations | Calendar `.ics`, CSV, an application export | Explicit import/export; Sync owns continuous synchronization |
| Record extensions | Namespaced schemas, field editors, validators, and derived representations | Recipe ingredients, laboratory measurements, protocol provenance | Storage, revisions, permissions, and generic fallback inspection |
| Actions and Karma | Named operations, argument schemas, result types, and optional deterministic calculations | Publish selected Records, convert units, extract document text | Authorization, scheduling, durable effects, receipts, and audit history |
| Sands and Castles | Views, controls, compositions, and presentation bindings | Social feed, specialized chart, scientific Record editor | Workspace placement, input, accessibility, tokens, and authorized interactions |
| Protein and knowledge sources | Dataset adapters, derived fields, and bounded projections over available data | A mounted knowledge collection or domain statistics | Declarative reads, access filtering, explicit dataset selection, and query limits |
| Vocabulary and domain packages | Meaning definitions, Assertion presets, units, and workflow templates | Nutrition vocabulary or manufacturing workflow | Meaning identity, publisher attribution, adoption, and revision handling |
| Fiote | Model providers, agent adapters, tools, and explicit context sources | Another inference provider or an approved domain tool | Sessions, credentials, tool authority, and activation policy |
| IDE | Language support, formatter and linter descriptors, project commands | A new language toolchain or project inspector | Workspace access, process supervision, edits, and diagnostics |
| Documents and media | Readers, decoders, conversion, transcription, and extraction | OCR or a specialized document format | Resource budgets, safe artifacts, previews, and retained assets |
| Maps and visualization | Data sources, renderers, styles, and domain overlays | Regional map data or industrial visualization | Coordinates, clipping, input, platform support, and rendering budgets |
| Devices and sensors | Device drivers, input sources, and measurement mappings | Environmental sensor, controller, or camera source | Device permissions, capture lifetime, privacy, and retention |
| Notifications and delivery | Delivery destinations and message formatting | Desktop notifications or an approved external destination | Attention policy, audiences, authorized payloads, and delivery status |
| Transfers | External execution Actions and evidence mappings | Shipment tracking or settlement evidence | Agreement validation, balances, and interpretation of evidence |
| Simulation | Fixtures and simulated implementations of contributed operations | Replay a connector stream or simulate a device response | Deterministic execution and prevention of live external effects |

Prioritize Sync, Record mappings and schemas, Actions, and existing presentations. Add other registries as their feature contracts become concrete. Themes, vocabularies, and static templates can be package data without requiring executable code.

Plugins may add implementations and domain behavior. Core authority, Fact integrity, Transfer accounting, and Record mutation invariants remain enforced by Lince.

## One package with several contribution registries

The plugin manifest identifies the package, its version and publisher, required host interfaces, supported platforms, dependencies, requested permissions, licenses, credits, and contributions. Each contribution has a stable identifier within the plugin's namespace, a feature family, a descriptor, and an implementation reference.

Use stable identifiers in saved configurations and expose display names separately. A renamed Sync option must retain its identity. Duplicate identifiers are errors, not an invitation for the last discovered package to replace another implementation.

The shared plugin host manages discovery, validation, availability, runtime lifetime, grants, and diagnostics. Each feature owns a typed registry: Sync receives Sync providers; Actions receives operations; presentation receives views. The features do not exchange unrestricted callbacks or expose the Store, Engine, or Bevy World directly to executable plugins.

A package can register related contributions together. Its Nostr Sync provider can reference its mapping schema and publishing Action. Required dependencies are validated before activation, and dependent contributions become usable together so a provider cannot run with a missing required mapping. An optional desktop view may be unavailable on mobile without making the package's independent backend contributions unavailable.

Built-in implementations should enter the same feature registries. Refactor one feature at a time and preserve its specific semantics; sharing a registry does not make Organ replication, directory reconciliation, and external publishing identical algorithms.

### Startup and lifecycle

1. Read installed manifests and persisted enabled state using local storage.
2. Validate identities, package integrity, host interfaces, platform support, dependencies, and contribution references. Describe unavailable packages and the reason.
3. Register validated metadata so feature catalogues can list available implementations and unavailable configured selections.
4. Activate enabled implementations through the selected runtime and establish only the host capabilities granted to them.
5. Restore configured feature instances. Sync starts enabled connections; a document reader waits for an open request; a view waits for presentation.
6. Supervise active work and stop it during shutdown or disablement.

Discovery itself does not publish data or open protocol connections. Activation and operation follow saved feature settings and grants. A broken plugin must leave the Cell and unrelated plugins usable. Host-owned execution limits prevent a plugin from blocking startup, exhausting queues, or hanging shutdown.

Receiving a package through an Organ does not automatically enable it. Package signatures identify the publisher and contents; operational authority comes from local grants. Executable updates are pinned and additional requested access requires approval in the product's plugin management flow.

Disabling stops new work and retains queued intentions and ordinary Records for inspection. Removing a package does not silently delete content. Missing contributions leave saved bindings visible with an unavailable status. Hot reload is outside the initial plan; startup registration is sufficient to prove the design.

## Sync as the Blood host

Keep four identities distinct: the plugin supplies an implementation; a connection selects an account or endpoint; a binding joins external resources to selected Lince data; a presentation displays the result.

A proposed Sync provider contract covers its configuration schema, credential requirements, advertised capabilities, external resource discovery, initial loading, subscription or polling, translation, outgoing operations, reconciliation, checkpointing, and cancellation. Support persistent sessions as well as periodic work.

The Sync host owns connection lifetime, executor selection, grants, incoming and outgoing queues, retention, retry policy, history, and user-visible status. Providers implement the protocol-specific steps through bounded host interfaces. Sync owns record interoperability protocols; other plugin families retain their own operations, such as Fiote provider inference or IDE language-server sessions.

### Protein selection and direction

Every binding names its provider, connection, external scope, destination Organ or existing Record, direction, mapping, and selection policy.

- Outgoing selection uses an authorized Record Protein. Aggregated query results are not directly writable objects. Publishing requires a supported mapping for the selected fields and relations.
- Incoming selection first limits the external scope, then evaluates a Protein over normalized candidates or the selected available source dataset before retaining them. It must not ingest an entire service just to apply a local filter.
- Providers may translate a supported subset of selection into remote subscription filters. Protein remains the final selection contract. Report unsupported predicates, bounded fetch coverage, and incomplete history; never silently broaden a private query.
- Bidirectional bindings keep external identity and imported base state so subsequent changes can be reconciled rather than treated as unrelated objects.
- Entering or leaving a Protein result is a membership change, not automatically a publish or remote deletion. Configure those consequences explicitly. Preview the initial outgoing set before enabling automatic publication.

Coordinate dataset adapters with [the external knowledge plan](vocabulary-knowledge-plan.md). Temporary source rows, retained references, and permanent copies remain distinct. Protein evaluates available snapshots; explicit feature requests or configured Sync work fetch data. A normal Protein read must not secretly contact a service, disclose a query, or retain content.

### Record mapping and writeback

Normalize readable content into ordinary Records and relations. Keep external identity, author, source revision, timestamps, verification evidence, audience, and mapping version alongside that content. Destination Organ ownership and upstream authorship are separate. Private credentials, cursors, and execution state belong in private operational storage.

Imported authors are observed external identities. The plugin's authority to import a Message does not give it authority to impersonate that author as a local Person. Facts identify the connector and evidence responsible for the local change.

Each binding specifies which fields are external, local, or writable in both directions. Local annotations should survive upstream changes. Protocol capabilities determine whether an operation means publication, replacement, deletion request, or a local edit only. Bidirectional does not promise arbitrary remote CRUD.

Commit incoming changes, identity reconciliation, deduplication, and checkpoints atomically. Commit outgoing intentions before contacting the external system. Track delivery independently of whether a Message is finished. Reconcile repeated events and remote echoes without republishing imported changes through the same binding. Relaying content into another binding is an explicit routing policy with audience checks.

Recheck permissions, mapping versions, destination, and enabled state when queued operations execute. Report uncertain outcomes after interrupted sends and use protocol idempotency where available. Preserve a base revision for conflict review instead of overwriting local annotations.

Recommend one designated executor Cell per connection initially. Other authorized Cells can view retained data and submit intentions. Executor changes require explicit ownership and recovery handling; a network partition must not cause two Cells to publish the same intention independently.

## Example of one Nostr plugin

At startup, one installed Nostr package registers a Sync provider, its event mappings and provenance schema, supported publishing Actions, and optionally a feed composition. Nostr appears in Sync's provider catalogue through registration.

The person creates a connection, selects relays and identity, chooses a Protein and destination, and enables the supported directions. Incoming events become selected retained Records. Outgoing selected Records become supported events through a declared mapping. Existing feed and Record views read those Records; the optional presentation adds useful controls.

Nostr uses signed events and relay subscriptions with a defined set of filter fields. The provider must verify event identity and signatures, scope acknowledgements to each relay, and deduplicate the same event received from multiple relays. These protocol responsibilities follow [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md).

For ordinary immutable events, changing a local copy cannot rewrite the signed event. Supported replaceable or addressable event kinds have different mapping rules. Removing a Record locally must remain distinguishable from requesting remote deletion. Deletion requests cannot guarantee erasure from all relays or clients. See [NIP-01 event kinds](https://github.com/nostr-protocol/nips/blob/master/01.md#kinds) and [NIP-09 deletion requests](https://github.com/nostr-protocol/nips/blob/master/09.md).

This is the shape of the first useful integration, not a commitment to every Nostr event kind. IRC and ActivityPub later enter the same Sync registry with their own capabilities and mapping semantics.

## Existing foundations and refactoring points

| Current code | Refactoring purpose |
| --- | --- |
| `crates/nucleus/src/sand_package.rs`, `crates/engine/src/sand_packages.rs` | Reuse package identity, integrity, licensing, and catalogue concepts; generalize for multiple contribution families and executable artifacts. |
| `crates/cell/src/lib.rs` | Own plugin discovery, activation, service supervision, and shutdown. Backend contributions must work in a headless Cell. |
| `crates/nucleus/src/sync.rs` | Add external provider identities and destinations, configuration, and capabilities without one core variant per protocol. |
| `crates/engine/src/sync_service.rs` | Extend current activity and queue reporting into a provider host with instance lifecycle and diagnostics. |
| `crates/engine/src/file_sync.rs`, peer and workspace sync modules | Register existing implementations through shared discovery while retaining their specific semantics. |
| `crates/protein/src/lib.rs`, authority and query modules | Reuse authorized selection; add dataset and projection hooks where needed without making reads perform external effects. |
| Record extension modules in nucleus, Store, and Engine | Add contribution schemas, provenance, and namespace ownership through authorized mutation paths. |
| `crates/engine/src/actions.rs`, `effects.rs`, message dispatch | Route contributed Actions, ingestion, and reconciliation; retain permission checks, delivery receipts, and uncertainty handling. |
| `crates/interface/src`, desktop Sync and Thread Castles | Share catalogue and configuration models, add provider selection and status, and reuse existing presentations. |
| Fiote driver, provider catalogue, and tool registry | Reuse supervision and descriptor patterns; later register contributions through the shared lifecycle. |
| Editor, document, media, and simulation crates | Add typed registries sequentially as their feature contracts are implemented. |

The current Sync service mostly tracks activity and queues; startup provider discovery and general protocol hosting still need implementation. Existing Sand packages accept declarative and desktop-layout payloads; executable plugin loading is additional work.

## Sequential implementation

1. **Define package and contribution contracts.** Add stable namespaced identities, descriptors, dependencies, enabled state, availability, and feature catalogue requests. Present installation and contribution status in the native interface. Validate duplicate IDs, missing dependencies, and unsupported platforms. Establish metadata discovery without executing unknown code.
2. **Implement one executable host.** Select the runtime after a focused platform experiment, then implement startup activation, scoped host capabilities, supervision, revocation, limits, and shutdown. Test a deliberately failing plugin. Prove that an installed package is usable without rebuilding Lince; compiled Rust modules alone do not complete this step.
3. **Refactor Sync to receive providers.** Register existing implementations, add provider and connection descriptors, Protein bindings, direction, capability reporting, queues, and reconciliation. Build the native configuration flow and retain current sync behavior.
4. **Complete one Blood provider.** Recommend Nostr as the first example, subject to choosing the initial protocol scope. Deliver incoming retention and supported outgoing publication through ordinary Records, with multi-endpoint delivery outcomes, source inspection, conflict handling, and shared views.
5. **Prove one package extends several features.** Have the same package contribute a Sync provider, a mapping/schema, and a named Action usable through shared controls and Karma. Confirm that disabling the package consistently stops work while ordinary Records remain readable.
6. **Complete a second provider and extract demonstrated common behavior.** Choose IRC or ActivityPub to exercise different identity, session, acknowledgement, and writeback semantics. Its provider must appear without new protocol-specific core or UI branches. Add file-format contributions through the same package system where their contracts fit.
7. **Extend further features one at a time.** Prioritize Record editors and compositions, knowledge datasets, Fiote, and IDE integrations according to concrete requests. Add media, rendering, devices, and other registries only with a working feature and its verification.

Finish each step's backend, necessary native interface, and meaningful validation before advancing. Correctness checks cover restarts, duplicate and out-of-order events, checkpoint failures, selection changes, and unavailable plugins. Security checks cover revoked grants, cross-binding access, authorship, credentials, and audiences. Performance checks cover bounded startup, history loading, plugin execution, queues, and ordinary view responsiveness. Simulation uses recorded inputs and mock effects.

Implementation follows repository constraints: current branch, no spawned coding agents or worktrees, `cargo check` with warnings denied, execution through `cargo xtask` or Nix, no code comments, and no edits to `AGENTS.md`, `README.md`, `.lingua`, or the unplugged web crate. Schema changes use a new migration numbered above every existing migration. Never change past migrations or checksum entries; append the filename and exact checksum computed with `sha384sum` to `crates/store/migrations.sha384`. Run `cargo check -p store` and the migration guard tests before continuing. Embedded Sand dependencies include their licenses and available credits.

## Choices still to make

- Execution runtime: sandboxed Wasm, supervised trusted native processes, or a measured combination. One package and registration contract can cover different supported payloads. Do not equate process supervision with sandboxing.
- First protocol and its supported operations. Nostr is the current example; encrypted messaging, federation hosting, and broad protocol coverage are separate scopes.
- Package distribution and update policy. Start with local installation and explicit enablement; decide how existing Organ package exchange joins that lifecycle.
- Retention defaults and whether a binding initially uses temporary data, retained references, or full copies. Preserve the external knowledge distinction where applicable.
- Automatic publication policy for outgoing Protein selections. Bidirectional mode needs a clear initial preview and an explicit policy for later matching Records.

The first completed experience is an independently installed package discovered at startup, contributing to several features, selectable in Sync with a Protein, and exchanging supported data in both directions. Its retained Records remain useful when the plugin is stopped.
