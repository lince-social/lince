**Transfer — remaining acceptance, reviewed 2026-09-30**

The agreed non-Karma work in the [Transfer description](Lince.lingua) and [Transfer Tasks](Tasks.lingua) has backend and native UI implementations: disclosure, multi-party donations/trades, private accounting, reservations/forecasts, parent/dependency agreement, grouped Needs, loans/returns, and optional Simulation with reusable checks and comparisons. Steps 1–9 are implemented. Six recovery variants, the four-Cell check comparison with exact replay, and the final backend/native compile check passed. Step 10 remains open; the receipt fix still needs verification.

Follow these checks in order on consistent current sources. Reuse existing cases with two to four Linces, independent Organs and authorized sibling Cells. Ordinary agreement stays independent of Simulation.

1. **Verify receipts after revocation.** Run the two `transfer_trade` tests selected by `revok`, in hosted and replicated modes, with exact replay.

   A pending receipt must return `transfer_delivery_inactive` without changing the database. An exact retry of a completed receipt must return the original application, publish no new Facts and leave quantities unchanged. Changing its amount under the same request ID must return `transfer_remote_application_preview_stale` without changes. Verify the completed receipt's restart/recovery path.

2. **Finish the fixed workflow regressions.** Inspect intermediate transitions, final quantities, privacy, findings and replay evidence in the existing tests.

   | Workflow | What must hold | Simulation test targets |
   | --- | --- | --- |
   | Disclosure and access | Each viewer receives only permitted item fields. Hidden Record identities, parties, quantities and locations stay hidden in queries, packages and history. Revocation blocks future access. | `replay` filtered to `transfer_item_visibility_survives_delivery_and_replay`; `transfer_trade` revocation cases |
   | Independent and multi-party transfers | Donations, trades, source-free items and OPEN claims use each owner's private bindings. Declining never joins or changes stock. Counteroffers invalidate old agreement. | `transfers`, `transfer_trade` |
   | Accounting and reservations | Public 10/private 5 gives deductions 2/3 for partial payments 4/6. Corrections reverse original effects after policy changes. Cancellation releases only the remainder; competing reservations and sibling recovery cannot duplicate commitments or overspend hard limits. | `transfer_applications`, `transfer_balances`, `transfer_cancellation`, `transfer_replicas`, `transfer_campaign` |
   | Parents, dependencies and observers | Required real outcomes determine readiness, independently of hidden rows. Agreed differs from settled. Stale/revoked evidence blocks readiness while preserving completed downstream work; observers receive only authorized evidence. | `transfer_parents`, `transfer_dependencies` |
   | Grouped Needs and corrections | Bike/Transport become 0/0 for a bike, -1/0 for a ride; an already-met Need is not fulfilled twice. Failed review/permission applies none of the group. Correction preserves unrelated edits across restart/retry. Check both final quantities in saved results. | `transfer_effect_groups`, `transfer_campaign` |
   | Loans and returns | The three-day interval is end-exclusive. Extensions, early/partial/missing returns, timezone boundaries and incompatible units produce correct Stored/Available results. Expiry reopens the Need without inventing a physical return. | `transfer_loans` |
   | Optional scenarios and checks | Count only the selected unsettled remainder. Delayed receipts expose shortages. Comparisons share a snapshot and unchanged assumptions. A 5 → -1 → 5 path fails continuous checking while passing at the end; sampled coverage and no-check Unverified remain explicit. | `assumptions`, `checks` |

3. **Confirm the native controls connect the workflows.** Reuse the Transfer/Simulation UI tests and perform one focused walkthrough.

   Send a proposal, agree without simulating, counteroffer and renew agreement, then partially deliver and inspect actual/reserved/surplus. Simulate before sending, during negotiation and after partial delivery with local bindings; compare alternatives and preview grouped Needs and loan expiry/return timelines.

   Create a quantity bound, save/reuse its set, select evaluation timing, inspect a failure and disable checks to see Unverified. Old results retain their definitions and show when current inputs make them out of date. Share only selected authorized fields; recipients use their own bindings. Running or sharing leaves live quantities and agreement unchanged. Record which checks used automated tests and which used the walkthrough.

4. **Run the 23-family/46-case campaign and close acceptance.** Use both delivery modes where supported, with delay, duplicates, loss, disconnection, restart and recovery. Compare logs with each case's expected quantities, agreement, privacy, reservations and pending work. Each expected refusal needs its event/reason; unexpected refusals fail. Legal retries apply effects at most once, and authorized siblings converge.

   Retain failing seeds, evidence and reduced cases. Fix failures within scope and rerun affected cases. Require complete check coverage and exact replay of fixed recovery cases. Repeat backend/native `cargo check --tests --examples` with warnings treated as errors if further code changes require it. Close step 10 only when these four items pass; unfinished cases remain open.

Evidence is under `.cache-lince-transfer-acceptance/evidence`. Preserve `checks-4-6.json` and `regression-budget.json`; volume measurements are complete. The successful compile check is `current-final-check-native.log`. The last receipt attempt failed before testing because concurrent edits produced mismatched Simulation interfaces (`revocation-fix-tests-stable.log`); its dependent campaign never ran. Resume item 1 using consistent sources and the existing compilation cache. The environment setup is `/tmp/lince-transfer-check-env.sh`; existing runs use release dependencies and optimization level 1 for engine, Cell and Simulation.

Karma automation and automatic writer/queue handoff remain outside this acceptance. The 32-Cell workload is removed.
