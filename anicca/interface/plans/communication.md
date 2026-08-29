# Communication implementation plan

Purpose: Retain the detailed Record, action, media, encryption, provider, and Karma-intent stages.

Owner source: no dedicated Sands Record currently exists. Ontology and Karma
remain authoritative where linked from the specification.

Status: Separate backlog, not the active interface foundation.

Read when: Communication is explicitly assigned.

[Corpus map](../README.md) · [Current context](../current.md) · [Communication specification](../communication.md)

---

- [ ] A conversation is a normal Lince Record; it is the durable
      communication object (head, body, participants, group links, threads,
      messages, call sessions, recording refs, transcript refs).
- [ ] Conversations are discovered by tag (`@communication` by default, any
      chosen `@slug`); no special conversation table.
- [ ] Audio/video is an activation mode of a conversation or one of its
      threads, never a separate product; a call never exists without a parent
      conversation Record.
- [ ] Room semantics are Discord-like: the conversation/thread Record is the
      durable room scope and a session is one occupancy stored as a child
      Record.
- [ ] Conversation, thread, message, signaling, and media access is encrypted
      to authorized participants across Organs. Relays route ciphertext and do
      not become participants merely by transporting it.
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

- [ ] Define the conversation Record shape in docs: an ordinary or identity
      `@communication` assertion first; do not introduce a dedicated
      `conversation` kind merely for filtering.
- [ ] Define the conversation assertion convention: a unary identity or
      ordinary assertion using `@communication`; any Record can be promoted to
      a conversation by asserting it.
- [ ] Define assertion predicates: `@participant` (→ person/user Records, local or
      remote organ), `group-of` (→ group Record), `call-session-of`
      (session → conversation or thread room scope), `call-recording` / `call-transcript`
      (session → media/transcript refs). Threads/messages stay exactly as
      the Record sand already does them.
- [ ] Define the `communication.v1` record extension:

      ```json
      {
        "namespace": "communication.v1",
        "provider": "native-webrtc" | "livekit" | "jitsi" | "mediasoup",
        "room_id": "stable room identifier",
        "conversation_record_id": "root conversation Record",
        "scope_record_id": "conversation or thread Record",
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
      available|failed } }`, linked `call-session-of` → its conversation or
      thread room scope.
- [ ] Define the participant-access and group-key envelope for encrypted
      conversation, thread, message, signaling, media, recording, and
      transcript data. Thread-scoped rooms inherit or narrow access and key
      rotation follows membership changes.
- [ ] Call intents (Karma-driven room control) are deferred entirely to the
      Far-Future part; their shape and machinery live in F2, not here. Near-
      term stages carry zero intent plumbing.
- [ ] Rule: recording files are resource references (object storage or local
      `/host/media`), never blobs in `record.body`; Lince stores metadata,
      hash, duration, owner, retention, access policy.

- [ ] Read/write helpers for the `communication.v1` extension.
- [ ] Query: Records carrying a given tag, ordered by newest activity
      (last message / last session), with last-message preview and
      participant links resolved in one shot for the list view.
- [ ] Create/close `call_session` Records and their links atomically with
      room state transitions.
- [ ] Per-crate store tests for tag query and session lifecycle. (Call-intent
      claim races belong to F2, not this stage.)

- [ ] Typed actions: `conversation-create` (participants from this organ +
      connected organs + groups; creates Record, tags `@communication`,
      links participants), `room-bind-controller`, `room-open`, `room-join`,
      `room-leave`, `room-close`, `recording-start`, `recording-stop`.
      (`call-claim-intent` is Karma machinery — deferred to F2.)
- [ ] Every room action carries the root conversation and its conversation-or-
      thread scope. The currently landed conversation-only signatures below
      must be replaced before S3 can complete.
- [ ] Message sending is NOT duplicated here — it stays the Record-sand
      message action.
- [ ] Participant authorization: only members authorized for the selected
      conversation/thread scope may join or receive its keys; cross-Organ
      identity is resolved through the Organ network.
- [ ] Room state machine enforced server-side: idle → active on first join
      (opens a session Record), active → idle on last leave/close (closes
      the session Record).
- [ ] Provider tokens (when a provider exists) are minted host-side; never
      stored in `widgetState`.

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

- [ ] `GET contract` returns tag filter, conversation list, provider,
      allowed actions, participant lists, controller binding, recording
      policy. (Client reads list/room from the Protein rows already; a distinct
      contract endpoint for provider/ICE is only needed at S7.)
- [ ] Selftest: Sand imports, contract loads, and the honest empty-list state
      renders.

- [ ] Selftest: tag listing, row click drives the Record Sand + deep mode,
      conversation creation.

- [ ] Let the call view activate the whole conversation or a selected thread,
      show the effective access group before joining, and prevent a thread
      room from widening its inherited authorization.
- [ ] Record sand (later, optional): small "call active — join" banner when
      the viewed Record has an active room.
- [ ] Selftest: open/join/leave/close transitions, session Record writes,
      occupant strip updates, list-row join buttons.

- [ ] WebRTC signaling messages ride the existing board WS (transport
      crate); Rust relays offers/answers/ICE between authorized occupants
      only.
- [ ] Use end-to-end media encryption for every cross-Organ room, including
      provider/SFU paths. Transport encryption alone is insufficient when an
      intermediary can terminate it; a recorder is an explicit authorized
      participant with separately visible consent.
- [ ] `getUserMedia` audio; 1:1 `RTCPeerConnection`; mute/unmute; device
      picker. Local ephemeral UI state (muted, device, levels) stays in the
      browser.
- [ ] STUN/TURN configuration surface (usually `coturn`) host-side; sand
      receives ICE servers from the contract, never hardcodes them.
- [ ] Joining audio from the list row (S6 buttons) actually connects.
- [ ] Selftest: two driven pages join the same room and exchange a
      connected peer state (media-level assertions as far as headless
      allows).

- [ ] `getUserMedia` video; camera toggle independent from audio.
- [ ] **Participant grid**: one tile per participant; a participant's
      camera and (later) screenshare are tiles within the grid, divided per
      participant. Responsive layout, active-speaker highlight, local
      preview tile.
"- [ ] Small-mesh support" (2–4 peers) with the honest limits documented:
      mesh scales poorly; group scale waits for F1.
- [ ] Selftest: grid renders N tiles for N occupants; camera toggle updates
      tile state.

- [ ] Selftest: toggle during simulated call keeps room membership; controls
      hide on idle timer and return on interaction.

- [ ] `getDisplayMedia` share; the screenshare enters the grid as another
      tile of that participant (per S8's per-participant division).
- [ ] Share always needs explicit user activation; stop-share affordance
      always visible even when controls are auto-hidden (or shown on the
      call bar).
- [ ] Selftest: share tile appears/disappears in the grid.

- [ ] Manual local recording via `MediaRecorder` as the prototype path;
      explicit start/stop, visible recording indicator to all occupants.
- [ ] Artifact flow: session Record → `recording.state` transitions →
      recording Record/resource ref created → linked `call-recording` → a
      message lands in the conversation's thread (available / failed /
      deleted), so room history reads like any conversation history.
- [ ] Encrypt recording and transcript artifacts to their authorized access
      group and rotate future artifact keys when that group changes.
- [ ] Transcripts, summaries, decisions, action items are messages or
      linked Records, never hidden provider state.
- [ ] Selftest: recording lifecycle writes the session sidecar, link, and
      thread message.

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
- [ ] Reject a provider path that cannot preserve end-to-end media encryption
      for group calls. Server-side recording is possible only by admitting a
      visible, explicitly authorized recorder participant for that session.
- [ ] License rule: vendored client/embedded assets keep their
      LICENSE/NOTICE/credit files beside the sand package, bundled with the
      widget assets.

- [ ] Define the call intent shape (rides the existing `action_intents`
      machinery): `conversation_record_id`, `scope_record_id`,
      `target_controller_sand_uid`,
      `action` (`room.open` | `room.close` | `recording.start` | …),
      `source`, `status`, `nonce`, lease fields, payload. No new intent table.
- [ ] Store: reuse `action_intents` for call intents (target matching, atomic
      claim + lease, idempotence by nonce, completed/failed writes) with a
      store test for claim races.
- [ ] Action: `call-claim-intent` (a sand claims a pending intent for its
      bound conversation/scope + `controller_sand_uid`).
- [ ] Karma consequences create call intents instead of direct UI commands:
      e.g. "at 10:00 create `room.open` for controller X", "when Transfer Y
      reaches agreed, create `room.close` for its room".
- [ ] Claim rules enforced: a sand claims only intents for its bound
      conversation/scope and its `controller_sand_uid`; atomic claim + lease;
      one winner; idempotent by nonce; completion writes completed/failed.
- [ ] Selftest: two controller instances race a claim; loser observes
      claimed state; intent completes.
