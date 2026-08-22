# Lince

## No comments in code

Source files carry **no comments**. None — not `//`, not `///`, not `//!`, not
`/* */`. Do not add one, do not restore one, do not "just this once" for a
tricky line.

Naming and structure carry what the code does. Everything else — the reasoning,
the history, the rejected alternative, the cluster a change belongs to — goes in
`anicca/*.lingua`, which is the project's actual memory and is versioned,
linked and searchable. A comment is a second place for that, and the second
place is the one that goes stale.

Never put planning vocabulary (cluster names, task ids, dates, decisions) into a
source file.

## Only the owner writes `.lingua`

Never write prose into a file under `anicca/`. Read them freely.
Ticking state is allowed — a checkbox, or the `quantity:` / `@todo` / `@wip`
that goes with landing a task — but only for work that is actually finished.

The owner gives one line — one feature. We discuss it and the implications of
implementing it. The owner does the editing. The Records are the truth of the
plan and are decided entirely by them; an agent writing into them turns the
agent's design into something that reads as the owner's decision.

When discussing, separate what is **needed for it to work** from what is
**needed for it to be fast**, and say which is which. A slowness nobody has
measured is not yet a problem.

### The `--- AI Below Only ---` line

In a file under `anicca/`, everything from the top of the file down to the line
`--- AI Below Only ---` is the owner's. Never change it, under any
circumstance, for any reason, however small or however obviously right the
change looks. Only what sits **below** that line is yours to write.

A file with no marker is protected in full. An agent never places, moves or
removes a marker; only the owner does. Moving it is not a small edit — deleting
it makes the whole file read as the owner's, and adding one shrinks the owner's
part to whatever sits above it.

The one exception above the line is state that goes with landing finished work:
ticking a checkbox, or changing `quantity` / `assert todo|wip|done|stable`.
Nothing else — not a typo, not a stray blank line, not a heading, not
re-wrapping a paragraph.

`.claude/hooks/anicca-guard.pl` enforces this on every `Edit`, `Write` and
`Bash` call, reverting the file and failing the call. Enforcement is not
permission: an attempt that gets reverted is still a rule broken.

**The guard cannot tell the owner's writes from an agent's.** It compares the
protected region against a snapshot taken just before the call, so an edit the
owner makes while a call is running looks exactly like an agent's and is
reverted. That has already cost the owner one edit. So the guard never destroys
to protect: the version it is about to replace is copied to
`.claude/.anicca-guard/attic/` first and the error names the path. If a revert
was wrong, restore from there and say so.
