# Transfer Sand

This note turns the current placeholder Transfer sand into a concrete product surface. It is about the sand and the backend contract it needs, not the whole Transfer theory.

## Current State

The existing Transfer sand is a hello-world placeholder:

- Source: `crates/web/src/sand/transfer/mod.rs`
- Feature flag: `sand.transfer`
- Package filename: `transfer.html`
- Runtime type: `OfficialWidgetBuilder::Html`
- Server requirement: `requires_server: false`
- Permissions: none
- Scripts: none
- Body: static `Transfer` text

The current database shape is also only a starter:

- `transfer` has `id` and `quantity`.
- `transfer_item` links one contribution side to one need side and stores `first_agreement`, `second_agreement`, `date`, and `location`.

That is enough to prove the table exists, but not enough for the real workflow. A useful sand cannot safely be built only by sending generic table writes, because agreement invalidation, visibility, event history, and settlement are application invariants.

## Correct Placement

The current notes placement is correct:

- The main tracker is `notes/TRANSFER.md`.
- The old consulted draft is `notes/OLD_TRANSFER.md`.
- The simulation file is `notes/transfer-simulation-settlement.md`.
- This sand-specific note belongs beside the other split Transfer notes.

## Product Role

The Transfer sand should be the operational UI for Transfers. It should not be a marketing page, a generic table editor, or a standalone demo.

Its job is to let a person:

1. See Transfers that matter to their Cell or Organ.
2. Create a Transfer proposal.
3. Add Needs, Contributions, support items, tasks, information, or reservations.
4. Connect items through interactions and dependencies.
5. Decide who can see and edit which fields.
6. Reach agreement according to the chosen policy.
7. Track messages and history.
8. Confirm delivery and receipt.
9. Settle the Transfer into real Lince data changes.
10. Inspect quantity influence when a sand or SQL view exposes it.

## First Real Version

The first useful sand should be server-backed.

Manifest direction:

```rust
PackageManifest {
    icon: "⇄".into(),
    title: "Transfer".into(),
    author: "Lince Labs".into(),
    version: "0.1.0".into(),
    description: "Create, review, agree, and settle Lince Transfers.".into(),
    details: "Server-backed Transfer workflow for Lince Records, parties, visibility, agreement, history, and settlement.".into(),
    initial_width: 8,
    initial_height: 6,
    requires_server: true,
    permissions: vec![
        "bridge_state".into(),
        "read_transfer_stream".into(),
        "write_transfer".into(),
    ],
}
```

The exact permission names can change. The important design point is that Transfers deserve a dedicated permission and backend action surface. `write_table` may be useful for early prototypes, but it is too broad and too weak for final Transfer behavior.

## Proposal Versus Settlement

The Transfer proposal must be separate from final Record quantity mutation.

Creating or editing a Transfer should create Transfer data:

- Transfer header.
- Transfer items.
- Interactions.
- Agreement rows.
- Event rows.
- Optional quantity influence rows.

It should not immediately subtract from one Record and add to another Record.

The Record quantity mutation happens only when settlement is applied. Settlement is the point where the system says the social process has completed and the Transfer is now allowed to affect real Lince data.

For an apple example:

```text
Before proposal:
  my_cell.apple.quantity = 10
  lince_server_organ.apple.quantity = 3

Proposal:
  item A: my Cell contributes 1 apple
  item B: Lince server Organ receives 1 apple
  influence: my apple -1, server Organ apple +1

During proposal/agreement:
  my_cell.apple.quantity is still 10
  lince_server_organ.apple.quantity is still 3
  Transfer detail shows planned movement: -1 / +1

After settlement:
  my_cell.apple.quantity = 9
  lince_server_organ.apple.quantity = 4
```

This keeps a proposed Transfer from accidentally becoming reality before the parties agree and confirm what happened.

## Hello World Apple Transfer

The first implementation slice can be:

```text
my Cell proposes one apple Transfer to the Lince server Organ.
my local Lince coordinates the Transfer.
my Cell: -1 apple
Lince server Organ: +1 apple
```

This should be local-coordinator-backed, not central-server-backed and not full p2p yet.

Recommended hello-world flow:

1. The Transfer sand opens in my local Lince.
2. Local Lince identifies my Cell subject and the Lince server Organ subject.
3. The user picks or creates an `Apple` Record for their Cell.
4. Local Lince has, fetches, or creates a visible placeholder for the Lince server Organ's `Apple` Record.
5. The user creates a Transfer proposal coordinated by local Lince.
6. Local Lince appends `transfer_created`.
7. Local Lince appends item/interaction events for `my_cell.apple -1` and `server_organ.apple +1`.
8. The user agrees as their Cell.
9. The Lince server Organ agrees by sending or exposing a visible agreement event, or through a temporary local demo action.
10. Local Lince records delivery and receipt confirmation, or uses a temporary one-click confirm action.
11. Settlement applies the Record quantity changes exactly once.
12. The sand shows the event history and the before/planned/after quantities.

For this first slice, local Lince is the coordinator and operational source of truth for the Transfer event order. The Lince server Organ is a participant. It may store a mirror of the visible Transfer events, but it does not need to be the central coordinator.

If the server Organ cannot actually receive the proposal yet, the first demo can represent its participation locally as a subject/party. That should be treated as a temporary stand-in for the later networked participant path.

What the hello world should include:

- A coordinator subject or local node id on the Transfer.
- Transfer event rows for every state change.
- A sync cursor shape, even if it only syncs locally at first.
- Agreement as explicit data.
- Settlement as an idempotent backend action.
- Planned quantity influence separate from settled quantity mutation.

What the hello world can skip:

- Direct Cell-to-Cell HTTP sync.
- Signed events.
- Discovery caches.
- Field-level visibility UI, if the subjects are both local and demo-visible.
- Nested Transfers.
- Complex agreement policies.
- A central coordinator.

## Runtime Contract

The sand should get a contract from the host, similar in spirit to the Kanban runtime contract.

The contract should include:

- Widget identity.
- Server and Organ identity.
- Authentication state.
- Declared permissions.
- Supported Transfer actions.
- Current source mode: local server, Organ server, or later peer/coordinator.
- Whether streams are enabled.
- Liveness intervals.
- Data contract version.
- Known enum values for rendering controls.
- Optional diagnostics.

Example shape:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferWidgetContract {
    pub widget: TransferWidgetMeta,
    pub source: TransferWidgetSource,
    pub permissions: TransferWidgetPermissions,
    pub data_contract: TransferDataContract,
    pub actions: Vec<&'static str>,
    pub liveness: TransferLivenessContract,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDataContract {
    pub agreement_types: Vec<&'static str>,
    pub settlement_modes: Vec<&'static str>,
    pub item_roles: Vec<&'static str>,
    pub interaction_kinds: Vec<&'static str>,
    pub confirmation_kinds: Vec<&'static str>,
}
```

The browser should render from this contract instead of hardcoding storage strings.

## Backend Actions

The sand should call typed backend actions. The backend actions should parse JSON into Rust request structs, then convert storage strings into Rust enums at the boundary.

Needed actions:

- `list-transfer-summaries`
- `load-transfer-detail`
- `create-transfer`
- `update-transfer`
- `create-child-transfer`
- `add-transfer-party`
- `remove-transfer-party`
- `set-transfer-visibility`
- `add-transfer-item`
- `update-transfer-item`
- `remove-transfer-item`
- `add-transfer-interaction`
- `update-transfer-interaction`
- `remove-transfer-interaction`
- `add-transfer-dependency`
- `remove-transfer-dependency`
- `agree-transfer-scope`
- `reset-transfer-agreement`
- `add-transfer-message`
- `confirm-transfer-delivery`
- `confirm-transfer-receipt`
- `settle-transfer`
- `cancel-transfer`

The actions should not directly trust client-side agreement state. They should load the current Transfer state, validate the actor, append the correct event, and return the updated detail or tell the sand to wait for the stream.

## Streaming

The sand should receive Transfer updates from a stream.

Two useful stream shapes:

- Summary stream: all visible Transfer summaries for the current server or selected source.
- Detail stream: one Transfer event log or materialized detail snapshot.

The stream should support:

- Created Transfers.
- Edited Transfers.
- Added or removed parties.
- Visibility changes.
- Item changes.
- Interaction changes.
- Dependency changes.
- Agreement changes.
- Messages.
- Delivery and receipt confirmations.
- Settlement.
- Cancellation.

The event log is the better long-term source, but a materialized snapshot is easier for the first UI.

## Main Views

The sand needs these views.

### Index

The index shows visible Transfers.

Useful groups:

- Draft
- Active
- Waiting for me
- Waiting for others
- In transfer
- Ready to settle
- Settled
- Cancelled

Useful summary fields:

- Title.
- Quantity or active marker.
- Parent/child indicator.
- Agreement state.
- Last event.
- Visible parties.
- Open confirmations.
- Current actor permissions.

### Detail

The detail view is the main workspace for one Transfer.

It should show:

- Header.
- Parent and children.
- Parties.
- Items.
- Interactions.
- Dependencies.
- Agreement state.
- Visibility state.
- Messages.
- History.
- Confirmations.
- Settlement state.
- Quantity influence when present.

### Create Proposal

The create flow should support:

- Empty Transfer.
- Transfer from one Record.
- Transfer from multiple selected Records.
- Child Transfer under a parent.
- Copy visibility defaults from parent.
- Choose agreement type.
- Choose settlement mode.
- Set initial quantity, normally neutral `0`.

### Item Editor

The item editor should keep source Record fields separate from Transfer-specific fields.

It should support:

- Source Record id.
- Role in this Transfer.
- Transfer item title.
- Transfer item description.
- Quantity.
- Unit or extension namespace.
- Location or delivery metadata when applicable.
- Record head snapshot.
- Record body snapshot.
- Visibility toggles for every sensitive field.

This is what allows the guitar example: a private `Play Guitar` Record can be used in a public restaurant Transfer while exposing only the Transfer item title.

### Interaction Graph

Transfers with many items need a visible interaction model.

The sand should show:

- `A -> B` contribution paths.
- Dependencies.
- Blocked/unblocked state.
- Connected items affected by an edit.
- Whether each connected scope can agree or settle independently.

This can start as a list and later become a graph.

### Agreement Panel

The agreement panel should make agreement scoped and explainable.

It should show:

- Agreement policy for the Transfer or child Transfer.
- Current actor's visible scope.
- Which items/interactions are already agreed.
- Which agreements were reset by later edits.
- Which parties still need to act.
- Whether the policy activates the Transfer.

The backend must own agreement invalidation. The frontend can show it, but should not be the source of truth.

### Visibility Panel

Visibility must be a first-class panel, not a hidden advanced setting.

The UI should let a permitted actor choose:

- Can discover.
- Can view.
- Can edit.
- Can agree.
- Can confirm delivery.
- Can confirm receipt.
- Can settle.
- Per-field visible/editable flags.

Field examples:

- `transfer.title`
- `transfer.description`
- `transfer.quantity`
- `transfer.status`
- `transfer_item.title`
- `transfer_item.description`
- `transfer_item.quantity`
- `transfer_item.record_id`
- `transfer_item.record_head_snapshot`
- `transfer_item.record_body_snapshot`
- `transfer_party.display_name`
- `transfer_event.message`
- `record.head`
- `record.body`
- `record.quantity`

### History And Messages

History and messages are related but not identical.

History should show append-only facts:

- Created.
- Edited.
- Agreement reset.
- Agreement raised.
- Delivery confirmed.
- Receipt confirmed.
- Settled.

Messages should be human communication inside the Transfer:

- Transfer-level messages.
- Interaction-level messages.
- Visibility-aware messages.

### Settlement Panel

Settlement should not be a hidden consequence of pressing agree.

The panel should show:

- Required delivery confirmations.
- Required receipt confirmations.
- Settlement mode.
- Quantity changes that will become final.
- Whether settlement already happened.
- Idempotent settlement result.

## Data Needed Before The Sand Is Complete

The current schema needs more structure before the sand can be fully useful.

Needed tables or table changes:

- Transfer header with parent, title, description, agreement type, settlement mode, coordinator, timestamps, and neutral quantity default.
- Transfer party.
- Transfer item with item id, role, source Record, snapshots, Transfer-specific title/description, quantity, unit, location, and metadata.
- Transfer visibility subject/rule/field.
- Transfer interaction.
- Transfer dependency.
- Transfer event.
- Transfer message.
- Transfer confirmation.
- Transfer settlement.
- Transfer quantity influence.
- Transfer sync cursor or replica marker.

Those are described in the other notes. This file only states what the sand needs from them.

## Validation Rules The Sand Depends On

Backend rules needed for a trustworthy sand:

- Every action checks actor permission.
- Storage strings parse into typed Rust enums.
- Invalid enum values are rejected before database writes.
- Editing a connected item resets only affected agreement.
- Agreement is scoped to item, interaction, child Transfer, or full Transfer according to policy.
- Settlement can run more than once without double-applying quantity changes.
- Event history is append-only.
- Proposal events can exist without mutating source Record quantities.
- Only settlement mutates final Record quantities.
- Coordinator events can be mirrored by participating Cells.
- A stale client cannot overwrite a newer Transfer state silently.
- Visibility filtering happens before data reaches the sand.

## Growth Path

The sand can grow in stages.

### Stage 0: Current Placeholder

Static text only.

### Stage 1: Read-Only Inspector

Server-backed sand that can list existing `transfer` and `transfer_item` rows.

This can use the current minimal schema, but it should be treated as inspection, not the real workflow.

### Stage 2: Dedicated Contract And Actions

Add a Transfer runtime contract and typed backend actions.

The UI can then create a Transfer through rules instead of raw table writes.

### Stage 3: Apple Hello World

Implement one coordinator-backed Transfer:

- My Cell proposes `-1 apple`.
- Lince server Organ receives `+1 apple`.
- My local Lince is the coordinator.
- Transfer event history records the proposal, agreement, confirmation, and settlement.
- Planned quantity influence is visible before settlement.
- Final Record quantity changes happen only at settlement.
- The coordinator event log can be mirrored through a sync cursor shape, even if direct p2p sync is not implemented yet.

### Stage 4: Proposal Builder

Add Transfer creation, item editing, and source Record selection.

### Stage 5: Parties And Visibility

Add party management and field-level visibility.

This is the first stage where Transfers begin to match the real privacy model.

### Stage 6: Interactions And Agreement

Add item interactions, dependencies, agreement scopes, and agreement invalidation.

This is the first stage where multi-party Transfers become meaningfully different from table rows.

### Stage 7: Events, Messages, And Confirmation

Add event history, messages, delivery confirmation, and receipt confirmation.

### Stage 8: Settlement And Influence

Add settlement actions and show quantity influence facts.

### Stage 9: Discovery And Networking

Add discovery caches, peer summaries, and coordinator/source display.

Direct Cell-to-Cell sync can come after a local-coordinator-backed version works.

## Minimum "Makes Sense" Scenario

The sand starts making sense when this scenario works:

1. A person opens the Transfer sand on a server.
2. They create a Transfer from a private Record.
3. They give the Transfer item a public title and description.
4. They hide the source Record id, head, and body from the other party.
5. They invite or select another Organ as a party.
6. They create an interaction between the Need and Contribution.
7. Both parties agree to the visible current state.
8. A later edit resets the connected agreement.
9. Both parties can see why agreement changed.
10. Delivery and receipt are confirmed separately.
11. Settlement applies the final Lince data change once.
12. The event history explains what happened.

Until that works, the sand is still a prototype.

## Open Decisions

- Whether the official Transfer sand should stay `Html` or become an archive package like larger sands.
- Whether the first stream should be event-log based or snapshot based.
- Whether final permissions should be `read_transfer_stream` and `write_transfer`, or reuse existing permissions temporarily.
- Whether current `transfer_item` should be migrated directly or replaced with a richer table.
- Whether the first UI should focus on one selected Transfer or a dashboard plus detail split.
