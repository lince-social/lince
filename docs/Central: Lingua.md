https://github.com/hydro-project/rust-sitter

## [x] Lingua — concepts

- [x] `create-concept`, `adopt-concepts` (foreign concepts keep their uid and
  lineage, re-adoption is a no-op), `declare-equivalence` (cross-dialect
  same-ness).
- [x] The parent DAG powers widening (`concept_in: "food"` matches through
  the DAG) and dialect fallback (`nearest_ancestor_in` treats an unknown
  concept as its nearest known ancestor).
- [x] Unit conversion: one authoritative `concept_conversion` row per
  unordered pair, the reverse direction derived as `1/factor`, only honored
  within a shared ancestor dimension.
- [x] Concepts travel automatically inside sync packages — a record arrives
  already understandable.

## The DAG is the tag system

> Moved here from `docs/Central: Karma.md` on 2026-07-29. These are facts about
> the concept model; Karma was only the first thing that leaned on them hard
> enough to write them down.

`concept` + `concept_parent` + `descendants_including` are already multilingual
through `concept_name` and already hierarchical, so classifying something
`@ice-cream` makes it answer a query for `@food` and `@cost` with no list to
maintain. **Do not add a parallel flat tag field** — a second classification
axis is a second thing to disagree with the first.

**A concept may have many parents, and that already works.** `concept_parent` is
`UNIQUE(concept_uid, parent_uid)` — a many-to-many edge table, not a single
parent pointer. So `@food` can be a child of both `@substance` and `@cost` at
once, and querying `@cost` reaches every food. Nothing needs building for this.
Worth stating because a role like `@cost` sitting above a kind like `@food`
looks wrong under a strict taxonomy; here it is fine, because direction is a
delta's sign, so food you *sell* appears as a positive movement under the same
concept rather than needing a second one.

## A Record carries many concepts, not one

- [x] A toothbrush is a cost *and* a health item, and both must be queryable —
  as a recurring expense and as a view of hygiene items. `record.concept_uid` is
  a single column and 57 call sites depend on it, including Transfer matching
  and sync, so it **stays** as the Record's *identity* concept — what the thing
  is. A `record_concept` join table carries its additional classifications, and
  every concept query reads the union of both. This is additive: existing
  matching keeps working untouched, and it is semantically right rather than a
  compromise — a toothbrush *is* a toothbrush and *counts as* a cost and a
  health item. **Landed** in `0036_classification.sql` with
  `add_record_concept`, `remove_record_concept`, `record_concepts` and
  `records_with_concept`; the last expands down the DAG and unions both sources,
  so a Record classified either way appears exactly once.
- [x] Classification therefore has two levels that must not be confused. A
  **Record's** concepts say what the thing is and counts as, and drive views
  like "everything I use for hygiene". A **Fact's** concept says what a
  particular movement was, and drives totals. Buying the toothbrush is one `-10`
  classified `@hygiene-purchase`; the toothbrush Record being `@health` is a
  separate, standing truth. A query may filter on either, and the surface must
  say which it used. **Landed as two named predicates** —
  `resource_concept_in` picks the Records, `concept_in` picks the movements —
  and the context row reports both back, so "which axis produced these numbers"
  is on the wire rather than inferred.

## Conversion is exact, and has no time dimension

- [x] **Convert between units explicitly, and exactly.** `concept_conversion`
  holds one factor row per unordered unit pair, derives the inverse at read
  time, and honors conversion only within a shared dimension. The factor is
  stored as an exact integer **numerator and denominator**, so `kg → g` is exact
  and `kg → lb` stays exact until something asks it to round. Conversion is an
  explicit operation with a declared result scale and rounding rule; it never
  converts implicitly to make an expression type-check, because an implicit unit
  coercion is how a rule quietly computes the wrong number.

  **Deviation, decided 2026-07-25:** the plan said to store the ratio *alongside
  the existing float*; the float column was **removed** instead. Keeping both is
  the same two-representations-of-one-quantity ambiguity the exact-Ledger work
  spent a whole slice deleting, and there was no legacy database to protect. The
  legacy `f64` `convert()` now derives its factor from the exact ratio, so the
  two paths cannot disagree about what a conversion means. Inverting a rational
  is lossless, which is what makes the derived `b → a` direction exact too.

- [ ] **Time-varying rates are a separate design.** `concept_conversion` has
  **no time dimension**, which is correct forever for `kg → g` and wrong for a
  currency the moment a rate moves — converting a 2020 expense at today's rate
  silently rewrites history. Guard rail for whoever gets there: do not reuse
  that table for currencies. Until then, sums stay unit-separated and simply
  refuse to combine two units.

A currency is an ordinary Unit Record. `12.50 @brl` and `2.5kg` are the same
token shape; there is no money type anywhere in the kernel. See
`docs/Central: Karma.md`, "There is one dimensioned type, not two".
