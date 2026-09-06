# Reusable workers and a bounded review queue

The planner owns assignment and certification. The integrator owns the shared
source and merge queue. The inspector owns runtime evidence and the exclusive
machine lease. A builder's handoff is a candidate, not completion of the node.

## Three independent limits

- **Agent slots:** use the limit exposed by this session, accounting for its
  stated treatment of the parent. Reserve coordination and verification before
  filling builder slots. A configured limit for another client is not evidence
  that this session supports more agents.
- **Source checkouts:** use the fixed worktree or clone pool agreed under
  [ISOLATION.md](ISOLATION.md). One writer and at most one active build/test
  process family per checkout. More checkouts are not more permitted agents.
- **Machine permits:** admit compilation, linking and executable tests under
  [COMPILATION.md](COMPILATION.md). A worker may reason or edit while waiting
  for a compiler permit. A shared Corgi store is a cache, not a scheduler.

For a session with four total slots, use the planner, two builders and one
verification agent. The latter integrates and then inspects serially. If a
later session allows more, separate integrator and inspector first, then grow
the builder pool while review throughput and machine measurements justify it.
Do not launch another process or a nested agent tree to bypass an enforced cap.

## What stays warm

Keep the source checkouts, pinned environment, external disk-backed Corgi store
and the single Cargo verification target across nodes. A warm cache does not
depend on retaining a model conversation, running a persistent Corgi daemon
or keeping the previous task's unreviewed source on the new branch.

The planner allocates the pool and its owners once. A builder may prepare its
assigned checkout if explicitly delegated that bootstrap step; it may not create
additional worktrees/clones or a second Cargo target whenever it wants a job.
The owner permits Git worktrees; follow the shared-ref and persistence rules
in [ISOLATION.md](ISOLATION.md).

## Handoff and immediate reuse

1. The planner claims a certified-ready node and records its worker, checkout,
   branch, certified base revision, owned files, model/effort and permitted
   commands. A worker cannot take a second claim while still editing the first.
2. The builder edits and runs its permitted candidate proof with a machine
   permit. It stops the job and supplies the immutable commit id, base id,
   exact diff scope, test commands/results, unrun exclusive gates and any
   requested shared-file changes. Keep a named local ref to the candidate;
   no remote publication is implied.
3. The planner records the candidate as `in-review`. The integrator verifies
   it can resolve that exact commit in the shared repository, or fetch it from
   a separate clone, before the worker's
   branch is reused. Neither builder nor integrator silently amends a queued
   candidate; a fix produces a new commit and invalidates its old evidence.
4. Once its working tree is clean and no command uses it, the builder starts
   the next assigned independent node from the planner's certified base in
   the same checkout. Do not build it on the previous unreviewed candidate or
   on the integrator's newer but uncertified HEAD. Preserve candidate refs,
   unrelated edits and caches; do not use destructive resets to make reuse easy.
5. The integrator processes candidates one at a time and re-proves the
   accumulated certified work on the new tree. The inspector checks spec
   drift and fresh exclusive evidence at that revision. Only the planner can
   certify the node and release its dependents.

A rejected candidate returns as a repair assignment against its exact base
and current certified dependencies. Preserve any new in-flight node before
switching assignments. Repairs that unblock others outrank starting another
unrelated feature. Work already queued or underway against a changed shared
contract becomes stale and is revalidated before it can count.

## Keep the queue moving, not growing forever

Refill a free builder slot as soon as independent ready work and review
capacity permit. Choose critical-path unblockers first, then tasks that fit
the worker's context and the available compiler/test resources. Never relax
dependency certification merely to keep a worker occupied.

Start with a queue high-water mark equal to the builder count, recorded in the
graph. When that many unmerged candidates await verification, pause new
independent dispatch and favor review, repairs or bounded read-only work.
Already-running nodes may finish; never discard them to meet the queue target.
Adjust the bound from observed waiting time and certification throughput.

The planner assigns compiler permits and records the owning process families.
Ready integration receives priority at the next permit release; it cannot be
starved by builders repeatedly reacquiring permits. Unused capacity may serve
builders rather than being permanently reserved for an idle verifier. A worker
releases its permit when the command finishes and waits for another if needed.
Compiler permission is not permission to change checkout contents while a
command is still reading them.

Before a measured run, stop dispatch, drain or safely cancel all run-owned
builds/tests/apps, collect acknowledgements, and let the inspector verify the
machine is quiet. Freeze the measured tree and pause other run-owned local
tool work until release. Outside load invalidates the sample; never kill
unrelated user processes. Cached tests cannot stand in for fresh timing,
network-fixture, GPU, accessibility or lifecycle witnesses.

## Context and model choice

Follow [CONTEXT.md](CONTEXT.md) for every role's small current packet,
checkpoint triggers, fresh-context handoff and resume checks. Keep the
checkout/cache warm independently of the conversation. Every assignment
restates the exact node and current certified base. A follow-up is not a
context reset, and a written summary is not proof of forced compaction.

Inspect available models and effort levels at launch. Preserve the parent's
settings unless the owner or this skill's task-based routing justifies an
override; state overrides in the brief. Use a strong high-effort model for
shared runtime, authorization, persistence, editor state and recovery work;
use higher effort for ambiguous failures and foundational design. Routine
well-specified workflow migrations can use a balanced coding model at medium
or high; mechanical checks and reference audits can use a smaller model with
bounded proof. Running an already-defined command does not need the same
reasoning budget as deciding whether its evidence is sufficient.

Scheduling belongs to the planner, not the inspector. Spend the strongest
reasoning on difficult re-cuts and cross-feature decisions. A high-effort
verifier with a precise spec may be enough for ordinary certification;
escalate subtle security, concurrency or measurement problems. Do not claim a
model is best on this repository without comparing accepted work, rework and
latency. A model's greater effort does not reserve local RAM or improve an
invalid test.

When the session exposes `collaboration.spawn_agent`, use a focused
`fork_turns="none"` brief for a new worker; an explicit model/effort override
must obey that tool's restrictions. Use `followup_task` to give a completed
worker its next node, and `send_message` for a running worker's steering.
Completion notifications drive scheduling; do not busy-poll. Follow-up tasks
do not themselves change the worker's model. If a different model is needed,
use a newly configured agent within available slots and the actual lifecycle
controls; do not assume interrupting a worker frees its slot or kills its jobs.

## Persistence and stopping

After launch is authorized, keep coordinating through idle workers, pending
reviews and ordinary failures until the requested milestone's proofs pass.
Write enough graph state to resume after an interruption: candidate/base ids,
certified revision, queue, ownership, active processes and next ready nodes.
A queue entry or successful candidate test is never a certificate.

Keep the owner informed at the cadence required by the current session and
surface meaningful failures early. Ask for a decision when scope, authority,
isolation or a required human judgment is missing; do not invent approval to
avoid stopping. If account, tool, session or infrastructure limits interrupt
the run, checkpoint and report the exact remaining work. A skill is a procedure,
not a durable background service or a guarantee of unattended completion.
