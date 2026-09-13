**Temporary discussion notes for Lince.lingua L595–600**

Reviewed on 2026-09-13 against the current working tree. Delete these notes when the feature is finished. The Record remains the source of truth. These notes distinguish the owner's decisions from recommendations and unresolved choices. They are not authorization to implement the feature. No feature code was changed or tests run for this review.

The intended experience is clear: the same Record can appear in a Castle, a card, a table or a conversation, and people can see each other's work. The wording currently combines several decisions: shared field bindings, text merging, structured property changes, presence, message authorship and the Full Record Castle itself. L596–599 overlap. “All properties use Loro” would be a stronger and more problematic requirement than “all supported properties can be edited together.”

**Owner's decisions from the refinement discussion**

- Any Sand that can be filled by a Record property can plug into the shared binding. The Full Record Castle is one user of that capability.
- Different properties use different change and sync rules. Text merging, quantities, slugs, relationships and work metadata should retain their respective meanings.
- Address the gaps in the existing collaboration path, including validation, permissions, persistence, delivery state and cursor integration.
- Keep message authorship and editing behavior as simple as possible. Explicitly report remaining message work when the implementation tasks are finished; do not silently present a partial result as complete.
- Allow accepted changes and Areas of Influence to move Sands under a person's cursor. Do not freeze sorting, filtering or physics because someone is editing.
- Route property updates through one backend change primitive. It applies the appropriate property rules and integrates with Sync, including reaching the origin where required, durable queues and retries.
- Reuse and enhance the existing Sync capabilities. Avoid redundant snapshots and operations, and avoid a separate delivery implementation for each Sand or property.

The previous suggestion to keep an active editor visibly stationary is withdrawn. Binding identity and pending work must still survive movement or removal of a view. Detailed conflict policies, offline acceptance rules and snapshot scheduling remain recommendations to refine below.

**What exists, and what it does not yet establish**

| Finding in current code | Architectural consequence |
| --- | --- |
| [collab.rs](../crates/engine/src/collab.rs) maintains a Loro document per Record with `head` and `body`. | Reuse the existing document identity and history. The feature is not starting with an empty backend. |
| [Property editors](../crates/interface/src/protein_area/rows.rs) submit full strings through Actions on focus loss. Dirty fields hold their local value and flag a conflict when refreshed. | This supports ordinary editing but does not yet merge simultaneous typing in the native interface. Applying a stale complete string to the latest server document can overwrite work the writer never saw. |
| [RecordBinding](../crates/interface/src/protein_area.rs) contains an Area entity, UID and source; writes require that Area to remain open. | A binding used by any Sand needs a lifetime independent of a spawning Area or row. Filtering a Record out of an Area must not erase unfinished work. |
| [collab_guard.rs](../crates/engine/src/collab_guard.rs) validates text operations, touched properties, dependencies and resource bounds using a provisional document. Its callers found in this review are tests. | The production collaboration path still imports binary updates directly into its cached document. Integrating admission checks is required work, not a property the current endpoint can be assumed to have. The guard accepts a constrained JSON operation format, so it is not a drop-in binary decoder. |
| [Session collaboration](../crates/transport/src/session.rs) sends full snapshots on relevant Facts. | Live typing needs bounded incremental delivery and a separate plan for subscription invalidation. A name such as `CollabChange` does not mean the payload is a small delta. |
| [Message creation](../crates/engine/src/actions.rs) stores `author`, `operator` and `state`; [Protein thread reads](../crates/protein/src/lib.rs) expose them. | Native attribution should use these fields. Audit every creation route: the separate `threads::send_message` helper does not attach the same lifecycle metadata. |

**One binding, with rules for each property**

Recommend one shared Record session per authenticated data context and Record UID, with property adapters used by every presentation. Its state covers accepted data, pending local edits, acknowledgements and current write capabilities. Each view keeps its own focus, scroll and selection. Source routing and identity must be explicit: two routes containing the same UID must not silently combine permissions or submit an edit twice.

The Full Record Castle composes these adapters. A Text Sand with a Record binding uses the same adapter; an unbound Text Sand remains local workspace text. A view receiving an update must not emit it again as a new user Action. Reusable Castles should store binding configuration without carrying another person's data or credentials into the template.

| Property | Recommended collaboration behavior | Decision still needed |
| --- | --- | --- |
| Title and description | Loro text operations, reflected during typing. | Plain text or Markdown source is a much smaller contract than a shared rich-text structure. The current guard rejects formatting operations. |
| Quantity | Exact decimal values through existing quantity Actions and Facts. | “Set to 7” and “add 2” are different intentions. Two offline sets must not accidentally become two additions or an unexplained overwrite. |
| Slug | Submit a complete valid value and check uniqueness atomically. | Define the namespace across Organs and what happens when offline peers claim the same slug. Current local storage has a unique slug column. |
| Assertions and assignees | Add/remove individual relationships through their Actions. | Define concurrent add/remove behavior and coupled changes such as replacing one exclusive status with another. |
| Dates and work metadata | Change specific typed fields while validating related values together. | The current editor reconstructs a whole `work` extension. A stale save can overwrite another person's change to a different key; use server-side conflict checks or field updates. |
| Work logs and other specialized properties | Use their domain operations and lifecycle rules. | List which fields are editable, calculated or locked. “Full Record” must not expose a generic way to bypass Transfer, message or other Record-kind rules. |

For structured fields, I recommend keeping incomplete input local and sharing the accepted value immediately after a valid commit. Showing that another person is editing the field can be separate from publishing their unfinished number or date. Local conflict detection alone cannot close a race; the backend must validate against the state it actually commits.

Loro itself distinguishes merging from transactional rules, uniqueness and authorization. This supports a common editing experience with different storage and validation rules per property. [Loro: when CRDTs are insufficient](https://www.loro.dev/docs/concepts/when_not_crdt).

**Durability and authorization belong at the write boundary**

An incoming edit should be inspected on provisional state, checked against the authenticated actor and actual touched properties, committed durably with its sync information and query projection, and only then acknowledged as saved. The current cached document is mutated before later database writes; separate writes can fail after memory has advanced. Client retries and restart need to recover the same accepted state without duplicate effects.

Use the same enforcement for native clients, remote sessions, sync import and non-interactive writers. A client-supplied Loro peer ID is not proof of authorship. Roles, Record visibility, specialized locks and revocation must still apply. A rejected operation must not remain hidden in the accepted CRDT history and become visible after a later merge.

The existing `head`/`body` document is also a sharing boundary: `validate_scope` currently requires both fields or neither in a sync scope. Retain that rule unless we deliberately redesign document boundaries. Merely hiding a property in the Castle does not prevent its content or retained history from being sent in a snapshot. Record presence must be filtered by current access too.

Sharing with an owner does not automatically imply sharing with everyone the owner knows. Specify which existing grant and sync path carries an edit to each participant, particularly the A → owner → B case. Direct live access and an authorized local replica have different offline behavior. Cursor transport must be explicitly routed; durable Record sync does not automatically carry presence.

The interface needs to distinguish pending, saved locally, awaiting remote acceptance and rejected changes where those states apply. “Instantly” should mean immediate local feedback and prompt propagation when connected, without presenting disconnection as success. After revocation or rejection, preserve the person's unsent input for recovery under the agreed privacy rules rather than silently discarding it or replaying it under a different login.

**Cursors and undo need actual editor integration**

Represent a cursor by Record, property, session and stable text anchors. Canvas coordinates are unsuitable because the same property can be wrapped and positioned differently in every view. Loro provides anchors that follow edits. Presence should expire on departure or inactivity and should not create Records, Facts or durable edit history. [Loro cursors](https://www.loro.dev/docs/tutorial/cursor), [ephemeral presence](https://loro.dev/docs/tutorial/ephemeral).

The existing generic Lane API checks stream permission; it does not establish Record-specific visibility from a room name. Reusing its transport requires Record-scoped authorization, authenticated names, bounded updates and revocation handling. Decide whether read-only viewers appear, whether a person can hide presence, and how two devices belonging to one person are displayed.

The native adapter must preserve cursor position and selection when remote edits arrive, handle input-method composition and asynchronous paste, and convert text positions correctly. The installed Rust Loro API defaults to Unicode scalar positions; editor integration also encounters byte positions and grapheme clusters. Test accented text, combining marks, emoji and right-to-left text.

Undo should undo this editing session's operations while retaining other writers' work. Decide whether Ctrl+Z follows the active field or the whole Record across its views. A shared server peer cannot by itself supply each person's local undo history. Loro's UndoManager tracks one peer, and peer IDs must be unique to concurrent editing sessions. [Local undo](https://www.loro.dev/docs/advanced/undo), [peer identity](https://www.loro.dev/docs/concepts/peerid_management).

**Authorship survives the editing session**

Message attribution should identify the original author, the operator when acting through an Agent, and the originating Organ when useful. Resolve names through authorized data and provide an honest fallback when identity details are unavailable. Never label a message with the last editor's name as though they originally wrote it.

Keep this part simple as the owner requested. Recommend displaying the stored author and following existing backend permissions and lifecycle rules, without adding a new message-specific role system or a full edit-history interface for this feature. Any remaining gaps in consistent creation-time attribution, finished-message editing or Agent attribution must be named when reporting completion. The shared binding must respect writing/finished/interrupted states. A generic Record editor must not become a route around the specialized message Action's operator checks. Private drafts stay private until the send operation makes them shared; reusing an editor does not reuse the publication policy.

**Cost and failure cases that affect the design**

The engine currently has a global import lock and a global document-registry mutex. Snapshot generation, history validation and query refresh under rapid typing can become expensive. Share active sessions between views, keep work off the render thread, batch small edits, prioritize durable edits over expendable presence, and release idle subscriptions. Measure the current path before replacing its locking model.

The guard bounds peers and retained history; these are lifetime limits for the document history, not just currently connected people. Fresh session IDs accumulate. Full snapshots preserve history, so the existing compaction should not be described as solving those limits. Decide the behavior at the bound and preserve unsent work. Shallow snapshots are separately listed in the Record; this review does not assume them as part of this feature.

Define how accepted typing triggers Facts, Protein refreshes, Karma and Areas of Influence. The owner allows a title-based filter, sort or force to move or remove a field while someone types. Bind edits to the Record UID and property rather than a row index, screen location or recycled entity. Preserve pending work independently of the view, and let the normal layout and physics react to accepted changes. Tentative remote requests must not trigger committed domain effects before the chosen authority accepts them.

Useful acceptance cases are: the same field in two views; two people editing overlapping text; two structured edits based on the same old value; duplicate or reordered delivery; revocation during typing; a crash between receiving and acknowledging an edit; reopening with pending work; a filtered-out active row; cursor expiry; and message authorship after another authorized person edits the text. Include an idle workspace and a large visible list when measuring cost.

L602's displaced-change recovery and L604's private drafts are adjacent requirements. The existing short-lived change store keeps up to 50 entries per Record for seven days, but it is not a complete collaborative undo or rejected-draft recovery system. Agree on the minimum recovery behavior here without silently claiming those later tasks are complete.

**Further architectural refinements after the owner's response**

One backend change entry point fits the requested direction. Recommend placing it in Engine alongside Actions. It accepts a typed intention, checks and applies the property's rules, and hands durable delivery to Sync. Storage supplies transaction helpers, transport supplies authenticated connections, and the interface consumes the result. A general `set(property, JSON)` that skips existing Actions would lose quantity, relationship and lifecycle rules.

The common request needs a stable change ID, Record UID, typed property operation and any required expected version. Authenticated actor and authority context are supplied or verified by the backend. Local authoring and accepting an existing remote change can share validation and commit machinery while preserving the original operation identity. Receiving a change must not mint a fresh local edit, replay a quantity Action, or create a new forwarding loop.

The local state, accepted change, deduplication result and outgoing delivery work should commit together. For remote-authoritative writes, the local transaction instead stores the pending intention; only the authority's acceptance produces canonical state. Refresh notifications follow durable commit. [Transaction-aware sync logging](../crates/store/src/sync_ops.rs) and [Area transitions](../crates/engine/src/area_transition.rs) already provide useful transaction and request-replay patterns. This is an integration of existing domain paths, not a requirement to redesign every unrelated Lince Action before building property editing.

**What the current Sync implementation already provides**

- [sync_ops.rs](../crates/store/src/sync_ops.rs) persists operations and an outbox, selects contacts or accepted replica grants, and has transaction-aware logging helpers. Acknowledgement cleanup includes the sent sequence so it does not delete a newer queued change to the same field.
- [drain_outbox](../crates/engine/src/sync.rs) rechecks sharing, groups delivery, retains failed work and records attempts. [Wire delivery](../crates/engine/src/wire.rs) already tries a live connection, then another connection or the configured mailbox path. Reuse this path for prompt updates and reconnect recovery.
- [The Cell runner](../crates/cell/src/sync_runner.rs) wakes on Facts and has a timed catch-up path. It currently waits 250 ms to collect notifications after a Fact. If we decouple change notification from Facts, explicitly wake Sync from the durable change path so edits do not wait for unrelated activity.
- [SyncService](../crates/engine/src/sync_service.rs) currently wraps activity reporting and queue observation. The retry and operation logic lives in the other modules. Calling something through SyncService alone does not make it a durable mutation or a retriable request.

**Incremental delivery needs a receiver baseline**

Recommend treating the outbox as a durable indication that a recipient lacks changes. For collaborative text, construct a bounded delta containing everything missing from the recipient's acknowledged document version. Persist the underlying history until the delivery and retention rules allow removal. A simple latest-entry queue can remain useful as a scheduling index, but it must not become the only retained copy of the newest small delta.

Today, the outbox coalesces by contact, table, UID, field and operation kind. The current collaboration code exports a cumulative tail from its snapshot version. Replacing that tail with only the newest keystroke delta without changing delivery coverage would omit earlier edits and dependencies. Compaction also changes the tail's baseline; it cannot advance a recipient's acknowledged state. The engine's sync-operation progress and Loro's document version are different things and must not be substituted for one another.

| Situation | Suggested payload |
| --- | --- |
| First opening without a usable document baseline | An authorized snapshot at a known version, followed by changes after that version. |
| Connected editing or reconnect with retained history | Missing operations, batched and bounded, from the receiver's acknowledged version. |
| Receiver already has the change | Acknowledgement without another mutation or unnecessary state transfer. |
| Required history is unavailable | Explicit recovery using a snapshot, preserving pending local work before replacing any client state. |
| Cursor moved | Only the latest expiring presence update; no durable retry queue. |

Generating a storage checkpoint does not require broadcasting it. A joined document whose text version did not change needs no text payload when quantity, assertions or presence change. Snapshot creation and subscription must share a version boundary so a racing edit is covered by either the snapshot or the subsequent delta. Snapshot history still follows the sharing scope described above.

Some retransmission is necessary when an acknowledgement is lost. The guarantee should be that the same change has one effect and an old acknowledgement cannot clear newer work. Do not interpret a queued mailbox package as acceptance by the origin.

The current wire reply carries an `Applied` count, and import can skip or quarantine individual operations while returning a successful batch result. That is insufficient for telling an editor that a particular change was accepted. Recommend receipts for a change or a precisely covered range, distinguishing applied, already present, awaiting dependencies and refused. A refusal must retain a useful reason for the writer instead of disappearing when the batch outbox entry is cleared.

**The origin and the operation's author are different identities**

Sync should resolve where an update belongs from the Record's authority and existing sharing configuration. Preserve the Record origin, acting Person, writing Cell and change identity separately. A bound Sand should never calculate routing or assume that the Organ serving a read is authorized to accept every kind of write.

For text, an authorized replica can keep locally durable operations while disconnected and merge later under the agreed access rules. For operations needing an origin decision, such as a unique slug claim or an absolute quantity change, recommend saving an intention locally and confirming the canonical result at that authority. Applying a quantity delta locally and later trying to undo it after refusal can already have fired Karma or changed derived quantities.

The existing outbox principally carries committed operations. It is not automatically a queue for unaccepted commands. Pending origin requests and accepted operations can belong to the same Sync subsystem, but they need distinct states. Session authentication also changes on reconnect: keep a stable application change ID while renewing the authenticated envelope. The existing signed-Action sequence protects a connection from replay; it does not replace durable request deduplication across new connections.

The A → origin → B case needs an explicit delivery test. The currently inspected import path records incoming operations without ordinary outbox enqueue, and admission checks bind operation authorship to the sending Organ. Therefore, simply forwarding someone else's batch or stamping it as the origin's own write is not a complete relay design. Choose an origin-accepted change representation with preserved authorship, or an explicitly authorized relay path. This is a concrete Sync gap to resolve for this feature's promised audience, without widening sharing to unrelated contacts.

**Suggested simple conflict policy, awaiting the owner's answer**

For absolute structured-field changes, send the expected field version with the new value. If the field still matches that version, apply it. If the requested result is already current, acknowledge no further effect. If another change intervened with a different result, keep the accepted value and hold the intention for review. Check a relevant field version rather than the entire Record so a title edit does not reject an unrelated date change. Related changes such as an assertion replacement can share one atomic request.

This policy makes two changes from 5 to 7 end at 7, while different requests for 7 and 9 become an explicit disagreement. An addition remains an addition and each accepted request applies once. The owner has been asked whether competing structured changes should instead let the second accepted value replace the first; no answer is assumed here. This policy does not promise global serialization across independent offline authorities: operations that require a single decision must identify where that decision is made.

For the simpler message scope, recommend showing the existing stored author with a fallback label, preserving that attribution during edits, and using the existing permission and lifecycle path. Report missing specialized edit controls or Agent details as remaining work. This does not defer enforcement of existing permissions or draft privacy.

**Wording to discuss**

Any Sand that can show a Record property can use the same binding to edit it. The Full Record Castle composes these bindings. Titles and descriptions can be written together, with visible cursors. Other properties share valid changes through their own rules. One backend change path applies these rules and uses Sync for delivery, saved queues and retries, reaching the origin when needed. Each field shows whether a change is saved, pending or refused. Messages show their stored author. Sharing and editing follow the Record's permissions wherever it appears. Normal sorting and Areas of Influence may move a Sand while it is being edited.

This wording still needs decisions about offline acceptance, conflicting structured changes, structured-field commit gestures, description formatting and the Full Record Castle's exact property list. Message work is deliberately kept simple and remaining gaps must be reported.
