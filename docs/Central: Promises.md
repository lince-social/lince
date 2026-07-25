## [x] Promises — the social atom

- [x] `create-promise` (including OPEN Needs/Contributions with a known
  proposer and an unfilled counterparty),
  `promise-transition` (validated state machine: open → proposed → agreed →
  active → kept/broken/withdrawn), and standalone `edit-promise-delta`.
  Bundled promise edits use a complete signed Transfer revision and invalidate
  current agreement atomically.
- [x] Expiry is automatic each heartbeat: agreed/active past-window →
  broken (and enqueues an expiry decision); open/proposed past-window →
  withdrawn, quietly. Sands never write their own deadline logic.
- [x] Reservation: `reserve_from` per promise, else the bundle transfer's
  `reserve_default`, else `active`; `include: { availability: true }`
  returns `available` and `planned`.
