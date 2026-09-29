
The remaining work is now clear enough to put in order. The main gaps are per-field disclosure, explicit routes between several parties, observer links, outgoing and multi-Record accounting, fuller surplus projections, temporary fulfilment, and derived agreements. I’m keeping the plan in the Transfer task record’s order, with native UI work and Simulation checks attached to each step.
The remaining work is mostly completing and connecting existing Transfer features. I compared the Transfer description (institute/anicca/Lince.lingua:559), Transfer tasks (institute/anicca/Tasks.lingua:269), and Simulation record with the current backend, native UI, and tests.

Several unchecked points already have substantial support:

 Area                   What exists                                                                         What remains
━━━━━━━━━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 Transfer workflow      Native editor, invitations, counteroffers, signed agreements, claims, partial       Complete these flows between independent Linces
                        settlement, disputes and corrections
─────────────────────  ──────────────────────────────────────────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────────────
 Visibility             Whole-Transfer visibility and some private accounting fields                        Per-field disclosure and independent public item descriptions
─────────────────────  ──────────────────────────────────────────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────────────
 Several parties        Multiple participants and matched promise pairs                                     Explicit giver/receiver routes, ambiguous matches, and observer fulfilment
─────────────────────  ──────────────────────────────────────────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────────────
 Resource accounting    Reservations, availability, receipt formulas                                        Custom outgoing changes, multiple local Records, and fuller projections
─────────────────────  ──────────────────────────────────────────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────────────
 Agreement              Dependency checks, hierarchy summaries and several agreement modes                  Authoritative parent agreement and the dependency behaviour described in the
                                                                                                            record

The plan below follows the Transfer task record’s order. Each step includes backend work, proposed native UI, and checks using Simulation.

Finish visibility for Transfer fields and items.

Give each public Transfer item its own title and description, independent of the source Record. Add disclosure rules for source identity, parties, quantities, locations and item fields, with a preview of what each recipient can see.

Enforce those rules in Protein queries, subscriptions, discovery, delivery packages, history and signed evidence. Filtering the screen alone is insufficient. Preserve verifiable authorship without sending hidden terms inside another payload. Public history should expose only permitted claims and outcomes.

UI and completion: Extend the Transfer editor with disclosure controls and recipient previews. Simulate different recipients viewing the same Transfer, including after a revision or permission change. Verify that hidden values and identities cannot be recovered from linked Records, filters, totals or delivered evidence. The current projection explicitly reports field overrides as unsupported (crates/protein/src/lib.rs:7949).

Finish coordination between several parties, including observers.

Add explicit giver and receiver assignments to each exchange. Support A giving to B while C gives separately to A and B, including repeated items with identical quantities. Validate allocations, ownership and agreement when the terms are written. Current activation infers the counterparty and rejects ambiguous matches (crates/protein/src/lib.rs:4286).

Add private observer links: someone can use an allowed outcome from another Transfer to satisfy their own commitment without joining that Transfer or appearing in its participant list. Observation must not grant additional access or authority to change anyone else’s resources.

UI and completion: Show editable “from → to” rows and a private “depends on this outcome” control. Simulate three independent Organs, split deliveries, one refusing participant, and an observer. Track progress separately for every exchange; finish observer agreement semantics in step 7.

Complete donations, trades and private resource accounting.

Reuse the existing donation and sale workflows. Exercise proposal, acceptance, agreement, delivery, receipt and settlement on each participant’s own Lince, with their own Records and signing authority. Close any gaps in remote actions, retries and local application.

Extend private accounting to outgoing quantities. The record’s example—publicly paying 10 while privately deducting 5—is currently blocked by the fixed outgoing formula (crates/protein/src/lib.rs:3948). Keep public obligations separate from private changes, and apply the same reviewed policy locally and remotely. Bind settlement previews to the exact policy version and remaining amount.

UI and completion: Show “agreed amount” beside “changes to my Records.” Test donations without a return contribution, two-way trades, custom deductions, partial settlement, stale previews and corrections. Repeated delivery must never apply a change twice. Cover fractional quantities, units and the remaining floating-point calculations.

Finish reservations and surplus projections.

Calculate committed amounts from outstanding obligations, accounting for partial settlement, cancellation, correction and private resource changes. Distinguish what exists now, what is reserved, what is available now, and what is expected later. Current availability logic (crates/protein/src/lib.rs:1952) sums promise amounts and clamps surplus to zero.

Keep quantity − reserved_quantity visible, including a shortage. Show confirmed incoming resources separately until the scenario assumes their arrival. Explain which commitments contribute to each figure. Detect competing promises against the same stock; enforce any chosen hard reservation rule in normal writes as well as Simulation. Negative Need quantities must remain valid.

UI and completion: Add an expandable resource balance and dated projection. Simulate 30 apples, 10 committed, consumption of one apple, partial delivery, cancellation and delayed incoming stock. Compare fixed commitments with a manually revised surplus offer; automated offer adjustment remains outside this plan.

Combine private Needs behind one public item, including temporary fulfilment.

Let one Transfer outcome map to several private Records. Define which result satisfies which Needs: receiving a bike may satisfy both “bike needed” and “transport to work needed”; receiving a ride may satisfy only transport.

Extend settlement to review and apply those local changes together, with permission checks, duplicate protection and corresponding corrections. Reuse the accounting work from step 3. Support alternative solutions without counting the same fulfilment twice.

Add temporary fulfilment with explicit start and end dates. A three-day loan should show when each Need becomes unmet again and link to a return Transfer that the user can create now.

UI and completion: Provide a private fulfilment editor and Calendar/Simulation view. Simulate a donated bike, a ride, a three-day loan, an extension and an early return. Live resource changes still require ordinary authorized actions. Automatic return proposals and scheduled reposting belong to Karma.

Derive agreement for parent Transfers.

Build on the existing hierarchy, but calculate parent agreement from its required children and any direct obligations. Define empty parents, incomplete children, withdrawn alternatives, disputes and revised agreements explicitly.

Derive this state in the backend from the complete required set. The existing hierarchy summary (crates/protein/src/lib.rs:6031) is based on visible query results; filtering out a child must never make its parent appear agreed. Reveal only permitted explanations for hidden blockers.

UI and completion: Show parent agreement and the children still blocking it. Simulate nested Transfers, changing child terms and restricted viewers. A parent’s derived agreement must not manufacture signatures or settle its children.

Make dependency agreement match the record.

Reuse transfer_dependency; the record’s reference to transfer_interaction does not match the current storage model. Dependencies already affect readiness, but dependency mode also requires local party agreement (crates/protein/src/lib.rs:4048).

Derive dependency agreement from the required upstream Transfer being agreed or settled, as requested. Keep permission to activate obligations and apply resource changes separate from that derived state. Check the upstream Transfer’s actual agreement or settlement result rather than treating a collection of promise states as equivalent.

Support remote evidence, revisions, disputes, lost access and stale information. Unknown or unavailable evidence must remain unresolved. Reuse this behaviour for observer links, reject cycles, and use one definition across backend decisions and UI.

UI and completion: Show the required upstream outcome and current blocker. Simulate a chain of several Transfers, a branching chain, a cycle attempt and an upstream revision after downstream readiness.

Prove the complete workflow with many Transfers in Simulation.

Extend Simulation throughout these steps rather than creating a separate Transfer simulator. Use ordinary actions, permissions, signed messages and settlement paths. The current Transfer fixture (crates/simulation/src/fixtures/transfer.rs:10) performs the transaction on one Cell before sending its result elsewhere.

Add reusable scenarios for donations, trades, group exchanges, observers, private accounting, competing reservations, loans and dependent chains. Include independent Organs and several Cells belonging to the same Organ.

Run them with delayed, duplicated, reordered and lost messages; disconnections; restarts; concurrent revisions; revoked access; and partial settlement. Check public agreement consistency, each owner’s expected local changes, privacy, bounded recovery and absence of duplicate application. Private accounting deliberately means local balances need not mirror public quantities.

UI and completion: Let Transfer open a Simulation scenario and inspect each participant’s permitted view, timeline and resource changes. Start with small reproducible cases, then generated runs—for example, 4 Linces with 100 Transfers, growing to 32 with 1,000. Measure runtime, memory and message growth, retain reproducible failures, and run focused tests plus cargo check with warnings treated as errors.

Use shared presentation models in interface and extend the existing native Transfer Castle in desktop. Connect Transfer invitations to the shared pending-offers work when that surface is available; its backend already includes them.

Karma-triggered proposals, recurring purchases, automatic visibility expansion and Frequency-driven returns are excluded. No files were changed and no tests were run; this was a source review and implementation plan.
