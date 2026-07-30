# Sand: Karma

The Karma sand is a **sand**. It owns no primitive.

It is the human surface over *rules*: create one, read it, revise it, retire it,
and see the Record it acts on drawn through its past, its present, and its
declared future. A rule is a trigger, a condition, an arithmetic, and a
consequence. That is the whole subject of this document.

Everything below is a description of one surface built out of the general
machinery: Records, Facts, exact `DecimalValue` deltas, concepts, promises, and
the domain-neutral schedule kernel. Nothing here is implemented in `nucleus`,
`store`, `engine`, or `protein` as a Karma-sand-shaped thing, and nothing here
may become one. If a paragraph seems to ask for a surface-named type in a
backend crate, that paragraph is wrong and the general primitive is the answer.

## Economy is not a sand

It was, briefly, and the name is retired. **Economy is a way of using this
surface**: the Karma sand with rules about a balance in it, exactly as a pantry
is this surface with rules about flour, and a training log is this surface with
rules about repetitions. The difference between them is which Records exist and
what the concepts are called — data in someone's Cell, not a feature.

The rule, stated once so it cannot be lost: **no domain-specific code in the
backend or the database.** A gain is a positive delta; a cost is a negative one.
A category is a concept. A recurring cost is a `Cadence` plus an amount. A budget
is a query. There is no money type anywhere in the kernel and there must not be
one: a currency is a unit like any other, `Quantity { amount, unit }` says it,
and "my token is worth five of theirs" is a rule someone writes.

This document was extracted from `docs/Central: Karma.md` on 2026-07-26, where
it had accumulated as sections E0–E3, K11, and a per-crate module table.

**Corrected the same day, in the other direction.** The first extraction took
too much: E0.0–E1 came along with it, and those describe the Ledger's
exactness, the engine's read and write halves, exact arithmetic, virtual-clock
projection, classification and the schedule primitive — the engine loop itself,
which this surface merely called first. Roughly 965 lines went back to Karma.
A per-crate table assigning `economy/` modules to `nucleus`, `store`, `engine`
and `protein` was deleted outright rather than moved, because it described the
arrangement this document exists to forbid.

**Renamed 2026-07-28.** `crates/web/src/sand/economy/` became
`crates/web/src/sand/karma/`, `sand.economy` became `sand.karma`, and this file
became `docs/Sand: Karma.md`. K12's "Karma Flow Plane" is no longer a second
sand; it is this sand's map view, built after the rule-CRUD surface below.

**What this document is now:** a build guide for one surface. It says what the
sand shows, how the graph is drawn, and which already-finished backend features
it wires into. It specifies no primitive. If you find yourself reading it to
learn how the backend works, the answer is in Karma and this file is failing.

**Enforced by a test, not by good intentions.**
`crates/web/tests/sand_boundary.rs` walks `nucleus`, `store`, `engine`,
`protein`, `transport` and `lince`, strips comments, and fails if `economy`,
`money`, `currency` or `finance` appears in any remaining code — a module, type,
table, column, string literal or serde tag. Prose is exempt on purpose: the
comments explaining why `economy.add` was rejected in favour of the generic
`record.add-quantity` are among the most useful lines in those files, and a
guard that deleted them would be worse than no guard.

The guard passes today, and a companion test proves it can fail rather than
being decoration.

## The honest limit today

A rule on this surface **declares**; it does not fire itself. Authorized intents
are durable (`store::karma::intents`, K5.2) but inert: the effect worker (K5.3)
does not exist, and E0.3's write side has no recorded implementation. So "apply
this occurrence" is a person pressing apply, and the inbox below is the real
mechanism rather than a placeholder for one.

This is stated up front because the surface is built to survive the worker
landing without changing shape. What changes then is the *consequence* a rule is
allowed to carry — none, propose, or apply under a named grant — which is a field
on the binding, not a new kind of rule.

## What is actually built

Shipped and tested as of 2026-07-26, all of it on domain-neutral primitives:

| Layer | What exists | Domain-neutral name |
| --- | --- | --- |
| `nucleus` | The one schedule type | `karma::Cadence`, `CadenceStep`, `CadenceBound`, `InvalidDay` |
| `store` | Classified changes, rules, derived dates | `entries`, `ledger`, `recurrence`, `exact` |
| `engine` | 6 Actions for rules, 4 for changes | `CaptureEntry`, `ReviseEntry`, `VoidEntry`, `CreateRecurrence`, … |
| `protein` | Read sources | `Source::Entry`, `Source::Recurrence`, `Source::Timeline` |
| `web` | The surface | `sand/karma/` |

The sand offers one-line capture, correction and voiding, re-tagging, recurring
rules with an apply/skip inbox, and a concept timeline spanning settled past,
current position, and declared future.

### The cadence, in full

A rule's step is a **sum of components**, applied largest unit first, then
optionally rolled forward onto a chosen weekday:

1. **Calendar** — years and months, resolved against the anchor's own
   day-of-month. A month too short for that day either clamps to its last day
   or is skipped, per the rule's `invalid_day`.
2. **Fixed** — weeks, days, hours, minutes, seconds, milliseconds, added as an
   exact duration.
3. **Landing** — roll forward whole days until the instant falls on one of the
   allowed weekdays.

So `1 month + 1 day + 1 second + 10 milliseconds, then forward to Friday` is a
single expressible rule, and every component is typeable in the sand.

**Where it stops is part of the same rule.** A `bound` of `unbounded`, `count`,
or `until` closes it, and `count: 1` is the whole of "this happens on the 14th,
once" — the rule produces its anchor and retires. That is why there is no
separate one-shot object anywhere below this surface, and no `ends_at` column
beside the cadence: a promise, a dated reminder and a standing order are one
kind of thing with different bounds. The sand offers this as a plain "Repeating"
control, and a rule set to happen once is the only case where an empty step is
accepted.

A count is of occurrences *produced*, not attempts made: a February skipped for
being too short does not spend one of the twelve payments the author asked for.

Three properties the implementation holds and the tests pin:

- **Landing never feeds back into phase.** It is applied per-occurrence to the
  result, never to the anchor of the next one. Otherwise a monthly rule that
  lands on Friday would gain days every month and stop being monthly.
- **Calendar months are multiplied from the anchor, never stepped.** Stepping
  would make 31 January clamp to 28 February and then carry the 28th forever.
- **Landing collapses duplicates, and duplicates are dropped.** Several base
  dates can land on the same Friday; two occurrences sharing an instant would
  share an idempotency key, so the second would silently replay as an
  already-applied change — a date visible but impossible to apply.

**A derivation can be a prefix and says so.** A rule stepping in milliseconds
produces more dates than any window can hold, so `Derived::truncated` is carried
from the kernel through `store::recurrence::Occurrences`, into the
`recurrence` row's `truncated` field and the timeline's `projection_truncated`,
and is rendered as "these are the next dates, not all of them". Only the
declared half can be truncated; the settled half is read from Facts and is
always complete.

### What "applying" is

A rule writes nothing. A date becomes real only when applied, and applying
routes through the ordinary `capture-entry` path, so a rule-applied change is
indistinguishable downstream from a hand-typed one.

Idempotency needs no new table: the occurrence's whole identity is
`<recurrence_uid>:<due_at>` used as the entry's `request_id`, and
`entry_revision.request_id` is already `UNIQUE`. Occurrences are **derived, never
materialized** — there is no occurrence table and no cursor.

---

The remainder is the surface's own specification: the acceptance gate, what the
sand shows, the graph, the wiring into Protein and Actions, the information
architecture, and the proof gates. Where older prose still names an
Economy-shaped backend type, a superseding note maps it onto the general
primitive that actually exists — those notes are the correction, not a
suggestion to build the named thing.

## Gate — the first vertical sand

*Extracted from Karma.md K11.*


**Runs now, immediately after K5.2.** The original text required K0–K10 first;
that requirement is withdrawn. What Economy actually depends on is the exact
kernel (K1), the durable model and Actions (K2), the occurrence and Frequency
machinery (K2–K3), the evaluator (K4), and the grant boundary (K5.1–K5.2) —
all of which exist and pass. It depends on no Signal, no model, no Trust scope,
no workflow engine, and no simulation harness. E0.4's projector is not that
harness: folding the loop forward on a virtual clock is a product capability
built from the production evaluator, while K10 is the adversarial machinery that
generates faults and hunts for invariant violations. The first needs only a
snapshot and a horizon; the second needs everything.

**There is no Economy backend, and the sentence that used to sit here said there
was.** It read: "Implement the Economy reference workflow specified below. Its
gain/loss event types, event drafts, recurrence occurrences, projections,
Actions, and Protein source come before its interface… E0–E1 finish the Economy
backend; only E2 starts the sand." That ordering — backend Economy types first,
interface second — is the arrangement this document exists to forbid, and
following it is how the original domain silo got built.

The correct reading: E0–E1 finished the **engine loop** (exact deltas,
classification, recurrence), all of it generally named and usable by any
surface. Their specification lives in `docs/Central: Karma.md`, not here. E2 is
the sand, and the sand is HTML and JavaScript.

The first frontend delivered on this stack is the **Karma** sand. It replaced an
unwired Finance placeholder, was briefly registered as `sand.economy`, and is now
`sand.karma`/`sand/karma`; compatibility was deliberately not preserved at either
step. `sand.karma` is a feature-flag string of exactly the same shape as
`sand.transfer`, holds no privilege the other sands lack, and appears in no
table. The two renames tell one story: the surface kept getting named after the
first thing people did with it, and what it actually is is the rules plane.

Deferring or advancing a sand must never permit it to invent another decimal,
schedule, capability, or replay contract. If it needs one, the general primitive
gets stronger.

This surface was chosen to **exercise** the finished loop first, because rules
over a balance happen to touch exact values, Records and Facts, units and
concepts, reusable Frequencies, corrections, authority, aggregates and
explanations without requiring an external effect. That is a property of the
choice, not a property of the core — the core gained nothing domain-shaped by
being exercised this way, and any surface with the same reach would have served.
E3 adds Fiote ergonomics later without broadening anything below the sand.

**Exit gate:** through the real socket, a person creates individual gain/loss
events over selected resource Records, corrects/voids them without rewriting
history, defines
recurring gains/losses, resolves their due occurrences, and sees server-made
monthly gains, losses, net flow, tag/source profiles, resource trend, and
recurring projection series with drill-down. The same draft/correction Actions
accept a software-agent principal, but no Fiote/model inference can apply a
Fact without the same review or grant as a human client. Restart, stale edit,
duplicate request, unit separation, visibility, keyboard/screen-reader, and
projection explanation tests pass.


### The surface itself

**Product name and boundary.** The surface is the **Karma** sand: CRUD over the
rules that change Records. Its first and simplest rule shape is the one below —
an individual or recurring change to one Record's quantity, declared or applied.
It was called Finance, then Economy, and both names described a *use* rather than
the surface; economy is now a preset of Records and concepts a person loads into
it, not a product boundary. Records/Ledger own the resource and the actual change; Lingua
owns its concept and unit; `store::recurrence` owns repeating declarations and
their derived dates; Protein computes every view; and the sand sends typed
Actions. Neither the sand nor Fiote gets a private financial database or
arithmetic path.

One correction to the older wording: recurrence is **not** Karma. It is its own
general primitive, deliberately separate from `CalendarSchedule`, because a
declared date needs no timezone provider and no execution semantics — nothing
fires on it. A read path can therefore derive upcoming dates without dragging
the scheduler runtime behind it. If a sand ever needs a rule that genuinely
*executes*, that is Karma, and it goes through the ordinary grant and budget
boundary like any other effect.

Do not add account ledgers, double-entry postings, reconciliation, budgets,
goals, debts, investments, bank imports, exchange-rate portfolios, shared-book
semantics, tax tooling, or other conventional finance-product features to this
plan. If one is requested later, design it then against the pillars rather than
preloading the sand with unused abstractions.

#### Standing invariants for this surface

1. **A gain/loss is one signed resource delta.** The Action carries a positive
   magnitude and direction; the engine canonicalizes `gain` to a positive Fact
   delta and `loss` to a negative Fact delta on exactly one resource Record.
   Arbitrary client-provided signs are rejected.
2. **Values are exact and unit-bearing.** The event uses the resource Record's
   Lingua unit and the K0 fixed decimal quantity representation; incompatible
   resources/units are never silently summed or converted.
3. **Applied history is append-only.** Drafts are revisioned and editable. An
   applied mistake is changed by compensating its Fact and appending the
   replacement; “delete” compensates and voids. Original Facts, authorship, and
   causal order remain visible.
4. **Actual and expected are distinct.** An applied individual event is actual.
   A recurring-plan occurrence is expected until a person or authorized agent
   applies it. A projection renders expected change separately and never writes
   it into the resource quantity.
5. **Visibility gates precede totals.** Hidden events cannot leak through
   monthly totals, tag/source profiles, percentages, graph points, forecasts,
   explanations, or Fiote context.
6. **Source, tags, and capture origin remain separate.** `source` identifies or
   labels where the gain/loss came from; Record links/Lingua concepts classify
   it; `capture_origin` says manual, typed, voice, photo, or agent; cause links
   retain the plan occurrence, capture, Program, and Facts. The UI may group by
   any of them without merging their meanings.

#### What the backend already provides

**This sand builds no primitive.** Everything below is assumed ready, lives in
`docs/Central: Karma.md`, and is named generally because other sands use the
same machinery. If something here is missing, the fix is a stronger general
primitive in the backend — never a private domain path around it. That mistake
has already been made once: an `economy_event` table and a private
`Source::Economy` existed only because generic Fact aggregation summed with
`f64` and could not group by what a change was *for*. Routing around a weak
primitive is how a domain silo starts.

The E0.0–E1 slices that used to be specified here were moved back to Karma on
2026-07-26. They describe the Ledger, the engine's read and write halves, exact
arithmetic, virtual-clock projection, classification and recurrence — the
engine loop itself. This surface was only their first caller.

| The sand needs | Backend feature it uses | Where |
| --- | --- | --- |
| Amounts that never drift | `DecimalValue` mantissa/scale, `aligned_add`, canonical text | `nucleus`, Karma E0.0 |
| A change to a resource | Hash-chained Fact with an exact signed delta | `store::facts`, `store::ledger` |
| Correction and undo | Compensating Fact + `expected_revision` | `store::entries`, Karma E0 |
| "What is this for?" | `fact_concept` assertion + concept DAG descent | `store::ledger`, Karma E0 |
| Repeating dates | `nucleus::karma::Cadence` compound step + landing | Karma E1 |
| Rules and their dates | `store::recurrence`, derived not materialized | Karma E1 |
| Reading any of it | `Source::Entry`, `Source::Recurrence`, `Source::Timeline` | `protein` |
| Writing any of it | The 10 Actions listed under *wiring* below | `engine::actions` |
| Live redraw | `protein_subscribe` invalidation on Fact append | board bridge |

Two consequences worth stating plainly, because they shape the sand's code more
than anything else:

**No math in JavaScript.** Every total, running position and projected point
arrives as exact decimal *text* already computed by Protein. The sand's
`format.js` contains no arithmetic at all. The only numbers it produces are
pixel coordinates for the graph, which are approximate by definition and are
never read back. `-10.10 + -20.20` becoming `-30.299999999999997` is not a
rounding annoyance; it is the ledger ceasing to be exact.

**No durable truth in the client.** State holds filters and draft UI. No event,
no cursor, no total and no projection lives in JavaScript, so two open sands
cannot disagree about what happened.

#### Still open in the backend

Not blocking the sand, but the sand is where their absence shows:

- E1's `suggest` / `draft` / granted `apply` route split. Today every date is
  applied by hand through the same Action an automatic path would call, which
  is the right shape — the automatic caller just does not exist yet.
- Downtime catch-up, per-occurrence overrides as durable objects, and
  `cancelled` as distinct from `skipped`.
- DST-correct local anchoring. `Cadence` works in UTC by design; a rule
  authored across a DST boundary drifts an hour locally.
- E3 Fiote ergonomics, which run after K5.3 and change no part of the model
  below.

##### E2 — first viable rules surface

**Landed 2026-07-26.** The unwired Finance placeholder is deleted and
`sand.karma` is registered as an archive package (`LincePackage::new_archive`,
like Transfers) with `bridge_state` / `protein_subscribe` / `act`. Split into
`sand/karma/{mod,body}.rs` + `styles.css` + `app/{main,state,format,entries,
recurrence,graph}.js`.

Built as **six** JS modules rather than the eight originally listed:
`dashboard`, `profile` and `activity` were not built, because monthly
dashboards, concept/source profiles and an activity feed are not what this
slice was asked for. The bullets below record what shipped and what did not.
- [x] **One timeline, past present and future in a single view.** Built for a
  CONCEPT rather than a resource, which is the sharper reading of the same
  intent: "how is `@rent` going" spans every wallet it was paid from, where one
  resource cannot. `Source::Timeline` shows actual Facts behind it, its current
  quantity, and its projected future ahead — where the past mixes rule-caused and
  hand-entered movements without distinguishing them structurally, and the future
  mixes E0.4 program folds with classified promises. This is the view the pillar
  exists to produce, and it is one query over one classification, not four
  panels stitched together.
- [x] **Capture is one line, and relating it is not the person's job.** "ice
  cream, `@cost`, -10" is the whole interaction: pick a resource, a magnitude
  with sign, a concept, and a date. The sand never asks anyone to add it to a
  month's expenses, to a category total, or to a sum Record — those are E0
  queries over the classification, and they update because the Fact exists.
  A form that requires choosing which total to affect has reintroduced the
  bookkeeping this design removed.
- [x] **Authoring recurring rules lives here**, in the sand's own words: an
  amount, a cadence picked from Monthly/Weekly/Every-N-days/Yearly, and a
  classification. The form shows only the fields the chosen cadence uses, so a
  rule cannot be described two ways at once. **It does not create a Karma
  Program or Frequency** — see E1 for why a declared recurring amount does not
  need the scheduler runtime — so there is no grant to display. Complex or
  multi-node rules still hand off to K12's Flow Plane.
  The rule list pauses and resumes; the "Expected next" inbox applies or skips
  one date, and applying accepts a one-off amount without editing the rule.
- [x] **Human-friendly CRUD with the consequence spelled out.** Forms expose
  resource, exact signed amount, occurred date, classification, and note, with
  **no direction control** — the sign carries it. Edit says it "compensates the
  original change and appends a replacement. Both stay in the Ledger; nothing is
  rewritten." Void says the amount "is retracted from the period it happened in"
  and that a real refund is a positive capture today, which is the distinction
  the backend enforces and the one a person deleting a March expense in July
  most needs. Re-tagging is its own operation and says it appends no Fact.
  Voided entries stay visible: an append-only Ledger has no delete, and hiding
  them would make a correction look like a disappearance.
- [x] **The concept graph.** Pick a concept and a window; the chart draws its
  running cumulative with the settled half solid and the declared half dashed,
  so the two never look like the same claim. Above it: the current position,
  what the concept did before the window, and how much is declared ahead. Below
  it: a table carrying the same points (the chart is `role="img"` and
  decorative — no number exists only as a shape), and a list of every rule and
  promise making up the future. **JS converts exact text to pixel coordinates
  and nothing else**; a coordinate is approximate by definition and is never
  read back into a total. The "declared ahead" figure reports a *count* of dates
  rather than a sum, because summing exact decimals in the client is the one
  thing this design forbids and inventing a total the backend did not compute
  would be worse than not showing one.
- [ ] **Not built:** monthly actual/expected dashboards, concept and source
  profiles, and per-bucket drill-down from a total to its contributing Facts.
  The graph's future half drills to its rules and promises; the settled half
  does not yet drill to Facts.
- [ ] **Not built: the driven browser selftest.** No chromium harness was run
  for this sand, so live invalidation across two open sands, stale-edit
  recovery, and keyboard operation are **unverified in a browser**. The Rust
  side is covered (45 new tests across nucleus/store/engine/protein plus 3
  package tests), and the accessible table alternative exists in the markup, but
  neither substitutes for driving it. This is the largest open gap in E2.

**E2 exit:** not yet reached. The workflow is complete and the backend beneath it
is verified, but the browser selftest above is part of this gate.

###### Review pass 2026-07-26 — four real defects caught before shipping

Recorded because three of them were invisible to a passing test suite, which is
the failure mode worth remembering:

1. **The running total added kilograms to a currency.** `execute_timeline`
   bucketed points per `(bucket, unit)` correctly but accumulated `opening` and
   `current` as single scalars across every unit in the concept family — and the
   sand rendered `current` as the largest number on the screen. Every timeline
   test passed because none of the fixtures set a unit at all. Both are now
   `BTreeMap<unit, DecimalValue>`, each line seeds from its own unit, and
   `current` is **null** when a concept spans more than one unit rather than
   inventing a scalar. Pinned by
   `two_units_under_one_concept_never_contaminate_each_other`.
2. **Factless mutations never refreshed the screen.** Live invalidation is
   driven by committed Facts, and declaring a rule, pausing one, skipping a
   date, and re-tagging all deliberately commit none — as
   `declaring_a_rule_moves_nothing` asserts. So adding the new sources to
   `affects()` bought nothing for exactly the controls this slice is about: the
   panel would sit stale until an unrelated capture landed. Fixed in the sand by
   re-reading after those actions. Explicitly *not* fixed by making them append
   a Fact, which would put a phantom change in the Ledger to trigger a redraw.
3. **The category filter searched only the newest page.** `execute_entries`
   passed the caller's limit into SQL and filtered the result, so a rare
   category read as empty while matching changes sat just past the cut. Now
   scans wide and cuts after, like `execute_facts`. Pinned by
   `filtering_by_category_searches_past_the_page_limit`.
4. **Dates displayed a day early west of Greenwich.** Captures are stored at
   midnight UTC on purpose, but the sand formatted them in the viewer's zone, so
   a 1 March rent read as 28 February. Display is now pinned to UTC to match
   storage.

##### E3 — future Fiote ergonomics

- [ ] Add `create-capture`, extract, review, apply-to-draft, reject, and
  redact Actions. Typed shorthand is parsed by the pure compiler; voice
  transcription and photo/OCR/model recognition enter as captured Signal
  observations with adapter/version/hash, not privileged Fact writes.
- [ ] Extract only what the narrow event needs: gain/loss direction, magnitude,
  unit/resource candidate, occurred time, source label/Record, tags, and note
  describing recognized purchases or income. Store alternatives, confidence,
  and source spans/bounding boxes; do not grow receipt accounting, line-item
  inventory, or import subsystems inside the sand.
- [ ] Fiote produces the same `EntryDraft` as the sand. It may apply the
  draft only through a current narrow grant over resource, direction, unit,
  magnitude/rate, capture kinds, evidence threshold, time window, and expiry;
  otherwise it leaves an editable draft for the person.

**E3 exit:** typing, speaking, and photographing the same gain/loss can produce
the same canonical draft, while malformed input, ambiguous resource/unit,
revoked permission, or model disagreement remains reviewable and changes no
resource Fact.

#### Canonical objects, types, and short references

These are domain objects, not new silos. Every mutable handle is a Record with a
schema-owned sidecar and expected revision. Operational projections are query
results, not Records merely to make a chart convenient.

> **Superseded 2026-07-26 — read as vocabulary, not as a schema.** The table
> below names `edraft:`, `eevent:`, `eplan:`, `eocc:` and `ecap:` as durable
> handles. Domain-named durable objects are exactly what the standing rule
> forbids, and none of them were built. What shipped instead is general and
> already covers the same ground:
>
> | This table says | What exists |
> | --- | --- |
> | `edraft:` event draft | nothing — a change is captured, not drafted; a draft with no quantity effect is a form, and forms live in the sand |
> | `eevent:` applied event | `store::entries` — a classified change with its Fact, revision, and correction chain |
> | `eplan:` recurring plan | `store::recurrence` — a rule with `nucleus::karma::Cadence` |
> | `eocc:` plan occurrence | **nothing durable** — occurrences are derived from cadence and anchor; only skips are stored |
> | `ecap:` capture | not built; E3 Fiote work, and it must land as a general capture primitive |
> | Rule projection | `Source::Timeline` — recomputed per read, never stored as history |
>
> The rows are kept because their *meanings* are still right, and because the
> lifecycle distinctions they draw are the ones the sand honors.

> **E0 supersedes the `direction` and `tags` vocabulary used throughout the rest
> of this specification.** These tables predate the classification model
> and still speak of a gain/loss direction field and a separate tag list. Read
> both as one thing: a movement's sign carries its direction, and its concept
> classification carries what it was. Where a table below says `direction_in`,
> read a sign filter; where it says `tags`/`tag_in`, read a concept filter over
> the DAG including descendants. The reasoning is in E0 and the substitution is
> mechanical, so these tables are left as written rather than rewritten ahead of
> the implementation that will settle their exact field names.

| Reference / object | Durable meaning and data effect |
| --- | --- |
| `record:@cash` — Resource | Existing Record whose exact quantity and unit are changed by gains/losses. The sand creates no parallel balance. |
| `edraft:r_...` — Event draft | Mutable revisioned gain/loss proposal: resource, direction, positive magnitude, occurred time, source, tags, note, and capture/occurrence cause. It has no quantity effect. |
| `eevent:r_...` — Applied event | Gain/loss event linked to its signed resource Fact, draft revision, actor, source/tags, correction chain, and provenance. |
| `eplan:@rent.monthly` — Recurring plan | Mutable handle plus immutable revision containing Frequency reference, event template, start/end, missed/inactive-gap behavior, and route ceiling. |
| `eocc:r_...` — Plan occurrence | One expected boundary, optional override, lifecycle state, and applied-event link. It changes no quantity until the event Action succeeds. |
| `ecap:r_...` — Capture | Typed text/audio/photo observation, hashes/retention, recognizer versions, alternative extracted event fields, evidence spans, and review state. |
| Rule projection | Cursor-bound response containing actual gain/loss/net, expected recurring gain/loss/net, resource points, exclusions, and contributing ids. It is recomputed and never used as actual history. |

Keep the canonical source timestamps distinct:

| Time | Meaning |
| --- | --- |
| `occurred_at` | When the individual gain/loss happened; default monthly grouping basis. |
| `applied_at` | When Lince appended the signed resource Fact. |

#### Wiring — the real Protein queries and Actions

There is no `source:"economy"`, and the section that used to specify one has
been deleted. It described a private discriminated union whose only reason to
exist was that generic Fact aggregation summed with `f64` and could not group by
what a change was for. Both weaknesses were fixed in the general primitive
instead, so the sand reads through three ordinary sources that any surface can
use.

**Three subscriptions, opened once in `state.js`.** `filter` is spelled `where`
on the wire, and Predicates are externally tagged snake_case.

    { "source": "entry",  "where": [{ "classified_in": "rent" }], "limit": 100 }
    { "source": "recurrence", "where": [] }
    { "source": "timeline", "where": [{ "classified_in": "rent" }],
      "at_since": "…", "at_before": "…" }

`timeline` requires a `classified_in` predicate — a timeline is *of* something,
and a timeline of everything is just the Ledger. It answers with four row kinds:

| Row kind | Carries |
| --- | --- |
| `timeline_context` | concept, window, `now`, bucket, `current`/`opening` and their per-unit maps, `units`, `projection_truncated` |
| `timeline_point` | bucket, unit, `phase` (`actual` \| `present` \| `expected`), actual/expected amounts, running `cumulative` |
| `timeline_source` | `origin` (`recurrence` \| `promise`), uid, amount, `at`, state, note — so every projected point names what produced it |
| — | applied and skipped occurrences are excluded from the expected half, or a rule-driven month would be counted twice |

`current` is deliberately **null** for a multi-unit concept. A single scalar
there would add kilograms to a currency, and it would do it in the largest
number on the screen.

**Ten Actions, all `{"action": "kebab-case", …}`.** Six for rules, four for
changes:

| Action | Effect |
| --- | --- |
| `capture-entry` | Appends one classified Fact. Direction is the sign of `amount`. |
| `revise-entry` | Compensates and replaces; quotes `expected_revision`. |
| `void-entry` | Retracts in the period it happened in. A real refund is a positive capture today, not a void. |
| `reclassify-entry` | Re-tags via the Fact, leaving the amount alone. |
| `create-recurrence` | Declares. Writes **no** Fact. |
| `revise-recurrence` | Changes what is expected; leaves what already ran alone. |
| `set-recurrence-paused` | Hides future dates, keeps past ones. |
| `apply-recurrence-occurrence` | Turns one date into an ordinary `capture-entry`. |
| `skip-recurrence-occurrence` | Records a declined date, so it does not read as merely unanswered. |
| `unskip-recurrence-occurrence` | Offers it again. |

**Live redraw is Fact-driven, and four of those Actions append no Fact.**
Declaring, pausing, skipping and re-tagging commit nothing to the Ledger, so
subscription invalidation never fires for them. The sand re-reads after those
four explicitly. It does **not** append a phantom Fact to force a redraw — a
Fact is a claim that something happened, and inventing one to refresh a panel
would put a lie in the permanent record.

#### Shorthand capture and recurring DSL

Use readable words rather than accounting initials. This deterministic
shorthand is a capture language, not an executable shell. The unit after the
amount is an ordinary Record reference, which is why `@brl` and `@kg` are the
same kind of token here:

    lose 68.40 @brl on record:@cash from merchant:@market
      on 2026-07-22T18:14-03:00 #household

    gain 5000.00 @brl on record:@cash from merchant:@employer
      on 2026-07-31 #work

`lose` and `gain` compile to the typed event draft. `spend`/`earn` may be input
sugar. `@spenditure`,
`@expenditure`, `@expense`, or a local word may be Lingua aliases/equivalences
for recognition, but the canonical direction is `loss`; `@income` maps to
`gain`. Missing resource, unit, magnitude, or ambiguous date yields a draft
question/error, never a guessed Fact.

A recurring plan reuses the Karma schedule language rather than creating a second
cron engine. Since the schedule merge there is one grammar for every "when", and
the calendar clause is a compound step with a bound rather than one of three
fixed shapes:

    frequency household.rent.monthly {
      every calendar(step(months(1)), invalid-day(clamp), bound(unbounded));
      anchor civil("2026-01-05T09:00:00.000");
      timezone America/Sao_Paulo;
      timer { resolution 1s max_lateness 5m coalesce_window 0ms }
      missed latest
      inactive_gap skip_to_next_anchor
      rephase preserve_anchor
    }

    plan rent.monthly revision 1 {
      owner person:@ana
      on freq:@household.rent.monthly
      expect lose quantity(2, "1800.00", unit:@brl) on record:@cash
      source merchant:@landlord tags #housing #rent
      route draft
    }

The day of month and the time of day both come from the anchor — there is no
second place to state them. A one-off rent, say a deposit due on one date, is the
same block with `bound(count(1))`.

The amount is a `quantity` in a unit Record, not a money literal. `unit:@brl` is
an ordinary Record someone created; the kernel has no idea it is a currency, and
"worth five of theirs" is another rule multiplying by a rate.

At the boundary the scheduler creates `eocc:@rent.monthly/<tick>` and a
forecast contribution. `route draft` may prepare a reviewed event. `route act`
must name a narrow apply grant; without it, time never changes the
resource quantity by itself.

#### The graph, precisely

One concept, one line, three regions. The whole point is that **history, current
state and declared future sit on the same axis at the same scale**, because the
question being asked is "where is this going", and that question is unanswerable
if the past and the future are drawn in two different pictures.

```
        │                                   ╭╌╌╌╌╌ declared (dashed)
 amount │            ╭──────╮      ╭────╌╌╌╌╯
        │   ╭────────╯      ╰──────╯ ●  ← now
        │───╯  settled (solid)
        └─────────────────────────────────────────────
           past                    │            future
```

- **Solid** to the left of `now`: settled, read from Facts, never a projection.
- **A marked point at `now`**: the current position, the number a person came
  for.
- **Dashed** to the right: declared — recurrence dates and classified promises.
  Nothing is extrapolated from the past. A trend line fitted through history
  would be the sand inventing a claim the Ledger never made.

Construction rules the implementation follows:

1. **The line starts where the concept already stood.** `opening` seeds the
   running total, or the first bucket would look like the concept sprang into
   existence at the window's edge.
2. **Cumulative comes from the server as exact text.** The sand plots it; it
   never accumulates it.
3. **One line per unit.** Units never share an axis. A concept holding both
   two different units draws two lines and reports no single scalar.
4. **Every future point is attributable.** Hovering or focusing a projected
   point names the rule or promise behind it via `timeline_source`. A number on
   a chart that nobody can explain is worse than no number.
5. **A truncated projection says so.** When `projection_truncated` is set the
   dashed region is a lower bound, and the caption says it.

**Accessibility is not an afterthought here.** The SVG carries a `<title>`, and
every graph has an equivalent table of the same rows plus a contributor list —
not a degraded fallback, the same data. All interactions are keyboard operable.
A chart that only exists visually is a chart half the point of which is lost.

#### Karma sand information architecture

The default view shows a civil month selector, resource selector, unit/filter
scope, and exact cards for **gains**, **losses**, **net**, and **expected
recurring net**. Actual and expected values are never merged. Quick-add offers
Gain, Loss, and Recurring; the same editor handles create and correction.

The primary graph shows the resource's actual quantity history and separately
styled expected path from recurring occurrences. A companion flow graph shows
gain/loss/net by day, week, or month. Selecting a point opens its events/Facts.
A semantic table with identical points is mandatory for keyboard and screen-
reader use; color is not the only actual/expected distinction.

The profile groups by source, Record tags/Lingua concepts, direction,
recurring/individual, capture origin, and cause. Each group shows exact total,
event count, and its event ids. Clicking it issues a narrower server query; the
browser never derives a total from a partial local list.

The activity list visibly separates applied events, drafts/captures, and due
occurrences. The event drawer shows resource, direction, magnitude/unit, time,
source, tags, note, correction chain, capture/occurrence cause, and Facts. The
recurrence view supports next boundary, edit-one/edit-future, pause/end/skip,
and projected impact. There are no account, budget, debt, investment, import,
reconciliation, or tax pages in this scope.

#### Fiote and recognition control boundary

Fiote is an actor-neutral client of the same capture/draft contract. Its useful
future jobs are: parse “I spent 42 reais on lunch,” transcribe a voice note,
read the total/source/items description from a photo, resolve the target
resource and known source/tag slugs, and prepare or correct a matching recurring
occurrence. Each job returns gain/loss event field candidates, not an opaque
final mutation or a broader accounting object.

The canonical capture state is
`captured → extracted → needs_review|ready → applied|rejected`, with a separate
Fact-application state on the resulting draft. Every inferred field records its
recognizer/version, input hash, candidate value, `conf`, source span/bounding box, and
alternatives. A person can edit any field; that feedback may become eligible
learning evidence later but never rewrites the captured model output. Raw voice
or photos can be discarded after review according to retention while keeping a
hash and selected structured evidence.

Fiote may rank existing resource/source/tag Records, but it may not silently
create or select an ambiguous one. Its delegation is deliberately narrow: exact
principal, Program/revision, resource, direction, unit, per-event and period
magnitude, capture kinds, minimum evidence/confidence, time window, rate, and
expiry. A model score is not the grant. Revoking the grant before apply returns
the draft to review without losing it.

#### Proof gates for this surface

- [ ] Gain always derives a positive delta and loss a negative delta; magnitude
  is positive/exact and must match the resource unit. No incompatible units are
  summed and no client sign can invert the declared direction.
- [ ] Editing/voiding an applied event preserves its original Fact and produces
  the correct resource quantity and monthly gain/loss/net under replay, sync
  duplication, stale requests, and crash at every transaction boundary.
- [ ] Recurring boundaries create expected occurrences, never actual Facts.
  Edit-one/edit-future, pause/end/skip, downtime, and DST neither lose nor
  duplicate an occurrence/event.
- [ ] Monthly overview uses explicit civil timezone/bounds; actual and expected
  remain separately queryable. Every total, profile group, and graph point
  drills to exactly its visible event/occurrence/Fact set.
- [ ] Manual form, deterministic shorthand, and future Fiote text/voice/photo
  converge on the same event-draft/apply validator. Ambiguous, low-confidence,
  malformed, or revoked cases change no resource Fact.
- [ ] Two open Karma sands converge live without clobbering a dirty draft;
  charts have equivalent tables, all interactions are keyboard accessible, and
  sensitive hidden entries cannot be inferred through grouping, prior-period
  comparison, projections, or small-cohort differencing.


