> Extracted from `docs/Central: Karma.md` on 2026-07-29. Protein is the read
> half of the control plane every pillar shares; it was never Karma's.

## [x] Protein — the read contract

- [x] Six sources, one JSON shape: `record` (state vector, all
  predicates/includes), `promise` (`state_in`), `decision` (the open
  Decision Queue, never exported to remote subjects), `fact` (the Ledger
  itself — `at_since`, `cause_kind_eq`, `record_eq`, `concept_in`),
  `concept` (Lingua vocabulary), `transfer` (bundles with derived status,
  parties, promises, balance).
- [x] Predicates: `all/any/not`, `quantity_lt/lte/gt/gte/eq`, `uid_eq`,
  `kind_eq`, `slug_eq`, `concept_in` (DAG-aware), `linked_to`, `state_in`,
  `near`.
- [x] Includes: `facts` (provenance), `promises`, `links` (kinds, direction,
  depth + hop), `threads` (nested messages), `extension`, `availability`,
  `projection` (`{at:"+7d"}` folds agreed/active promise deltas — full rule
  simulation is the engine-side `project`/`snapshot` pair).
- [x] Aggregation (`sum`/`count` by concept/kind on records, by
  cause_kind/day/concept on facts) — the visibility gate applies BEFORE
  aggregation, hidden rows can't leak through sums.
- [x] Saved Proteins are records (`kind='protein'`) referenced by slug — the
  old "view" concept, done right.
- [x] Maneirisms: the wire `where` is a JSON array = implicit `all`;
  fact-source predicates don't nest (flat list) in v1; `at_since: "30d"`
  resolves against wall-clock now (use absolute RFC3339 for reproducible
  reads); remote subjects see only whole-row visibility grants — the
  Decision Queue and concept-level promises never leave a Cell through
  Protein.
- [ ] **Verifiable aggregates**: a `verified: true` Protein filter
  restricting an aggregate to signed facts only, so a number like "@maria's
  kept-promise ratio for @food, last 12 months" is *provable* to a
  counterparty without either side trusting the computing Cell (leans on
  `docs/Central: Trust.md`). Opt-in only, between mutually-confiding organs — never a
  global or public score.

