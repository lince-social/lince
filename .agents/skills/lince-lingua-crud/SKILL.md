---
name: lince-lingua-crud
description: Read, create, update, rename, or delete Lince .lingua Record projections safely. Use whenever working with .lingua files, docs/records, Instinct or First Steps documentation, or File Sync-backed Records.
---

# Lince Lingua CRUD

Treat `.lingua` as a projection of Lince data, not as YAML, generic front
matter, or a second database. Discover the current contract before editing;
do not preserve a copied schema in this skill.

## Establish the current contract

1. Read [references/bootstrap.md](references/bootstrap.md).
2. Locate the current parser, renderer, tests, and repository-specific bundle
   contract. Read the relevant sources completely before changing a file.
3. Prefer a current deterministic `lingua describe`, `check`, `inspect`, or
   equivalent command when the repository provides one. Treat its result and
   the parser as authoritative over examples in prose.
4. Separate stable semantics from the current syntax:
   - the fenced prelude is the machine-readable Lingua projection;
   - the remainder is the Record body;
   - `@@` identifies what the Record IS and `@` states an assertion;
   - links identify Records by uid, never by title alone;
   - quantities are exact decimal text, never floats.
5. Preserve any existing expression that the current implementation supports
   but this skill does not describe. Never delete an unfamiliar line merely
   because it is unfamiliar.

## Gather only the needed Lince context

1. Search prelude lines before searching prose. Use exact Concept searches
   such as `rg '^@interface(?: |$)'`, `rg '^@karma(?: |$)'`, or the Concept
   named by the task when those Concepts exist.
2. Follow structural assertions such as `@part-of`, `@chapter`, and
   `@see-also`; do not infer hierarchy from filenames or directory order.
3. For unfamiliar Lince fundamentals, read the smallest applicable First
   Steps Records about Record, Concept, Assertion, Quantity, Organ, Cell, and
   Protein. For implementation work, also read the relevant project-document
   Record and its task Record.
4. Treat filenames and bodies as discoverable content, not identity. A uid is
   identity; a title is decoration.

## Choose the operation

### Read

- Parse the prelude and body as separate regions.
- Explain assertions as Lince meaning, not as arbitrary key-value metadata.
- Report ambiguity instead of guessing what an unknown Concept or expression
  means.

### Create

- Determine whether the target is an ordinary File Sync folder or a shipped
  bundle. An ordinary hand-written file may be adoptable without a uid; a
  cross-linked bundle may require pre-minted deterministic uids.
- Use the repository's current uid minting or conversion mechanism. Never copy
  a neighbouring uid, make up a plausible one, or address a link by title.
- Use only Concepts that the target Cell or bundle deliberately provides.
  A file must not invent vocabulary as a side effect of import.
- Include every local invariant required by the target bundle, such as its
  selection Concept, identity, parent relationship, and quantity state.

### Update

- Keep the uid unchanged.
- Make the smallest semantic edit. Preserve unrelated assertions, quantities,
  body text, and syntax introduced by newer format revisions.
- Resolve the target Record before adding or changing a link. Preserve its uid
  even when its displayed title changes.
- Treat changes to identity, assertions, links, units, and quantity as real
  Lince mutations. Do not call the prelude decorative metadata.
- Never partially salvage a refused edit. Unknown Concepts, malformed links,
  or invalid exact quantities must fail closed.

### Rename

- Preserve the Record uid.
- Follow the target folder's current head/filename and link-title rules.
- Update decorative link titles only where the current validator or renderer
  requires it; never retarget a link and never rewrite body prose merely to
  chase a rename.

### Delete

- Establish what deletion means for the target before removing anything. In a
  multi-format File Sync folder, removing one projection may not delete the
  Record; removing all projections may.
- Check incoming links, parent/child assertions, selection filters, and bundle
  validation. Do not silently orphan Records.
- Ask before proceeding when the requested scope does not clearly authorize a
  Record deletion rather than removal of one file projection.

## Validate

1. Run the narrowest current parser or bundle validator covering every changed
   file.
2. For `docs/records`, run the repository commands listed in
   [references/bootstrap.md](references/bootstrap.md).
3. Run relevant Rust tests when projection semantics changed; use `cargo
   check`, never `cargo build`, for compilation verification in Lince.
4. Inspect the diff. Confirm that uids, unrelated prelude expressions, and
   unrelated body text did not move or disappear.
5. Report refusals as refusals. Do not rewrite a file into a guessed older
   shape to make it pass.
