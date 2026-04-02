== Web Components

This document is the canonical technical specification for web components in Lince.

Current code is more authoritative than this document when there is conflict.
This file focuses on ways to get data, ways to use data, and recommended workflows.

=== Basics

There are two families of web components: standalone imported widgets and official `sand/` widgets. Standalone imported widgets are single HTML documents executed inside an iframe through `srcdoc`. Official `sand/` widgets are Lince-maintained components, usually rendered with Rust + Maud, and should move toward one host-managed runtime contract instead of each widget inventing its own transport.

The main actors are the backend, which owns persisted business data, generic CRUD, saved views, streaming, and semantic actions; the host, which owns widget instance identity, permissions, auth state, board layout, and persisted `widgetState`; the bridge, which connects host and widget; and external systems such as third-party APIs, foreign sites, files, browser APIs, and WASM runtimes.

Treat the bridge as control plane, not bulk data plane.
Use it for metadata, state persistence, and runtime flags.
Do not use it as the main large-record transport when the backend or official stream should be the source.

*Standalone widget package contract*

When generating a standalone imported widget, produce one complete self-contained HTML document that is safe to save directly as a `.html` file and execute inside an iframe via `srcdoc`.

Hard requirements:

- do not depend on remote CDNs
- do not depend on remote fonts
- do not assume network access unless the host contract explicitly gives it
- do not render a fake outer browser frame inside the card
- use inline CSS and inline JS
- do not call `parent` window APIs directly
- use the injected bridge if host communication is needed

The embedded manifest block must exist near the top:

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

Manifest fields are straightforward: `title` is the short user-facing name, `author` is the author string, `version` is semantic version text, `description` is the short summary, `details` is the longer explanation, `initial_width` and `initial_height` are integers from `1` to `6`, and `permissions` is the list of host-recognized capabilities.

*Host runtime contract*

When a widget runs inside the board, it runs in an iframe through `srcdoc`, the host injects a vendored Datastar runtime, the host injects `window.LinceWidgetHost`, and the frame may include dataset metadata.

Do not add a CDN Datastar import.

If internal state must use `localStorage`, namespace it by widget instance:

```js
const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
```

Prefer host-persisted state through the bridge instead of relying only on `localStorage`.

Bridge helper:

```txt
window.LinceWidgetHost
```

Bridge surface: `getState()`, `getMeta()`, `getCardState()`, `setCardState(nextState)`, `patchCardState(patch)`, `setStreamsEnabled(enabled)`, `subscribe(handler)`, `requestState()`, and `print(label)`.

Bridge rules are simple: persist long-lived widget ergonomics in `widgetState`, do not store backend credentials there, and do not store backend configuration copies there either.

Datastar bridge example:

```html
<div
  data-lince-bridge-root
  data-on:lince-bridge-state="$bridge = evt.detail.bridge; $meta = evt.detail.meta"
></div>
```

=== Ways To Get Data

Pick one obvious primary source.
Only mix sources when there is a real reason.

*Host metadata and bridge state*

Use this for widget instance identity, `serverId`, `viewId`, stream enabled state, and persisted `widgetState`. Host metadata may first appear in frame dataset values such as `window.frameElement?.dataset?.linceServerId` and `window.frameElement?.dataset?.linceViewId`. Once the bridge is ready, treat `meta.serverId`, `meta.viewId`, and `meta.streams.enabled` as authoritative.

*Generic table CRUD*

Use this when the widget edits ordinary rows in known tables.

Direct backend routes:

```txt
GET    /api/backend/table/{table}
POST   /api/backend/table/{table}
GET    /api/backend/table/{table}/{id}
PATCH  /api/backend/table/{table}/{id}
DELETE /api/backend/table/{table}/{id}
```

Host-mediated routes for configured remote servers:

```txt
GET    /host/integrations/servers/{server_id}/table/{table}
POST   /host/integrations/servers/{server_id}/table/{table}
GET    /host/integrations/servers/{server_id}/table/{table}/{id}
PATCH  /host/integrations/servers/{server_id}/table/{table}/{id}
DELETE /host/integrations/servers/{server_id}/table/{table}/{id}
```

Prefer host-mediated routes for remote configured widgets, do not pass `server_id` as a query parameter to `/api/backend/...`, and state clearly which table the widget owns.

*View streams*

Use this when the widget consumes a saved view as a live dataset.

Current routes:

```txt
GET /api/backend/sse/view/{view_id}
GET /host/integrations/servers/{server_id}/views/{view_id}/stream
```

Current SSE behavior is simple: events are named, `snapshot` carries the latest JSON payload, and `error` carries a JSON error payload. Reconnect cautiously, keep the UI usable before the first snapshot, do not assume workspace switching remounts the iframe, validate the payload shape, and show compact mismatch UI when the shape is wrong.

*Instance-aware official runtime*

This is the preferred direction for official `sand/` widgets:

```txt
GET  /host/widgets/{instance_id}/stream
GET  /host/widgets/{instance_id}/contract
POST /host/widgets/{instance_id}/actions/{action}
```

Use this when the host should own:

- resolving widget config by instance
- consuming backend streams internally
- returning normalized widget-facing contracts
- exposing semantic actions instead of raw multi-table choreography

Short `/contract` example:

```json
{
  "widget_kind": "kanban",
  "runtime_shape": "server-shaped board with rich local state",
  "required_columns": {
    "id": "integer",
    "quantity": "integer",
    "head": "text",
    "body": "text"
  },
  "permissions": {
    "read_view_stream": true,
    "write_records": true,
    "write_table": true
  }
}
```

Use `/contract` to describe widget-facing data, not raw storage internals.

*Provisioned backing resources*

Some widgets cannot start with a pre-existing `viewId`. They need the host to create or bind resources first.

Preferred provisioning direction:

```txt
GET  /host/widgets/{instance_id}/contract
POST /host/widgets/{instance_id}/provision
POST /host/widgets/{instance_id}/actions/{action}
```

Provisioning should answer which server the widget is bound to, whether provisioning is allowed, whether resources already exist, which `record_id`, `record_extension_id`, and `view_id` belong to the widget, and whether the stream is ready.

Chess is the simplest example: one `record` for game identity, one `record_extension` row for game state JSON, and one `view` that streams the record plus sidecar payload.

Example widget-owned JSON:

```json
{
  "history": [],
  "current": {}
}
```

The host should own provisioning and linkage.
The widget should own the meaning of the JSON.

*Local-only or foreign data*

Use this when the widget does not depend on Lince domain data as its main source. Typical examples are clocks, local notes, weather, Spotify control, imported HTML wrappers, and terminal or WASM surfaces. Keep the runtime honest, do not pretend a foreign integration is a normal Record widget, and use `widgetState` for local ergonomics when that state should survive reloads.

=== Ways To Use Data

Use the simplest storage model that still keeps the feature queryable and maintainable.

*Core table rows*

Current stable core truth:

- `record`: `id`, `quantity`, `head`, `body`
- `view`: `id`, `name`, `query`

Keep `record` generic.
Do not keep adding task-specific columns to `record` unless they become central to Lince itself.

*Sparse sidecar JSON*

Use `record_extension` for optional, versioned, widget-specific or feature-specific metadata.

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

Good uses:

- `task.categories`
- `task.schedule`
- `task.effort`
- `task.type`

Do not store widget UI state there.

*Relations and repeated data*

If the feature points to its own entity table, use relations instead of scalar metadata.

Recommended relation table:

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

Example:

- use `record_link` to `app_user` for assignment when users live in their own table
- only fall back to scalar assignment metadata when no identity table exists

*Dedicated feature tables*

Use dedicated tables for repeated or lifecycle-heavy data.

Examples:

- comments: use `record_comment`
- worklogs: use `record_worklog`
- reusable external resources: use `record_resource_ref`

Recommended comment rule:

- keep comments as comment rows
- do not model first Kanban comments as child `record` rows

Recommended worklog rule:

- `started_at` and `ended_at` are canonical interval truth
- `seconds` is derived or cached
- do not allow more than one open interval for the same `(record_id, author_user_id)` pair

Recommended worklog index:

```sql
CREATE UNIQUE INDEX idx_record_worklog_one_open_interval
ON record_worklog(record_id, author_user_id)
WHERE ended_at IS NULL;
```

*Normalized views*

Widgets should consume normalized views, not raw storage assumptions.

Examples:

- simple record table: use only `record`
- lightweight Kanban: use `record` plus optional category metadata
- planning Kanban: use `record` plus sidecars and aggregates
- worklog dashboard: use `record_worklog` aggregates plus optional effort data

When arrays are needed in a SQL projection, prefer JSON text in v1.

*Widget state*

Use host `widgetState` for:

- panel open or closed state
- filters
- local display modes
- selected tabs
- persisted UI ergonomics

Do not use `widgetState` for:

- backend credentials
- durable business truth
- copies of backend config that the host already owns

=== Recommended Workflows

Keep the workflow obvious.
Most mistakes come from mixing too many responsibilities.

*Server-shaped official widget*

Use this when backend truth is primary.

Recommended shape:

- host-owned auth and runtime state
- bridge as control plane
- Maud for structure
- Datastar fragments for server-owned repeated structure
- Datastar signals for shell and local UI state
- JS only for imperative browser edges

Good fits:

- record tables
- CRUD panels
- dashboards with server-owned structure
- Kanban-like surfaces with strong domain rules

*Stable shell with local state*

Use this when the shell is stable and most visible changes are local presentation state.

Recommended shape:

- small stable DOM shell
- frontend Datastar signals for display state
- optional backend seed data
- host `widgetState` for persisted ergonomics

Good fits:

- clocks
- counters
- compact dashboards
- notes with view toggles

*Client-owned renderer*

Use this when the frontend owns rendering and interaction rules.

Recommended shape:

- raw JSON snapshot or SSE
- JS or vendored framework renders the surface
- Datastar is optional
- host still owns auth and transport

Good fits:

- Chess
- client-heavy graphs
- legacy ports
- library-driven widgets

*Foreign integration*

Use this when truth mainly lives outside Lince.

Recommended shape:

- wrap or adapt the foreign system honestly
- keep Lince involvement narrow
- use bridge state for host integration, not to fake domain truth

Good fits:

- weather widgets
- Spotify control
- embedded apps
- imported HTML wrappers

*Engine-driven surface*

Use this when the browser runtime is the real surface.

Recommended shape:

- engine shapes the meaningful UI
- DOM shell is mostly chrome or fallback
- JS handles lifecycle and integration glue

Good fits:

- terminals
- canvas tools
- media tools
- WASM inspectors

*Self-provisioning workflow*

Use this when the widget must create or bind its own backing resources.

Recommended order:

1. Widget boots with bridge metadata and no fake assumptions.
2. Host decides whether the widget is configured, provisionable, or blocked.
3. Widget requests provisioning when needed.
4. Host creates or finds resources idempotently.
5. Host stores resolved linkage in `widgetState`.
6. Widget continues through `/contract`, `/stream`, and actions.

The board should support a `provisionable` state.
A stream widget without `viewId` is not always broken.

*Maud authoring conventions*

For official `sand/` widgets written with Maud:

- prefer `@if`, `@else`, `@for`, and `@match` over building HTML strings by hand
- keep repeated structure on the server when the server already knows it
- keep fragments readable enough to become stable Datastar patch targets

Prefer shorthand when it makes the markup clearer:

- `.class-name { ... }`
- `#element-id { ... }`
- `p.class-name { ... }`
- `button.toolbar-btn { ... }`

Example:

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

- large concatenated HTML strings when the server knows the structure
- verbose `div class="..."` markup when Maud shorthand is clearer
- pushing simple server-owned structure into client JS without a reason
