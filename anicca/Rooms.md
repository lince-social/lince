A room is a conversation with more than two people in it. Lince's transport already does this and its semantics deliberately do not, so a room is not a new subsystem — it is a Conversation-like root granted to several contacts, plus everything that makes a room feel like a room.

What is already true. `replica_grant` is keyed `PRIMARY KEY (root_record, contact_organ)` at `0042_individual_replica.sql:45`, so one root holds a grant row per contact and fans out to all of them, each with its own offered or accepted state, and revoking one deletes one row. Containment is settled separately: every Record carries `record.replica_root` pointing at its root, resolved once at creation, so a grant covers the whole tree without walking assertions. What is two-party is the semantics above that — `threads.rs` builds a Conversation as the root shared with one contact, and `start_conversation` takes a single `contact_organ`.

Three things a peer-to-peer room cannot pretend about, and each is a design consequence rather than a caveat.

Membership is not a list. Each grant is independent, so nobody can enumerate who else holds the root, and there is no agreement on who is in. Somebody has to state it, and the honest answer is that the room's creator does, as a Record everyone can read and disagree with.

Removal is not deletion. Revoking a grant stops future ops; it does not recall what was already delivered. A room that implies a moderator power it does not have is worse than one that admits it has none.

Fan-out is not free. Every message becomes an op to every outbox, so chatter in a five-person room is five times the traffic. That is the reason ephemeral traffic stays off the Record path rather than a performance note.

A person about to type into a synced thread should know it is not local, which makes "who can see this and who is in it" part of the room rather than a settings screen.

- [ ] Build the room: one root granted to several contacts, so several people can share one thread.
- [ ] State membership as a Record everyone can read and disagree with, written by the room's creator.
- [ ] Say plainly what revoking does not undo — it stops future ops and recalls nothing already delivered.
- [ ] Say who can see a thread and who is in it, on the thread.
