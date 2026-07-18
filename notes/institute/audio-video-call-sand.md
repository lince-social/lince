# Audio-Video Call Sand

Planning note for a Lince web sand that represents a call around a Record:
audio/video, screen sharing, recording, meeting chat, and Karma-triggered start
or stop actions.

## Agreements

- A call should be a normal Lince Record, for example "Call: X, Y, Z" or
  "Weekly group call".
- The call Record is the durable meeting object. It owns title, body/agenda,
  participants, group links, recording references, transcript references, and
  message thread links.
- The call sand should be an official sand, not a random imported widget, for
  the first production path.
- Rust should own the durable control plane: Record model, participant
  authorization, call state, Karma consequences, command leases, recording
  metadata, message creation, audit/provenance, and provider tokens.
- Browser JS should own browser-only edges: camera/mic permission, screen share
  permission, WebRTC peer connection or provider SDK calls, local device state,
  and responsive media layout.
- Embedding a ready open-source call solution is the third option, mainly when
  group calls and reliable server-side recording matter more than keeping the
  media plane Rust-native.
- Karma should not broadcast vague commands that any sand can consume. A Karma
  consequence should create a typed, targetable call intent that only the
  explicitly configured controller sand can claim.
- Multiple call sands may exist, but each command must target one call Record
  and one configured controller instance or controller binding. That keeps it
  clear which sand will pick up the command.
- Meeting messages should reuse the message/thread model rather than inventing
  a separate chat table. Live chat can be mirrored into persistent messages.
- Recording should be explicit and visible. Silent automatic recording should
  not be the default.

## Record Model

Use one normal Record for the call itself:

```txt
record.kind = "call"
record.head = "Call: Alice, Bruno, Carla"
record.body = agenda, notes, or summary
```

Use a sidecar extension for call-specific metadata:

```json
{
  "namespace": "call.v1",
  "provider": "native-webrtc" | "livekit" | "jitsi" | "mediasoup",
  "room_id": "stable room identifier",
  "state": "idle" | "ringing" | "active" | "ended",
  "controller_sand_uid": "board/card/widget instance id",
  "recording": {
    "mode": "manual" | "karma" | "disabled",
    "state": "idle" | "recording" | "processing" | "available" | "failed"
  },
  "started_at": null,
  "ended_at": null
}
```

Use links for repeated relationships:

- `call-participant` from call Record to person/user/organ Records.
- `call-group` from call Record to a group Record.
- `thread-of` from thread Record to call Record.
- `message-in` from message Records to the meeting thread.
- `call-recording` from call Record to media/recording Records or resource refs.
- `call-transcript` from call Record to transcript Records or resource refs.

Recording files should be resource references, not blobs in `record.body`.
Object storage or local media storage can hold the file; Lince stores the
metadata, hash, duration, owner, retention, and access policy.

## Sand Shape

The sand should be a server-shaped official widget with a small JS media layer.

Rust side:

- `GET /host/widgets/{instance_id}/contract` returns the call Record, provider,
  allowed actions, participant list, controller binding, and recording policy.
- `GET /host/widgets/{instance_id}/stream` or Protein subscription returns call
  state, participants, messages, recording state, and pending call intents.
- `POST /host/widgets/{instance_id}/actions/{action}` exposes typed actions:
  `call-create`, `call-bind-controller`, `call-start`, `call-join`,
  `call-leave`, `call-end`, `call-start-recording`,
  `call-stop-recording`, `call-send-message`, and `call-claim-intent`.
- The host generates any provider token. The sand never stores backend secrets
  in `widgetState`.

JS side:

- Uses `navigator.mediaDevices.getUserMedia` for mic/camera.
- Uses `navigator.mediaDevices.getDisplayMedia` for screen sharing.
- Uses WebRTC directly for the native/pure JS path, or the selected provider SDK
  for the embedded/provider path.
- Keeps local ephemeral UI state in the browser: muted, selected device,
  active speaker, local preview, layout mode.
- Persists only durable user choices through host state, not credentials.

## Stack Preference

### 1. Rust-first native path

Use Rust for signaling, authorization, state, and possibly media routing.

Good v0:

- Lince Rust backend provides WebSocket signaling.
- Browser uses standard WebRTC APIs.
- Calls start as one-to-one or very small mesh calls.
- Browser `MediaRecorder` can produce a local/manual recording as a prototype.
- Durable recording metadata is still written through Rust actions.

Rust media options:

- `webrtc-rs`: async-friendly Rust WebRTC implementation; good for a Rust-native
  WebRTC service, but still a serious media project.
- `str0m`: Sans-I/O Rust WebRTC; attractive if Lince wants deterministic,
  explicit runtime control later.
- `mediasoup` Rust API: lower-level SFU-oriented option with a C++ worker and
  Rust/Node integration surface.

Tradeoff: this is the most aligned with Lince, but a reliable SFU plus
recording pipeline is a large subsystem. Do not hand-roll a production group
call recorder in the first pass.

### 2. Pure JS second path

Use browser WebRTC directly inside the official sand.

Good for:

- One-to-one calls.
- Small trusted calls.
- Manual screen sharing.
- Local recording experiments.
- Proving the Lince data model, actions, Karma intents, and message flow.

Limits:

- Mesh calls scale poorly as participants increase.
- Browser-local recording is not authoritative for a group meeting.
- Screen sharing always needs user activation and browser permission.
- NAT traversal still needs STUN/TURN infrastructure, usually `coturn`.

### 3. Embedded open-source solution

Use an existing WebRTC stack when reliable multi-party calls and recording are
the product requirement.

Most pragmatic first production candidate:

- LiveKit: open-source WebRTC SFU with self-hosting, tokens, SDKs, native screen
  sharing support, and Egress for room/track recording. Server and Egress are
  Apache-2.0, but the core server is Go, not Rust.

Other candidates:

- Jitsi Meet: Apache-2.0, full meeting product, easy to embed, uses Jibri for
  recording/live streaming. Heavier and less Lince-native.
- mediasoup: ISC, low-level SFU building block with Rust support. Strong if we
  want to build our own product surface; more work than LiveKit.
- Janus: capable WebRTC server, but GPL-3.0, so it is less attractive for
  bundling or tight embedding unless that license is explicitly acceptable.

License rule: if a sand vendors client assets or embedded app assets, keep the
required LICENSE/NOTICE/credit files beside the sand package and bundle them
with the widget assets.

## Recording And Messages

Recording should produce ordinary Lince artifacts:

1. The call Record moves to `recording.state = "recording"`.
2. The media provider creates the recording.
3. The backend receives completion/failure.
4. Lince creates or updates a recording Record/resource ref.
5. The call Record links to the recording.
6. A message is added to the call thread: recording available, failed, or
   deleted.

Message flow:

- Live chat can be sent through provider data channels or Lince actions.
- Durable messages are written as normal message Records.
- Transcript chunks, summaries, decisions, and action items should be messages
  or linked Records, not hidden provider state.
- If "speak minimally" is a goal, the sand should emphasize meeting chat,
  decisions, and async notes, with audio/video as an activation mode rather than
  the whole product.

## Karma Command Model

Karma should create call intents. A call intent is durable, targetable, and
claimable.

Example intent Record:

```json
{
  "kind": "call_intent",
  "call_record_id": 42,
  "target_controller_sand_uid": "sand:board-a/card-7",
  "action": "call.start",
  "source": "karma:108",
  "status": "pending",
  "nonce": "uuid",
  "created_at": "timestamp",
  "lease_owner": null,
  "lease_expires_at": null,
  "payload": {
    "recording": "manual" | "start",
    "reason": "scheduled call window opened"
  }
}
```

Claim rules:

- A sand can claim only intents for the call Record it is bound to.
- A sand can claim only intents targeted to its `controller_sand_uid`, unless
  the call Record explicitly names it as the controller.
- Claiming is atomic and writes a lease.
- If two matching sands exist, only one wins the lease; the other sees the
  claimed/completed state.
- The action is idempotent by nonce.
- Completion writes `status = completed` or `failed` plus an error message.

Karma examples:

- "At 10:00 and if Call Record 42 has participants ready, create
  `call.start` for controller sand UID X."
- "When Record A quantity enters a range, create `call.start-recording` for
  Call Record 42."
- "When Transfer Y reaches agreed, create `call.end` for the call that was
  opened for that negotiation."

This keeps Karma declarative: it asks for an effect; the official call action
decides whether the effect is allowed.

## Implementation Slices

- [ ] Add this document as the current architecture agreement.
- [ ] Define `call.v1` Record extension and links in docs before schema work.
- [ ] Define call Actions and the intent claim protocol.
- [ ] Build a non-media call sand shell that can bind to a call Record, show
  participants, show messages, and claim no-op intents.
- [ ] Add manual message creation in the call sand.
- [ ] Add pure JS one-to-one WebRTC with Rust signaling.
- [ ] Add screen sharing.
- [ ] Add local/manual prototype recording and link the artifact to the call.
- [ ] Decide production media provider: continue Rust-native, LiveKit, Jitsi, or
  mediasoup.
- [ ] Add provider-backed server-side recording if group calls are required.
- [ ] Add Karma consequences that create call intents instead of direct UI
  commands.
- [ ] Add driven browser selftests for intent claiming, message writes, and
  state updates before broadening the media path.

## Open Questions

- Is the first call target one-to-one, small group mesh, or serious group calls?
- Are recordings always local to one organ, or can they sync between organs?
- Should screen sharing be recordable by default, or require a separate consent?
- What is the retention policy for recordings and transcripts?
- Should call participant identity use existing organ/user Records or a new
  identity abstraction?
- Should a Call Record represent one meeting occurrence or a recurring room with
  many session child Records?

## References

- Local: `notes/institute/Sand.md`
- Local: `notes/institute/Karma.md`
- Local: `docs/new-version-capabilities-and-maneirisms.md`
- MDN: `RTCPeerConnection`, `MediaRecorder`, and `getDisplayMedia`
- LiveKit docs: Egress overview and screen sharing
- LiveKit GitHub: Apache-2.0 SFU and Apache-2.0 Egress service
- Jitsi Meet and Jibri GitHub: Apache-2.0 meeting stack and recording service
- mediasoup GitHub: ISC, low-level SFU with Rust API
- Janus GitHub: GPL-3.0 WebRTC server
