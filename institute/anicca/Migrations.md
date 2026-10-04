# Migration reset: database schema cleanup and optimization proposal

Status: migration consolidation is complete. The human has now authorized conservative schema optimization and removal of unused storage, while preserving business rules and automation grants. The completed changes and measured costs are recorded below; broader redesign and release acceptance remain outside this pass.

Research date: 2026-10-03. Repository: `main`, observed HEAD `8884f54e9ae73597f6b10404a389b309c10f6ffe`, with substantial staged, unstaged and untracked work. The refreshed inventory includes migrations through `0342_workspace_author_admission.sql`, including changes outside this task. HEAD alone cannot reproduce this snapshot.

The intended result is one deliberate initial schema, with obsolete storage removed and every retained table justified by existing behavior. Records, automation, sharing, Transfers and the native interface should function equally or better after the reset. Consolidation happens after the preceding feature changes settle their storage needs, and before final release acceptance. There is no requirement to upgrade old installations or preserve old migration compatibility.

The human narrowed this task to database schema optimization. Preserve the existing workflows, automation grants and related intent storage. The earlier proposals to change Transfer numeric representations, redesign authority, build missing features or rename request families are withdrawn from this plan. Backend and frontend edits are authorized only where a schema change needs corresponding consumers or regression fixes. The feature descriptions below explain what must keep working.

### Concrete direction for review

| Proposed database change | Why it could improve the existing application | Current evidence |
| --- | --- | --- |
| Remove six indexes whose complete keys are already indexed identically | Less index storage and maintenance on writes, with the same uniqueness rules | Matching columns/order/collation/predicates verified; D10 names each pair. |
| Index concept children and names in the direction the existing queries search | Avoid scanning every hierarchy edge or alias to resolve one parent/name | Isolated plans switch from scans to covering indexed searches; D02. |
| Index active assertions by predicate and extensions by namespace | Start a feature lookup from its matching subset instead of all active Records | Isolated plans switch their starting relation; realistic selectivity still needs measurement; D02. |
| Use `WITHOUT ROWID` for selected small relationship tables | Store a composite-key relationship without a second tree for the same primary key | Synthetic storage trials show a reduction for `contact_share` and `role_permission`; D08 records limits. |
| Tune indexes for active work, ordered pages and expensive JSON filters | Keep the existing worker/search operations bounded as inactive history grows | Concrete candidates identified; benefits and tradeoffs remain to be measured; D02/D10. |
| Remove proven superseded frequency/move storage and duplicated membership representation | Fewer obsolete definitions, relationships and sources to keep consistent | Frequency replacement exists; remaining dependencies and Role semantics require audit; D03/D05. |
| Reduce projection invalidation only where the existing dependency model proves it safe | Avoid extra revision writes and cache rebuilds on unrelated changes | Broad startup triggers are verified; removable dependencies are not yet proven; D07. |
| Declare the final schema directly in one initial migration | Faster, clearer installation without historical table rebuilds/backfills | Consolidation improves installation and maintenance; it does not itself accelerate steady-state queries; D09. |

Human review:

## Completed migration reset

The earlier narrow instruction authorized consolidation of the current chain without changing the resulting schema. Replayed all 163 migrations into a new database, extracted final application definitions and migration-created rows, and replaced the chain with [0001_init.sql](../../crates/store/migrations/0001_init.sql). A second fresh database created from that file matches all 283 tables, 150 explicit indexes, 258 SQL-defined triggers and one view. Column definitions, table properties, implicit/explicit index keys, foreign keys and every stored row also match. Both databases pass integrity and FK checks. SQL comparison ignores comments/whitespace and SQLite's equivalent quoting of FTS shadow-table identifiers after renames.

The initial file's SHA-256 is `91f9b3eb092cae321c5c2892927ffe85c5a562e546c0183f606d9628aad7b159`. The old chain, databases, schema extracts and comparison report are retained for this session under `/tmp/lince-migration-reset-sjhbzssk/`. The historical migration fingerprint below describes that archived chain, not the new single-file directory. Automation grants, intents, status seeds and all other schema behavior are retained. Runtime installers remain unchanged.

Historical backfill-only tests were retired or adapted to fresh-schema behavior. The durable-open regression checks a positive migration count, retained intent status seeds and database integrity. Actual SQLx/Store startup and reopen reproduce all 1,008 generated triggers and the structural seed state of the original schema. Existing development databases were not opened or deleted; the new migration history is for fresh databases.

Validation: `cargo check -p store --all-targets` passed before and after consolidation; a final check of the final source also passed in the separate `/tmp/lince-migration-reset-sjhbzssk/check-target` build directory. Nine focused test targets produced 85 passes, one failure and one deliberately filtered existing failure. The latest durable-open tests were compiled separately against the current Store and passed their seven relevant cases, including the new seed/integrity assertions. Shared build-directory contention was avoided by using compiled test binaries and an isolated final check.

Two existing test expectations remain unresolved outside this reset: `ordinary_open_keeps_its_existing_normal_profile` expects `synchronous = NORMAL`, but the existing Store implementation uses `FULL`; `admin_bootstrap_flips_admin_exists` seeds an incomplete recovery-permission catalogue. The bootstrap case was reproduced against both the archived original schema and the consolidated schema with the same current runtime code. The broader Store test run stopped at the ordinary-open failure; no full-suite success is claimed. No schema mismatch or reset-specific failure was found.


## How to review and later refresh this document

Write comments below any `Human review` line, in a table's purpose cell, or as a new paragraph naming the relevant decision, feature or table. Stable IDs such as D03, F08 and P04 make comments understandable after the document changes. An unchecked implementation step means unfinished work; it is not permission to start.

For a first review, read D01–D10 for the decisions, then the feature contract you want to comment on. The full inventory is a lookup reference. The sequential plan, acceptance checks and journal follow it.

The owner-authored [Lince Record](Lince.lingua) determines intent. This Markdown holds implementation reasoning. Separate feature proposals do not widen the authorized scope of this reset. Owner Records remain owner-edited.

When asked to refresh, preserve human comments and accepted decisions, replace observations with current evidence, and identify proposals invalidated by subsequent feature work. Do not silently reinterpret approval of an earlier version as approval of new feature scope.

Human review:

## Conservative schema optimization, 2026-10-03

The later instruction authorizes common schema optimization and removal of unused storage with minimal business-rule changes. This pass keeps existing constraints, numeric formats, scheduling rules, authorization, automation grants and current Karma frequency definitions. No frontend workflow changes are needed.

Removed `frequency`, `frequency_revision`, `anicca_rule_firing` and the old `record_move` progress table. The current frequency facade resolves and mutates `karma_frequency` and its revision/activation history. Current file-backed rules resolve named frequencies through that facade; applied occurrence receipts use `karma_rule_application`. Current moves use `record_move_offer` and `record_move_member`. The retired tables have no active SQL consumer or seeded rows. Removed the unused `frequency_uid` FK columns from `recurrence` and `recurrence_revision`, and removed the old frequency table from bundle collection, manifests, unchanged checks and destination validation. The current Karma frequency tables and bindings stay in move bundles.

Removed eight indexes: three belong to the retired tables (`idx_frequency_slug`, `idx_frequency_revision_request`, `record_move_by_contact`); five exactly duplicate retained unique keys (`karma_frequency_activation_frequency`, `karma_schedule_occurrence_activation`, `karma_program_state_event_history`, `karma_intent_event_history`, `social_gossip_item_document`). The retained keys have identical complete column order, collation, direction and predicates. No source query names the removed indexes with `INDEXED BY`. This removes redundant B-tree maintenance while retaining the uniqueness rules.

Added only these measured lookup indexes. The read fixture contains 20,000 hierarchy edges, 20,000 aliases and 20,000 Record extensions with 20 sparse namespace matches. Tests use the existing consumer SQL, run `ANALYZE`, compare result sets and take the median of 60 reads. API correctness, generated-trigger behavior and production workloads are separate checks. These are synthetic local measurements, not a production latency guarantee.

| Index | Existing lookup | Before → after median | Indexed storage at 20,000 rows | Batch insert median without → with index |
| --- | --- | --- | --- | --- |
| `concept_parent_children(parent_uid, concept_uid)` | Children of one concept; 20 matches | 2,670 → 31 μs; scan → covering search | 400 KiB | 83 → 126 ms |
| `concept_name_lookup(name, concept_uid)` | Alias resolution; two matches | 2,637 → 15 μs; scan → covering search | 504 KiB | 146 → 218 ms |
| `record_extension_namespace(namespace, record_uid)` | Active Records with one namespace; 20 matches | 2,633 → 66 μs; extension scan → namespace search | 444 KiB | 75 → 110 ms |

Write measurements use 12 alternating before/after in-memory batch insert trials and retain the original unique keys. They omit application triggers and ran alongside compilation, so absolute timings are noisy. The additional index space and write cost are accepted for these direct, repeatedly used read directions; large extension payloads are excluded from the index. A second extension fixture check returned the same 19,980 common-namespace rows before/after; its median was 149 → 91 ms in this noisy concurrent run. No broad-namespace slowdown was observed in that fixture. No speculative expression indexes, lifecycle indexes, physical table-layout conversions or membership normalization were applied.

The final working-tree initial SQL SHA-256 is `f361d671bc55fce4c4b5c800a028870e6b274d52e8eeab2e63775f274198987c`. The fresh SQL schema now has 279 application tables (273 ordinary, one FTS virtual, five FTS shadows), 145 explicit indexes, 370 FK constraints, 258 SQL-defined triggers and one view. Seeded rows in all retained tables match the baseline. Only the two named recurrence columns changed among retained table columns. SQL-defined triggers and views are identical; integrity and FK checks pass. Fresh SQL-only database storage fell from 907 to 893 pages at 4,096 bytes per page (56 KiB); this excludes later runtime installers and application data. Original reset evidence below remains historical evidence for the pre-optimization schema.

Verification artifacts: `/tmp/lince-schema-optimization-plktwlpw/` contains `before.sql`, `before.sqlite`, `source-before/`, `measurement.json`, `write-cost-paired.json`, `extension-broad.json`, `fresh-storage.json` and `schema-verification.json`. Session scripts `/tmp/lince-schema-optimization.py`, `/tmp/lince-index-write-cost-paired.py` and `/tmp/lince-verify-schema-optimization.py` record fixture construction and comparison. Preserve these if continuing outside this session.

Validation passed: `cargo check -p store -p engine --all-targets` with warnings denied; 45 focused Store tests and all 16 Engine Record-move integration tests, zero test failures. Two known baseline cases were filtered by exact name: `ordinary_open_keeps_its_existing_normal_profile` (NORMAL expectation against the existing FULL configuration) and `admin_bootstrap_flips_admin_exists` (incomplete recovery-permission fixture). These tests and their production behavior were not changed in this pass.

Store coverage includes fresh durable open/reopen, DAG and alias behavior, extension visibility, current frequencies, recurrence, occurrence processing, schedules, program history and seeding. Move coverage includes typed frequency/program revision identities, signed origins, dependency bundles, permission failures, conflicting identities, cancellation, real-peer reconnect and restart before source deletion. Store tests used the current Cargo-produced artifacts, with three current test sources compiled directly using `rustc --test -D warnings` and the matching dependency fingerprints while Engine held the shared target lock. Logs: `/tmp/lince-schema-optimization-check.log`, `/tmp/lince-schema-optimization-plktwlpw/focused-tests.log` and `/tmp/lince-schema-optimization-moves.log`. The initial Store Cargo invocation stopped at a corrected test-fixture constructor error; the interrupted queued retry is not counted as a successful Cargo test run. Engine's `cargo test -p engine --test record_moves -- --test-threads=1` completed successfully.

The index-only checkpoint is committed as `720851e1`; standalone frequency table/column cleanup is committed as `85b77198`. Only the old move-table removal and its companion consumer/test/documentation changes remain in the working tree: the replacement move implementation is another contributor’s modified/untracked work, and tracked HEAD still has old move-table consumers. Do not commit table removal alone against that HEAD; include the replacement consumers when their owner commits them. This task only removed stale frequency references in those shared files, preserving their other edits.

Human review:

## Historical research evidence and limits before consolidation

All 163 SQL migration files, from `0001` through `0342` with intentional numbering gaps, were replayed in filename order into a separate SQLite database. Each file ran inside a transaction, with foreign keys enabled. No application database was opened or altered.

| Observed result | Meaning |
| --- | --- |
| 283 non-system tables | 277 ordinary application tables, one FTS5 virtual table and five FTS5 shadow tables. This is not 283 independent product concepts. |
| 150 explicitly created indexes | Excludes indexes SQLite creates for primary keys and uniqueness constraints. |
| 258 migration-defined triggers | Excludes triggers generated by Rust during Store initialization. |
| One view | `transfer_application_effect_handoff`, combining local and remote application handoffs. |
| 375 foreign key constraints, comprising 402 column entries | Compound foreign keys account for the difference. |
| 136 table declarations containing `STRICT` | The rest of the application schema uses older declarations. |
| `PRAGMA integrity_check`: `ok` | The isolated SQL replay is internally readable. |
| `PRAGMA foreign_key_check`: no rows | No foreign key violations in the migration-created, largely empty database. |
| SQLite 3.53.3 in the research interpreter | This is the research runtime, not verification of the application's SQLx-linked SQLite version. |

These are research checks, not a full fresh-installation test. Rust seeding, identity creation, runtime trigger installation, network workers, UI workflows, representative populated queries and performance benchmarks were not executed for this documentation task. Existing tests were read for their contracts; their presence is not a passing result.

The inventory below was checked against the resulting SQLite schema and literal references in Rust sources outside the disconnected web crate and vendored code. Literal references establish possible consumers, not reachability from a user action. Conversely, no literal reference does not prove that a table is unused: runtime installers, snapshot enumeration and table allowlists use dynamic SQL. This distinction matters particularly for older Karma definitions and FTS internals.

The first inventory covered 273 tables through `0334`. Eight later migrations added ten tables; the refresh below includes them. The refreshed SQL replay passed both integrity checks. The migration-input fingerprint is SHA-256 `4a0e1129b5da7409c83d71ded59f3425fafce7e719a92ea81d4c6230d2a9abbc`, computed over filename-sorted UTF-8 lines of `filename`, one space, each file's SHA-256 hex digest, and a newline. This fingerprint detects input changes; it does not archive uncommitted source contents. Recheck it before implementation.

The pre-optimization inventory had exactly one entry for each of the 283 tables, with no missing/extra tables or duplicate entries. The current inventory below omits the four retired tables and updates the two recurrence FK target sets. Every listed FK target set matches the replayed definition, and all 319 local links resolve at this snapshot. Source files and neighboring documents may still change independently of the migration fingerprint.

Additional research used private scratch databases to compare index layouts, empty-schema query plans and two synthetic relationship-table storage layouts. D02, D08 and D10 report those observations. These are not populated application benchmarks or verification of Rust installers and consumers.

### Principal source anchors

| Source | What it establishes |
| --- | --- |
| [Store initialization](../../crates/store/src/lib.rs) | SQLite connection policy, migration runner, transaction sequence and separate opening contracts. |
| [Migrations](../../crates/store/migrations) | Existing columns, constraints, indexes, triggers, structural defaults and historical rebuilds. |
| [Seed](../../crates/store/src/seed.rs), [Cell startup](../../crates/cell/src/lib.rs), [admin bootstrap](../../crates/cell/src/admin_bootstrap.rs) | Configuration, permission catalogue, Roles, local identities and administrator creation. |
| [Record repositories](../../crates/store/src/records.rs), [Fact append](../../crates/engine/src/append.rs), [exact decimals](../../crates/store/src/exact.rs) | Existing quantity representations, folds and transaction boundaries to preserve. |
| [Access](../../crates/engine/src/access.rs), [authentication](../../crates/store/src/auth.rs), [session access](../../crates/store/src/session_access.rs) | Person authority, policy, authentication generations and stale-session refusal. |
| [Record documents](../../crates/store/src/record_docs.rs), [collaboration](../../crates/engine/src/collab.rs), [read-model rebuild](../../crates/engine/src/rebuild.rs) | CRDT qualification, property merge, replay and retained generations. |
| [Transfer capture](../../crates/store/src/transfer_replication.rs), [capture schema](../../crates/store/src/transfer_replication/schema.rs), [capture policy](../../crates/store/src/transfer_replication/policy.rs) | Runtime-generated triggers, ownership, raw row serialization and privacy filtering. |
| [Projection](../../crates/store/src/projection.rs), [snapshot](../../crates/store/src/snapshot.rs), [state hashing](../../crates/store/src/snapshot/hashing.rs) | Startup invalidation triggers and consumers that enumerate the schema. |
| [Public social storage](../../crates/store/src/social.rs), [social services](../../crates/engine/src/social.rs) | Search, publication, authority, private delivery, retention and worker ownership. |
| [Frequency facade](../../crates/store/src/frequency.rs), [durable frequencies](../../crates/store/src/karma/frequencies.rs), [recurrence](../../crates/store/src/recurrence.rs) | Current frequency path and the older tables it has displaced. |
| [Native workspace storage](../../crates/desktop/src/workspace/storage.rs), [shared-workspace engine](../../crates/engine/src/workspace_sync.rs), [interface models](../../crates/interface/src/lib.rs) | Native file restore, current shared-workspace SQL consumers and interface models. |
| [Person Roles](../../crates/store/src/person_roles.rs), [Organ access catalogue](../../crates/store/src/organ_access.rs), [Sand packages](../../crates/store/src/sand_packages.rs), [description assets](../../crates/engine/src/description_assets.rs) | Consumers of storage added since the first inventory. |

## D01 — Keep existing feature semantics while changing their storage

The owner describes the product through Records, Facts and Promises. Keep ordinary content and identity in `record`, classification in concepts and assertions, and specialized durable behavior in the existing typed sidecars or extensions. The reset is a chance to improve their keys, indexes, physical layout and constraints. It does not propose a different abstraction for creating, sharing or automating content.

There are several kinds of stored truth, and the implementation should name which one each table represents:

| Responsibility | Examples | Treatment |
| --- | --- | --- |
| Authored content and identity | `record`, concepts, assertions, extension payloads | Preserve identity and permissions; synchronize only through the relevant protocol. |
| Evidence and accepted changes | Facts, agreement events, signed requests, compensations | Preserve immutable evidence and the rules linking evidence to current state. |
| Current state derived from evidence | Record quantities, candidate state, agreement level | Define the writer and a consistency/rebuild check. Do not create a second independent authority. |
| Durable unfinished work | Outboxes, schedule cursors, delivery jobs, leases | Resume after restart; prevent duplicate effects and stale ownership. |
| Security memory | Revocation generations, ended-post floors, replay receipts | Retain for the protocol's replay window or permanently where required; expiry of the original payload does not automatically make these disposable. |
| Rebuildable cache | Projection spans, search index, discovery hints | Name the source, invalidation and rebuild path; bound storage and work. |
| Device-local presentation and secrets | Workspace snapshots, editor recovery, Fiote credential vault | Preserve their file ownership and privacy boundary; include them in installation/reset acceptance. |

Preserve current quantities, signatures, authorization, retry namespaces, retention and user-visible state. Keep state tables and history tables separate when one supports efficient current reads and the other supplies evidence. Fewer tables alone is not an optimization. Repository restructuring and new UI are outside this plan; update existing SQL consumers only as required by an approved schema change.

Human review:

## D02 — Match indexes to the existing lookup direction

Several relationship keys start with the owning Record/concept because that enforces uniqueness. Existing features also query those relationships in the reverse direction. Add or replace a secondary index where that direction is measured to matter; keep the relationship and its uniqueness unchanged.

| ID | Existing query and source | Candidate schema change | Scratch observation and remaining check |
| --- | --- | --- | --- |
| C01 | `concepts::descendants_including`: children `WHERE parent_uid = ?` | Index `concept_parent(parent_uid, concept_uid)` alongside the existing unique forward key | Plan changes from `SCAN concept_parent` to a covering indexed search. Measure broad/narrow descendant trees and extra edge-write cost. |
| C02 | Concept name resolution: `WHERE name = ? ORDER BY concept_uid` | Index `concept_name(name, concept_uid)` | Plan changes from scanning the concept-first unique index to a covering name lookup. Preserve multiple aliases, ambiguity and language behavior. |
| C03 | `ledger::records_with_concept`: active assertions whose predicate is in a concept set | Partial index `record_assertion(predicate_uid, subject_uid) WHERE retracted_at IS NULL` | Plan starts at matching predicates instead of walking active Records and probing their assertions. `DISTINCT` still needs work; compare selective and broad concept sets. |
| C04 | `records::all_extensions`: active Records with a requested extension namespace | Index `record_extension(namespace, record_uid)` | Plan starts at matching extensions instead of all active Records. Keep `fds` out of the index because large payload duplication would increase storage/write cost. |
| C05 | Active Record list: `WHERE deleted_at IS NULL ORDER BY created_at, uid` | Trial an ordered active-row index; consider replacing the existing deletion index only after auditing its other consumers | A trial `(created_at, uid) WHERE deleted_at IS NULL` was not selected on the empty schema: the temporary sort remained. This is an unresolved candidate, not an established improvement. Use populated statistics and compare a deletion-first composite alternative. |
| C06 | Anonymous post revocation in `store::social`: snippet owner key from JSON plus a generation bound | Trial expression index `(json_extract(body, '$.anonymous.owner_key'), generation) WHERE kind = 'snippet'` | Plan changes from filtering snippets to searching owner and generation. Audit all accepted body shapes and write/FTS cleanup costs before retaining it. |
| C07 | Shared-workspace history in `engine::workspace_sync`: workspace plus `uid` pagination; applied changes since a revision | Trial `(workspace_uid, uid)` and a partial `(workspace_uid, applied_revision) WHERE status = 'applied'` | Current history index is `(workspace_uid, created_at, uid)`, which does not match either ordering. These new consumers were inspected but not trialed or benchmarked. |

C01–C06 plans were obtained with `EXPLAIN QUERY PLAN` against the current migration-only schema, changing one index at a time in independent scratch copies. There were no application rows or `ANALYZE` statistics. A different plan is evidence that the key is usable, not a latency result. After feature-API fixtures and statistics exist, compare result sets, rows visited, read latency, write latency and file size before selecting the final indexes.

For JSON filters, prefer an index over the existing source expression before adding another independently maintained copy of a signed document field. SQLite requires the query expression to match the indexed expression; body validity and expression-index write behavior also need checking. [SQLite expression-index documentation](https://www.sqlite.org/expridx.html).

Human review:

## D03 — Retire the superseded frequency storage, including its remaining references

Implemented for the four obsolete tables and two recurrence columns described in the conservative optimization section. The following paragraphs preserve the original dependency audit and invariants.

`store::frequency` is an API facade over `karma_frequency`, its revision and activation tables. The independent `frequency` and `frequency_revision` tables remain from an earlier design. `anicca_rule_firing` has no literal Rust consumer in this snapshot. These are the strongest obsolete-storage candidates.

Removal still requires a complete dependency edit. `recurrence.frequency_uid` and `recurrence_revision.frequency_uid` reference the old `frequency` table. The Record-move bundle's table allowlist still includes `frequency`. Historical fixture tests may reference the old migration files. Reconcile file import/export, rule resolution and bundle collection before omitting these tables and their indexes from the new initial schema.

Keep `recurrence`, `recurrence_revision` and `recurrence_skip`: the current rule editor and runtime use them. Keep `karma_rule_frequency` and `karma_signal_frequency`, which bind rules and Signals to the current frequency model. Do not mistake multiple historical frequency designs for proof that current activation revisions, timer fencing or occurrence receipts are unnecessary.

Prefer dropping unused old columns rather than repurposing them to mean another identity. If a legacy field still stores distinct state, retain it until that dependency is understood. Removing superseded definitions is schema cleanup, not a redesign of scheduling. Preserve file-backed behavior and the current durable occurrence/application path. A retry after a database commit but before a source-file cursor advances must not apply the rule twice.

Apply the same test to `record_move`: omit the older progress table only if negotiated `record_move_offer`/member storage fully replaces its consumers. Keep live offer, refusal and outcome data. The expected benefit is a smaller, clearer schema and less redundant state; these table removals have no measured read-speed benefit yet.

Human review:

## D04 — Retain automation grants and their related intent storage

Human decision: keep automation grants. The earlier removal proposal is withdrawn. Retain all three grant tables (`karma_grant`, `karma_grant_revision`, `karma_grant_request`) and all four related intent tables (`karma_intent`, `karma_intent_state`, `karma_intent_event`, `karma_intent_status`). The prior text also overstated the family count; the current schema has seven such tables. Keep their status seeds, checks, compound foreign keys, immutable revisions, authorization and reservation rules in the consolidated schema.

The original literal-reference scan found no Rust consumers for these names. That observation is a coverage limit, not a deletion criterion or a request to build a different automation feature. Refresh their actual consumers before implementation. Current runtime checks, Organ execution capability, `signed_action_intent` and `fact_action_intent` retain their own responsibilities.

This family participates in the same physical schema review as the rest: remove an index only when an equivalent retained index and query checks justify it. For example, `karma_intent_event_history` duplicates the existing unique `(intent_hash, state_revision)` key; omitting that extra index preserves every intent event and uniqueness rule. There is no grant/intent feature removal step in the implementation plan.

Human review:

## D05 — Review the split Role membership representation

The refreshed schema already has `person_role` and the additive membership repository [person_roles](../../crates/store/src/person_roles.rs). `ids_on` unions `person_access.role_id` with rows from `person_role`; `replace_on` stores the first sorted Role in `person_access` and the remaining Roles in the join table. This is existing behavior, not a feature to introduce through this reset.

One concrete normalization candidate is to store all memberships in `person_role`, keeping `person_access` for access policy/revision. That would remove the membership union and the split writes. It is conditional: authentication/principal/display code still reads the singular field, so first establish whether it has a separate primary-Role meaning. If so, preserve that meaning and relation. Do not substitute the lowest Role ID without proving it reproduces current behavior.

Measure the tradeoff before selecting this change: one relationship source simplifies consistency, but joins and revision-maintenance triggers may cost more than the small duplicated representation. Preserve exactly the current permission union, empty-membership behavior, recovery checks, compare-and-set and session invalidation. This candidate does not alter Role scope, workspace authority or user workflows.

Shared-workspace, admission, draft and receipt tables also now exist (`0338`–`0342`). Their presence updates the inventory; their feature design is not part of this optimization proposal.

Human review:

## D06 — Tighten local constraints while respecting external identities

For retained ordinary tables, audit explicit non-null keys, `STRICT` declarations and checks against the inputs already accepted by current readers/writers. Carry existing state, counter, paired-field and JSON invariants into the initial schema. Add missing constraints only where current feature validation already defines the same invariant; document affected fixtures and rejection behavior. Constraint cleanup improves integrity, not automatically query speed. Preserve current numeric representations and JSON formats.

For every proposed foreign key, classify the target first. A local sidecar's `record_uid` can reference a local Record; a remote post, Organ, sender key, historical receipt or tombstone may intentionally outlive or precede a local row. `record.organ_uid`, many delivery identities and several journal fields currently have no declared FK. Do not add local FKs just because they end in `_uid`.

Keep compound scope constraints in Transfers: party membership, revision, agreement coalition and occurrence identity are stronger than separate existence checks. Specify deletion behavior for each retained relation. Default to retained history and logical deletion for evidence; use cascading deletion only for data that truly belongs to a disposable parent. Expiry workers must remain able to remove allowed payloads without erasing replay or revocation protection.

D08 covers selective `WITHOUT ROWID` trials. Facts and several histories/queues use rowid order; retain their current ordering mechanism. FTS5 virtual/shadow tables are SQLite-owned and are outside ordinary-table conversion. Strengthening checks also requires updating deliberately malformed fixtures so tests still exercise the intended boundary.

Human review:

## D07 — Measure projection-trigger write amplification

`Store::migrate` runs SQL migrations and then `transfer_replication::install` and `projection::install`. Both installers discover tables and build triggers dynamically. Transfer capture also serializes column names/types from the live schema. The projection installer adds invalidation triggers broadly, with special exclusions and conditions. Thus the fully opened database has more behavior than the SQL replay counted.

The physical optimization candidate is fewer unnecessary projection invalidations. Most ordinary tables receive three invalidation triggers; updates compare changed columns and some tables have extra conditions. A write to unrelated operational state can therefore advance `projection_source` and expire reusable cached work. Measure the per-write revision/trigger cost and actual rebuild rate first.

Narrow trigger tables or columns only after proving their full dependencies, including dynamically evaluated program inputs. Arbitrary/dynamic reads may require broad invalidation. If dependency completeness cannot be established without changing feature semantics, keep the existing coverage. There is no blanket trigger-removal proposal. Immutability, authorization, capture, revision and replay triggers remain required.

Choose one authoritative definition for each trigger at consolidation. Keeping generated triggers as a separate install phase is acceptable; enumerate and verify the resulting schema on fresh open/reopen. Preserve the current capture membership/privacy policy. An installer manifest can document existing behavior without redesigning which features synchronize.

`write_tx` starts `BEGIN IMMEDIATE` and advances `commit_sequence`. Transfer capture groups changes and Facts through this sequence. Preserve that transaction boundary. Opening or reinstalling triggers currently also uses this writer; verify restart effects rather than assuming startup leaves all counters untouched.

Schema changes also affect snapshot column enumeration, state hashes, Record-move bundles, Transfer messages, fixtures and signed row payloads. No backward protocol adapter is required, but all current producers and consumers must agree on the new shape in the same release.

Human review:

## D08 — Trial compact storage for small composite-key relationships

Ordinary SQLite tables with composite primary keys generally store a rowid table plus a separate primary-key index. `WITHOUT ROWID` makes the declared primary key the table's storage key. Selected small membership/acknowledgment rows can use less space without changing their public IDs or feature relationships. It can also change lookup and secondary-index costs, so this is a selective physical layout choice. [SQLite WITHOUT ROWID documentation](https://www.sqlite.org/withoutrowid.html).

First candidates: `role_permission`, `contact_share`, `lingua_concept`, `person_role` and `mailbox_device_ack`. Retain their columns, composite uniqueness, secondary access paths, foreign keys and deletion behavior. Do not convert large BLOB/JSON payload tables, integer-rowid allocation tables or histories/queues with rowid consumers simply because they have a composite key.

An isolated storage trial used the actual current declarations of `contact_share` and `role_permission`, retaining their explicit indexes. Each independent in-memory database held 20,000 deterministic rows, with 4,096-byte pages and SQLite 3.53.3. `contact_share` used 200 contact IDs, 20,000 distinct Record IDs, 36-character ID strings, integer flags and a fixed 20-character timestamp. `role_permission` used 1,000 role IDs with 20 permission IDs per Role. Parent tables, runtime triggers, application seeding and realistic lifecycle distributions were absent; foreign-key enforcement was off in this physical-layout example.

To reproduce the sample rows, insert `i = 0..19999` in that order. Contact/Record IDs use prefix `00000000-0000-0000-0000-` followed by a 12-digit lowercase hexadecimal suffix (`i % 200` / `i` respectively); `picked = int(i % 3 == 0)`, `held = int(i % 5 == 0)` and `added_at = '2026-10-03T00:00:00Z'`. Role/permission pairs are `(i // 20 + 1, i % 20 + 1)`. Compare original declarations with the same declarations plus `WITHOUT ROWID`, keeping the explicit `contact_share_by_record` index in both.

| Table | Rowid layout, allocated bytes | `WITHOUT ROWID`, allocated bytes | Observed reduction in this sample |
| --- | --- | --- | --- |
| `contact_share` | 4,952,064 | 4,059,136 | 18.0% |
| `role_permission` | 503,808 | 208,896 | 58.5% |

These are `page_count × page_size` storage observations for synthetic standalone tables, not predicted whole-database savings or read/write speedups. The first trial needs a fresh file-backed Store with realistic fixtures, full foreign keys/installers and current lookup/write workloads. Recheck secondary-index size, snapshot/capture serialization, absent `rowid` assumptions, insert-result handling and any SQLite hooks. Keep each conversion only if it reduces relevant cost while preserving behavior.

Existing request/receipt families retain their identity, replay and retention contracts. No generic queue/receipt merger or request-table renaming is proposed.

Human review:

## D09 — Consolidate actual final definitions, not historical steps

The new `0001_init.sql` should declare only the final retained tables, view, constraints, justified indexes, required triggers and structural singleton rows. Do not paste the 163 historical files together and preserve obsolete create-copy-drop-rename work, old backfills or temporary rebuild tables. This mainly improves installation work and schema readability; rewriting migration history alone does not shrink the already-final table layout or speed up its queries.

Maintain the distinct open contracts from `store::lib`:

| Entry point | Contract to preserve |
| --- | --- |
| `Store::open` / `open_memory` | Apply schema and runtime schema setup, ensure local Vocabulary and identity as their current path specifies. |
| `open_durable` | Create/migrate a durable file and install runtime schema, without domain defaults. Used by controlled durable workflows. |
| `open_existing_durable` | Open an existing file; do not create it, migrate it or insert defaults. |
| Cell seeding | Ensure configuration and permission catalogue, seed Roles and grants according to creation rules, then bootstrap identity/account behavior through its owners. |

Structural rows include `commit_sequence`, occurrence counters, rule progress, projection revision, update state, gossip scan, sync-history policy and Transfer-import control. Retain `karma_intent_status` rows and their reservation semantics under D04. Preserve `authority_seed_state` and its current once-only permission seeding contract. FTS internal rows belong to SQLite. Read seeded values from the current final definitions when implementing; do not assume every singleton starts at zero.

Seeding is idempotent but intentionally does not regrant permissions that an administrator revoked. New installation, repeated seed, restart and administrator bootstrap must preserve that distinction. Domain defaults belong to the seed path rather than making `open_durable` create a usable account.

Resetting migration history means old developer databases will not match the new SQLx checksums. Provide an explicit fresh-development setup procedure using a separate data directory, or intentional removal of a selected disposable fixture. Do not make normal startup silently delete an existing database, keyring, workspace or attachment directory. Backup/export of developer data can be an explicit convenience, not an old-schema upgrade requirement.

Human review:

## D10 — Measure query and write workloads before changing indexes

The exact duplicate-index candidates below are now removed. Five are covered by retained unique keys; the legacy frequency index was retired with its table. The conservative optimization section records current measurements and the index checkpoint commit `720851e1`. Other trials remain unimplemented.

Six groups of indexes have identical key layouts in the replayed schema. These are removal candidates, not performance results:

| Candidate to omit | Existing covering key to retain | Required check |
| --- | --- | --- |
| `idx_frequency_slug` | `frequency.slug` UNIQUE autoindex | D03 may remove the whole table instead. |
| `karma_frequency_activation_frequency` | UNIQUE `(frequency_uid, activating_handle_revision)` | Check plans and any query naming the index. |
| `karma_schedule_occurrence_activation` | UNIQUE `(activation_hash, sequence)` | Preserve uniqueness and occurrence paging. |
| `karma_program_state_event_history` | UNIQUE `(program_uid, node_id, state_revision)` | Preserve node history order. |
| `karma_intent_event_history` | UNIQUE `(intent_hash, state_revision)` | Retain the grant/intent family and its event uniqueness under D04. |
| `social_gossip_item_document` | `social_gossip_item_identity`, UNIQUE `(kind, document_hash)` | Retain the unique identity check and verify forwarding queries. |

Equality here means the inspected index key columns, order, collation and predicates matched. The unique indexes enforce additional behavior and must be retained. Check explicit `INDEXED BY` consumers before removing or renaming any index: social search and cleanup already use named indexes. An index sharing a prefix with another is not automatically redundant.

A refreshed scan of Rust and migration SQL found these six explicit names only in their creation migrations, with no Rust `INDEXED BY` references. Recheck all current consumers at implementation. This removes six redundant B-trees, or five if D03 removes the legacy `frequency` table altogether. No write-time or whole-file saving has been measured for those removals.

Avoid speculative indexes on every FK or every possible Protein filter. First measure the actual query, returned rows, intermediate work, sorting, write cost and storage. Preserve uniqueness indexes that enforce invariants even when the associated read query is cheap.

Two further index trials should target current worker SQL. For `mailbox_outbox`, compare the existing pending index with `(next_attempt, created_at, uid)` because the due-copy query orders by those columns; expiry and receipt count remain filters. For Karma, compare the full lifecycle deadline index with a partial deadline index for `lifecycle = 'armed'`, keeping separate leased/expiry and history queries supported. A partial index is useful only when active work is a suitably small subset and its predicate is implied by the actual query. The current schema already uses partial indexes in several places; this proposal extends them only where measurements justify it. [SQLite partial-index documentation](https://www.sqlite.org/partialindex.html).

### Workloads to baseline and rerun

| ID | Representative workload and source | Evidence to capture | Relevant feature check |
| --- | --- | --- | --- |
| Q01 | Record listing, search, active assertions and concept descendants: `store::records`, `store::concepts`, `store::ledger`, Protein | Plans, rows visited, query count per refresh, result payload size; small and large assertion fanout. | Same Records, no deleted/private leakage, stable ordering. |
| Q02 | Fact history, quantity fold and work entries: `store::facts`, `store::ledger`, `store::entries` | History paging, exact fold time, last-hash lookup, write transaction latency and trigger cost. | Exact totals, truthful ordering, no duplicate application. |
| Q03 | Person authentication and authority snapshots: `store::auth`, `store::person_roles`, `store::session_access`, `engine::access` | Cold/hot snapshot read, many memberships/policies, split membership representation, bounded payload validation and revocation latency. | Same current Role unions and primary/display semantics; stale sessions fail. |
| Q04 | Sync feed, per-contact queue and replay: `store::sync_ops`, `engine::peer_sync`, `engine::rebuild` | Large histories, selective Organ/root feeds, queue coalescing, restart/replay cost and writer contention. | No cross-Organ leakage, lost edits or incorrect catchup completion. |
| Q05 | Karma due cursor, expired lease, occurrence expansion and history: `store::karma`, `store::karma_schedules` | Many paused rules versus due rules, timer lateness, bounded batches, lease recovery and cache invalidation. | Intended-time semantics, no repeated consequences, no stale lease writes. |
| Q06 | Transfer detail, agreement, reservation, stock and settlement: `store::transfers`, accounting/balance/stock repositories | Party/child fanout, active versus old revisions, many partial settlements, transaction latency and competing writers. | Same permissions, exact balances, no overcommit or rewritten agreement. |
| Q07 | Public discovery: `store::social::search` / `search_public` | 1k, 10k and 100k documents; dense/sparse terms; filters; late/empty pages; conflict/removed/expired ratios. Record both probe and fallback queries. | Complete pagination, withdrawal/consent/host filtering, bounded response. |
| Q08 | Social expiry, authority revocation and FTS maintenance: `store::social` | Cleanup batch duration, FTS entry lookup, JSON scans, remaining payload bytes and unrelated-document impact. | No resurrection, no retained revoked snippets or orphan search entries. |
| Q09 | Mailbox and social-private delivery workers | Due-job claim time, retry batches, receipts, large cipher payloads, lease/restart behavior and quota checks. | Stored versus delivered status, refusal, replay rejection and atomic session advancement. |
| Q10 | Transfer capture and projection | Per-write trigger count/cost, journal bytes, repeated projection invalidations, cache reuse, snapshot/state-hash time. | No private capture; unchanged inputs retain usable projection; repair remains valid. |
| Q11 | Record-move preview and accept: `store::record_move::offers`, `engine::record_moves` | Dependency scans, bundle bytes, stale-preview checks and followup queue cost. | Permission recheck, complete dependencies, honest accepted/received/complete state. |

Use representative valid fixtures through feature APIs, with fixed seeds and disclosed distributions. Record SQLite/SQLx version, hardware, journal/synchronous settings, page/cache settings, fixture size, warm/cold state and concurrent worker load. The application currently uses WAL, `synchronous=FULL`, foreign keys and a four-connection file pool; memory tests use one connection and do not substitute for file contention measurements.

Capture `EXPLAIN QUERY PLAN`, repeated latency distributions including p50/p95, write throughput, database/WAL size and relevant scheduler deadlines. Separate database time from network/rendering time. Run the same correctness assertions and workloads before and after each proposed optimization. Agree on workload-specific budgets from the baseline; do not invent a universal latency threshold in this research document.

Keep only indexes whose measured workload benefit or integrity role justifies their maintenance cost. Prefer narrow keys and expression/partial indexes over indexing large source payloads or creating a second authority for derived fields. No new directory, ranking, query abstraction or network feature is part of this reset.

Human review:

## Feature contracts across database, backend and frontend

Each feature ID below is also used by the table inventory. These descriptions explain existing consumers and the regression checks for schema changes; they are not a feature-building backlog. Keeping a table means keeping its purpose and safety boundary, not every historical column. The disconnected `crates/web` is excluded from implementation. Native desktop/interface and the active browser Facade must continue to work through their existing interfaces.

### F01 — Records, Facts, work metadata and extensions

People create and edit Records, express Needs/Contributions with exact quantities, capture and correct work entries, classify history, and inspect what changed. Store owns content and history; engine appends Facts and folds quantities atomically. Protein exposes the data to Record, Todo, Kanban, Time, Calendar and History views. Work metadata and several feature settings live in extension payloads rather than dedicated tables.

Preserve Record identity, active assertion rules, entry compensation, single-writer quantity folds, revisions and all extension namespaces used by live consumers. Do not assume Facts alone rebuild every authored title/body/property: CRDT documents and sync operations supply separate history. Tests: `store/tests/exact_ledger.rs`, `record_transactions.rs`, `assertion_transactions.rs`, `engine/tests/record_changes.rs`, and native Record/work presentation tests.

Human review:

### F02 — Vocabulary, concepts, identity assertions and location

People give assertions meaning through Vocabulary adoption, aliases, hierarchies, equivalences and conversions. Store/engine resolve the stable concept identity; Ontology and assertion editors show names and relationships. Location fields serve Record and Transfer queries, with geographic coordinates remaining approximate.

Preserve active unary/binary assertion uniqueness and one active identity assertion, including retractions. Preserve current concept scope, naming uniqueness and provenance. D02 improves lookup keys without changing those meanings. Do not change concept identity merely because its name is edited. Test hierarchy/cycle/ambiguity behavior, exact conversions, concept hydration and permission-filtered relations.

Human review:

### F03 — People, Roles, credentials and current authority

Access Control manages accounts, Roles and policies. Authentication uses credentials plus retained generations; engine evaluates permissions and Record policy on the acting person. Person standing is in `lince.person`, separate from credential and role storage. Organ login/device admission has its own versioned checks.

The refreshed code already unions multiple Role memberships; D05 reviews the split physical representation. Preserve policy revision compare-and-set, role-permission revisions, device/login generation changes, bounded malformed-data refusal, credential confidentiality and bootstrap behavior. Existing UI saved/refused/conflicting states must remain accurate. Tests: Store person/access/policy/session suites, engine access/private-auth suites and native `access_control` tests; Facade auth/security tests remain relevant.

Human review:

### F04 — Cells, Organs, contacts, keys and discovery

Cells/Organs are Records with identity/configuration extensions. Contact rows store trust, sync scope, reach and watermarks; signed rosters and succession/revocation rows carry network authority. Pairing, introductions, login discovery and execution capability must not blur different Organs together.

Retain closed-by-default sharing, blocking, scope versioning, roster freshness and key floors. Shared identity metadata and local operational keys have different replication rules. UI coverage: Organ, Configuration, Access Control and Sync. Tests: scope/private-contact/session access suites, engine contact/pairing/enrolment/roster/device tests and Cell LAN pairing tests.

Human review:

### F05 — Record synchronization, collaboration and review

Operations, CRDT snapshots, property clocks, request receipts, delivery versions and edit drafts support concurrent editing and restart recovery. A Record revision and a qualified document generation/base revision solve different stale-state problems. `record_change` is a bounded recent-change log, not the durable Fact ledger or the complete sync history.

Preserve the separate responsibilities and recheck authority on incoming changes. Explicitly define which current data can be rebuilt before pruning its source history. Verify title/body merge, independent property changes, slug conflict, deletion/resurrection rules, stale document refusal, draft recovery and data visibility in Sync/history/native editors. Tests: Store document/revision/receipt suites; engine collab, record changes, sync/rebuild suites; native edit/history tests.

Human review:

### F06 — Sharing, replicas, Record moves, offers and refusals

Reference reads, replica grants and contact selections control who receives content. The new Record-move workflow has a previewed dependency bundle, negotiation states and acknowledgment boundaries. `record_move_offer` and members now carry the active move path; older `record_move` storage needs a refreshed consumer audit. Refusal and local-outcome rows serve multiple offer kinds without replacing their feature-specific state.

Keep preview fingerprints, current permission checks, required dependency inclusion, canceled/declined outcomes and independent user refusal memory. Do not equate a transmitted bundle with completed handover. Native Sync should show incoming/outgoing offers and changed-preview recovery. Tests: engine `record_moves.rs`, contact/share/replica tests, Store refusal tests and native Sync offers tests after this in-progress work settles.

Human review:

### F07 — Threads, groups, messages, calls and Fiote conversation content

Conversation, thread, message and call-session objects are Records linked by assertions and extensions. `communication.v1`, `communication.session.v1`, `lince.message-content`, message progress and Fiote thread/task metadata are existing consumers of `record_extension`. Group revisions/delivery and thread invitations add protocol-specific state.

Do not create parallel message/thread tables merely because the names are absent from SQL. Preserve invitation consent, group ownership/revisions, message attachments, content hashes, progress and questions. Native Thread/communication/recording controls and Fiote must hydrate the same content and respect the same actor authority. Local recordings/files live outside SQL. Test conversation membership, group updates, message editing/retry, attachments and call-session history.

Human review:

### F08 — Rules, Frequencies, calendars and file-backed habits

People author conditions, thresholds and ordered consequences, bind references, choose a Frequency, preview outcomes and pause/resume execution. Current recurrence/field/binding storage and durable frequency activations are both live. Rule applications, evidence and progress make runtime history and retries understandable.

Implement D03 without changing intended-time versus observed-time semantics. Preserve DST/timezone behavior, immutable frequency activation history, pause/retire/supersede states, timer admission, fencing and exactly-once internal application where receipts provide it. Habit import must preserve conflicts and retry ownership. Native Rule, Frequency, Karma, Instinct and Calendar views must retain their current behavior. Tests: Store frequency/recurrence/Karma suites, engine schedule recovery/DST/habit/sync suites and simulation Karma suites.

Human review:

### F09 — Durable Karma programs, runs, state and candidate review

Program definitions/revisions, occurrence ingress, captured program epochs, run receipts and node-state events record reproducible evaluation. Candidate proposals and review history store inert suggestions and the person's response. They coexist with the simpler recurrence rule interface and cannot be removed merely because both use the word Karma.

Preserve deterministic occurrence order, captured revision/epoch, replay fingerprinting, fuel/resource limits, state resets and candidate accept/reject/snooze history. D04 retains grants and intent definitions, including their constraints and status seeds. Refresh their consumers without changing feature scope. Required regression checks: Store program/run/candidate/state tests where present, engine Karma history/actions/runtime tests and deterministic simulation replay. Missing coverage is recorded as a coverage gap.

Human review:

### F10 — Scheduled changes, commands, external effects and Fiote activation

Scheduled boundary revisions, command invocations, effect attempts/outcomes, Signal samples and Fiote activation rows let the Cell resume unfinished work and explain results. Commands can run outside SQL and have different retry guarantees from internal quantity changes.

Keep claim/lease and request identities, captured configuration/version, numeric output, error detail and actor checks. Do not promise exactly-once execution of an arbitrary external command after a crash; preserve the feature's observable attempt/outcome semantics. Fiote configuration is partly in Record extensions and local connection/vault files. Native Karma/command/Fiote views should show queued/running/failed/held states and stopping accurately. Test disable/restart/repeat activation, unavailable provider, command partial failure and recovery without a second internal Fact.

Human review:

### F11 — Transfer proposals, parties, terms and agreement

Transfer Castle lets people propose exchanges/donations, invite participants, revise terms, agree to levels and activate agreed work. Typed sidecars hold parties, promises, routes, revisions and events; frozen coalition membership and scope constraints prevent later changes rewriting an accepted agreement.

Preserve open proposal ownership/reuse, invitation retry identity, agreement target idempotency, expected revisions, coalition thresholds, activation and child/dependency requirements. Keep current quantity representations and accepted results. Existing UI accepted revisions, refusal/conflict and dependency states must remain accurate. Tests: Transfer engine/Store suites, agreement/negotiation/counterparty tests, native Transfer Castle tests and simulation trade/donation/dependency scenarios.

Human review:

### F12 — Transfer fulfillment, settlement, dispute and correction

Occurrences represent intended fulfillment. Claims and disputes supply evidence; slices apply accepted amounts and formulas to local Records. Compensation, successor promises and correction lineage preserve what originally happened. Cancellations and bulk requests have their own review/retry boundaries.

Keep immutability, actor/scope checks, atomic Fact application, cumulative bounds, consented cancellation and inverse compensation. Keep current amount, cumulative and remainder calculations. Existing promised/claimed/settled/disputed/corrected presentation and partial completion must remain accurate. Tests: settlement/correction/bulk/cancellation suites, current fractional cases, concurrent requests and native detail/history presentation.

Human review:

### F13 — Transfer delivery, remote references and private application

Delivery policy, envelopes, pull work, receipts and remote commands transport selected Transfer state. Remote references are deliberately not the same authority as a local Transfer. Handoffs/attestations connect origin evidence with a participant's separately authorized local effect. The view `transfer_application_effect_handoff` presents local and remote handoffs consistently.

Preserve disclosed versus private state, canonical hashes, request nonces, monotonic cursors/revisions, retry/conflict evidence and local authorization. An attestation must not bypass a person's private formula or Record permissions. Keep offline resume and policy revocation behavior. Protein and Transfer Castle should show queued/sent/acknowledged/applied separately. Tests: engine Transfer delivery/replication/private-application/outcome suites, simulation restart/replicas/volume scenarios and native delivery views.

Human review:

### F14 — Private policies, stock limits, loans and Transfer capture

Private effect policies map shared commitments to personal Records; stock restrictions guard resource use; loans have agreement and extension history. Capture journals and signed Transfer messages preserve writer ownership across Cells and use schema-derived row formats.

Do not expose local policy/signature/key or private application data through capture rules. Preserve stock roster history for authorization, minimum-stock rechecks under concurrent writers, private effect groups and loan interval/extension consistency. Validate the current capture policy against retained schema definitions under D07. Tests: simulation balances, stock/loans/effect-group scenarios and engine private policy/capture/security suites. UI coverage remains Transfer detail/policies and Sync/device status.

Human review:

### F15 — Public Needs/Contributions, profiles, assets and search

Public posts/profile documents are signed public projections with revision/authority floors, not an automatic view of all local Records. Publication jobs carry consented destinations and receipts. FTS is maintained from accepted searchable documents; expiry, conflict, local listing removal, fulfillment/withdrawal and authority revocation affect eligibility.

Preserve anonymous versus identified publication, edits and key rotation, explicit destinations/redistribution, complete filtered pagination and withdrawal propagation. Retain ended-post floors and local moderator removal independently from current document bodies. Migration `0332` indexes FTS IDs for post lookup; consolidation must retain that final virtual-table definition. Measure Q07/Q08 before additional indexes. Test search pages, conflict/ending/revocation/expiry cleanup, host privacy, assets and publication receipts through native Organ/social controls and the active Facade if it exposes the path.

Human review:

### F16 — Private social delivery and service mailboxes

Local message work produces sealed envelopes and recipient destinations. Device account/session state, session peers, mailbox pins and pickup sequence are security state; replay identities and completion receipts prevent repeated message application. Service routes, envelopes, sender admission and budgets implement a separate ciphertext-carrier role.

Keep recipient authority generation, attachment metadata, authenticated refusal, independent destination states, same-ID/different-content rejection and atomic ratchet/session advancement with durable outbox/inbox writes. Rebuilding public search must not touch private transport state. Owner backup intentionally excludes live sessions and holds associated pending mail; preserve that contract. Native Thread and Organ/social controls must distinguish queued/stored/delivered/refused/held and offer permitted recovery. Test late delivery, transport/preparation failures, queue restart, profile ending, attachments and key/session replacement.

Human review:

### F17 — Consented gossip, Ask, saved subscriptions, reports and retention

These are separate public-discovery workflows over existing social documents. Gossip tracks each consented forwarding job; Ask tracks requests and reply reservation; subscriptions track leased polling and seen/notification state; reports track bounded submission, admission and operator inspection. Context retention drains or retires local private contexts without deleting security memory prematurely.

Preserve consent, source provenance, conflict evidence, cancellation, leases, deadlines, notification deduplication, moderation boundaries and bounded per-peer resource use. Expiry is not permission to lose refusal/revocation floors. Native Organ/social panels already contain discovery sources, subscription, report/moderation and operator controls; keep their states accurate. Tests: social discovery/gossip/Ask/subscription/report/retention/security suites and native social journey tests. Future directory partitioning or ranking is outside this reset.

Human review:

### F18 — Blind mailbox transport, reach and contact delivery

The generic blind mailbox has registrations, invites/requests, quota, ciphertext bundles, device acknowledgments, completion/expiry notices and local inbox/outbox work. `mail_left`, contact-rate and peer-delivery state explain reach and fallback. This is distinct from the social reply-service carrier.

Keep quota enforcement, signed roster/device freshness, expiry versus completion, requested copy policy, current outbound authority hash and delivery retry. Recent `mailbox_outbox_authority` work must settle before freeze. Native Sync and Organ/contact status should explain carrier storage without reporting recipient application. Test mailbox outbox guards, multiple devices/carriers, revocation after queueing, repeat pickup and contact delivery/reach restart.

Human review:

### F19 — Blob copying, storage, configuration, backup, updates and simulation

Blob metadata tracks offers, manifests, destinations and progress; the bytes live in files. Configuration controls storage and interface snapshots. Update state and simulation check sets have their own durable uses. Projection caches support bounded future/past time views without becoming authored commitments.

Preserve blob accept/cancel/resume and hash/path validation, configured budgets, schema-only durable opens, repeated seeding, owner-backup sanitation, restart and update state. Simulation snapshots/state hashes enumerate tables and may need intentional baseline changes after removal/refactors. Tests: Store durable/budget/seed suites, Cell backup tests, engine blob/projection tests and simulation restart/replay/checks. Native file/Sync/Configuration/Information/backup/simulation controls must still work.

Human review:

### F20 — Native workspace files, shared workspaces, local tools and packages

Native workspace topology, Sand/Castle composition, placements, Areas, templates and many tool settings persist through workspace documents and snapshots. The refreshed SQL schema also stores hosted shared-workspace layout/policy/revision, reviewed changes, actor/request receipts and saved drafts. `description_assets` stores bounded Record-description image/drawing data and `sand_package` stores received/published package metadata. IDE recovery, other document/media assets, recordings, terminal state and Fiote connections/vault have additional file/runtime owners.

`record_extension` also supplies saved Protein queries/presentation and published custom-component metadata. Inspect current namespace/type definitions rather than guessing JSON fields. Published Sand package formats and execution permissions are still changing; preserve author/origin/license/credits and the distinction between receiving and enabling a package. Imported files should not acquire unintended Record copies or authority.

Accept the reset only after existing native workspace restore, shared-workspace review/retry/draft behavior, description assets and package consumers work against the fresh schema. Refresh any further storage changes before cutover. The index candidates in D02 optimize current workspace queries without changing their collaboration model. Do not add speculative presentation tables. Current `crates/web` remains untouched.

Human review:

## Complete current table inventory

The inventory records each physical non-system table present after SQL replay. `PK` means the declared primary key; an arrow lists declared FK target tables, with details available in the linked owner and migrations. Tables without a declared FK can still have logical/local or external relationships described in their purpose. The Source link is an investigation entry point, not a claim that all access is centralized there.

Treatment words: **Keep** preserves a live purpose with the decisions above; **Refactor** recommends a coordinated change; **Review/remove** is conditional on the refreshed consumer/feature audit; **SQLite-owned** is maintained by FTS5. No inventory row authorizes deletion by itself.

### F01 — Records, ledger and work history

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `record` | Keep: shared identity/content and existing quantity cache; retain Organ/replica scope, logical deletion and revisions. Review active-list indexing under D02 and constraints under D06. | PK `uid`; → `concept`, `place` | [store/src/records](../../crates/store/src/records.rs) |
| `fact` | Keep: signed, chained change evidence; atomic quantity folding and committed-batch association. Retain history order and origins. | PK `uid`; → `record` | [store/src/facts](../../crates/store/src/facts.rs) |
| `commit_sequence` | Keep: singleton transaction sequence linking captured Transfer changes and Facts; advance through the Store writer. | PK `id`; no declared FK | [store/src/lib](../../crates/store/src/lib.rs) |
| `record_extension` | Keep: versioned per-Record JSON namespaces for work, configuration, queries, messages and packages; validate owners and privacy. | UNIQUE `record_uid, namespace`; → `record` | [store/src/records](../../crates/store/src/records.rs) |
| `record_revision` | Keep: current Record revision and retained document generation; stale-operation/CRDT protection, not a history table. | PK `record_uid`; → `record` | [store/src/record_docs](../../crates/store/src/record_docs.rs) |
| `entry` | Keep: current exact work/ledger entry, occurrence time and application Fact; correction uses compensating evidence. | PK `uid`; → `fact`, `record` | [store/src/entries](../../crates/store/src/entries.rs) |
| `entry_revision` | Keep: entry mutation history and unique retry request linking original/compensated Facts. | PK `uid`; → `entry`, `fact` | [store/src/entries](../../crates/store/src/entries.rs) |
| `fact_concept` | Keep: current Fact classification pointing to its classification event and optional concept. | PK `fact_uid`; → `concept`, `fact`, `fact_concept_event` | [store/src/ledger](../../crates/store/src/ledger.rs) |
| `fact_concept_event` | Keep: classification changes with actor/note; do not silently rewrite previous classification evidence. | PK `uid`; → `concept`, `fact` | [store/src/ledger](../../crates/store/src/ledger.rs) |
| `fact_origin` | Keep: original Organ/Cell and payload for imported Facts; supports signature/provenance and semantic checks. | PK `fact_uid`; → `fact` | [store/src/facts](../../crates/store/src/facts.rs) |
| `retention_policy` | Keep: configurable Fact-kind horizon; audit pruning against signatures, replay and downstream evidence dependencies. | PK `kind`; no declared FK | [store/src/facts](../../crates/store/src/facts.rs) |
| `operation_receipt` | Keep: accepted actor/Organ operation fingerprint and result; identical retries replay, changed payloads fail. | PK `organ_uid, person_uid, operation_uid`; → `record` | [store/src/operation_receipts](../../crates/store/src/operation_receipts.rs) |
| `operation_receipt_record` | Keep: Records touched by an accepted operation, scoped to its composite receipt identity. | PK `organ_uid, person_uid, operation_uid, record_uid`; → `operation_receipt`, `record` | [store/src/operation_receipts](../../crates/store/src/operation_receipts.rs) |

Human review:

### F02 — Vocabulary, assertions and places

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `concept` | Keep: stable meaning, origin and current canonical-name uniqueness. Preserve existing scope and naming behavior. | PK `uid`; no declared FK | [store/src/concepts](../../crates/store/src/concepts.rs) |
| `concept_name` | Keep: language/name aliases for a concept; preserve ambiguity resolution and provenance. | UNIQUE `concept_uid, lang, name`; → `concept` | [store/src/concepts](../../crates/store/src/concepts.rs) |
| `concept_parent` | Keep: concept hierarchy edges; validate cycles and permission-aware descendant queries. | UNIQUE `concept_uid, parent_uid`; → `concept` | [store/src/concepts](../../crates/store/src/concepts.rs) |
| `concept_equivalence` | Keep: declared meaning equivalence, with declaring identity; do not infer equivalence from matching names. | UNIQUE `a_uid, b_uid`; → `concept` | [store/src/concepts](../../crates/store/src/concepts.rs) |
| `concept_conversion` | Keep: rational unit conversion numerator/denominator; preserve exact direction and invalid-conversion refusal. | UNIQUE `a_uid, b_uid`; → `concept` | [store/src/concepts](../../crates/store/src/concepts.rs) |
| `lingua` | Keep: Vocabulary identity, owner and visibility; distinguish it from the Lingua file syntax. | PK `uid`; no declared FK | [store/src/linguas](../../crates/store/src/linguas.rs) |
| `lingua_concept` | Keep: adopted concept membership and adoption time; synchronization uses stable concept identities. | PK `lingua_uid, concept_uid`; → `concept`, `lingua` | [store/src/linguas](../../crates/store/src/linguas.rs) |
| `record_assertion` | Keep: unary/binary/identity classification with optional exact amounts and retraction; uniqueness predicates are behavior. | PK `uid`; → `concept`, `record` | [store/src/assertions](../../crates/store/src/assertions.rs) |
| `place` | Keep: geographic/address/area metadata referenced by Records; preserve current REAL coordinates. | PK `uid`; no declared FK | [store/src/places](../../crates/store/src/places.rs) |

Human review:

### F03 — Accounts, authority and admission

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `role` | Keep: named Role identity and existing scope/membership semantics. Role names are not hardcoded privileges. | PK `id`; no declared FK | [store/src/roles](../../crates/store/src/roles.rs) |
| `permission` | Keep: unique subject/action capability catalogue; seeding must not undo revoked grants. | PK `id`; no declared FK | [store/src/auth](../../crates/store/src/auth.rs) |
| `role_permission` | Keep: Role-to-capability membership; permissions combine through settled authority semantics. | PK `role_id, permission_id`; → `permission`, `role` | [store/src/auth](../../crates/store/src/auth.rs) |
| `role_permission_revision` | Keep: Role capability generation used to reject stale authority; preserve overflow/malformed-state refusal. | PK `role_id`; no declared FK | [store/src/roles](../../crates/store/src/roles.rs) |
| `role_policy` | Keep: bounded versioned Record policy JSON, updated with compare-and-set and permission checks. | PK `role_id`; → `role` | [store/src/role_policies](../../crates/store/src/role_policies.rs) |
| `person_access` | Keep/review: access policy/revision plus first/singular role_id; current additive membership also uses person_role. Audit whether the split representation can be normalized without changing primary-Role semantics. D05. | PK `person_uid`; → `record`, `role` | [store/src/auth](../../crates/store/src/auth.rs) |
| `person_role` | Keep: additional Person-to-Role memberships; insert/delete triggers advance access revision. D05 reviews the split source and D08 the physical layout. | PK `person_uid, role_id`; → `person_access`, `role` | [store/src/person_roles](../../crates/store/src/person_roles.rs) |
| `authority_seed_state` | Keep: singleton marker distinguishing initial authority seeding from later restarts; prevents revoked grants being restored. | PK `id`; no declared FK | [store/src/seed](../../crates/store/src/seed.rs) |
| `person_credential` | Keep: local Person username/password hash; do not replicate credential material into public/contact payloads. | PK `person_uid`; → `record` | [store/src/auth](../../crates/store/src/auth.rs) |
| `person_auth_generation` | Keep: retained credential/authentication generation; changing credentials invalidates previously captured authority. | PK `person_uid`; no declared FK | [store/src/auth](../../crates/store/src/auth.rs) |
| `person_device` | Keep: Person/device admission with revocation and revision; device IDs can be external, not local Records. | PK `person_uid, node_id`; no declared FK | [store/src/session_access](../../crates/store/src/session_access.rs) |
| `organ_login` | Keep: local Organ-to-Person login admission; distinct from workspace editing and Record authority. | PK `organ_uid`; → `record` | [store/src/private_contacts](../../crates/store/src/private_contacts.rs) |
| `organ_login_generation` | Keep: retained admission generation even after login changes; stale session refusal depends on it. | PK `organ_uid`; no declared FK | [store/src/private_contacts](../../crates/store/src/private_contacts.rs) |

Human review:

### F04 — Network identities, contacts and rosters

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `identity_key` | Keep: actor/key-id public-key identity; do not merge operational private keys into this catalogue. | UNIQUE `actor_uid, key_id`; no declared FK | [store/src/transfer_replication](../../crates/store/src/transfer_replication.rs) |
| `identity_succession` | Keep: signed key succession chain for an Organ; retain authority continuity and bounded traversal. | PK `organ_uid, old_key, new_key`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `identity_revocation` | Keep: signed revoked-key memory; payload expiry cannot resurrect a revoked editor. | PK `organ_uid, revoked_key`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `organ_contact` | Keep: trust, scope versions, sharing, reach and sync cursors; preserve current local transport/shared policy boundaries. | PK `record_uid`; → `record` | [store/src/organs](../../crates/store/src/organs.rs) |
| `organ_access_catalog` | Keep: retained observed Organ generation/granted state; reads require known contact trust and updates refuse older generations. | PK `organ_uid`; → `record` | [store/src/organ_access](../../crates/store/src/organ_access.rs) |
| `organ_roster` | Keep: signed Organ membership/capability snapshot and freshness; execution authority depends on it. | PK `organ_uid`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `organ_public_record` | Keep: cached public Organ packet for discovery; preserve consent and freshness, not private configuration. | PK `organ_uid`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `local_capability` | Keep: device-local capabilities used to construct permitted roster/execution behavior. | PK `capability`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `enrolment_token` | Keep: hashed one-use enrollment token with expiry/use time; do not persist a public usable token. | PK `token_hash`; no declared FK | [store/src/roster](../../crates/store/src/roster.rs) |
| `door_request` | Keep: received unknown-peer introduction request; node deduplication and admission are separate from contact trust. | PK `uid`; no declared FK | [store/src/door](../../crates/store/src/door.rs) |

Human review:

### F05 — Sync, collaboration and retained edits

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `sync_op` | Keep: ordered/HLC operation history scoped by Organ/root; unique actor clock and replay semantics matter. | PK `seq`; no declared FK | [store/src/sync_ops](../../crates/store/src/sync_ops.rs) |
| `sync_outbox` | Keep: coalesced per-contact operation delivery work; queued sequence is not a recipient-applied receipt. | PK `contact_organ, tbl, uid, field, kind`; no declared FK | [store/src/sync_ops](../../crates/store/src/sync_ops.rs) |
| `sync_quarantine` | Keep: refused/untrusted operation payload and reason under quarantine budgets; no automatic replay bypass. | PK `uid`; no declared FK | [store/src/organs](../../crates/store/src/organs.rs) |
| `sync_activity` | Keep: bounded diagnostic sync history; disposable according to explicit UI/history policy. | PK `seq`; no declared FK | [store/src/sync_activity](../../crates/store/src/sync_activity.rs) |
| `sync_history_policy` | Keep: singleton age/count bounds for sync activity; do not apply to authoritative sync evidence indiscriminately. | PK `id`; no declared FK | [store/src/sync_activity](../../crates/store/src/sync_activity.rs) |
| `record_doc` | Keep: CRDT snapshot and through-sequence qualified by generation/base revision; preserve stale/unqualified refusal. | PK `record_uid`; → `record` | [store/src/record_docs](../../crates/store/src/record_docs.rs) |
| `record_doc_delivery` | Keep: document version sent per contact; local delivery knowledge, not authored content. | PK `record_uid, contact_organ`; no declared FK | [engine/src/sync](../../crates/engine/src/sync.rs) |
| `record_property` | Keep: per-property winning clock/peer/change/value, used for merge, conflict handling and rebuild. | PK `record_uid, property`; → `record` | [store/src/facts](../../crates/store/src/facts.rs) |
| `record_change` | Keep: recent local/remote change and displaced-value log; bounded independently from Fact history. | PK `seq`; no declared FK | [store/src/record_changes](../../crates/store/src/record_changes.rs) |
| `record_change_receipt` | Keep: actor/change replay payload and result, including rejected/stale mutation identity. | PK `actor, change_uid`; no declared FK | [engine/src/record_change](../../crates/engine/src/record_change.rs) |
| `record_edit_draft` | Keep: editor-source/Record recovery draft; local unresolved editing state, not a published document revision. | PK `source, record_uid`; no declared FK | [engine/src/record_change](../../crates/engine/src/record_change.rs) |

Human review:

### F06 — Sharing, moves and offer outcomes

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `contact_share` | Keep: per-contact selected/held Record set; scope/Protein selections still require current permission checks. | PK `contact_organ, record_uid`; no declared FK | [store/src/contact_share](../../crates/store/src/contact_share.rs) |
| `replica_grant` | Keep: offered/accepted replica scope and delivery identity; not a generic authorization grant. | PK `root_record, contact_organ`; no declared FK | [store/src/replica](../../crates/store/src/replica.rs) |
| `reference_read` | Keep: per-reader Record reference freshness; logical remote identity and retained reads need deliberate expiry. | PK `reader_organ, record_uid, root_record`; no declared FK | [store/src/replica](../../crates/store/src/replica.rs) |
| `record_move_offer` | Keep: incoming/outgoing previewed move, payload, state/error and timestamps; confirm completion acknowledgment after current work settles. | PK `uid`; no declared FK | [store/src/record_move/offers](../../crates/store/src/record_move/offers.rs) |
| `record_move_member` | Keep: Record membership in a move bundle; Record IDs can identify not-yet-imported/deleted content. | PK `offer_uid, record_uid`; → `record_move_offer` | [store/src/record_move](../../crates/store/src/record_move.rs) |
| `offer_refusal` | Keep: remembered refusal by kind/subject/other party with optional expiry; prevents repeated prompting. | PK `kind, subject_uid, other_party`; no declared FK | [store/src/offers](../../crates/store/src/offers.rs) |
| `offer_local_outcome` | Keep: local outcome for heterogeneous offers; complements feature state and refusal rather than replacing receipts. | PK `kind, subject_uid, other_party`; no declared FK | [store/src/offers](../../crates/store/src/offers.rs) |

Human review:

### F07 — Conversations and invitations

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `conversation_group` | Keep: conversation group owner, accepted signed payload and revision; root is a protocol identity. | PK `root`; no declared FK | [engine/src/groups](../../crates/engine/src/groups.rs) |
| `conversation_delivery` | Keep: group revision delivered per Organ; preserve re-send after edits and recipient consent. | PK `root, organ`; → `conversation_group` | [engine/src/wire/groups](../../crates/engine/src/wire/groups.rs) |
| `thread_invite` | Keep: invitation Record connecting sender Organ/root with consented thread joining. | PK `record_uid`; → `record` | [store/src/invites](../../crates/store/src/invites.rs) |
| `decision` | Keep: attention/choice sidecar with options, expiry and answer; do not make accepting a choice bypass feature authority. | PK `record_uid`; → `record` | [store/src/misc](../../crates/store/src/misc.rs) |

Human review:

### F08 — Recurrence, Frequencies and habits

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `recurrence` | Keep: live rule target, condition/bindings, consequences, cadence and revision; stale standalone frequency FK removed. | PK `uid`; → `record` | [store/src/recurrence](../../crates/store/src/recurrence.rs) |
| `recurrence_revision` | Keep: rule mutation/retry history with captured definition; stale standalone frequency FK removed. | PK `uid`; → `recurrence` | [store/src/recurrence](../../crates/store/src/recurrence.rs) |
| `recurrence_skip` | Keep: explicit skipped intended occurrence, distinct from a paused rule or failed attempt. | PK `recurrence_uid, due_at`; → `recurrence` | [store/src/recurrence](../../crates/store/src/recurrence.rs) |
| `karma_field` | Keep: reusable condition/consequence field definition with kind/source/revision, owned by the current editor. | PK `uid`; no declared FK | [store/src/karma_fields](../../crates/store/src/karma_fields.rs) |
| `karma_field_binding` | Keep: rule-to-field binding; keep references consistent through field edits/moves. | PK `rule_uid, kind`; → `karma_field`, `recurrence` | [store/src/karma_fields](../../crates/store/src/karma_fields.rs) |
| `karma_editor_request` | Keep: editor operation result identity for retried field/rule creation. | PK `request_id`; no declared FK | [store/src/karma_fields](../../crates/store/src/karma_fields.rs) |
| `karma_rule_frequency` | Keep: current rule-to-durable-frequency binding with captured rule revision. | PK `recurrence_uid`; → `karma_frequency`, `recurrence` | [store/src/record_move/offers](../../crates/store/src/record_move/offers.rs) |
| `karma_signal_frequency` | Keep: Signal-to-current-frequency binding, independent from deprecated standalone frequency storage. | PK `signal_uid`; → `karma_frequency`, `signal` | [store/src/frequency](../../crates/store/src/frequency.rs) |
| `karma_rule_application` | Keep: intended occurrence/rule/revision application result and attempt, supporting repeat-proof rule execution/history. | PK `event_id, rule_uid, rule_revision, attempt`; no declared FK | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `karma_rule_evidence` | Keep: captured rule attempt evidence; retry and inspection must agree with application identity. | PK `event_id, rule_uid, rule_revision, attempt`; no declared FK | [engine/src/karma_history](../../crates/engine/src/karma_history.rs) |
| `karma_rule_progress` | Keep: singleton progress through ordered occurrence ingestion; preserve restart cursor semantics. | PK `singleton`; no declared FK | [engine/src/rule_runtime](../../crates/engine/src/rule_runtime.rs) |
| `karma_frequency_usage` | Keep: shared-frequency enabled/auto-paused usage; do not conflate with definition/activation state. | PK `frequency_uid`; → `karma_frequency` | [engine/src/rule_runtime](../../crates/engine/src/rule_runtime.rs) |
| `karma_frequency` | Keep: Record-backed frequency handle with active/head revision and activation identity. | PK `record_uid`; → `karma_frequency_activation`, `karma_frequency_revision`, `record` | [store/src/karma/schedules](../../crates/store/src/karma/schedules.rs) |
| `karma_frequency_revision` | Keep: immutable authored/compiled frequency definition and canonical DSL. | PK `revision_hash`; → `karma_frequency` | [store/src/karma/frequencies](../../crates/store/src/karma/frequencies.rs) |
| `karma_frequency_activation` | Keep: captured parameters, compiled calendar/elapsed epoch and predecessor; timer contract depends on it. | PK `activation_hash`; → `karma_frequency`, `karma_frequency_activation`, `karma_frequency_revision` | [store/src/karma/frequencies](../../crates/store/src/karma/frequencies.rs) |
| `karma_frequency_request` | Keep: fingerprinted frequency mutation result and evidence; current request family, not the obsolete revision table. | PK `request_id`; → `fact`, `karma_frequency`, `karma_frequency_activation`, `karma_frequency_revision`, `karma_request` | [store/src/karma/frequencies](../../crates/store/src/karma/frequencies.rs) |
| `karma_habit_import` | Keep: Organ/tutorial import definition and completed status; import is separate from tutorial learning progress. | PK `organ_uid, tutorial`; → `record` | [engine/src/file_sync/lingua/habits](../../crates/engine/src/file_sync/lingua/habits.rs) |
| `karma_habit_object` | Keep: objects created/owned by a habit import; cancellation/conflict cleanup must respect ownership. | PK `organ_uid, tutorial, kind`; → `karma_habit_import` | [engine/src/file_sync/lingua/habits](../../crates/engine/src/file_sync/lingua/habits.rs) |
| `karma_habit_request` | Keep: habit import fingerprint/result per request; repeated import must not duplicate rules/Records. | PK `organ_uid, request_id`; → `karma_habit_import` | [engine/src/file_sync/lingua/habits](../../crates/engine/src/file_sync/lingua/habits.rs) |

Human review:

### F09 — Program evaluation and review

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `karma_program` | Keep: Record-backed program handle and active/head definition revision; do not confuse with recurrence rule storage. | PK `record_uid`; → `karma_program_revision`, `record` | [store/src/karma/programs](../../crates/store/src/karma/programs.rs) |
| `karma_program_revision` | Keep: immutable program AST, canonical DSL and proof result for repeatable evaluation. | PK `revision_hash`; → `karma_program` | [store/src/karma/programs](../../crates/store/src/karma/programs.rs) |
| `karma_program_request` | Keep: fingerprinted program mutation result and original Fact/revision. | PK `request_id`; → `fact`, `karma_program`, `karma_program_revision`, `karma_request` | [store/src/karma/programs](../../crates/store/src/karma/programs.rs) |
| `karma_program_execution` | Keep: local program execution switch/note, distinct from synchronized definition and global authority. | PK `program_uid`; → `karma_program` | [store/src/karma/execution](../../crates/store/src/karma/execution.rs) |
| `karma_occurrence` | Keep: durable ingress identity, source/logical time, sequence and causal parent. | PK `occurrence_hash`; → `karma_occurrence` | [store/src/karma/occurrences](../../crates/store/src/karma/occurrences.rs) |
| `karma_occurrence_sequence` | Keep: singleton allocator for local durable occurrence sequence. | PK `singleton`; no declared FK | [store/src/karma/occurrences](../../crates/store/src/karma/occurrences.rs) |
| `karma_occurrence_processing_state` | Keep: singleton cursor for program processing through ingested occurrences. | PK `singleton`; no declared FK | [store/src/karma/runs](../../crates/store/src/karma/runs.rs) |
| `karma_occurrence_program_epoch` | Keep: captured program membership/revisions for an occurrence and resumable next ordinal. | PK `occurrence_hash`; → `karma_occurrence` | [store/src/karma/runs](../../crates/store/src/karma/runs.rs) |
| `karma_run` | Keep: deterministic occurrence/program run evidence, status, fuel and captured revision/epoch. | PK `run_hash`; → `karma_occurrence`, `karma_occurrence_program_epoch`, `karma_program`, `karma_program_revision` | [store/src/karma/runs](../../crates/store/src/karma/runs.rs) |
| `karma_program_node_state` | Keep: current node state pointing to a definition/activation/event; derived from state history. | PK `program_uid, node_id`; → `karma_program`, `karma_program_revision`, `karma_program_state_event` | [store/src/karma/states](../../crates/store/src/karma/states.rs) |
| `karma_program_state_event` | Keep: node state transitions/reset evidence with predecessor and source run. | PK `event_hash`; → `karma_program`, `karma_program_revision`, `karma_program_state_event`, `karma_run` | [store/src/karma/states](../../crates/store/src/karma/states.rs) |
| `karma_candidate` | Keep: inert proposal emitted by a captured run/output; proposal storage does not itself authorize an effect. | PK `candidate_hash`; → `karma_occurrence`, `karma_program`, `karma_program_revision`, `karma_run` | [store/src/karma/candidates](../../crates/store/src/karma/candidates.rs) |
| `karma_candidate_state` | Keep: current candidate decision/snooze state and event revision. | PK `candidate_hash`; → `karma_candidate` | [store/src/karma/candidates](../../crates/store/src/karma/candidates.rs) |
| `karma_candidate_review_event` | Keep: immutable actor review decision/evidence and Fact with predecessor. | PK `event_hash`; → `fact`, `karma_candidate`, `karma_candidate_review_event`, `karma_request` | [store/src/karma/candidates](../../crates/store/src/karma/candidates.rs) |
| `karma_candidate_review_request` | Keep: candidate-review payload fingerprint/replayed result and expected revision. | PK `request_id`; → `fact`, `karma_candidate`, `karma_candidate_review_event`, `karma_request` | [store/src/karma/candidates](../../crates/store/src/karma/candidates.rs) |
| `karma_request` | Keep: shared request identity/family registry for live program/frequency/candidate repositories. | PK `request_id`; no declared FK | [store/src/karma/candidates](../../crates/store/src/karma/candidates.rs) |
| `karma_grant` | Keep: automation capability-grant handle, current status/revision and authorizing identity. Retained by human decision. D04. | PK `record_uid`; → `karma_grant_revision`, `record` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_grant_revision` | Keep: immutable signed capability-grant definition/revision; retain its scope and limits. D04. | PK `grant_uid, revision_hash`; → `karma_grant` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_grant_request` | Keep: grant mutation fingerprint/result/evidence linked to the request family and applied revision. D04. | PK `request_id`; → `fact`, `karma_grant`, `karma_grant_revision`, `karma_request` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_intent` | Keep: accepted candidate authorized by a captured grant/program revision, including reservation and retry identity. D04. | PK `intent_hash`; → `karma_candidate`, `karma_grant`, `karma_grant_revision`, `karma_program`, `karma_program_revision` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_intent_state` | Keep: current intent status/event pointer and cancellation reason; preserve one-way transition rules. D04. | PK `intent_hash`; → `karma_intent`, `karma_intent_event`, `karma_intent_status` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_intent_event` | Keep: immutable per-intent transition chain and cause request; review only its duplicate history index under D10. | PK `event_hash`; → `karma_intent`, `karma_intent_event`, `karma_intent_status`, `karma_request` | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `karma_intent_status` | Keep: authorized/cancelled status definitions and reservation flags; retain structural seeds. D04. | PK `status`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |

Human review:

### F10 — Timers, scheduled changes and external work

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `karma_schedule` | Keep: editable named scheduled-change set with revision/cancellation. | PK `uid`; no declared FK | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `karma_schedule_revision` | Keep: captured scheduled inputs and revision history; old boundaries remain explainable. | PK `schedule_uid, revision`; → `karma_schedule` | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `karma_schedule_request` | Keep: fingerprint/result for schedule mutations under retry. | PK `request_id`; → `karma_schedule` | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `karma_schedule_boundary` | Keep: current/historical scheduled purpose, intended time, rule/frequency/application outcome. | PK `uid`; → `karma_frequency`, `karma_schedule`, `recurrence` | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `karma_schedule_cursor` | Keep: per-activation timer lifecycle, admitted resolution, intended boundary and fenced lease. Retain retirement history. | PK `activation_hash`; → `karma_frequency`, `karma_frequency_activation` | [store/src/karma/schedules](../../crates/store/src/karma/schedules.rs) |
| `karma_schedule_occurrence` | Keep: durable emitted timer occurrence covering intended boundaries, including coalesced emissions. | PK `occurrence_hash`; → `karma_frequency_activation` | [store/src/karma/schedules](../../crates/store/src/karma/schedules.rs) |
| `karma_schedule_occurrence_expansion` | Keep: resumable bounded expansion ordinal; restart must not repeat expanded occurrences. | PK `schedule_occurrence_hash`; → `karma_schedule_occurrence` | [store/src/karma/expansions](../../crates/store/src/karma/expansions.rs) |
| `karma_transfer_stage` | Keep: scheduled boundary mapped to ordered Transfer consequence stages and parent rule revision/origin. | PK `boundary_uid`; → `karma_schedule_boundary` | [store/src/karma_stages](../../crates/store/src/karma_stages.rs) |
| `karma_transfer_command` | Keep: automation-origin remote Transfer command with captured rule revision and dispatch/cancellation. | PK `command_uid`; → `transfer_remote_command` | [store/src/karma_commands](../../crates/store/src/karma_commands.rs) |
| `karma_command_invocation` | Keep: captured command version/configuration/host/actor, output/error/numeric result and execution status. | PK `uid`; no declared FK | [engine/src/commands](../../crates/engine/src/commands.rs) |
| `karma_signal_sample` | Keep: latest captured numeric Signal result tied to invocation/time; source result must remain explainable. | PK `signal_uid`; → `karma_command_invocation`, `record` | [engine/src/commands](../../crates/engine/src/commands.rs) |
| `signal` | Keep: Record sidecar selecting external input/parse/actor; refresh actual sampling path before deleting seemingly older fields. | PK `record_uid`; → `record` | [store/src/misc](../../crates/store/src/misc.rs) |
| `effect_queue` | Keep: durable command/notification work, request identity, attempts and final result. | PK `uid`; no declared FK | [store/src/misc](../../crates/store/src/misc.rs) |
| `karma_effect_outcome` | Keep: per-effect attempt outcome; external retry semantics differ from internal Fact idempotency. | PK `effect_uid, attempt`; no declared FK | [store/src/karma_schedules](../../crates/store/src/karma_schedules.rs) |
| `fiote_activation` | Keep: activation request/actor/value/cause and created thread/status; synchronize only permitted data, not credentials. | PK `request_id`; → `record` | [cell/src/fiote/activations](../../crates/cell/src/fiote/activations.rs) |

Human review:

### F11 — Transfer negotiation and agreement

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `transfer` | Keep: Record-backed shared commitment with terms, parent/source/revision and defaults; preserve existing numeric and feature semantics. | PK `record_uid`; → `record`, `transfer` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_party` | Keep: participant identity/role in a Transfer; preserve compound party-membership constraints. | PK `uid`; → `transfer` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `promise` | Keep: intended amount/window/unit, party/Transfer/rule and source lineage; preserve current numeric representation. | PK `uid`; → `concept`, `promise`, `record` | [store/src/misc](../../crates/store/src/misc.rs) |
| `transfer_revision` | Keep: accepted term revision evidence and retry key; preserve immutable revision identity. | PK `transfer_uid, revision`; → `fact`, `transfer` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_invitation` | Keep: addressed Person invitation, attempt/status/party, expiry and sender. | PK `uid`; → `record`, `transfer`, `transfer_party` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_invitation_event` | Keep: invitation attempt/actor/revision history with Fact and retry identity. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_invitation` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_agreement` | Keep: current party agreement level and latest event/revision; not the original evidence itself. | PK `uid`; → `transfer`, `transfer_party` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_agreement_event` | Keep: immutable party/person agreement transition and accepted revision/Fact. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_party` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_agreement_target_request` | Keep: fingerprint/result for setting a reviewed target agreement level. | PK `idempotency_key`; → `record`, `transfer` | [store/src/transfers/agreement_target](../../crates/store/src/transfers/agreement_target.rs) |
| `transfer_agreement_coalition` | Keep: frozen eligible count/threshold/epoch for collective agreement. | PK `transfer_uid, revision`; → `transfer`, `transfer_agreement_event` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_agreement_coalition_member` | Keep: exact frozen coalition membership within Transfer/revision; current roster changes do not rewrite it. | PK `transfer_uid, revision, party_uid`; → `transfer_agreement_coalition`, `transfer_party` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_activation_event` | Keep: accepted activation revision and actor/Fact/retry linkage. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_phase4_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_exchange_path` | Keep: paired promise route for an exchange, with public-route identity and captured revision. | PK `uid`; → `promise`, `transfer` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_open_claim_pair` | Keep: source/proposer/claimant lineage and reuse policy for an open proposal claim. | PK `uid`; → `promise`, `record`, `transfer`, `transfer_revision` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_dependency` | Keep: revision-scoped upstream/local/remote condition for work or a promise; retain disclosure checks. | PK `uid`; → `promise`, `transfer`, `transfer_remote_reference` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_child_requirement` | Keep: required child Transfer edge; validate hierarchy and completion dependencies. | PK `parent_uid, child_uid`; → `transfer` | [store/src/transfer_children](../../crates/store/src/transfer_children.rs) |
| `transfer_child_request` | Keep: child-creation request payload and evidence; avoids repeated derived proposals. | PK `request_id`; → `fact` | [store/src/transfer_children](../../crates/store/src/transfer_children.rs) |
| `transfer_draft_discard` | Keep: reviewed expected-revision discard receipt and acting Person/Fact; logical removal is not silent deletion. | PK `request_id`; → `fact`, `record`, `transfer` | [engine/src/transfer_discard](../../crates/engine/src/transfer_discard.rs) |

Human review:

### F12 — Fulfillment, settlement and corrections

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `transfer_occurrence` | Keep: activated promise fulfillment with parties, current quantity, window/place and claim/dispute state. | PK `uid`; → `concept`, `fact`, `promise`, `record`, `transfer`, `transfer_activation_event`, `transfer_exchange_path`, `transfer_party` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_claim_event` | Keep: actor/role claim or withdrawal evidence and retry identity; preserve claims separate from settlement. | PK `uid`; → `fact`, `record`, `transfer_occurrence`, `transfer_phase4_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_dispute_event` | Keep: Person dispute transition tied to occurrence/Transfer and correction request. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_occurrence`, `transfer_phase5_correction_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_application_policy` | Keep: current receiver formula/version and authorizing policy event. | PK `occurrence_uid`; → `record`, `transfer_occurrence`, `transfer_occurrence_application_event` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_application_event` | Keep: receiver formula change evidence/version; settlement captures the applied formula. | PK `uid`; → `fact`, `record`, `transfer_occurrence`, `transfer_phase4_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_remainder_policy` | Keep: owner-specific behavior for a partially unfulfilled occurrence. | PK `occurrence_uid`; → `record`, `transfer_occurrence` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_settlement_slice` | Keep: immutable partial fulfillment and local application evidence/formula/cumulatives in their current representation. | PK `uid`; → `concept`, `fact`, `promise`, `record`, `transfer`, `transfer_occurrence`, `transfer_phase5_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_occurrence_settlement_compensation` | Keep: inverse application tied to original slice/Fact; preserve current quantities and never rewrite the settled slice. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_occurrence`, `transfer_occurrence_settlement_slice`, `transfer_phase5_correction_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_correction_link` | Keep: source revision/occurrence to new correction Transfer lineage and corrected amount in its current representation. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_occurrence`, `transfer_revision` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_promise_successor` | Keep: predecessor-to-successor promise relationship with reviewed revision and Fact. | PK `uid`; → `fact`, `promise`, `record`, `transfer`, `transfer_revision` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_cancellation` | Keep: proposed exact cancellation payload, required people and proposal/applied evidence. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_exchange_path` | [store/src/transfer_cancellations](../../crates/store/src/transfer_cancellations.rs) |
| `transfer_cancellation_application` | Keep: Person's authorized application of a cancellation with request fingerprint/evidence. | PK `request_id`; → `fact`, `record`, `transfer_cancellation` | [store/src/transfer_cancellations](../../crates/store/src/transfer_cancellations.rs) |
| `transfer_occurrence_cancellation` | Keep: exact canceled amount per occurrence and cancellation Fact; retained separate from normal settlement. | PK `occurrence_uid`; → `fact`, `transfer_cancellation`, `transfer_occurrence` | [store/src/transfer_cancellations](../../crates/store/src/transfer_cancellations.rs) |
| `transfer_phase4_request` | Keep: activation/claim/policy retry receipt family; preserve current name, operation namespace and evidence. | PK `idempotency_key`; → `fact` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_phase5_request` | Keep: settlement retry receipt family; preserve namespace, payload/result and effect identity. | PK `idempotency_key`; → `fact` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_phase5_correction_request` | Keep: correction/dispute/compensation receipt family with its own evidence and replay namespace. | PK `idempotency_key`; → `fact` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_phase6_bulk_request` | Keep: actor-reviewed bulk action snapshot/request identity and item count. | PK `uid`; → `record` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_phase6_bulk_item` | Keep: each reviewed bulk occurrence/revision/claim expectation and resulting evidence; not a live re-query. | PK `bulk_uid, ordinal`; → `fact`, `transfer`, `transfer_occurrence`, `transfer_occurrence_claim_event`, `transfer_phase6_bulk_request` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_source_group_result` | Keep: winning source-group/reuse outcome, settlement and Fact; preserves competition behavior. | PK `uid`; → `fact`, `record`, `transfer`, `transfer_occurrence_settlement_slice` | [store/src/transfers](../../crates/store/src/transfers.rs) |
| `transfer_source_group_loser` | Keep: displaced/losing Transfer revision linked to the accepted result and Fact. | PK `uid`; → `fact`, `transfer`, `transfer_source_group_result` | [store/src/transfers](../../crates/store/src/transfers.rs) |

Human review:

### F13 — Remote delivery and application handoffs

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `signed_action_intent` | Keep: signed authority for an intended Transfer/local action; distinct from the retained karma_intent family. | PK `uid`; → `identity_key`, `record` | [store/src/action_intents](../../crates/store/src/action_intents.rs) |
| `fact_action_intent` | Keep: Fact-to-authorizing-intent relation; proof of application authority is separate from network delivery. | PK `fact_uid`; → `fact`, `signed_action_intent` | [store/src/action_intents](../../crates/store/src/action_intents.rs) |
| `fact_remote_command` | Keep: Fact-to-remote-command evidence; preserves command/result causality. | PK `fact_uid`; → `fact`, `transfer_remote_command` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `organ_transfer_request_nonce` | Keep: sender/method/path/request fingerprint and nonce replay protection for Transfer requests. | PK `sender_organ_uid, nonce`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_policy` | Keep: recipient mode/state and revision for consented Transfer delivery. | PK `uid`; → `transfer` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_policy_event` | Keep: delivery policy changes with actor/request/Fact and old/new mode/state. | PK `uid`; → `fact`, `transfer_delivery_policy` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_outbox` | Keep: signed envelope/cursor work with retries and acknowledged cursor; no completed-state shortcut. | PK `uid`; → `transfer_delivery_policy` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_pull_request` | Keep: remote reference catchup request/result cursor and retry state. | PK `uid`; → `transfer_remote_reference` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_receipt` | Keep: signed stored/received evidence and cursor/hash, distinct from settlement/applying authority. | PK `uid`; → `fact` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_delivery_retry_event` | Keep: explicit delivery retry request/evidence, not overwritten diagnostic text. | PK `uid`; → `transfer_delivery_outbox`, `transfer_delivery_policy` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_remote_reference` | Keep: received remote commitment view/disclosure and monotonic cursor/revision; remote IDs lack local FK intentionally. | PK `uid`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_remote_policy_event` | Keep: retained remote policy revision/change evidence and signed payload. | PK `uid`; → `transfer_remote_reference` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_replica_envelope` | Keep: received signed envelope evidence and reference/cursor before projection application. | PK `envelope_uid`; → `transfer_remote_reference` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_remote_command` | Keep: outgoing/incoming command request fingerprint, expected revision, attempts and authoritative result. | PK `command_uid`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_remote_conflict` | Keep: reviewed command/envelope/revision conflict payload; supports human recovery rather than silent overwrite. | PK `uid`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_outcome_evidence` | Keep: received signed outcome/cursor/revision evidence; source data for permitted outcome inspection. | PK `envelope_uid`; → `transfer_remote_reference` | [store/src/transfer_outcomes](../../crates/store/src/transfer_outcomes.rs) |
| `transfer_application_handoff` | Keep: origin-to-participant application state, slice hash and request/attestation identity. | PK `uid`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_application_handoff_detail` | Keep: canonical amount/cumulative/remainder/direction and origin evidence for local handoff; preserve source numeric representation. | PK `handoff_uid`; → `fact`, `transfer_application_handoff` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_application_handoff_event` | Keep: handoff state/attestation/Fact/reason history; evidence survives projected-state changes. | PK `uid`; → `fact`, `transfer_application_attestation`, `transfer_application_handoff` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_application_attestation` | Keep: signed participant application acknowledgment with canonical commitment and local application evidence identity. | PK `uid`; no declared FK | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_application_attestation_outbox` | Keep: independently retried signed attestation to origin; do not expose private policy data. | PK `attestation_uid`; → `transfer_remote_application_handoff`, `transfer_remote_reference` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_remote_application_handoff` | Keep: canonical remote slice/evidence/cursor and received application state; preserve current values and external IDs. | PK `uid`; → `transfer_remote_reference` | [store/src/transfer_delivery](../../crates/store/src/transfer_delivery.rs) |
| `transfer_local_application` | Keep: separately authorized local delta/cumulatives/formula snapshot and Fact; preserve current values, privacy and receipt identity. | PK `uid`; → `fact`, `record`, `signed_action_intent` | [store/src/transfer_accounting](../../crates/store/src/transfer_accounting.rs) |

Human review:

### F14 — Resource safeguards, private effects, loans and capture

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `transfer_private_policy` | Keep: current Person/Transfer/exchange private application policy pointer; not a shared formula grant. | PK `transfer_uid, exchange_uid, person_uid`; → `record`, `transfer_private_policy_event` | [store/src/transfer_accounting](../../crates/store/src/transfer_accounting.rs) |
| `transfer_private_policy_event` | Keep: signed/versioned local mapping/formula/effect policy and authorizing key/request. | PK `uid`; → `record` | [store/src/transfer_accounting](../../crates/store/src/transfer_accounting.rs) |
| `transfer_private_effect` | Keep: private grouped effect Fact, source occurrence/commitment and local Record/unit; exclude unconsented capture. | PK `fact_uid`; → `concept`, `fact`, `record` | [store/src/transfer_effects](../../crates/store/src/transfer_effects.rs) |
| `transfer_private_application_correction` | Keep: reviewed local application correction request/Fact without rewriting original evidence. | PK `uid`; → `fact`, `record`, `transfer_local_application` | [store/src/transfer_accounting](../../crates/store/src/transfer_accounting.rs) |
| `transfer_stock_limit` | Keep: exact minimum, authorized writer/Person and version/event for a local Record. | PK `record_uid`; → `record` | [store/src/transfer_stock](../../crates/store/src/transfer_stock.rs) |
| `transfer_stock_limit_event` | Keep: signed stock restriction policy/request evidence and public-key verification data. | PK `uid`; → `record` | [store/src/transfer_stock](../../crates/store/src/transfer_stock.rs) |
| `transfer_stock_roster_history` | Keep: retained signed roster authorization at stock-policy time; deliberately excluded from generic Transfer capture. | PK `organ_uid, version`; no declared FK | [store/src/transfer_replication/schema](../../crates/store/src/transfer_replication/schema.rs) |
| `transfer_loan_agreement` | Keep: accepted loan interval/revision/exchange and agreement Fact; preserve interval semantics. | PK `transfer_uid, exchange_uid, revision`; → `fact`, `record` | [store/src/transfer_loans](../../crates/store/src/transfer_loans.rs) |
| `transfer_loan_extension` | Keep: extension request payload, resulting revision and Fact; do not replace agreed history. | PK `request_id`; → `fact` | [store/src/transfer_loans](../../crates/store/src/transfer_loans.rs) |
| `transfer_sync_control` | Keep: singleton import guard preventing echoed capture during remote materialization. | PK `id`; no declared FK | [store/src/transfer_replication](../../crates/store/src/transfer_replication.rs) |
| `transfer_sync_journal` | Keep: owned row changes grouped by commit sequence, source Organ/Cell and sequence; preserve current capture coverage under D07. | PK `seq`; no declared FK | [store/src/transfer_replication](../../crates/store/src/transfer_replication.rs) |
| `transfer_sync_message` | Keep: immutable signed per-Organ/Cell transaction stream and included journal end. | PK `organ_uid, cell_uid, sequence`; no declared FK | [store/src/transfer_replication](../../crates/store/src/transfer_replication.rs) |
| `transfer_sync_owner` | Keep: owning Cell per table/row key; changes must route to the authoritative writer. | PK `table_name, row_key`; no declared FK | [store/src/transfer_replication](../../crates/store/src/transfer_replication.rs) |
| `visibility_rule` | Keep/review: live field/target disclosure grant rules; reconcile with settled role policy and current replication filtering. | PK `uid`; no declared FK | [store/src/visibility](../../crates/store/src/visibility.rs) |

Human review:

### F15 — Public social publication and search

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `social_document` | Keep: accepted current signed snippet/profile plus query projection; maintain state/generation/hash consistency. | PK `kind, id`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_revision` | Keep: alternative/historical signed document revisions retained for conflict and current authority checks. | PK `kind, id, hash`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_profile_authority` | Keep: identified profile root/editor generation plus retained revision/hash floor; protect against old profile resurrection. | PK `organ`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_posting_authority` | Keep: anonymous owner/editor generation floor; distinct from identified profile identity. | PK `owner`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_ended_post` | Keep: permanent/retained ending authority and revision/hash floor; body removal cannot permit stale repost. | PK `id`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_publication_job` | Keep: consented destination/body and retry/receipt; separate transport completion from local signed source. | PK `hash, destination`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_public_asset` | Keep: bounded public image bytes/dimensions/hash/use time; local private assets remain separate. | PK `hash`; no declared FK | [engine/src/social/media](../../crates/engine/src/social/media.rs) |
| `social_listing_removal` | Keep: operator's local hide/removal memory with reason/time; not the author's signed withdrawal. | PK `post`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_search` | SQLite-owned derived index: FTS5(id,title,text) with indexed IDs after 0332; maintain accepted eligible documents and rebuild explicitly. | No declared PK; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `social_search_config` | SQLite-owned: FTS5 configuration shadow table; never declare/drop/seed independently of social_search. | PK `k`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `social_search_content` | SQLite-owned: FTS5 stored content shadow; not a second authored document table. | PK `id`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `social_search_data` | SQLite-owned: FTS5 index blocks/metadata; physical internals are not stable application data contracts. | PK `id`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `social_search_docsize` | SQLite-owned: FTS5 token-size metadata; remove only by removing/rebuilding the virtual table. | PK `id`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |
| `social_search_idx` | SQLite-owned: FTS5 segment index; no direct repository or custom independent schema. | PK `segid, term`; no declared FK | [initial schema](../../crates/store/migrations/0001_init.sql) |

Human review:

### F16 — Private social messages and service carriers

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `social_device_state` | Keep: device-local account/session/private transport state with compare-and-set version; never synchronize live ratchets as ordinary content. | PK `id`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_session_peer` | Keep: context/peer mapping to local session state; replacement requires authorized fresh session. | PK `context, peer`; → `social_device_state` | [engine/src/social/delivery](../../crates/engine/src/social/delivery.rs) |
| `social_mailbox_pin` | Keep: trusted service owner/signing/identity/pickup keys and monotonic pickup sequence. | PK `mailbox`; no declared FK | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_message_work` | Keep: local message/conversation/context preparation queue and expiry/retry/error; preparation is durable work. | PK `record_uid`; → `record` | [engine/src/social/delivery](../../crates/engine/src/social/delivery.rs) |
| `social_message_event` | Keep: edited/content message event linked to original Fact and retry scheduling. | PK `event`; → `fact`, `record` | [engine/src/social/delivery](../../crates/engine/src/social/delivery.rs) |
| `social_message_identity` | Keep: stable protocol message/content-hash to local message/conversation identity; prevents duplicate materialization. | PK `message`; no declared FK | [engine/src/social/conversation](../../crates/engine/src/social/conversation.rs) |
| `social_private_outbox` | Keep: sealed envelope, Record link, recipient owner/generation and delivery/held state; includes attachment envelope metadata. | PK `id`; → `record` | [engine/src/social/delivery_worker](../../crates/engine/src/social/delivery_worker.rs) |
| `social_private_destination` | Keep: independent service destination attempt/receipt/state for a sealed envelope; current attachments migration final shape. | PK `envelope, service`; → `social_private_outbox` | [engine/src/social/delivery_worker](../../crates/engine/src/social/delivery_worker.rs) |
| `social_private_seen` | Keep: inbound envelope/cipher/content identity and receipt; changed ciphertext/content must not reuse an accepted ID. | PK `envelope`; no declared FK | [engine/src/social/conversation](../../crates/engine/src/social/conversation.rs) |
| `social_receive_failure` | Keep: bounded failed inbound delivery/discard/retry reference under context authority; preserve recoverable errors. | PK `context, service, envelope`; → `record` | [engine/src/social/refusal](../../crates/engine/src/social/refusal.rs) |
| `social_peer_work` | Keep: per-conversation preparation/synchronization retry work without duplicating message rows. | PK `conversation`; → `record` | [engine/src/social/delivery_worker](../../crates/engine/src/social/delivery_worker.rs) |
| `social_pickup_work` | Keep: context/service pickup scheduling and retries; lease/authority contracts live in the worker. | PK `context, service`; no declared FK | [engine/src/social/delivery_worker](../../crates/engine/src/social/delivery_worker.rs) |
| `social_owner_control` | Keep: private context owner generation/ending proof; older control cannot revive ended authority. | PK `owner`; no declared FK | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_reply_route` | Keep: signed mailbox reply route/post/owner/partition/expiry context; carriers see permitted routing, not plaintext Records. | PK `id`; no declared FK | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_sender_admission` | Keep: admitted/refused sender policy per route; authorization separate from rate counters. | PK `route, sender`; → `social_reply_route` | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_sender_counter` | Keep: expiring sender introduction/provisional quota accounting; bound hostile input. | PK `route, sender`; no declared FK | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_service_envelope` | Keep: carrier-held ciphertext/envelope metadata with route, expiry and pickup sequence; no application plaintext. | PK `id`; → `social_reply_route` | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_service_completed` | Keep: completed envelope hash/receipt/stage with expiry; retained replay protection after body deletion. | PK `id`; no declared FK | [engine/src/social/mailbox](../../crates/engine/src/social/mailbox.rs) |
| `social_service_budget` | Keep: per-source/direction time-window byte/work quotas; independent from total storage budget. | PK `source, direction`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |

Human review:

### F17 — Distributed discovery, subscriptions, reports and retention

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `discovery_cache` | Keep: live proximity/discovery cache of remote open promises; preserve current amount/confidence representations and source/freshness behavior. | PK `promise_uid`; no declared FK | [store/src/senses](../../crates/store/src/senses.rs) |
| `sense_rule` | Keep: live Record-backed concept/proximity/confidence matching rule and current automatic behavior. | PK `record_uid`; → `record` | [store/src/senses](../../crates/store/src/senses.rs) |
| `social_discovery_source` | Keep: each document hash's observed source/freshness for helper selection and provenance. | PK `post, hash, source`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_discovery_conflict` | Keep: same post/generation/revision conflicting hashes/bodies and observation time; surface human review. | PK `post`; no declared FK | [store/src/social](../../crates/store/src/social.rs) |
| `social_gossip_item` | Keep: consented signed document/control packet and assigned forwarding status; uniqueness by kind/document hash. | PK `hash`; no declared FK | [engine/src/social/gossip_store](../../crates/engine/src/social/gossip_store.rs) |
| `social_gossip_forward` | Keep: item-to-peer/contact forwarding retry state; cancellation must follow consent/trust changes. | PK `hash, peer`; → `social_gossip_item` | [engine/src/social/gossip_worker](../../crates/engine/src/social/gossip_worker.rs) |
| `social_gossip_seen` | Keep: expiring forwarded-packet identity/control flag for loop/replay suppression. | PK `hash`; no declared FK | [engine/src/social/gossip](../../crates/engine/src/social/gossip.rs) |
| `social_gossip_scan` | Keep: singleton resumable scan cursor and failure; restart does not repeat an unbounded publication scan. | PK `id`; no declared FK | [engine/src/social/gossip_worker](../../crates/engine/src/social/gossip_worker.rs) |
| `social_ask_query` | Keep: local actor request, selected peers, deadline/results/state; inquiry is not a Transfer agreement. | PK `id`; no declared FK | [engine/src/social/ask_worker](../../crates/engine/src/social/ask_worker.rs) |
| `social_ask_seen` | Keep: request binding/source, reserved reply/deadline; bounded duplicate-response behavior. | PK `id`; no declared FK | [engine/src/social/ask](../../crates/engine/src/social/ask.rs) |
| `social_subscription_job` | Keep: saved-search config hash, fenced polling lease, attempts/completion/source/results; definition remains in authored configuration. | PK `id`; no declared FK | [engine/src/social/subscriptions/worker](../../crates/engine/src/social/subscriptions/worker.rs) |
| `social_subscription_seen` | Keep: subscription/post/hash match and notification receipt/expiry; restart must not notify twice. | PK `id`; no declared FK | [engine/src/social/subscriptions/worker](../../crates/engine/src/social/subscriptions/worker.rs) |
| `social_report_work` | Keep: actor/service report submission queue with bounded signed body/hash, expiry and retry/result; references live inside the body. | PK `id`; no declared FK | [engine/src/social/reports](../../crates/engine/src/social/reports.rs) |
| `social_report_seen` | Keep: source/report hash replay protection and expiry for admitted reports. | PK `id`; no declared FK | [engine/src/social/reports](../../crates/engine/src/social/reports.rs) |
| `social_report_admission` | Keep: source time-window/count budget for report submissions. | PK `source`; no declared FK | [engine/src/social/reports](../../crates/engine/src/social/reports.rs) |
| `social_received_report` | Keep: admitted bounded report body/source/hash, receive time and expiry for operator inspection; no separate handling-state column. | PK `id`; no declared FK | [engine/src/social/reports](../../crates/engine/src/social/reports.rs) |
| `social_context_retention` | Keep: active/dormant/retired/review state with staged metadata/queue cleanup deadlines and errors. | PK `context`; → `record` | [engine/src/social/retention](../../crates/engine/src/social/retention.rs) |

Human review:

### F18 — Generic mailbox and contact delivery

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `mailbox_registration` | Keep: admitted Organ/root key, label and quota; registration is different from pending request. | PK `organ_uid`; no declared FK | [store/src/mailbox](../../crates/store/src/mailbox.rs) |
| `mailbox_request` | Keep: Organ/root mailbox service request pending operator admission. | PK `organ_uid`; no declared FK | [store/src/mailbox](../../crates/store/src/mailbox.rs) |
| `mailbox_invite` | Keep: hashed one-use invite, quota, expiry/use and registering identity. | PK `token_hash`; no declared FK | [store/src/mailbox](../../crates/store/src/mailbox.rs) |
| `mailbox_bundle` | Keep: encrypted generic bundle, sender/recipient/device identity, bytes and expiry; reference admitted recipient quota. | PK `uid`; → `mailbox_registration` | [store/src/mailbox/delivery](../../crates/store/src/mailbox/delivery.rs) |
| `mailbox_device_ack` | Keep: recipient-device pickup acknowledgment for retained bundle; not an applied business effect. | PK `uid, node_id`; → `mailbox_bundle` | [store/src/mailbox/delivery](../../crates/store/src/mailbox/delivery.rs) |
| `mailbox_roster_floor` | Keep: newest admitted signed recipient roster/version; revoked devices cannot keep collecting old mail. | PK `organ_uid`; → `mailbox_registration` | [store/src/mailbox/delivery](../../crates/store/src/mailbox/delivery.rs) |
| `mailbox_completion` | Keep: completed recipient/body-hash receipt with expiry; suppress identical completed bundle replay. | PK `uid`; → `mailbox_registration` | [store/src/mailbox/delivery](../../crates/store/src/mailbox/delivery.rs) |
| `mailbox_expiry_notice` | Keep: expired bundle metadata and notification state without retaining the entire cipher body. | PK `uid`; no declared FK | [store/src/mailbox](../../crates/store/src/mailbox.rs) |
| `mailbox_outbox` | Keep: sender-local sealed delivery intent/copy count, retry/error and expiry; current authorization must remain valid. | PK `intent`; no declared FK | [store/src/mailbox/outbox](../../crates/store/src/mailbox/outbox.rs) |
| `mailbox_outbox_authority` | Keep: policy hash captured for queued delivery and current-policy recheck; refresh recent 0333 work before freeze. | PK `uid`; → `mailbox_outbox` | [engine/src/mailbox](../../crates/engine/src/mailbox.rs) |
| `mailbox_outbox_receipt` | Keep: per-carrier accepted storage receipt; do not expose it as recipient application. | PK `uid, carrier_node`; → `mailbox_outbox` | [store/src/mailbox/outbox](../../crates/store/src/mailbox/outbox.rs) |
| `mailbox_inbox` | Keep: retained received ciphertext/hash, carrier and retry state before authorized application. | PK `uid`; no declared FK | [store/src/mailbox/delivery](../../crates/store/src/mailbox/delivery.rs) |
| `mail_left` | Keep: carrier/recipient reach and expired-mail history used by fallback/status. | PK `uid, carrier_node`; no declared FK | [store/src/mail_left](../../crates/store/src/mail_left.rs) |
| `peer_delivery` | Keep: per-Organ/Cell/node attempted/successful delivery, covered sequence and reach addresses; local operational state. | PK `organ_uid, cell_uid, node_id`; no declared FK | [store/src/peer_delivery](../../crates/store/src/peer_delivery.rs) |
| `contact_rate` | Keep: sender/kind time-window counts and backoff reason; preserve abuse/retry bounds separately from trust. | PK `from_organ, kind`; no declared FK | [store/src/contact_rate](../../crates/store/src/contact_rate.rs) |

Human review:

### F19 — Storage, projection, update and simulation state

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `blob_sync` | Keep: file-copy offer/manifest/peer/owner/state/progress/destination/error; validate paths/hashes and keep actual bytes in file storage. | PK `id`; no declared FK | [store/src/blob_sync](../../crates/store/src/blob_sync.rs) |
| `configuration` | Keep: typed singleton defaults and interface/storage settings; preserve current seed/open contracts and settings. | PK `id`; no declared FK | [store/src/config](../../crates/store/src/config.rs) |
| `projection_source` | Keep: source revision/runtime identity for invalidation; narrow trigger coverage only with proven dependencies and measured benefit under D07. | PK `id`; no declared FK | [store/src/projection](../../crates/store/src/projection.rs) |
| `projection_window` | Keep: cached context/window/revision and incomplete status; discard/rebuild safely, never treat as authored intent. | PK `id`; no declared FK | [store/src/projection](../../crates/store/src/projection.rs) |
| `projection_span` | Keep: bounded cached time spans and Record references; interval query indexes follow measurement. | PK `id`; no declared FK | [store/src/projection](../../crates/store/src/projection.rs) |
| `projection_schedule` | Keep: cached scheduled point/open-or-bounded interval payloads; preserve window query and cache rebuild behavior. | PK `id`; no declared FK | [store/src/projection](../../crates/store/src/projection.rs) |
| `self_update` | Keep: singleton update/restart durable state; installer/startup checks depend on it. | PK `id`; no declared FK | [cell/src/information](../../crates/cell/src/information.rs) |
| `simulation_check_set` | Keep: named/check-definition revision used by Simulation Castle and deterministic checks; not production action authority. | PK `uid`; no declared FK | [store/src/simulation_checks](../../crates/store/src/simulation_checks.rs) |

Human review:

### F20 — Native operations, shared workspaces, assets and packages

| Table | Purpose and proposed treatment | Declared key and connections | Source |
| --- | --- | --- | --- |
| `interface_area_transition` | Keep: actor/request/payload receipt for Area-caused database change; no workspace topology is stored here. | PK `request_id`; no declared FK | [engine/src/area_transition](../../crates/engine/src/area_transition.rs) |
| `description_assets` | Keep: bounded image/drawing bytes attached to a Record description; composite asset identity and parent cascade. Large payloads exclude it from the first WITHOUT ROWID trial. | PK `record_uid, asset`; → `record` | [engine/src/description_assets](../../crates/engine/src/description_assets.rs) |
| `sand_package` | Keep: origin/id/version package document/manifest/digest and publication/verification/source metadata; retain library ordering and receiving/enabling distinctions. | PK `origin, id, version`; no declared FK | [store/src/sand_packages](../../crates/store/src/sand_packages.rs) |
| `shared_workspace` | Keep: hosted workspace name, revision, policy and layout JSON; existing compare-and-set and admission behavior. | PK `uid`; no declared FK | [engine/src/workspace_sync](../../crates/engine/src/workspace_sync.rs) |
| `workspace_change` | Keep: pending/applied/rejected workspace change, original actor admission, review and applied revision. D02 reviews history/revision lookup indexes. | PK `uid`; → `shared_workspace` | [engine/src/workspace_sync](../../crates/engine/src/workspace_sync.rs) |
| `workspace_receipt` | Keep: scoped actor/request payload/result replay receipt for an accepted workspace operation. Preserve exact replay identity and parent cascade. | PK `workspace_uid, actor_uid, request_id`; → `shared_workspace` | [engine/src/workspace_sync](../../crates/engine/src/workspace_sync.rs) |
| `workspace_draft` | Keep: actor-owned saved proposed change with host/workspace/base revision; may refer to a workspace outside the local database. | PK `uid`; no declared FK | [engine/src/workspace_sync](../../crates/engine/src/workspace_sync.rs) |

Human review:

## The remaining schema objects

The five search shadow tables are included above for completeness. SQLite's `sqlite_sequence`, automatic indexes and SQLx's `_sqlx_migrations` are excluded from the product table count. The application migration runner will create its own metadata on actual installation.

Keep `transfer_application_effect_handoff`: it unions a received remote application handoff with the local origin's handoff/detail/application state. Preserve its output columns, current numeric representations and result semantics. It is a read view, not another independently writable authority.

The 258 migration-defined triggers fall into several responsibilities: immutable evidence, allowed state transitions, compound Transfer scope, revision/generation maintenance, account/admission invalidation, application evidence matching and import/setup guards. Inventory their names and SQL from a fully opened fresh Store at P02, including generated triggers. For each retained trigger, record a positive and a rejecting case where it protects a feature invariant. Do not remove an immutability or authorization trigger as a generic write optimization.

Migration-only backfill/setup guards must be assessed separately. A guard used only to move historical data is not needed in an empty initial database; an import/replay guard used by live replication is still needed. Search for trigger names and diagnostic text used in tests before changing definitions. Rewrite old migration-fixture tests around the current invariant rather than keeping historical migration files solely to satisfy those tests.

### Structural changes that need a reader/writer audit

| Change | Readers/writers that must agree | Concrete check |
| --- | --- | --- |
| Remove a table/column | Typed repository, direct SQL callers, dynamic installers, move/capture allowlists, snapshot/state hash, import/export and fixtures | No live query, bundle or trigger refers to the removed object; fresh reopen works. |
| Change a table's physical layout | Explicit/implicit rowid uses, insert results, primary/secondary lookup keys, SQLx integration, capture/snapshot enumeration and SQLite hooks | Same rows, ordering and lifecycle behavior; demonstrated storage or workload benefit. |
| Tighten an enum/check/FK | Every state transition, seed/default, expiry worker, import path and malformed-data test | Valid lifecycle still completes; invalid combinations fail before partial effects. |
| Normalize split membership storage | Authentication/principal/display code, policy combination, generations, session checks and existing UI | Identical current memberships, primary/display Role, permission union, recovery and stale-session results. |
| Remove/rename an index | Query plans, `INDEXED BY`, uniqueness/compound-FK requirements and write workload | Same correctness; measured read/write/storage result justifies change. |
| Change a trigger | Its installer, revision/capture/import transaction order and tests | Positive behavior, rejection, rollback and restart agree; no duplicate installation. |
| Remove superseded storage | Current replacement, legacy FK columns, import/export, fixtures and history presentation | No loss of distinct live state, provenance, replay protection or workflow behavior. |

Human review:

## Sequential implementation plan, only after review and permission

The human first requested consolidation alone, completing P10, and then authorized conservative optimization and obsolete-storage removal. P03 and the storage-removal portion of P07 are implemented; P04 is implemented only for C01, C02 and C04. The separate membership normalization proposal remains unimplemented. Remaining candidates need measurement and must fit the current minimal-business-change scope. Keep the migration chain usable as features and approved schema changes land. Do not repeat this reset unless explicitly requested.

Each checkpoint should make one or several focused commits, with compiling consumers and appropriate regression checks at each meaningful boundary. A schema normalization may need corresponding backend/UI readers in the same commit. Work on the existing branch, with no worktrees or parallel coding agents. Stage only files belonging to the checkpoint and inspect the index so another person's staged work is not swept into a commit.

| Step | Work and deliverable | Validation and proposed commit boundary |
| --- | --- | --- |
| [ ] P01 | Refresh this research against settled storage needs. Reconcile human comments, keep grants/intents, and select schema-only candidates from D02–D10. Record commit plus uncommitted file hashes and retained/obsolete object list. | Human approval of the concrete refreshed optimization scope; documentation checkpoint. No reset yet. |
| [ ] P02 | Establish current application baseline with isolated valid fixtures. Capture fully initialized schema including generated triggers, seed/open behavior and Q01–Q11 results. Identify existing failures separately. | Existing Store/engine/UI suites appropriate to scope; repeatable fresh-install/query baseline and fixture/check commit if new harness work is necessary. |
| [x] P03 | Remove verified duplicate indexes under D10; retain the complete enforcing unique keys. Audit named-index queries and migration-fixture assumptions. | Same uniqueness and representative reads; measure index/file size and write cost. One focused duplicate-index commit. |
| [ ] P04 | Trial lookup indexes C01–C04 and ordered/worker indexes C05/C07 against populated fixtures and statistics. Keep only worthwhile keys, replacing old secondary indexes only when their other uses remain supported. | Q01, Q03–Q05/Q09 as relevant; same aliases, hierarchy, Record visibility/order, extensions, worker claims and workspace history. Separate measured index commits. |
| [ ] P05 | Trial JSON expression and active-work partial indexes C06/D10. Retain source payloads and existing lifecycle semantics. | Q05/Q08/Q09, skewed active/history distributions, body validity, revocation/FTS cleanup, expiry, leases and write/file costs. Commit each accepted optimization separately. |
| [ ] P06 | Trial D08 physical layouts one relationship table at a time. Do not convert rowid-dependent evidence/queues or large payload tables. | Equal rows, lookup/result order, FK/cascade behavior, seeding, snapshot/capture, hooks and insert results; file size/read/write measurements. Small layout commits. |
| [x] P07 | Removed the four D03 obsolete tables, two columns and stale move references after auditing their current replacements. D05 membership normalization was not applied; it remains a separate proposal. | Frequency/rule import/export, timer restart/retry, complete Record-move bundles and offers; unchanged Role unions, authentication/display, recovery and revisions. Commit each complete storage cleanup independently. |
| [ ] P08 | Carry or tighten approved constraints under D06, matching existing validated invariants. Preserve current numeric types, wire payloads, states, external references and deletion/retention rules. | Valid seed/lifecycle/import paths still succeed; current invalid combinations fail atomically. Focused constraint commits with rejecting cases. |
| [ ] P09 | Measure generated-trigger cost and trial D07 projection dependency narrowing only if complete. Verify current capture/install definitions and snapshot/backup consumers. | Q10, unchanged projection results under relevant changes, cache reuse under proven irrelevant changes, rollback/privacy/reopen and no stale derived state. Retain broad coverage if proof or benefit is absent. |
| [x] P10 | Consolidated the existing final definitions and migration-created rows into `0001_init.sql`; removed the previous migration files and adapted historical backfill tests. No table/index optimization or feature redesign was applied. | Fresh before/after databases have matching schema, table properties, keys, indexes and data; integrity/FK checks pass. Actual Store checks and tests are recorded in the reset journal below. |
| [ ] P11 | Run full release acceptance on fresh initialized Cells plus deterministic scenarios and native/Facade consumers. Recheck representative query/write budgets after the consolidated install. | Workspace checks/tests, fresh seed/restart, transport and UI journeys; physical device/provider checks recorded separately when they cannot be automated. Fix each discovered regression in focused commits. |
| [ ] P12 | Refresh this document to the implemented design: final table inventory, actual metrics, decisions, commits, defects fixed and remaining limitations. | Human can trace retained tables to their existing features and removals to verified equivalent keys or superseded storage. Documentation acceptance commit. |

Feature work continuing elsewhere changes the baseline, not the scope of this plan. Refresh affected checkpoints before implementation. If storage is still unsettled, complete independent approved optimizations and record the dependency; delay the single consolidation rather than redesigning or rebuilding that feature here.

Human review:

## Acceptance checklist for the implementation

### Fresh schema and seeds

- [ ] A new file installs the intended schema through the actual SQLx/Store path, not only an SQL script replay.
- [ ] Required runtime-generated triggers match the recorded current behavior or an explicitly approved measured dependency change, and exist after reopening.
- [ ] `_sqlx_migrations` contains the single initial application migration after consolidation; its checksum is reproducible from the committed file.
- [ ] `open_durable` installs structural state without creating domain defaults; existing-only open does not create/migrate/seed a file.
- [ ] Ordinary Store/Cell startup produces the correct local Vocabulary/identity/configuration/permission/bootstrap state exactly as its contract requires.
- [ ] Repeated seeds and restart do not recreate deleted content, regrant revoked permissions or prompt for bootstrap incorrectly.
- [ ] All automation grant/intent tables, their constraints and seeded intent status/reservation rules remain present.
- [ ] `integrity_check` and `foreign_key_check` pass after seed and representative workflows, not only on an empty database.

### Correctness and security

- [ ] Existing quantity types, calculations, hashes, signatures and wire/UI results remain unchanged through settlement, local application and compensation; exact ledger tests still pass.
- [ ] Every current-state/evidence pair remains consistent; audit/rebuild operates only from sufficient retained sources.
- [ ] Failed multi-table writes roll back without partial Facts, quantities, receipts, cursor movement or session advancement.
- [ ] Repeated identical requests return the original result; changed-payload retries and stale revisions fail.
- [ ] Permission unions, primary/display Role behavior, Record policy, workspace admission and actor identity preserve their current contracts.
- [ ] Separate Organs, recipients and private/public data remain isolated through query, sync, capture, backup, logs and UI hydration.
- [ ] Revoked devices/editors and stale rosters/authority generations cannot resume queued work or revive withdrawn/ended content.
- [ ] All supported lifecycle transitions and deletion/expiry cases preserve required proof and replay memory.
- [ ] Malformed JSON, unsupported versions, invalid/oversized identities/payloads, exhausted counters, unsafe paths and hostile query inputs are refused within existing feature limits.

### Performance, restart and real interaction

- [ ] Changed Q01–Q11 workloads have before/after measurements with disclosed fixtures and comparable runtime settings; the remaining workloads pass representative regression checks. Indexes have an integrity or measured query justification.
- [ ] File-backed WAL tests cover concurrent writers/readers, worker claims, busy handling and crash/reopen behavior.
- [ ] Queues, timer leases, ratchets, publications, bundles and partial Transfers resume honestly after forced interruption at important commit boundaries.
- [ ] Native Record, Ontology, Access Control, Organ, Sync, Thread, Rule/Frequency/Karma, Transfer, Configuration and Information workflows function against the fresh schema.
- [ ] Workspace/Area/presentation restore, local file tools, attachments, editor drafts and package metadata remain functional; failed saves/operations are visible and recoverable.
- [ ] Active Facade authentication, security and Record/settings views pass; the disconnected web crate remains untouched.
- [ ] Simulation replay/restart/transfer/Karma checks agree with intentional schema/state-hash changes; physical device/provider validation has its actual result recorded.

### Commands and evidence discipline

Use `cargo check`, never `cargo build`, and keep the workspace's warnings-as-errors policy. Start with checks/tests covering the changed feature; repeat or broaden only for changed concerns and the final integration stage. For example, after implementation and in the repository's configured development environment:

```sh
cargo check -p nucleus -p store -p engine -p protein -p lince-cell --all-targets
cargo test -p store
cargo test -p engine
cargo test -p lince-simulation
nix develop .#interface --command cargo check -p lince-interface -p lince-desktop --all-targets
nix develop .#interface --command cargo test -p lince-desktop
cargo check --workspace --all-targets
cargo test --workspace --all-targets
```

These are planned command families, not commands run for this document. Confirm current feature flags and system requirements at implementation time. Run tests that invoke external effects against fixtures/stubs or explicitly configured test directories, rather than personal provider accounts or the normal data directory. An unavailable toolchain/system library or unrelated concurrent failure is recorded with its command and cause; it is not a passing application check.

Human review:

## Refresh checklist before implementation and between checkpoints

1. Capture the current branch/HEAD and working-tree state. Do not assume another person's uncommitted work disappeared or is part of this task.
2. Replay the current migration chain into a separate database; compare table/view/index/trigger sets and per-table columns, keys, checks, defaults and seeded structural values with this inventory.
3. Compare fully opened Store schema with migration-only schema, including both runtime installers. Recheck the actual SQLx-linked SQLite feature set.
4. Audit changed table references across Store, engine, nucleus, Protein, Cell, transport, simulation, native desktop/interface/mobile and active Facade. Include dynamic allowlists, schema enumeration, SQL JSON paths, signatures and file import/export.
5. Reconcile F01–F20 with current consumers and newly completed features. Update actual extension/file-storage contracts, not just SQL names. Recheck split Role representation, shared-workspace/package/asset storage and any new indexes/triggers.
6. Mark storage candidates live, physically redundant or superseded from actual behavior. Keep grants/intents regardless of the earlier literal-reference coverage gap. Preserve human comments and explain changed recommendations.
7. Update baseline queries/fixtures and acceptance tests affected by the change. Name known failures and remaining physical/provider validation separately from schema defects.
8. Record the completed checkpoint's commit(s), tests, measurements, new reasoning and next step before context compaction. Do not begin a changed-scope implementation under stale approval.

The Record-move migration was edited during this research: `cancel_sent` was added to `record_move_offer` while its worker was evolving. That is an example of live storage needs settling, not a stable defect report. Refresh its state machine, source snapshot and final schema before cutover.

Migrations `0335`–`0342` then added description assets, Sand packages, projection schedules, Organ admission observations, additive Role membership/seeding state and shared-workspace changes/receipts/drafts/admission snapshots. This refresh records them as existing storage. Before approving implementation, rerun the inventory and query review if those definitions have changed again.

## Implementation journal

Migration consolidation and the conservative optimization pass above are implemented. Other schema/layout/index candidates remain unimplemented. Future agents should extend the journal when each approved checkpoint completes, with enough context to resume without reconstructing the entire investigation.

| Checkpoint/date | Commit(s) and exact scope | Implemented schema/backend/frontend behavior | Checks and measured results | New reasoning, human decisions and next step |
| --- | --- | --- | --- | --- |
| Research, 2026-10-03 | Documentation only; no feature or migration commit | Current table/feature map and proposed reset architecture | Isolated SQL replay, integrity/FK checks, table coverage and static duplicate-index review; application suites not run | Await human review. Refresh after feature storage settles, then obtain implementation permission. |
| Scope correction and schema refresh, 2026-10-03 | Documentation only; no application/migration changes | Existing workflows retained; automation grants/intents retained; schema-only optimization candidates and sequential plan | 163-migration SQL replay, 283-table inventory, six duplicate layouts, six empty-schema plan probes and two synthetic storage examples; no application benchmark/check/test run | Human rejected feature redesign and grant removal. Earlier numeric/authority/receipt feature proposals withdrawn. Refresh candidates and run real baselines only after implementation permission. |
| Conservative optimization, 2026-10-03 | Index checkpoint `720851e1` and frequency cleanup `85b77198`; old move-table removal plus consumer/test/documentation changes in the working tree | Four obsolete tables and two columns removed; five redundant indexes removed, three obsolete indexes retired, three measured lookup indexes added; grants and workflow rules retained | Seed/column/trigger/view comparisons, integrity/FK checks and Store/Engine all-target checks pass; 45 focused Store tests and all 16 move integration tests pass, two baseline Store tests filtered; three read and paired write/index-size measurements recorded above | Human authorized common schema optimization and unused-storage removal. Current working tree includes other contributors’ untracked move sources; preserve their work when committing. No feature redesign or speculative table-layout conversion. |
| Migration reset, 2026-10-03 | Consolidation only under the later narrow instruction; initial schema and corresponding migration-history tests | 163 migrations replaced by one initial migration with identical final schema and seed data; runtime installers and feature behavior retained | SQL/structural/data comparisons and integrity/FK checks pass; 1,008 generated triggers and runtime seeds match on fresh open/reopen; Store checks pass; focused tests: 85 passed, one existing bootstrap failure, one existing durability expectation filtered | Broader optimization remains unimplemented. Original migrations and verification artifacts retained in the session scratch directory. Two baseline test issues are documented above; release acceptance remains pending. |

For later entries, include the concrete invariants changed, any rejected approach that explains the final design, a reproducible failing case/fix, files touched outside the obvious repository module, unresolved gaps, and the next numbered checkpoint. If a regression appears, relate it to the smallest responsible commit and preserve its evidence. Do not claim the goal of an unbroken application is achieved until P11/P12 acceptance has actually completed.

Human review:
