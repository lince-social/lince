# Frequency and Karma simulation

1. [x] Fix the occurrence identity so changing a Frequency anchor can apply an already-used logical date again, while repeating a delivery or manual application stays idempotent.
2. [x] Add a reusable simulator checkpoint that checks many records at the same time, with bounded work and honest failure or incomplete results.
3. [x] Generate seeded 100-day scenarios with 100 daily resets to -1, additional signed and fractional quantities, shared daily, weekly, monthly, and yearly frequencies, additive rules, restarts, rewinds, and retries. Use UTC without daylight saving time.
4. [x] Include this family in randomized campaigns, check every task every day, and verify deterministic generation and replay.
5. [x] Run the Frequency and Karma regression suites and the generated simulations.
6. [x] Copy the current workspace consistently, renew authorization only in the copy for the simulation horizon, reproduce the daily rewind, then simulate 100 days with daily checks of every Frequency-linked task. Keep the live database untouched.

An occurrence now belongs to its Frequency activation as well as its logical date. Previously, an application from an earlier activation could suppress a newly requested reset on the same date. Repeated delivery within an activation is still consumed once.

`QuantitiesEqual` is a reusable simulator checkpoint: one Cell, a map of Record references to expected quantities, and a simulated timestamp. It supports the existing quantity basis, check budgets, witnesses, saved scenarios, and current-data time rebasing. Missing access or exhausted budgets cannot produce a passing check.

`simulation::fixtures::frequencies::shared(seed)` creates 132 tasks sharing four Frequencies: 108 daily, eight weekly, eight monthly, and eight yearly. The first 100 daily tasks reset to -1. The others use randomized signed quarter-unit amounts, with both reset and additive rules. Reset tasks are completed between daily checks, so a scheduler that stops resetting them fails the next applicable checkpoint. Expected dates are calculated independently from the engine scheduler.

The family is included in the existing randomized campaign at indices 23, 47, 71, and every subsequent 24th case. Its seed advances with each campaign cycle, including both leap-year and year-boundary starts. The Simulation Castle's existing **Resume search** action runs this campaign; saved generated scenarios can also be run individually. The default example campaign now covers 48 cases, two complete family cycles.

Verification on 2026-10-10:

- Four actual campaign-generated cases, using family seeds 0, 1, 77, and 902, passed all 100 days. Each checked 132 quantities every day: 52,800 quantity evaluations in total. All 104 checkpoints and history checks completed in each case.
- Generation and JSON round trips passed for 64 seeds. The smaller 100-day rewind case replayed exactly. Wrong-quantity and exhausted-budget regressions passed.
- Engine Frequency editing, recurrence, and Karma runtime suites passed: 39 tests. Simulator quantity checks, Karma rules, schedules, and projection suites passed.
- The current-workspace copy passed all 100 days for its five daily and three weekly tasks: 800 daily quantity evaluations, plus the initial rewind checkpoint and history checks. Banho was observed changing from 0 to -1 through its existing Karma rule after the rewind. Tasks were completed between checkpoints to require fresh resets. The original database and the input snapshot were not modified. Authorization was renewed and signed only in the prepared copy because the original roster expires before the end of the simulation horizon.
- `cargo check --offline -p engine -p lince-simulation -p lince-desktop` and `git diff --check` passed. Local long-run testing used optimized cryptography dependencies without changing repository build profiles.
- Broader replay coverage exposed two remaining replication failures: `four_cells_converge_after_duplicates_partition_loss_and_restart` did not copy the stock to the other Cells, and `two_organs_enrol_discover_and_sync_through_four_cells` refused a sync message as `Forbidden`. These cases contain no Frequency rules and were left unchanged. The unrelated network campaign resume test was excluded from the targeted run.

Scenarios, traces, source snapshots, costs, results, and test logs are retained under `target/frequency-karma-verification/`. This folder is private because it also holds the workspace copy. Daylight saving transitions are excluded from this test family by using UTC.
