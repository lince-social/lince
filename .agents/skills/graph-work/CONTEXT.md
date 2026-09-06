# Small working contexts, durable handoffs

This applies to every role, including the planner. The durable graph and exact
source/evidence are the memory of the run; a conversation is its current desk.
Reuse a warm checkout without requiring a warm conversation.

## What the tools can actually do

Automatic compaction belongs to the client. A handoff summary does not itself
remove earlier messages. Neither `followup_task` nor `send_message` resets
context, and sending the text `/compact` to a worker is not a compaction API.
Only claim a forced compaction or context reset when an exposed control really
performed it. Do not guess a worker's remaining tokens when usage is hidden.

Use the client's supported compaction when available. Otherwise checkpoint
before the task becomes hard to reconstruct, let automatic compaction operate,
and use a fresh focused agent at a safe boundary when lifecycle/slot controls
allow it. Interruption is not closure and does not prove a slot or process was
released. If a fresh thread is impossible, use the checkpoint to resume the
existing one honestly; ask for a session continuation if the actual limit
prevents reliable progress. Never evade the agent cap with nested CLI runs.

## One current packet per role

At launch the planner assigns a durable run-state directory outside disposable
worker trees, build outputs and the product merge. Each role writes only its
own note there; only the planner writes the graph. Keep raw logs separately,
redacted, with revision and command identity. Do not put passwords, tokens,
private data or full command environments in notes or logs.

A current packet normally fits about one page; 500–900 words is a useful
editing target, not permission to drop a constraint. Replace superseded status
instead of appending a diary. Keep only what changes the next decision:

- Role, node/attempt, allowed files, absolute checkout, branch, base and HEAD;
  dirty files and where unfinished work is safely preserved.
- Accepted requirement and source links, non-goals, authority boundaries and
  rejected approaches with the reason each was rejected.
- Current result and unresolved questions, separating observations from
  assumptions. State the next small action, not an entire queued backlog.
- Exact checks/proofs, features, revision, result and raw evidence path;
  candidate, integrated and exclusive evidence stay distinct. Unrun means unrun.
- Owned processes, sessions and permits, or explicit none, plus pause/lease
  state. A remembered pause acknowledgement expires across unknown process state.

Builders retain their current implementation, failures worth not repeating and
handoff candidate. Integrators retain merge order, exact accepted refs, shared
contract changes and accumulated proofs due. Inspectors retain the revision
and workload under review, drift findings, measurement protocol, raw reports,
missing human checks and lease. Planners retain the latest owner decisions,
certified base, claims, queue, stale nodes, resource budgets and next frontier.
A combined verifier keeps integration and inspection state distinct in its note.

## When to checkpoint and rotate

Checkpoint after a meaningful proof or design decision, before a handoff or
subject change, and before a long investigation continues. On a long node,
checkpoint at coherent internal stages; if it cannot produce a reviewable
unit in one worker session, the planner splits it before dispatch. Unfinished
work may be preserved without certifying a partial feature or releasing deps.

Use a fresh, narrowly briefed thread for an unrelated domain or when repeated
re-reading, forgotten constraints or confused revisions show the context is
no longer helping. Reuse a thread for closely related nodes and repairs while
it remains useful. High reasoning effort does not repair a polluted context.
Do not rotate on every command or keep an unrelated conversation alive merely
because its checkout/cache is warm. Long same-subject work still checkpoints
and rotates when needed; subject similarity is not an unlimited context budget.

Give new agents `fork_turns="none"` with their role instructions, current
packet and the small set of required source references. They read applicable
skills themselves and open relevant files, not the whole corpus or transcript.
Put large test output and exploration in artifacts. Return a short result or
delta with evidence paths, not raw logs or the whole updated packet on every
message. A summary never substitutes for the inspector reading the actual diff.

## Resume from evidence, not a chain of summaries

After automatic compaction, a fresh agent or a session interruption, reload
the role packet, authoritative graph and required instructions. Read current
source at the named revision; do not repeatedly summarize an old summary and
gradually lose the original decision. Re-read the original requirement when
its meaning matters or the owner's direction changed.

Before editing or testing, verify checkout/branch/HEAD, dirty state, claim and
current certified dependencies. Reconcile running jobs and permits with the
planner. Confirm log/report paths exist and belong to the claimed content;
stale or missing evidence stays pending. Preserve unrelated edits, invalidate
affected claims when contracts changed, and report mismatches before proceeding.

The planner resumes the same way. It must not remember a candidate as certified,
give one node to two workers, silently lose a pending refusal or forget why CEF
is excluded. Restore facts from the run ledger, not from worker confidence.

## Bootstrap rehearsal

Before fan-out, exercise a handoff using only the packet, graph and named raw
artifacts, without depending on its author's conversation. Cover all role
states, including an unfinished builder, queued candidate, integration change,
pending exclusive proof and a planner pause. Use a fresh reader where the
agreed session permits one; otherwise record the limitation of a local rehearsal.

Include a changed base, missing/stale report and a process still holding a
permit. The reader must refuse invalid certification and recover the correct
next action without erasing unfinished work. Record what was actually tested.
This proves the resumability protocol, not that a model's automatic compaction
is lossless or that an unlimited autonomous session is available.
