# New-version capabilities and maneirisms

State of the Cell backend after the 2026-07-11 finalization pass. This is the
practical manual: what each pillar can do **today**, how you invoke it, its
maneirisms (the little behaviors you must know to not fight it), and what a
sand could be built on top of it right now. The canonical blueprint stays
`docs/fable-improvement.md`; this file is the "so what can I actually do"
companion.

**Where we stand:** the legacy layer (`persistence`, `lince-legacy.db`, old
karma/views/tui/gui, the FullUi legacy server) is deleted. Everything runs on
`nucleus → store → engine → protein → transport`, 128 backend tests green.
Blueprint Parts 0–XIII and XV are implemented and tested; Fiote (XIV) is
deferred by decision. What remains is surface work (sands) plus three named
backend gaps: OSM routing (`route_eta`, polygon `within`), the CRDT text relay
for collaborative head/body editing, and the periodic polling task that drives
the organ HTTP boundary automatically (the boundary itself is live).

---

## 1. How a sand talks to the Cell

One WebSocket at `/host/transport/ws`, multiplexed. A sand never sees SQL or
tables — only Protein (reads) and Actions (writes), plus ephemeral lanes for
presence/events.

```js
// through the board bridge (widget host)
H.subscribeProtein("my-sub", { source: "record", where: [{ quantity_lt: 0 }] },
                   rows => render(rows));
H.act({ action: "set-quantity", target: "apples.stock", value: 3 });
H.joinRoom("abi:recordClicked"); H.onLane(...); H.emit("recordClicked", {...});
```

Wire maneirisms:
- Actions are JSON with a kebab-case `"action"` tag; every other field is
  snake_case. Protein predicates/includes are snake_case too.
- A subscription answers with a snapshot, then re-executes and pushes rows on
  every relevant commit (`live` invalidation is coarse-by-source in v1: your
  sand may get refreshes it doesn't strictly need — render idempotently).
- Action responses carry `created` (uid of what was made), `facts` (what the
  Ledger committed, including any Karma cascade), and `warnings` (non-fatal
  advisories like link-cycle warnings and rule Proof loops). Show warnings;
  never treat them as errors.
- Ephemeral lane traffic (cursors, clicks, presence) is **never** persisted.

## 2. Records and the Ledger (the ground truth)

Everything is a record: tasks, rules, signals, transfers, decisions, organs,
people, saved Proteins, threads, messages. Every change is a fact with a hash
chain and (when a signer is set) an ed25519 signature.

What you can do:
- `create-record`, `set-quantity`, `add-quantity`, `edit-record-text`,
  `set-slug`, `set-concept`, `set-unit`, `set-place`, `set-extension`
  (namespaced fds JSON), `activate`/`deactivate`.
- **Undo** = `compensate { fact }`: appends the inverse delta with
  `cause=compensation`. There is no destructive undo.
- Provenance for free: any sand adds `include: { facts: { limit: N } }` and
  gets the "why did this change" drawer — delta, at, cause_kind, cause, actor.
- Checkpoints (`Engine::checkpoint_all`) snapshot levels; compaction folds
  pre-checkpoint history into a cold JSONL archive whose SHA-256 is anchored
  back into the Ledger. Retention horizons are per record-kind
  (`retention_policy`); no policy = keep forever.

Maneirisms:
- `quantity` is a **cache**; the fact is the truth. Negative = Need, positive
  = Contribution, zero = peace. Activation of rules/transfers/signals/sands is
  the same knob (quantity 0 = off).
- **Delete does not exist.** "Delete" is `deactivate` (quantity → 0). Sands
  should filter `quantity_gt: 0` or by state, not expect rows to vanish.
- Metadata edits (slug/concept/unit/extension/text) drop **zero-delta
  annotation facts** so live subscriptions refresh — expect facts with
  `delta: 0` and a JSON payload describing the edit.
- Appending a fact whose uid already exists is a silent no-op (replay safety).
- Slugs are optional local conveniences (`dot.case`); uids are identity.
  Actions accept either.

## 3. Lingua (concepts)

- `create-concept`, `adopt-concepts` (foreign concepts keep their uid and
  lineage; re-adoption is a no-op), `declare-equivalence` (cross-dialect
  same-ness, e.g. `apple ≡ maçã-fuji`).
- The parent DAG powers widening: `concept_in: "food"` matches `@apple`
  through `apple → fruit → food` in every Protein source that supports it.
- Unit conversion: `concept_conversion` rows, one authoritative row per
  unordered pair, reverse derived as `1/factor`, only within a shared ancestor
  dimension.
- Dialect fallback: `nearest_ancestor_in` — an unknown `@blocks-softly` is
  treated as its nearest known ancestor `@blocks`.
- Concepts travel automatically inside sync packages (see §10) — a record
  arrives already understandable.

## 4. Links (the graph)

- `add-link`, `remove-link`, `relink-order` (drag-reorder sugar). Identity is
  the triple (from, kind, to) — the same two records carry many link kinds.
- Adding a link of an **order-like** kind (`precedes`, `before`, `order`, or
  their descendants) that closes a loop **succeeds but warns**
  (`outcome.warnings`: "these N records form a loop: a -> b -> a"). Non-order
  kinds (like `needs`) never warn — mutual recipes are legal.
- Protein: `include: { links: { kinds: [...], direction, depth } }`. `depth: 2`
  BFS-expands the tree and stamps each link with its `hop`. Ordering:
  `order: [{ topo: "before" }, ...]` — the focus queue.
- Tags/clusters are links: `linked_to: { kind: "tag", to: "tasks" }` composes
  with `any`/`not`/`all` for include+exclude filtering.

## 5. Promises (the social atom)

- `create-promise` (including OPEN promises — published Needs/Contributions
  with an unfilled party slot), `promise-transition` (state machine validated:
  open → proposed → agreed → active → kept/broken/withdrawn),
  `edit-promise-delta` (a counteroffer — it resets every party's agreement).
- **Expiry is automatic**: each heartbeat, past-window promises move
  `agreed/active → broken` (a commitment was not kept — this also enqueues an
  `expiry` decision) or `open/proposed → withdrawn` (a lapsed offer, quietly).
  Sands never need their own deadline logic.
- Reservation: `reserve_from` per promise — explicit value, else the bundle
  transfer's `reserve_default`, else `active`. Protein's
  `include: { availability: true }` returns `available` and `planned`.

## 6. Karma (automation)

A rule is a record (sidecar: condition, gate, carry, debounce) plus consequences.
Full math conditions with these tokens, all live:

```
@x  /  quantity(@x)         sum(@x, 30d)   sum_pos(@x, 30d)   sum_neg(@x, 30d)
freq(@freq.daily-7am)       signal(@fridge-cam)       value(@rules.burn-rate)
promise_state(@p_...)       confidence(@p_...)        projected(@x, 7d)
hours_since_fact(@x)        distance(@a, @b)          demand(@food)
```
(`route_eta` parses but errors cleanly until OSM data lands.)

Consequences: `set_quantity`, `add_quantity`, `emit_promise`, `run_command`,
`run_query` (executes a saved Protein, logs the row count), `run_action`
(re-enters `Engine::act` with a typed Action from the effect queue),
`set_visibility`, `advance_transfer` (within policy, never past it),
`activate`/`deactivate` (works on rules — quiet hours is just a rule turning
another rule off), `ask` (enqueue a decision), `notify` (budgeted, see §9).

How you drive it from a sand:
- `create-rule` / `update-rule` — both reload the registry and return **Proof
  warnings** in `outcome.warnings` when your new rule closes a loop
  ("these 2 rules form a loop"). Save still succeeds; show the warning.
- `create-signal` (command/http/sensor/query source on a schedule; samples land
  as facts and cascade like any change), `create-frequency` (the time
  primitive; day-of-week and catch-up supported).

Maneirisms:
- Delivery is reactive: only rules reading a changed record re-evaluate, then
  their writes cascade (capped at 256 per delivery — a runaway loop survives).
- `debounce: "1h"` holds a rule for an hour after it fires even if inputs keep
  changing. In-memory: the hold resets on rule reload.
- A rule with **zero consequences is a named derived value** — read it with
  `value(@rules.monthly-burn)` like a spreadsheet cell.
- Effects (`run_command`/`run_action`/`run_query`/`notify`) run OUTSIDE
  evaluation from a durable queue, and each logs a zero-delta provenance fact
  on the rule record.
- Worked examples that are real tests: daily habit, reorder ask, quiet hours,
  trust-ahead (`confidence(@p) > 0.9` → `advance_transfer`).

## 7. Protein (the read contract)

Six sources, one JSON shape:

| source | what you get | notes |
|---|---|---|
| `record` | the state vector | all predicates + all includes |
| `promise` | promises (state-filterable) | `state_in` |
| `decision` | the open Decision Queue | never exported to remote subjects |
| `fact` | the Ledger itself | `at_since` ("30d" or RFC3339), `cause_kind_eq`, `record_eq`, `concept_in`; the finance workhorse |
| `concept` | the Lingua vocabulary | names, instincts, parents |
| `transfer` | bundles **with derived status** | plus parties, promises, balance |

Predicates: `all/any/not`, `quantity_lt/lte/gt/gte/eq`, `uid_eq`, `kind_eq`,
`slug_eq`, `concept_in` (DAG-aware), `linked_to`, `state_in`, `near` (place
Instinct), and the fact-source trio above.

Includes on record rows: `facts` (provenance), `promises`, `links` (kinds,
direction, `depth` tree with `hop`), `threads` (nested messages),
`extension` (one fds namespace), `availability` (`available`/`planned`),
`projection` (`{ at: "+7d" }` → `projected` = quantity + agreed/active promise
deltas closing by then — the promise fold; full rule simulation is
`Engine::project`, engine-side).

Aggregation: `{ op: sum|count, by: concept|kind }` on records,
`by: cause_kind|day|concept` on facts. The visibility gate applies **before**
aggregation — hidden rows can't leak through sums.

Saved Proteins are records (`kind='protein'`, AST in the `lince.protein`
extension) written by `save-protein`; sands reference them by slug — the old
"view" concept, done right.

Maneirisms:
- The wire `where` is a JSON array = implicit `all`.
- Fact-source predicates don't nest (flat list) in v1.
- `at_since: "30d"` resolves against wall-clock now (fine for dashboards; use
  absolute RFC3339 for reproducible reads).
- Remote subjects see only whole-row visibility grants; the Decision Queue and
  concept-level promises never leave the Cell through Protein.

## 8. Transfers (promise bundles under agreement)

The full ladder is derived, never stored — a `source: transfer` row carries
`status`: `inactive → draft → proposed → agreed → in_transfer → settled`.

Flow a sand can drive today, all typed Actions:
1. `create-transfer` (agreement `individual|full|percentage|dependency`,
   `satiation`, `reserve_default`, `require_confirmation`).
2. `add-party`, `add-promise-to-transfer` (a condition string makes it a chain
   link or spectator — same grammar as Karma conditions).
3. `agree-transfer { level: 2 }` — level 2 also advances that party's bundled
   promises to agreed. Editing any bundled promise drops everyone back to 0.
4. `activate-transfer` (within policy), then `settle-transfer` — the ONLY
   thing that mutates record quantities, idempotently, with
   `cause=settlement`. Chains fire, satiation withdraws sibling bundles.
5. If `require_confirmation` was set: settlement refuses until both
   `confirm-transfer { confirmation: "delivery" }` and `"receipt"` annotation
   facts are in.
- Advisory balance on every transfer row: `balance` (per-concept promise sums)
  and `balanced` — a trade sums to zero per concept, a donation deliberately
  does not. Advisory only; never blocks.
- Transfer chat = `create-thread` on the transfer record + `create-message`;
  read back with `include: { threads }`. No transfer-specific message model.

## 9. Attention (the Decision Queue — "what should I do next")

`source: decision, live: true` is the queue; `decide` is the answer. Four
deterministic feeders run **inside the heartbeat**, no UI required:

1. **Broken promises** → `kind: "expiry"` decisions.
2. **Karma `ask`** consequences → `kind: "ask"`.
3. **Senses matches** (§11) → `kind: "draft"` — "your promise X meets Y from
   organ Z (score 0.87)", options propose/dismiss.
4. **Projected crossings** → `kind: "crossing"` — "apples.stock hits 0 on
   Tuesday 2026-07-14 (promise)", a week's horizon, per-record deduped.

Maneirisms:
- Answering: `decide { decision, answer }` closes it through the Ledger
  (quantity 1 → 0, so Karma can react to answered decisions). If the chosen
  option carries an `action` field (any typed Action), **decide executes it**
  — one-tap flows like "yes → set-quantity".
- Decisions with `expires_at` are auto-closed as `"expired"` by the heartbeat.
- Every sweep dedups by `(subject, kind)` — one situation asks exactly once
  while its decision stays open.
- **Notify budget**: `configuration.attention_budget_per_day` (default 12) is
  hard. Over-budget notify effects complete with result `parked:digest ...`
  instead of being delivered — a digest sand reads the parked rows; nothing is
  lost, nothing interrupts.

## 10. Sync and Organs (multi-Cell)

Implemented and tested end-to-end (two engines, in-memory wire = the same code
path the HTTP boundary calls):

- **Introduction**: `GET /organ/introduction` returns who I am + my public
  keys; `Engine::adopt_introduction` registers the contact **under the remote
  organ's own uid** (identity replicates by uid) and stores its keys so its
  signed facts verify.
- **Contacts**: `organ_contact` — trust `unknown|known|blocked`, numeric
  `proximity`, per-organ `sync_out`/`sync_in` policy. **Blocked rejects
  everything everywhere** (imports AND discovery), tested.
- **Push**: `enqueue_sync_to(organ)` builds a visibility-gated package (the
  same single gate Protein uses) and queues it in `sync_outbox`;
  `drain_outbox(sender)` sends with retry (failures stay queued). The wire
  endpoint is `POST /organ/inbox`.
- **Import hardening**: every incoming fact must pass its chain step (content
  → hash) and its signature (hash → author); rejected rows land verbatim in
  `sync_quarantine` with a reason, the rest of the package still applies.
  Import is idempotent by fact uid; deltas commute — quantity sync is
  conflict-free by construction.
- **Concepts ride along**: a package carries every concept (uid + name +
  ancestors) its records speak; import adopts them with lineage before the
  records land — a stranger's data arrives understandable.
- **Discovery**: `GET /organ/open-promises` (subject in the `X-Lince-Organ`
  header) exports the OPEN promises that subject may see;
  `refresh_discovery` upserts them into the local cache, stamping proximity
  from OUR contact row (your proximity never travels outward).

**The one unwired piece**: nothing calls the boundary on a timer yet. A small
web-side task (or even a Karma `run_command` with curl, today) needs to
periodically drain the outbox to each contact's `base_url` and pull
open-promises into the cache. Everything on both sides of that call exists.

## 11. Senses + Imagination (the recommendation engine)

- A **match rule is a record**: `create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }` — activate/deactivate like any rule.
  `max_proximity` is a HARD ceiling; matching never auto-expands.
- Each heartbeat, `senses_pass` joins your OPEN promises against the discovery
  cache: sign-opposite deltas, Lingua-aligned concepts (same uid, or through
  the DAG), window overlap, confidence floor — ranked drafts straight into the
  Decision Queue.
- `Engine::project(now, until)` folds promises + rules forward on a virtual
  clock (signals frozen); `Engine::snapshot(now)` hands you the mutable input
  — toggle a rule, clear a promise, re-fold, diff the two timelines: the
  scrubbable/branching future is an engine call, the sand only renders.
- Deterministic numbers, no ML: `confidence(@p)` = the party's
  Laplace-smoothed kept-ratio; `demand(@concept)` = the current hour's share
  of the trailing-30d activity on that concept.

## 12. Trust

Every locally-authored fact is signed on the write path; imported facts keep
their **origin** signature so downstream Cells can still verify the original
author. Two-layer tamper model: the hash chain guards content, the signature
guards authorship. Compaction archives stay verifiable file-side; the anchor
fact makes the file tamper-evident from inside the Ledger. No reputation
scores, ever — kept/broken history is queryable raw material.

---

## 13. So: what sands could we build right now?

Everything below needs **zero backend work** — each maps to Proteins/Actions
that exist and are tested:

- **The Inbox ("what should I do / how can I contribute")** — the headline.
  `source: decision, live: true`; group by `kind` (draft/crossing/expiry/ask);
  answer with `decide`, with one-tap options executing Actions. This is the
  Attention pillar made visible, and it fills itself from four engine sweeps.
- **Finance / statistics dashboard** — `source: fact` with
  `aggregate: { sum, by: cause_kind }` for monthly flows, `by: day` for the
  spark-line, `concept_in: "money"` scoping; drill-down via `record_eq` for
  the provenance list. The old finance views are one Protein each now.
- **Transfer desk / marketplace panel** — `source: transfer` renders the whole
  lifecycle from the derived `status`; buttons are the §8 Actions; the balance
  advisory badges trades vs donations; the chat tab is the threads include.
- **Pantry / inventory with a future** — records with `availability` and
  `projection` includes: "8 now, 5 available (3 reserved), 2 by Friday"; the
  crossing decisions surface "you run out Thursday" without the sand doing
  math.
- **Timeline (the scrubbable future)** — render `Engine::project` output
  points; branching UI = mutate the snapshot and re-fold; diff view = compare
  two timelines. Engine-side only; needs a small transport verb to expose
  `project` (the one genuinely new endpoint a timeline sand would want).
- **Karma Orchestra v2** — rule CRUD through `create-rule`/`update-rule` with
  live Proof-loop warnings surfaced on save; derived values readable via
  `value()`; signals/frequencies creatable; the DepGraph render feeds off the
  rule records.
- **Neighborhood matching** — a settings card for `create-match-rule` +
  visibility grants ("publish this Need to @neighborhood"), with results
  arriving in the Inbox as drafts. Once the polling task is wired, this is the
  full stranger's-offer-meets-your-need loop.
- **Organ contacts manager** — list contacts with trust/proximity/sync policy,
  introduce via URL (`GET /organ/introduction` → `adopt_introduction`), block
  button, quarantine viewer (every rejected row with its reason).
- **Cluster/tag boards** — kanban/table already do this: `linked_to` tags with
  include/exclude, columns by any field, all writes typed.

**Not buildable yet (backend gaps, by decision):** collaborative text editing
(CRDT relay), route/ride matching (`route_eta`, OSM), automatic background
organ polling (endpoints exist, scheduler doesn't), Fiote conversation sands
(deferred), per-sand capability/permission enforcement on Actions (any sand
can currently call any Action — fine for official sands, gate before running
imported ones freely).

## 14. Cross-cutting maneirisms cheat-sheet

- Everything is a record; activation is quantity; delete is deactivate.
- The fact is the truth, quantity is the cache; undo is compensation.
- Metadata/state changes announce themselves as zero-delta annotation facts.
- Warnings are advice (cycles, Proof loops), never rejections.
- Heartbeat order: promise expiry → decision expiry → timers → signal sampling
  → effects (budgeted notify) → senses pass → crossings pass.
- One situation, one open decision (dedup by subject+kind).
- Uids are identity everywhere, across Cells; slugs are local sugar and get
  dropped on collision at import.
- Visibility is default-hidden, whole-row, enforced in exactly one place —
  and applied before aggregation.
- Blocked organs are rejected at every door (import, discovery, outbox).
