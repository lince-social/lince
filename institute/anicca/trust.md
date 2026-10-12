# Trust in Lince

Architecture discussion, started 2026-10-11. This document is a proposal, not an implementation report. Confirmed requirements, existing capabilities, recommendations, and unanswered questions are kept distinct. Update the decisions here as the discussion progresses.

Trust should help someone answer: **Who am I considering interacting with, what relevant evidence can I inspect, and why would I rely on them for this particular activity?**

## Confirmed requirements

- Trust must work with ordinary Transfers and their specialized workflows, including Transport.
- Lince nodes may help people discover trust-related information. An Institute node can be one such source.
- A discovery node does not decide whether a party is trustworthy. Verification and judgment must not depend on a mandatory server.
- Evidence should be independently checkable by other nodes. Whether general claims are part of the first version is still open.
- We are refining the architecture and details in this Markdown before implementing the trust feature.

The owner-authored documentation already requests a public trust portfolio: permitted completed Transfer history tied to verifiable identity, with private fields preserved. See [Transfer in Lince.lingua](Lince.lingua), especially lines 701–702. That Record remains the source of truth; this document develops the proposal around it.

## What already exists

The current repository has foundations for trust, but no complete trust portfolio or contextual reputation system.

| Foundation | What it provides | What it does not establish |
| --- | --- | --- |
| Organ contacts | Local unknown, known, and blocked choices; a numeric Proximity value | Whether somebody reliably performs a particular service |
| Identity keys and signed Facts | Verification of signatures against available keys | A unique human identity or the truth of physical-world statements |
| Organ rosters | Root-authorized Cells, capabilities, succession and revocation machinery | Permission for every member or device to speak for every Person |
| Transfer stages | Agreement, activation, delivery/receipt claims, disputes, settlement and compensation evidence | Independent observation that physical work occurred |
| Transfer delivery | Signed packages, actor proofs, remote commands and application attestations | A public, minimal, independently usable trust receipt |
| Visibility controls | Organ includes/excludes, Proximity ranges, separate saved/live location permissions | Permission to publish a private event in a reputation index |
| Social discovery | Signed announcements, selected helper Organs, source provenance, conflicts, consented gossip, withdrawals and bounded storage | A complete search of all Lince history, or a trustworthy server score |
| Social reports | Evidence and explanations for an operator to inspect | A verdict about a person or an automatic reputation penalty |

Relevant implementation references:

- [Identity signatures](../../crates/engine/src/trust.rs), [Organ rosters](../../crates/engine/src/roster.rs), and [roster signature tests](../../crates/engine/tests/roster_fact_signatures.rs).
- [Contacts and Proximity](../../crates/store/src/organs.rs) and [session identity](../../crates/store/src/session_access.rs).
- [Transfer evidence types](../../crates/nucleus/src/transfer.rs), [delivery and application attestations](../../crates/nucleus/src/transfer_delivery.rs), and [delivery verification](../../crates/engine/src/transfer_delivery.rs).
- [Transfer field disclosure](../../crates/nucleus/src/transfer/disclosure.rs) and [Record/location visibility](../../crates/nucleus/src/visibility.rs).
- [Discovery provenance and conflicts](../../crates/engine/src/social/discovery_sources.rs), [consented gossip](../../crates/nucleus/src/social/gossip.rs), and [report handling](../../crates/engine/src/social/reports.rs).

Proximity means the relationship value chosen for an Organ contact; lower numbers are closer in the approved location/visibility design. It is not geographic distance and should not silently become an inferred reputation score. Tutorial wording about nearby discovery must not define the new trust model.

Karma is Lince's automation system. Its name must not imply that a person's reputation is already implemented there. Trust can eventually be an input to explicitly configured Karma rules.

Transport currently exercises generic Transfer, identity, visibility and location capabilities. The documentation still lists dedicated guided Transport controls and geographic discovery as further work. Trust should not depend on those controls being finished or on a map being available.

## Separate the questions that people call trust

| Question | Suggested representation |
| --- | --- |
| Did this key sign these exact bytes? | Cryptographic verification result |
| Why associate this key with this Person or Organ? | Identity binding, introduction, pinned key or authorized succession |
| Was the signer authorized to make this statement? | Role and delegation evidence |
| What was recorded about this interaction? | Transfer evidence or a signed statement with an explicit meaning |
| Do I rely on this source to report accurately? | A judgment about an evidence issuer |
| Do I rely on this party for this activity? | A judgment about the party in a particular context |
| May this viewer read or redistribute it? | Visibility and publication policy |
| Is the information sufficiently current for my decision? | Status, expiry and last-check information |

Recommendation: keep these answers separate in storage and UI. A valid signature can accompany an unreliable statement. A known contact can be a poor choice for one service. A reliable driver is not automatically a reliable medical adviser or a careful recommender.

The W3C Verifiable Credentials model explicitly separates cryptographic verification from the truth of a claim. We can adopt that distinction without adopting its entire credential stack. [W3C verification terminology](https://www.w3.org/TR/vc-data-model-2.0/#terminology).

## What more nodes actually contribute

One Lince can check a signature and the supplied protocol evidence locally. Repeating that calculation on a hundred nodes does not add a hundred independent observations of a ride.

More nodes help with availability, obtaining revisions and corrections, comparing conflicting signed versions, and finding evidence that one source omitted. Those benefits depend on reaching sources with different information; the number of endpoints alone is not proof of independence.

An actual witness contributes a different kind of evidence: a signed account of what they observed. We must record what they witnessed, their relationship to the parties when disclosed, and why the reader relies on them. A server that cached a receipt is a carrier, not a witness to its physical event.

One operator can create many keys, Organs, devices and fake interactions. Douceur's Sybil analysis explains why redundancy based on unknown electronic identities can fail. For Lince, the consequence is to avoid node-count voting and avoid presenting distinct keys as proven distinct humans. [The Sybil Attack, 2002](https://www.microsoft.com/en-us/research/wp-content/uploads/2002/01/IPTPS2002.pdf).

Recommendation: verify locally, discover through several selected sources when useful, and make the evidence and its limitations inspectable. A majority of available nodes should not decide a person's reputation.

## The smallest useful first version

Recommendation, pending discussion: start with a **Transfer portfolio plus personal judgments**.

1. Inspect your own permitted interaction history with a party.
2. Inspect minimal published Transfer summaries offered by that party or discovered through chosen sources.
3. Check signatures, identity bindings, roles, revisions and available status locally.
4. Distinguish participant-confirmed results from unilateral accounts, disputes and missing evidence.
5. Save your own judgment for the activity you care about, with optional reasons and references.
6. Use this information to filter or review Transfer candidates without changing permissions or agreeing to a Transfer automatically.

This avoids requiring a general-purpose claims registry. It still involves statements: delivery and receipt are already claims in Lince. The distinction is between a small vocabulary tied to the Transfer protocol and an unrestricted system where anyone can make arbitrary assertions about anyone.

Recommendations, qualifications and witness statements can be added later as generic signed Record evidence. Their existence should not force the first version to solve universal truth, social graph scoring or every kind of credential.

Unknown means insufficient information. It should not become a negative judgment, especially for new users, private participants or people whose history is unavailable offline.

## Identity and who owns a judgment

Recommended model: a judgment has an author and a subject, each explicitly identified as a Person or Organ. Its context identifies an activity, role or Concept. A Cell/node is a device or endpoint, not the subject of a person's service reputation.

Allow personal judgments and Organ judgments, visibly separate. An Organ judgment needs a defined authorization rule for who can issue or revise it. A Person's private opinion must not appear as the Organ's collective position merely because it is stored there.

Person authorship and personal secrecy are separate requirements. Current Organ/session permissions and Transfer Person-level field audiences need an audit before promising that a judgment stored in a shared Organ is hidden from its other members. Do not infer a general Person-private storage capability from location's Organ-level visibility rules.

Useful identity rules:

- Display names and slugs are labels; use durable identifiers and key bindings for evidence.
- A self-contained signature verifies control of the included key. It does not by itself bind that key to an already known party.
- Pairing, a checked fingerprint, a trusted introduction or an authorized key chain can establish a binding; show which was used.
- Changing a device must not create a new reputation identity. An authorized succession can preserve identity continuity.
- A broken or unavailable succession chain stays unresolved. Do not merge identities by matching names.
- Membership in an Organ does not automatically transfer that Organ's portfolio to every member, or a member's portfolio to the Organ.
- Government identity and public legal names should be optional evidence, not a prerequisite for ordinary use.
- Histories in separate Organs remain separate unless someone explicitly links them. Public linking has privacy consequences.

Existing Organ succession and revocation are starting points. Person key rotation, recovery and delegation must be specified and checked rather than assumed to have the same complete implementation.

## A Transfer portfolio must say exactly what was confirmed

| Evidence | Appropriate interpretation |
| --- | --- |
| Signed agreement | The identified actor agreed to the specified terms or revision |
| Activation | The protocol started an occurrence |
| Delivery claim | The giver reported delivery |
| Receipt claim | The receiver reported receipt |
| Both participant statements | The parties made compatible statements about the result |
| Settlement/application evidence | The protocol recorded the specified accounting/application result |
| Package received/seen receipt | A network package was received or seen |
| Dispute or correction | A specified actor contested or changed a statement |
| Witness account | That witness reported the described observation |

None of these alone establishes that a real-world ride happened safely. Compatible participant signatures are harder for an unrelated attacker to forge than a unilateral statement, but colluding participants can sign a fictitious event.

Recommendation: make the default portfolio about supported results in a chosen context. Show cancellations, disputes, reversals and unilateral accounts distinctly where the viewer is permitted to see them. Cancellation is not automatically misconduct; compensation is not automatically proof of wrongdoing.

Do not turn the existing Transfer application attestation into a ride-safety badge. It attests to a bound application/accounting operation. Do not confuse the package receipt with the receiver's service receipt claim.

Count an occurrence once per subject and role within the visible portfolio. Revisions, repeated deliveries of evidence and settlement slices are not additional successful interactions. Show occurrence counts and distinct Transfer counts separately if both are useful. Confirm this counting unit before implementation.

For multi-party events, state whose result is covered. A receipt from one passenger must not assert that every passenger confirmed the journey.

## Portable evidence without disclosing private Transfers

A normal public Record view and a portable proof are different surfaces. Existing field projection should be reused for access decisions, but removing fields from signed bytes does not preserve the original signature.

Recommended first mechanism: participants review and sign a new, minimal **portfolio receipt** derived from a Transfer occurrence. It is an ordinary Record capability with typed proof data, not a second Transfer or a transport-only rating object.

The receipt should identify the statement being made and who made it. A newly signed summary proves those signers approved that summary; it does not secretly prove every hidden source Fact. Where stronger protocol proof is requested, include the authorized original evidence and identity bindings, or label that proof unavailable. Advanced selective-disclosure proofs are a separate project.

Proposed fields, to refine rather than treat as a finalized wire format:

| Field | Purpose |
| --- | --- |
| Protocol and meaning version | Prevent incompatible parsers or changed Concepts from changing the interpretation |
| Public receipt/event ID | Detect duplicate imports and repeated presentations without requiring private Record IDs |
| Subject identity and role | State which Person/Organ and activity the evidence concerns |
| Issuer identity and authority | Distinguish participants, authorized Organ representatives and witnesses |
| Context Concept and definition version | Express transport, lending, delivery or another ordinary workflow |
| Result and evidence basis | Distinguish jointly reported completion, unilateral account and protocol/accounting proof |
| Coarse event period | Optional context without exact travel timestamps |
| Permitted proof references | Bind disclosed supporting evidence; do not publish private source UIDs by default |
| Publication audience and redistribution terms | Separate permission to inspect from permission to index or forward |
| Revision, previous hash and status references | Make corrections and incompatible versions detectable |
| Issued/expiry times and signing key IDs | Support freshness and key-status checks |
| Signatures over the exact body and roles | Bind every co-signer to the same approved statement |

Use deterministic, versioned signing bytes, domain separation, bounded sizes, stable IDs and idempotent import, following existing Lince protocol practices. Reuse the current signature machinery; a new credential framework is not a prerequisite.

A public event ID must stay stable for the same occurrence in the same publication scope. Otherwise a party can reissue the same event to inflate counts. Different audience-specific IDs can preserve unlinkability but prevent global deduplication; that tradeoff must be explicit. Even stable IDs cannot stop colluding actors from inventing different fictitious occurrences.

Corrections to a jointly signed body need fresh signatures from the relevant co-signers. Separately signed status updates can retract an issuer's own statement or publication permission, but cannot rewrite another actor's words, revoke their signature, or silently undo a Transfer settlement.

## Visibility, publication and unfavorable evidence

Private should remain the starting point for new trust notes and portfolio receipts. Inspecting your own permitted history does not publish it.

Reuse the established Organ allow/exclude and Proximity semantics where appropriate: includes combine with OR, criteria within a condition with AND, and exclusions win. Field-level Person audiences and publication redistribution also matter; one outer Record permission cannot grant access to all referenced evidence.

Recommended publication flow:

1. Choose an occurrence and the minimal statement to publish.
2. Preview every field, identified participant, audience and destination.
3. Obtain the required signatures and publication consent for that exact statement.
4. Publish through selected services or share directly.
5. Propagate authorized corrections or withdrawal through those same channels.

Consent policy is unresolved. Requiring every identified participant's consent protects private history, but lets a participant prevent an unfavorable event from appearing. Allowing unilateral publication supports warnings and accountability, but needs careful handling of evidence, privacy, harassment and replies. A positive co-signed portfolio and a contested account should have separate meanings. Operator moderation reports are not a substitute for this decision.

Recommendation for the first increment: jointly reviewed portfolio receipts; personal unfavorable judgments remain private. Do not promise an objective failure rate or complete history under a selective publication model. Say “visible published history”, not “all Transfers” or “no disputes”. This recommendation is pending the publication answer.

Keep exact addresses, routes, live positions, private messages, prices, internal Need/Contribution identifiers and uninvolved passengers out of receipts by default. A map is neither needed to inspect trust nor sufficient evidence of fulfillment. Even a coarse time and participant pair can identify a journey.

Cached data cannot be recalled from someone who already received it. Withdrawal should stop future indexing and redistribution by conforming nodes. Expiry and limited retention help, but do not make prior public disclosure reversible.

Counters, filtering and explanations must use only permitted data. “Three hidden failures” leaks information even if the hidden Records cannot be opened. New evidence permissions must also be enforced in sync, export, cache, attachments and persisted Castle state.

## Discovery nodes and an Institute node

A node can expose an optional portfolio discovery service using the existing social publication/search family. Explicitly add trust evidence types; the existing announcement service does not already understand them.

Suggested responsibilities:

- Accept authorized signed public receipts and status updates within resource limits.
- Validate format, signatures and publication rules for admission. Admission is not a judgment about the subject.
- Index permitted subject identities, activity Concepts, periods and proof kinds.
- Return the actual signed artifacts, pagination, immediate source and retrieval information.
- Support content-hash retrieval, duplicate suppression, expiry, withdrawal and conflict reporting.
- Optionally exchange redistributable public artifacts with consenting peer services.

An Institute node uses the same protocol and can be replaced or disabled. It should be a convenient discovery source without a privileged signing key for approving users. Its own recommendations, if later supported, would be ordinary issuer statements which each reader may choose to rely on.

Clients should retain direct sharing and offline import. An already cached proof can be checked without its discovery server. Current status may still require a refresh; “signature valid, status last checked yesterday” is different from “currently accepted”. A user may require recent status for automation, or choose manual review while offline.

Query several chosen sources with bounded parallelism, deadlines, pages and cancellation. A new trust search must not automatically add contacts, change unknown to known, modify Proximity or grant sync/location access. Incoming artifacts must never execute Actions or Karma rules.

More sources improve the chance of discovering an omission or conflict; they do not prove that nothing else exists. Servers can omit, delay, refuse or selectively return results. Compare hashes and signed revisions, retain bounded conflict evidence, and identify which sources were searched. A signature protects a returned artifact; it does not authenticate an unsigned search result's completeness.

Public append-only logs are a possible later tool for detecting inconsistent service responses. RFC 9162 separates log auditing from trusting the logged material and describes comparing signed tree heads to detect inconsistency. This is an analogy for optional public evidence logs, not a proposal to put private Transfers in Certificate Transparency or to add a global blockchain. [RFC 9162 security considerations](https://www.rfc-editor.org/rfc/rfc9162.html#section-11).

Lookup metadata also needs a decision: asking a node about a party reveals interest in that party. Prefer explicit source selection and reuse local evidence before querying. Do not broadcast private contact lists or fetch every contact's reputation automatically.

## Local judgment and later recommendations

Recommended judgment fields: author, subject, activity/role, assessment, reasons, evidence references, creation/review date and audience. Judgments should be editable through ordinary Actions with attributable history; editing a judgment does not modify the underlying Transfer.

Start with a small set of meaningful assessments such as “not assessed”, “would rely on”, “review first” and “would avoid for this activity”. These labels are proposals. A numeric scale can be offered later if wanted, but should describe the author's assessment rather than pretend to be a globally comparable probability.

Keep a separate decision about whose recommendations you rely on. GnuPG's ownertrust distinction is a useful precedent: trusting someone's identity is different from relying on their certifications of others. For Lince, trusting someone as a driver need not mean trusting them as a judge of drivers. [GnuPG FAQ, validity and ownertrust](https://www.gnupg.org/faq/gnupg-faq.html).

If referrals are approved, begin with directly selected recommenders and explicit scope. A recommendation is a signed opinion with reasons and optional evidence. It must retain its original issuer through discovery and forwarding. Avoid unrestricted recursive propagation: cycles, several paths to the same issuer and many Cells belonging to one Organ must not multiply its weight.

Any threshold based on several recommenders needs a user-selected issuer set and a clear independence assumption. Different keys and an apparently diverse graph are insufficient to prove different real people.

## UI and integration with ordinary Lince features

Recommended UI proposal, requiring refinement before implementation:

- A reusable Trust Castle for a selected Person or Organ, with identity/binding, permitted portfolio, personal/Organ judgments, discovery sources and verification details.
- A compact Transfer party summary showing your relevant judgment and the evidence behind it. Transport supplies a context and role to this shared component.
- A reviewed “Share portfolio receipt” action after a suitable occurrence, distinct from settlement and from a review of the provider.
- An “Inspect evidence” view that explains signer, role, context, proof basis, identity binding, status and immediate discovery source in plain language.
- Source settings and manual refresh, with offline, partial, stale and conflict states visible.
- Optional Protein filters over permitted evidence and local judgments. Explain why a result is included or excluded.

Keep identity binding, signature validity, content conflict and freshness as separate result fields. A receipt can have valid signatures, unresolved identity and stale status at once. Avoid one unexplained green “trusted” badge.

Transport example: a passenger reviews a driver's visible transport receipts and their own previous experience. They choose whether to proceed. After the journey, both may approve a minimal result receipt. Live location remains separately consented and is excluded from that receipt. The same mechanism works for lending a tool or completing a delivery.

Recommendation: first use trust for inspection and candidate filtering. If automatic selection or agreement is later approved, use explicit Karma rules with reviewable context, thresholds, freshness and an unknown-data policy. New endorsements must not silently change agreements or access controls.

## Sequential implementation proposal

Implementation is not authorized by this document. Each step needs a settled meaning and usable frontend/backend behavior.

1. **Define semantics and examples.** Decide judgment ownership, publication consent, evidence kinds, counting unit, assessments and treatment of disagreements.
2. **Audit identity and evidence.** Trace Person/Organ bindings, signer roles, key history and the difference between service claims, application attestations and package receipts. Identify any missing authorization or lifecycle checks.
3. **Build local inspection and judgments.** Use permitted existing Transfer data, explicit actor/context ownership and a reusable Trust view. Work offline and without maps or discovery servers.
4. **Build reviewed portable receipts.** Add minimal typed Record proof data, signing/consent actions, verification, safe import/export, correction and withdrawal. Add new migrations only if necessary; never rewrite existing ones.
5. **Add optional discovery.** Extend selected-source publication/search and consented gossip with bounded trust artifact types. Run an ordinary Institute node as one available source.
6. **Integrate candidate filters.** Reuse Protein and Transfer components; Transport contributes its context. Keep permissions and agreement authority independent.
7. **Add referrals or automation if chosen.** Specify issuer trust, scope, propagation bounds, freshness and unknown-data handling before implementation.

Avoid a new trust-specific ledger, bespoke Transport scores, compulsory centralized identity verification or a global majority vote. Shared families are Records/Assertions/Facts/Actions, identity/roster, Transfer evidence, visibility/disclosure, and social publication/discovery.

## Laboratory and adversarial verification plan

No new trust tests have run: this is a design task. When implementation is approved, use at most four Linces per topology and add native Laboratory cases through shared fixtures.

| Scenario | Required result |
| --- | --- |
| Provider, receiver, reader and discovery node | Reader verifies a permitted receipt locally; provider/receiver can share it directly if the node is offline |
| Provider and receiver with two discovery nodes | Replayed copies count once; a conflicting signed revision remains inspectable rather than becoming a vote |
| One operator represented by multiple Cells/keys | Extra replicas and signatures are not shown as extra independent humans or interactions |
| Known identity versus similarly named stranger | History is not attached by display name, slug or unanchored included key |
| Delivery claimed but receipt absent | No jointly confirmed service result |
| Package received but service not received | Network receipt does not increase completed service counts |
| Settlement reversed or result disputed | Appropriate visible status changes without inventing misconduct or rewriting history |
| Revised jointly signed summary | Old signatures cannot authorize changed content; unilateral updates remain attributed |
| Private Transfer and denied linked evidence | No private source data, IDs, addresses or hidden counts in queries, cache, sync, exports or Castle snapshots |
| Private judgment in a shared Organ | Behavior matches the settled Person/Organ read and write permissions |
| Key rotation, revoked device, missing chain | Authorized continuity works; unresolved or unauthorized identity cannot inherit a portfolio |
| Offline, expired or withheld status | Signature result and freshness remain separate; automation follows the chosen unknown-data rule |
| Unfavorable account and subject reply | Issuer and evidence remain attributable; neither side silently rewrites the other |
| Large or hostile input | Bounded parsing, signature work, graph traversal, search, queue and storage; cancellation keeps Lince usable |

Measure local signature verification separately from UI rendering and remote search. Use bounded synthetic portfolios to measure import time, deduplication, cold/warm queries, memory and p95 UI response. Choose budgets from measurements on representative hardware; do not invent a passing performance claim before running them.

A simulated fake transfer can prove the implementation correctly labels participant statements. It cannot prove a decentralized system detects every collusive physical-world lie. Test both rejection of invalid evidence and honest labeling of valid-but-limited evidence.

## Decisions to refine

The first three questions have been sent for this discussion. None is recorded as agreed until answered.

| Order | Question | Current recommendation | Status |
| --- | --- | --- | --- |
| 1 | Start with Transfer evidence and personal judgments, or include others' recommendations immediately? | Evidence and personal judgments first | Awaiting answer |
| 2 | Who owns judgments: Person, Organ, or both? | Both, with separate authorship and authority | Awaiting answer |
| 3 | What consent is needed to publish an identified Transfer account? | Jointly reviewed receipts first; clarify unilateral unfavorable accounts separately | Awaiting answer |
| 4 | Which dimensions matter first: reliability, quality, safety, or simply whether you would interact again? | One activity-specific assessment plus reasons; avoid pretending to measure safety | Open |
| 5 | Should judgments use categories, numbers, or a configurable vocabulary? | A small editable vocabulary before numerical aggregation | Open |
| 6 | What is the unit of history: Transfer, occurrence, or fulfilled promise? | Occurrence per subject/role; show Transfer count separately | Open |
| 7 | How should disputes, cancellations and compensation appear? | Attributed status and context; no automatic penalty | Open |
| 8 | What may a receipt identify about counterparties, time and context? | Minimum reviewed fields; addresses and live positions excluded | Open |
| 9 | Are judgments private to a Person, shared within an Organ, or optionally published? | Private by default; specify the actual storage/audience boundary | Open |
| 10 | What can a signed correction retract, and for how long must status be retained? | Retract the issuer's statement/publication, retain bounded current status; settle retention periods | Open |
| 11 | Should Lince ship an Institute discovery source enabled, suggested, or absent? | Suggested and replaceable; require a deliberate first remote lookup | Open |
| 12 | If referrals are included, whose opinions count and how far can they propagate? | Directly selected issuers and contexts initially | Open |
| 13 | Can trust automatically select parties or change agreement levels? | Inspection/filtering first; explicit Karma automation later | Open |
| 14 | How should a new party with no visible portfolio participate? | Show unknown; allow manual interaction and ordinary direct introductions | Open |

Refine questions 1–3 first, then the evidence and publication lifecycle, then UI and implementation details. The document should accumulate actual decisions without turning unanswered recommendations into requirements.
