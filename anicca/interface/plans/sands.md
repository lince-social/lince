# Sand implementation plan

Purpose: Define production Sand schema, authoring, composition, package, HTML, Website, and migration work.

Owner source: no dedicated Sands Record currently exists;
[Interface in Lince](../../Lince.lingua) governs shared interface decisions.

Status: Coordinated with Customization C3-C5; semantic kernel, primitive
Gallery and recursive composition landed, and Box follows the gate.

Read when: implementing the Sand contract or external runtime surface.

[Corpus map](../README.md) · [Current context](../current.md)

---

## Landed C1 boundary

Sand schema and ABI version 1, persisted/runtime separation, projection
manifests, package/hash/license validation, generated JSON Schemas and
valid/stale fixtures have landed. The 19-definition retained Gallery and its
Installed CEF HTML/CSS/ES-module projection use that contract; the Website
companion retains no Lince authority. Keyboard, pointer and AccessKit actions
reach the retained state, a Protein-shaped mount reaches Installed HTML and
`record-clicked` returns through its exact event grant. Release parity and the
joined Wayland report pass. Package admission resolves only declared relative
static module imports, and the live Installed decoder proves malformed
unknown-field refusal before accepting a valid mount.

## Landed C2 boundary

Composition schema version 1 adds the exact-revision catalog, strict artifact,
placement, typed binding and lineage records above the Sand graph. One
Rust-owned recursive host mounts definition, child and instance patches,
chooses every declared adapter kind and owns all renderer and Behavior
handles. Shared edits propagate transactionally through exact ancestors;
invalid publications preserve the last-good catalog. Save as definition,
save/reopen, lock, override/reset and fork/detach are normalized operations,
not renderer-specific shortcuts.

The F10 workbench and its AccessKit nested Button are the human surface. A
standalone Button and twice-nested video-call compound expose the same typed
identity through native retained and Installed HTML projections. Protein
reads, Sand events and Action writes have visibly different arrows. Installed
HTML creates collision-free instance DOM ids, repairs label and ARIA
references and explicitly tears down its listener. Rust construction,
recursive Maud output and workbench artifacts agree in the release parity
report, and the joined Wayland report passes. Castle remains only a human name
for a saved compound.

## What is left

### Sand

- [ ] On the accepted [Native Interface Laboratory](../laboratory.md#native-interface-laboratory)
  runtime, make native Rust retained-UI or world-renderer constructors the
  first-party Plan A path, paired with the exact
  node/port/configuration metadata Box needs. Preserve Maud as the standard
  Plan B and HTML-backed authoring path without changing HTML packages into a
  Rust-only format. Its paired constructors produce accessible `Markup` and the
  same metadata; reject naked markup as a declared child boundary.
- [ ] Define the Sand artifact compiler that normalizes the selected authored
  graph (Rust/Maud or declared raw HTML metadata),
  validates it through the authoritative Rust schema, renders and hashes
  fragments, gathers native JavaScript modules, Wasm modules, generated loader
  glue, shaders and other assets, and enforces manifest, capability, source to
  artifact hash, LICENSE, NOTICE, and credit completeness.
- [ ] Define the logical Sand runtime ABI once and generate its adapter
  projections: native Rust retained-UI calls, retained world-scene handles, validated
  size- and rate-bounded CEF messages for installed external HTML,
  wrapper-only Website ports, Plan B scoped DOM/`MessagePort` calls, and an
  optional WIT projection for Wasm Behavior. Prove the adapters agree on
  lifecycle, typed ports, attribution, capabilities, state planes, Action
  requests, errors, camera-only presentation culling, and teardown while
  passing no raw DOM, GPU object, pointer, credential, or global Box store
  through the portable boundary.
- [ ] Define the GPU renderer vocabulary and package rules for built-in
  patterns, sprites/glyphs, zones, connections, drawings, selection, and
  specialized leaves. Use one host rendering device, queue/submission policy
  and retained scene with stable visual-node uids and partial buffer updates;
  isolated CEF GPU producer contexts cross only the synchronized external-
  surface boundary. An arbitrary shader is installed
  executable content with an exact hash, declared GPU capability, resource
  budget, validation, license, credits, and deterministic disposal.
- [ ] Rebuild official workflow Sands into referenced primitive/compound Sand
  definitions plus Behavior after the contract is proven. Do not preserve the
  legacy component API or old board state merely to avoid rebuilding.
- [ ] After the Customization completion gate and official-Sand migration,
  prove the Box model vertically with one current Protein item, visual result
  fields, a mixed bound/unbound result-template group, repeated row instances,
  and force/sort/mutation areas. Do not use this spatial proof to finish the
  component or composition foundations underneath it.
- [ ] In that vertical proof, remove and restore one stable Protein row and
  change one result field incompatibly. Show live, retired, restored and broken
  binding states, retain bounded recoverable instance-local state, and provide
  visible reconnect/clear/replace operations through **Why is it here?**.
- [ ] Expose the landed instantiate/override/reset/save/fork operations in Box
  edit mode. A code-owned native or Maud definition is never rewritten; Box
  edits a visibly forked user definition and preserves lineage and revision
  inspection.
- [ ] Write concise author documentation that starts with composing existing
  pieces and progresses to HTML, Protein, Actions, ports, permissions, and
  packaged assets.

- [ ] Define and version the Sand manifest, bridge handshake, typed ports,
  capability vocabulary, provenance record, resource limits, CSP, and package
  signature/integrity rules together.
- [ ] Prove the installed external path with one CEF HTML Sand that receives a
  mapped Protein Record summary, emits `record-clicked`, consumes a Box event,
  keeps local browser state, and requests one granted typed Action. Run the same
  semantic fixture through Plan B `MessagePort`. Reject undeclared ports,
  malformed values, excessive size/rate, spoofed instance identity, and the
  same messages from an arbitrary Website.
- [ ] Replace the current broad iframe grant with the trust tiers above and
  prove that a denied Sand cannot reach Actions through another Sand or leak
  data through lanes, navigation, popups, downloads, or network requests.
- [ ] Add import, inspect-before-run, permission review, update review,
  revoke, disable, and delete surfaces with honest failure and empty states.
- [ ] Add a runtime-health and admission surface for heavyweight Sands. Show
  process and texture cost, configured budget, denied starts, renderer crashes,
  recovery attempts and the exact unavailable reason; keep retry, disable and
  clear-storage controls reachable without opening developer tools.
- [ ] Build the Website Sand with an isolated CEF browser surface and request
  context under Plan A and a sandboxed iframe in browsers. Keep origin/security chrome above
  remote pixels and prove that Website content cannot invoke Lince native
  APIs, overlap system chrome, or receive a privileged parent message. Moving
  it off-camera culls composition only and does not unload, suspend, or throttle
  its browser execution.
- [ ] Enforce HTTPS navigation; deny custom protocols, filesystem access, and
  all Lince native capabilities; and harden every local HTTP/WebSocket
  endpoint against foreign origins, unauthenticated requests, and CSRF. In CEF
  or dedicated WebViews, additionally intercept requests to block loopback,
  link-local, private-network destinations, unsafe redirects, and DNS
  rebinding. In a browser iframe, disclose that broader private-network egress
  cannot be guaranteed rather than presenting it as enforced.
- [ ] Add the isolated per-origin Website profile, explicit persistent/private
  modes, storage quotas, usage inspection, clear-data controls, and tests for
  cookies, local storage, IndexedDB, Cache Storage, service workers, restart,
  and cross-instance sharing.
- [ ] Add per-origin permission and activity surfaces for downloads, popups,
  external protocols, clipboard, camera, microphone, location, notifications,
  screen capture, and storage-access requests. Denial and platform limitations
  have honest in-Sand explanations.
- [ ] Test malicious Websites for local-network requests, CSRF against Lince,
  navigation spoofing, popup escape, downloads, resource exhaustion, tracking
  identifiers crossing profiles, and frame/IPC confusion. Keep CEF, WebView,
  and browser runtimes patched; sandboxing does not eliminate engine exploits.
- [ ] In browser/Plan B iframe mode, detect sites that prohibit framing and
  offer an explicit open-in-browser fallback. Never strip or proxy around
  `frame-ancestors` or `X-Frame-Options`. In Plan A CEF mode, detect sites,
  authentication, protected media, or browser policies that still reject the
  embedded runtime and offer the same fallback.
- [ ] Package the authoring documentation and a minimal bridge test kit so
  external HTML can integrate without copying an official Sand as folklore.
- [ ] Remove the legacy nested-payload frame and old Lynx component API during
  the rebuild. Route `.lince` imports by inspected content rather than a
  legacy filename suffix; unknown shapes fail closed.
- [ ] Federate published Sand packages between connected Organs while
  preserving content hash, lineage, author, capabilities, and licenses. The
  contract must not depend on whether bytes live on disk or in a future object
  store, and it must not require a central registry.

- [ ] Later on, some form of creation of data, similar to ontology's trail should exist and be able to see it in this sand, to input in some dsl or lingua the creation of data to make this demo of paradigms of intelligence: https://paradigms-of-intelligence.github.io/morpho/.

## Task-bound terminal Sand (Fiote)

Owner ask, 2026-08-29: "we should be able to have in the end a libghostty or
equivalent sand so we can attach properties to that specific sand which can be
the record of the task we are trying to do in such terminal."

The design reasoning lives in [Fiote build notes](../../Karma.md) (D1, D13,
D14); only the interface consequence belongs here.

**The binding needs no schema change.** `BoardCard` already carries
`widget_state: Value` — arbitrary per-instance JSON persisted with the card. A
terminal pane bound to a task is `widget_state.record_uid`, set when the pane
is opened from a task and read back on reload. What the instance carries is a
*binding*, never a copy: the Record is the truth, the pane displays it.

- [ ] Let a Ghostty terminal instance carry a bound Record uid in
  `widget_state`, shown in the pane's own chrome (title, status control) so a
  board of several terminals is readable at a glance. The honest empty case is
  "not bound to a task", distinct from "task deleted" and from "no permission
  to see it".
- [ ] Open a bound terminal *from* a Record — the inverse direction is what
  makes the binding worth having, because a person starts from the task, not
  from a pane.
- [ ] Let the bound Record be changed or cleared on a live pane without
  killing the session. The PTY and the binding have different lifetimes, and
  today the PTY's is the shorter one
  (`crates/transport/src/terminal.rs` owns sessions per websocket connection —
  see Fiote D2, which moves agent sessions off that ownership).
- [ ] Show the same binding on whatever pane a Fiote cub runs in, so the
  terminal view and the Fiote view are two projections of one task rather than
  two unrelated surfaces.
- [ ] Decide whether `widget_state` bindings are workspace-local or travel
  with an exported workspace/archive. A shared board that references a Record
  the recipient cannot see must degrade to the honest empty case above rather
  than a dead uid.

### Fiote sessions stream into the interface

Owner, 2026-08-30, marked high priority for the interface refactor: whatever
Fiote and its cubs are doing must be watchable here, live.

This is the same surface as the task-bound terminal above — a pane bound to a
Record — with a different source behind it. The harness design lives in
[Fiote build notes](../../Karma.md); only the surface consequence belongs here.

- [ ] Stream a Fiote or cub session into a pane as it runs: assistant text,
  thinking, tool calls and their results, and the bytes of any command it ran.
  The stream is host-owned and the pane attaches to it, so closing the pane
  does not end the session and reopening replays the backlog.
- [ ] Show per-session state beside the stream — model, tokens used, context
  percentage, cost, and whether it is streaming, waiting on a tool, waiting on
  a person, or finished. A pane with no state readout cannot be told apart
  from a stalled one.
- [ ] Let a person type into a running session (steering) and have it land
  between tool calls rather than mid-stream.
- [ ] Several sessions on one board at once, each labelled by which Fiote owns
  it and which task it is on, since a board of unlabelled panes stops being
  readable at about four.
- [ ] Decide whether a raw terminal view of a session is a separate pane kind
  or a mode of the same one. The Ghostty sand already renders VT bytes; a
  session that ran a command has bytes worth rendering that way.

### Pinned and queued messages, one list

Owner, 2026-08-30. The reasoning is in [Fiote build notes](../../Karma.md) D32;
this is the surface.

A draft message is a Record only its author can see, carrying assertions for
what it will do: `#pinned` (a preset, copied on send, survives), `#steer`
(delivered at the next safe point in a running turn), `#next` (delivered when
the turn finishes). A queued draft without `#pinned` is consumed when it sends.
One list holds both, and the tags say what each entry will do.

- [ ] One drafts list per conversation, private to its author, holding presets
  and queued messages together. Reorder by dragging; promote an entry to go
  next; edit or delete in place.
- [ ] The send control shows what pressing it will do, because that changes
  with what is running: send now, deliver at the next safe point, or queue
  behind what is already queued. Never learned by surprise.
- [ ] Delivery controls appear only while a turn is in flight, and when inert
  they are still reachable and say why — "no turn running: this sends now".
  Presets are unconditional; a canned reply is useful in a conversation between
  people too.
- [ ] Show a queued entry's age. One queued an hour ago and fired unattended is
  a stale intent, not an instruction.
- [ ] "Send next" must never be labelled "now" while a tool is running: the
  honest maximum is when that tool returns. Abort-and-send is a separate
  control and looks destructive.

### The terminal and the thread are two views of one session

- [ ] A session's pane can show the thread (messages, durable) or the terminal
  (raw bytes of what it ran, ephemeral), and switching between them does not
  restart or interrupt anything. The terminal is the uncollapsed form of what
  a folded tool result already shows.
- [ ] Decide whether the terminal is read-only or interactive. If interactive,
  typing into it **bypasses the model** — it is not a message and the agent
  does not know it happened unless the output returns to its context. That must
  be visibly distinct from steering: a different pane state, and a line in the
  thread recording that a person typed directly.
- [ ] Ctrl-C in the terminal stops the running command; a stop control on the
  session cancels the whole turn. An interrupted turn must be marked as
  interrupted where the model can see it, or it reads its half-finished work as
  finished.
- [ ] The terminal owns its keys entirely (it already has its own keymap), so
  composer shortcuts such as Tab-to-queue apply only in the composer. No
  keyboard trap in either pane.

### Three views of a session, and live messages

Owner, 2026-08-30. Reasoning in [Fiote build notes](../../Karma.md) D31, D32.

Agreed and settled: a queued draft is consumed on send and a pinned preset is
copied; delivery aspects are assertions on the draft; delivery controls appear
only while a turn is in flight and say honestly what they would do when inert;
a queued entry shows its age; "send next" is never labelled "now" while a tool
is running; queued messages are editable; when a turn ends with several queued,
they are delivered one at a time by default and the setting can be flipped.

- [ ] A session pane offers three views, and switching never restarts or
  interrupts anything: **the thread** (messages, durable, what other people
  see), **what the agent ran** (its commands and their output, rendered as a
  terminal because that is what they are, read-only), and **a real terminal for
  the person** (their own shell in the session's working directory, fully
  interactive — theirs, not the agent's).
- [ ] Match what Pi's own interface shows, and list the gaps rather than
  excusing them: tool calls with arguments, results, diffs, token and context
  counts, queue state, compaction. All of it arrives over the protocol; where
  our rendering is thinner, that is work, not a missing capability.
- [ ] Never let the agent's command view be typed into as if it were a shell.
  Input there bypasses the model and the model does not know it happened.
  Either keep it read-only or make the intervention visibly exceptional and
  record it in the thread.
- [ ] Render an assistant message **live, as it is written** — the Record's
  body grows and the thread shows it growing, including for a person on another
  Organ watching the same thread.
- [ ] Show a message's state: still writing, finished, or interrupted. A reader
  who cannot tell a live message from a stalled one does not know whether to
  wait.
- [ ] A streaming message is read-only until it finishes; editing arrives after.

### A session is a Sand group

Owner, 2026-08-30: when we have a session with an agent we have a Sand group —
the thread, what the agent ran, and possibly the terminal — reusing existing
Sands and inventing the few that are missing.

The board already has the mechanism: `BoardCard` carries `groupId` and a
nested `groupIds` stack, and `group-logic.js` has `wrapInGroup`. Nothing new is
needed to make a session a group; what is new is what goes in it and where the
group comes from.

**What is reused, and what is actually new.**

| View | Sand |
| --- | --- |
| The thread | **`conversation`**, unchanged. It already reads Conversation → Thread → Message over Protein and sends with `create-message` / `open-thread`. A session thread is an ordinary thread. |
| The task | **`record`**, unchanged, bound to the Record the session hangs off. |
| A real terminal | **`terminal`**, unchanged. |
| What the agent ran | **New.** A tool timeline: each call with its arguments, its result, diffs it produced, folded by default. Command output renders through the same VT path the terminal Sand already uses, read-only. |
| Session control | **New.** The cub tree, spawn and stop, model and thinking level, tokens / context / cost, compaction. |
| Drafts and queue | Part of the composer, not its own Sand. |

- [ ] Bind every member of a session group to the same session Record through
  `widget_state`, and give the group one lane room so its members coordinate
  without going through the Ledger.
- [ ] Opening a session reconstructs its group; closing it destroys nothing,
  because the session is host-owned and outlives every view of it (Fiote D2).
  *Where the layout itself lives — a recipe on the session Record versus loose
  cards on one board — is deferred to the v2 interface refactor (owner,
  2026-08-30), along with anything mobile.*
- [ ] **One group per Fiote, not per cub.** Three or four Sands per session
  multiplied by several cubs is a board nobody can read — the same failure as
  unlabelled panes. Cubs are rows in the session-control Sand, and any one of
  them can be *promoted* into its own group on demand. Nested groups already
  support that; auto-spawning them does not.
- [ ] A new Sand needs its permission in the manifest, the way the terminal
  Sand declares `terminal_session`. The session Sands need one of their own
  rather than borrowing `act`.
- [ ] Keep the operator's view and the shared artifact separate: **another
  person opens the same thread with only the `conversation` Sand** and sees
  the conversation, because the thread is the shared thing and the group is
  one person's way of working on it. Nothing about the group should be
  required to read what happened.

### The two new session Sands, in tiers

Owner, 2026-08-30: face the feature increase — write down the full version and
build it piece by piece. First cut is what the Fiote prototype needs
(Karma.md Phase 2); full is what it becomes.

**Session control.**

- [ ] *First cut:* the roster — this Fiote and its cubs, each with a state
  (idle, streaming, running a tool, waiting on a person, finished) and the task
  it is on; tokens used and context percentage per session; stop.
- [ ] Model and thinking level per session, changeable mid-session.
- [ ] Budget: a ceiling per session and per Fiote, spend so far, and what
  happens when it is reached — stopping, not warning.
- [ ] Tool policy: which MCP servers and which tools this agent may use, each
  allow / ask / deny.
- [ ] Which Agent Record the session's prompt came from **and which revision**,
  so "why did these two behave differently" has an answer.
- [ ] Traversal policy: the link Concepts followed, direction, depth, and
  whether each is glanced at, summarised or read in full.
- [ ] The draft queue with ages, reorder and promote (shares the composer's
  list).
- [ ] Compaction: current context use, the compact gesture, and what a past
  compaction dropped.
- [ ] Provider trouble: the last retry, the last error, and whether it is
  retrying now.
- [ ] Spawn a cub, fork a session, rename a session.

**Tool timeline.**

- [ ] *First cut:* a folded chronological list — tool name, what it acted on,
  ok or error, duration. Nothing is persisted (Fiote D33): the timeline is
  live while the session runs and empty afterwards, and "output not kept" is
  stated rather than looking like "no output".
- [ ] Render a payload by what it is: text, VT for a command, a diff for an
  edit, an image for an image.
- [ ] Promote a payload into the thread — the one way anything about a tool
  call becomes durable, since nothing is kept by default (Fiote D33). Copying
  it into a Message is an ordinary write and needs no persistence layer.
- [ ] Filter by tool and by status, and search within outputs.
- [ ] Jump from a call to the turn it belongs to, and back.
- [ ] Copy the command.
- [ ] **Not to be built casually: re-run a call.** Replaying a side effect
  outside the context that produced it is a different act from repeating a
  query, and the timeline should not make them look alike.

### Fiote arranging the board

- [ ] Once a group is derived rather than hand-placed, a Fiote adding or moving
  a Sand is an ordinary write and needs no privileged path — which is what
  [Karma](../../Karma.lingua) imagined. It must be **visible and undoable**,
  or a board that rearranges itself reads as haunted rather than helpful.

### The agent work board

- [ ] Task Records assigned to agents form a `part-of` tree, which `kanban` and
  `relations` already render. "What are my agents working on" is likely an
  existing Sand with a filter rather than a new one — check before building.
