# Fiote — build notes

**Bring-your-own and subscriptions: ACP, later** (D29). Our own loop cannot
redeem a Claude or ChatGPT subscription; only the vendor's client can. An ACP
backend behind the same seam covers both that and "use your own agent". Not
now.

**Extending it needs no compiler** (D23). Prompts, traversal policy, allowlists,
models and which MCP servers an agent may use are data on the Agent Record
(`record_extension`, namespace `lince.fiote`), edited in Lince.

**Retrieval is not instruction** (D29). What enters a context is a deterministic
walk of `record_assertion` — by Concept uid, direction, depth, with an action of
glance / summarize-by-cub / read-in-full. Instructions only shape what the model
does with what arrived. Budgets are arithmetic; permissions are a gate.

**Identity is built already** (D17). `Action::CreateAgent { head, operated_by }`
makes an Agent Actor and links it to the Person answerable for it. Each Fiote is
one of these; its behaviour is its Record's body. Missing: a published key per
agent, and authorship shown on messages.

**Landed:** `crates/fiote` — supervisor, child process, line framing, replayable
backlog, attach/detach, send and kill, one passing integration test driving a
real Pi session.

**Next: Phase 0 beside C4-C5, then Fiote Phase 2.** The Interface refactor has
already landed its native runtime, Sand ABI and recursive composition host.
Phase 0 is three independent cleanups that must finish before Fiote resumes.
C3 has now landed the domain-derived compound recipe. The remaining Phase 1
work maps concretely onto C4 Conversation and official-Sand migration and the
C5 human-use gate. Fiote's own prototype starts after C5 without becoming a
prerequisite for Box.

**Also high priority, and split at the right boundary:** C4 gives every
conversation live Messages, authorship and private drafts. Fiote Phase 2 adds
the session-control and tool-timeline Sands, reusing the Terminal renderer for
read-only command bytes and a separate Terminal Sand for the person's shell.
The compound and its task/session Record bindings are written up in
`anicca/interface/plans/sands.md`.

**Parked after this planning pass** (owner, 2026-08-30). Fiote resumes once the
new Interface lands. What must be built *during* that refactor rather than
after it is listed under **Carry into the Interface refactor**, below.

**Open:** D9's sharing level for code (reference, patch, or bundle).

**Known walls:** Karma cannot dispatch anything (its intents are inert), so
every notification lands on `effect_queue` until that changes. Sand feature
flags are labels, not gates — `feature_flag()` is dead code. There is no
secret-at-rest story, so provider keys follow the `0600` file precedent.

## What Pi is

`@earendil-works/pi-coding-agent` (repo `earendil-works/pi`, formerly
`badlogic/pi-mono`), v0.84.4, TypeScript, run by Node. It is a harness, not a
model: `pi-ai` is a provider-unifying LLM client (Anthropic / OpenAI / Google /
others), `pi-agent-core` is the agent loop with tool calling and state, and
`pi-coding-agent` is the CLI with read/bash/edit/write/grep/find/ls tools,
sessions, skills and extensions.

Four modes, and only one of them matters for us:

- `interactive` — the TUI a human types into.
- `--print` — one shot, exits.
- `--mode rpc` — **JSONL over stdin/stdout.** Commands in, events out.
- SDK — `createAgentSession()` in-process, TypeScript only.

Extensions are `.ts` files loaded through `jiti` (no build step) from
`~/.pi/agent/extensions/*.ts`, `.pi/extensions/*.ts`, or `-e path`. An
extension registers tools (`pi.registerTool`), slash commands, keybindings,
lifecycle hooks (`session_start`, `before_agent_start`, `tool_call`,
`tool_result`, `context`, `agent_end`), persistent entries (`pi.appendEntry`),
and can spawn child agents with `pi.sendMessage`. Skills are files pulled into
context on demand.

Sessions are files on disk (`--session-dir`), addressable by id, and support
`fork` (branch from an earlier user message) and `clone`.

### The RPC surface, and why it is the whole integration

Probed locally, no API key needed to start:

    printf '{"type":"get_state","id":"1"}\n' | \
      vendor/pi/node_modules/.bin/pi --mode rpc --no-session --offline

returns model, thinking level, streaming flag, session id, message counts.

Commands: `prompt`, `steer` (guidance delivered after the current tool calls
finish), `follow_up` (queued for after the turn), `abort`, `clear_queue`,
`bash`, `abort_bash`, `get_state`, `get_messages`, `new_session`,
`switch_session`, `fork`, `clone`, `set_session_name`, `export_html`,
`set_model`, `cycle_model`, `get_available_models`, `set_thinking_level`,
`compact`, `set_auto_compaction`, `get_session_stats`, `set_auto_retry`,
`set_steering_mode`, `set_follow_up_mode`.

Events: `agent_start/end/settled`, `turn_start/end`,
`message_start/update/end` (with `text_delta`, `thinking_*`, `toolcall_*`
deltas), `tool_execution_start/update/end`, `bash_execution_update`,
`queue_update`, `compaction_start/end`, `auto_retry_start/end`, and an
extension-UI sub-protocol (`extension_ui_request` / `_response`) carrying
`select` / `confirm` / `input` / `editor` — which is exactly a tool-approval
prompt when an extension asks for one.

**Everything the owner asked for is in that list.** "Show each cub's context
and what it is comprised of" is `get_session_stats` + `get_messages`. "Pet it
and make it sleep until compacted" is `compact` plus `compaction_start/end`.
"Add my own message mid-task" is `steer`. "Which cub said what" is the session
id on the event. None of it is recoverable from a stream of terminal bytes.

## Decisions

### D1. A structured protocol is the spine. The terminal sand is a viewer.

*The mechanism is now ACP to `goosed` (D22, D27), not Pi RPC. The argument
below is unchanged and is why: a PTY cannot carry what the surface needs.*

The owner's sketch was "a sand with a bunch of terminals, libghostty running
them, Fiote controlling the cubs". Half right: the multi-pane *look* is right,
the *mechanism* is not. A PTY gives us rendered ANSI. We would then be
screen-scraping structure that the RPC channel hands over as JSON, and we would
still have no way to steer, compact, fork, or read token counts.

So: a session is a structured conversation with a harness, and the Fiote sand
renders its state from events. A raw TTY view stays *possible* later — attach a
Ghostty pane to a harness running interactively when a human wants to drive one
by hand — and is not the road anything else travels.

### D2. The supervisor lives in the host process, not on the transport connection.

`crates/transport/src/terminal.rs` holds `TerminalHost { sessions: HashMap<..> }`
on the websocket connection, and `protocol.rs:102` says so out loud — "an
ephemeral PTY owned by this transport connection". Reload the board and the PTY
dies.

That is correct for a terminal and fatal for an agent. A cub that runs twenty
minutes must survive a browser reload, a laptop lid, and a second board opened
on the phone. So the Pi supervisor is host-owned with its own lifetime; the
sand *attaches* to a session and *detaches*, and detaching is not killing. This
is the one thing that had to be decided before any code, and it is why
`crates/fiote` is its own crate rather than more of `transport`.

### D3. Opt-in means a cargo feature, not a runtime `if`.

`fiote` is off by default. Not compiled, no sand in the registry, and — since
nothing is bundled (2026-08-30) — no runtime for anyone to carry either: Pi is
a local tool the owner already has, so a build without the feature has no
opinion about it at all. Two switches, both named the same thing:

- workspace cargo feature `fiote` → gates the `crates/fiote` dependency, the
  transport messages, and the sand registration;
- a runtime switch for the sand, so a build that *has* Fiote can still be run
  without it.

**The second one does not exist yet.** Checked: `sand::FEATURE_FLAG` strings
are declared per sand and `OfficialWidgetBuilder::feature_flag()` carries
`#[allow(dead_code)]` and is read nowhere. `sand.ghostty_terminal` is a label,
not a gate — every registered sand is built and offered. So either Fiote's
runtime switch is the first real use of that mechanism (and building it is part
of the milestone that ships the sand), or the cargo feature is the only opt-in
and the doc must not imply otherwise. Do not write `sand.fiote` into the
registry and call it opt-in.

### D4. Identity: one lasting Actor, many Session Records. Not many Actors.

*Partly superseded by D10 (2026-08-29): still correct for a local-only Fiote,
wrong for any agent whose words cross to another Organ.*

Read from the code, not from the sketch:

- `nucleus::CauseKind::Fiote` exists with `Cause { kind, uid: Option<String> }`
  — an attribution slot that is *never constructed anywhere today*
  (`grep CauseKind::Fiote` finds the enum and its two string arms, nothing
  else).
- `store::visibility::grant` documents `subject_kind` as
  `organ | actor | public | fiote`, and `0001_init.sql:225` adds `role`. But
  `visible_targets` only honours `subject_uid = ?` **or**
  `subject_kind = 'public'`. **A `fiote` class grant is a placeholder that
  grants nothing.** Anything that needs to read must have a subject uid.
- There is no `Agent` in `RecordKind`. An actor uid, in practice, is a `Person`
  Record bound to an `app_user` (`0011_app_user_person.sql`).

Therefore:

- **Fiote is one lasting Person Record**, bound to its own `app_user`, holding
  its own grants. That is the only shape that already has permissions, an
  authored-fact trail, and a login door.
- **A cub is not an Actor.** It is a Fiote Session Record — child of the Task it
  works on — carrying the Pi session id, model, token and cost counters, and
  state. Its writes are Fiote's writes, stamped
  `Cause { kind: Fiote, uid: Some(session_uid) }`, which is precisely what the
  unused `uid` slot is for.

Giving each cub its own Actor would mean an `app_user` row, a grant set and a
revocation story per ephemeral process, and would still not let anyone ask
"what has Fiote done" without a union over dead identities. One accountable
identity, many attributable sessions.

Consequence to build: `Cause::fiote(session_uid)` and the write path that
stamps it, or the Ledger cannot say which cub moved what.

### D5. Records for decisions, lanes for chatter.

*The line moved 2026-08-30 (D31): message TEXT is a growing Record, not lane
traffic. The lane keeps raw terminal bytes, tool progress and queue changes.
The flood argument below is why the lane still exists at all.*

`threads.rs` makes every message a Record: it hits the Ledger, the op log, the
outbox and every contact's feed. A cub emitting a few hundred turns an hour
through that would flood sync with text nobody will ever re-read.

The lane ABI already exists for exactly this — `joinRoom` / `emit` / `onLane`,
"sand-to-sand ABI over ephemeral lanes (never the Ledger)".

So the split is:

- **Lane** (`fiote:<session_uid>`): token deltas, thinking, tool start/end,
  bash output, queue updates, context percentage. Ephemeral by construction;
  nothing durable is lost when it is dropped.
- **Record**: the task and its decomposition, the assignment, the result of a
  turn the human should see, an approval and its answer, the summary a
  compaction produced. These are few, they are what a person reviews, and they
  deserve to sync.

The human-visible conversation with Fiote reuses Conversation → Thread →
Message unchanged, so "I can add a message too" needs no new mechanism. Open
question: `start_conversation(contact_organ: &str, ...)` wants a contact Organ
and a local-only conversation with Fiote has none. The
empty-string-is-our-own-Cell convention exists on the board side; whether it
holds through `start_conversation` is unverified and must be checked before
anything relies on it.

**Backlog, unmeasured.** `Cub::attach()` replays a 2000-*line* ring buffer, and
that replay is the whole of D2's reload survival — without it, reattaching shows
a blank pane on a cub that has been working for ten minutes. 2000 is a guess. It
is a line cap, not a byte cap, and a cub streaming `text_delta` events emits a
line per token fragment, so it may cover seconds rather than minutes. Measure at
Phase 2 against a real run before choosing the real policy — likely a byte budget,
and likely retaining assembled messages rather than deltas.

### D6. Token cost is set by context, not by hosting.

The owner asked whether terminal multiplexing plus markdown files would be
cheaper. It would not. What costs money is what enters a context window; how
the process is hosted costs nothing either way. A shared markdown mailbox is
*more* expensive than structured RPC — every agent re-reads the whole file to
find its part, on every poll, and the file grows monotonically.

The levers that actually reduce spend, all already in Pi:

- `compact` / `set_auto_compaction` — the thing the "pet it to sleep" gesture
  should be bound to.
- `set_thinking_level` per cub: `off` for a cub whose job is to run a command.
- scoped skills and `--no-context-files`: a cub is given the two skills its task
  needs, not the whole of `anicca/`.
- `-nbt` / `--tools`: a narrow allowlist is also a shorter system prompt.
- never routing one cub's streaming deltas into another cub's context. A cub
  reports its *result*; the deltas exist for the human's screen only.

Measure before optimising further: `get_session_stats` returns tokens, cost and
context-window percentage per session, so the sand can show real numbers
instead of us guessing.

### D7. How the cubs actually talk to each other

State of the art, as it applies here, is unglamorous: **agents coordinate
through a shared task store, not by talking to each other.** A model reading
another model's stream pays tokens to re-derive a conclusion the other one
already wrote down.

The shape:

- Fiote (the lasting session) owns decomposition. It writes Task Records with
  the existing assertion graph — `part-of` for the tree, an assignment
  predicate for who holds one, state as a Concept the way every other Lince
  state works.
- Each cub is spawned with **one task uid and a narrow brief**. Its context is
  the task, its parent's statement of done, and the skills it was given.
  Nothing else.
- A cub finishes by writing its result onto its task Record. That Record is the
  message. Fiote reads results, not transcripts.
- Cross-cub questions go *up*, never sideways: a cub that needs something asks
  Fiote, which answers from what it holds or asks the human. Sideways chatter
  between peers is where token budgets die, and where two agents talk each other
  into a wrong answer.
- The human can `steer` any session at any time; steering is delivered after the
  current tool call, which is why it does not corrupt a turn in flight.

This is Lince's own model — Records are the memory, the graph is the plan — so
the agents get it for free rather than needing a mailbox invented for them.

### D8. What Buzz gets right, and what it is not.

`block/buzz` (Apache-2.0, released 2026-07-21) is the closest existing thing to
what Fiote is reaching for, and it is **Rust**: `buzz-core` / `buzz-relay`
(axum, Postgres, Redis, S3), `buzz-cli`, `buzz-acp`, `buzz-agent`,
`buzz-workflow`, with a Tauri + React desktop client. Worth reading; not worth
adopting.

Three ideas transfer:

1. **Human-agent parity.** An agent is not a bot with a webhook. It joins the
   same rooms, holds its own membership, and its actions land in the same log as
   a person's. Lince already has the substrate for this — an agent that can be
   granted things is just an Actor — so parity costs us an identity decision,
   not a subsystem.
2. **The agent holds its own key; the person signs a narrow authorization.**
   Buzz gives every participant a Nostr keypair and has the human sign a scoped
   delegation. The point is the *revocation* property: if the agent is
   compromised you revoke the agent, not yourself. That is the pattern we want.
3. **Git as signed events, not as hosting.** Buzz does NIP-34 — repo
   announcements, patches, issues, review approvals as signed events — plus
   `git-sign-nostr` and `git-credential-nostr`. Feature branches surface as
   channels, CI posts results there, an agent does a first-pass review, and the
   merge decision lands in the same room as its evidence. **Buzz does not host
   repositories.** It is a place where the conversation about a patch is
   signed and auditable. That distinction is the whole design, not a caveat.

What does not transfer: **Nostr.** Adopting `nostr-sdk` would give Lince a
second identity system beside the one it has — ed25519 keys, signed action
intents (`0018_signed_action_intent.sql`), grants, contacts, HLC-ordered ops.
Two key hierarchies is how a project ends up unable to say who authored
anything. Take the delegation pattern, implement it on the signed-intent
machinery already here.

### D9. Code lives in git. Lince is the index and the accountability layer.

The owner asked for "a git repo equivalent" and, in the same breath, for GitHub
to stay the main remote with a Forgejo mirror on the VPS/LAN. The second answer
dissolves the first, and the second answer is the right one.

**There is no blob sync in Lince.** Media is host-local, served from
`/host/media` by `presentation/http/media_assets.rs`; the sync path carries ops
and Loro snapshots and nothing else. So "Lince as a git remote" is not a
feature, it is a prerequisite subsystem — a content-addressed blob store plus a
transfer path — and a bundle of a live repository is tens to hundreds of
megabytes, which neither the op log nor the 30-day sealed mailbox is built to
carry.

Three levels, and the cheapest one is enough:

- **Reference.** A Record holds `remote + branch + sha`. A tiny string, already
  expressible as ops today, zero new machinery. Lince says *what* the work is,
  *who* decided, and *where the bytes are*; git moves the bytes, as it is good
  at.
- **Patch.** A diff is text, so a patch can be a Record body with no blob store
  — Buzz's NIP-34 shape, reachable on the existing op path. This is what makes
  a review conversation self-contained: the thing being discussed travels with
  the discussion.
- **Bundle.** A `git bundle` is one file and would make Lince a real mirror.
  Needs blob sync. Not now, and probably not ever as the default.

**Decision: reference now, patch at review time, bundle never by default.**
Nothing here blocks on blob sync, which is what makes it buildable at all.

### D10. Agent identity, revised. Supersedes half of D4.

D4 concluded "one lasting Actor, many Session Records", and for a Fiote that
only ever works on this Cell that is still right — the ephemeral cub attributes
through `Cause { kind: Fiote, uid: session_uid }` and needs no identity of its
own.

It is wrong the moment an agent's words cross to another Organ. A contact
receiving a message must be able to answer "who wrote this, and on whose
authority" without trusting a column the sender filled in, and
`visible_targets` honours only `subject_uid` or `public` — so an agent that is
granted anything must have a real uid behind a real key.

The rule, then:

- **An agent that authors anything another Organ will see has its own keypair
  and its own Actor Record**, plus a delegation: an `operated-by` assertion to the
  Person, and a signed, narrowly scoped authorization from that Person saying
  what it may do and until when. Revoke the agent, keep yourself.
- **A local-only ephemeral cub has neither.** It is a Session Record under
  Fiote, attributed by cause uid.

So "such Agent belongs to such Person" is an assertion plus a signed grant, and
it is what makes another person's Lince able to display *"proposed by Ana's
reviewer agent"* and mean it.

Cross-person agent messages need less than it sounds: Conversation → Thread →
Message already syncs to a contact, and an Actor with a key can author into it.
What is missing is the delegation chain and **showing authorship on a message**
— today nothing on the surface says a Message came from an agent rather than a
person, and that is the first thing to build, because an unlabelled agent
message in a human conversation is the failure mode.

### D11. Restored by D29. Kept for the reasoning.

*This argued on 2026-08-29 for writing our own loop on `genai`. D22 set it
aside for goose; D29 (2026-08-30) brings it back as the decision. The parts
list lives in D29; the reasoning is here.*

The parts argument: take the boring bottom of every harness — provider
abstraction and the tool-call round-trip — and own everything above it, because
above it is where Lince's model (Records, assertions, grants, Ledger) has to
live.

- **`genai`** (`0.7.0-beta.19`): one API over 26+ providers on their *native*
  protocols, tool choice, streaming, reasoning content
  (`ContentPart::ReasoningContent`). Explicitly **no agent loop** — which was
  the reason to pick it. Pre-1.0, so churn.
- **`rig-core = "0.42.0"`**: provider-neutral messages, completion models,
  portable tools; the loop lives in `rig-agent`, so it can be used loop-free
  too. Brings memory and vector-store contracts we do not need.
- Still true regardless of harness: **`rmcp = "3.1.4"`** for MCP,
  **`gix = "0.87.1"`** for git, **`forgejo-api = "0.11.1"`** for PRs,
  `mistralrs` / `ollama-rs` for local models someday.

Why it lost: writing a loop is the *easy* part of a harness and the least of
what one is. Compaction, retry, steering, session forking, skills, permission
prompts and provider quirks are the hard parts, and they are a year of polish
that goose already has.

### D12. The workflow, end to end.

What the owner does today does not change: code on the laptop, commit locally,
push to GitHub. Everything below hangs off that.

1. **A task becomes a Record.** Either the owner says it, or Fiote proposes it
   and the owner accepts. Decomposition is `part-of` children, exactly as
   `Ontology` already does trees.
2. **A cub takes one task.** Narrow brief, one task uid, the skills it needs.
   It works in a checkout on this machine. Its chatter is lane traffic; its
   result is written to the task Record.
3. **The result is a branch, and the Record holds the reference.** `remote`,
   `branch`, `sha` — read with `gix`. If the work is under review, the patch
   text goes on the Record too, so the discussion is self-contained.
4. **The owner reviews in Lince**, where the conversation, the task tree and
   the patch are one thing, and can `steer` the cub mid-turn.
5. **On accept, a bridge opens the PR.** `forgejo-api` against the LAN/VPS
   Forgejo; the same step can target GitHub. The PR body links back to the
   Record; the Record holds the PR url. Nothing about the repository moves
   through Lince.
6. **Forgejo mirrors GitHub** (its own mirror feature — check Forgejo's docs
   for pull- vs push-mirror before committing to a direction). GitHub stays the
   main remote. Forgejo is the backup and the place PRs from local agents land
   without going through a third party.
7. **Other people see what was granted.** A contact who has the task
   Conversation sees the decisions, the patch and who authored them — their
   Lince, their copy, verified by the delegation chain from D10. Their own
   agents read those Records like any other Record. Cloning the code is
   `git clone`, because the code was never in Lince.

The property worth naming: **Lince holds the reasoning, git holds the code, and
Forgejo holds the merge.** Each is good at one of those and bad at the other
two. Buzz put all three in one relay; that is a fine choice for a company that
wants one server, and the wrong one for a person who already has git working.

### D13. A cub is a full coding agent. Fiote is the orchestrator.

*Owner, 2026-08-29: cubs "have full capabilities normal coding agents have, not
only confined to Lince's Protein Actions". And: "Maybe Fiote can be the harness
an orchestrator, and not write code at all."*

This corrects D7's implicit picture of narrow Lince-only workers. Two roles,
and they are genuinely different programs:

- **Fiote — orchestrator.** Talks to the human, decomposes work into task
  Records, spawns cubs, reads results, asks for approval, reports. Its tools
  are Lince's: create and link Records, read the graph, assert and retract,
  propose field candidates, spawn/steer/stop a cub. It does not have `edit` or
  `write`. Long-lived, one lasting Actor (D4/D10).
- **A cub — a real coding agent.** Full tool set: read, write, edit, bash,
  grep, find, in a working directory. Plus the Lince tools, offered as ordinary
  tools/MCP/skills, so it can attach its result to the task it was given
  without a privileged path. Ephemeral, one task, its own session.

The consequence worth stating: **a cub with bash is a cub that can do anything
the user can.** That is the point, and it is also why D5's permission work is
not optional decoration — a working directory, an allowlist and an approval
prompt are what make "full capabilities" a decision rather than an accident.
The narrow-tool argument from D6 survives only as a *token* argument, and it
now applies to the orchestrator, which genuinely needs few tools.

**Lince's Actions reach a cub as tools, never as a special path.** The Record
already fixes this ("Fiote is a client of that contract, not a privileged
path"), and it is the same contract a Sand uses. An MCP server over the Action
surface is the obvious shape and is the one thing here worth building early,
because it is also what makes repository agents (this session included) able to
drive a running Lince.

### D14. Waking Fiote: assertions, `effect_queue`, and NOT Karma.

*Owner, 2026-08-29: put `#ai` or `#fiote` on a Record — configurable in
`#fiote` — and that wakes Fiote to a task, when the Record also carries a
configured `#wip`.*

The mechanism exists and it is not the obvious one.

- `record_assertion` rows carry `asserted_by`, so "who tagged this" is already
  answerable — which matters, because a tag written by a cub must not be able
  to wake another cub unless the owner said so.
- `effect_queue` (`0001_init.sql:170`) is a durable queue with
  `kind = command | notify`, `status = queued | running | done | failed`,
  `attempts`, and `origin_uid`, plus claim/finish in `store/src/misc.rs`.
  That is exactly a wake queue, already written, already persisted, already
  survives a restart.
- **Karma cannot do this yet.** The owner's own Record says so: Karma intents
  are inert, "nothing dispatches an accepted candidate into an op". Designing
  the wake on a rule that cannot fire would produce a feature that looks built
  and does nothing.

So: an assertion write whose predicate is in the configured wake set, on a
Record that also carries the configured work tag, enqueues one `effect_queue`
row; Fiote's runner claims it and decides. The tag pair is configuration, not
constants — `#fiote` and `#wip` are the defaults, both editable in the Fiote
sand. When Karma dispatch lands, Karma becomes the *configurable front* over
the same queue and this becomes one rule among many. Until then the queue is
the mechanism and the sand is the configuration.

Two things this must not do: fire on assertions that arrived from a contact's
sync (a peer must not be able to spend our tokens), and fire on its own
writes (a cub tagging its own output re-waking itself is the runaway loop the
Karma budget notes already worry about).

### D15. Provider credentials are a real blocker, and there is no place to put them.

*Owner, 2026-08-29: a Fiote sand for "configuring its logins of different llm
providers (dependency for that)".*

Checked. Lince has no secret-at-rest story for this:

- `store/src/logins.rs` is not credentials — it binds a contact Organ to a
  Person, explicitly "a BINDING, not a credential".
- `identity_key` stores **public** keys only.
- `configuration` is a typed singleton of UI/policy settings, unencrypted, and
  it is the wrong shape and the wrong safety class for an API key.
- The one real precedent is `engine::trust::load_or_create_secret` — 32 raw
  bytes in a file created `0o600`, deliberately not in the database, with the
  node key kept distinct from the identity key.

**Follow that precedent.** A provider credential lives in a `0600` file under
the Cell's config directory, one per provider, never in a Record, never in the
op log, never in `configuration`, and therefore never synced to another Organ
or carried in a backup of the database. The sand edits it through a narrow
host call that writes the file and reports only whether a key is *present* and
whether it *works* — the sand never reads a key back. Environment variables
stay supported for the developer path, and the sand must say which source a
provider is using, or "it works on my machine and not in the app" becomes
unanswerable.

This is a dependency of Phase 2, not a late concern: the first
moment a person can type at a cub is the first moment they need a key in it.
### D16. The room, and the agents in it.

Buzz's collaboration model is one relay, channels with members, and every
participant — human or agent — holding a key and a membership. A feature branch
*is* a channel: patches, CI results, the agent's first-pass review and the merge
decision all land in the same room, so the evidence and the decision are never
in two systems.

That shape is right, and Lince cannot express it today. The relevant fact:

- `replica_grant` is keyed `PRIMARY KEY (root_record, contact_organ)`
  (`0042_individual_replica.sql:40`). **One root can hold a grant row per
  contact**, so a root granted to several contacts fans out to all of them,
  each with its own `offered`/`accepted` state, and revoking one deletes one
  row. Containment is a separate question and is already settled: every Record
  inside carries `record.replica_root` pointing at the root, resolved once at
  creation, so the grant covers the whole tree without walking assertions.
- But `threads.rs` builds Conversation as "the root, shared with **one**
  contact", and `start_conversation` takes a single `contact_organ`.

So the transport already does multi-party; the *semantics* are deliberately
two-party. A room is therefore not a new subsystem — it is a Conversation-like
root granted to N contacts — and what is genuinely missing is everything that
makes a room feel like a room:

- **Membership is not a list.** Each grant is independent, so nobody can
  enumerate who else holds the root, and there is no agreement on who is in.
  Somebody has to be the one who states membership, and the honest answer for a
  peer-to-peer system is that the room's creator does, as a Record everyone can
  read and disagree with.
- **Removal is not deletion.** Revoking a grant stops future ops; it does not
  recall what was already delivered. A room must say that plainly rather than
  implying a moderator power that does not exist.
- **Fan-out is not free.** Every message becomes an op to N outboxes. This is
  the second reason D5's lane/Record split matters: agent chatter in a
  five-person room would be five times the flood.

**Agents in the room follow D10 unchanged.** An agent that posts into a shared
room has its own key, its own Actor Record and an `operated-by` delegation from
the person who runs it, so every other member's Lince can render "Ana's
reviewer agent, acting for Ana" and verify it rather than take the sender's
word. That is precisely Buzz's property — revoke the agent, not the person —
reached with the machinery already here instead of a second identity system.

**And the code still does not travel through the room** (D9). What is shared
is: the task tree, the patch text under review, the branch reference, the PR
url, and the conversation — all small, all text, all already expressible as
ops. Each person clones from git and pushes to git. The room is where the
*reasoning* is shared and where it stays attributable; git is where the bytes
are; Forgejo is where the merge happens. A room whose members include two
people and three agents then works the same as a room of two people, which is
the actual goal of "human-agent parity" — not that agents are special, but that
nothing has to be special-cased for them.

### D17. Correction: an Agent Actor already exists, and `operated-by` is the link.

Found 2026-08-30, and it invalidates the "needs building" half of D10.

`Action::CreateAgent { head, operated_by }` (`engine/src/actions.rs:848`) is
shipped. It:

- ensures the Concept `actor` with `person` and `agent` beneath it, so "is this
  an Actor" is answered by the Concept DAG for both kinds;
- creates a `RecordKind::Person` Record — deliberately, because standing, the
  four login doors and dormant absorption are all written about people and
  widening the word would blur them;
- asserts `agent` on it and sets `agent` as its identity predicate;
- asserts **`operated-by`** → the Person answerable for it, resolving that
  Person *before* creating anything, because "an Agent nobody is answerable for
  is the thing this field exists to prevent";
- and needs nothing for the Organ half, since every Record already carries the
  Organ it originated at.

So: **D10's proposed `agent-of` is wrong. The predicate is `operated-by` and it
is built.** "Link Actors to say that such Agent belongs to such Person" is not
work; it is `CreateAgent` with `operated_by` set.

`assigned-to` also exists as a predicate (the Record sand gives assignees their
own section rather than counting them as links).

What is actually missing is narrow:

- **A signing key per Agent.** `identity_key` is keyed `(actor_uid, key_id)`
  and holds public keys; it is populated when a client publishes a key for a
  Person through the action-intent session, and by transfer delivery. An Agent
  *is* a Person Record, so the path exists — but nothing publishes a key for an
  agent uid today, and no surface offers to.
- **Authorship display.** Nothing on a Message says an agent wrote it, or which
  Person operates that agent.

Those two, not a new identity model, are D10's real remainder.

### D18. The owner's design, 2026-08-30.

Written in the owner's framing. Objections are in *Criticism* below, kept
separate on purpose.

**Fiote is a kind, not a singleton.** Each Fiote is an Agent — one
`CreateAgent` Record — and there are many. A Fiote's *behaviour is its Record*:
the description on its Actor-Agent Record is its prompt. Roles follow from
that, not from code: a **Receptionist** Fiote is woken by a notification and,
by its own prompt, delegates to a **Builder** or **Architect** Fiote, which in
turn delegates to a cub.

**One shared prompt, held as a Record.** A Record named `Agent` carries the
system prompt every Fiote shares. All Fiotes are *assigned to* it, so reading
that Record and following its links discovers its assignees — which is how a
notification finds who to notify, human or agent, with the context they need.

**Notification is a Karma primitive.** A Condition of the shape
`#ai and #wip and not assignee` matches a set of Records; the consequence is
"notify those assigned to Record X about the Records that reached this state".
The same primitive serves people and agents; only the delivery differs.

**Collaboration is free.** Fiote 1 talks to Fiote 2; a cub of Fiote 1 talks to
Fiote 2 and to Fiote 2's cubs. Nobody is a mandatory relay.

**Limits are per task.** What a Fiote or cub may edit — the source checkout,
the Lince database or workspace, its own harness — follows from the task it is
doing.

### D19. What of D18 already exists.

Most of it. Checked, so the build reduces to a few pieces rather than a system.

- **Multiple Fiotes:** `CreateAgent`, above. Behaviour as the Record's body
  needs no schema — Records have bodies.
- **Assignment and discovery:** `assigned-to` is a real predicate, assertions
  are the link graph, so "all Fiotes assigned to the `Agent` Record" is an
  ordinary assertion query.
- **The Condition is a Protein view, not a new language.** `#ai and #wip and
  not assignee` is a *selection over Records*, and Protein is already the
  selection layer. Karma can consume it directly:
  `InputSource::SavedProtein { view }` (`karma/ast.rs:191`) takes a saved
  Protein as a rule input. Do not add a second query language to Karma —
  Karma's `Condition` is numeric on purpose (condition → gate → carry, on exact
  decimals) and forcing set-matching into it would wreck the one thing it does
  well.
- **The trigger exists:** `TriggerSource::Fact { record, concept }`
  (`karma/ast.rs:165`) fires on a Fact about a Record or a **Concept** — which
  is precisely "an assertion I care about was made".
- **The route exists:** `CandidateRoute::{Observe, Recommend, Draft, Ask, Act}`.
  A notification is `Ask` — put this in front of someone — and the vocabulary
  is already frozen in the revision hash.

What is missing is the **consequence**: nothing dispatches. Same wall as D14.
So the notification primitive is designed against Karma's vocabulary and
*implemented* on `effect_queue` until dispatch lands, at which point Karma
becomes its front without the design changing.

One more confirmation of D15 in passing: `InputSource::SecretMetadata { secret }`
exists in the AST with no secret store anywhere behind it.

### D20. Editing limits: three zones, three different risks.

"What may it edit" is not one question. The zones the owner named fail
differently, so they get different answers.

- **The source checkout.** A cub gets a working directory and edits inside it.
  Mistakes here are recoverable — that is what git is for — so this is the
  permissive zone, bounded by the directory and an allowlist, not by review of
  every write.
- **The Lince database.** **No cub ever writes it directly.** Every change goes
  through Actions, which is already the rule in the Record — "Fiote is a client
  of that contract, not a privileged path" — so this needs enforcement, not a
  new policy. The reason is not tidiness: a direct write skips the Ledger, the
  op log and the outbox, so it is invisible to attribution and never reaches a
  contact. There is also no backup story, so a bad direct write is
  unrecoverable in a way a bad commit is not.
- **The harness itself.** A cub editing the code that runs it is the one zone
  with no undo *and* no boundary — a broken harness cannot be relied on to
  report that it is broken. Allowed only as an ordinary source-checkout task in
  a separate checkout, never against the running build, and never by a cub
  spawned by the Fiote whose own code is being changed.

The general rule underneath: a task names its zone before the cub starts, and
the zone is a property of the task Record, not of the cub's good intentions.
### D21. If we start with Pi: what the build and runtime actually are.

Asked 2026-08-30. Short answer, because it is a short answer.

**Build.** Nothing changes in the Rust build. `crates/fiote` compiles with no
knowledge of Pi — it spawns a path. `cargo check -p lince-fiote` proves the
supervisor compiles and `cargo test -p lince-fiote` proves it can drive a real
`pi` **if one is installed**; neither proves Pi is present for a user, and the
test says so by refusing rather than skipping. Pi is not a cargo dependency and
never appears in `Cargo.lock`.

**Runtime process tree.** One extra process per cub:

    lince (host)
      └─ crates/fiote::Supervisor
           ├─ node → pi --mode rpc     (cub 1, own cwd, own session file)
           ├─ node → pi --mode rpc     (cub 2)
           └─ …

Each child owns stdin/stdout/stderr as JSONL, lives in the host process's
lifetime (D2), and holds its own session file under `--session-dir`. A cub that
runs `bash` forks further children of its own, which the supervisor does not
see and cannot account for — worth knowing before trusting any "kill the cub"
button.

**The dependency is Node, not Pi.** Three ways to have it, and they are the
open question:

- **On PATH.** Zero work, works today, and means the app fails on a machine
  without Node with an error a normal person cannot act on.
- **Vendored** (`vendor/pi/node_modules`, what M0 did). Reproducible for a
  developer, and invisible to git — `node_modules/` is ignored, so a fresh
  checkout has no Pi. It also does not solve Node itself.
- **Flake input.** Right answer for this repository's own development, since
  `flake.nix` already provisions the toolchain. Does nothing for a packaged
  desktop build.

**What a shipped desktop build would carry.** The Tauri bundle would have to
include a Node runtime (~50 MB before the npm tree) plus `pi`'s dependency
graph, per platform, and keep both patched. That is the real cost of starting
with Pi, and it is why D11 treats Pi as the reference implementation rather
than the destination: a Rust harness in-process removes the runtime, the
bundle weight and the patching, and `Cub` is already the seam that lets the
swap happen without the sand noticing.

**Subscriptions are the catch.** "Point a subscription at it" is not an API-key
setting. A Claude or ChatGPT subscription is redeemable only through that
vendor's own client, which owns the OAuth — which is exactly why goose reaches
subscriptions *via ACP*, by proxying to Claude Code or Codex. So subscription
support and "no external agent process" are in tension no matter which harness
we pick: keys are ours to hold (D15), subscriptions are not. If subscriptions
matter more than process purity, ACP comes back — as a **client** path for
auth, never as the surface we expose. That is a real amendment to D11's
dismissal of ACP.

**Where goose fits.** If the wish is "ready in Rust, forkable, subagents
built in", `block/goose` is the closest: Rust, Apache-2.0, native subagents,
desktop/CLI/API, donated to the Agentic AI Foundation. Two caveats before
anyone types `cargo add`: the crates.io name `goose` is an unrelated
load-testing framework, so embedding Block's goose means a git dependency or a
fork; and its subscription support is the ACP path above, not something it owns.

### D22. The harness is goose, bundled. ACP is the door for anyone else's.

*Superseded by D29 (2026-08-30): Fiote is our own harness; goose is a reference
to read, and an optional ACP backend for anyone who wants their own agent or
needs a subscription. The comparison below is why goose, not Pi, is the one
worth reading.*

*Owner, 2026-08-30, as a hard restriction: embed the best harness and serve it
by default, and let people use their own.*

**Default: `block/goose`.** Rust, Apache-2.0, donated to the Agentic AI
Foundation, ~29k stars. It is chosen over Pi on four counts that come straight
from the restriction:

1. **Subagents are native and are already our cub model.** goose subagents "run
   in isolated sessions with their own context windows, extension sets, and
   turn limits", the parent delegates and "receives structured JSON summaries
   back", and "if a subagent fails, the parent gets a failed task result rather
   than a crashed session". That last clause is failure isolation we would
   otherwise have to build, and D13's Fiote/cub split stops being an invention.
2. **Extending it needs no rebuild** — see below, which is the whole of the
   owner's recompile worry.
3. **One Rust binary, not a language runtime.** Pi is excellent and costs a
   bundled Node runtime per platform (D21). goose ships as a binary plus
   `goosed`, its headless daemon with an API, which is exactly the shape
   `crates/fiote` already supervises.
4. **Forkable in the language we want.** If a behaviour is wrong we can change
   it, in Rust, in a fork we control.

Two caveats, stated so nobody trips: the crates.io name `goose` is an unrelated
load-testing framework, so embedding Block's goose is a git dependency or a
fork rather than `cargo add`; and its subscription support is the ACP path
below, not something it owns.

**Bring-your-own: ACP.** `agent-client-protocol = "2.0.0"`. This reverses D11
and it is now a requirement rather than a nicety: the restriction says people
must be able to point Lince at their own agent, and ACP is the protocol that
already means that. Pi becomes one of the supported "your own" options through
the existing `pi-acp` adapter, which is the happy ending for "I like Pi and I
need Rust" — Pi is reachable without Node being load-bearing for anyone who
does not choose it.

It is also the **only path to a subscription**. A Claude or ChatGPT
subscription is redeemable through that vendor's own client, which owns the
OAuth; nobody can hand us that. So "point a subscription at Lince" means
speaking ACP to Claude Code or Codex, which is precisely how goose does it.
Keys we hold ourselves (D15); subscriptions we borrow.

### D23. Editing the harness from inside Lince: three layers, one rebuild.

The owner's worry — "if it's Rust do we recompile at runtime to change it?" —
has a clean answer, and it is the strongest argument for goose rather than
against it.

- **Data. Edited from Lince, takes effect immediately.** System prompts (Agent
  Records, D18), recipes, tool allowlists, model and thinking level, which MCP
  servers a given agent may use, working directory. All of it is configuration,
  and all of it is exactly what a person means by "editing Fiote". No build, no
  restart beyond starting the next session.
- **Capabilities. A separate process, no goose rebuild.** goose's extension
  mechanism *is* MCP, and every new MCP server "immediately becomes available
  to Goose without any changes to Goose itself". So a person extending Fiote
  writes an MCP server in whatever language they like and points an agent at
  it. Lince's own Actions become one of these (Phase 2) — which is how Fiote gets
  Lince powers and how a user adds their own on the same footing.
- **The loop. Rust, needs a rebuild.** Turn handling, compaction, retry,
  streaming. This is the *right* thing to require a build for: nobody should
  edit a turn loop from a chat box while it is running, and Pi's advantage here
  (`.ts` extensions through jiti) buys editability we get from MCP anyway, at
  the price of a bundled Node runtime.

So: nothing a person would call "editing Fiote" requires a compiler, and the
one thing that does is the thing that should.

### D24. Agents read Lince. Where they talk is a habit, not a wall.

*Owner, 2026-08-30: stop thinking about brothers and cousins. Every agent and
subagent can read all of Lince and write a message wherever it wants, for
communication or for thinking. By default they should not pry around — context
is expensive — and they should converse in a common, well-known place.*

Adopted, and the kinship framing from the earlier draft is deleted. It was
solving a problem that does not exist: who spawned whom says nothing about who
should be involved.

**Reading.** A cub reads as its Fiote's Actor, so "all of Lince" means all of
what that Fiote can see. `visible_targets` gates every read by grant, plus
whatever the Actor itself created. So there is a choice to make explicitly
rather than by accident: either **each Fiote Actor is granted broadly** — which
is what "read all of Lince" plainly means and is a reasonable default on a
personal Cell — or "all of Lince" quietly degrades to "all the little this
agent happened to be granted", which reads as the agent being broken. Take the
first, say it out loud in the Fiote sand, and keep the grant visible and
revocable like any other.

**Writing.** Anywhere. A message is a Record; agents write Records. The only
thing that matters is that the write is attributed, which it already is: the
authoring Actor is the Fiote, and `Cause { kind: Fiote, uid: session_uid }`
names the session inside it. Nothing needs to be forbidden for accountability
to hold.

**Not prying is a habit, and habits live in the prompt.** The shared `Agent`
Record (D18) is where it belongs: read what your task needs, ask before you
wander, and hold conversations in the known place rather than scattering them.
This is guidance, not enforcement — an agent that ignores it is wasteful, not
dangerous, and the cost lands on the person who owns it, visibly, in the token
readout. That is the right kind of pressure.

**The common place is one Record per subject, not per pair.** A task's thread
for work on that task; a standing thread for the agents themselves. Anyone who
can see it can read it and can join; nobody has to be told who their siblings
are.

**What survives from the earlier objection**, and it is small: a *conclusion*
belongs to an Actor, not to a session. Two cubs can talk each other into an
answer; when that answer becomes the task's result, a Fiote wrote it.
### D25. Everyone eligible is notified; they agree by claiming.

*Owner, 2026-08-30: notify all eligible agents, and imbue a policy that when
two are notified they agree between themselves who takes it — so a Receptionist
Fiote is one option, one Builder is another, many Builders a third, and none of
it is wired in.*

That is the design. What follows is how to make it cost almost nothing.

**The policy, as behaviour.** Every agent notified follows the same rule:
*before you begin, claim the work; if it is already claimed, stand down.* The
claim is a write everyone can see — an assertion on the task naming the
claiming Actor — so the agreement is real and inspectable rather than implicit.
First write wins; the store settles ties, because two writes cannot both be
first.

**Why the claim rather than a conversation.** A genuine negotiation costs one
model call per agent per notification, and produces an answer — "you take it" —
that nobody will read again. The claim is the same policy with the deliberation
removed: everyone is notified, everyone follows the rule, one ends up with the
work, and the losers spend nothing. If a decision ever genuinely needs
discussion, the agents can hold it in the common place (D24) — but it should be
a choice, not the default cost of every notification.

**Roles are configuration, exactly as asked.** A Fiote whose prompt says
*receive and route* claims the work and hands it on; one whose prompt says
*build* claims it and does it. Put one Receptionist in front of several
Builders and you have a routing tier; point notifications straight at Builders
and you have none. Nothing in the mechanism knows the difference — that is the
point.

**Capacity stays a number, not a judgement.** Each agent has a limit on
concurrent cubs. At the limit it does not claim; under it, it does. A model is
never asked to reason about whether it is busy, because a model asked that will
sometimes decide to be helpful.

**Standing down must be free.** An agent that finds the work claimed does
nothing further — no message, no summary, no note. Otherwise "everyone is
notified" turns into everyone writing a Record saying they did not take it.

**Names for this, since it comes up:** the queue form is *competing consumers*;
the negotiate-and-award form is the **Contract Net Protocol** (Smith, 1980);
the modern harness word is orchestrator or supervisor. What is written above is
competing consumers wearing the owner's policy, and it upgrades to Contract Net
without redesign if a bid ever needs to express capability — bids computed from
queue depth and capability tags first, prompted only if that fails.
### D26. The notification, in plain words.

The earlier version of this section led with internal names and explained
nothing. Here it is as four questions. Each has one answer, and the four are
independent — that separation *is* the design.

**1. Which Records am I watching?**
A saved filter. "Every Record tagged `#ai`, also tagged `#wip`, with nobody
assigned to handle it." You build it once, you can look at it, and it lists
Records the way any other view does.

**2. What makes it look?**
Somebody adds or removes a tag. The filter is re-checked at that moment — not
on a timer, because a timer re-scans a growing pile forever and gets expensive
without anyone noticing.

**3. Who hears about it?**
The Actors assigned to a Record you name. Usually that is not one of the
Records the filter returned — you point at a Record that holds your agents, and
its assignees are the recipients. **These are two different questions and the
surface must show them as two boxes**: *what did we find* and *who should hear
about it*. People and Agents are both Actors, so one rule covers both, and only
the delivery differs.

**4. What actually happens?**
A row lands in a work queue, one per finding. Every recipient sees it. The
first one free claims it and the others stand down (D25). Nothing is decided by
anyone on anyone else's behalf.

**Worked example.** You tag a Record `#wip` and `#ai` and assign nobody. The
tag write wakes the check. The filter matches that Record. The rule's recipient
Record is *My Agents*, which has three Fiotes assigned. One queue row appears,
visible to all three. The Receptionist claims it, reads the task, and hands it
to a Builder by assigning the Record to it — which is itself a tag write, which
wakes the check again, which now matches nothing because the Record has an
assignee. The loop closes on its own.

**Where each part lives in the code that exists.** The filter is a saved
Protein view, and Karma can already take one as a rule input
(`InputSource::SavedProtein`). The "somebody tagged it" trigger is
`TriggerSource::Fact { record, concept }`. The "put this in front of someone"
route is `CandidateRoute::Ask`, already frozen in Karma's vocabulary. The queue
is `effect_queue`, which has `status`, `attempts` and `origin_uid` and its
claim/finish helpers already written. **Karma cannot yet dispatch anything**
(its intents are inert, by the owner's own Record), so the row is enqueued
directly until that lands, at which point Karma becomes the front and nothing
above changes.

**Hardening, all of it learned from ways this shape goes wrong:**

- **Dedupe.** One live row per (rule, Record, recipient). Tag and untag five
  times and you get one row, not five cubs.
- **Never fire on a contact's assertion.** A tag that arrived over sync from
  someone else's Organ must not spend our tokens. The assertion carries who
  asserted it, and every Record carries the Organ it came from.
- **Never fire on our own agents' writes** unless the rule says so explicitly,
  or a cub tagging its own output wakes itself forever.
- **A ceiling per rule per window.** Over it, the rule pauses and says which
  rule and which cycle — the shape Karma already chose for runaway cycles —
  rather than being silently killed or silently infinite.
- **A visible queue.** What is waiting, who claimed it, what failed and why.
  A notification system nobody can inspect cannot be told apart from a broken
  one.

### D27. One `goosed`, many sessions, and ACP is the only client we write.


*Superseded by D29: we do not spawn `goosed` by default. What survives is the
ACP client, kept as an optional backend — and the observation that one daemon
can host many sessions, which is the shape our own in-process sessions take.*

Checked 2026-08-30, and it simplifies the architecture rather than complicating
it.

`goose-server` (the `goosed` daemon) is axum-based and exposes goose over a
REST + SSE HTTP API with a WebSocket interface — the same backend the goose
desktop app uses. It runs **many sessions concurrently**, with an agent per
session and extension sets isolated between them: enabling an extension in one
session does not affect another. It supports auth by secret key.

So the process tree is one daemon, not one process per agent:

    lince (host)
      └─ crates/fiote::Supervisor
           └─ goosed                    (one child, many sessions)
                ├─ session: Fiote "Receptionist"
                ├─ session: Fiote "Builder"
                └─ session: cub of Builder, own context and extensions

That answers the owner's condition directly — embed it at build, spawn many
Fiotes and their agents from it, configure their prompts in Lince — and it is
cheaper than a process per cub, which is what the Pi shape would have been.

**And goose speaks ACP itself.** The `goose-acp` crate exposes session
management, streaming, tool execution with permission flows and session
resumption over a single `POST /acp` endpoint (HTTP with websocket upgrade). So
the default harness and the bring-your-own harness speak **the same protocol**,
and Lince writes **one** client:

- default: Lince → ACP → bundled `goosed`;
- bring your own: Lince → ACP → whatever the person points at, Pi included via
  `pi-acp`.

This retires the last argument for a bespoke protocol adapter. `Cub` stays a
seam, but what is behind it is an ACP endpoint rather than a family of
per-harness dialects, and the M0 supervisor keeps its job of owning the child
process and its lifetime (D2).

**Dependency: neither a crate nor a git dependency — a binary.** *Owner asked
2026-08-30 whether goose can come in as a crate.* Checked: `goose-providers` is
published at `0.1.0-alpha.7` (with `goose-provider-types`), but
**`goose-server`, `goose-acp` and `goose-mcp` are not on crates.io at all** —
so the pieces we actually need cannot be a crate dependency today.

That does not mean a git dependency. It means we never link goose as Rust at
all: **spawn `goosed` and speak ACP over HTTP.** The only new crate is
`agent-client-protocol` from crates.io. No `Cargo.toml` entry for goose, no
building seventy crates in our workspace, no version coupling to their
internals, and `crates/fiote::locate` already does exactly this job for a
binary. This supersedes the "git dependency" line written earlier in D22.

Fork later only if a behaviour is wrong, and then in Rust, which was the point
of choosing it.

**Still to check before Phase 2:** whether `goosed`'s per-session prompt and
extension configuration can be set per session at creation (which is what
Agent Records configuring Fiotes requires), or only globally. The session
isolation reported for extensions suggests per session; confirm it against the
API rather than assuming.

### D28. Predicates are already Concepts. Nothing is hardcoded English.

*Owner, 2026-08-30: do not hardcode `part-of`, it is English and not everyone
speaks it.*

Correct instinct, and the schema already agrees — this needs no new mechanism
and no tokens spent guessing:

- `record_assertion.predicate_uid` **references `concept(uid)`**. A predicate
  is a Concept, not a string. `part-of` is a *label on a uid*, not the identity.
- `concept_name(concept_uid, lang, name)` holds names per language, so the same
  uid can be `part-of` in English and whatever the owner wants in Portuguese,
  both pointing at one Concept.
- `concepts::resolve` accepts a uid, the canonical name, **or any localized
  name**, so typing either word finds the same thing.
- `concept_parent` is a DAG and `descendants_including` already powers
  `concept_in @food` matching `@apple` through `apple → fruit → food`.
  `concept_equivalence` exists for saying two Concepts mean the same.

**So the rule for Fiote is: never compare a predicate string.** Resolve the
containment Concept once, then match anything beneath it or equivalent to it.
A person who renames it, translates it, or introduces a narrower notion of
containment keeps working, because the match was on the Concept and not on the
word.

This also disposes of "spend a cub reading the links to guess which are child
tasks": the graph states it. Use a model only where the graph genuinely does
not — a link whose Concept says nothing about containment — and even then the
answer it produces should be written back as an assertion, so the same question
is never paid for twice.

**Which Concepts an agent follows is configuration, and configuration is data.**
*Owner, 2026-08-30: it must be configurable — by code, or by text in the system
prompt?* Neither. By code would mean recompiling to change which links an agent
cares about, which is exactly what D23 forbids; by prompt would mean a uid
living in a string a model can mangle, and matching that must be exact and free.

So: a Fiote's Agent Record carries a `record_extension` row —
`record_extension(record_uid, namespace, version, fds)` with
`UNIQUE(record_uid, namespace)`, already in `0001_init.sql` — under a
`lince.fiote` namespace, holding **resolved Concept uids**: which link Concepts
this agent follows, which it ignores, which tags wake it. The matcher reads
uids and never sees a word.

The prompt then gets a *rendered* line derived from that data — "you follow
links that mean containment (`part-of`, `parte-de`)" — in the reader's own
language, for explanation only. **Data is authoritative, the prompt is a view
of it, never the reverse.** Changing which Concepts a Fiote follows is editing
a field in the Fiote sand, and the next session's prompt says something
different because the data did.

### D29. Fiote is our own harness after all. The parts, and what we take.

*Owner, 2026-08-30, reversing D22: build it from fundamental blocks — abstract
the LLM providers and tool management, write the rest ourselves — so we learn
how to build the best agent for Lince, control spawning, headless or not, and
fit the ecosystem instead of being constrained by someone else's.*

**Agreed, and this is the right call for this project.** `AGENTS.md`'s standing
rule applies: where the cheap option and the best long-term architecture
differ, the reason to pick cheap does not exist here. goose and Pi are both
built for a person at a terminal editing a git checkout; Fiote is built for a
person inside Lince editing a graph. Every place those differ, an adapter would
have been a translation layer that never stops costing.

Two honest caveats, stated once and then dropped:

- **Steal shapes, do not re-derive them.** Pi's RPC command vocabulary is a
  good specification of what a session must expose; goose's per-session
  extension sets are the right granularity for capability; ACP's
  permission-request shape is a solved UI contract. Read all three, copy the
  shapes, write the code.
- **A subscription cannot be redeemed by our own harness.** Claude and ChatGPT
  subscriptions are only spendable through their vendor's own client. So keep
  an **ACP client as an optional backend behind the same seam** or that
  capability is simply gone. That is a constraint, not a preference.

#### The split the owner asked about

*"We are just making code that does ergonomy and structure around giving it
instructions in its context for things to read, no?"*

Half. Context assembly is exactly that — ergonomics, and it is most of the
value. But three things must never be instructions:

- **Traversal is a query, not a request.** Walking `record_assertion` by
  Concept and direction is deterministic and free; asking a model to do it
  means paying tokens to re-derive what SQL answers exactly. **Retrieval
  decides what enters the context before the model sees anything; instructions
  only shape what it does with what arrived.**
- **Budget is arithmetic.** A ceiling that a model can talk itself past is
  not a ceiling.
- **Permission is a gate.** Advisory permissions are documentation.

#### Traversal policy, concretely

This is the owner's "relationship importance" idea, and it needs no new
mechanism — it is the `lince.fiote` `record_extension` from D28. Per rule:

- **Concept** — which link, by uid, matched through the DAG so narrower kinds
  come along (D28);
- **direction** — subject→object or object→subject, native because
  `record_assertion` carries both columns, and this is the "of this side" the
  owner asked for;
- **depth** — how far to follow;
- **action** — *glance* (head and state only), *summarize* (hand the subtree to
  a cub and put its summary in the overseer's context), or *read in full*.

So "children of the task, one level, read in full; everything that references
this task, any depth, summarized by a cub" is four fields, not a paragraph of
English. The prompt then carries a rendered sentence explaining it, in the
reader's language, derived from the data and never the source of it.

#### The walk itself, and the only part that is actually hard

*Owner: "if we coded something that when the model checks for records it
ingests the tree of certain links from the record in question, then it would be
fine — is it not that?"*

**It is exactly that.** Start at the Record, follow the links the policy names,
put what you find in the context. Nothing subtler is going on. The whole of the
design work is what happens when the result is too big, so:

- **It is a graph, not a tree.** Records can link back to each other and
  around; the same Record can be reachable by two paths. So the walk carries a
  visited set. This is the one place the naive version genuinely *breaks*
  rather than merely costing too much — without it, a cycle is an infinite
  walk.
- **Three bounds, and only one of them matters.** Depth and breadth-per-node
  are cheap guards. The real limit is a **token budget**: fill until it is
  spent, then stop.
- **Order closest first.** Then when the budget runs out, what was dropped is
  the far material rather than the Record's own children. Breadth-first from
  the starting Record, cheapest form of the right answer.
- **Head-and-state or full body is a budget decision, not a semantic one.**
  That is the only reason *glance* exists as an action; if context were free
  everything would be read in full.
- **Say what was cut.** "12 more children not included" in the context itself,
  so the model can ask for them and a person can see why something was missed.
  Silent truncation is how an agent confidently answers from half the picture.
- **Deterministic.** Same Record, same policy, same data → byte-identical
  context. That is what makes provider prompt caching actually hit, and what
  makes a session reproducible when something goes wrong.

Everything above is ordinary code over `record_assertion`. No model is
involved, nothing is embedded, and the *summarize* action is the only rung that
spends tokens — deliberately, and only where the owner's policy asked for it.

#### The fundamental parts

In the order Phase 2 needs them.

1. **Provider layer — taken, not written.** `genai`: 26+ providers on their
   native protocols, streaming, tool calls, reasoning content. The one part
   with no upside to writing ourselves.
2. **Our own message and content model.** Turns; content parts for text,
   thinking, tool call, tool result, image. Everything keys off this, and
   provider types leak into everything if they are allowed to.
3. **Tool registry and dispatch.** A trait carrying name, JSON schema and a run
   function; `rmcp` so an external MCP server registers as an ordinary tool.
   Lince's Actions are tools here, on the same footing as anyone else's.
4. **The turn loop.** Send → stream deltas → collect tool calls → execute →
   append results → repeat until no calls remain or the budget stops it.
   **Cancellation and mid-turn abort from the first version** — retrofitting
   them into a loop that assumes it runs to completion is painful.
5. **The session as an append-only event log.** Already Lince's idiom, and it
   makes replay, fork and resume fall out of the design instead of being
   features built on top of it.
6. **Context assembly.** Shared `Agent` Record prompt, plus the Fiote's own
   body, plus the task brief, plus traversal results, plus tool schemas. This
   is the Lince-specific part and the actual reason to build our own.
7. **Budget and accounting.** Tokens and cost per session and per Fiote, with
   ceilings that stop rather than warn.
8. **Permission policy.** Allow / ask / deny per tool call with a callback to
   the surface. This is where D13's "a cub with bash can do anything you can"
   gets its answer.
9. **Steering queue.** A message that lands after the current tool call rather
   than in the middle of a stream.
10. **Compaction.** Summarize and replace behind a pinned prefix, recording
    what was dropped. Do not let it be silent.
11. **One typed event stream out.** The sand, the lane and any Ledger writer
    subscribe to the same events. Headless is then simply nobody subscribing —
    which is what the new interface needs.
12. **A scripted provider for tests.** Deterministic, no network, no key. This
    is what makes `cargo test -p lince-fiote` mean something, and it is the
    same shape the Ontology's DST work already wants.

#### Structural consequence

Sessions become **in-process**. `crates/fiote`'s supervisor stops supervising
child processes and starts owning session tasks. The attach / detach /
replayable-backlog design survives unchanged — that was the part of M0 worth
building — but `locate()` and the spawn path become dead code for the default
backend, and live on only for the optional ACP backend that talks to somebody
else's binary.

### D30. File sharing: the hash rides the Record, the bytes ride iroh.

*Not scheduled (owner, 2026-08-30). Kept because the compatibility check is
worth not repeating, and because it retires a blocker the rest of the document
was planning around.*

*Owner, 2026-08-30: pick an Organ, send a file in a message, they click to
download it.*

D9 said this needed a content-addressed blob store first, and treated that as a
subsystem standing in the way. It mostly is not: **most of it already ships
with the transport Lince uses.**

- `iroh = "=1.0.3"` is already a workspace dependency.
- `iroh-blobs = "0.103.0"` — "content-addressed blobs for iroh", BLAKE3, with a
  filesystem store behind the `fs-store` feature.
- **Verified compatible**: `iroh@=1.0.3` and `iroh-blobs@0.103.0` resolve
  together into one lockfile with a single `iroh 1.0.3`. This was checked, not
  assumed.

So the design is:

- **The attachment is a tiny op.** Hash, filename, size, mime type — a handful
  of bytes on a Message Record. The op log, the Ledger and every contact's feed
  stay exactly as small as they are today, and nothing about sync's shape
  changes.
- **The bytes move out of band**, over the iroh connection the two Organs
  already have, fetched when someone clicks. Content-addressed, so a file
  received twice is stored once and a corrupted transfer is detectable rather
  than merely suspicious.
- **The UX is the one asked for**: choose an Organ, attach, send; on the other
  side, a Message with a file on it and a button.

**The honest failure case, which must be visible.** Fetch-on-demand means a
file whose sender is offline cannot be retrieved right now. That is the same
property the sealed mailbox has, and it needs the same treatment: "not fetched
yet — the sender is not reachable" is a state the surface says out loud, never
a spinner that looks like corruption or an error that looks like the file is
gone.

**What this retires.** D9's ladder (reference / patch / bundle) was built
around not having a blob store. Reference-and-patch remains the right default
for *code*, because a repository is not an attachment and git is better at
moving it. But "Lince cannot carry bytes" stops being true, so a bundle, an
image, a PDF or a design file in a conversation is now an ordinary thing rather
than a blocked one.

**Rejected: base64 in the body or an extension.** It works today with no new
dependency, and it puts a megabyte of payload into an op that is replicated to
every grantee and kept forever. The hack the owner named — a Command that
stuffs file contents into Record bodies — is the same trade in a different
shape, and they already said it sucks. They are right.
### D31. The session IS a thread. This is the centre of the whole design.

*Owner, 2026-08-30, with a diagram: what I say to my agent is a Message, what
it says back is a Message, the session is a Thread. People can read it, and
other people and their agents can talk in it. My conversation with my local
agent is saved in Lince; the Record the thread hangs off becomes the refined
plan, and branches into child tasks.*

This is the thing the rest of the document was circling. Adopt it.

#### Reconciling with D5

D5 said chatter goes on lanes and only decisions become Records, because a cub
emitting hundreds of turns an hour would flood the op log. That argument is
still correct, and it is not in conflict with this — it just needs the line
drawn in the right place:

- **A user prompt → a Message Record.**
- **A completed assistant turn → a Message Record.**
- **Tool calls → an envelope on the turn, not the output** (refined by D33):
  what was called and what it acted on, ok or error, duration, size. The
  payload itself is live-only unless someone promotes it. Collapsed by default,
  because most of them are noise and any of them may matter.
- **Streaming deltas → lane traffic.** A token fragment is not a message. It is
  the sound of a message being written, and nobody needs it replicated.

So the flood D5 feared came from the deltas, not from the turns. A
conversation's worth of turns is the same order of magnitude as a conversation
between people, which the Thread model was built for.

#### Why it works with no new mechanism

Everything in the owner's diagram is already expressible:

- **A VPS is just another Organ.** "The agent reads a thread on the VPS" means
  the thread is replica-granted to this Cell, the agent reads the local copy,
  and its writes sync back. There is no remote read and there does not need to
  be one.
- **Several people and several agents in one thread** is the multi-party grant
  from D16: `replica_grant` is keyed `(root_record, contact_organ)`, so one
  root fans out to as many contacts as it is granted to. This promotes D16 from
  a far milestone to load-bearing — the diagram does not work without it.
- **"Its agent listening to make changes locally"** is exactly the wake path of
  D25/D26, with an arriving Message as the trigger instead of a tag. That
  mechanism now has its canonical use case rather than a hypothetical one.
- **Agents are Actors** (D17), so "who said this" is answered for a person and
  an agent by the same field, and `operated-by` says whose agent it was.
- **The thread hangs off a Record** that the conversation refines into a plan,
  and `part-of` children are the branching. The Ontology already does trees.

#### What it demands

- **Authorship on every Message, visibly** (D10's remainder). In a thread with
  two people and two agents, an unlabelled message is the failure mode, and
  this is now the common case rather than an edge one.
- **A turn boundary that is honest.** A Message should appear when the turn is
  complete, not while it streams — otherwise the Record log fills with partial
  text. The lane carries the in-progress view; the Record lands once.
- **Barging in must be steering.** A person or another agent writing into the
  thread while a turn is running is D29's steering queue: delivered after the
  current tool call, never mid-stream.
- **Everyone knowing the consequences**, as the owner put it: the thread says
  who can see it and who is in it, because a person about to type into a
  synced thread should know it is not local.

#### Streaming into the Message itself, 2026-08-30

*Owner: the non-lane traffic should be the messages, and if possible streamed —
the model's output arriving as edits to the Message Record, taken as appends.*

**Yes, and it is better than the lane split D5 proposed for message text.**
Lince already has the mechanism: record bodies are Loro documents
(`engine/src/collab.rs`, `store/src/record_docs.rs`), sands join a record's doc
and receive every change live, and the sync path already carries `crdt` and
`snapshot` blobs between Organs. An assistant message that grows by appending
is exactly what that path does.

What it buys, and it is the thing the owner's diagram needs: **a person on
another Organ watches the agent write in real time.** With the lane, streaming
was local to the session that opened it; with the doc, liveness rides the same
grant everything else does. Person 2 on the VPS sees Person 1's agent thinking,
without a second delivery mechanism existing.

**The cost is op count, and it is bounded by batching.** `COMPACT_OPS = 100` —
a stored snapshot is refreshed once a record has a hundred crdt ops past it. A
token-per-op stream would cross that every hundred tokens and churn snapshots
all day. So **commit on a cadence, not per token**: flush the accumulated text
every few hundred milliseconds. At 250 ms a two-thousand-token message is
roughly forty appends rather than two thousand, which is under one compaction —
and the reader sees it arrive smoothly enough that nobody can tell.

**What this changes about D5.** The split stands, but the line moves: message
*text* is no longer lane traffic, it is a growing Record. The lane keeps what
is genuinely not a message — raw terminal bytes, tool progress, queue changes.
One path for messages, one for everything else, instead of two paths for
messages.

**Three things it demands:**

- **A streaming state on the Record.** "Still writing", "finished",
  "interrupted" — a reader who cannot tell a live message from a stalled one
  has no way to know whether to wait.
- **An assistant message is single-writer.** We are using a CRDT for liveness,
  not for merging: only the model appends. If a person edits an assistant
  message *while it streams*, Loro will merge that edit somewhere nobody chose.
  Make a streaming message read-only, and editable once finished.
- **An interrupted turn leaves a partial body**, so the abort path must set the
  state, not just stop the writes.
### D32. The message queue. One decision settles most of it.

*Owner, 2026-08-30: for every message send, in a thread too — Tab queues for
after the prompt ends, Enter sends after the current tool call, queued messages
can be clicked to send next and dragged to reorder, and the model does not know
about the queue unless it goes looking, because they are not in the thread yet.*

**Most of this already exists in the harness.** Pi's `steer` lands after the
current tool calls finish; `follow_up` lands after the turn; both have an
all-versus-one-at-a-time mode (`set_steering_mode`, `set_follow_up_mode`);
`clear_queue` returns the pending text rather than dropping it; `queue_update`
events report changes. So Enter and Tab map onto shipped primitives and the new
work is almost entirely Lince-side.

#### The decision: Lince owns the queue, the harness gets one message at a time

Everything else falls out of it. Reorder, promote, edit and delete become local
and atomic, because the harness never held the other messages. And "the model
does not know about the queue" becomes true **by construction** rather than by
the harness agreeing to keep a secret.

The alternative — pushing every queued message into Pi's own queue — means
reordering is `clear_queue` plus a re-push, two queues to reconcile, and a
window where a message is in flight and no longer editable.

#### Where the queue lives, which decides who can see it

Three options and only one survives:

- **Browser state.** Lost on reload, which contradicts Phase 2's own
  reload-survival condition, and useless across devices — queue on the phone
  while the turn runs on the laptop and it is simply gone.
- **A Record in the thread, marked pending.** Then everyone in a shared thread
  watches your half-formed drafts arrive and get reordered.
- **A Record that is not in the thread's message list, visible only to its
  author.** Survives a reload, works across devices, invisible to the room —
  and it makes the owner's "unless it checks for all messages" a real,
  addressable thing: an explicit tool the model may call to read what is
  pending. Take this one.

#### The details worth arguing about

- **"Immediately" is a promise that cannot be kept.** While a tool is running,
  the honest maximum is *when this tool returns*, and if that tool is a
  ten-minute build then "now" is ten minutes away. The control says **next**,
  not *now*. Abort-and-send exists as a separate, clearly destructive action —
  it throws away work in progress and must look like it.
- **Enter means three different things** depending on what is running: send,
  steer after the current tool, or queue behind what is already queued. That is
  the right behaviour and it must be *shown* on the send control, not learned
  by surprise.
- **Tab must not trap the keyboard.** Overriding Tab in a text field breaks
  keyboard-only navigation, so Shift+Tab still moves focus out.
- **Two queues, one list.** Steering and follow-up are different delivery
  points but a person thinks of one ordered list. Show one list, mark each
  entry with where it will land, and make promoting an entry to *next* convert
  it from follow-up to steer.
- **What happens when a turn ends with three things queued** is a default to
  choose, not a thing to invent: all merged into one message (cheap, but the
  model may conflate them) or one at a time (three turns, three times the
  cost). One at a time is the safer default and the more expensive one; the
  owner should pick.
- **A queued message that fires while nobody is watching** is autonomous
  action from a stale intent. At minimum show its age; consider whether one
  queued an hour ago should ask before it goes.
- **Editing a queued message** is free once Lince owns the queue, and is the
  next thing anyone will ask for.

#### Answers, 2026-08-30

**Where they live: Records only the author can see.** No new machinery —
`visible_targets` is default-hidden and returns a subject's grants, public
Records, and what that subject created. A draft I made and granted to nobody is
already visible to me and to nobody else.

**Stopping is not the same as steering, and Pi has both.** `abort` stops the
current operation immediately and `abort_bash` kills a running command; that is
a hard cancel. `steer` is not a weaker cancel — it is a *message* that arrives
at the next safe point. Two different things that sound like one. So the
simplest surface is exactly what the owner asked for: Ctrl-C in the terminal
view of a session maps to `abort_bash`, and a stop control maps to `abort`.
One consequence that must not be skipped: an aborted turn has to be **marked
interrupted in the context**, or the model reads its half-finished work as
finished.

**Queued and pinned are the same object with one boolean difference.** The
owner's instinct is right and it is not a shortcut:

- a **queued** draft is *consumed* on send — it becomes the Message and is gone
  from the list;
- a **pinned preset** is *copied* on send — the original survives for next
  time.

Same Record, same list, same UI, one flag. And the delivery aspects are
ordinary assertions, because predicates are Concepts already (D28): `#steer`,
`#next`, `#now`, `#pinned` are tags on the draft, not new columns on Message.
A person can have three prepared answers pinned and two things queued, in one
list, and the tags say what each will do.

**Not bloating ordinary conversations: make the controls conditional on state,
never on a mode.** Steer and next only *mean* anything while a turn is in
flight; a conversation between people has no current tool call, so both
collapse to "send". So:

- one send button by default;
- the extra controls appear when a turn is actually running;
- when they are inert they are still reachable and say so honestly — "no turn
  running: this sends now" — rather than being hidden and rediscovered.

Presets stay unconditional, because a canned reply is useful in a human thread
too. The data model never forks: it is one draft Record with tags in both
cases.

#### Why there are exactly two seams (the mechanics)

A turn works like this. Your message joins an ordered list. The model reads the
whole list and emits text, or tool calls, or both. The tools run, their results
are appended to the list, and the model reads the list again and continues. It
finishes when it emits a turn with no tool calls in it. The entire list is
re-sent every round — that is what a context window holds.

You cannot inject text into the middle of the model's response, because it is
producing one continuous stream. **The only safe insertion points are between
messages**, and there are exactly two of them:

- **Steer** inserts at the boundary between a tool result and the model's next
  thought. The model reads it *before* deciding its next step, so it can change
  course in the middle of a job. What it costs: your text may contradict what
  the model just did, and tool calls already in flight still complete and still
  land in the list.
- **Next** appends after the turn is completely finished. The model reads it as
  a fresh instruction with the completed work behind it. Cleaner, but the thing
  you wanted stopped has already run.

**Abort** is the third option and differs in kind rather than in timing: it
cancels rather than delivering anything.

#### The terminal and the thread are two views of one session

*Owner: I want Lince's thread UI and the terminal for that session.*

They are not two systems. A session produces both, and the terminal view is the
**uncollapsed rendering of what the thread view summarises** — the raw bytes of
commands the agent ran, which the Message view shows as a folded tool result.
Thread content is durable and syncable; terminal bytes are lane traffic and
ephemeral (D5, D31).

The discriminating question is whether the terminal is **interactive**:

- **Read-only** — a window onto what the agent is doing. Simple, honest, and
  enough most of the time.
- **Interactive** — you can type into the shell the agent is using. Then the
  crucial property: **what you type is not a message, and the model does not
  know you did it** unless the resulting output is fed back into its context.
  Taking the wheel silently is a trap, so this must look visibly different from
  steering — a different pane state, a different colour, a line in the thread
  saying a person typed directly.

**Effect on the queue: none, and that is the point.** The queue is a *message*
queue. Terminal input is not a message, so the terminal has no queue and must
not grow one. This also settles the Tab worry: the terminal owns its keys
entirely — the Ghostty sand already has its own `keymap.js` — so Tab-to-queue
applies only in the message composer, and no keyboard trap appears in either.

#### The full terminal experience, and what it costs, 2026-08-30

*Owner: I want the full normal terminal experience — every agent and subagent
interactable and inspectable the way they would be in Pi's own UI.*

There is one exclusivity to name up front, because it decides the shape:
**a harness cannot serve its own TUI and a structured protocol at the same
time.** Pi is either in `interactive` mode drawing to a PTY, or in `rpc` mode
emitting JSON. If Lince attaches a terminal to Pi's own interface, Lince cannot
build the thread from it — there are no events, only rendered ANSI (D1).

That is not a loss, because **everything Pi's TUI shows arrives over RPC**:
tool calls and their arguments, results, diffs, token and context counts,
queue state, compaction. Pi's interface is a *rendering* of that stream, and
Lince renders the same stream. So "the full Pi experience" is work on our
surface, not a capability we lack. Where Lince's rendering is thinner than
Pi's, that is a gap to close, and the gaps should be listed rather than
excused.

So a session offers **three** views, not two:

1. **The thread** — messages, durable, synced, the thing other people see.
2. **What the agent ran** — the commands and their output, rendered as a
   terminal because that is what they are, reconstructed from the RPC stream.
   Read-only by nature: it is a record of what happened.
3. **A real terminal for the person** — their own shell, in the session's
   working directory, fully interactive. This is the "normal terminal
   experience": it is *yours*, not the agent's, so there is no question of
   silently taking the wheel and no need to feed anything back into a model's
   context. The Ghostty sand already does exactly this.

The one thing to be careful about is view 2 pretending to be view 3. If a
person can type into the agent's command output, that input bypasses the model
and the model will not know it happened. Either keep view 2 read-only, or make
typing into it visibly exceptional and write a line into the thread saying a
person intervened.
### D33. Tool calls are not persisted. The simplest thing that is honest.

*Owner, 2026-08-30: pick the simplest option; I no longer mind either way.*

**Nothing is persisted.** A tool call and its output live on the lane while the
session runs, are visible in the timeline as they happen, and are gone
afterwards. No schema, no envelope, no setting — a configurable choice is more
machinery than either of the choices it selects between, and none of it has to
exist yet.

The reason this is safe rather than merely cheap: **the Ledger already records
everything the agent changed in Lince.** Agents write through Actions like
anything else, so Facts, ops and attribution exist regardless. What is not kept
is what the agent did *outside* Lince — files read, commands run — and that is
the part git and the filesystem already hold in their own way.

What is given up, said plainly so nobody is surprised later: someone reading
the thread afterwards sees the agent's account of what it did and not a
transcript of it, and the model itself keeps tool results in its own context,
so it can reason from something the thread no longer shows. If that becomes
painful, the fix is small and known — persist a summarised *envelope* per call
(name, what it acted on, ok or error, duration, size) and leave payloads
ephemeral, promotable into the thread when one matters. Written down here so it
does not have to be rediscovered; not built.

**The timeline must still say which it is.** "Output not kept" and "no output"
are different, and a pane that shows nothing has to name which one it means.

### D34. Privacy of what an agent touched. Last, deliberately.

*Owner, 2026-08-30: keep the privacy point, implement it at the very end of the
Fiote work, on visibility.*

Whatever a session eventually shares about its tool use — today nothing, later
perhaps envelopes — the arguments carry file paths, command lines and the
shape of a person's machine. In a thread with several people that is a
disclosure nobody asked for.

So when anything about tool use becomes persistent, it arrives with:

- **A per-folder and per-Fiote setting**, not one global switch. A working
  directory is the natural privacy boundary, because it is what the person
  already thinks of as "this project".
- **Summarised, never verbatim.** What it touched, not every flag.
- **Default closed.** The first version that persists anything shares it with
  nobody until someone says otherwise.

This belongs at the end because it is a *visibility* decision, and visibility
questions get answered badly when the thing being made visible does not exist
yet.

### D35. Loose end closed: a local conversation needs no contact.

Flagged twice as unverified; resolved 2026-08-30 by reading it.

`start_conversation` does four things: create a Conversation Record,
`replica::make_own_root`, `replica::offer(contact_organ)`, and `open_thread`.
**The offer is a separate call.** So a conversation with a local Fiote is the
same function minus one line — create, make its own root, open a thread — and
needs no contact Organ, no empty-string convention and no new machinery. The
grant is what makes a conversation shared, and simply not granting it is what
makes one local.

Nothing about D31 is blocked here.
## Criticism

These are the agent's objections to D18, kept out of it deliberately.

**1. Free collaboration between *cubs* has no accountable writer.** Fiote-to-
Fiote is fine: each is an Actor with `operated-by` reaching a Person, so a
conversation between two Fiotes is two identities talking and every write is
attributable to one of them. Cub-to-cub is different. A cub is a session, not
an Actor; its writes land as `Cause { kind: Fiote, uid: session_uid }` under
some agent's identity. If a cub of Fiote 1 negotiates directly with a cub of
Fiote 2 and they agree on something, the Ledger records a decision that no
lasting identity ever made, and the two Fiotes that are answerable for them were
not party to it. That is not a theoretical purity concern — it is the specific
question "who decided this" returning nobody.

The narrow fix keeps the freedom: **cubs may talk freely, and may not decide.**
A cross-Fiote exchange between cubs is lane traffic; anything that becomes a
Record is written by a Fiote, which is exactly the D5 split applied one level
up. Free collaboration, accountable conclusions.

**2. A Receptionist is a single point of failure and a token tax.** Every
notification paying for a model call to decide who should handle it is
expensive, slow, and wrong in a specific way: the routing decision is usually
mechanical (this tag, this assignee, this Fiote) and a model asked to do
mechanical routing will occasionally do something creative instead. Make the
Receptionist the *fallback*, not the front door: a notification whose target is
determined by the rule goes straight there, and only an ambiguous one wakes a
model to sort it. Same design, one condition added, most of the cost gone.

**3. "Behaviour is the Record's description" is right and needs a version.**
A prompt that lives in an editable Record is a prompt that changes under a
running agent. Two cubs spawned five minutes apart would then be running
different instructions with no way to tell afterwards which. The Record stays
the source; a session must record *which revision* of it was used. Records
already have Facts and a Ledger, so this is stamping, not new machinery.

**4. The shared `Agent` Record is a single blast radius.** Every Fiote taking
its system prompt from one Record means one bad edit changes every agent at
once, including the ones mid-task. Wanted — that is the point of a shared
prompt — but it needs the same treatment as any other broad change: a visible
diff, and agents that pick it up at session start rather than mid-turn.

**5. `not assignee` is doing something subtle.** Matching "Records with no
assignee" and then "notifying those assigned to Record X" mixes two different
assignment questions in one rule. It reads fine and will confuse whoever writes
the second rule. Worth naming the two separately in the surface: what the
condition *selects*, and who the notification *reaches* — they are almost never
the same set.

**6. Nothing here fires yet.** D14's wall is still the wall: Karma intents are
inert, so every notification design in D18 lands on `effect_queue` first. That
is fine, but the doc must not read as if writing the Condition makes something
happen.

**7. The zone rules in D20 are unenforceable against a cub with `bash`.** An
allowlist that the agent can circumvent by running a shell command is
documentation, not a boundary. Real enforcement is OS-level — a working
directory it cannot leave, and no credential in its environment for anything
outside it. Until that exists, say plainly that the zones are a convention the
cub is asked to respect, not a wall, because believing otherwise is worse than
knowing it is a convention.

## Criticism, resolved 2026-08-30

The owner answered every objection above. Where they overruled, that is the
decision; where they refined, the refinement is better than the objection.

- **1 (cub collaboration) — refined, see D24.** The objection was narrower than
  it read. Reading was never the issue (grants already govern it) and writing
  is fine because it is attributed to a Fiote. What survives: a *conclusion*
  belongs to an Actor, never to a session alone. Kinship is not the line; the
  grant on the task root is.
- **2 (Receptionist) — the owner is right, see D25.** Receptionist-ness is an
  agent's configuration, not a wired-in role: an agent prompted to route will
  route, one prompted to build will build. The cheap default goes underneath it
  — the queue is drained by whoever is free, so no model pays to decline.
- **3 (prompt revisions) — accepted with the owner's amendment.** They want to
  edit a prompt mid-flight and pay the cost, including for cubs already
  running. Do that; still stamp which revision a session ran, so "why did these
  two behave differently" has an answer.
- **4 (shared `Agent` Record blast radius) — same answer.** Editing it changes
  every agent, deliberately. Show the diff; do not silently swap a prompt
  mid-turn.
- **5 (`not assignee`) — clarified by the owner.** It is a filter for Records
  with no Actor assigned to handle them; the recipients come from who is
  assigned to Record X. Two different questions, which is exactly why D26 keeps
  selection and recipients separate in the surface.
- **6 (nothing fires yet) — stands.** Karma dispatch is still inert; the
  mechanism is `effect_queue` until it is not.
- **7 (zone rules) — overruled by the owner, correctly.** Allow it. A person
  who wants a real boundary will impose one at the OS level. The note that
  remains is one line, not an argument: an allowlist a shell command can walk
  around is a convention, and the surface should say "convention" rather than
  implying a wall.
## Carry into the Interface refactor

*Fiote is parked after this planning pass and resumes once the new Interface
lands (owner, 2026-08-30). In the current waterfall that means the C5
pre-Box foundation gate. The concrete list is **Phase 0 and Phase 1** of the
build order below; this is the principle behind it.*

**The two must be designed against each other, and one rule keeps both
honest:** an item belongs in the Interface refactor only if a conversation
between two people would want it. If it is only useful because an agent exists,
it is Fiote's work.

That rule cuts both ways deliberately. It stops the refactor being bent around
one unbuilt feature — nothing in Phase 1 mentions an agent in its
justification, and each item improves an ordinary conversation. And it stops
the Fiote surface being bolted on later, because almost everything Fiote needs
from a thread UI is something a thread UI should have had anyway: knowing who
wrote a message, whether it is finished, watching it arrive, queueing a reply
for when the other side is free.

Applying it moved exactly one item out: a permission for session Sands is
agent-specific, so it sits in Phase 2 rather than Phase 1. The rest survived,
which is the evidence that the two bodies of work are genuinely aligned rather
than one being made to serve the other.

**The consequence worth naming:** if Phase 1 lands, Fiote's prototype and the
thread model stop being two milestones. A session is a thread from its first
version, and there is no retrofit.

## Build order

Reorganised 2026-08-30 into phases, because the old list had accreted out of
sequence and because Fiote and the Interface refactor have to be built with
each other in mind. Every step must be usable by a person when it lands
(`AGENTS.md`), so the surface travels with the mechanism.

**The test that keeps the two honest.** An item belongs in the Interface
refactor only if **a conversation between two people would want it**. If it is
only useful because an agent exists, it is Fiote's work, not the interface's.
That is what stops the refactor being skewed toward one feature, and it is also
what stops the Fiote surface being bolted on afterwards: the things Fiote needs
from a thread UI are, almost all of them, things any thread UI should have.

Applying that test moved one item out of the carry list: a **permission for
session Sands** is Fiote-specific, so it lands in Phase 2 rather than Phase 1.
The other five survive it.

---

### Phase 0 — beside C4-C5 and before Fiote resumes.

- [ ] **Delete `OfficialWidgetBuilder::feature_flag()`** and the `#[allow(dead_code)]`
  around it. It looks like a gate, gates nothing, and the refactor decides what
  a real runtime switch is (owner, 2026-08-30: remove now, do it properly
  there).
- [ ] **Delete the misleading `subject_kind` source comment** at
  `0001_init.sql:225`. It documents `organ | actor | role | public | fiote`;
  `visible_targets` honours only `subject_uid` or `public`. Source comments are
  forbidden in this repository anyway; the real behavior belongs in these
  notes until the feature exists.
- [ ] **Make `cargo test -p lince-fiote` opt-in** rather than failing on a
  fresh checkout with no Pi. It must still refuse to pass silently when the
  harness is absent — a test that goes green without the thing it tests is the
  failure this guards against — but a red workspace nobody caused teaches
  people to ignore red.

### Phase 1 — the Interface refactor, with Fiote's needs designed in.

Each of these is a general improvement that Fiote happens to need first. None
of them mentions an agent in its justification.

- [ ] **Authorship on every message.** Who wrote it — and when a Record's author
  is an Agent, whose agent (`operated-by`, D17). Any thread with more than two
  participants wants this.
- [ ] **Message state: being written, finished, interrupted.** A reader who
  cannot tell a live message from a stalled one does not know whether to wait.
  A person typing is the same state as a model writing.
- [ ] **A message body that grows while it is watched** (D31). Record bodies
  are already Loro documents and sync already carries `crdt` and `snapshot`
  blobs; what is new is a thread that renders a body while it changes, and
  holds it read-only until it settles. Correctness requires ordered state,
  interruption and restart behavior. Coalescing deltas into commits is a
  separate measured performance policy; do not make a guessed cadence part of
  the Message contract.
- [ ] **A composer that says what sending will do**, with a drafts list beside
  it holding presets and queued messages (D32). Presets are canned replies,
  useful between people; queueing is what happens when the other side is busy.
  A queued draft is consumed on send, a pinned preset is copied.
**Landed Interface prerequisite:** C3 supplies renderer-neutral domain launch
recipes that instantiate an exact compound definition, typed Record inputs
and stable placements idempotently. Reusable layout lives in the definition;
workspace placement and user overrides live in Box host state; the domain
Record does not absorb UI layout. Fiote's later use of that mechanism remains
Phase 2 work, not an unfinished Interface mechanism.

Interface work is written in `anicca/interface/plans/sands.md`; the Fiote
reasoning behind each is in the D-sections here.

### Phase 2 — Fiote's prototype. The first thing a person can use.

Rungs 1 and 2 of the ladder, plus the surface. Because Phase 1 landed the
thread properly, the session **is** a thread from the first version rather than
a chat window whose history goes nowhere — the two milestones that used to be
M1 and M11 collapse into this one.

- [ ] A session's system prompt is rendered by Lince from Records: a shared
  `Agent` Record plus each Fiote's own body (D18, D17). Pi never holds the
  source, and the session records which revision it ran.
- [ ] Lince's Actions as an **MCP server**, with at least one Action reachable
  from a cub (D23). This is capability leaving Pi's hands and the first piece
  of it that stops mattering.
- [ ] One Fiote managing several cubs, concurrent, each with its own context
  and token count.
- [ ] The session Sand group: `conversation` reused for the thread, `record`
  for the task, `terminal` for a real shell, plus the two new Sands — the tool
  timeline and the session control, first cut only (roster, state, tokens and
  context, stop).
- [ ] A **permission for session Sands**, declared the way `terminal_session`
  is. Permissions are genuinely enforced, so this is a gate rather than a
  label.
- [ ] Provider credentials in a `0600` file with the panel that sets one and
  says whether it works (D15). Without it the surface cannot reach a model.
- [ ] Reload survival: close the board, reopen it, sessions still running with
  their backlog replayed (D2).

### Phase 3 — accountability. What an agent did, and under whose name.

- [ ] `Cause::fiote(session_uid)` and the write path that stamps it, so the
  Ledger says which session moved what.
- [ ] The grants each Fiote Actor holds, including the broad read D24 assumes —
  stated in the surface and revocable, never silently wide.
- [ ] A published key for an agent uid, so an agent's words can be verified by
  another Organ (D10's remainder).
- [ ] A panel listing what each Fiote has done.

### Phase 4 — tasks, and Fiote as an orchestrator.

- [ ] Task Records, decomposition by `part-of`, assignment to a Fiote or a cub,
  results written back to the task.
- [ ] The traversal policy as data (D28, D29): Concept uid, direction, depth,
  and glance / summarise / read-in-full, on the Agent Record's `lince.fiote`
  extension. The prompt renders a sentence from it and is never its source.
- [ ] The context walk itself: visited set, depth and breadth guards, a token
  budget, closest-first ordering, and it says what it cut.
- [ ] Many Fiotes, each a `CreateAgent` Record with `operated_by`, behaviour in
  its body — receptionist, builder, architect are prompts, not code (D18).
- [ ] Check first whether the agent work board is `kanban` or `relations` with
  a filter rather than a new Sand.

### Phase 5 — initiative, and the bounds on it.

- [ ] Permission policy per tool call: allow / ask / deny, with a callback to
  the surface (D13).
- [ ] Budgets that stop rather than warn, per session and per Fiote.
- [ ] The wake (D25, D26): a saved Protein view as the filter, a tag write as
  what makes it look, a recipient Record whose assignees hear about it, one
  `effect_queue` row that every recipient sees and the first free one claims
  with the rest standing down for free. Dedupe, never fire on a contact's
  assertion or on our own agents' writes, a ceiling per rule per window, and a
  visible queue.
- [ ] Only after all of the above: anything autonomous.

### Phase 6 — outward. Other people, and their agents.

- [ ] The room (D16, D31): one root granted to several contacts, a stated
  membership Record, honest words about what revoking does not undo, agents
  posting under their `operated-by` delegation. The owner's diagram — two
  people and two agents around one thread on a VPS — does not work without it.
- [ ] Agent authorship verified across Organs, not merely displayed.

### Phase 7 — code, and the world outside Lince.

- [ ] `gix` reads branch and sha; a task Record holds `remote + branch + sha`
  (D9).
- [ ] `forgejo-api` opens the pull request on accept; the Record holds its url;
  GitHub stays the main remote (D12).

### Later, and deliberately unscheduled

- ACP as a second backend, which is both bring-your-own and the only route to a
  subscription (D22, D29).
- Our own turn loop on `genai`, replacing Pi's — rung 6 (D29).
- Files in messages over `iroh-blobs` (D30).
- General Files sync with LSP-derived code links (Ideas).
- **Privacy of what an agent touched (D34), last of all**, because it is a
  visibility decision and those are answered badly before the thing being made
  visible exists.

## Open, for the owner

1. *Closed by D22.* Node is no longer a dependency of the default path — goose
   is a Rust binary. Node returns only for a person who chooses Pi through the
   ACP door, which is their choice to install, not ours to bundle. What remains
   is the smaller version of the same question: how the **goose binary** is
   obtained — flake input for development, bundled with the desktop build for
   users, or required on PATH.

2. D9's level: reference, patch, or bundle. The recommendation is reference now
   and patch at review time, because both ride the existing op path and neither
   needs the blob sync Lince does not have. Bundle would make Lince a real git
   mirror and costs a content-addressed blob store first.

Deliberately not asked yet: D5's lane/Record split. Nothing in M1's surface
depends on it, and it will be decided better against measured event volume from
a live cub than against a guess.

## Ideas, recorded and deliberately not implemented

### General Files sync, and code as links

*Owner, 2026-08-30, explicitly as an idea and not as work.*

A File Sync mode that takes **every** file in a directory, respects
`.gitignore`, and makes each one a Record — optionally with an assertion
marking it. Then a visual text editor over those Records. Then a "developer"
flag on that folder which turns the code's own structure into links: in Rust,
`mod myfile` becomes an import link, and a call to a function or a use of a
struct or trait becomes a link between the Records. Links a person adds by hand
stay hand-made and never rewrite the file.

**Why it belongs in this document rather than a File Sync one.** If code files
are Records with import and call links, then D29's traversal policy walks
*code* — the same primitive, doing double duty. "Read this function's Record,
glance at everything it calls, summarise everything that calls it" is one
policy over a graph rather than a bespoke code-intelligence feature. That is a
large part of what makes an agent good at a codebase, and it would arrive as
configuration.

**What it would cost, from what is already known:**

- `FileFormat` is `Markdown | Lingua` and a folder is flat — one `head.md` per
  Record. Nested paths, arbitrary extensions and a file's own name as identity
  are all new.
- Bodies are text, so binary files in such a folder land on D30's blob store.
- **Derived links must be distinguishable from hand-made ones.** A refresh
  must not wipe a person's manual link, and must not add a second copy of one
  it made before. `asserted_by` plus a provenance marker is the shape of the
  answer; getting it wrong is worse than not having the feature, because it
  silently eats work. **This gets more load-bearing under LSP, not less** —
  derived links then refresh on every edit rather than once.

**The links come from an LSP, not from a model and not from our own parsers.**
*Owner, 2026-08-30, and it is the right call.* A language server already
answers exactly these questions — `textDocument/definition` for where a symbol
is defined, `textDocument/references` for who uses it, `documentSymbol` for
what a file contains, and call hierarchy for who calls whom. One integration
buys every language that has a server, with no per-language parser to write and
no tokens spent. It is also more correct than parsing: a language server
resolves through imports, generics and re-exports, which a regex or a
tree-sitter query does not.

What that costs instead: a language server has to be installed and running per
language, it wants a real project (a `Cargo.toml`, a `tsconfig.json`) rather
than a loose folder, and it is slow to start on a large repository — so the
refresh is a background job with a visible state, not something that happens
while a person waits.

Not now.
