# Shared Sands and Fiote's interface

The general Sand tasks are gathered in [Sands, Effects and Castles](interface.md#sands-effects-and-castles), [Records and collaboration](interface.md#records-and-collaboration) and [Specialized content](interface.md#specialized-content). [The Sand model](../sand-model.md) explains composition.

The owner places work useful to two people talking under Interface, and work useful only because an agent exists under Fiote. The notes below preserve the extra Fiote details for manual review alongside Fiote in [Lince.lingua](../../Lince.lingua). They are not another interface checklist.

## Conversation and terminal

Conversation shows authorship, messages being written, finished or interrupted, private drafts and reusable replies. A send control says whether it sends now, steers at the next safe point or queues after a turn. Queued entries show their age and can be edited, reordered, promoted or deleted. A consumed draft disappears only after acknowledgement; a reusable reply remains.

Keep one ordinary send button when no turn is running. Presets remain useful between people. Extra delivery controls explain when they are inactive, preserve keyboard escape from the composer and never promise immediate delivery during a running tool. Abort is a separate action.

A terminal can be opened from a task Record and display its binding. Changing or clearing the task does not kill the shell. Missing, deleted and unreadable task references have different explanations. Sharing the placement carries the reference, not the task's private data or access rights. Terminal rendering still needs a native implementation.

## A Fiote session group

A group brings together Conversation, the task Record, a tool timeline, session controls and an optional real terminal. Prefer one group per Fiote, with cubs shown as rows that can be opened into their own groups. Reopening finds the existing session and arrangement; closing the view does not stop the session.

The thread is the shared conversation. Tool calls and command output are shown read-only. The person's terminal is their own shell. Switching views does not interrupt the session, and other people can read the thread without receiving the operator's tools or layout. Session controls need their own permissions.

Show each Fiote and cub's task, state, tokens, context use and Stop. Further controls include model and thinking level, budget and spend, tool permissions, prompt Record and revision, traversal settings, queue, compaction, provider retries, spawn, fork and rename. A reached budget stops work. Explain what compaction dropped.

The tool timeline starts as a folded list of calls with target, result and duration. Show text, diffs, images and terminal output in suitable views. Allow search, filters, copying a command and navigation to its turn. Output is temporary unless deliberately promoted into a thread Message; say “output not kept” when it is gone. Re-running a side effect requires a separate decision.

Steering enters through the conversation. Ctrl-C in the person's terminal stops its command; Stop cancels the agent turn. Keep agent command output read-only unless an explicit intervention design records that a person stepped in. Interrupted work must be visible to both person and model.

Use an existing filtered Kanban or Relations view for agent tasks where it fits. If Fiote arranges Box, use the same attributed, visible and undoable operations as a person.

## Differences to reconcile with the Record

The old markdowns stored draft timing in a private extension; Lince.lingua asks for tags. The old notes also settled one-at-a-time queue delivery while the Record still asks the owner to choose. Follow the Record for both. Its remaining reference to a final CEF lane conflicts with the current no-embedded-browser direction; terminal work needs an explicit native design. This rewrite leaves those Record lines for the owner to edit.
