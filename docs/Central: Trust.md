> Extracted from `docs/Central: Karma.md` on 2026-07-29. Automation Trust scopes
> (Karma phase K8) stay in that doc — they gate automation, not authorship.

## [x] Trust

- [x] Every locally-authored fact is signed on the write path; imported
  facts keep their ORIGIN signature so downstream Cells can still verify the
  original author — a two-layer tamper model (the hash chain guards
  content, the signature guards authorship).
- [x] Compaction archives stay verifiable file-side; the anchor fact makes
  the file tamper-evident from inside the Ledger.
- [x] No universal/global/public reputation score, ever. Kept/broken signed
  history is the shareable raw material; Karma may derive a local,
  purpose-specific estimate for one concept/role/window, but must show its
  ingredients and must not publish it as a fact about a Person's character.
- [ ] Karma phase K8 adds local Automation Trust scopes above coarse
  `known|blocked`: exact concept/direction and stage ceilings, Person/origin-
  Organ/via-Organ/proximity selectors, probability/confidence thresholds,
  limits, expiry, and deny-first Proof. This policy gates automation only; it
  is not reputation, visibility, consent, or a Program grant.
- Field-level grants and most-specific-wins precedence are tracked in Transfer
  Phase T3, with Transfer as the first consumer of this shared Trust behavior;
  today grants remain whole-row only.
