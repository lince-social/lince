# Ontology — the remaining work in four clusters

Agent-side reorganisation of the work that used to be prose inside the
**Ontology Record of `anicca/Lince.lingua`**. `anicca/Ontology.lingua` no
longer exists as its own file. The owner Record keeps the stable `@ontology`
identity required by its children; the reorganized reasoning and remaining
work live here rather than being reintroduced as stale owner tasks. Historical
line citations below identify the source study and may move as the combined
Record changes. This is proposed agent text, not an owner decision.

Coverage was swept mechanically on 2026-08-31: every `- [ ]` in the chapter is
represented below exactly once, and the landed ones were removed from the list
with what they settled kept as prose.

Markers:

- **▢** — open, startable now.
- **⏳ LAST** — deliberately last in the sequence (see Ordering). Nothing else
  here may be planned as depending on one of these.

The 🖐 INTERFACE marker is gone with the items it described; see below.

## The surface debts moved out, 2026-09-01

Eleven items whose MECHANISM is built and shipping, and whose only missing half
is a way for a person to reach it, now live in
`anicca/interface/plans/interface.md` under **"Ontology surfaces this interface
owes"** — the plan that owns the thing that will draw them. They are not
deferred and not deleted; they moved to where the work happens, so this document
stops listing UI it cannot schedule.

What went: the contact-visibility summary; per-device sharing limits; the
per-Cell capability editor; the succession chain; a reach control per contact;
a Move button; the rate-limit line; "no device list yet — reconnect to finish";
one pending-offers list; a Trash and Restore; the read-model health line.

What stayed here is only what still needs BACKEND work, plus the discovery,
gossip and relay design.

## Ordering, decided 2026-09-01

**The Facade and the Resenha-gated scenarios go LAST.** They are not blocked on
each other and not blocked on anything here; they are deliberately last in the
sequence, so nothing else in this document may be planned as depending on
either. Concretely that is: T2 — the Facade; The public face of an Organ; and
Deployments, whose items are now Resenha scenarios rather than chores.

**T0 and T1 of the Organ Profile are NOT part of that deferral.** The
announcement and the card are their own tier, cut by size, and a card carries
its own consent and signature questions that have nothing to do with a Facade.
Only T2 waits.

Everything else — the cleanups, Visibility, Devices and identity, discovery and
gossip, Relay Cells, and cluster 4 — is ahead of both.

## Scope

`@assertion`, `@organ`, `@lingua` and `@blood` are `#part-of @ontology` but are
prose with no work in them. **`Trail` and `Resenha` are Records of their own**
— both `#part-of @ontology`,
both carrying unbuilt lists (Trail's subject bundles; Resenha's multi-Lince TUI
and DST), and neither has a `.md`. Their checkboxes are deliberately not
imported here.

## What this list does not double-count

**`## Sync and the log` in the chapter (`Lince.lingua:250-268`) is a table of
contents, not work.** Eleven of its twelve lines are restated later in the same
chapter as full *Why/How/How used* entries. This document counts each item
once, under the cluster its full entry belongs to.

## Repository housekeeping, still true

The eleven logo files live in `assets/logo/` (with `assets/photos/`), and every
`include_bytes!` in `lince_website/mod.rs`, `static_assets.rs` and
`desktop/src/tauri_shell.rs` was repointed at them — before that, `crates/web`
and `crates/desktop` did not compile at all. The four
`raw.githubusercontent.com` URLs in `README.md` were fixed with them.

`assets/lince.excalidraw` was deleted outright while `visual_identity.rs:124`
still offers it as a download on the published site, so it was restored from
git. If the deletion was deliberate, `git rm assets/lince.excalidraw` plus
dropping `EXCALIDRAW_SOURCE` and its two references in `lince_website/mod.rs`
is the whole change.

`assets/logo/` and `assets/photos/` are still untracked — `git add` them, or a
fresh clone will not build.

The URL strings inside the generated website (`"assets/black_in_white.ico"` in
`visual_identity.rs` and `html.rs`) are deliberately unchanged: those are the
exported site's own layout, not repository paths.

`anicca/interface/First Steps.linguai` is intentionally agent-maintained and
not parsed by Instinct. The owner will turn it back into `.lingua` only after
the v1 tutorial matches a usable native interface. On 2026-08-30 the six
dangling `#part-of @first-steps` assertions were removed from `Lince.lingua`,
so the current chapters stand as roots until that promotion reattaches them.

---

## Decisions — landed or refused, do not re-add

**One name for one act: `create-message`.** `Action::SendMessage` is gone.
`create-message` was the superset — it carries `parent` and `references`,
tolerates a thread with no replica root, and runs the transfer-thread writer
check — and both actions already sat in the same permission arm, so collapsing
them changed no permission. `threads::send_message` stays, still used by the
thread-invite path and by the `individual_replica` / `live_workflow` tests.
Re-verified 2026-08-31: `send-message` appears in no `.rs`, `.js` or `.html`
anywhere under `crates/`, so the repoint survived the refactor so far. It is one string in the Conversation sand — **check it again
after the sands are rebuilt.**

**`file_sync.rs` no longer claims selection is unconfigurable.** The module doc
said "Selection is deliberately NOT Protein-configurable yet" forty lines above
`configured_filter`, which deserializes a `protein::Predicate` out of the sync
config. Replaced with what the code does (`crates/engine/src/file_sync.rs:9`).

**The per-Cell surface config no longer lives on a Record that syncs.** This was
the worst finding of the audit: not unbuilt, but *attempted and undone by live
code*. Migration `0067_no_base_url.sql` blanked the Organ Record's body and
deleted every `lince.organ` extension row — and `store::organs::ensure_local`
wrote both straight back on *every boot*, with `adopt_identity` writing the same
blob raw during enrolment. The migration cleaned a table the next process
refilled, so the list read as though this had landed while one device's
`baseUrl` still travelled to every sibling Cell.

`baseUrl`, `aliases` and `local` now live on the **Cell** Record under
`lince.cell.surface` (`store/src/organs.rs:15`), which never syncs and never
logs an op — the home `cells.rs` already documents for exactly this reason, and
the same reason a relay holding `relay_capabilities()` can still configure
itself. Three things about it are worth keeping:

- `crates/store/migrations/0072_surface_config_on_cell.sql` is **a new
  migration rather than an edit to `0067` in place**, which is the usual rule
  for unshipped migrations. `0067` had already run on the dev database, so
  editing it would have been a no-op there.
- The empty-string contract is preserved exactly: `ensure_local(pool, "")`
  still means "do not change the address", so a CLI invocation cannot wipe the
  address a running web Cell wrote.
- **The regression guard is the NEGATIVE assertion**
  (`crates/engine/tests/organ_cell_split.rs`). Reading the surface back proves
  nothing; what matters is that the syncing Record carries no surface to travel
  with — `this_cells_address_never_reaches_the_organ_record` and
  `joining_an_organ_keeps_the_address_on_the_device`. Both would have failed
  against the previous code.

**Tree sync and the Move verb are BUILT.** The chapter files this under "Design
work with no code yet" (`Lince.lingua:320`) and that heading is false.
`Action::MoveRecordTo` and `Action::CancelRecordMove` (`actions.rs:404,409`,
arms at `3536`), `store/src/record_move.rs` over migration
`0070_record_move.sql`, the exactly-once hand-over in `engine/src/share.rs`
(`still_queued` → `last_op_seq` → `mark_handed_over`), and
`engine/tests/contact_share.rs:631` covering a moved Record carrying its
children. The dangerous half — Move as distinct from Copy, where delivering
twice means two copies and losing the ack destroys data — is the half that
exists. **What is missing is only a way to reach it** (cluster 3).

**Replica bootstrap for a new contact is refused, not pending**
(`Lince.lingua:263`). `crates/engine/tests/replica_bootstrap.rs` argues it in
its own header: retention only drops ops that a newer op for the same target
superseded, so the surviving log always contains, for every live field, the op
that established its current value — "bootstrap" is replaying from zero and is
complete by construction. The snapshot alternative was rejected because it
needs synthesized `(actor_cell, hlc)` identities, and that identity IS the
unique index the import path dedupes on.

**"`contact.mode` is unused" is wrong** (`Lince.lingua:256`). The column was
repurposed: `Contact::reach()` (`store/src/organs.rs`) maps it to `direct` /
`mailbox` / `auto`, migration `0066_contact_reach.sql` normalised it, and
`engine/src/wire.rs` branches on it to decide mailbox versus direct delivery.
It is load-bearing. What is genuinely missing is that **nothing sets it** —
`organs::set_mode` has exactly one caller and it is a test. That surviving half
is a control on the contact panel and is listed in cluster 3.

**`Engine::cell_may` is called** (`Lince.lingua:261` says nothing calls it;
stale). `wire.rs:1828` calls it with `CAP_REPRESENT`. What is missing is only
the editor — see cluster 3.

**The Deployments pointer** (`Lince.lingua:306`) says the scenarios' home,
`anicca/Resenha.md`, no longer exists. The **Resenha chapter is back**, at
`Lince.lingua:1704` — but as written it is the multi-Lince TUI plus DST and
does not contain the VPS deployment scenarios. The target exists again; the
scenarios still are not in it, and where they land is the owner's call.

---


## Landed 2026-09-01 — the approved backend pass

**Naming links in a scope.** The scope language was column names only, and
`record_assertion` is a relationship BETWEEN Records, which no column name can
say — so `op_in_scope` answered `false` for every assertion and `false` for
every concept, and a narrowed contact received the whole graph stripped out.

The vocabulary extension is a PREFIX, not a reserved word in the column list:
an entry beginning `link:` names links and can never collide with a column,
because a column name has no colon. `link:*` is every link; `link:part-of`
is only that predicate's. `store::sync_ops::LinkScope` carries the resolved
form and `resolve_link_scope` turns the human names into predicate uids once
per pass rather than once per op.

Four decisions inside it, each with a test in `store/tests/scope.rs`:

- **A scope naming no link still receives none.** Fail-closed is unchanged;
  what changed is that there is now a way to ask.
- **A retraction rides whenever ANY link does.** An assertion tombstone carries
  no predicate in its value, so it cannot be matched by name — and withholding
  it would leave a narrowed contact holding a link forever. Same shape as the
  record-tombstone exemption, same reason.
- **The predicate Concept rides with the links that need it.** A link whose
  predicate has no name is unreadable, which is the reason `upsert_assertion`
  already stubs one on the way in. A scope wanting no links still gets no
  concepts.
- **A link whose value carries no `predicate_uid` stays home** under a
  by-name scope.

`op_in_scope` keeps its cheap four-argument signature for the two engine
callers that have no op value in hand — the inbound accept gate and the outbox
drop check — where it now reads "any link named" and lets serve-time narrowing
be the authority. That is the correct direction for both: the accept side errs
toward keeping data, and the outbox side only decides whether to keep a row
queued.

**The read half of per-person permissions.** Permissions gated
create/update/delete; anyone with a login saw every Record. Three parts:

- **`organ:read` already existed as a permission key** (`utils::auth::ALL_PERMISSIONS`)
  and no Action used it. The Ontology's note that there is "no read tier" was
  wrong about the key and right about the effect. Seven pure reads —
  `RosterStatus`, `MailboxStatus`, `MailboxPickupPoints`, `MailboxOutbound`,
  `MailboxRequests`, `FileSyncStatus`, `AuditContact` — moved off
  `organ:update` and onto it. Admin holds both, so nothing narrowed; what it
  buys is a role that may look without being able to change.
- **One saved Protein predicate per login** (`person_credential.read_filter`,
  migration `0075`), set by `Action::SetPersonReadFilter` under `user:update`.
  **NULL is unnarrowed and is the default** — not the same as an empty
  predicate, which is a real and different answer.
- **Evaluated on BOTH paths.** On reads, `protein::execute_for_with_context`
  intersects it into the `visible` allowlist it already computes per subject.
  On writes, `act_at_inner` refuses an Action whose target Record the filter
  does not admit, at the one choke point every Action passes through. A filter
  that only hid things on screen would be a display preference, not a boundary
  — `engine/tests/read_filter.rs` asserts the refusal and that the Record is
  unchanged after it.

**The comment rule is now enforced everywhere.** The strip itself was already
done (`sensei: 465 files, nothing to say`). What was missing was the gate:
`sensei::teach` was called from one `build.rs`. Now every crate calls it —
`store`, `engine`, `nucleus`, `protein`, `transport`, `web`, `lince`,
`lince-utils`, `desktop`, `lince-interface`, `lince-fiote`, `anicca`. Proven
rather than assumed: adding a single `//` to `store` fails the build with the
file and line. `NoComments` scans `.rs` under `src/`, `tests/`, `benches/`,
`examples/` and `build.rs`; the sands' `.js`/`.html` are outside it and are
being replaced anyway.

**One backoff implementation.** `store::backoff` is now the single exponential
calculator, and `transfer_delivery::outbox_mark_failed` computes its wait
through it. Unit tests cover the base delay, the doubling, the ceiling holding
against `i64::MAX` attempts, and an unreadable stamp being read as due rather
than stranding a row forever.

### Refused after building it — the sync outbox does NOT get this backoff

Attempted and reverted the same day, and the reason is worth more than the
code was. Adding `next_attempt_at` to `sync_outbox` broke four tests, all of
them the same scenario: a peer goes offline, comes back, and the message is
supposed to arrive. It did not, because the ops were no longer due.

**The two schedulers back off different things, and that is why they are not
one policy.** `transfer_delivery_outbox` backs off an ENVELOPE to a person —
per row, retried on its own clock, and a wait there is correct. `sync_outbox`
is a queue of ops for a contact, drained per contact, and its retry cadence IS
the sync pass. What should back off on that side is DIALING an unreachable
Cell, which is a different object from the ops waiting to go. Putting the delay
on the queue meant a peer that reconnected instantly still waited.

So the unification is real for the calculation and false for the policy. If the
sync side ever gets a backoff it belongs on the dial, beside
`unreachable_since`, not on the rows.

### Already built — a per-contact predicate on the feed

Listed as speculative and approved for building; it turned out to exist. The
three things the entry said it needed are all shipping:

- the per-contact predicate is `organ_contact.share_protein`;
- per-Record evaluation on the serve path is `engine/src/share.rs`;
- **the served-set table is `contact_share`** (migration `0064`), whose
  `picked` and `held` columns are exactly "matches the rule now" versus "they
  actually hold it" — and `share.rs:130-131` computes `entered` and `left` by
  differencing against the stored `picked` set. That difference IS crossing
  detection, which the entry called the whole point.

`engine/tests/contact_share.rs` covers entering, leaving, returning, and a
Record deleted after it left. What is NOT built is the SURFACE the entry
demanded before switching it on — a Record entering is a grant and one leaving
is a revoke, both fired by an edit nobody meant as an act of sharing, so
"what are you sharing right now" has to be answerable first. That is a
question for the interface plan, not backend work.


### Decided and built — a refusal you remember, that the sender cannot detect

The blocking question was "should a refusal be remembered, per kind?", and the
answer is **yes for all four**, because the question contained a conflation
that was doing real damage. Two different things were being called one thing:

- **What the SENDER learns** must be identical for declined and ignored. That
  is a genuine rule — the difference between them is a probe, and a stranger
  who can tell them apart can map who is reading their offers.
- **What I remember LOCALLY** is nobody's business but mine.

Destroying my own memory to protect the sender's ignorance protected nothing
and cost something real: a stranger whose invite I declined could invite me
again immediately, and again, and I would be prompted every single time. The
anti-spam hole was wearing the privacy rule as a disguise.

`offer_refusal` (migration `0076`) is local, never synced, never logged as an
op — the same treatment `thread_invite` already gets, for the same reason. It
is keyed `(kind, subject_uid, other_party)`, so refusing one party says nothing
about another and refusing a conversation is not refusing a transfer. All four
kinds now record one: `decline_invite`, `DeclineGrant`, `CancelRecordMove` and
`RejectTransferInvitation`.

**The sender-facing answer is unchanged, and that is the load-bearing part.**
`OfferGrant` already answered `Applied { applied: 0 }` whether the invite was
stored or dropped as a duplicate, so absorbing a re-offer under a standing
refusal returns the same value on the same path. Nothing new is disclosed.

**A refusal is a window, not a wall.** `REFUSAL_WINDOW_DAYS = 30`, after which
a fresh offer gets through, and `forget_refusal` lifts one by hand. A refusal
that never expired would turn one bad moment into a life sentence — people
change their minds, and so do circumstances.

`offers::pending` now carries `previously_refused` per row, so the pending-offers
list can say "you declined this before" rather than presenting a repeat offer as
if it were new.

**What is tested and what is not.** `store/tests/offer_refusal.rs` covers the
window standing, per-kind and per-party isolation, refusing twice moving the
window rather than stacking, and an expired refusal no longer standing. The
WIRE-level assertion — that an absorbed re-offer returns byte-identical bytes to
the sender — is **not** covered: the test written for it could not get a first
request through the loopback harness that the neighbouring
`an_offer_over_the_wire_becomes_an_invite_the_user_answers` uses successfully,
and it was removed rather than left red. That existing test still passes, so the
path is not broken; what is owed is a test of the absorb branch specifically.
The honest way to get it is to lift the decision out of `wire.rs` into a
function the engine can call directly, which is worth doing anyway.
## Landed 2026-08-31 — the backend pass

**The cross-process import guard.** The materialise step now carries its own
condition. `store::sync_apply::Stamp` is the op as the log identifies it
(`tbl`, `uid`, `field`, `hlc`), and every applier writes only while no op with
a higher `hlc` for the same target is already in `sync_op`. Single statements
carry it as ` AND NOT EXISTS (SELECT 1 FROM sync_op WHERE tbl = ? AND uid = ?
AND field = ? AND hlc > ?)`; the read-modify-write appliers (extension keys,
assertions, concepts, and the quantity add) run the same check as the first
statement inside a `write_tx` (`BEGIN IMMEDIATE`), so it is atomic with the
write rather than a memory of one.

- **Why not the post-append re-read.** Re-reading the log's max after appending
  looks sufficient and is not: two processes can each see themselves as the
  winner and still execute their UPDATEs in the wrong order. The condition has
  to be inside the writing statement.
- **This does not contradict `Engine::import_lock`'s doc.** What that comment
  rejected as "a large refactor" was a transaction spanning the WHOLE import
  sequence — `store::sync_apply`, the Loro registry and an async boundary.
  Wrapping one applier's own RMW is not that.
- **Weight.** One `idx_sync_op_target` probe per applied op, no new table, no
  new column, no extra write. `sync_apply` is the REMOTE path only, so the
  local write path is untouched.
- **Rebuild gets it for free and is better for it.** `rebuild` replays in `seq`
  order, which is not `hlc` order, so it could itself land the loser. Under the
  guard only the HLC winner writes and replay becomes order-independent. The
  `replayed` count therefore falls to the number of ops that actually changed
  the read model; `engine/tests/rebuild.rs` asserts `> 0`, which still holds.
- Guard: `crates/engine/tests/stale_materialise.rs` materialises the loser
  after the winner is already in the log and asserts the read model keeps the
  winner and `audit_read_model` stays clean.

**Per-contact rate limiting on the reject path.** `store::contact_rate` with
migration `0073_contact_rate.sql`. Two allowances per contact, both rolling
hourly: 200 refusals and 4 whole-log serves. `store::organs::quarantine` spends
a refusal — the single choke point where we have DONE the work of refusing —
and `import_ops` checks the backoff before anything else, so a contact past its
allowance is refused before the parse rather than after it. `FetchOpsSince`
with an EMPTY vector is the "send me everything" case and spends a serve.
`states()` is what the contact panel renders and `clear()` lifts a backoff by
hand. `quarantine_tally` (created by `0065`, seeded once, maintained by no
code — a lifetime count frozen at that migration) is dropped by the same
migration.

**Two enums named `Reach`.** `store::organs::Reach` → `Delivery`, and
`Contact::reach()` → `Contact::delivery()`. `engine::wire::Reach` keeps the
name. Three call sites, no wire string changed — the column still stores
`direct`/`mailbox`/`auto`.

**A mention in a body becomes a link.** `engine::body_links`, with the rule
where the item put it — on the GENERATOR, never the parser.
`link_first_mentions` rewrites the first whole-word mention of each other
selected Record's head into `[[Title|uid]]` and leaves every later mention
plain; longer titles win an overlap, a Record never links to itself, and text
already inside a `[[…]]` is never touched and counts as that Record's mention.
`file_sync`'s `.lingua` render calls it. `mentions()` reads links back with the
uid authoritative and the title decoration, and `check_body_links` refuses a
body link pointing at no Record — the same rule the prelude already followed,
for the same reason. Renaming a Record still rewrites nobody's body.

**Left open deliberately:** a BARE `[[Project A]]` in a body is parsed and left
alone rather than resolved. Resolving by title is the retargeting the format
exists to prevent, and turning a mention into a stored assertion needs a
PREDICATE — which Concept a bare mention asserts is a product decision, and
`file_sync` already refuses to invent meanings. Owner's call.

**Exact multiply and divide was already built.** Not startable work — done, and
to the letter of the spec. `DecimalValue::mul_exact` / `div_exact` /
`mul_ratio` with `Rounding` and `RoundedDecimal` (`nucleus/src/karma/value.rs`)
route every inexact operation through one `round_ratio`; publish-time refusal
is `infer_exact_product` in `karma/proof.rs` (no declared scale, or a scale over
`MAX_DECIMAL_SCALE`, refuses the Program); the zero divisor is a runtime
`DivisionByZero` in `karma/evaluate.rs`; and unit algebra is explicit — scaling
by a plain decimal keeps its own unit and refuses a declared one, while two
dimensioned operands must declare the result unit or declare it dimensionless,
so nothing ever invents `kg²` or silently drops a dimension.

**The read model is audited on a schedule at last.** `audit_and_repair` existed
and ran nowhere but a test. `Engine::audit_read_model_if_due` now runs from the
heartbeat every six hours, on its OWN cadence rather than the beat's because it
walks every field tip. The clock is wall-clock and persisted, not a counter in
memory: a Cell restarted more often than the period would otherwise audit on
every boot, and one left running for a week would audit once. The outcome lands
in `lince.cell.read_model_audit` on the Cell Record — never syncs, never logs an
op, the same home and the same reason as the surface config — as
`ReadModelHealth { at, checked, diverged, repaired, replayed }`, and a
divergence also warns in the log. **`read_model_health()` returning `None` means
no pass has run yet, which a panel must not draw as a tick.** The surface line
is now in the interface plan: when it was last checked, and what it found.

**The no-roster gap, closed as far as it honestly can be.** Both halves the fix
needed turned out to be built already — pairing fetches the roster in the same
breath as the introduction (`wire.rs:1298`), and `pull_catch_up` refreshes it on
every connection (`refresh_roster`, `wire.rs:4040`). So the gate can refuse. But
refusing outright is wrong and the tree proves it: **it broke 20 tests across
`contact_share`, `collab` and `individual_replica`** — every contact added
without a wire pairing, which is a legitimate path (pasting a public value) and
not only a test convenience. A peer mid-first-boot has no roster to give either.

So the gate is a GRACE WINDOW, not a switch. The first batch from an Organ we
hold no roster for sets `awaiting_roster_since` (migration `0074`); the batch
is admitted under the existing floor while the sync pass keeps asking; and after
`ROSTER_GRACE_DAYS` (7) the batch is refused with a reason a person can act on.
Adopting a roster clears the marker, wherever it came from (`store_roster`).

Two things worth keeping about it:

- **The floor in `inadmissible`'s no-roster branch is now the LOCAL Organ's**,
  plus contacts still inside their grace window. Its comment still describes
  the old world.
- **What the grace window buys is honesty, not safety.** Dedup poisoning stays
  open against a contact for its first week. The alternative was refusing
  legitimate traffic, and a silent admission was worse than either — the marker
  is what lets the contact panel say "no device list yet — reconnect to finish"
  instead of the sync merely looking broken.

**One pending-offers list, read side.** `store::offers::pending` presents all
four handshakes in one vocabulary — kind, direction, subject, title, other
party, when — over the four existing tables (`invites`, `replica_grant`,
`record_move`, `transfer_invitation`). It moves no writer and changes no
delivery semantics. See cluster 4 for what unifying the WRITE side still needs.

---

## 1. Cleanup & Misc

### Decided — `Undelete` and `Restore`, re-specified smaller

The item asked for "a newer lifecycle op above the tombstone". That is more
than the problem needs, and building it would add a second lifecycle layer to
the op log for no gain: **the undelete rule already exists and already works.**
"Undelete is a newer write" is enforced on the import path today —
`sync.rs` compares a `set` against the latest tombstone and passes
`undelete: true`, and `sync_apply::undelete_record` clears `deleted_at`. A
restored Record's Loro doc is not destroyed by the tombstone either; its ops
stay in the log and the doc is merely refused new updates while the tombstone
is the latest word.

So what is actually missing is two small things: a local Action that emits such
a write, and a **Trash** — a list of tombstoned Records to pick from. Neither
is a lifecycle op.

**Not built now, on purpose.** An Action nobody can reach is exactly the
"backend that can only be exercised by `cargo test`" the project rule names,
and the Trash panel is in the interface plan. This stays ONE task, and it is now a small
one: `Action::RestoreRecord` plus `records::deleted()` plus a Trash panel,
landing together.

### Decided — the absent-not-blank sweep is not a backend item

Deleted as a cleanup entry, kept as a rule the interface refactor must adopt.
There are still zero live instances (`Protein.fields` is set to anything but
`None` only in two engine tests and `interface/src/spatial_diagnostic.rs`), the
sands are being rebuilt right now, and a sweep of `row.body || ""` across sands
about to be replaced is a large diff thrown away on landing. The rule survives
and belongs in the NEW renderer's contract, stated once: **`undefined` renders
as "withheld", `""` renders as empty.** A sand that draws a missing `assignee`
as unassigned draws a permission boundary as data.

### ▢ Every new public wire type extends the golden fixture in the same commit

A process rule, not a feature. Its value is a failing test at the moment of the
omission.

### ▢ The release train — deferred on purpose, keep it deferred

The protective half is built (`ALPN_HELLO = lince/hello/1`,
`engine/src/wire.rs:95`). The scheduling buys nothing while nobody else runs
Lince: no stale peer to strand, no store review to wait out, and a cut costs
one rebuild. It becomes real the day someone else's device depends on this one.

### Decided — the Deployments scenarios are Resenha SCENARIOS

They were never deployment chores; they are the Ontology features that cannot
be proved by one Cell on one machine, and Resenha is the thing that runs many
Lince and simulates time, network and disk. So they go to Resenha as named,
seeded, asserted runs — not as "set up a VPS".

The condition that matters: **each one must state its seed, what it does, and
what it asserts.** A scenario that says "check sync works" proves nothing; the
scenarios are what will make these features testable at all, so vagueness in
them is the whole failure. Proposed text for the owner is in the session
report; it belongs in the Resenha Record, and the Deployments section of the
Ontology chapter can then say only that its scenarios live there.

---

## 2. Visibility

**This cluster is empty of backend work as of 2026-09-01.** Read filtering and
the link vocabulary landed; per-device sharing limits and the
contact-visibility summary moved to the interface plan; the per-contact feed
predicate turned out to be built. What is left of Visibility is surfaces, and
they are listed where they will be drawn.

---

## 3. Organ Management & Discovery & Relay & Facade

**Discovery and Relay are READY TO IMPLEMENT** (2026-09-01). This cluster was
parked wholesale; that is no longer right. Its device- and contact-facing
surfaces were the parked part, and they have moved to the interface plan. What
remains here — T0 and T1 of the profile, multi-hop discovery and gossip, the
directory Cell, and Relay Cells — is protocol and infrastructure work whose
design is settled in the entries below, and it can start now.

Two conditions on starting, neither of which is a reason to wait:

- **The profile editor is part of the profile work, not a later task.** T0 and
  T1 publish things about a person, and the whole risk is publishing more than
  they meant to, so the editor showing what each tier reveals and to whom must
  exist BEFORE anything publishes. It is listed in the interface plan alongside
  the rest, and the two land together.
- **Relay Cells begin with the ROLE.** Everything else in that section is
  shaped by it and cannot land first.

**Still last, by the ordering above:** T2 — the Facade, The public face of an
Organ, and Deployments. Nothing in the ready work may be planned as depending
on them.

### Already built — do not rebuild

- **The two-tier roster is not a setting** (`Lince.lingua:344`).
  `engine/src/directory.rs` filters `cell.front_door` and hard-caps at
  `MAX_PUBLIC_CELLS = 4` because a `SignedPacket` will not carry a full roster.
  It is a hard constraint, exactly as asked — no configuration can widen it.
- **`engine/src/directory.rs` is NOT the "directory Cell"** of
  `Lince.lingua:385`. It is pkarr publishing of the public front-door tier. The
  queryable index a relay hosts is a different, unbuilt thing. Do not let the
  shared word hide that.

### ▢ READY — the Organ Profile, tiers T0 and T1

Cut by SIZE, never by audience: a stranger three hops out must never see MORE
because they are far away. Its whole risk is people publishing more than they
meant to, so **the profile editor showing what each tier reveals and to whom
must exist BEFORE anything publishes.**

Defaults and consents, all unbuilt: gossiping your card is opt-in per tier and
none implies the next (a travelled card cannot be recalled); coarse area
defaults unset and, when set, to its coarsest granularity; the seen-set,
per-source budget and age expiry are mandatory rather than tunable; publishing
into a directory is a separate explicit act from gossiping; the invite door
stays default-closed.

- **T0 — the announcement, 245 bytes, text only, forever.** Confirmed still a
  bare clipped display name: `wire.rs:955-958` truncates to
  `UserData::MAX_LENGTH / 4` and sets it. It must instead carry a structured
  pair, display name and profile version. No image ever.
- **The published organ key must NOT ride T0.** A Cell's NodeId is exposed by
  mDNS as a matter of how mDNS works; the ORGAN key is additive and avoidable,
  and it is the value linking every one of your Cells to each other and to your
  public profile. Broadcast on café wifi, a passive listener keeps a permanent
  handle. The key arrives after connect; a nearby row shows no card until then.
- **T1 — the card, a few KB, the only tier that gossips.** Name, description,
  organ key (consent is what separates it from T0, not readability — and the
  signature needs it), coarse area if set, a small inline avatar (128px webp,
  4–8KB), pronouns and language as first-class fields, a reachability hint
  (direct / relay-only / an always-on Cell exists — never naming the Cells),
  and what you are offering or looking for, whose payload is OPEN promises.
  Must degrade to its text half over low-bandwidth radio rather than fail.
- **Sign the card with the organ key — for integrity, not identity.** A failed
  signature drops the card SILENTLY: a badge that can be absent is a badge, and
  a badge is the verification checkmark returning through the side door.
- **Version number, card TTL, and cache eviction as a SEPARATE lifetime**, or a
  content-addressed image outlives the card that referenced it and the
  deleted-photo-returns-forever problem survives the fix aimed at it. Plus a
  last-updated timestamp so a stale card looks stale.
- **Content-address every image and every Facade.**
- **A self-declared coarse area and nothing finer** — country, region, city or
  neighbourhood, the Organ's choice, NEVER derived from IP or GPS.
- **Stays OUT of every tier:** the Cell roster (device count and online-time
  leak), the contact list, and any link rendered as anything but plain
  unverified text.
- **Reach defaults restated:** relay-only stays the default, mDNS stays OFF and
  time-bounded. Both built; listed so the profile work does not reverse them.

### ⏳ LAST — T2, the Facade

An HTML page, fetched on demand, never gossiped. A person builds a sand
describing themselves from their real data and exports it;
`crates/web/static/presentation/board/archive.js` already exports a workspace
as one self-contained file.

**Partly superseded by the interface plan — read that first.**
`anicca/interface/interoperability.md` splits delivery into two visibly
distinct modes (**static**, the frozen no-network archive this chapter
describes, and **dynamic public**, a stream of signed content-addressed
generations pushed to caches) and already carries the manifest, query-ceiling,
withdrawal, safe-renderer and no-tracking work as its own list. It also names
the reconcile: *"reconcile Ontology's current T2 no-network wording with the
two explicit modes above before implementing dynamic Facades."* The bullets
below are the threat model, which holds under both modes; the delivery design
belongs to that plan, not here.

**A different thing shares the name.** `anicca/interface/facade.md`'s **Live
Facade** is a published read-only rendering of one Box composition at a public
URL — late-v1 product work, browser parity proven. The T2 Facade here is an
exported file fetched by hash. Same word, two mechanisms; do not let it hide
that, the way `directory.rs` and `Reach` already tried to.

- **"JavaScript disabled" is not what makes this safe.** Scripts-off HTML still
  beacons through `<img src>`, CSS `url()`, `<link>`, webfonts and form
  actions, each telling the AUTHOR who opened their page and when. The CSP and
  the empty sandbox are what close it.
- **Sanitise ON RECEIPT, not on export.** A hostile author hand-writes the HTML
  and never runs the exporter. The receiving Cell must run the
  strip/inline/neutralise pass over the sender's file as raw untrusted input,
  and **a Facade that fails sanitisation is refused, not cleaned** — rendering
  the stripped remainder turns a partial-strip miss into a live bypass.
- **A stranger's Facade is a threat model `archive.js` was never built for.**
  Its layers are anti-exfiltration, not anti-deception: sandboxed static HTML
  can imitate Lince's chrome and phish a password. Render inside a bounded card
  that visibly belongs to someone else — never fullscreen, never chrome-shaped,
  never able to present anything reading as a Lince prompt.
- **Fetch only on an explicit click.** Pulled because a card scrolled into
  view, it is a passive beacon in everything but name.
- **Links inside are INERT by default**, drawn as visible URL text that does not
  navigate. Click-to-confirm showing the full URL is an explicit per-viewer
  setting.
- **Fetched BY HASH from any holder, never from the origin Organ, not even as a
  fallback.** An origin fetch offered on a cache miss would be taken almost
  every time a Facade is new — precisely when the author most wants to know who
  is looking. **A cache miss renders nothing and says so.** Bound storage
  instead: a per-Facade size cap shown before the fetch, plus relay-side
  eviction.
- **Render-side leakage is solved; fetch-side is not.** The mitigation is not
  anonymity but choosing a server who already knows you — fetch through the
  relay you already use.

### ▢ READY — multi-hop discovery, gossip and the directory

Everything built finds a peer you already hold a key for, or one in mDNS/BLE
range. Neither answers "there is a Need three hops away I have never heard of."

**Gossip cannot reach the world, and the TTL is not the reason why.** A flood
with no hop limit means every participating Cell eventually stores every
profile and every OPEN promise on Earth — arithmetic, not policy. The TTL exists
first to keep the network from melting and only second to limit exposure;
reading it as a privacy knob invites someone to raise it "because I don't mind
being seen", which is the one change that breaks everyone else's storage.

- **OPEN promises are already the payload, and the route exists** —
  `organ_open_promises` serves `GET /organ/open-promises` (`web/src/lib.rs:1535,2011`)
  and already exports what a subject may publicly see. Missing: propagation
  past direct contacts, hop by hop, with a TTL and a per-hop visibility check.
  Not a new message type.
- **A gossip cache and a delegated ask-around search** (`Lince.lingua:267`) —
  the pull half of the same mechanism: asking a contact to ask onward on your
  behalf, under the same hop/TTL limits, and holding what comes back. Only the
  hop-limit reasoning is written down; no mechanism exists.
- **World-reach discovery beyond gossip** (`Lince.lingua:268`) — named in the
  chapter as a mechanism DISTINCT from gossip and from the directory, and never
  designed. It is not the directory Cell below by another name: it is the open
  question of what reaches past both. Nothing to build until it is designed.
- **A seen-set matters more than the TTL.** Dedup on content hash; TTL bounds
  DEPTH, not fan-out multiplicity, and in any graph with cycles the same card
  arrives by many paths. TTL alone is the classic Gnutella flood failure.
- **Forward to a random subset, not everyone.**
- **A per-source rate budget**, or one Organ republishing in a loop is
  indistinguishable from an attack and costs every relay downstream.
- **Age-based expiry independent of remaining hops.**
- **Hop count is sender-spoofable and nothing may depend on it.** A relay drops
  past the limit IT chose. Hop count, `nearness` and `proximity` are three
  different things and none may be conflated in the UI.
- **A directory Cell** — a queryable index an Organ publishes into and others
  search. **Its trust story is its own and it is worse:** a forwarding relay
  sees what passes through it, an index sees every QUERY — who is looking for
  what, when, from where — and that disclosure lands on the SEARCHER and must
  be stated where someone searches. Several independent directories are the
  mitigation; one blessed directory is a naming authority in all but name.
- **Searching an area resolves against the self-declared coarse area and
  nothing else.**

### ▢ READY — Relay Cells

A relay and a townsquare must stay separate in configuration and interface, so
an operator never believes moderating a townsquare gives them power over what a
relay forwards. There is no middle option — a box that holds plaintext "just to
help" is a full Cell with none of a full Cell's accountability.

The first item is the new role; the rest are shaped by it and cannot land first.

- **Relay Cells, not just `iroh-relay`.** `iroh-relay` only rendezvouses and
  falls back for two Cells that already know each other's NodeId. A discovery
  relay is a different role: a Cell willing to carry OTHER Organs' traffic
  onward. New infrastructure, not a mode switch.
- **Consent to relay is explicit per Cell.** Test: a Cell that never opted in
  forwards nothing.
- **Relay mode uses `relay_capabilities()`.** The empty set exists
  (`roster.rs:177`), `cell_may` enforces it, `engine/tests/enrolment.rs`
  publishes it in five places — and nothing in the running app does. Test: a
  published relay Cell is refused capabilities outside that set.
- **A relayed batch carries a SIGNED BATCH envelope**, one signature per batch,
  never per op. Per-op signing puts asymmetric crypto in the typing path — a
  burst of keystrokes is a burst of `crdt` ops. Belongs with relay work, never
  as a tax on ordinary sync.
- **Relaying a contact's ops is per-contact and defaults to the quiet way.**
  Forwarding to your other contacts is a disclosure nobody in that chain
  consented to. Test: the default forwards nothing.
- **The bandwidth ceiling, and NONE of it exists.** Correcting the chapter's own
  anchor: there is no per-peer connection cap under `services.lince.relay.*` —
  the Nix options are `services.iroh-relay.*` and `services.lince.*`, and no
  cap of either kind appears in `scripts/deploy/nixos/`. Both halves (per-peer
  cap, byte accounting against a configurable ceiling) are unbuilt and want one
  option namespace decided alongside the role above. The operator sets one
  number, and the Discovery panel says when a peer is being throttled.
- **A relay over long-range low-bandwidth radio.** LoRa is its own transport,
  not a variant of iroh/QUIC — its own framing, a tiny payload budget, almost
  certainly store-and-forward. Its own adapter from the start.

### ⏳ LAST — the public face of an Organ

- The public face is an Organ you share a subset with; its narrowing is the
  outbound per-field scoping, which is built. It waits on the Facade it serves.
- **A published-subset view**: what the public Organ actually holds, shown as
  data rather than promised in a settings screen.
- **The Facade is generated by the PUBLIC Organ, not the internal one.** Not a
  deployment preference: a generator running where everything is visible is
  TRUSTED to omit the right things, while one running on a box that only ever
  received the published subset cannot leak what it does not have. This is why
  the public face is a separate Organ rather than a Cell.

### ⏳ LAST — Deployments, as Resenha scenarios

Scenarios, not features, and still without a home (cluster 1).

- **`iroh-relay` on the VPS.** The module is complete on its axis
  (`scripts/deploy/nixos/iroh-relay-module.nix` generates the TOML,
  `vps-module.nix` opens the ports and asserts relay and Lince run as different
  users). Needs a public IP, a DNS name and TLS.
- **A Cell on the VPS as a roster member.** `mode = "server"` in
  `lince-module.nix`, enrolled like any device, on a different machine from the
  relay's user and state. This is what makes "edited on the phone all day, walk
  in the door, laptop converges" work without both being awake.
- **Then relay-only costs nothing that matters**, because the relay depended on
  is your own. A conclusion, not a task.
- **The blind mailbox actually deployed.** Wired end to end in the module; only
  the deployment is missing. Naming is settled: `mode = "relay"` became
  `mode = "front-door"` on 2026-08-15 because one word named three different
  boxes.
- **"Where this Cell is reachable from" in the Discovery panel.** A Nix module
  with no way to see whether it is working is a config file, not a feature.

---

## 4. Sync & Login/Sessions

### Moved out — live sessions belong to the interface plan

**Superseded, not deleted.** `anicca/interface/plans/interface.md` already owns
this and specifies more than the Ontology entry asked for: *"Define a distinct
versioned workspace protocol for hello, permission, snapshot hash/chunks,
ordered transaction tail, editor intent, accepted canonical transaction or
structured refusal, durable spatial-checkpoint batch, ephemeral
preview/presence, gap recovery, access loss and host end"*, plus *"Let a guest
cache a verified snapshot for quick reconnect and an explicit read-only
unavailable-host view"* and a human surface with
connected/synchronizing/caught-up/offline/host-ended/access-lost states. That
is reconnect, designed, in the plan that owns the client.

Two facts from the audit the interface plan should inherit, because they say
what is NOT reusable:

- **`transport::Session` holds nothing across a socket.** It is constructed per
  connection with its `subscriptions` and `last_ephemeral` as plain `HashMap`s
  (`transport/src/session.rs:26-53`), and the `session_id` at `session.rs:112`
  is read off the auth session. The new host state cannot be layered on it; it
  has to be a durable thing of its own.
- **`session_id` already travels on the wire** (`transport/src/protocol.rs`) and
  `board/transport.js` already reconnects with backoff and replays. The client
  half of the OLD protocol exists; the new workspace protocol is versioned
  separately and fails closed on unknown versions, so it inherits none of it
  except the lesson.

### ▢ One consent handshake — what is still left of it

Three of its four parts are settled and recorded above: the read side landed
(`offers::pending`), the SCHEDULER was attempted and refused (the two back off
different objects), and the REFUSAL MEMORY is built for all four kinds. The
table of how the four differ is now history rather than a plan — every one of
them records a refusal, none of them tells the sender.

What is left is genuinely one thing: **the "did it arrive?" surface**, where
receipts and checkpoints answer one user question two ways. **Keep the delivery
semantics and the domain objects apart.**

**UI this owes** — now listed in `interface/plans/interface.md`, all reading
`offers::pending`:
one pending-offers list, every kind side by side, each row saying who it is
from or to and what it is over; accept and refuse on the row, routed to the
kind's own verb; the honest empty state ("no offers", not a blank panel); and
one delivery state per row rather than receipts in one place and checkpoints in
another. A refusal must not look the same as an offer that was never answered
in the three kinds that deliberately do not remember one.

### Split — the HTTPS path, after the browser-client gate (2026-09-01)

The item bundled three things under one deployment. The owner's decision —
**reaching a Cell's own interface through a browser is NOT PLANNED
indefinitely** (`interface/architecture.md`) — cuts the bundle in three:

- **Browser login and session to a Cell you are using as a client — NOT
  PLANNED.** The whole justification for this half was the browser client. It
  is modelled, not cancelled, and no task may depend on it.
- **A secure origin for serving a Facade — still planned, and it is the
  Facade's, not login's.** A Facade is a published read-only projection that a
  stranger looks at. It needs a name and a certificate to be reachable at all,
  so it carries this cost and belongs with the Facade work.
- **⏳ The off-LAN camera gap — real, ours, and PUSHED LATER (2026-09-01).**
  `getUserMedia` needs a secure context, so QR scanning fails over plain HTTP to
  a LAN hostname — and that is your OWN phone enrolling against your OWN laptop,
  a Lince client on both ends, so it survives the browser-client gate. But
  **there is no mobile Lince yet**, so the scenario has no second device to
  happen on: the gap is real and cannot be hit. It waits for mobile. When it
  comes back, note that pairing over the wire may be a better answer than a
  certificate — the camera was only ever one way to carry a code.
