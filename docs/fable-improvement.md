# - [ ] Lince Core — Implementation Blueprint (v4)

This document is the executable form of the theory (the v3 essay lives in this file's git history; its meaning is preserved here in condensed form so this document is self-sufficient). Every part and section title carries a checkbox — check it when that piece is implemented and verified. Each part ends with an **Interactions** block documenting its contracts with every other part, so no section's meaning depends on context outside this file.

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared concepts; Instinct tier = concepts with engine functions) · Karma (Signals → Rules → Effects) · Transfer (promise bundles under agreement + visibility) · Senses (matching open promises, proximity-scoped) · Trust (verifiable signed deltas) · Imagination (state(t) projection + confidence) · Protein (declarative reads) & Actions (typed writes) · Attention (the Decision Queue) · Fiote (optional agent operating the same knobs).

**The placement rule (the Window):** the core owns what must be computed, verified, or agreed across Organs; interfaces own what is seen; embed honestly what the world already built well. Altitude ladder: Primitive → Pillar engine → Instinct → Lingua concept/unit → fds sidecar → Sand → Embedded foreign app.

**Non-negotiables carried from theory:** quantity stays central (negative = Need, positive = Contribution, zero = peace); quantity-as-activation on everything; full math in Karma conditions (tokens substituted, then evaluated); compatibility fully ignored — greenfield build, old data ported by hand; DNA is a synonym for the database, nothing more; local-first; no global reputation score, ever.

**Conventions used below:** SQL is SQLite dialect (sqlx today; Protein is the abstraction that later permits AniccaDB). Rust sketches are shape-accurate, not compile-ready. `@slug` denotes a record or concept reference by slug; `r_/f_/p_/l_/c_/t_` prefix ULIDs denote uids by type (record, fact, promise, link, concept, transfer). Timestamps RFC3339; durations `90s`, `2h`, `30d`.

---

# - [ ] Part 0 — The Spine: one organism, one write path

**Why.** Today Lince has four quantity write paths (UI edit, Karma, Transfer settlement, CRDT sync) with four histories. The new core has exactly one: everything that changes state goes through the fact appender. The Spine is the process architecture that enforces this.

## - [ ] 0.1 Crate layout

Implemented crate names (the pure core cannot
be called `core` — that name collides with Rust's built-in — so it is `nucleus`,
the part of the Cell that holds the machinery of meaning):

```text
crates/
  nucleus/    # pure domain: types, ids, expression parser + eval, link-graph
              # algorithms, frequency math. NO IO, NO SQL. DST-testable alone.
  store/      # the only crate that speaks SQL. Schema, migrations, typed repos.
  engine/     # the organism: fact appender, karma scheduler, effect runner;
              # later: senses matcher, attention router, imagination, sync.
  protein/    # Protein AST -> store reads; canned Proteins (focus/decision queue).
              # Read-only by construction: the crate has no write path at all.
  transport/  # (pending) first-party channel, ephemeral lanes, HTTP boundary.
```

- [x] `nucleus` compiles with no async, no sqlx, no network deps.
- [x] `store` exposes typed repositories only; no other crate imports sqlx (engine uses `store::sqlx` re-export for transaction types only).
- [x] `engine` is the only writer; Protein will be read + Action forwarding.
- [ ] A DST harness runs `nucleus` + in-memory `store` with a virtual clock and replays a fact log deterministically. (Foundations in place: every engine entry point takes explicit `now`, `Store::open_memory()` exists, `seal` is replay-deterministic — the harness itself is not built.)

## - [ ] 0.2 The engine loop

```rust
// engine/src/main_loop.rs — the whole organism in one select
loop {
    tokio::select! {
        // 1. A fact landed: recompute only the rules whose inputs changed.
        fact = fact_bus.recv()        => karma.on_change(&fact),
        // 2. A timer fired (Frequency wheel): evaluate time-dependent rules.
        tick = timer_wheel.next()     => karma.on_timer(tick),
        // 3. Effects run OUTSIDE evaluation, from a durable queue.
        eff  = effect_queue.due()     => effects.run(eff).await,
        // 4. Signals sample the world on their own schedule -> facts.
        sig  = signal_scheduler.due() => signals.sample(sig).await,
        // 5. Sync: inbound packages decompose into fact/promise imports.
        pkg  = sync.inbound()         => importer.import(pkg).await,
        // 6. Senses: periodic matching pass over cached open promises.
        _    = senses_interval.tick() => senses.match_pass().await,
    }
}
```

- [x] `fact_bus` is an in-process broadcast every appended fact is published to (`Engine::subscribe`).
- [x] State changes funnel through `append()` (0.3) — grep-proven: `UPDATE record SET quantity` appears exactly once in the codebase. (The full `select!` loop with signal sampling, sync import, and senses arms lands with their stages; today `append`/`tick`/`run_due_effects` are the arms, called synchronously — deterministic and test-friendly.)

## - [ ] 0.3 The one write path

```rust
// engine/src/append.rs
pub fn append(tx: &mut Tx, new: NewFact) -> Result<Fact> {
    let fact = seal(new, tx.prev_hash()?)?;   // uid, at, hash-chain, author signature
    tx.insert_fact(&fact)?;
    tx.bump_quantity_cache(&fact.record_uid, fact.delta)?;  // SAME transaction
    Ok(fact)                                   // caller commits; bus publishes on commit
}
```

**The Fact is the truth; the quantity is the cache.** `record.quantity` is a real mutable column with exactly one writer — this function. Reads are O(1) column reads; nothing ever folds the log at read time.

- [x] `append` is the only function that touches `record.quantity` (`engine/src/append.rs` + `store::records::bump_quantity`).
- [x] Publishing to `fact_bus` happens post-commit (no ghost notifications on rollback).
- [x] Batch variant `append_all` for settlements and sync imports (one tx, many facts).
- [x] Idempotency: inserting a fact whose `uid` already exists is a no-op success (tested: replay leaves quantity untouched).

### Interactions (Part 0)
- **Memory (II)** defines the fact row `append` writes. **Karma (VI)** consumes `fact_bus` and emits through `append` with `cause=rule:<uid>`. **Transfer (VIII)** settlement calls `append_all` with `cause=settlement:<uid>`. **Sync (XV)** imports by calling `append` with `cause=sync:<organ>` preserving original author signatures. **Protein (VII)** never writes; **Actions (VII)** terminate in `append` or sidecar-table updates inside the same engine. **DST** replays a recorded fact log through `core` with the virtual clock.

---

# - [ ] Part I — Record: the state vector

**Why.** All Needs and Contributions are Records; quantity's sign is the moral direction. The new core widens identity (uid + slug), meaning (concept), measure (unit), and location (place) — each optional so the four-column soul survives.

## - [ ] I.1 Schema

```sql
CREATE TABLE record (
    uid         TEXT PRIMARY KEY,             -- 'r_' + ULID
    slug        TEXT UNIQUE,                  -- optional, dot.case: 'apples.stock'
    kind        TEXT NOT NULL DEFAULT 'plain',
    -- plain | rule | signal | transfer | decision | device | organ | person | protein | sand
    head        TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    quantity    REAL NOT NULL DEFAULT 0,      -- CACHE. Single writer: append().
    concept_uid TEXT REFERENCES concept(uid), -- Lingua: what this IS
    unit_uid    TEXT REFERENCES concept(uid), -- Lingua: what quantity MEASURES
    place_uid   TEXT REFERENCES place(uid),   -- Instinct: WHERE it lives
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_record_concept ON record(concept_uid);
CREATE INDEX idx_record_kind    ON record(kind);
```

**Everything is a Record.** Rules, signals, transfers, decisions, devices, organs, people, saved Proteins, and published sands are rows here (`kind` selects the sidecar table that holds their specifics). `quantity` is their universal activation knob: a rule with `quantity=0` is paused; a transfer with `quantity=0` is inactive. This buys one visibility system, one sync system, one search, one graph, and lets Karma act on anything.

```text
record  uid=r_8K2  slug=apples.stock         kind=plain     concept=@apple unit=@count  quantity=8
record  uid=r_9C1  slug=rules.apple-reorder  kind=rule                                  quantity=1
record  uid=r_2F7  slug=xfer.saturday-beans  kind=transfer                              quantity=1
record  uid=r_D40  slug=devices.kitchen-scale kind=device                               quantity=1
```

- [x] ULID uid generation with `r_` prefix; uids never reused (`nucleus::id`).
- [x] Slug: optional, unique, `[a-z0-9]+(\.[a-z0-9-]+)*`; rules resolve slugs to uids at registry load and keep both. (Rename Action + re-resolution event: pending with Actions.)
- [x] `kind` enum in `nucleus` with sidecar-table mapping (rule/signal/frequency/decision/transfer sidecars in the schema).
- [ ] Text edits (`head`/`body`) go through the text-CRDT relay (XV) and drop a zero-delta provenance fact (II.3).
- [ ] Time is deliberately NOT a column: record timing = Karma + Frequency counters, generated by interface sugar from a date-range picker. Declarative time lives on Promises (V).

## - [ ] I.2 Extensions ladder (fds stays, as the nursery)

```sql
CREATE TABLE record_extension (            -- unchanged in spirit from today
    record_uid TEXT NOT NULL REFERENCES record(uid),
    namespace  TEXT NOT NULL,              -- 'chess.game', 'task.effort'
    version    INTEGER NOT NULL DEFAULT 1,
    fds        TEXT NOT NULL CHECK (json_valid(fds)),
    UNIQUE(record_uid, namespace)
);
```

**Promotion path:** fds (one sand cares) → Lingua-typed attribute (Organs must agree) → Instinct/core column (the engine must compute). When one key recurs across many namespaces, graduate it.

- [ ] fds CRUD via Actions; namespaced; never holds widget UI state (that stays in host widgetState).

### Interactions (Part I)
- **Memory (II)** owns all quantity changes; Record's `quantity` is its cache. **Lingua (III)** supplies `concept_uid`/`unit_uid` and link kinds. **Link (IV)** connects records. **Karma (VI)** reads any record by slug/uid, writes via facts, and treats `quantity` of rule/transfer/device records as activation. **Protein (VII)** is the only read surface; **Actions** the only write surface. **Instinct place (IX)** backs `place_uid`. **Sync (XV)** replicates records by uid; slugs are local conveniences that travel as suggestions, never as identity.

---

# - [ ] Part II — Memory: the Ledger

**Why.** Needs are flows, not states; the unit of meaning is the change, not the value ("ate one apple", not "quantity became 4"). Storing meaningful changes and deriving the level unifies history, sum, provenance, undo, sync, and DST — and kills the four-write-path problem.

## - [ ] II.1 Schema

```sql
CREATE TABLE fact (
    uid        TEXT PRIMARY KEY,          -- 'f_' + ULID
    record_uid TEXT NOT NULL REFERENCES record(uid),
    delta      REAL NOT NULL,             -- 0 allowed for annotation facts (text edits, checkpoints)
    at         TEXT NOT NULL,             -- RFC3339, engine clock (virtual in DST)
    actor_uid  TEXT,                      -- the person/organ record that authored it
    cause_kind TEXT NOT NULL,
    -- user_edit | rule | settlement | sync | signal | action | fiote | checkpoint | text_edit | compensation
    cause_uid  TEXT,                      -- the rule/transfer/signal/organ uid behind it
    payload    TEXT,                      -- optional JSON (e.g. text-edit summary)
    prev_hash  TEXT NOT NULL,             -- hash chain per Cell
    hash       TEXT NOT NULL,
    signature  TEXT                       -- author signature over hash (Trust, XI)
);
CREATE INDEX idx_fact_record_at ON fact(record_uid, at);
CREATE INDEX idx_fact_cause     ON fact(cause_kind, cause_uid);
```

```text
f_01  apples.stock  -1  08:12  actor=@ana  cause=user_edit                      (ate one)
f_02  apples.stock  +5  10:03  actor=@maria cause=settlement:t_7Q1              (delivered)
f_03  apples.stock  -2  10:04  actor=@ana  cause=rule:rules.apple-donation      (rule fired)
f_04  apples.stock   0  23:59  actor=engine cause=checkpoint payload={"level":10}
```

- [x] Hash chain: `hash = H(prev_hash ‖ canonical(fact))`; one chain per Cell; `verify_chain_step` tested. (Verification *on import* lands with Sync, XV.)
- [ ] Every fact signed by its author's key at creation (XI); imported facts keep the origin signature. (Column exists; signing lands with Trust.)
- [x] `sum` over a trailing window as a query helper over `fact` (`store::facts::sum_window`) — the old `sum` table dies. (only-positive/only-negative/end-lag variants: pending.)
- [ ] Undo = compensation fact (`cause_kind=compensation` exists; the undo Action lands with Part VII).

## - [ ] II.2 Checkpoints & compaction (growth stays controlled)

- [x] Checkpoint fact per record: `delta=0, payload={"level": q}` — `Engine::checkpoint_all` (tested: bypasses the cascade, idempotent sweep; wire it to a nightly rule/heartbeat when the daemon config lands).
- [ ] Compaction: facts older than the retention horizon AND older than the last checkpoint can be folded into the checkpoint and archived to a cold file; hash chain restarts from an anchor fact recording the archive's hash.
- [ ] Config: retention horizon per record kind (finance records may keep forever; signal records days).

## - [ ] II.3 Provenance answers "why"

Any surface can show causality with zero custom plumbing: `include: facts(with: cause)` in a Protein (VII). Text edits drop `delta=0, cause_kind=text_edit` facts whose payload names the CRDT update, so even prose changes are traceable.

### Interactions (Part II)
- **Part 0** `append()` is the only writer. **Karma (VI)**: every firing = fact with `cause=rule:`; Signals write facts with `cause=signal:`. **Transfer (VIII)**: settlement = facts with `cause=settlement:`; replay-safe by fact uid. **Trust (XI)** signs and verifies. **Sync (XV)** replicates facts (deltas commute; conflict-free by construction). **Imagination (XII)** folds facts + promises. **Attention (XIII)**: decision answers are Actions that end in facts. **DST**: the log *is* the test fixture.

---

# - [ ] Part III — Lingua: one vocabulary, four jobs

**Why.** Cross-Cell matching is impossible while "Apple" is only text. Lingua is a concept table *in your own database* — shared tags with lineage, imported like sands. One vocabulary classifies: (1) record concepts `@apple`, (2) units `@kg`, (3) link kinds `@before`, (4) the Instinct tier (IX). A stranger's subgraph becomes *understandable*, not just copyable, because its structure speaks the same tongue as its contents.

## - [ ] III.1 Schema

```sql
CREATE TABLE concept (
    uid            TEXT PRIMARY KEY,      -- 'c_' + ULID
    canonical_name TEXT NOT NULL,         -- snake_case: 'apple', 'before', 'kg'
    origin_organ   TEXT,                  -- lineage: who coined it
    instinct       TEXT,                  -- NULL | 'place' | 'duration' | 'currency'
    created_at     TEXT NOT NULL
);
CREATE TABLE concept_name (               -- multilingual, plural names
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    lang        TEXT NOT NULL,            -- 'en', 'pt-br'
    name        TEXT NOT NULL,
    UNIQUE(concept_uid, lang, name)
);
CREATE TABLE concept_parent (
    concept_uid TEXT NOT NULL, parent_uid TEXT NOT NULL,
    UNIQUE(concept_uid, parent_uid)       -- DAG: 'apple' -> 'fruit' -> 'food'
);
CREATE TABLE concept_equivalence (        -- declared cross-organ same-ness
    a_uid TEXT NOT NULL, b_uid TEXT NOT NULL, declared_by TEXT,
    UNIQUE(a_uid, b_uid)
);
```

```text
c_APL apple    names: en:[Apple], pt-br:[Maçã]      parents: [c_FRT fruit]
c_BEF before   names: en:[before], pt-br:[antes]    parents: [c_PRE precedes]
c_KG  kg       parents: [c_MASS mass]               (a unit is just a concept)
```

- [x] `@name` resolution: canonical name, any language name ("Maçã" → `@apple`, tested), or uid. (Ambiguity as save-time error: pending with Actions.)
- [x] Parent walks power widening queries: `concept in @food` matches `@apple` via the DAG (`store::concepts::descendants_including`, tested).
- [ ] Fallback semantics: an engine that doesn't know `@blocks-softly` treats it as its parent `@blocks`.
- [ ] Unit conversion (later): `concept_conversion(a, b, factor)` rows; only within a shared parent dimension.

## - [ ] III.2 Publishing & adoption

- [ ] Concept packages ride the exact same publication flow as sands (record + extension + resource ref); importing = inserting concept rows preserving uid + lineage.
- [ ] One-tap adoption: when an inbound Transfer/Trail uses unknown concepts, the package carries them; the import UI offers "adopt N concepts".
- [ ] No central authority: forks are normal; `concept_equivalence` lets Senses match across dialects; convergence is social.

### Interactions (Part III)
- **Record (I)**: `concept_uid`, `unit_uid`. **Link (IV)**: `kind_uid` is a concept. **Senses (X)** matches on concept identity/equivalence/parents; unit compatibility via shared dimension. **Instincts (IX)** are concepts with `instinct` set — the engine ships functions for them. **Protein (VII)** exposes `concept in @x` (DAG-aware) filters. **Trails** (emergent): links + concepts make knowledge bundles transplantable. **Sync (XV)**: concepts replicate by uid, so cross-organ joins are trivial.

---

# - [ ] Part IV — Link: relations with meaning and quantity

**Why.** The graph is the structure of life. One typed, quantified link primitive turns decomposition, recipes, curricula, and ordering into structure instead of prose.

## - [ ] IV.1 Schema

```sql
CREATE TABLE link (
    uid      TEXT PRIMARY KEY,            -- 'l_' + ULID
    from_uid TEXT NOT NULL REFERENCES record(uid),
    kind_uid TEXT NOT NULL REFERENCES concept(uid),
    to_uid   TEXT NOT NULL REFERENCES record(uid),
    quantity REAL,                        -- 'cake needs 2 flour'; '0.2 of the whole'
    created_at TEXT NOT NULL,
    UNIQUE(from_uid, kind_uid, to_uid)    -- identity is the TRIPLE
);
CREATE INDEX idx_link_from ON link(from_uid, kind_uid);
CREATE INDEX idx_link_to   ON link(to_uid, kind_uid);
```

**Identity is the triple**, so the same two records carry many links of different kinds at once — `small-step @part-of big-step` and `small-step @before big-step` coexist. Each kind is an independent graph over the same records: the focus queue walks `@before`, progress roll-up walks `@part-of`, recipes walk `@needs`.

## - [ ] IV.2 Core graph algorithms (in `nucleus`, reused everywhere)

```rust
pub fn topo_order(records: &[Uid], kind: ConceptUid, g: &LinkGraph) -> Vec<Uid>;
pub fn walk_tree(root: Uid, kind: ConceptUid, g: &LinkGraph) -> Tree;       // BOM/trail expansion
pub fn derive_needs(root: Uid, qty: f64, g: &LinkGraph) -> Vec<(Uid, f64)>; // recipe explosion
pub fn cycle_check(kind: ConceptUid, g: &LinkGraph) -> Vec<Vec<Uid>>;       // SCCs, warn on save
```

- [x] `topo_order` restricted to a candidate set (active Needs) keeps disjoint chains internally ordered; ties keep candidate order (tested in nucleus and end-to-end via the focus queue).
- [x] `derive_needs(@cake, 2)` multiplies link quantities down the tree → "4 flour, 6 eggs" (tested, incl. shared sub-ingredients merging by sum).
- [ ] Cycle warning on link creation for order-like kinds (children of `@precedes`). (`nucleus::graph::cycles` exists; the on-save hook lands with Actions.)

### Interactions (Part IV)
- **Lingua (III)** supplies kinds. **Protein (VII)** exposes `include: links(kind=@x)` and `order: topo(@x)`. **Imagination (XII)** walks `@needs` to propagate projected shortfalls. **Senses (X)** matches sub-needs from recipe explosions. **Focus queue (Window W1b)** = topo(@before) over active Needs. **Trails** = records + `@before`/`@requires`/`@part-of` links + concepts, published as packages.

---

# - [ ] Part V — Promise: the social atom

**Why.** The bridge between a Need and a Contribution is the promise: a delta that hasn't happened yet, with a window, a party, maybe a condition. One primitive replaces transfer items, quantity influence, reservations, chain links, spectators, and scheduled Karma actions — and makes simulation a query.

## - [ ] V.1 Schema

```sql
CREATE TABLE promise (
    uid          TEXT PRIMARY KEY,        -- 'p_' + ULID
    record_uid   TEXT REFERENCES record(uid),   -- target record (local)
    concept_uid  TEXT REFERENCES concept(uid),  -- OR concept-level (open, cross-organ)
    delta        REAL NOT NULL,
    window_start TEXT,                    -- NULL = now
    window_end   TEXT,                    -- NULL = no deadline
    party_uid    TEXT,                    -- who keeps it; NULL = OPEN slot
    state        TEXT NOT NULL DEFAULT 'proposed',
    -- open | proposed | agreed | active | kept | broken | withdrawn
    condition    TEXT,                    -- optional expr, same grammar as Karma conditions
    transfer_uid TEXT,                    -- bundle membership (VIII)
    rule_uid     TEXT,                    -- emitting rule, if automation-born
    reserve_from TEXT NOT NULL DEFAULT 'active',
    -- never | proposed | agreed | active : state at which it counts against availability
    signature    TEXT,                    -- author-signed (XI)
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    CHECK (record_uid IS NOT NULL OR concept_uid IS NOT NULL)
);
CREATE INDEX idx_promise_record_state ON promise(record_uid, state);
CREATE INDEX idx_promise_transfer     ON promise(transfer_uid);
```

```text
p_9A  apples.stock  +5  window[..Thu 18:00]  party=@maria  state=agreed  transfer=t_7Q1
p_9B  concept=@apple -3 window[..Fri]        party=NULL    state=open            (published Need)
```

## - [ ] V.2 State machine

```text
open ──claim──> proposed ──agree──> agreed ──activate──> active ──settle──> kept
  └─withdraw─┐      └─withdraw/expire─┐                     └──fail/expire──> broken
             └────────> withdrawn <───┘        (edits to connected data drop agreed → proposed)
```

- [ ] Transitions are Actions (VII), each dropping a zero-delta annotation fact on the target record for provenance.
- [ ] `kept` is only ever set by settlement (VIII) or by the rule engine for self-promises — and always alongside the real delta fact.
- [ ] Expiry: a background check moves past-window promises to `broken` (if agreed/active) or `withdrawn` (if open/proposed) and enqueues a decision (XIII) when configured.

## - [ ] V.3 Derived quantities (replaces reservation machinery)

```sql
-- availability view, computed in Protein layer:
available(r)  = r.quantity - Σ |delta| of outgoing promises counting per reserve_from
planned(r,t)  = r.quantity + Σ delta of promises with state>=agreed and window_end<=t
surplus(r)    = r.quantity - Σ reserved outgoing
```

- [ ] The five old reservation policies collapse into `reserve_from` per promise (default from transfer config).
- [ ] Chain links = private promises with `condition: promise_state(@p_upstream) == kept`.
- [ ] Spectators = zero-party local promises watching a source transfer's role settlement, same condition grammar.

### Interactions (Part V)
- **Transfer (VIII)** bundles promises; agreement policy gates state transitions; settlement turns `active → kept` + facts. **Karma (VI)** emits promises (`emit_promise` consequence), reads `promise_state()/confidence()` in conditions; scheduled consequences are promises first (previewable/cancelable). **Senses (X)** searches open promises across organs and drafts fills. **Imagination (XII)**: `state(t) = facts ≤ now + promises kept by t`. **Attention (XIII)**: promises entering `proposed`/`open` targeting you enqueue decisions. **Trust (XI)**: kept/broken history is the confidence raw material. **Windows** are the *declarative* time strangers match on — record-level timing stays procedural (Karma+Frequency).

---

# - [ ] Part VI — Karma 2.0: Signals → Rules → Effects

**Why.** Same soul (if/and/then over full math), better body: pure conditions, a derived dependency graph instead of 60s polling, provenance on every firing, many inputs and many outputs. Commands leave evaluation not as a loss but so DST and Imagination become possible.

## - [ ] VI.1 Schema (rule/signal/effect are records with sidecars)

```sql
CREATE TABLE rule (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),  -- kind='rule'; quantity = active
    condition  TEXT NOT NULL,          -- full math expr, tokens+functions (VI.2)
    gate       TEXT NOT NULL DEFAULT '!=0',  -- '!=0' | '==N' | '<N' | '>N' | 'always'
    carry      TEXT NOT NULL DEFAULT 'value',-- 'value' | 'one' | 'const:N'
    debounce   TEXT                    -- optional min interval between firings ('90s')
);
CREATE TABLE rule_consequence (
    uid      TEXT PRIMARY KEY,
    rule_uid TEXT NOT NULL REFERENCES rule(record_uid),
    position INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    -- set_quantity | add_quantity | emit_promise | run_command | run_query | run_action
    -- set_visibility | advance_transfer | activate | deactivate | ask | notify
    target   TEXT,                     -- slug/uid of record/transfer/effect target
    params   TEXT CHECK (params IS NULL OR json_valid(params))
);
CREATE TABLE signal (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),  -- kind='signal'; quantity = enabled
    source_kind TEXT NOT NULL,         -- command | http | sensor | query
    source      TEXT NOT NULL,         -- shell line / URL / device topic / SQL
    schedule    TEXT NOT NULL,         -- '90s' | frequency slug | 'on_push'
    parse       TEXT NOT NULL DEFAULT 'number' -- number | json:<pointer>
);
CREATE TABLE frequency (               -- kept, sharpened: the time primitive
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    seconds INTEGER, days INTEGER, months INTEGER, day_of_week INTEGER,
    next_at TEXT NOT NULL, finish_at TEXT, catch_up INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE effect_queue (
    uid TEXT PRIMARY KEY, kind TEXT NOT NULL, payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',   -- queued|running|done|failed
    attempts INTEGER NOT NULL DEFAULT 0, created_at TEXT, finished_at TEXT, result TEXT
);
```

## - [ ] VI.2 Condition grammar — full math, honest tokens

Tokens are substituted with real values, then the whole expression is evaluated (Rhai or equivalent). **Booleans are native numbers** (`true=1`, `false=0`) — `quantity(@apples.stock) < 3` needs no `* 1` workaround. Bare `@slug` sugar means `quantity(@slug)`.

```text
quantity(@x)              current cached quantity            @x  (sugar)
freq(@daily-7am)          periods elapsed since last check (0 almost always; catch-up aware)
signal(@fridge-cam)       last sampled value of a signal record
sum(@x, 30d)              net delta over window    sum_pos / sum_neg variants
value(@rules.burn-rate)   another rule's derived value (VI.4)
promise_state(@p)         0..6 ordinal of state     promise_delta(@p)
confidence(@p)            Imagination confidence 0..1 (XII)
projected(@x, +7d)        Imagination state(t) for record x (XII)
distance(@a, @b) / route_eta(@a, @b)     place Instinct (IX)
hours_since_fact(@x)      time since last fact on x
```

Examples of full-math composition (all legal):

```text
-1 * freq(@daily-7am)
(quantity(@apples.stock) + quantity(@apples.incoming)) / 2
(@checking - value(@rules.monthly-burn)) < 500
freq(@weekly) * signal(@books-count) + sum(@reading.log, 7d)
```

- [x] Parser extracts token set (`Expr::tokens()`) → registry resolves slugs to uids at load and keeps both.
- [x] Purity enforced: conditions evaluate against a prefetched `MapResolver` — no command execution inside evaluation is even possible (Signals are separate rows with their own schedule; sampler pending).
- [x] Implemented condition functions: bare `@x`/`quantity`, `freq`, `signal`, `sum(@x, <dur>)`, `value`, `promise_state`, `hours_since_fact`. Parsed-but-pending (error cleanly): `confidence`, `projected`, `distance`, `route_eta`, `demand` (Stages 3/5).

## - [ ] VI.3 The pipeline: condition → gate → carry → consequences

```rust
fn evaluate(rule: &Rule, ctx: &ReadCtx) -> Option<Firing> {
    let v = eval(&rule.condition, ctx)?;          // full math, pure
    if !rule.gate.passes(v) { return None; }      // '!=0' default; '<3'; 'always'
    let carried = match rule.carry {              // trigger and payload finally separate
        Carry::Value => v, Carry::One => 1.0, Carry::Const(c) => c,
    };
    Some(Firing { carried, consequences: rule.consequences.clone() })
}
```

Consequence execution (each independent; all provenance `cause=rule:<uid>`):

| kind | effect |
|---|---|
| `set_quantity` / `add_quantity` | fact on target (delta computed from carried value) |
| `emit_promise` | promise row (params: delta/window/party/reserve_from) — previewable automation |
| `run_command` / `run_query` / `run_action` | enqueue in `effect_queue`; result logged as fact on the effect's record |
| `set_visibility` | visibility rule change on target (XV) |
| `advance_transfer` | agreement/state Action on a transfer (VIII), within policy |
| `activate` / `deactivate` | fact setting target record quantity 1/0 — works on rules, transfers, signals, sands |
| `ask` | decision-record enqueued (XIII) — optional, never mandatory |
| `notify` | notify effect routed per platform config (XIII) |

- [x] **Zero consequences = named derived value.** Tested: a consumer reads `value(@rules.double-x) + 1`, and the dependency graph expands transitively so changes to the derived rule's *inputs* re-evaluate its consumers.
- [x] Worked example — daily habit (tested): freq `@freq.daily-7am`; rule `condition: -1 * freq(@freq.daily-7am)`, gate `!=0`, carry `value`, consequence `set_quantity(@exercise)` → exercise becomes -1 on tick.
- [x] Worked example — reorder ask (tested): `condition: @apples.stock`, gate `<3`, carry `one`, `ask("send reorder proposal?")` enqueues a decision-record; `emit_promise` variant creates a `proposed` promise naming its rule.
- [ ] Worked example — trust-ahead: `condition: confidence(@p.maria-apples)`, gate `>0.9`, consequence `advance_transfer(...)`. (Parses today; `confidence` and `advance_transfer` land with Stages 4–5.)
- [ ] Worked example — quiet hours: `deactivate(@rules.noisy-notifications)` — the consequence kind is implemented; the end-to-end example is untested.

## - [ ] VI.4 Reactive scheduler + Proof

```rust
struct DepGraph { reads: Map<RecordUid, Vec<RuleUid>>, writes: Map<RuleUid, Vec<RecordUid>> }
```

- [x] Built from parsed tokens at registry load; the cascade re-evaluates only readers of changed records, follows their writes, and is capped (256/delivery) — a deliberate two-rule loop is survived in tests.
- [x] Timer wheel from `frequency` rows (`Engine::tick(now)`); catch-up multiplies the returned count; day-of-week filter counts matching boundaries only.
- [x] **Proof (static analysis)**: SCC over reads∘writes → "these N rules form a loop: a -> b", surfaced by `reload_rules()` and tested. (Divergence heuristic: pending.)
- [x] The heartbeat: `Engine::heartbeat(now)` (timers fire, signals sample, effects run — DST calls it with a virtual clock) and `Engine::run(period)` as the daemon wrapper. Signal sampler implemented and tested: samples land as facts with `cause=signal` and trigger the ordinary cascade; unchanged values make no noise.

### Interactions (Part VI)
- **Memory (II)**: every firing and signal sample is a fact; provenance total. **Promise (V)**: `emit_promise`, `promise_state`, scheduled actions as promises. **Transfer (VIII)**: `advance_transfer`, activation via quantity; Karma never invents parties or settles silently — it operates the same Actions a human may. **Imagination (XII)**: simulates rules by running this same pure evaluator on a virtual clock with Signals frozen at last-known values; exposes `confidence/projected` back to conditions. **Attention (XIII)**: `ask`/`notify` consequences; decision-records are rule-readable. **Protein (VII)**: rules/derived values queryable; the Karma Orchestra sand renders the DepGraph. **DST**: pure evaluator + virtual clock = deterministic replay.

---

# - [ ] Part VII — Protein & Actions: the expression layer

**Why.** DNA is the database; it does nothing until expressed. Protein is how any interface reads Lince; Actions are how any interface writes it. Sands and every first-party surface speak *only* Protein/Actions — never tables or SQL — which is what makes the storage engine replaceable (SQL today, AniccaDB later) and provenance/promises/projections available to every sand as one `include`.

## - [ ] VII.1 The Protein AST (canonical wire form: JSON)

```json
{
  "source": "record",
  "where": { "all": [
      { "concept_in": ["@food"] },
      { "lt": ["quantity", 0] },
      { "fn": ["within", "place", {"area": "@home-neighborhood"}] }
  ]},
  "include": {
    "links":    { "kind": "@needs", "depth": 1 },
    "promises": { "state": ["agreed", "open"] },
    "facts":    { "limit": 10, "with": ["cause", "actor"] },
    "extension":{ "namespace": "task.effort" },
    "projection": { "at": "+7d" }
  },
  "aggregate": { "sum": "quantity", "by": "concept" },
  "order": [ { "topo": "@precedes" }, { "asc": "window_end" }, { "asc": "created_at" } ],
  "limit": 50,
  "live": true
}
```

Semantics, normatively:

- [ ] `source`: `record | promise | fact | concept | decision | transfer` — *record/promise/decision implemented and tested*; fact/concept/transfer sources pending.
- [x] `where`: boolean tree (`all/any/not`) of typed predicates; `concept_in` walks the Lingua parent DAG (tested: `@food` matches apple-tagged records, not the hammer). `fn` Instinct predicates: pending (IX).
- [x] `include`: facts (provenance — "the end of custom plumbing", tested), promises (state-filtered), links (kind + direction). Pending: link tree `depth`, extensions, availability (V.3), projection (XII).
- [ ] `aggregate`: sum/count/avg with `by` (concept, unit, day, cause_kind) — the finance/statistics workhorse.
- [x] `order`: `topo(kind)` restricted to the result set with field keys as tie-break — the focus queue ships as `protein::focus_queue()`, tested end-to-end: completing the head promotes the next task.
- [ ] `live: true`: snapshot, then incremental updates. (`protein::affects()` gives coarse `fact_bus` invalidation; the subscription machinery lands with transport.)
- [ ] Saved Proteins are records (`kind='protein'`, the AST in a sidecar) — the old `view` table's successor; sands reference them by slug.
- [ ] **Visibility is enforced here** — one gate: a Protein evaluated for a remote Organ or the sandbox host passes every row and every included attachment through the visibility rules (XV). There is no other read path to leak from.
- [x] The wire format is JSON both ways (tested: the documented request shape parses into the AST; rows come out as JSON).

Worked Proteins (the standards):

```json
// W-focus: the pinned focus queue (Window 1b)
{ "source":"record", "where":{"all":[{"lt":["quantity",0]},{"kind":"plain"}]},
  "order":[{"topo":"@precedes"},{"asc":"promise.window_end"},{"asc":"created_at"}],
  "include":{"links":{"kind":"@part-of"}}, "live":true }

// W-finance: monthly flows by cause
{ "source":"fact", "where":{"all":[{"gte":["at","-30d"]},{"concept_in":["@money"]}]},
  "aggregate":{"sum":"delta","by":"cause_kind"}, "live":true }

// W-provenance: the "why did this change" drawer of any sand
{ "source":"fact", "where":{"eq":["record_uid","r_8K2"]},
  "include":{"facts":{"with":["cause","actor"]}}, "limit":20 }

// W-map: needs near me (L4)
{ "source":"record", "where":{"all":[{"lt":["quantity",0]},
    {"fn":["near","place",{"of":"@me.location","radius":"2km"}]}]},
  "include":{"promises":{"state":["open"]}}, "live":true }
```

## - [ ] VII.2 Actions — the typed write surface

Semantic verbs, validated in the engine, all terminating in `append()` and/or sidecar updates, each logged with its cause:

```text
record:   create-record, edit-record-text (→CRDT), set-quantity, add-quantity,
          set-concept, set-unit, set-place, set-slug, set-extension
links:    add-link, remove-link, relink-order (drag-reorder sugar)
karma:    create-rule, update-rule, create-signal, create-frequency, activate, deactivate
promise:  create-promise, claim, agree, activate-promise, withdraw
transfer: create-transfer, add-party, add-promise-to-transfer, agree-transfer,
          advance-transfer, settle-all-local, set-visibility-policy, send-to-organ
lingua:   create-concept, adopt-concepts, declare-equivalence
attention:decide (answer a decision-record), configure-source, set-budget
publish:  publish-package (sand/concepts/trail subgraph), install-package
```

- [x] First Action set implemented as `engine::actions::Action` (typed, serde kebab-case tag) with `Engine::act`: create-record, set-quantity, add-quantity, activate, deactivate, create-concept, add-link, remove-link, create-promise (incl. open promises), promise-transition (state machine validated; transitions drop annotation facts), decide (closes the decision through the Ledger so Karma can react). Tested end-to-end against Protein reads.
- [x] Every Action carries `actor` and produces provenance (facts and/or annotation facts).
- [x] Protein never mutates (the crate has no write path at all); Actions never query. Permission checks on Actions (role model) and the Protein visibility gate: pending with XV.

## - [x] VII.3 Transport & the ephemeral lanes

- [x] One bidirectional typed streaming channel for first-party surfaces — **WebSocket chosen** (`transport` crate). The transport-agnostic `Session` is the contract; the axum WebSocket driver (`transport::ws`, behind the `axum` feature) is the socket. Proven end-to-end by `membrane/tests/pilot.rs`: a real WS client subscribes, receives a snapshot, sends an Action, receives the live update.
- [x] Multiplexed per connection: N Protein subscriptions + Action request/response + **ephemeral lanes** (`ClientMessage`/`ServerMessage` in `transport::protocol`).
- [x] **Ephemeral lanes**: presence, cursors, typing, call signaling — scoped to a room, fanned out through `transport::LaneHub`, **never written to the Ledger** (tested). Contract: `LaneJoin`/`LaneSend`/`LaneEvent`.
- [x] HTTP endpoints remain only at the boundary for external systems; sands speak only Protein + Actions over the socket.

## - [ ] VII.4 The web/sand migration (the finalization)

**Decision (locked):** the sand system is refactored so **every sand speaks only Protein (reads) + Actions (writes) over the transport WebSocket**. Compatibility is not kept — the old SSE-saved-view streams and the `/api/backend/table` CRUD path are *removed*, not bridged. The new host is the `membrane` crate (the Cell surface); the old `web` crate is frozen and retired at cutover, with data hand-migrated. Whatever this refounding built is the **source of truth**; sands are ported to it, never the reverse.

**The line that must not move:** *board chrome is frontend-only presentation state; sand data is Protein/Actions.* The migration ports the data path of every sand to the new mode **while preserving all the frontend-only board features** that already exist in the web/Tauri version. Those features are host state, not Ledger truth — they live in the board-state store / host `widgetState`, exactly as today, and are carried over verbatim:

- [ ] Infinite canvas with pan/zoom (`BoardCamera`).
- [ ] Multiple named **workspaces** (`BoardWorkspace`), switchable.
- [ ] Per-card **move / resize** (`x, y, width, height`).
- [ ] **Pin** (`pinned`), **z-index ordering** (bring-to-front/back), **grouping** (`group_id`).
- [ ] **Edit mode** (the board's authoring state).
- [ ] **Sand importing** — `.html` and `.lince` archive packages (`LincePackage`, `PackageManifest`, `PackageTransport`, validation).
- [ ] **Sand publishing** — the export/publish flow and the DNA/hub catalog pickup.
- [ ] The **widget bridge** (`window.LinceWidgetHost`) — but re-pointed: its data plane becomes Protein subscriptions + Actions instead of SSE views + table CRUD; its control plane (host metadata, persisted `widgetState`, board layout) stays.
- [ ] **Sand-to-sand ABI events** (`abi_listen`) — carried on ephemeral lanes, never the Ledger.

**What changes for a sand:** it stops choosing a data source (SSE view vs. table CRUD vs. host-mediated routes) and instead (1) subscribes with a Protein for everything it reads — gaining live updates, provenance includes, promises, projections for free — and (2) writes only through typed Actions. The focus-queue corner sand (`membrane/assets/focus.html`) is the reference: ~40 lines, no SQL, no table knowledge. Every existing sand (kanban, transfer, relations, karma orchestra, trail, table, home manager) is re-authored to this shape.

**Sands to port** (each: replace its data path, keep its surface):
- [ ] Table sand → `source: record` + create/set-quantity/edit/set-extension Actions.
- [ ] Kanban → record Protein with category/work-metadata includes; card moves are Actions; comments via the message model.
- [ ] Relations graph → `include: links(kind=…)`; edges are add-link/remove-link Actions.
- [ ] Karma Orchestra → rules/derived-values Protein; the DepGraph the engine already derives.
- [ ] Transfer → the Transfer Actions (create/party/promise/agree/activate/settle) + promise/availability includes.
- [ ] Trail → the emergent records+links+concepts subgraph (Part IV coda), imported via packages.
- [ ] Home manager / dashboard → aggregate Proteins.

**Acceptance for the finalization:** the ported board runs every workflow the Tauri board runs today — resize, pin, move, workspaces, edit mode, import, publish — with zero sand still speaking the old data path, and with the new capabilities (live subscriptions, provenance, promises, projections, visibility-gated remote reads) available to every sand uniformly.

### Interactions (Part VII)
- **Everything reads through Protein** — board, TUI, GUI, mobile, sandbox host, and Fiote included. **Karma (VI)** rules and derived values are queryable; `run_query` consequences execute saved Proteins for reads and Actions for writes. **Imagination (XII)** exposes projections as an `include` and as its own source. **Visibility (XV)** has exactly one enforcement point: here. **Attention (XIII)**: the queue is `source: decision`; `decide` is an Action. **Collab (Window case 7)**: CRDT text flows through `edit-record-text`; cursors ride ephemeral lanes. **AniccaDB**: replacing `store` must not change one character of this Part — that is the acceptance test for storage independence.

---

# - [x] Part VIII — Transfer: promise bundles under agreement

**Why.** A Transfer is *a bundle of promises + an agreement policy + a visibility policy*. Everything the cathedral won survives — append-only signed history, settlement-only mutation, derived status — on a quarter of the moving parts. Records never permanently become Needs or Contributions; promises carry the roles.

## - [ ] VIII.1 Schema (a transfer is a record, kind='transfer')

```sql
CREATE TABLE transfer (
    record_uid     TEXT PRIMARY KEY REFERENCES record(uid),  -- quantity = active
    agreement_type TEXT NOT NULL DEFAULT 'individual',
    -- individual | full | percentage | dependency
    agreement_pct  INTEGER,               -- for 'percentage'
    settlement     TEXT NOT NULL DEFAULT 'individual',  -- individual | full
    visibility     TEXT NOT NULL DEFAULT 'hidden',      -- hidden | restricted | public
    max_proximity  INTEGER,               -- for 'restricted' (+ organ allow rules, XV)
    satiation      TEXT,                  -- NULL(inherit) | none | first_completes
    parent_uid     TEXT REFERENCES transfer(record_uid), -- children keep own policies
    source_uid     TEXT                   -- duplication lineage (spectators watch this)
);
CREATE TABLE transfer_party (
    uid TEXT PRIMARY KEY, transfer_uid TEXT NOT NULL,
    actor_uid TEXT NOT NULL,              -- person/organ record
    kind TEXT NOT NULL DEFAULT 'participant',  -- participant | coordinator | observer
    UNIQUE(transfer_uid, actor_uid)
);
CREATE TABLE transfer_agreement (
    uid TEXT PRIMARY KEY, transfer_uid TEXT NOT NULL, party_uid TEXT NOT NULL,
    level INTEGER NOT NULL DEFAULT 0,     -- 0 none/invalidated | 1 reviewed | 2 committed
    at TEXT NOT NULL,
    UNIQUE(transfer_uid, party_uid)
);
-- The items ARE promises (V) with transfer_uid set. Transfer-specific item title/description
-- (hiding the private source record) lives on the promise via a small annotation, not a table.
```

- [ ] Status is derived, never stored: from promise states + agreement levels + policy (`draft → proposed → agreed → in_transfer → settled`; `inactive` when quantity=0).
- [ ] Edits to a bundled promise reset connected parties' agreement to level 0 (counteroffers are edits; agreement returns when parties re-accept).
- [ ] Balance check (advisory, per Lingua concept across parties): a trade sums to zero per concept; donations are deliberately unbalanced.
- [ ] Messages: the unified message model attaches to any record (Window case 6), so transfer chat is simply `messages where subject = t_uid` — no transfer-specific message table.

## - [ ] VIII.2 Agreement policies

```rust
fn policy_satisfied(t: &Transfer, ag: &[Agreement], deps: &[Promise]) -> bool {
    match t.agreement_type {
        Individual => true,                    // each party binds only its own promises
        Full       => all_parties_at_level2(ag),
        Percentage => level2_count(ag) >= ceil(parties(t) * t.pct / 100),
        Dependency => upstream_transfers_satisfied(deps), // via promises' conditions
    }
}
```

## - [x] VIII.3 Settlement (idempotent, the only Record mutation)

```rust
fn settle_all_local(tx: &mut Tx, t: TransferUid, actor: Uid) -> Result<Vec<Fact>> {
    ensure(policy_satisfied(t))?;
    let due = promises_of(t).filter(|p| p.state == Active && p.party_is_local(actor));
    let facts = due.map(|p| NewFact {
        record_uid: p.record_uid, delta: p.delta,
        cause: Cause::Settlement(t), ..signed_by(actor)
    });
    let applied = append_all(tx, facts)?;      // Part 0 — idempotent by uid
    due.for_each(|p| set_state(tx, p, Kept));
    trigger_conditional_promises(tx, t)?;      // chains & spectators (V.3)
    apply_satiation(tx, t)?;                   // first_completes → withdraw siblings
    Ok(applied)
}
```

- [ ] Delivery/receipt confirmations: two annotation facts (`cause=settlement`) gate `active → kept` when the transfer demands confirmation (per-transfer config).
- [ ] Karma may `advance_transfer` and `activate/deactivate` transfers but never invents parties and never settles silently — it calls the same Actions under the same policy checks.

## - [ ] VIII.4 Worked bundles (the standards)

```text
DONATION   t_1: promise(@apples.stock, -10, party=@ana) + promise(concept=@apple, +10, @bruno)
           agreement=individual, visibility=restricted(organ=@neighborhood)
SALE       t_2: promise(@bike, -1, @ana) + promise(@ana.money, +300, @carlos)
              + promise(@carlos.money, -300, @carlos) + promise(concept=@bike, +1, @carlos)
           agreement=full — balanced per concept across parties
RIDE       t_3: promise(concept=@transport-a-b, +1, @rui, window[Thu 9:00 ±15m])
              + promise(@ana.money, -20, @ana); matched by Senses via route overlap (IX/X)
PARTY      t_parent + child transfers (cake, sound, venue), each child its own policy;
           parent exposes aggregate state; dependency agreement chains the children
PRODUCTION farmer→miller→baker: private conditional promises (V.3) relay settlements
           downstream without exposing the chain to the other parties
```

### Interactions (Part VIII)
- **Promise (V)** is the item model; `reserve_from` defaults come from transfer config. **Memory (II)**: settlement facts + confirmation annotations mean the Ledger *is* the transfer history — no separate `transfer_event` table. **Trust (XI)**: signed promises and settlement facts are the verifiable good. **Senses (X)** drafts transfers from open-promise matches. **Karma (VI)** activates/advances within policy. **Attention (XIII)**: inbound proposals and agreement requests arrive as decision-records. **Sync (XV)**: a transfer package = its record + promises + parties + agreements + relevant facts, exported under visibility rules, imported idempotently by uid. **Imagination (XII)** folds agreed/active promises into projections — the farmer's two-days-ahead view.

---

# - [ ] Part IX — Instincts: concepts with engine muscle (place first)

**Why.** Some concepts could be generic strings interpreted by interfaces, but are strictly superior when the engine computes over them. Those graduate to the **Instinct** tier — things the lynx knows without learning: concept + engine functions, callable from Karma conditions and Protein queries, never reimplemented per interface.

## - [x] IX.1 Place

```sql
CREATE TABLE place (
    uid     TEXT PRIMARY KEY,             -- 'pl_' + ULID
    lat REAL, lon REAL,                   -- resolved coordinates
    address TEXT,                         -- human form; geocoded to lat/lon
    area    TEXT                          -- optional polygon (GeoJSON) for regions
);
```

```rust
// nucleus/src/instinct/place.rs — pure over loaded map data
pub fn distance(a: &Place, b: &Place) -> Meters;               // haversine
pub fn route(a: &Place, b: &Place, g: &MapGraph) -> Route;     // A*: path, eta, alternatives
pub fn near(p: &Place, center: &Place, radius: Meters) -> bool;
pub fn within(p: &Place, area: &Area) -> bool;
pub fn routes_cross(r1: &Route, r2: &Route, slack: Meters) -> Option<CrossPoint>;
```

- [ ] Map data: OSM extracts loaded as a local resource (offline-first); geocoding local against the extract; live traffic, if ever, arrives as Signals — never as hidden network calls inside evaluation.
- [ ] Exposure: Karma condition functions `distance(@a,@b)`, `route_eta(@a,@b)`; Protein predicates `near/within` and include `route(a,b)`.
- [ ] `record.place_uid` and promise windows together give logistics: a delivery is a promise with a window and two places.
- [ ] Future Instincts, each only when the engine must compute over it: duration/calendar math (Frequency is the proto-Instinct of time), currency conversion (over Lingua dimension `@money`).

### Interactions (Part IX)
- **Lingua (III)**: an Instinct is a concept row with `instinct` set — it still has names, parents, lineage. **Senses (X)**: route/window overlap scoring. **Protein (VII)**: `fn` predicates + route includes. **Karma (VI)**: place functions in conditions stay pure (map data is local). **The map & THE Game (Window L4)** render what these functions already computed.

---

# - [ ] Part X — Senses: discovery and matching

**Why.** The join Lingua enables becomes proposals: watch open promises across known Organs, propose meetings. Scoped by proximity, hard — automation only inside your ingroups; widening the circle is always a deliberate act.

## - [ ] X.1 The matcher

```rust
struct MatchRule {                        // itself a record (kind='rule' variant 'sense')
    watch: ConceptFilter,                 // e.g. @food and children
    max_proximity: u32,                   // HARD ceiling: organs at/under this only
    min_confidence: f64,                  // counterparty confidence floor (XII)
    auto: Autonomy,                       // draft_only | ask | auto_propose
}

fn match_pass(cache: &DiscoveryCache, my: &OpenPromises, rules: &[MatchRule]) -> Vec<Draft> {
    for r in rules {
        for theirs in cache.open_promises(r.watch, r.max_proximity) {
            for mine in my.complementary(theirs) {       // sign-opposite deltas
                let s = score(mine, theirs);             // see below
                if s.total >= threshold { drafts.push(draft_transfer(mine, theirs, s)); }
            }
        }
    }
}

fn score(a: &Promise, b: &Promise) -> Score {
    concept:  same uid || equivalence || shared parent (weighted by depth)
    unit:     same || convertible within dimension
    window:   overlap(a.window, b.window)
    place:    1 / (1 + route_eta(a.place, b.place))      // or routes_cross for rides
    trust:    verified kept-ratio of the counterparty (XI/XII)
}
```

- [ ] Discovery cache: known-organ open promises polled/pushed under existing contact + trust states (`unknown | known | blocked`); blocked organs excluded everywhere.
- [ ] **Never auto-expands**: no matching against organs beyond `max_proximity`; no public-pool matching unless a rule explicitly says `@public` — and such rules default to `draft_only`.
- [ ] Output: draft transfer (VIII) + decision-record (XIII); `auto_propose` only sends the proposal — agreement always stays with humans or their explicit Karma.

### Interactions (Part X)
- **Lingua (III)** is the retina (concepts/equivalences/parents). **Promise (V)**: open promises are the search space. **Place (IX)**: feasibility and ride matching. **Trust (XI)/Imagination (XII)**: counterparty confidence in scoring. **Transfer (VIII)**: drafts. **Attention (XIII)**: drafts arrive as decisions. **Sync (XV)** feeds the discovery cache. **Karma (VI)**: match rules are records — activatable, schedulable, publishable like any rule.

---

# - [x] Part XI — Trust: verifiable deltas first

**Why.** Settled Transfers signed are an archive of real, checkable good. The near-term job is only verifiability: signatures on facts and promises, authorship undeniable wherever visibility lets data travel. Scores, leaderboards, and any reputation→capability linkage are explicitly deferred; no global score, ever.

## - [x] XI.1 Keys and signatures

```sql
CREATE TABLE identity_key (
    actor_uid TEXT NOT NULL,              -- person/organ record
    key_id    TEXT NOT NULL,              -- rotation: 'ed25519:ana:2026-07'
    public_key TEXT NOT NULL,
    UNIQUE(actor_uid, key_id)
);
```

- [ ] Ed25519 per user and per Cell/Organ; private keys outside the db and repo (OS keychain / file with tight perms).
- [ ] `signature = sign(fact.hash)` at creation (facts) and on each state transition (promises).
- [ ] Import verifies: unknown key → fetch via organ introduction; bad signature → reject row, keep package (quarantine list).
- [ ] Verification is automatic and silent (like sand package checks today) — no user ceremony.

## - [ ] XI.2 Verifiable aggregates (read-only, later UI)

- [ ] Protein over facts/promises with `verified: true` filter: "kept-promise ratio of @maria for @food concepts, last 12 months", "who donated most @clothing in @neighborhood".
- [ ] Leaderboards exist only as an opt-in sand among Organs that mutually confide at a chosen trust level.
- [ ] Deferred by decision: reputation gating capabilities; global scores; transitive trust math.

### Interactions (Part XI)
- **Memory (II)** carries the signatures; hash chain anchors them. **Promise (V)** state transitions are signed — kept/broken history is attributable. **Sync (XV)**: authorship survives replication; visibility decides *what* travels, signature makes *who* undeniable. **Imagination (XII)** computes confidence from this verified history. **Senses (X)** uses confidence in scoring. **Fiote (XIV)**: `cause=fiote` facts are signed by the user's key with an agent marker — delegation is visible, not hidden.

---

# - [x] Part XII — Imagination: state(t) and confidence

**Why.** Fold promises and rules forward: nobody shows a person their projected state vector with other people's commitments folded in — let alone lets their automations trade on it. A backend engine, exposed through Protein; never an interface trick.

## - [x] XII.1 The fold

```rust
// nucleus/src/imagination.rs — pure, DST-shared with Karma
pub fn project(base: Snapshot, until: Time, opts: ProjOpts) -> Timeline {
    let mut clock = base.now; let mut state = base.quantities.clone();
    let mut events = merge(                       // one ordered stream:
        frequency_fires(base.frequencies, until), // timers
        promise_keeps(base.promises, opts.mode),  // expected keep-times (window_end or history-typical)
        );
    while let Some(ev) = events.next_before(until) {
        clock = ev.at;
        apply(&mut state, ev);                    // promise delta / freq tick
        run_rules_pure(&mut state, &base.rules, clock, &mut events); // may schedule more
        timeline.sample(clock, &state);
    }
    timeline                                      // points + threshold crossings
}
```

- [ ] Signals frozen at last-known values during projection (pure by construction).
- [ ] Branching: `project` with modified inputs (toggle a rule, drag a promise) = the scrubbable future; diffing two timelines = the 5D compare view.
- [ ] Threshold-crossing extraction ("apples hit 0 on Thursday", "checking < rent on the 5th") feeds Attention.

## - [ ] XII.2 Confidence (deterministic, from verified history)

```text
confidence(promise) = kept_ratio(party, concept_class, recency_weighted)
                      × window_tightness_factor × dispute_penalty
demand_curve(concept, hour) = normalized histogram of verified facts/promises
                              for that concept by hour-of-day over the trailing window
```

- [ ] Pure functions over verified Ledger data — same inputs, same numbers; no ML in core (Fiote may *suggest*, never silently score).
- [ ] Exposed as Karma tokens (`confidence(@p)`, `projected(@x, +7d)`, `demand(@apple, hour)`) and Protein includes — enabling act-ahead proposals at ≥N% confidence, surprise gifts for recurring needs of people you know, and buying before rush hour.

### Interactions (Part XII)
- **Memory (II) + Promise (V)** are the inputs; **Karma (VI)** shares the same pure evaluator and consumes the tokens. **Protein (VII)**: `include: projection` and `source: timeline`. **Attention (XIII)**: projected crossings enqueue decisions. **Senses (X)**: confidence in match scoring. **Trust (XI)**: only *verified* history feeds confidence. **The timeline sand** (scrubbable future) is pure rendering of `project()` output.

---

# - [ ] Part XIII — Attention: the Decision Queue

**Why.** Attention is the scarcest resource. One deterministic object holds everything awaiting a human choice; whispers are routed renderings of it, LLM-less by default, under a budget the user owns.

## - [ ] XIII.1 Decision-records (kind='decision')

```sql
CREATE TABLE decision (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),  -- quantity: 1 open, 0 decided/expired
    subject_uid TEXT NOT NULL,           -- the promise/transfer/rule/draft it is about
    kind        TEXT NOT NULL,           -- proposal | agreement | ask | crossing | draft
    options     TEXT NOT NULL,           -- JSON: [{label, action, params}]
    default_opt TEXT,                    -- for 'silence = yes' ONLY when a rule granted it
    expires_at  TEXT,
    decided_at  TEXT, answer TEXT
);
```

Sources (all deterministic): promises entering `proposed`/`open` targeting you; Senses drafts; Karma `ask` consequences; Imagination threshold crossings; drafted rules/records from UI sugar or Fiote.

- [ ] Everything is a record ⇒ Karma can read the queue: expire stale decisions, escalate quiet ones, batch low-urgency ones into a digest.
- [ ] `decide` Action executes the chosen option's Action list and drops provenance.

## - [ ] XIII.2 Whisper routing (outward) and capture (inward)

- [ ] **Outward = `notify` Effect**: native sibling of shell Effects; routes a decision or event to a platform channel. Channels are device records (`kind='device'`): desktop toast, mobile push, sound, text digest — each with per-source on/off and quiet hours; the attention budget (max interruptions/day) is a hard user-owned config; overflow parks in the digest.
- [ ] **Voice is LLM-less by default**: templated event text from typed events — "Transfer *Beans, Saturday* advanced to agreed", "apples below 3". If Fiote is active for a source, it narrates the same event its way, per configuration.
- [ ] **Inward = Signals**: every capture source (phone, scale, camera, mic) is a visible signal-record with an off switch; helpers whisper reality into the Cell as facts. As AI-less as possible; where AI is genuinely the best collector (fridge photo → stock facts), it is an implementation of a Signal, nothing more.
- [ ] Everyday shape: desktop = dense powerful work; mobile = agile-first interactions; far-future ambient hardware is explicitly out of near-term scope.

### Interactions (Part XIII)
- **Karma (VI)** produces (`ask`) and consumes (queue-reading rules) decisions; `notify` is an Effect kind. **Promise (V)/Transfer (VIII)/Senses (X)/Imagination (XII)** are the four deterministic sources. **Protein (VII)**: the queue sand is `source: decision, live: true`; `decide` is an Action. **Trust (XI)**: stranger proposals below trust thresholds never interrupt — they park. **Fiote (XIV)** may answer decisions only at `act-within-budget` autonomy.

---

# - [ ] Part XIV — Fiote: the optional operator

**Why.** The human delegates to the AI the switching of knobs they could switch themselves; nothing Fiote does is a different kind of thing. Without an LLM everything works; with one, whisper quality rises and delegated decisions leave the queue on their own. Theory fixed here; stack deliberately unchosen.

- [ ] Autonomy ladder, set per scope (concept subtree / record set / source): `observe → suggest → draft → act-within-budget`.
- [ ] Reads only through Protein against data marked allowed (visibility subject `@fiote`); writes only through Actions.
- [ ] Every write lands with `cause=fiote`, signed by the user's key with an agent marker — delegation visible, inspectable, reversible (compensation facts).
- [ ] Budgets: max actions/day, max promise value, forbidden Action kinds (e.g. never `settle-all-local`) — enforced by the engine, not by the prompt.
- [ ] Narration: optional per-source rewriting of templated whispers; Ask/Agent/Tinkerer modes from the institute notes map to ladder rungs (see `notes/institute/Karma Recommendation, Agentic and Tinkerer.md`).

### Interactions (Part XIV)
- **Protein/Actions (VII)** is the entire API — Fiote has no privileged path. **Attention (XIII)**: suggestions and drafts enter the queue like everything else. **Trust (XI)** marks agency. **Karma (VI)**: Fiote's main output is *drafted rules* — automation the user reads before it lives.

---

# - [ ] Part XV — Organs, visibility, and sync

**Why.** Cells connect into Organs; visibility is data; replication is fact-shipping. One visibility system and one sync system for everything, because everything is a record and every change is a fact.

## - [ ] XV.1 Visibility

```sql
CREATE TABLE visibility_rule (
    uid TEXT PRIMARY KEY,
    subject_kind TEXT NOT NULL,      -- organ | actor | role | public | fiote
    subject_uid  TEXT,
    target_uid   TEXT NOT NULL,      -- ANY record (rule, transfer, plain, concept...)
    field        TEXT,               -- NULL = whole row; else 'head'|'quantity'|'place'|...
    grant        TEXT NOT NULL       -- visible | hidden
);
```

- [ ] Default hidden; most-specific rule wins (actor > role > organ > public).
- [ ] Enforced in exactly one place: Protein evaluation (VII) — package export, sandbox host, and remote reads all pass through it.
- [ ] Karma `set_visibility` consequence makes publish/retract automatable ("make transport Need visible to @neighborhood when quantity < 0").

## - [x] XV.2 Sync

- [ ] **Facts replicate**: per-organ policy (which records, which direction); outbox with retry; import via `append` (idempotent by uid; deltas commute — conflict-free for quantities by construction).
- [ ] **Text replicates via the CRDT relay** (head/body), unchanged in spirit from the current design; the one record-editor sand owns editing everywhere.
- [ ] **Rows replicate by uid** (records, promises, links, concepts); slugs travel as suggestions, never identity.
- [ ] Organ contacts, introduction, polling, and `unknown/known/blocked` trust states carry over from the current networking design; blocked rejects everything everywhere.
- [ ] Proximity is a per-organ numeric property (Senses ceilings and restricted visibility use it); offer-ordering sends to closer organs first without exposing local proximity.

### Interactions (Part XV)
- **Memory (II)** is the transport unit. **Trust (XI)**: origin signatures survive relay. **Senses (X)** reads the discovery cache this layer maintains. **Transfer (VIII)** packages = filtered projections of records+promises+parties under visibility. **Protein (VII)** is the only gate. **Everything-is-a-record (I)** is what makes one visibility table govern rules, transfers, sands, and plain records alike.

---

# - [ ] Part XVI — The Window: workflow acceptance tests

**Why.** The Window is the triage discipline: hold every workflow against the primitives; place each part on the altitude ladder; only what the deduction forces enters the core. Held against twenty-one workflows, the triage forced exactly four core additions — place Instinct, ephemeral lanes, messages-attach-to-anything, embed-honestly — and nothing else. Each case below is an acceptance test: check it when the workflow runs end-to-end on the new core.

- [ ] **1. Todo / knowledge base** — records+links; Karma daily counters; todo/kanban sands. *Accept:* create task, habit re-arms daily, done posts a fact with cause.
- [x] **1b. Focus queue (ordered doing)** — order is links, never staggered frequencies. `@before` chains task records (recurring keep position across days; one-shots link in or fall to tail). Arrival=Karma+Frequency, sequence=`@before` graph, urgency=promise windows. One Protein: `where quantity<0, order: topo(@precedes), then window, then oldest`. Focus = head; next ones dimmed. Pinned corner sand is pure rendering; completing posts a fact and the stream recomputes. — **Done and shipped as the first sand on the new host.** `protein::focus_queue()`; the pinned corner sand (`membrane/assets/focus.html`) speaks only Protein+Actions over the WebSocket; the full path (subscribe → snapshot → set-quantity Action → live Update promoting the next task) is proven by a real WebSocket client test (`membrane/tests/pilot.rs`). Remaining polish: window tie-break in the sort (needs promises in queue math), drag-reorder (`relink-order` Action exists; UI pending), deadline-jump config.
- [ ] **2. Recurring tasks** — Karma+Frequency alone. *Accept:* monthly rule fires exactly once, catch-up works.
- [ ] **3. Donation & buying** — open promises + Senses + Transfer + Trust; storefront sands; delivery = promise window + place route. *Accept:* the DONATION and SALE bundles (VIII.4) run against a second Cell.
- [ ] **4. Transport A→B** — `route()` in core (IX); Senses matches by route×window overlap; ride sand shows both parties one proposal. *Accept:* the RIDE bundle drafts automatically from two Cells' open promises.
- [ ] **5. Group coordination** — Organs; assignment = promise with party=assignee; kanban/gantt sands. *Accept:* assigning creates a promise; completing settles it; the standup view fills itself.
- [ ] **6. Chat & calls** — messages attach to any record; Protein live streams; presence/typing on ephemeral lanes; AV embedded (Jitsi pattern) in a sand; core: contact list, call-invite Action, Karma-triggered calls. *Accept:* "call the parties when the Transfer reaches agreed" works as a rule.
- [ ] **7. Real-time collab docs** — CRDT relay + the one record-editor sand; cursors ephemeral; settings fds. *Accept:* two Cells edit one body; both see cursors; Ledger shows only text_edit annotations.
- [ ] **8. Social network** — zero new core: posts=public-visibility records+media refs; follows=contacts; feed=one Protein across organs; profile=published Collection on the Playground sandbox host. *Accept:* federated feed renders from two organs with visibility respected.
- [ ] **9. Command flows (n8n)** — Karma 2.0 is the engine; Orchestra sand renders the DepGraph. *Accept:* signal→rule→effect chain builds visually and runs.
- [ ] **10. Code editor** — skipped by doctrine (terminal sand + embed).
- [ ] **11. CRM / people** — @person records, relationship links, birthday frequencies, fds until promotion, Protein aggregations for interaction metrics; sands carry the UX. *Accept:* birthday whisper fires; interaction report is one aggregate Protein.
- [/] **12. Personal finance** — currency units, facts with causes, rule-emitted bill promises, transfer income promises, Imagination runway. *Accept:* "rent leaves you short on the 5th unless X settles" appears as a projected crossing. — the projected-crossing engine is done and tested (`imagination_projects_the_scrubbable_future`); the finance sand awaits transport.
- [ ] **13. Inventory & production** — units+places; `@needs` links as BOM; promise chains as production runs; transfer chains to customers; Imagination scheduling. *Accept:* `derive_needs(@cake, 20)` explodes the shopping list; the PRODUCTION chain relays settlement.
- [ ] **14. World statistics** — Protein aggregation across consenting organs + Imagination trends; optimization engine explicitly far-future; L4 sands render. *Accept:* need-mountains aggregate renders from N organs without leaking hidden rows.
- [ ] **15. AI conversation** — pure sand: Protein + Fiote + any LLM. *Accept:* runs with zero core changes.
- [ ] **16. Calendar & time budgeting** — time-cost records, Frequency, Imagination timeline; calendar sand. *Accept:* projected week renders; moving a promise recomputes it.
- [ ] **17. Health & IoT** — devices as signal-records; rules; blob rewards. *Accept:* scale posts weight facts; streak rule fires; source off-switch stops it.
- [ ] **18. Games** — fds state (chess), embedded engines (Freedoom), THE Game reading records with Karma as rulebook. *Accept:* chess still works on the new primitives.
- [ ] **19. Education** — classes=Organs, sprints=promise bundles, curricula=trails; cohort progress = visible facts. *Accept:* an imported trail shows per-student progression (see the two Trail notes).
- [ ] **20. Garden & farm** — plant records with places, watering rules, death-chance signals; scales into case 13.
- [ ] **21. Recaps (TMIL)** — monthly rule queries the Ledger and publishes a record bundle. *Accept:* "this month in this Cell" generates itself.

**The Window's standing law:** apps are projections of one organism. Finance = inventory = pantry (units+facts+promises+Imagination); chat = comments = negotiation (messages on a shared object); profiles = catalogs = libraries (published records behind visibility). When a new workflow arrives, triage it here; if it doesn't decompose, the missing piece is named by what resists — that is how the next abstraction gets deduced instead of appended.

---

# - [ ] Part XVII — The build order

Greenfield: no staged migration, no dual paths, no old API. Old data ported by hand at the end. Dependency order; each stage is usable alone — **live in each stage with real daily data before building the next** (the dogfood replaces the compatibility safety net).

- [ ] **Stage 1 — Core schema + Spine** (Parts 0, I, II, III, IV, V schemas): record/concept/link/fact/promise; `append()`; quantity cache; checkpoints. *Usable as:* a ledgered todo/inventory. — *Done except compaction* (crates `nucleus`/`store`/`engine`; full schema migrated; append + cache + idempotency + hash chain + checkpoints + concepts/links repos, all tested).
- [ ] **Stage 2 — Karma 2.0** (VI): pipeline, DepGraph, reactive delivery, Proof warnings, Signals/Effects. *Usable as:* habits + automation with provenance. — *Done except*: debounce, and the run_query/run_action/set_visibility/advance_transfer consequence kinds (they belong to Stages 3–4 anyway). Pipeline, derived values, transitive dep graph, tick, signal sampler with cascade, heartbeat/daemon, Proof loops, command/notify/ask/emit_promise consequences — all tested (42 tests).
- [x] **Stage 3 — Protein + Actions + transport** (VII) + place functions (IX): includes, topo order, live subscriptions, ephemeral lanes, Action catalog. *Usable as:* the board rebuilt on one contract; the focus queue ships here. — **Done.** The `protein` crate (record/promise/decision sources, predicate tree with Lingua DAG, facts/promises/links/availability includes, topo order, aggregates, saved Proteins, the single visibility gate `execute_for`, the place `near` predicate, JSON wire, canned queues), the full Action catalog with provenance, and the `transport` crate (transport-agnostic `Session` state machine: multiplexed subscriptions + Actions + live `fact_bus` updates + ephemeral lanes, plus the axum-feature WebSocket driver) — all tested. **The transport is the sand boundary: everything through here is backend and does not touch the existing web/sand UI. Sand porting is the next step and needs product decisions.**
- [x] **Stage 4 — Transfer** (VIII): bundles, agreement policies (individual/full/percentage/dependency), settlement as the only Record mutation (idempotent), agreement invalidation on edit, chains/spectators via conditional promises, first_completes satiation, Karma `advance_transfer`. *Usable as:* two-Cell donations and sales — tested.
- [x] **Stage 5 — Imagination** (XII): `project()` folds promises + rules forward (deterministic), threshold crossings, confidence from verified kept-ratios, `confidence()`/`projected()` Karma tokens. The timeline *sand* awaits transport; the engine is done and tested.
- [/] **Stage 6 — Lingua publishing + Senses** (III.2, X): concept packages, adoption, matcher with proximity ceilings. — *Lingua repo + Senses matcher done and tested* (`engine::senses`: complementary open-promise matching, hard proximity ceiling, Lingua-DAG concept alignment so a specific offer meets a general Need, confidence floor, ranked drafts). Remaining: concept *package* publish/adopt flow, and the live discovery-cache feed (the matcher takes the cache as input today).
- [x] **Stage 7 — Trust** (XI): ed25519 keys (private key outside the db), every fact signed on the write path, `verify_fact` on import, authorship preserved across sync, the two-layer tamper model (chain guards content→hash, signature guards hash→author). Verifiable aggregates via Protein. Tested. (Leaderboard sands: deferred by design.)
- [ ] **Stage 8 — Attention** (XIII): decision-records, notify effect, budgets, capture sources. — *Decision-records + notify effect + ask consequence + Decide action done and tested*; the budget/digest config and capture-source registry await the transport/UI.
- [/] **Stage 8b — The web/sand finalization** (VII.4): port the **whole** sand system to Protein + Actions on the `membrane` host. No compatibility — the old SSE-view/table-CRUD data path is deleted. Preserve every frontend-only board feature (canvas pan/zoom, workspaces, move/resize/pin/z-index/grouping, edit mode, sand import `.html`/`.lince`, publish, the widget bridge re-pointed to Protein/Actions, ABI events on ephemeral lanes). — *Started*: `membrane` host + the focus-queue corner sand ported and proven end-to-end. Remaining: the board chrome (canvas/workspaces/edit-mode/import/publish) ported onto membrane, and the existing sands (table, kanban, relations, karma orchestra, transfer, trail, home manager) re-authored to the new mode. **This refounding is the source of truth; the old `web`/Tauri crate is retired at cutover with data hand-migrated.**
- [ ] **Stage 9 — Fiote** (XIV): autonomy ladder over existing knobs. — Unstarted (Fiote writes only through the Action catalog, which now exists).
- [ ] **Stage 10 — World + Synchrony**: the map, THE Game, multi-Cell choreography. — *Sync package layer done and tested* (visibility-filtered export, idempotent authored import, deltas commute); the map/game/choreography are UI-and-beyond.

**Risks, with answers:** Ledger growth → checkpoints + compaction (II.2). Lingua politics → forks with lineage + equivalences; convergence is social. Capture consent → every source a visible record with an off switch; local-first non-negotiable. Whisper fatigue → hard user-owned budget; Lince has no metric that benefits from interrupting anyone. Greenfield discipline → each stage dogfooded before the next; years of append-only features without holistic passes is how the old cathedral grew.

---

## The north star (kept from the theory)

Ana wakes. No dashboard. The kitchen scale posts a fact; beans cross their threshold; a promise to the roaster activates under a rule she approved months ago; two Cells settle Saturday pickup. One whisper on the walk to work — a nod, two promises change state. Work is an Organ; the standup is a view nobody fills in. A second whisper near the market — her mother's pantry Need, published to family only, met on the way home. In the evening she scrubs the timeline out of curiosity: rent fine, a bar's event Organ bit on her guitar Need, the tomatoes surplus in nine days and the donation rule is staged. The Lincegoshi grows fat and luminous and dissipates. Under four minutes of managing life, all of it decisions only a human could make.

That is the Death of Lince: management time asymptotically approaching the irreducible minimum — the moments of actual human choice. More needs met, more transactions peer-to-peer, more donations, more efficiency: the dance of the world, made executable.

**Everything is a Record. Every change is a Fact. Every intended change is a Promise. The rest is choreography — and Protein is how the dance is seen.**
