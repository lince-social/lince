== Trail

The Trail Relation sand is a packaged widget implemented in Rust and emitted as HTML with Maud.
The main files are:

- `crates/web/src/sand/trail_relation/mod.rs`
- `crates/web/src/sand/trail_relation/body.rs`
- `crates/web/src/sand/trail_relation/styles.rs`
- `crates/web/src/sand/trail_relation/script.rs`
- `crates/web/src/application/trail_widget.rs`
- `crates/web/src/presentation/http/api/widgets.rs`
- `crates/web/src/infrastructure/backend_api_store.rs`

The split of responsibility is:

- `mod.rs`: package manifest, package archive assembly, external assets
- `body.rs`: HTML skeleton
- `styles.rs`: CSS
- `script.rs`: browser behavior and endpoint calls
- `trail_widget.rs`: host widget service, host actions, local/remote orchestration
- `widgets.rs`: `/host/widgets/...` HTTP routes and trail SSE wrapping
- `backend_api_store.rs`: low-level filtered table reads and record batch update SQL

=== Package Assembly

The Trail widget is not a raw `.html` file in the source tree. It is an official `.lince` package assembled in Rust.

The package is built in `crates/web/src/sand/trail_relation/mod.rs` with:

```rust
crate::domain::lince_package::LincePackage::new_archive(
    Some("trail_relation.lince".into()),
    manifest,
    document(),
    "index.html",
    assets,
)
```

Important points:

- The package manifest is a Rust `PackageManifest`, not a `toml` file.
- The HTML is emitted by `document()` using Maud.
- Extra files are infused through the `assets` map.
- The widget loads those extra files with `WidgetScript::src(...)` or normal asset references inside the generated HTML.

The Trail package currently vendors:

- `d3.v7.min.js`
- `LICENSE.txt`

Those are infused like this:

```rust
assets.insert(
    "d3.v7.min.js".into(),
    include_bytes!("../relations/d3.v7.min.js").to_vec(),
);
assets.insert(
    "LICENSE.txt".into(),
    include_bytes!("../relations/LICENSE.txt").to_vec(),
);
```

And the script is loaded like this:

```rust
body_scripts: vec![
    crate::sand::WidgetScript::src("d3.v7.min.js"),
    crate::sand::WidgetScript::inline(script::script()),
],
```

So if you want your own package with extra files:

- put the file in the source tree
- add it to the `assets` map with `include_bytes!`
- reference it from the generated page
- if the asset license requires notice travel, include the license file too

There is no extra `toml` sidecar for this widget today. The package metadata lives entirely in Rust.

=== Browser Endpoints

The browser code in `script.rs` calls these routes.

==== 1. Contract

Route:

```text
GET /host/widgets/{instance_id}/contract
```

Example:

```text
GET /host/widgets/card_42/contract
```

What it returns:

- widget metadata
- source server metadata
- permissions
- current trail binding
- current sync metadata if a trail is bound
- optional snapshot in the contract payload
- supported actions

Relevant fields returned today:

```json
{
  "widget": {
    "instanceId": "card_42",
    "title": "Trail Relation"
  },
  "source": {
    "serverId": "04",
    "serverName": "Dashboard",
    "streamsEnabled": true
  },
  "binding": {
    "trailRootRecordId": 70,
    "viewId": 18,
    "sync": {
      "trailRootRecordId": 70,
      "syncSourceRecordId": 12,
      "scope": "t",
      "fields": "hb",
      "syncKarmaId": 8
    },
    "snapshot": {
      "rows": []
    }
  }
}
```

The widget uses this to initialize:

- the currently bound trail root
- the source server id
- sync scope and sync fields controls
- whether it should connect to the stream

==== 2. Stream

Route:

```text
GET /host/widgets/{instance_id}/stream
```

Example:

```text
GET /host/widgets/card_42/stream
```

This is an SSE endpoint. The Trail widget treats it as source of truth for the graph.

The important custom events are:

- `trail-sync`
- `trail-error`

`widgets.rs` wraps the underlying view `snapshot` and `error` events into these trail-specific events.

Example `trail-sync` payload:

```json
{
  "ok": true,
  "snapshot": {
    "rows": [
      { "id": 70, "quantity": -1, "head": "Record" },
      { "id": 71, "quantity": 0, "head": "Child" }
    ]
  },
  "binding": {
    "trailRootRecordId": 70,
    "viewId": 18,
    "sync": {
      "syncSourceRecordId": 12,
      "scope": "t",
      "fields": "hb"
    }
  }
}
```

Example `trail-error` payload:

```json
{
  "ok": false,
  "message": "The trail stream reported an error.",
  "binding": {
    "trailRootRecordId": 70,
    "viewId": 18
  }
}
```

==== 3. Search Trails

Route:

```text
POST /host/widgets/{instance_id}/actions/search-trails
```

Payload shape:

- `headContains?: string`
- `category?: string`
- `assignee?: string`

Example:

```json
{
  "headContains": "Record",
  "category": "documentation, copy",
  "assignee": "bomboclaat"
}
```

What it does:

- asks the host widget service to search `record`
- supports assignee matching by `app_user.id`, `app_user.name`, or `app_user.username`
- category matching is partial `LIKE`, not exact equality

==== 4. Search Assignees

Route:

```text
POST /host/widgets/{instance_id}/actions/search-assignees
```

Payload shape:

- `query?: string`

Example:

```json
{
  "query": "bomb"
}
```

This is used by the assignee autocomplete in:

- Discover
- Create Trail

==== 5. Bind Trail

Route:

```text
POST /host/widgets/{instance_id}/actions/bind-trail
```

Payload shape:

- `trailRootRecordId: number`

Example:

```json
{
  "trailRootRecordId": 70
}
```

What it does:

- validates that the record exists
- ensures or creates the derived trail view
- persists runtime state on the board card
- returns the binding and current sync metadata

Example response detail:

```json
{
  "trailRootRecordId": 70,
  "viewId": 18,
  "sync": {
    "trailRootRecordId": 70,
    "syncSourceRecordId": 12,
    "scope": "t",
    "fields": "hb",
    "syncKarmaId": 8,
    "syncConditionId": 3,
    "syncConsequenceId": 4,
    "conditionToken": "srthb12",
    "consequenceToken": "srthb70",
    "operator": "="
  }
}
```

==== 6. Create Trail

Route:

```text
POST /host/widgets/{instance_id}/actions/create-trail
```

Payload shape:

- `sourceRecordId: number`
- `assignee: string`
- `scope?: "n" | "t" | "nt"`
- `fields?: string`

Example:

```json
{
  "sourceRecordId": 12,
  "assignee": "bomboclaat",
  "scope": "t",
  "fields": "hb"
}
```

The normal flow is:

1. load original record
2. resolve assignee to `app_user.id`
3. create copied root record with `quantity = -1`
4. copy original `head` and `body`
5. copy categories and add `copy`
6. create `assigned_to` links
7. create or update Karma sync config
8. execute Karma
9. ensure or create derived view
10. persist widget runtime state
11. initialize trail quantities to `root = -1`, descendants = `0`

Example successful response:

```json
{
  "ok": true,
  "action": "create-trail",
  "record_id": 70,
  "await_stream_refresh": true,
  "detail": {
    "trailRootRecordId": 70,
    "viewId": 18,
    "sync": {
      "syncSourceRecordId": 12,
      "scope": "t",
      "fields": "hb",
      "syncKarmaId": 8
    },
    "initialization": {
      "changed": [
        { "recordId": 70, "quantity": -1 },
        { "recordId": 71, "quantity": 0 }
      ]
    }
  }
}
```

==== 7. Initialize Trail

Route:

```text
POST /host/widgets/{instance_id}/actions/initialize-trail
```

Payload shape:

- `trailRootRecordId?: number`

Example:

```json
{
  "trailRootRecordId": 70
}
```

This is the reset-to-start operation. It loads the current trail view and writes:

- root record: `-1`
- every other visible record in the trail view: `0`

==== 8. Run Trail Sync

Route:

```text
POST /host/widgets/{instance_id}/actions/run-trail-sync
```

Payload shape:

- `trailRootRecordId?: number`
- `scope?: "n" | "t" | "nt"`
- `fields?: string`

Example:

```json
{
  "trailRootRecordId": 70,
  "scope": "t",
  "fields": "hb"
}
```

This updates the `trail.sync` configuration if needed and executes the sync Karma again.

==== 9. Record Batch Patch

The current browser does not call `set-trail-quantity`. It computes the quantity changes locally and persists them through the generic record collection patch.

Route:

```text
PATCH /host/integrations/servers/{server_id}/table/record
```

Payload shape:

- array of record patches
- each object must contain `id`
- each object may contain `quantity`, `head`, and/or `body`

Example used by Trail:

```json
[
  { "id": 70, "quantity": 1 },
  { "id": 71, "quantity": -1 },
  { "id": 72, "quantity": 0 }
]
```

Possible non-trail example:

```json
[
  { "id": 10, "head": "New head" },
  { "id": 11, "body": "New body", "quantity": 1 }
]
```

The current implementation only accepts these writable fields for batch update:

- `id`
- `quantity`
- `head`
- `body`

Anything else is rejected.

=== Endpoints Used Internally By The Host Service

The browser mostly talks to `/host/widgets/...`, but `TrailWidgetService` fans out into local or remote backend calls.

For local servers it calls `BackendApiService`.
For remote servers it calls `ManasGateway`, which proxies the remote server.

The important backend routes used under the hood are:

- `GET /api/table/record`
- `GET /api/table/record?head_contains=...&category=...&assignee=...`
- `GET /api/table/record/{id}`
- `PATCH /api/table/record`
- `POST /api/table/record`
- `GET /api/table/app_user`
- `GET /api/table/app_user?identity=...`
- `GET /api/table/view`
- `GET /api/table/view/{id}`
- `POST /api/table/view`
- `PATCH /api/table/view/{id}`
- `GET /api/view/{view_id}/snapshot`
- `GET /api/table/record_extension`
- `POST /api/table/record_extension`
- `PATCH /api/table/record_extension/{id}`
- `DELETE /api/table/record_extension/{id}`
- `GET /api/table/record_link`
- `POST /api/table/record_link`
- `DELETE /api/table/record_link/{id}`
- `POST /api/table/karma_condition`
- `PATCH /api/table/karma_condition/{id}`
- `POST /api/table/karma_consequence`
- `PATCH /api/table/karma_consequence/{id}`
- `POST /api/table/karma`
- `PATCH /api/table/karma/{id}`
- `POST /api/karma/{id}/execute`

Important point:

- the browser does not call all of these directly
- the host widget service calls them while handling create, bind, sync, reset, and search

=== Services Involved

If you want to make your own widget, these are the main Rust services involved in Trail today.

At widget layer:

- `TrailWidgetService`
- `WidgetRuntime`

At local backend layer:

- `BackendApiService`
- `BackendApiStore`
- `ViewReadService`
- subscription service through `subscribe_view(...)`

At remote proxy layer:

- `ManasGateway`

State and authentication:

- `BoardStateStore`
- `AppAuth`
- `OrganStore`

The main TrailWidgetService methods worth reading are:

- `contract(...)`
- `prepare_stream(...)`
- `action(...)`
- `search_trails(...)`
- `search_assignees(...)`
- `bind_trail(...)`
- `create_trail(...)`
- `initialize_trail(...)`
- `run_trail_sync(...)`
- `upsert_trail_sync_config(...)`
- `load_trail_sync_metadata(...)`
- `ensure_derived_view(...)`
- `persist_runtime_state(...)`

=== How Data Is Stored

==== Browser

The browser keeps transient state in a big `state` object inside `script.rs`.
Important fields are:

- `binding`
- `sourceServerId`
- `snapshot`
- `discoverResults`
- `selectedOriginal`
- `selectedNodeId`
- `physics`
- `quantityOverrides`

The browser also stores one local persistence key:

```text
lince.widget.trail_relation.{instanceId}.boundTrailRoot
```

This is only a convenience fallback. The authoritative widget binding should come from the board state and stream.

==== Board State

The widget stores runtime state inside the board card `widget_state` under:

```text
trail_runtime
```

Current fields are:

- `server_id`
- `trail_root_record_id`
- `derived_view_id`

This is what lets the widget survive refresh and reopen the same bound trail.

==== Database Tables

Trail currently writes or reads these tables:

- `record`
- `record_extension`
- `record_link`
- `view`
- `karma_condition`
- `karma_consequence`
- `karma`
- `app_user`

The important conventions are:

- copied root and descendants are `record`
- categories are stored in `record_extension` namespace `task.categories`
- trail sync metadata is stored in `record_extension` namespace `trail.sync`
- assignees are `record_link` rows with `link_type = "assigned_to"` and `target_table = "app_user"`
- parent/child structure is normal `record_link` parent links
- the derived graph source is a `view`

Example `trail.sync` value:

```json
{
  "trail_root_record_id": 70,
  "sync_source_record_id": 12,
  "scope": "t",
  "fields": "hb",
  "sync_karma_id": 8,
  "sync_condition_id": 3,
  "sync_consequence_id": 4,
  "condition_token": "srthb12",
  "consequence_token": "srthb70",
  "operator": "="
}
```

=== Search Behavior

The Trail widget search behavior today is:

- `headContains`: partial case-insensitive match on `record.head`
- `category`: comma-separated partial matches against `task.categories`
- `assignee`: matches `app_user.id`, `app_user.name`, or `app_user.username`
- assignee autocomplete uses `identity` filtering over `app_user`

Examples:

```text
GET /api/table/record?head_contains=record
GET /api/table/record?category=documentation,copy
GET /api/table/record?assignee=bomboclaat
GET /api/table/app_user?identity=bomb
```

=== Progression Behavior

The current Trail graph progression behavior is:

1. use the SSE snapshot as source of truth
2. compute local quantity changes for the chosen node
3. apply them optimistically to the graph
4. persist them through the record batch patch endpoint
5. wait for the next SSE `trail-sync` frame, which replaces local truth

The browser-side parent rule enforced today is:

- all parents must be `1` before a node can become `1`

The reset button uses the host `initialize-trail` action, not the generic patch directly.

=== Derived View

The Trail widget binds to a derived `view`, not directly to raw `record` rows.

When you bind or create a trail, `TrailWidgetService` ensures a view named like:

```text
__lince_web_trail_{instance_id}
```

That view is the data source for the stream. The graph renders `snapshot.rows` from that view.

This means:

- if the graph is wrong at startup, inspect the trail view snapshot first
- if the graph is wrong after an optimistic update, inspect the next SSE frame
- if trail children are missing, inspect whether Karma sync or the derived view query is missing them

=== If You Want To Make Your Own

The practical recipe is:

1. Make a package module like `trail_relation/mod.rs`.
2. Keep the package manifest in Rust.
3. Generate HTML with Maud.
4. Put browser logic in a script module.
5. If you vendor third-party assets, add them to the package `assets` map and ship the license too.
6. Add a host service that exposes a small widget-facing action surface.
7. Let that host service decide whether to call local `BackendApiService` or remote `ManasGateway`.
8. Persist only small widget runtime state on the board card.
9. Treat the stream snapshot as source of truth for graph widgets.

For Trail specifically, the smallest public surface the browser really depends on is:

- `contract`
- `stream`
- `search-trails`
- `search-assignees`
- `bind-trail`
- `create-trail`
- `initialize-trail`
- `run-trail-sync`
- record batch `PATCH`

That is enough to rebuild the current widget from scratch.
