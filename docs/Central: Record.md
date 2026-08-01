## [x] Records and the Ledger — the ground truth

- [x] Everything is a record (tasks, rules, signals, transfers, decisions,
  organs, people, saved Proteins, threads, messages); every change is a
  hash-chained fact, signed when a signer is set.
- [x] Actions: `create-record`, `set-quantity`, `add-quantity`,
  `edit-record-text`, `set-slug`, `set-concept`, `set-unit`, `set-place`,
  `set-extension`, `activate`/`deactivate`, `delete-record`.
- [x] Undo = `compensate { fact }` — appends the inverse delta with
  `cause=compensation`; there is no destructive undo.
- [x] Any sand adds `include: { facts: { limit: N } }` for a free "why did
  this change" drawer (delta, at, cause_kind, cause, actor).
- [x] Checkpoints snapshot levels; compaction folds pre-checkpoint history
  into a cold, hash-anchored archive; retention horizon is per record-kind,
  no policy = keep forever.
- [x] `quantity` is a cache, the fact is truth — negative = Need, positive =
  Contribution, zero = peace; activation of rules/transfers/signals/sands is
  the same knob.
- [x] Deactivate and delete are different: `deactivate` (quantity → 0) keeps
  the record on every read surface (an honest zero-quantity column);
  `delete-record` HARD-tombstones it off every read path (Proteins,
  resolve, rule inputs, checkpoints) and frees its slug — the Ledger stays
  untouched, facts and hash chain remain, with a final zero-delta
  annotation recording the deletion.
- [x] Metadata edits drop zero-delta annotation facts so live subscriptions
  refresh.
- [x] Re-appending a fact whose uid already exists is a silent no-op (replay
  safety); slugs are optional `dot.case` local sugar, uids are identity —
  Actions accept either.

### Decided 2026-07-31 — worklogs stay properties, and become typed

Worklogs were considered for the Ledger, as time-concept delta facts. **They
stay record properties.** A worklog is an attribute of a task, not a movement of
a resource, and forcing it onto the Ledger would need a separate accumulator
Record per tracked thing — because a task's quantity already means its Need
level and cannot also mean hours. Rules do not read worklogs and do not need to.

What changes is *how* they are stored, because "some ideas are first-class
citizens" and this is one of them:

- [ ] **The `work` namespace becomes a typed, validated extension, not
  freestyle `fds`.** Same table, same single-row read — so nothing gets slower
  and nothing migrates — but the shape (`start`, `due`, `estimate_min`,
  `logs: [{start, end}]`) is a real type checked at the Action boundary rather
  than whatever JSON a client happened to send. The extension stays the escape
  hatch for one-off workflows; a namespace that several surfaces depend on has
  outgrown being an escape hatch.
- [ ] **`version` on `record_extension` starts meaning something.** The column
  exists and is always 1. A typed namespace is what makes it usable: a reader
  can refuse a shape it does not understand instead of silently seeing missing
  fields as absent values.
- [ ] **Editing a worklog is an operation, not a blob replacement.** Today the
  client splices the array and re-uploads the whole object, so a correction
  erases what it corrected and concurrent edits lose each other. Per-field
  merge is in `docs/Central: Sync and Organs.md` — the same problem as
  concurrent body editing, one nesting level up.


| Columns  | User Input | Actual Record | Data Type       |
|----------|------------|----------------|-----------------|
| Id       |            | 1              | Number (Int)    |
| Quantity | -1         | -1             | Number (Float)  |
| Head     | Eat Apple  | Eat Apple      | Text            |
| Body     |            |                | Text            |

All possible Needs/Contributions, be they habits, tasks, ideas, notes, items, goals... are put into Records. They are how we model everything in Lince. The rest of the features is just a way to interact with the Records, to change them, to act on the world based on the Record's state.

'id's are automatically generated.

Lince is build around the mental framework/philosphy that the 'quantity' represents the availability of the Record. If that quantity is negative, it is a Necessity, if positive, it is a Contribution, zero makes it not a Necessity and not a Contribution.

'head' can be thought of as a title and 'body' as a description.

So, for an example, imagine that you like apples and you want to create a task to eat it today. You create a Record, giving it '-1' to the 'quantity', for that action is a Necessity in your life right now, and 'Eat Apple' to the 'head'. The end result is the Record shown at the start.

Here is an example of different possible records for individual items and actions.

| Id | Quantity | Head        | Body            |
|----|----------|-------------|-----------------|
| 1  | -1       | Eat Apple   |                 |
| 2  | -1       | Apple       |       |
| 3  | -1       | Meditate    |           |
| 4 | -1 | Client Meeting | Remember to talk ab... |
| 5 | 0 | Class XYZ Notes | Introduction: The ... |

Records also have more data associated with them. Only 'quantity', 'head', and 'body' would not be enough to model all Needs. What about cost, location and media properties? That's why we have Record Metadata.

There's 'category' which is used mostly as tags to filter: 'task, work, projectMapleSyrup'.

There's also the Extension table, with a 'freestyle_data_structure' property or 'fds' for short. It can store anything, it's whatever. In the end it's just another text field that could go into like the 'body' of a record but it would pollute it, and we have no way of controlling the version of that fds. You can put anything in there like json to make a chess game with the past moves and current state, the sky is the limit.

The extension is a quick fix to the problem of having a lot of different workflows. We need to think about what concepts are commonly used to make them become first class citizens in Lince, like the cost of stuff and location, if they are simply text that we know the structure it is not as efficient to deal with and a security problem.

Different components in the Web Interface have further Metadata on Records, such as: logging time spent done something when using Records as tasks. Assignees to set what user of the Lince Organ will do the task, time estimate, start and end date for task...
