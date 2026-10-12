# Trust in Lince

Architecture discussion, started 2026-10-11 and refined 2026-10-12. This document is a plan, not an implementation report. Confirmed requirements, existing capabilities, recommendations, and unanswered questions are kept distinct. Update the decisions here as the discussion progresses.

Trust should help someone answer: **Who am I considering interacting with, what relevant evidence can I inspect, and why would I rely on them for this particular activity?**

The refined core is a family of configurable searches and analyses over visible Transfer history, counterparties and consented contact paths. Each view explains the evidence it used and the policy applied. Personal/Organ judgments are optional annotations; there is no intrinsic global trust balance attached to a party.

## Confirmed requirements

- Trust must work with ordinary Transfers and their specialized workflows, including Transport.
- Lince nodes may help people discover trust-related information. An Institute node can be one such source.
- A discovery node does not decide whether a party is trustworthy. Verification and judgment must not depend on a mandatory server.
- Evidence should be independently checkable by other nodes. The first version centers on Transfer evidence searches/analyses, with optional personal/Organ judgments; general recommendations and arbitrary claims are later extensions.
- We are refining the architecture and details in this Markdown before implementing the trust feature.
- Identity, evidence, contextual judgment and discovery remain separate responsibilities in the shared trust system.
- Allow both Person and Organ judgments, with separate authorship and authority.
- Trust should express experience and contribution: public evidence of selling, donating and meeting Needs, attached to a durable identity. These use one system rather than separate seller and donor reputation implementations.
- This experience has social recognition value. It is not money, and there is no requirement to make it transferable, spendable or redeemable.
- Readers should be able to base their view on transactions involving people and organizations they recognize and rely on, rather than treating arbitrary signed quantities or large numbers of bots as credible value.
- Experience is shown by activity and unit, with optional local summaries. There is no universal points total in the agreed design.
- Keep a conservative view using Persons/Organs directly chosen for each activity. Also support bounded contact-path analyses, including contributions to actors up to three visible contact links from a chosen anchor. A path does not automatically endorse an issuer or change contact/visibility settings.
- Bind credited outcomes to the selected Vocabulary's exact meanings and units. The receiver determines which of its Need's meanings and amounts it confirms, rather than the giver gaining experience from arbitrary added Assertions.
- The strongest contribution category requires a receiver-authored Need and a receiver confirmation bound to its exact meanings and actual amount. Other contributions remain visible in separately identified evidence categories.
- Support configurable weights by evidence level and visible contact distance, preserving the underlying quantities and explaining the calculation. These are local analysis policies, not universal trust or fraud probabilities.
- Sign the relevant Transfer events and share permitted evidence through bounded, consented gossip. Copies, relay count and alternative paths do not create additional contributions.

The 2026-10-12 agreement adopts the separation of identity/evidence/judgment/discovery, an evidence-first increment, and Person/Organ judgment ownership. The subsequent discussion expands the earlier direct-source-only scope into configurable contact-path analyses while retaining direct-source views. Explicit replies approve evidence/distance weighting and the Need-linked receiver-confirmed category. Jointly reviewed receipts remain the initial publication plan. The exact consent rights for publishing unfavorable accounts are still unresolved; broad approval of the plan does not settle that tradeoff.

The owner-authored documentation already requests a public trust portfolio: permitted completed Transfer history tied to verifiable identity, with private fields preserved. See the Transfer Record, `@transfer`, in [Lince.lingua](Lince.lingua). That Record remains the source of truth; this document develops the proposal around it. Use the Record identity rather than a line number, because the documentation is being reorganized.

## Experience and social recognition in one system

The agreed direction is a portable history of contribution. A seller fulfills a paid Need; a donor fulfills a Need without the same compensation. The underlying event, participant roles, confirmation, identity, permissions and proof machinery are shared. Payment/donation terms and the activity are attributes of the interaction, not different trust protocols.

An offer or donation pledge does not create completed experience. Credit in a portfolio should come from the supported outcome of an occurrence, with the basis shown: a giver's claim, a receiver's confirmation, compatible participant statements, or available protocol evidence. A receiver's confirmation supports the meaning they actually signed; it does not certify every possible downstream benefit.

Recommendation: use “experience” or “contribution history” as the product language. “Social coin” is a useful metaphor for recognition, but a coin balance suggests ownership, exchange and universal comparability that this evidence does not provide.

The durable object should be the evidence Record. A number displayed from it is a derived view, with its activity, unit, period, accepted sources and calculation visible. There is no global balance that a node mints when a Transfer is created, and no points are transferred from recipient to contributor.

### What can be made resistant to forgery

The desired guarantee needs precise wording: a verifier can check that the evidence was signed by keys bound to the stated identity, authorized for the stated role, and has not been changed without breaking its proofs. This relies on sound cryptography, correct authorization, protected keys and a checked identity binding.

That establishes attributable statements and protocol actions. It does not establish an unforgeable measurement of a physical event, a unique human behind a public key, or a universal measure of value. A compromised key can produce authentic signatures which the human did not intend; revocation and uncertainty about the affected period must be represented.

Some digital outcomes can be checked more directly: a blob has a particular hash, a protocol application was recorded, or an identified party confirmed a defined result. Physical delivery and beneficial impact usually need a receiver, witness or organization whose reporting the reader chooses to rely on.

For this plan, replace “unfalsifiable trust” with **verifiable attribution and evidence, interpreted through explicitly chosen sources**. The first is a technical property; trust in its meaning remains a decision.

### Criticism of the social-coin model

| Proposed intuition | Where it can fail | Recommended refinement |
| --- | --- | --- |
| A public identity proves the contributor is real | A durable identity can be a bot; a real organization can lie or lose its key | Show identity binding and the reason for relying on that issuer separately |
| A seller with more trust is less likely to scam | History can be fabricated, selectively published, stale, or exploited after genuine good behavior | Show relevant recent experience, disputes and comparable transaction scope; do not display a fraud probability without a justified model |
| A large donation proves substantial value | The quantity, unit, quality, recipient Need and downstream effect may be unsupported | Preserve exact units and the receiver's precise confirmation; distinguish supplied resources from measured benefit |
| Donation history proves someone is doing good | The need or activity may be harmful, unhelpful or simply valued differently by readers | Record what was confirmed; let people and communities explain the social value they assign to it |
| Sales and donations should all be one system | One scalar allows donations to inflate unrelated seller credibility, or purchases to buy general status | Share the protocol and portfolio, while keeping role, activity, evidence and quantities distinguishable |
| Valid counterparties defeat fake transactions | Authentic, recognized parties may collude, misreport, or misunderstand what they sign | Treat recognition as a starting point; select whose confirmations count for each activity and retain the evidence basis |
| More transactions mean more experience | Splitting one event, repeating transfers within a controlled group, or moving the same goods in circles can inflate counts | Deduplicate occurrences, show repeat counterparties and scope, and keep count separate from quantity and impact |
| A public portfolio shows someone's overall contribution | Private work and selectively omitted results are absent; people have unequal resources to contribute publicly | Label the view as published history and avoid making an empty portfolio a negative judgment |

Reputation can create incentives to preserve future relationships, but those incentives depend on circumstances. An experimental study observed providers exploiting a good reputation in its market setting. This supports treating history as evidence for review, not a guarantee of future honesty; it does not supply a calibrated scam probability for Lince. [Mir Djawadi et al., 2018, Maintaining vs. milking good reputation](https://wiwi.uni-paderborn.de/fileadmin-wiwi/dep1ls2/PDF_Dokumente/Djawad_Fahr_Haake_Recker.pdf).

In particular, many small, successful sales do not establish competence or honesty for a very large sale. A visible charitable history should not cancel a substantiated delivery dispute in an unrelated sale. A universal number hides both differences.

## Whose evidence counts in a particular view

The user wants fabricated transactions between unknown bots to have no persuasive force while retaining transactions involving recognized parties. This is achievable as a reader-selected evidence view, without a mandatory identity certification server.

The approved direct-source policy needs two explicit choices, with activity scope where relevant:

1. **Identity binding:** “I recognize this identity as that person/organization,” with a fingerprint, introduction, key chain or other basis.
2. **Evidence issuer reliance:** “I rely on this person/organization to confirm this kind of result accurately.”

The existing `known` contact state must not silently make every statement by that contact count as reliable evidence. Proximity can help select a reviewed audience or suggest parties, but closeness alone does not establish reporting competence.

Acceptable proof still needs a verified subject binding, issuer authority and permitted disclosure. In the conservative direct-source view, the reader's issuer set determines which otherwise valid confirmations enter their accepted-experience totals. Other signed evidence remains inspectable as unassessed or outside that view's scope. Contact-path views can include it under a separately declared policy, preserving its issuer and evidence category. A recognized charity is one possible selected issuer, not a privileged network authority.

An issuer may be a receiver, an authorized representative of a recipient Organ, or an explicitly chosen witness. Reliance can be limited: an organization may credibly confirm stock received but be an unsuitable source for assessing driving safety. Recognizing the contributor alone must not make their unilateral account equivalent to receiver confirmation.

Keep directly chosen Persons/Organs as a conservative preset. The expanded plan supports observing history through permitted contact chains; it does not declare the whole graph legitimate or delegate each person's issuer-reliance choices. A contact relation, an identity introduction and an activity-specific endorsement have different meanings.

### Example: food contribution

The following figures are examples, not results from existing tests.

1. A public contributor identity is bound to its established key history.
2. Two unknown bot identities sign that the contributor donated one billion food items. The signatures can be valid, but the reader has not selected those issuers for food-receipt evidence. They add nothing to the reader's accepted-experience totals.
3. A pantry recognized by the reader signs that it received 40 kg of rice from the contributor in a defined period. Its representative's authority and the receipt proof can be checked locally. If the reader relies on that pantry for this confirmation, the view can show “40 kg rice received, confirmed by Pantry A”.
4. If the pantry later reports 120 meals distributed, that is a different statement, with its own issuer, unit and evidence basis. It must not be inferred automatically from 40 kg of rice or added to it as another 120 units of generic goodness.
5. Another reader may rely on a different set of sources and obtain a different experience view from the same available evidence. Both should show how their view was derived.

If that same contributor has 200 compatible transport receipts from sources the reader relies on, those belong to its transport experience. They do not establish food quantities or erase a dispute about the rice.

## Summarizing experience without inventing a currency

Agreed summary structure: activity and compatible units, with optional local summaries. The following display details are recommendations:

| Dimension | Example display | Important boundary |
| --- | --- | --- |
| Activity and role | Food contributor; transport provider; tool borrower | Relevance does not automatically transfer between roles |
| Supported outcomes | 12 confirmed occurrences | Count a defined occurrence once; a confirmation is not an independent physical observation |
| Quantity and unit | 40 kg rice; 8 hours assistance | Sum compatible units and meanings only; no automatic conversion into generic social points |
| Evidence sources | Confirmed by Pantry A and Clinic B | Display distinct identities, not proven independent humans |
| Period and recency | Last 90 days; most recent confirmation last week | Old evidence remains history; its relevance can diminish |
| Disclosed disagreement | One visible dispute, with the issuer's account and status | No invisible-failure counts or automatic moral verdict |
| Coverage | Published evidence from three selected sources; some status unavailable | A selected portfolio is incomplete; unknown is not zero contribution |
| Your assessment | Would rely on for this kind of contribution; reasons | An authored judgment rather than a protocol fact |

Represent quantities with Lince's existing decimal/unit machinery. Unit identity, Concept meaning and any conversion rule must be explicit. A billion custom “food tokens” cannot be treated as a billion meals merely because the author used a similar label.

A fulfilled Need's local quantity change can be useful protocol evidence, but changing that quantity is not itself an independent measurement of real-world benefit. Exposing private Need details also requires permission. Publication can express a recipient's approved outcome without publishing its internal Record identifiers or beneficiary identities.

For chains of contribution, preserve attribution: donor, transporter, pantry and cook can each have supported roles in meeting a Need. They must not each claim an exclusive delivery of the entire chain's result merely because they participated. Multiple actors can deserve recognition for one shared result without creating four separate physical outcomes.

Show repeated interactions within the same group as such. Self-controlled or affiliated counterparties, where known, can still describe real work, but should not be presented as independent corroboration. Undisclosed affiliation cannot be solved by counting keys.

If communities later want badges or a combined experience score, make the issuing community, definition, version, accepted issuers, weights, units, period and evidence references explicit. The receipt remains portable; the interpretation is local or community-authored. The exact optional local summaries are still open; the universal points balance has been rejected for this plan.

A future score should be recomputed from permitted evidence and the current declared policy. An imported score is a signed judgment by its issuer, not proof that the reader's own policy would produce the same value. Removing a receipt from a selected view can change its total; this does not spend, confiscate or transfer the contributor's past experience.

Do not rank a person's overall worth from published donations. A count-based system favors event splitting; an amount-based system favors people with more resources; a visibility-based system overlooks private care and contribution. Activity summaries and transparent community judgments make these limitations easier to inspect.

## Vocabulary-bound outcomes and experience

Agreed direction: bind a Transfer's confirmed contribution to particular Assertions and units from an explicitly selected Vocabulary. The strongest category requires a receiver-authored Need and confirmation of its exact meanings and amount. The detailed binding format and lifecycle rules below remain proposals to refine.

A Vocabulary supplies shared meanings. An Assertion expresses something using a selected Concept, with any applicable object, amount and unit. The Transfer moves resources or performs work described by those meanings; it does not literally deliver a tag. Agreeing on the Vocabulary makes the statement interpretable, but does not itself establish that the described contribution occurred.

The current model already has [Vocabulary source identities and definitions](../../crates/nucleus/src/vocabulary.rs), [Assertions with predicates, objects, quantities, units and authors](../../crates/store/src/assertions.rs), and [Transfer promises/occurrences with Concept and unit references](../../crates/nucleus/src/transfer.rs). The owner-authored Vocabulary Record still lists stable definition versions and full preservation of imported claim meaning as unfinished work. Therefore version-bound portable trust evidence is planned work, not a capability assumed complete today.

Recommended lifecycle:

1. During agreement, choose the contribution meaning, Vocabulary/Concept identities, definition versions, relevant Assertion values/objects/conditions, amount, unit and participant roles. State what constitutes fulfillment and who may confirm it.
2. Bind those meanings to the agreed promise and occurrence revision. An arbitrary tag on a related Record is not automatically a credited outcome.
3. Record the actual result and the relevant participant confirmations against that same binding. Confirming receipt of rice does not automatically confirm its quality, ethical production or downstream meals.
4. Settlement and the required contribution confirmations make the supported outcome eligible for an experience view. Settlement by itself does not endorse every attached Assertion.
5. Apply publication permission, identity/authority checks, status and the chosen view's direct-source or contact-path policy for that activity. Attribute the result separately to each supported participant role.
6. Recompute activity/unit summaries from the eligible evidence. Do not mint generic points or count a revised receipt as a new occurrence.

Illustrative outcome, using human-readable labels rather than proposed Lingua syntax:

| Bound item | Example |
| --- | --- |
| Meaning source | Pantry Vocabulary, selected definition version |
| Resource/assertion | Rice contributed for food assistance |
| Actual confirmed amount | 40 kg |
| Contributor role | Alice delivered/donated 40 kg |
| Recipient role | Pantry A received/acknowledged 40 kg |
| Supported benefit | Resource received for the stated Need; no inferred meal count |
| Reader policy | Count Pantry A's food-receipt confirmations if directly selected |

Both parties can acquire role-specific experience from the same event. Alice's delivery history and the pantry's receipt history describe different roles; their combined physical contribution is not 80 kg. Receipt experience does not automatically establish that the pantry distributed the food well, just as donation history does not establish selling competence. Eligibility can differ by role because readers may rely on different confirming issuers.

Only actual supported quantities enter the relevant totals: if 35 kg were confirmed from a 40 kg promise, keep the remaining 5 kg separate. A disputed quantity or an unsupported outcome stays labeled rather than becoming settled proof of delivery.

One occurrence may support several explicitly confirmed outcomes, including non-quantitative ones. Each needs its precise meaning and evidence basis. Applying ten tags does not create ten completed Transfers or multiply a quantity tenfold. Related categories may be useful views over the same evidence, but overlapping category totals are not additive.

Pin meaning, not wording. Identical labels in different Vocabularies are not equivalent, and editing a Concept definition must not reinterpret old experience. Preserve the selected definition version or an immutable definition snapshot/hash with usable content. Translations and aliases may change presentation. Cross-Vocabulary mappings and unit conversions require explicit, versioned rules chosen by the reader; they cannot silently transfer a qualification or broaden a signed statement.

Recognition of a Vocabulary's meanings is also separate from reliance on an evidence issuer. Adopting a charity's Vocabulary does not make all its users truthful or all its receipts credible. Choosing an issuer does not mean accepting every Vocabulary or every interpretation they attach to an event.

## Contact-path searches and weighted analyses

Example query: “Show this contributor's visible food-assistance outcomes during the last year, in the selected Pantry Vocabulary, to receivers within three contacts of my chosen anchor. Separate Need-linked receiver confirmation from receiver-only confirmation and giver claims; apply my saved evidence and distance weights.”

Suggested query settings:

| Setting | Meaning |
| --- | --- |
| Viewer and subject | Who is searching and whose supported role/outcomes are being analyzed |
| Anchor identity or selected anchor set | Where contact distance starts; choosing an anchor never grants its permissions |
| Contact depth and relation kinds | Which permitted contact/introduction relations may be traversed, with a bounded maximum |
| Vocabulary/Concept versions, outcomes and units | What the contribution means and what quantities may be grouped |
| Period and freshness policy | Which event periods and available status checks qualify |
| Evidence categories | Need-linked receiver-confirmed, receiver-confirmed without that binding, or giver-claimed |
| Receiver/source policy | Directly selected issuers, path-qualified issuers, explicit exclusions and known affiliation filters |
| Weight policy | Evidence-level and distance coefficients, with a visible policy version |
| Work limits | Visits, path proofs, results, bytes, signature checks, deadlines and cancellation |

Use visible relations to calculate **shortest permitted observed distance**. For `Me → A → B → C`, C is three contact links from Me. C's Transfer with the contributor is a separate interaction edge; it is not a fourth contact link in this query. Network relays, devices within one Organ and the number of times a receipt was gossiped are not contact steps.

Contact-hop distance is also separate from the existing numeric Organ Proximity value and geographic distance. If a disclosed Proximity value is used to qualify a link, name whose value it is and keep it as an explicit query condition; do not add Proximity values along a path and call the result three contacts.

Contact graphs can be directed: A saying it knows B does not establish that B knows A or endorses A. Define the relation kinds and direction for the query. A claim-only introduction and a mutually acknowledged relation can be displayed differently; whether mutual acknowledgement is required is still open.

Every traversed relation and disclosed endpoint needs permission for the actual viewer. The viewer's own local contact choices can anchor a search; foreign relations need an attributable, authorized disclosure. Knowing A does not grant access to A's private contacts. A publicly visible receipt can be inspectable while its contact path is unavailable; an incomplete path must not be presented as verified three-hop proximity.

Recommend typed, signed contact/introduction disclosures with issuer, endpoint identities, relation meaning, audience, revision, expiry/status and any acknowledgement required by the selected relation kind. Reuse the ordinary Record/Assertion/identity/visibility families. These disclosures are new work; current local contact rows are not already portable signed social-path proofs.

Start traversal locally over permitted cached evidence. Optional remote searches can acquire more authorized artifacts, with shared work budgets and cancellation. A forwarder's assertion that a result is “three contacts away” is insufficient without the permitted path evidence. Relays may help deliver it without becoming members of that social path.

Cycles must not shorten a path by revisiting an identity. Multiple paths to the same receiver and repeated copies of an occurrence do not multiply either its quantity or its weight. Select the shortest qualifying visible path for a distance-only policy, retaining alternative paths only for inspection. A candidate appearing in its own referral path is a relevant dependence to display, not independent corroboration.

### Evidence levels, status and calculation

| Category | What it supports | Recommended treatment |
| --- | --- | --- |
| Need-linked receiver-confirmed | An authorized receiver confirmed defined fulfillment of its signed Need and actual amount | Strongest agreed contribution category; settlement and disclosure conditions still apply |
| Receiver-confirmed without the required Need binding | That receiver reported the stated outcome | Separate category; do not silently treat it as Need fulfillment |
| Giver-claimed | The giver reported delivering the stated contribution | Separate category with lower default influence; giver labels never become receiver-endorsed meanings |
| Proposed, agreed or pledged | An intention or commitment | Visible as pending work, with no completed contribution quantity |

Dispute, correction, reversal, expiry, conflict and missing status are separate lifecycle dimensions, not just another number in the evidence-level scale. A receiver-confirmed artifact can later be disputed or have stale status. The query must show the available state and its treatment.

The user approved configurable evidence and contact-distance weights. Recommended first formula, within a single compatible outcome/unit group:

`weighted_total = sum(eligible_actual_amount × evidence_weight × contact_distance_weight)`

Also show the unweighted amount, qualifying occurrence count, evidence breakdown, receiver identities and period. A weighted result is a policy-derived index with an explicit unit basis, not an amount delivered or a calibrated probability. For example, a 40 kg receipt with a combined weight of 0.3 yields an index of 12; the original receipt still says 40 kg. Call it “weighted experience index, kg basis”, not “12 kg donated”. Do not add different outcome/unit groups into a universal scalar.

Recommend finite, nonnegative, bounded decimal coefficients, with receiver-Need confirmation weighted at least as strongly as an otherwise comparable giver claim in the supplied presets. Exact presets and bounds remain open. Users can inspect, save and compare policies; editing a policy must not rewrite the evidence or another person's view. Invalid signatures, unauthorized actors, denied data and incompatible meanings cannot be made eligible by a large weight.

Weighting alone does not resist inflation: many fabricated receipts can outweigh a few real ones even with low coefficients. Keep the raw result traceable and make optional influence caps part of the policy discussion below.

### Why three contacts is useful but insufficient

| Failure mode | Consequence | Recommended response |
| --- | --- | --- |
| One known contact connects to a colluding group | Many fake receivers become close to the anchor | Show the common entry contact; consider optional influence limits by entry branch and receiver |
| Several paths share actors or control | Apparent corroboration is dependent | Show overlap and declared affiliation; count each event once |
| Someone adds false social relations | A fabricated path looks short | Verify signed disclosures, binding, relation kind and any required acknowledgement |
| Private or unavailable graph edges | The true shortest path cannot be known | Report the observed permitted path and partial coverage |
| A selected recipient signs fictitious Needs and receipts | Valid signatures accompany false physical events | Keep issuer accountability and optional witnesses; do not claim physical proof |
| One group repeatedly circulates resources | Throughput may be mistaken for unique goods or net benefit | State the outcome measured, show repetition and retain any permitted provenance |
| Search exhausts its budget | Some real evidence is missing | Show partial results; do not claim an exhaustive total or penalize absent evidence |

Optional policy controls to refine: limit weighted influence per receiver/activity/period, limit influence entering through the same first contact, and require an explicitly chosen confirmer for high-impact classifications. Preserve raw quantities alongside any capped index. Such caps can suppress genuine large-scale contribution, so they are analysis choices with visible tradeoffs, not corrections to historical facts or automatic accusations.

Research such as SybilLimit obtains social-graph defenses under assumptions about graph structure and the links between honest and malicious regions. An arbitrary three-hop contact search does not establish those assumptions. This supports studying entry-point dependence, while keeping Lince's initial guarantees limited to its actual evidence and policy checks. [SybilLimit, Yu et al.](https://www.comp.nus.edu.sg/~yuhf/yuh-sybillimit.pdf).

EigenTrust aggregates peer experience into a global score and uses selected pre-trusted peers to address malicious collectives. It is useful background, but its global-score design is not the agreed Lince product model. Adopt neither its guarantees nor its trusted-peer assumptions merely by adding gossip. [EigenTrust, Kamvar et al.](https://nlp.stanford.edu/pubs/eigentrust.pdf).

## Receiver-owned Needs and inflation resistance

The strongest metric should use a receiver-authored **Need outcome manifest**: the precise permitted meaning and requested scope of a Need, approved by its authorized actor. This is a signed projection/binding of an ordinary Need, not a new kind of economic object.

Recommended contents: stable public or audience-specific Need reference, author/authority, Need revision, exact Vocabulary/Concept definitions, relevant Assertion values/objects/conditions, compatible units, requested quantity or scope, occurrence/allocation binding, publication terms and signature. The full private Need and its internal UID need not be disclosed.

The receiver's receipt names that manifest, the giver, occurrence and accepted outcome/amount. It must say which meanings were accepted. A giver attaching `ethical`, `nutritious` or ten extra social-value tags cannot cause those meanings to enter the strongest metric unless the receiver actually confirms them under the agreed binding. Preserve the giver's assertions as its own claims.

Recommended constraints:

1. Establish the receiver's requested meaning before accepting the credited fulfillment; bind the agreed occurrence to that revision. Changed meanings need a new attributable revision and appropriate agreement.
2. Confirm actual partial amounts, with stable fulfillment identities. Replayed receipts, revisions and settlement slices do not create new fulfillment.
3. Where a Need has a declared capacity, track its confirmed consumption across linked occurrences and givers. Excess amounts need a revised/new requested scope or appear separately; they must not silently fulfill the same capacity twice.
4. Keep supported Need fulfillment distinct from resource throughput, giver effort, money spent and independently reported beneficiary outcomes. One event can support several views without inventing several physical results.
5. Publish only the approved manifest fields. Hidden capacities, beneficiaries, source Records and contact links must not leak through summaries.

These constraints prevent unilateral tag inflation and many duplicate or inconsistent accounting claims. They do not prevent a receiver from inventing a Need, enlarging its capacity or colluding with a giver. Need existence is a signed request and commitment, not proof of genuine deprivation or social benefit. Legitimate spontaneous contributions without the required prior binding remain visible in another category; absence of a Need link is not an accusation.

Self-confirmed work and confirmed work between affiliated actors can be useful history, but must not be displayed as independent recipient corroboration. Extra Cells within one Organ cannot become extra recipients or witnesses by changing device keys. Unknown control relationships remain unknown; do not label apparently different keys as proven independent people.

Ordering also needs precision. A manifest referenced by an agreement is cryptographically bound into that agreement, but issuer-supplied timestamps do not prove when physical events occurred. Local first-observed times, signed acknowledgements or optional independent observations can add context without becoming a compulsory timestamp server.

### The decentralized double-confirmation problem

A receiver with offline Cells could sign overlapping fulfillment against the same Need on different devices. Deduplicating event IDs detects repeated copies, but not two distinct IDs claiming the same remaining capacity. An attacker can also sign inconsistent cumulative histories.

Recommendation to investigate: designate one authorized fulfillment writer for a bounded Need epoch, or use explicit disjoint allocations to approved Cells, then sign revision/cumulative-consumption references. Follow existing single-writer/roster or allocation patterns where they fit; do not claim the current stock-limit feature already implements receiver Need capacity. This trades some offline availability for a checkable accounting invariant and is not a mandatory central service.

Gossip can expose contradictory signed branches and update a view after reconnection. It cannot guarantee that an undiscovered branch does not exist. On a known fork, keep the conflicting artifacts, suspend the conflicting capacity-bound classification and require an authorized resolution rather than choosing whichever reply arrived first.

If the capacity and consumption proof are private, a remote viewer cannot independently verify the whole bound. Show the receiver's confirmation without claiming public capacity verification. A privacy-preserving proof of a hidden capacity is an advanced optional project, not implied by hashing the Need.

## Signed events and bounded gossip

Exchange minimal, versioned artifacts that preserve who said what:

- Need outcome manifests and approved revisions.
- Participant statements about delivery and receiver statements about accepted outcomes.
- Any agreed minimal settlement/application proof needed by that evidence category.
- Permitted contact/introduction disclosures for path verification.
- Signed corrections, disputes, status/withdrawal updates and identity succession/revocation evidence.

Each statement carries its own signer role and immutable content hash. A co-signed summary is one convenient presentation, but settlement's origin signer cannot impersonate the receiver's meaning confirmation. Reuse existing participant proofs when they cover the exact statement; otherwise obtain the missing authorization/confirmation explicitly.

Reuse consented gossip's offer/fetch, deduplication, bounded queues and control-update prioritization. Relay the original signed bytes, not a relay-authored rewrite. A carrier's identity is immediate discovery provenance; it does not acquire authorship, recipient authority or contribution experience. Verify artifacts locally before admitting them to a view.

Separate permission to read from permission to index or redistribute. A viewer permitted to inspect a receipt may lack permission to gossip it or its contact path. A private reference or encrypted transport does not grant publication rights. Keep bounded control retention so an old receipt cannot easily reappear after a known withdrawal, according to the still-open retention policy.

The current [social gossip contract](../../crates/nucleus/src/social/gossip.rs) carries public announcements and their authorities, not trust events or signed contact paths. The [contact-query contract](../../crates/nucleus/src/social/ask.rs) already has work, byte, result and forwarding-depth limits, but returns announcements without social-path proofs. Its current forwarding-depth constant is not a verified three-contact relationship metric. Add the approved artifact/query types and their authorization; do not simply raise a depth constant and claim the trust workflow is complete.

Recommended local processing order: permission and size checks; signature/identity/role verification; immutable meaning and event bindings; known status/conflict checks; permitted path resolution; stable fulfillment deduplication; query eligibility; raw grouping; optional weighting/caps; explainable presentation. Cache keys must include the viewer, query policy, meaning versions, path/issuer revisions and evidence/status revisions. Re-evaluate after access changes rather than serving a stale permitted summary.

An Institute node can help find public artifacts and control updates through this same protocol. It neither needs to see private social graphs nor becomes the issuer of a trust value. Offline or incomplete searches remain useful if their coverage and last-check information are shown.

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
| Contact search forwarding | Consented asking/answering/forwarding with shared work, byte, result and depth bounds | Verified social-contact paths or trust-artifact queries |
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

Agreed: keep these responsibilities separate in storage and UI. A valid signature can accompany an unreliable statement. A known contact can be a poor choice for one service. A reliable driver is not automatically a reliable medical adviser or a careful recommender.

The W3C Verifiable Credentials model explicitly separates cryptographic verification from the truth of a claim. We can adopt that distinction without adopting its entire credential stack. [W3C verification terminology](https://www.w3.org/TR/vc-data-model-2.0/#terminology).

## What more nodes actually contribute

One Lince can check a signature and the supplied protocol evidence locally. Repeating that calculation on a hundred nodes does not add a hundred independent observations of a ride.

More nodes help with availability, obtaining revisions and corrections, comparing conflicting signed versions, and finding evidence that one source omitted. Those benefits depend on reaching sources with different information; the number of endpoints alone is not proof of independence.

An actual witness contributes a different kind of evidence: a signed account of what they observed. We must record what they witnessed, their relationship to the parties when disclosed, and why the reader relies on them. A server that cached a receipt is a carrier, not a witness to its physical event.

One operator can create many keys, Organs, devices and fake interactions. Douceur's Sybil analysis explains why redundancy based on unknown electronic identities can fail. For Lince, the consequence is to avoid node-count voting and avoid presenting distinct keys as proven distinct humans. [The Sybil Attack, 2002](https://www.microsoft.com/en-us/research/wp-content/uploads/2002/01/IPTPS2002.pdf).

Recommendation: verify locally, discover through several selected sources when useful, and make the evidence and its limitations inspectable. A majority of available nodes should not decide a person's reputation.

## The smallest useful first version

Refined starting scope: **Transfer evidence searches and analyses**, with activity/unit summaries, optional judgments, conservative direct-source presets and bounded contact-path policies with configurable evidence/distance weights.

1. Inspect your own permitted interaction history with a party.
2. Inspect minimal published Transfer summaries offered by that party or discovered through chosen sources.
3. Check signatures, identity bindings, roles, revisions and available status locally.
4. Distinguish participant-confirmed results from unilateral accounts, disputes and missing evidence.
5. Search permitted contact paths and compare raw and weighted summaries by activity/unit, showing the actual receiver confirmations, Need bindings and policy.
6. Optionally save your own judgment for the activity you care about, with reasons and references.
7. Use this information to filter or review Transfer candidates without changing permissions or agreeing to a Transfer automatically.

This avoids requiring a general-purpose claims registry. It still involves statements: delivery and receipt are already claims in Lince. The distinction is between a small vocabulary tied to the Transfer protocol and an unrestricted system where anyone can make arbitrary assertions about anyone.

Recommendations, qualifications and witness statements can be added later as generic signed Record evidence. Their existence should not force the first version to solve universal truth, social graph scoring or every kind of credential.

Unknown means insufficient information. It should not become a negative judgment, especially for new users, private participants or people whose history is unavailable offline.

## Identity and who owns a judgment

Agreed ownership direction: a judgment has an author and a subject, each explicitly identified as a Person or Organ. The proposed data model also identifies its activity, role or Concept. A Cell/node is a device or endpoint, not the subject of a person's service reputation.

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

Possessing or forwarding a receipt does not make its holder a participant or contributor. Verify the subject, role and authority inside the signed evidence, not the identity of whoever supplied the copy. A participant's history must remain attributed when any other node carries it.

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

Jointly reviewed receipts are the initial publication plan; the complete consent policy remains unresolved. Requiring fresh consent from every identified participant for every later update protects private history, but lets a participant prevent an unfavorable event from appearing. Allowing unilateral publication supports warnings and accountability, but needs careful handling of evidence, privacy, harassment and replies. A positive co-signed portfolio and a contested account should have separate meanings. Operator moderation reports are not a substitute for this decision.

Recommendation for the first increment: personal unfavorable judgments remain private unless a separate publication mechanism is explicitly chosen. Do not promise an objective failure rate or complete history under a selective publication model. Say “visible published history”, not “all Transfers” or “no disputes”. Joint receipt planning does not settle a right to publish a unilateral allegation.

Recommended improvement to refine: an optional **public-history mode** with a publication agreement reviewed before the occurrence. Identified participants approve the minimal event identity, roles, evidence fields, audiences, retention and which later status statements may be published. Delivery/receipt confirmations, corrections and disputes within that agreed scope then need the appropriate actor's signature, without a fresh unanimous approval of every unfavorable update. Adding new fields or audiences still needs a new publication decision.

That mode must not publish all Transfer contents. A public Contribution announcement alone is not consent to publish a buyer, beneficiary or location. A signed dispute is an attributable dispute, not a protocol verdict that a scam happened. Missing confirmation remains missing evidence; silence is not automatically refusal, failure or dishonesty. The precise withdrawal rights and status-retention rules for this mode are open.

A public-history agreement can reduce selective omission within an agreed event, but cannot force availability, stop off-platform activity or guarantee that all of a person's past interactions are known. Closed queries and discovery sources still need honest coverage labels.

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

Contact-chain analyses are approved as evidence searches. Recommendations about who is reliable remain a separate later feature: a recommendation is a signed opinion with reasons and optional evidence, and must retain its original issuer through discovery and forwarding. Avoid unrestricted recursive propagation: cycles, several paths to the same issuer and many Cells belonging to one Organ must not multiply its weight.

Any threshold based on several recommenders needs a user-selected issuer set and a clear independence assumption. Different keys and an apparently diverse graph are insufficient to prove different real people.

## UI and integration with ordinary Lince features

Recommended UI proposal, requiring refinement before implementation:

- A reusable Trust Castle for a selected Person or Organ, with identity/binding, permitted portfolio, personal/Organ judgments, discovery sources and verification details.
- A compact Transfer party summary showing your relevant judgment and the evidence behind it. Transport supplies a context and role to this shared component.
- An experience summary by activity/unit with the selected issuer policy, period and visible supporting receipts. No universal social-coin balance.
- A saved analysis query with anchor, maximum contact depth, relation types, selected Vocabulary/outcomes, period and explicit evidence/distance weights. Show raw quantities, weighted indices, partial coverage and which path qualifies a result.
- A receiver confirmation form that starts from its approved Need meanings and actual accepted amount. Extra giver assertions remain separately attributed; confirming a receipt does not silently endorse them.
- Controls to recognize an identity separately from relying on its confirmations for selected activities. Include preview of which evidence the choice would add or remove from the local view.
- A reviewed “Share portfolio receipt” action after a suitable occurrence, distinct from settlement and from a review of the provider.
- An “Inspect evidence” view that explains signer, role, context, proof basis, identity binding, status and immediate discovery source in plain language.
- Source settings and manual refresh, with offline, partial, stale and conflict states visible.
- Optional Protein filters over permitted evidence and local judgments. Explain why a result is included or excluded.

Keep identity binding, signature validity, content conflict and freshness as separate result fields. A receipt can have valid signatures, unresolved identity and stale status at once. Avoid one unexplained green “trusted” badge.

Transport example: a passenger reviews a driver's visible transport receipts and their own previous experience. They choose whether to proceed. After the journey, both may approve a minimal result receipt. Live location remains separately consented and is excluded from that receipt. The same mechanism works for lending a tool or completing a delivery.

Recommendation: first use trust for inspection and candidate filtering. If automatic selection or agreement is later approved, use explicit Karma rules with reviewable context, thresholds, freshness and an unknown-data policy. New endorsements must not silently change agreements or access controls.

## Sequential implementation proposal

Implementation is not authorized by this document. Each step needs a settled meaning and usable frontend/backend behavior.

1. **Define the remaining semantics and examples.** Activity/unit analysis, Vocabulary-bound outcomes, a Need-linked receiver-confirmed category, direct-source presets, bounded contact searches and configurable evidence/distance weights are agreed. Settle contact-edge meaning/direction/acknowledgement, weight presets, publication lifecycle, counting unit and treatment of disagreements.
2. **Audit identity, evidence and meaning bindings.** Trace Person/Organ bindings, signer roles, key history and the difference between service claims, application attestations and package receipts. Establish stable Vocabulary/Concept definition references for agreed outcomes, including any Assertion values/objects/conditions and units. Identify any missing authorization, meaning-version or lifecycle checks.
3. **Build local inspection and receiver outcome binding.** Use permitted existing Transfer data, explicit actor/context ownership and a reusable evidence view. Keep receiver Need meanings and confirmations distinct from giver claims. Add conservative activity/unit summaries, source choices and optional judgments. Work offline and without maps or discovery servers.
4. **Build reviewed portable receipts.** Add minimal typed Record proof data, signing/consent actions, verification, safe import/export, correction and withdrawal. Add new migrations only if necessary; never rewrite existing ones.
5. **Add path proofs and local analysis.** Implement authorized contact/introduction disclosures, bounded visible-path traversal, eligibility/deduplication, saved query policies and raw/weighted summaries. Reuse generic Record search/aggregation and Protein where suitable; do not put transport-specific scoring in the trust engine.
6. **Add optional discovery and gossip.** Extend selected-source publication/search, bounded contact queries and consented gossip with the approved artifact types. Prioritize corrections and prove paths where disclosed. Run an ordinary Institute node as one available source.
7. **Integrate candidate filters and measure.** Reuse Protein and Transfer components; Transport contributes its context. Validate manipulation, privacy, partial results and resource limits. Keep permissions and agreement authority independent.
8. **Add further controls or automation if chosen.** Refine influence caps and explicit endorsements separately from contact paths. Specify authority, freshness and unknown-data handling before automatic selection or agreement.

Avoid a new trust-specific ledger, bespoke Transport scores, compulsory centralized identity verification or a global majority vote. Shared families are Records/Assertions/Facts/Actions, identity/roster, Transfer evidence, visibility/disclosure, and social publication/discovery.

## Laboratory and adversarial verification plan

No new trust tests have run: this is a design task. When implementation is approved, use at most four Linces per topology and add native Laboratory cases through shared fixtures.

| Scenario | Required result |
| --- | --- |
| Provider, receiver, reader and discovery node | Reader verifies a permitted receipt locally; provider/receiver can share it directly if the node is offline |
| Provider and receiver with two discovery nodes | Replayed copies count once; a conflicting signed revision remains inspectable rather than becoming a vote |
| One operator represented by multiple Cells/keys | Extra replicas and signatures are not shown as extra independent humans or interactions |
| Known identity versus similarly named stranger | History is not attached by display name, slug or unanchored included key |
| Valid signature from an unknown bot pair claiming enormous donations | Evidence remains attributable but contributes nothing to the reader's accepted-experience totals without selected issuers |
| Recognized charity not selected for the activity | Identity recognition does not silently grant evidence-issuer reliance |
| Selected charity receipt for food, unrelated seller or driver context | Food quantities remain food experience; they do not inflate other activity summaries |
| Reader changes their chosen activity/issuer policy | The local view recomputes with a clear explanation; other readers and the underlying evidence are unchanged |
| Receipt copied by an unrelated holder | The signed subject gets attribution; the holder does not acquire the contributor's experience |
| Duplicated receipt, split settlement, incompatible units, circular goods movement | No duplicate occurrence credit or incompatible unit sum; multiple genuine legs are not silently presented as additional unique goods or measured benefit |
| Same label from different Vocabularies, changed definition, translated alias | Evidence stays attached to the selected meaning/version; no silent equivalence or historical reinterpretation |
| Tags added after agreement or many overlapping outcome tags | Unconfirmed Assertions do not gain experience; category views do not multiply occurrences or quantities |
| Partial confirmed contribution with settled accounting | The supported amount enters the relevant role view; unconfirmed amounts and downstream effects remain distinct |
| Public-history mode with an unfavorable status update | Approved fields follow the settled publication agreement; a dispute remains the issuer's account and no private fields escape |
| Four Linces arranged as reader → A → B → receiver, with a contributor among the existing identities | Three permitted social links qualify the receiver; the Transfer and transport/gossip hops are counted separately |
| Looping paths, repeated routes and receiver reachable through several contacts | Shortest qualifying observed path is explainable; one occurrence contributes once |
| Known first contact introduces a colluding collection of actor identities | Short paths do not imply independence; raw/weighted policies and optional branch caps behave as declared |
| Unilateral, mutual, expired, blocked and private contact disclosures | Traversal follows the chosen relation semantics and actual viewer permissions; hidden endpoints and counts are absent |
| Giver appends many Assertions; receiver confirms only its Need's meanings | Only receiver-confirmed outcomes enter the strongest category; other assertions remain giver claims |
| Same Need capacity confirmed twice using different offline Cell branches | Known contradictions suspend the capacity-bound classification; duplicate IDs and forks have distinct handling |
| Weight policy changes or path becomes shorter/longer/unavailable | The derived index changes with a visible reason while raw evidence remains attributed and no delivered quantity is rewritten |
| Depth/result/byte/signature budget exhausted or one source offline | The query stops promptly and labels partial coverage; absence is not a negative judgment |
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

The 2026-10-12 discussion accepts the core plan and explicitly settles activity/unit summaries, evidence/distance weighting and Need-linked receiver confirmation. The latest instruction expands direct-source views with bounded contact-path analyses. Fine details remain recommendations until answered; this document does not authorize implementation.

| Order | Question | Current recommendation | Status |
| --- | --- | --- | --- |
| 1 | Start with Transfer evidence and personal judgments, or include others' recommendations immediately? | Evidence and personal judgments first | Agreed direction |
| 2 | Who owns judgments: Person, Organ, or both? | Both, with separate authorship and authority | Agreed direction; exact shared-Organ privacy/authorization open |
| 3 | What consent is needed to publish an identified Transfer account? | Jointly reviewed receipts initially; consider reviewed public-history terms before an occurrence | Initial direction accepted; unfavorable accounts and withdrawal rights open |
| 4 | Which dimensions matter first: reliability, quality, safety, or simply whether you would interact again? | One activity-specific assessment plus reasons; avoid pretending to measure safety | Open |
| 5 | Should judgments use categories, numbers, or a configurable vocabulary? | A small editable vocabulary before numerical aggregation | Open |
| 6 | What is the unit of history: Transfer, occurrence, or fulfilled promise? | Occurrence per subject/role; show Transfer count separately | Open |
| 7 | How should disputes, cancellations and compensation appear? | Attributed status and context; no automatic penalty | Open |
| 8 | What may a receipt identify about counterparties, time and context? | Minimum reviewed fields; addresses and live positions excluded | Open |
| 9 | Are judgments private to a Person, shared within an Organ, or optionally published? | Private by default; specify the actual storage/audience boundary | Open |
| 10 | What can a signed correction retract, and for how long must status be retained? | Retract the issuer's statement/publication, retain bounded current status; settle retention periods | Open |
| 11 | Should Lince ship an Institute discovery source enabled, suggested, or absent? | Suggested and replaceable; require a deliberate first remote lookup | Open |
| 12 | Which relations qualify a visible contact path, and do they need acknowledgement? | Typed, attributable and permitted relations; contacts and endorsements have separate meanings | Contact-path search agreed; exact edge rules open |
| 13 | Can trust automatically select parties or change agreement levels? | Inspection/filtering first; explicit Karma automation later | Open |
| 14 | How should a new party with no visible portfolio participate? | Show unknown; allow manual interaction and ordinary direct introductions | Open |
| 15 | Universal social-coin balance or experience by activity and unit? | Activity/unit experience with optional local summaries | Explicitly agreed: activity/unit experience |
| 16 | Whose confirmations enter a view? | Directly selected issuer preset plus separately configured visible contact-path analyses | Earlier direct-only scope expanded by the current instruction |
| 17 | Should a public Transfer agree up front which minimal lifecycle/status evidence can be published? | Optional public-history mode; no automatic publication from a public announcement | Open |
| 18 | What can quantity and social-value summaries claim? | Preserve compatible units and precise confirmations; community/local judgments carry their definitions | Exact summary/badge vocabulary open; no universal value measure agreed |
| 19 | Which Vocabulary Assertions/units define a creditable outcome, and who confirms each? | Bind exact meanings/versions to agreed outcomes; the receiver confirms its Need's actual fulfillment | Agreed direction; exact binding format and lifecycle open |
| 20 | Can contact-path views apply evidence and distance weights? | Configurable local policy with raw quantities, evidence levels and an explainable weighted index | Explicitly agreed: configurable weights |
| 21 | Does the strongest category require a receiver-authored Need? | Signed Need meaning/scope and receiver confirmation of actual outcome and amount | Explicitly agreed: Need-linked receiver confirmation |
| 22 | How should Need capacity work across offline Cells? | Investigate one fulfillment writer or explicit disjoint allocations, with fork evidence | Open; gossip alone cannot guarantee global consistency |
| 23 | Should one receiver or entry contact have a weighted influence limit? | Optional per-view caps with visible raw amounts and declared scope | Open; evaluate manipulation resistance and legitimate scale |

Next refine what a contact edge means and which direction/acknowledgement it needs, then receiver Need capacity/fulfillment identity, weight presets and optional influence caps. Settle publication/withdrawal rights and retention alongside the proof schema before remote sharing. Preserve the agreed activity/unit analysis model and visible distinction between reported quantities and policy-derived indices. The document should accumulate actual decisions without turning unanswered recommendations into requirements.
