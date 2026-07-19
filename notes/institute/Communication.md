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

- [ ] Define the conversation Record shape in docs:
      `kind = "plain"` + `@communication` tag first; a dedicated
      `conversation` kind only if tag filtering proves insufficient.
- [ ] Define the tag convention: link from conversation Record to the
      `communication` tag Record; any Record can be promoted to a
      conversation by tagging it.
- [ ] Define link kinds: `participant` (→ person/user Records, local or
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
`{ source: "record", filter: [{ linked_to: { kind: "tagged", to: <tag> } }],
include: { links: { kinds: ["participant"] }, threads: { messages_limit: 1 },
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

- Local: `notes/institute/Sand.md`, `notes/institute/Karma.md`
- Local: `docs/new-version-capabilities-and-maneirisms.md`
- Local: `crates/web/src/sand/record/record.html` (thread/message surface)
- Local: `crates/store/src/action_intents.rs` (intent/lease machinery)
- MDN: `RTCPeerConnection`, `MediaRecorder`, `getDisplayMedia`
- LiveKit docs: Egress overview and screen sharing; LiveKit / Jitsi+Jibri:
  Apache-2.0; mediasoup: ISC; Janus: GPL-3.0
