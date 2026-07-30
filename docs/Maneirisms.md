> Extracted from `docs/Central: Karma.md` on 2026-07-29. These are the standing
> laws of the whole system, not of any one pillar — which is why they no longer
> live inside one pillar's file.

## [x] Cross-cutting maneirisms cheat-sheet

- [x] Everything is a record; activation is quantity; delete is deactivate
  (and hard-delete is a separate, further step).
- [x] The fact is the truth, quantity is the cache; undo is compensation.
- [x] Metadata/state changes announce themselves as zero-delta annotation
  facts.
- [x] Warnings are advice (cycles, Proof loops), never rejections.
- [x] Current heartbeat order: promise expiry → decision expiry → timers →
  signal sampling → effects (budgeted notify) → senses pass → crossings pass.
  Karma phase K3 replaces the polling heartbeat as timer owner with the
  tickless deadline director/sequencer while preserving explicit stable
  priority.
- [x] One situation, one open decision (dedup by subject+kind).
- [x] Uids are identity everywhere, across Cells; slugs are local sugar and
  get dropped on collision at import.
- [x] Visibility is default-hidden, whole-row, enforced in exactly one
  place — and applied before aggregation.
- [x] Blocked organs are rejected at every door (import, discovery,
  outbox).
- [x] Conventions: uids are prefixed ULIDs (`r_/f_/p_/l_/c_/t_` = record,
  fact, promise, link, concept, transfer); slugs are `dot.case`; timestamps
  RFC3339; durations `90s`/`2h`/`30d`; `@slug` in conditions is sugar for
  `quantity(@slug)`.
- [x] Time is deliberately NOT a record column: automated timing = Karma
  schedules/Frequencies; declarative time (what strangers match on) lives on promise
  windows.
- [x] Board chrome is frontend state; sand data is Protein/Actions.
