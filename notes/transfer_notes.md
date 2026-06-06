## Correction

"Quantity works for apples. It is weak for services, promises, knowledge, permissions, access, transportation, or tasks." Thats completely wrong all those things can work in lince, some of them are covered by transfer like transfers are promises before they are excecuted, knowledge can be in body of record,

## Properties

Maybe just like we have record extensions we could need transfer extensions, but for uncommon things like kilogram. I agree with you that we need special non generic/extension tables, like visibility, we only want certain cells or nodes to see certain transaction items. We also need messages of a transfer.

Im thinking of each transfer item have in the need a head of record and title/description of how that record means in the transfer, because i can model 'Play Guitar' which is a personal task i do everyday with karma and today it is going to be contributed to by playing in a restaurant, so i have in the transfer it as a need with a title of need: Play the guitar for a live audience in a restaurant.

## History Events

Yes we need a history of what happened in the transfer I agree with your points on this subject

I like the idea of having a system to check if the events are correct.

## Karma

On the karma side lets stick right now to it changing a quantity of the transfer (from 0 neutral to -1 (active)).
That way we keep the same pre-configured visibility and parties

## Visibility

For visibility we need to control what records and transfers can be seen by whom.
Please change the TRANSFER.md to have code, tables and functions of examples for each part we agree on building. Start with the visibility one, we need to make a relation between transfers and the visibility. In that visibility we may want to checkbox all the properties of the transfer/item we want to show to each party/node.

If we have record head and transfer item need title being two text fields that can describe our need then i'd want the option to hide the record head too, so the transfer can end up affecting my original record of play gitar without the other parties knowing that is the record they where contributing to.

## Simulation

Agree with the minus and plus model

## Intent

Someone

## Discovery

"Important rule: discovery should not mutate Records. It only creates candidate links." true

## Proposal

Im not sold on expiration just yet, as part of a feature in transfer, it might be achieved by using karma to set the quantity as -1 and then to 0 after some time, so we achieve expiration with that, but not as a harcoded feature or property of transfer.

## Negotiation

I dont know if i like the model of counteroffer, i think that whenever something changes, the parties envolved in a transaction loose their agreement levels, it's as if they cant agree anymore to something they dont know about. So everybody can change every part of the proposal on their part, but it should invalidate the agreement others made to prior conditions, counteroffers are just edits, whenever the parties reach a state of consensus they will elevate their agreement levels to two, at that point the transfer is on.

## Agreement

I agree that we need different policies in agreements, the default i think could be loose like:
A -> B: A gives to B
B -> C: B gives to C
B -> D: B gives to D
A -> D: A gives to D

If our transfer envolves those 4 transfer items then A and D could agree by themselves and their part of the transfer is done.

I think the default could be that inside a transfer there are many small interactions, for the items. Those interactions could be done independendly. Its just a grouping of transaction items. Lets call this mode Individual. The resetting of agreement levels should be only for the connected items. And for this we need an order of events. Like B can give to C, but only after A gives to B: A -> B -> C. And this may not require collective consensus, C may not need to agree that A gives to B. Or we may have A -> B -> C -> A, a triangle of contributions, and that may need individual item consensus or general one for this triangle to function.
This is a list of dependance, just like we have Record relations we need transfer relations.

The other modes are for percentage of people in agreement of the proposals, setup in the creation of the transaction, prefferably "immutable". Or a mode of full consensus. For the role one, dont do it yet.

## Reservation

I think we should show that the record quantity has a transfer influence, while it is being transfered.

## Confirmation

Each party should confirm their part, you change your part of proposal and see other's, when you are satisfied you make 1st or 2nd agreement, and that assumes you liked your part and their part. There should be a delivery/receivment confirmation too. I dont see a need for more on this part.

## Settlement

Something simple should suffice. But just how consensus must be achieved by individual items or full group we should probably take it into settlement aswell, the choice of when any individual item is resolved the record quantities change, or when all is resolved everything changes.

## Aftermath

Here you commented stuff that is related to kanban features we have. We have some properties of kanban related to expected amount of time it will take to complete, start and end date, assignees. I wonder if we can make those kanban features general purpose inside Lince, or at least attach it to transfers too (with records being a given).

---

## Server

A server introduces parties and coordinates the first proposal. Once both Cells know each other's endpoints and have the Transfer event log, they can sync directly.
I really like the idea of using your Lince cell as a node in a p2p network that can hold pub/sub transfer information about certain nodes, like a menu, when people reach your server they receive this cache and can add to their own or not, to expand their understanding of the needs of the world. The nodes that have the cache may ask for an up to date version or not, this is probably a theory of pub/sub i am missing, please help me in this.

What are signed events? How could we now already have cell to cell sync? Probably do the p2p node discovery through a central server, then find a cell you want to connect directly, and then probably adding them to the organ list (like a contacts list) and calling it directly.

"## Simulation
Simulation is the feature that makes Transfers larger than simple transactions.

A Record should be able to show multiple quantity views:

| Quantity          | Meaning                                                       |
| ----------------- | ------------------------------------------------------------- |
| Actual            | Current known real quantity.                                  |
| Reserved outgoing | Quantity promised to agreed Transfers.                        |
| Reserved incoming | Quantity expected from agreed Transfers.                      |
| Proposed outgoing | Quantity requested by draft/proposed Transfers.               |
| Proposed incoming | Quantity expected from draft/proposed Transfers.              |
| Available         | Actual minus relevant outgoing holds.                         |
| Planned           | Actual plus incoming minus outgoing across selected scenario. |
| Surplus           | Quantity above a user-defined threshold.                      |

"

I like this part, and i think that we can solve this with sql, so for some sand we would have the option to receive more information than "quantity" on each record, there would be properties arriving that would tell the user other ways to look at that quantity, counting with transfers in the proposed, reserved.. ways. But that is not something we need to think right now for the transfer feature, that is doable with sql and we should leave it to the sands that want to write it.

---

Now lets talk about the specifics of the code.

I want to be able to nest Transactions, so i can have like a big party transaction and inside it different transactions with different policies. If we arent able to nest we will have all the transactions that are related to a big subject scattered and forced to have one agreement policy for example.

In agreement (and everywhere you can to type the transaction options) make it an enum like this somewhere in the code, we can have a to/from string but it should always pass through this to assute that it is correct. Dont put foo.get("string") if you can, make it typed, and if you need to then make sure the serialization somewhere will fall into a Rust type:

```rust
enum AgreementType {
    Individual,
    Full,
    Percentage(f64), // they can all be saved as text and this one is just a number
    Dependecy
}
```

On visibility: Cell is an Organ used by one person (its an abstraction), i edited the visibility check on sql to remove it, continue the change.

---

We need to be able to receive transfer proposals from anyone if we want, or not, it might be a config, default is dont allow transfer proposals to arrive if we dont have them in the organ list. If we can then we need to put it in a cache of some sorts, that doesnt save, bc then we need a tmp transfers? and we need to manage it.
