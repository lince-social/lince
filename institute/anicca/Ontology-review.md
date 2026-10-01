# Ontology: remaining implementation plan for social features 1–6

Updated 2026-10-01. Implement the six approved packages sequentially, keeping the design simple and reusing existing sync. Work on the current branch and preserve concurrent Karma/UI changes. Never modify the owner’s .lingua files, AGENTS.md or README.md. Use cargo check, treat warnings as errors and add no code comments. Item 7 remains excluded.

## Active implementation guide

No whole package is complete. Publication/profiles, private Requests, remembered servers, directory browsing, contact gossip, contact queries, moderation, deliberate reports and saved searches have qualified implementations. Source disagreement, inactive metadata cleanup, restore and operational/resource closure remain open. Finish backend, native UI and meaningful qualification together. Replace a completed subject's long checklist with its short verified description; preserve the detailed requirements of unfinished subjects across compaction.

Verified private-flow subject: [the two-person regression](../../crates/engine/tests/social_request_flow.rs) passes on the normal test stack. It covers two independent hosts, one offline host, exact ciphertext retries, bounded provisional replies, acceptance after post withdrawal, actual device enrollment with history arriving before fresh session authorization, concurrent reception, quantity converging to one, signed profile Reveal, mutual Connect, disabled foreign general sync, own-device contact reconstruction and block/archive/unblock. Six mailbox regressions and retained history also pass. Social actions use a smaller dispatch path with the existing actor/permission checks, avoiding the general dispatcher’s stack overflow. The current native component conflict is recorded in the checkpoint below.

Completed controls: [persistent blocks/admissions](../../crates/engine/src/social/admission.rs), [private profile bindings and mutual contacts](../../crates/engine/src/social/reveal.rs), and [native Requests controls](../../crates/desktop/src/organ_castle/social.rs). Blocks survive archival and apply across conversation tokens from the same pseudonymous owner. Only a newer explicit introduction window resets host counters. Reveal is private, requires a current reviewed profile, and binds that profile to the token and both participants. Mutual Connect derives a known contact with general sync disabled, empty broad scopes and no invented network endpoint or private replica grant.

Existing foundations to reuse: typed signed public snippets/profiles, separate anonymous posting authority, exact preview, draft/edit/withdrawal actions, source privacy filtering, bounded public cache/index, durable publication work, seven-day profile/editor authority, offline profile drafts, safe deliberate image loading, per-device encrypted SDK state and independent delivery/pickup workers. Latest gates passed: publication 37, full Requests 1, contact queries 5, focused units 6, discovery 3, gossip 4 and private transport downgrade 1. Native Organ runtime passed 21 tests; Desktop and Cell cargo check --tests passed with warnings denied. Earlier six mailbox regressions remain retained evidence.

Confirmed device contract: the owner device authorizes fresh messaging keys through existing own-device sync. Already authorized keys last seven days; hosts immediately enforce known newer revocations. Retained history arrives even if the owner is offline, while sending from a fresh device waits. Each device keeps separate live accounts/ratchets. Independent hosts receive no Main identity secret or pseudonymous owner wallet. Restore history and the separate authority wallet, then establish fresh sessions rather than rolling back or cloning ratchets. Lost required keys and every backup cannot be recovered from history alone.

Asynchronous lease contract: a seven-day authorization limits new deposits; it does not shorten an accepted chat envelope's thirty-day retention. Pickup validates sender authorization at the selected host's recorded admission time, envelope expiry at the recipient's current time, and current known generation floors. The recipient still needs its own fresh pickup authorization. Admission time is a claim of the deliberately selected authenticated mailbox, not an independently provable clock. A host cannot forge sender signatures or decrypt content, but its timestamps and availability are trusted within this role. An explicit late-pickup regression is being added; do not claim it has passed until recorded here.

Concurrent-message contract: social Conversation/Thread/Message Records open at zero and assign presence one using the existing quantity register with offset one. Each device’s authenticated import Fact has a distinct UID and zero delta. Passive own-history import does not create ordinary synthetic social Message events. Actual send/receive enqueues one stable logical event for existing idempotent rule/effect processing. Convergence and effect-commit/queue-cleanup replay pass; full process/storage faults remain open without changing concurrent Karma code.

Completed backend subject: [recipient refusal](../../crates/engine/src/social/refusal.rs) retains at most 256 device-local failed-ciphertext references and errors, without another plaintext history. Explicit Discard saves intent before I/O and signs exact envelope/message/content hashes. Hosts preserve recipient-refused across retries/restart and retain spam counters. Sender verification rejects forged status, cancels pending local copies and retains Message history; another own device observes the refusal through existing sync. Six mailbox regressions and the expanded two-person flow pass. Native Organ runtime checks passed. This cannot erase copies already held by other devices or operators.

Additional verified qualifications: blocked fresh conversation tokens are refused; declined/blocked logical duplicates preserve the original retained Message and state. Existing Karma applies the Message effect once when its stable event is replayed after effect commit; this simulates lost queue cleanup, not a power-loss campaign. The real QUIC transport regression denies private own-history fetch on both an existing connection and a new connection after write-to-read-only downgrade and after removal, without adding private grants.

Remaining private-flow qualifications: full restore/key changes, old-route discard deferral, ending-at-first-registration, pagination/draft resumption and deletion cleanup. Expired-proof recovery now shows retained consent separately from current verifiable identity; fresh Reveal is required before contact reconstruction. Global block/admission metadata cleanup remains bounded but must not silently forget active denials. Full process/storage fault injection is still open.

Completed retained-block subject: [admission controls](../../crates/engine/src/social/admission.rs) keep newer deleted-context decisions on the stable private Organ, combine them with original entries using the monotonic window and require Organ settings permission for deleted-context controls. Own-sync qualification and two focused units pass: the same denial can be deliberately unblocked on another enrolled device without restoring its deleted Record, and original/retained maps share one 256-pair bound without double counting or dropping active denials. Host updates wait visibly for current device authority. Cleanup of inactive historical admission/window evidence still needs a retention rule and tests.

Completed archive/key-continuity subject: ended posts and archived materialized introductions stay hidden, reject edits/resumption and protect their key contexts from ordinary deletion. Archival waits for unfinished final-message windows. Active conversations/blocks keep authority renewable; fully inactive archives stop automatic renewal, preserve old transport keys through the delivery grace period, and allow deliberate preparation of fresh authority for later controls. Public units and the updated two-person flow pass these cases without restoring archived Conversations.

Completed proof-recovery subject: Requests and contact reconciliation validate retained profile bindings against current time and known authority floors. Native controls separate retained consent, an existing contact and the need for fresh Reveal; expiry preserves history and existing contact permissions. Stale proof cannot silently reconstruct a contact. Unit, native and full Requests gates passed.

Completed directory-frame subject: trim documents and controls to the complete response bound, explicitly label incomplete authority refresh, reject malformed control arrays and independently verify result hashes. Three discovery regressions and native continuation controls passed.

Implementation locations: [contracts](../../crates/nucleus/src/social.rs), [storage](../../crates/store/src/social.rs), [actions](../../crates/engine/src/social.rs), [profiles](../../crates/engine/src/social/profile.rs), [history/message transactions](../../crates/engine/src/social/conversation.rs), [drafts and Requests](../../crates/engine/src/social/outbound.rs), [delivery workers](../../crates/engine/src/social/delivery_worker.rs), [mailbox](../../crates/engine/src/social/mailbox.rs), [service handlers](../../crates/engine/src/social/service.rs), [public transport](../../crates/engine/src/wire/social.rs), [native forms](../../crates/desktop/src/organ_castle/social.rs). Migrations 0203–0204 and 0310–0325 belong to these features; preserve unrelated migrations and changes.

Next order: finish remaining publication/profile/history qualifications and inactive retention, package 5 reports/subscriptions/source disagreement, then package 6 restore/delivery/resource closure. Shared headless hosting/supervision is implemented. Existing full own-device Wire requires current write-capable sibling membership and rechecks it per frame; retain this protection for private history and shared editing leaves. Removed/downgraded devices can retain old keys, so authority rotation and host generation floors remain necessary.

Current qualification checkpoint: saved searches are qualified (backend 6, moderation 5, discovery 3, Native Organ 26, Desktop/Cell check --tests). Expired resend and exact delivery review are linked; Native Organ 27 and Engine library checking passed. Backend five-flow/mailbox/session gate v2 failed on the nested resend_races test module path, now corrected explicitly; rerun in /tmp/lince-social-resend-backend-v3.log. A fresh Desktop/Cell --tests check is blocked by concurrent component_push tests using removed ComponentState::Record.start_call (lines 405, 423, 469, 577); do not alter that agent's files. Thread qualification is queued in /tmp/lince-social-resend-threads-v1.log and may encounter the same mismatch. Profile-device avatar and gossip-source privacy reproduction gates are queued in /tmp/lince-social-{profile-device,source-privacy}-reproduce-v1.log; do not claim those bugs proven before their results. Preserve unfinished resend/media/source contracts. Encrypted restore, inactive cleanup, broader health, relay/fault/resource qualification and sharing audit remain; no whole package is complete.

Completed hosting/supervision subject: [Engine operator controls](../../crates/engine/src/social/operator.rs), [Cell startup/supervisor](../../crates/cell/src/social_host.rs), [native health](../../crates/desktop/src/organ_castle/social/operator.rs) and [NixOS configuration](../../flake.nix) pass backend 3, Cell 2, native Organ 22 and NixOS evaluation gates; Desktop/Cell checking also passes. `services.lince.social.enable` manages independent directory/townsquare/mailbox roles and validated limits/contact/policy through an 8 KiB JSON file loaded before Wire. Unset deployment preserves native choices; managed settings reject native overwrites. Social worker crashes restart with bounded backoff; shutdown cancels the active child. Health exposes bounded local counts/times without query or Message contents. Deliberate index rebuild revalidates signatures/current floors and preserves ending evidence. Existing systemd restart and separate Iroh relay modules are reused. Application onward carrying remains unavailable and is rejected; no customer identity wallet is granted to an independent host. Remaining operator moderation, broader delivery health, restore, physical limits and deployment qualification stay below.

### Retention and restore details still to close

Completed owner-context retention: [maintenance](../../crates/engine/src/social/retention.rs) and migration 0322 inspect 64 contexts per pass, isolate malformed contexts visibly, stop leases only for explicitly archived/deleted contexts without live activity, and preserve active blocks. Retirement on the authorizing owner waits through the last issued authority plus 30 days, rechecks current wallet/control and activity in a write transaction, and deletes only expired local transport work/account/session state. History, owner wallets, signed evidence, blocks and floors remain. Dormant/retired contexts leave the renewable 256-context set; registration checks also rotate in bounded batches. Eleven social units and the updated full Requests/session regressions pass ongoing conversations, pending Discard, grace boundaries, fresh authority, deliberate key recovery and malformed/completed-job handling. Native key status shows dormancy, earliest retirement and bounded private error detail.

Remaining retention work: other devices conservatively retain dormant keys because their offline copy cannot prove the owner's newest issued permissions. Propagate an owner-signed retirement decision before automatic deletion there, or keep that conservative bound explicit. Compact inactive historical admission/window evidence without forgetting active blocks or monotonic floors. Owner wallets and permanent host ledgers remain bounded; exhaustion must stay visible rather than silently dropping denials. This is independent from safe backup restore.

Encrypted backup/restore is still an actual missing feature. Reuse the existing database/Record history and separate authority-wallet encryption rather than copying live ratchets. Backup must capture a coherent database snapshot, relevant root/transport/pin material and the separately encrypted owner wallet; exclude live accounts/sessions or mark them unusable after restore. Authenticate the complete manifest and limits, encrypt before writing a new file, avoid logging passwords/keys, reject wrong passwords/corrupt or oversized files before mutation, and stage restore for atomic installation while the Cell is stopped. A stale owner roster must never reauthorize a removed device. The human explicitly confirmed owner-only authorization after restore, with fresh enrollment of every other device. Restoring old permissions must not authorize a formerly removed device. Even owner-only restore needs current host authority floors before publishing fresh anonymous/profile control generations. Hosts restored from old backups must similarly refresh floors before accepting old leases; inability to learn current floors is a visible recovery state.

Completed atomic session reset: [reset](../../crates/engine/src/social/session.rs) now preserves the local encryption wrapping key and deletes live accounts/ratchets, holds outgoing ciphertext and cancels destinations in one SQLite transaction. The injected database failure first reproduced unreadable accounts with the old filesystem-before-commit reset; the regression now passes rollback, successful reset, fresh messaging identities and separate authority-wallet continuity. Retained history is unaffected. The expanded queue assertions are included in the current report gate. This fixes reset atomicity; encrypted backup/restore remains missing.

### Discovery controls still to implement

Completed personal hiding and host removal: [backend](../../crates/engine/src/social/moderation.rs), migrations 0323 and [native controls](../../crates/desktop/src/organ_castle/social/moderation.rs) preserve signed cache/floor evidence. Private post/public-author mutes use existing Own field sync and tombstones, with a 256-entry/64-KiB live limit; paged review/unmute can resolve a concurrent over-limit merge. Raw search cursors survive hidden pages, and saved contact answers are filtered locally. Independent host removal persists across revisions/restart, suppresses listing/active refresh/onward gossip, and restores only currently valid evidence transactionally. Withdrawals and authority controls still propagate. Five moderation and five gossip regressions pass, including enrolled-device sync, foreign-export denial, overflow recovery, expired restoration and already-queued removal. Native Organ 26 and Desktop/Cell checking passed after paging and saved-search integration. Hiding/removal sends no report automatically.

Completed deliberate reports: [contracts](../../crates/nucleus/src/social/reports.rs), [Engine](../../crates/engine/src/social/reports.rs), migration 0324 and [native controls](../../crates/desktop/src/organ_castle/social/reports.rs) send one exactly previewed signed public Snippet and optional explanation to one chosen operator, with no attached Main profile/private source. Actor-private local work retains at most 32 jobs; supervised sending retries identical bytes and distinguishes intake acceptance, refusal and expiry. Directory/townsquare intake retains at most 256 reports / 4 MiB for seven days, with eight accepted per transport source and 256 globally per UTC day. Dismissal preserves independent replay/admission evidence; invalid, conflicting, expired and mailbox-only submissions fail. Report work expires after seven days, remains visibly expired for one day and can be cleared without recalling remote intake. Four backend regressions pass privacy, lost receipt/restart, actor isolation, queue/expiry and daily/global limits after dismissal/reopen; Native Organ 25 and Desktop/Cell checking with warnings denied pass. Reports do not remove a listing automatically, and transport origin anonymity is not promised.

Completed saved searches: [typed filters](../../crates/nucleus/src/social/subscriptions.rs), [Engine and worker](../../crates/engine/src/social/subscriptions.rs), migration 0325 and [native settings](../../crates/desktop/src/organ_castle/social/subscriptions.rs) use existing private Own field sync/tombstones. Sixteen filters, eight active, one-hour minimum and independent default-off Cell participation bound automatic queries. Each supervised pass claims one due filter with a durable lease and thirty-second deadline; current actor, membership, participation and unchanged configuration are rechecked inside imports and match commits. Offline cache/reconnect/restart scheduling, paged over-limit resolution and current withdrawal/mute filtering are qualified. Device-local public match references retain 256 seen identities per filter / 4,096 per Cell through expiry; clearing the display preserves deduplication and exhaustion is visible. Optional in-app notices use generic text, coalescing and local quiet hours (22:00–08:00 by default). Notice attempts are advisory, while saved matches persist. Six backend regressions, moderation 5, discovery 3, Native Organ 26 and Desktop/Cell checking pass with warnings denied. The human explicitly selected the existing in-app feed; OS push while closed remains outside this increment. Viewing settings/results makes no network request.

### Profile renewal and offline draft contract

An enabled public profile renews automatically with the same fields and selected hosts when its authority is near expiry and the owner can renew it. This changes signature freshness, not claimed activity or presence. Withdrawn and local-only profiles never renew automatically. Multiple heads require review; renewal checks the retained state again inside its write transaction so it cannot overwrite a concurrent human edit. Posts retain their deliberate lifetime and explicit renewal. An authorized device with expired or missing public editing authority still saves its selected profile fields, parents, hosts and publication/withdrawal choice as a bounded private draft through existing own sync. Keep one latest pending draft per device, plus signed profile branches; the owner signs queued edits after reconnecting. Commit draft consumption and the signed profile atomically, preserve concurrent branches, and keep errors visible when old parent history needs manual resolution. Native forms restore the local draft and distinguish saved/waiting from hosted publication. If owner authority is unavailable, retain the profile and show expiry rather than granting an independent service signing rights.

### Contracts to preserve during the remaining work

Messages stay in Conversation → Thread → Message Records. Live SDK accounts/sessions remain encrypted and device-local; work tables contain references, ciphertext and retry metadata. Introductions first save a signed MessageDraft and wait visibly for owner authorization. Ordinary retries retain identical ciphertext. A verified replacement recipient key requires a new session/envelope while preserving logical Message/content identity and creation time. Pending copies are cancelled or held after known revocation. Public post archival cannot strand a separately accepted conversation.

The private owner binding is signed by the Organ root and synchronized only between owned write-capable devices. Other devices request authorization with their current operational keys. The owner wallet retains latest requests and rejects old/conflicting requests after restore. Replacing live device identities advances the pseudonymous generation; routine certificate renewal preserves live account identity. Keep current/previous fallback keys for the advertised envelope lifetime. Restore clears live ratchets with a fresh local storage key while retaining the separately encrypted owner wallet and Record history. A complete filesystem rollback requires deliberate restore/reset.

Public envelope IDs derive from canonical immutable sender/ciphertext bytes. Logical Message identities include the private token and author owner. Conflicting same-ID content fails without advancing the saved session. Reception commits account/prekey/session changes, Records/links, deduplication, import Fact, stable event intent and recipient receipt atomically. Only then acknowledge storage at the host. Current certificate authorization can accompany unchanged ciphertext. A carrier receipt means storage; a signed recipient-durable receipt means import; explicit refusal, expiry and optional reading are separate states.

Public profile versions compare authority generation before revision. Signed parent evidence contains at most 64 ancestors; a host missing more requires explicit resolution or owner-authority advancement. A new generation supersedes every old editor revision, including the maximum integer. An ending-only delegation for an old identified post cannot authorize new active publication. Profile root pins and public ending floors survive display-cache expiry. Anonymous IDs bind separate owner authority, never the Main Organ root.

Remaining qualifications keep these concrete cases: actual enrolled-device offline/concurrent profile edits and media; more than 64 missed profile ancestors; restore without resurrected permission; old-route discard waiting for unavailable keys; ending delivered to a host that first registers after expiry; deletion and bounded denial/admission cleanup; meaningful effect replay after its commit; existing/new native connections after a write-to-read-only downgrade; native Thread/Requests controls; physical resource and fault limits. Backend/UI denial alone does not prove that current editing secrets are absent from an unauthorized device.

The full private content is limited to 20 KiB, the complete encrypted delivery to 32 KiB, and a public/collection frame to 256 KiB including wrappers. Signed purpose distinguishes introduction, text and control; decrypted content must match it. Introduction/control reserves remain usable after three provisional texts. A failed ciphertext reference is a temporary deferral unless the recipient explicitly chooses Discard. Do not classify a missing history/session prerequisite as permanent loss.

### Concrete implementation boundaries

- Keep source, publication-authority secrets, editor drafts and participant/request/reveal mappings in scoped Record extensions or associated Records. `store::records::set_extension_on` already logs extension changes to existing sync; use that path and the Engine actor/Fact path. Do not introduce a parallel private-history database or copy live session ratchets through ordinary history sync.
- Public snippets/profiles have narrow typed documents and fixed signing bytes. Public IDs differ from private Record UIDs. A directory stores only validated public documents; delivery services store admitted ciphertext. Private extension namespaces must be excluded from foreign-Organ exports, even through broad grants.
- Keep publication/hosting/search/reply permissions explicit. Native UI uses authenticated Engine actions. New public Wire handlers have bounded request/reply framing and never dispatch arbitrary private Actions.
- Give publication and delivery jobs durable identity, destination, state and retry deadlines. Reuse the existing inbox/outbox, receipts, idempotency and quota machinery where its identified-mail contract applies; anonymous Requests need their own identity-free envelope and admission scope.
- Extend the active Organ/Thread interface and add a simple Discovery view with My posts, Search/Browse and Requests. Every visible button must have its backend path and understandable offline/failure state. Integrate social message composition into existing Thread Records.
- Use existing Rust/Iroh/SQLite layers. Implement contact gossip with a bounded durable forwarding ledger; no additional gossip platform or compulsory federation. Evaluate a reviewed Rust session library rather than inventing cryptography.

### Package checkpoints to keep current

| Package | Code paths to reuse | Required retained evidence before slimming |
| --- | --- | --- |
| 1 — Publication | Engine Actions/actor permissions; Record extensions, OPEN export; native forms | Exact preview/signature, separate anonymous authority, source/privacy audit, edit/withdrawal and native flow |
| 2 — Shared profile | Organ identity/roster; Record/Fact edits; own-device extension sync; blob facilities | Same identity across devices/hosts, delegated editing, offline/conflict handling, anonymous posts unaffected |
| 3 — History | Conversation/Thread/Message, replica roots/grants, own-device sync, message lifecycle | Same retained UIDs on existing/new devices, private mapping isolation, concurrent receipt and separate session provisioning |
| 4 — Requests | Existing threads/invitations; admitted durable work; selected reviewed session protocol | Offline pseudonymous introduction/reply, separate stranger capacity, accept/block/reveal/connect, unchanged history |
| 5 — Discovery | Iroh/Wire; promise-cache concepts; SQLite index; consent/contact state | Two selected services, bounded search/gossip/query work, expiry/withdrawal, source labels and opt-in UI |
| 6 — Services/delivery | Persisted mailbox inbox/outbox/receipts; sync runner; Wire supervisor; headless/NixOS | Independent retry workers, prompt chat fallback, freshness/key-change policy, truthful receipts, roles/restore/resource and full social scenario |

For each increment record the implemented files, exact checks run and remaining gaps here. If concurrent changes block a target, continue an independent authorized task and retain the blocked check and its error; do not rewrite another agent's work to make the check pass.

## Scope and agreed behavior

The source is the [Ontology record](Lince.lingua) and its social networking tasks, now consolidated by the human in the current Records. Ontology connects the meaning of Records with Organ identity, synchronization and discovery. This plan completes its six approved social feature packages. The wider subjects previously grouped as item 7 remain excluded: vocabulary publishing/import, expanded Ontology/Relations exploration, file-sync closure, generic Trail packages, calendar export, Blood integrations and LoRa.

The following choices remain agreed. Unmentioned recommendations stay in force; a comment changes the relevant choice rather than deleting the surrounding feature.

- Each Need or Contribution can be published anonymously or under the Organ's public identity. Anonymous posts use separate posting keys by default; an optional persistent alias deliberately links posts. The owner explicitly authorizes anonymous editing too, using seven-day permissions and immediate enforcement of known revocations. Key replacement preserves the post identity; creating a fresh anonymous identity on another device waits for the owner when offline.
- Each Organ has one stable public identity and one current public profile, shared across authorized devices and selected hosts. Every UI device provides profile controls, with backend-enforced editing permissions.
- Strangers can exchange bounded private introductions and replies without revealing their Organ profiles. Conversation acceptance and becoming known contacts are separate choices. Profile reveal is independently chosen; contact conversion requires mutual acceptance.
- Conversations continue to use the existing Conversation → Thread → Message Records and UIDs. Retained authorized history and private participant/request mappings synchronize between own devices, including newly enrolled devices.
- Publication, directory search, gossip, carrying another contact's traffic and private replication have separate permissions. Selecting a server does not enable every role or disclose every query automatically.
- Anonymous publication hides the Organ association in the public document. Operators can still observe connection metadata, timing and text. Stronger network-origin anonymity is outside this scope.

Publication or discovery never creates an accepted Transfer, changes stock, establishes trust, or grants access to private Organ data. A valid signature proves control of a key, not a real name, skill or location.

## How the network pieces fit

Example: you publish “I can repair bicycles this weekend.” A stranger finds it, sends a private introduction while your phone is offline, and you reply later. Both can keep pseudonyms; sharing profiles and becoming contacts remain deliberate actions.

| Piece | Simple meaning | Remaining responsibility |
| --- | --- | --- |
| Cell / Organ | A running installation / the shared context represented by its authorized Cells | Keep device, identity and permission boundaries explicit |
| Public snippet | A small chosen announcement | Safe anonymous/identified publication, updates and withdrawal |
| Directory / townsquare | Searchable listings / browsing those listings | Independent selectable services with a validated public index |
| Gossip | Passing public announcements between willing peers | Consent, deduplication, expiry and bounded forwarding |
| Ask-around | Asking contacts to search and optionally ask onward | Separate query consent, deadlines and total work limits |
| Iroh relay | Infrastructure that helps endpoints establish and carry a connection | Verify operational availability separately from Lince service roles |
| Lince mailbox | Durable encrypted storage for later pickup | Admission, selected copies, retry, truthful receipts and bounded retention |
| Session / receipt | Private conversation encryption state / evidence of a delivery stage | Reviewed per-device sessions and truthful recipient delivery status |

A connection relay does not replace a mailbox. A directory does not retain private conversations. Gossip announces public offers; it does not distribute private history. Mailboxes cannot guarantee recovery after every copy or required key is lost or retention expires.

An independent always-online mailbox keeps deposited ciphertext available while people are offline. It does not renew their seven-day permissions. A returning owner device can renew its own keys; another device whose lease expired waits for that owner or a deliberately trusted personal authority server. Already admitted accepted-chat copies can remain readable for their thirty-day delivery window, subject to current known revocations and fresh recipient pickup authority. Public snippet expiry is separate: gossip and storage do not extend a post's reviewed lifetime.

## Stack and shared contracts

Decision references: D01–D03, D39, D67, D85. Keep Rust, the current Iroh transport, SQLite/SQLx and the active Rust interface. Add focused social protocols within these layers. No compulsory central account or external social network is required.

| Layer | Remaining responsibility |
| --- | --- |
| nucleus | Typed public documents, search, admission, session-routing and receipt contracts |
| store | Public cache and rebuildable FTS5 index; scoped private state; transactional message/session integration |
| engine | Publication, authority, consent, role isolation, revocation and session policies |
| Wire | Isolated social handlers, pinned service authentication, bounded framing and timeouts |
| cell | Supervised publication, delivery, collection, gossip and search workers |
| interface / desktop | Native social flows alongside each backend feature; reuse existing Organ and Thread views |
| Headless Lince / NixOS | Validated role configuration, durable service state, supervision and restore |

### Remaining shared prerequisites

Typed public documents, canonical signing bytes and Unicode/exact-number vectors are implemented. Private session selection is stable Olm v1 through pinned vodozemac 0.11.1 with default features disabled; its initiation, replies, replay/order and encrypted restart checks passed.

Completed descriptor subject: pinned endpoint inspection reports effective roles, limits, public operator contact/policy and five-minute expiry; disabled roles remain inspectable without enabling them.

- [ ] Finish restore/native qualification and protocol/version refusal across every new verb. Recipient refusal/import, sender authentication, complete own-history frames and bounded social request/reply framing have retained gates.
- [ ] Finish small-device/server resource qualification, supported-target checks and session/message transactions. Bound configured storage, retained ledgers, memory, connections, verification and worker work.

A ratchet advances conversation keys to limit exposure from a later key theft. Saved plaintext on a compromised device remains exposed. History synchronization and provisioning another device's session are separate jobs. [Double Ratchet](https://signal.org/docs/specifications/doubleratchet/), [Sesame](https://signal.org/docs/specifications/sesame/).

## 1. Anonymous or identified Needs and Contributions

Decision references: D04–D05, D12, D16–D20, D71–D72.

**Result:** publish a useful general offer or need without exposing the private Record behind it, with an explicit identity choice per post.

Implemented foundation: standalone and sanitized Record/OPEN-promise projections; chosen public fields and anonymous/identified mode; deliberate alias reuse; exact signed preview; draft, edit, pause, renew, fulfilled, withdrawal and archive actions; persistent ending floors; durable selected-host jobs. Native Discovery has composition, source review, My posts, previews, lifecycle actions and per-host results. Regressions cover privacy, source closure, quantities, stale/conflicting updates and real two-host publication.

### Remaining closure

- [ ] Qualify anonymous wallet restore and ending delivery to an offline mailbox host. Enrollment, owner waiting, actual removal, cached-viewer revocation and maximum-revision recovery passed; preserve their retained regressions.
- [ ] Complete native runtime gates, visible authorization/conflict states and physical resource qualification. Remove detailed lease implementation notes only after these gates pass.

**Completion evidence:** preview matches signed bytes; anonymity field/hosted-metadata audit passes; wrong-key/stale/conflicting updates fail; source changes cannot silently disclose fields; discovery leaves stock, agreements and trust unchanged.

## 2. One shared public Organ profile

Decision references: D09, D11, D13–D15, D73–D74.

**Result:** changing your profile on an authorized phone changes the same public profile seen on other devices and hosts. It does not create a second identity.

Implemented foundation: one Organ-bound public projection, narrow seven-day root delegation, authority/revision floors, succession and editor replacement, own-device Record/Fact sync, selected-host publication, bounded branch/ancestor merging and explicit public media preparation/fetch. Profile images use a separate validated public store. Native controls provide editing permissions, hosts, conflict resolution, withdrawal and opt-in media loading.

### Remaining closure

- [ ] Qualify a pending profile edit from an actually enrolled device through removal, concurrent edits and originating-device media upload. Automatic renewal and missing/expired-authority draft recovery passed; current roster validation and atomic per-device consumption are implemented.

Actual root-signed enrollment and normal Own export/import reproduced the originating-device avatar gap in /tmp/lince-social-profile-device-reproduce-v1.log: the signed draft syncs, but the owner cannot publish because only the selected hash arrives and the prepared image bytes remain on the editing device. Fix this with up to two selected prepared images in the device-signed pending draft, using the existing private Own extension. Each image stays within the current normalized 128 KiB limit; the whole retained profile state stays within 2 MiB and 32 latest-per-device pending drafts. Validate encoding, selected hashes, deduplication, content hash and bounded decoding before signing; import the validated assets, consume the exact draft and commit the signed profile together, after final current originating-device and local membership checks. Do not grant arbitrary blob access or automatically fetch foreign images. Removed-device drafts remain visibly rejected. Independent concurrent heads remain available for explicit review/resolution. Native pending/error/conflict controls must explain image synchronization and the resulting state. Qualify corrupt/wrong/unselected/oversized image data, removal, concurrent edits and selected-host publication before slimming this subject.
- [ ] Recover a host that missed more than the retained 64 ancestors: refresh/import its signed heads for explicit resolution or deliberately advance owner authority. Do not silently discard a conflicting host head.
- [ ] Complete native runtime, fresh Desktop/Cell checking and resource/restore qualifications.

**Completion evidence:** two devices and two hosts agree on one Organ identity; unauthorized backend edits fail; offline/conflicting edits survive restart; stale hosts cannot replace current state; name/avatar changes leave anonymous posts and conversation UIDs intact.

## 3. Conversation Records and own-device history

Decision references: D09, D38, D41, D75–D76.

**Result:** the same retained conversations appear on authorized devices, including a device enrolled later, without duplicating threads when someone reveals a profile.

Implemented foundation: retained Conversation/Thread/Message UIDs and private mappings synchronize through existing own-device operations, including a newly enrolled device; the regression refuses duplicate import and foreign export. Fresh per-device authorization and encrypted account persistence remain separate from history. Receiving typed content commits SDK accounts/sessions, scoped Records/links, immutable content metadata, an import Fact, event intent and recipient receipt together. Regressions passed rollback, two-host ciphertext duplicates, re-encrypted logical duplicates and fresh-key recovery with unchanged UIDs; no private replication grant is created. The aggregate complete sync-frame boundary, including escaping/wrappers, passed. Private introduction drafts reuse the same own-history wrapper and existing MessageDraft kind.

### Remaining integration

- [ ] Finish full process-restart/key-change/deletion and restore qualification. History-first, simultaneous fresh-device receipts, actual once-only rule-effect replay after commit and native Thread/Requests runtime gates pass. Keep the same logical Message and unchanged UIDs.

- [ ] Qualify the implemented fresh-device provisioning, replacement/removal and separate-wallet reset against actual backup/restore. Keep unavailable authority and all-keys/all-copies-lost states visible.
- [ ] Audit foreign-Organ exports, broad contact/root grants, restricted carrier paths and private blobs. Private participant mappings and session keys must not escape through general sharing. Revocation prevents future access under current authority but cannot erase plaintext already retained.
- [ ] Finish inactive admission/window cleanup and non-owner dormant-key bounds without forgetting active denials. Accepted text is immutable; explicit local Message deletion and close-before-archive already preserve other people's retained copies.

### Native UI work

Existing conversation/thread views now show pseudonymous or deliberately revealed social participants, Requests linkage, fresh-session waiting and permission-aware reply controls. Remaining UI work is explicit expired resend and restore recovery, alongside their backend work.

**Completion evidence:** reconnect a second device and enroll a third; recover identical retained UIDs once. Exercise concurrent receipt, cross-Organ denial, carrier key isolation, device removal, rollback and deliberate retention limits.

## 4. Stranger Requests, private replies and optional contacts

Decision references: D06–D08, D10, D39, D66, D77–D78.

**Result:** people who do not know each other can talk privately while keeping pseudonyms, then optionally reveal profiles and become contacts.

Implemented subject: pseudonymous owner/device-signed routes, stable Olm v1 sessions, atomic receive/deduplication, distinct stranger/control/trusted capacity, one introduction and three provisional texts, private acceptance/decline/block/close, profile-bound Reveal and mutual Connect. Native Requests and existing Threads provide those controls and retain the same history. Independent workers register, send, collect and inspect exact receipts while people are offline. Contact conversion creates no general feed or private replica grant. The two-person/enrolled-device test covers this flow, and native forms are permission-aware.

### Remaining closure

- [ ] Qualify simultaneous first messages, bounded out-of-order/restart/restore behavior, request floods, aggregate physical capacity and control progress under saturation.
Expired-proof recovery and existing Thread runtime checks pass: retained consent/history survive, and contact reconstruction waits for current verified Reveal. Remaining identity-key changes and restore recovery need explicit qualification.
- [ ] Qualify old-route discard deferral, durable discard intent after restart and bounded metadata cleanup; sender refusal authentication/retry/restart are verified.
- [ ] Finish the remaining root-grant/blob audit. Foreign general feeds and real transport downgrade denial are verified; consent stays separate from trust, private sharing, Main endpoint verification and optional read receipts.

**Completion evidence:** unknown people initiate while the recipient is offline, reply before reveal and continue after acceptance. Test reply-key substitution, expired permissions, floods, unilateral reveal, blocked new tokens and denied private data; conversion preserves history.

## 5. Directories, townsquares, gossip and ask-around

Decision references: D02, D21–D31, D63, D79–D80.

**Result:** someone can find relevant Needs and Contributions outside their current contacts through selected services and bounded peer sharing.

### Directory and browsing work

Completed browsing subject: bounded filters/public cursors and the exact query/selected endpoints survive native continuation. The 55-document regression covers 50 + 5 + empty pages and rejects private Record cursors. Cache labels keep availability unconfirmed; continuation enables no publication or additional service.

Completed inspection subject: an endpoint-authenticated five-minute descriptor reports effective directory/townsquare/mailbox roles, configured bounds, 30-day maximum mail retention and bounded operator contact/policy. Disabled roles can be inspected; unfinished relay/gossip flags are not advertised. Native inspection/QR and permission-aware hosting controls passed backend/native tests. Inspection enables no role or destination; policy is an operator statement, not verified uptime or physical independence.

Completed server-choice subject: [private per-device choices](../../crates/engine/src/social/servers.rs) and [native forms](../../crates/desktop/src/organ_castle/social.rs) retain at most sixteen pinned endpoints and eight selections per role. Publication, deliberate query and return-mailbox roles stay independent. Saving/removing choices changes no existing signed destination, private queue or contact grant, and makes no network request. Backend and native regressions passed. Operator labels help selection but do not prove independent operators/disks; there is no automatically selected starter host.

Implemented foundation: an isolated signed public cache and FTS5 index, bounded plain-text/typed-filter search, selected independent directory hosts, a separately enabled townsquare role, expiry/withdrawal maintenance and native search/My posts/source results. Publication and search regressions passed on the current target. Public listings remain distinct from private Records and known-Organ address lookup. [FTS5](https://sqlite.org/fts5.html).

- [ ] Qualify supported targets and measured resource tiers. Deliberate signature/floor-aware index rebuild and operator health are implemented and qualified.
- [ ] Qualify merged source disagreement and bounded partial authority refresh; pagination and effective role descriptors are implemented.
Pinned selectable independent servers, inspection/QR and separate publication/query/mailbox choices are implemented and qualified. An optional transparent starter list remains a separate choice; no host is selected automatically, and searching does not publish the searcher's Organ identity.
- [ ] Merge authenticated post/revision/hash results across sources, expose disagreement, show source-specific failures and cached freshness. Rank by relevance, explicit filters and freshness; distinguish declared area, known-contact proximity and unverified routing hints.

Source privacy boundary: an authenticated selected directory is the source of its response. Public replies must not export the directory's private incoming gossip/contact path, internal contact UID or local source labels. Keep such provenance available only in permitted local views; forwarding a chosen public announcement does not grant permission to disclose a contact relationship. A regression now imports real consented gossip, searches locally and queries the directory from outside to check this boundary. Its reproduction gate is queued in /tmp/lince-social-source-privacy-reproduce-v1.log. For subsequent source merging, verify signatures/hashes before observation, distinguish stale valid revisions from same-authority/same-revision equivocation, bound retained observations and keep expiry unchanged. Invalid or unavailable sources remain individually visible; source disagreement must not silently choose the first responder or turn routing hints into location/trust. Preserve raw pagination cursors when ranking a bounded page, and show the ranking basis plainly.
Local hiding, conversation blocks, host removal and deliberate public reports are implemented and qualified. Operator decisions preserve author-signed bytes and stay independent.

Confirmed conflict policy: when independently verified documents from the same signing authority claim different hashes for the same generation and revision, quarantine that public post from discovery, contact answers, subscriptions and onward gossip. Retain bounded signed evidence and show a local explanation. A valid newer author revision or withdrawal resolves the conflict; an older valid copy is merely stale and cannot create a conflict or renew expiry. Invalid signatures cannot quarantine a legitimate post. Use existing cache state/index and explicit source results instead of a new consensus protocol. Bound retained source observations and proof bytes; exhaustion must stay visible and must not leave a proven conflict discoverable. Do not expose private incoming contact paths in public responses. Qualify both arrival orders, restart, already queued forwarding, mute/removal interaction and resolution by a newer signed revision.

Conflict increment limits: keep up to eight immediate authenticated source observations for a post's current revision and at most 8,192 observations globally, evicting oldest observations when full. Keep at most two signed variant documents for each proven conflict, with 256 retained evidence pairs and an 8 MiB total evidence ceiling; an exhausted evidence store must still quarantine the cached post and report that full evidence could not be retained. These are local bounded observations, not a reliability score or agreement vote. Show the check time and source count in local results, and the conflicting public ID/revision and explanation in a bounded local review. Public directory responses continue to name only that directory. Ranking may reorder the bounded returned page by relevance and freshness with stable ties; preserve the raw UID continuation cursor and label this page-local ranking so users are not promised global best-first results. Ordinary source failures remain separate from signed equivocation.

### Gossip and cache work

Completed contact-forwarding subject: [receiver](../../crates/engine/src/social/gossip.rs), [durable ledger](../../crates/engine/src/social/gossip_store.rs), [independent worker](../../crates/engine/src/social/gossip_worker.rs), migration 0320 and native controls pass backend/native gates. Cell and separate per-contact send/receive consent default off. Only signed public announcements with author redistribution permission and public destinations enter forwarding; private history and profile media are excluded. Inventories use the public post ID and exact payload hash. Each retained revision is assigned at most three eligible peers, with persisted choices, bounded retries and traffic accounting. Withdrawals and related authority floors use reserved admission and suppress older copies, including when withdrawal arrives first. Changing redistribution after publication requires withdrawal and a fresh post.

Remaining qualification:

- [ ] Qualify endpoint-change/churn scenarios. The real three-endpoint Iroh/QUIC regression passes gossip, contact queries, withdrawal propagation and private-grant isolation; simulated handlers cover cycles and response loss.
- [ ] Measure physical ledger/cache/index overhead and verification work, many posting identities and traffic saturation. The configured entry/byte ceilings and admission/control reserve are implemented, not measured capacity claims.
- [ ] Qualify control progress when the shared public cache is full, as well as the forwarding ledger. Preserve active denials and ending floors across cleanup and restore.

Completed public-cache accounting fix: real signed revisions filled the configured byte budget and reproduced a refused withdrawal. Directory, gossip and query imports now validate the final transaction size after an authenticated withdrawal removes older revisions; ordinary new data still reserves space first, and identical duplicates do not reserve another copy. The regression passes valid withdrawal, duplicate replay and forged-withdrawal refusal. Entry/byte limits and ending floors remain enforced. Physical disk-full behavior and authority-control reserve saturation still require qualification.
A hop is one forwarding step. Local hop/fan-out limits constrain Lince's work; freely public text can still be copied and reintroduced. They cannot guarantee universal reach or a universal privacy boundary.

### Ask-around and broader discovery work

Completed contact-query subject: [receiver and actor-private history](../../crates/engine/src/social/ask.rs), [independent worker](../../crates/engine/src/social/ask_worker.rs), migration 0321 and [native controls](../../crates/desktop/src/organ_castle/social/ask.rs) pass five backend regressions, budget/clock units, native controls and real QUIC qualification. Participation and separate ask/answer/onward contact permissions default off. Queries start deliberately, retain a thirty-second deadline and split twelve total work credits, fifty results and 192 KiB of documents across at most three children per node. Four active local queries, twenty recent queries and bounded receiver reservations prevent unlimited retained work. Current actor permissions, pins, consent, signatures, expiry and known withdrawals are rechecked. Query history is private to its initiating actor; cancellation stops local work, clearing works offline and saved answers are filtered again. Onward questions carry no original Organ identity; contacted peers can read the question. No private history, local-only post or profile media is exported. Remaining work is churn, physical resource measurements and subscriptions, rather than another query protocol.

Stack comparison checked against the upstream projects on 2026-10-01: [iroh-gossip](https://github.com/n0-computer/iroh-gossip) provides topic swarms and broadcast trees with bootstrap peers. Our initial recommendation remains bounded forwarding over the existing Lince transport, retaining an adapter for a separately opted-in topic swarm later. This avoids making public topic membership a prerequisite for the approved per-contact flow. [Iroh relays](https://docs.iroh.computer/concepts/relays) carry live encrypted connections and retain no application messages; offline conversation recovery belongs to Lince mailboxes. Production relay availability remains an operator decision and qualification task.

- [ ] Qualify contact churn and receiver restart during an unfinished query. Completed reply replay, cancellation, local expiry and restart, conflicting request IDs, aggregate budgets, permission revocation and blocked peers pass. Keep any missing-parent lookup separately bounded rather than recursively fetching unrestricted history.
- [ ] Compare world-reach discovery extensions beyond directories/contact gossip, retaining an adapter boundary. Evaluate separately opted-in public topics/iroh-gossip and bootstrap/privacy/abuse costs. Initial coverage comes from selected directories and bounded gossip; no universal-delivery promise or compulsory server federation. Mirror only to author-allowed destinations, respecting each operator's moderation.

### Native UI work

- [ ] Finish disagreement/freshness labels. Native Browse/Search/My posts, filters, server selection, paging, mute/block/removal/reports, gossip/contact consent and Ask contacts/cancel are implemented; rendering does not start network work.
Saved searches/subscriptions, deliberate notification opt-in, bounded frequency and quiet hours are qualified. Engagement ranking, public comments and group social features remain outside this release.

**Completion evidence:** two independent directories and a contact cycle handle duplicates, conflicting sources, stale results, out-of-order withdrawals, cache-full behavior, query cancellation and abuse within configured budgets. Distinguish proximity from physical nearness and hops in the UI.

## 6. Always-online services and the remaining delivery work

Decision references: D03, D32–D38, D40–D44, D81–D84.

**Result:** private conversations continue across offline periods and server failures, with truthful progress and bounded service costs. The existing durable mailbox mechanisms are reused; the following gaps remain.

### Complete autonomous delivery and authorization

Implemented and qualified: independent supervised preparation/send/pickup workers retain durable work, exact ordinary-retry ciphertext and destination schedules. Current membership and fresh seven-day authority gate sending; known revocations hold obsolete local copies. Prompt direct delivery and selected mailbox fallback work independently. Authenticated carrier and recipient receipts distinguish queued, stored, recipient-durable, conversation-ready, refused and expired states. Receiver-approved trusted admission remains separate from bounded introductions and control capacity. Read receipts, presence and typing remain off. The two-person flow, mailbox and new-device history regressions cover these paths; a full process/storage fault campaign remains below.

- [ ] Add deliberate expired-message resend in an accepted conversation. Give it a fresh authenticated delivery window while preserving the logical Message/content/history and effect identity. Do not renew introductions, refusals or closed conversations automatically.

Expired resend increment contract (next implementation): add a distinct confirmed action, leaving ordinary Resume unable to extend a lifetime. Require the current actor's access, current own-device write membership, a retained outgoing Message in a live accepted conversation, an expired delivery window and immutable authenticated Text content. Refuse introductions/control messages, archived/deleted/closed/blocked conversations and known recipient-durable/refused messages. An archived source announcement does not close an accepted conversation. Prepare current owner-authorized device keys through the existing path; if the owner is unavailable, keep a visible waiting work item rather than granting a host signing power. Preserve Message/Thread/Conversation UIDs, content hash, original creation time, quantity and effect identity. Give only the new envelope/work a thirty-day lifetime; current seven-day authorization is still required. Ordinary retries of that envelope remain identical. Inside one write transaction, recheck actor/membership, participant/content/delivery snapshots and live Records, cancel old local copies/destinations, replace the old preparation work and save the fresh deadline/origin state. Do not depend on work_on's ordinary conflict branch, which intentionally does not extend expires_at. A late nonterminal response for a cancelled copy must not overwrite the fresh attempt; a verified terminal receipt still applies to the same logical Message. Existing encrypted preparation creates current envelope IDs/ciphertext while preserving logical content. Other devices learn the same logical status through Own sync and may deliberately resume that fresh attempt. Old deposited copies expire normally; they cannot be recalled. Recipient logical-content deduplication and stable effect events must prevent duplicate Records/stock changes even if the old receipt was lost. Native Requests controls show the immutable Message, original creation date and new deadline before confirmed resend; Thread delivery UI links to the same controls. Rendering never resends. Qualify expiry boundary, refusal/durable/provisional/closed/permission denial, concurrent state change, same content/UIDs/effect, changed envelope/fresh authorization, restart and recipient replay.
- [ ] Qualify sender/recipient key replacement, root succession and permission changes during pending retries. Show held/replacement-envelope states clearly, and retain exact bytes when the same envelope remains authorized.
- [ ] Finish old-route Discard deferral and ending delivery to a host that first registers after expiry. Preserve durable intent through unavailable keys and restart.
- [ ] Complete encrypted restore and refresh authority floors before resumed publication/pickup. Stale host or restored-device evidence must leave a visible recovery state.

### Isolate roles and finish operator support

Implemented and qualified: independently disabled directory/townsquare/mailbox roles, remembered role-specific servers, pinned inspection, native settings, deployment-managed NixOS configuration and worker supervision. Existing Wire connection/handshake/frame limits are reused. Index rebuild preserves signed evidence and current floors. Independent mailbox hosts receive ciphertext and scoped authorization rather than private Organ/session/identity secrets.

Confirmed relay scope: use existing Iroh relays for live connections and Lince mailboxes for delayed delivery. No separate Lince server-to-server forwarding protocol is added in this release. Iroh relay deployment/configuration and fallback still require concrete verification. Application-relay role flags remain rejected rather than advertised as available.

- [ ] Add outbox/destination health, including held/expired/refused work, delivery queue age and delayed retries. Keep default logs and health summaries free of Message/query bodies.
- [ ] Finish admission/block/resource/retention views. Report intake/review is qualified; host-local listing removal is separate from author withdrawal and private conversation blocks.
- [ ] Extend measured capacity from payload quotas to disk/index/WAL overhead, headroom, memory and verification work. Qualify control progress under data saturation rather than claiming configured ceilings are measured capacity.
- [ ] Verify client relay selection and real direct/relay fallback. A configured standalone Iroh relay does not automatically select that relay in each client.
- [ ] Finish stopped atomic encrypted backup/restore and identity continuity. A deployment needs a named external target; none has been provided, so no operating public server is claimed.

### Native UI work

Existing Requests/Threads and service settings show permission-aware actions, selected copies, authorization waiting, receipts and limits. Remaining UI must accompany expired resend, restore recovery, admission/resource controls and broader delivery health. Background work must remain off the UI path.

### Qualification still required

- [ ] Run fault injection at deposit, local inbox, message/import/session commit and acknowledgement boundaries; combine response loss, client/host restart, failover and changed batches. Separate process-crash tests from power-loss/storage assumptions.
- [ ] Exercise disk full, database busy, physical storage overhead, concurrent quotas, maximum legal frames and restore. Existing unit/integration regressions do not replace this campaign.
- [ ] Test permissions changing during retries, a revoked device presenting an old roster to a stale host, key rotation, new-device history and restore without widened access.
- [ ] Test source floods, many identities, stranger-partition exhaustion, worker/connection saturation and control-message progress; measure memory, CPU, disk and traffic ceilings.
- [ ] Audit every carrier/private replication/blob path and verify another operator cannot decrypt, rewrite valid private envelopes or forge recipient receipts. Reproduce the full offline two-person/two-host flow below.

## Starting limits to implement or verify in the new social paths

Decision references: D65–D67. These are configurable design bounds, not measured capacity claims. Existing mail retention and limits are reused where appropriate; new protocols and UI must advertise and enforce them.

| Area | Agreed starting bound |
| --- | --- |
| Snippet | Title 160 / text 1,200 Unicode characters; complete signed document within 6 KiB |
| Public profile / images | 16 KiB text; avatar 256 KiB / banner 512 KiB encoded; decoded at most 2,048 × 2,048 / 4,096 × 2,048, plus decoder work/memory limits |
| Introduction | 2 KiB UTF-8 text; 32 KiB envelope; 32 pending / 1 MiB per recipient; separate service-wide stranger ceiling |
| Provisional exchange / chat | Three replies per side within seven days; ongoing chat text 16 KiB; attachments separately requested |
| Social reply/frame | 50 items and 256 KiB maximum, paginate; envelope limits reserve all framing overhead; larger existing sync traffic has separate bounds |
| Public freshness | Seven-day revision lifetime; five-minute future-date allowance; forwarding never renews; withdrawals suppress every older permitted revision through expiry plus allowance |
| Retention / copies | Advertise existing 30-day mail retention; two independent selected hosts where available, visible single-copy operation otherwise |
| Storage / cache | 64 MiB trusted mailbox, separate 1 MiB stranger partition, control reserve and global capacity; personal cache 10,000 disk entries with separate bounded memory/mobile profile |
| Gossip / ask-around | Three eligible gossip peers per revision while ledger retained; query 30 seconds, three onward peers, four active local queries, 50 results / 256 KiB accumulated locally and explicit aggregate work limits |
| Modest-server measurement defaults | 64 global / eight per-peer connections; 1 GiB service storage; 4 MiB/min incoming and 4 MiB/min outgoing globally, stricter source limits and bounded pending handshakes |
| Local metadata / disclosure | Bounded private search history and receipts with clearing; explicit publication/query destinations; unknown contacts get no automatic trust/proximity grants |

Validate impossible/negative configurations and legal envelope/reply combinations. Persist budget state that must survive restart. Account for input, verification and output independently; generating another key does not reset a Cell-wide ceiling. Text/media fetching remains deliberate, hash-verified and bounded.

## Sequential implementation order

Feature numbers remain the approved scope; the steps below follow dependencies. UI and checks accompany the backend at each step. Do not release an advertised contact flow with unfinished reply/delivery controls.

Reuse the existing Cell startup and hourly `renew_local_roster` path for Main membership renewal; social seven-day leases are a separate authorization layer. A trusted personal authority server uses those existing paths. Independent service hosts receive neither the user's Main identity key nor its pseudonymous authority wallet.

| Step | Remaining work | Exit evidence |
| --- | --- | --- |
| 1 | Finish enrolled profile/media, long-missed host recovery and scoped sharing audit | Actual enrollment/offline/concurrent/removal tests, same identity/history and no private export |
| 2 | Complete expired resend, pending key/permission changes and inactive cleanup | Atomic renewal, fresh authority, stable logical history, terminal denial and restart evidence |
| 3 | Complete source disagreement/freshness and discovery churn | Authenticated source merging, visible conflicts/failures, withdrawal progress and opt-in isolation |
| 4 | Finish delivery/admission/resource health and operator controls | Useful bounded native views, private-content-free counters and restart behavior |
| 5 | Implement stopped encrypted owner-only restore and recovery | Wrong/corrupt/oversized backup rejection, fresh keys, re-enrollment and current floors without permission resurrection |
| 6 | Qualify Iroh relay selection and direct/relay/mailbox fallback | Deliberate client configuration and actual connection/failure tests; no extra application forwarder |
| 7 | Run process/storage/security faults and declared resource measurements | Reproducible crash/busy/full/quota/abuse cases, reference machine and honest physical limits |
| 8 | Reconcile end-to-end evidence and slim completed subjects | All approved flows qualified, concrete limitations and an external target only if supplied |

Add server supervision/resource controls as their roles arrive; step 8 completes operational qualification. Package work follows the human's 1–6 order, with necessary shared prerequisites introduced when required. A step closes only with its native UI where needed and relevant correctness/security/performance evidence. Keep the active checkpoint above current across compaction. When a complete subject passes its evidence, replace its checklist here with a short description and implementation/test links.

## End-to-end completion and measurements

Decision references: D61–D64, D68–D69. One social entry point provides Browse, Search, My posts and Requests; profile editing belongs in the active Organ view, private conversation in existing threads, operator controls in settings. Include keyboard/accessibility checks. New embedded Sand dependencies require their licenses and credits.

- [ ] Two previously unknown people demonstrate both anonymous and identified posts; select services and find bicycle help outside their contact network. Preview, public identity choice, source, freshness and query disclosure are understandable.
- [ ] An encrypted introduction waits while its recipient is offline. Both exchange private pseudonymous replies, accept continued conversation and optionally reveal chosen profiles. Only mutual consent converts them to contacts; discovery grants no private data.
- [ ] Fail one independently selected host, lose a deposit response and reconnect devices. Retry produces one logical message and once-only Facts, with truthful carrier/recipient/failure status and usable local UI.
- [ ] Edit a profile offline on one device. The same Organ identity/profile converges across authorized devices and selected hosts; conflicts remain visible; anonymous posts stay unlinked.
- [ ] Enroll another authorized device and recover retained Conversation/Thread/Message UIDs and private participant state, with separately provisioned sessions. Profile reveal preserves the same history.
- [ ] Withdraw a post and revoke a device. Current discovery and future authorized delivery reflect those changes; stale/offline hosts have visible bounded freshness rather than a promise of instant unseen revocation. Test restore without resurrected permissions.
- [ ] Show optional deliberate Transfer creation without discovery itself changing stock or creating an agreement. Public services/carriers see only permitted public data, routing/admission metadata and ciphertext.
- [ ] Benchmark declared small-device and modest-server profiles: 10,000 cached snippets, 100,000 directory listings and simulated 500-Cell churn/abuse. Target p95 local search below 250 ms on the named reference machine; measure memory, worker, storage, verification and traffic limits before claiming capacity.

Use deterministic simulations and real network/storage tests where assumptions require them. Run relevant changed-target tests and `cargo check`, with warnings as errors. Full-disk, power-loss assumptions, backup restore, key/permission changes and real multi-device flows must be tested before calling the social system complete.
