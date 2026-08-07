# Web Interface

The Web interface is an HTML based one. It can be run in browsers or as a desktop app with Tauri.

The base app should be minimalist to give as much space as possible for user's to express themselves. That expression should feel familiar, reflecting what they want.

In addition to what we say is the base app we have possibly many components, widgets, called 'Sand'. They are HTML iframes inside a canvas, so like blocks of lego in a whiteboard. We have an edit mode for being able to move components around, add or remove them, including many other actions.

Vibe: As minimalist as possible, without loosing friendliness.
"No dashboard." The north star is Ana spending under four minutes, all of it on decisions only a human can make. The UI's job is to disappear. Attention is the scarcest resource in the system, so color, motion and elevation are spent, never decorated with.
Honesty over decoration. "A number on a chart that nobody can explain is worse than no number." Surfaces are opaque, line styles carry truth (settled vs. declared), color never carries meaning alone, as much as possible we replace colors as the main meaning (like a green ball for connection up with an icon for good connection that can be configured to have a color).
A whiteboard, not a cockpit. Sand are lego blocks on a blank canvas — dots, strokes, hand-drawn arrows, blocks the user arranges. Familiar, paper-like, user-owned. When the user makes their lince, it feels like they are creating an art piece, the built-in ui should be minimalist to not carry the composition away from the user's intention.
The brand is black and white by default, with purple as the supporting color.
Components are flat by default. A component may opt into the one restrained shadow utility when it needs to appear above an adjacent region; the area that is not content should feel tight and shunk, boiled to the essence, minimal, functional.

## Lince palette reference

Roxo Cobalt — CMYK: 66, 71, 0, 36; HEX: `#3730A3`; RGB: 55, 48, 163.
Roxo Noturno — CMYK: 59, 58, 0, 5; HEX: `#6366F1`; RGB: 99, 102, 241.
Chumbo Profundo — CMYK: 10, 10, 0, 92; HEX: `#121214`; RGB: 18, 18, 20.
Cinza — CMYK: 10, 5, 0, 12; HEX: `#A7B4C2`; RGB: 203, 213, 225.
Branco Gelo — CMYK: 2, 1, 0, 1; HEX: `#F8FAFC`; RGB: 248, 250, 252.

# Web Platform

## Board and sand infrastructure — the shipped web surface

- [x] One WebSocket (`/host/transport/ws`) shared by the unified bridge and the Data panel; the bridge speaks both the legacy nested-payload chrome shape and the current flat `frame.js` shape, routing by subscription id and lane room (ids never collide across consumers).
- [x] Sands are Rust-canonical: each official sand is a self-contained `.html` via `include_str!`, registered in `OFFICIAL_WIDGETS`; groups ship as `.lince` workspace archives; the catalog peeks content so a group archive is never mis-parsed as a single sand, and a group entry replaces a same-named single sand.
- [x] Groups nest: `BoardCard.group_ids` (outer → inner) is authoritative; disbanding an outer group preserves inner ones; adding a catalog group re-homes to a fresh inner id each time, so repeated adds are independent.
- [x] Events are scoped to a grouped sand's innermost group; ungrouped sources broadcast board-wide; cross-session mirroring rides lane rooms, never persisted.
- [x] (2026-07-19) Kanban, Relations, and Communication no longer ship as a GROUP bundled with their own Record sand — every board already has exactly one pinned Record (`shell-record`, bottom-right corner, icon by default), so bundling a second one per sand was redundant and, worse, its group scoping meant a grouped kanban's `recordClicked` never reached the pinned one. These three now ship as plain single `.html` packages (ungrouped), so their board-wide `recordClicked`/`recordCreate` reaches the pinned Record directly. The generic group-archive machinery (`.lince` workspace archives, `is_group` catalog entries, drag-drop import) stays for user-authored/imported groups — only the three OFFICIAL auto-grouped catalog entries were removed. Kanban's default add-to-board size also grew (`initial_width`/`initial_height` 6×6, up from 7×5 pre-clamp) since it's no longer sharing space with a bundled Record card.
- [x] Per-card host state flows both ways (`H.getCardState()`/`H.onCardState`/`H.patchCardState`) — any sand persists UI prefs without touching the Ledger; board chrome itself (pan/zoom/workspaces/position/size/pin/z-index/grouping/edit mode) is ALWAYS host state, never a Ledger fact.
- [x] The Data panel is the one place Protein gets configured (source, nested AND/OR filters up to 10 levels, sort, limit, includes) per card — sands ship with NO default driving Protein; an unconfigured card shows an explicit "pick a Protein" prompt instead of silently dumping every record. Negation belongs to one condition. Record filters include assignee, work dates, assertion predicate, quantity, text, and generic directional assertions. The builder autocompletes predicate inputs from a `concept` source subscription; "All records" drives an explicit `{source:"record"}`, distinct from "unconfigured." The binary-assertion include is multi-predicate ("+ predicate" rows, `"*"` = every predicate, both AST spellings round-trip) — one Protein pulls several predicates and the Relation graph draws parallel assertions between the same two Records as fanned-out bent lines. See [Ontology](Ontology.md) for the shared model.
- [x] The shared slash-block editor (`window.LinceBodyEditor`) is used by every sand that touches record bodies: `/` opens a Notion-like block palette (headings, image placeholder, checkbox), `@` opens the Record picker; the body stays canonical markdown, checkboxes toggle by original line index, and `@slug` chips navigate and become `@references` assertions on save. Optional — a sand without it degrades to a plain textarea.
- [x] Local images: the editor's "/image" block picks/uploads a file (native OS dialog first, browser `<input type=file>` fallback), sniffs bytes against a raster allowlist, and stores under an opaque generated name — there is still no route serving an arbitrary disk path.
- [x] Action `warnings` reach sands end-to-end (bridge → `frame.js` → amber sand status), never surfaced as errors.
- [x] Record deletion is permission-gated (`record:delete` vs `record:delete_own` + creator match) at the one `DeleteRecord` action — since threads/messages are themselves records, this single gate covers all three; viewer identity (`H.getViewer()`/`H.onViewer`) flows to every sand so delete controls can show/hide correctly, though the engine gate (not the UI hint) is what actually enforces it.
- [x] The permission/role/user system is Protein(`source:"auth"`) + five gated Actions (`create-role`, `create-user`, `assign-role`, `grant-permission`, `revoke-permission`) — a plain CRUD sand on top, no different in kind from any other sand; auth-table mutations emit no facts, so the sand re-subscribes after every mutation instead of relying on live invalidation.
- [x] (2026-08-07) Package publish/catalog is disk-backed, not bucket-backed — no object-store backend runs anywhere in this codebase (see `media_assets.rs`) and `crates/transport` carries no package-fetch frames, so it is scoped to this Cell's own local organ (`/organ` already only ever returns the local organ). `sand_publisher` (now registered in `OFFICIAL_WIDGETS`) previews an uploaded `.html`/`.sand`/`.lince` package, writes it under `paths::dna_dir()` (`lince/dna/sand/<prefix>/<slug>/<version>/...`), and creates a `record` + `record_extension(namespace="lince.dna")` — the same op-log sync that already replicates `record_extension` (`engine::sync`) carries a published package to a paired organ with no bespoke cross-organ publish protocol. Cross-organ *search* (browsing another organ's catalog before it has synced in) stays out of scope until such a protocol exists. Unpublish drops the extension row only — the Record itself stays, since removing it from the catalog is not the same act as deleting it (that stays the permission-gated `delete-record` Action's job).
<!-- - [ ] Per-sand capability/permission model before imported sands can write arbitrary Actions (today any sand can call any Action — fine for official sands, needed before running imported ones freely); sand provenance `cause=sand:<uid>`. -->
<!-- - [ ] Blanket read/write permission enforcement across every OTHER Protein source and Action (today only `delete-record` and the five auth actions are gated) — sequenced after more of the role-management UI exists. -->
<!-- - [ ] `.lince` GROUP drag/drop import: client routing still checks the `.group.sand` extension — route by content instead, like the catalog does. -->
<!-- - [ ] Host-state sync for board presentation state across devices. -->

## Wire protocol — how a sand talks to the Cell

- [x] One WebSocket (`/host/transport/ws`), multiplexed: Protein (reads) + Actions (writes) + ephemeral lanes (presence/cursors/events) + explicit host capabilities (e.g. a terminal PTY session) whose bytes don't belong in the Ledger.
- [x] Actions are JSON with a kebab-case `"action"` tag, snake_case everywhere else; Protein predicates/includes are snake_case too.
- [x] A subscription answers with a snapshot then re-executes and pushes on every relevant commit; invalidation is coarse-by-source — render idempotently, a sand may get refreshes it doesn't strictly need.
- [x] Action responses carry `created`, `facts` (what the Ledger committed, including any Karma cascade), and `warnings` (non-fatal advisories) — show warnings, never treat them as errors.
- [x] Ephemeral-lane and host-capability traffic (cursors, clicks, presence, PTY bytes) is never persisted; terminal PTYs are scoped to one connection and die with it.


# Supercomponent, a Theory

All components are a configuration of the theoretical potential Supercomponent. They are configured with some sliders, like:

How much information of records do you show?

How much information of the interactions between records (links and such) do you show?

Does a record position on the component matters?

How much information of other features of Lince can I see that are related to this Record (like transfers and automations that envolve it)?

The supercomponent could have a configuration to change how records are shown and simulate a kanban, a relation graph, etc.

Kanban sand is: variable info, not much interaction beyond parent/child task, position matters a lot because of columns for state, no extra info about automations and transfers.

Relation sand is: low information about record, spatial positioning doesnt matter, interactions between records matter, links. Show no automation or transfers.

Karma is: show little info, spatial doesnt matter, interactions matter for automation, transfers matter when records have automatic transfers.

The supercomponent could be built once, and then when someone wants a karma that shows a lot of info, or that orders records spatially in a certain way they will tweak configurations of the component, not create a new one. I feel like that is the future of interfaces, configuration above creation, AI might make creation easy, but if i dont have a need to create another, pressing one button is always less work than prompting then we minimize work on the side of users to fit their workflows.

We are implementing different sands, not thinking about the supercomponent, how much implementation of the same physics engine, the same sorting, the same querying of record info can we do until we decide its time for the ultimate Lince component?

But let's go one step further, what if we dont have the supercomponent, what if base lince has all of those capabilities? We are doing canvas inside canvas, remove nesting, records will be nodes just like other html components, why not?

We need to know when to go further in the crazyness, and when to retreat to a more strategic point of implementation. The theoretical supercomponent is too powerful to be implemented right now, and should be guarded from existing until the time is right. The best right now if we are to advance in it's direction is to join done components into a proto-Supercomponent. When Kanban is ready it is joined with Relation, so we can manifest a Kanban, a Relation sand or some other one that has some features of each but different points, something in between, never seen. And we could then keep on absorbing components, with a click in it's configuration to make it instantly fit it's lever and sliders to be exactly like a kanban or a Relation, snap into position, so it feels like we have many components inside one, but it's actually exponentially diverse.

Down here are the past implementations of a subset of the Supercomponent, with specific names like Kanban, Relation, etc. They have an identity and think they are unique, but they are actually part of a whole, they just don't know it:

# Record

Slash commands, to be able to put several types of blocks in the body of Records as cards of kanban, or even as any Markdown body (reusable). If you type the underlying character/s you will end up seeing the same visual block, but you can enter slash mode to select from a list by name or start typing characters to filter them.

The list, with the characters and their blocks goes as following:

- [ ] '#': Headers 1-7
- [ ] '![](url)': Images either url or bucket if no prefix, maybe we can choose if external source of bucket, maybe we can grab from pc and then put in bucket.
- [ ] '- [ ]': Checkbox

- [ ] Being able to reference other Tasks inside comments.

- [x] **Record** (formerly "record_info" — the sole markdown editor, viewer, and creator for a record, and the home for every other per-record concern) — the get view IS the edit view (head/slug/quantity/ body writable, Save writes only what changed, a dirty form is never clobbered by live updates); Zero (`deactivate`) and Delete (`delete-record`, permission-gated) are separate buttons; creation mode shows the same fields empty, Create + focuses the new record; carries the shared slash-block editor (headings/images/checkboxes/`@slug`, the same palette everywhere in a body); collapsible sections for **Work** (start/due dates, estimate, worklogs with play/pause, on the `work` record extension, offline-queued writes), **Assignees** (`assigned-to` assertions), **Relations** (every hop-1 binary assertion in either direction, predicate+object inputs, both autocompleted — a document/URL is an asserted relationship or inline media in the body, with no separate resource/attachment model), and **Threads** (a real multi-thread system — a tab per thread, search filters which tabs list without hiding messages; chat-style runs (2026-08-07) show the sender name — `user@organ` when the message's origin-organ name differs from the sender's, just `user` when they match (the common single-user-organ case) — only on the first message of an unbroken run from the same `created_by`, every message keeps its own bottom-right timestamp and edit/delete controls, and editing an existing message now uses the same shared slash-block editor as composing one; `@slug` in a post becomes a Record reference, delete controls per permission). Reusable — any sand drives it via a scoped `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel. Full real-time collaborative editing is blocked on the CRDT text relay in [Synchronization](<Synchronization.md>).

# Kanban
The Kanban sand when ready will be able to provide teams the organization necessary to tackle projects together in a classic way. The data they CRUD in Kanban is accessible in other sands to fit greater workflows though.

- [x] Have a way to create a new Record.
- [x] Moving one card from one column to the other issues an update on the quantity of the Record.
- [x] Cards can be shown minimally or with a lot of information about them displayed.
- [x] The columns of the Kanban dictate what quantity the Records have underneath. The user doesnt have to know that column Done is for quantities 1 by default. But they have to know if they want to change it. I have a place to configure my columns, in that case i need to select one quantity for the the column, so Records with that quantity are shown in the respective column (would be cool to select a range, like from 1 to 2, 3 to 10). I must be able to sort them with drag and drop to say that column X is to be -1 and move it to -2. I must be able to click buttons to create new columns, give a name and type a quantity. Maybe have a tooltip to signal the reason behind using quantity.
  - [x] Have a way to CRUD column presets, like instead of Todo, WIP, Done its Backlog, Next, WIP, Finished. And i can apply one to this Kanban.
- [x] Having a small indicative that the connection with the backend is ok, can be used to signal that an update is taking place and when it is finished (maybe a cute little ball with different colors for the states - duds).
- [x] Be able to select one or more Records, to execute possible actions: move to another column, delete.
- [x] Currently, metadata of Records is only visible and editable in the Record sand, a reusable sand for editing in-depth info about Records. We must be able to see such metadata, even if we can only interact with it through Record sand. Either way, here are the tasks for metadata control someway:
  - [x] Date for the supposed start and end of the task.
  - [x] Time estimate, how much time do i think this is going to take, in hours and minutes (the data saved is in minutes).
  - [x] Play/Pause button to log time spent in the task. Play starts a work log, Pause ends one, time is added on Pause. Also we need to be able to full CRUD this so that if i spent some time before I can add it, if I inputted something wrong i can update the existing or delete it.
  - [x] Assign the task to someone, by their name or username.
  - [x] Be able to set the parent/children of this task.
  - [x] Being able to CRUD threads and messages as links of records (that belong to a record) that can have the same complexity of body content: text, images...
  - [x] The interaction with the body of Record must be able to have slash '/' commands to put add content in an easy way: typing /h3 will give you ### which is the end result that remains in the body (###). If we can make the body of a kanban have text, why not make it have the full editing and visualization that the 'Record' sand has for the body of the Record? We implemented the same checkbox clicking in the body of the record in kanban, why not put the body of the Record of the 'Record' sand?
    - [x] Changing the body of Records in Kanban cards by clicking on it to write in it or to check a box (slash commands only in Record sand? too hard to implement such feature twice? gotta be a way)
- [x] Filter and search cards through Protein by assignee, work date, assertion predicate, directional binary assertion such as `@parent`/`@child`, quantity, or text. Filters can be nested in AND/OR groups up to 10 levels. See [Ontology](Ontology.md).
- [x] Optionally group cards in swimlanes by assignee, parent, concept, or a Protein grouping key.
- [x] Order based on several important fields, from head of record, to quantity, @concept and links. 

# Relation

Relation is a graph projection of **binary Record assertions**. It does not own
a separate relation or link model; the shared data semantics, CRUD operations,
hierarchy widening, and Protein behavior live in [Ontology](../Ontology.md).

- [x] **Relations** — d3 force graph with physics sliders, golden-angle layout,
  zoom/pan/fit, directed arrows, and assertion-predicate labels. Shift+drag
  asserts a directed binary assertion optimistically; edge-click selects it;
  the header chip retracts it. Predicate inputs autocomplete from Concepts.
- [x] **Trail mode** — projects a selected predicate's forward assertion graph
  topologically, with a Done/Undo promotion cascade over shared status
  predicates (quantity or Concept buckets such as
  `@todo/@next/@wip/@done`). Delete retracts the selected assertion; Ctrl+Z
  undoes the local session action.
- [x] **Protein trails and Focus** — consumes a directed assertion-order item
  from Record Protein. The returned ordering supplies traversal and the
  earliest root; Focus advances through matching Record states.

# Table

Table sand is responsible for being the base of the components. Since original data in database is in a table the Table sand is the simplest to translate the incoming data to a visual structure.

Rebuilt on LynxUI (2026-08-07): a persistent gutter (configurable line numbers,
per-card host state) reveals a per-row delete button on hover, permission-gated
the same way as everywhere else (`record:delete`/`record:delete_own`); the
corner triangle is the Add trigger, creating a blank row and focusing its head
cell for immediate typing instead of a bottom form bar; head/quantity cells
edit in place with an outline-only focus state so nothing shifts size; rows
are persistent per-uid DOM nodes reused across Protein pushes so a focused
edit is never clobbered by a live update.

Functionalities we Need:

- [ ] Filters?

# Communication

# Communication Sand — Implementation Plan

The Communication sand is a messaging-app surface for Lince. Every
conversation — with one user of your organ, users of other organs, or a mixed
group — is a normal Record with the full thread/message system, and any
conversation can additionally carry an audio and/or video room
(Discord-style: a room you activate and join, not a phone call you dial).

Core stance: **this is mainly a Record with audio/video room capabilities.**
The Communication sand never invents a chat system. It imports into places as
a **group together with the Record sand** (same one-product pattern as
kanban + record_info): Communication owns the list and the room; Record owns
threads, messages, and `@slug` references.

Everything below is the implementation order. A stage's title checkbox is
ticked only when all its inner checkboxes are ticked and its selftest passes.

## - [ ] S0 — Standing agreements (respected by every stage)

- [ ] A conversation is a normal Lince Record; it is the durable
      communication object (head, body, participants, group links, threads,
      messages, call sessions, recording refs, transcript refs).
- [ ] Conversations are discovered by tag (`@communication` by default, any
      chosen `@slug`); no special conversation table.
- [ ] Audio/video is an activation mode of a conversation, never a separate
      product; a call never exists without a conversation Record.
- [ ] Room semantics are Discord-like: Record = durable room, session =
      one occupancy, stored as a child Record.
- [ ] Official sand, built on the table template, Protein/Actions only, one
      WS per board, chrome state lives in host state.
- [ ] Rust owns the durable control plane (model, authorization, room state,
      Karma consequences, leases, recording metadata, message creation,
      audit, provider tokens). Browser JS owns browser-only edges (device
      permissions, WebRTC/provider SDK, local device state, media layout).
- [ ] Karma never broadcasts vague commands; it creates typed, targetable,
      claimable call intents.
- [ ] Recording is explicit and visible; silent automatic recording never.
- [ ] Each UI/backend stage lands with a driven chromium selftest before the
      next stage starts.

## - [ ] S1 — Vocabulary and model definition (docs before schema)

- [ ] Define the conversation Record shape in docs: an ordinary or identity
      `@communication` assertion first; do not introduce a dedicated
      `conversation` kind merely for filtering.
- [ ] Define the conversation assertion convention: a unary identity or
      ordinary assertion using `@communication`; any Record can be promoted to
      a conversation by asserting it.
- [ ] Define assertion predicates: `@participant` (→ person/user Records, local or
      remote organ), `group-of` (→ group Record), `call-session-of`
      (session → conversation), `call-recording` / `call-transcript`
      (session → media/transcript refs). Threads/messages stay exactly as
      the Record sand already does them.
- [ ] Define the `communication.v1` record extension:

      ```json
      {
        "namespace": "communication.v1",
        "provider": "native-webrtc" | "livekit" | "jitsi" | "mediasoup",
        "room_id": "stable room identifier",
        "room": {
          "state": "idle" | "active",
          "media": "audio" | "audio+video",
          "occupants": ["advisory mirror of live state"],
          "session_record_id": null
        },
        "controller_sand_uid": "widget instance bound as room controller",
        "recording_policy": "manual" | "karma" | "disabled"
      }
      ```

- [ ] Define the `call_session` child Record:
      `head = "Call · <timestamp>"`, sidecar `{ started_at, ended_at, media,
      peak_participants, recording: { state: idle|recording|processing|
      available|failed } }`, linked `call-session-of` → conversation.
- [ ] Call intents (Karma-driven room control) are deferred entirely to the
      Far-Future part; their shape and machinery live in F2, not here. Near-
      term stages carry zero intent plumbing.
- [ ] Rule: recording files are resource references (object storage or local
      `/host/media`), never blobs in `record.body`; Lince stores metadata,
      hash, duration, owner, retention, access policy.

## - [ ] S2 — Store layer

- [ ] Read/write helpers for the `communication.v1` extension.
- [ ] Query: Records carrying a given tag, ordered by newest activity
      (last message / last session), with last-message preview and
      participant links resolved in one shot for the list view.
- [ ] Create/close `call_session` Records and their links atomically with
      room state transitions.
- [ ] Per-crate store tests for tag query and session lifecycle. (Call-intent
      claim races belong to F2, not this stage.)

## - [ ] S3 — Actions and authorization (nucleus/engine)

- [ ] Typed actions: `conversation-create` (participants from this organ +
      connected organs + groups; creates Record, tags `@communication`,
      links participants), `room-bind-controller`, `room-open`, `room-join`,
      `room-leave`, `room-close`, `recording-start`, `recording-stop`.
      (`call-claim-intent` is Karma machinery — deferred to F2.)
- [ ] Message sending is NOT duplicated here — it stays the Record-sand
      message action.
- [ ] Participant authorization: only linked participants (or group members)
      may join a room; cross-organ identity resolved through the organ
      network.
- [ ] Room state machine enforced server-side: idle → active on first join
      (opens a session Record), active → idle on last leave/close (closes
      the session Record).
- [ ] Provider tokens (when a provider exists) are minted host-side; never
      stored in `widgetState`.

### S3 landed code

The handler logic already lives in `crates/engine/src/communication.rs`
(an `impl Engine` block: `communication_create`, `communication_bind_controller`,
`communication_join`, `communication_leave`, `communication_close`,
`communication_recording`, plus `require_communication_participant` and the
local `comm_resolve` / `comm_annotate` helpers). Only the wiring below belongs
in `actions.rs`, which is being refactored by the transfer work — paste it when
that file is green again.

- [ ] `Action` enum variants (in the `#[serde(tag="action", rename_all="kebab-case")]`
      enum near the top of `actions.rs`):

      ```rust
      ConversationCreate {
          head: String,
          #[serde(default = "default_communication_tag")]
          tag: String,
          #[serde(default)]
          participants: Vec<crate::communication::ParticipantRef>,
          #[serde(default)]
          groups: Vec<String>,
      },
      RoomBindController { conversation: String, controller_sand_uid: String },
      RoomJoin {
          conversation: String,
          #[serde(default = "default_room_media")]
          media: String,
      },
      RoomLeave { conversation: String },
      RoomClose { conversation: String },
      RecordingStart { conversation: String },
      RecordingStop { conversation: String },
      ```

      with the two serde defaults as free fns in `actions.rs`:

      ```rust
      fn default_communication_tag() -> String { "communication".into() }
      fn default_room_media() -> String { "audio".into() }
      ```

      (`room-open` is folded into `room-join`: the first join opens the session.
      A distinct `RoomOpen` variant delegating to `communication_join` can be
      added if the sand needs to pre-open a room without joining.)

- [ ] Dispatch arms (in the `match action { … }` in `act_at`):

      ```rust
      Action::ConversationCreate { head, tag, participants, groups } => {
          self.communication_create(&head, &tag, &participants, &groups, actor, now, &mut outcome).await?;
      }
      Action::RoomBindController { conversation, controller_sand_uid } => {
          self.communication_bind_controller(&conversation, &controller_sand_uid, actor, now, &mut outcome).await?;
      }
      Action::RoomJoin { conversation, media } => {
          self.communication_join(&conversation, &media, actor, now, &mut outcome).await?;
      }
      Action::RoomLeave { conversation } => {
          self.communication_leave(&conversation, actor, now, &mut outcome).await?;
      }
      Action::RoomClose { conversation } => {
          self.communication_close(&conversation, actor, now, &mut outcome).await?;
      }
      Action::RecordingStart { conversation } => {
          self.communication_recording(&conversation, true, actor, now, &mut outcome).await?;
      }
      Action::RecordingStop { conversation } => {
          self.communication_recording(&conversation, false, actor, now, &mut outcome).await?;
      }
      ```

- [ ] Engine test (new `crates/engine/tests/communication.rs`): create a
      conversation, bind a controller, join twice (room goes active, one
      session, peak = 2), leave twice (last leave closes the session, room
      idle), and assert a non-participant actor is rejected by
      `require_communication_participant`.

## - [ ] S4 — Sand group scaffold

Landed code (client written; server verification blocked on the engine build):
`crates/web/src/sand/communication/mod.rs` (package builder) +
`communication.html` (the sand), registered in `crates/web/src/sand/mod.rs`
(catalog + `build_communication_group_archive` beside a Record sand +
`render_official_groups` wiring + a group test mirroring kanban/relations). The
list Protein needs no new server source — it is
`{ source: "record", where: [{ assertion: { predicate: "tagged", direction: "out", object: <record> } }],
include: { assertions: { predicates: ["participant"] }, threads: { messages_limit: 1 },
extension: { namespace: "communication.v1" } } }`. The chromium selftest and
`cargo test -p web` stay unticked until the engine compiles (web → engine).

- [x] Official `communication` sand on the table template; registered like
      the other official sands.
- [x] Imports into places as a **group by default: Communication sand +
      Record sand**, maintained as one product (kanban + record_info
      pattern). Record sand handles all threads/messages for whatever
      conversation is selected.
- [x] One WS per board; Communication subscribes via Protein for list
      updates, room state, occupants, and new-message signals (one
      `subscribeProtein("communication", …)` over the shared bridge; recording
      state rides the same rows via the `communication.v1` extension). Pending
      call intents belong to F2.
- [ ] `GET contract` returns tag filter, conversation list, provider,
      allowed actions, participant lists, controller binding, recording
      policy. (Client reads list/room from the Protein rows already; a distinct
      contract endpoint for provider/ICE is only needed at S7.)
- [x] Chrome/host state persists: chosen tag, selected conversation, view
      mode (`patchCardState({ communication: { tag, selected, view } })`,
      re-adopted via `onCardState`). No credentials ever in host state.
- [ ] Selftest: group imports, contract loads, empty list renders.

## - [ ] S5 — List view (no media)

Landed in `communication.html` (needs the engine build + a driven page to tick
the selftest). Participant remote-vs-local badge and the unread indicator have
placeholder wiring (`to_remote` flag, `unread:false`) pending the SD-open
unread-storage decision and the Protein exposing a participant's origin organ.

- [x] Tag selector: defaults to `@communication`, switchable to any `@slug`;
      list = Records carrying that tag, newest activity first (sorted client
      side by newest message/created time).
- [x] Row contents: head, participant names, last-message preview, unread
      indicator (local-vs-remote badge + unread are placeholder until origin
      organ is on the row and unread storage is decided — SD).
- [x] Row click does BOTH: emits `recordClicked` so the grouped Record sand
      opens the conversation's threads/compose, AND switches the
      Communication sand itself into the deeper room mode (S6) for that
      Record.
- [x] "New conversation" flow: `conversation-create` action → opens it. (v0
      creates an empty tagged conversation; the participant/group picker UI is
      a follow-up — the action already accepts `participants`/`groups`.)
- [ ] Selftest: tag listing, row click drives Record sand + deep mode,
      conversation creation.

## - [ ] S6 — Room mode, call view shell (no media yet)

Landed in `communication.html`; the S9 view-switch/call-bar/idle-auto-hide
skeleton also landed early since it is the same UI surface (see S9). Server
verification waits on the engine build.

- [x] Deeper mode of the Communication sand per conversation: the **call
      view**. Back affordance returns to list view.
- [x] Room configuration controls in call view: media mode
      (audio / audio+video), provider, recording policy, controller
      binding — controller binding writes `communication.v1` via
      `room-bind-controller` on open; media/provider/recording selects are
      wired to state (persisting them through an action is a small follow-up).
- [x] Room state plumbing end to end: `room-join` / `room-leave` /
      `room-close` call the S3 actions (which mutate the extension + session
      Records); live occupant strip renders from the Protein rows. A "room"
      you can be in silently — no media. (`room-open` folded into first join.)
- [x] List view shows a live-room strip on active rows: who is in, audio or
      video, with **Join audio** / **Join video** buttons directly on the
      row (join without leaving list view).
- [ ] Record sand (later, optional): small "call active — join" banner when
      the viewed Record has an active room.
- [ ] Selftest: open/join/leave/close transitions, session Record writes,
      occupant strip updates, list-row join buttons.

## - [ ] S7 — Media v0: one-to-one audio (pure JS + Rust signaling)

- [ ] WebRTC signaling messages ride the existing board WS (transport
      crate); Rust relays offers/answers/ICE between authorized occupants
      only.
- [ ] `getUserMedia` audio; 1:1 `RTCPeerConnection`; mute/unmute; device
      picker. Local ephemeral UI state (muted, device, levels) stays in the
      browser.
- [ ] STUN/TURN configuration surface (usually `coturn`) host-side; sand
      receives ICE servers from the contract, never hardcodes them.
- [ ] Joining audio from the list row (S6 buttons) actually connects.
- [ ] Selftest: two driven pages join the same room and exchange a
      connected peer state (media-level assertions as far as headless
      allows).

## - [ ] S8 — Video and the participant grid

- [ ] `getUserMedia` video; camera toggle independent from audio.
- [ ] **Participant grid**: one tile per participant; a participant's
      camera and (later) screenshare are tiles within the grid, divided per
      participant. Responsive layout, active-speaker highlight, local
      preview tile.
- [ ] Small-mesh support (2–4 peers) with the honest limits documented:
      mesh scales poorly; group scale waits for S13.
- [ ] Selftest: grid renders N tiles for N occupants; camera toggle updates
      tile state.

## - [ ] S9 — Call-mode UX: two views, one call

The UX shell landed early with S6 (same surface). The `media` seam keeps the
call alive independent of the view, so these hold once S7 fills real tracks;
"audio plays / video paused in list view" needs the real track handles from S7
to fully honor. Selftest waits on the engine build + a driven page.

- [x] View toggle while a call is live: **call view** (grid + controls) ⇄
      **list view** (the conversation list). The call persists across the
      toggle — the `media` object is view-independent, switching never
      disconnects.
- [x] In list view during a call: a persistent slim **call bar** shows the
      active room, mute, and a jump-back-to-call-view button (`#call-bar`,
      shown via `body.in-call.view-list`). Audio-keeps-playing /
      video-paused is enforced once S7 owns the real tracks.
- [x] In call view: after a period of no interaction (mouse/keyboard/touch),
      **all controls auto-hide to maximize screen space** (`controls-hidden`
      class, `IDLE_MS` timer); any interaction brings them back. Auto-hide
      only in call mode + call view.
- [x] `recordClicked` still works during a call: browsing other
      conversations in list view drives the Record sand without touching
      the live call (`openConversation` emits `recordClicked`; the `media`
      object is untouched by view/selection changes).
- [ ] Selftest: toggle during simulated call keeps room membership; controls
      hide on idle timer and return on interaction.

## - [ ] S10 — Screen sharing

- [ ] `getDisplayMedia` share; the screenshare enters the grid as another
      tile of that participant (per S8's per-participant division).
- [ ] Share always needs explicit user activation; stop-share affordance
      always visible even when controls are auto-hidden (or shown on the
      call bar).
- [ ] Selftest: share tile appears/disappears in the grid.

## - [ ] S11 — Recording (prototype) and artifacts

- [ ] Manual local recording via `MediaRecorder` as the prototype path;
      explicit start/stop, visible recording indicator to all occupants.
- [ ] Artifact flow: session Record → `recording.state` transitions →
      recording Record/resource ref created → linked `call-recording` → a
      message lands in the conversation's thread (available / failed /
      deleted), so room history reads like any conversation history.
- [ ] Transcripts, summaries, decisions, action items are messages or
      linked Records, never hidden provider state.
- [ ] Selftest: recording lifecycle writes the session sidecar, link, and
      thread message.

# Far-Future part (after all near-term stages above)

Everything in this part is deferred until the near-term stages (S0–S11) are
landed and proven. Karma-driven interactions are the very last thing built,
after the provider/scale decision.

## - [ ] F1 — Group scale and provider decision

- [ ] Decide the production media path once mesh limits bite: continue
      Rust-native (`webrtc-rs` / `str0m` / mediasoup-rust — most aligned,
      but an SFU + recording pipeline is a large subsystem; do not
      hand-roll a production group recorder), or embed **LiveKit**
      (pragmatic first candidate: Apache-2.0 SFU, self-host, tokens, SDKs,
      screen share, Egress recording; Go server), or Jitsi/Jibri
      (Apache-2.0, heavier), or mediasoup (ISC, low-level). Janus is
      GPL-3.0 — only with explicit license acceptance.
- [ ] If a provider is embedded: server-side recording replaces the S11
      prototype path; host mints provider tokens; the S8 grid and S9 UX are
      preserved on top of the provider SDK.
- [ ] License rule: vendored client/embedded assets keep their
      LICENSE/NOTICE/credit files beside the sand package, bundled with the
      widget assets.

## - [ ] F2 — Karma call intents (last of all)

- [ ] Define the call intent shape (rides the existing `action_intents`
      machinery): `conversation_record_id`, `target_controller_sand_uid`,
      `action` (`room.open` | `room.close` | `recording.start` | …),
      `source`, `status`, `nonce`, lease fields, payload. No new intent table.
- [ ] Store: reuse `action_intents` for call intents (target matching, atomic
      claim + lease, idempotence by nonce, completed/failed writes) with a
      store test for claim races.
- [ ] Action: `call-claim-intent` (a sand claims a pending intent for its
      bound conversation + `controller_sand_uid`).
- [ ] Karma consequences create call intents instead of direct UI commands:
      e.g. "at 10:00 create `room.open` for controller X", "when Transfer Y
      reaches agreed, create `room.close` for its room".
- [ ] Claim rules enforced: a sand claims only intents for its bound
      conversation and its `controller_sand_uid`; atomic claim + lease; one
      winner; idempotent by nonce; completion writes completed/failed.
- [ ] Selftest: two controller instances race a claim; loser observes
      claimed state; intent completes.

## - [ ] SD — Decisions still open (resolve before the stage that needs them)

- [ ] Cross-organ signaling authority for a mixed-organ room: the
      conversation Record's origin organ? (needed by S7)
- [ ] Recordings local to one organ or syncable between organs? (S11)
- [ ] Screen share recordable by default or separate consent? (S10/S11)
- [ ] Retention policy for recordings and transcripts. (S11)
- [ ] Unread/"last read" per user per conversation: sidecar, link property,
      or host state? (S5)
- [ ] Auto-hide idle delay and whether it is user-configurable. (S9)

## References

- Local: `docs/Sand: Index.md`
- Canonical Karma architecture: `docs/Karma.md`
- Cross-cutting standing laws: `docs/Maneirisms.md`
- Local: `crates/web/src/sand/record/record.html` (thread/message surface)
- Local: `crates/store/src/action_intents.rs` (intent/lease machinery)
- MDN: `RTCPeerConnection`, `MediaRecorder`, `getDisplayMedia`
- LiveKit docs: Egress overview and screen sharing; LiveKit / Jitsi+Jibri:
  Apache-2.0; mediasoup: ISC; Janus: GPL-3.0

# 2D Map

- [ ] Take the location of Records and/or people, display them in a 2d map in real time, synced between organs.

# Ergon

Ergon is the enduring, physical manifestation of our conscious actions that shapes both our world and our own evolution. Born from the Proto-Indo-European root *wérǵom, it represents the primordial energy of bringing reality into being through purposeful creation. Though long degraded by ruling elites as the mindless toil of the unfree, it is truly the highest form of conscious practice—a liberating force that allows a species to reclaim its creative output and consciously co-create its destiny with nature. - Gemini.

The coordination of production for our Needs requires specific interfaces? We will know when time comes. Possible future Needs not covered by future sands are:
- [ ] Transparent stock control
- [ ] Logistic distribution and instant correction from a flicker of operational change of the brute mineral extractor to the chip manufacturer.
- [ ] Order management, how much requests affect production.

# Playground Facade

- [x] Be able to setup a Web workspace and export it in a file.html as an archive of the state of a component at a time. It doesnt make any requests, has no access tokens.
- [ ] Be able to export something that can still make some types of request, like the GET of proteins.
- [ ] If we integrate with some payment system, we can even do some buy/sell process.
- [ ] Make an online shop for the Lince Institute with JIT production, Needs are created/assigned when an order arrives.
    - [ ] T-shirts
    - [ ] Stickers
    - [ ] 3D Keychain Accessory
    - [ ] Hoodies

# Configuration

The sand exists, to configure normal lince data. We need to make it expand to configure more things. The sand will be the door to configure database stuff and board settings, like:
- [ ] Colorscheme (with examples).

## Lince palette reference

Roxo Cobalt — CMYK: 66, 71, 0, 36; HEX: `#3730A3`; RGB: 55, 48, 163.
Roxo Noturno — CMYK: 59, 58, 0, 5; HEX: `#6366F1`; RGB: 99, 102, 241.
Chumbo Profundo — CMYK: 10, 10, 0, 92; HEX: `#121214`; RGB: 18, 18, 20.
Cinza — CMYK: 10, 5, 0, 12; HEX: `#A7B4C2`; RGB: 203, 213, 225.
Branco Gelo — CMYK: 2, 1, 0, 1; HEX: `#F8FAFC`; RGB: 248, 250, 252.

## Customization and architecture

- [ ] In Web Interface, user can control all the basic aspects of the ui, the padding, margin gap of elements, border radius, thickness and colorscheme. In web version there should not be even one color hardcoded, only use tags like primary-background, or light-accent. The default style should come from the main style .css file, that has comments on every variable to explain where it is used, so when people make their .css files and add to dir of styles and choose in configuration table which style they want (name of file) they get the variables values from file and the app changes (either on boot if makes app faster or during setting). When we speak of specific details of style here like default colorscheme and scale units we are talking about default file, if people want they can customize it.
- [ ] Architecture (from Sand: Colorschemes): The system is defined as named semantic tokens, not hex values — surface-raised, ink-primary, need, contribution, accent, focus — resolved per active colorscheme at runtime (the old a2 Operation to switch schemes is the spiritual ancestor). Scaling tokens (padding-s, radius-m) ride the same mechanism. A Sand author never picks a color; they name a slot, and the user's scheme decides what it looks like. That's how "the base app is minimalist so users can express themselves" survives contact with real widgets.
- [ ] The default style is always loaded first and defines every variable. User style files are optional overrides: when a variable is absent, the value from the default style remains. Every variable in the default file has a comment explaining its use.
- [ ] Style files live in the Web styles directory and are selected by safe `.css` filename only. Invalid, missing, or unreadable files fall back to the default without preventing the app or a Sand from loading.
- [ ] Styles load in this order: default style, configured global style, per-Sand style. The global style is stored in the configuration table. A per-Sand style is stored in that card's host state and overrides only that Sand; choosing "inherit global style" removes the override.
- [ ] The configuration table selects the global style and applies it immediately. The Sand gear configuration, together with login, Protein, and behavior, shows the inherited global style and selects an optional style for that Sand. Both choices persist.
- [ ] Every Sand iframe loads the style layers itself because CSS variables from the board do not cross the iframe boundary. Changing the global style updates Sands that inherit it without replacing a Sand's own override.
- [ ] The colorscheme has 16 semantic color slots, each with light, default, and dark values, for 48 color variables: primary-background, secondary-background, raised-background, primary-ink, secondary-ink, border, accent, focus, need, contribution, peace, info, success, warning, danger, and selection. The default Lince style may repeat colors between slots and tones; custom styles may define all 48 for finer control. LynxUI does not map backend data or a badge type to those slots; a Sand may opt into a local mapping at its own boundary.
- [ ] The default colorscheme is Lynx with Dark Lynx as its initial mode and Light Lynx as its inverse. Dark Lynx uses Chumbo Profundo for the background and Branco Gelo for primary characters; Light Lynx reverses them. A single icon button switches mode immediately; both modes use the same scale, geometry, and component rules.
- [ ] Lynx derives close neutral steps from Chumbo Profundo and Branco Gelo. Default component backgrounds remain Chumbo or Branco; the 10% lighter and darker variants are used only for subtle inputs and diffuse shadows. Default content is void-background with white or gray foreground; it does not assign red, green, amber, or any semantic color to data. Roxo Cobalt is the primary accent in Light Lynx and Roxo Noturno is the primary accent in Dark Lynx for stronger contrast; the other purple is the supporting accent.

## Space, thickness, roundness, rigidity

- [ ] Grid: 4px base unit; spacing scale 4, 8, 12, 16, 24, 32, 48. Compact component internals may use the 2px half-unit.
- [ ] Density: slim by default. Main content regions touch with no decorative gaps or outer padding. Component padding is half the previous demo spacing. Records use 5px internal padding; form controls use 3px vertically and 5px horizontally so empty space does not exceed the text height.
- [ ] Keep a compact 4px gap between text and adjacent metadata in the same compartment, such as a column name and its card count.
- [ ] Borders: use 0.5px hairlines only when needed to show where one region ends and another starts. `.lynx-shadow` is a small tokenized CSS shadow on an individual component’s own root; it follows that element’s shape and radius exactly, has no JavaScript or wrapper cost, and may be locally directed with `--lynx-shadow-x`, `--lynx-shadow-y`, and `--lynx-shadow-blur`. Its default is dark and falls to the right and bottom. `.lynx-shadow--light` adds an optional lighter top-left companion. Buttons, Kanban cards, Kanban column headers, and the message channel-list boundary opt into the dark shadow. Inputs, textareas, and dropdown controls are flat with a foreground border. Outer Sands and large workflow regions do not use shadows. Adjacent regions share one boundary; no double borders and no boxes inside boxes. Sand outer edges have no border by default on the free canvas. Stronger focus and active state remain distinct from ordinary borders.
- [ ] Roundness: Lynx is square by default. Badges and buttons use a restrained 2px radius. Custom styles may change the radius tokens.
- [ ] Badge is the component name and keeps the `.lynx-status` class/API. It has a transparent background and primary foreground hairline border by default; its optional icon uses the same color. The component does not interpret data as good, bad, warning, or any other semantic hue. A Sand may locally set `--status-color` when its own domain calls for it.
- [ ] Invalid inputs keep the error-color border and show an in-field error icon. Hovering or focusing that icon reveals the validation message in a transparent, error-color outlined tooltip; the message also remains associated with the input for assistive technology.
- [ ] Buttons use a subtle 2px radius. Use familiar, distinct action icons such as plus, check, close, save, download, and trash; keep text when the icon alone is ambiguous.
- [ ] Radio controls are minimal circles filled completely with the accent when selected. Dropdowns, selects, and disclosures use one small chevron treatment. LynxUI selects use the library menu instead of a browser-styled popup.
- [ ] Rigidity: "firm paper." Cards hold their shape with crisp hairline borders; nothing bounces, nothing elastic.

## Content and Sand chrome

- [ ] Prefer content directly on the Sand surface. A card is a card, not a floating card inside another box; a column is a column, not a box containing another padded box.
- [ ] Remove redundant headings and labels. Do not show both "Sand / Communication" and "Messages", or text beside a self-explanatory icon.
- [ ] Omit a Sand header when the content already explains itself. Keep visible controls focused on creating, editing, moving, completing, or replying to Records.
- [ ] Only the focused Sand shows its gray bottom-right corner. Hovering or focusing the corner reveals configuration, Protein, layout, and other Sand controls like the corner of a page turning.
- [ ] The base web uses an always-visible gray folded top-right corner for system controls. It holds the mode switch and development tools without adding a persistent header; Sand controls remain in their bottom-right corners.
- [ ] Do not add dividers when content already makes the boundary clear. Message identity and avatar separate consecutive messages without a line.
- [ ] Use the darker generated black-and-white step, never gray or the lighter step, for background separation. Kanban, Messages, and workflow regions share the canvas background without outer shadows; only individual components may opt into the shared shadow.
- [ ] Do not divide Kanban columns, column headers from their first card, cards, table rows, list rows, or ordinary adjacent items with lines. Use grouping, spacing, and close surfaces only when a boundary needs to be understood.
- [ ] Inputs, textareas, and closed dropdown controls use the base surface with a foreground border and no shadow. Invalid controls use the `hot-border` token, red in default Lynx. Unchecked checkboxes use the base surface; checked checkboxes use the accent.
- [ ] Hovering an element on the base surface uses the subtle 10%-lighter surface. Hover never darkens the current base surface.
- [ ] Forms, tables, lists, and adjacent workflow regions align to shared edges. Do not use spacing that makes neighboring compartment boundaries stop at different positions, and omit row or field lines when grouping is already clear. A separator belongs between items, never after the final item.
- [ ] Prefer clear icons for actions and give every icon button an accessible name and a tooltip on hover. Keep text when an icon would be ambiguous.
- [ ] Keep tooltips within the boundary of the Sand or component that owns them, and give SVG strokes enough internal view-box space that icons are never clipped.
- [ ] When the Sand is idle, the visible UI prioritizes Records, their state, and direct interaction with them rather than configuration of the Sand.

## Transparency and elevation

- [ ] Data surfaces are always opaque. Honesty rule: you must always know exactly which surface a number sits on. No glassmorphism, no frosted panels over content.
- [ ] Translucency is allowed only for ephemeral chrome: edit-mode handles, drag previews, presence cursors, auto-hiding call UI — things that are explicitly not Ledger truth.
- [ ] Overlays/scrims at 50–60% ink. Menus are opaque. Tooltips use Chumbo Profundo with Branco Gelo text and a hairline border in that same foreground color; dialogs use a Cinza border.
- [ ] Elevation is flat by default. `.lynx-shadow` is the sole shared elevation utility and is opt-in except for ordinary buttons and form controls, which use its default dark lower-right shadow. Paper doesn't hover.

## Line, shape, and texture grammar

- [ ] A deliberate second channel so color is never the only carrier: Solid = settled (Ledger facts, committed quantities). Dashed = declared (promises, projections, staged rules). This is as load-bearing as any hue.

## Typography

- [ ] Numbers first: tabular figures everywhere quantities appear, negative quantities use a true minus (−3), and zero and positive quantities have no sign (0, 5). The zero state is styled quietly — peace is the one value that should never demand attention.
- [ ] Lato is the default body and interface typeface. Aleo is used mostly for titles and semantic headings. Quantities and technical metadata keep the monospace token. EN/PT is supported from day one, with generous line lengths and no cramped all-caps labels.
- [ ] Ordinary interface text is 14px by default. Compact secondary metadata stays readable at 11–12px; do not shrink routine labels or content to create density.
- [ ] Inline icons, counts, and quantity-state marks are optically centered with the adjacent text. Correct a glyph inside its SVG when its drawing is off-center; do not move the entire control.

## Motion

- [ ] No animations, things change instantly, and they dont pulse, if something is green it is synced and ok.
- [ ] Remove interface transitions, keyframe animations, hover movement, startup drawing, workspace sliding, animated modal entrances, and JavaScript that waits for transition completion. Content owned by a Sand, such as a game or terminal, is not interface motion.

## The test

- [ ] Every design decision gets one question: does this get Ana to her four minutes of human choice faster, or is it the tool asking to be looked at? The Death of Lince applies to its UI first. If the user wants, they can use lince as an app that lets them create and interact with beautiful and cool things and produce awesome graphs and automation visualizations, but that is a choice, the default of lince is meeting your need to use apps like it with minimal effort.

## Design system work

- [ ] Migrate the board, shared components, and every official Sand from hardcoded visual values to the semantic color, spacing, border, radius, typography, elevation, and motion variables. User-owned expression colors remain data, not system chrome.
- [ ] Add a design-system check that rejects hardcoded colors in the Web interface and official Sands, excluding the default style definitions, vendored assets, and explicit user-owned expression values.
- [ ] Test default fallback, optional partial styles, safe filename handling, global persistence, per-Sand persistence and isolation, live style changes, and style loading inside iframes.
- [ ] Test quantity formatting, tabular figures, solid and dashed truth lines, and that status meaning is available through an icon, label, shape, or line style instead of color alone.

## LynxUI

- [x] Select Lynx and keep one evolving light/dark demo at [`lynx-ui-concepts/lynx.html`](lynx-ui-concepts/lynx.html). Update this design-system description whenever the demo guidelines change.
- [x] Build the LynxUI base as a framework-free component library for official Sands. Use native semantic HTML, explicit `lynx-*` classes, and a small JavaScript layer only for behavior that HTML does not provide consistently.
- [x] Serve shared `lynx-ui.css` and `lynx-ui.js` assets. LynxUI uses the design-system tokens and defines no separate colorscheme, spacing scale, motion, or elevation. Component selectors have low specificity so global and per-Sand styles can override them.
- [x] Provide Catppuccin Macchiato as the second style. Its CSS changes only colorscheme variables, uses the official Base, Mantle, Crust, Text, Subtext, Overlay, Mauve, Lavender, Red, Yellow, Green, and Blue values, and carries the Catppuccin MIT notice.
- [ ] Load styles in this order: default tokens, LynxUI, Sand structural CSS, configured global style, per-Sand style.
- [x] The first component set has buttons and button groups; inputs, textareas, selects, checks, radios, labels, help and errors; boxes, panels, stacks, rows, grids, toolbars and dividers; badges, callouts and empty states; tables, lists, dropdowns, tooltips, dialogs, tabs and disclosures; and a small first-party SVG icon set.
- [ ] Add consistent native date, time, and datetime-local fields, plus compact absolute and relative date display, for Record work metadata and Kanban card metadata.
- [ ] Add an accessible combobox/autocomplete with a suggestion list, keyboard navigation, and single-select support. Record uses it for assertion predicates and objects; Kanban can use it for column and Concept choices.
- [ ] Add a removable token picker for Record assignees, selected assertions, and thread predicates. It composes the combobox and badge instead of creating a separate data model.
- [ ] Add a file attachment primitive: picker trigger, upload/busy state, attachment row, and remove action. Record owns message attachment semantics and previews.
- [ ] Add a compact semantic metadata list (`dl`) for Record head/slug/quantity/work facts and Kanban card metadata; it is a flat key–value display, not a panel.
- [ ] Add a compact duration field for Record estimates and worklog values. Record owns its timer and worklog behavior.
- [ ] Document a destructive confirmation-dialog composition using the existing dialog, for Kanban bulk deletion and Record hard deletion.
- [ ] Add an anchored action/context menu based on the existing menu behavior, for Record links and attachments and Kanban cards.
- [ ] Add small inline loading and progress states for Record saves, uploads, and Kanban moves; transient notifications, avatars, and range sliders remain out of scope until a Sand needs them.
- [x] Static components use native markup and classes. Interactive components use `data-lynx-*` attributes and one delegated event and keyboard handler per iframe. `window.LynxUI` provides icon and icon-button helpers for dynamic elements.
- [ ] Migrate every official Sand to LynxUI for ordinary interface elements and remove duplicated component CSS. Specialized graphs, terminals, games, document rendering, canvases and artwork keep their own implementation; their ordinary surrounding controls use LynxUI when practical.
- [ ] LynxUI is official-first and initially unversioned. User-authored Sands may load the same assets, but compatibility is not promised until the API is deliberately stabilized.
- [ ] Keep `LynxDS-components.js` compatible with persisted older shell HTML while new code uses `window.LynxUI`.
- [ ] Components have keyboard navigation, focus management, ARIA state, associated help and errors, non-color status meaning, and no transitions or animations.
- [ ] Treat specialized exceptions as a code-review convention, without manifest declarations or exemption attributes.
- [x] Add a development-only LynxUI Gallery with one scrollable page: a compact showcase of all LynxUI components sits beside a stacked set of seeded Kanban, message, inventory, and request-review Sand previews at their normal board sizes. It uses canonical assets and fixture data without Protein or Action requests. `mise run lynxui` serves it at `http://127.0.0.1:6175` and recompiles the gallery package for Lince on source changes.
- [x] Add a folded base control that highlights LynxUI components and shows their component names on hover. Sand-specific structure stays unmarked so the boundary is clear.
- [ ] Add concise Sand-author documentation, component behavior tests, iframe tests, theme override tests, and checks that LynxUI has no hardcoded design colors, unauthorized shadows, transitions, or animations.

<!-- - [ ] Chart Library -->
<!-- - [ ] Be able to draw (arrows, boxes, text and erasing at first is ok). If we fill the space with too much stuff in lets say, svg it will at some point if the person is making a complext diagram or making a painting frame around their component it will get heavy and make the app slow. We must solve that problem, excalidraw does that really well, putting a lot of drawings on the screen, lots of elements, doesnt make it slow. How do they do it? what do they implement? -->




--- End of Supercomponent area ---

Down here is stuff not related to board sands and supercomponent hocus pocus, that is simple software development, here things start to get a higher quality, unlocking different devices and technologies to do normal things in a different way, or going plus ultra.

# Mobile

Its hard making one repo for all platforms, but worth since mobile is so useful.

# Embeded

We need a version of lince to be wearable in a simple way, we can devise an esp with bluetooth, screen and batery and make a minimal version with a specific tui or something even simpler, it will be able to:
- [ ] CRUD organ
- [ ] Login to organ
- [ ] Select protein from organ to view data
- [ ] Look at records in a list
- [ ] Control quantity of a record simply, like -1, 0, 1

It can be done harcoding all of that. So i will be able to hardcode with .env that there will be an organ at such endpoint and such login and such protein to choose from, the device wakes up, and uses the Sync feature of Lince to edit the records they can see only the quantity according to the protein they can see.


# The Game of Life - Digital Real World Maps

Being able to see the world or a digital space with it's actors and needs/contributions.
- [ ] One can see the world as a plane with lines for the streets.
- [ ] Bonus points for terrain data, elevation, like mountains. With that in rendering we can portrait a more accurate picture of the world and also use the elevation to show the Needs and Contributions in a 3d way. If there are a lot of Needs in one area that is like a mountain visually.
- [ ] Integrate that with Transfer Proposal. Being able to accompany the whole process through the maps, like a delivery; understanding who is closest to Contribute to your Need.

https://github.com/orgs/Far-Beyond-Pulsar/discussions/40

Maybe the way to go is using a game engine in gpui like Pulsar if it allows for the rendering of a Component in a canvas or something similar to display like a game level.

GPU can be used for highly efficient rendering. That can be used from finantial spreadhseets, to immersive visualization of Records across real world maps and more, this interface is for bulky rendering.

But if we are going to those lenghts, why not code it like a game already? Games are fun.

What if we could make our Records be part of the game? From influencing the seed to a real live preview of them as parts of the landscape with bigger mountains for bigger quantities of a certain record, to becoming enemies we Need to defeat. What if Karma could be used for the rules of the game? evaluating as frequently as possible

