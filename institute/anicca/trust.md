# Transfer completion, trust and Transport

Implementation authorized 2026-10-14. The decisions and ordered checklist in this opening section supersede unresolved recommendations in the retained research discussion. Complete phases in order, with usable native controls and backend checks together. Remove only verified completed tasks; retain deferred work and concrete blockers. The owner-authored Records remain unchanged.

## Approved architecture

Trust means reader-owned search and analysis of permitted Transfer history. It is not a currency, global balance, physical-truth guarantee or server verdict. Reuse Records, Assertions, Facts, Actions, identity/roster, Transfer outcomes, visibility/disclosure, Protein and bounded social discovery. Transport is a preset over those shared families.

| Concern | Settled behavior |
| --- | --- |
| Meaning | Immutable Vocabulary/Concept definition revision/hash, unit versions and explicit conversion versions. Reuse existing Vocabulary revision work. Labels, aliases and translations never imply equivalence. New quantities and analyses use exact decimals. |
| Audience | Authenticated Person and whole-Organ principals. Includes combine with OR, criteria within a condition with AND, exclusions win; private by default. Outer Record, field, saved-place, live-location, publication and redistribution gates remain independent. Organ membership alone cannot consume a Person grant. Explicit administration is separate. |
| Amounts | Proposed, giver-reported, receiver-confirmed, canonical-settled and private-effect amounts remain distinct. Public #x may privately fulfill #y through existing policies for up to 32 distinct local Records. Requested 10/received 12 are two facts, not a global quota violation. Private compensation does not establish physical non-delivery. |
| Strongest evidence | Receiver-authored Need binding to precise meanings/units plus receiver confirmation and attributable settled occurrence. Giver tags do not create receiver endorsement. Application acknowledgments and package receipts do not establish physical outcomes. |
| Counting | Stable source occurrence/outcome deduplicates replicas, corrections, observer copies and settlement slices. Preserve conflicts instead of averaging or voting. Repeated transfers, quantities and measured benefit are separate views. |
| Proof | Typed capabilities of ordinary Records, deterministic domain-separated signatures, original bytes, checked identity/role/key history. Redacted data needs its own minimal signed receipt. Imported evidence/recipes cannot execute Actions or scripts. |
| Analysis | Filter subject, role, meaning/version, unit, period, stage, evidence class, sources, counterparties and contact paths. Always show raw quantities beside policy-derived indices; indices are neither delivered quantities nor fraud probabilities. |
| Weights | Configurable defaults: Need-linked receiver 1, receiver-only 0.5, giver claim 0.1. Direct-contact weight 1, halved each additional hop. Strongest evidence category selected initially. Optional issuer and first-entry-contact influence caps start disabled and affect indices only. |
| Contact paths | Mutually acknowledged, deliberately disclosed relations by default; unilateral relations opt-in and separate. Default depth 3, hard maximum 6. Deterministic shortest permitted observed path; multiple paths do not multiply an outcome. Show common first contacts, repeat counterparties and disclosed affiliations. Social hops differ from network relays, Organ Proximity and geographic distance. |
| Filters | Separate query-endpoint, original-issuer, counterparty and intermediary include/exclude choices, plus exclude-everywhere. Known/unknown selectable; closeness is not reliability. Network blocks and consent remain binding. Strict endpoint exclusion uses direct queries; malicious remote internal behavior cannot be certified. |
| Budgets | 1024 identities, 5000 events, 8 MiB aggregate, 4 concurrent requests, 15-second remote deadline, bounded individual artifacts/signature work. Report partial coverage explicitly. |
| Claims | Signed ordinary Records linked from profiles; Person and Organ subjects, no automatic member-history inheritance. Exact/at-least comparator, amount/count, meaning/unit, role, period/as-of, measurement basis, issuer, revision and recipe references. A visible 10 against claimed 100 means partial support, not proof the rest never happened. |
| Automation | Public counters auto-update only within reviewed input/field/audience/measurement recipes. Scope changes require review. Reader policy remains independent. New local evidence may recompute locally; remote Ask around happens only on deliberate Refresh, without background polling. |
| Publication | Private default; optional public-history terms agreed upfront. Correctly signed later statements, disputes and corrections within that scope need no opponent veto. New fields/audiences require new authorization. Default public receipt identifies both approving Organs and omits private Persons, locations and private Record mappings. |
| Retention | Default proposal one year, configurable within custodian limits. Honor agreed fixed public terms; corrections attach and either party can refuse renewal. Minimal control metadata survives serving expiry plus 30 days. Disclosed data cannot be recalled technically. |
| Custody | Opt-in ordinary Organ automatically accepts under configured sender/audience/storage/retention rules. Acknowledgment identifies actual retained artifacts and expiry. Custodian is not a physical witness or credited contributor. Observing dependencies and public-copy authority are separate. |
| Helper | Removable, disabled-by-default seeded Institute configuration using an operator-supplied valid identity/endpoint. No hardcoded judge/key/live endpoint. Reuse ordinary headless Lince and bounded consented gossip; prioritize corrections. Removal survives restart; direct exchange works without a helper. |
| Location | Explicit source-device selection and source approval. Cross-clock-safe bounded ages, epochs/sequences and freshness challenges; retain 60-second fix maximum. Elapsed accounting includes suspension or invalidates the fix. Accuracy and intentional radius are separate. Handover ends the old epoch and needs new approval. Private invitations review recipients/precision, including outsiders. |
| Sharing lifetime | Stop on explicit journey finish, cancel, manual stop or expiry; settlement may continue afterward. Existing one-hour default, one-day maximum, no silent restart. Saved place and live sharing remain separate normal-Record capabilities. |
| Transport | Generic Need transport/Offer transport preset: origin/destination Record or #address, pickup window, passenger count/capacity. Reserve through existing Agreed policy, release on cancel/finish/expiry; unactivated booking grace 15 minutes after pickup-window end. Progress events are not receipts, settlement or publication consent. |
| Discovery | Typed approximate geography/time announcements through Protein. Immediate expiry 15 minutes; scheduled pickup-window end plus 15 minutes, maximum seven days. List works without map/helper. History is advisory; automatic selection/agreement only through explicit existing Karma rules. |
| Map/mobile | Keep Rust Bevy/Galileo, local Natural Earth overview, no buildings, Shortbread MVT directories/PMTiles/optional HTTP and theme tokens. Finish desktop maps before mobile. Transfers and location work without a map. |

## Ordered implementation checklist

Existing migrations and checksum entries are immutable. Before a schema addition re-enumerate migration numbers, append a new migration and its exact SHA-384, then run `cargo check -p store` and migration-guard tests before dependent work. Do not count historical Laboratory reports as current verification.

### 2. Outcomes, portable receipts and local history

- [ ] Sign/export/import minimal publication-approved receipts, verify original authority and scope locally, retain contradictions/corrections and deduplicate source outcomes. Evidence import executes no Actions. Observer-only commitments contribute no resource quantity.

For portable receipts, do not treat a copied source extension or an arbitrary embedded key as verified history. The local source index is not an import protocol. Include the original role/activation/settlement and signing-authority proofs needed to check the particular minimal claim, with explicit publication approval. Current public Transfer proposal/settlement amounts still use their existing floating-point interfaces; new exact outcome amounts must remain distinct, and exported canonical quantities must declare and preserve their actual recorded precision rather than inventing recovered decimal accuracy. Remote receiver-owned Needs need an authorized signed proof path rather than requiring the origin Cell to own or inspect a foreign private Need.

Local outcome implementation contract, 2026-10-15:

- Reuse `nucleus::transfer::outcome`, `store::transfer_evidence` and the native Transfer Castle's outcome controls. Need bindings and giver/receiver statements are ordinary zero-quantity Records carrying the `lince.transfer-outcome` capability. A new append-only index retains original payloads and their existing signed Facts; it is not another resource ledger.
- Binding requires the receiving Person's own negative-quantity Need, its exact reviewed Record revision and a current assertion. Pin the assertion's historical Concept/unit definitions in a named Vocabulary. The submitted binding also carries the reviewed measurement references: changing a Record unit's definition without editing the Need must still invalidate that review. An assertion's explicit positive amount is the requested amount; otherwise use the Need's magnitude and its unit. Preserve both the original negative Need quantity and the selected assertion's requested amount. Local-only `g_local` meanings need adoption into a named Vocabulary before use here.
- A statement explicitly selects giver or receiver, exact meaning/unit/object references, a nonnegative decimal amount, an optional receiver-authored binding and an optional preceding version. Zero received and withdrawing an earlier statement are distinct. A Need-linked statement must match the binding's exact measurement. A new or corrected binding never silently upgrades an earlier statement.
- Corrections retain occurrence, author, role and measurement. One root per occurrence/author/role/measurement prevents repeated roots from multiplying credit. Competing corrections remain competing visible heads; later analyses must not add or arbitrarily choose them. There is no global Need quota: requested 10, giver-reported 100, receiver-reported 12 and canonical settlement 10 can coexist as different statements/stages.
- Default Record visibility grants only the author, and that initial audience is recorded in the normal sync log alongside the new Record. Transfer history requires access to the evidence Record and its referenced Need/binding/object/predecessor; sharing the statement alone does not expose its private Need mapping through that history. Ordinary Record visibility controls are reused. Public minimal receipts require a separately reviewed projection; never export these private source payloads wholesale as public receipts.
- Existing Transfer revision/activation proofs and author signing keys or verified signed Action intents remain the authority path. Requests are idempotent, stale reviews fail, and a replay does not publish a second Fact notification. Reporting does not set the existing completion flags, settle a Transfer, or execute private effects. Observation-only Transfers without a resource occurrence cannot produce these resource statements.
- Need choices are bounded to 64 and local history to 256 permitted Records, with explicit partial indicators. This is a local origin-Cell increment. Portable authority bundles, remote receiver-authored Need proofs, minimal Organ-level receipt review/export/import, publication terms, bounded Ask around and history analysis remain the next work. No public trust score or strongest-evidence total is computed from these local controls.

Validation for this increment: six new engine outcome tests passed, including a Need for 10, giver report of 100, receiver report of 12 and canonical settlement of 10; eighteen-place decimal precision; original signed Facts; wrong-party/role/hash rejection; stale Need/Transfer/unit-definition review rejection; private mappings and their sync-log audience; duplicate roots/replays; conflicting corrections and immutable originals. Existing sale/history tests passed (4), dependency/observer tests passed (3), private-effect group tests passed (2), and the new nucleus exact-amount test passed (1). The migration guard passed all 7 cases. The private-effect privacy test now checks a Forbidden result and absence of the hidden UID rather than historical English error wording. Store and native-presentation `cargo check` passed. The native exact-amount form case passed through the compiled native test runner under `nix develop .#interface`; it is also registered in the Laboratory catalogue. These are **24 targeted passing cases**. The earlier foundation Laboratory report does not cover this increment; no full Laboratory rerun or rendered-UI validation is claimed. The two completed local outcome/control tasks have been removed; portable publication-approved receipts are the next ordered task.

### 3. Analyses, contact proofs and profile claims

- [ ] Add signed mutual contact disclosures and bounded permission-aware traversal with deterministic shortest paths, expiry/revocation, loop handling and no route multiplication.
- [ ] Add saved reader-owned queries, purpose-specific source filters, raw/weighted breakdowns, optional influence caps and explicit partial/budget states; aggregate only identical versioned meanings/units/stages.
- [ ] Add manual/generated signed profile claims, reviewed publication recipes, exact/at-least comparisons, local recomputation and deliberate remote Refresh.
- [ ] Add native history, profile claims, Ask around/source controls and evidence explanations; integrate advisory filters through shared Protein/Transfer views.

### 4. Publication, custody, gossip and helper

- [ ] Implement upfront public-history terms, scoped unilateral corrections/disputes, fixed-term retention, renewal refusal and minimal expiry metadata. Keep private fields out of public signed payloads.
- [ ] Add custodian policy/actual-retention acknowledgment, bounded serving/storage, expiry cleanup and permission-aware correction/forwarding without observer contribution inflation.
- [ ] Extend normal selected-source discovery/ask/gossip with original proof artifacts and disclosed contacts, cancellation, consent and resource bounds. Prefer local evidence first.
- [ ] Add removable disabled helper seed requiring operator identity/endpoint; test removal persistence and direct operation when the helper is absent.

### 5. Location lifecycle and invitations

- [ ] Replace cross-clock wall-time assumptions with bounded age/challenge/epoch handling, replay rejection, suspend invalidation and authority freshness.
- [ ] Add explicit handover and private invitations with recipient/precision review and new-source approval. Preserve same-Organ Person isolation and distinct save/share controls.
- [ ] Connect stop behavior to journey finish/cancel/manual/expiry and preserve original measurement age, accuracy and sharing radius through every projection.

### 6. Generic workflow presets and Transport

- [ ] Add shared preset/progress capabilities and native Transport form with #address origin/destination, window, party/capacity and list-only operation.
- [ ] Integrate reservations/releases/grace period without turning progress into receipt or public consent; add geographic/time discovery, expiry, Protein filters and explicit Karma automation.
- [ ] Add three/four-Lince Laboratory scenarios for public/private quantities, Need meanings, partial receipt, #address, exact/approximate/outsider permissions, reservations, finish and helper failure.

### 7. Desktop maps

- [ ] Replace first-32 source limit with paginated viewport results and clustering computed only from permitted geometry. Hidden coordinates must not affect visible counts/distances/ranges.
- [ ] Carry original live age, measurement accuracy and sharing radius separately; add explicit Locate me distinct from save/share.
- [ ] Add regional pack manifests with coverage, zoom/detail, dataset version/hash, resource limits and credits; atomic install/replace/cancel preserves last working pack.
- [ ] Version caches by dataset, complete keyboard pan/selection/download area, focus/accessibility, Unicode, themes/scaling, resize/multimap and cleanup.
- [ ] Validate actual desktop portal/cross-node UI and measure dense-map performance/release-package sizes. Preserve the existing stack and optional configured external sources.

### 8. Mobile last

- [ ] Reuse shared map/Transport/evidence controls on Android, touch/pinch, accessible cards and surface pause/resume/recreation.
- [ ] Test actual-device denied/approximate/revoked location, lock sharing, notification Stop, process kill, network changes and completion; measure battery/thermal/network/memory/render behavior. Compilation alone does not fulfill device validation.

## Deferred after the ordered work

- Multi-stop journeys and capacity per segment.
- Replaceable routing/geocoding providers and globe rendering.
- General third-party recommendations/credentials.
- Advanced graph defenses and privacy-preserving proofs.

## Current implementation evidence

Foundation implementation, 2026-10-15:

- `nucleus::vocabulary::reference` adds canonical domain-separated meaning hashes and conversion references pinned to immutable definitions. Store APIs resolve selected historical versions. Existing Vocabulary work is reused.
- New append-only migrations `0021_conversion_versions.sql` and `0022_conversion_definition_binding.sql` preserve ratios, unit definition versions and withdrawals. Updates/deletes of saved history and changes to conversion identities are rejected. Changing a unit definition makes the current conversion unavailable until explicitly reviewed again, including when its ratio stays the same. Private application formulas still preserve their existing exact reviewed formulas.
- Shared visibility conditions now support Persons AND Organs/proximity, OR inclusions and exclusion precedence. Person grants cannot authorize an Organ-only read. A matching Person exclusion withholds whole-Organ replication, while other authenticated Persons can retain their own permitted views.
- Saving a first location or adding the first saved-address link as an authenticated Person initializes private access for that creator. Other members gain no access from membership alone. Existing policies are preserved on later edits; initialization never overwrites an exclusion or regrants revoked access. The common `ensure_private_for_person_on` helper also supports other normal-Record capabilities.
- Native Visibility controls select Persons and Organs and preview bounded combinations. The two UI checks are registered in the native Laboratory catalogue.
- Local Record read ceilings honor Person audiences. Saved and linked-place projections combine the subject's and target's area restrictions, redact precise address text, withhold private points and avoid selecting based on hidden candidates. Geographic search, role predicates and personal filters use projected centres.
- Authenticated saved-place reads revalidate device admission. Remote Person reads require the requesting device's current registration and active Person; explicit named grants can admit an outside device without treating an unknown Organ as a contact. Network blocks and device revocation remain binding. Raw Place reference authority is withheld for approximate/private access.
- A staged edit cannot turn a hidden raw Place ID into a readable point by attaching it to another Record; reference access is checked against the pre-edit state. Already authorized references still work. Whole-Record peer reads also honor existing role/actor/field restrictions and refuse an unscoped whole-Organ copy when Person-specific rules require a scoped view.
- Live location applies Person exclusions over direct invitations and honors an explicit outer Record gate. The location controller retains their own exact access.

Verified: `cargo check -p store` and `cargo check -p lince-interface --features native-presentation`; seven migration-guard tests; two conversion-history tests; five existing Concept/conversion tests; one historical meaning-reference test; two store audience/projection tests; three visibility-policy tests; one meaning-hash test; eighteen engine location/disclosure tests; forty existing access-boundary tests plus the additional hidden-Place relinking regression; two shared Bevy visibility UI tests; and two four-Lince simulations. This is 84 targeted test cases. Warnings are errors through the workspace lint policy.

The four-Lince simulations exercise a map-free Transfer with shared locations and a Transport use case with `#address` and user-configurable geographic areas. They validate existing general Transfer capability; they do not constitute the guided Transport preset in phase 6.

The headless Laboratory completed with **373 behavior checks passed, zero failed**, including the two visibility-editor cases and `four_lince_transport_address_permissions_draw_exact_points_and_configurable_areas`. The report is [transfer-foundations-laboratory-20261015.json](../../target/transfer-foundations-laboratory-20261015.json). Its debug CPU stress run completed 61 measurements, with 30 exceeding the 20 ms budget. It measures Sand/physics updates, without UI layout, rendering or GPU measurement, and ran alongside concurrent compilation. These timings do not establish desktop-map or mobile performance. Later owner-default and disclosure hardening is covered by the targeted engine regressions; no separate full Laboratory rerun is claimed for those final changes.

There is a reusable interface-only runner, avoiding the full application's media features. Give each run a new report filename because it refuses to overwrite reports:

```sh
nix develop --command cargo run -p lince-interface --no-default-features --features native-presentation --example laboratory-headless -- target/laboratory-NEXT.json
```

The completed phase-1 tasks were removed from the remaining checklist. The next authorized implementation step is phase 2; implementation permission has already been given, and no new approval round is needed for the agreed local code work.

**Not implemented by this foundation work:** Need outcome bindings, portable public receipts, weighted trust analysis/contact proofs/profile claims, publication/custody/gossip/helper, the remaining location lifecycle work, the guided Transport preset, desktop map completion and mobile. Their ordered tasks remain above. Real-device verification remains pending.

## Retained research discussion

Research discussion started 2026-10-11 and refined 2026-10-12. Its architectural arguments support the approved decisions above. Implementation evidence is recorded separately above; historical suggestions below do not reopen settled defaults.

Trust should help someone answer: **Who am I considering interacting with, what relevant evidence can I inspect, and why would I rely on them for this particular activity?**

The refined core is a family of configurable searches and analyses over visible Transfer history, counterparties and consented contact paths. Each view explains the evidence it used and the policy applied. Personal/Organ judgments are optional annotations; there is no intrinsic global trust balance attached to a party.

## Confirmed requirements

- Trust must work with ordinary Transfers and their specialized workflows, including Transport.
- Lince nodes may help people discover trust-related information. An Institute node can be one such source.
- A discovery node does not decide whether a party is trustworthy. Verification and judgment must not depend on a mandatory server.
- Evidence should be independently checkable by other nodes. The first version centers on Transfer evidence searches/analyses, with optional personal/Organ judgments; general recommendations and arbitrary claims are later extensions.
- The refined architecture is approved for implementation through the ordered checklist above.
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
- Organs can present manual or automatically generated claims/summaries of their activity, with an action to independently ask around and inspect support for a selected claim.
- Claim searches need include/exclude policies for known and unknown Organs, including sources the reader considers unreliable. Separate the public statement being checked from the sources selected to check it.
- Claim analysis can inspect giver and receiver accounts, with an explicit choice of proposed, delivered, received, settled or publicly disclosed Need-fulfillment measures. Public quantities and private Record effects must retain their distinct meanings.
- Preserve public Transfer terms independently of each party's private applications to one or more Records. A public `#x` outcome can have private effects on Records associated with `#y`, subject to unit/application rules.
- An observing Organ can be an optional, consented custodian of disclosed Transfer evidence for a defined period. Observation, dependent fulfillment and public archiving have separate authority and counting rules.

The 2026-10-12 agreement adopts the separation of identity/evidence/judgment/discovery, an evidence-first increment, and Person/Organ judgment ownership. The subsequent discussion expands the earlier direct-source-only scope into configurable contact-path analyses while retaining direct-source views. Explicit replies approve evidence/distance weighting and the Need-linked receiver-confirmed category. Jointly reviewed receipts remain the initial publication plan. Public-history terms are agreed upfront; later actor-signed statements within that scope do not require an opponent veto.

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

The current model already has [Vocabulary source identities and immutable definition versions](../../crates/nucleus/src/vocabulary.rs), [Assertions with versioned predicates, objects, quantities, units and authors](../../crates/store/src/assertions.rs), [imported claims with values, conditions, references, source revisions and explicit meaning mappings](../../crates/nucleus/src/knowledge.rs), and [Transfer promises/occurrences with Concept and unit references](../../crates/nucleus/src/transfer.rs). Binding these versions into portable Transfer trust evidence still needs the proof and receiver confirmation work described here.

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
3. Preserve requested and confirmed amounts independently across linked occurrences. A request for 10 and receipt of 12 is valid evidence of those two amounts, without a global capacity ceiling. Deduplicate the source outcome; do not reinterpret two distinct confirmations as a quota violation or automatically count private bookkeeping effects as public fulfillment.
4. Keep supported Need fulfillment distinct from resource throughput, giver effort, money spent and independently reported beneficiary outcomes. One event can support several views without inventing several physical results.
5. Publish only the approved manifest fields. Hidden capacities, beneficiaries, source Records and contact links must not leak through summaries.

These constraints prevent unilateral tag inflation and many duplicate or inconsistent accounting claims. They do not prevent a receiver from inventing a Need, enlarging its capacity or colluding with a giver. Need existence is a signed request and commitment, not proof of genuine deprivation or social benefit. Legitimate spontaneous contributions without the required prior binding remain visible in another category; absence of a Need link is not an accusation.

Self-confirmed work and confirmed work between affiliated actors can be useful history, but must not be displayed as independent recipient corroboration. Extra Cells within one Organ cannot become extra recipients or witnesses by changing device keys. Unknown control relationships remain unknown; do not label apparently different keys as proven independent people.

Ordering also needs precision. A manifest referenced by an agreement is cryptographically bound into that agreement, but issuer-supplied timestamps do not prove when physical events occurred. Local first-observed times, signed acknowledgements or optional independent observations can add context without becoming a compulsory timestamp server.

### The decentralized double-confirmation problem

A receiver with offline Cells could sign overlapping fulfillment against the same Need on different devices. Deduplicating event IDs detects repeated copies, but not two distinct IDs claiming the same remaining capacity. An attacker can also sign inconsistent cumulative histories.

Deferred research only: a workflow that explicitly requires a capacity invariant could designate a fulfillment writer or disjoint Cell allocations and sign cumulative references. This is not part of the approved trust metric, which has no global Need quota. Do not introduce a judging Organ or reject genuine overfulfillment while implementing this plan.

Gossip can expose contradictory signed branches and update a view after reconnection. It cannot guarantee that an undiscovered branch does not exist. Preserve forks and mark the affected outcome conflicted until an attributable resolution, rather than accepting the first reply or summing competing revisions. Distinct occurrences sharing a Need are not automatically a fork.

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
| Private Transfer applications | Owner-signed formulas and groups of quantity/fulfillment effects on local Records, separate from canonical amounts | Public proof of private Need meanings or a generic conversion from one claimed benefit to another |
| Transfer dependency observation | Evaluation of upstream agreed/settled outcomes; observation-only commitments and dependent readiness | A public evidence archive or an additional resource contribution merely because an observer exists |
| Visibility controls | Organ includes/excludes, Proximity ranges, separate saved/live location permissions | Permission to publish a private event in a reputation index |
| Social discovery | Signed announcements, selected helper Organs, source provenance, conflicts, consented gossip, withdrawals and bounded storage | A complete search of all Lince history, or a trustworthy server score |
| Contact search forwarding | Consented asking/answering/forwarding with shared work, byte, result and depth bounds | Verified social-contact paths or trust-artifact queries |
| Social reports | Evidence and explanations for an operator to inspect | A verdict about a person or an automatic reputation penalty |

Relevant implementation references:

- [Identity signatures](../../crates/engine/src/trust.rs), [Organ rosters](../../crates/engine/src/roster.rs), and [roster signature tests](../../crates/engine/tests/roster_fact_signatures.rs).
- [Contacts and Proximity](../../crates/store/src/organs.rs) and [session identity](../../crates/store/src/session_access.rs).
- [Transfer evidence types](../../crates/nucleus/src/transfer.rs), [delivery and application attestations](../../crates/nucleus/src/transfer_delivery.rs), and [delivery verification](../../crates/engine/src/transfer_delivery.rs).
- [Private application policies](../../crates/store/src/transfer_accounting.rs), [grouped effects](../../crates/store/src/transfer_effects.rs), [native Transfer forms](../../crates/interface/src/native/transfer_castle/forms.rs), and existing [accounting](../../crates/engine/tests/transfer_accounting.rs)/[grouped-effect tests](../../crates/engine/tests/transfer_effect_groups.rs).
- [Dependency outcome evaluation](../../crates/store/src/transfer_agreement.rs), [remote outcome evidence](../../crates/store/src/transfer_outcomes.rs), and existing [dependency tests](../../crates/engine/tests/transfer_dependencies.rs)/[four-Lince observer simulation](../../crates/simulation/tests/transfer_dependencies.rs).
- [Transfer field disclosure](../../crates/nucleus/src/transfer/disclosure.rs) and [Record/location visibility](../../crates/nucleus/src/visibility.rs).
- [Discovery provenance and conflicts](../../crates/engine/src/social/discovery_sources.rs), [consented gossip](../../crates/nucleus/src/social/gossip.rs), and [report handling](../../crates/engine/src/social/reports.rs).

Proximity means the relationship value chosen for an Organ contact; lower numbers are closer in the approved location/visibility design. It is not geographic distance and should not silently become an inferred reputation score. Tutorial wording about nearby discovery must not define the new trust model.

Karma is Lince's automation system. Its name must not imply that a person's reputation is already implemented there. Trust can eventually be an input to explicitly configured Karma rules.

Transport currently exercises generic Transfer, identity, visibility and location capabilities. The documentation still lists dedicated guided Transport controls and geographic discovery as further work. Trust should not depend on those controls being finished or on a map being available.

## Organ profile claims and independent checking

Agreed direction: an Organ can show claims about its activity when someone opens its profile. Examples include “We delivered at least 100 units of this contribution” or a generated summary of visible Transfer history. Claims can appear as compact profile fields/cards and can be referenced from the description. A reader can select one and use “Ask around” to inspect supporting evidence under their own query/source policy.

Recommendation: store a checkable claim as a signed ordinary Record with a structured claim descriptor; link it from the public profile. The description can explain the claim, but arbitrary prose should not silently become an executable verification query. The current [public profile fields](../../crates/nucleus/src/social.rs) contain descriptive fields and no structured trust-claim descriptor; this addition is planned work.

Recipes are bounded, declarative searches/aggregations. Imported claims must not execute Actions, scripts or arbitrary network callbacks. The reader chooses the actual source and interpretation policy even when a publisher supplies an illustrative recipe.

Suggested descriptor fields:

- Claim ID/revision, publishing Organ/authorized actor, and subject/role being described.
- Exact Vocabulary/Concept definition references, outcome meaning, unit and measurement basis.
- Amount or count, comparator such as exact or at least, period and as-of time.
- Manual authorship or automatic recipe, with recipe/policy version and any publishable evidence references.
- Intended evidence level, coverage description, correction/status references and publication expiry.

An Organ's claim may concern the Organ itself, a named authorized Person, or a disclosed collection of activities. It must not silently acquire all members' personal history or count the same event twice through different affiliations. A signed profile proves who published the claim; it does not make the claim independently verified.

An automatic recipe should use only evidence and derived fields approved for its publication audience. A public summary must not aggregate every private Transfer held by the Organ, expose excluded counterparties, or publish the author's private trust/proximity settings. Reading a private source does not grant permission to publish its count or outcome.

Approved automation behavior: configure and review a bounded publication recipe, then allow authorized automatic updates within that scope. Sign each revised output and retain its recipe version/as-of time. Changing the audience, meaning, measurement basis or permitted input scope needs a new publication decision. Expired/unavailable inputs should produce an honest status/coverage change rather than preserve an unexplained verified-looking total.

Opening a profile should show the publisher's statement and any already available checking result. “Ask around” is a deliberate action with source settings, work limits, progress and cancellation. It should not accept the publisher's selected verifier set as the reader's own or launch unbounded network work merely because a description contains a link.

Keep published claim, available support and local interpretation separate. The reader can inspect “Publisher claims at least 100; this search supports 10 within its selected visible scope”, with the relevant receipts and reasons. Insufficient public evidence is not proof that the missing activity never happened.

## Source and actor include/exclude filters

The user requires policies for known/unknown Organs and parties considered unreliable. Recommended design: separate filter purposes, while offering a simple preset that excludes an Organ from every purpose if desired.

| Filter purpose | What it controls |
| --- | --- |
| Query sources/custodians | Which nodes may receive requests or supply cached evidence to this query |
| Evidence issuers/confirmers | Whose original signed statements may support the selected metric, regardless of who carried the copy |
| Counterparties | Which receiver/giver identities qualify as counterparties in the analysis |
| Contact paths | Which disclosed actors/relations may qualify a path or act as intermediaries |

Do not conflate these purposes: a disliked carrier can forward an otherwise valid receipt signed by a selected receiver, while a trusted carrier can forward an unreliable giver's statement. Show both original issuer and immediate source. A party whose claim is being checked can remain the query's subject while being excluded as a source of corroboration.

Use reusable include/exclude conditions with explicit Organ IDs, known/unknown state, any deliberately selected Proximity bounds, relation/evidence role and applicable activity scope. Reuse the approved rule composition: OR across includes, AND within a condition, exclusions win. A trust-search preset does not change contact state, visibility grants or network consent. Unknown participation must be selectable rather than automatically treated as either credible or malicious.

Existing blocked/network-disallowed contacts remain unavailable for unauthorized requests. No claim recipe or high weight can override that restriction. Reading already permitted public artifacts, accepting an issuer's testimony, asking an endpoint, traversing a social relation and forwarding evidence remain distinct decisions.

Apply hard query-source exclusions before this Lince's network requests and subsequent refreshes. Forwarding peers should receive the requested source scope and honor it along with their own consent. A signature on that scope is a commitment, not proof that an untrusted server never contacted a disallowed node. Offer a strict mode that disables delegated forwarding and asks only explicitly selected endpoints. Disclosed provenance can explain returned evidence without claiming a complete audit of a server's internal data gathering. Issuer/counterparty/path exclusions apply to local eligibility and cache reuse too. Retain the policy version with results so a changed exclusion does not leave a stale eligible total.

Only explain exclusions using information the reader may see. Do not reveal a hidden contact path, private Need or hidden outcome count to justify an omission.

## Public terms, two-sided evidence and private effects

**Current implementation:** public/canonical Transfer amounts are distinct from owner-private application policies. A policy can apply formulas to as many as 32 distinct local Records, with quantity or fulfillment effects. The native form exposes private Record/Need effects. Quantity effects require compatible units or an explicit conversion; fulfillment effects can meet a different Need up to its outstanding amount. This is supported by the inspected backend, native form and existing focused tests; no new test run is claimed in this documentation turn.

That supports the user's public `#x` / private Record-with-`#y` separation as an accounting/application pattern. Explicit, immutable Vocabulary-Assertion outcome binding and public trust summaries still need the planned proof/UI work. A private effect is currently configured against a Record; it does not automatically create a receiver-signed public assertion that `#y` was fulfilled.

| Quantity/statement | Meaning | Possible visibility |
| --- | --- | --- |
| Proposed or promised canonical amount | What the parties offered/agreed to move or perform | According to Transfer field disclosure |
| Giver delivery statement | What the giver reported as delivered | According to its publication policy |
| Receiver confirmation | What the receiver accepted/confirmed under the selected meanings | According to its publication policy |
| Canonical settlement slice | The amount recorded by the agreed Transfer settlement protocol | According to its disclosure/proof scope |
| Owner's private Record effect | Local delta/fulfillment computed using the owner's policy | Private unless separately authorized for disclosure |

All claim recipes and “Ask around” queries need an explicit measurement basis, subject role, period, unit/meaning and event stage. Offer an inspection view of both parties' permitted statements instead of choosing giver or receiver silently. The strongest Need-linked metric continues to require the receiver's precise confirmation; merely labeling a published contribution as a Need does not establish receiver fulfillment.

Examples:

- **Promised 100 units of `#x`, supported receipt/settlement amount 10 units of `#x`:** show the promise, supported amount and remaining/cancelled/disputed amount if disclosed. Do not present the original promise as 100 completed contributions. Current occurrence delivery/receipt flags are not already per-Assertion, independently quantified receipts; that richer binding remains planned.
- **Canonical 100 units of `#x`, private effect 10 units toward a Record associated with `#y`:** this can be a legitimate private formula/fulfillment mapping. It does not contradict receipt of 100 units of `#x`. The precise mode and compatible-unit rules still apply.
- **Giving and receiving statements claim different amounts for the same occurrence, role pair, meaning and unit:** preserve both signed accounts and identify the inconsistency. Do not average them into a confirmed amount or silently choose the larger one.
- **Public contribution/source stock decreases by 100, recipient local Record changes by 10:** those changes alone do not prove delivery of 100 or failure to receive 90. They may have different formulas, units, scopes or causes. Public Record deltas are not interchangeable with participant confirmations.

Public agreement/settlement quantities should have a consistent canonical basis for the bound exchange. The protocol can represent losses, transformation, consumption or distinct outcomes through explicit terms and effects; local accounting is not a universal conservation law. Keep any private receiver effects independent rather than requiring identical public/private deltas or fabricating public `#y` data to make totals equal.

Every comparison needs to establish comparable meanings, stages and scope first. A claimant saying “100 offered” and a receiver saying “10 received” are different facts; the discrepancy becomes problematic if the first is advertised as 100 received/completed. A hidden mapping should yield “local effect not disclosed” or “bases not comparable”, not an accusation and not a guessed conversion.

Recommended verification statuses: supported within this scope; partially supported; incompatible measurement basis; explicit conflicting statements; missing/private evidence; or stale/partial search. For “at least 100”, finding 100 qualifying evidence can support the bound; finding only 10 cannot disprove additional undisclosed events. A claimed exhaustive exact total needs stronger coverage than a selective portfolio normally provides.

## Observing Organs, dependencies and timed evidence custody

**Current implementation:** dependency Transfers can require upstream Transfer agreement or settlement, or appropriate promise state. Remote outcomes use authorized signed received evidence with revision, freshness and access checks. The present live dependency freshness window is 120 seconds; that is an activation/status rule, not the lifespan of all historical trust evidence. See [remote outcome handling](../../crates/store/src/transfer_outcomes.rs).

There is an important observation-only case: a dependency Transfer with one party, unbound non-exchange promises and no children can project `observed = true` and `settled = ready`. It does so without creating a resource delivery or ordinary settlement occurrence. A dependent economic Transfer still needs its proper participant/action authority. Do not count every `settled` Transfer as another contribution or use observer commitments to multiply the source event's experience.

The desired observing Organ can perform two separate jobs:

1. **Conditional workflow:** observe the authorized upstream outcome and make a commitment/downstream action eligible under the existing dependency/Karma authority model.
2. **Evidence custody:** retain and serve a consented, signed projection of that event for a defined audience and period, together with relevant corrections/status.

The first has existing foundations. The second is an explicit planned archival/publication capability; adopting an observer role does not grant it automatically. A custodian receives only approved fields, preserves original signatures and occurrence identity, and cannot upgrade participant claims into independent physical observation.

Recommended custody agreement: who authorizes the disclosure, exactly which event/manifest/status fields may be copied, allowed viewers and redistribution, retention/expiry, refreshing/corrections, key/status dependencies, withdrawal behavior and what happens if the custodian disappears. Keep this authority distinct from the permission to trigger a dependent Transfer.

Use a linked evidence Record/projection for the public archive. If a separate public Transfer is useful for the observer's actual workflow, link its source event and retain the distinct observation/resource roles. A mirror/archive/dependency entry does not represent a new donation or delivery of the source quantity.

Private details can support a narrow public statement only with the relevant authorization. An observer's signed “I checked the authorized private application” is testimony by that observer with a declared inspection scope; it is not public proof of all hidden Records and does not replace the original receiver's signature. Its credibility follows the reader's chosen issuer/evidence policy, including affiliation and conflict considerations.

Retain the original assertion time/period and the custodian's own observed/received time separately. A current custody signature does not make an old event recent. Serve an as-of state and check known later corrections rather than freeze an attractive historical status. Preserve already authorized downstream effects under their correction rules; a later access loss does not silently reverse them.

Bounded custody can improve availability and make selectively disappearing evidence harder to hide while a valid copy remains available. It cannot compel completeness, make private history public without consent, guarantee deletion from all recipients, or turn an Institute node into the final judge. A live dependency needs fresh available outcome evidence; an expired archive is simply unavailable under its retention policy, not proof of non-contribution.

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

Public-history terms follow the approved decisions above. Requiring fresh consent from every identified participant for every later update protects private history, but lets a participant prevent an unfavorable event from appearing. Allowing unilateral publication supports warnings and accountability, but needs careful handling of evidence, privacy, harassment and replies. A positive co-signed portfolio and a contested account should have separate meanings. Operator moderation reports are not a substitute for this decision.

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
- Organ claim/summary cards linked from the profile and description, clearly marked as publisher statements. “Ask around” opens the reader's query settings and shows available support beside the claim, including manual/automatic provenance and as-of time.
- Separate source/issuer/counterparty/path include/exclude controls, known/unknown choices and a strict direct-query option. Choose the quantity basis and inspect both parties' permitted accounts before treating different totals as inconsistent.
- A custody/sharing form for an observing Organ with approved fields, audience, expiry and correction behavior. Distinguish observer-only fulfillment from source resource occurrences and retain private-application controls.
- Controls to recognize an identity separately from relying on its confirmations for selected activities. Include preview of which evidence the choice would add or remove from the local view.
- A reviewed “Share portfolio receipt” action after a suitable occurrence, distinct from settlement and from a review of the provider.
- An “Inspect evidence” view that explains signer, role, context, proof basis, identity binding, status and immediate discovery source in plain language.
- Source settings and manual refresh, with offline, partial, stale and conflict states visible.
- Optional Protein filters over permitted evidence and local judgments. Explain why a result is included or excluded.

Keep identity binding, signature validity, content conflict and freshness as separate result fields. A receipt can have valid signatures, unresolved identity and stale status at once. Avoid one unexplained green “trusted” badge.

Transport example: a passenger reviews a driver's visible transport receipts and their own previous experience. They choose whether to proceed. After the journey, both may approve a minimal result receipt. Live location remains separately consented and is excluded from that receipt. The same mechanism works for lending a tool or completing a delivery.

Recommendation: first use trust for inspection and candidate filtering. If automatic selection or agreement is later approved, use explicit Karma rules with reviewable context, thresholds, freshness and an unknown-data policy. New endorsements must not silently change agreements or access controls.

## Laboratory and adversarial verification plan

Use at most four Linces per topology and add native Laboratory cases through shared fixtures. The table contains acceptance criteria, not reports of successful tests.

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
| Same source occurrence confirmed on different offline Cell branches | Replayed copies count once; conflicting revisions remain inspectable. Distinct occurrences can overfulfill a Need without creating a global quota violation |
| Weight policy changes or path becomes shorter/longer/unavailable | The derived index changes with a visible reason while raw evidence remains attributed and no delivered quantity is rewritten |
| Depth/result/byte/signature budget exhausted or one source offline | The query stops promptly and labels partial coverage; absence is not a negative judgment |
| Organ manually claims at least 100, selected visible search supports 10 | Show partial support/coverage; do not infer that undisclosed 90 never existed |
| Auto-summary recipe includes private transfers, excludes an issuer, or changes its stage/unit basis | Only publication-approved eligible inputs enter the output; policy/as-of changes remain explicit and private fields do not leak |
| A publisher supplies a self-serving verifier set or executable recipe | Reader policy controls checking; imported claim data executes no actions or callbacks |
| Excluded source carries a permitted receiver receipt, or selected source carries an excluded issuer claim | Query-source filtering and original-issuer eligibility follow their separate purposes and are explained |
| Promised 100 versus received 10 in the same public meaning/unit | Promise and supported quantity remain distinct; proposed amounts are not completed contribution |
| Canonical 100 of `#x` versus private fulfillment 10 toward `#y` | No false numerical conflict, inferred private mapping or extra public outcome credit |
| Archived receipt, original receipt and observation-only settled commitment | Stable source occurrence counts once; observer-only settlement contributes no source resource quantity |
| Observer receives private data and serves a timed public projection | Only authorized fields/audience are published; original signer, source period and custody times are preserved |
| Archive expires, source revokes access, or a correction arrives | Custody/dependency freshness and historical proof status remain distinct; existing downstream effects follow their own correction rules |
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

## Decision provenance

The approved decisions above consolidate the user’s answers and authorization to choose sane architectural details. The retained research supplies rationale and adversarial acceptance criteria, not additional unanswered approval gates.
