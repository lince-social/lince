# Fable Improvement v2: The Lince Rebirth

A deep analysis of Record, Karma, and Transfer; the structural change they are asking for; the pillars Lince still needs; and the experience that carries Lince from a tool you open to a synchrony you live inside.

This is v2. The first version was annotated in place and every comment is absorbed here: names corrected (Record stays Record, Karma stays Karma, DNA goes back to being a synonym for the database and nothing more), embedded questions answered (relations, extensions, ledger speed, Lingua), and one major concept added that v1 missed entirely: **Protein**, the expression layer. Compatibility is deliberately and fully ignored — this describes the best architecture, to be built clean, with old data ported by hand at the end.

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

5. **`record_extension.freestyle_data_structure` is a confession.** Every JSON escape hatch marks a primitive the model lacks. *Resolution:* keep fds, but give structure a **promotion path** with a clear rule for each tier: fds is the private incubator (only one sand cares); a Lingua-typed attribute is the middle tier (Organs must *agree* on it, so it needs shared vocabulary); a core column is the top tier (the *engine* must compute over it — quantity, concept, unit, place). When the same key keeps appearing across many fds namespaces, that is the signal to graduate it. fds stops being a confession and becomes a nursery.

### Karma: a beautiful idea running on assembly-language ergonomics

Condition → Operator → Consequence is exactly the right size of idea: small enough to explain to a child ("if, and, then"), general enough to build finance on. Records as memory cells that rules read and write makes Karma a *spreadsheet of behavior* — and the spreadsheet is the most successful end-user programming model in history. That instinct is correct.

**Where it strains:**

1. **It is a polling register machine over global mutable state.** Every 60 seconds, every condition re-evaluates. Cascades happen by one rule writing a quantity another rule reads. No dependency graph, no loop detection (Proof.md admits this), no termination argument. *This is the real problem #1.*

2. **Tokens are numeric addresses.** `rq1`, `f3`, `c4`. Ids are unstable across sync (the CRDT work already had to invent sync-id mapping because local ids don't travel), unreadable in a month, hostile to sharing. A published blueprint full of `rq14 * f2` cannot be transplanted without surgery. *Resolution:* stable UIDs for machines, slugs for humans — `rq14` dies, `apples.stock` lives.

3. **No provenance.** When a quantity changes, nothing records *why*. Trust in automation is exactly proportional to the system's ability to explain itself. *This is the real problem #2, and the Ledger kills it structurally.*

4. **Delivery every 60s is both too slow and too wasteful.** Too slow for "react to what I just did"; too wasteful for monthly rules. Spreadsheets solved this decades ago: recompute what depends on what changed. The loop must get better; Karma 2.0's derived dependency graph is how.

**Accepted trade-offs, kept on purpose:** the Operator's conflation of trigger and payload (`=` passes non-zero and carries the value) stays — it is quirky but it is The Lince Way and it works. Shell Commands stay *out* of condition evaluation not as a loss but as an intentional gain: conditions that never touch the world are what make Deterministic Simulation Testing and Imagination possible. Commands become Signals (sampled on their own schedule, written as facts) and Effects (consequences, queued and logged) — the same power, honestly placed.

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
- `place` — where this Need/Contribution physically lives, for the map and logistics.

Time is deliberately **not** a Record column: record-level timing is Karma + Frequency composed from first principles, with interface sugar that writes the counters for you when you pick a date range. Time lives declaratively on the Promise, where strangers need to read it.

And Records get **names, not just ids**: UID for machines and sync, slug for humans and rules. Published Karma becomes readable, transplantable data.

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
        condition?, transfer_uid?, rule_uid?)
```

A promise is a fact that hasn't happened yet: a delta, a declarative time window, a party behind it (or an **open** party slot — see below), possibly a condition. Example:

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
- **Rules** — reactive, named, pure: `when <records/signals change or timer fires> if <expr over projections> then <emit facts | promises | effects | ask>`. The engine derives the dependency graph from the expressions (the token parser already exists — this is close), recomputes only what changed (spreadsheet semantics; the 60s heartbeat survives only as a floor for timers), detects cycles statically, and warns: "these three rules form a loop that diverges." That is the Proof.md feature, falling out of the dependency graph nearly free.
- **Effects** — shell/SQL/network actions, queued, logged as facts, never run inside evaluation.

The consequence kind **`ask`** is new and small but load-bearing: instead of acting, the rule enqueues a human decision (see Attention, Part IV). "When apples < 3, *ask me* whether to send the reorder proposal" is a first-class rule, not a notification hack.

Rules reference records by slug. Every firing writes its fact with `cause = rule:<uid>` — automation that can always answer "why". Operator semantics (`=`, `=*`) survive unchanged inside the `if` expression.

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

- **Includes are the end of custom plumbing.** In v1 this was a hand-wave ("sands get provenance for free"); here is what it means: today, a sand that wants to show *why* a card's number changed would need bespoke joins across history tables that don't even exist coherently. Under Protein, it adds `facts(with: cause)` to its request — one line — and every sand on the platform gains provenance, promises, and projections the same way. The capability lives once, in the engine.
- **Saved views are saved Proteins.** The `view` table's raw SQL becomes a named Protein; the SSE view stream becomes the `live: true` behavior of any Protein.
- **Visibility is enforced here.** A Protein evaluated for a remote Organ passes through the same visibility rules as everything else — one gate, not one per endpoint.
- **Storage-independence is the point.** The backend translates Protein to SQL today; when AniccaDB (the future Lince-native store) exists, Protein translates to Anicca queries and no interface notices. Protein is the contract that makes the storage engine replaceable.
- **Writes are Actions**: semantic, typed verbs — `create-record`, `agree`, `settle-transfer`, `activate-rule` — the direction the widget contract already took, now universal. Protein never mutates; Actions never query. Every Action lands in the Ledger with its cause.
- **Transport:** first-party surfaces need one bidirectional, typed, streaming channel (websocket, gRPC — an implementation choice, not a theory commitment). Plain HTTP endpoints remain — not for sands, but for the rest of the world: external systems integrating with a Cell speak HTTP; Lince's own interfaces speak Protein.

### What deliberately does not change

Quantity stays central. The sign convention stays. Head/body stay. Cells and Organs stay. Sands stay. Karma keeps its name, its operator, its spirit. The Lince Way — model with numbers, join with math, act on thresholds — is not being replaced; it is being given a memory, a vocabulary, and a way to be seen.

---

## Part IV — The pillars

Ten pillars. Existing ones keep their names; new ones earn theirs.

**1. Record.** *(existing → refounded)* The state of the world: identity + concept + unit + place, quantity as the universal knob. The nouns. Everything else in Lince is a way of interacting with Records — that sentence from the original notes survives every refounding.

**2. Memory — the Ledger.** *(new, structural)* Every fact, forever, with its cause and optional signature. History, sync, undo, provenance, and audit are all this one pillar. The quantity cache makes it free to read; checkpoints make it bounded to keep.

**3. Lingua — shared concepts.** *(new)* A `concept` table *in your own database* — Lingua is part of your DNA, not an external service. A concept is a shared tag with lineage: `concept(uid, names[], parents[], default_unit?)`. Names are plural and multilingual — "Apple" and "Maçã" are one concept. Your Record points at `@apple`; my Record points at the same uid; matching becomes a join instead of a human squint. You import slang the way you import a sand: inserting concept rows, published and versioned through the same package flow. Units are concepts too, so quantity dimensions ride the same system. No central ontology authority — Organs converge on vocabularies the way communities converge on slang, forks carry lineage, and Senses matches across declared equivalences. Without Lingua, Lince is a diary; with it, Lince is a language.

**4. Karma.** *(existing → refounded)* Signals, Rules, Effects, and `ask`. Reactive delivery over a derived dependency graph, static loop warnings, full provenance. Fast, explainable, previewable, provable — same name, same soul.

**5. Transfer.** *(existing → refounded)* An abstraction that *organizes* the primitives rather than owning its own machine: promises bundled under an agreement policy and a visibility policy. Everything already won survives — append-only, signed, derived status, settlement-only mutation — on a quarter of the moving parts, and the total moving-part count across all pillars goes *down*.

**6. Senses — discovery and matching.** *(new)* The layer that watches open promises across known Organs and *proposes* meetings: same concept, compatible units, overlapping windows, feasible places → draft Transfer. **Scoped by proximity, hard.** Automatic discovery and matching operate only within Organs at or under a proximity ceiling you set per matching rule — your ingroups, the Organs you actually know. Senses never auto-expands toward unknown or public Organs; widening the circle is a manual act or an explicitly Karma-gated one (the existing visibility-wave mechanism, kept deliberate). The lynx gets its famous eyesight, pointed only where you aim it.

**7. Trust.** *(new, minimal by intent)* The first and only near-term job: **make every interaction and delta verifiable.** Signatures on facts and promises (the transfer-event signing generalized), so your history of kept promises, donations, and settled Transfers is a public archive *you* can prove and others can check. On top of verifiability — later, carefully: search and aggregation ("who donated what"), and leaderboards only as an opt-in sand among Organs that mutually confide at a chosen trust level. Explicitly deferred: any linkage between reputation and capability — verifiable facts do not become tickets to do more on other nodes until that design is actually thought through. No global score, ever.

**8. Imagination — projection.** *(new, the sleeper hit)* Fold promises and rules forward: `state(t)` for any future `t`. "Your apples run out Thursday. Rent leaves you 300 short on the 5th — unless the freelance Transfer settles, and Maria's verifiable history gives that 92% confidence." Todo apps show tasks; banks show balances; *nobody shows a person their projected state vector with other people's commitments folded in*. This is a **backend engine, a core Lince feature** — like Karma, it is computed in the core and exposed through Protein so every interface gets it at full speed; it is never an interface-side trick. Under Ledger + Promise it is a query loop, not a subsystem.

**9. Protein — expression.** *(new, from the annotations)* The declarative read contract plus typed Actions (Part III). DNA is what you store; Protein is how it comes out and shows its power. Sands speak only Protein/Actions; HTTP remains for the outside world; storage stays swappable underneath (SQL now, AniccaDB later).

**10. Attention — the Decision Queue.** *(new as a pillar, concrete and near-term)* One deterministic object: the queue of everything currently awaiting a human choice —

- promises entering `proposed` or `open` states that target you,
- draft Transfers from Senses matches,
- rules whose consequence is `ask`,
- threshold crossings Imagination projects,
- drafted rules and records awaiting one-tap approval (from UI sugar or from Fiote).

A **whisper** is not generated; it is *routed* — a per-platform rendering of the queue (notification, sound, text digest; configured per device and per UI level) under a budget the user owns, with per-source on/off switches. Whispers also flow *inward*: devices are helpers doing the bulk work of making Records reflect reality — a phone, a scale, a sensor posting facts to your central node — and every capture source is a visible signal-record with an off switch. The everyday shape is plain: desktop for dense, powerful work; mobile for agile-first interactions; helpers whispering in both directions. The far-future ambient hardware (crowns, AR) is explicitly *not* the near-term design — the Decision Queue is, and it works with zero exotic hardware and **zero LLMs**.

**Fiote — the cub.** *(the optional operator, not a pillar)* An agent that turns the same knobs a human turns: it reads what you allow, and it creates Karma rules, drafts records, approves proposals — always as inspectable data with `cause = fiote`, always within a delegated autonomy level you set: *observe → suggest → draft → act-within-budget*. The human delegates to the AI the switching of knobs they could switch themselves; nothing Fiote does is a different *kind* of thing. Without an LLM, the Decision Queue works fully. With one, the quantity and quality of whispers rises, and — at the autonomy levels you grant — decisions leave the queue before they ever cost you attention. Theory only; no stack is chosen here.

**Where Alexandria went.** Trails of knowledge — the shareable bundles that install a capability ("keep a sourdough starter", "run a small farm") — are no longer a pillar or a feature, because they no longer need to be: a trail is an importable subgraph of records + links + concepts + rules + views, published through the ordinary package flow. Records give it identity, links give it structure, Lingua makes it transplantable, Protein makes it visible. A built-in capability with no built-in concept: the abstraction the primitives were sharpened to allow.

---

## Part V — The experience: all levels of interacting

Levels, from bedrock to air. Each level is complete — nobody is forced upward — and every level is a view over the same Ledger, through the same Protein.

**L0 — The Table.** Raw database access: SQL, the table sands, the Operation command line. The truth, always inspectable. Trust in the higher levels is anchored by the permanent ability to drop here and see the same facts.

**L1 — The Board.** Sands composed on boards: kanban, relation graphs, calendars, dashboards, chess. Today's main surface, refounded on Protein: a sand states *what* it needs — source, filters, aggregation, includes — and gets a snapshot plus a live stream, with provenance and promises one `include` away. Building a rich sand stops meaning "invent your own data plumbing" and starts meaning "write a good Protein and a good surface."

**L2 — The Conversation.** Operation grows into a language; natural language compiles *to* it. "Every Monday I need to prep meals for the week" → a drafted rule + records shown for one-tap approval — never silently applied. Human and AI switch the same knobs: the AI's drafts are ordinary rules and promises with `cause = fiote`, reviewed in the ordinary queue. Explainability is structural, not aspirational.

**L3 — The Whisper.** The Decision Queue, rendered. A few routed whispers a day — notification on the phone, a line of text on the desktop, a sound in the kitchen — each answerable by voice, one tap, or a pre-authorized nothing ("silence = yes" only where a rule explicitly granted it). Inward, the helpers work: the fridge photo becomes stock facts, the receipt posts deltas, each source visible and disableable. You live; helpers whisper reality into your Cell; the plan recompiles when reality diverges. No exotic hardware required, no LLM required — those only raise the ceiling.

**L4 — The World.** The map and the game. Needs and Contributions rendered over real terrain — mountains of unmet Need, valleys of surplus — scoped by visibility and consent. Walk your neighborhood with the overlay: the bakery's flour Need, the school's volunteer window, the neighbor's surplus tomatoes on your literal route home. And THE Game: your real Records seed the landscape; Karma writes the game rules; finishing your tasks feeds the Lincegoshi blob that grows and bursts into light. Play and life stop pretending to be different activities.

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

1. **Core schema.** `record` (uid, slug, concept, unit, place, quantity-cache) + `concept` + `link` + `fact` + `promise`. The single write path: the fact-appender with its transactional quantity cache and checkpoint/compaction policy.
2. **Karma 2.0 engine.** Signals, Rules, Effects, `ask`; dependency graph; reactive delivery; static loop warnings (Proof ships here); provenance on every firing.
3. **Protein + Actions.** The read contract with includes and live streams; typed Actions; the first-party transport channel; HTTP kept at the boundary for external systems.
4. **Transfer on promises.** Bundles, parties, agreement policies, visibility policies; settlement as fact-appending; open promises as published Needs.
5. **Imagination engine.** `state(t)` in the backend core; the timeline sand; the scrubbable future.
6. **Lingua publishing + Senses.** Concept packages through the ordinary publication flow; the matcher over open promises, proximity-ceilinged, ingroup-only automation.
7. **Trust.** Signatures on facts and promises; verifiable public history; search and aggregation over verified deltas. (Leaderboard sands and anything linking reputation to capability: explicitly later.)
8. **Attention.** The Decision Queue; whisper routing per platform; capture sources as disableable signal-records.
9. **Fiote.** The autonomy ladder over the existing knobs; `cause = fiote` everywhere.
10. **World + Synchrony.** The map, the game, multi-Cell choreography.

**Risks worth respecting:**

- **Ledger growth** — controlled by design: checkpoint facts + compaction of archived ranges; facts are foldable, and the quantity cache means read paths never depend on log size.
- **Lingua politics** — vocabularies drift and fork; don't fight it, forking is how language works. Concepts carry lineage; Senses matches across declared equivalences; convergence is social, not enforced.
- **Capture consent** — every signal source is a visible record with an off switch; every fact shows its cause; local-first stays non-negotiable; nothing leaves the Cell without a visibility rule saying so.
- **Whisper fatigue** — the attention budget is a *hard* budget the user owns, and Lince never has a growth metric that benefits from interrupting anyone. This is the structural advantage of not being a platform with a cut.
- **Greenfield discipline** — the cost of ignoring compatibility is losing the dogfood safety net. Replace it deliberately: stand the new core up early and live in it with real daily data (even hand-entered), so the primitives are tested by life long before the last pillar lands. The v1 lesson stands: years of append-only features without holistic passes is how the cathedral grew next to the hut — the rebirth earns its simplicity only if each stage is used, not just built.

---

## Coda

The current Lince proves the philosophy can be data. The reborn Lince makes the philosophy *executable at the level it was always aimed at*: not one person's spreadsheet of habits, but the connective tissue between people who intend to meet each other's Needs — remembering every change, speaking a common tongue, keeping its promises visible, imagining forward, and asking for a human only when a human is what's needed.

Everything is a Record. Every change is a Fact. Every intended change is a Promise. The rest is choreography — and Protein is how the dance is seen.
