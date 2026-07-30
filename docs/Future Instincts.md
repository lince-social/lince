> Extracted from `docs/Central: Karma.md` on 2026-07-29.

## [ ] Future Instincts and product surfaces

- [ ] OSM place data: a local offline extract, local geocoding, `route_eta`,
  polygon `within`, a `route(a,b)` include (`distance`/`near` already
  live).
- [ ] **Time-varying unit conversion.** `concept_conversion` holds one exact
  rational factor per unordered pair with no time dimension — right forever for
  `kg → g`, wrong for a currency the moment a rate moves, because converting a
  2020 expense at today's rate silently rewrites history. Time-versioned rates
  are a separate design and must not reuse that table.
- [ ] Storage-engine independence (AniccaDB): swapping out `store` must not
  change one character of the Protein/Action contract.
- [ ] New product surfaces beyond the Transfer workstream: route/ride
  planning, calls, calendar/time budgeting, simple Economy projections,
  social feed.
