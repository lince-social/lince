# Rooms

A Room is a Conversation that several people share instead of two. It is the backend half of Discord-like communication: the data model for multi-person Conversation semantics and membership disclosure. The surface a person sees and clicks — the conversation list, the call view, the participant grid, screen-share consent, recording controls — is the Communication Sand, specified in `anicca/interface/communication.md`. This file is the truth that surface inherits and cannot renegotiate.

## Why this lives here and not in the Sand notes

The honesty constraints below are properties of the replica layer, not of a widget. Any surface — the Communication Sand, a terminal client, an agent — meets them unchanged, so they belong beside Ontology as backend truth rather than inside a presentation spec. The interface notes also answer to no Record: their own header says "no dedicated Sands Record currently exists." Rooms is destined for a `.lingua`, so the model states its limits once, here, and the Sand references them.

## What is already true

`replica_grant` is keyed `PRIMARY KEY (root_record, contact_organ)` at `0042_individual_replica.sql:45`, so one root holds an independent grant row per contact — each with its own offered or accepted state — and revoking one deletes one row. Containment is settled separately: `record.replica_root` (`0042_individual_replica.sql:17`) points every Record at its root, resolved once at creation, so a grant covers a whole tree without walking assertions. What is still two-party is the layer above: `start_conversation` in `crates/engine/src/threads.rs` takes a single `contact_organ` and calls `replica::offer` once, and `accept_conversation` / `revoke_conversation` each act on one contact. The multi-person Conversation handlers exist in `crates/engine/src/communication.rs` (`communication_create`, `communication_join`, `communication_leave`, `communication_close`, `communication_recording`, `require_communication_participant`); the crate has not been built against the current tree.

## Three things a peer-to-peer Room cannot pretend about

Each is a design consequence, not a caveat.

Membership is not a list. Each grant is independent, nobody can enumerate who else holds the root, and there is no agreement on who is in. Somebody has to state it, and the honest answer is that the Room's creator does, as a Record everyone can read and disagree with.

Removal is not deletion. Revoking a grant stops future ops; it does not recall what was already delivered. A Room that implies a moderator power it does not have is worse than one that admits it has none.

Fan-out is not free. Every message becomes an op to every outbox, so chatter in a five-person Room is five times the traffic. That is why ephemeral traffic — presence, typing, call signaling, media negotiation — stays off the Record op-log path rather than being a performance note.

A person about to type into a synced thread should be able to see that it is not local, which makes "who can see this and who is in it" part of the Room rather than a settings screen.

## Conflict to resolve — the durable Room shape

`Lince.lingua:130` lists `conversation`, `thread`, `message`, `thread-invite`, and `call-session` among the Record kinds, and `threads.rs:20` creates a `RecordKind::Conversation`. The interface notes disagree: `anicca/interface/plans/communication.md` says not to introduce a `conversation` kind "merely for filtering", preferring an `@communication` assertion that promotes any Record, with `call-session-of` as a predicate rather than a kind. The Record is the higher source of truth; this file does not resolve the disagreement, only records it for the owner.

## Work left

- [ ] Fan one Conversation root to several contacts: add the multi-contact form of `start_conversation` / `accept_conversation` / `revoke_conversation` so one root offers to and accepts from several contacts, rather than the single `contact_organ` the engine takes today.
- [ ] Write membership as a Record the creator authors and everyone in the Room can read and disagree with — a claim about who the creator believes is in, never read back as the grant table.
- [ ] Detect drift between that membership Record and the actual `replica_grant` rows: nothing reconciles them, so a revoked contact still named, or a granted contact never named, goes unnoticed. Either surface the difference or state plainly it is unsurfaced.
- [ ] State in the model that revoking a grant stops future ops and recalls nothing already delivered, so no surface can imply a moderator power the replica layer does not have.
- [ ] Carry "who can see this thread and who is in this Room" as readable Record state on the thread and Conversation, not a settings screen, so a person typing into a synced thread sees it is not local.
- [ ] Choose the transport for ephemeral Room traffic — presence, typing, call signaling, media negotiation — and keep it off the Record op-log path, because every message is already one op per outbox and a five-person Room multiplies that. The decision names a reason but no path yet.
- [ ] Encrypt Conversation, thread, message content and live-room key material to the authorized group across Organs; relays route ciphertext and never become participants by transporting it.
- [ ] Name the signaling authority for a mixed-Organ Room: the Conversation Record's origin Organ.
- [ ] Keep recording bytes at the producing storage owner; replicate only access-controlled metadata and resource references.
- [ ] Decide a ceiling on Room size, or state there is none and why the fan-out cost is acceptable at the sizes allowed. The interface notes defer this (F1); it is an open backend decision.
- [ ] Reconcile the durable Room shape with `Lince.lingua` Record kinds versus the `@communication`-assertion proposal in the interface notes, and land one.

## What goes to the interface notes instead

These are not Room work; they belong in `anicca/interface/communication.md` because they do not survive deleting every Sand: the participant grid, the call view and call bar, join-audio / join-video affordances, WebRTC device handling, screen-share capture with its per-share consent dialog, the recording-consent dialog and retention-policy display, and controls that auto-hide after idle. The backend model bullets currently sitting in `anicca/interface/plans/communication.md` — the "Rust owns the durable control plane" split, the `@participant` / `call-session-of` / `call-recording` predicate definitions, and the `communication.v1` extension shape — are candidates to move here, but that is the owner's call and is left as a proposal, not done.

Box count: 11, against the source's 4. Every unbuilt item in the source prose was swept in and appears once; the media/UX items were routed out rather than dropped.
