# The Levels of Lince — an Institute primer

A short, teachable map of what Lince *is*, bottom to top. Each level is a thin
contract the level above depends on and the level below never knows about. If you
remember one sentence: **everything is a Record, every change is a Fact, sands
speak only Protein (reads) and Actions (writes).**

## 1. Record — the noun

Everything is a Record: a person, a task, a rule, a transfer, a saved query.
One table, one identity model — `uid`, `slug`, `kind`, `head`, `body`,
`quantity`, `concept`, `unit`. What a record *is* comes from its `kind` and its
`concept`, not from a bespoke table. This is why we don't add a `user` row type
or a `todo` row type — they're records.

## 2. Fact — the history

Every change to a record's `quantity` is an append-only **Fact**: a `delta`, a
`cause`, an `actor`, hash-chained to the one before it. `quantity` is only a
*cache* of the running sum, with exactly one writer (the engine's `append`).
Nothing is ever mutated or deleted in place. **Undo is a compensating Fact**
(the inverse delta), never a row deletion — see the `Compensate` Action.

## 3. Promise — the future

An intended change that hasn't happened yet: a `delta`, a time `window`, a
`party`, and a small state machine (proposed → agreed → active → settled). A
Transfer is a bundle of promises between parties.

## 4. Lingua — the vocabulary

Concepts form a DAG that gives records meaning: a record's **concept** (what it
is), its **unit** (how it's measured), and **link kinds** (how records relate).
Multilingual names all resolve to one concept, and `concept_in("food")` matches
`@apple` through `apple → fruit → food`.

## 5. Instinct — meaning that computes

A concept that carries a built-in engine function — e.g. **Place**, which knows
proximity math. Instincts are where semantics attach to the vocabulary.

## 6. Link — the graph

Typed edges between records: `(from, kind, to, quantity)`, where `kind` is a
Lingua concept. Identity is the *triple*, so two records can be joined by many
kinds at once.

## 7. Protein — the READ layer

A declarative query **AST**, the successor to SQL views. A Protein has:

- `source`: `record | promise | decision`
- `where`: a boolean predicate tree — `all`/`any`/`not` over `kind_eq`,
  `slug_eq`, `uid_eq`, `concept_in`, `quantity_lt/gt/eq`, `state_in`, `near`
- `include`: extra data attached per row — `facts` (provenance), `promises`,
  `links`, `availability`
- `order`: `asc(field)` / `desc(field)` / `topo(kind)` (graph order)
- `limit`

Rows come out as JSON records: `{ uid, slug, kind, head, body, quantity,
concept, unit, …includes }`. **Sands read only through Protein.** Protein never
mutates.

## 8. Action — the WRITE layer

Typed verbs — `create-record`, `set-slug`, `edit-record-text`, `set-quantity`,
`set-concept`, `set-unit`, `set-extension`, `deactivate`, `compensate`,
`create-promise`, `save-protein`, … Every write is an Action; each is validated
in the engine and terminates in `append()` with provenance. **Sands write only
through Actions**, and there is no privileged path — even Fiote (the agent) uses
the same verbs.

## 9–11. Karma, Transfer, Trust — behavior

- **Karma**: `kind='rule'` records — `condition → gate → carry → consequences`;
  facts trigger cascades.
- **Transfer**: multi-party settlement of bundled promises (agreement levels,
  satiation, chains).
- **Trust**: ed25519 identity keys and visibility rules gate who sees what.

## 12. Sand — the surface

A Sand is a UI widget (a sandboxed iframe package) that speaks **only** Protein +
Actions over one transport WebSocket, plus ephemeral **lanes** for presence and
sand-to-sand ABI events. Board chrome — layout, camera, pins, grouping — is
*host state*, not Ledger. A sand is fully defined by two things:

1. the **Protein** that feeds it data, and
2. the **Actions** it emits.

Swap the Protein and you swap what the sand shows. That is the whole idea behind
per-card Protein selection (below).

## Saving a Protein is a protein-record

Yes — a named, saved query is the textbook use of a Record. A **saved Protein**
is a `kind='protein'` record whose `slug` is its name, `head` its title, and
whose AST lives in the `lince.protein` extension. It is the direct successor to a
named SQL view, and its CRUD is just records + Actions:

| Operation | How |
|---|---|
| **Create** | `save-protein { slug, head, ast }` |
| **Read / run** | `subscribe_saved(slug)` — resolves the record, runs its AST live |
| **Update** | `save-protein` again with the same slug (upsert: updates head + AST) |
| **Delete** | `deactivate { target: slug }` (append-only: hide, don't erase) |
| **List** | Protein `{ source: record, where: [{ kind_eq: "protein" }] }` |

## Per-card Protein selection (the new "view selection")

Because a sand is *defined* by the Protein feeding it, a board card carries a
**driving Protein** in its host-side card state — either a saved-Protein slug or
an inline AST. The sand subscribes to that (falling back to a broad
`{ source: record }` when none is set). The sand-settings modal's old
view-picker becomes a **Protein CRUD**: browse/create/edit/delete saved Proteins
and pick the one that drives this card. A raw-AST editor (with validation)
exposes *every* Protein feature; a friendlier form builder can layer on later.

### When the data doesn't match what a sand renders

A Protein may return more than a given sand understands (e.g. a todo sand pointed
at a Protein that `include`s aggregated work-metadata). The rule is
**tolerant-ignore**: a sand renders the fields it knows and silently drops the
rest — it must never break on an unexpected shape. It may *optionally* constrain
the editable Protein to shapes it can render, but ignoring extra data is the
default and preferred behavior. Missing expected fields degrade to empty, not to
errors.
