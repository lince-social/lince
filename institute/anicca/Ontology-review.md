# Ontology: implementation plan for social features 1–6

## Working agreement

Planning update, 2026-09-29. This document specifies six social features: optional anonymous/identified publication, one shared public Organ profile, Record-based conversation sync, stranger introductions, directory/gossip discovery, and reliable server delivery. The wider work previously grouped under item 7 is excluded from the current implementation plan.

The human has chosen: small Needs and Contributions that can be posted anonymously or under the Organ's public identity; connections between strangers; gossip; servers that remain online; reliable asynchronous conversations; questions and recommendations before implementation.

Each decision below has a stable number, a simple explanation, a question, and a recommended answer. Recommendations are the document's working defaults. The human's comments amend those defaults; a comment does not remove the rest of a feature unless it explicitly says so. Unmentioned recommendations stay in the document, as requested. The human requested that items 1–6 be carried through into this plan. This document records agreed behavior and implementation requirements; editing it does not mean those features have been built.

Status labels distinguish **existing code**, **partial foundation**, **remaining work**, and **design recommendation**. A checked box means code was found, not that a production deployment was verified. Source and existing tests were inspected; tests were not run. The working tree contains other agents' changes, so implementation must recheck the relevant code.

The planned work includes the social frontend/backend and correctness, security and performance checks. This refinement changes the Markdown. The owner continues to author .lingua.

### Confirmed identity and connection decisions

- Each post can be anonymous or identified, chosen by its author. Anonymous publication conceals the Organ identity in the public listing; stronger network-origin anonymity is outside the current scope.
- Anonymous posts use separate posting identities by default, with an optional persistent alias.
- Strangers can send a bounded private introduction and exchange private replies before sharing Organ profiles.
- Each participant chooses when to reveal their identity. Becoming known Organ contacts requires mutual acceptance.
- An initial reply permission allows a bounded introduction; acceptance allows continued conversation.
- Each Organ has one stable public identity and one current public profile shared by its authorized devices. A device or profile host does not create another public identity.
- Devices with a UI must provide public-profile viewing and editing controls. Authorized edits sync to the Organ's other devices and update its published profile.
- Conversations remain the existing Conversation, Thread and Message Records, synchronized between authorized devices. Anonymous identities and delivery machinery integrate with those Records.

These are confirmed decisions. The other numbered recommendations remain in the plan unless amended. They are requirements for the six planned features.

## 1. Scope and source coverage

The [Ontology record](Lince.lingua) describes managing data, its meaning, sync, and Organ discovery. Its direct children are Assertion, Organ, Lingua, Blood, Trail, and Sync. Its explicit remaining checkbox is the LoRa adapter. The detailed social networking checklist is in [Ontology Tasks](Tasks.lingua). The Interface task also asks for exploring Concepts, directed Assertions, Trails, and progress without graph movement changing relationships.

The record's #done tag is not evidence that all these tasks are implemented.

| Source | Relevant material carried into this plan |
| --- | --- |
| Lince.lingua, Ontology and its Organ/Sync/Assertion children | Organ identity, existing Record relationships, grants and synchronization boundaries |
| Tasks.lingua, Organ Profile and Discovery | Rich profiles, hosting, OPEN promise propagation, cache, delegated search and independent directories |
| Tasks.lingua, Relay Cells | Separate application carriers, explicit consent, batch authentication, capability enforcement, byte/connection limits and throttling UI |
| The human's refinements | Optional anonymous/identified posts, one shared public identity/profile, editable UI and synchronized conversation Records |

### The six features in scope

These numbers match the requested 1–6. They are feature packages; the later numbered implementation steps describe the dependency order.

| Item | Planned result | Existing foundation to reuse | What remains |
| --- | --- | --- | --- |
| 1 | Publish a Need or Contribution anonymously or under the Organ's public identity | Records, signed data, OPEN promise export | Safe publication projection, per-post authority, preview, destinations, revisions and ending/renewal controls |
| 2 | Maintain one editable public profile per Organ across its authorized devices and chosen hosts | Organ Records, profile properties, signed rosters and own-device sync | Public projection and restricted signing authority, native editor, media limits, conflict handling and signed republication |
| 3 | Keep the same retained conversations on authorized devices, including newly enrolled devices | Conversation, Thread and Message Records, grants and sync | Prove history coverage; sync private participant/request state; provision new sessions; handle concurrent receives and revoked devices |
| 4 | Let strangers introduce themselves and continue privately before optionally becoming contacts | Existing conversations, invitations, contact controls and sealing primitives | Pseudonymous reply/session contract, Requests, separate admission, explicit reveal, mutual contact conversion and block/close controls |
| 5 | Find people through independent directories, browsing, bounded gossip and ask-around search | Local discovery, known-Organ address lookup and promise cache | Public searchable index, validated cache, consent, bounded forwarding/query workers, source merging, expiry and withdrawal |
| 6 | Keep asynchronous delivery working through always-online services | Headless Cell, Wire, sealed mailbox batches, quotas and connection caps | Mailbox correctness fixes, separate service roles, durable inbox/outbox, redundant copies, current authorization, operator controls and restore evidence |

Completion means these six packages work together through the stranger-to-conversation acceptance scenario. Existing helpers are reuse candidates, not evidence that a package is complete.

The excluded item 7 subjects—vocabulary publishing/import, expanded Ontology/Relations exploration, file-sync closure, generic Trail packages/content/progress, .ics export, Blood integrations and LoRa—have no implementation tasks in this plan. Existing concepts, units and sync may be reused where a social feature needs them.

## 2. Learn the pieces through one example

You post: “I can repair bicycles this weekend.” Someone you have never met finds it and sends “Could you help with my brakes?” You can answer while keeping your personal profile hidden. If both choose, you become Organ contacts and create a Transfer.

| Word | Simple meaning | Job in the example |
| --- | --- | --- |
| Cell | One running Lince installation | Your phone or a server |
| Organ | A shared context represented by Cells | Your personal identity, family, or workshop |
| Public snippet | A small announcement you choose to distribute anonymously or with your Organ profile | Your repair offer |
| Posting identity | A key used for an anonymous post, separate from your Organ's public identity | Proving who can update the offer without naming you |
| Public Organ identity | The same stable identity and current profile across your authorized devices | Showing your chosen name and profile on identified posts |
| Signature | Proof that the holder of a key approved specific bytes | Preventing a server from rewriting your offer |
| Gossip | Passing announcements between willing peers | A contact passes the offer onward |
| Cache | Locally saved results that may become stale | Seeing an offer without asking its author every time |
| Directory | A searchable index of published listings | Finding bicycle repair outside your contact network |
| Townsquare | A public place to browse published snippets | Browsing local offers without a specific search |
| Address lookup | Finding how to reach an identity already known | Connecting after an Organ identity is shared |
| Iroh relay | Network infrastructure for reaching endpoints | Carrying an encrypted connection when a direct route is unavailable |
| Lince carrier or relay | A service permitted to carry Lince envelopes | Accepting traffic under Lince's consent and budget rules |
| Mailbox | A carrier that stores encrypted traffic for later pickup | Holding the introduction while your phone is offline |
| Vocabulary | Shared definitions with stable identities | Agreeing what a repair service or an hour means |
| Receipt | A statement about a delivery stage | Distinguishing stored on a server from delivered to a device |

Iroh relays handle connectivity. Application mailboxes must provide the durable storage required for offline conversation. These are separate responsibilities. [Iroh relay documentation](https://docs.iroh.computer/concepts/relays).

An always-online server improves availability; it cannot guarantee delivery after every copy is lost, keys are lost, storage fills, or retention expires. The app must show those states and support retries.

~~~mermaid
flowchart LR
    A["Author's private Cell"] -->|"Choose anonymous or identified"| P["Anonymous posting key or shared Organ identity"]
    P --> G["Consenting gossip peers"]
    P --> D["Chosen directories and townsquares"]
    G --> S["Stranger finds snippet"]
    D --> S
    S -->|"Encrypted introduction"| M["Author's chosen mailbox"]
    M -->|"Pick up after reconnecting"| A
    A --> C["Private reply and optional identity sharing"]
    C --> T["Mutual contact and optional Transfer"]
~~~

The signature identifies a posting key. It does not prove a real name, skill, physical location, or trustworthiness.

## 3. What exists and what still needs work

| Subject | Status and evidence | Remaining work |
| --- | --- | --- |
| Concepts, assertions, identity, hierarchy | Existing: [Ontology UI](../../crates/desktop/src/ontology/ui.rs), engine and desktop ontology tests | Reuse stable concept/unit references and Record links in social features; new definition publishing and exploration are excluded |
| Separate Organs and contacts | Existing: Organ Castle, store/organs.rs | Optional anonymous/identified posting and deliberate conversion to contacts |
| Local discovery | Existing: engine/wire/discovery.rs uses mDNS; Nearby UI | Public internet discovery and server selection |
| Known-Organ address publication | Existing: engine/directory.rs signs Organ identity, roster version, front-door endpoints | Searchable listings; this file is not the requested directory index |
| OPEN promise export | Partial: engine/sync.rs open_promise_export(), organ_sync tests | Network request, explicit publish action, public snippet projection |
| Promise cache and matching | Existing foundation: store/senses.rs, refresh_discovery(), engine/senses.rs | Unknown publishers, anonymity, signed versions, expiry, withdrawal |
| Contact proximity | Existing: store/organs.rs and Organ Castle | Clear distinction from physical nearness and gossip routes |
| Conversations, invitations, threads, messages | Existing foundation: engine/threads.rs, Wire conversation requests, desktop/thread_castle | Anonymous introductions integrated into these Records, complete own-device history sync, durable delivery status, robust recovery |
| Sealed mailbox batches | Existing: engine/seal.rs, engine/mailbox.rs, Wire mailbox verbs, mailbox and seal tests | Reliability fixes, pseudonymous delivery, current authorization, redundancy |
| Signed batch envelope | Already exists for sealed mail: SealedBundle signs sender, recipient, ciphertext, and key wrapping metadata | Stable delivery ID, signed expiry, replay policy; consistent contract for other forwarding |
| Restricted Cell capabilities | Existing foundation: relay_capabilities(), roster projection, migration 0055, enrolment tests | Enforced role configuration; restriction on reading is a separate concern |
| Connection and frame limits | Existing in engine/wire.rs; private Wire limits configurable | Shared service budgets, smaller social frames, response byte limits, UI |
| Mailbox registration, quota, expiry, invitations | Existing: engine/mailbox.rs and store/mailbox.rs | Atomic quota reservation, invitation-to-registration transaction, sender admission |
| Searchable directories, bounded gossip, delegated search | No complete implementation found in inspected paths | Core social work below |
| Anonymous or identified public posting | No complete implementation found | Per-post choice, distinct anonymous authority, shared Organ profile, discovery and reply contracts |
| Lince relay/townsquare role configuration | Partial foundations only | Roles, opt-in, policy, accounting, operator management |

Correction to round 1: there is already one signature per sealed mailbox batch. The task is to complete and reuse that contract, not invent another copy of the same envelope.

The Tasks record says GET /organ/open-promises exists. That route was not found in the active checkout. The export function is real; an active endpoint and its authorization still need to be demonstrated. Implementation should use the current transport boundaries rather than recreate an old web route by assumption.

## 4. Bugs and gaps to resolve before extending delivery

These are source-inspection findings, not executed reproductions. “Observed” describes the code path; “risk to verify” marks a suspected failure requiring a regression test.

| ID | Finding and evidence | Required correction and proof |
| --- | --- | --- |
| B01, critical, observed | Wire.collect_mail() sends MailboxCollected before collect_own_mail() parses, opens, and imports the bundles. The server deletes acknowledged rows. | Persist a received envelope locally before acknowledging it. Invalid or temporarily unreadable mail must remain recoverable or enter a bounded quarantine. Kill the client at every step and prove recoverable mail survives. |
| B02, high, observed | Engine.accept_bundle() reads held_bytes(), checks quota, then deposits in a separate operation. | Reserve quota and insert in one serialized transaction. Concurrent deposits at the boundary must not exceed recipient or global storage limits. |
| B03, high, observed | Every accepted deposit gets a new random mailbox UID. Retry of the same sealed payload has no stable deposit identity here. | Define a stable authenticated envelope ID. Duplicate deposit returns the existing receipt and consumes no extra quota. A retry after a lost response must store one logical envelope. |
| B04, high, observed | may_collect() accepts a valid presented roster and checks whether it names the peer; that function does not compare a current roster floor or revoked membership. | Require current authorization or a bounded, explicit freshness policy. An old signed roster must not let a removed device collect or delete current mail. |
| B05, high, observed | Mailbox collection authenticates Organ membership. confirm_collected() deletes Organ-wide rows. | Define ownership of the acknowledgement and distribution to sibling devices. One device must not erase the only copy before others have a recoverable path. |
| B06, high, observed | accept_bundle() checks format/version, registration and size, but has no recipient-issued sender admission token or stable replay check. | Separate public introductions from approved conversation delivery; cap stranger traffic without letting it consume the trusted inbox. Invalid mail must not starve valid mail. |
| B07, privacy, observed | SealedBundle exposes from_organ, to_organ, and from_cell outside the ciphertext. open_mailed() requires a known sender roster. | Do not use that envelope unchanged for anonymous stranger introductions. Add a separately authenticated pseudonymous contract without public Organ identifiers. |
| B08, security, risk to verify | relay_capabilities() removes authoring capabilities. seal_batch_for() selects every unexpired recipient sealing key in the roster. | A blind carrier must not join a private Organ or receive its mail keys. Test restricted roles cannot obtain private plaintext. “Cannot write” does not mean “cannot read.” |
| B09, recovery, observed | redeem_mailbox_invite() spends the invite before a separate registration write. | Claim the invitation and create registration atomically. A crash must not consume an invitation without creating the mailbox. |
| B10, user experience, observed | Automatic mailbox fallback waits MAIL_AFTER, currently ten minutes. leave_mail() returns after the first accepting pickup point. | Chat gets prompt mailbox fallback and optional two-server replication. Keep distinct policy for ordinary sync batches. Show how many durable copies exist. |
| B11, availability, observed | Collection clamps the number of bundles, but a count limit is not a reply byte limit. | Page by bytes and count so legal batches cannot produce an oversized Wire response. Test many maximum-sized envelopes. |
| B12, key lifetime, design gap | Current sealing is encrypted batch delivery, not a demonstrated ratcheting chat protocol. | State the actual guarantees. Decide D39 before promising that later key compromise cannot reveal captured older messages. |

Also verify: expired mail is excluded even before the sweep runs; full disks never produce a successful storage receipt; database durability settings match the receipt promise; acknowledgements are bound to the exact recipient and envelope; source limits survive restart; role policy changes cannot silently widen capabilities.

The tests already present cover sealing, recipient collection, quotas, fallback pickup points, expiry notices, and conversation-root import. They are useful foundations, but their existence does not close the crash, concurrency, stale-roster, and anonymity gaps above.

## 5. Recommended stack and ownership

Keep the existing Rust application and reuse its domain model, storage, and networking. Add small explicit protocols for social features.

| Layer | Working choice | Reason and required check |
| --- | --- | --- |
| Domain types | nucleus: typed profiles, snippets, identities, envelopes, policy, statuses | One meaning for each message across UI and server |
| Persistence | store: SQLite through existing SQLx; transactional inbox/outbox and service quotas | Durable work survives process restarts |
| Search | SQLite FTS5 plus typed concept, direction, language, and coarse-area filters | Useful plain-text search without a separate search cluster; verify FTS5 availability on supported targets |
| Validation and decisions | engine: signatures, consent, matching, roles, retry rules, expiry | UI and network cannot bypass the same rules |
| Runtime | cell: supervised workers, offline/online transitions, bounded queues | Fits the app's existing orchestration |
| Network | Existing Iroh endpoint/Wire; named social protocol handlers; transport interfaces where they already fit | Reuse encrypted connections and address lookup; isolate public service verbs from private replication |
| Gossip | Start with bounded forwarding among eligible contacts; evaluate iroh-gossip for separately opted-in public topics | Application visibility policy differs from broadcasting to everyone in a topic |
| Profiles and media | Existing blob facilities; content hashes, size limits, on-demand fetch | Images do not travel in gossip |
| Signing and sealing | Reuse existing Ed25519 and reviewed sealing primitives where applicable | Preserve batch signatures; new anonymity/session contracts need explicit review |
| UI | Rust UI integrated through current cell/actions and desktop feature modules, using interface widgets | Extend the active Rust interface; do not revive the old web frontend |
| Server | Headless Lince, separate service data, NixOS configuration; persistent storage and health checks | Reuse deployment support while adding clear social roles |

FTS5 supplies text indexing and ranking; its external-content indexes need correctly maintained updates and deletion. Treat the index as rebuildable from validated listings. [SQLite FTS5 documentation](https://sqlite.org/fts5.html).

SQLite WAL permits concurrent readers but still has one writer at a time. Keep transactions short and handle busy responses. Durable service receipts need a storage mode that meets the promised crash/power-loss behavior, plus realistic storage testing. [SQLite WAL](https://www.sqlite.org/wal.html), [synchronous settings](https://www.sqlite.org/pragma.html#pragma_synchronous).

The manifest currently declares Iroh 1.2.0 and its companion mDNS adapter. This document does not propose upgrading them. Evaluate any new crate against the repository's locked dependency graph.

iroh-gossip broadcasts among endpoints subscribed to a topic and needs bootstrap peers. It does not supply Lince's private visibility, directory search, or durable mailbox behavior. A topic also reveals participation to other participants. The recommendation to start with application-controlled forwarding is an architectural judgment based on those differences. [iroh-gossip documentation](https://docs.rs/iroh-gossip/latest/iroh_gossip/).

**D01 — Stack.** Do you agree with keeping Rust, Iroh, SQLite and the current UI, adding social contracts around them? **Recommendation:** yes. Add a new service dependency only for a measured missing capability. Do not introduce a compulsory Matrix, Nostr, or central-account dependency.

**D02 — Gossip library.** Should shared public topics be available alongside contact-to-contact gossip? **Recommendation:** design the adapter now, evaluate iroh-gossip against consent, privacy, resource limits and Iroh compatibility, and enable public topics only as a separate opt-in mode. Contact gossip remains required.

**D03 — Implementation boundary.** Should a public server be allowed to serve private Organ replication just because it provides a directory or mailbox? **Recommendation:** no. Its public handlers and storage are isolated; each private replication permission remains explicit.

## 6. Public identity, profiles, and snippets

### Identity and first contact

**D04 — Optional anonymous or identified publication. Confirmed.** For each Need or Contribution, its author chooses anonymous publication or publication under the Organ's public identity. Anonymous listings carry no public link to that Organ; identified listings deliberately link to its one public profile. Stronger concealment of network origin from operators is outside the current scope. Connection details, timing and the text itself can still identify or correlate a person; direct contacts may recognize who sent a snippet.

Anonymous listings point to chosen service mailboxes, not the author's personal Iroh endpoint. They use neither the main Organ signing key nor its public roster. Public identity concealment and concealment from direct contacts/operators remain different guarantees.

**D05 — Linkability. Confirmed.** Anonymous posts use separate posting keys by default; a persistent anonymous alias is optional. Identified posts use the same public Organ identity across devices and hosts. Separate anonymous keys do not guarantee unlinkability when timing, servers, text or network identifiers match.

**D06 — Introductions. Confirmed.** A stranger can send a short private introduction through a scoped reply address before either participant reveals an Organ. It goes to Requests, not the trusted inbox. The recipient may respond pseudonymously, decline, or block. Identified posts use the same permission controls.

**D07 — Identity reveal. Confirmed.** Each participant chooses when and what profile to reveal. Mutual acceptance creates known Organ contacts. Preserve the same Conversation, Thread and Message Records and deliberately link them to the revealed contact; do not duplicate the conversation or publish its association.

**D08 — Reply permission. Confirmed.** A public reply address permits a bounded introduction; acceptance grants separate renewable permission for continued conversation. Rotate or close the introduction address when a snippet ends. A capability is a narrow permission, not an Organ login.

**D09 — One public Organ identity, device controls and conversation sync. Confirmed.** Each Organ has one stable public identity, one current public profile, and the same retained conversations across its authorized devices. Every device with a UI must offer profile viewing and editing controls; only authorized devices/users may save edits. Changing the name, description, avatar, banner or other chosen public fields changes that profile, not the Organ's stable identity.

Profile changes are saved through the existing Record/Fact write path and synchronized within the Organ. For already enabled public-profile publication, saving an authorized edit queues an updated signed profile to its selected hosts and updates local identified-post views. Offline edits remain saved locally and publish after reconnecting; show pending, published and failed status. Server hosts and other devices hold copies of that same identity/profile.

Conversations remain Conversation Records containing Thread and Message Records. Synchronize retained history, invitations, participant mappings and relevant conversation state across authorized devices, including authorized newly enrolled devices. Keep Organ boundaries and conversation grants intact. Anonymous posting keys and private mappings follow those same authorized devices but never enter public profile exports.

Cryptographic session state and device delivery cursors remain distinct from conversation content: provision them according to the chosen session protocol rather than copying a live session key indiscriminately. Define backup, device removal and lost-key recovery. A device without editing authority can view the profile and the reason editing is unavailable.

**D10 — Trust.** Should the app label a signed anonymous post as a verified person? **Recommendation:** no. Show “signature valid” separately from user trust. Blocking one posting key cannot guarantee blocking every new identity a person creates; global service budgets are still needed.

### Rich profiles and hosting

**D11 — Profile contents.** What should the shared Organ profile publish? **Recommendation:** chosen name, description, avatar, banner, contact route, vocabulary references, version and expiry. Exact address, work context and personal contact details are separate opt-in fields. The one-identity model and profile controls on authorized UI devices are confirmed under D09.

**D12 — Profile association by publication mode.** Should an anonymous snippet automatically show the rich Organ profile? **Recommendation:** no. Anonymous posts show only their alias and deliberately selected anonymous details. Identified posts link to the Organ's current public profile. A profile edit updates identified views without changing a snippet's anonymous/identified choice or revealing anonymous posts.

**D13 — Hosted profile identity.** Should a host require a second public Organ identity? **Decision:** no; D09 confirms one public identity per Organ. A host serves a signed copy of that Organ's current public profile. Hosting registration may be an operator-local account, but it does not create another public identity or replace the author's signature. Separate Organs remain separate contexts.

**D14 — Offline profile access.** Should profiles remain readable when their author is offline? **Recommendation:** yes, from chosen hosts and validated caches until expiry. Show version and freshness, with media fetched only when requested.

**D15 — Images and previews.** Should listings carry images and external link previews? **Recommendation:** snippets remain text first. Profiles may have bounded images. Fetch previews only on user action; avoid leaking searches or opening private-network URLs through server-side preview fetching.

### What people publish

**D16 — Eligible content.** What should travel publicly? **Recommendation:** explicit public Need and Contribution snippets, including standalone general offers and sanitized projections of OPEN promises. A discovered post is not an agreement, and publication does not reveal the private Record, its history, or its counterparties.

**D17 — Post format.** How much detail belongs in a snippet? **Recommendation:** title, short text, Need/Contribution direction, optional exact quantity and unit, vocabulary/concept reference, language, optional coarse area, availability, signed revision, expiry, reply descriptor and authenticated anonymous/identified mode. Include a public Organ/profile reference only in identified mode. Quantity may be absent for “I can teach guitar”; do not invent a numeric balance.

**D18 — Posting flow.** Should making a Record public immediately send its complete contents? **Recommendation:** no. A Publish action previews the exact exported fields, anonymous/identified choice and chosen destinations. Keep a private mapping to the source Record. Updating the underlying Need/Contribution changes a draft; automatic snippet republication requires a separate saved rule and consent.

The shared public profile has a different update rule: once its publication and hosts are enabled, authorized profile edits update and republish that same profile under D09. Existing snippets keep their selected author mode. Changing an already published snippet's mode requires an explicit preview; publishing an identity link cannot later erase older public copies. To make a previously identified offer anonymous, prefer a fresh anonymous post and withdrawal of the old one.

**D19 — Ending and updating.** How should an offer stop circulating? **Recommendation:** monotonic signed revisions and a signed withdrawal. Fulfilled, paused, cancelled and expired states are distinct. Keep a small withdrawal marker long enough to suppress older revisions; expiry limits damage when cancellation cannot reach everyone. For concurrent edits by your devices, keep both candidates private and resolve the publication revision explicitly; do not silently pick a conflicting signed payload with the same revision.

**D20 — Visibility.** Do contact-only offers use the same public mechanism? **Recommendation:** preserve existing private/direct sharing. Public gossip requires deliberate permission to redistribute. Every hop checks the publication policy, expiry and destination's consent before forwarding. Receiving data does not authorize forwarding it, and later hiding a public post cannot erase other people's copies.

### Tasks and acceptance

- [ ] Add anonymous/identified selection per Need or Contribution, with the selected mode authenticated in the publication.
- [ ] Add separate anonymous posting identities and a private mapping to source Records; identified posts reference the one public Organ identity.
- [ ] Define profiles, anonymous aliases, snippet payloads, revisions, withdrawals and reply descriptors.
- [ ] Connect existing OPEN promise export through an explicit sanitized publication path.
- [ ] Support general standalone snippets without requiring an agreed Transfer.
- [ ] Add hosted, signed profile copies and on-demand media retrieval.
- [ ] Add a shared public-profile Record projection and viewing/editing controls on every UI device, enforcing edit permissions through the backend.
- [ ] Synchronize profile edits and publish new signed profile versions to already selected hosts; show pending/offline/failure state.
- [ ] Resolve concurrent device profile edits into one current profile with visible conflicts; preserve stable Organ identity and detect stale hosted copies.
- [ ] Add public-preview, destination selection, edit, pause, renew and withdraw controls.
- [ ] Prove an anonymous listing contains no Organ UID, private Record UID, roster, device ID, private assertion, private address or unwanted profile link.
- [ ] Prove an identified listing resolves to the same Organ profile from every authorized device and selected host.
- [ ] Prove editing the public profile never reveals anonymous listings, changes their posting keys or silently changes their publication mode.
- [ ] Prove discovery never modifies stock, creates an accepted agreement, or turns an unknown publisher into a trusted contact.

## 7. Gossip, ask-around search, and directory discovery

These features have three different jobs. Gossip spreads known public announcements. Ask-around sends a particular question through consenting peers. Directory search consults an index. Each needs its own consent and resource limits.

### Gossip and delegated search

**D21 — World reach.** Must every post reach every Lince user? **Recommendation:** promise access through selected independent directories plus bounded gossip, not universal delivery. Design the extension points for public-topic networks, additional discovery adapters and content routing now. Evaluate their coverage, bootstrap dependence, privacy and abuse costs before adding them; do not quietly remove the source's “beyond gossip and directory” design task.

**D22 — Hop limits.** Should a hop limit be a strict privacy boundary? **Recommendation:** no. A hop is a forwarding step, but public bytes can be copied and reintroduced. A sender's count and even a removable signed trail cannot prove a universal path limit. Enforce local forwarding limits, age, destination consent and budgets regardless of the sender. Keep the source's distrust-of-hop-count requirement; if a hard global route limit is essential, it needs a different permission protocol and cannot be promised for freely public snippets.

**D23 — Fan-out and deduplication.** How widely should a Cell forward one revision? **Recommendation:** to at most three randomly selected eligible peers, once per revision while its forwarding record is retained. Deduplicate by stable signed payload hash, excluding changing routing metadata. Keep revision identity separate from content identity. Full caches stop admitting new work or use a documented bounded eviction policy; no unbounded seen-set.

**D24 — Rate budgets.** Should every new posting key receive a fresh unlimited allowance? **Recommendation:** no. Enforce limits per connection, posting key, service registration and Cell-wide total. Source limits constrain one identity; the global ceiling still applies to people making many keys. Incoming parsing, verification and outgoing work all count.

**D25 — Age and clocks.** Does remaining hop allowance keep an old offer alive? **Recommendation:** no. Signed creation and expiry plus a locally enforced maximum lifetime control freshness. Forwarding never resets expiry. Reject or hold implausibly future-dated posts; use monotonic time for retry waits and wall time only where needed for published dates.

**D26 — Ask-around behavior.** Should plain-text search be available, with optional filters? **Recommendation:** yes. Search the local cache first. “Ask contacts” is a deliberate action with an explanation that peers may see the query. Use a unique request ID, short expiry, bounded onward requests, replies, result count and total bytes. Cancellation stops local work; it cannot erase a question already seen.

**D27 — Result ordering.** What should rank a result? **Recommendation:** clear text relevance and explicit filters, then freshness. Distinguish a known contact's user-set proximity, a stranger's unknown proximity, declared area, and any unverified route information. Never invent an Organ bond score from hops or infer exact distance from network addresses.

**D28 — Meaning of search words.** Should “apple” automatically equal every concept named apple? **Recommendation:** use plain text for broad finding, and stable vocabulary/concept identity for precise matching. Explain when a result merely shares words. Exact quantities, units and agreed definitions matter only when moving toward a Transfer.

### Directories and townsquares

**D29 — Independent services and bootstrap.** How does a new user find servers without contacts? **Recommendation:** support user-selected, independently operated directories through addable links/QR codes and an optional transparent starter list. Pin each chosen service's identity. Do not designate one as authoritative or silently enable publication or search to all listed servers.

**D30 — Publication, search and disclosure.** Should a directory have to know your personal Organ? **Recommendation:** anonymous publication authenticates with its posting key and service-specific admission policy; identified publication verifies the declared public Organ/profile association. Search need not require revealing an Organ. At the search action, identify which services receive the query; they can observe content, timing and connection metadata. Querying more directories improves coverage while disclosing to more operators.

**D31 — Townsquare, area and federation.** What belongs in the public browsing service? **Recommendation:** paginated public snippets, optional manually declared city/region, language and concept filters. The directory supplies search; townsquare supplies browsing. One operator may run both. Public listings may be mirrored only to destinations the author allowed; operators retain their own moderation. Defer compulsory server-to-server federation until its consent and withdrawal protocol is proven.

### Message contracts

| Contract | Required fields and rules |
| --- | --- |
| PublicSnippet | Protocol/domain tag, authenticated anonymous/identified mode, posting authority, Organ/profile reference only when identified, random public post ID, monotonic revision, signed creation/expiry, public fields, optional meaning/area references, reply descriptor, redistribution policy, signature |
| Withdrawal | Same posting authority and post ID; higher revision; signed state and expiry; retained suppression marker |
| GossipOffer | Announcement IDs and revision/hash inventory; bounded fetch; optional routing hint never trusted as proof |
| SearchRequest | Random request ID, bounded text/filters, expiry, local work budget, opaque reply route; no automatic main Organ identity |
| SearchReply | Request ID and bounded independently verified snippets; never trust a peer's text as the author's payload |
| PublishReceipt | Service identity, post/revision/hash, accepted retention, status; “accepted by this directory” is not worldwide publication |
| ServiceDescriptor | Pinned operator identity, offered roles, endpoints, limits, supported protocol versions, operator policy and expiry |
| Profile | Stable public Organ identity, current revision/expiry, selected fields and hashed media references; signed by an authorized profile authority; devices and hosts serve copies of the same profile |

Use deterministic signing bytes, protocol domain separation, bounded lengths, and fixed published test vectors. Ordinary JSON display order is not a signing specification. Unknown fields and protocol versions must have an explicit rejection/extension rule. No compatibility scaffolding for older Lince releases is required.

Plain-text search treats the person's words as data. Compile a bounded safe query instead of passing arbitrary text straight into SQL or FTS query syntax. Limit filters, wildcard/prefix expansion and execution time; parameter binding alone does not make every search inexpensive.

### Tasks and acceptance

- [ ] Add active snippet export/fetch and publication handlers under the current transport, with identical validation for local UI and network requests.
- [ ] Extend the existing promise cache where its semantics fit; keep unknown public snippets separate from trusted Organ data.
- [ ] Add stable hashes, revision ordering, withdrawal suppression, expiry, cache ceilings and persistent forwarding work.
- [ ] Forward to the allowed random subset; respect Cell and contact opt-in before considering a destination.
- [ ] Add delegated search with its own bounded request/reply protocol and private local history.
- [ ] Add directory publication, bounded search, update, removal, expiry and rebuildable indexes.
- [ ] Add townsquare browsing as a separately enabled role.
- [ ] Add independently selectable servers, source labels, duplicate merging and source-specific failure status.
- [ ] Add coarse-area search from deliberately declared data only.
- [ ] Record and compare designs for discovery beyond gossip/directories; measure what coverage each actually provides.
- [ ] Prove cycles, duplicates, sender-reset hop hints, many identities and large replies cannot exhaust a Cell's configured budget.
- [ ] Prove withdrawn/expired revisions do not return as current through stale peers or directory mirrors.
- [ ] Show “not yet refreshed” when offline; do not label cached availability as confirmed.

## 8. Relay Cells and reliable asynchronous conversations

A transport carries bytes. A mailbox makes them durable. A conversation gives them meaning and permissions. Completing one does not automatically complete the other two.

### Roles, consent and configuration

**D32 — Relay contract.** Should a relay provide live forwarding, delayed delivery, or both? **Recommendation:** both are defined, with delayed encrypted delivery implemented first because it directly solves offline conversations. Live forwarding uses the same signed-envelope and budget policies later. Existing Iroh relays continue to handle connection fallback.

**D33 — Cell roles.** Should personal devices automatically help carry traffic? **Recommendation:** no. Explicit role choices are personal, carrier, directory and townsquare; a machine may combine public roles with separate switches, storage and budgets. Every Cell that never opted in forwards nothing.

**D34 — Blind carrier isolation.** Should a mailbox join the user's private Organ to help? **Recommendation:** no. It stores ciphertext using scoped registration and delivery permissions. If a restricted Cell is published in an Organ roster, enforce relay_capabilities() and forbid widening that role; reading and key access require additional restrictions. Public directory authoring belongs to a separate service context.

**D35 — Contact permissions.** Is adding a contact permission to relay its operations? **Recommendation:** no. Default to no onward carrying. Distinguish permission to publish public snippets, send to a mailbox, carry a specific contact's envelopes, and import private operations. Relays never gain ownership of the originating Facts.

**D36 — Settings namespace.** Where should server roles be configured? **Recommendation:** one Lince social-service namespace: services.lince.social.relay, .mailbox, .directory and .townsquare in NixOS, mirrored by one structured runtime configuration. Reuse current connection-limit machinery. Keep services.iroh-relay separate; clearly map old checklist terminology to this chosen namespace before implementation.

### Delivery correctness

**D37 — What counts as sent?** Should “the server accepted it” mean “the person received it”? **Recommendation:** no. Expose queued locally, stored by carrier, durably received by recipient, available in conversation, and failed/expired. Read receipts are separate and off by default. A server receipt promises only that server's storage stage.

**D38 — Reliability and duplicate messages.** Should a retry risk displaying the same message twice? **Recommendation:** at-least-once transport with idempotent processing: retry as needed, store one logical message per stable ID. Persist sender outbox, recipient inbox, receipt and retry state. Acknowledge only after a durable recoverable local copy; commit chat/session state atomically where applicable.

**D39 — Encryption strength.** Should future theft of a chat key reveal previously captured messages? **Recommendation:** target forward secrecy for private conversations through an established reviewed session protocol, including offline initiation and multiple devices. Existing sealed sync batches remain useful, but are not proof of that guarantee. Select the session library in a dedicated pre-implementation decision after checking license, maintenance, platform support, key/state persistence and tests; do not handwrite a ratchet.

A ratchet changes keys as a conversation advances and deletes old keys where appropriate. That limits what a later compromise can expose; it does not protect plaintext already saved on a compromised device. Offline session initiation and multiple devices add separate requirements. [Double Ratchet specification](https://signal.org/docs/specifications/doubleratchet/), [Sesame session-management specification](https://signal.org/docs/specifications/sesame/).

Recommend evaluating **vodozemac/Olm** first for the private one-to-one session layer: the project implements cryptographic ratchets in Rust, has an Apache-2.0 license, and documents an external audit. This is an integration recommendation, not a claim that Lince's complete messaging design has been audited. It does not require joining the Matrix network. [vodozemac project](https://github.com/matrix-org/vodozemac).

The evaluation must prove offline prekey publication/consumption, authentication of the anonymous posting authority, per-device sessions, atomic session-state/message commits, crash recovery, out-of-order bounds, revocation and supported target builds. Do not copy a live ratchet between devices. If those requirements cannot be met, record the failed criterion and choose another reviewed protocol before building the session layer.

libsignal remains an alternative to assess, with its external-use support constraint stated explicitly. [libsignal repository](https://github.com/signalapp/libsignal).

**D40 — Multiple servers.** How many copies should be stored? **Recommendation:** support two independent recipient-chosen mailboxes by default when available. Sender retries retain the same logical envelope ID, with separate receipts per server. A single configured server remains usable but is shown as one durable copy. Replication improves resilience and exposes metadata to more operators.

**D41 — Multiple devices.** Can one device delete everyone else's delivery copy? **Recommendation:** not until there is a documented recoverable distribution path. Prefer per-device delivery cursors/acknowledgements for the authorized recipient set. A shared ciphertext may be stored once with per-device state. Under confirmed D09, retained Conversation, Thread and Message Records synchronize to authorized existing and newly enrolled devices; synchronize public-profile edits and private participant mappings within the same Organ as well. Keep record-history sync distinct from session-key provisioning. Define removal and replacement; do not let a revoked device erase current traffic.

**D42 — Retention and offline behavior.** How long should servers wait? **Recommendation:** an initial 30-day mail retention policy, visibly advertised and acknowledged by the sender, with configurable quotas. Local conversation history has its own retention. If all server copies expire, mark delivery failed and offer resend. Fetch on reconnect; polling is the baseline, with optional wake notifications carrying no message text.

**D43 — Retry and capacity.** Should failure cause repeated immediate reconnects? **Recommendation:** use capped exponential backoff with randomness, per-destination health, and a durable retry schedule. Chat falls back to mailboxes promptly after a short direct attempt, unlike the current ten-minute general-sync delay. Quota-full, refused, offline and storage-error states remain distinct.

**D44 — Limits, operation and recovery.** What must an operator control? **Recommendation:** global and per-peer connections, bytes in/out, reserved storage, per-mailbox quotas, request sizes and retention. Include worker health, disk-full behavior, encrypted backups, tested restore, identity continuity and signed operator configuration. Logs avoid message bodies and raw search text by default. A restored mailbox must not resurrect removed permissions or reset abuse budgets unnoticed.

### Required private envelope

The carrier sees only routing and admission data needed for delivery, plus ciphertext and unavoidable transport metadata. It must not receive a personal Organ identity for the anonymous introduction flow.

A private delivery contract includes a protocol/domain tag, stable envelope ID, destination mailbox/device scope, opaque conversation reference, ciphertext hash, signed creation/expiry, admission proof, and authenticated sender/session information as appropriate to the selected protocol. Existing Organ sync mail remains a distinct authenticated use case.

One transport signature covers a batch. Preserve existing Fact provenance where the domain requires it; “one batch signature” does not authorize removing semantic authorship validation or accepting unsigned arbitrary operations.

Allocate the logical message/envelope ID before sealing, authenticate it with the content and recipient scope, and persist the sealed bytes for retry. Re-sealing on each attempt must not invent a new logical message. The same authenticated ID with different contents is refused. Recipient receipts name the exact ID/hash and stage, and carriers cannot fabricate them.

Anonymous introduction permissions and established conversation permissions are separate. A public introduction address is inherently discoverable; it cannot by itself eliminate spam. Limit it, isolate its storage, let recipients close it, and let operators refuse abuse without requiring a real name.

### Reliable message state

~~~mermaid
stateDiagram-v2
    [*] --> LocalQueue
    LocalQueue --> CarrierStored: Durable deposit receipt
    LocalQueue --> RecipientInbox: Direct durable receipt
    CarrierStored --> RecipientInbox: Fetch and local commit
    RecipientInbox --> ConversationReady: Validate and process
    ConversationReady --> Delivered: Recipient receipt reaches sender
    LocalQueue --> Failed: Deadline or explicit refusal
    CarrierStored --> Failed: Every recoverable copy expired or lost
    Failed --> LocalQueue: User chooses resend
~~~

Retries can occur at any stage without creating another logical message. “Read” is deliberately outside this state machine.

### Tasks and acceptance

- [ ] Fix B01–B11 in the implementation sequence before claiming reliable public mailbox delivery.
- [ ] Define signed envelopes, replay behavior, durable receipts and pseudonymous introduction/session delivery.
- [ ] Add explicit role, Cell and per-contact consent, capability enforcement and isolated public service handlers.
- [ ] Add a transactional service inbox/outbox, stable IDs, retry workers and bounded quarantine.
- [ ] Add receiver-authorized stranger Requests, acceptance, decline, block, renewable conversation permission and optional identity reveal.
- [ ] Select and review the conversation session library before implementing its cryptographic state.
- [ ] Integrate anonymous and identified conversations into the existing Conversation, Thread and Message Records and UI; public posts never automatically grant replica-root access.
- [ ] Synchronize retained conversation history, invitations and participant identity/reveal mappings to authorized devices, including authorized newly enrolled devices.
- [ ] Preserve the same conversation/thread/message UIDs when a pseudonymous participant reveals an Organ; do not create duplicate histories.
- [ ] Prove a message received on one device remains recoverable and appears once on the other authorized devices after reconnecting.
- [ ] Add multiple pickup points with durable replication and source-specific status.
- [ ] Add current device authorization, per-device delivery state, revocation and key-rotation behavior.
- [ ] Add operator byte accounting, atomic storage reservation, traffic ceilings and throttling notices.
- [ ] Add NixOS/runtime role settings, persistent service state, supervision, health checks, backups and restore tests.
- [ ] Prove a never-opted-in Cell and a never-authorized contact cause no forwarding.
- [ ] Prove carrier operators cannot decrypt private message content, alter valid envelopes, or manufacture recipient receipts.
- [ ] Prove a revoked Cell cannot collect or acknowledge new mail using an old roster.
- [ ] Prove losing a deposit response, retrying through another server, or crashing during collection preserves one recoverable logical message.

## 9. Implementation contracts for items 1–6

These contracts turn the preceding decisions into work a developer can execute. They preserve the confirmed choices and add recommended defaults for details those choices leave open. D45–D60 and D70 are intentionally absent: their previous subjects are outside this scope. New decisions use D71 onward so earlier references keep their meaning.

### Item 1 — Optional anonymous or identified publication

**D71 — Public and private data.** Should a public snippet reuse the UID and full contents of its private source Record? **Recommendation:** give it a random public ID and a narrow typed projection. Keep the source UID, internal assertions, counterparties, device membership and editing history inside the author's Organ. Validate the publication through the same backend path whether requested by UI, automation or network. A public listing never creates stock movements or an accepted Transfer.

The posting screen offers standalone Need/Contribution text or a preview derived from an eligible existing Record/OPEN promise. Quantity and unit are optional; when supplied, preserve exact decimal values and validate direction. Publish concept/unit identifiers only when they already have an allowed public reference; otherwise use an explicit plain-text label rather than exposing a private definition Record. Select language, coarse area, availability, expiry and allowed destinations. Anonymous mode generates a separate posting identity by default; the optional alias control explicitly explains that reuse links posts. Identified mode links the one public Organ profile.

**D72 — Updating a publication.** Should editing a private source automatically change a public offer? **Recommendation:** save a changed public draft and require a new preview, except where the author enabled a separate explicit republication rule. Support edit, pause, resume, renew, fulfilled and withdraw actions. An identified-to-anonymous change creates a fresh anonymous post and offers withdrawal of the old one; the UI explains that older identified copies can remain.

A revision authenticates its issue time and expiry. A renewed revision can have a new issue time without pretending the original post was created again. A post ID cannot be taken over by another authority. Reject conflicting payloads with the same signed revision. Concurrent updates that share a parent are publication conflicts; retain both private drafts and resolve them explicitly. Publication results name each selected service and the accepted revision/hash.

**Exit evidence:** preview equals the signed public projection; anonymous payload and hosted metadata have no public Organ/private-source link; alias reuse is deliberate; wrong-key updates and stale/conflicting revisions fail; mode changes never expose unrelated anonymous posts.

### Item 2 — One shared public Organ profile

**D73 — Identity and authorized editors.** Does one public identity require copying the Organ's root private key to every device or profile host? **Recommendation:** no. The identity is stable; profile fields are editable data. Use a narrowly scoped, root-authorized profile-signing delegation for permitted editors, with expiry and revocation. Hosts receive signed public documents and verification material, never private signing or conversation keys. Viewing, editing and publishing permissions are distinct and enforced by the backend.

Use an explicit public-profile projection of the existing Organ Record: chosen display name, description, avatar/banner hashes, optional coarse area and contact route. Do not automatically publish the private Organ name, record tree or full Cell roster. Known identity keys must match the stored trust anchor or a verified succession chain. A valid signature proves control of that key; it does not prove a person's name.

**D74 — Conflicting edits and freshness.** What happens if the phone and laptop edit the profile while disconnected? **Recommendation:** save both edits within the Organ and record the publication base/head for each. Nonconflicting field edits can merge; conflicting fields remain visible for resolution. Signed publication revisions bind their parent revision/hash and editor authority so concurrent branches are detectable. Hosts preserve evidence of a conflict rather than silently using whichever packet arrives last. Resolution creates a new signed revision covering the resolved heads.

The UI identifies the active Organ, shows a preview and permitted fields, explains disabled editing, and displays pending/published/failed status per host. Once the person enabled publication to selected hosts, authorized saves queue republication automatically. Device views converge after sync; disconnected devices and hosts can temporarily show older versions with a freshness label. Switching publication off stops further exports and offers withdrawal, while explaining that public copies cannot be recalled.

Identified snippets carry a bounded profile reference and authority proof, not the complete rich profile; a 16 KiB profile cannot fit inside a 6 KiB snippet. Fetch its signed document separately and label an unavailable or stale profile.

Media is separate from profile text. Grant read access only to the deliberately published assets; a profile host must not expose the author's general private blob store. Fetch only after a user request, verify content hashes, enforce encoded-byte and decoded-dimension bounds, and offer a text fallback. A current cached profile may update the display of an identified post; it cannot change the post's signed content, author mode or authority.

**Exit evidence:** one Organ identity across two UI devices and two hosts; forbidden edits fail through direct backend calls; offline and conflicting edits survive restart; stale hosts cannot silently replace newer accepted state; a name/avatar change leaves anonymous posts and conversation UIDs untouched.

### Item 3 — Conversation Records and own-device history

**D75 — Where history lives.** Should anonymous conversations create a second chat database and a new UI history? **Recommendation:** preserve the existing Conversation → Thread → Message Records and their durable UIDs. Store private posting/participant mappings, request state and delivery metadata as scoped extensions or associated private Records. Delivery queues are work state, not a competing source of conversation content.

Materializing a validated incoming message must be idempotent. Commit the message, its thread relationship, receipt/processing marker and applicable cryptographic state atomically. The same logical message arriving through another server or device must not create another Record or repeat Fact effects. A conflicting message with the same ID is refused.

**D76 — New and removed devices.** Should enrolling a device give it a copied live chat session? **Recommendation:** synchronize retained authorized history through Organ sync, then establish that device's own sessions through the reviewed protocol. Existing and newly enrolled devices get the same permitted conversation/thread/message UIDs, private participant mappings and request decisions. Per-device session keys, delivery cursors and authorization remain separate from history.

The composer and message lifecycle must route social conversations through the social delivery path while reusing existing Thread UI. Edits, deletion and closing obey the existing message permissions and explicit social lifecycle policy; they do not imply deletion from another person's device. Revocation denies future session provisioning, pickup and acknowledgements once the current authority is known. It cannot erase plaintext or keys that a device already retained.

Filter social private namespaces and key material from foreign-Organ exports, including broad contact scopes and explicitly shared roots. Carrier-role Cells must not become plaintext recipients merely because their authoring capability list is empty.

**Exit evidence:** reconnect a second device after delivery and enroll a third later; both recover retained authorized history once, with identical UIDs. Test foreign-contact export, restricted carrier access, device revocation, concurrent receipt, crash recovery and deliberate history-retention limits.

### Item 4 — Stranger Requests and optional contact conversion

**D77 — Conversation acceptance versus becoming contacts.** Should accepting an introduction reveal identities or grant access to private Organ Records? **Recommendation:** neither happens automatically. Acceptance opens continued private communication. Each person can reveal their selected profile independently. Only after both choose contact conversion does the app establish the authenticated Organ relationship; private data grants and login rights remain explicit.

The reply descriptor names a scoped pseudonymous mailbox/session route, not the author's personal endpoint or public Organ roster in anonymous mode. Bind the encryption/session key and admission scope to the posting authority so a directory cannot substitute its own reply key. Give the sender a separate pseudonymous reply route. Authenticate session setup against that authority, validate offline prekeys, and keep one-time-key consumption and session state durable.

**D78 — Talking before acceptance.** How much communication is allowed while the request is undecided? **Recommendation:** allow one introduction of at most 2 KiB UTF-8 text and a small provisional exchange, initially three replies per side within seven days. Recipient acceptance grants renewable ongoing conversation permission. Decline closes that attempt; block rejects that identity and its delivery capabilities. A per-post/key limit is supplemented by service-wide limits because an attacker can create new keys.

Requests occupy a reserved, bounded stranger partition. Start with at most 32 pending attempts and 1 MiB per recipient, at most 32 KiB for a complete introduction envelope, plus a configured service-wide stranger-storage ceiling. Refuse or visibly throttle new attempts when that partition is full; trusted conversation/control capacity remains separate. They cannot fill the trusted inbox. Ending a post closes new introductions but preserves separately accepted conversations. Show accept, bounded reply, decline, block, reveal, connect and close actions with their actual effects. Both users may remain pseudonymous indefinitely.

When both convert to contacts, bind the revealed Organ identity to a verified public authority and authenticated route, then link the existing conversation. Preserve all Record UIDs, messages and disclosure decisions. Never turn a name in a message into a trusted contact automatically.

**Exit evidence:** unknown people initiate while the recipient is offline, exchange bounded private replies, accept ongoing conversation and optionally reveal/connect without duplicate histories. Test reply-key substitution, expired capabilities, block, request floods, unilateral reveal and denied private-data access.

### Item 5 — Directories, gossip and delegated search

**D79 — Discovery consent.** Should selecting a server enable all publication, search and forwarding automatically? **Recommendation:** use separate choices. Pin the service identity; show its roles and policy; select which servers receive posts and queries. Gossip needs Cell participation, author redistribution permission and eligible-contact consent. A public announcement can be copied outside the protocol, so these controls govern Lince's behavior rather than guaranteeing control over every public copy.

The directory stores validated public projections and signed profile copies; its FTS5 index is rebuildable. Search treats input as bounded data, with safe text tokenization and explicit direction, language, concept/unit and declared-area filters. Browse uses bounded pages. Search local cache first, then selected services when requested; show cached freshness, source, per-service failures and the next-page control. Merge matching post/revision hashes across sources and surface disagreement.

**D80 — Forwarding and asking onward.** Should a peer be trusted to declare the true hop count? **Recommendation:** no. Maintain a persistent, bounded local ledger keyed by signed announcement revision/hash. Forward once to up to three randomly selected eligible peers while that ledger entry is retained. Do not reset signed expiry. Count inbound bytes, verification work and outbound bytes against independent source and global budgets.

Ask-around uses a different request contract: random ID, at most 30 seconds, at most three eligible onward peers at each Cell, four active local requests and a bounded aggregate response. Retain query deduplication for its lifetime, stop on cancellation/deadline, validate each returned announcement, and explain that contacted peers can read the query. A short lifetime and fan-out limit still need a global work budget to bound branching.

Withdrawals retain a revision floor/suppression marker until all permitted older versions expire plus clock allowance. They are processed even if the original offer never arrived. Reserve bounded control capacity so ordinary traffic cannot starve withdrawal and receipt processing. Public-topic or server-federation alternatives remain design comparisons; they are not necessary to deliver the initial contact-gossip/directory flow.

Add local mute/block, service-specific operator removal and deliberate reports. Reports share only selected evidence; operator decisions never rewrite the author's signed payload. Optional saved searches/subscriptions use explicit notification opt-in, bounded frequency and quiet hours.

**Exit evidence:** two independent directories and a contact cycle; text/typed filtering, duplicates, source disagreement, out-of-order withdrawal, expiry, cache-full behavior, many posting keys, oversized replies, query cancellation and throttling all stay within configured limits.

### Item 6 — Always-online services and reliable delivery

**D81 — What gets acknowledged.** When may a receiver tell a mailbox to discard its copy? **Recommendation:** only after a durable, recoverable local inbox commit, followed by the documented device-distribution policy. Processing may happen later; unreadable mail stays in a bounded retry/quarantine state. For multiple devices, retain per-device acknowledgements for the authorized recipient set rather than letting the first pickup erase everyone else's copy.

Allocate the logical message ID before creating recipient-device envelopes. Each envelope has its own stable authenticated ID/hash, recipient scope, signed issue/expiry and admission proof. Persist the exact bytes for retries. Different device sessions may require different ciphertext envelopes for one logical message; the UI still displays one Message Record. Identical deposit retries are idempotent; same envelope ID with changed bytes is refused.

**D82 — Redundancy and retention.** How much protection should two servers provide? **Recommendation:** deposit promptly to two independent recipient-selected mailboxes when configured, track each durable receipt separately, retain the sender outbox and fetch on reconnect. One selected server is usable with a visible single-copy status. Use the advertised 30-day retention and quotas; distinguish carrier-stored, recipient-durable, conversation-ready and failed/expired. A carrier cannot issue the recipient's authenticated delivery receipt.

Atomic storage reservation covers global capacity, per-recipient quota and the separate introduction partition. Make invite redemption plus registration transactional. Collection is bounded by the exact encoded response bytes as well as count. Every legal envelope must fit at least one legal reply, with room for framing/receipt metadata, so maximum-sized mail cannot become permanently uncollectable.

Current authorization needs a persisted monotonic floor and an explicit freshness policy. Reject presented rosters/device manifests older than the known floor. Publish removal updates to all selected hosts and show hosts still awaiting the update. A disconnected server cannot know an unseen revocation instantly; narrow expiring pickup permissions bound that exposure. Restore must refresh permission floors before resuming collection/deletion.

**D83 — Worker behavior.** Should restart or repeated network failure reset delivery state? **Recommendation:** durable queues, bounded supervised workers, capped randomized exponential backoff, destination-specific health, timeouts and persistent retry deadlines. Each work item is resumable. Expiry is explicit and does not silently delete local conversation history. Full disks produce a storage failure, never a successful durable receipt. Use a database/storage durability setting that meets the receipt promise and test process crashes separately from power-loss assumptions.

**D84 — Server roles and operator defaults.** What should a continuously running server expose? **Recommendation:** independently disabled-by-default directory, townsquare, mailbox and application-relay switches under services.lince.social, mirrored by validated runtime settings. A directory indexes listings, a townsquare browses them, a mailbox stores authorized ciphertext, and an application relay carries only explicitly permitted envelopes. None grants private Organ replication or signing rights. Live onward forwarding may be enabled only with its own per-contact policy; delayed mailbox delivery is the initial reliability path.

Record a small-device and modest-server resource profile before enabling public service. Starting server controls may use 64 global and eight per-peer connections, 1 GiB service storage, 64 MiB per registered mailbox and 4 MiB/minute global incoming plus 4 MiB/minute global outgoing budgets, with stricter individual-source and stranger limits. These are proposed configuration defaults for measurement, not demonstrated capacity claims. Health checks report worker liveness, quota pressure, effective roles and durable queue age without logging message bodies or raw queries.

Add operator controls for admission, quotas, removal/block lists, retention, restore and identity continuity. Service descriptors advertise effective limits and operator contact/policy. Select independent failure domains when choosing two hosts. Iroh connection relays remain a separate infrastructure choice; verify their production availability and fallback behavior rather than treating development relay access as an uptime guarantee. [Iroh relay documentation](https://docs.iroh.computer/concepts/relays).

**Exit evidence:** fix B01–B11 with regressions; restart/kill at each commit boundary; race concurrent quota deposits; retry after a lost response; fail one host; reconnect devices; refuse stale authorization; restore without widening permissions; show truthful failure/copy/throttling states in native UI.

### Shared data ownership and signing rules

| Data | Authoritative home | May public services receive it? |
| --- | --- | --- |
| Source Need/Contribution and private assertions | Existing private Organ Records | Only the explicitly selected public projection |
| Public profile and announcement | Signed author projection; validated hosted/cache copies | Yes, selected public fields |
| Anonymous private key and source/participant mapping | Authorized private Organ/device storage | No |
| Conversation history | Existing Conversation, Thread and Message Records under intended grants | Only encrypted transport envelopes |
| Live session state | Its specific authorized device and reviewed protocol storage | No; history sync does not clone it |
| Service roles, admission and budgets | The operating Cell's configuration and durable service state | Advertise effective public limits, not private operator keys |
| Delivery work and receipts | Durable sender outbox, carrier queue and recipient inbox | Only the routing/admission/ciphertext and receipt fields required for that stage |

**D85 — Exact signing bytes.** Should two implementations choose their own JSON formatting before signing? **Recommendation:** use a documented canonicalization contract, such as RFC 8785 JCS, for typed public documents, with a distinct domain/version tag for profiles, snippets, withdrawals, service descriptors and permissions. Represent exact amounts and large revision values as canonical decimal strings; reject duplicate/unknown fields and unsupported versions. Sign the document without its signature field; changing transport/source metadata does not change the author's payload hash. Reuse the existing sealed-batch transcript where applicable and keep Fact provenance intact. [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785).

Publish fixed vectors covering Unicode, optional fields, exact decimals, nested profile authority, revisions and domain separation before writing network handlers. Validate all size limits against serialized bytes, including base64 expansion and wrappers. Keep public handlers on an isolated social protocol boundary; no public request is translated into arbitrary private sync operations.

### Planned code ownership and review evidence

These are proposed modules or responsibilities, not files claimed to exist already. Keep the existing Engine action/authorship path, Actor/Organ context and Record permissions authoritative; a UI shortcut or worker must not bypass them.

| Layer | Planned responsibility | Review evidence |
| --- | --- | --- |
| nucleus social types | Typed public profile, snippet, search, service descriptor, admission, receipt and command contracts | Unknown/duplicate fields, exact amounts, canonical signing vectors and encoded-size tests |
| store social repositories and existing Record repositories | Validated public cache/index; atomic admission/quota/inbox/outbox/receipt work; scoped private Record state | Unique IDs, transactional message/session commits, recovery, quota races, rebuild/restore tests |
| engine social policies and existing mailbox/sync modules | Publication validation, actor permissions, authority/capability checks, consent, revocation, session integration and private-export filtering | Direct backend forbidden-action tests, cross-Organ isolation and B01–B12 evidence |
| Wire social handlers | Isolated public protocol admission, bounded request/reply framing, pinned service authentication and timeouts | Public service verbs cannot call private replication; malformed/oversized/flood cases |
| cell supervised workers | Queued publication, gossip/search, prompt delivery, collection, backoff, expiry and health | Offline/restart/failover behavior; bounded work and UI responsiveness |
| Native Rust interface and desktop feature modules | Discovery, post/profile forms, Requests, existing Thread integration, status, subscriptions and server/operator controls | Full keyboard-accessible stranger flow, forbidden-edit state, consent previews and failure/resend behavior |
| Headless/NixOS service configuration | Role switches, durable paths, effective limits, identity continuity and operational health | Disabled-by-default tests, validated configuration, restart and restore without permission widening |

Each feature review records code paths, tests actually executed, resource measurements and open failures. A source-inspected helper is marked as a foundation; a passing type check is marked as a type check. Neither closes the social end-to-end gate.

## 10. Interface and social behavior

The interface should teach through visible actions and statuses. It should not expose protocol machinery unless it explains a choice the person needs to make.

**D61 — Where the features live.** Should users navigate several technical network panels to meet someone? **Recommendation:** one Discovery/social entry point with Browse, Search, My posts and Requests. Reuse Organ Castle, Thread Castle and Transfer Castle for their existing jobs; place server/operator details in settings. Under confirmed D09, include a Public profile view/editor in Organ Castle on every UI device, with active-Organ selection, save/preview, publication destinations, permission state, update status and conflict resolution.

**D62 — Browsing and notifications.** Should the first social release include an engagement-ranked feed, public comments and constant push alerts? **Recommendation:** start with useful searchable/browsable snippets and private introductions. Optional subscriptions notify on selected Needs/Contributions, with quiet hours and bounded frequency. Typing/presence and read receipts are off by default. Public discussions and group social features remain separately designed additions.

**D63 — Moderation and blocking.** How should strangers and operators handle abuse? **Recommendation:** local block/mute plus operator-specific listing removal and inbox admission limits. Reporting shares only the content the person explicitly selects. A server can decline a listing without changing its author's signed bytes. Show server policy and appeals/contact information where available; an operator ban is not a universal network verdict.

**D64 — Status and consent.** What must users see? **Recommendation:** anonymous/identified state, selected publication destinations, query disclosure, listing freshness, source, vocabulary, coarse area, pending introductions, delivery stage, number of durable copies, expiry and throttling. Reveal a profile or accept a connection deliberately. Keep network errors understandable: “mailbox full,” “waiting for connection,” or “server limited requests.”

### Required interaction coverage

| Feature | Backend work | UI work |
| --- | --- | --- |
| Anonymous or identified publication | Per-post mode, anonymous posting identity or shared Organ identity, sanitized payload, consent, revisions | Compose/preview, anonymous/identified choice, alias/profile display, targets, renew/withdraw |
| Discovery | Cache, directories, query and source merging | Browse/search/filter, disclosure, source/freshness/meaning |
| Gossip | Destination consent, forwarding ledger, budgets | Simple participation setting; per-contact exceptions and status |
| Introductions | Scoped permission, encrypted Requests, rate limits | Requests, reply, accept, decline, block, identity reveal |
| Conversations | Existing Conversation/Thread/Message Records, retained-history and participant-mapping sync, durable delivery, sessions, retries, device authorization | Same conversations across authorized devices; existing threads plus clear delivery/failure/resend controls |
| Profiles | One public identity/profile per Organ, authorized Record/Fact edits, own-device sync, signed republication, hosting and bounded media | Public profile view/editor on every UI device, preview, permission/conflict/update state, hosted sources, on-demand images |
| Servers | Roles, registration, quotas, health, backups | Add/remove/select server; operator role and throttling controls |

No user-facing feature is complete with only a backend handler. Operator-only tasks need operator controls rather than a public feed. New embedded Sand dependencies must include the required licenses and credits.

## 11. Starting bounds and policy defaults

**D65 — Initial limits.** Are these suitable starting defaults? **Recommendation:** use them as configurable initial bounds, then adjust from measured usage. The implementation must count encoded bytes and processing work, not only visible characters. Defaults are design choices, not existing measured guarantees.

| Item | Starting recommendation |
| --- | --- |
| Snippet title / text | 160 / 1,200 Unicode characters, within a 6 KiB total signed envelope |
| Profile document | 16 KiB; media separate |
| Avatar / banner | 256 / 512 KiB encoded; at most 2,048 × 2,048 / 4,096 × 2,048 decoded pixels, with decoder memory/time limits |
| Stranger introduction | 2 KiB UTF-8 text; 32 KiB complete envelope; at most 32 pending attempts / 1 MiB per recipient and a separate service-wide ceiling |
| Chat message | 16 KiB text; attachments separately requested |
| Social mailbox envelope/frame | 256 KiB complete encoded social frame; reserve wrapper overhead so one accepted envelope always fits a legal reply. Existing larger sync batches have separate explicit limits |
| One service reply | At most 50 items and 256 KiB, whichever comes first; paginate |
| Public listing lifetime | Seven days maximum per signed revision; deliberate renewal |
| Clock allowance | Five-minute future-date tolerance; no expiry extension by forwarding |
| Withdrawal suppression | At least until all permitted older revisions expire, plus clock allowance |
| Mail retention | 30 days, disclosed before relying on it |
| Recipient storage | 64 MiB trusted-mail quota; separate 1 MiB stranger partition, control reserve and global service capacity |
| Server durability | Two independently chosen mailbox copies where available |
| Personal social cache | 10,000 entries on disk; memory bounded separately; smaller mobile profile |
| Gossip fan-out | Three eligible peers per revision per forwarding cycle; persistent ledger and overall budget |
| Ask-around | 30-second lifetime; three eligible next peers; four active local queries; 50 results / 256 KiB maximum accumulated local result |
| Connections | Reuse current per-peer cap of eight; role-specific global cap and pending-handshake deadlines |
| Rate enforcement | Separate inbound/outbound byte and work budgets, per-source and global; persist accounting needed across restart |
| Search history / receipts | Local and private by default; bounded storage and explicit clearing |
| New connections | Unknown until accepted; no automatic visibility/proximity grant |
| Post identity | Explicit anonymous/identified choice; anonymous keys separate from the Organ profile |
| Public profile | One identity/profile per Organ; authorized edits sync and update already enabled publication destinations |
| Own-device history | Retained Conversation, Thread and Message Records synchronize within authorized Organ/grant boundaries |
| Public search services | Explicit selection; no automatic search to every available directory |

Exact inbound/outbound rates and server global storage ceilings must be selected for the intended machine profile before exposure. Validate configuration, reject impossible/negative settings and publish effective limits. Reserve control capacity for withdrawals and receipts so heavy ordinary traffic cannot prevent cleanup; control traffic is still authenticated and bounded.

**D66 — Spam admission.** Should public services require real names or payment? **Recommendation:** neither as the default. Use size/rate/storage budgets, invitation or operator admission where needed, and separate trusted inboxes from introductions. Anonymous unlimited access and strong spam resistance cannot both be assumed; stronger proof-of-work or anonymous credential systems require a separate measured design.

**D67 — Attachments and fetching.** Should every incoming message download media immediately? **Recommendation:** text first, explicit bounded downloads, content-hash verification, safe media handling and cache limits. Never execute incoming content or auto-open arbitrary URLs. Directory and mailbox operators get no private attachment keys merely by hosting ciphertext.

## 12. Sequential implementation order

This is the implementation sequence for feature packages 1–6. Their detailed contracts are in sections 6–10; the build sequence follows dependencies. A step is complete only after its frontend/backend work and exit evidence are recorded.

| Step | Work | Required exit evidence |
| --- | --- | --- |
| 1 | Apply confirmed identity/profile/conversation decisions; settle remaining contracts, session-library integration and server resource profile | Written contracts, tested dependency choice and unresolved gates named; no contradictory promises |
| 2 | Typed domain contracts, deterministic signing vectors, policy/capability interfaces and isolated public transport handlers | Malformed/version/size tests; anonymity field audit; limits enforceable |
| 3 | Existing mailbox corrections B01–B11, transactional inbox/outbox and durable receipt state | Crash/concurrency/replay/revocation regression tests pass |
| 4 | Isolated service roles, opt-in, operator limits, current device authorization | Never-opted-in/no-per-contact-consent tests; restricted-role privacy tests |
| 5 | One shared public Organ profile with device editing/sync/republication; anonymous/identified standalone snippets and OPEN promise projections | Stable identity across devices/hosts; authorized profile updates visible; publication preview matches exact bytes; anonymous posts remain anonymous |
| 6 | Searchable directory and townsquare, hosting, server selection, moderation | Two independent servers; expiry/removal/index recovery; bounded query tests |
| 7 | Stranger Requests, chosen session protocol, pseudonymous replies and mutual Organ conversion within existing Conversation/Thread/Message Records | Offline initiation, preserved conversation UIDs, identity reveal, block, key-change and multiple-device tests |
| 8 | Reliable chat over direct connections and replicated mailboxes; authorized retained-history sync | Both people/devices offline at different times; failover, receipts, one shared logical history, duplicate and expiry behavior |
| 9 | Contact gossip, stable cache, consent, withdrawal and delegated search | Cycles, offline gaps, churn, adversarial source and total-budget tests |
| 10 | Full social UI and notification settings integrated throughout steps 5–9 | User can complete the whole flow and understand each failure state |
| 11 | Headless server operations, restoration and end-to-end release qualification | Restart/restore/failover campaign, privacy and resource measurements |

UI work happens alongside each corresponding feature. The excluded item 7 subjects in section 1 are outside this implementation sequence.

## 13. Verification and definition of done

**D68 — Social acceptance scenario.** Is this the right observable result? **Recommendation:** two people with no prior contact publish/find anonymous or identified snippets, exchange a private introduction, communicate across offline periods and a failed server, optionally reveal identities and connect, then deliberately create a Transfer. Demonstrate both publication modes. Edit a public profile on one authorized device and see the same identity/profile update on the others and selected hosts; synchronize the same conversation history across devices. Neither person obtains the other's private Organ data through discovery.

**D69 — Performance target.** Which workloads should drive limits? **Recommendation:** define one small personal-device profile and one modest server profile. Benchmark 10,000 cached snippets, 100,000 directory listings, and a simulated 500-Cell network with churn and abusive peers. Target p95 local search under 250 ms on the declared reference machine, with all social background queues and memory bounded. Measure first; do not present these targets as achieved.


### Social release gate

- [ ] A fresh user can select services and discover someone outside their contacts.
- [ ] An anonymous snippet has no unwanted public link to the author's Organ.
- [ ] The author can deliberately publish a Need or Contribution under the one public Organ identity instead.
- [ ] Every UI device exposes public-profile controls and enforces editing permissions.
- [ ] Editing the profile on one authorized device updates the same profile on other devices and selected hosts, with queued status while offline.
- [ ] Concurrent profile edits converge to one current profile with visible conflict handling; stale hosts cannot replace a newer profile.
- [ ] Profile edits and identified posting never expose anonymous posts or their private source mappings.
- [ ] Operators/public readers see only what the documented privacy model permits; known metadata limitations are shown honestly.
- [ ] A stranger can send a bounded encrypted introduction while its recipient is offline.
- [ ] Both parties can keep pseudonyms or reveal chosen profiles; contact status changes only through deliberate acceptance.
- [ ] Identity reveal preserves the existing Conversation, Thread and Message Records and their UIDs.
- [ ] Authorized devices, including authorized newly enrolled devices, receive retained conversation history and participant mappings within the intended Organ/grants.
- [ ] Private conversation works while each person is offline at different times.
- [ ] A server outage, response loss or client crash does not destroy the only acknowledged recoverable copy.
- [ ] Retrying and multiple servers produce one logical message, not duplicate UI messages or Facts.
- [ ] Device revocation blocks later collection and deletion; key changes do not silently redirect a conversation.
- [ ] Carrier operators cannot read private content; session-security claims match the reviewed implementation.
- [ ] A post can be edited, paused, fulfilled, cancelled or expired without stale results appearing current.
- [ ] Gossip, delegated search, public browsing and every carrier respect opt-in and measurable resource ceilings.
- [ ] The person sees delivery, freshness, failure, throttling and server-copy status.
- [ ] Local data remains usable without internet access; network workers do not freeze the interface.

### Required test campaign

| Area | Cases that matter |
| --- | --- |
| Identity and privacy | Anonymous/identified mode integrity; anonymous field leakage; alias reuse; one Organ identity across devices/hosts; signed profile substitution; key changes; text/metadata disclosure |
| Profile editing and sync | Authorized and forbidden edits; offline save/republication; concurrent edits; stale hosts; stable identity during name/avatar changes; anonymous posts unaffected |
| Signatures and encoding | Wrong key, altered content, cross-protocol replay, noncanonical bytes, unknown versions, malformed lengths |
| Publication lifecycle | Out-of-order revisions, same revision with conflicting payload, withdrawal before original arrival, stale renewal, clock skew |
| Delivery | Kill process before/after deposit commit, lost acceptance response, local receive commit, processing commit and acknowledgement |
| Storage | Concurrent quota races, disk full, database busy, service restart, encrypted backup/restore, duplicate deposit |
| Permissions | Never opted in, per-contact default denial, stale roster, revoked Cell, cross-Organ import, carrier key isolation |
| Search and gossip | Cycles, random subset behavior, bounded seen-sets, many identities, expensive queries, oversized replies, directory disagreement |
| Sessions and devices | Offline initiation, simultaneous first messages, out-of-order messages, skipped-message limits, retained-history sync/new-device enrollment, no duplicate conversation on identity reveal, device removal, state rollback |
| Sync and meaning | Exact amounts, differing vocabularies, private links, conflict resolution, non-repeated Fact effects |
| UX | Entire stranger-to-contact flow, keyboard/accessibility support, offline statuses, block/withdraw, understandable retry errors |

Use deterministic simulations where possible and real network/device tests for assumptions simulations cannot prove. Run cargo check for changed targets, with warnings treated as errors, and the relevant correctness/security/performance tests. No cargo build. Do not spawn coding agents or create worktrees. Do not edit AGENTS.md, README.md or the owner's .lingua records.

## 14. Criticisms and recommendations carried forward

1. **Finding people and delivering messages are separate systems.** Share validation and budgets, but give publication, search and delivery distinct contracts and statuses.
2. **Anonymous cannot mean everything is hidden.** Separate public identity from network metadata and content-based identification. Stronger network privacy changes the stack and must be deliberately chosen.
3. **Existing sync payloads expose Organ identity.** Reuse their durable mechanisms, not their identifying envelope unchanged for public anonymity.
4. **A mailbox receipt is not delivery.** The early-acknowledgement bug is a concrete example; persist before acknowledging and show each stage.
5. **Always-online does not mean loss-proof.** Two independent copies, durable local state and restoration tests are necessary; expiry and lost keys still have visible failure outcomes.
6. **A hop ceiling does not secure freely public data.** Local budgets, deduplication, expiry and consent provide enforceable limits. Do not label unverified routing hints as facts.
7. **Public visibility needs withdrawal and versioning.** A cancellation can limit current discovery without recalling copies already distributed.
8. **Multiple directories bring both resilience and disclosure.** Let people select them and see which service received each action.
9. **Empty authoring capabilities do not establish confidentiality.** Blind carriers need separate data/key boundaries.
10. **Shared words are not shared meaning.** Preserve vocabulary identity and exact units when moving from discovery to agreements.
11. **Cryptographic primitives are not a complete messenger.** Sessions, device changes, durable state and rollback behavior need their own review.
12. **Complete must be observable.** Use the six-feature release gates above; do not close a feature because a helper function exists.

## 15. Refinement rounds

Comments can refer to decision numbers. Change only what the human changes, update dependencies and test criteria accordingly, and leave unmentioned recommendations in place. Record tensions with .lingua openly rather than editing the owner's source.

1. Identity and connection: D04–D09 confirmed in round 3, with optional anonymous/identified posting, one public Organ identity/profile, device editing controls and Record-based conversation sync. D10 remains the unchanged trust recommendation.
2. Public content and discovery: D11–D31, including public redistribution and world reach.
3. Servers and conversation reliability: D32–D44, including the session stack and multiple devices.
4. Interface, moderation and bounds: D61–D67.
5. Acceptance and measurements: D68–D69.
6. Implementation detail review: D71–D85, covering projection boundaries, concurrency, device sessions, admission, disclosure, durability and exact signing bytes.

The next refinement group is profile publication and discovery, D11–D31 and D71–D74/D79–D80, using the confirmed identity model. Session-library integration and the server resource profile remain named technical selection gates. Recommendations remain working defaults unless the human alters them; do not treat a missing comment as a feature removal.

Implementation progress and executed checks must be recorded against each of the six feature packages. This document update does not mark their implementation complete.
