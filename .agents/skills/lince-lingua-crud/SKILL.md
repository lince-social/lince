---
name: lince-lingua-crud
description: Read and change Lince .lingua Record projections and coordinate Lince task Records safely. Use for .lingua files, docs/records, Instinct content, File Sync-backed Records, and task assignment or state changes.
---

# Lince `.lingua` CRUD

Treat `.lingua` as a projection of Lince data, not YAML, generic front matter,
or a second database. Lince owns the data; the current parser and validator own
the file shape.

## Read the current contract

1. Read [references/bootstrap.md](references/bootstrap.md).
2. Read the current parser and the contract for the target folder before
   editing. Do not rely on a schema copied into this skill.
3. Use the current validation interface available to the agent or harness.
   Prefer deterministic parsing over visual inspection.

Keep these stable meanings in mind:

- The fenced prelude is machine-readable Lingua; everything below it is the
  Record body.
- `uid` is identity. A filename or title is not.
- `@@concept` says what the Record IS. `@concept` states an assertion.
- `[[Title|uid]]` links by uid; the title is only for readers.
- Quantities are exact decimal text, never floats.
- A prelude edit is a real mutation. Invalid input must fail as a whole.
- Preserve supported syntax you do not understand. Never simplify it into an
  older shape.

## Find the needed context

- Search exact prelude Concepts first, then body text. Follow `@part-of`,
  `@chapter`, and `@see-also`; do not infer structure from filenames or
  directory order.
- Read only the relevant Records. Use the relevant Instinct Records for
  unfamiliar Lince fundamentals. Use the matching project document for
  architecture and its task Record for open work.
- Report ambiguity. Do not invent a Concept, uid, link target, unit, or field.

## Coordinate task Records

Before coding, find the relevant `@@task` Records and inspect their quantity,
`@wip`, and `@assigned-to` state.

- `1` means done or stable; `0` unplanned; `-1` todo; `-2` plus `@wip` in
  progress. Treat any quantity below `-1` as active work.
- Claim a task when the user assigns it to you or asks you to take the next
  open task. Confirm your Agent name and uid from current Lince or harness
  state; never guess them. A matching Agent assignment does not prove this
  chat owns the task. If the harness exposes active chats or runs, find the run
  doing the work; leave the task alone when another run is active.
- Where the current contract permits it, claiming means adding
  `@assigned-to [[Agent name|agent uid]]`, adding `@wip`, and setting the exact
  quantity to `-2` through the available agent interface.
- Never remove or replace another assignee unless the user explicitly hands
  the task over. If another agent holds it, leave it and choose other work.
- A new user assignment is another task claim, not permission to erase an
  earlier one. Finish or explicitly hand off earlier work before removing your
  assignment.
- Land work completely. Delete its open task entry and move durable knowledge
  into the explanatory Record; do not leave a checked box that means “mostly.”

## Change a Record

### Create

- Determine whether this is an ordinary File Sync folder or a shipped,
  cross-linked bundle. A hand-written File Sync file may omit a uid; a bundle
  may require a deterministic pre-minted uid.
- Use the current uid mechanism and vocabulary. Never copy or fabricate a uid,
  resolve a link by title, or create a Concept as an import side effect.
- Include the target folder's required identity, selection Concept, parent,
  and quantity state.

### Update or rename

- Keep the uid unchanged and make the smallest semantic edit.
- Preserve unrelated assertions, quantities, body text, and newer syntax.
- Resolve every link target before editing it. A title change must not retarget
  the uid or trigger unrelated body rewrites.

### Delete

- Confirm whether the request removes one file projection or the Record. With
  multiple formats, a Record may remain until every projection is gone.
- Check incoming links, children, filters, and bundle rules. Do not orphan data
  or infer broader deletion authority.

## Validate

1. Run the narrowest current parser or validator covering every changed file.
2. Run the repository checks in
   [references/bootstrap.md](references/bootstrap.md) for `docs/records`.
3. Inspect the diff for lost uids, prelude expressions, links, or body text.
4. If validation refuses the edit, report the refusal; do not weaken the
   contract or guess a fallback representation.
