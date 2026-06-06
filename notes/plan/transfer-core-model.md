# Core Model

This note carries the product and conceptual model from `OLD_TRANSFER.md`.

## Current Direction

- Transfer deals only with Lince data for now.
- Quantity stays central. Services, promises, knowledge, tasks, permissions, and transportation can still be represented in Lince through Records, Record bodies, Transfer items, Karma, and Transfer history.
- A Transfer is a process and a grouping of smaller interactions, not necessarily one indivisible transaction.
- Transfers can be nested under a larger parent Transfer.
- Visibility is a first-class table, not a loose property.
- Agreement is invalidated by edits to the connected items that were agreed to.
- Karma should initially only activate/deactivate preconfigured Transfers by changing Transfer quantity, not invent parties or visibility.
- Transfer should expose enough data for SQL and sands to calculate richer quantity views. The Transfer core should not overbuild those projections at first.
- A Lince Cell is modeled as an Organ used by one person. In networking language, that personal Organ can still act as a p2p node that caches and relays public/subscribed Transfer information from other nodes.

## Basic Shape

A Transfer is a structured promise before it is executed. It can group one or more item-level interactions between Records, parties, personal Organs, or shared Organs.

1. Someone creates a Transfer from one or more Records.
2. Each Transfer item describes how a Record means something inside this Transfer.
3. Visibility decides which party, personal Organ, or shared Organ can see which fields.
4. Parties edit their parts of the proposal.
5. Edits reset agreement only for the connected items affected by the edit.
6. Parties raise agreement levels when they accept the current visible state.
7. When the selected agreement policy is satisfied, the relevant item or group becomes active.
8. Transfer influence appears in simulation as plus/minus quantity.
9. Parties confirm delivery and receipt.
10. Settlement applies the actual Lince data changes.

## Important Distinctions

- A Record describes something in a personal or shared Organ.
- A Transfer item describes what that Record means in this Transfer.
- A Transfer interaction describes a movement, promise, or dependency between items.
- A parent Transfer groups child Transfers under a larger subject without forcing one agreement policy on every child.

## Example

The Record can be `Play Guitar`, used every day by Karma as a personal task. In one Transfer, that same Record can appear as a Need with item title `Play guitar for a live audience in a restaurant`. The other parties may see only the item title and description, not the private source Record head.

## Quantity Still Works

The earlier claim that quantity is weak for services, promises, knowledge, permissions, access, transportation, or tasks was wrong for Lince.

In Lince, those can work because:

- A promise is exactly what a Transfer is before execution.
- Knowledge can live in a Record body or linked Record.
- A service can be represented by a Record and a quantity of hours, sessions, attempts, visits, or completed units.
- A permission can be represented by a Record state, link, or assignment.
- Transportation can be represented by a Record, route description, quantity, and delivery/receipt events.
- A task can be represented by a Record and later generalized Kanban/work metadata.

Quantity should stay generic. Specialized units like kilograms can be represented by Transfer extensions when needed. The core should not hardcode every unit or domain.

## Need And Contribution Are Transfer Roles

Need and Contribution are contextual. A Record does not permanently become one or the other.

A Transfer item should include:

- The source Record id.
- The role in this Transfer: need, contribution, support, task, information, reservation, etc.
- A Transfer-specific title.
- A Transfer-specific description.
- A visibility policy that can hide the source Record fields.

This lets a private Record be affected by a public Transfer without exposing the private Record identity or head.

## Expiration Is Not Hardcoded Yet

Expiration should not be a built-in Transfer property in the first version.

If a Transfer should expire, Karma can change its quantity or active marker:

- `0`: neutral/inactive
- `-1`: active/open/proposed, depending on local convention
- `0` again: inactive/closed by rule

This keeps time behavior inside Karma and avoids hardcoding Transfer-specific expiry before the model stabilizes.

## Counteroffers Are Edits

The UX should not be centered on counteroffers.

Everyone can edit the parts they are allowed to edit. Any edit invalidates agreement levels for connected items because nobody should remain agreed to terms they have not seen.

Agreement returns only when the relevant parties accept the new state.

## Default Agreement Is Individual

A Transfer may contain many interactions:

- `A -> B`
- `B -> C`
- `B -> D`
- `A -> D`

The default mode should be `individual`: each connected interaction can reach agreement and settle independently.

Other agreement modes can exist:

- `full`: every required party and item must agree before anything is active.
- `percentage`: a configured percentage of parties must agree.
- `dependency`: a connected chain or cycle must agree together.

Role-based agreement should be deferred.

## Design Principle

Transfer should be a protocol for making Record quantity changes and Record relationships socially valid before they become final database changes.

The system should let people expose only the Transfer-specific meaning they want to expose, agree only to the connected parts they understand, and see how active Transfers influence their Records before settlement.
