---
name: lingua
description: How to read `.lingua` Records — the owner-authored files under `anicca/` that hold Lince's documentation, declarations, and task plan. Use before reading or quoting a `.lingua`, when planning from one, when reconciling it against an adjacent `.md`, or when asked to change what a Record says.
---

# Lingua

`.lingua` files under the repository-root `anicca/` are **Records**: the
owner's documentation, declarations, and task plan in one format. Instinct
ingests them, so they are read by people who were not in your conversation.

## Read them. The owner writes them.

Read any `.lingua` freely, quote it, plan from it, reconcile against it.

When you have text that belongs in a Record, put it in chat or in the adjacent
`.md` and the owner pastes it. The editing stays theirs — a Record half in
their voice and half in yours is one nobody can trust.

This holds for prose, a heading, a typo, a checkbox, a `quantity`, everything:
never write into a `.lingua`. The one exception is the owner asking for that
exact edit in that exact file.

Your own writing goes in `anicca/<Same-Name>.md`, beside the `.lingua` it
belongs to. Create them whenever you want one. Nothing ingests them.

## Precedence

Where `anicca/<Subject>.lingua` and `anicca/<Subject>.md` both exist, the
Record is the higher source of truth and the Markdown holds agent reasoning.
Reconcile the Markdown to the Record. If they conflict, keep the Record's
decision and say out loud that the Markdown disagrees.

## Reading a Record

```
Lince, the tool (@tool: 1 @apple, is #chapter, #instinct, #part-of @first-steps, #done) { r_YJQ...
Free description text: Markdown, mermaid, whatever the owner wrote.
} r_YJQ...
```

- **Title** — everything before `(`.
- **`@tool:`** — the slug, the short name other Records reference as `@tool`.
  A Record may be anonymous, and then the header opens straight at its quantity.
- **`1 @apple`** — the quantity and its optional unit, itself a Record's slug.
  Negative is a Need, positive a Contribution, zero neither.
- **`is #chapter`** — an identity field: what this Record *is*.
- **`#instinct`, `#done`** — assertions: tags the owner applied.
- **`#part-of @first-steps`** — an assertion linking to another Record. An
  assertion may also carry an amount, as `#needs: 3 @apple`.
- **`r_YJQ...`** — the durable uid, written back into the file by the tooling.
  It is authoritative; the title is decoration.
- **`[[Title|uid]]`** in body text — a link to another Record, resolved by uid.

A file with no metadata block is adopted: derived uid, filed under the root.
That is normal, not a defect.

## Authority

`crates/anicca/src/grammar.rs` is the sole syntax authority — a typed
`rust-sitter` grammar shared by parser, formatter, and checker. The living
explanation is `anicca/Lingua.lingua`. Read those rather than trusting a schema
copied into some other document, including this one.

## Commands

```
cargo run -p anicca --bin lingua -- check anicca
cargo run -p anicca --bin lingua -- fmt anicca
```

`check` parses the tree and counts Records, Frequencies, and Rules; `fmt`
without `--write` prints and changes nothing. `fmt --write` rewrites the
owner's files, so leave it to the owner.
