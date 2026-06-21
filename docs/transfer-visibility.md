# Visibility

Visibility affects every Transfer surface, but implementation is deliberately last. The Transfer shape, package format, interaction model, work metadata, and settlement behavior should settle before field-level filtering is enforced.

When this is implemented, the same subject/rule/field model must apply consistently to sand projections, packages, streams, public gossip caches, Transfer work metadata, item work metadata, interaction work metadata, messages, event payloads, parties, source Record references, quantities, locations, and settlement/projection views.

## Requirements

We need to control:

- Which Records can be seen.
- Which Transfers can be seen.
- Which Transfer items can be seen.
- Which properties of each Transfer or item can be seen.
- Which parties, personal Organs, or shared Organs can see them.

This must support field-level checkboxes. For example, a party may see `transfer_item.title` and `transfer_item.description`, but not `record.head`, `record.body`, `record_id`, exact quantity, location, or other parties.

## Tables

```sql
CREATE TABLE transfer_visibility_subject (
    id INTEGER PRIMARY KEY,
    subject_kind TEXT NOT NULL,
    local_user_id INTEGER,
    organ_id INTEGER,
    display_name_snapshot TEXT,
    CHECK (subject_kind IN ('user', 'organ', 'public'))
) STRICT;

CREATE TABLE transfer_visibility_rule (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL,
    scope_id INTEGER,
    can_discover INTEGER NOT NULL DEFAULT 0,
    can_view INTEGER NOT NULL DEFAULT 0,
    can_edit INTEGER NOT NULL DEFAULT 0,
    can_agree INTEGER NOT NULL DEFAULT 0,
    can_confirm_delivery INTEGER NOT NULL DEFAULT 0,
    can_confirm_receipt INTEGER NOT NULL DEFAULT 0,
    can_settle INTEGER NOT NULL DEFAULT 0,
    CHECK (scope_kind IN ('transfer', 'transfer_item', 'transfer_event', 'record'))
) STRICT;

CREATE TABLE transfer_visibility_field (
    id INTEGER PRIMARY KEY,
    visibility_rule_id INTEGER NOT NULL REFERENCES transfer_visibility_rule(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    visible INTEGER NOT NULL DEFAULT 0,
    editable INTEGER NOT NULL DEFAULT 0,
    redaction_label TEXT,
    UNIQUE (visibility_rule_id, field_name)
) STRICT;
```

## Field Examples

```text
transfer.title
transfer.description
transfer.quantity
transfer.status
transfer_item.role
transfer_item.title
transfer_item.description
transfer_item.quantity
transfer_item.unit
transfer_item.record_id
transfer_item.record_head_snapshot
transfer_item.record_body_snapshot
transfer_item.location
transfer_item.delivery_window
transfer_party.display_name
transfer_party.organ_id
transfer_event.message
record.head
record.body
record.quantity
```

## Function Example

```rust
pub struct VisibleTransferItem {
    pub id: i64,
    pub role: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub record_id: Option<i64>,
    pub record_head_snapshot: Option<String>,
    pub record_body_snapshot: Option<String>,
}

pub async fn visible_transfer_item(
    db: &SqlitePool,
    subject: VisibilitySubject,
    item_id: i64,
) -> Result<VisibleTransferItem, Error> {
    let item = load_transfer_item(db, item_id).await?;
    let fields = load_visible_fields(db, subject, "transfer_item", item_id).await?;

    Ok(VisibleTransferItem {
        id: item.id,
        role: fields.value("transfer_item.role", item.role),
        title: fields.value("transfer_item.title", item.title),
        description: fields.value("transfer_item.description", item.description),
        quantity: fields.value("transfer_item.quantity", item.quantity),
        unit: fields.value("transfer_item.unit", item.unit),
        record_id: fields.value("transfer_item.record_id", item.record_id),
        record_head_snapshot: fields.value("transfer_item.record_head_snapshot", item.record_head_snapshot),
        record_body_snapshot: fields.value("transfer_item.record_body_snapshot", item.record_body_snapshot),
    })
}
```

## UX Example

```text
Subject: Restaurant Organ

[x] Can discover this Transfer
[x] Can view Transfer title
[x] Can view Transfer item title
[x] Can view Transfer item description
[ ] Can view source Record id
[ ] Can view source Record head
[ ] Can view source Record body
[x] Can view quantity
[ ] Can view other invited Organs
[x] Can send messages
[x] Can agree
[x] Can confirm receipt
```

This supports the guitar example: the restaurant can contribute to the private `Play Guitar` Record without knowing the private Record head.

## Organ Proximity

When visibility is implemented, Organs should have a `proximity` or similar relationship-weight property. It should express how much this node cares about another Organ for visibility decisions.

Examples:

- a family Organ can receive broader discovery and Transfer visibility;
- a close collaborator Organ can see more Transfer context than a public stranger;
- a random public Organ may only see public proposals or redacted Transfer summaries.

This proximity value should not grant access by itself. It should be an input to the visibility rule UI/defaults so users can choose policies like "family can discover donation Transfers" while still relying on explicit visibility rules before fields leave the node.

## Enforcement

Visibility tables are not enough by themselves. Backend projection and package code must apply visibility before data leaves the server boundary.

Networking must treat visibility as a hard export boundary. Before any Transfer data leaves a node through package export, streams, contact discovery, public gossip caches, public-square indexing, or future delegated search, the visibility layer must decide:

- whether the requesting subject can discover that the Transfer exists;
- whether the subject can view the Transfer header;
- which structured parties are visible;
- which item and interaction fields are visible;
- whether source Record ids, heads, bodies, quantities, locations, work metadata, settlement/projection values, messages, and event payload fields are visible;
- whether a redacted field should be omitted, set to `null`, or replaced with a redaction label.

Contact discovery must stay separate from Transfer visibility. `/transfer/contacts/discover` should expose only explicitly discoverable Organ/contact metadata, never Transfer rows, Transfer summaries, Record data, work metadata, messages, settlement facts, or package event payloads.

Package export must use the same visibility path as UI projections. A package sent to a family Organ, a known collaborator, a public proposal endpoint, or a future public-square indexer should be generated from a subject-aware visible projection, not from raw Transfer tables.

Visibility is still the last Transfer feature to implement, but all networking features must remain compatible with it. New networking endpoints should avoid returning Transfer data unless they can call the visibility projection first.

Remaining enforcement work:

- Apply `transfer_visibility_rule` and `transfer_visibility_field` before data reaches sands, packages, streams, or public gossip caches.
- Redact source Record identity, Record head/body, parties, locations, quantities, messages, and event payload fields when a subject cannot view them.
- Make package export use the same visibility path as UI projections.
- Make package export reject or redact hidden work metadata, settlement/projection values, structured party rows, and structured interaction data.
- Keep contact discovery restricted to Organ metadata only.
- Add tests proving a party can see Transfer item title/description without seeing source Record head/body.
- Add tests for field-level visibility on structured Transfer summary/detail projections.

Here is the current map, separated by what exists in the docs/schema versus what your new idea adds.

1. Visibility Subjects

Current mapped subjects are:

- user: a local app user.
- organ: another Organ/node/contact.
- public: unauthenticated or broad public visibility.

For your use case, most decisions are organ visibility: “what can this specific Organ know or receive?”

2. Visibility Scopes

Current mapped scopes are:

- transfer: whole Transfer/header-level access.
- transfer_item: individual structured items or legacy contribution/need item fields.
- transfer_event: history/event payload visibility.
- record: source Record visibility.

I would extend this mentally to include package/send visibility, even if it reuses transfer rules:

- can_discover: Organ can know the Transfer exists.
- can_view: Organ can receive/view the visible Transfer projection.
- can_edit: Organ can propose/edit permitted parts.
- can_agree: Organ can agree.
- can_confirm_delivery
- can_confirm_receipt
- can_settle

3. Field Visibility

Mapped field examples include:

- Transfer fields: transfer.title, transfer.description, transfer.quantity, transfer.status.
- Transfer item fields: role, title, description, quantity, unit, source Record id, source Record snapshots, location,
  delivery window.

- Party fields: display name, Organ id, other party visibility.
- Event/message fields: event payload/message text.
- Record fields: record.head, record.body, record.quantity.

So the important distinction is:

- An Organ may discover the Transfer exists.
- It may view the Transfer header.
- It may view some item fields.
- It may not view the backing Record.
- It may not view exact quantities/location/other parties.
- It may receive a package, but that package must be built from this filtered projection.

4. Package Visibility

Current rule in docs: packages must not export raw Transfer tables. A package sent to an Organ should be generated
through the same visibility projection as the UI.

So package sending has two gates:

- Existence gate: can this Organ know this Transfer exists?
- Field gate: which fields are included or redacted?

That means “whole transfer being sent to them” should not mean “full Transfer package.” It should mean “a package view
for that Organ.”

5. Contact Discovery Is Separate

Already mapped: /transfer/contacts/discover only exposes Organ/contact metadata. It must not expose Transfers, Records,
work metadata, messages, settlement facts, or event payloads.

So discovering Organs and discovering Transfers are different systems.

6. Organ Proximity / Affinity

Docs currently mention proximity as a future Organ relationship-weight input. It should not grant access by itself.

Your new order system fits here, but I would name it separately from visibility:

- visibility: what an Organ is allowed to see.
- priority/affinity/proximity: when or how eagerly we offer it to that Organ.

Concrete model:

- Family Organ: high proximity, gets donation Transfers earlier.
- Close collaborator: medium/high proximity, gets richer context if explicit rules allow it.
- Random public Organ: low proximity, maybe only sees redacted/public proposals later.
- Blocked Organ: receives nothing.

So proximity should rank delivery, not bypass visibility.

7. Karma And Visibility

The docs say Karma does not invent visibility. I think that is still correct.

Karma can decide:

- when a Transfer becomes active;
- which Transfer quantity is available;
- which priority wave opens;
- maybe which Organ group is eligible now.

But after Karma selects a candidate Organ/group, visibility still decides what can be sent.

So “Organ allowed to view a Transfer with Karma” should mean:

1. Karma/order policy says this Organ is in the current offer wave.
2. Visibility says this Organ can discover/view the Transfer.
3. Package export sends only fields visible to that Organ.

4. Refusal / Triage

Your “received but not interested” idea is not visibility. It is recipient-side package triage.

I would model received package state like:

- received: package arrived and is stored.
- triage: needs user decision.
- not_planned: recipient saw it but is not interested now.
- interested: recipient wants to continue.
- accepted/duplicated: recipient consumed/duplicated it into a local Transfer.
- blocked/rejected: do not keep engaging with this sender/Transfer.

And optionally send a signed response event/message back:

- transfer_offer_received
- transfer_offer_not_planned
- transfer_offer_interested
- transfer_offer_declined

But I would keep this simple at first: local triage state only, then later add signed response events if useful.

My Suggested Visibility Categories

For implementation planning, I would map visibility into these types:

1. Discovery visibility: can this Organ know the Transfer exists?
2. Header visibility: can it see title/status/topic/basic summary?
3. Item visibility: can it see item roles/title/description/quantity/unit/location?
4. Record visibility: can it see source Record id/head/body/actual quantity?
5. Party visibility: can it see other parties/Organs/public keys?
6. Work visibility: can it see start/end/estimate/assignees/completion notes?
7. Agreement/event visibility: can it see history, messages, signatures, agreement state?
8. Settlement/projection visibility: can it see reserved/planned/available quantities?
9. Package export visibility: what exact filtered package is allowed to leave this node?
10. Offer priority/triage: not visibility, but controls ordering and recipient state.
