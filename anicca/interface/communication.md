# Communication Sand

Purpose: Specify conversation Records, messaging presentation, room semantics, media, encryption, and staged delivery.

Owner source: no dedicated Sands Record currently exists. Ontology and Karma
remain authoritative where linked below.

Status: Separate Sand backlog; it does not gate the active interface refactor unless explicitly assigned.

Read when: implementing Communication rather than general Sand composition.

[Corpus map](README.md) · [Current context](current.md) · [Communication plan](plans/communication.md)

---

### Communication

The Communication sand is a messaging-app surface for Lince. Every
conversation — with one user of your organ, users of other organs, or a mixed
group — is a normal Record with the full thread/message system, and any
conversation can additionally carry an audio and/or video room
(Discord-style: a room you activate and join, not a phone call you dial).

Core stance: **this is mainly a Record with audio/video room capabilities.**
The Communication Sand never invents a chat system. It owns the conversation
list and room surface; the one pinned Record Sand owns threads, messages, and
`@slug` references. Communication ships ungrouped and drives that Record Sand
through board-scoped `recordClicked` and `recordCreate` events. This replaces
the earlier design that bundled another Record Sand into each workflow.

A conversation access group may contain several People from several Organs.
Its durable conversation/thread/message content and its live-room key material
are encrypted to the authorized group. A room can be activated for the whole
conversation or for one thread. A thread room remains inside its conversation,
inherits or narrows the thread's access set, and cannot widen access beyond the
conversation merely by becoming a call.


Everything below is the implementation order. A stage's title checkbox is
ticked only when all its inner checkboxes are ticked and its selftest passes.

#### [ ] S0 — Standing agreements (respected by every stage)

#### [ ] S1 — Vocabulary and model definition (docs before schema)

#### [ ] S2 — Store layer

#### [ ] S3 — Actions and authorization (nucleus/engine)

##### S3 landed code

The handler logic already lives in `crates/engine/src/communication.rs`
(an `impl Engine` block: `communication_create`, `communication_bind_controller`,
`communication_join`, `communication_leave`, `communication_close`,
`communication_recording`, plus `require_communication_participant` and the
local `comm_resolve` / `comm_annotate` helpers). Only the wiring below belongs
in `actions.rs`, which is being refactored by the transfer work — paste it when
that file is green again.

#### [ ] S4 — Sand surface scaffold

Landed code (client written; server verification blocked on the engine build):
`crates/web/src/sand/communication/mod.rs` (package builder) +
`communication.html` (the Sand), registered in `crates/web/src/sand/mod.rs`.
The earlier grouped package builder was removed: the official catalog entry is
now a single ungrouped Sand so it can reach the pinned Record surface. The list
Protein needs no new server source — it is
`{ source: "record", where: [{ assertion: { predicate: "tagged", direction: "out", object: <record> } }],
include: { assertions: { predicates: ["participant"] }, threads: { messages_limit: 1 },
extension: { namespace: "communication.v1" } } }`. The chromium selftest and
`cargo test -p web` stay unticked until the engine compiles (web → engine).

- [x] Official `communication` sand on the table template; registered like
      the other official sands.
- [x] Imports as one ungrouped Communication Sand. Board-scoped events drive
      the one pinned Record Sand, which handles threads/messages for whichever
      conversation is selected.
- [x] One WS per board; Communication subscribes via Protein for list
      updates, room state, occupants, and new-message signals (one
      `subscribeProtein("communication", …)` over the shared bridge; recording
      state rides the same rows via the `communication.v1` extension). Pending
      call intents belong to F2.
- [x] Chrome/host state persists: chosen tag, selected conversation, view
      mode (`patchCardState({ communication: { tag, selected, view } })`,
      re-adopted via `onCardState`). No credentials ever in host state.
#### [ ] S5 — List view (no media)

Landed in `communication.html` (needs the engine build + a driven page to tick
the selftest). Participant remote-vs-local badge and the unread indicator have
placeholder wiring (`to_remote` flag, `unread:false`) pending the settled
private per-Person last-read state and Protein exposing a participant's origin
Organ.

- [x] Tag selector: defaults to `@communication`, switchable to any `@slug`;
      list = Records carrying that tag, newest activity first (sorted client
      side by newest message/created time).
- [x] Row contents: head, participant names, last-message preview, unread
      indicator (local-vs-remote badge + unread are placeholder until origin
      Organ is on the row and private last-read state is implemented).
- [x] Row click does BOTH: emits `recordClicked` so the pinned Record Sand
      opens the conversation's threads/compose, AND switches the
      Communication sand itself into the deeper room mode (S6) for that
      Record.
- [x] "New conversation" flow: `conversation-create` action → opens it. (v0
      creates an empty tagged conversation; the participant/group picker UI is
      a follow-up — the action already accepts `participants`/`groups`.)
#### [ ] S6 — Room mode, call view shell (no media yet)

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
#### [ ] S7 — Media v0: one-to-one audio (pure JS + Rust signaling)

#### [ ] S8 — Video and the participant grid

#### [ ] S9 — Call-mode UX: two views, one call

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
#### [ ] S10 — Screen sharing

#### [ ] S11 — Recording (prototype) and artifacts

#### Far-future communication work

Everything in this part is deferred until the near-term stages (S0–S11) are
landed and proven. Karma-driven interactions are the very last thing built,
after the provider/scale decision.

##### [ ] F1 — Group scale and provider decision

##### [ ] F2 — Karma call intents (last of all)

##### Settled call decisions

For a mixed-Organ room, the conversation Record's origin Organ is the
signaling authority. Recording bytes remain at the producing storage owner;
only access-controlled metadata and resource references replicate. Starting a
screen share does not consent to recording it: capturing the share requires a
separate visible consent each time. A recording cannot begin until its
inherited or explicit retention policy is shown, and a missing policy does not
silently mean permanent retention.

Unread/last-read position is durable private per-person state that can follow
the person across their devices; it is not shared conversation truth and not
Cell-local Box state. Call controls auto-hide after three idle seconds by
default, the value is user-configurable, and reduced-motion settings do not
disable the hide itself because it is an instantaneous visibility change.

#### References

- Canonical Karma architecture: [Karma in Lince](../Lince.lingua)
- Shared data and federation model: [Ontology](../Lince.lingua)
- Local: `crates/web/src/sand/record/record.html` (thread/message surface)
- Local: `crates/store/src/action_intents.rs` (intent/lease machinery)
- MDN: `RTCPeerConnection`, `MediaRecorder`, `getDisplayMedia`
- LiveKit docs: Egress overview and screen sharing; LiveKit / Jitsi+Jibri:
  Apache-2.0; mediasoup: ISC; Janus: GPL-3.0
