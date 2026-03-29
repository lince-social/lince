== Web Components

This document is the canonical technical specification for creating web components in Lince.

Use it as AI-facing reference material.

Current code is more authoritative than this document when there is conflict.
This document exists to provide one dense, unified view of:

- standalone widget packaging
- official host runtime behavior
- bridge and state contracts
- backend authentication and endpoint usage
- current schema and SSE behavior
- recommended sidecar-table evolution
- Sand Class taxonomy
- authoring defaults for widgets that interact with Lince and foreign resources

=== Primary distinction

There are two broad families of web components:

- standalone imported widgets
- official `sand/` widgets

Standalone imported widgets are single HTML documents executed inside an iframe through `srcdoc`.

Official `sand/` widgets are Lince-maintained components, normally rendered with Rust + Maud and increasingly expected to use an internal runtime contract rather than each widget hand-rolling transport logic.

=== Canonical questions for classification

Every widget should be understood by five questions:

1. Where does truth normally live?
2. How does truth normally reach the widget?
3. Who normally shapes the UI?
4. Where does interaction state normally live?
5. How portable or foreign is the runtime?

Do not invent a new Sand Class because a widget has a different domain or a different visual style.
Only create a new class if at least two of those five answers fundamentally change.

=== Runtime actors

==== Backend

The backend owns domain truth.

It normally owns:

- persisted business data
- generic table CRUD
- saved views
- view streaming
- semantic actions when they exist

==== Host

The host is the Lince web runtime that embeds the widget.

It normally owns:

- widget instance identity
- permissions
- auth state
- board layout
- persisted `widgetState`
- stream enable or pause state
- locked/configure/misconfigured host states

==== Bridge

The bridge is the host-to-widget communication layer.

Treat it as control plane, not bulk data plane.

Use it for:

- bootstrap state
- runtime metadata
- auth and permissions
- `widgetState` persistence
- stream enable or pause requests
- subscriptions to host state updates

Do not use it as the main large-record snapshot transport when the backend or official stream is the correct source.

==== External system

Anything outside Lince, such as:

- third-party APIs
- foreign websites
- imported HTML
- mounted foreign apps
- local files
- browser APIs
- WebAssembly runtimes

=== Standalone widget package contract

When generating standalone imported widgets, the package contract is:

- one complete HTML document
- self-contained
- safe to save directly as a `.html` file
- executed inside an iframe via `srcdoc`

Hard requirements:

- do not depend on remote CDNs
- do not depend on remote fonts
- do not assume network access unless the prompt or host contract explicitly provides it
- do not render a second outer browser frame or a fake card inside the card
- use inline CSS and inline JS when generating a standalone HTML widget
- do not call `parent` window APIs directly
- use the injected bridge if host communication is needed

The embedded manifest block must exist near the top of the document:

```html
<script type="application/json" id="lince-manifest">
  {
    "title": "Widget title",
    "author": "Author",
    "version": "0.1.0",
    "description": "Short summary",
    "details": "Longer explanation",
    "initial_width": 4,
    "initial_height": 3,
    "permissions": []
  }
</script>
```

Manifest fields:

- `title`: short user-facing name
- `author`: short author string
- `version`: semantic version string
- `description`: compact summary
- `details`: longer explanation
- `initial_width`: integer between `1` and `6`
- `initial_height`: integer between `1` and `6`
- `permissions`: host-recognized capabilities

=== Visual and design contract

The host visual language for imported or official widgets is:

- dark
- minimal
- technical
- compact
- premium rather than playful by default

Preferred visual behavior:

- use restrained color
- avoid gradients unless explicitly wanted
- make small card sizes viable
- do not overflow aggressively
- remain useful in preview mode
- use strong information hierarchy
- prefer compact controls

=== Host runtime contract

==== Runtime environment

When a widget runs inside the board:

- the widget runs in an iframe through `srcdoc`
- the host injects a vendored Datastar runtime
- the host injects `window.LinceWidgetHost`
- the widget may receive dataset metadata on the frame element

Do not add a CDN Datastar import.

==== Local persistence fallback

If internal widget state must use `localStorage`, namespace it with the widget instance:

```js
const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
```

Prefer host-persisted state through the bridge instead of relying only on `localStorage`.

==== Bridge helper

The host bridge helper lives at:

```txt
window.LinceWidgetHost
```

Bridge surface:

- `getState()`
- `getMeta()`
- `getCardState()`
- `setCardState(nextState)`
- `patchCardState(patch)`
- `setStreamsEnabled(enabled)`
- `subscribe(handler)`
- `requestState()`
- `print(label)`

Bridge persistence rules:

- prefer `patchCardState()` or `setCardState()` for per-widget runtime state
- keep long-lived visual ergonomics in `widgetState`
- do not store backend credentials or backend configuration copies there

==== Datastar bridge integration

If reactive host state is needed:

- add `data-lince-bridge-root` to a root element
- listen for `lince-bridge-state`

Event shape:

```txt
{ bridge, meta }
```

Datastar example:

```html
<div
  data-lince-bridge-root
  data-on:lince-bridge-state="$bridge = evt.detail.bridge; $meta = evt.detail.meta"
></div>
```

For Datastar-driven UI state:

- keep UI state in signals
- persist relevant state through signal-patch handlers into host `widgetState`

=== Backend authentication and permissions

==== Ownership

Backend authentication is host-owned.

Widgets must not:

- render their own login UI for the standard Lince backend flow
- store backend tokens in `localStorage`
- store backend tokens in widget archives
- store backend tokens in cookies
- store backend tokens in board state

Treat remote backend credentials as host-managed and in-memory only.

==== Host-provided identifiers

When a widget needs server-backed data, assume the host card configuration provides identifiers such as:

- `server_id`
- `view_id`

For host-configured widgets, those values may appear at:

- `window.frameElement?.dataset?.linceServerId`
- `window.frameElement?.dataset?.linceViewId`

Once the bridge is available, treat host metadata as authoritative:

- `meta.serverId`
- `meta.viewId`
- `meta.streams.enabled`

Do not create a second widget-local configuration store for server or view identity.

==== Host-recognized permissions

Current important permissions for backend-aware widgets:

- `read_view_stream`
- `write_records`
- `write_table`

Meaning:

- `read_view_stream`: widget consumes a configured SSE view stream and therefore needs `server_id` and `view_id`
- `write_records`: widget mutates record rows through host-mediated routes
- `write_table`: widget mutates generic tables through host-mediated routes and is the important generic permission for sidecar-table evolution

Do not invent near-miss permission names.

==== Auth endpoint

Current backend login endpoint:

```txt
POST /api/backend/auth/login
```

Request body:

```json
{ "username": "...", "password": "..." }
```

Success response:

```json
{ "token": "...", "token_type": "Bearer" }
```

Authenticated backend calls use:

```txt
Authorization: Bearer <token>
```

Widgets should not perform this login flow themselves for the normal host-mediated backend workflow.

=== Backend endpoint contract

==== Generic backend endpoints

Current direct backend endpoints:

```txt
GET    /api/backend/table/{table}
POST   /api/backend/table/{table}
GET    /api/backend/table/{table}/{id}
PATCH  /api/backend/table/{table}/{id}
DELETE /api/backend/table/{table}/{id}
GET    /api/backend/sse/view/{view_id}
```

==== Host-mediated widget routes

Widgets that are configured against a remote server should prefer host-mediated routes:

```txt
GET    /host/integrations/servers/{server_id}/views/{view_id}/stream
GET    /host/integrations/servers/{server_id}/table/{table}
POST   /host/integrations/servers/{server_id}/table/{table}
GET    /host/integrations/servers/{server_id}/table/{table}/{id}
PATCH  /host/integrations/servers/{server_id}/table/{table}/{id}
DELETE /host/integrations/servers/{server_id}/table/{table}/{id}
```

Rules:

- do not call `/api/backend/...` directly from a widget when the widget is supposed to use a configured remote server
- do not pass `server_id` as a query parameter to `/api/backend/...`
- use one obvious data source per widget unless there is a deliberate reason to mix them

==== Recommended official widget runtime direction

For official `sand/` widgets, the preferred long-term direction is an instance-aware runtime:

```txt
GET  /host/widgets/{instance_id}/stream
GET  /host/widgets/{instance_id}/contract
POST /host/widgets/{instance_id}/actions/{action}
```

Purpose:

- resolve widget config by instance
- centralize host/runtime state lookup
- consume backend streams internally
- emit Datastar fragments or signals without each widget rebuilding transport
- support richer, semantic actions instead of requiring client-side multi-table orchestration

This is recommended direction, not a claim that every route already exists in the current implementation.

==== Suggested `/contract` payload

When an official widget exposes:

```txt
GET /host/widgets/{instance_id}/contract
```

the preferred response shape is:

```json
{
  "widget_kind": "kanban",
  "class_hint": "Engineer with Clown traits",
  "base_kind": "record_centric",
  "required_columns": {
    "id": "integer",
    "quantity": "integer",
    "head": "text",
    "body": "text"
  },
  "optional_columns": {
    "primary_category": "text|null",
    "categories_json": "json_text<string[]>",
    "start_at": "datetime_utc|null",
    "end_at": "datetime_utc|null",
    "estimate_seconds": "integer|null",
    "actual_seconds": "integer|null",
    "assignee_ids_json": "json_text<integer[]>",
    "assignee_names_json": "json_text<string[]>",
    "parent_id": "integer|null",
    "parent_head": "text|null",
    "children_count": "integer|null",
    "children_json": "json_text<object[]>|null",
    "task_type": "enum|null",
    "comments_count": "integer|null",
    "last_comment_preview": "text|null",
    "active_worklog_count": "integer|null"
  },
  "enums": {
    "task_type": ["epic", "feature", "task", "other"]
  },
  "permissions": {
    "read_view_stream": true,
    "write_records": true,
    "write_table": true
  },
  "relations": {
    "assignees": {
      "link_type": "assigned_to",
      "target_table": "app_user",
      "many": true
    },
    "hierarchy": {
      "link_type": "parent",
      "target_table": "record",
      "many": false
    }
  },
  "detail_contract": {
    "focus_target": "#kanban-focus-card",
    "load_action": "load-record-detail",
    "summary_stream_only": true
  },
  "liveness": {
    "stream_heartbeat_seconds": 15,
    "stale_after_seconds": 45,
    "disconnected_after_seconds": 90
  }
}
```

Rules:

- use this route to declare the normalized widget-facing contract, not raw storage internals
- prefer JSON text for arrays in view projections when the SQL layer makes native arrays awkward
- use UTC datetimes in RFC3339-like text such as `2026-03-28T15:04:05Z`
- if the contract route exists, widgets should trust it over heuristic client-only schema guesses

=== Current schema and table contract

==== Generic table rules

- table endpoints are generic
- table names are path parameters
- rows are JSON objects
- mutations use JSON request bodies
- row identity is expected to use integer `id`

CRUD widgets should state which table they operate on.

If the user asks for a specific table, constrain the widget to that table.

==== Current core schema reference

Current important table reference derived from migrations and present docs:

- `record`: `id`, `quantity`, `head`, `body`
- `view`: `id`, `name`, `query`
- `collection`: `id`, `quantity`, `name`
- `configuration`: `id`, `quantity`, `name`, `language`, `timezone`, `style`, plus later config columns
- `collection_view`: `id`, `quantity`, `collection_id`, `view_id`, `column_sizes`
- `karma_condition`: `id`, `quantity`, `name`, `condition`
- `karma_consequence`: `id`, `quantity`, `name`, `consequence`
- `karma`: `id`, `quantity`, `name`, `condition_id`, `operator`, `consequence_id`
- `frequency`: `id`, `quantity`, `name`, `day_week`, `months`, `days`, `seconds`, `next_date`, `finish_date`, `catch_up_sum`
- `command`: `id`, `quantity`, `name`, `command`
- `transfer`: `id`, `quantity`
- `sum`: `id`, `quantity`, `record_id`, `interval_relative`, `interval_length`, `sum_mode`, `end_lag`, `end_date`
- `history`: `id`, `record_id`, `change_time`, `old_quantity`, `new_quantity`
- `dna`: `id`, `name`, `origin`, `quantity`
- `query`: `id`, `name`, `query`
- `pinned_view`: `id`, `view_id`, `position_x`, `position_y`, `z_index`, `width`, `height`

Important present truth:

- `record` is still the core generic table
- its core schema is still only `id`, `quantity`, `head`, `body`

=== SSE contract

==== Current view SSE behavior

Underlying backend route:

```txt
GET /api/backend/sse/view/{view_id}
```

Typical host-mediated route:

```txt
GET /host/integrations/servers/{server_id}/views/{view_id}/stream
```

Current event model:

- named SSE events
- `snapshot` carries the latest JSON payload for the view snapshot
- `error` carries a JSON payload describing a backend or subscription error

Widget rules for current SSE:

- reconnect cautiously
- keep the UI usable while waiting for first snapshot
- do not assume workspace switching remounts the iframe
- persist UI state explicitly
- validate expected schemas
- show compact mismatch state if payload shape is wrong

==== Mismatch behavior

When a widget expects a specific shape:

- validate it
- show compact mismatch UI
- log expected shape and received shape briefly in the console

=== Recommended sidecar-table evolution

==== Primary rule

Keep `record` generic.

Do not keep adding task-specific columns to `record` unless they become universally central to Lince itself.

Do not treat `record.body` as canonical structured storage for every evolving feature.

Body front matter is acceptable only as:

- import path
- migration bridge
- prototype shortcut

==== Recommended data layering

Use three layers:

1. minimal core table
2. sparse sidecar metadata
3. dedicated tables for repeated or relational data

==== Minimal core table

Keep:

- `record.id`
- `record.quantity`
- `record.head`
- `record.body`

==== Sparse sidecar metadata

Recommended table:

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

Use it for sparse, optional, versioned metadata.

Recommended namespaces:

- `task.categories`
- `task.schedule`
- `task.effort`
- `task.type`

Do not store component UI state there.

If a feature points to a first-class entity, prefer a relation instead of scalar metadata.

Example:

- use `record_link` to `app_user` for assignment when users are first-class entities
- use scalar assignment metadata only as a fallback when no identity table exists

==== Relational and repeated side tables

Recommended generic relation table:

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

Recommended comment table:

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

Recommended comment rule:

- keep comments as dedicated comment rows in v1
- do not model comments as child `record` rows for the first Kanban implementation
- comments have different lifecycle, ordering, and interaction needs than task hierarchy

Recommended worklog table:

```sql
CREATE TABLE record_worklog (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    author_user_id INTEGER,
    started_at DATETIME NOT NULL,
    ended_at DATETIME,
    last_heartbeat_at DATETIME,
    seconds REAL,
    note TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

Recommended semantic rule:

- allow many active intervals across different users
- do not allow more than one open interval for the same `(record_id, author_user_id)` pair
- treat `started_at` and `ended_at` as canonical interval truth
- treat `seconds` as derived or cached, not as an independent source of truth

If crash recovery for active timers matters, use coarse heartbeat updates such as every 5 minutes.

Recommended enforcement:

```sql
CREATE UNIQUE INDEX idx_record_worklog_one_open_interval
ON record_worklog(record_id, author_user_id)
WHERE ended_at IS NULL;
```

Also validate the same invariant in the service layer.

==== External resource references

If records must reference many reusable external resources, such as bucket images that may appear in many records, use a relational model instead of scalar metadata and do not treat body parsing as canonical storage.

Recommended table:

```sql
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
```

Recommended v1 values:

- `provider = "bucket"`
- `resource_kind = "image"`

Recommended use:

- keep attachments and reusable bucket images queryable without parsing `record.body`
- use `position` for stable ordering inside focused detail views
- use `data_json` only for minor optional metadata such as captions or sizing hints

This is separate from `task.categories`.

`task.categories` is for task grouping or categorization inside task semantics.
Bucket image references are external resource links to reusable assets.

==== View contract strategy

Widgets should consume normalized views, not raw storage assumptions.

Examples:

- simple record table: use only `record`
- lightweight Kanban: use `record` plus optional category metadata
- planning Kanban: use `record` plus sidecars and aggregates
- worklog dashboard: use `record_worklog` aggregates and optional effort data

This keeps the base schema generic while letting widgets stay strict about what they need.

=== Sand Classes

==== Purpose

Sand Classes are not permission boundaries.
They are a compressed vocabulary for the usual data, runtime, and rendering philosophy of a widget.

Primary classes:

- `Engineer`
- `Clown`
- `Monk`
- `Mercenary`
- `Astromancer`

Hybrid naming is encouraged when it is more truthful than forcing one bucket.

==== Resource scale

All scale values are:

- `none`
- `low`
- `mid`
- `high`

==== Resource matrix

#table(
  columns: (2.5fr, 1fr, 1fr, 1fr, 1fr, 1fr),
  [Resource], [Engineer], [Clown], [Monk], [Mercenary], [Astromancer],
  [Maud templating], [high], [low], [none], [none], [none],
  [Datastar signals in frontend], [low], [high], [low], [none], [none],
  [Datastar signals from backend], [low], [high], [none], [none], [none],
  [Datastar HTML fragments from backend], [high], [low], [none], [none], [none],
  [Raw JSON or hand-parsed data], [none], [none], [high], [low], [low],
  [Host or bridge runtime state], [high], [high], [low], [low], [low],
  [Official Lince endpoints], [high], [mid], [low], [none], [low],
  [External APIs, pages, or apps], [none], [low], [low], [high], [none],
  [Files or local resources], [none], [low], [low], [mid], [mid],
  [Iframe or embed isolation], [none], [none], [none], [mid], [none],
  [Foreign framework runtime], [none], [none], [mid], [mid], [none],
  [WASM or browser-engine runtime], [none], [none], [none], [none], [high],
)

==== Five-question matrix

#table(
  columns: (1.7fr, 2.1fr, 2.1fr, 2.1fr, 2.1fr, 2.1fr),
  [Question], [Engineer], [Clown], [Monk], [Mercenary], [Astromancer],
  [
    Where does truth normally live?
  ], [
    Backend
  ], [
    Mixed: backend for business, frontend for presentation
  ], [
    Varied, often client-owned or remote-contract-owned
  ], [
    Foreign system or local widget space, with Lince optional
  ], [
    Resource or engine dependent
  ],
  [
    How does truth normally reach the widget?
  ], [
    Official backend stream and endpoints through the host
  ], [
    Backend seed, optional backend stream, host runtime state, and heavy signal-driven local state
  ], [
    Raw JSON SSE, fetch JSON, manual parsing, or vendored runtime adapters
  ], [
    Foreign page, foreign API, files, embeds, or adapter layers
  ], [
    File, buffer, resource binding, engine feed, or runtime adapter
  ],
  [
    Who normally shapes the UI?
  ], [
    Maud plus backend fragments and a few signals
  ], [
    Small shell plus frontend Datastar signals
  ], [
    JS or vendored framework
  ], [
    Foreign UI, foreign framework, or adapter runtime
  ], [
    JS or WASM engine, with DOM shell secondary
  ],
  [
    Where does interaction state normally live?
  ], [
    Frontend for ergonomics, backend for domain mutations
  ], [
    Mostly frontend
  ], [
    Varied, usually frontend
  ], [
    Mostly frontend or foreign runtime
  ], [
    Engine-local or use-case dependent
  ],
  [
    How portable or foreign is the runtime?
  ], [
    Low, strongly Lince-coupled
  ], [
    Low to mid, still fairly Lince-shaped
  ], [
    Mid to high, usually self-contained
  ], [
    High, intentionally foreign-capable
  ], [
    Mid, more constrained by runtime capabilities than by Lince
  ],
)

==== Engineer

Definition:

- default official class for robust Lince-native workflows

Transport and shaping:

- backend truth
- official endpoints
- Maud-shaped structure
- Datastar HTML fragments as main structural patch mechanism
- backend signals only as secondary support

JavaScript profile:

- use JS sparingly
- preferred uses: drag/drop edges, resize edges, focus-sensitive editing, browser APIs awkward in Datastar

Best fits:

- record tables
- CRUD panels
- record browsers
- search results
- audit logs
- server-owned dashboards

==== Clown

Definition:

- signal-heavy, frontend-reactive class

Transport and shaping:

- often mixed truth
- stable shell
- frontend Datastar signals do most of the visible orchestration
- small backend fragment use is acceptable when targeted structural patches help

JavaScript profile:

- low
- use JS for imperative browser edges around signal-driven state

Best fits:

- clocks
- counters
- compact status widgets
- dashboards with stable shells and lively presentation state
- inspectors with many local display modes

==== Monk

Definition:

- client-shaped class built from raw data contracts

Transport and shaping:

- raw JSON SSE or fetched JSON
- JS or vendored framework shapes the UI
- Datastar is optional and minor

JavaScript profile:

- high
- JS is the main rendering tool
- use for snapshot parsing, DOM construction, library lifecycles, client-owned interaction flows

Best fits:

- experimental widgets
- legacy ports
- client-heavy graphs
- widgets using libraries that expect raw data

==== Mercenary

Definition:

- integration-first, foreign-capable class

Transport and shaping:

- truth often lives outside Lince
- wraps, embeds, fetches from, or adapts foreign systems
- Lince participates opportunistically rather than exclusively

JavaScript profile:

- low to mid
- use for adapters, wrappers, embed coordination, framework mounting, and API mediation

Best fits:

- embeds
- foreign sites or apps
- wrapper widgets
- framework-backed integrations
- imported HTML with fallback behavior

==== Astromancer

Definition:

- runtime-heavy class for engine-driven browser surfaces

Transport and shaping:

- engine or runtime shapes the meaningful surface
- DOM shell is mostly chrome and fallback structure

JavaScript profile:

- mid
- use JS for engine integration, lifecycle orchestration, browser capability access, and glue around WASM or other runtime cores

Best fits:

- terminal widgets
- canvas surfaces
- media tools
- graph engines
- WASM-powered inspectors

==== Practical hybrid naming

Preferred examples:

- official Kanban: `Engineer with Clown traits`
- portable integration: `Mercenary with Monk fallback`
- terminal: `Astromancer with Engineer chrome`

=== Authoring rules for AI and implementation work

==== General

- prefer one obvious primary data source
- do not invent endpoints that are not present in code or explicitly requested
- do not invent columns that are not present or explicitly specified
- if schema is ambiguous, stay generic and say what is assumed

==== For imported standalone widgets

- self-contained HTML
- no remote dependencies
- no custom login UI for standard host-backed flows
- no parent-window APIs

==== For official `sand/` widgets

- default to `Engineer`
- add `Clown` traits when local presentation state becomes rich
- use `Monk` deliberately, not lazily
- use `Mercenary` when outside systems are a real product requirement
- use `Astromancer` when the browser runtime itself is the main engine

==== Current widget mapping

Recommended current mapping:

- `extra_simple`: `Mercenary` during migration, then `Engineer`
- `view_table_editor`: `Engineer`
- `kanban_record_view`: `Engineer with Clown traits`
- `ops_clock`: `Clown`
- `local_terminal`: `Astromancer`

=== Appendix: current official `sand/` mapping

This appendix maps the current official widgets in `crates/web/src/sand/` to:

- the closest class today
- the recommended target runtime contract
- the main note or migration direction

#table(
  columns: (1.8fr, 1.7fr, 2.6fr, 2.4fr),
  [Widget], [Closest class today], [Recommended target runtime contract], [Notes],
  [
    `bucket_image_view`
  ], [
    `Engineer`
  ], [
    host or bridge runtime state plus host-backed file or bucket fetch; persist the selected path in `widgetState`
  ], [
    resource viewer; not a view-stream widget
  ],
  [
    `extra_simple`
  ], [
    `Monk`
  ], [
    migrate to official instance-aware widget stream with backend fragments or targeted signals
  ], [
    minimal reference widget; current implementation is a transport bridge, not the target architecture
  ],
  [
    `calendar`
  ], [
    `Clown`
  ], [
    local-only runtime by default; no backend contract required unless explicitly extended
  ], [
    compact ergonomic surface with stable shell and frontend-owned state
  ],
  [
    `general_creation`
  ], [
    `Engineer`
  ], [
    host bridge plus generic table CRUD and `write_table`
  ], [
    official generic creation surface for backend tables
  ],
  [
    `kanban_record_view`
  ], [
    `Engineer with Clown traits`
  ], [
    official instance-aware stream plus semantic action endpoints plus sidecar-backed writes
  ], [
    canonical Record board target
  ],
  [
    `lince_logo_led`
  ], [
    `Clown`
  ], [
    local-only runtime with no backend contract
  ], [
    visual brand surface; local animation is the point
  ],
  [
    `link_chip`
  ], [
    `Mercenary`
  ], [
    standalone or host-embedded link wrapper with no backend data contract
  ], [
    foreign-link surface, not a Lince-data surface
  ],
  [
    `local_terminal`
  ], [
    `Astromancer`
  ], [
    `/host/terminal/sessions` engine contract with terminal runtime
  ], [
    terminal engine; acknowledge it directly instead of forcing DOM-first assumptions
  ],
  [
    `markdown_notes`
  ], [
    `Clown`
  ], [
    local note state with optional host `widgetState` persistence
  ], [
    editor plus preview toggle; stable shell and frontend-owned presentation
  ],
  [
    `organ_management`
  ], [
    `Engineer with Clown traits`
  ], [
    host bridge plus host CRUD or control routes plus persisted UI context
  ], [
    control surface with meaningful host/runtime state
  ],
  [
    `ops_clock`
  ], [
    `Clown`
  ], [
    local runtime signals only
  ], [
    compact status widget
  ],
  [
    `record_crud`
  ], [
    `Engineer`
  ], [
    host-mediated `record` CRUD using `write_records`
  ], [
    strict Record tool
  ],
  [
    `spotify_control`
  ], [
    `Mercenary with Clown traits`
  ], [
    host or service mediation for Spotify read and control state
  ], [
    foreign service integration with compact local presentation state
  ],
  [
    `tasklist`
  ], [
    `Clown`
  ], [
    local or lightweight task API contract with stable shell and frontend-owned list state
  ], [
    task microfrontend, not a generic table editor
  ],
  [
    `tasks_table`
  ], [
    `Engineer`
  ], [
    structured table or metrics contract, ideally host-backed and schema-explicit
  ], [
    data-surface widget that should remain shape-aware
  ],
  [
    `view_table_editor`
  ], [
    `Monk`, migrating toward `Engineer`
  ], [
    official instance-aware stream plus fragment-driven table rendering plus table or action endpoints
  ], [
    current version still carries manual SSE and client-shaped transport logic
  ],
  [
    `weather`
  ], [
    `Mercenary with Clown traits`
  ], [
    host or external weather and location contract with no Record or view assumption
  ], [
    foreign data service surface with compact reactive presentation
  ],
)

=== Maud authoring conventions

For official `sand/` widgets written with Maud:

- prefer Maud-native control flow such as `@if`, `@else`, `@for`, and `@match` instead of building large HTML strings by hand
- prefer server-rendered repeated structure to be expressed directly in Maud loops
- prefer conditional regions to be expressed directly in Maud conditionals
- keep Maud fragments structured and readable enough that they can become stable Datastar patch targets

Preferred idiomatic element style:

- when the element is a `div`, omit the explicit `div` tag when class or id shorthand already makes the tag obvious
- prefer `.class-name { ... }` over `div class="class-name" { ... }`
- prefer `#element-id { ... }` or `.class-name #child-id { ... }` when that shape is natural in Maud
- for non-`div` tags, prefer shorthand such as `p.class-name { ... }`, `button.toolbar-btn { ... }`, `section.panel { ... }`
- only fall back to explicit `class="..."` attributes when shorthand would become awkward or unclear

Examples:

```rust
@if items.is_empty() {
    .empty-state {
        "No items"
    }
} @else {
    ul.item-list {
        @for item in items {
            li.item-row data-id=(item.id) {
                .title { (item.title) }
            }
        }
    }
}
```

Avoid:

- large concatenated HTML strings when the structure is known on the server
- verbose `div class="..."` style everywhere when Maud shorthand is clearer
- pushing simple branching or repeated structure into client JS when Maud can own it cleanly

=== Final operational summary

For official long-lived widgets:

- backend truth
- host-owned auth and runtime state
- bridge as control plane
- Maud for structure
- Datastar fragments for server-owned repeated structure
- Datastar signals for stable-shell runtime and local UI state
- JS only where imperative browser behavior is the right tool

For foreign-facing widgets:

- treat portability as a first-class architectural property
- do not fake an `Engineer` when the widget is really `Mercenary` or `Monk`

For engine-driven widgets:

- acknowledge `Astromancer` directly instead of forcing canvas, terminal, or WASM runtimes into ordinary DOM-first assumptions
