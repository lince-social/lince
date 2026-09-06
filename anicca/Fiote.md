# Fiote
Fiote is Lince's agent. A person talks to it, it understands what they want, and it does the work or hands the work to smaller agents called cubs. It is not a chat window bolted to the side of Lince: it reads Records, writes Records, and everything it does lands in the Ledger under a name somebody is answerable for.

A Fiote is not one thing, it is a kind of thing. Each Fiote is an Agent Record, and its behaviour is its own description — the text you write in that Record is the prompt it runs on. So a Fiote that receives and routes work, a Fiote that builds, and a Fiote that designs are three Records with three descriptions, not three pieces of code. Put one router in front of three builders and you have a routing tier; point work straight at the builders and you have none. The mechanism does not know the difference, and that is the point.

A cub is different. A cub is one session of work on one task, spawned with a narrow brief, and it is a real coding agent: it reads, writes, edits files and runs commands in a working directory. It is not an Actor and it has no identity of its own — its writes belong to the Fiote that spawned it, stamped with the session they came from. Fiote orchestrates and does not edit code; cubs edit code and do not decide.

Fiote itself has no `edit` or `write` tool. Its tools are Lince's: create and link Records, read the graph, assert and retract, propose field candidates, and spawn, steer or stop a cub. It is long-lived and it is one lasting Actor. A cub gets the full set — read, write, edit, run commands, search — plus those same Lince tools, offered as ordinary tools rather than through a privileged path, so it can attach its result to the task it was given the same way anybody else would.

A cub with a shell can do anything the person can. That is the point of giving it one, and it is also why permission work is not decoration.

Fiote is meant to be off by default, and today it is off by accident. Neither switch exists. There is no `fiote` cargo feature anywhere in the workspace, and nothing depends on `crates/fiote` — it is a workspace member with its own binary, wired into no other crate, so there is no crate dependency, no transport message and no sand registration for a feature to gate. The runtime switch that would let a build which *has* Fiote still be run without it does not exist either. Both are cleanups below, and until they land nothing may be described as opt-in.

## The session is a thread

This is the centre of the whole design. What you say to your agent is a Message. What it says back is a Message. The session is a Thread, hanging off a Record, and that Record is the plan the conversation refines, branching into child tasks. Other people can read it, and other people's agents can talk in it, because a thread shared with several contacts is something Lince's grants already do.

A conversation with an agent therefore needs no new machinery. `start_conversation` does four things: create a Conversation Record, make it its own replica root, offer it to a contact Organ, and open a thread. The offer is a separate call. So a conversation with a local Fiote is the same function minus one line, and needs no contact Organ, no empty-string convention and nothing new. A grant is what makes a conversation shared; simply not granting it is what makes one local. This was flagged twice as unverified and then resolved by reading it.

Not everything a session produces is a message. A finished turn is a Message; a token fragment arriving mid-sentence is not — it is the sound of a message being written, and nobody needs it replicated to every contact. The original worry was flooding: a cub emitting a few hundred turns an hour through the thread path would hit the Ledger, the op log, the outbox and every contact's feed with text nobody re-reads. That worry was about the deltas, not the turns. A conversation's worth of turns is the same order of magnitude as a conversation between people, which the Thread model was built for.

So the line falls here:

- A person's prompt is a Message Record.
- A completed assistant turn is a Message Record.
- Streaming deltas are lane traffic, on the ephemeral sand-to-sand path that never touches the Ledger.
- Raw terminal bytes, tool progress and queue changes stay on the lane too.

The lane is a built mechanism. `crates/transport/src/lane.rs` holds a `LaneHub` of broadcast rooms with `join`, `send` and `prune`, carrying `LaneEvent { room, from, payload, from_subject, organ }`; the wire messages are `LaneJoin`, `LaneLeave`, `LaneSend` and `LaneEvent`; a sand reaches it as `host.joinRoom` and `host.onLane`, one room per ABI topic named `abi:<topic>`. Two of its properties decide what may ride it. A lane never leaves our own Cell — the board bridge routes lane rooms as board-scoped traffic, always to our own Cell — which is the mechanical reason message text had to become a growing Record instead, because the whole point was a person on another Organ watching the agent write. And a room's channel holds 256 events before a slow reader starts losing them silently, which is fine for presence and worth measuring for a cub emitting a fragment per token.

Message text streams into the Record itself rather than onto a lane. Record bodies are already Loro documents — `engine/src/collab.rs` and `store/src/record_docs.rs` — sands join a record's doc and receive every change live, and the sync path already carries `crdt` and `snapshot` blobs between Organs. An assistant message that grows by appending is exactly what that path does. What it buys is the thing the whole design needs: a person on another Organ watches the agent write in real time, over the grant they already had, with no second delivery mechanism. With a lane, streaming would have been local to the session that opened it.

The cost is op count, and it is bounded by batching. A stored snapshot is refreshed once a record has a hundred crdt ops past it (`COMPACT_OPS = 100`), so a commit per token would cross that every hundred tokens and churn snapshots all day. Commit on a cadence instead — flushing accumulated text every few hundred milliseconds turns a two-thousand-token message into roughly forty appends rather than two thousand, under one compaction, and smooth enough that no reader can tell.

Tool calls are not kept at all. What was called and what came back is visible while it happens and gone afterwards. No schema, no envelope, no setting — a configurable choice would be more machinery than either option it chooses between. This is safe rather than merely cheap, because everything the agent changed *in Lince* is in the Ledger regardless: agents write through Actions like everything else, so Facts, ops and attribution exist either way. What is lost is what it did outside Lince — files read, commands run — which git and the filesystem hold in their own way. Said plainly so nobody is surprised: someone reading the thread later sees the agent's account of what it did and not a transcript of it, and the model keeps tool results in its own context, so it can reason from something the thread no longer shows. If that turns out to hurt, the fix is known and small — keep a summarised envelope per call and leave the payload ephemeral.

## Identity, and who is answerable

An Agent Actor already exists in the code. `Action::CreateAgent { head, operated_by }` ships, declared at `engine/src/actions.rs:448` and handled at `actions.rs:4022`. It ensures the Concept `actor` with `person` and `agent` beneath it, so "is this an Actor" is answered by the Concept DAG for both kinds; creates a `RecordKind::Person` Record, deliberately, because standing, the four login doors and dormant absorption are all written about people and widening the word would blur them; asserts `agent` on it and sets `agent` as its identity predicate; and asserts `operated-by` to the Person answerable for it, resolving that Person *before* creating anything, because an Agent nobody is answerable for is the thing that field exists to prevent. It needs nothing for the Organ half, since every Record already carries the Organ it originated at.

So "this Agent belongs to that Person" is not work to do; it is a call that ships, and the predicate is `operated-by`. `assigned-to` exists too, and the Record sand gives assignees their own section rather than counting them as links.

The rule that follows: an agent whose words cross to another Organ has its own keypair and its own Actor Record, plus a delegation — the `operated-by` assertion, and a signed, narrowly scoped authorization from that Person saying what it may do and until when. Revoke the agent, keep yourself. A local-only cub has neither: it is a Session Record under its Fiote, attributed by cause uid.

Giving each cub its own Actor would mean an `app_user` row, a grant set and a revocation story per ephemeral process, and would still not answer "what has Fiote done" without a union over dead identities. One accountable identity, many attributable sessions.

Cubs may talk freely and may not decide. A conclusion belongs to an Actor, never to a session alone. Fiote-to-Fiote conversation is two identities talking and every write is attributable to one of them; two cubs agreeing on something would put a decision in the Ledger that no lasting identity ever made, so when two cubs work something out, a Fiote is the one who writes it down. Free collaboration, accountable conclusions.

Reading is governed by grants, and there is a choice to make out loud rather than by accident: `visible_targets` gates every read by grant plus whatever the Actor itself created, so either a Fiote Actor is granted broadly — which is what "read all of Lince" plainly means and is a reasonable default on a personal Cell — or "all of Lince" quietly degrades into "all the little this agent happened to be granted", which reads as the agent being broken. Take the first, say it in the Fiote sand, and keep the grant visible and revocable like any other.

Writing is allowed anywhere. A message is a Record and agents write Records; the only thing that matters is that the write is attributed, which it already is. Not prying is a habit that lives in the prompt, not a wall in the code — an agent that wanders is wasteful, not dangerous, and the cost shows up in the token readout of the person who owns it, which is the right kind of pressure. Agents converse in one Record per subject rather than one per pair: a task's thread for work on that task, and a standing thread for the agents themselves. Anyone who can see it can read it and join, and nobody has to be told who their siblings are.

Who spawned whom says nothing about who should be involved. There is no kinship model here, and there was never a problem for one to solve.

## The harness is ours

We build our own loop. The provider layer is taken rather than written; everything above it is ours, because above it is where Lince's model — Records, assertions, grants, the Ledger — has to live.

The reasoning: the boring bottom of every harness is provider abstraction and the tool-call round-trip, and there is no upside to owning either. The parts above that are the ones that differ. Two existing harnesses were examined closely and both are built for a person at a terminal editing a git checkout; Fiote is built for a person inside Lince editing a graph. Every place those differ, an adapter would have been a translation layer that never stops costing. Where the cheap option and the best long-term architecture differ, and there is no reason to pick cheap, pick the architecture.

Steal shapes, do not re-derive them. Pi's RPC command vocabulary is a good specification of what a session must expose. Goose's per-session extension sets are the right granularity for capability. ACP's permission-request shape is a solved UI contract. Read all three, copy the shapes, write the code.

One capability our own harness cannot have: a Claude or ChatGPT subscription is redeemable only through that vendor's own client, which owns the OAuth, and nobody can hand that to us. So an ACP client stays as an optional backend behind the same seam — that is bring-your-own-agent and subscription support through one door. Keys we hold ourselves; subscriptions we borrow. This is a constraint, not a preference.

### What we read, and what it taught us

**Pi** — `@earendil-works/pi-coding-agent`, repo `earendil-works/pi`, formerly `badlogic/pi-mono`, v0.84.4, TypeScript, run by Node. It is a harness and not a model: `pi-ai` is a provider-unifying LLM client covering Anthropic, OpenAI, Google and others; `pi-agent-core` is the agent loop with tool calling and state; `pi-coding-agent` is the CLI with read, bash, edit, write, grep, find and ls tools, plus sessions, skills and extensions.

It has four modes and only one mattered: `interactive` draws a TUI to a PTY, `--print` runs one shot and exits, `--mode rpc` speaks JSONL over stdin and stdout with commands in and events out, and an in-process TypeScript SDK. Its extensions are `.ts` files loaded through `jiti` with no build step, from `~/.pi/agent/extensions/*.ts`, `.pi/extensions/*.ts` or `-e path`; an extension registers tools, slash commands, keybindings and lifecycle hooks (`session_start`, `before_agent_start`, `tool_call`, `tool_result`, `context`, `agent_end`), can append persistent entries, and can spawn child agents. Sessions are files on disk under `--session-dir`, addressable by id, supporting `fork` from an earlier user message and `clone`.

The RPC surface was probed locally, with no API key needed to start:

    printf '{"type":"get_state","id":"1"}\n' | \
      vendor/pi/node_modules/.bin/pi --mode rpc --no-session --offline

It returns model, thinking level, streaming flag, session id and message counts. The full command list — worth keeping as the specification of what a session must expose — is `prompt`, `steer` (guidance delivered after the current tool calls finish), `follow_up` (queued for after the turn), `abort`, `clear_queue`, `bash`, `abort_bash`, `get_state`, `get_messages`, `new_session`, `switch_session`, `fork`, `clone`, `set_session_name`, `export_html`, `set_model`, `cycle_model`, `get_available_models`, `set_thinking_level`, `compact`, `set_auto_compaction`, `get_session_stats`, `set_auto_retry`, `set_steering_mode` and `set_follow_up_mode`.

The events are `agent_start`, `agent_end`, `agent_settled`, `turn_start`, `turn_end`, `message_start`, `message_update` and `message_end` carrying text, thinking and tool-call deltas, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`, `bash_execution_update`, `queue_update`, `compaction_start` and `compaction_end`, `auto_retry_start` and `auto_retry_end`, and an extension-UI sub-protocol carrying select, confirm, input and editor requests — which is exactly a tool-approval prompt when an extension asks for one.

Everything asked for is in that list. "Show each cub's context and what it is comprised of" is session stats plus messages. "Pet it and make it sleep until compacted" is compaction plus its events. "Add my own message mid-task" is steering. "Which cub said what" is the session id on the event. None of it is recoverable from a stream of terminal bytes.

Pi's cost is Node. A shipped desktop build would carry a Node runtime, roughly fifty megabytes before the npm tree, plus Pi's whole dependency graph, per platform, patched forever. Three ways to have Node — on PATH, which works today and fails on a machine without it with an error a normal person cannot act on; vendored, which is reproducible for a developer and invisible to git because `node_modules/` is ignored, and does not solve Node itself; or a flake input, which is right for this repository's own development and does nothing for a packaged build.

Two Pi details are worth keeping past the survey because our own loop has to decide the same things. `steer` and `follow_up` each carry an all-versus-one-at-a-time mode, set by `set_steering_mode` and `set_follow_up_mode`, which is the harness-side form of the queued-message question below. And `clear_queue` returns the pending text rather than dropping it, which is the right behaviour: clearing a queue must not silently eat what a person typed.

**Goose** — `block/goose`, Rust, Apache-2.0, donated to the Agentic AI Foundation, around 29k stars. Its subagents run in isolated sessions with their own context windows, extension sets and turn limits; the parent delegates and receives structured JSON summaries back, and a failed subagent returns a failed task result rather than crashing the parent session. That failure isolation is real and is the shape our own cub model takes. Its extension mechanism *is* MCP, so a new MCP server becomes available with no change to the harness. Its daemon `goosed` is axum-based, exposes REST plus SSE with a WebSocket interface, is the same backend its desktop app uses, runs many sessions concurrently with an agent per session and extension sets isolated between them, and supports auth by secret key. Its `goose-acp` crate exposes session management, streaming, tool execution with permission flows and session resumption over a single `POST /acp` endpoint.

Two traps worth writing down: the crates.io name `goose` is an unrelated load-testing framework, and the pieces that would matter — `goose-server`, `goose-acp`, `goose-mcp` — are not on crates.io at all, only `goose-providers` at `0.1.0-alpha.7` with `goose-provider-types`. So it could never have been `cargo add`; it would have been a spawned binary spoken to over ACP, or a fork. Its subscription support is the ACP path, not something it owns.

**Buzz** — `block/buzz`, Apache-2.0, released 2026-07-21, and Rust: `buzz-core`, `buzz-relay` on axum with Postgres, Redis and S3, plus `buzz-cli`, `buzz-acp`, `buzz-agent`, `buzz-workflow` and a Tauri and React desktop client. It is the closest existing thing to what Fiote reaches for. Worth reading, not worth adopting.

Three of its ideas transfer. **Human-agent parity**: an agent is not a bot with a webhook, it joins the same rooms, holds its own membership, and its actions land in the same log as a person's — which costs Lince an identity decision rather than a subsystem, because an agent that can be granted things is just an Actor. **The agent holds its own key and the person signs a narrow authorization**: Buzz gives every participant a keypair and has the human sign a scoped delegation, and the property that matters is revocation — compromise the agent and you revoke the agent, not yourself. **Git as signed events rather than hosting**: repository announcements, patches, issues and review approvals as signed events, feature branches surfacing as channels, CI posting results there, an agent doing a first-pass review, and the merge decision landing in the same room as its evidence. Buzz does not host repositories. It is a place where the conversation about a patch is signed and auditable, and that distinction is the whole design rather than a caveat.

What does not transfer is Nostr. Adopting it would give Lince a second identity system beside the one it has — ed25519 keys, signed action intents in `0018_signed_action_intent.sql`, grants, contacts, HLC-ordered ops. Two key hierarchies is how a project ends up unable to say who authored anything. Take the delegation pattern and implement it on the signed-intent machinery already here.

### The crates

`genai` at `0.7.0-beta.19` is the provider layer: one API over twenty-six or more providers on their *native* protocols, with tool choice, streaming and reasoning content, and explicitly no agent loop — which is the reason to pick it. It is pre-1.0, so expect churn. `rig-core` at `0.42.0` was the alternative, provider-neutral with the loop in a separate crate so it can be used loop-free, but it brings memory and vector-store contracts we do not need. `rmcp` at `3.1.4` is MCP. `gix` at `0.87.1` is git. `forgejo-api` at `0.11.1` is pull requests. `mistralrs` and `ollama-rs` are for local models someday. `agent-client-protocol` at `2.0.0` is the optional ACP backend, and would be the only new crate that path needs.

### The process shape

Sessions are in-process. `crates/fiote`'s supervisor stops supervising child processes and starts owning session tasks. The attach, detach and replayable-backlog design survives unchanged, because that was the part worth building; the locate-and-spawn path becomes dead code for the default backend and lives on only for the optional ACP backend that talks to somebody else's binary.

The supervisor being host-owned is not a detail. `crates/transport/src/terminal.rs:17` holds `TerminalHost { sessions }` on the websocket connection, so a PTY is owned by that connection and reloading the board kills it. That is correct for a terminal and fatal for an agent. A cub that runs twenty minutes must survive a browser reload, a closed laptop lid and a second board opened on the phone. Detaching is not killing. This is the one thing that had to be decided before any code, and it is why `crates/fiote` is its own crate rather than more of `transport`. A cub that runs a command forks children of its own that the supervisor cannot see and cannot account for, which is worth knowing before trusting any button that says it kills a cub.

Nothing a person would call "editing Fiote" needs a compiler. Prompts, recipes, tool allowlists, model, thinking level, working directory and which MCP servers an agent may use are all data on the Agent Record, taking effect at the next session. New capabilities are separate MCP processes in whatever language somebody likes, and Lince's own Actions become one of these — which is how Fiote gets Lince powers and how a person adds their own on the same footing. Only the turn loop — turn handling, compaction, retry, streaming — needs a rebuild, and a turn loop is the right thing to require a build for: nobody should edit one from a chat box while it is running.

## What enters the context is a query, not a request

Most of Fiote's value is ergonomics around what the model gets to see. Three parts of that must never be instructions: retrieval is a deterministic walk of the assertion graph, decided before the model sees anything; budget is arithmetic, because a ceiling a model can talk itself past is not a ceiling; and permission is a gate, because advisory permissions are documentation. Instructions only shape what the model does with what already arrived.

The walk is exactly what it sounds like — start at a Record, follow the links the policy names, put what you find in the context. Nothing subtler is going on. The design work is all in what happens when the result is too big. The policy is four fields per rule: which link Concept, which direction (subject to object or object to subject, native because an assertion carries both columns), how deep, and whether to glance at it (head and state only), hand the subtree to a cub for a summary, or read it in full. "Children of this task, one level, in full; everything referencing it, any depth, summarised" is four fields, not a paragraph of English.

Everything about the walk is ordinary code over `record_assertion`. No model is involved and nothing is embedded. Summarising is the only rung that spends tokens, deliberately, and only where the policy asked for it.

Never compare a predicate string. `record_assertion.predicate_uid` references `concept(uid)`, so a predicate is a Concept and `part-of` is a label on a uid rather than the identity. `concept_name(concept_uid, lang, name)` holds names per language, so one Concept can be `part-of` in English and something else in Portuguese. `concepts::resolve` accepts a uid, the canonical name, or any localized name. `concept_parent` is a DAG and `descendants_including` already powers matching `@apple` under `concept_in @food` through apple, fruit, food; `concept_equivalence` exists for saying two Concepts mean the same. So resolve the containment Concept once and match anything beneath or equivalent to it, and a person who renames it, translates it or introduces a narrower notion keeps working.

This also disposes of spending a cub to read links and guess which are child tasks: the graph states it. Use a model only where the graph genuinely does not.

Which Concepts an agent follows is configuration, and configuration is data — neither code, which would mean recompiling to change which links an agent cares about, nor prompt text, which would put a uid in a string a model can mangle when the match must be exact and free. It is a `record_extension(record_uid, namespace, version, fds)` row with `UNIQUE(record_uid, namespace)`, already in the initial migration, under a `lince.fiote` namespace, holding resolved Concept uids: which link Concepts this agent follows, which it ignores, which tags wake it. The matcher reads uids and never sees a word. The prompt then carries a rendered sentence derived from that data — "you follow links that mean containment (`part-of`, `parte-de`)" — in the reader's own language, for explanation only. The data is authoritative and the prompt is a view of it, never the reverse.

## Where things may be edited

The zones fail differently, so they get different answers. The source checkout is permissive: a cub gets a working directory and edits inside it, mistakes there are recoverable, and that is what git is for. The Lince database is never written directly by any cub — every change goes through Actions, which is already the rule, so this needs enforcement rather than a new policy. The reason is not tidiness: a direct write skips the Ledger, the op log and the outbox, so it is invisible to attribution and never reaches a contact, and there is no backup story, so a bad direct write is unrecoverable in a way a bad commit is not. The harness itself is the one zone with no undo *and* no boundary, because a broken harness cannot be relied on to report that it is broken; changing it is an ordinary source-checkout task in a separate checkout, never against the running build, and never by a cub spawned by the Fiote whose own code is being changed.

Said plainly, because believing otherwise is worse than knowing it: an allowlist that a shell command can walk around is a **convention**, not a wall. Real enforcement is OS-level — a working directory it cannot leave, and no credential in its environment for anything outside it. Somebody who wants a real boundary imposes one. The surface must use the word convention rather than implying a wall.

## Being woken

The rule layer itself, its wall and its bounds are in `anicca/Karma.md`.

Four independent questions, and keeping them apart is the design.

**Which Records am I watching?** A saved filter. "Every Record tagged `#ai`, also tagged `#wip`, with nobody assigned to handle it." You build it once, you can look at it, and it lists Records the way any other view does.

**What makes it look?** Somebody adds or removes a tag. The filter is re-checked at that moment, not on a timer, because a timer re-scans a growing pile forever and gets expensive without anyone noticing.

**Who hears about it?** The Actors assigned to a Record you name. That is usually not one of the Records the filter returned — you point at a Record that holds your agents, and its assignees are the recipients. These are two different questions and the surface must show them as two boxes: what did we find, and who should hear about it. People and Agents are both Actors, so one rule covers both and only delivery differs.

**What actually happens?** A row lands in a work queue, one per finding. Every recipient sees it. The first one free claims it and the others stand down. Nothing is decided by anyone on anyone else's behalf.

Worked through: you tag a Record `#wip` and `#ai` and assign nobody. The tag write wakes the check. The filter matches. The rule's recipient Record is *My Agents*, which has three Fiotes assigned. One queue row appears, visible to all three. A router claims it, reads the task, and hands it to a builder by assigning the Record to it — which is itself a tag write, which wakes the check again, which now matches nothing because the Record has an assignee. The loop closes on its own.

Nobody negotiates. A claim is a write everybody can see — an assertion on the task naming the claiming Actor — first write wins, and the store settles ties because two writes cannot both be first. A real negotiation costs one model call per agent per notification to produce an answer nobody reads again. Capacity is a number, not a judgement: at its limit an agent does not claim, under it it does, and no model is ever asked whether it feels busy, because a model asked that will sometimes decide to be helpful. Standing down is free — no message, no summary, no note — or "everyone is notified" becomes everyone writing a Record saying they passed. If a decision ever genuinely needs discussion, the agents can hold it in their common place, but that should be a choice rather than the default cost of every notification.

The names for this, since it comes up: the queue form is *competing consumers*, and the negotiate-and-award form is the **Contract Net Protocol** (Smith, 1980). What is written above is competing consumers, and it upgrades to Contract Net without redesign if a bid ever needs to express capability — bids computed from queue depth and capability tags first, prompted only if that fails.

Most of the pieces exist. The filter is a saved Protein view, and `InputSource::SavedProtein { view }` at `nucleus/src/karma/ast.rs:178` already takes one as a rule input — it stays a Protein view rather than becoming a second query language inside Karma, whose `Condition` is numeric on purpose, gating and carrying on exact decimals, and would be wrecked by forcing set-matching into it. The trigger is `TriggerSource::Fact { record, concept }` at `nucleus/src/karma/ast.rs:152`, which fires on a Fact about a Record or a Concept — precisely "an assertion I care about was made". The route is `CandidateRoute::Ask`, one of `Observe`, `Recommend`, `Draft`, `Ask` and `Act` at `nucleus/src/karma/value.rs:15`, already frozen in the revision hash. The queue is `effect_queue` at `0001_init.sql:170`, durable, with `kind` of command or notify, `status` of queued, running, done or failed, `attempts` and `origin_uid`, and claim and finish helpers in `store/src/misc.rs` — it survives a restart. `record_assertion` rows carry `asserted_by`, so "who tagged this" is already answerable, which matters because a tag written by a cub must not wake another cub unless the owner said so.

**Karma cannot dispatch anything.** Its intents are inert — nothing dispatches an accepted candidate into an op. So the notification is designed against Karma's vocabulary and implemented on `effect_queue` until dispatch lands, at which point Karma becomes the configurable front over the same queue and none of the design changes. Nothing here should read as though writing a Condition makes something happen. One related gap in passing: `InputSource::SecretMetadata { secret }` exists in the AST with no secret store anywhere behind it.

## Code, and the world outside

How code travels is `anicca/Code.md`; how bytes travel is `anicca/Files.md`. What is below is only what Fiote adds to them.

Lince holds the reasoning, git holds the code, Forgejo holds the merge. Each is good at one of those and bad at the other two. Putting all three in one relay is a fine choice for a company that wants one server and the wrong one for a person who already has git working.

There is no blob sync on the op path: media is host-local, served from `/host/media` by `presentation/http/media_assets.rs`, and the sync path carries ops and Loro snapshots and nothing else. So "Lince as a git remote" is not a feature, it is a prerequisite subsystem, and a bundle of a live repository is tens to hundreds of megabytes, which neither the op log nor the thirty-day sealed mailbox is built to carry. Three levels exist and the cheapest is enough: a **reference** is a remote, a branch and a sha, a tiny string expressible as ops today with zero new machinery; a **patch** is text, so it can be a Record body, which is what makes a review conversation self-contained because the thing being discussed travels with the discussion; a **bundle** is one file that would make Lince a real mirror and needs blob sync. Reference now, patch at review time, bundle never by default.

The workflow, end to end, and what the owner does today does not change — code on the laptop, commit locally, push to GitHub:

1. A task becomes a Record, either because the person said it or because Fiote proposed it and they accepted. Decomposition is `part-of` children, exactly as the Ontology already does trees.
2. A cub takes one task with a narrow brief and the skills it needs, working in a checkout on this machine. Its chatter is lane traffic; its result goes on the task Record.
3. The result is a branch, and the Record holds the reference — remote, branch, sha, read with `gix`. Under review, the patch text goes on the Record too.
4. The person reviews in Lince, where the conversation, the task tree and the patch are one thing, and can steer the cub mid-turn.
5. On accept, a bridge opens the pull request with `forgejo-api` against the LAN or VPS Forgejo, or the same step can target GitHub. The PR body links back to the Record and the Record holds the PR url. Nothing about the repository moves through Lince.
6. Forgejo mirrors GitHub using its own mirror feature — check its docs for pull versus push before committing to a direction. GitHub stays the main remote; Forgejo is the backup and the place PRs from local agents land without a third party.
7. Other people see what was granted. A contact holding the task Conversation sees the decisions, the patch and who authored them, verified by the delegation chain. Their own agents read those Records like any other. Cloning the code is `git clone`, because the code was never in Lince.

Files are a different question and are no longer blocked, though not scheduled. `iroh` at `=1.0.3` is already a workspace dependency and `iroh-blobs` at `0.103.0` — content-addressed blobs, BLAKE3, with a filesystem store behind its `fs-store` feature — was verified rather than assumed to resolve alongside it into one lockfile with a single `iroh 1.0.3`. So an attachment is a tiny op on a Message: hash, filename, size, mime type, leaving the op log, the Ledger and every contact's feed as small as they are today. The bytes move out of band over the iroh connection the two Organs already have, fetched when someone clicks, content-addressed so a file received twice is stored once and a corrupted transfer is detectable rather than merely suspicious. The user experience is the one asked for: choose an Organ, attach, send, and on the other side a Message with a file on it and a button.

That retires "Lince cannot carry bytes" — a bundle, an image, a PDF or a design file in a conversation becomes ordinary rather than blocked — while reference-and-patch remains right for *code*, because a repository is not an attachment and git is better at moving it. Rejected outright: base64 in a Record body or an extension. It works today with no new dependency and it puts a megabyte of payload into an op replicated to every grantee and kept forever.

The honest failure case must be visible: fetch-on-demand means a file whose sender is offline cannot be retrieved right now. That is the same property the sealed mailbox has and needs the same treatment — "not fetched yet, the sender is not reachable" said out loud, never a spinner that looks like corruption or an error that looks like the file is gone.

## What already works

`crates/fiote` has the supervisor, the child process, line framing, the replayable backlog, attach and detach, send and kill, and one passing integration test driving a real session.

`Action::CreateAgent { head, operated_by }` ships, and so does `assigned-to`. Predicates are Concepts with per-language names and a DAG behind them. Saved Protein views work as Karma rule inputs, the Fact trigger fires on a Record or a Concept, and the `Ask` route is frozen in the vocabulary. `effect_queue` is durable with its claim and finish helpers written. `replica_grant` is keyed `PRIMARY KEY (root_record, contact_organ)` at `0042_individual_replica.sql:45`, so one root holds a grant row per contact and fans out to all of them, each with its own offered and accepted state, and revoking one deletes one row; containment is settled separately because every Record carries `record.replica_root` pointing at its root, resolved once at creation, so a grant covers the whole tree without walking assertions. Record bodies are Loro documents that sync live. `start_conversation` needs no contact for a local conversation.

The Interface refactor has landed its native runtime, the Sand ABI and recursive composition host, plus renderer-neutral domain launch recipes that instantiate an exact compound definition with typed Record inputs and stable placements, idempotently — reusable layout in the definition, workspace placement and user overrides in Box host state, and the domain Record not absorbing UI layout.

Fiote is parked until the new Interface lands, and resumes at the C5 pre-Box foundation gate. The cleanups below run beside C4 and C5; the thread work is part of the refactor itself. Fiote's own prototype starts after C5 without becoming a prerequisite for Box.

## Known walls

Where a secret can live at all is `anicca/Secrets.md`.

Karma cannot dispatch anything, so every notification lands on `effect_queue` until that changes.

Sand feature flags are labels rather than gates. `sand::FEATURE_FLAG` strings are declared per sand, `OfficialWidgetBuilder::feature_flag()` carries an allow-dead-code and is read nowhere, and every registered sand is built and offered. So Fiote's runtime switch would be the first real use of that mechanism, or the cargo feature is the only opt-in and nothing should imply otherwise. Do not write `sand.fiote` into the registry and call it opt-in.

There is no secret-at-rest story. `store/src/logins.rs` is not credentials — a `Login` is `organ_uid`, `person_uid` and `created_at`, so it binds a contact Organ to a Person and holds no secret. `identity_key` stores public keys only, keyed `(actor_uid, key_id)`, populated when a client publishes a key for a Person through the action-intent session and by transfer delivery. `configuration` is a typed singleton of UI and policy settings, unencrypted, the wrong shape and the wrong safety class for an API key. The one real precedent is `engine::trust::load_or_create_secret`: thirty-two raw bytes in a file created `0o600`, deliberately not in the database, with the node key kept distinct from the identity key. Provider keys follow that precedent.

`nucleus::CauseKind::Fiote` exists with `Cause { kind, uid: Option<String> }` — an attribution slot never constructed anywhere today.

`store::visibility::grant` documents `subject_kind` as `organ | actor | public | fiote` and the initial migration adds `role`, but `visible_targets` honours only `subject_uid = ?` or `subject_kind = 'public'`. A `fiote` class grant is a placeholder that grants nothing; anything that needs to read must have a subject uid.

There is no `Agent` in `RecordKind`. An actor uid in practice is a `Person` Record bound to an `app_user` through `0011_app_user_person.sql`.

## Objections raised, and how they were settled

**Free collaboration between cubs has no accountable writer.** Refined rather than accepted: reading was never the issue because grants govern it, and writing is fine because it is attributed to a Fiote. What survives is that a conclusion belongs to an Actor and never to a session alone. Kinship is not the line; the grant on the task root is.

**A router is a single point of failure and a token tax.** The owner is right that router-ness is configuration and not a wired-in role. What survives is the cheap default underneath: the queue is drained by whoever is free, so no model pays to decline, and routing that is mechanical stays mechanical — a model asked to do mechanical routing will occasionally do something creative instead. Wake a model only for the ambiguous case.

**"Behaviour is the Record's description" needs a version.** Accepted with the owner's amendment: they want to edit a prompt mid-flight and pay the cost, including for cubs already running. Do that, and still stamp which revision a session ran on, so "why did these two behave differently" has an answer. Records already have Facts and a Ledger, so this is stamping and not new machinery.

**One shared prompt Record is one blast radius.** Same answer: editing it changes every agent, deliberately, because that is the point of a shared prompt. It gets the treatment any broad change gets — a visible diff, and agents picking it up at session start rather than mid-turn.

**"No assignee" is doing something subtle.** Clarified: it is a filter for Records with no Actor assigned to handle them, while the recipients come from who is assigned to a different Record. Two different assignment questions in one rule reads fine and will confuse whoever writes the second rule, which is exactly why selection and recipients stay separate boxes in the surface.

**Nothing here fires yet.** Stands. Karma dispatch is inert and `effect_queue` is the mechanism until it is not.

**The zone rules are unenforceable against a cub with a shell.** Overruled, correctly. Allow it. What remains is one line rather than an argument: an allowlist a shell command can walk around is a convention, and the surface should say convention.

## How this fits the Interface refactor

The two are designed against each other, and one rule keeps both honest: an item belongs in the Interface refactor only if a conversation between two people would want it. If it is only useful because an agent exists, it is Fiote's work.

That cuts both ways deliberately. It stops the refactor being bent around one unbuilt feature — nothing in the thread work below mentions an agent in its justification, and each item improves an ordinary conversation. And it stops the Fiote surface being bolted on later, because almost everything Fiote needs from a thread UI is something a thread UI should have had anyway: knowing who wrote a message, whether it is finished, watching it arrive, queueing a reply for when the other side is free.

Applying the rule moved exactly one item across: a permission for session Sands is agent-specific, so it sits with the prototype rather than with the refactor. Everything else survived, which is the evidence that the two bodies of work are genuinely aligned rather than one being made to serve the other.

The consequence worth naming: if the thread work lands, Fiote's prototype and the thread model stop being two milestones. A session is a thread from its first version, and there is no retrofit.

Every step must be usable by a person when it lands, so the surface travels with the mechanism. The interface side of this work is written in `anicca/interface/plans/sands.md`.

## Cleanups to do first

- [ ] Delete `OfficialWidgetBuilder::feature_flag()` and the dead-code allowance around it. It looks like a gate and gates nothing; the refactor decides what a real runtime switch is.
- [ ] Delete the misleading `subject_kind` comment at `0001_init.sql:225`. It promises `organ | actor | role | public | fiote`, and only a subject uid or `public` is honoured. Source comments are forbidden in this repository anyway, and the real behaviour belongs in these notes until the feature exists.
- [ ] Make `cargo test -p lince-fiote` opt-in instead of failing on a fresh checkout with no harness present. It must still refuse to pass silently when the harness is absent — a test that goes green without the thing it tests is the failure being guarded against — but a red workspace nobody caused teaches people to ignore red.
- [ ] Add the `fiote` cargo feature and gate the crate dependency, the transport messages and the sand registration behind it. No such feature exists today and nothing depends on `crates/fiote`, so the crate is excluded from every build by being unwired rather than by being optional — which looks the same until the first thing imports it, and then Fiote is on for everyone.
- [ ] Build the runtime switch that lets a build which *has* Fiote still be run without it, or stop calling the sand opt-in.

## The thread, which any conversation wants

Each of these is a general improvement to conversations between people, that Fiote happens to need first.

- [ ] Show who wrote every message, and when the author is an Agent, whose agent it is. Any thread with more than two participants wants this, and an unlabelled agent message in a human conversation is the failure mode.
- [ ] Give a message a state: being written, finished, interrupted. A reader who cannot tell a live message from a stalled one has no way to know whether to wait. A person typing is the same state as a model writing.
- [ ] Render a message body while it grows, and hold it read-only until it settles. Only the model appends to an assistant message — we are using a CRDT for liveness, not for merging, and a person editing mid-stream would have their edit merged somewhere nobody chose. Editable once finished.
- [ ] Make a message land when the turn is complete rather than while it streams, so the Record log does not fill with partial text. The live view is the growing body; the turn boundary must be honest.
- [ ] Make an interrupted turn set the message state, not merely stop the writes, so a partial body is never mistaken for a finished one.
- [ ] Decide how often streamed text is committed by measuring rather than guessing, and keep that cadence out of the Message contract. The correctness half — ordered state, interruption, restart — is what belongs in the contract.
- [ ] Build a composer that says what sending will do, with a drafts list beside it holding presets and queued messages. Presets are canned replies, useful between people; queueing is what happens when the other side is busy.

## The message queue

- [ ] Keep the queue in Lince and hand the harness one message at a time. Reorder, promote, edit and delete are then local and atomic, because the harness never held the other messages, and "the model does not know about the queue" is true by construction rather than by the harness agreeing to keep a secret. Pushing every queued message into the harness's own queue would mean clearing and re-pushing to reorder, two queues to reconcile, and a window where a message is in flight and no longer editable.
- [ ] Store a queued message as a Record its author alone can see. Grants are default-hidden and a person already sees what they created, so this needs no new machinery. It survives a reload and works across devices, which browser state does not — queue on the phone while the turn runs on the laptop and browser state is simply gone — and it stays out of the thread's message list, so nobody in a shared thread watches half-formed drafts arrive and get reordered.
- [ ] Give the model an explicit tool to read what is pending, so "unless it goes looking" is a real, addressable thing rather than a hope.
- [ ] Make a queued draft and a pinned preset the same object with one flag: a queued draft is consumed on send and is gone from the list, a pinned preset is copied on send and survives for next time. Same Record, same list, same UI. A person can have three prepared answers pinned and two things queued in one list.
- [ ] Express delivery as tags on the draft rather than new columns on Message — steer, next, now, pinned — because predicates are already Concepts.
- [ ] Offer two seams and one cancel, and name them honestly. A turn works by your message joining an ordered list, the model reading the whole list and emitting text or tool calls or both, the tools running, their results appending to the list, and the model reading again; it finishes when it emits a turn with no tool calls. The entire list is re-sent every round, which is what a context window holds. You cannot inject text into the middle of a response, so the only safe insertion points are between messages, and there are exactly two. **Steer** inserts between a tool result and the model's next thought, so it can change course mid-job, at the cost that your text may contradict what it just did and calls already in flight still complete and still land. **Next** appends after the turn is completely finished, cleaner, but the thing you wanted stopped has already run. **Abort** differs in kind rather than timing: it cancels instead of delivering anything.
- [ ] Map a stop control to abort and Ctrl-C in a terminal view to killing the running command. Stopping is not a weaker kind of steering; they are two different things that sound like one.
- [ ] Never promise "immediately". While a ten-minute build is running, the honest maximum is when this tool returns, so the control says next.
- [ ] Mark an aborted turn as interrupted in the model's context, or it reads its own half-finished work as finished.
- [ ] Show steering and follow-up as one ordered list, marking each entry with where it will land, and make promoting an entry to *next* convert it from a follow-up into a steer. A person thinks of one list; the tags already say which is which, so promotion is a retag rather than a move between two lists.
- [ ] Make abort-and-send look destructive if it exists at all. Wanting both at once — throw away what is running and send this instead — is a third gesture, and it must not sit on the send control looking like a third way to send.
- [ ] Keep Shift+Tab moving focus out of the composer once Tab queues, because overriding Tab inside a text field is how keyboard-only navigation breaks.
- [ ] Show the extra controls only while a turn is actually running, conditional on state and never on a mode — a conversation between people has no current tool call, so both collapse to "send". When they are inert keep them reachable and honest, "no turn running: this sends now", instead of hidden and rediscovered. One send button by default. Presets stay unconditional. The data model never forks: it is one draft Record with tags in both cases.

## The first Fiote a person can use

Because the thread landed properly, a session is a thread from its very first version rather than a chat window whose history goes nowhere, and there is no retrofit.

- [ ] Render a session's system prompt from Records: one shared `Agent` Record that every Fiote is assigned to, plus that Fiote's own body. Reading that shared Record and following its links is also how a notification finds who to notify, human or agent, with the context they need. The harness never holds the source.
- [ ] Stamp which revision of those Records a session ran on, so "why did these two behave differently" has an answer.
- [ ] Show a diff when the shared `Agent` Record changes, and let agents pick a new prompt up at session start rather than mid-turn.
- [ ] Expose Lince's Actions as an MCP server, with at least one Action reachable from a cub. This is how a cub gets Lince powers on the same footing as anyone else's tools, with no privileged path, and it is also what lets a repository agent drive a running Lince.
- [ ] Run one Fiote managing several cubs at once, each with its own context and token count.
- [ ] Build the session Sand group: the conversation Sand for the thread, the record Sand for the task, a terminal for a real shell, plus two new Sands — the tool timeline, and a session control with the roster, state, tokens, context and a stop, first cut only.
- [ ] Offer three views of a session and keep them distinct. **The thread** is durable, synced, and what other people see. **What the agent ran** is the commands and their output, rendered as a terminal because that is what they are, reconstructed from the event stream, and read-only by nature because it is a record of what happened. **A real terminal for the person** is their own shell in the session's working directory, fully interactive, theirs and not the agent's, so there is no question of silently taking the wheel. A harness cannot serve its own TUI and a structured protocol at the same time, but everything a harness TUI shows arrives over the protocol anyway — tool calls and arguments, results, diffs, token and context counts, queue state, compaction — so a rich session view is work on our surface rather than a capability we lack. Where our rendering is thinner, list the gap rather than excusing it.
- [ ] Keep the agent's output pane from pretending to be the person's shell. If a person can type into it, that input bypasses the model and the model will not know it happened — so either keep it read-only, or make typing into it visibly exceptional and write a line into the thread saying a person intervened.
- [ ] Keep the terminal out of the queue entirely. Terminal input is not a message, so the terminal has no queue and must not grow one — which also settles Tab: the terminal owns its keys, the Ghostty sand already has its own keymap, so Tab-to-queue lives only in the composer and no keyboard trap appears in either.
- [ ] Say which it is when a tool pane is empty: "output not kept" and "no output" are different things.
- [ ] Declare a real permission for session Sands, the way the terminal session one is declared. Permissions are genuinely enforced, so this is a gate rather than a label.
- [ ] Store provider credentials in a `0600` file under the Cell's config directory, one per provider, following the node-secret precedent — never in a Record, never in the op log, never in `configuration`, and therefore never synced to another Organ or carried in a database backup. Build the panel that sets one through a narrow host call and reports only whether a key is present and whether it works; the sand never reads a key back. Keep environment variables supported for the developer path, and say which source a provider is using, or "it works on my machine and not in the app" is unanswerable. Without this the surface cannot reach a model at all, so it is a dependency of this milestone and not a late concern.
- [ ] Survive a reload: close the board, reopen it, sessions still running and their backlog replayed.
- [ ] Measure the backlog before fixing its size. Attaching replays a two-thousand-*line* ring buffer, and that replay is the whole of reload survival — without it, reattaching shows a blank pane on a cub that has been working for ten minutes. Two thousand is a guess, and it is a line cap rather than a byte cap, so a session emitting a line per token fragment may cover seconds instead of minutes. Measure against a real run, then likely choose a byte budget and retain assembled messages rather than deltas.
- [ ] Make the system prompt and the tool set settable per session at creation rather than only for the whole process, because an Agent Record configuring a Fiote is worth nothing if every session shares one configuration. This began as a question to ask somebody else's daemon; with our own loop it is a thing to build.

## Our own turn loop

The parts, in the order they are needed.

- [ ] Take the provider layer rather than writing it: `genai`, one API over many providers on their native protocols, with streaming, tool calls and reasoning content, and no agent loop of its own. It is the one part with no upside to owning.
- [ ] Write our own message and content model — turns, and content parts for text, thinking, tool call, tool result, image. Everything keys off this, and provider types leak into everything if they are allowed to.
- [ ] Build a tool registry and dispatch: a trait carrying a name, a JSON schema and a run function, with `rmcp` so an external MCP server registers as an ordinary tool. Lince's Actions are tools here on the same footing as anyone else's.
- [ ] Write the turn loop: send, stream deltas, collect tool calls, execute them, append results, repeat until no calls remain or the budget stops it. Cancellation and mid-turn abort belong in the first version — retrofitting them into a loop that assumes it runs to completion is painful.
- [ ] Make the session an append-only event log, which is Lince's idiom anyway, so replay, fork and resume fall out of the design instead of being features built on top of it.
- [ ] Assemble the context: the shared prompt, the Fiote's own body, the task brief, the traversal results, and the tool schemas. This is the Lince-specific part and the actual reason to build our own.
- [ ] Account for tokens and cost per session and per Fiote, with ceilings that stop rather than warn.
- [ ] Apply a permission policy per tool call — allow, ask or deny — with a callback to the surface. This is where "a cub with a shell can do anything you can" gets its answer.
- [ ] Add a steering queue that delivers after the current tool call rather than into the middle of a stream.
- [ ] Add compaction that summarises and replaces behind a pinned prefix, recording what was dropped. Never silently.
- [ ] Emit one typed event stream that the Sand, the lane and any Ledger writer all subscribe to. Headless is then simply nobody subscribing, which is what the new interface needs.
- [ ] Write a scripted provider for tests: deterministic, no network, no key. This is what makes the crate's tests mean anything, and it is the same shape the Ontology's deterministic simulation work wants.
- [ ] Never route one cub's streaming deltas into another cub's context. A cub reports its result; the deltas exist for the human's screen only.
- [ ] Keep the levers that actually reduce spend in reach, because what costs money is what enters a context window and how a process is hosted costs nothing either way. Compaction and auto-compaction bound to a plain gesture. Thinking level per cub, off for one whose job is to run a command. Scoped skills rather than the whole of `anicca/`. A narrow tool allowlist, which is also a shorter system prompt. Show real numbers from session stats — tokens, cost, context-window percentage — instead of guessing. A shared markdown mailbox is *more* expensive than structured messages, because every agent re-reads the whole file to find its part on every poll and the file grows forever.

## Accountability

- [ ] Stamp writes with the cub they came from — `Cause::fiote(session_uid)` and the write path behind it — or the Ledger cannot say which session moved what. The attribution slot exists in the code and is never constructed anywhere today.
- [ ] State in the surface which grants each Fiote Actor holds, including the broad read, and make them revocable. Never silently wide.
- [ ] Publish a key for an agent uid, so another Organ can verify an agent's words rather than trusting a column the sender filled in. The key table is keyed by actor and key id and an Agent is a Person Record, so the path exists; nothing publishes for an agent today and no surface offers to.
- [ ] Build a panel listing what each Fiote has done.

## Tasks and orchestration

- [ ] Make tasks Records, decomposed with `part-of`, assigned to a Fiote or a cub, with results written back onto the task. The task Record is the message: Fiote reads results, not transcripts. Agents coordinate through a shared task store rather than by talking to each other, because a model reading another model's stream pays tokens to re-derive a conclusion the other one already wrote down. State is a Concept, the way every other Lince state works.
- [ ] Spawn each cub with one task uid and a narrow brief: the task, its parent's statement of done, and the skills it was given. Nothing else.
- [ ] Send questions up, never sideways. A cub that needs something asks its Fiote, which answers from what it holds or asks the person. Peer chatter is where token budgets die and where two agents talk each other into a wrong answer.
- [ ] Store the traversal policy as data on the Agent Record's `lince.fiote` extension: Concept uid, direction, depth, and glance or summarise or read in full. The prompt renders a sentence from it and is never its source.
- [ ] Build the context walk itself. Carry a visited set, because it is a graph and not a tree, the same Record is reachable by two paths, and a cycle is otherwise an infinite walk — this is the one place the naive version genuinely breaks rather than merely costing too much. Guard depth and breadth-per-node cheaply, and let a token budget be the real limit: fill until it is spent, then stop. Order breadth-first from the starting Record so what gets dropped is far material rather than the Record's own children. Whether something is glanced at or read in full is a budget decision and not a semantic one. Say what was cut — "12 more children not included" — inside the context, so the model can ask for them and a person can see why something was missed, because silent truncation is how an agent confidently answers from half the picture. Make it deterministic: same Record, same policy, same data, byte-identical context, which is what makes provider prompt caching actually hit and a session reproducible when something goes wrong.
- [ ] Write back what a model had to work out because the graph did not say it, as an assertion, so the same question is never paid for twice.
- [ ] Create many Fiotes, each an Agent Record with an operator and behaviour in its body. Receptionist, builder and architect are prompts, not code.
- [ ] Route mechanically first and wake a model only for the ambiguous cases.
- [ ] Name the task's zone on the task Record before the cub starts. The zone is a property of the task, not of the cub's good intentions.
- [ ] Check whether the agent work board is the kanban Sand, or the relations Sand with a filter, before building a new one.

## Initiative, and its bounds

- [ ] Build the wake: a saved Protein view as the filter, a tag write as what makes it look, a recipient Record whose assignees hear about it, and one `effect_queue` row every recipient sees and the first free one claims.
- [ ] Make the wake tag pair configuration rather than constants — `#fiote` and `#wip` as defaults, both editable in the Fiote sand — so that when Karma dispatch lands this becomes one rule among many without the design changing.
- [ ] Keep one live row per rule, Record and recipient. Tag and untag five times and you get one row, not five cubs.
- [ ] Never fire on an assertion that arrived from a contact's sync. A peer must not be able to spend our tokens. The assertion carries who asserted it and every Record carries the Organ it came from.
- [ ] Never fire on our own agents' writes unless a rule says so explicitly, or a cub tagging its own output wakes itself forever.
- [ ] Cap each rule per window, and when it is hit, pause and say which rule and which cycle — not silently killed, not silently infinite.
- [ ] Show the queue: what is waiting, who claimed it, what failed and why. A notification system nobody can inspect cannot be told apart from a broken one.
- [ ] Cap each agent's concurrent cubs, so capacity stays a number rather than a judgement.
- [ ] Only after all of the above: anything autonomous.

## Other people, and their agents

What a room is, and what it cannot pretend about, is `anicca/Rooms.md`.

A machine somewhere else is just another Organ. "The agent reads a thread on the VPS" means the thread is replica-granted to this Cell, the agent reads the local copy, and its writes sync back. There is no remote read and there does not need to be one.

The transport already does multi-party, because a grant row exists per contact. The *semantics* are deliberately two-party: a Conversation is built as the root shared with one contact. So a room is not a new subsystem — it is a Conversation-like root granted to several contacts — and what is genuinely missing is everything that makes a room feel like a room.

- [ ] Build the room: one root granted to several contacts, so two people and two agents can share one thread.
- [ ] State membership as a Record everyone can read and disagree with, written by the room's creator, because each grant is independent, nobody can otherwise enumerate who holds the root, and the honest answer for a peer-to-peer system is that somebody has to say it.
- [ ] Say plainly what revoking does not undo. It stops future ops; it does not recall what was already delivered, and implying a moderator power that does not exist is worse than admitting there is none.
- [ ] Say who can see a thread and who is in it, because a person about to type into a synced thread should know it is not local.
- [ ] Have agents post under their `operated-by` delegation, so every member's Lince can render "Ana's reviewer agent, acting for Ana" and verify it rather than take the sender's word. Nothing is special-cased for agents; a room of two people and three agents works the same as a room of two people, which is what human-agent parity actually means.
- [ ] Verify agent authorship across Organs, not merely display it.
- [ ] Remember fan-out is not free: every message becomes an op to every outbox, which is the second reason chatter stays off the Record path — agent noise in a five-person room would be five times the flood.

The code still does not travel through the room. What is shared is the task tree, the patch text under review, the branch reference, the PR url and the conversation — all small, all text, all already expressible as ops. Each person clones from git and pushes to git. The room is where the reasoning is shared and stays attributable.

## Git and the merge

- [ ] Read branch and sha with `gix`, and hold a remote, a branch and a sha on the task Record.
- [ ] Put the patch text on the Record at review time, so the thing being discussed travels with the discussion.
- [ ] Open the pull request with `forgejo-api` on accept, link the Record from the PR body, and hold the PR url on the Record. GitHub stays the main remote.
- [ ] Check whether Forgejo's mirror should pull or push before committing to a direction.

## Later, and deliberately unscheduled

- [ ] Add ACP as a second backend with `agent-client-protocol`, which is both bring-your-own-agent and the only route to a subscription. It is a **client** path for auth, never a surface we expose. Pi is reachable this way through its own ACP adapter, so somebody who wants Pi can have it without Node being load-bearing for anyone who does not.
- [ ] Put files in messages over `iroh-blobs`, and say out loud when a file cannot be fetched because its sender is not reachable — never a spinner that looks like corruption or an error that looks like the file is gone.
- [ ] Persist a summarised envelope per tool call — name, what it acted on, ok or error, duration, size — promotable into the thread when one matters, if losing them turns out to hurt. Payloads stay ephemeral.
- [ ] Do the privacy of what an agent touched last of all, because it is a visibility decision and those are answered badly before the thing being made visible exists. Whatever a session eventually shares about its tool use, the arguments carry file paths, command lines and the shape of a person's machine, which in a thread with several people is a disclosure nobody asked for. So it arrives with a per-folder and per-Fiote setting rather than one global switch, because a working directory is what a person already thinks of as "this project"; summarised rather than verbatim, what it touched and not every flag; and closed by default, sharing with nobody until somebody says otherwise.

## Three questions for the owner

- [ ] What happens when a turn ends with three things queued. Either they are merged into one message, which is cheap but lets the model conflate three separate asks, or they are delivered one at a time, which is three turns and three times the cost. One at a time is the safer default and the more expensive one, and this is a default to choose rather than a thing to invent.
- [ ] How the harness runtime is obtained for anyone who chooses the bring-your-own path — a flake input for development, bundled with the desktop build, or required on PATH. The default path no longer needs one, and Node is the choice of a person who picks Pi rather than something we bundle.
- [ ] Whether code sharing settles at reference, patch or bundle. Reference now and patch at review time is the recommendation, because both ride the existing op path and neither needs blob sync; a bundle would make Lince a real git mirror and needs a content-addressed blob store first.

Deliberately not asked yet: exactly where the lane and Record line falls. Nothing in the first usable version depends on it, and it will be decided better against measured event volume from a live session than against a guess.

## An idea, recorded and not scheduled

The idea in full is `anicca/FileSync.md`.

A File Sync mode that takes **every** file in a directory, respects `.gitignore` and makes each one a Record, optionally with an assertion marking it, and then a visual text editor over those Records. Then a developer flag on that folder that turns the code's own structure into links: in Rust, a module declaration becomes an import link, and a call to a function or a use of a struct or trait becomes a link between the Records. Links a person adds by hand stay hand-made and never rewrite the file.

It belongs with Fiote because the traversal policy would then walk *code* — the same primitive doing double duty. "Read this function's Record, glance at everything it calls, summarise everything that calls it" becomes one policy over a graph instead of a bespoke code-intelligence feature, and it would arrive as configuration. That is a large part of what makes an agent good at a codebase.

The links come from a language server, not from a model and not from our own parsers. A language server already answers exactly these questions — where a symbol is defined, who uses it, what a file contains, and who calls whom — so one integration buys every language that has a server, with no per-language parser and no tokens spent. It is also more correct than parsing, because it resolves through imports, generics and re-exports, which a regex or a tree-sitter query does not. The cost is that a server has to be installed and running per language, wants a real project rather than a loose folder, and is slow to start on a large repository — so refreshing is a background job with a visible state and not something that happens while a person waits.

- [ ] Support nested paths, arbitrary extensions and a file's own name as identity. A sync folder is flat today, with one `head.md` per Record and a `FileFormat` of only Markdown or Lingua.
- [ ] Send binary files in such a folder to the blob store, since Record bodies are text.
- [ ] Keep derived links distinguishable from hand-made ones. A refresh must never wipe a person's manual link and must never add a second copy of one it made before. `asserted_by` plus a provenance marker is the shape of the answer. Getting this wrong is worse than not having the feature, because it silently eats work — and it gets more load-bearing under a language server, not less, because links then refresh on every edit rather than once.

## Where the old numbered decisions went

The build notes this replaces numbered their decisions D1 to D35, and `anicca/interface/plans/sands.md` still cites D1, D13, D14 and D31 to D33. This is where each one lives now, so those citations keep resolving. Delete this block once that file names sections instead of numbers.

D1, D31, D33 and D35 are **The session is a thread**. D2, D21, D22, D27 and D29 are **The harness is ours**, with the survey in *What we read* and the parts list in **Our own turn loop**. D3 is the opt-in paragraph in **What Fiote is** and the last cleanup. D4, D10, D17 and D24 are **Identity, and who is answerable**. D5 is the lane and Record line in **The session is a thread**. D6 is the spending levers in **Our own turn loop**. D7, D13 and D18 are **What Fiote is** and **Tasks and orchestration**. D8 is Buzz in *What we read*. D9, D12 and D30 are **Code, and the world outside**. D11 and D23 are **The harness is ours**. D14, D19, D25 and D26 are **Being woken**. D15 is the credentials paragraph in **Known walls** and its checkbox in **The first Fiote a person can use**. D16 is **Other people, and their agents**. D20 is **Where things may be edited**. D28 is **What enters the context is a query, not a request**. D32 is **The message queue**. D34 is the last item under **Later, and deliberately unscheduled**. The Criticism sections are **Objections raised, and how they were settled**, and *Carry into the Interface refactor* is **How this fits the Interface refactor**.
