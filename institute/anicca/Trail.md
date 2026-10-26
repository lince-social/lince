# Trail — ordinary linked documents

A Trail is a use of Records, assertions, Vocabulary, Karma, resources, and views. `is #trail` is ordinary discovery data. There is no mandatory index, root Record, published catalogue, transfer protocol, learning vocabulary, or built-in leveling formula. Levels, mastery percentages, requirements, answers, completion, and points remain replaceable authored data. An author may define level 100 as their theoretical maximum; Lince does not impose that meaning.

## Implemented

- Shared file/HTTP(S) references, document bases, fragments, known Record IDs, exact hashes, and optional alternate locations.
- Compact composition imports with each part's own base, cycle detection, identity checks, and conflict refusal.
- Durable URI assertion targets; accepted local definitions supply derived links without replacing the original URI or creating placeholder Records.
- Private HTTP caching with freshness, conditional requests, Vary, no-store, and streamed bodies. Saved files and accepted definitions survive automatic cache loss.
- Incremental media reads using strong validators; safe complete-response fallback and whole-file verification when a pinned range snapshot becomes complete.
- Read-only source browsing separated from Add to Cell, accepted source updates, personal state, and reviewed Karma activation.
- Ordinary Markdown links, Lince references, transclusions, assertion links, and media controls use shared source/resource handling.
- Folder selection without an index; ZIP extraction; external files registered in place; optional owned retention and portable export.
- Institute index removed. Philosophy, What is Philosophy, Philosophers, and Plato are separate Records with empty descriptions and parent/child links. The original EPUB remains external.
- Development, checklist, internal assertion/points, and unscored examples remain authored choices. Ergon retains discovery links without provider integrations.
- Handbook and practice declarations moved out of the executable. The owner's `institute/anicca/Lince.lingua` was read, not edited.
- Optional content ZIP generation and a separate Nix content package. Binary-only startup remains usable.

## Authoring and use

Use hyperlinks for navigation and media. Use imports only for declarations required to compose a document:

```text
Import "./parts/Definitions.lingua"
Import @r_00000000000000000000000001 from ["./parts/Definitions.lingua", "https://example.org/parts/Definitions.lingua"]
```

The second form names one Record's defining document and gives alternative locations for it. It does not import several unrelated publications. UID lookup uses known source mappings; an unknown ID needs a location. A UID is identity data, not proof of authorship.

URI assertion endpoints remain short:

```text
#part-of "./Philosophers.lingua#r_00000000000000000000001003"
```

Markdown links and `[[Title|./Plato.lingua#r_00000000000000000000001004]]` can navigate the same graph. `![[...]]` embeds a view with bounded recursion. Relative references use the declaring document's effective location, including after redirects. Queries affect retrieval; fragments select within the result. Native file-picker paths are converted to file URIs without treating filename punctuation as URL syntax. A remote source cannot authorize a local-file read.

Open any intended node through Read source. Navigation retrieves accessed documents and explicit imports; displaying a hyperlink does not fetch its destination. Use Prepare Add to Cell to review definitions, then Add to Cell. Accept source updates explicitly. Enable reviewed Karma separately when the publication authors actions. Plain, unscored documents need no auxiliary binding Record. Personal edits and evidence survive accepted definition updates.

Use a local folder to select definitions, or Extract ZIP to obtain an ordinary directory tree. Existing originals can stay in that tree; opening/registering them does not copy them into owned storage. Keep offline makes an owned retained copy. Export may include originals or retain their external addresses. The local link audit checks a selected scope and reports remote/outside-scope targets without crawling them. Optional inverse predicate pairs are audit input, not a built-in Trail ontology.

Create a content-only distribution with `cargo xtask content DESTINATION.zip`, or add an executable with `cargo xtask content DESTINATION.zip EXECUTABLE`. The archive uses `content/` and optionally `bin/lince`; relative references survive moving the tree. Nix exposes `packages.<system>.lince-content`. Handbook discovery checks `LINCE_CONTENT`, adjacent `content/`, then the user's configured content location. `LINCE_CONTENT` can name the publication directly or a content directory; remote directory roots need a trailing slash. Development shells default to the repository's publications.

## Measured verification

- Store snapshots and assertion transactions: 44 tests passed, including URI persistence, accepted source associations, rollback, and corrupt-data refusal.
- Source import/browsing, directory sync, and Instinct import: 30 tests passed. Personal edits, authored points, scoped names, and independent entry nodes were exercised.
- Protein authority/query tests: 78 passed, including the new exact-URI grant test. The four existing assertion-intent disagreements below remain separate.
- Final reference/publication/archive/cache checks: 24 tests passed. Coverage includes constrained mirrors, actual obtained entries, literal native filename punctuation, changed redirect bases on 304, moved exports, and malformed ZIP entries. Earlier range and FFmpeg retention tests also passed.
- Native controls: seven focused tests passed, including ordinary Record-card navigation, connection context, source-selection reset, bound inputs, reviewed consequences, hyperlinks without prefetch, and remote-file restrictions.
- Fourteen final offscreen captures were generated across 360/680 widths; representative views were inspected. Browsing from Philosophy and Plato, Add to Cell, reviewed Karma, answer points 5/0, EPUB viewing, owned retention, and view-state round trips passed. Short destination labels fit the narrow source view.
- Grammar round trips, optional external-content startup, and practice data passed. Native and headless checks passed. Scoped formatting and migration guards passed; all 41 checksums matched.
- The regenerated content-only ZIP contains 42 files and no index/executable. After relocation, all 11 Philosophy navigation links resolved, the original EPUB bytes matched, and the external handbook loaded. The ordinary empty/folder/direct-file startup cases also passed.

Verification artifacts live under `/home/user/tmp/lince-trail-disk`: `final-validation.log`, `final-http-tests.log`, `final-content-tests.log`, `content-verification.json`, `trail-verified-captures/`, and `lince-content-complete.zip`. They are generated build output, not required publication files.

## Known limits and recommendations

- Opaque IDs have no global discovery service. Publish a reachable URL/path and reverse context links for entry from a child.
- Identical UIDs or titles do not authenticate mirrors. Exact pins constrain bytes; conflicting known revisions need an explicit source/revision choice.
- HTTP directory listing is not portable. Publish linked loose files or an ordinary ZIP; Lince does not scrape an arbitrary host's folders.
- Inverse links are author-maintained data. Audit selected local scopes; unvisited remote targets remain unverified.
- Automatic cache storage has no new quota/eviction interface in this delivery. Keep its files disposable and retain essential originals explicitly; consider a generic cache-management control separately.
- Partial bytes cannot prove a whole-file hash. Strong validators prevent mixing range versions; the declared hash is checked once all chunks are present. Explicit retention verifies the complete resource.
- Absolute native paths are machine-specific. Prefer relative document links in portable publications and keep original provenance when exporting rewritten definitions.
- Export/audit recognize the supported inline link forms. Exotic Markdown constructs require further parser coverage; authors should verify exports with their content before distribution.
- Handbook/practice declarations load at startup. Reloading or changing the configured handbook during a running session remains separate work.

## Deferred work

1. Perform physical keyboard, multiple-display, accessibility, and live video/audio checks; offscreen captures cannot establish those results.
2. Reconcile the existing Protein assertion-intent test expectations with current grant aggregation/content-selector behavior. Four tests disagree with code already present at HEAD; this delivery preserves that policy behavior.
3. Expand Institute routes through reading, argument maps, projects, experiments, translations, agriculture, STEM, biology/medicine, philosophy, and production. Retain the Socratic argument, low-voltage circuit, and soil-infiltration proposals. Early literacy and knowledge-provider integrations remain outside this task.

For verification, use disk-backed `/home/user/tmp/lince-trail-disk` for TMPDIR/Nix cache. Stay on the current branch and preserve unrelated work. Do not alter past migrations, AGENTS.md, README.md, or the unplugged web crate.
