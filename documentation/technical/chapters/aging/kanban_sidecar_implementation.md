# Holistic Implementation: Kanban + Sidecar Tables + Official Web Runtime

## Purpose

This document proposes a holistic implementation for:

- keeping `record` generic
- moving rich task-specific data into sidecar tables
- exposing that data cleanly through views and endpoints
- supporting multiple official `sand/` components with different behavior
- implementing the official Kanban as `Sand Class B: Hybrid Reactive`

This document complements:

- `web_sand_classes.md`
- `kanban_sand_class_b_spec.md`

## Problem

Lince wants `record` to stay generic, but serious widgets such as Kanban need richer task data:

- bucket
- start date
- end date
- estimate
- actual accumulated work
- assignee
- hierarchy
- comments

If all of that is pushed directly into the `record` table, the base schema becomes overloaded and task-specific.

If all of that is pushed into `record.body`, the data becomes hard to query, validate, evolve, and update.

The system needs to support:

- many users
- many use cases
- many widgets
- long-term schema evolution

without filling the `record` table with mostly-unused columns.

## Core Decision

The implementation should use three layers of data.

### 1. Core universal table

Keep `record` minimal and universal.

Core fields:

- `id`
- `quantity`
- `head`
- `body`

Meaning:

- `head` is the short human label
- `body` is the human long text
- `quantity` stays as the generic operational field

This allows:

- simple tables
- minimal lists
- simple Kanban
- note-like widgets

with zero task-specific schema expansion.

### 2. Sparse sidecar metadata

Use one sparse JSON sidecar table for optional per-record scalar metadata.

This solves:

- optional fields
- user-specific evolution
- versioned data formats
- avoiding many rarely-used columns

### 3. Dedicated side tables for repeated or relational data

Do not put everything in one JSON blob.

Use proper tables for:

- comments
- worklog
- record-to-record or record-to-entity links

This keeps the data queryable and evolvable.

## Data Model

## Keep `record` generic

`record` should remain the universal base object.

Do not add direct columns for:

- bucket
- start/end date
- estimate
- actual hours
- assignee
- parent
- comments

unless one day the project decides that those properties have become universally central to Lince itself.

## New sidecar tables

### `record_extension`

Purpose:

- sparse, versioned, optional metadata per record
- domain-oriented facets

Recommended columns:

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
```

Recommended indexes:

```sql
CREATE INDEX idx_record_extension_record_id
    ON record_extension(record_id);

CREATE INDEX idx_record_extension_namespace
    ON record_extension(namespace);
```

Use cases:

- `task.bucket`
- `task.schedule`
- `task.effort`
- `task.assignment`

Recommended namespace policy:

- business-oriented namespaces
- not component-oriented namespaces

Good:

- `task.schedule`
- `task.effort`
- `task.assignment`

Bad:

- `kanban.left_column_ui`
- `table_widget_sort_mode`

Component UI state belongs in board state, not in business sidecars.

### `record_link`

Purpose:

- generic links from one record to another entity
- supports hierarchy, assignment, bucket linkage, dependency graphs

Recommended columns:

```sql
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
```

Recommended indexes:

```sql
CREATE INDEX idx_record_link_record_id
    ON record_link(record_id);

CREATE INDEX idx_record_link_target
    ON record_link(target_table, target_id);

CREATE INDEX idx_record_link_type
    ON record_link(link_type);
```

Use cases:

- parent/child hierarchy
- assigned user
- linked bucket entity
- blocked-by / depends-on

Examples:

- `link_type = 'parent'`, `target_table = 'record'`
- `link_type = 'assigned_to'`, `target_table = 'app_user'`

### `record_comment`

Purpose:

- appendable and editable comments
- queryable task discussion

Recommended columns:

```sql
CREATE TABLE record_comment (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER,
    body TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME
);
```

Recommended indexes:

```sql
CREATE INDEX idx_record_comment_record_id_created_at
    ON record_comment(record_id, created_at);
```

### `record_worklog`

Purpose:

- track actual work spent
- support running timers or manual time entries

Recommended columns:

```sql
CREATE TABLE record_worklog (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER,
    started_at DATETIME NOT NULL,
    ended_at DATETIME,
    seconds REAL,
    note TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

Recommended indexes:

```sql
CREATE INDEX idx_record_worklog_record_id_started_at
    ON record_worklog(record_id, started_at);
```

## Facet strategy

The sidecar model should be treated as a facet system.

Recommended first facets:

- `task.bucket`
- `task.schedule`
- `task.effort`
- `task.assignment`

Example JSON payloads:

```json
{
  "bucket": "ops"
}
```

```json
{
  "start_date": "2026-03-28",
  "end_date": "2026-04-02"
}
```

```json
{
  "estimate_hours": 5
}
```

```json
{
  "assignee_name": "Annie Case",
  "assignee_id": 14
}
```

The important rule is:

- the core `record` remains generic
- optional business features become facets
- repeated or relational features become proper tables

## Why not encode all this into `record.body`

Using `body` front matter or body-prefixed JSON is acceptable only as:

- migration bridge
- import/export convenience
- zero-schema prototype path

It should not be the long-term canonical storage model because:

- queryability becomes poor
- writes become fragile string rewrites
- validation becomes component-specific
- comments and worklog fit badly
- different widgets will invent different formats

If body front matter ever exists, the system should provide an importer that extracts it into sidecar tables and leaves `body` as human-readable text.

## View strategy

Widgets should consume normalized views, not raw tables.

That is the main mechanism that lets different `sand/` components reuse `record` without sharing the same rich schema requirements.

### Example widget-specific view types

#### Simple record table

Needs only:

- `id`
- `quantity`
- `head`
- `body`

No sidecar joins required.

#### Lightweight Kanban

Needs:

- core `record` columns
- optional bucket

#### Planning Kanban

Needs:

- core `record` columns
- bucket
- schedule
- estimate
- actual hours
- assignee
- hierarchy
- comment count

#### Comment-focused board

Needs:

- core `record`
- comment aggregates

#### Time-focused board

Needs:

- core `record`
- estimate
- worklog aggregates

Each widget consumes a view contract, not the storage model directly.

## Example normalized Kanban view

An example view could project:

```sql
SELECT
    r.id,
    r.quantity,
    r.head,
    r.body,
    json_extract(bucket_ext.data_json, '$.bucket') AS bucket,
    json_extract(schedule_ext.data_json, '$.start_date') AS start_date,
    json_extract(schedule_ext.data_json, '$.end_date') AS end_date,
    json_extract(effort_ext.data_json, '$.estimate_hours') AS estimate_hours,
    COALESCE(worklog_sum.actual_hours, 0) AS actual_hours,
    json_extract(assign_ext.data_json, '$.assignee_name') AS assignee_name,
    hierarchy.parent_id,
    hierarchy.depth,
    hierarchy.children_count,
    COALESCE(comment_sum.comments_count, 0) AS comments_count,
    comment_sum.last_comment_preview
FROM record r
LEFT JOIN record_extension bucket_ext
    ON bucket_ext.record_id = r.id
   AND bucket_ext.namespace = 'task.bucket'
LEFT JOIN record_extension schedule_ext
    ON schedule_ext.record_id = r.id
   AND schedule_ext.namespace = 'task.schedule'
LEFT JOIN record_extension effort_ext
    ON effort_ext.record_id = r.id
   AND effort_ext.namespace = 'task.effort'
LEFT JOIN record_extension assign_ext
    ON assign_ext.record_id = r.id
   AND assign_ext.namespace = 'task.assignment'
LEFT JOIN (
    SELECT
        record_id,
        SUM(COALESCE(seconds, 0)) / 3600.0 AS actual_hours
    FROM record_worklog
    GROUP BY record_id
) worklog_sum
    ON worklog_sum.record_id = r.id
LEFT JOIN (
    SELECT
        child.record_id AS record_id,
        MAX(CASE WHEN child.link_type = 'parent' AND child.target_table = 'record' THEN child.target_id END) AS parent_id,
        0 AS depth,
        0 AS children_count
    FROM record_link child
    GROUP BY child.record_id
) hierarchy
    ON hierarchy.record_id = r.id
LEFT JOIN (
    SELECT
        record_id,
        COUNT(*) AS comments_count,
        MAX(body) AS last_comment_preview
    FROM record_comment
    WHERE deleted_at IS NULL
    GROUP BY record_id
) comment_sum
    ON comment_sum.record_id = r.id;
```

This query is only illustrative. The important point is that the widget sees a clean task-shaped projection while storage remains generic.

## Migration plan

## Phase 1: database migration

Add migrations:

- `*_record_extension.up.sql`
- `*_record_link.up.sql`
- `*_record_comment.up.sql`
- `*_record_worklog.up.sql`

Down migrations should drop the tables and indexes.

No change should be made to the existing `record` schema in this phase.

### Optional compatibility migration

If any task data already exists encoded in `body`, provide an optional importer that:

- reads recognized front matter or body prefix format
- writes to sidecar tables
- leaves the remaining body text clean

This should be an explicit migration tool, not an automatic hidden rewrite on every read.

## Phase 2: backend generic table support

The generic backend table API already exists.

Current generic routes:

- `/api/table/{table}`
- `/api/table/{table}/{id}`
- `/host/integrations/servers/{server_id}/table/{table}`
- `/host/integrations/servers/{server_id}/table/{table}/{id}`

Implementation changes required:

- extend `ApiTable` with new sidecar tables
- add row structs
- add field specs
- add parser support in `BackendApiStore`

New `ApiTable` variants should include:

- `RecordExtension`
- `RecordLink`
- `RecordComment`
- `RecordWorklog`

These tables should be available via the existing generic table CRUD layer.

This is important because it avoids inventing many new base API patterns when the generic table model already fits.

## Phase 3: permission and widget contract update

Current board behavior already recognizes:

- `read_view_stream`
- `write_records`
- `write_table`

The AI-facing docs mention only `write_records`, but the host UI already treats `write_table` as a server-backed capability.

Recommended initial permission strategy for the Kanban:

- `read_view_stream`
- `write_records`
- `write_table`

Meaning:

- `read_view_stream` for the stream
- `write_records` for core record mutations
- `write_table` for sidecar CRUD

This avoids inventing a new permission system immediately.

Longer term, finer-grained permissions may be added, but they are not required for the first implementation.

## Phase 4: official widget runtime endpoints

Official `sand/` widgets should not be limited to raw generic endpoints.

Add an official instance-aware runtime layer:

- `GET /host/widgets/{instance_id}/stream`
- `GET /host/widgets/{instance_id}/contract`
- `POST /host/widgets/{instance_id}/actions/{action}`

### `GET /host/widgets/{instance_id}/stream`

Purpose:

- resolve widget config from board state
- read `serverId`, `viewId`, and `widgetState`
- subscribe internally to local or remote SSE
- render Maud fragments
- emit Datastar patch events

### `GET /host/widgets/{instance_id}/contract`

Purpose:

- return widget runtime metadata
- expose accepted columns and optional fields
- help debug mismatches

### `POST /host/widgets/{instance_id}/actions/{action}`

Purpose:

- support high-level component actions that span multiple tables
- encapsulate validation and orchestration

For Kanban, recommended actions:

- `create-record`
- `update-record`
- `move-record`
- `delete-record`
- `upsert-extension`
- `set-parent`
- `assign-user`
- `add-comment`
- `edit-comment`
- `delete-comment`
- `start-worklog`
- `stop-worklog`

These actions can internally use the generic table layer, but the component should not have to orchestrate all multi-table business logic client-side.

## Endpoint model by Sand Class

## Sand Class A: Fragment Shell

Recommended endpoint profile:

- main runtime: `GET /host/widgets/{instance_id}/stream`
- optional writes: generic `/table/{table}` or narrow action endpoints

Best for:

- tables
- record browsers
- fragment-first layouts

## Sand Class B: Hybrid Reactive

Recommended endpoint profile:

- main runtime: `GET /host/widgets/{instance_id}/stream`
- main mutations: `POST /host/widgets/{instance_id}/actions/{action}`
- generic fallback: `/table/{table}`

Best for:

- official Kanban
- dashboards with interaction
- widgets with local UI state and structured backend data

This is the class chosen for the Kanban.

## Sand Class C: Signal Surface

Recommended endpoint profile:

- same stream endpoint
- stream emits mostly signal patches instead of large fragments
- writes can stay generic or action-based

Best for:

- clocks
- compact summaries
- stable small surfaces

## Sand Class D: Imperative Specialist

Recommended endpoint profile:

- optional stream endpoint
- often uses direct generic routes or dedicated service routes
- may combine with terminal/file/session endpoints

Best for:

- terminal
- canvas
- pointer-heavy custom interactions

## Sand Class E: External-Compatible Official

Recommended endpoint profile:

- prefers existing generic host-mediated routes
- can optionally use the official stream when embedded in Lince
- keeps stronger fallback behavior

Best for:

- showcase widgets
- migration widgets
- reference bridge widgets

## Kanban implementation

## Chosen class

The official Kanban should be implemented as `Sand Class B: Hybrid Reactive`.

Reason:

- rich server-owned structure
- per-card local UI state
- drag and drop
- column resize
- overlays and sheets
- explicit connection/auth state

## Data contract for the Kanban

### Required columns

- `id`
- `quantity`
- `head`
- `body`

### Optional columns

- `bucket`
- `start_date`
- `end_date`
- `estimate_hours`
- `actual_hours`
- `assignee_name`
- `parent_id`
- `depth`
- `children_count`
- `comments_count`
- `last_comment_preview`

The Kanban must reject incompatible views with a compact error state.

## Kanban layout model

The widget shell should contain:

- header
- connection and action toolbar
- board viewport
- optional status/footer region
- create/edit drawers or sheets

### Fragment patch targets

Recommended targets:

- `#kanban-header-meta`
- `#kanban-toolbar-state`
- `#kanban-columns`
- `#kanban-empty-or-error`
- `#kanban-create-sheet`
- `#kanban-edit-sheet`

### Signal-owned UI state

Recommended Datastar signals:

- connection badge state
- paused/running state
- global body display mode
- per-card body display mode
- collapsed columns
- column widths
- active sheet or drawer
- filters
- temporary draft values that are safe to persist

### Pure JavaScript responsibilities

Keep pure JS for:

- drag and drop between columns
- pointer-driven column resize
- focus-sensitive text editing
- scroll behavior requiring imperative handling

## Widget persistence

Persist UI state in `widgetState` in board state:

- column widths
- collapsed columns
- global body display mode
- per-card body display mode
- local filters
- stream paused preference

Do not persist in sidecar business tables:

- column width
- collapsed state
- per-widget visual preferences

These are component runtime concerns, not domain data.

## Kanban feature mapping to storage

### Core features stored in `record`

- title => `head`
- description => `body`
- kanban status => `quantity`

### Optional scalar task metadata in `record_extension`

- bucket => `task.bucket`
- start/end date => `task.schedule`
- estimate => `task.effort`
- simple assignee metadata => `task.assignment`

### Relational metadata in `record_link`

- parent/child hierarchy
- assigned user relation
- linked bucket relation if needed later

### Repeated data in dedicated tables

- comments => `record_comment`
- accumulated real work => `record_worklog`

## Kanban CRUD flows

### Create card

Should create:

- `record`
- optional `record_extension` rows
- optional `record_link` rows

Prefer a high-level widget action endpoint for this flow.

### Edit core card

Should update:

- `record.head`
- `record.body`
- `record.quantity`

### Move card across columns

Should:

- patch `record.quantity`
- update UI optimistically
- reconcile with server response

### Delete card

Should delete:

- `record`

with cascading deletes automatically removing:

- `record_extension`
- `record_link`
- `record_comment`
- `record_worklog`

### Comments

Should use dedicated actions or generic table routes for:

- create
- edit
- delete

### Worklog

Should support:

- start timer entry
- stop timer entry
- manual entry later if desired

## Simple table component implementation

One of the main goals of this architecture is to allow simple components to remain simple.

A simple record table should:

- read a core `record` view
- ignore all sidecar tables
- render core columns only

This proves that the richer task features do not contaminate the generic base model.

## How multiple official widgets reuse the same model

### Simple Record Table

Storage:

- `record` only

Class:

- Sand Class A or C depending on UX

### Minimal Kanban

Storage:

- `record`
- optional `task.bucket`

Class:

- Sand Class B

### Planning Kanban

Storage:

- `record`
- `record_extension`
- `record_link`
- `record_worklog`
- `record_comment`

Class:

- Sand Class B

### Timeline or scheduling board

Storage:

- `record`
- `task.schedule`

Class:

- Sand Class A or B

### Worklog dashboard

Storage:

- `record`
- `record_worklog`
- optional `task.effort`

Class:

- Sand Class C or B

## Implementation changes in the web crate

## Database and backend

Add new migrations.

Extend:

- `ApiTable`
- `BackendApiStore`
- generic table CRUD support

Update validation and row mapping for:

- `record_extension`
- `record_link`
- `record_comment`
- `record_worklog`

## Official widget runtime

Add:

- instance-aware widget stream endpoint
- widget contract endpoint
- widget action endpoints

Implement a registry or service layer that maps widget instance to:

- runtime configuration
- view subscription
- server/client auth state
- renderer

## Kanban widget

Replace the current raw-SSE/manual-rerender flow with:

- official instance stream
- Maud-rendered fragments
- Datastar signals for UI state
- narrow JS for drag/resize/editing

## Documentation

Update AI-facing docs to:

- document `write_table`
- document the new sidecar tables
- document the official widget runtime endpoints

## Rollout plan

### Step 1

Create sidecar tables and indexes.

### Step 2

Expose sidecar tables through generic backend CRUD.

### Step 3

Implement official widget runtime endpoints.

### Step 4

Rewrite Kanban around the instance stream and action endpoints.

### Step 5

Optionally add import tooling for any legacy body-encoded task metadata.

## Final position

The system should not try to make `record` carry every future use case directly.

The correct architecture is:

- minimal `record`
- sparse sidecar metadata
- proper tables for repeated and relational task data
- widget-specific normalized views
- class-based official widget runtimes

This gives Lince:

- genericity
- long-term evolution
- better querying
- cleaner widgets
- strong support for rich Kanban without polluting the core schema
