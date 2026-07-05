# Fable Improvement v3: The Lince Rebirth

A deep analysis of Record, Karma, and Transfer; the structural change they are asking for; the pillars Lince still needs; and the experience that carries Lince from a tool you open to a synchrony you live inside.

This is v3. Beyond v2's refounding and Protein, this version absorbs a second round of annotations: **Instincts** (built-in concepts with engine muscle, place being the first), the precise Karma pipeline (full math kept, the bool workaround killed, many-in/many-out guaranteed), Imagination as an economic sense (confidence-driven proposals), the exact mechanics of where whispers arrive, and a whole new Part VII — **The Window** — that triages every app-domain Lince intends to cover and deduces, from first principles, what belongs in the core and what never should. The affected far-future notes in `notes/institute/` have been updated to match. Compatibility remains deliberately and fully ignored — best architecture, built clean, old data ported by hand at the end.

---

## Part I — What the three pillars actually are

### Record: the state vector of a life

`record(id, quantity, head, body)`. Four columns. The sign convention (negative = Need, positive = Contribution, zero = peace) is the single best idea in Lince: it gives a *moral direction* to a number. Every other system stores "tasks" or "inventory" or "balances" as separate universes; Lince stores one universe and lets the sign say whether the world owes you or you owe the world.

The pattern quietly repeats everywhere: every table (`karma`, `command`, `frequency`, `transfer`, `configuration`...) carries a `quantity` column used as its activation/state knob. That is not an accident — it is The Lince Way leaking into the infrastructure, and it is a strength worth naming: *quantity as the universal enable*.

**Where the model strains, and how each strain resolves:**

1. **Quantity means too many things at once.** Count of apples, intensity of a desire, money balance, boolean done/not-done, activation flag. A bare `f64` has no unit. This is tolerable inside one head; it breaks the moment two Cells trade, because Transfer cannot verify that my `5` and your `5` are the same kind of five. *Resolution:* an optional `unit` on Records — and units are Lingua concepts (Part III), so two Organs agree on `kg` the same way they agree on `Apple`. Nullable unit = pure number, exactly today's behavior.

2. **A Record is a kind and an instance at the same time.** "Apple" the concept and "my three apples" share a row, so nothing can say that Record 12 in my Cell and Record 731 in yours are *about the same thing* — all cross-Cell matching is human-only. *Resolution:* the category instinct is correct and becomes Lingua — canonical, importable concepts that Records point at. I may call my record "suculentas vermelhas" (sucks) and still trade apples, because the concept tag matches. The Transfer item's own title/description stays as the human-meaning layer on top of the machine-matching layer.

3. **Time is bolted on, not built in.** Needs have deadlines and windows ("3 apples *by Friday*"). *Resolution — two-sided, by design:* inside your Cell, Karma + Frequency remain the first principles; "this is a Need from tomorrow until Thursday" is a counter the interface generates for you when you select a range — no new Record column. What *does* get a declarative `window` is the **Promise** (Part III), because a promise crosses the wire to strangers whose Senses must match on "by when" without running your rules. And since Karma 2.0 rules are pure, Imagination can still project your procedural timing forward — composing primitives loses nothing to simulation.

4. **Relations are second-class.** `record_link` exists, but link meaning lives in free strings and the mental model is a flat table. *Resolution:* one typed, quantified link primitive — `link(uid, from_record, kind, to_record, quantity?)` — where `kind` is a Lingua concept and the optional quantity makes links *recipes*: "cake —needs→ 2 flour". The graph becomes a bill-of-materials for life: a Need for one cake can derive Needs for its ingredients, Imagination walks the tree, Senses matches the sub-needs. Decomposition stops being prose and becomes structure.

5. **`record_extension.freestyle_data_structure` is a confession.** Every JSON escape hatch marks a primitive the model lacks. *Resolution:* keep fds, but give structure a **promotion path** with a clear rule for each tier: fds is the private incubator (only one sand cares); a Lingua-typed attribute is the middle tier (Organs must *agree* on it, so it needs shared vocabulary); an Instinct or core column is the top tier (the *engine* must compute over it). When the same key keeps appearing across many fds namespaces, that is the signal to graduate it. fds stops being a confession and becomes a nursery.

### Karma: a beautiful idea running on assembly-language ergonomics

Condition → Operator → Consequence is exactly the right size of idea: small enough to explain to a child ("if, and, then"), general enough to build finance on. Records as memory cells that rules read and write makes Karma a *spreadsheet of behavior* — and the spreadsheet is the most successful end-user programming model in history. That instinct is correct.

**Where it strains:**

1. **It is a polling register machine over global mutable state.** Every 60 seconds, every condition re-evaluates. Cascades happen by one rule writing a quantity another rule reads. No dependency graph, no loop detection (Proof.md admits this), no termination argument. *This is the real problem #1.*

2. **Tokens are numeric addresses.** `rq1`, `f3`, `c4`. Ids are unstable across sync (the CRDT work already had to invent sync-id mapping because local ids don't travel), unreadable in a month, hostile to sharing. A published blueprint full of `rq14 * f2` cannot be transplanted without surgery. *Resolution:* stable UIDs for machines, slugs for humans — `rq14` dies, `apples.stock` lives.

3. **No provenance.** When a quantity changes, nothing records *why*. Trust in automation is exactly proportional to the system's ability to explain itself. *This is the real problem #2, and the Ledger kills it structurally.*

4. **Delivery every 60s is both too slow and too wasteful.** Too slow for "react to what I just did"; too wasteful for monthly rules. Spreadsheets solved this decades ago: recompute what depends on what changed. The loop must get better; Karma 2.0's derived dependency graph is how.

**Accepted trade-offs, kept on purpose — and nothing narrows.** Shell Commands stay *out* of condition evaluation not as a loss but as an intentional gain: conditions that never touch the world are what make Deterministic Simulation Testing and Imagination possible. Commands become Signals (sampled on their own schedule, written as facts) and Effects (consequences, queued and logged) — the same power, honestly placed. The guarantee, spelled out in full in Karma 2.0 (Part III): commands, queries, record quantities, and frequencies all remain Condition inputs; commands, queries, and record changes all remain Consequences; full math over the condition survives exactly as today — tokens substituted, then the whole expression evaluated. The rebirth *widens* both ends: promises, transfer agreements, visibility, and every activatable thing become inputs and outputs too.

### Transfer: the cathedral next to the hut

Transfer has grown into the most sophisticated part of Lince: parties, structured items with six roles, interactions, three agreement levels, four agreement modes, five reservation policies, confirmations, idempotent settlement, append-only hash-chained signed events, field-level visibility design, chains, spectators, satiation policies. Twenty-five-plus tables.

The deep principles are *right*: a Transfer is a structured promise before execution; settlement is the only thing that mutates Records; history is append-only; visibility is data; status is derived from facts. This is the most philosophically mature pillar.

**Where it strains:**

1. **The elegance inversion.** Record is 4 columns; Transfer is a cathedral. When one pillar needs 25 tables and the other needs 1, the missing abstraction is hiding between them. What Transfer actually is, underneath: *a bundle of promised quantity-deltas that several parties must agree on before they become real*. Nearly every table is scaffolding around the absence of "promised delta" as a primitive. Reorganizing the primitives dissolves the barriers: the new primitives shrink the *total* moving parts across all pillars, and Transfer becomes an organization of them rather than its own machine.

2. **Transfer already invented the event log — but only for itself.** `transfer_event` is append-only, hash-chained, signable, syncable. Meanwhile direct Record edits, Karma consequences, and CRDT sync each mutate quantity through *different* paths with *different* histories. Lince has four write paths and one of them accidentally built the architecture the other three need.

3. **Matching is manual.** Discovery caches summaries and polls peers, but nothing can *propose* that my public apple Need and your public apple Contribution belong together. *Resolution:* canonical concepts (Lingua) make matching a join; the Senses pillar turns the join into automatic draft proposals — scoped, crucially, to Organs you already know (see Part IV).

4. **Trust is deferred, and it is the raw material Lince is already sitting on.** Settled Transfers are signed events: a vast, verifiable archive of real people doing real, checkable good. The first job is not scores or privileges — it is making sure every interaction and delta is *verifiable*. What gets built on top of that (search, aggregates, opt-in leaderboards among Organs that confide in each other) comes after, carefully.

---

## Part II — Philosophy: why it could be better

**Needs are flows, not states.** The current model stores the *level* of the tank; life is about the *rates* — filling, draining, promised inflows, scheduled outflows. Every interesting question a person asks ("will I run out?", "can I afford to give this away?", "when do I need to act?") is a question about flows integrated over time. A model that stores levels and discards deltas answers none of them natively. `history` and `sum` exist precisely because levels aren't enough — they are the model apologizing for its own shape.

**The unit of meaning is the change, not the value.** "Quantity became 4" is meaningless; "ate one apple", "Maria delivered five", "the daily rule reset it" are meaningful. Lince currently stores the meaningless form and reconstructs the meaningful one with side tables. It should be the other way around: store the meaningful changes; derive the value. (And derive it *cheaply* — the exact mechanics, with no fold-at-read cost, are in Part III.)

**A promise is the social atom.** The philosophy says life divides into Needs and Contributions. But the *bridge* between them — the thing exchanged between people — is neither: it is the promise. "I will bring five apples Thursday." Transfers, Karma consequences-in-waiting, reservations, and the farmer's "the chain will unclog in two days" prediction are all the same object: a delta that has not happened yet, with conditions on it and a party behind it. Lince has this object five times under five names and zero times as a primitive.

**Meeting needs through people is the point; automation is the bonus.** Test the current architecture against its own philosophy: Karma (the bonus) is core-adjacent and load-bearing, while matching, trust, and shared vocabulary (the point) are absent. The structure inverts the philosophy. A better structure puts *the meeting* — discovery, matching, promising, trusting — at the center and makes automation a set of hands that operate that same machinery. Human hands and automated hands turn the same knobs.

---

## Part III — The structural change

One refounding, three sentences:

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

### 1. Everything is a Record

Rules, commands, transfers, views, sands, organs, people — all get Record identity: a stable UID, an optional human slug, a head, a body, and quantity-as-activation. This is already half-true (`quantity` on every table; sand publications are records) — finish it.

Concretely, with examples:

```
record   uid=r_8k2  slug=apples.stock        head="Apples"          concept=@apple  unit=@count   quantity=8
record   uid=r_9c1  slug=rules.apple-reorder head="Reorder apples"  (a Karma rule)                quantity=1
record   uid=r_2f7  slug=xfer.saturday-beans head="Beans, Saturday" (a Transfer bundle)           quantity=1
record   uid=r_5a0  slug=sand.kanban         head="Kanban"          (a published sand)            quantity=1
```

What this buys, uniformly and for free:

- **One visibility system.** Publishing a rule to your farming Organ uses the same visibility rules as publishing a Record. Sharing a workflow is not a special feature.
- **One sync system.** If it is a record, the Ledger replicates it. Nothing gets its own bespoke sync path ever again.
- **Karma on anything.** A rule that sets `rules.noisy-notifications.quantity = 0` after 22:00 is just a rule — because the other rule is a record. Karma activating a Transfer (which already exists as a pattern) stops being a special token and becomes the general case.
- **One search, one graph, one permission model, one publication flow.**

Records gain three optional first-class fields (each nullable = today's behavior):

- `concept` — a reference into Lingua, the shared vocabulary.
- `unit` — also a Lingua concept (`@count`, `@brl`, `@kg`, `@hour`).
- `place` — where this Need/Contribution physically lives. Place is not a plain concept: it is Lince's first **Instinct** (below).

Time is deliberately **not** a Record column: record-level timing is Karma + Frequency composed from first principles, with interface sugar that writes the counters for you when you pick a date range. Time lives declaratively on the Promise, where strangers need to read it.

And Records get **names, not just ids**: UID for machines and sync, slug for humans and rules. Published Karma becomes readable, transplantable data.

#### Instincts — concepts with engine muscle

Some concepts could be left as generic strings for interfaces to interpret, but are strictly superior when the engine itself understands them. Those graduate into a named tier: an **Instinct** — something the lynx knows how to do without learning. An Instinct is a concept the core ships *functions* for, callable from Karma conditions and Protein queries alike, never reimplemented per interface.

**Place is the first Instinct.** A place is world coordinates or an address (resolvable to coordinates), and the engine ships the operations: `distance(a, b)`, `route(a, b)` with A\* over map data (returning path, ETA, and alternatives), `near(place, radius)`, `within(place, area)`. A delivery route, "who is closest to Contribute", and "our routes cross Thursday at 18:04" are engine answers, not sand tricks. Map data (e.g. OSM extracts) loads as a resource; live traffic, if ever, arrives as Signals.

Future Instinct candidates, each graduating only when the engine genuinely needs to compute over it: duration/calendar math (Frequency is already the proto-Instinct of time), currency conversion. The ladder from Part I stays intact and gains its top rung: **fds → Lingua attribute → Instinct/core column.**

### 2. Every change is a Fact (the Ledger — the Memory pillar)

The single biggest change. One append-only table at the center of the entire system:

```
fact(uid, record_uid, delta, at, actor, cause_kind, cause_uid, signature?)
```

Examples:

```
fact  apples.stock  -1  08:12  cause=user_edit                     (ate one)
fact  apples.stock  +5  10:03  cause=settlement:xfer.saturday-beans (Maria delivered)
fact  apples.stock  -2  10:04  cause=rule:rules.apple-donation      (donation rule fired)
```

**The Fact is the truth; the quantity is the cache.** `record.quantity` remains a real, mutable, instantly-readable column — but it gets exactly *one writer*: the fact-appender, which appends the fact and updates the column in the same transaction, like a bank keeps a balance column next to the statement. Simple GET workflows read the column at O(1); nothing ever folds events at read time. Periodic **checkpoint facts** snapshot a record's level so older facts can be archived or compacted — growth stays controlled by design, because facts are foldable.

**Authorship travels with the data.** Facts and promises carry their author's signature. Visibility rules decide *what* leaves the Cell; the signature makes *who* undeniable wherever it goes. When your donation fact is visible in another Organ, that Organ knows it was you — traceability across Organs is what turns the Ledger into the raw material of Trust.

What collapses into this one primitive:

| Today | Under the Ledger |
|---|---|
| `history` | the Ledger itself |
| `sum` (delta/positive/negative over windows) | a query over facts |
| Karma provenance (missing) | `cause` on every fact — the traceability of *why the number changed* |
| CRDT quantity sync | fact replication — deltas commute; merges are conflict-free by construction |
| Transfer settlement idempotency machinery | settlement = appending facts that reference the transfer; replay-safe by uid |
| Undo (missing) | compensating fact |
| DST | replay the log; the system becomes deterministic by architecture, not discipline |
| Four write paths | one write path |

Transfer already proved this architecture works — `transfer_event` *is* this, scoped too narrowly. Promote it to the spine of the whole organism.

### 3. Every intended change is a Promise

```
promise(uid, record_uid | concept, delta, window, party_uid?,
        state: open|proposed|agreed|active|kept|broken|withdrawn,
        condition?, transfer_uid?, rule_uid?, signature?)
```

A promise is a fact that hasn't happened yet: a delta, a declarative time window, a party behind it (or an **open** party slot — see below), possibly a condition, signed by its author. Example:

```
promise  apples.stock  +5  window[..Thu 18:00]  party=maria  state=agreed  transfer=xfer.saturday-beans
```

This one primitive replaces and unifies:

- **Transfer items and quantity influence** — a Transfer becomes: *a bundle of promises + an agreement policy + a visibility policy*. The 25-table cathedral collapses to roughly: `transfer` (the bundle, itself a Record), `promise`, `party`, `agreement`, and the Ledger it writes to. Double-entry generalized: a balanced Transfer is a bundle whose promises sum to zero per concept across parties.
- **Publishing a Need** — an *open promise*: a promise with an unfilled party slot, visible per your visibility rules. Senses matching (Part IV) is precisely the search for parties to fill open promises. The window travels with it, so a stranger's Cell can match "by Friday" without ever running your Karma.
- **Reservations** — a promise in `active` state *is* the reservation; `available = quantity − Σ active outgoing promises`. Five reservation policies become one question: at which agreement state does the promise start counting.
- **Chain links and spectators** — private promises conditioned on other promises being kept.
- **Karma-scheduled actions** — a rule that will change something *emits a promise first* (instantly self-kept for immediate consequences, pending for scheduled ones). Automation becomes previewable and cancelable *in the same UI that shows human promises*.
- **Simulation** — projection is now trivially defined: `state(t) = facts ≤ now + promises kept by t`. The farmer seeing "the chain unclogs in two days" is a query, not a feature.

### Karma 2.0: same soul, better body

Split the tangle into three honest parts:

- **Signals** — inputs sampled from outside (command output, sensor, HTTP, SQL), each with an explicit sampling schedule, written into the Ledger as facts on signal-records. Conditions become *pure*: they read the Ledger, never the world. Determinism restored; DST and Imagination become possible. (Simulation simply runs without live commands — intentional.)
- **Rules** — reactive, named, pure. The engine derives the dependency graph from the expressions (the token parser already exists — this is close), recomputes only what changed (spreadsheet semantics; the 60s heartbeat survives only as a floor for timers), detects cycles statically, and warns: "these three rules form a loop that diverges." That is the Proof.md feature, falling out of the dependency graph nearly free.
- **Effects** — shell/SQL/network actions, queued, logged as facts, never run inside evaluation.

**The rule pipeline, precisely:** `condition → gate → carry → consequences`.

- **Condition** — full math, exactly as today: tokens/slugs are substituted with real values, then the whole expression is evaluated. `apples.stock + other_thing` stays legal; so does any composition of every input below. One repair: **booleans are native numbers** in evaluation (`true = 1`, `false = 0`), so `apples.stock < 3` needs no `* 1` workaround ever again.
- **Gate** — optional and explicit, replacing the old operator's double duty: default `!= 0` (today's `=`), or `<`, `>`, `==` against a threshold, or `always` (today's `=*`).
- **Carry** — what flows to the consequences: the condition's value (default), `1`, or a constant. Trigger and payload finally separate, without losing the old behavior.
- **Consequences** — zero or more, each independent.

**Many in, many out — the guarantee.** Condition inputs: record quantities (and other record fields), frequencies, commands and sensors (as Signals), queries, promise and transfer states, Imagination tokens (below), and other rules' values. Consequences: change records (facts), run commands (Effects), run queries and Actions, emit promises, change visibility, advance or activate or execute Transfers and their agreements, activate/deactivate *anything* — rules, sands, transfers, capture sources — because everything is a record, plus `ask` and `notify` (Part IV). Karma feeds on the whole organism and can move any part of it.

**A rule with zero consequences is a named derived value** — a spreadsheet cell. Other rules reference it by slug; Proteins can query it. "Monthly burn rate" stops being a condition fragment copy-pasted into five rules and becomes one named formula.

The consequence kind **`ask`** is available but never mandatory: instead of acting, the rule enqueues a human decision (see Attention, Part IV). "When apples < 3, *ask me* whether to send the reorder proposal" is a first-class rule, not a notification hack — and "when apples < 3, just send it" remains equally first-class.

Rules reference records by slug. Every firing writes its fact with `cause = rule:<uid>` — automation that can always answer "why".

### Protein & Actions: the expression layer

DNA is the database — nothing more. But DNA does nothing until it is *expressed*. **Protein** is how any interface asks Lince for data; **Actions** are how any interface changes it. Sands and every first-party surface speak *only* Protein and Actions; they never see tables, SQL, or storage.

A Protein is a declarative read request:

```
protein {
  source:    record
  where:     concept in @food and quantity < 0
  include:   links(kind=@needs), promises(state: agreed|open),
             facts(limit: 10, with: cause), work_metadata
  aggregate: sum(quantity) by concept
  order:     quantity asc
  live:      true            # snapshot, then stream updates
}
```

- **Includes are the end of custom plumbing.** Today, a sand that wants to show *why* a card's number changed would need bespoke joins across history tables that don't even exist coherently. Under Protein, it adds `facts(with: cause)` to its request — one line — and every sand on the platform gains provenance, promises, and projections the same way. The capability lives once, in the engine.
- **Instinct functions are callable in Proteins** — `where: distance(place, @home) < 2km`, `include: route(@home, place)` — the same functions Karma conditions use, computed once, in the core.
- **Saved views are saved Proteins.** The `view` table's raw SQL becomes a named Protein; the SSE view stream becomes the `live: true` behavior of any Protein.
- **Visibility is enforced here.** A Protein evaluated for a remote Organ passes through the same visibility rules as everything else — one gate, not one per endpoint.
- **Ephemeral lanes.** The live transport also carries presence — cursors, typing indicators, call signaling — scoped to a Protein subscription and *never written to the Ledger*. Real-time togetherness without polluting Memory. (Part VII deduces why this must exist.)
- **Storage-independence is the point.** The backend translates Protein to SQL today; when AniccaDB (the future Lince-native store) exists, Protein translates to Anicca queries and no interface notices. Protein is the contract that makes the storage engine replaceable.
- **Writes are Actions**: semantic, typed verbs — `create-record`, `agree`, `settle-transfer`, `activate-rule` — the direction the widget contract already took, now universal. Protein never mutates; Actions never query. Every Action lands in the Ledger with its cause.
- **Transport:** first-party surfaces need one bidirectional, typed, streaming channel (websocket, gRPC — an implementation choice, not a theory commitment). Plain HTTP endpoints remain — not for sands, but for the rest of the world: external systems integrating with a Cell speak HTTP; Lince's own interfaces speak Protein.

### What deliberately does not change

Quantity stays central. The sign convention stays. Head/body stay. Cells and Organs stay. Sands stay. Karma keeps its name, its full-math conditions, its spirit. The Lince Way — model with numbers, join with math, act on thresholds — is not being replaced; it is being given a memory, a vocabulary, and a way to be seen.

---

## Part IV — The pillars

Ten pillars. Existing ones keep their names; new ones earn theirs.

**1. Record.** *(existing → refounded)* The state of the world: identity + concept + unit + place, quantity as the universal knob. The nouns. Everything else in Lince is a way of interacting with Records — that sentence from the original notes survives every refounding.

**2. Memory — the Ledger.** *(new, structural)* Every fact, forever, with its cause, its author's signature, and — wherever visibility lets it travel — undeniable authorship in other Organs. History, sync, undo, provenance, and audit are all this one pillar. The quantity cache makes it free to read; checkpoints make it bounded to keep.

**3. Lingua — shared concepts.** *(new)* A `concept` table *in your own database* — Lingua is part of your DNA, not an external service. A concept is a shared tag with lineage: `concept(uid, names[], parents[], default_unit?)`. Names are plural and multilingual — "Apple" and "Maçã" are one concept. Your Record points at `@apple`; my Record points at the same uid; matching becomes a join instead of a human squint. You import slang the way you import a sand: inserting concept rows, published and versioned through the same package flow. Units are concepts too, so quantity dimensions ride the same system. Above plain concepts sits the **Instinct** tier — concepts the engine ships functions for (place first; Part III). No central ontology authority — Organs converge on vocabularies the way communities converge on slang, forks carry lineage, and Senses matches across declared equivalences. Without Lingua, Lince is a diary; with it, Lince is a language.

**4. Karma.** *(existing → refounded)* Signals, Rules, Effects; the condition→gate→carry→consequences pipeline with full math preserved; many inputs, many outputs, zero-consequence rules as named values. Reactive delivery over a derived dependency graph, static loop warnings, full provenance. Fast, explainable, previewable, provable — same name, same soul.

**5. Transfer.** *(existing → refounded)* An abstraction that *organizes* the primitives rather than owning its own machine: promises bundled under an agreement policy and a visibility policy. Everything already won survives — append-only, signed, derived status, settlement-only mutation — on a quarter of the moving parts, and the total moving-part count across all pillars goes *down*. And because agreements and activation are record-shaped state, Karma can advance, activate, and execute Transfers as ordinary consequences.

**6. Senses — discovery and matching.** *(new)* The layer that watches open promises across known Organs and *proposes* meetings: same concept, compatible units, overlapping windows, feasible places and routes (the place Instinct at work) → draft Transfer. **Scoped by proximity, hard.** Automatic discovery and matching operate only within Organs at or under a proximity ceiling you set per matching rule — your ingroups, the Organs you actually know. Senses never auto-expands toward unknown or public Organs; widening the circle is a manual act or an explicitly Karma-gated one (the existing visibility-wave mechanism, kept deliberate). The lynx gets its famous eyesight, pointed only where you aim it.

**7. Trust.** *(new, minimal by intent)* The first and only near-term job: **make every interaction and delta verifiable.** Signatures on facts and promises (the transfer-event signing generalized), so your history of kept promises, donations, and settled Transfers is an archive *you* can prove and others can check — and since authorship travels with whatever visibility permits, doing verifiable good in one Organ is legible in another. On top of verifiability — later, carefully: search and aggregation ("who donated what"), and leaderboards only as an opt-in sand among Organs that mutually confide at a chosen trust level. Explicitly deferred: any linkage between reputation and capability — verifiable facts do not become tickets to do more on other nodes until that design is actually thought through. No global score, ever.

**8. Imagination — projection.** *(new, the sleeper hit)* Fold promises and rules forward: `state(t)` for any future `t`. "Your apples run out Thursday. Rent leaves you 300 short on the 5th — unless the freelance Transfer settles, and Maria's verifiable history gives that 92% confidence." This is a **backend engine, a core Lince feature** — computed in the core and exposed through Protein so every interface gets it at full speed; never an interface-side trick. And it is an *economic sense*, not just a viewer: Imagination emits **confidence and distributions** — derived deterministically from the temporal shape of facts and public proposals — and exposes them as condition tokens: `confidence(promise)`, `projected(record, t)`, demand and price curves over the hours of a day. Rules can then act ahead of time: continue a proposal automatically when confidence ≥ 90%; notice that someone you know recurringly needs something and propose before they ask — or quietly buy it and surprise them; see the demand curve and make the purchase before rush hour. Todo apps show tasks; banks show balances; *nobody shows a person their projected state vector with other people's commitments folded in* — let alone lets their automations trade on it.

**9. Protein — expression.** *(new, from the annotations)* The declarative read contract plus typed Actions (Part III). DNA is what you store; Protein is how it comes out and shows its power. Sands speak only Protein/Actions; HTTP remains for the outside world; storage stays swappable underneath (SQL now, AniccaDB later); the live transport carries the ephemeral presence lanes.

**10. Attention — the Decision Queue.** *(new as a pillar, concrete and near-term)* One deterministic object: the queue of everything currently awaiting a human choice —

- promises entering `proposed` or `open` states that target you,
- draft Transfers from Senses matches,
- rules whose consequence is `ask`,
- threshold crossings Imagination projects,
- drafted rules and records awaiting one-tap approval (from UI sugar or from Fiote).

**Where whispers arrive, mechanically.** Not through Karma's Command signal — capture and delivery are two different flows, and neither should impersonate a shell command. *Inward*, helpers speak **Signals**: a phone, a scale, a sensor posting facts to your central node, every capture source a visible signal-record with an off switch. *Outward*, delivery is a built-in **`notify` Effect** — a native sibling of shell Effects that routes a queue entry to a platform (notification, sound, text digest) per your configuration. And the queue entries themselves are **decision-records** — everything is a record — which closes the loop: Karma can read the queue, react to it, expire stale decisions, or escalate quiet ones. Whispers arrive as decision-records, are delivered by notify Effects, and are answered by Actions.

A **whisper** is therefore not generated; it is *routed* — rendered per platform under a budget the user owns, with per-source on/off switches. **The voice is LLM-less by default**: templated event text — "Transfer *Beans, Saturday* advanced to agreed", "apples below 3" — assembled from the same typed events everything else uses. If Fiote is active for a source, it reads the same update and gives you *its* version, configured how you want. The everyday shape is plain: desktop for dense, powerful work; mobile for agile-first interactions; helpers whispering in both directions. The far-future ambient hardware (crowns, AR) is explicitly *not* the near-term design — the Decision Queue is, and it works with zero exotic hardware and zero LLMs.

**Fiote — the cub.** *(the optional operator, not a pillar)* An agent that turns the same knobs a human turns: it reads what you allow, and it creates Karma rules, drafts records, approves proposals — always as inspectable data with `cause = fiote`, always within a delegated autonomy level you set: *observe → suggest → draft → act-within-budget*. The human delegates to the AI the switching of knobs they could switch themselves; nothing Fiote does is a different *kind* of thing. Lince talks to you LLM-less by default; Fiote, where activated, narrates the same events its own way and — at the autonomy levels you grant — takes decisions out of the queue before they ever cost you attention (creating the same Karma automations you could create, approving proposals a rule could approve). Theory only; no stack is chosen here.

**Where Alexandria went.** Trails of knowledge — the shareable bundles that install a capability ("keep a sourdough starter", "run a small farm") — are no longer a pillar or a feature, because they no longer need to be: a trail is an importable subgraph of records + links + concepts + rules + views, published through the ordinary package flow. Records give it identity, links give it structure and order of implementation, Lingua gives it exact measures, Protein makes it visible — and a community can grow a public library from basic subjects to PhD level out of nothing but those primitives. A built-in capability with no built-in concept: the abstraction the primitives were sharpened to allow. (The institute notes — `Alexandria- Information Trail.md` and `Trail Progression Management.md` — now carry this architecture in implementation detail.)

---

## Part V — The experience: all levels of interacting

Levels, from bedrock to air. Each level is complete — nobody is forced upward — and every level is a view over the same Ledger, through the same Protein.

**L0 — The Table.** Raw database access: SQL, the table sands, the Operation command line. The truth, always inspectable. Trust in the higher levels is anchored by the permanent ability to drop here and see the same facts.

**L1 — The Board.** Sands composed on boards: kanban, relation graphs, calendars, dashboards, chess. Today's main surface, refounded on Protein: a sand states *what* it needs — source, filters, aggregation, includes — and gets a snapshot plus a live stream, with provenance and promises one `include` away. Building a rich sand stops meaning "invent your own data plumbing" and starts meaning "write a good Protein and a good surface."

**L2 — The Conversation.** Operation grows into a language; natural language compiles *to* it. "Every Monday I need to prep meals for the week" → a drafted rule + records shown for one-tap approval — never silently applied. Human and AI switch the same knobs: the AI's drafts are ordinary rules and promises with `cause = fiote`, reviewed in the ordinary queue. Explainability is structural, not aspirational.

**L3 — The Whisper.** The Decision Queue, rendered. A few routed whispers a day — notification on the phone, a line of text on the desktop, a sound in the kitchen — each answerable by voice, one tap, or a pre-authorized nothing ("silence = yes" only where a rule explicitly granted it). Inward, the helpers work: the fridge photo becomes stock facts, the receipt posts deltas, each source visible and disableable. As AI-less as possible by design — but where an AI is genuinely the best collector for a capture, using it is completely fine. You live; helpers whisper reality into your Cell; the plan recompiles when reality diverges.

**L4 — The World.** The map and the game. Needs and Contributions rendered over real terrain — mountains of unmet Need, valleys of surplus — scoped by visibility and consent, drawn from the place Instinct the engine already computes with. Walk your neighborhood with the overlay: the bakery's flour Need, the school's volunteer window, the neighbor's surplus tomatoes on your literal route home. And THE Game: your real Records seed the landscape; Karma writes the game rules; finishing your tasks feeds the Lincegoshi blob that grows and bursts into light. Play and life stop pretending to be different activities.

**L5 — Synchrony.** The level that doesn't exist anywhere yet, in any product. Multiple Cells' *projections* meet and negotiate ahead of time. My apples run out Thursday; your tree over-produces Wednesdays; Senses — inside the proximity circle we both allow — notices our routes cross Thursday 18:04, drafts the Transfer, both our queues agree (mine by rule, yours by nod), and Thursday evening the handoff happens like it was always going to. Scale it up: a street coordinating bulk purchases; a party where forty promises choreograph themselves; a farming co-op whose Transfer chains reschedule around weather signals; a city whose need-mountains visibly erode week by week. More needs met, more transactions peer-to-peer, more donations, more efficiency — the dance of the world, made executable.

### A day inside 2.0

Ana wakes. No dashboard. Coffee — the kitchen scale posts a fact; beans cross their reorder threshold; a promise to the roaster two streets over activates under a rule she approved months ago; the roaster's Cell and hers settle it for Saturday pickup. Silence.

8:40, one whisper on the walk to work: *"Say yes to Rui's ride offer Thursday? It unblocks your clinic appointment and he passes your door at 9."* One nod. Two promises change state in two Cells.

Work is an Organ; her tasks are Needs assigned through Transfers; her worklog facts post as she works — the standup is a view nobody has to fill in. Lunch: projection quietly moves the "eat out" budget record and says nothing, because nothing needs saying.

17:30, second whisper: *"Your mother's pantry Organ shows rice below her comfort level; you're 200m from a market seller with a strong verifiable history. Take it?"* Yes. The Need was never spoken; her mother published it to family only, and family means something because visibility is data.

Evening, she scrubs the timeline out of curiosity — a habit, not a duty: rent fine; the guitar-performance Need she published got a bite from a bar's event Organ (decent confidence from its verifiable history; proposal parked in tomorrow's queue); the balcony tomatoes will surplus in nine days and the donation rule is already staged. The Lincegoshi is fat and luminous because everything today got met. It dissipates. She sleeps. Total time managing life: under four minutes, all of it decisions only a human could make.

That is the Death of Lince the notes ask about: not a missing app, but management time asymptotically approaching the irreducible minimum — the moments of actual human choice.

### The flagship innovation, named

If one thing gets built to define 2.0, build **the scrubbable future**: the timeline of `state(t)` as a primary surface — your Records projected forward with rules, frequencies, and *other people's promises* folded in, confidence-weighted by verifiable history, where dragging a promise or toggling a rule live-recomputes the future. Planning, finance, habits, and coordination collapse into one gesture: *look at tomorrow, and bend it.* Everything intertwined without becoming a mess — all interactions as data to control, automate, and generate insight on the bigger picture. Every pillar feeds it; no competitor has the primitives to copy it, because no competitor stores promises between people as data.

---

## Part VI — The rebirth order

Compatibility is fully ignored: no staged migration, no dual-path bridges, no keeping the old API alive. Build the best architecture clean; at the end, old data is ported by hand into the new world, starting fresh. The order below is dependency order — each stage needs only the ones before it:

1. **Core schema.** `record` (uid, slug, concept, unit, place, quantity-cache) + `concept` + `link` + `fact` + `promise`. The single write path: the fact-appender with its transactional quantity cache and checkpoint/compaction policy. Place lands here as a stored field; its functions come with stage 3.
2. **Karma 2.0 engine.** Signals, Rules, Effects; the condition→gate→carry→consequences pipeline; dependency graph; reactive delivery; static loop warnings (Proof ships here); provenance on every firing.
3. **Protein + Actions.** The read contract with includes and live streams; typed Actions; the first-party transport channel with its ephemeral presence lanes; the place Instinct's engine functions (distance, route, near) exposed to Proteins and Karma conditions; HTTP kept at the boundary for external systems.
4. **Transfer on promises.** Bundles, parties, agreement policies, visibility policies; settlement as fact-appending; open promises as published Needs.
5. **Imagination engine.** `state(t)` in the backend core; confidence and distribution tokens for Karma; the timeline sand; the scrubbable future.
6. **Lingua publishing + Senses.** Concept packages through the ordinary publication flow; the matcher over open promises — concept, unit, window, route — proximity-ceilinged, ingroup-only automation.
7. **Trust.** Signatures on facts and promises; verifiable public history with authorship legible across Organs; search and aggregation over verified deltas. (Leaderboard sands and anything linking reputation to capability: explicitly later.)
8. **Attention.** The Decision Queue as decision-records; the `notify` Effect; whisper routing per platform with templated LLM-less voice; capture sources as disableable signal-records.
9. **Fiote.** The autonomy ladder over the existing knobs; `cause = fiote` everywhere; optional narration per source.
10. **World + Synchrony.** The map, the game, multi-Cell choreography.

**Risks worth respecting:**

- **Ledger growth** — controlled by design: checkpoint facts + compaction of archived ranges; facts are foldable, and the quantity cache means read paths never depend on log size.
- **Lingua politics** — vocabularies drift and fork; don't fight it, forking is how language works. Concepts carry lineage; Senses matches across declared equivalences; convergence is social, not enforced.
- **Capture consent** — every signal source is a visible record with an off switch; every fact shows its cause; local-first stays non-negotiable; nothing leaves the Cell without a visibility rule saying so.
- **Whisper fatigue** — the attention budget is a *hard* budget the user owns, and Lince never has a growth metric that benefits from interrupting anyone. This is the structural advantage of not being a platform with a cut.
- **Greenfield discipline** — the cost of ignoring compatibility is losing the dogfood safety net. Replace it deliberately: stand the new core up early and live in it with real daily data (even hand-entered), so the primitives are tested by life long before the last pillar lands. The v1 lesson stands: years of append-only features without holistic passes is how the cathedral grew next to the hut — the rebirth earns its simplicity only if each stage is used, not just built.

---

## Part VII — The Window: deducing every app from first principles

The list below is a window, not a roadmap. Its purpose is philosophical: take the workflows the world already runs on dedicated apps, hold each one against the primitives, and ask *where does each part land?* Two things come out of that exercise. First, a placement for every case, so nothing un-Lince-y ever leaks into the core — a workflow that would soil the primitives gets jammed into the web version as a sand, deliberately. Second, and more valuable: the cases that *don't* decompose cleanly are exactly the ones that reveal a missing primitive. The Window is how the core stays honest — abstractions are deduced from first principles, and only what the deduction forces gets added.

### The altitude ladder

Every part of every workflow lands on exactly one rung. It is never 8-or-80 — one app mixes rungs freely:

1. **Primitive** — record, fact, promise, link, concept. Only what everything else is made of.
2. **Pillar engine** — Karma, Transfer, Senses, Imagination, Trust, Attention, Protein. Computed in the backend core, exposed to every interface.
3. **Instinct** — a concept with engine functions (place). Graduates only when the engine must compute over it.
4. **Lingua concept / unit** — shared vocabulary with no engine functions. Meaning, not machinery.
5. **fds sidecar** — structured data one sand alone cares about. The incubator.
6. **Sand (web interface)** — everything that is *seen*; state abstractions; whole un-Lince-y apps.
7. **Embedded foreign app** — the Freedoom pattern: when the world already built it well and the license allows, embed it in a sand and wire Lince's capabilities around it, honestly.

**The placement rule:** *the core owns what must be computed, verified, or agreed across Organs; interfaces own what is seen; embed honestly what the world already built well.*

### The cases

**1. Todo, knowledge base, learning.** Records and links, Karma for the daily Need counters; the todo and kanban sands render. Learning and research organization is web-sand territory (easy HTML customization); trails of knowledge emerge from records + links + Lingua (Part IV). *Rungs: primitives + sands.*

**2. Recurring tasks.** Karma + Frequency, nothing else — the frequency fires, the rule posts the fact, the Need appears on the set day. *Rung: pillar, pure.*

**3. Donation and buying (iFood/Amazon-like).** Core-heavy: catalog items are records with concepts and units; offers and wants are open promises; Senses drafts the match; Transfer carries agreement and settlement; Trust makes the seller's history checkable; delivery is a promise window plus the place Instinct's route. The storefront, cart, and browsing experience are sands. *Rungs: primitives + four pillars + one Instinct + sands.*

**4. Transport from A to B.** The case that forced an Instinct. `route(a, b)` — path, distance, ETA, alternatives via A\* over map data — is an engine function, because matching, choreography, and simulation all need it: Senses matches riders and drivers by route-overlap × window-overlap; both parties see the same proposal rendered by a ride sand. The ride-share *interface* is not core; the *route* is. Live traffic, if ever, arrives as Signals. *Rungs: Instinct + Senses/Transfer + sand.*

**5. Group coordination and delegation.** Organs hold the people; assignment is a promise whose party is the assignee; task metadata rides work-metadata; kanban/gantt sands render the flow, and card-pushing automations are ordinary Karma. *Rungs: primitives + pillars + sands.*

**6. Chat and calls (Discord/WhatsApp-like).** Thin core, deliberately: messages attach to *anything* (the unified message table generalized — a chat is messages referencing a record, a Transfer, an Organ), synced through the Ledger, streamed live through Protein. Presence and typing indicators ride the ephemeral lanes — never persisted. Calls: the AV transport is never core — embed an open-source stack (the Jitsi/Freedoom pattern) inside a sand; the core contributes what it is uniquely good at: the contact list (Organ users), a call-invite Action, and Karma-triggered calls — "call mom Sunday 19h" or "open a call with all parties when the Transfer reaches agreed" are rules ending in a notify Effect plus a call deep-link. *Rungs: primitives + ephemeral lanes + embed + sand.*

**7. Real-time collaborative documents.** The text CRDT relay (already designed for `record.head`/`record.body`) plus the one record-editor sand every other sand embeds. Cursors and selections are ephemeral-lane presence, never Ledger. Document-specific settings sit in fds. *Rungs: pillar (Memory/sync) + ephemeral lanes + sand + fds.*

**8. Social network (federated, mastodon-healthy).** Zero new core — the strongest proof the primitives are right. Posts are records with organ/public visibility and media resource refs; follows are Organ contacts; the feed is one Protein across followed Organs; boosts are republications; replies are messages. The public profile — "this is me, here is my verifiable history, here is what I offer" — is a published Collection served by the sandbox host already specified in `Playground.md`. Tweets, photos, long videos, blogposts: all of it lives in web sands, jammed there on purpose. *Rungs: primitives + existing pillars + sands.*

**9. Command flows (n8n-like).** Karma 2.0 *is* an n8n: Signals in, rules as nodes, Effects out, the dependency graph already derived. The Karma Orchestra sand is the node-graph editor — pure rendering over rules that already exist as data. *Rung: pillar + sand.*

**10. Code editor.** Skipped by doctrine. The terminal sand already carries helix and agent CLIs; classic terminal workflows should stay terminal workflows. The world built great editors; embed or ignore. *Rung: embed/none.*

**11. CRM and people management.** Thin core, fat sand — and that is correct. People are records with `@person` concepts; relationships are links; birthdays are Frequencies; likes/dislikes/goals are fds until patterns earn Lingua promotion; interaction metrics are Protein aggregations over facts ("how did this Organ's members interact"). The pipeline views, profile pages, and reminders UX are sands. *Rungs: primitives + fds + Protein aggregation + sands.*

**12. Personal finance.** The core showcase. Accounts are records with currency units; every movement is a fact with a cause; recurring bills are rule-emitted promises; income is transfer promises from other parties; runway and "will rent clear" are Imagination. The sand is mostly charts over Proteins. *Rungs: nearly all core; sand is presentation.*

**13. Inventory, production, and scheduling (family producer → industry).** The second showcase. Stock is records with units and places; recipes and bills-of-material are quantified links; production runs are promise chains; customer connection is Transfer chains; scheduling and "can we deliver by the 12th" are Imagination over the whole graph. Gantt and planning boards are sands. *Rungs: primitives + links-as-recipes + pillars; sands render.*

**14. World analysis and statistics.** Protein aggregation across consenting Organs (visibility gates everything) + Imagination trend projections; clustering, marginal-utility analysis, and chain-of-promises optimization are a far-future *Imagination extension* — in the backend when they come, exposed through Protein, never trapped in one interface. The heavy visualization (maps of need-mountains, trend dashboards) is L4-style sand work. *Rungs: pillar engines + sands; optimization explicitly future.*

**15. AI conversation over your data.** Pure sand: Protein for the data, Fiote for the agency, any LLM behind it. Nothing about it touches the core, which is exactly why it is safe to build. *Rung: sand.*

### Expansions — the same window, wider

The variety above, checked against every note in the institute, extends without changing the pattern:

**16. Calendar and time budgeting** *(Calendar.md)* — records with a time cost occupy the timeline; Frequency supplies recurrence; Imagination lays the projected calendar out; the calendar sand renders and edits. **17. Health, habits, and IoT** *(Microcontrollers, Computer Vision, Food)* — devices are Signal sources posting facts (the scale, the camera, the sensor), rules do the habit pressure, the Todo-blob does the reward; capture-heavy, UI-thin. **18. Games** *(Chess, Freedoom, Game of Life, THE Game)* — three honest shapes: game state in fds (chess today), embedded engines in sands (Freedoom), and THE Game reading real Records with Karma as the rulebook (L4). **19. Education** *(GdE)* — classes are Organs, sprints are promise bundles, curricula are trails, cohort progress is visible facts. **20. Garden and farm** *(Digital Garden.md)* — plant records with places, watering rules, death-chance signals; scales continuously up into case 13. **21. Recaps** *(TMIL)* — a monthly rule queries the Ledger and publishes a bundle of records; provenance makes "what happened this month" free.

### What the Window deduces

Held against twenty-one workflows, the triage forced exactly **four** additions into the core — and no more:

1. **The place Instinct** — because matching, delivery, and choreography must compute routes, not render them.
2. **The ephemeral lanes** — because presence, cursors, and call signaling are real-time truth that must *never* become Ledger facts.
3. **Messages attach to anything** — because chat is not an app, it is a dimension of every shared object.
4. **The embed-honestly doctrine** — because the cheapest correct implementation of a solved problem is the world's, wired to Lince's automation and contacts.

Everything else in all twenty-one cases lands on sands, fds, Lingua, or pillars that already exist. That is the Window's verdict on the primitives: they are the right size. And its deeper lesson is the repetition — finance, inventory, the pantry, and the farm are *the same shape* (units + facts + promises + Imagination); chat, comments, and negotiation are the same shape (messages on a shared object); profiles, catalogs, and libraries are the same shape (published records behind visibility). Apps are projections of one organism. The Window is how we keep deducing the next abstraction from first principles instead of appending it.

---

## Coda

The current Lince proves the philosophy can be data. The reborn Lince makes the philosophy *executable at the level it was always aimed at*: not one person's spreadsheet of habits, but the connective tissue between people who intend to meet each other's Needs — remembering every change, speaking a common tongue, keeping its promises visible, imagining forward, and asking for a human only when a human is what's needed.

Everything is a Record. Every change is a Fact. Every intended change is a Promise. The rest is choreography — and Protein is how the dance is seen.
