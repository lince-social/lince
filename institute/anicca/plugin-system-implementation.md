# Unified plugins: first implementation

Lince discovers many installed packages when a Cell starts. A package declares a stable identifier, version, executable, licenses, credits, and several contributions in `plugin.json`. One installed executable can extend multiple features. Lince keeps discovery and execution in a shared host; Sync and Actions consume their own typed contributions, and Sync validates imported metadata against the registered Record schema.

This implements the startup system, generic native controls, a working Nostr example, and calendar/document exports. Protocol-specific networking stays in the example package. Lince's engine and interface contain no Nostr branches. [Calendar exports](ics-sync.md) use Schedule Proteins and independent directory destinations through a shared document writer.

## Use the example

1. Identify the data directory used by your Lince Cell.
2. Run `cargo xtask plugin-example /absolute/path/to/the/data-directory`.
3. Start or restart Lince. Open the existing Sync Castle and its Plugins section.
4. Review the package and enable it. Packages are installed native programs with your computer account's access. The first implementation is intended for trusted local packages.
5. Choose **Nostr notes**, set relay URLs, optional author hex public keys, and the recent-note limit. Choose **Receive**, then **Save and start Sync** to retain returned notes as ordinary Records. An optional incoming text filter uses Protein predicates.
6. To publish, first run **Create or show publication identity** from the same package. This explicitly creates a private key in the package's local state directory and returns its public key.
7. Choose a saved Record Protein, select **Publish** or **Both**, and preview the content. Review the public text, then save and start Sync. The enabled binding continues publishing future matching local Records.
8. Use **Refresh now**, **Stop Sync**, or **Disable package** to control the connection. Failed or uncertain deliveries appear with their result and an explicit retry control.

The installer copies the Rust executable into `<data-directory>/plugins/example.nostr/` and writes its manifest. Package state is stored separately in `<data-directory>/plugins/.state/example.nostr/`. Reinstalling preserves the identity. Restarting discovers the new package bytes; a changed package requires enablement again.

Receiving does not need a publication identity. The example supports NIP-01 public kind-1 notes, several relays, authors, signatures, and acknowledgements. It does not implement private messages, relay authentication, reactions, deletions, or a complete historical archive. Its default connection is not contacted until a binding is explicitly saved and enabled.

## Startup and lifecycle

The host reads up to 128 package directories, validates manifests, and registers contributions by `<plugin-id>/<contribution-id>`. Discovery does not execute packages. Malformed packages become diagnostics while healthy packages remain available. Duplicate package IDs disable every copy of that ID. Conflicting schema namespaces disable all affected packages.

Enabling pins a SHA-256 digest of the manifest and executable. Before each call, the host checks both files again. Replacing files requires restart and review. The executable must resolve inside its package directory; executable path traversal is rejected. Bindings and package enablement are local Cell configuration, stored separately from ordinary Records and shared content.

A call starts a supervised child process, performs the protocol handshake, sends the requested operation, and closes the process. Each process gets an empty inherited environment plus `LINCE_PLUGIN_STATE`; its working directory is its state directory. Calls have a 45-second timeout, four concurrent execution slots, and a 1 MiB frame limit. Manifest, executable, metadata, checkpoint, and queue budgets are enforced separately.

This process boundary is not an operating-system sandbox. A trusted native package can open files and network connections as its user. Declarative permissions and a sandboxed runtime would be a separate extension of this implementation. Plugin management and invocation are restricted to local interface operations and are unavailable in Simulation.

## Contributions implemented now

| Contribution | Contract | Consumer |
| --- | --- | --- |
| `sync` | Settings fields, supported directions, schema, and whether replaying the same prepared publication is safe | Shared Sync service, binding worker, and generic Sync controls |
| `action` | Declared argument fields and an `invoke` operation | Generic plugin Action form and the engine's plugin command |
| `schema` | An owned `plugin.<package-id>.*` namespace and typed metadata fields | Record ingestion and prepared-publication validation |

Fields support text, integer, Boolean, and JSON values, with required flags and defaults. The native interface creates settings and Action forms from these declarations. It lists schemas and shows manifest licenses and credits. Unsupported contribution types are rejected instead of silently ignored.

Document format providers support `.ics` and Markdown exports. Views, general import codecs, editors, search providers, storage implementations, and other candidates in the plan have not been implemented. They can receive additional typed contribution variants and feature-specific consumers through the same host when their contracts are defined. Plugins currently cannot replace core authorization, accounting, or Record identity behavior.

The current runtime starts a process per operation. Polling protocols can use persistent plugin state and binding cursors. Continuous connections such as IRC need a persistent-session lifecycle before adding their adapters; they should not be implemented by repeatedly connecting and disconnecting a chat client. That lifecycle belongs in the shared host and Sync contract, while the feature registries can stay in place.

## Executable protocol

The stdin/stdout protocol is newline-delimited JSON. Stdout is reserved for replies. Each request includes `protocol`, `id`, `contribution`, `operation`, and `input`. A reply echoes the protocol and request ID and contains exactly one result or error. The protocol identifier is `lince.plugin-rpc.v1`; the manifest format is `lince.plugin.v1`.

The host first calls `initialize` with an empty contribution ID. The package returns `{"protocol":"lince.plugin-rpc.v1"}`. Subsequent operations are:

| Operation | Input | Result |
| --- | --- | --- |
| `invoke` | The declared Action arguments | An Action result displayed by the native controls |
| `validate` | `settings` | Settings validation; this operation should have no external side effects |
| `pull` | `settings`, previous `cursor` | `records` and the next `cursor` |
| `prepare` | `settings`, a publication Record | A canonical external `record` plus the protocol `payload` |
| `push` | `settings`, the persisted prepared result | `accepted` plus protocol delivery details |
| `render` | `settings`, a selected Record or Schedule `snapshot` | A complete document's `content` for the shared file writer |

An imported Record has `external_id`, `head`, `body`, and schema-validated `metadata`. A publication Record has `uid`, `head`, `body`, and a persisted `created_at` timestamp. Preparing must not publish: the result is saved before `push`, so retries can reuse the same remote identity and payload.

The Nostr package validates signatures and event IDs before producing imported Records. It signs outgoing text as the Record's heading, a blank line, and body; a Record with no body publishes only its heading. It requires an existing publication identity. Retries reuse the stored signed event, and success requires a positive acknowledgement from every configured relay.

## Sync behavior and retained content

Each binding has its own provider, settings, direction, interval, filters, cursor, and revision. Changing connection settings or incoming filters resets the checkpoint while preserving retained Records and identity mappings, so the new scope can observe its recent history. The worker checks due bindings every five seconds. Refresh intervals range from 30 seconds to one day. It integrates active operations and outcomes with existing Sync history.

Incoming selection uses a bounded subset of Protein predicates that can be evaluated on an unretained candidate: text, kind, quantity, and Boolean groups. Unsupported predicates fail explicitly. Selection does not silently fetch additional external data or retain filtered-out candidates.

Selected incoming content is stored as a plain Record with the plugin schema extension and a `lince.source` provenance extension. Importing adds a Sync Fact. Content and cursor commit together. Remote identities deduplicate retries; local deletion does not cause a note to be recreated on the next poll. Changed content for an already retained remote identity stops with a conflict result rather than overwriting local content.

Outgoing protocol selection requires a saved, non-aggregating Record Protein. It is capped at 100 local plain Records and excludes Records with `lince.source` provenance to prevent accidental re-export. Document providers declare their accepted source and can export up to 1,000 entries. Enabling outgoing or bidirectional Sync requires a preview tied to the binding, selected content, and approved package digest. Changed settings or content invalidate that preview. The enabled binding is a continuing publication policy, not a one-time send.

The durable outbox stores content snapshots, prepared payloads, attempts, acknowledgements, and failed or uncertain outcomes. It contributes pending and held counts to the shared Sync overview, and delivery outcomes add Sync Facts and history. Before publishing, the worker rechecks whether a Record still matches the Protein and whether its content changed. Stopping or revising a binding prevents subsequent operations from starting. A publication already sent to a relay may finish after Stop.

Published remote identities map back to their local Records before network delivery begins. Incoming echoes reconcile with that mapping. Restart converts interrupted running deliveries to uncertain, and does not automatically replay them. Only providers declaring replay safety offer explicit retries. Nostr is replay-safe because the identical signed event has the identical event ID.

The first version retains at most 1,024 unresolved deliveries and 10,000 delivery receipts. It has no receipt-maintenance UI yet. A binding's connection settings cannot change while deliveries remain unresolved. Stopping a binding or removing a package preserves its received Records.

Deduplication currently belongs to each binding. Separate incoming bindings can intentionally retain the same remote note separately. Nostr polling observes bounded recent relay snapshots; it does not guarantee a complete event history. Source identity, observed data, and relay acknowledgement are evidence of protocol behavior, not proof that an author's claim is true.

## Code and verification

- `crates/nucleus/src/plugins.rs`: shared manifest, contribution, command, and transport types.
- `crates/plugins/`: discovery, registration, digest pinning, supervised execution, and framing.
- `crates/engine/src/plugins/`: filtering, ingestion, publication queue, identity mappings, and worker.
- `crates/engine/src/plugins.rs`: feature commands, catalog, binding validation, and preview.
- `crates/desktop/src/sync_castle/plugins.rs`: generic native plugin controls.
- `crates/plugin-nostr/`: installable example and local-relay integration tests.
- `crates/plugin-files/`: installable iCalendar and Markdown formats, using the shared document writer.
- `crates/store/migrations/0004_plugins.sql`: append-only migration for local configuration, mappings, and outbox.
- `crates/store/migrations/0008_plugin_projection.sql`: removes bookkeeping triggers that would invalidate the schedule being exported.

Run `cargo xtask check -p lince-plugin-nostr -p lince-desktop -p lince-cell` and `cargo xtask test -p lince-plugins -p lince-plugin-nostr`. The relay tests use loopback WebSockets and isolated temporary data directories. They do not publish to public relays.

For document formats, run `cargo xtask test -p lince-plugin-files --tests`. See [Calendar exports from Karma simulation](ics-sync.md) for installation, Protein windows, per-Sync destinations, and calendar behavior.
