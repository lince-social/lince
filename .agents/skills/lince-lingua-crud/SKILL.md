---
name: lince-lingua-crud
description: Read and change Lince Anicca .lingua Records and coordinate Lince work safely. Use for anicca/, .lingua files, File Sync-backed declarations, and task state changes.
---

# Lince `.lingua` CRUD

Treat `.lingua` as Lince data with one typed language, not YAML, generic front
matter, Markdown with a generated prelude, or a second database schema.

## Read the current contract

1. Read [references/bootstrap.md](references/bootstrap.md).
2. Read `crates/anicca/src/grammar.rs` and the relevant part of
   `anicca/Lingua.lingua` before editing. Do not rely on syntax copied into a
   prompt, this skill, or `AGENTS.md`.
3. Parse before interpreting and use the Rust validation interface after every
   change.

Keep these meanings in mind:

- An unqualified top-level declaration is a Record. The JSON string before `(`
  is its title and the text inside `{ ... }` is its Markdown description.
- `@slug: quantity` gives a Record an addressable slug; a bare quantity creates
  a valid slugless Record. Slugless Records cannot be the target of authored
  `@slug` references.
- Immutable uid identity is machine-owned closing metadata. Never fabricate,
  copy, or change it. A title and slug are not uid identity.
- `is #concept` is the identity assertion. `#concept` is ordinary;
  `#predicate @target` is binary; `#predicate: quantity` carries an amount.
- Record quantity and assertions such as `#done`, `#todo`, and `#wip` are
  independent facts. Never infer one from the other.
- Local references are written by unique slug and resolved project-wide to uid
  before projection. Unknown or duplicate identities fail the whole change.
- Comments in Record metadata are syntax and must survive formatting and
  runtime write-back. Inside the description, `//` is ordinary text.
- Quantities are exact decimal text, never floats.

## Find the needed context

- Search exact `is #concept` and `#concept` metadata first, then description
  text. Follow binary assertions such as `#part-of @slug`; do not infer
  structure from filenames or directory order.
- Read only relevant Records. Use `anicca/Ontology.lingua` for task order and
  `anicca/Lingua.lingua` for language design.
- Report ambiguity. Do not invent a Concept, slug, uid, target, unit, or field.

## Coordinate work

Before coding, inspect `anicca/Ontology.lingua` under "What we work on next".
There is one task statement, not a mirrored task Record elsewhere.

- Record quantity is exact state. Assertions such as `#todo` and `#wip` add
  meaning but do not secretly rewrite quantity.
- Never replace another assignee or active work claim. If live Lince or the
  harness exposes assignment state, use it; never guess an Agent uid.
- Land work completely. Remove a finished open item and move durable knowledge
  into the nearby prose rather than leaving a checked duplicate elsewhere.

## Change a declaration

### Create

- A person may omit uid identity; Lince mints it and writes it after the closing
  brace before projection.
- A person may omit a Record slug by writing only its quantity. Give it a slug
  only when it needs a readable unique address.
- Resolve every referenced slug across the whole target directory before any
  database mutation. Never create a Concept or retarget by title as an import
  side effect.

### Update or rename

- Keep machine uid identity unchanged and make the smallest semantic edit.
- Preserve unrelated metadata, comments, quantities, and description text.
- A title edit is presentation. A slug edit changes the readable address but
  must not change uid identity.

### Delete

- Confirm whether the request removes one file projection, one declaration, or
  the database object. Check incoming `@slug` references first.
- Do not orphan data or infer broader deletion authority.

## Validate

1. Run `cargo run --offline -p anicca -- check anicca` for the checked-in tree,
   or the same command with the actual File Sync directory.
2. Run `cargo run --offline -p anicca -- fmt TARGET`; use `--write` only when
   formatting is intended.
3. For parser or projection changes, run `cargo test --offline -p anicca` and
   the File Sync tests covering Anicca.
4. Inspect the diff for changed uids, slugs, quantities, assertions, comments,
   or lost description text. If validation refuses the edit, do not weaken the
   contract or guess a fallback representation.
