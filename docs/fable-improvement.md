# Fable Improvement: Evolving Lince

A deep analysis of Record, Karma, and Transfer; the structural change they are asking for; the pillars Lince still needs; and the experience that could carry Lince from a tool you open to a synchrony you live inside. Compatibility is deliberately ignored. This is written against the code as it exists today and the plans in `notes/institute/`.

---

## Part I — What the three pillars actually are

### Record: the state vector of a life

`record(id, quantity, head, body)`. Four columns. The sign convention (negative = Need, positive = Contribution, zero = peace) is the single best idea in Lince: it gives a _moral direction_ to a number. Every other system stores "tasks" or "inventory" or "balances" as separate universes; Lince stores one universe and lets the sign say whether the world owes you or you owe the world.

The pattern quietly repeats everywhere: every table (`karma`, `command`, `frequency`, `transfer`, `configuration`...) carries a `quantity` column used as its activation/state knob. That is not an accident — it is The Lince Way leaking into the infrastructure, and it is a strength worth naming: _quantity as the universal enable_.

**Where the model strains:**

1. **Quantity means too many things at once.** Count of apples, intensity of a desire, money balance, boolean done/not-done, progress percentage, activation flag. A bare `f64` has no unit and no dimension. `-1 Apple` and `-1 Be someone who plants apple trees` are incommensurable, yet the model treats them identically. This is tolerable inside one head; it breaks the moment two Cells try to trade, because Transfer has no way to verify that my `5` and your `5` are the same kind of five.

2. **A Record is a kind and an instance at the same time.** "Apple" the concept and "my three apples" share a row. There is no way to say that Record 12 in my Cell and Record 731 in yours are _about the same thing_. Every act of matching Needs to Contributions across Cells is therefore human-only. The market Lince dreams of (the LNHM) cannot exist without a shared vocabulary layer.
   H: i can think of difference in apple records as possible, and solved by something that acts as transfer and has title and description

3. **Time is bolted on, not built in.** Quantity is an instantaneous scalar. Needs have deadlines, decay curves, and windows ("3 apples _by Friday_", "insulin _today_"). Today that is expressible only via Frequency + Karma contortions. The most important property of a Need — _when it stops being meetable_ — has no column.
   H: Correct, has no column, but if we can use karma +frequency as first principles then setting something to be a need tomorrow and creating a counter of it, setting quantity to zero day after tomorrow can work, we only need the interface to create that counter when selecting a range.

4. **Relations are second-class.** `record_link` exists, but the meaning of links lives in strings, and the core mental model is still a flat table. Real needs decompose: the party needs the cake needs the flour needs the trip to the store. The graph _is_ the structure of life; Lince renders it as an afterthought.
   H: explain how you would do better.

5. **`record_extension.freestyle_data_structure` is a confession.** Every JSON escape hatch marks a primitive the model lacks. The notes already know this ("what concepts are commonly used... become first class citizens"). The recurring escapees are: unit, location, time window, category, and cross-record identity.
   H: explain how you would do better.

### Karma: a beautiful idea running on assembly-language ergonomics

Condition → Operator → Consequence is exactly the right size of idea: small enough to explain to a child ("if, and, then"), general enough to build finance on. Records as memory cells that rules read and write makes Karma a _spreadsheet of behavior_ — and the spreadsheet is the most successful end-user programming model in history. That instinct is correct.

**Where it strains:**

1. **It is a polling register machine over global mutable state.** Every 60 seconds, every condition re-evaluates. Cascades happen by one rule writing a quantity another rule reads. There is no dependency graph, no ordering guarantee, no loop detection (Proof.md admits this), no termination argument. It is TIS-100 wearing a habit-tracker's clothes.

2. **Tokens are numeric addresses.** `rq1`, `f3`, `c4`, `sql1`, `tq7`. Ids are unstable across sync (the CRDT work already had to invent sync-id mapping precisely because local ids don't travel), unreadable in a month, and hostile to sharing. A published Karma blueprint full of `rq14 * f2` is dead DNA — it cannot be transplanted without surgery.

3. **Conditions have side effects.** A shell Command inside a Condition runs _during evaluation_. That mixes query and effect, makes evaluation non-deterministic, and directly fights the Deterministic Simulation Testing ambition. You cannot replay a day whose conditions shelled out to the network.
   H: We may have for simulation not commands, its ok if its intentional to keep DST.

4. **The Operator conflates _when_ and _what_.** `=` means "non-zero passes, and the evaluated value is carried into the consequence." Trigger and payload are the same number. Wanting "when X crosses 10, set Y to 1" requires arithmetic gymnastics.
   H: thats ok.

5. **No provenance.** When a quantity changes, nothing records _why_. The user's trust in automation is exactly proportional to the system's ability to explain itself, and today it explains nothing.
   H: yeah we need traceability of why the number changed. Important.

6. **Delivery every 60s is both too slow and too wasteful.** Too slow for "react to what I just did"; too wasteful for rules that fire monthly. Spreadsheets solved this decades ago: recompute what depends on what changed.
   H: i agree, the model needs to get better on this loop.

### Transfer: the cathedral next to the hut

Transfer has grown into the most sophisticated part of Lince: parties, structured items with six roles, interactions, three agreement levels, four agreement modes, five reservation policies, confirmations, idempotent settlement, append-only hash-chained signed events, field-level visibility design, chains, spectators, satiation policies. Twenty-five-plus tables.

The deep principles are _right_: a Transfer is a structured promise before execution; settlement is the only thing that mutates Records; history is append-only; visibility is data; status is derived from facts. This is the most philosophically mature pillar.

**Where it strains:**

1. **The elegance inversion.** Record is 4 columns; Transfer is a cathedral. When one pillar needs 25 tables and the other needs 1, the missing abstraction is hiding between them. What Transfer actually is, underneath: _a bundle of promised quantity-deltas that several parties must agree on before they become real_. Nearly every table is scaffolding around the absence of "promised delta" as a primitive.

2. **Transfer already invented the event log — but only for itself.** `transfer_event` is append-only, hash-chained, signable, syncable. Meanwhile direct Record edits, Karma consequences, and CRDT sync each mutate quantity through _different_ paths with _different_ histories. Lince has four write paths and one of them accidentally built the architecture the other three need.

3. **Matching is manual.** Discovery caches summaries and polls peers, but nothing can _propose_ that my public Need for apples and your public apple Contribution belong together — because (see Record §2) there is no layer at which they are the same thing.
   H: i am thinking of having a set of patterns you default to. To say that there are Canonical categories or such, and if a record has the Apple category, then it is an apple, i may call it something else (sucks) and trade with others that have that category, that way we can meet things to create transfer proposals automatically.

4. **Trust is deferred, and it is the actual bottleneck.** Agreement levels model consent, not confidence. The thing that decides whether a stranger's proposal deserves attention is a track record — and Lince, whose settled Transfers are signed events, is sitting on the raw material of the most honest reputation system possible without computing anything from it.
   H: i think that we should be able to capitalize on the vast amount of info we can show about ourselves that interacted with real people to do real verifiable good, and so build trust. The signed events are good for that.

---

## Part II — Philosophy: why it could be better

**Needs are flows, not states.** The current model stores the _level_ of the tank; life is about the _rates_ — filling, draining, promised inflows, scheduled outflows. Every interesting question a person asks ("will I run out?", "can I afford to give this away?", "when do I need to act?") is a question about flows integrated over time. A model that stores levels and discards deltas answers none of them natively. `history` and `sum` exist precisely because levels aren't enough — they are the model apologizing for its own shape.
H: agreed completely.

**The unit of meaning is the change, not the value.** "Quantity became 4" is meaningless; "ate one apple", "Maria delivered five", "the daily rule reset it" are meaningful. Lince currently stores the meaningless form and reconstructs the meaningful one with side tables. It should be the other way around: store the meaningful changes; derive the value.
H: i agree, if we can do it without creating a huge event array that takes time to compute, something than can make the change/events main focus and have the resulting level as a quick usage is good (my fear is having to construct all events to show a number, but i know you dont think that, so help me understand more your plan for it).

**A promise is the social atom.** The philosophy says life divides into Needs and Contributions. But the _bridge_ between them — the thing exchanged between people — is neither: it is the promise. "I will bring five apples Thursday." Transfers, Karma consequences-in-waiting, reservations, and the farmer's "the chain will unclog in two days" prediction are all the same object: a delta that has not happened yet, with conditions on it and a party behind it. Lince has this object five times under five names (`transfer_quantity_influence`, reservation rows, chain links, spectators, Karma-activated transfers) and zero times as a primitive.
H: i dont discard the possibility that reorganizing the primitives will dissolve barriers and create genericness and bring good.

**Meeting needs through people is the point; automation is the bonus.** The user's own framing. Test the current architecture against it: Karma (the bonus) is core-adjacent and load-bearing, while matching, trust, and shared vocabulary (the point) are absent. The structure inverts the philosophy. A better structure puts _the meeting_ — discovery, matching, promising, trusting — at the center and makes automation a set of hands that operate that same machinery.
H: i agree

**Attention is the scarcest resource Lince manages, and it is unmodeled.** Interfaceless is not a UI feature; it is the claim that the user's attention should be spent only where a human decision is genuinely needed. That requires the system to know what deserves attention — which requires urgency, deadlines, trust, and projection. Every missing primitive above is also a missing input to the attention decision.
H: interfaceless is far into the future, let's not think about it for now

---

## Part III — The structural change

One refounding, three sentences:

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

### 1. Everything is a Record

Rules, commands, transfers, views, sands, organs, people — all get Record identity (a stable UID, a head, a body, a quantity-as-activation). This is already half-true (`quantity` on every table; DNA publications are records) — finish it. What it buys, uniformly and for free: one visibility system, one sync system, one permission system, one publication system, one way for Karma to act on _anything_ (a rule can activate another rule, a view, a transfer — because they are all records), one search, one graph.

Records gain four optional first-class fields, retiring the biggest `fds` escapees:

- `concept` — a reference into the shared vocabulary (below). Nullable; private records need none.
- `unit` — dimension of quantity (`count`, `BRL`, `kg`, `hour`, `bool`). Nullable = "pure number", exactly today's behavior.
- `window` — the time shape: deadline, validity range, decay. Nullable = eternal, today's behavior.
- `place` — where this Need/Contribution physically lives, for the map and for logistics. Nullable.

And Records get **names, not just ids**: a stable UID for machines and sync, an optional unique slug for humans and rules. `rq14` dies; `apples.stock` lives. Published Karma becomes readable, transplantable DNA.
H: i see the vision, but needs more polishing and examples.

### 2. Every change is a Fact (the Ledger)

The single biggest change. Drop `record.quantity` as a mutable cell. Introduce one append-only table at the center of the entire system:

```
fact(uid, record_uid, delta, at, actor, cause_kind, cause_uid, signature?)
```

`quantity(record) = Σ delta` — a projection (materialized, cached, whatever; an implementation detail). Every fact carries its cause: `user_edit`, `rule:<uid>`, `settlement:<transfer_uid>`, `sync:<organ_uid>`, `sensor:<signal_uid>`.

What collapses into this one primitive:

| Today                                        | Under the Ledger                                                                                |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `history`                                    | the Ledger itself                                                                               |
| `sum` (delta/positive/negative over windows) | a query over facts                                                                              |
| Karma provenance (missing)                   | `cause` on every fact                                                                           |
| CRDT quantity sync                           | fact replication — deltas commute; merges are trivial and conflict-free by construction         |
| Transfer settlement idempotency machinery    | settlement = appending facts that reference the transfer; replay-safe by uid                    |
| Undo (missing)                               | compensating fact                                                                               |
| DST                                          | replay the log; the whole system becomes deterministic by architecture instead of by discipline |
| "why did this change?" (missing)             | read the fact                                                                                   |

Transfer already proved this architecture works — `transfer_event` _is_ this, scoped too narrowly. Promote it to the spine of the whole organism.

H: i like this, but i'd like access to be cheap and fast, no calculation for simple GET workflows, so having a mutable is important, but making the Fact the main.

### 3. Every intended change is a Promise

```
promise(uid, record_uid | concept, delta, window, party_uid,
        state: proposed|agreed|active|kept|broken|withdrawn,
        condition?, transfer_uid?, rule_uid?)
```

A promise is a fact that hasn't happened yet, owned by a party, possibly conditional. This one primitive replaces and unifies:

- **Transfer items and quantity influence** — a Transfer becomes: _a bundle of promises + an agreement policy + a visibility policy_. The 25-table cathedral collapses to roughly: `transfer` (the bundle, itself a Record), `promise`, `party`, `agreement`, and the Ledger it already writes to. Double-entry generalized: a balanced Transfer is a bundle whose promises sum to zero per concept across parties.
- **Reservations** — a promise in `active` state _is_ the reservation; `available = quantity − Σ active outgoing promises`. Five reservation policies become one question: at which agreement state does the promise start counting.
- **Chain links and spectators** — private promises conditioned on other promises being kept.
- **Karma-scheduled actions** — a rule that will change something _emits a promise first_ (instantly self-kept for immediate consequences, pending for scheduled ones). Automation becomes previewable and cancelable _by the same UI that shows human promises_.
- **Simulation** — projection is now trivially defined: `state(t) = facts ≤ now + promises kept by t`. The farmer seeing "the chain unclogs in two days" is a query, not a feature.

### Karma 2.0: from register machine to reflex arc

Split the tangle into three honest parts:

- **Signals** — inputs sampled from outside (command output, sensor, HTTP, SQL), each with an explicit sampling schedule, written into the Ledger as facts on signal-records. Conditions become _pure_: they read the Ledger, never the world. Determinism restored; DST becomes possible.
- **Rules** — reactive, named, pure: `when <records/signals change or timer fires> if <expr over projections> then <emit facts | promises | effects>`. The engine derives the dependency graph from the expressions (it parses tokens already — this is close), recomputes only what changed (spreadsheet semantics; the 60s heartbeat survives only as a floor for timers), detects cycles statically, and warns: "these three rules form a loop that diverges." That is the Proof.md feature, and it falls out of the dependency graph nearly free.
- **Effects** — shell/SQL/network actions, queued, logged as facts, never run inside evaluation.

Trigger and payload separate (`when`/`if` vs `then`). Rules reference records by slug. Every firing writes its fact with `cause = rule:<uid>` — automation that can always answer "why".

### What deliberately does not change

Quantity stays central. The sign convention stays. Head/body stay. Cells, Organs, DNA stay. Sands stay. The Lince Way — model with numbers, join with math, act on thresholds — is not being replaced; it is being given a memory, a vocabulary, and a sense of time.

---

## Part IV — The pillars: existing, refounded, and missing

Lince already names itself biologically: Cell, Organ, DNA. Complete the organism. Each pillar below is (existing → refounded) or (new).

**1. Anatomy — Records.** _(existing → refounded)_ State of the world: identity + concept + unit + window + place. The nouns.

**2. Memory — the Ledger.** _(new, structural)_ Every fact, forever, with its cause. History, sync, undo, provenance, and audit are all this one pillar.

**3. Lingua — shared concepts.** _(new)_ A federated, folk-sourced vocabulary: `concept(uid, names[], parent_concepts[], default_unit)`. Published and versioned exactly like sand DNA packages (the infrastructure exists!), adopted virally — when a Transfer arrives using a concept you lack, you can adopt it in one tap. No central ontology authority; Organs converge on vocabularies the way communities converge on slang. This is the pillar that makes machine matching, unit checking, aggregation ("all food-Needs in the neighborhood"), and honest AI assistance possible. Without it Lince stays a diary; with it Lince becomes a language.

**4. Reflex — Karma.** _(existing → refounded)_ Signals, Rules, Effects. The autonomic nervous system: fast, explainable, previewable, provable.

**5. Handshake — Transfer.** _(existing → refounded)_ Promises bundled under agreement and visibility policies. Keeps everything already won: append-only, signed, derived status, settlement-only mutation — on one-quarter of the moving parts.

**6. Senses — discovery and matching.** _(new)_ The active layer that watches public Needs and Contributions across known Organs and _proposes_ meetings: same concept, compatible units, overlapping windows, feasible places, sufficient trust → draft Transfer, ranked by proximity. Today's polling/discovery cache is the plumbing of this pillar; Lingua is its retina. The lynx finally gets its famous eyesight.

**7. Trail — trust.** _(new)_ Not a global score (explicitly rejected in TRANSFER.md, correctly). Locally computed, subjectively weighted: kept-promise ratio, settled Transfers, vouches from Organs you already trust — all derived from signed Ledger facts you can verify yourself. Trust gates attention: strangers with strong Trails may whisper; strangers without them wait in a folder. (The word "Karma" ironically belongs to this pillar; too late, the automation took it.)

**8. Imagination — projection.** _(new, the sleeper hit)_ Fold promises and rules forward: `state(t)` for any future `t`. "Your apples run out Thursday. Rent leaves you 300 short on the 5th — unless the freelance Transfer settles, which María has 92% Trail odds of keeping." Todo apps show tasks; banks show balances; _nobody shows a person their projected state vector with other people's commitments folded in_. This is Lince's most defensible, most philosophy-aligned differentiator, and under the Ledger + Promise model it is a query loop, not a subsystem.

**9. Trails-of-Knowledge — Alexandria.** _(elevate from notes)_ Shareable DNA bundles: records + concepts + rules + views + sands that install a _capability_ ("keep a sourdough starter", "run a small farm", "onboard a new employee"). The wikihow that integrates into your days. Lingua makes bundles transplantable; slugs make them readable; the DNA hub makes them distributable. This is how Lince spreads: not as an app you learn but as ways-of-living you install.

**10. Attention — the interface pillar.** _(new as a pillar, existing as scattered UI)_ One explicit budget: what deserves a human decision _right now_? Inputs: windows (urgency), projection (consequence), Trail (credibility), user rules (consent). Output: at most a few whispers a day, everything else silently handled or parked. Every interface level in Part V is a different rendering of this single pillar. Interfaceless is not "no interface" — it is _this pillar working so well the others become optional_.

---

## Part V — The experience: all levels of interacting

Levels, from bedrock to air. Each level is complete — nobody is forced upward — and every level is a view over the same Ledger.

**L0 — The Table.** Raw DNA: SQL, the table sands, the Operation command line. The truth, always inspectable. Trust in the higher levels is anchored by the permanent ability to drop here and see the same facts.

**L1 — The Board.** Sands composed on boards: kanban, relation graphs, calendars, dashboards, chess. Today's main surface. Refounded models make sands richer for free (every sand can show provenance, promises, and projections without custom plumbing).

**L2 — The Conversation.** Operation grows into a language; natural language compiles _to_ it. "Every Monday I need to prep meals for the week" → a drafted rule + records shown for one-tap approval — never silently applied. The AI (Ask / Agent / Tinkerer from the notes) lives here with one hard law: **AI proposes in the open — as inspectable rules, records, and promises — and the Ledger disposes.** Nothing the AI does is a different _kind_ of thing than what a human does; it is the same primitives with `cause = agent`. Explainability is structural, not aspirational.

**L3 — The Whisper (Interfaceless).** Lince stops being a place you go. It rides in the pocket, the watch, the earphone, the kitchen speaker. Capture is ambient: a photo of the fridge becomes stock facts; a voice note becomes a Need; walking out of the supermarket, the receipt (or the camera, or NFC) posts the deltas. Output is the Attention pillar's few daily whispers, each answerable by voice, one tap, or a pre-authorized nothing ("silence = yes" only where a rule explicitly granted it). The day itself becomes the input device: you live; sensors confirm; facts post; the plan recompiles when reality diverges.

**L4 — The World.** The map and the game (2D Map, Digital-Real-World Maps, GPU Interface notes). Needs and Contributions rendered over real terrain — mountains of unmet Need, valleys of surplus — scoped by visibility and consent. Walk your neighborhood with the overlay: the bakery's flour Need, the school's volunteer Contribution window, the neighbor's surplus tomatoes on your literal route home. And THE Game: your real Records seed the landscape; Karma writes the game rules; finishing your tasks feeds the Lincegoshi blob that grows and bursts into light. Play and life stop pretending to be different activities.

**L5 — Synchrony.** The level that doesn't exist anywhere yet, in any product. Multiple Cells' _projections_ meet and negotiate ahead of time. My apples run out Thursday; your tree over-produces Wednesdays; the Senses pillar notices our routes cross Thursday 18:04, drafts the Transfer, both our Whispers agree (mine by rule, yours by nod), and Thursday evening the handoff happens like it was always going to. Scale it up: a street coordinating bulk purchases; a party where forty promises choreograph themselves; a farming co-op whose Transfer chains reschedule around weather signals; a city whose need-mountains visibly erode week by week. This is "the dance of the world" from the philosophy note, made executable.

### A day inside 2.0

Ana wakes. No dashboard. Coffee — the kitchen scale posts a fact; beans cross their reorder threshold; a promise to the roaster two streets over activates under a rule she approved months ago; his Cell and hers settle it for Saturday pickup. Silence.

8:40, one whisper on the walk to work: _"Say yes to Rui's ride offer Thursday? It unblocks your clinic appointment and he passes your door at 9."_ One nod. Two promises change state in two Cells.

Work is an Organ; her tasks are Needs assigned through Transfers; her worklog facts post as she works — the standup is a view nobody has to fill in. Lunch: projection quietly moves the "eat out" budget record and says nothing, because nothing needs saying.

17:30, second whisper: _"Your mother's pantry Organ shows rice below her comfort level; you're 200m from a market with a strong-Trail seller. Take it?"_ Yes. The Need was never spoken; her mother published it to family only, and family means something because visibility is data.

Evening, she scrubs the timeline out of curiosity — a habit, not a duty: rent fine, the guitar-performance Need she published got a bite from a bar's event Organ (Trail: decent; proposal parked in tomorrow's whispers), the balcony tomatoes will surplus in nine days and the donation rule is already staged. The Lincegoshi is fat and luminous because everything today got met. It dissipates. She sleeps. Total time managing life: under four minutes, all of it decisions only a human could make.

That is the Death of Lince the notes ask about: not a missing app, but management time asymptotically approaching the irreducible minimum — the moments of actual human choice.

### The flagship innovation, named

If one thing gets built to define 2.0, build **the scrubbable future**: the timeline of `state(t)` as a primary surface — your Records projected forward with rules, frequencies, and _other people's promises_ folded in, with Trail-weighted confidence, where dragging a promise or toggling a rule live-recomputes the future. Planning, finance, habits, and coordination collapse into one gesture: _look at tomorrow, and bend it._ Every pillar feeds it; no competitor has the primitives to copy it, because no competitor stores promises between people as data.

---

## Part VI — Build order and honest risks

Sequenced so every step ships value alone:

1. **The Ledger.** Facts with causes; quantity as projection. Rewires Karma writes, Transfer settlement, and CRDT sync onto one path. Everything else stands on this.
2. **Slugs + units + windows** on Records. Small, unblocks Lingua, projection, and readable rules.
3. **Karma 2.0.** Signals/Rules/Effects, dependency graph, reactive delivery, loop warnings, provenance. The Proof feature ships here.
4. **Promises**, and Transfer re-founded on them. The cathedral becomes a bungalow with the same views.
5. **Imagination.** `state(t)`, the timeline sand, the scrubbable future.
6. **Lingua + Senses.** Concepts as DNA packages; the matcher; draft-Transfer proposals.
7. **Trail.** Locally computed trust over signed facts; attention gating.
8. **Attention + Whisper.** The interfaceless layer, mobile/wearable capture, ambient confirmation.
9. **World + Synchrony.** The map, the game, multi-Cell choreography.

**Risks worth respecting:**

- **The Ledger's growth** — an append-only log of a life is big. Answer: periodic snapshots + fact compaction (facts are foldable by design), and it is still smaller than the photos a person takes in a month.
- **Lingua's politics** — shared vocabularies drift and fork. Answer: don't fight it; forking is how language works. Concepts carry lineage; the Senses pillar matches across declared equivalences; convergence is social, not enforced.
- **Sensor consent** — L3 lives or dies on trust. Answer: every signal is a visible record with an off switch; every fact shows its cause; local-first stays non-negotiable; nothing leaves the Cell without a visibility rule saying so.
- **Whisper fatigue** — one manipulative notification and users amputate L3. Answer: the attention budget is a _hard_ budget the user owns, and Lince never has a growth metric that benefits from interrupting anyone. This is the structural advantage of not being a platform with a cut.
- **The rewrite trap** — this document ignores compatibility, but ship each stage into the living dogfood; the Ledger can be introduced under the existing API surface (writes append facts, reads hit the projection) before anything above it changes.

---

## Coda

The current Lince proves the philosophy can be data. The refounded Lince makes the philosophy _executable at the level it was always aimed at_: not one person's spreadsheet of habits, but the connective tissue between people who intend to meet each other's Needs — remembering every change, speaking a common tongue, keeping its promises visible, imagining forward, and asking for a human only when a human is what's needed.

Everything is a Record. Every change is a Fact. Every intended change is a Promise. The rest is choreography.
