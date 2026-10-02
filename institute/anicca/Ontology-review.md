# Ontology: social features 1–6 and remaining qualification

Updated 2026-10-01. Ontology connects Record meanings, Organ identity, synchronization and discovery. The six approved feature packages have backend and native interface implementations. Keep development simple, reuse existing Record synchronization, and finish qualification before claiming the complete social system is ready. Item 7 remains excluded.

## Scope and decisions to preserve

- Each Need or Contribution can be anonymous or identified, chosen per post. Anonymous posting authority is separate from the Main Organ identity; deliberate alias reuse can link posts. Public anonymity hides that association, while operators can still observe connection metadata and timing.
- Each Organ has one stable public identity and one current profile across authorized devices and selected hosts. UI devices provide profile controls. Offline drafts survive; concurrent edits require review. Updating the profile leaves anonymous post identities and conversation UIDs intact.
- The owner device authorizes fresh per-device messaging keys and anonymous editing keys through existing Own sync. Permissions last seven days; hosts enforce newer known revocations immediately. A newly enrolled device receives retained history before fresh sending authority; sending waits if the owner is unavailable.
- Conversation → Thread → Message Records retain their UIDs through replies, enrollment, Reveal and Connect. Own history sync carries retained history and private mappings; live encryption accounts and ratchets stay separate on each device.
- Accepting a conversation, revealing a profile and becoming contacts are separate decisions. Only mutual Connect creates a known contact. It creates no general feed, trust expansion or private replica grant. Blocks remain effective after deleting or archiving a conversation.
- Directory, townsquare, mailbox, publication, query, gossip and private sharing choices are independent. No host is selected automatically. Gossip and scheduled searches default off; saved filters sync, but each Cell separately opts into searching and existing in-app notifications. Quiet hours and deduplication apply.
- Iroh relays carry live connections. Lince mailboxes retain encrypted messages while recipients are offline. These packages add neither a Lince server-to-server forwarding protocol nor closed-app system push notifications.
- Dedicated backup/restore development is removed from this scope. The requested backup is a copy or zip of the complete Lince data directory with Lince stopped. Copying a directory does not enroll another device. Existing optional encrypted export code remains, without further backup/restore work.

Excluded item 7: vocabulary publishing/import, expanded Ontology/Relations exploration, general file-sync closure, Trail packages, calendar export, Blood integrations and LoRa. Group social features, public comments, engagement ranking and stronger network-origin anonymity are also outside this increment.

## How the network works

Example: publish “I can repair bicycles this weekend.” Someone outside your contacts finds it and sends an encrypted introduction while your phone is offline. A selected mailbox stores it; your phone collects it later. You can reply under a pseudonym, then separately choose whether to reveal profiles and connect.

| Piece | Simple explanation | Boundary |
| --- | --- | --- |
| Cell / Organ | A running installation / the identity shared by authorized installations | Device membership and each actor's permissions both matter |
| Directory / townsquare | Searchable public listings / browsing those listings | Public documents only; no private conversation history |
| Gossip | Willing contacts pass signed public announcements onward | Separate send/receive consent, bounded fan-out, expiry and withdrawal |
| Ask-around | Ask selected contacts to search and optionally ask onward | Contacts see the question; original contact pins, consent and deadlines are checked |
| Iroh relay | Helps endpoints connect and carries live traffic when needed | Connection infrastructure; it is not an offline inbox |
| Lince mailbox | Stores admitted encrypted envelopes for later pickup | Selected hosts, authorization, quotas, retention and authenticated receipts |
| Session / ratchet | Private encryption state that changes as messages are processed | Per-device state; rejected imports must not advance it |
| Receipt | Evidence of a delivery stage | Host storage and recipient durable import are different stages |

Always-online hosts receive public documents or scoped ciphertext, not Main identity secrets or pseudonymous owner wallets. A mailbox does not renew seven-day authority. Already admitted accepted-chat envelopes retain their thirty-day delivery window; fresh pickup authority and known revocations still apply. Gossip never extends a post's reviewed lifetime. Losing all retained copies or required keys is not recoverable by adding another relay.

## Implemented packages

| Package | Completed behavior | Main implementation and evidence |
| --- | --- | --- |
| 1. Needs and Contributions | Anonymous/identified choice, exact signed preview, source privacy review, separate editing authority, drafts, edit/renew/pause/fulfill/withdraw/archive, durable selected-host publication and visible per-host results | [publication](../../crates/engine/src/social/publication.rs), [native Discovery](../../crates/desktop/src/organ_castle/social.rs), [publication regressions](../../crates/engine/tests/social_publication.rs) |
| 2. Shared public profile | Stable Organ identity, delegated edits, current authority floors, trusted root succession, offline/conflict drafts, explicit normalized images and unchanged anonymous identities | [profiles](../../crates/engine/src/social/profile.rs), [drafts](../../crates/engine/src/social/profile_draft.rs), [multi-device regressions](../../crates/engine/tests/social_publication/profile_devices.rs) |
| 3. Retained conversation history | Existing Own synchronization, stable Conversation/Thread/Message UIDs, newly enrolled history, atomic session/Record/event/receipt import, private mapping isolation and separate session provisioning | [history](../../crates/engine/src/social/history.rs), [receive](../../crates/engine/src/social/conversation.rs), [history regression](../../crates/engine/tests/social_history.rs) |
| 4. Requests and contacts | Offline introductions, provisional replies, acceptance/decline/block/close, Reveal, mutual Connect, existing Thread composition and retained history | [sending](../../crates/engine/src/social/outbound.rs), [Reveal/Connect](../../crates/engine/src/social/reveal.rs), [two-person and enrollment scenarios](../../crates/engine/tests/social_request_flow.rs) |
| 5. Discovery | Selectable role-specific services, signed FTS5 cache, search/browse/paging, source disagreement review, gossip, contact queries, saved searches, mute/removal/reports and native controls | [service/cache](../../crates/engine/src/social/service.rs), [gossip](../../crates/engine/src/social/gossip.rs), [queries](../../crates/engine/src/social/ask.rs), [saved searches](../../crates/engine/src/social/subscriptions.rs) |
| 6. Services and asynchronous delivery | Independent preparation/send/pickup workers, exact ciphertext retries, selected mailbox copies, truthful receipts, revocation/key replacement, Discard, health, retention, headless hosting, supervision and client relay selection | [workers](../../crates/engine/src/social/delivery_worker.rs), [mailbox](../../crates/engine/src/social/mailbox.rs), [Cell host](../../crates/cell/src/social_host.rs), [relay settings](../../crates/engine/src/wire/relays.rs) |

Completed details remain in code and tests rather than repeated implementation checklists. Discovery never creates a Transfer, changes stock or expands trust. Explicit later sharing has its own contract; retained plaintext on another person's device cannot be recalled.

### Security closure in this increment

Social authoring writes retain the original actor and exact command permission, and recheck that principal, Login and current local device authority inside the final write transaction. The authorization scope belongs to its originating Engine; simulated remote hosts cannot inherit it. Hosting and saved-search settings use the same guarded transaction. Profile signing-key preparation checks the retained private state and current authorized editors again.

Contact queries and gossip recheck the original Organ/endpoint pair, direction consent, current block/trust state and applicable deadline before reserving outgoing work. Budget reservation and these checks share a transaction. Final query/reply-cache commits recheck current authority and consent; changed pins cannot transfer an old question's permission to another contact. Cancellation prevents late private results from being saved.

Outgoing conversation writes recheck owner-wide blocks and the current participant snapshot. Incoming encrypted content checks current device authority, sender control, terminal participant decisions and owner-wide blocks within the transaction that changes encryption state, Records, event intent and recipient receipts. A rejected new message rolls those writes back. Authenticated mailbox requests still advance their separate anti-replay pickup counter; the fault fixtures compare protected account/ratchet state separately from it. Authentic late host acknowledgements may remain local evidence without changing shared history. Reconstructing a revealed contact also rechecks current participant state, deletion, retained block decisions and device authority before creating the contact.

Evidence: [controlled final-write pauses](../../crates/engine/src/social/authority/tests.rs), [late device-permission changes](../../crates/engine/tests/social_request_flow/late_delivery/permission_races.rs), [block/close during collection](../../crates/engine/tests/social_request_flow/late_delivery/consent_races.rs), [subscription races](../../crates/engine/tests/social_subscriptions.rs). Use actual role, signed membership and consent changes; do not substitute a mock boolean for authorization.

Reset deletes live account/session state and holds pending sends in one SQLite transaction. The persistent local storage wrapping key and separate owner wallet remain; fresh messaging keys change. Rollback keeps previous accounts decryptable. Reply-key and session regressions qualify this contract.

### Native Thread crash closure

The large shared Action dispatcher overflowed the normal test-thread stack while creating the Agent Record used by both native regressions. Heap allocation alone did not fix polling. [Small internal dispatch handlers](../../crates/engine/src/actions/dispatch.rs) preserve action routing and early-return behavior while reducing each polling frame. Native Enter-to-send/numbered tabs and people/agent mentions now pass on the normal stack. Keep authorization, ordinary thread and social regressions alongside this change.

### Storage, interrupted delivery and scale

[Storage qualification](../../crates/engine/src/social/qualification.rs) exercises actual SQLite writer locking and allocation exhaustion through public intake, verifies no partial document/authority write, and retries the same signed document after recovery. These are real SQLite failures, not a claim about OS ENOSPC or power loss.

The same fixture uses 512 distinct signed anonymous owners/sources with sixteen concurrent requests. With a deliberately small 100-entry/1-MiB configuration, twenty posts were admitted; authority/control reservations consume capacity before the entry ceiling. Excess data is refused, valid withdrawal still progresses, stale replay receives refusal and search cannot resurrect the post. The final run allocated 4,734,976 database bytes and 4,152,992 WAL bytes in 12.04 seconds. Database allocation includes Lince's schema; payload quota is not a physical file-size ceiling. This is not 512 physical connections or a production throughput claim.

[Lost-response and cancelled-worker cases](../../crates/engine/tests/social_request_flow/late_delivery/faults.rs) combine one offline host, persistent client reopen and normal workers. They qualify unchanged retry ciphertext and once-only Message, quantity and event. [Receiver process interruption](../../crates/engine/tests/social_request_flow/late_delivery/process_faults.rs) kills a separate receiver after local import commits and before host acknowledgement, then reopens it and exercises retry. An actual process kill is distinct from task cancellation and from power-loss testing.

Retained [query-only baseline](Ontology-search-baseline.json): disk-backed 10,000-entry local search p95 109.52 ms; 100,000 directory entries p95 663.69 ms, 401,502,208 database bytes, 399,953,152 WAL bytes and peak RSS 120,324 KiB. Bulk loading and one anonymous owner isolate search behavior; they do not qualify admission throughput, many-owner ledgers, connection churn or production capacity. The 100,000-entry result exceeds the proposed 250-ms target and must not be described as meeting it.

## Remaining work, in order

The current security, native crash fixes and fault-test increment is qualified by the checkpoint below. These remaining campaigns are needed before declaring the whole social system complete.

1. **Combine real transport and mailbox faults.** Existing real Iroh/QUIC gossip/query tests and direct/relay selection tests are qualified separately. Exercise delayed delivery while a real relay/connection fails, reopen a persistent host, and recover through the other independently selected mailbox. Verify storage vs recipient receipt stages, stable ciphertext/Message identity and visible single-copy operation. Simulation should control loss/timing; retain actual transport authentication checks.
2. **Qualify remaining message/key failure schedules.** Existing fresh-device history, trusted root succession, old-root revocation, pending/stored key replacement, session-reset rollback and Discard/reopen cases pass. Extend interruption points to sender preparation/session commit and host commit/acknowledgement, simultaneous initiation and bounded out-of-order/restart behavior. Verify denied work creates no ratchet advance, event, receipt or private grant. Explicitly label which process boundaries are covered; add no restore protocol.
3. **Measure physical resource tiers and abuse.** Run a many-owner workload beyond the query-only baseline on named modest-server and small-device profiles. Measure verification cost, memory, disk/index/WAL, traffic, workers, handshakes and connection churn. Combine source/request floods and stranger-partition exhaustion with withdrawal/revocation/receipt progress. Existing quotas and control reserves are implemented; configured limits are not measured capacity. Keep counters global so new identities cannot reset a Cell ceiling.
4. **Finish declared targets and complete native journey.** Check protocol/version refusal for each new public verb and maximum legal frames on supported targets. Exercise keyboard/accessibility, offline profile edits, conflict review, publication/search, stranger introduction, acceptance, Reveal/Connect, enrollment/history, withdrawal/revocation and retained block/error/waiting states through the native entry points. Automated backend and native tests support this review; they do not replace target-specific/manual qualification.

External deployment needs a named target; none is provided. No operating public host or deployed relay availability is claimed. Physical power-loss guarantees also require a storage model and environment; deterministic mocks cannot establish them. These dependencies do not reintroduce backup/restore work or block local implementation checks.

## Bounds and acceptance contracts

Keep Rust, existing Iroh/Wire, SQLx/SQLite/FTS5 and the native Rust interface. Olm v1 uses pinned vodozemac with default features disabled. Reuse signed Own membership, Record extensions/Fact paths and existing worker supervision; do not add another private history database or copy live ratchets through Own history sync.

| Area | Current agreed bound / behavior |
| --- | --- |
| Public snippet | Title 160 / text 1,200 Unicode characters; full signed document 6 KiB; seven-day lifetime and five-minute future tolerance |
| Public profile/images | 16 KiB text; two prepared 128-KiB images; input 4 MiB and 2,048×2,048 / four million pixels; normalized within 1,024×512; decoder 64 MiB, two jobs, three-second deadline |
| Introduction / provisional replies | 2 KiB UTF-8 introduction; 32 pending / 1 MiB recipient partition; three provisional texts per side within seven days; separate stranger/control/trusted capacity |
| Accepted private content | Chat text 16 KiB, full private content 20 KiB, envelope 32 KiB; thirty-day accepted delivery; attachments separately requested |
| Frames / pages | Fifty results, 256-KiB complete public frame including wrappers; existing Own history frames have separately enforced aggregate limits |
| Gossip | Separate default-off Cell/contact send/receive choices; three assigned eligible peers per retained revision; durable forwarding ledger, ending floors, retries and traffic accounting |
| Ask-around | Thirty seconds; twelve total work credits, three children, fifty results/192 KiB; four active/twenty retained local queries; current pins/consent/cancellation |
| Saved searches | Sixteen filters/eight active; hourly minimum; one leased thirty-second job; 256 dedup entries/filter and 4,096/Cell; separate notification opt-in, 22:00–08:00 quiet hours |
| Source evidence | Eight immediate sources/post, 8,192 observations; 256 signed variant pairs/eight MiB; same-authority/generation/revision contradiction hides until a newer signed revision/withdrawal even when evidence capacity is full |
| Service choices / relays | Sixteen remembered endpoints, eight selections per role; eight canonical root HTTPS relay URLs/1,024 bytes; empty uses preset, Local disables relays |
| Moderation / reports | 256 Own hides/tombstones/64 KiB with paged overflow review; 32 local report jobs, 256 host reports/four MiB/seven days, eight/source and 256 globally per UTC day; deliberate report sends only reviewed public evidence |
| Storage / server design defaults | Trusted mailbox 64 MiB, separate stranger 1 MiB and control/global reserve; personal disk cache 10,000; modest-server profile 1 GiB, 64 global/eight per-peer connections, four MiB/min each direction with stricter source budgets |

Validate impossible configurations, retain non-resettable budget/floor state through restart, and account for framing and retained authority overhead. Reject malformed signatures and unrelated controls before import; never accept a carrier's claim as an authenticated recipient receipt. Fetch public images and attachments only deliberately within their own permissions and bounds.

## Evidence checkpoint

Current increment: 219 passing tests, with no unresolved test failure. Backend integration: 153 cases across eighteen suites (107 in `/tmp/lince-social-scope-regressions-v1.log`, eight in `v2`, thirty-eight in `v3`; each earlier failed target was corrected and rerun). Final social units: eighteen passed and the explicit search benchmark ignored (`/tmp/lince-social-security-resource-final.log`). Fresh native Organ/social: thirty-one passed (`/tmp/lince-social-native-organ-final.log`); fresh native Thread: seventeen passed and the authenticated live ACP-agent case ignored (`/tmp/lince-social-native-thread-final.log`). Both previous native stack failures pass on the normal stack. Final Engine/Desktop/Cell/application `cargo check` passes with warnings denied, including tests, the application binary and optional facade (`/tmp/lince-social-scope-final-check.log`). Command: `cargo check -j2 -p engine -p lince-desktop -p lince-cell -p lince --features lince/facade --lib --bin lince --tests`, using the interface Nix shell and `RUSTFLAGS="-C link-arg=-fuse-ld=mold -C target-cpu=native"`. No check was skipped because of concurrent Karma changes.

Additional retained gates cover Cell shutdown/relay restart and direct/relay selection, carrier/blob boundaries, cleanup and the recorded disk-backed search baseline. They were not all rerun in this increment. The explicit search benchmark and authenticated live ACP-agent case have separate opt-in requirements. Broader transport/storage/target campaigns remain listed above; local checks do not claim public deployment or physical power-loss durability.

Run checks sequentially on the current branch, preserve concurrent Karma/UI changes, use `cargo check` rather than `cargo build`, treat warnings as errors, and add no code comments. Do not edit the owner's .lingua files, AGENTS.md, README.md or the unplugged web crate. Keep completed subjects short here and put new unresolved failure details in the relevant remaining step.
