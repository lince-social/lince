== Kanban

This document is the Kanban-specific creation and implementation specification.

It is intentionally separate from the general web component specification.
Read `web_components.typ` first for:

- host/runtime assumptions
- endpoint contracts
- Sand Classes
- sidecar-table strategy

=== Purpose

This specification defines the official Record Kanban for the web version.

The target is:

- Record-centric
- fed by a view SSE source
- persisted per widget through host `widgetState`
- authenticated by the host-owned web login
- implemented as `Engineer with Clown traits`

=== Chosen class

The official Kanban should be:

- `Engineer with Clown traits`

Reason:

- server-owned board structure matters
- per-card UI state matters
- drag and drop matters
- resize matters
- connection/auth clarity matters
- local display ergonomics matter

Pure `Engineer` is too rigid for the amount of local interaction state.
Pure `Clown` pushes too much structure into the browser.
`Monk` makes JS the default rendering model.
`Mercenary` is not the center of gravity for a Lince-native Kanban.
`Astromancer` is the wrong class for a DOM-first board.

=== Responsibility split

Normal center of gravity:

- Maud: main structural owner
- Datastar: local UI state, status, selected stable-shell reactivity
- JavaScript: drag/drop, resize, focus-sensitive editing, and scroll behavior that needs imperative handling

Normal transport split:

- HTML fragments from backend or official widget stream: primary
- signal patches: secondary
- raw JSON SSE: only as narrow fallback during transition

=== Runtime contract

Preferred runtime direction:

```txt
GET  /host/widgets/{instance_id}/stream
GET  /host/widgets/{instance_id}/contract
POST /host/widgets/{instance_id}/actions/{action}
```

Do not keep the final official Kanban limited to direct consumption of:

```txt
/host/integrations/servers/{server_id}/views/{view_id}/stream
```

The instance-aware runtime should:

- resolve the widget instance
- read `serverId`, `viewId`, `widgetState`, and stream state
- subscribe internally to backend data
- validate a record-centric contract
- render Maud fragments
- emit Datastar fragment and signal patches

=== Data contract

==== Required columns

The Kanban accepts record-centric view projections.

Minimum required core:

- `id`
- `quantity`
- `head`
- `body`

If those are not present, the widget must reject the input with a short mismatch state.

The widget does not need to enforce physical single-table origin.
It should instead enforce a normalized record-centric contract.

==== Optional columns

If present, these should be consumed:

- `primary_category`
- `categories_json`
- `start_at`
- `end_at`
- `estimate_seconds`
- `actual_seconds`
- `assignee_ids_json`
- `assignee_names_json`
- `parent_id`
- `parent_head`
- `depth`
- `children_count`
- `children_json`
- `task_type`
- `comments_count`
- `last_comment_preview`
- `active_worklog_count`

If absent:

- the board still functions with core Record behavior
- dependent UI should hide cleanly
- the widget must not invent values

==== Current schema truth

Current stable Record truth is still:

- `id`
- `quantity`
- `head`
- `body`

That means:

- full CRUD for `head`, `body`, and `quantity` is already conceptually valid
- richer fields require view projection and recommended sidecar tables

==== Record-centric validation

The validation rule should be:

- the projection must expose the required Record core
- optional task metadata may come from joins, sidecars, aggregates, or relations
- the widget should prefer server-declared contract metadata when available
- if an official `/contract` route exists, it should be treated as the preferred validation source

The rule should not be:

- only one physical table is allowed

==== Normalized projection contract

Preferred normalized fields and types:

- `id`: integer
- `quantity`: integer
- `head`: text
- `body`: text
- `primary_category`: nullable text, used for compact badges when needed
- `categories_json`: JSON-encoded array of strings, representing all task categories in v1
- `start_at`: nullable UTC datetime text in `YYYY-MM-DDTHH:MM:SSZ`
- `end_at`: nullable UTC datetime text in `YYYY-MM-DDTHH:MM:SSZ`
- `estimate_seconds`: nullable integer
- `actual_seconds`: nullable integer derived from worklog intervals
- `assignee_ids_json`: JSON-encoded array of integer `app_user.id` values
- `assignee_names_json`: JSON-encoded array of display names aligned with assignee ids
- `parent_id`: nullable integer
- `parent_head`: nullable text
- `depth`: nullable integer
- `children_count`: nullable integer
- `children_json`: nullable JSON-encoded array of summary objects shaped as `{ "id": int, "head": string, "quantity": int, "task_type": string | null }`
- `task_type`: nullable enum of `epic`, `feature`, `task`, or `other`
- `comments_count`: nullable integer
- `last_comment_preview`: nullable text
- `active_worklog_count`: nullable integer counting all currently open intervals for the task across all users

Rules:

- arrays should prefer JSON text in v1 because SQL view projections are easier to keep stable that way
- category cardinality in v1 is many, not one
- `task_type` labels shown in the UI may be capitalized, but wire values should stay lowercase
- `actual_seconds` must not be edited directly; edit the intervals instead
- summary-level `actual_seconds` may lag behind currently running timers until the next heartbeat or detail refresh
- focused detail may add live client-side elapsed time on top of persisted or heartbeated interval totals
- if `/contract` is available, these fields should be declared there explicitly

Task inclusion rule:

- records appear in Kanban when `task_type` is present and valid
- preferred valid values are `epic`, `feature`, `task`, and `other`
- records with missing or empty `task_type` should not appear in the Kanban by default
- setting a valid `task_type` on a normal `record` is enough to make that record Kanban-ready
- categories are optional labels and are not required for Kanban inclusion

Category runtime rule:

- preserve category display casing and insertion order
- compare categories case-insensitively for filtering and matching

Task-type evolution rule:

- if the allowed task-type enum changes in a future release, migrate persisted `task.type` rows with an explicit SQL migration
- do not silently reinterpret unknown historic values at runtime
- prefer rewriting old values to new valid enum values or `NULL`, then updating the normalized Kanban views accordingly

Old-data migration policy:

- schema migration and old-data backfill are part of Lince, not widget-only behavior
- first implementation may ship with schema migrations only and no forced backfill of old records
- historic records without `task_type` remain valid `record` rows but do not appear in Kanban until a valid `task_type` is set
- the widget should not guess task types for legacy records on its own
- no bulk task-type action is required for v1

==== Recommended normalized view shape

The Kanban should normally consume a saved backend view whose query already projects the normalized record-centric shape.

The recommended direction is:

- one base saved view defines the dataset and normalized columns
- widget-local filter state is edited in the GUI and persisted in structured form in `widgetState`
- the internal widget stream service applies those filters by deriving SQL executed in SQLite from structured filter state
- the widget should not receive thousands of irrelevant rows just to filter them in JavaScript or Rust after the query runs

The widget should not treat user-edited SQL as the first implementation path.

Filter persistence scope:

- v1 persists structured filter clauses per widget instance in `widgetState`
- the host derives the effective SQL from those clauses when opening the Kanban stream
- the GUI should be populated from the structured clauses, not by reverse-parsing SQL
- do not mutate the saved base `view.query` whenever one Kanban instance changes filters
- do not store only a raw SQL filter string if the GUI must later reconstruct the applied filters

Future expansion:

- named presets or shared filtered boards are not required in v1
- if they become necessary later, add a dedicated persisted filter or preset model
- do not overload `view.query` with per-widget temporary filters

==== Example normalized SQL direction

The following is a recommended shape for a normalized Kanban view under the current sidecar plan:

```sql
SELECT
    r.id,
    r.quantity,
    r.head,
    r.body,
    json_extract(categories_ext.data_json, '$.categories[0]') AS primary_category,
    json_extract(categories_ext.data_json, '$.categories') AS categories_json,
    json_extract(schedule_ext.data_json, '$.start_at') AS start_at,
    json_extract(schedule_ext.data_json, '$.end_at') AS end_at,
    json_extract(effort_ext.data_json, '$.estimate_seconds') AS estimate_seconds,
    COALESCE(worklog_sum.actual_seconds, 0) AS actual_seconds,
    COALESCE(worklog_sum.active_worklog_count, 0) AS active_worklog_count,
    assignee_sum.assignee_ids_json,
    assignee_sum.assignee_names_json,
    parent_rel.parent_id,
    parent_record.head AS parent_head,
    child_sum.children_count,
    child_sum.children_json,
    json_extract(type_ext.data_json, '$.task_type') AS task_type,
    COALESCE(comment_sum.comments_count, 0) AS comments_count,
    comment_sum.last_comment_preview
FROM record r
LEFT JOIN record_extension categories_ext
    ON categories_ext.record_id = r.id
   AND categories_ext.namespace = 'task.categories'
LEFT JOIN record_extension schedule_ext
    ON schedule_ext.record_id = r.id
   AND schedule_ext.namespace = 'task.schedule'
LEFT JOIN record_extension effort_ext
    ON effort_ext.record_id = r.id
   AND effort_ext.namespace = 'task.effort'
LEFT JOIN record_extension type_ext
    ON type_ext.record_id = r.id
   AND type_ext.namespace = 'task.type'
LEFT JOIN (
    SELECT
        rl.record_id,
        json_group_array(rl.target_id) AS assignee_ids_json,
        json_group_array(u.name) AS assignee_names_json
    FROM record_link rl
    JOIN app_user u
      ON u.id = rl.target_id
    WHERE rl.link_type = 'assigned_to'
      AND rl.target_table = 'app_user'
    GROUP BY rl.record_id
) assignee_sum
    ON assignee_sum.record_id = r.id
LEFT JOIN (
    SELECT
        rl.record_id,
        MAX(rl.target_id) AS parent_id
    FROM record_link rl
    WHERE rl.link_type = 'parent'
      AND rl.target_table = 'record'
    GROUP BY rl.record_id
) parent_rel
    ON parent_rel.record_id = r.id
LEFT JOIN record parent_record
    ON parent_record.id = parent_rel.parent_id
LEFT JOIN (
    SELECT
        rl.target_id AS parent_id,
        COUNT(*) AS children_count,
        json_group_array(
            json_object(
                'id', child.id,
                'head', child.head,
                'quantity', child.quantity,
                'task_type', json_extract(child_type.data_json, '$.task_type')
            )
        ) AS children_json
    FROM record_link rl
    JOIN record child
      ON child.id = rl.record_id
    LEFT JOIN record_extension child_type
      ON child_type.record_id = child.id
     AND child_type.namespace = 'task.type'
    WHERE rl.link_type = 'parent'
      AND rl.target_table = 'record'
    GROUP BY rl.target_id
) child_sum
    ON child_sum.parent_id = r.id
LEFT JOIN (
    SELECT
        rw.record_id,
        CAST(SUM(
            CASE
                WHEN rw.ended_at IS NOT NULL
                    THEN COALESCE(rw.seconds, strftime('%s', rw.ended_at) - strftime('%s', rw.started_at))
                ELSE COALESCE(rw.seconds, 0)
            END
        ) AS INTEGER) AS actual_seconds,
        SUM(CASE WHEN rw.ended_at IS NULL THEN 1 ELSE 0 END) AS active_worklog_count
    FROM record_worklog rw
    GROUP BY rw.record_id
) worklog_sum
    ON worklog_sum.record_id = r.id
LEFT JOIN (
    SELECT
        rc.record_id,
        COUNT(*) AS comments_count,
        (
            SELECT rc2.body
            FROM record_comment rc2
            WHERE rc2.record_id = rc.record_id
              AND rc2.deleted_at IS NULL
            ORDER BY rc2.created_at DESC, rc2.id DESC
            LIMIT 1
        ) AS last_comment_preview
    FROM record_comment rc
    WHERE rc.deleted_at IS NULL
    GROUP BY rc.record_id
) comment_sum
    ON comment_sum.record_id = r.id
WHERE json_extract(type_ext.data_json, '$.task_type') IN ('epic', 'feature', 'task', 'other');
```

Critique of older query shapes:

- do not use `task.bucket`; use `task.categories`
- do not use scalar assignment metadata when assignees are real `app_user` relations
- do not use `MAX(body)` to mean latest comment preview; order by comment timestamps instead
- do not divide into `actual_hours` in storage or wire contracts; keep seconds canonical and format in the UI
- do not pretend hierarchy `depth` exists if the view is not actually deriving it

If hierarchy depth is not implemented, it should be `NULL` or omitted rather than faked.

==== Filter strategy

The Kanban should support structured GUI filters.

Preferred v1 model:

- a saved base view defines the broad dataset
- widget-local filters are stored in `widgetState`
- every filter row created in the GUI is combined with the others using logical `AND`
- multi-value filters inside one row use logical `OR`
- the internal widget stream service derives a filtered SQL wrapper around the base query
- the widget should reconnect the official stream after filter changes
- the base saved view SQL is not edited as raw text in the widget UI

Preferred v1 filter fields:

- `text_query`
- `categories_any_json`
- `assignee_ids_any_json`
- `quantities_json`
- `task_types_json`
- `only_with_open_worklog`

Filter semantics:

- `text_query`: case-insensitive match against `head` and optionally `body`
- `categories_any_json`: match if the record has any of the selected categories
- `assignee_ids_any_json`: match if the record has any of the selected assignees
- `quantities_json`: match explicit Kanban columns
- `task_types_json`: match selected task-type enum values
- `only_with_open_worklog`: restrict to records with `active_worklog_count > 0`

Default dataset rule:

- prefer `task_type IN ('epic', 'feature', 'task', 'other')` as the default inclusion rule for Kanban records
- do not require a redundant `"Task"` category when `task_type` already encodes that meaning
- use categories for project, team, or domain labels such as `project-1`, `project-2`, or `design`

Runtime readiness rule:

- any normal `record` can become Kanban-visible at runtime by setting a valid `task_type`
- no additional schema conversion is required for that record to enter the Kanban dataset
- categories remain optional and are mainly useful for filtering, grouping, and badges

Recommended derived SQL direction:

```sql
SELECT *
FROM (
    /* base kanban view query */
) base
WHERE 1 = 1
  AND (
      :text_query IS NULL
      OR lower(base.head) LIKE '%' || lower(:text_query) || '%'
      OR lower(base.body) LIKE '%' || lower(:text_query) || '%'
  )
  AND (
      :only_with_open_worklog = 0
      OR COALESCE(base.active_worklog_count, 0) > 0
  )
  /* quantities_json => base.quantity IN (...) */
  /* task_types_json => base.task_type IN (...) */
  /* categories_any_json => EXISTS (SELECT 1 FROM json_each(COALESCE(base.categories_json, '[]')) c WHERE c.value IN (...)) */
  /* assignee_ids_any_json => EXISTS (SELECT 1 FROM json_each(COALESCE(base.assignee_ids_json, '[]')) a WHERE CAST(a.value AS INTEGER) IN (...)) */
ORDER BY base.quantity ASC, lower(base.head) ASC, base.id DESC;
```

Implementation rules:

- build the filter SQL on the host from structured filter clauses, not by accepting raw SQL fragments from the widget
- execute the derived filter query in SQLite
- do not stream a superset and then filter rows in Rust
- persist the filter clauses in `widgetState`
- if desired for debugging, the host may expose the effective SQL string in `/contract` or diagnostics, but it should remain derived data
- `apply-filters` should persist the new filters and increment a filters-version marker
- after `apply-filters`, the widget should reconnect `/host/widgets/{instance_id}/stream`
- the new stream connection should read the latest persisted filters and start a new filtered subscription generation
- use parameterized values when building the filtered query wrapper

=== Host services

The official Kanban should rely on explicit host-side services rather than pushing orchestration into the widget.

Preferred host service set:

- `KanbanContractService`
- `KanbanStreamService`
- `KanbanFilterService`
- `KanbanDetailService`
- `KanbanActionService`
- `KanbanWorklogService`
- `KanbanLivenessService`

`KanbanContractService`

- resolves widget instance metadata
- validates that the configured base view can produce the normalized Kanban contract
- returns `/host/widgets/{instance_id}/contract`
- may expose effective SQL for debugging, but not as the primary GUI persistence format

`KanbanStreamService`

- opens the instance-aware stream
- resolves the configured server and base view
- asks `KanbanFilterService` for the current filtered SQL wrapper
- subscribes to the filtered view stream
- emits Datastar fragments and signals or official event shapes for the widget

`KanbanFilterService`

- reads structured filter clauses from `widgetState`
- validates the filter rows and operators
- derives a parameterized SQL wrapper around the base view query
- increments and tracks `filters_version`
- is the authority for filter persistence and filter-to-SQL translation

`KanbanDetailService`

- loads the focused record detail by `record_id`
- returns the focused card HTML fragment and detail payload
- includes comments, child summaries, resources, assignees, and worklog interval detail

`KanbanActionService`

- handles semantic actions such as:
  - `create-record`
  - `update-record`
  - `move-record`
  - `delete-record`
  - `apply-filters`
- coordinates sidecar writes and relation writes when needed
- should own normal task-type and category CRUD as part of record create or update flows
- should not require a dedicated service per simple metadata field in v1
- should reject invalid hierarchy writes such as self-parent and obvious cycles
- last write wins is acceptable for concurrent metadata edits in v1

`KanbanWorklogService`

- handles `start-worklog` and `stop-worklog`
- enforces one open interval per `(record_id, author_user_id)`
- updates `last_heartbeat_at`
- computes or refreshes summary worklog totals when needed

`KanbanLivenessService`

- emits heartbeat or ping events for the official stream
- tracks stale and disconnected thresholds
- distinguishes paused updates from disabled transport

Service boundary decision:

- use one combined `KanbanActionService` for Record create, edit, delete, task-type edits, and category edits
- use one dedicated `KanbanFilterService` for filter validation, persistence, SQL derivation, and filter-version tracking
- do not create separate `TaskTypeService` or `CategoryService` in v1 unless they later acquire materially different business rules

=== Quantity mapping

Current Kanban column mapping must stay compatible:

- `0` => Backlog
- `-1` => Next
- `-2` => WIP
- `-3` => Review
- `1` => Done

This mapping is odd, but should remain for compatibility unless a broader migration is chosen.

=== Shell structure

The Kanban shell should be stable and Maud-owned.

Stable regions:

- widget header
- connection and action toolbar
- board viewport
- optional footer or status region

Preferred fragment patch targets:

- `#kanban-header-meta`
- `#kanban-toolbar-state`
- `#kanban-columns`
- `#kanban-empty-or-error`
- `#kanban-create-sheet`
- `#kanban-edit-sheet`

Preferred signal-owned state:

- connection badge state
- paused or running state
- live connection enabled or disabled state
- global body display mode
- per-card body display mode
- focused card mode
- collapsed columns
- column widths
- active sheet or drawer
- local filters
- local drafts that are safe to persist

=== Technology ownership

==== Maud

Maud should own:

- shell
- loading state
- locked state
- mismatch state
- compact error state
- header and board structure
- columns and cards
- create and edit surfaces
- empty states

==== Datastar

Datastar should own:

- local UI signals
- connection/auth/runtime indicators
- collapse state
- body mode state
- small stable-shell reactivity
- signal-driven persistence glue into `widgetState`

==== JavaScript

JavaScript should own:

- drag and drop between columns
- pointer-driven column resize
- focus-sensitive editing
- imperative scroll behavior
- optimistic move choreography where needed

=== Persistence

Persist runtime ergonomics in `widgetState` through the host:

- column widths
- collapsed columns
- global body display mode
- per-card body display mode
- focused card identifier when applicable
- local filters
- stream paused preference

Do not persist business truth in `widgetState`.

Do not persist widget ergonomics in sidecar business tables.

Persistence should stay sparse:

- persist global defaults first
- persist per-card overrides only when they differ from the default

This remains the preferred model even if board state is later split into multiple files or directories.

=== Card content model

Core card content:

- `head` as title
- `body` as description
- `quantity` as Kanban status

Optional content extensions:

- categories
- dates
- estimate
- actual work
- assignees
- hierarchy
- comments
- task type
- parent label

Important distinction:

- task categories are task metadata used for grouping or categorization inside Kanban semantics
- bucket image references are external asset references to files or objects stored in a bucket
- these are different concepts and should not reuse the same storage model

Body display modes per card:

- hidden
- excerpt
- full

Focused card mode:

- a selected card may take over the component as a dedicated focus surface with a way back to the board
- this should be treated as focus mode, not only as another inline body mode

There should also be a global control to switch body display mode across the board.

=== Frontend plan

==== Task type and category CRUD

Preferred surfaces:

- create sheet for new records
- edit sheet for existing records
- focused card mode for richer editing context

Preferred controls:

- `task_type`: single-select control with values `epic`, `feature`, `task`, `other`
- `categories`: multi-value chip or token input

Preferred frontend behavior:

- create sheet should require `head` and allow optional `body`
- create sheet should default `task_type` to empty
- create sheet should allow setting `task_type` immediately so the new record can become Kanban-ready at creation time
- edit sheet should allow changing `task_type`
- edit sheet should allow adding and removing categories
- category chips should preserve insertion order in v1 and do not need sorting
- if `task_type` is cleared, show a short warning that the record will disappear from the Kanban after save
- category edits should update badges and category filter availability on the next stream refresh
- when all categories are removed, the preferred write behavior is to remove the `task.categories` sidecar row instead of storing an empty array

Preferred action usage:

- use `create-record` for new records with optional `task_type` and `categories`
- use `update-record` for task-type and category edits
- do not add `set-task-type` or `set-categories` actions in v1

==== Filter builder GUI

Preferred placement:

- a toolbar button opens a dedicated filter drawer or sheet
- active filters should also be visible as compact chips in the header or toolbar

Preferred v1 filter-builder model:

- each row in the GUI is an `AND` clause
- multi-value selection inside a row is an `OR`
- the filter builder edits structured filter rows, not SQL text
- filters are per widget instance and are not shareable in v1
- full text search should use case-insensitive `LIKE` in v1; FTS is not required

Preferred filter-builder row types:

- text contains
- categories any-of
- assignees any-of
- task types any-of
- quantities any-of
- only with open worklog

Preferred frontend behavior:

- editing filters does not mutate the saved base `view.query`
- applying filters persists the structured rows into `widgetState`
- after save, the widget calls `apply-filters` and reconnects the official stream
- the header should show the currently active filters in human-readable form
- the drawer should be able to reconstruct itself from persisted structured filter rows
- provide a clear-all action
- provide per-row remove actions

Preferred validation:

- do not allow empty filter rows to persist
- do not allow unknown filter fields or operators
- normalize multi-value rows to unique values before persisting

=== CRUD model

==== Create

Create flow should support:

- creating the core `record`
- optionally writing `record_extension`
- optionally writing `record_link`

Prefer a semantic action endpoint when available, but do not block the first implementation on it.
Generic writes for `record`, `record_extension`, and `record_link` are acceptable for v1.

==== Edit

Edit flow should support:

- editing `head`
- editing `body`
- editing supported optional metadata when present

Task metadata CRUD in v1 should be handled through the same create and update surfaces as the core Record.

Recommended v1 rule:

- `task_type` and `categories` are edited through the normal Kanban create or edit sheet
- do not create a separate dedicated CRUD surface only for task type or categories in the first implementation
- clearing `task_type` is allowed and removes the record from Kanban visibility after stream refresh
- clearing all categories is allowed and does not remove the record from Kanban visibility

==== Move

Move flow should:

- patch `record.quantity`
- update UI optimistically if useful
- reconcile against server truth

==== Delete

Delete flow should delete the core `record`.

If sidecar tables are implemented with cascading deletes, that should remove:

- `record_extension`
- `record_link`
- `record_comment`
- `record_worklog`

=== Optional feature mapping

Use the recommended sidecar strategy from `web_components.typ`.

Mapping:

- task categories => `record_extension` namespace `task.categories`
- start/end date => `record_extension` namespace `task.schedule`
- estimate => `record_extension` namespace `task.effort`
- task type => `record_extension` namespace `task.type`
- assignees => `record_link` to `app_user`, many-to-many using `assigned_to`
- hierarchy => `record_link` using `parent`
- comments => `record_comment`
- accumulated real work => `record_worklog`

Bucket image references:

- if records need queryable reusable bucket image references, use `record_resource_ref`
- do not make body parsing the canonical attachment model
- do not overload `task.categories` with bucket-object references
- use `provider = "bucket"` and `resource_kind = "image"` in v1
- keep these references as detail-oriented data, not mandatory board-summary payload

=== Proposed SQLite migrations

The Kanban plan assumes the following tables and indexes exist.

Core sidecars:

```sql
CREATE TABLE record_extension (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    data_json TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, namespace),
    CHECK (json_valid(data_json))
);

CREATE INDEX idx_record_extension_namespace_record
ON record_extension(namespace, record_id);

CREATE TABLE record_link (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL,
    target_table TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    position REAL,
    data_json TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, link_type, target_table, target_id),
    CHECK (data_json IS NULL OR json_valid(data_json))
);

CREATE INDEX idx_record_link_record_type
ON record_link(record_id, link_type, target_table);

CREATE INDEX idx_record_link_target_type
ON record_link(target_table, target_id, link_type);

CREATE TABLE record_comment (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL,
    body TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME
);

CREATE INDEX idx_record_comment_record_created
ON record_comment(record_id, created_at DESC);

CREATE TABLE record_worklog (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    started_at DATETIME NOT NULL,
    ended_at DATETIME,
    last_heartbeat_at DATETIME,
    seconds REAL,
    note TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_record_worklog_record_started
ON record_worklog(record_id, started_at DESC);

CREATE INDEX idx_record_worklog_author_started
ON record_worklog(author_user_id, started_at DESC);

CREATE UNIQUE INDEX idx_record_worklog_one_open_interval
ON record_worklog(record_id, author_user_id)
WHERE ended_at IS NULL;

CREATE TABLE record_resource_ref (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_path TEXT NOT NULL,
    title TEXT,
    position REAL,
    data_json TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, provider, resource_path),
    CHECK (data_json IS NULL OR json_valid(data_json))
);

CREATE INDEX idx_record_resource_ref_record_position
ON record_resource_ref(record_id, position, id);
```

Recommended namespace payloads:

```json
task.categories => { "categories": ["project-1", "design"] }
task.schedule => { "start_at": "2026-03-28T12:00:00Z", "end_at": null }
task.effort => { "estimate_seconds": 14400 }
task.type => { "task_type": "feature" }
```

=== Worklog model

Worklog should use a dedicated table, not sidecar JSON arrays.

Recommended model:

- one row per interval
- intervals are per `app_user`
- many different users may have active intervals on the same record at the same time
- one user should not have more than one open interval for the same record at the same time
- `actual` is derived from interval sums
- `started_at` and `ended_at` are canonical interval truth
- `seconds` is derived or cached only, never a competing source of truth

Current-user resolution for worklog authorship in v1:

- the host resolves a stable current `app_user.id` and caches it in widget runtime state under `kanban_runtime.current_user_id`
- if the server session exposes a stable `username_hint`, prefer matching that to `app_user.username`
- if no username hint can be resolved in the current host runtime, fall back deterministically to the smallest available `app_user.id`
- widgets do not send arbitrary `author_user_id` values for normal start or stop worklog flows

Canonical interval fields:

- `record_id`
- `author_user_id`
- `started_at`
- `ended_at`
- `last_heartbeat_at`
- optional `note`

Running-time behavior:

- frontend shows live elapsed time while an interval is active
- backend writes should not happen every second
- coarse heartbeat writes such as every 5 minutes are acceptable for crash recovery

Enforcement:

- enforce the one-open-interval-per-`(record_id, author_user_id)` rule in both service logic and database indexing
- treat `ended_at IS NULL` as the definition of an open interval
- do not treat `seconds` as a separately editable source of truth

Offline stop queue storage:

- persist queued offline stops in host `widgetState`, namespaced under a dedicated key such as `pending_worklog_stops`
- queue entries should include only the known interval id and intended `ended_at`
- replay on reconnect and prune successful entries immediately
- do not promise full offline timer start in v1

Offline stop behavior:

- queue offline stop only for a known active interval that was already started online
- capture the intended `ended_at` locally and replay when connectivity returns
- full offline start is not required for the first implementation

=== Summary and detail loading

The board should prefer a summary projection for normal board rendering.

Preferred board payload:

- core Record fields
- category, dates, task type, hierarchy summary
- assignee summaries
- comments count or preview
- worklog summary

Preferred detail loading:

- full comments
- full attachment or bucket-image references
- richer edit surfaces
- any heavy card detail not needed for normal board rendering

This means:

- stream summary data for the board
- fetch or patch focused detail lazily when needed

Preferred focused-detail action:

- `POST /host/widgets/{instance_id}/actions/load-record-detail`

Preferred focused-detail request:

```json
{
  "record_id": 123
}
```

Preferred focused-detail response:

```json
{
  "record_id": 123,
  "target": "#kanban-focus-card",
  "mode": "replace",
  "html": "<section>...</section>",
  "signals": {
    "focusedRecordId": 123,
    "focusMode": true
  },
  "detail": {
    "primary_category": "project-1",
    "categories": ["project-1", "design"],
    "assignees": [
      { "id": 7, "name": "Ana" }
    ],
    "comments": [
      {
        "id": 88,
        "author_user_id": 7,
        "body": "Full comment body",
        "created_at": "2026-03-28T12:00:00Z",
        "updated_at": "2026-03-28T12:00:00Z"
      }
    ],
    "children": [
      {
        "id": 456,
        "head": "Child task"
      }
    ],
    "resources": [
      {
        "id": 12,
        "provider": "bucket",
        "resource_kind": "image",
        "resource_path": "bucket/path/image.png",
        "title": "Screenshot"
      }
    ],
    "worklog": {
      "actual_seconds": 14400,
      "active_worklog_count": 2,
      "intervals": [
        {
          "id": 991,
          "author_user_id": 7,
          "started_at": "2026-03-28T10:00:00Z",
          "ended_at": null,
          "last_heartbeat_at": "2026-03-28T12:00:00Z",
          "seconds": 7200,
          "note": null
        }
      ]
    }
  }
}
```

Response rules:

- `html` is the primary render surface for focus mode
- `detail` exists to support imperative interactions and small local computations without reparsing DOM
- `signals` should remain minimal and only express focus-state transitions

=== Action contract

For semantic Kanban actions, prefer one common response envelope.

Preferred success envelope:

```json
{
  "ok": true,
  "action": "move-record",
  "message": "Record moved.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": null
}
```

Preferred failure envelope:

```json
{
  "ok": false,
  "action": "move-record",
  "message": "Unable to move record.",
  "code": "validation_error"
}
```

Action payloads:

`create-record`

Request:

```json
{
  "record": {
    "head": "New task",
    "body": "Longer description",
    "quantity": 0
  },
  "task_type": "task",
  "categories": ["project-1", "design"],
  "start_at": null,
  "end_at": null,
  "estimate_seconds": 14400,
  "assignee_ids": [7, 9],
  "parent_id": null
}
```

Response:

```json
{
  "ok": true,
  "action": "create-record",
  "message": "Record created.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": {
    "focus_record_id": 123
  }
}
```

`update-record`

Request:

```json
{
  "record_id": 123,
  "head": "Updated title",
  "body": "Updated body",
  "quantity": -1,
  "task_type": "feature",
  "categories": ["project-1"],
  "start_at": "2026-03-28T12:00:00Z",
  "end_at": null,
  "estimate_seconds": 21600,
  "assignee_ids": [7],
  "parent_id": 45
}
```

Response:

```json
{
  "ok": true,
  "action": "update-record",
  "message": "Record updated.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": null
}
```

`move-record`

Request:

```json
{
  "record_id": 123,
  "quantity": -2
}
```

Response:

```json
{
  "ok": true,
  "action": "move-record",
  "message": "Record moved.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": {
    "quantity": -2
  }
}
```

`delete-record`

Request:

```json
{
  "record_id": 123
}
```

Response:

```json
{
  "ok": true,
  "action": "delete-record",
  "message": "Record deleted.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": null
}
```

`start-worklog`

Request:

```json
{
  "record_id": 123,
  "note": null
}
```

Response:

```json
{
  "ok": true,
  "action": "start-worklog",
  "message": "Worklog started.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": {
    "interval": {
      "id": 991,
      "author_user_id": 7,
      "started_at": "2026-03-28T10:00:00Z",
      "ended_at": null,
      "last_heartbeat_at": "2026-03-28T10:00:00Z",
      "seconds": 0,
      "note": null
    }
  }
}
```

`stop-worklog`

Request:

```json
{
  "record_id": 123,
  "interval_id": 991,
  "ended_at": "2026-03-28T12:00:00Z"
}
```

Response:

```json
{
  "ok": true,
  "action": "stop-worklog",
  "message": "Worklog stopped.",
  "record_id": 123,
  "await_stream_refresh": true,
  "detail": {
    "interval_id": 991,
    "actual_seconds": 14400
  }
}
```

`apply-filters`

Request:

```json
{
  "filters": [
    { "field": "categories_any_json", "operator": "any_of", "value": ["project-1"] },
    { "field": "assignee_ids_any_json", "operator": "any_of", "value": [7, 9] },
    { "field": "task_types_json", "operator": "any_of", "value": ["feature", "task"] },
    { "field": "quantities_json", "operator": "any_of", "value": [0, -1, -2] },
    { "field": "text_query", "operator": "contains", "value": "search term" }
  ]
}
```

Response:

```json
{
  "ok": true,
  "action": "apply-filters",
  "message": "Filters applied.",
  "record_id": null,
  "await_stream_refresh": true,
  "detail": {
    "filters_version": 2
  }
}
```

`apply-filters` rules:

- the response means the filter state was persisted, not that the old stream was mutated in place
- the widget should open a new stream connection after success
- the new stream must be backed by SQL-filtered results, not Rust-side row filtering

Preferred focused-detail responsibilities:

- render the focused card surface into `#kanban-focus-card`
- include full comments
- include bucket image references
- include child task summaries using `children_json`
- preserve board scroll and column scroll positions while focus mode is open

=== Connection and auth behavior

The board must clearly communicate:

- whether SSE is connected
- whether stream delivery is paused
- whether live connection is disabled
- whether auth has fallen
- whether host configuration is missing

Rules:

- do not implement widget-local login UI for the standard host-mediated flow
- use the general web login
- if auth is missing, show locked or waiting state
- if host config is missing, show misconfigured state
- if `meta.streams.enabled` is `false`, stop reconnecting until re-enabled

The runtime should separate:

- paused updates
- live transport enabled or disabled
- disconnected or stale connection

Heartbeat or explicit liveness signaling is preferred over purely heuristic last-message guesses.

Preferred liveness timing:

- emit a stream heartbeat or ping every 15 seconds when no snapshot has been sent
- mark the connection stale after 45 seconds without snapshot or heartbeat
- mark the connection reconnecting or disconnected after 90 seconds without snapshot or heartbeat

Required controls:

- pause receiving SSE data
- toggle connection or reception behavior if the runtime supports it

=== Layout and scrolling

The board should:

- occupy the full height of the component
- allow horizontal scrolling when columns overflow
- allow internal component scrolling like a page
- allow individual column scrolling

Column behavior should support:

- per-column scroll
- resize by dragging
- collapse or minimize

=== Hierarchy behavior

Hierarchy is metadata-first in the first implementation.

Rules:

- child and parent may exist in different columns
- moving a parent does not move children
- indenting is not required
- show the parent relation as a clickable label or chip inside the card in every body mode
- the same parent link should remain clickable in focus mode
- focused mode should also show child summaries with `head` only and allow clicking into another focused card
- task type color is separate from hierarchy relation
- self-parent is invalid
- parent links that would create a cycle are invalid

Grouping subtrees may come later, but is not required for the first implementation.

Meaning of metadata-first rather than tree-first:

- store and expose parent-child relations as normal metadata
- show parent and child navigation hints in cards and focus mode
- do not make recursive layout, nested rendering, subtree drag, or cascade moves part of the first implementation
- do not compute or rely on deep hierarchy behavior unless a later phase explicitly adds it

=== Comments behavior

Comments should use a summary-versus-detail model.

Board-level behavior:

- use `comments_count`
- optionally show one preview

Focused-detail behavior:

- load or patch full comments lazily
- do not stream full comment threads into the main board payload by default
- comments should remain in `record_comment`, not child `record` rows, for the first implementation
- focused mode may show full comment bodies and timestamps

=== Permissions and actions

Preferred v1 permission model:

- `read_view_stream` for summary board streaming
- `write_records` for core Record create, edit, move, and delete
- `write_table` for `record_extension`, `record_link`, `record_comment`, `record_worklog`, and `record_resource_ref`

Do not require finer-grained per-feature permissions in v1 unless the host already exposes them.

Preferred v1 semantic actions:

- `load-record-detail`
- `create-record`
- `update-record`
- `move-record`
- `delete-record`
- `start-worklog`
- `stop-worklog`
- `apply-filters`

Generic table writes remain acceptable when semantic actions do not yet exist.

=== UX states

Required explicit states:

- loading
- empty
- mismatch
- locked/auth missing
- misconfigured
- live
- paused
- focus mode
- reconnecting or disconnected
- stale if heartbeat or liveness expires

=== Patch strategy

HTML fragment patches are preferred for:

- columns
- card lists
- empty/error panels
- create/edit surfaces
- structural board updates

Signal patches are preferred for:

- connection state
- pause state
- body mode state
- collapse state
- width metadata
- header/toolbar status

Rules:

- do not patch the entire document
- do not stream script tags as normal update fragments
- patch stable inner targets
- keep JS loaded once, then patch HTML and signals around it

=== Functional checklist

Must support:

- host `widgetState` persistence
- SSE view data consumption
- record-centric validation
- CRUD on Record
- quantity-based column mapping
- full-height component layout
- horizontal board scroll
- internal component scroll
- per-column scroll
- per-column resize
- column collapse
- per-card body mode
- global body mode
- focused card mode
- pause/reconnect/live state visibility
- backend/auth failure visibility
- delete cards

Schema-dependent advanced features:

- comments
- hierarchy
- assignees through `app_user`
- category metadata
- dates
- estimate and actual work

=== Implementation step checklist

Each step below is intended to be testable before moving to the next one.

Current status convention:

- `implemented in code`: the required code path exists and compiles
- `runtime verification pending`: the feature still needs authenticated browser testing in the real widget flow
- `final polish pending`: the architecture is in place, but some UX cleanup or simplification still remains

- [x] Step 1: Freeze the implementation contract

Requirements:

- the normalized Kanban projection fields are frozen
- the task-type enum is frozen
- the semantic action payloads and response envelopes are frozen
- the `/contract` payload is frozen
- the SQL-side filter model is frozen
- the liveness thresholds are frozen

Step 1 passes when:

- no open architecture questions remain in this document for core Kanban behavior
- implementation can begin without redefining data shapes

- [x] Step 2: Add database migrations

Requirements:

- add `record_extension`
- add `record_link`
- add `record_comment`
- add `record_worklog`
- add `record_resource_ref`
- add the required indexes and unique constraints

Step 2 passes when:

- migrations apply cleanly to a fresh database
- migrations apply cleanly to an existing database
- the one-open-worklog invariant is enforced at the DB level

- [x] Step 3: Validate data-layer behavior

Requirements:

- generic CRUD works for the new tables
- `app_user` relations work for assignees
- `task.type` and `task.categories` writes behave as specified
- removing all categories removes the `task.categories` sidecar row
- hierarchy writes reject self-parent and obvious cycles

Step 3 passes when:

- manual table writes and reads produce the expected normalized data
- invalid hierarchy writes are rejected

- [x] Step 4: Implement host contract service

Requirements:

- implement `KanbanContractService`
- `/host/widgets/{instance_id}/contract` resolves widget instance metadata
- the contract route exposes required and optional fields
- the contract route exposes permissions, liveness thresholds, and relation declarations

Step 4 passes when:

- a configured widget instance returns a stable contract response
- a misconfigured widget instance returns a compact contract error

- [x] Step 5: Implement SQL-side filter service

Requirements:

- implement `KanbanFilterService`
- filter rows are persisted in structured form in `widgetState`
- the service derives parameterized SQL executed in SQLite
- the service increments and tracks `filters_version`
- no Rust-side row filtering is used after the query runs

Step 5 passes when:

- applying filters changes the effective SQL-backed dataset
- the GUI can be reconstructed from persisted filter rows

- [x] Step 6: Implement instance-aware stream service

Requirements:

- implement `KanbanStreamService`
- `/host/widgets/{instance_id}/stream` resolves server, base view, filters, and liveness state
- the stream uses the filtered SQL wrapper from `KanbanFilterService`
- heartbeat and stale/disconnected timing are emitted according to the spec

Step 6 passes when:

- the widget can connect through the instance-aware stream
- filter changes cause a new filtered subscription generation
- stale and disconnected states can be observed deterministically

- [x] Step 7: Implement semantic action services

Requirements:

- implement `KanbanActionService`
- implement `KanbanWorklogService`
- implement `create-record`
- implement `update-record`
- implement `move-record`
- implement `delete-record`
- implement `start-worklog`
- implement `stop-worklog`
- implement `apply-filters`

Step 7 passes when:

- every action returns the specified response envelope
- task-type and category CRUD works through `create-record` and `update-record`
- worklog invariants hold through the service layer

- [x] Step 8: Implement Maud shell and fragments

Requirements:

- implement the stable shell in Maud
- use Maud-native `@if` and `@for` where structure is server-owned
- keep Maud markup idiomatic with shorthand such as `.class-name {}` and `p.class-name {}`
- add stable fragment targets for header, toolbar, board, create sheet, edit sheet, and focus sheet
- implement loading, locked, mismatch, empty, and misconfigured states in Maud

Step 8 passes when:

- the Kanban can render from server-owned Maud fragments without client-built HTML strings for the main board
- the fragment targets match the contract in this document

- [x] Step 9: Implement signal hydration and widget-state persistence

Requirements:

- signal-patch local runtime state into the widget
- persist sparse UI state in `widgetState`
- persist widths, collapsed columns, body modes, focus state, and structured filters
- keep host state and local ergonomics separate from business truth

Step 9 passes when:

- reloads preserve the intended UI ergonomics
- no redundant dense per-card state is written when defaults are sufficient
- implementation note: this is implemented in code, but further reduction of imperative JavaScript in favor of Datastar remains valid incremental cleanup work

- [x] Step 10: Implement core board interactions

Requirements:

- drag and drop between quantity columns
- optimistic move with rollback on failure
- full-height board layout
- board scroll and per-column scroll
- column resize
- column collapse
- global and per-card body modes

Step 10 passes when:

- the current board ergonomics are preserved or improved
- `move-record` replaces direct generic Record patching from the widget
- implementation note: drag resize now uses the column side borders rather than dedicated width buttons

- [x] Step 11: Implement create and edit surfaces

Requirements:

- create sheet supports `head`, optional `body`, optional `task_type`, optional `categories`
- create sheet defaults `task_type` to empty
- edit sheet supports task-type and category CRUD
- category chips preserve insertion order
- clearing `task_type` warns that the record will disappear from Kanban after save

Step 11 passes when:

- a normal record can become Kanban-ready by setting `task_type`
- clearing all categories does not remove the record from Kanban
- clearing `task_type` removes it from Kanban after refresh

- [x] Step 12: Implement filter builder GUI

Requirements:

- toolbar button opens filter drawer or sheet
- each row is `AND`
- multi-value within a row is `OR`
- row types include text, categories, assignees, task types, quantities, and open-worklog
- active filters are shown as compact chips
- clear-all and per-row removal exist

Step 12 passes when:

- the GUI can round-trip persisted filters without raw SQL editing
- applying filters reconnects the stream and yields SQL-filtered results
- implementation note: filter opening should be local and immediate because options are prefetched with the contract; runtime verification is still required on a fresh imported widget instance

- [x] Step 13: Implement focus mode detail loading

Requirements:

- implement `KanbanDetailService`
- implement `load-record-detail`
- focus mode renders into the dedicated focus target
- preserve board and column scroll positions while focus mode is open
- parent links are clickable in cards and focus mode
- child summaries in focus mode show `head` and allow navigation

Step 13 passes when:

- focus mode can open and close without losing board context
- parent and child navigation behave as specified
- implementation note: the focus path is implemented in code, but it still requires runtime verification on a fresh imported widget instance

- [x] Step 14: Implement relational and aggregate task detail

Requirements:

- comments summary on the board
- full comments in focus mode
- assignees through `app_user`
- task categories and badges
- worklog intervals, live timer display, heartbeat, and offline stop queue

Step 14 passes when:

- comments, assignees, task type, categories, and worklog are all visible and editable through the intended surfaces
- running worklog state stays consistent across reconnects
- implementation note: comments remain `record_comment`, not `record`; resources remain separate relations; live reconnect behavior still needs authenticated browser verification

- [ ] Step 15: Implement external resource detail and final polish

Requirements:

- bucket image references through `record_resource_ref`
- focus-mode resource rendering
- final connection and auth UX polish
- review patch granularity and remove obsolete client-side SSE assumptions

Step 15 passes when:

- the widget no longer depends on raw snapshot parsing for its primary runtime
- the end-to-end Kanban matches this specification closely enough that only polish tasks remain

Current implementation summary:

- implemented in code:
  - sidecar migrations and indexes
  - record-centric contract route
  - SQL-side derived filtering
  - instance-aware Kanban stream
  - semantic Kanban actions
  - Maud-owned board shell and fragments
  - Datastar-driven sheet, query, and focus shell state
  - create and edit sheets
  - filter builder GUI
  - focus mode detail loading
  - markdown preview in board card bodies
  - worklog start, stop, heartbeat, and offline stop queue
  - comments, assignees, hierarchy summaries, and resource references
  - sparse `widgetState` persistence
  - column drag resize through side borders
  - removal of the old width increment and decrement buttons
- runtime verification pending:
  - authenticated browser verification of focus mode on a fresh imported widget
  - authenticated browser verification of the filter GUI round trip on a fresh imported widget
  - authenticated browser verification of worklog and reconnect flows
  - final visual verification that the empty intermediate box and column-height box behavior are fully gone in the fresh imported widget
- final polish pending:
  - continue replacing imperative JavaScript with Datastar where it materially simplifies stable-shell state
  - simplify any remaining legacy widget runtime assumptions after the fresh imported widget is confirmed

=== Current code path migration checklist

Current widget path:

- `crates/web/src/sand/kanban_record_view.rs`

Current responsibilities that should be migrated:

- `buildStreamUrl()`: replace direct `/host/integrations/servers/{server_id}/views/{view_id}/stream` usage with `/host/widgets/{instance_id}/stream`
- `buildRecordUrl()`: replace direct generic Record patching from drag/drop with `move-record`
- `validateSnapshot()`: replace Record-only short-form validation with `/contract`-backed normalized contract validation
- `normalizeRows()`: replace minimal `{id, quantity, head, body}` normalization with normalized task projection fields produced by the internal service
- `parseEventBlock()` and `consumeSseResponse()`: stop doing low-level SSE parsing in the widget once the official stream emits Datastar fragments/signals or official event shapes
- `renderCard()`, `renderColumn()`, and `renderBoard()`: move structural HTML ownership to Maud fragments
- `handleSnapshot()`: stop storing raw backend snapshot shape as the main rendering input
- `handleDrop()`: keep imperative drag/drop, but switch the mutation call from generic `PATCH /table/record/{id}` to `move-record`
- `persistUi()`: keep the persistence path, extend it to filter clauses and focus state, and keep the representation sparse
- `toggleWidgetStream()` and `reconnect()`: keep the concepts, but drive them through the instance-aware runtime and liveness model

Current behavior already worth preserving:

- quantity-to-lane mapping
- optimistic move with rollback on failure
- host `widgetState` persistence for widths, collapse state, and body modes
- per-column scroll and width controls
- pause/resume controls
- compact mismatch and transport error states

Preferred implementation order against the current file:

1. keep the widget shell alive while introducing `/contract`
2. replace raw snapshot validation with contract validation
3. replace direct stream URL construction with instance-aware stream
4. move board HTML generation into Maud fragment responses
5. keep drag/drop JS but switch writes to semantic actions
6. add filter editor UI and wire it to `apply-filters`
7. add focus mode detail loading through `load-record-detail`
8. add comments, assignees, hierarchy, categories, and worklog detail
9. add bucket image detail in focus mode
10. remove obsolete client-side SSE parsing and raw snapshot assumptions

=== Final position

The official Record Kanban should be built as a server-shaped board with a rich local UI layer.

That means:

- `Engineer` for structure and data authority
- `Clown` traits for local board ergonomics
- sidecar tables for richer task metadata
- normalized relations for users and hierarchy
- dedicated interval-based worklog storage
- host-owned auth and runtime state
- semantic actions where multi-table workflows become complex
