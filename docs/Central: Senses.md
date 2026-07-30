> Extracted from `docs/Central: Karma.md` on 2026-07-29. Senses were scattered
> through the Perception and Learning chapters there rather than sitting in a
> section of their own, which is why this file was empty.

## [~] Senses — the pure recognizer

A **Sense** is a pure, named recognizer over current Facts, Signals, discovery
data, and projected crossings. It emits evidence-backed candidates; **it cannot
write state or contact another Cell by itself.** That restriction is the whole
definition — a recognizer that could act would be a rule, and a rule is a
different object with a different authority story.

In the authoring vocabulary a Sense is `sense:@slug`, written `sense name = ...`,
and it appears inside a Program graph as an input rather than an outcome.

## [x] What ships today

- [x] A match rule is a record — `create-match-rule { watch_concept,
  max_proximity, min_confidence, auto }` — and activates/deactivates like any
  other rule through its quantity. `max_proximity` is a hard ceiling; matching
  never auto-expands past it.
- [x] Each heartbeat, `senses_pass` joins local OPEN promises against the
  discovery cache using sign-opposite deltas, Lingua-aligned concepts,
  overlapping windows, and a confidence floor, then places ranked drafts in the
  Decision Queue. It proposes; a person decides.
- [x] Deterministic `confidence(@p)` is the counterparty's Laplace-smoothed
  kept-promise ratio. `demand(@concept)` is the current hour's share of trailing
  30-day activity for that concept. **Neither is a global reputation score** —
  both are local, purpose-specific, and computed from visible signed history.

## The name is too broad for where this is going

`confidence(@p)` is a useful ingredient, but it blurs quantities the destination
keeps apart — recurrence probability, estimate confidence, counterparty
evidence, expected utility, and authority eligibility are five different claims.
The table naming them lives in `docs/Central: Karma.md` ("Recommendations and
learning"), because it is that chapter's routing vocabulary; a Sense only has to
know that it produces evidence, never permission.

A likelihood is not permission, and a threshold is a routing policy rather than
a truth. The machinery that turns a Sense's output into a recommendation, a
draft, a question, or an action is Karma's.
