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
