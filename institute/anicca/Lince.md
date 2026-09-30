Other Simulation follow-ups: broader Lingua declarations; controlled external responses, disk failures and crashes; more varied seeds and concurrent actions; blob contents saved with runs; real direct-connection tests across NATs, relays and mailboxes; focused Cell/event editing and Calendar links to explanations. Projection follow-ups include additional filters and permitted viewers, time boundaries, larger workloads and replacing older approximations where the shared runtime has coverage. Karma's authorized Program execution and older recurrence tests need a separate completion review. General Record proposals, versions, history and transclusion should reuse the document machinery. Restore during a run and finer cache invalidation remain deferred until needed.

## Karma implementation

The approved plan from the [Rule Thoughts discussion](Tasks.lingua) is implemented. Sections 1, 3, 5, 6, 7, 10 and 9 were completed in that order, with Transfer automation last. No active implementation task remains in this plan. Sections 4 and 8 remain deferred; section 2's exclusions remain in force.

Completed tasks and obsolete compiler checkpoints have been removed from the pending plan. The following record preserves the implemented behavior, architecture and acceptance evidence for later work.

### Completed scope

| Order | Agreed section | Implemented behavior | Status |
| --- | --- | --- | --- |
| 1 | 1 | Numeric extension readings, stable bindings, reactive updates and native choices | Complete |
| 2 | 3 | Bounded Simulation execution and causal feedback/recurrence reports | Complete |
| 3 | 5 | One-time and paired range schedules, recovery, outcomes and native editing | Complete |
| 4 | 6 | Copied Simulation of unsaved Rule edits and compact results | Complete |
| 5 | 7 | Fiote uses ordinary Karma actions, previews and recorded explanations | Complete |
| 6 | 10 | Cleaning Room import and completion through Instinct | Complete |
| 7 | 9 | Transfer readings, guarded state assignments, stages and optional retreat recommendations | Complete |

### Implemented behavior and architecture

**Extension readings.** `extension(@record, "namespace", "property")` reads a stored JSON number into the existing exact decimal evaluator. Namespace and property are explicit literals. Missing or nonnumeric data, deleted Records and denied access are explained errors. Condition bindings retain Record UIDs after renames. Local and synced extension changes wake indexed Rules without requiring a quantity change. The Karma editor offers readable properties and value previews.

**Bounded Simulation.** Horizon, wall-time and Rule-evaluation limits apply inside reaction chains. Shared execution controls retain committed evidence when stopping or pausing. Causal Rule/Fact links distinguish immediate feedback, feedback that settles and recurrence across advancing time; rising values can still form feedback. A cycle stops a run only when the selected restriction requires it. Reports state the actual endpoint, first broken restriction, contributing Rules, stop reason and coverage. These are observations within the selected run, not a proof about unbounded execution.

**Schedules.** One future change uses an ordinary Frequency with one occurrence and a Rule; a range uses two, edited as one group. Migrations 0115–0116 persist revisions, intended dates, boundary purpose and outcomes independently of Simulation caches. Creation, replacement and cancellation invalidate old work transactionally. Successful work retires after its actions have outcomes; failed work remains inspectable and retryable with its original identity. Recovery suppresses an already-expired range's start. A configured end-to-zero writes the Record's current quantity to zero, including intervening edits. Native controls resolve local dates with pinned IANA providers or elapsed durations and expose creation, editing, cancellation and outcomes.

**Unsaved proposals.** The native Simulate control sends current draft fields and revisions through ordinary save validation in a copied database. `simulation::karma_preview` uses the shared World, checks and execution controls. It reports final values, restrictions, cycles, stopping point and incomplete coverage without saving the live edit. Draft/source fingerprints mark old results stale. Supplied inputs run through ordinary handlers; unavailable external adapters and pending remote origin work remain explicit. Selected saved check sets are reused. Actual Person signing keys needed by Transfer previews are copied in memory and never serialized into scenarios or reports.

**History and Fiote.** Migration 0117 stores immutable readings, Threshold decisions and effect outcomes for applications/refusals. Current access is checked before exposing inputs. Native history renders those recorded decisions. Fiote's existing `lince_describe`, `lince_query` and `lince_action` paths expose the same typed Actions and Protein queries as the interface, including proposal previews and Transfer automation. The native catalog explains the expressions, revisions, request identities and queued/accepted distinction. Ordinary Session permissions and signed request verification remain authoritative.

**Instinct habit import.** Migration 0118 tracks stable tutorial objects and original definitions. Preview/import creates Cleaning Room, its daily Frequency and a Rule that sets it to -1; its checkbox sets it to zero. Reimport preserves quantities, user edits and pauses, and reports naming conflicts. Scheduling begins at the selected upcoming local occurrence and does not accumulate missed days. The pinned timezone provider is selected before decoding each stored cursor; read projections do not create configuration rows. Instinct links the imported objects to Karma.

**Shared paths.** Actions pass through `engine::actions::Action`, authorization, the editor/store and `rule_runtime`. Linked fields use `store::karma_fields`; exact Conditions, Gate and Carry remain in nucleus. `RuleIndex` tracks bound source UIDs and Frequency occurrences. A firing commits quantity Facts atomically and queues other effects with event/Rule/revision/position identities. Save, pause and delete invalidate stale queued work. Native models live in interface and controls in desktop; the unplugged web crate is outside this implementation.

### Transfer automation

The existing features described in [Transfers.md](Transfers.md) are reused: disclosure, exchanges, private accounting, reservations, parent/dependency agreement, grouped Needs, loans, signed remote delivery and ordinary activation. Karma acceptance does not close that document's separate workflow/campaign acceptance.

**Readings and coherent state.** `agreement_level(@transfer, @person)`, `agreement_changed_at` (UTC milliseconds), `agreement_age` (elapsed seconds), `transfer_revision`, `transfer_active`, `transfer_published` and `transfer_ready` read the authorized Transfer projection. Protein adds the typed snapshot when `fields` requests `karma_state`. One evaluation shares one snapshot. Stable bindings retain Transfer/Person identities after renames. A remote Rule keeps its logical Transfer target and uses its acting local Person as the storage anchor; it does not require a synthetic Transfer Record. State comes from actual agreement/promise/publication data.

**Exact targets.** `AssignTransferAgreementLevel` accepts only whole levels 0, 1 or 2. Migration 0119 stores exact-target receipts and the last agreement-change identity. Direct 0→2 or 2→0 uses the existing adjacent transitions within one transaction, preserves actual history and rolls back every step if any step is refused. Same-target assignments and exact retries produce no extra event, Fact or reservation. Conditions can calculate a target; the Threshold determines whether to apply it. Fixed zero remains a valid retreat.

**Deferred guards.** Effects capture the original terms revision, own participant/change identity and the Transfer fields actually read. Returning to the previous level does not restore an old guard. Workers and first remote dispatch recheck the parent Rule/revision, pause, current authority and input visibility. The domain transaction checks the source state again before mutation. An unrelated unread participant change does not invalidate an own-only stage. Original effect/request identities survive retries.

**Publication and fulfillment.** `@trade: publish(@me)` uses an ordinary signed whole-draft publication while preserving private bindings and existing promises/dependencies. `@trade: activate(@me, @promise, "purchase-1")` selects an explicit fulfillment. Its stable identity derives from Transfer, Person, promise and key, independently of the firing request. Repeating either consequence reaches the same state/fulfillment. Another fulfillment requires another explicit key. Existing readiness, participant, reservation and writing-Cell checks apply.

**Stages.** `@trade: agreement(@me, 2, 3d)` schedules an ordinary one-time change from the actual preceding agreement change. Migration 0120 stores parent Rule/revision, effect position, original guard, due time and causal occurrence in the same transaction as that schedule. Repeated evaluations reuse that stage. Parent editing, pausing or deletion invalidates old children; resuming cannot revive them. A preceding change made after downtime still gives its full delay before the next stage.

**Remote commands.** Migration 0121 links real signed commands to their Rule/effect origin. Unsent work can be cancelled; issued retries retain their identity and receipt. Command acceptance/rejection updates schedule outcomes and recorded evidence. The native interface explains that pausing stops future firings and unsent commands, while an issued command may still finish. Signed authorized projections wake readers only when state actually changes. Causal links use the agreement-history Fact identity carried by the signed projection; redacted deliveries do not supply raw Facts.

**Native controls.** Karma supplies stable-ID reading/consequence choices and uses those same consequences in single/range schedule forms. `InspectTransferKarma` returns the acting Person's related Rules, fixed/current calculated targets, known pending dates and command status without mutation. Transfer Castle offers guarded exact levels and, after a confirmed decrease on unchanged terms, selected/all pause, keep enabled and open/edit options. Retreat and queued acknowledgements never automatically pause Rules. Opening a related Rule preserves another Castle's unsaved draft. Paused Rules remain visible and editable.

**Simulation evidence.** `RuleStep.transfer_changes` records authorized before/after state, actual origin Cell and observed/virtual times, including zero-quantity Transfer Facts. Delayed evidence updates the contributing cycle samples; loading validates cells, identities and bounds. World records automatic command outcomes as DatabaseEffect under the real command UID. Its scheduler watches the existing deadline-change epoch and ticks again when effects create a schedule. Scenario-only `@{capture}` references bind actual UIDs before ordinary Rule actions. Final values include typed Transfer state, and a copied single-Cell preview awaiting an unavailable origin remains incomplete.

To advance one level and stop at 2:

```text
Condition: 1 * (agreement_level(@trade, @me) == 0) + 2 * (agreement_level(@trade, @me) == 1)
Threshold: > 0
Consequence: @trade: agreement(@me)
```

Agreement changes can wake that Rule again and advance immediately to 2. Multiplying its Condition by `freq(@step)` advances once per occurrence. A selected delay uses the stage consequence above. To retreat only from 1, use Condition `agreement_level(@trade, @me) == 1`, Threshold `!= 0` and fixed consequence `@trade: agreement(@me, 0)`.

### Acceptance evidence

Final acceptance completed 2026-09-30 12:46 UTC. Warnings-denied native `cargo check --tests` and combined test compilation passed. The final runtime suites passed:

| Suite | Passed |
| --- | --- |
| Transfer backend guards, inspection, Rules, stages, states and targets/readings | 23/23 |
| Native Transfer automation, including an actual Cell pause and preservation of an unsaved draft | 5/5 |
| Native Karma editor, schedule templates, preview and history | 14/14 |
| Transfer model templates and retreat detection | 2/2 |
| Copied Transfer proposals, including one step per Frequency, fixed zero, fractional refusal, publication/fulfillment and delayed stage | 5/5 |
| Bounded Rule Simulation and replay | 14/14 |
| Proposal/tool/access regressions | 15/15 |
| Hosted/replicated receipt revocation, completed retry/recovery and exact replay | 2/2 |
| Existing sealing regressions after narrow shared compile corrections | 16/16 |

Earlier accepted section 9 suites include seven remote command cases with exact replay, loss/restart/duplicate delivery, stale first dispatch, unsent/issued pause, retreat with/without pause and actual contributing Rule/origin Cell assertions; seven causal checks including two-hop delivery; and three schedule/replay cases. Section 10 passed six backend imports, 21 ingestion regressions, four native workflows, shared copied Simulation/replay and the timezone provider/read-only regression. Prior section checkpoints also accepted extension reads/reactivity, schedule recovery/access, native limits and proposal workflows, immutable history and authorized Fiote tools.

The two receipt regressions exposed an ordinary authorization preflight that looked up a revoked remote exchange before its receipt-aware handler. It now authorizes the original affected Records for completed receipts and uses the existing inactive-delivery check for new receipts. Both cases passed in hosted/replicated modes with exact replay. This is a correction to the reused Transfer path.

Logs are under `.cache-lince-karma`: `final-native-runtime-acceptance.log`, `final-coherent-test-compilation.log`, `final-domain-runtime-acceptance.log`, `final-editor-frequency-test-compilation.log` and `final-editor-frequency-runtime-acceptance.log`. The shared source fingerprint changed during other agents' mailbox work; final selected runtime suites used the combined compiled executables. Narrow mailbox/seal/form compilation corrections were verified by compilation and the existing sealing tests. The editor's old toolbar/input assertions now account for the preview controls while retaining its editing/deletion checks.

### 2. Exclusions

Thresholds retain the ordinary number-and-gate behavior. Debounce, cooldown and rate limiting are outside this implementation. There is no .ics work, central-tendency report or proof about unbounded Rule execution.

### 4. Signal work — deferred

**Your idea:** A command we call saves its result in a Signal. A request we receive also saves a Signal sample. Conditions then read the saved value.

**Concern:** A command can fail after a previous successful reading. A sender can retry a request. Neither should turn a failure into zero or run the same downstream work twice.

**Recommendation and reason:** Use one sample-recording action for both sources. Keep the last successful value and its source/time, plus the latest error. The value remains available after a later failure; a Signal that has never received a value is unavailable. Values do not expire automatically. This preserves data while making failure visible.

**Future work:** Complete manual command calls and incoming HTTP samples through that shared action. Reuse Facade's HTTP service and ordinary permissions. Add revocable device/service credentials limited to selected Signals or saved commands. Keep the existing browser origin checks.

Offer two operations: send a sample to a selected Signal, or request a selected saved command whose result feeds its Signal. The request supplies data; command execution uses the saved command configuration. Check the caller's permission for that target.

Give each submitted sample or command request an identity. Receiving the same request again returns its recorded result. Different requests remain different samples, even when their numbers match. Save the sample and the work it wakes before acknowledging receipt.

Add native controls to choose the source, configure a command, call it, grant/revoke incoming access and inspect the value/error. Check invalid data, refused or revoked access, wrong targets, retries and restart. Simulation supplies sample data and command results through the same path.

Keep these ideas documented. Signal features, HTTP endpoints, credentials and their new controls are outside the current implementation plan.

### 8. Keep board automation for later

**Your idea:** At a chosen time, Karma can show Sands, Castles or areas inside a pinned area that arranges them. Examples include notifications, a task list and a scheduled call.

**Future recommendation:** Save a request naming the destination area, saved template and Records/settings to show. One firing produces one displayed item, including after reconnect. Closing the item and completing its task are separate actions.

A Protein area can show task titles whose quantities are negative, with a checkbox that sets a task to zero. Scheduled calling remains a configurable action; displaying its Castle is a separate presentation choice. Keep these ideas documented and outside the present implementation work.
