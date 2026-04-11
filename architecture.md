# Lince Trail Architecture

This document describes the backend workflow for the Trail Relation sand only. It intentionally skips the frontend JavaScript implementation.

## Overview

Trail Relation is a widget package that binds a board card to a trail root record, materializes a derived `view`, and streams that view through SSE. The backend flow is:

1. Resolve the widget instance from the board state.
2. Decide whether the target organ is local or remote.
3. Ensure the derived trail view exists and store its runtime binding.
4. Read a snapshot of that view.
5. Subscribe to the view so later SQLite writes refresh the snapshot automatically.

The key code paths are:

- `crates/web/src/application/trail_widget.rs`
- `crates/web/src/application/backend_api.rs`
- `crates/web/src/application/subscription.rs`
- `crates/persistence/src/write_coordinator.rs`
- `crates/persistence/src/repositories/view.rs`
- `crates/web/src/presentation/http/api/backend.rs`
- `crates/web/src/presentation/http/api/integrations.rs`
- `crates/web/src/presentation/http/api/widgets.rs`

## Database Shape

The Trail workflow uses the same SQLite tables as the rest of Lince. The important tables are:

- `record`
- `view`
- `record_extension`
- `record_link`
- `app_user`
- `karma_condition`
- `karma_consequence`
- `karma`
- `view_dependency`

### Core trail data

Trail Relation reads and writes mainly through these tables:

- `record`
  - Core row data.
  - Trail expects at least `id`, `quantity`, `head`, and `body`.
- `record_extension`
  - JSON payloads keyed by `namespace`.
  - Trail uses `trail.sync` to store sync metadata for a root record.
- `record_link`
  - Relationship table.
  - Trail uses `assigned_to` links to `app_user` and `parent` links to other `record` rows.
- `karma_condition`, `karma_consequence`, `karma`
  - Used to store and execute the trail sync rule that copies or progresses data.
- `view`
  - Stores the derived SQL view that Trail subscribes to.

### View dependency table

`view_dependency` is the table that makes automatic refresh work.

- Each `view_id` is associated with the base tables referenced by its SQL query.
- `view` itself is always included as a dependency.
- The dependency rows are regenerated whenever a view is inserted, updated, or deleted.

## How Automatic Refresh Works

The refresh loop is a chain of three parts: write tracking, dependency lookup, and subscription refresh.

### 1. Writes are tracked centrally

`WriteCoordinator` wraps the SQLite connection and installs update, commit, and rollback hooks.

- Every write statement records the touched tables.
- After a successful commit, the coordinator emits an `InvalidationEvent` containing `changed_tables`.
- `view_dependency` changes are removed from the invalidation set so dependency maintenance itself does not cause refresh loops.

Relevant code:

- `crates/persistence/src/write_coordinator.rs`

### 2. View dependencies are inferred from SQL

`ViewRepository` reads the stored SQL query and extracts table names from `FROM` and `JOIN` clauses.

- Special internal views are excluded from dependency inference.
- The dependency set is persisted in `view_dependency`.

Relevant code:

- `crates/persistence/src/repositories/view.rs`

### 3. Subscriptions refresh on matching invalidations

`SubscriptionRegistry::subscribe_view` does the actual SSE subscription logic.

- It loads the initial snapshot immediately.
- It loads the current dependency set for the view.
- It subscribes to `WriteCoordinatorHandle::subscribe_invalidations()`.
- On each invalidation, it refreshes only when one of the changed tables intersects the dependency set.
- If the refreshed snapshot payload is identical to the previous payload, nothing is emitted.
- If the snapshot changes, a new `Snapshot` frame is sent.
- If refresh fails, an `Error` frame is sent.

Relevant code:

- `crates/application/src/subscription.rs`

## Trail Derived View

Trail does not stream a raw `record` table. It first builds a derived SQL view for the selected trail root.

### Derived view name

The derived view name is deterministic:

- `__lince_web_trail_{instance_id}`

### View SQL

The derived query starts from the root record and walks the trail through parent links:

- A recursive CTE starts at the trail root record id.
- It follows `record_link` rows where:
  - `link_type = 'parent'`
  - `target_table = 'record'`
- It computes the tree depth and then joins in:
  - `record_extension` for categories and trail sync metadata
  - `record_link` for assignees and parents
  - `app_user` for assignee names
  - `record` for child counts and parent text

The resulting row contract includes:

- Required columns:
  - `id`
  - `quantity`
  - `head`
  - `body`
- Optional columns:
  - `depth`
  - `primary_category`
  - `categories_json`
  - `assignee_ids_json`
  - `assignee_names_json`
  - `assignee_usernames_json`
  - `parent_ids_json`
  - `parent_heads_json`
  - `children_count`
  - `children_json`
  - `sync_source_record_id`
  - `trail_root_record_id`

Relevant code:

- `crates/web/src/application/trail_widget.rs`

## Trail Widget Workflow

`TrailWidgetService` owns the backend behavior for the Trail sand.

### 1. Resolve the widget instance

The service loads the board card from `BoardStateStore` and validates that:

- the card is a package widget
- the package filename is `trail_relation.lince` or `trail_relation.html`
- the required permissions are present

It then loads the organ from `OrganStore` and decides whether the organ is local or remote.

### 2. Local vs remote

The same backend code supports two transport modes:

- Local:
  - Use the in-process backend API.
  - Subscribe directly through `BackendApiService::subscribe_view`.
- Remote:
  - Use `ManasGateway`.
  - Open the remote stream with `GET /api/sse/view/{view_id}` and a bearer token.

If the remote server returns `401`, the session is expired and the stored server session is invalidated.

### 3. Persist runtime binding

After binding or creating a trail, the service writes runtime data back into the board state under `trail_runtime`.

Stored fields:

- `server_id`
- `trail_root_record_id`
- `derived_view_id`

This is how later calls recover the active binding without re-discovering it from scratch.

### 4. Bind or create trail

Trail exposes three core backend operations:

- `bind-trail`
  - Attach an existing root record to the widget.
  - Ensure the derived view exists.
  - Load the snapshot.
- `create-trail`
  - Create a new root `record`.
  - Copy the source content into a new root.
  - Create or update `record_extension` sync metadata.
  - Create the `karma_condition`, `karma_consequence`, and `karma` rows for sync.
  - Execute the karma rule.
  - Ensure the derived view exists.
- `run-trail-sync`
  - Reuse the existing sync configuration.
  - Update the sync rule if scope or fields changed.
  - Execute the karma rule again.

Relevant code:

- `crates/web/src/application/trail_widget.rs`

## SSE Endpoints

Trail uses plain SSE on the server side. Datastar is not required for the backend implementation.

### Backend SSE

The backend route is:

- `GET /api/sse/view/{view_id}`

Behavior:

- Authenticates with `Authorization: Bearer ...`
- Calls `BackendApiService::subscribe_view`
- Streams `SseFrame::Snapshot` and `SseFrame::Error`
- Encodes them as SSE events named:
  - `snapshot`
  - `error`

The snapshot endpoint is:

- `GET /api/view/{view_id}/snapshot`

It returns the serialized snapshot payload for a single view read.

Relevant code:

- `crates/web/src/presentation/http/api/backend.rs`

### Host proxy SSE

The host-facing proxy route is:

- `GET /host/integrations/servers/{server_id}/views/{view_id}/stream`

It chooses one of two paths:

- Local organ:
  - Subscribe directly to the local backend view stream.
- Remote organ:
  - Proxy the remote SSE stream from the remote server.
  - Forward `text/event-stream`.

There is a matching snapshot proxy:

- `GET /host/integrations/servers/{server_id}/views/{view_id}/snapshot`

Relevant code:

- `crates/web/src/presentation/http/api/integrations.rs`

### With or without Datastar

From the backend point of view, the implementation is the same either way:

- Without Datastar:
  - Keep the endpoint as a normal SSE stream.
  - Emit plain events such as `snapshot` and `error`.
- With Datastar:
  - The endpoint still remains standard SSE.
  - Datastar is only a client-side consumer of that stream.
  - The backend does not need Datastar-specific transport logic.

In current Lince code, Datastar is not what makes the stream work. The stream works because the server emits normal SSE frames and the subscription system refreshes the view snapshot when its dependencies change.

## Widget Endpoints

The widget layer exposes generic endpoints for any supported sand package.

- `GET /host/widgets/{instance_id}/contract`
  - Returns the widget contract.
  - For Trail, this includes the binding, permissions, source organ, and data contract.
- `GET /host/widgets/{instance_id}/stream`
  - Returns the widget SSE stream.
  - For Trail, this wraps the trail subscription and adds `trail-sync` or `trail-error` companion events.
- `POST /host/widgets/{instance_id}/actions/{action}`
  - Dispatches Trail-specific actions.

Trail action names:

- `search-trails`
- `search-assignees`
- `bind-trail`
- `create-trail`
- `run-trail-sync`

Relevant code:

- `crates/web/src/presentation/http/api/widgets.rs`
- `crates/web/src/application/trail_widget.rs`

## Generic CRUD Endpoints Used by Trail

Trail does not implement its own table CRUD API. It uses the generic backend API instead.

### Local backend API

The backend router exposes:

- `GET /api/table/{table}`
- `POST /api/table/{table}`
- `PATCH /api/table/{table}`
- `GET /api/table/{table}/{id}`
- `PATCH /api/table/{table}/{id}`
- `DELETE /api/table/{table}/{id}`

Specialized routes:

- `POST /api/table/record/quantities`
- `GET /api/karma`
- `POST /api/karma`
- `GET /api/karma/{id}`
- `PATCH /api/karma/{id}`
- `DELETE /api/karma/{id}`
- `POST /api/karma/{id}/execute`
- `GET /api/view/{view_id}/snapshot`
- `GET /api/sse/view/{view_id}`

Trail uses these tables heavily:

- `record`
- `record_extension`
- `record_link`
- `app_user`
- `karma_condition`
- `karma_consequence`
- `karma`
- `view`

Relevant code:

- `crates/web/src/presentation/http/api/backend.rs`
- `crates/web/src/application/backend_api.rs`
- `crates/web/src/infrastructure/backend_api_store.rs`

### Remote proxy CRUD

For remote organs, the host forwards the same CRUD semantics through:

- `GET /host/integrations/servers/{server_id}/table/{table}`
- `POST /host/integrations/servers/{server_id}/table/{table}`
- `PATCH /host/integrations/servers/{server_id}/table/{table}`
- `GET /host/integrations/servers/{server_id}/table/{table}/{id}`
- `PATCH /host/integrations/servers/{server_id}/table/{table}/{id}`
- `DELETE /host/integrations/servers/{server_id}/table/{table}/{id}`

The proxy also forwards:

- file access
- view snapshot reads
- view SSE streams

## What Trail Actually Writes

Trail writes only through the generic tables. The important mutations are:

- `record`
  - create the trail root
  - update copied or edited records
- `record_extension`
  - store trail sync metadata and category metadata
- `record_link`
  - store parent links and assignee links
- `karma_condition`, `karma_consequence`, `karma`
  - create or update sync rules

Because the derived view depends on those tables, any write to them can invalidate the current snapshot and trigger a fresh SSE update.

## Practical Notes

- The refresh system is table-driven, not UI-driven.
- A view only refreshes when a changed table matches its inferred dependency set.
- If a snapshot does not change after refresh, no SSE update is sent.
- Trail uses the same infrastructure for local and remote organs; only the transport changes.
- Datastar does not change the server implementation of SSE in this codebase.

