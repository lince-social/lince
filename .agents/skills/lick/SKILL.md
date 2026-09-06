---
name: lick
description: Write or maintain a planning `.md` in Record voice — plain prose for what is settled, one checkbox for every piece of work left — so the owner can paste it into a `.lingua` or delete the notes it replaces. Use when asked to linguafy, clean up, restate or condense a working-notes markdown, and whenever writing up a plan, a decision or the remaining work into an `anicca/*.md` or updating one as new decisions land.
---

# Lick

A planning document accretes. Decisions get superseded in place, the same
idea is argued three times under three headings, and the work left to do
ends up buried in paragraphs where nobody can count it. Lick turns one of
those into a file in Record voice: plain prose for everything settled, one
checkbox for everything left, nothing lost.

The input is markdown written by an agent or by anyone else. The output is
markdown too — `.lingua` files are the owner's to write.

## Never write a `.lingua`

Read the `lingua` skill before you start. Your output goes in
`anicca/<Name>.md`, in Record voice, ready for the owner to paste. If a
`.lingua` Record already covers the subject, it is the higher source of
truth: reconcile to it, and where the notes disagree, keep the Record's
decision and say out loud that the notes disagree. List the conflicts; do
not resolve them yourself.

Name the output for its subject, not for the file it replaces. Karma.md was
Fiote's build notes; the readable version is `Fiote.md`.

## The line that carries all the risk

**Prose is what is built or settled. A checkbox is what is left to do.**

Everything else in this transform is style. This one is correctness, so do
it first and do it explicitly: go through the source and classify every
claim as *landed*, *decided but unbuilt*, or *open question*. Verify the
landed ones against the code — a source that says "shipped" may be a year
old.

Getting it backwards costs real work in both directions. A shipped thing
turned into a checkbox invents work somebody will do twice. An unbuilt
thing left as prose claims a feature exists, and the next planner builds on
it.

Landed items keep their detail. "`Action::CreateAgent { head, operated_by }`
ships, and the predicate is `operated-by`" is prose that stops a whole
milestone being re-planned.

## Sweep before you write

Every unbuilt thing becomes exactly one checkbox — **including the ones
buried in prose**, which is most of them. Do not sweep only the existing
`- [ ]` lines. In the notes this skill was written from, 31 items were
boxed and 93 were real; two thirds of the work was sitting in paragraphs
that read like description.

Watch for the shapes work hides in:

- "Consequence to build: …" and "What is missing is …" mid-paragraph.
- Hardening lists — dedupe, never fire on X, a ceiling per window.
- "Measure before choosing" — a decision deferred is still a task.
- "Must be checked before anything relies on it."
- Demands stated as properties: *it must say what it cut*, *it must be
  deterministic*.
- Parts lists enumerated as prose.

Then say in the output that the sweep happened and each item appears once,
the way `anicca/Ontology.md` does. Count the boxes when you finish. Fewer
than the source had is proof you dropped something.

## Compress the archaeology, never the consequence

Cut freely: superseded decisions, the debate that reached a decision, "D22
is superseded by D29", dated attributions, an objection that was later
resolved, the same point made in three sections.

Never cut: a decision's consequence, a version number, a file path or line,
a rejected option **and why it was rejected**, a named failure case, an
honest limit ("this is a convention, not a wall"), a measured number, a
verified compatibility claim.

Rejections are the highest-value sentences in a planning document and the
first thing a summariser drops. "Rejected: base64 in the body, because it
puts a megabyte into an op replicated to everyone and kept forever" is what
stops that idea coming back next quarter. Keep the reason attached; a bare
"we rejected X" gets overturned by the next person who thinks of X.

An objection that was resolved keeps its resolution, not its argument. One
line: what was objected to, and what survived.

## Voice

Match `anicca/Lince.lingua`. Plain language about what things are and how
they work.

- One paragraph per idea, on **one unwrapped line**. Break lines only
  between paragraphs, the way ordinary writing does.
- Plain words *around* identifiers, never instead of them. `effect_queue`,
  `iroh-blobs`, `Cause::fiote(session_uid)` stay; the sentence around them
  becomes readable.
- Keep the project's vocabulary exactly — Record, Fact, Cell, Organ, Sand,
  Protein, Cub. Never rename a concept for clarity.
- Drop dates, owner attributions and section numbers. The file states the
  decision, not who said it when.
- No inventory of headings, no nested sub-bullets. A sub-bullet flattens
  into a sentence inside its checkbox.
- Keep the honesty. Sentences that admit a limit are the ones a reader
  trusts.

## Checkboxes

Imperative, one action each, carrying their own reason so the box is
actionable without opening anything else. That makes them long. Long is
correct — a box nobody can act on is a box somebody re-derives.

> - [ ] Never fire on an assertion that arrived from a contact's sync. A
>   peer must not be able to spend our tokens. The assertion carries who
>   asserted it and every Record carries the Organ it came from.

Order is inherited, never invented. If the source has phases, keep the
phases and their sequence; reordering silently rewrites the plan.

## Keeping one up to date

Most of the time there is no source document — a decision was reached in
conversation, or work landed, and an `anicca/*.md` has to say so. The same
line governs: when a checkbox is done, **delete the box** and fold what it
settled into the prose above. A file where finished work stays ticked stops
being a plan and becomes a diary.

New decisions arrive as prose. New work arrives as boxes. If a decision
reverses an earlier one, rewrite the prose to state the decision now in
force and drop the reversal — the argument is not the plan.

## When the source is to be deleted

Ask, or check what the owner said. If the notes are going away, the output
must stand alone: no "see the other file", no section numbers that only
resolve there.

Then grep the repository for inbound links to the source and report every
one. Files themed to another feature are not yours to edit — say where they
point and let the owner decide. If you stripped section identifiers the
links depend on, leave a mapping block at the end of the output — old
identifier to new section name — with a line telling the owner to delete
the block once the links are re-aimed.

## Finishing

Write the file in one pass with the Write tool, rather than assembling it
through edits.

Then check:

- No reference to the source file, if it is being deleted.
- Box count reported, and higher than the source's boxed count.
- Every landed claim you kept as prose is one you actually verified.
- Conflicts with any `.lingua` Record listed for the owner.
