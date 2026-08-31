# Ontology — the remaining work in four clusters

Agent-side reorganisation of `anicca/Ontology.lingua`, audited against the tree
on 2026-08-29 and re-cut into the four clusters the owner asked for. The
`.lingua` is untouched; this is proposed text, not a decision.

Markers used throughout:

- **✅ FIXED** — landed in this pass, with the files.
- **⛔ DELETE** — the item is wrong, not pending. Nothing to build.
- **🖐 INTERFACE** — real work, blocked behind the interface refactor. Do not
  start it before `crates/interface` settles.
- **▢** — open, code-side, startable now.

Two structural notes before the clusters.

**`## Sync and the log` in the `.lingua` (lines 11–28) is a table of contents,
not work.** Eleven of its twelve lines are restated later as full
*Why/How/How used* entries: 12↔46, 14↔38, 15↔36, 17↔32, 18/19/20↔68/70/72,
24↔56, 25↔82, 27/28↔the gossip cluster, 7↔52, 8↔48. Counting both doubles the
list. This document counts each item once.

**The logo assets moved and the code has been repointed.** The owner moved the
eleven logo files into `assets/logo/` (and added `assets/photos/`), which left
every `include_bytes!` in `lince_website/mod.rs`, `static_assets.rs` and
`desktop/src/tauri_shell.rs` pointing at paths that no longer existed —
`crates/web` and `crates/desktop` did not compile at all. Paths fixed, plus the
four `raw.githubusercontent.com` URLs in `README.md`.

`assets/lince.excalidraw` was NOT part of that move — it was deleted outright,
while `visual_identity.rs:124` still offers it as a download on the published
site. Restored from git so the build stays honest; if the deletion was
deliberate, `git rm assets/lince.excalidraw` plus dropping `EXCALIDRAW_SOURCE`
and its two references in `lince_website/mod.rs` is the whole change.

`assets/logo/` and `assets/photos/` are still untracked — `git add` them, or a
fresh clone will not build.

The URL strings inside the generated website (`"assets/black_in_white.ico"` in
`visual_identity.rs` and `html.rs`) are deliberately unchanged: those are the
exported site's own layout, not repository paths.

---

## 1. Cleanup & Misc

### ✅ FIXED — `file_sync.rs` no longer claims selection is unconfigurable

The module doc said "Selection is deliberately NOT Protein-configurable yet"
forty lines above `configured_filter`, which deserializes a
`protein::Predicate` out of the sync config and is used at `file_sync.rs:1007`.
Replaced with what the code does. `crates/engine/src/file_sync.rs:9-11`.

### ✅ FIXED — one name for one act: `create-message`

`Action::SendMessage` is gone. `create-message` was the superset — it carries
`parent` and `references`, tolerates a thread with no replica root, and runs
the transfer-thread writer check — and both actions already sat in the same
permission arm, so this changed no permission. The Conversation sand and its
assertion were repointed.

- removed: `crates/engine/src/actions.rs` (variant, execution arm, permission arm)
- kept: `threads::send_message`, still used by the thread-invite path
  (`actions.rs:3831`) and by `individual_replica` / `live_workflow` tests
- repointed: `crates/web/src/sand/conversation/conversation.html:283`,
  `crates/web/src/sand/conversation/mod.rs:23,57`

The sand touch is one string. It is named here because the sands are the thing
being refactored — **re-verify this line survives the refactor.**

### ⛔ DELETE — replica bootstrap for a new contact (`.lingua` line 23)

`crates/engine/tests/replica_bootstrap.rs` argues in its own header that there
is nothing to build. Retention only drops ops that a newer op for the same
target superseded, so the surviving log always contains, for every live field,
the op that established its current value — "bootstrap" is replaying from zero
and is complete by construction. The snapshot alternative was rejected because
it needs synthesized `(actor_cell, hlc)` identities, and that identity IS the
unique index the import path dedupes on. This is a decision to record above the
list, not a box.

### ⛔ DELETE — "`contact.mode` (replica vs live) column is unused"

Wrong twice over. The column was repurposed: `Contact::reach()`
(`store/src/organs.rs`) maps it to `direct` / `mailbox` / `auto`, migration
`0066_contact_reach.sql` normalised it, and `engine/src/wire.rs:3479,3523`
branches on it to decide mailbox versus direct delivery. It is load-bearing.
What is actually missing is that **nothing sets it** — `organs::set_mode` has
exactly one caller and it is `engine/tests/contact_reach.rs:183`. That is a
control on the contact panel, so it moves to cluster 3 as 🖐 INTERFACE.

### ▢ New: two different enums are both called `Reach`

`store::organs::Reach` (direct/mailbox/auto, how we deliver to a contact) and
`engine::wire::Reach` (Local/Relay/Internet, how this Cell is reachable) share
a name and mean unrelated things. Every grep for one returns the other. Rename
one — `store::organs::Delivery` reads correctly for the contact-side enum.

### ▢ `Undelete` and `Restore`

Deletion is a hard tombstone with no way back, and a record tombstone freezes
its Loro doc. Wants a newer lifecycle op above the tombstone, landing before
edits resume on a frozen doc. Engine work is startable; the button on the
Record sand is 🖐 INTERFACE and must land in the same task, so in practice this
waits.

### ▢ Exact multiply and divide

Multiplication states its result scale, division states its rounding, scale
overflow is a publish-time refusal and a zero divisor a runtime one. Unit
algebra explicit, never inferred. Pure engine work, startable now.

### ▢ A mention in a body becomes a link

`[[Title|uid]]`: the uid is authoritative, the title is decoration, and
renaming a Record must NOT rewrite the bodies that mention it — that would be
an op storm across every peer for a cosmetic change. First mention per file
becomes a link, later mentions stay plain: a rule for whoever GENERATES text,
never for the parser.

### ▢ Every new public wire type extends the golden fixture in the same commit

A process rule, not a feature. Its value is a failing test at the moment of the
omission.

### ▢ The release train — deferred on purpose, keep it deferred

The protective half is built (`ALPN_HELLO = lince/hello/1`,
`engine/src/wire.rs:95`). The scheduling buys nothing while nobody else runs
Lince: no stale peer to strand, no store review to wait out, and a cut costs
one rebuild. It becomes real the day someone else's device depends on this one.

### ▢ The Deployments pointer is dead

`.lingua` line 66 sends the deployment scenarios to `anicca/Resenha.md`, noting
it no longer exists. It still does not. `anicca/` now holds `Karma.lingua`,
`Lince.lingua`, `Ontology.lingua` and `interface/`. Those scenarios need a home
or an owner's decision to drop them.

### Housekeeping spotted in passing

`anicca/interface/First Steps.linguai` — untracked, typo'd extension. Not
parsed by anything (`crates/anicca/src/lib.rs` and `crates/engine/build.rs`
collect `.lingua` only), so it is invisible rather than broken.

---

## 2. Visibility

_The disclosure bug that lived in this cluster's neighbourhood — one device's `baseUrl` travelling to every sibling Cell — is fixed under cluster 4, because its cause was where the config was stored, not who could read it._

### ▢ The read half of per-person permissions

The `.lingua`'s Visibility item asks for two things and only one exists.

- **Built:** write-side roles and permissions. `crates/web/src/sand/permissions/`
  ships create-role, create-user, assign-role, assign-user-person,
  set-person-standing, grant/revoke-permission, every one enforced engine-side
  against a permission key.
- **Unbuilt:** read filtering. Someone who logs into your Organ still sees
  everything; permissions gate create/update/delete only. The mechanism that
  would do it exists — a `Protein` predicate per logged-in Actor, evaluated on
  read and on the write path so an edit can only touch what the filter admits.
  The same treatment is wanted per Cell.

Note the honest cost, already recorded in `actions.rs` beside `RosterStatus`:
the organ permission set is create/update/delete with **no read tier**, so
several reads are currently gated as updates. Adding `organ:read` belongs with
this work rather than before it.

### 🖐 INTERFACE — a summary of what each contact can see of you

The single most-wanted read in this cluster and the one a per-contact panel can
never answer. Pure UI: a read-only aggregate over existing `organ_contact` rows
— direction, both scopes, hide-list count, and the unreadable-scope flag
`store::organs` already derives. No new storage, no sync change. One screen,
every contact side by side, each row saying everything / only these columns /
nothing but which Record, broken ones called out, clicking a row opening the
contact panel that owns the change.

### 🖐 INTERFACE — per-device sharing limits

A device is all-or-nothing today: full access or logged out. Give roster
entries the `scope_fields` column the per-contact scope already has (migration
`0056_contact_scope`) and evaluate it in the same predicate so the two cannot
drift. **A different axis from capabilities** — capabilities say what a device
may DO, this says what it may SEE. One scope control per device, beside Remove.

### ▢ Naming links in a scope

`record_assertion` is a relationship BETWEEN Records, which the column
vocabulary cannot name, so a narrowed contact receives no links at all —
fail-closed and correct, but narrowing costs the whole graph. Wants a
vocabulary extension for the scope language, not a reserved word smuggled into
the column list. "Share the head and the `@part-of` links" is the target.

### ▢ Deferred, correctly — the absent-not-blank sweep

`row.body || ""` collapses "withheld" and "empty" (five in
`sand/kanban/kanban.html` alone). Still deferred, and the reason still checks
out: `Protein.fields` is set to anything but `None` only in
`engine/tests/organ_contact.rs`, `engine/tests/organ_sync.rs` and
`crates/interface/src/spatial_diagnostic.rs` — **no shipped sand narrows**, so
there are zero live instances and a sweep would be a large diff against a
hypothetical. It becomes 🖐 INTERFACE work the day the first sand narrows.

### ▢ A per-contact predicate on the feed — speculative, not blocked

The concrete need ("keep these Records from this contact") is met by the hide
list. Closing it honestly needs a per-contact predicate, per-Record per-op
evaluation on the serve path, and a **served-set table** so a Record CROSSING
the predicate can be told from one that always matched. The hazard to answer
first: a Record entering is a grant and one leaving is a revoke, both fired by
a change nobody meant as an act of sharing. Live references and Karma rules
both put predicates near the serve path; either will say what shape this wants.
Building it first is guessing at an interface with no caller.

---

## 3. Organ Management & Discovery & Relay & Facade

**This whole cluster needs the new interface.** Items marked ▢ inside it are
engine-side and could start early, but every one of them carries a surface that
cannot be drawn yet, and the project rule is that the surface ships with the
mechanism. Treat the cluster as parked.

### Already built — do not rebuild

- **The two-tier roster is not a setting** (`.lingua` line 104).
  `engine/src/directory.rs` filters `cell.front_door` and hard-caps at
  `MAX_PUBLIC_CELLS = 4` (`directory.rs:84,111-113`) because a `SignedPacket`
  will not carry a full roster. It is a hard constraint, exactly as asked — no
  configuration can widen it.
- **`engine/src/directory.rs` is NOT the "directory Cell"** of `.lingua` line
  145. It is pkarr publishing of the public front-door tier. The queryable
  index a relay hosts is a different, unbuilt thing. Do not let the shared word
  hide that.

### 🖐 Devices and identity

- **Narrow a stolen phone instead of revoking it.** The mechanism is built and
  signed into the roster — `CellEntry.capabilities` (`roster.rs:105`) and
  `cell_may` (`roster.rs:690`). **Correction to the `.lingua`:** "nothing calls
  it yet" is stale; `wire.rs:1828` calls it with `CAP_REPRESENT`. What is
  missing is only the *editor* — no Action edits one Cell's capabilities, and
  `roster-status` merely reads them to decide whether to show a relay badge
  (`sand/organ/organ.html:1619`). A capability editor that republishes the
  roster, needing the root like every roster change, beside Remove.
- **Succession UI.** `store/src/roster.rs:245-304` stores and reads the
  `identity_succession` chain. Nothing renders it.
- **A reach control per contact.** `contact.mode` drives mailbox-vs-direct
  delivery and nothing sets it (see cluster 1). One control on the contact
  panel.
- **A Move button.** The Move verb is built end to end (see cluster 4); no
  surface reaches it.

### ▢ Close the no-roster gap in the Cell check — engine-side, startable

When we hold no roster for the sending Organ the op is admitted, because
refusing would drop every contact paired before rosters travelled (`wire.rs`
says so at the admit site, and mid-first-boot is what makes it load-bearing).
Until it closes, dedup poisoning — pre-inserting `(your_cell, future_hlc)` so
your real op is dropped everywhere as already-seen — is open against exactly
those contacts. Fix: make roster exchange part of pairing, then refuse an op
from an Organ whose roster we do not hold. The only visible consequence is the
contact panel saying "no device list yet" for a contact still on the old path.

### 🖐 The Organ Profile in three tiers — the largest thing left

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

### 🖐 T2 — the Facade

An HTML page, fetched on demand, never gossiped. A person builds a sand
describing themselves from their real data and exports it;
`crates/web/static/presentation/board/archive.js` already exports a workspace
as one self-contained file.

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

### 🖐 Multi-hop discovery, gossip and the directory

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

### 🖐 Relay Cells

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
- **The bandwidth ceiling, and NONE of it exists.** Correcting the `.lingua`'s
  own anchor: there is no per-peer connection cap under `services.lince.relay.*`
  — the Nix options are `services.iroh-relay.*` and `services.lince.*`, and no
  cap of either kind appears in `scripts/deploy/nixos/`. Both halves (per-peer
  cap, byte accounting against a configurable ceiling) are unbuilt and want one
  option namespace decided alongside the role above.
- **A relay over long-range low-bandwidth radio.** LoRa is its own transport,
  not a variant of iroh/QUIC — its own framing, a tiny payload budget, almost
  certainly store-and-forward. Its own adapter from the start.

### 🖐 The public face of an Organ

- The public face is an Organ you share a subset with; its narrowing is the
  outbound per-field scoping, which is built. It waits on the Facade it serves.
- **A published-subset view**: what the public Organ actually holds, shown as
  data rather than promised in a settings screen.
- **The Facade is generated by the PUBLIC Organ, not the internal one.** Not a
  deployment preference: a generator running where everything is visible is
  TRUSTED to omit the right things, while one running on a box that only ever
  received the published subset cannot leak what it does not have. This is why
  the public face is a separate Organ rather than a Cell.

### 🖐 Deployments — need a second machine

Scenarios, not features. They have no home since `Resenha.md` went (cluster 1).

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

### ✅ FIXED — the per-Cell surface config no longer lives on a Record that syncs

This was the worst finding of the audit: not unbuilt, but **attempted and
undone by live code.** Migration `0067_no_base_url.sql` blanked the Organ
Record's body and deleted every `lince.organ` extension row — and
`store::organs::ensure_local` wrote both straight back on *every boot*
(`cell_bootstrap.rs:49` and `store/src/lib.rs:180` both call it at start), with
`adopt_identity` writing the same blob raw during enrolment. The migration
cleaned a table the next process refilled, so the migration list read as though
this had landed. One device's `baseUrl` still travelled to every sibling Cell.

`baseUrl`, `aliases` and `local` now live on the **Cell** Record under
`lince.cell.surface`, which never syncs and never logs an op — the home
`cells.rs` already documents for exactly this ("Local-only settings … belong on
this Record precisely because it does not sync"), and the same reason a relay
holding `relay_capabilities()` can still configure itself.

- `crates/store/src/organs.rs` — `CELL_SURFACE_CONFIG` replaces
  `LOCAL_ORGAN_EXTENSION`; `ensure_local` writes through `cells::set_config`;
  `local()` reads the Organ's identity from the Organ Record and this machine's
  surface from the Cell Record; the Organ Record body is no longer a base URL;
  `adopt_identity` writes the Cell's config inside the identity-swap
  transaction, so a device cannot end up enrolled carrying the address of the
  Organ it just left.
- `crates/store/migrations/0072_surface_config_on_cell.sql` — clears what live
  code had been refilling since `0067`. **A new migration rather than an edit
  to `0067` in place**, which is the usual rule for unshipped migrations: `0067`
  has already run on the existing dev database, so editing it would have been a
  no-op there.
- The empty-string contract is preserved exactly: `ensure_local(pool, "")` still
  means "do not change the address", so a CLI invocation cannot wipe the
  address a running web Cell wrote. `enrolment.rs:301` reads the current
  address and passes it through the swap, so joining an Organ keeps it.

**The regression guard is the NEGATIVE assertion**, added in
`crates/engine/tests/organ_cell_split.rs`. Reading the surface back proves
nothing on its own; what matters is that the syncing Record carries no surface
to travel with.

- `this_cells_address_never_reaches_the_organ_record` — the Organ Record's body
  is empty, it holds **zero** extension rows, and the surface config is on the
  Cell Record and that Cell's alone.
- `joining_an_organ_keeps_the_address_on_the_device` — after `adopt_identity`
  the device still has its own address, the joined Organ has zero extension
  rows, and the Cell Record is the same row repointed.

Both would have failed against the previous code, which wrote one extension row
onto the Organ and a base URL into its body.

Verified: `cargo check -p store -p engine -p lince-web` clean; `cargo test -p store`,
`-p engine --test enrolment --test organ_cell_split --test organ_sync --test
contact_share --test rebuild`, and the `lince-web` conversation sand tests all
pass. (`lince-web` was verified before and after the asset-path fix above.)

### Already built — Tree sync and the Move verb

`.lingua` line 80 files this under "Design work with no code yet". That heading
is false. Built: `Action::MoveRecordTo` and `Action::CancelRecordMove`
(`actions.rs:404,409` and the arms at `3563-3594`), `store/src/record_move.rs`
over migration `0070_record_move.sql`, the exactly-once hand-over in
`engine/src/share.rs:305-328` (`still_queued` → `last_op_seq` →
`mark_handed_over`), and `engine/tests/contact_share.rs:631` covering a moved
Record carrying its children. The dangerous half — Move as distinct from Copy,
where delivering twice means two copies and losing the ack destroys data — is
the half that exists. **What is missing is only a way to reach it**, which is
cluster 3.

### ▢ A database-level guard for the import sequence, ACROSS PROCESSES

The clearest startable item in this cluster. `Engine::import_lock` serializes
read-compare-append-materialise in-process (`sync.rs:415`, `collab.rs:167,316,551`),
but two Cells sharing a database in separate processes — which is what happens
whenever the CLI touches the store while the web Cell runs, and WAL mode
permits it — can interleave and leave a lower-HLC value in the read model while
the log keeps the higher one. It does not self-heal.

Fix: a conditional `UPDATE … WHERE field_hlc < ?` on the materialise step, so
the database refuses the stale write rather than the process remembering not to
make it. `engine/src/lib.rs:185` already says this is what should be there.
**No longer blocked on being untestable** — `engine/tests/multi_process.rs`
drives the `cell_worker` binary and already calls `audit_read_model` at line 161.

### ▢ Nothing recomputes the read model on a schedule

A divergence found by `audit_read_model` does not self-heal without someone
running `audit_and_repair`, and `audit_and_repair` appears only in
`engine/tests/rebuild.rs`. Pairs naturally with the guard above: the guard stops
the divergence, this notices the ones that got through.

### ▢ Per-contact rate limiting on the reject path

The quarantine ring bounds storage, but a hostile contact can still make us do
the work of refusing, every pass, for free — and an empty version vector
legitimately means "send me everything", so a peer sending one every pass makes
us serve the whole log repeatedly. `store::budget` holds the shape (a named
budget kind plus `evict_plan`) but has only byte quotas; this adds a rate
dimension, backing off the contact rather than the queue. Its surface — "we are
now answering them less often", with the reason — is 🖐 INTERFACE.

### ▢ Live sessions do not resume — the SERVER half is what is missing

The browser half is already there: `board/transport.js` reconnects with backoff
and replays, and `session_id` travels on the wire
(`transport/src/protocol.rs:38,51,150,160`). What does not exist is the server
half — `transport::Session` is constructed per connection with its
`subscriptions` and `last_ephemeral` as plain `HashMap`s (`session.rs:26-53`),
and the `session_id` at `session.rs:112` is read off the auth session. Nothing
outlives a socket. A guest whose connection is genuinely lost is dropped
mid-sentence and comes back as a stranger.

Wants: a reconnecting client presenting its prior session id, the server
rebuilding that state, and an explicit decision about what may be replayed
(collab already re-exports from its last ACKED version; subscriptions would
re-run). Engine/transport work — startable without the interface, since the
board side already exists.

### ▢ One consent handshake, three surfaces (four implementations)

Thread invites, replica agreement, accepting a tree and transfer acceptance
spell the same handshake four ways: an offer lands, it sits pending, the
receiver accepts or refuses, nothing moves before, a refusal is remembered.
Unify the handshake, the retry/backoff scheduler (the transfer delivery worker
and the outbox drain are two schedulers with two policies and two chances to
mishandle an offline peer), and the "did it arrive?" surface (receipts and
checkpoints answer one user question two ways). **Keep the delivery semantics
and the domain objects apart.** The one pending-offers list it produces is
🖐 INTERFACE; the unification underneath is not.

### ▢ The ordinary HTTPS login path

No longer on the critical path — live-over-iroh replaced it for the workflow
that motivated it — but still the only way a plain browser reaches a Cell that
is not its own, and it is what closes the off-LAN camera gap: `getUserMedia`
needs a secure context, so QR scanning does not work over plain HTTP to a LAN
hostname, which is exactly how a second device reaches this Cell. Wants a
documented reverse-proxy deployment with a real certificate, and the Cell
knowing its own external name.
