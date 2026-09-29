**Transfer work left — reviewed 2026-09-28**

Complete the non-Karma work in the [Transfer description](Lince.lingua) and [Transfer Tasks](Tasks.lingua), including the shared Simulation checks needed by Transfer, Karma and other features. Implement the numbered steps sequentially: visibility, coordination, resources, agreement, combined Needs, temporary fulfilment. Each step includes backend behaviour, native UI and a reproducible Simulation case. The configurable-checks task is currently in the Simulation record in Tasks.lingua.

Status: implementation authorized. Steps 1–3 are complete. Step 4 is in progress; steps 5–10 are pending. Shared checks and optional Simulation access belong in step 3, before completing resource and agreement features; step 10 is final validation. No Simulation step is required for Transfer agreement.

Reuse the finished Simulation foundation: isolated Cells, copied databases, Lingua seeds, controlled time and network, saved runs, replay, failure reduction and native run/pause/step/inspect controls. Extending Simulation with reusable checks, scheduling of checks and scenario inputs is authorized scope for the future implementation. Build those capabilities for all simulations, then use them from Transfer.

Karma runs in ordinary Cells too. Simulation drives the same engine with controlled time against copied state, and existing Calendar projection also calls that engine. Runs include the owner's already-enabled, authorized Karma by default. This plan adds no new Karma automation feature or scheduler; an optional run with Karma disabled is labelled as a different assumption.

The existing Transfer workflow already includes drafts, invitations, counteroffers, signed revisions and agreements, OPEN promises, delivery and receipt claims, partial settlement, disputes, corrections, remote delivery and a native Castle. Extend these paths. Use shared presentation models in `interface` and the existing Castle in `desktop`; leave the old `web` crate alone.

**Current gaps confirmed in code**

| Area | Existing support | Remaining work |
| --- | --- | --- |
| Visibility | Whole-Transfer visibility and private accounting fields | Independent item text and per-recipient field disclosure; the projection still reports `field_overrides_supported: false` |
| Coordination | Multiple participants and matched promise pairs | Explicit exchange routes, repeated matching items, split contributions and private observer links |
| Resources | Donation/sale presets, reservations and receipt formulas | Independent-Organ workflows, custom outgoing accounting and accurate remaining reservations |
| Agreement | Dependency checks and visible hierarchy summaries | Authoritative parent agreement and agreement derived from upstream outcomes |
| Combined Needs | One source Record per promise; one local Record per remote application | Several private effects for one public outcome and time-limited fulfilment |
| Optional exploration | Signed Transfer revisions, remote delivery and local Simulation runs | Simulate at any time; optionally share a scenario or selected results through normal discussion; keep existing agreement levels |
| Shared checks | Fixed predicates including quantity equality, nonnegative quantity, fact-chain and duplicate checks | User-created checks, reusable sets, selectable evaluation times, failure handling and measured cost |
| Simulation coverage | Local sale followed by remote package delivery; generated campaigns and replay | Participants acting from independent Organs, Transfer-specific checks and varied Transfer campaigns |

The source review covered [Transfer actions](../../crates/engine/src/actions.rs), [read projections](../../crates/protein/src/lib.rs), [storage](../../crates/store/src/transfers.rs), [remote delivery](../../crates/engine/src/transfer_delivery.rs), [native Transfer UI](../../crates/desktop/src/transfer_castle.rs), [Simulation scenarios](../../crates/simulation/src/scenario.rs), [Transfer fixture](../../crates/simulation/src/fixtures/transfer.rs) and [Simulation UI](../../crates/desktop/src/simulation_castle.rs). These are implementation findings, not a claim that the tests were rerun during this review.

**Rules shared by every step**

- Public terms say what is promised between people. Private bindings say which local Records change. Each owner controls their bindings; another Organ cannot write those Records directly.
- Keep invitation acceptance, agreement, delivery/receipt claims, public settlement and local application distinct. A received network package proves delivery of the package, not delivery of the resource.
- Bind public decisions to the exact Transfer revision and private settlement reviews to the exact local policy. Public term changes reset agreement as the existing revision flow requires. Private accounting changes invalidate affected settlement previews and mark old scenario results out of date for current data, without creating a requirement to run Simulation.
- Keep completed facts. Corrections add linked evidence and compensating changes. Repeated requests, messages and restarts must not duplicate effects.
- Simulation uses ordinary authorized actions and receiving checks. Selected checks observe the result independently. Report execution completion separately from check coverage: a completed run with no enabled checks is unverified, and a stopped or incomplete run cannot pass. Simulated changes never become live facts merely by accepting a preview.
- Add any Transfer-specific scenario adapter or check alongside the step that needs it, then reuse it in later scenarios.

**Agreement and Simulation**

The normal workflow is: a party suggests terms → the required parties agree using the existing agreement levels → the Transfer proceeds. A counteroffer changes the public revision and resets agreement through the normal path. A person may use Simulation to help decide, or agree without it. The existing checked/agreed levels remain human decisions; a successful run never advances them. There is no additional “everyone simulated,” verification quorum, review certificate or simulation-feedback state on a Transfer.

Simulation is available independently from the Simulation Castle and as “Simulate” from selected Records, Karma and Transfers. It can run before drafting, while negotiating, after agreement, during partial delivery or from a saved historical snapshot. Receiving a proposal does not launch a run or insert a required review screen. Opening, running, changing checks, changing hypothetical assumptions or sharing a run does not change live Transfer terms or agreement. Changes to live terms always require an explicit ordinary action.

| Data | Contents and handling |
| --- | --- |
| Scenario | Stable identity/revision, time window, typed assumptions and exact referenced Record/Transfer revisions. Transfer assumptions include quantities, delivery times/order and selected alternatives. |
| Local run | Snapshot identity/time, actor permissions, private Record bindings, policy versions, enabled Karma/runtime, resolved check set, seed, results and coverage. Saved on the owner's Organ. |
| Optional shared scenario/result | Only authorized terms and chosen result fields, with scenario identity, source time and check coverage. Sent explicitly through normal discussion/delivery. No automatic reporting or invalidation messages to other parties. |

Each recipient can run shared intended changes against their own data whenever they choose. Local database hashes, private Record IDs, quantities, rules, check definitions and full run artifacts stay local unless explicitly selected and authorized for sharing. A shared check referring to a private Record needs the recipient's local mapping; it cannot query a hidden source by guessing its UID. A Transfer scenario carries typed domain intentions, not a database patch, arbitrary executable code or sender-authored instructions to modify unrelated Records.

Run one scenario by default. “Compare” optionally runs the current plan and the proposed plan from the same snapshot with the same seed, local Karma and assumptions for unchanged work. Replace a revised Transfer's old terms in the proposed run. Count each unsettled exchange once, exclude its already-settled amount, and include only selected alternatives. Save timelines as well as final quantities.

Future delivery is an explicit scenario-only assumed outcome, using the same amount, unit and private-effect calculations as live settlement. It creates no real claim, signature or agreement. The local owner authorizes the run; other parties are not impersonated with production credentials. Unsupported external effects make the affected run incomplete unless an explicit simulation adapter supplies a recorded response. Running an isolated scenario sends no real messages or external commands.

A saved result remains evidence about its original snapshot. Mark it out of date for current-data comparisons when domain data, bindings, formulas, permissions, enabled Karma, runtime or scenario inputs change. Writing run metadata does not invalidate itself. Reuse the projection cache's expiry and refresh rules for current forecasts. A rerun is a user choice; stale or failed simulation results never block ordinary agreement, reset it or cancel existing obligations. Live permission, revision and hard-reservation checks remain in the ordinary actions.

**Shared Simulation checks**

Checks are a Simulation capability used by Karma, Transfer and other features. Extend the existing predicate/check/result types instead of adding a Transfer-only checker. A user can add a check for one run, save a named set, enable a subset for a scenario or campaign, and override that set for a new run. Freeze the resolved definitions and versions in the run manifest so replay uses the same checks. Editing a set later does not rewrite old results.

| Part of a check | First implementation |
| --- | --- |
| Identity and scope | Stable ID, name, enabled flag, selected Cell/Record or supported domain target, optional simulated-time window |
| Condition | Exact quantity comparison (`<`, `<=`, `=`, `>=`, `>`), including bounds and Need = 0; existing integrity, duplicate, refusal and convergence predicates |
| Evaluation | At run end; at a chosen simulated time; after every relevant committed change; every N runner events; or every duration of simulated time |
| Failure handling | Stop on the first finding by default, or record findings and continue. This affects the isolated run only. |
| Evidence | Expected and observed value/unit, offending event and time, first failure, evaluation count, covered interval and skipped/incomplete reason |

Built-in definitions provide common checks; a user-created quantity check is a parameterized instance selecting its Record, comparison, bound and evaluation mode. Keep these read-only. Do not add a second automation language or arbitrary user code for this first version. More domain predicates can use the same registration and result format later. Only offer evaluation modes a predicate can support; an end-of-run duplicate audit may cover the whole saved history and must report that history coverage explicitly.

Define evaluation precisely:

- **End:** inspect the state after the requested horizon is reached and all included events at that time are handled. A stopped run has no completed end check.
- **At time:** inspect the state after all modeled events at the chosen instant. The runner must visit that instant even when no domain event occurs there, rather than checking a later state.
- **Every relevant change:** inspect the initial state of the selected window and every committed change affecting the check, including multiple commits inside one runner step or at the same simulated time. For quantity bounds this supports “never below/above during this run.” Do not inspect transient updates inside one atomic transaction. Extend committed-event evidence where end-of-step snapshots currently lose intermediate states; missing evidence means incomplete coverage.
- **Every N events / every duration:** sample at the declared interval, include the initial and final in-window state, and label the result sampled. These modes can miss a fall followed by a recovery between samples. Never present sampled success as “never crossed.” Durations use the simulated clock, not wall time. Count domain runner events, excluding check evaluations themselves. Persist the cadence and cursor across pause/restart/replay.

Scheduling a check must not change domain event order, consume domain randomness, trigger extra Karma work or spend an action's execution budget. In continue mode, changing the enabled checks or their timing must leave the modeled actions and resulting state unchanged. Checking overhead and check limits are measured separately from domain work.

For example, a Record moves 5 → -1 → 5 in two separate commits. `quantity >= 0` after every relevant change must fail even if both commits occur in one runner step. The end check passes. A coarse sampled check may pass, with its sampled coverage visible. A jump directly from 5 to -1 still violates the bound even though the stored quantity never equalled zero.

Checks observe and report the actual simulated path. Stop mode preserves the failing state and evidence, then stops further runner work once the finding is detected; it must not suppress the offending operation and report success. Choosing fewer checks never disables authorization, input validation or existing hard constraints in normal engine actions. If a user wants a new live write restriction, that is an explicit domain policy, separate from a simulation check.

Report run completion and verification separately. Each enabled check reports passed, failed or incomplete together with its evaluation coverage; disabled checks report skipped. A completed run with no enabled checks is unverified. A required Record missing at its evaluation is a failed check; inaccessible data, unsupported units/conditions or exhausted budgets make the relevant evaluation incomplete. An initial violation counts. A runtime error or early stop cannot turn outstanding checks into passes. Preserve this distinction in the UI, saved artifacts, CLI and campaign summaries. Allow no-check exploratory runs; remove the current validation rule that requires at least one check without introducing a vacuous passed result.

**Check cost and UI:** Add a Checks panel to the existing Simulation Castle: add a built-in/custom quantity check, choose its target/value, evaluation and window, enable/disable it, save/select a set, and choose stop/continue. Show failures and coverage beside run results. Index checks by their changed inputs, consume committed-event evidence incrementally and schedule only the requested evaluation times. The existing duplicate check rereads the trace and the chain check rereads history; retain cursors/seen identities instead where correctness allows it. Reuse state hashes already produced by the runner where possible. Measure execution, evidence recording and checking separately before promising a speedup.

**Acceptance:** Use the same check definition in a Karma-only case and a Transfer case. Test the 5 → -1 → 5 path, strict/inclusive bounds, an initially bad state, a deleted target, atomic multi-Record changes, same-time commits, quiet-time scheduled checks, and stop versus continue. Run a fixed scenario with full, selected, sampled and no checks; in continue mode its domain events and final state must match. Report evaluation count, checking time and events/second for each setting. Changing check configuration creates a new run; a replay retains the original manifest. User-selected checks may vary, while the feature's required regression suite still runs all its acceptance checks.

**Execution ownership:** Shared scenario/check/result types live in `nucleus`; saved local configuration/cache metadata in `store` where needed; permissions and reusable live effect calculations in `engine`; scheduling and evaluation of Simulation checks in `simulation`; authorized results through `protein`; reusable controls in `interface` and the existing `desktop` Castle. The current `simulation → cell → engine` dependency stays one-way. Use a shared committed-change observation boundary so checking works for all domain features. Live Transfer actions do not depend on Simulation, and Calendar and interactive scenarios consume the same effect calculations and projected quantities.

The first version has independent runs, optional comparisons, reusable read-only checks and ordinary Transfer discussion/agreement. Each participant keeps their own data. A central copy of everyone's state, global optimization, automatic bargaining and a distributed all-or-nothing commit are outside this work.

**Implementation checkpoints**

Complete steps 1 and 2 before step 3; their disclosure and exchange identities are inputs to Transfer scenarios. Step 3 adds shared Simulation checks and independent scenario access while completing ordinary donation/trade. Step 4 extends accounting, step 5 quantities, steps 6–7 agreement evidence, and steps 8–9 fulfilment. Each extension reuses the shared scenario/check/result types and runs the earlier acceptance cases affected by its changes.

Track each numbered step as pending, in progress or complete. Mark it complete only after its backend, native workflow and acceptance case pass. When coding is authorized, update this same file at the end of each step with status, changed entry points, exact checks/results and the next unfinished item. After context compaction, read these architecture decisions and the first unfinished step, inspect the current diff, and continue there. Do not reopen settled design choices or start a later step to avoid an unfinished acceptance check.

| Step | Deliverable | Status |
| --- | --- | --- |
| 1 | Item disclosure and recipient views | Complete |
| 2 | Explicit exchanges and observer links | Complete |
| 3a | Shared checks, saved sets, scheduling and results | Complete |
| 3b | Independent participants and Transfer scenario inputs | Complete |
| 3c | Independent runs, optional comparisons and sharing | Complete |
| 3d | Native donation/trade and optional Simulation access | Complete |
| 3e | Combined remote workflows and check acceptance | Complete |
| 4 | Private incoming/outgoing accounting | In progress |
| 5 | Reservations and scenario quantities | Pending |
| 6 | Parent agreement | Pending |
| 7 | Dependency agreement and observer fulfilment | Pending |
| 8 | Several private Needs behind one outcome | Pending |
| 9 | Temporary fulfilment and returns | Pending |
| 10 | Combined scenarios and native acceptance | Pending |

Current implementation item: step 4. Add owner-controlled outgoing and incoming policies, exact cumulative accounting and unit handling across live settlement and Simulation. Finish the private review controls, stale-policy checks, split settlements and recorded-effect corrections before starting reservations in step 5.

Step 1 implementation checkpoint: item text and disclosure live in `nucleus::transfer::disclosure`, signed promise snapshots and `promise.item_json`. Drafts may omit a private Record. Protein applies the same field projection to recipient reads and delivery, protects generic history/promise/timeline reads, and preserves only permitted history metadata. Counteroffers keep undisclosed terms, private bindings and disclosure settings; revisions cancel queued packages containing older item terms. The native composer has item text, audience controls and recipient previews. Passed: disclosure unit tests (3), engine disclosure tests (3), agreement regression tests (4), native Transfer tests (6), hosted and replicated delivery/replay, and backend/native `cargo check --tests` with warnings treated as errors. Source bindings require read access; hidden existing terms remain intact in counteroffers. Test targets: `nucleus transfer::disclosure`, `engine --test transfer_disclosure --test transfer_agreement`, `lince-desktop --lib transfer_castle::tests`, and `lince-simulation --test replay transfer_item_visibility_survives_delivery_and_replay`. Step 2 is next.


Step 2 implementation checkpoint: signed `TransferItem.exchange` names a stable route, giver and receiver. Agreement, activation and progress use that route; OPEN claims, remainders and corrections preserve their lineage. Source-free items activate without a private Record on the origin. The native composer has From/To/Amount fields and a private “Use this outcome” action. Observer links require readable local or active received remote terms and never add an upstream participant. The separate-Organ acceptance case uncovered and fixed delivery to pending invitees, explicit viewer delivery, the stale Organ lookup in local application, and nondeterministic ordering in signed delivery projections. Simulation now carries signed remote commands, replies and application acknowledgements through shared production Cell handlers. Private stock is excluded from the scenario's identity sync. Passed: engine agreement (4), disclosure (3), exchange/observer/revision (3), native Transfer (7), settlement fact identity (1), backend/native `cargo check --tests`, and `lince-simulation --test transfers` including exact replay (107 seconds for the combined run and replay). The four-Cell fixture `transfer::three_parties()` settles three routes independently, rejects observer delivery before read access, keeps the observer's link private, preserves each command's Person/Organ binding, and leaves private donor stocks at 28 and 3. Step 3 completes receiver application and remote recovery scenarios; this fixture currently verifies giver application and public route completion. No Simulation run is an agreement requirement.

Step 3a implementation checkpoint: shared check definitions and coverage live in `nucleus::simulation::checks`; scheduling, committed-change checks, budgets and cost reports live in `simulation`. Store migrations 0102–0103 preserve committed Fact batches and revisioned private check sets. Checks support end, a chosen time, every committed change, event intervals and time intervals, with windows and stop/continue controls. No-check runs execute normally and report Unverified. Native Simulation has check editing, saved sets and selected-check searches. Saved manifests freeze check definitions and options; replay excludes wall-clock cost comparisons. Passed: generic check integration (3), atomic-change and missing-evidence unit coverage, replay regressions (12), campaign tests (4), native Simulation tests (5), three-party replay, and native `cargo check --tests` with warnings treated as errors. Continuous checks inspect each committed quantity change; sampled checks report their narrower coverage. Initial measurements show state hashing remains a significant check cost; optimize from combined-workload evidence in step 10.

Step 3b implementation checkpoint: `engine::transfer_counterparty` creates one application handoff for the other endpoint of each settled route, without another public promise. `ApplyTransferApplication` handles local-origin and remote receipts through the same owner-selected Record review. Migration 0104 preserves private application history and exposes both handoff sources through one view. Protein supplies the private preview; native controls and Simulation use that projection. Reviewed amounts and earlier slices are checked inside the transaction. Exact request replay preserves the original private effect and signed acknowledgement. Application, public settlement and network acknowledgement have separate progress. Simulation now retries durable remote commands and application attestations through the production Cell handlers; late verified packages preserve the newest view. Passed: donation in hosted/replicated modes, lost acknowledgements and duplicate packets, three-party receipts with exact replay (3 tests, 150 seconds), partial receipts in both modes including stale previews, out-of-order application and exact replay of private changes (1 test, 24 seconds), all migrations with no foreign-key violations, and native `cargo check --tests`. Donation leaves 20/10 with one public contribution; three-party receipts leave Ana/Beto/Carla at 29/3/3. Private stock Records stay on their own Organs. Step 3c is next; broader trade, OPEN-offer, changed-term and correction combinations remain in 3d–3e.

Step 3c implementation checkpoint: `simulation::assumptions` applies owner-selected hypothetical changes through ordinary quantity facts on copied databases, using the shared default accounting calculator. Referenced revisions and routes are checked; already-applied amounts are excluded and whole-exchange assumptions cannot be mixed with individual occurrences. `comparison` pins both runs to the same snapshot; artifacts retain source versions and identify changed live data. Native Simulation has assumption editing, private quantity timelines, Compare, inspection of both runs, and selected sharing with an exact preview. Transfer drafts/items and received shared intentions open optional scenarios; recipients choose their own bindings and checks. Typed shared messages use ordinary Transfer discussions and delivery, apply item disclosure, and omit private Record IDs, formulas, check definitions and database hashes. Passed: assumption integration (3 tests, 26 seconds), including six-versus-five with existing Karma, exact replay, stale source labels, partial settlement in hosted/replicated modes, ordinary shared-message delivery with unchanged agreement, and a temporary shortage despite a positive ending; disclosure tests (4); native Simulation tests (6); native `cargo check --tests` with warnings treated as errors. Native tests use an enlarged test stack. Step 3d is next.


Step 3d implementation checkpoint: Donation and Trade presets create explicit routes. Native claim, invitation, dispute and discussion forms now match their action schemas. Record and saved Karma views open independent Simulation; Transfer entry uses the scenario's actual end time. `BeginTransferSettlement` prepares source-free local or remote applications; the owner then chooses a local Record through the shared application review. Preparation reserves its canonical slice transactionally and refuses changed request replay. Passed: source-free donation in hosted/replicated modes with exact replay, unchanged stock before review, altered-request refusal and foreign-Record refusal (54 seconds); native Simulation tests (6); native `cargo check --tests`. Native Transfer tests passed (9), including the corrected preset expectations and action payloads.

Step 3e implementation checkpoint: `simulation/tests/transfer_trade.rs` contains separate-Organ trade, counteroffer, public OPEN claim, refusal and revocation cases. Trade and refusal passed in hosted/replicated modes with replay. The trade checks the unpaid state after bike delivery and final payment split into 4 and 6. Simulation's action allowlist now includes counteroffers, OPEN claims, disputes and settlement compensation; its invitation adapter can also decline. Migration 0105 lets an OPEN pair retain optional private sources, and native claims leave local Record selection to the application review. Donation recovery now restarts the sender after commit but before acknowledgement. Further passed: public OPEN claims in both delivery modes with replay, restart-before-acknowledgement recovery (71 seconds), and existing settlement/correction/exchange regressions (7 tests). The counteroffer fixture was corrected to retain only pending invitations. Revocation uncovered a missing Simulation refresh adapter: Cell now shares pull preparation and response handling between live refresh and Simulation. Passed: revocation learned through ordinary refresh refuses a pending local application without changing stock (both delivery modes, 21 seconds); counteroffer received through refresh resets agreement, preserves private bindings, rejects the old revision before and after renewed agreement, and settles only the new amount with exact replay (61 seconds). Simulation and native `cargo check --tests` passed with warnings treated as errors. The optional-run/check cases from 3c and three-party cases from 3b complete this step without adding any Simulation requirement to agreement. Step 4 is next.


Step 4 work checkpoint: the shared calculator in `nucleus::transfer::application` now evaluates restricted arithmetic with exact decimals, up to 18 places and half-even rounding. Its two focused tests passed, covering fractional full/split totals, nonlinear cumulative changes, rounding ties, division by zero and overflow. This does not complete exact settlement: the existing action/storage/projection paths still use floating-point amounts and must be converted or backed by exact stored values. Next complete owner-local incoming/outgoing policy storage and review, use the same policy/calculator for local settlement, handoffs and assumptions, then cover unit conversion and recorded-effect compensation. Do not mark this step complete on the calculator tests alone.

1. **Choose what each person can see.**

   **Workflow:** Ana offers a bike using a private inventory Record. She publishes the item title “City bike” and a separate description. Beto can see those fields and the offered count, but cannot see the inventory UID, head, body or stock balance. A courier can see only the agreed delivery details. Other viewers cannot see the courier's identity or exact collection address.

   **Build:** Store item title and description in the public terms, with optional private source bindings. Add recipient rules for item text, source identity, parties, quantities and locations. Use one disclosure policy across Protein reads, filters, subscriptions, discovery, history, proofs and both hosted and replicated delivery. Required participants must be able to review their own obligations before agreeing. A revision or action payload must not carry fields hidden by its projection.

   **Native UI:** Add field visibility controls and “View as…” previews to the composer and detail view. Show hidden fields as unavailable; never substitute zero or an empty identity that appears authoritative. Let people inspect permitted signed history and authorship for public Transfers.

   **Done when:** A saved scenario checks every viewer's query result and received package before and after a disclosure change. Hidden values must not leak through item links, search results, counts, history or evidence. Revocation stops future access and updates; the UI does not promise to erase copies already received.

2. **Make every exchange between several people explicit.**

   **Workflow:** In one Transfer, Ana gives Beto two crates; Carla gives Ana one crate and Beto one crate. Each person accepts their part. Confirming and settling Carla's crate to Ana completes only that exchange. Repeated items with the same quantity still have distinct recipients and progress.

   **Build:** Give each exchange a stable identity, public item, giver, receiver, amount and unit. Bind local Records privately on each participating Organ. Validate allocations and reservation totals when terms change. Preserve these identities through agreement, activation, partial settlement and corrections. Replace ambiguous counterparty inference with the chosen route. A split contribution can be several explicit exchange rows.

   Add a private observer link to an outcome the viewer is allowed to read. For example, Dora links her own “road ready” commitment to a public road-repair Transfer without joining it or appearing among its parties. The link records the required outcome and permitted evidence; step 7 completes the shared dependency-agreement behaviour. Observation alone never changes the repair Transfer or Dora's resource quantities.

   **Native UI:** Use “From / To / Item / Amount” rows and per-exchange progress. Offer a private “Use this outcome” action on visible Transfers. Keep observer links out of the observed Transfer's participant list and messages.

   **Done when:** A scenario with Ana, Beto and Carla on separate Organs completes the three routes independently. A refused or revised route cannot activate under old agreement. Dora can observe permitted evidence without joining; guessing a private Transfer UID grants nothing. “Anonymous observer” means absent from participation records, without claiming network anonymity.

3. **Complete remote donations and trades, with optional Simulation and shared checks.**

   **Donation workflow:** Ana starts with 30 apples and offers 10 to Beto, who starts with zero. Ana sends the proposal. Beto accepts from his own Lince, binds his own apple Record, and both agree to the same revision. Ana confirms delivery; Beto confirms receipt. Each reviews their local changes. Completion leaves Ana with 20 and Beto with 10, with no return contribution required.

   **Trade workflow:** Ana starts with one bike and no money; Beto starts with no bike and 100 money. They agree to bike Ana → Beto and 10 money Beto → Ana. Both routes have separate claims and settlement. Completion leaves Ana with zero bikes and 10 money, and Beto with one bike and 90 money. If only the bike is delivered, the money route remains outstanding and the whole trade is not shown as settled.

   **Optional Simulation workflow:** Ana has 10 apples. Her existing enabled Karma consumes three by Friday. Before sending anything, she selects a draft donation of six, assumes delivery before Friday and adds a private check: apples >= 2 after every committed change until Friday. Running it shows one remaining and a failed check. An optional comparison shows seven without the donation. She changes the hypothetical donation to five and reruns; two remain and the check passes. She explicitly updates the live draft and sends it. Beto can agree immediately using the normal agreement controls, or run the proposal against his own apple Record and check for five arriving by Friday. Sharing chosen results or suggesting different terms is optional. Ana's live stock stays 10 until ordinary live actions change it.

   Ana can save the same check in a named set and reuse it in a Karma-only run. After agreement or partial delivery, either party can simulate a delay or another proposal without changing that agreement. For three parties, each can simulate their own desired quantities independently; all may also simply agree. No participant's run creates a new approval requirement for the others.

   Implement step 3 in this order, completing backend, native controls and focused acceptance for each substep before continuing:

   - **3a — Shared checks:** Extend Simulation's check definitions, saved sets, evaluation scheduling, stop/continue controls and coverage/results as specified above. Add the Checks panel, no-check runs, immutable run manifests and generic correctness/performance cases. Prove a custom quantity check on an ordinary Record changed by existing Karma before connecting Transfer assumptions.
   - **3b — Independent participants:** Complete controlled remote message/action adapters through the production handlers. Add typed Transfer scenario inputs and owner-controlled local bindings, using step 2's exchange identities and step 1's disclosure rules. Prove one donation with each participant acting on their own Organ and copied database.
   - **3c — Independent runs:** Support single scenarios, optional comparisons from the same snapshot, remaining-amount assumptions, source-version labels and private timelines. Allow an explicit shared scenario or selected result through ordinary discussion/delivery; show the exact fields being shared and apply disclosure rules. A recipient supplies their own bindings and check choices. Sharing adds no agreement state.
   - **3d — Native workflows:** Complete the ordinary composer, inbox, counteroffer, donation and trade paths. Expose Simulation from its Castle and relevant Record/Karma/Transfer views, with optional Inspect/Compare actions. Keep Agree/Decline/Propose change directly available under their ordinary rules. Receiving a proposal neither starts Simulation nor requires a run.
   - **3e — Combined acceptance:** Prove donation, trade and three-party workflows both without Simulation and with optional runs at different stages. Run the shared-check acceptance cases with Transfer effects, including delayed incoming resources and transient shortages. Complete remote retry/recovery cases before step 4.

   Use the current default accounting first; step 4 adds custom outgoing effects.

   **Build:** Exercise the existing remote command, settlement handoff and application acknowledgement paths. Complete missing integration between local record bindings, remote decisions, public progress and corrections. Cover addressed invitations first, then public/proximity offers and claiming an OPEN promise. Keep one authoritative result per exchange and distinguish pending local application from confirmed completion. Do not represent several Organs' separate writes as one atomic transaction.

   **Native UI:** Show ordinary agreement levels and direct “Agree,” “Decline” and “Propose change” actions. Offer “Simulate” independently in draft and existing Transfer views; it opens the Simulation Castle with selected terms as editable assumptions. The run shows private timelines, selected checks and coverage, with optional “Compare” and “Share selected results.” No per-party simulation badge or checklist gates agreement. Keep the active Person/Organ, cancellation, retry, dispute and pending local application visible. Reuse the shared pending-offers surface where available; its store already includes Transfer invitations.

   **Simulation integration:** Extend the existing action adapter and controlled delivery events to carry remote commands, policy changes, hosted reads and application acknowledgements through the production handlers. The current allowlist omits several of these actions and the fixture performs the sale on one Cell. Each participant must act with their own identity; the test must not switch everyone onto the origin Cell to bypass remote work.

   **Done when:** Donation and trade pass in hosted and replicated modes, including an OPEN claim, refusal, counteroffer, partial delivery, changed terms, disconnect before acknowledgement, duplicate messages and restart. Revoked or stale commands fail without resource changes. A public progress view agrees after reconnection and each private balance has exactly its expected value.

   The optional-run cases must also prove: six fails Ana's check and five passes; both runs include the same existing Karma; Beto runs on his own snapshot; private mappings and failures remain private unless explicitly shared; changing live data marks old results out of date while preserving them; no run changes live resources or agreement levels. A payment before an assumed incoming payment fails a throughout minimum-balance check even if the final balance is positive. A failed, incomplete or no-check run has an accurate result and never changes human agreement.

   In a three-party case, one party simulates before sending, another agrees without simulating and the third runs after agreement. Agreement succeeds through ordinary levels in every case. Running, changing a check set or sharing a result leaves agreement unchanged; explicitly submitting changed public terms resets it. Simulation remains available after partial settlement and counts only the remaining obligation.

4. **Let each owner choose their private resource changes.**

   **Workflow:** Beto publicly pays Ana 10, but chooses to deduct only 5 from his private Money Record. He previews both figures. Ana's agreed receipt remains 10. Settling 4 and then 6 deducts 2 and then 3 from Beto, giving the same total as settling 10 once.

   **Build:** Extend application policies to givers as well as receivers and support the same policy locally and remotely. Outgoing changes currently use fixed `-incoming()`; remote receipts currently use the Cell default. Keep the public amount, private formula, formula version, units and affected Record distinct. For partial settlement, apply the difference between the new cumulative result and the amount already applied. Define exact decimal and rounding behaviour before expanding quantity calculations; remove floating-point ambiguity from affected settlement paths.

   Private policy edits invalidate open settlement previews and cached forecasts for current data, while retaining historical run results with their original inputs. They do not rewrite settled changes, reset public agreement or require Simulation. Run the same cumulative-effect calculator in the scenario and live settlement, so reviewing a deduction of five cannot later produce ten under unchanged inputs. A correction reverses the recorded local effect from the original application, rather than recalculating it using today's formula. Invalid formulas or incompatible units refuse the operation with an explanation.

   **Native UI:** Show “Agreed amount” and “Changes to my Records” together in the local review. Provide an ordinary amount/ratio control and an optional formula editor. Only the owner sees their accounting details.

   **Done when:** Full and split settlement produce the same private total, including fractional values and supported unit conversions. A stale preview, duplicate request or correction cannot double-apply an amount. The other party sees 10 throughout and never receives the private deduction of 5.

5. **Make reservations and surplus reflect remaining work.**

   **Workflow:** Ana has 30 apples and commits 10. Show actual 30, reserved 10 and surplus 20. She eats one: actual 29, reserved 10, surplus 19. She settles delivery of four: actual 25, reserved 6, surplus 19. Agreed cancellation of the remaining six releases them: actual 25, reserved 0, surplus 25.

   **Build:** Derive reservations from remaining local obligations at the configured reservation point. Account for partial settlement, custom accounting, cancellation, correction and alternative offers without counting the same commitment twice. Show `actual - reserved` even when negative; show a separate nonnegative amount if the UI needs a “can offer now” value. Keep negative quantities valid for Needs.

   Show current actual, current reserved, current surplus and “if this scenario completes” as distinct values. For a simple future case, actual is 10, four are reserved for delivery tomorrow, and five are expected the following day: current surplus is six, the scenario shows actual six after delivery and 11 after receipt. Agreed incoming resources are still future assumptions until settled; they cannot fund a hard reservation today.

   Reuse step 3's single-run timelines and optional comparisons. Add a selected proposed Transfer to the existing commitments, or replace its prior revision, rather than adding the same promise twice. Include only the chosen alternative and the unsettled remainder. Show partial, delayed and failed-delivery variants by changing their assumptions and rerunning; no separate forecasting formula. Unknown units, missing evidence or unsupported active Karma effects make the affected result incomplete. The user may attach final or throughout quantity checks to any scenario without making them a condition of live agreement.

   Detect overcommitment. Where the owner chooses a hard stock limit, enforce it during ordinary writes on the owning Organ, including concurrent reservations; a Simulation check alone cannot enforce that limit. Otherwise show the shortage explicitly.

   **Native UI:** Add a balance breakdown with links to commitments, a selected scenario and a future-date view. Selecting a projected change opens its Transfer and assumption. Mark stale/incomplete projections visibly. Let the user manually revise a surplus offer. Automatic offer resizing remains Karma work.

   **Done when:** The apple workflow matches every intermediate figure. Add competing reservations, delayed incoming stock, custom outgoing accounting, corrections and disconnection of a second Cell belonging to the same Organ. Replication must not create an extra reservation or allow stale authority to silently spend the same hard-reserved stock.

6. **Derive a parent Transfer's agreement from its children.**

   **Workflow:** A dinner Transfer has required food and transport children. Food agrees first; the parent still waits for transport. Once both agree, the parent is agreed. If transport terms change, the parent waits again. Hiding the transport child from a viewer does not change the parent's result.

   **Build:** Store which children are required; use all children as the default required set. Derive agreement from those children plus any direct obligations. An empty parent with no agreed obligations is not agreed. A required withdrawn or disputed child blocks readiness until the plan is revised. Existing settled evidence remains visible after later corrections; do not erase history when current readiness changes.

   Evaluate the complete required set on the authoritative backend, then filter explanations for the viewer. Current hierarchy rollups use visible query rows and cannot serve as this authority. Adding/removing required children changes the parent terms. Parent agreement does not sign or settle children on anyone's behalf.

   A saved group simulation records possible outcomes and selected checks; it has no agreement status. It cannot mark the parent agreed while a required child lacks real agreement. A parent can select its children for a scenario without creating another copy of their obligations, and can agree without anyone running that scenario.

   **Native UI:** Show required children, current agreement and the next blocker. If details are hidden, show only an allowed “Waiting on a required part” explanation.

   **Done when:** Nested parents produce the same permitted result whether queried alone, in a filtered list or through another Organ. Empty parents, changed children, disputes, withdrawn children, unavailable remote evidence and hierarchy cycles have explicit tested outcomes.

7. **Derive dependency agreement from the required upstream outcome.**

   **Workflow:** A delivery plan requires the food Transfer to be settled. Signing the delivery plan does not satisfy that condition. Food becoming merely agreed is still insufficient. Once food is settled, the delivery plan's dependency agreement becomes ready from that evidence; this status does not depend on collecting local agreement signatures. Authority checks still apply before anyone activates a delivery or changes resources.

   **Build:** Reuse `transfer_dependency`; the record's `transfer_interaction` wording names a table the current implementation does not use. Give Transfer dependencies agreed/settled conditions based on authoritative Transfer results. Keep promise-level conditions for promise dependencies. Current dependency mode also requires local agreement levels and approximates upstream Transfer state using its promises.

   Keep derived agreement separate from permission to act. Refresh it on upstream revision, settlement, correction, dispute and access change. Use signed, revision-bound evidence for remote outcomes. Unavailable or stale evidence stays unresolved. Reject dependency cycles and evaluate parent/dependency chains consistently. A future simulated outcome must not satisfy a live dependency.

   Connect the private observer links from step 2. Dora's outcome-based “road ready” commitment becomes fulfilled from authorized evidence of the completed repair, without making her a repair participant or inventing delivery/receipt claims from her. Record the evidence behind that fulfilment. Changing her local quantities still needs an authorized action.

   **Native UI:** Show the required upstream outcome, current result and a permitted explanation of what is missing. Reuse this view for private observer links and parent blockers.

   **Done when:** Agreement and settlement requirements behave differently, upstream evidence alone determines dependency agreement, and separate action authorization remains enforced. Dora's commitment is fulfilled only by the required observed outcome. Test branching chains, a seven-Transfer chain, cycle attempts, remote revisions, loss of access and corrections after downstream work has already begun. Preserve that work and expose the changed dependency instead of silently undoing it.

8. **Satisfy several private Needs through one public item.**

   **Workflow:** Beto has Bike = -1 and Transport to work = -1. He publishes one item: “Bike broke; need to get to work tomorrow.” Receiving a bike applies +1 to both private Needs. Receiving a ride applies +1 only to Transport to work. The contributor sees one agreed outcome and no private Need identities.

   **Build:** Extend the private application policy to a list of Record changes bound to a specific accepted outcome. Preview and apply the local group in one transaction with permission checks on every Record. Separate quantity effects from fulfilment links where needed; do not pretend that a bike and a journey share a unit. Preserve the effect list for partial settlement, replay and compensation.

   Use the existing alternative/first-completes mechanism where applicable. If a ride settles first, a later bike must not satisfy the already-met Transport Need twice. Reservations and surplus from step 5 must use the outstanding private effects after either result. Alternatives that remain physically active require an explicit cancellation or revised agreement.

   **Native UI:** Add a private “This outcome meets…” editor listing each Need and its proposed change. Reuse step 3 to compare the bike and ride as separate alternatives against the same snapshot and local checks, then choose or revise the accepted solution. The preview must show Bike/Transport values of 0/0 for the bike and -1/0 for the ride.

   **Done when:** Bike and ride alternatives yield the stated values, no hidden mapping is delivered, and retry changes nothing. If any local permission or reviewed precondition fails, none of the group applies. Correcting a fulfilment preserves later unrelated edits and reverses only its own recorded effects.

9. **Show temporary fulfilment and a manual return workflow.**

   **Workflow:** Beto borrows a bike from Monday 09:00 until Thursday 09:00. The accepted loan meets the mapped Needs for that interval. The timeline shows them becoming unmet at Thursday 09:00. Beto can publish that future Need now and create the linked return proposal now. Returning the bike still requires ordinary delivery, receipt and settlement actions.

   **Build:** Add explicit start/end times and an origin link for temporary fulfilment. Use the shared Simulation/Calendar projection to show the interval and the expected return. The end is exclusive. Separate expected availability from physical possession: an overdue return must not falsely put the bike back in the lender's stock. Expiring a projected fulfilment does not silently edit live quantities.

   Extend a loan through revised terms and renewed agreement. An early return records the actual return and shortens the remaining fulfilment. Recalculate the affected Need projections without duplicating the original receipt or return.

   **Native UI:** Show the loan interval, unmet Need after expiry, linked return, overdue status and an action to simulate an extension or early return. “Propose return” and “Publish future Need” are manual actions here.

   **Done when:** The three-day loan, early return, extension, missing return and timezone boundary all display the correct interval and balances. Advancing Simulation alone never produces a live return. Frequency-driven proposals, reminders and automatic reposting remain excluded.

10. **Close Transfer with shared scenarios and native workflow checks.**

    Add each preceding scenario with its feature; this final step runs them together. Reuse the finished Simulation controls and saved evidence. Extend its checks only where Transfer needs an independent assertion for visibility, agreement, reservation, settlement or local application. Check intermediate transitions as well as final balances.

    **Native workflow:** Verify step 3's ordinary send/agree/counteroffer flow with every later feature, including private accounting, scenario quantities, grouped Needs and loans. Exercise optional Simulation before sending, during negotiation, after agreement and during partial delivery. Verify creating a check, saving/reusing a set, selecting evaluation timing, disabling checks and inspecting findings in the existing Simulation Castle. Remote participants' private results are visible only if they chose to share them. Any live proposal or agreement is submitted through ordinary checks; there is no “merge simulated facts” operation.

    **Campaigns:** Add Transfer families to the existing generator: donations, trades, multiple parties, observers, custom accounting, competing reservations, nested agreements, dependency chains, combined Needs and loans. Begin with four Linces and 100 Transfers; extend to 32 Cells and 1,000 Transfers. Include both independent Organs and multiple Cells belonging to one Organ. These are acceptance workloads to measure, not claims about current capacity.

    Vary message delay, duplicates, ordering and loss; offline intervals; clock offsets; restart after a commit but before acknowledgement; concurrent term changes; permission revocation; partial settlement and corrections. For every legal retry, require at most one effect. Refused stale or replayed messages may be expected, but each expected refusal must name the event and reason; an unexpected refusal fails the case.

    Check public agreement and progress consistency after recovery, each owner's expected private balance, remaining reservations, hidden fields and bounded pending work. Also check saved source revisions, definitions and evaluation schedules; historical/out-of-date result labels; isolation of local snapshots; authorized sharing; identical comparison assumptions for unchanged work; and agreement between simulated effects and live effects under the same inputs. Runs and their results must leave ordinary agreement unchanged. Private accounting means cross-Organ local balances need not equal public amounts or each other. Replicas of the same authorized state should converge.

    Compare full, selected, sampled and no-check configurations on the same campaign seeds with continue-on-finding enabled. Their domain events and final state must match. Record execution/evidence/checking costs separately and confirm incremental checks avoid repeatedly scanning all prior events. Replay the exact original check manifest; a fresh run is required to test a different set. A sampled success only covers its samples, and a completed no-check run stays unverified. Run the complete required check set in the regression suite regardless of exploratory user settings.

    **Done when:** All fixed cases and native workflows pass; generated failures retain a reproducible seed, evidence and reduced case. Record runtime, memory, message counts and recovery steps for both workload sizes, then set regression limits from those measurements. Run focused correctness/security tests and `cargo check` for changed crates with warnings treated as errors. Mark unfinished or budget-exhausted cases incomplete, never passing.

Creating new Karma-triggered proposals, recurring purchases, automatic visibility expansion, automatic surplus resizing and Frequency-driven returns is outside this plan. Running the user's existing authorized Karma in any selected scenario, and adding shared checks that Karma, Transfer and other features can use, is included. Signed history supports inspecting public conduct; a new reputation score or marketplace is not required to finish these Transfer points.
