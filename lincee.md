Below are two first level header for Sand and Kanban, your job is to keep only those two and refine it with the things i asked you. Maintain human readable, concise paragraphs prefferably over bullet points. Put examples json/html... data and endpoints when you have to explain the architecture.

# Sand (Widgets): Web Components / Extensions

If web components are to be anything, they might also be evil and try to do evil stuff. We need to constraint them by default, but them in a sandbox. What goes into a sandbox? Sand.
The web version has a canvas you can put any HTML component inside, from normal tables to kanban, weather forecast, video streaming, even games with WASM (check the Freedoom Sand).

The Sand are supposed to be components that you can share and get from others, they are Contributions to your Need of components: therefore they are Records. But they can't be just a Record, they need to link with a resource, in HTML or somefile.lince form. In reality they point to a file in the Bucket.

=== Basics

There are two families of web components: standalone imported widgets and official `sand/` widgets. Standalone imported widgets are either single HTML documents executed inside an iframe through `srcdoc` or .lince files that are a ZIP of an HTML and anything it may need. Official `sand/` widgets are Lince-maintained components, usually rendered with Rust + Maud, and should move toward one host-managed runtime contract instead of each widget inventing its own transport.

The main actors are the backend, which owns persisted business data, generic CRUD, saved views, streaming, and semantic actions; the host, which owns widget instance identity, permissions, auth state, board layout, and persisted `widgetState`; the bridge, which connects host and widget; and external systems such as third-party APIs, foreign sites, files, browser APIs, and WASM runtimes.

Treat the bridge as control plane, not bulk data plane.
Use it for metadata, state persistence, and runtime flags.
Do not use it as the main large-record transport when the backend or official stream should be the source.

_Standalone widget package contract_

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

_Host runtime contract_

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

_Host metadata and bridge state_

Use this for widget instance identity, `serverId`, `viewId`, stream enabled state, and persisted `widgetState`. Host metadata may first appear in frame dataset values such as `window.frameElement?.dataset?.linceServerId` and `window.frameElement?.dataset?.linceViewId`. Once the bridge is ready, treat `meta.serverId`, `meta.viewId`, and `meta.streams.enabled` as authoritative.

_Generic table CRUD_

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

_View streams_

Use this when the widget consumes a saved view as a live dataset.

Current routes:

```txt
GET /api/backend/sse/view/{view_id}
GET /host/integrations/servers/{server_id}/views/{view_id}/stream
```

Current SSE behavior is simple: events are named, `snapshot` carries the latest JSON payload, and `error` carries a JSON error payload. Reconnect cautiously, keep the UI usable before the first snapshot, do not assume workspace switching remounts the iframe, validate the payload shape, and show compact mismatch UI when the shape is wrong.

_Instance-aware official runtime_

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

_Provisioned backing resources_

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

_Local-only or foreign data_

Use this when the widget does not depend on Lince domain data as its main source. Typical examples are clocks, local notes, weather, Spotify control, imported HTML wrappers, and terminal or WASM surfaces. Keep the runtime honest, do not pretend a foreign integration is a normal Record widget, and use `widgetState` for local ergonomics when that state should survive reloads.

=== Ways To Use Data

Use the simplest storage model that still keeps the feature queryable and maintainable.

_Core table rows_

Current stable core truth:

- `record`: `id`, `quantity`, `head`, `body`
- `view`: `id`, `name`, `query`

Keep `record` generic.
Do not keep adding task-specific columns to `record` unless they become central to Lince itself.

_Sparse sidecar JSON_

Use `record_extension` for optional, versioned, widget-specific or feature-specific metadata.

Recommended table:

```sql
CREATE TABLE record_extension (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    freestyle_data_structure TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, namespace),
    CHECK (json_valid(freestyle_data_structure))
);
```

Good uses:

- `task.categories`
- `task.schedule`
- `task.effort`
- `task.type`

Do not store widget UI state there.

_Relations and repeated data_

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
    freestyle_data_structure TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(record_id, link_type, target_table, target_id),
    CHECK (freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure))
);
```

Example:

- use `record_link` to `app_user` for assignment when users live in their own table
- only fall back to scalar assignment metadata when no identity table exists

_Dedicated feature tables_

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

_Normalized views_

Widgets should consume normalized views, not raw storage assumptions.

Examples:

- simple record table: use only `record`
- lightweight Kanban: use `record` plus optional category metadata
- planning Kanban: use `record` plus sidecars and aggregates
- worklog dashboard: use `record_worklog` aggregates plus optional effort data

When arrays are needed in a SQL projection, prefer JSON text in v1.

_Widget state_

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

_Server-shaped official widget_

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

_Stable shell with local state_

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

_Client-owned renderer_

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

_Foreign integration_

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

_Engine-driven surface_

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

_Self-provisioning workflow_

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

_Maud authoring conventions_

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

Be able to use a View someone else made.
How do we use it? Editing binary is not tha wae.
Can i write it in a language that is not Rust? An interface library that is not the one used by the GUI? Can I run ratatui?

== Bucket

The bucket is one configured object store.

The split between published sand widgets and general media should be modeled with key prefixes, not separate buckets.

Reserved bucket roots in v1:

- `lince/dna/sand/` for published sand widget packages derived from records
- `lince/media/` for everything else

This section records the current agreement for moving away from the GitHub `dna` repo as the runtime source of installable widgets.

=== Agreements

- The long-term source of truth for remote DNA widgets should be bucket objects plus database records, not the GitHub `dna` repo.
- The publication row should be a normal `record`.
- The human-facing listing fields should come from `record.head` and `record.body`.
- The bucket package layout should mirror the current package structure path used today.
- The bucket object key should not be stored in `record.body`.
- The bucket object reference should live in `record_resource_ref`.
- The client should not scan broad table data from every organ and infer a catalog locally.
- Each organ should expose a dedicated DNA catalog endpoint.
- The host should aggregate those organ-level DNA catalogs for the web UI.
- After approval, installed widgets should land in one shared local directory: `~/.config/lince/web/sand`.
- The older split between `~/.config/lince/web/widgets` and `~/.config/lince/web/sand` is not the desired end state.
- The UI should always show which organ a sand package comes from.
- The canonical source identifier should be the organ id, not the display name.
- The publication channel should remain explicit as `official` or `community`.
- `official/community` should remain a publication label for now, not a cryptographic trust proof.
- The UI should show both the source organ and the publication channel.
- All built-in official sands should be created locally at startup.
- Built-in official sands should not be pushed into the bucket automatically in any startup flow.
- Bucket publication should be a manual action through the sand export or sand publisher flow.
- If startup creation fails for one sand, the process should log it and continue booting.
- Published sand packages should use versioned bucket paths.
- Version comparison should be driven by package metadata, not by filename alone.
- Same version plus different bytes should be treated as a hard failure during publication.
- Signature-based official verification should be documented as the future trust model, but it should not be required in the current runtime flow.

=== Package layout

Preferred package prefix layout:

```text
lince/dna/sand/{channel}/{first_two_letters}/{package_name_lower_snake_case}/{version}/
```

Example:

```text
lince/dna/sand/official/ka/kanban_record_view/1.4.0/
```

Recommended object layout inside that package prefix:

```text
lince/dna/sand/official/ka/kanban_record_view/1.4.0/
  sand.toml
  kanban_record_view_metadata.html
  kanban_record_view.lince
  ...other package contents...
```

This mirrors the current structure path in the publication flow instead of inventing a second package layout.

If the local package id continues to use dash-style normalization elsewhere in the codebase, the translation between local id and bucket package name should be explicit.

The bucket convention here is lower snake case by agreement, not by current `slugify` behavior.

=== Record model

The preferred publication shape is:

- `record` as the main row
- `record_extension` for structured DNA publication metadata
- `record_resource_ref` for the bucket-backed package reference

The task-category system discussed in Kanban is still task metadata.

Bucket-backed DNA publication metadata should remain explicit and structured.

Recommended sidecar namespace:

```text
record_extension.namespace = "lince.dna"
```

Suggested `record_extension.freestyle_data_structure` shape:

```json
{
  "published": true,
  "channel": "official",
  "version": "1.4.0"
}
```

Recommended package reference:

```text
record_resource_ref.provider = "bucket"
record_resource_ref.resource_kind = "sand"
record_resource_ref.resource_path = "lince/dna/sand/official/ka/kanban_record_view/1.4.0/kanban_record_view_metadata.html"
```

Suggested `record_resource_ref.freestyle_data_structure` shape:

```json
{
  "slug": "kanban_record_view",
  "channel": "official",
  "version": "1.4.0",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/",
  "transport_filename": "kanban_record_view_metadata.html",
  "available_files": [
    "kanban_record_view_metadata.html",
    "kanban_record_view.lince"
  ],
  "package_format": "html",
  "mime_type": "text/html",
  "sha256": "optional-content-hash"
}
```

The slug belongs with the bucket-backed package reference because it identifies the published artifact, not only the record text.

In v1, a published DNA record should have exactly one primary `record_resource_ref` with:

- `provider = "bucket"`
- `resource_kind = "sand"`
- a `resource_path` under `lince/dna/sand/`

If more resources are later attached to the same record, the `sand` entry remains the canonical install target and the others are auxiliary resources.

The canonical transport target may be either:

- a plain `.html` package
- a `.lince` package archive

All preview, install, and local catalog flows should be able to fetch raw bytes, detect the format, unzip when needed, and use either transport.

=== Category direction

There is a separate broader refactor direction around categories.

The current Kanban-specific `task.categories` model is not the right long-term place for every component family that needs to differentiate records.

The preferred direction is a generic record-level category or tagging model that views can filter on.

That would cover use cases such as:

- project-scoped Kanban views
- family-specific view filtering
- identifying records that publish sand packages

Preferred direction:

- keep categories many-valued
- keep them attached to records through structured side metadata
- let views query them directly
- avoid treating category as a magic implicit routing rule outside query logic

Suggested future namespace:

```text
record_extension.namespace = "record.categories"
```

Possible payload shape:

```json
{
  "categories": ["project_alpha", "lincedna", "sand"]
}
```

This is a direction, not a blocker for the bucket DNA cutover.

The bucket DNA flow should still use `lince.dna` metadata as the canonical publication contract even if categories become queryable and useful for discovery.

=== Endpoint contracts

Preferred organ-local catalog route:

```text
GET /api/dna/catalog
```

Each row should expose the record-facing fields as `head` and `body`, not as `title` and `description`.

Suggested item shape:

```json
{
  "organ_id": "manas",
  "organ_name": "Lince Manas",
  "record_id": 123,
  "head": "Kanban Record View",
  "body": "Record-centric board with comments, worklog, and resources.",
  "slug": "kanban_record_view",
  "channel": "official",
  "version": "1.4.0",
  "bucket_key": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/kanban_record_view_metadata.html",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/",
  "transport_filename": "kanban_record_view_metadata.html",
  "available_files": [
    "kanban_record_view_metadata.html",
    "kanban_record_view.lince"
  ],
  "package_format": "html",
  "mime_type": "text/html",
  "updated_at": "2026-04-04T12:00:00Z"
}
```

Preferred host aggregation routes:

```text
GET /host/packages/dna/catalog
GET /host/packages/dna/{organ_id}/{record_id}/preview
POST /host/packages/dna/{organ_id}/{record_id}/install
```

The preview and install steps should converge into the same package parsing and local materialization flow already used for standalone package imports.

Preferred preview/install behavior:

- fetch raw bytes from `bucket_key`
- use the transport filename to determine whether the source is `.html` or `.lince`
- parse both through the same package parser
- unzip `.lince` as needed during preview or install
- keep enough metadata to remember which transport file was the source

=== Channel semantics

The web UI should keep `official/community` as an explicit publication channel.

Preferred behavior:

- the DNA browser should always show the source organ name beside the package channel
- the publication channel should be returned directly by the catalog endpoint
- `official` should be treated as a publisher claim in the current model
- `community` should remain the fallback for everything that is not intentionally published as `official`
- the canonical source key in state, cache, and provenance remains `organ_id`

This is a publication and browsing distinction, not a trust proof.

The web version should not hardcode a list of official hosts in the preferred current direction.

If a stronger trust model is needed later, signature-based release verification should be layered on top of the same record and bucket structure rather than replacing it.

=== Signature-based official verification

This is the preferred future trust model, but it should not be required in the current runtime flow.

The goal is to preserve the current `official/community` publication model while adding a cryptographic way to confirm that an `official` release was signed by a trusted publisher.

Recommended shape:

- keep `record` as the publication root
- keep `record_extension(namespace = "lince.dna")` for release metadata
- add `record_extension(namespace = "lince.signature")` for signature metadata
- keep `record_resource_ref` as the canonical artifact pointer

Recommended signing model:

- `Ed25519` key pairs
- a stable `publisher_id`
- a `key_id` for rotation
- a signed release payload that includes artifact hash and release metadata

Recommended signed payload shape:

```json
{
  "schema": "lince.dna.release.v1",
  "publisher_id": "lince:manas:official",
  "slug": "kanban_record_view",
  "channel": "official",
  "version": "1.4.0",
  "package_format": "lince",
  "entry_path": "index.html",
  "sha256": "artifact-content-hash",
  "created_at": "2026-04-04T12:00:00Z"
}
```

Recommended `record_extension(namespace = "lince.signature")` payload shape:

```json
{
  "algorithm": "ed25519",
  "schema": "lince.signature.v1",
  "publisher_id": "lince:manas:official",
  "key_id": "ed25519:lince:manas:2026-04",
  "signature": "base64-signature",
  "signed_payload": {
    "schema": "lince.dna.release.v1",
    "publisher_id": "lince:manas:official",
    "slug": "kanban_record_view",
    "channel": "official",
    "version": "1.4.0",
    "package_format": "lince",
    "entry_path": "index.html",
    "sha256": "artifact-content-hash",
    "created_at": "2026-04-04T12:00:00Z"
  }
}
```

Key storage model:

- private signing keys stay outside the database and outside the repository
- public verification keys live in trusted client configuration or the shipped web build
- signature metadata lives with the publication record so mirrors do not need a second trust database

Verification model:

- verification should be automatic during catalog view, preview, and install
- there should be no user approval step for signature checking itself
- if a release claims `official` but the signature is missing or invalid, the UI should not bless it as verified official

The stable long-term design point is that `official/community` remains the publication channel, while signature validation becomes the separate proof layer.

=== Local install direction

The local machine should keep one shared `sand` directory for web widgets.

That simplifies the user's mental model, but it also means the system must stop treating `sand` as meaning only "built-in official widget".

After this refactor, source metadata should remain explicit even if the filesystem directory is unified.

If `.lince` is meant to remain a first-class transport format, the local catalog should preserve enough source metadata to know whether the imported source was `.html` or `.lince`.

If preserving the original archive matters beyond provenance, local persistence should not immediately collapse every imported archive into `.html`.

Useful source labels include:

- built-in official widget
- uploaded local file
- imported from organ DNA catalog

That distinction should live in metadata, not in separate directories.

=== Startup local creation

Built-in official sands should be regenerated locally on every startup.

Automatic startup creation should only target the local machine's `~/.config/lince/web/sand`.

It should not publish into the bucket automatically.

Preferred startup behavior:

- build or render every built-in official sand into the local `sand` directory
- overwrite or upsert the local generated copies
- if one package fails to build or write, log it and continue startup

There should be no startup decision point around bucket publication in the current simplified model.

=== Manual publication direction

Bucket publication should be explicit and manual.

Preferred behavior:

- the user chooses a local sand package in the sand export or sand publisher widget
- the host validates the package automatically
- if publication succeeds, the host uploads the versioned package to the bucket and upserts the publication record
- no startup flow should publish into the bucket on the user's behalf

=== Versioning direction

Published sands should use versioned bucket paths while keeping one stable canonical publication record per slug.

Preferred model:

- stable record identity per slug
- package version from the widget manifest
- versioned bucket prefix per release
- canonical `record_resource_ref` points to the current release
- `record_extension(namespace = "lince.dna")` stores the current canonical version

Recommended release policy:

- same version and same bytes: no-op
- same version and different bytes: hard fail
- newer version: upload new versioned files and repoint the canonical resource ref
- older version: refuse downgrade

This preserves stable discovery while still allowing historical artifact paths.

=== Automatic verification

Sand verification should be automatic.

That means:

- parse and validate package metadata automatically during preview, install, and publish
- verify package format automatically for both `.html` and `.lince`
- if signature-based official verification is later enabled, perform that verification automatically too
- do not require extra user approval for package or signature validation steps

=== Retention and pruning

Recommended default retention policy for published sand artifacts:

- keep the current canonical version
- keep the previous two versions
- never prune versions younger than 30 days
- prune only after a successful publish of a newer version

Local built-in sand copies under `~/.config/lince/web/sand` are reproducible artifacts.

Those local generated copies do not need the same historical retention as bucket-published releases.

=== Migration boundary

The GitHub-backed DNA catalog described elsewhere in the documentation should be treated as transitional.

Once bucket-backed DNA publication and dedicated organ endpoints exist, runtime discovery and installation should use the bucket-plus-record flow described here.

== Extensions

This document is a planning note for a future public extension hub for Lince.

Read `web_components.typ` first for:

- runtime contracts
- host and bridge assumptions
- runtime shape guidance
- standalone versus official widget packaging

Read `kanban.typ` for one concrete example of an official widget that is not only "some HTML", but a hybrid runtime surface with a stronger contract.

Current code is more authoritative than this document when there is conflict.
This note is intentionally incomplete and keeps unresolved issues visible instead of pretending they are solved.

=== Goal

The intended direction is:

- `lince` is a producer only
- `dna` is the public extension hub and canonical catalog
- `dna` owns checksum generation
- `dna` owns canonical writes, promotion, renames, and deletion
- community submission is open
- official publication is curated
- `upsert` is the normal write operation
- hard delete is allowed and explicit

This means the extension hub is not the same thing as widget authoring.
It is the catalog, packaging, ingestion, and governance layer around authored artifacts.

=== Why this needs its own plan

The present web component documentation already distinguishes:

- official `sand/` widgets
- standalone imported widgets
- different runtime shapes

But an extension hub has to classify more than runtime style.
It also has to classify:

- trust level
- package family
- ingestion path
- canonical storage
- compatibility and runtime expectations

That is why runtime descriptions alone are not enough.

=== Primary axes

The extension hub should model at least three different axes:

#table(
columns: (1.5fr, 1.7fr, 2.8fr),
[Axis], [Examples], [Why it matters],
[Channel], [`official`, `community`], [This is the hard trust and governance boundary. It should not be inferred from runtime style.],
[Family], [`sand`, `db`], [Different families have different payloads, validation rules, and user expectations.],
[Runtime requirements], [`standalone iframe HTML`, `host-bound stream widget`, `foreign integration`, `engine-driven surface`], [Useful for install warnings and human understanding, but too descriptive to be the main storage category.],
)

Rules:

- channel is not runtime shape
- family is not runtime shape
- runtime shape is not a permission boundary
- runtime notes do not define the canonical folder layout

=== Relationship to runtime guidance

The runtime guidance in `web_components.typ` remains useful, but only as documentation for how a widget behaves.

They help answer:

- where truth lives
- how truth reaches the surface
- who shapes the UI
- where interaction state lives
- how foreign or portable the runtime is

They do not answer:

- whether an artifact is official or community
- whether the payload is a widget or a database example
- how checksums are owned
- how deletion or promotion works
- how the hub should store the artifact on disk

This matters because a runtime label can look like a package type when it is not.

Examples:

- `kanban` is a server-shaped board with rich local state, but that does not make "server-shaped" a package family
- a foreign integration widget and a client-owned renderer may both still be ordinary `sand` packages
- a database example does not need runtime-shape labeling at all

=== Kanban as warning

`kanban.typ` is useful here because it shows a real official widget is:

- hybrid
- host-configured
- instance-aware
- stream-backed
- action-oriented
- not portable in the same way as a standalone imported HTML widget

That means the extension hub cannot treat every extension as if it were just a static asset plus a friendly name.

At minimum, official widget metadata will likely need to describe:

- permissions
- required host configuration
- runtime contract style
- compatibility expectations
- whether the package is meant for import, direct hosting, or internal official use

=== Planned hub responsibilities

The intended split is:

- `lince` may produce code, HTML, source snapshots, or other raw extension payloads
- `lince` does not own canonical publication
- `dna` ingests submissions from a lounge area
- `dna` validates the submission according to family rules
- `dna` computes checksums
- `dna` performs upsert into the canonical tree
- `dna` performs hard delete when explicitly requested
- `dna` rebuilds the search catalog TOML

Community submission being "free to push" should mean:

- contributors can submit without maintainer curation before ingest into the community catalog
- `dna` still applies schema and safety validation
- the hub still has a canonical search TOML and checksum records
- maintainers may still rename, replace, or delete packages when they decide to

It should not mean:

- no validation
- no naming rules
- no canonical metadata

=== Near-term on-disk direction

The practical near-term direction should be family-first and channel-explicit on disk.
Packages should be sharded by the first two letters of the package name, similar to the broad shape used by `nixpkgs`.
There should be no JSON catalog file.
Instead, the hub should maintain one search TOML for `sand`.

Suggested direction:

```text
dna/
  lounge/
    <package-name>/
      index.html?        # required for sand packages
      sand.toml?         # sand package metadata
      *.sqlite
      ...
  sand/
    catalog.toml
    official/
      <prefix>/
        <package-name>/
          <package-name>.html
          <package-name>_metadata.html
          sand.toml
          sha256.txt
    community/
      <prefix>/
        <package-name>/
          <package-name>.html
          <package-name>_metadata.html
          sand.toml
          sha256.txt
  db/
    official/
      <prefix>/
        <package-name>/
          ...
          sha256.txt
    community/
      <prefix>/
        <package-name>/
          ...
          sha256.txt
```

Example:

```text
sand/official/he/hello_world/
```

Here the package name is the stable package identity used in the path.

Rules:

- package names are first-come, first-served
- the package name is the stable package id
- package names must be `lower_snake_case`
- there must not be two packages with the same name in the same family across `official` and `community`
- `official` has naming priority over `community`
- when a community package conflicts with an official package, the community package must be renamed
- maintainers may rename community or official packages when needed
- the prefix is derived from the first two letters of the `lower_snake_case` package name
- if the package name has fewer than two letters, use the available letters
- runtime notes do not affect the on-disk layout

This keeps the hard official versus community split visible while avoiding one flat directory with too many entries.

=== Catalog TOML

The hub should keep one search TOML for `sand`.

Suggested direction:

```text
sand/catalog.toml
```

Lookup rule:

1. Normalize the package name.
2. Open `sand/catalog.toml`.
3. Read the package record from there instead of scanning the whole tree.

Suggested `sand` catalog shape:

```toml
[packages.hello_world]
title = "Hello World"
description = "Example package"
path = "community/he/hello_world"
```

Rules:

- the catalog is the fast lookup layer
- the canonical package directory remains the artifact source of truth
- `sand` should have only one search TOML in the first version
- the catalog must be updated on create, upsert, rename, promotion, and delete

=== Rename history

Package renames should be recorded.

The simplest direction is a per-family migrations ledger such as:

```text
sand/migrations.txt
db/migrations.txt
```

Each rename entry can be append-only and human-readable, for example:

```text
2026-03-29T12:00:00Z community/tes -> community/test
2026-03-29T12:10:00Z community/hello_world -> official/hello_world
```

This does not need to be a heavy migration system.
It only needs to preserve package rename history in a form that is easy to inspect.

=== Package metadata

The hub should not use `manifest.json` as the package metadata file.

Use family-specific TOML files instead:

- `sand.toml` for web components

For `sand`, the package should normally contain:

- `index.html`
- `sand.toml`
- optional extra assets if the package format later allows them

For now, extra assets should be allowed broadly.
That includes arbitrary supporting files and directories.
Asset hygiene can stay as a manual maintainer responsibility rather than an automated ingest restriction.

In canonical storage, the ingested package should normally contain:

- `<package-name>.html`
- `<package-name>_metadata.html`
- `sand.toml`
- optional extra assets if the package format later allows them

The family-specific TOML file is where package metadata belongs.
There is no need for one generic package manifest for all families in the first version.

For `db`, metadata remains intentionally unspecified for now.
At this stage, `db` is just storage for SQLite database files and is not the center of the automated metadata plan.

==== `sand.toml`

`sand.toml` should be closer in spirit to `Cargo.toml` package metadata than to an ad-hoc JSON manifest.

Suggested first version fields:

- `name`: required
- `channel`: required, either `official` or `community`
- `version`: required
- `author`: required
- `description`: required
- `title`: optional
- `icon`: optional
- `details`: optional
- `initial_width`: optional
- `initial_height`: optional
- `permissions`: optional list of strings
- `tags`: optional list of strings

Rules:

- `name` must be `lower_snake_case`
- `name` must be at most `200` characters
- `name` must match `^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$`
- `name` must not be one of the reserved names `official`, `community`, `sand`, `db`, `lounge`, `map`, `migrations`, `con`, `prn`, `aux`, `nul`, `com1` through `com9`, or `lpt1` through `lpt9`
- `version` must be valid semantic versioning text
- `initial_width`, if present, must be an integer from `1` to `6`
- `initial_height`, if present, must be an integer from `1` to `6`
- the lounge directory name and `sand.toml` `name` should agree

`sand.toml` should be the source of truth for package name and channel during ingest.

Defaults for metadata-expanded HTML generation:

- `title` defaults to a humanized form of `name`
- `details` defaults to `description`
- `initial_width` defaults to `4`
- `initial_height` defaults to `3`
- `permissions` defaults to `[]`

==== Future compatibility metadata

Simple standalone `sand` packages do not need much compatibility metadata beyond ordinary package fields.

Official or host-bound widgets may later need optional compatibility fields such as:

- `required_permissions`: optional list of required host permissions
- `required_host_meta`: optional list of required host metadata keys such as `server_id` or `view_id`
- `host_contract_version`: optional string describing the expected host runtime contract
- `min_lince_version`: optional semantic version describing the minimum supported Lince version

If these fields are absent, the conservative assumption should be that the package is a simpler standalone-style package rather than a strongly host-bound official runtime surface.

If these fields are present:

- ingest should validate their syntax and basic semantics
- hub ingest should reject malformed compatibility metadata
- runtime compatibility should be enforced when the package is installed or executed, not by guessing during catalog ingest

==== Sand HTML files

When a `sand` package is ingested:

- the lounge `index.html` should be copied into canonical storage as `<package-name>.html`
- `dna` should also generate `<package-name>_metadata.html`

The purpose of `<package-name>_metadata.html` is to provide the current embedded metadata block expected by the existing standalone widget contract.

When generating that embedded metadata block:

- manifest `icon` should come from `sand.toml` `icon`
- manifest `title` should come from `sand.toml` `title`, or fall back to a humanized form of `name`
- manifest `author` should come from `sand.toml` `author`
- manifest `version` should come from `sand.toml` `version`
- manifest `description` should come from `sand.toml` `description`
- manifest `details` should come from `sand.toml` `details`, or fall back to `description`
- manifest `initial_width` should come from `sand.toml` `initial_width`, or default to `4`
- manifest `initial_height` should come from `sand.toml` `initial_height`, or default to `3`
- manifest `permissions` should come from `sand.toml` `permissions`, or default to `[]`

If the source HTML already contains embedded metadata, `dna` may still regenerate the metadata version so that the canonical metadata-expanded file is deterministic.

This keeps:

- the original author HTML, preserved under the package name
- a metadata-expanded HTML file, compatible with the current embedded-manifest expectations

=== Package identity

The simplest direction is:

- the package id is the package name
- the package id does not change
- upsert replaces the current canonical package at that path
- hard delete removes that path

This is a deliberately simple model.
It avoids introducing a second hidden identifier before there is a real reason for one.

=== Validation

For now, only `sand` needs a stronger validation story.

The minimum first-version validation for `sand` should be:

- `index.html` exists
- `sand.toml` exists
- `index.html` passes a loose HTML validity check through `HTMLHint`
- `sand.toml` matches the expected schema
- the package path and the `name` declared in `sand.toml` agree
- the `channel` declared in `sand.toml` is valid
- the `version` declared in `sand.toml` is valid semantic version text
- `permissions`, if present, is a list of strings
- the package `name` fits `lower_snake_case`
- the package `name` is at most `200` characters

The HTML validation should stay loose.
It only needs to confirm that the document is valid enough to be treated as HTML, more like an HTML linter than a strict runtime certification.

The ingest command should show short user-facing error messages on validation failure.
Recommended messages:

- `missing sand.toml: sand packages require sand.toml`
- `missing index.html: sand packages require index.html`
- `invalid package name 'Hello World': use lower_snake_case with at most 200 characters`
- `invalid channel 'beta': expected 'official' or 'community'`
- `invalid version '1.0': expected semantic versioning such as '1.0.0'`
- `version must increase: current canonical version is '0.2.0'`
- `community package 'hello_world' conflicts with official package 'hello_world': choose another name`
- `html validation failed: document is not valid enough for sand ingest`
- `reserved package name 'official': choose another name`

`HTMLHint` should be the first-version linter.
The practical tooling direction is to have `mise ingest` install or expose `HTMLHint` so the ingest command has the linter available.

=== Checksums

`dna` owns checksum generation.

The first version does not need checksum history or lineage tracking.
It only needs the current checksum of the canonical package written to `sha256.txt`.

The checksum should be computed across the canonical package files in alphabetical relative-path order.

The deterministic hashing rule should be:

- sort canonical file paths alphabetically
- feed each relative path and its file bytes into the hash in that order

This avoids ambiguity once a package contains more than one file.

On upsert:

- require a semantic version bump relative to the current canonical package
- recompute the checksum
- overwrite `sha256.txt`
- update the corresponding catalog entry

On hard delete:

- remove the package directory
- remove the corresponding catalog entry
- leave the filesystem and migrations ledger as the remaining source of truth

=== Lounge ingestion

The ingest path should be `lounge/`, not `.lounge/`.

The simplest direction is:

- `lounge/<package-name>/...`

The ingest command can infer the family from the package contents.

The first version inference rule should be:

- if `sand.toml` is present, treat the package as `sand`
- `sand` requires `index.html`
- if `sand.toml` is absent, the package is not part of the automated sand ingest flow

For now, `db` remains out of the automated metadata validation path.
Maintainers may still place SQLite database files in the `db/` tree manually.

That is enough for now because `sand` is the only fully-specified automated package flow in this plan.

=== Practical operations

==== Create

For creation:

1. Put the package under `dna/lounge/<package-name>/`.
2. Run the `dna` ingest command.
3. `dna` treats the package as `sand` if `sand.toml` is present.
4. `dna` validates the package according to the `sand` rules.
5. `dna` reads the target channel from package metadata.
6. `dna` checks for a name collision in that family across `official` and `community`.
7. If the package is community and the name is taken by an official package, ingest must reject it until the community package uses another name.
8. `dna` computes the checksum.
9. `dna` creates the canonical directory if it does not already exist.
10. `dna` writes the canonical package into the chosen family and channel.
11. `dna` writes or updates the corresponding catalog entry.

This gives the operator the simple workflow of "put it somewhere, run one command, done".

==== Upsert

For updates:

1. Reuse the same package name.
2. Submit the new package contents.
3. Run ingest again.
4. `dna` validates again.
5. `dna` reads the target channel from package metadata.
6. `dna` requires the submitted semantic version to be greater than the current canonical version.
7. `dna` replaces the canonical package at the same path.
8. `dna` recomputes checksum.
9. `dna` updates the corresponding catalog entry.

==== Rename

For renames:

1. A maintainer chooses the new package name.
2. The rename must include a semantic version bump because the package metadata and canonical filenames change.
3. `dna` moves the canonical package to the new path.
4. `dna` updates the package metadata name.
5. `dna` appends a line to the relevant family migration file.
6. `dna` updates the old and new catalog entries.

This is the main administrative escape hatch for collision handling and curator intervention.

==== Promotion

For promotion from `community` to `official`:

1. A maintainer selects the existing community package.
2. If the target official name is already used by another official package, promotion is rejected.
3. If an official ingest or promotion takes a name currently held by `community`, `dna` should auto-rename the displaced community package, patch-bump it, and write that rename to the migration file.
4. Promotion must include a semantic version bump because the package metadata changes.
5. `dna` moves the package from `community` to `official`.
6. `dna` updates the package metadata channel.
7. `dna` appends a line to the migration file recording the channel move.
8. `dna` updates the catalog entry.

Promotion should be a move, not a copy.
The package keeps the same name unless a maintainer explicitly renames it.

==== Hard delete

For deletion:

1. Submit a delete request or invoke an explicit delete command with the family, channel, and package name.
2. `dna` removes the canonical package directory.
3. `dna` removes the catalog entry.

The plan does not try to soften the consequences of hard delete.

=== Recommended practical direction for now

The most practical direction today is:

- keep only `sand` and `db`
- store packages by family, then channel, then two-letter prefix, then package name
- use `sand.toml`
- use one `sand/catalog.toml` file for search
- treat the package name as the stable id
- keep package names unique per family, with `official` taking precedence over `community`
- use `lounge/` as the operator drop zone
- keep `official` and `community` as the hard boundary
- let `dna` own checksum and canonical writes
- let `lince` remain a producer only
- require `lower_snake_case` names
- require semantic versions for `sand`
- use loose `HTMLHint` validation for `sand`
- keep runtime expectations in documentation and compatibility fields
- treat `db` as manual storage for now rather than part of the automated metadata flow

This is much simpler than a generalized hub model, and it matches the current stage of the project better.

=== Criticisms of the current direction

Several criticisms remain even after simplifying the design:

- First-come, first-served naming is simple, but the real answer to disputes is maintainer override rather than a neutral policy.
- Using the package name as the stable id is pragmatic, but it means renames need explicit migration bookkeeping.
- The two-letter prefix sharding is practical, but it makes names and path normalization a hard policy boundary instead of a forgiving one.
- Official widgets like Kanban still need richer runtime metadata than a generic standalone HTML package, even if the storage layout stays simple.
- A single `sha256.txt` is enough operationally, but it intentionally gives up richer provenance.
- A single `sand/catalog.toml` is simpler than many map files, but it becomes one shared file that changes on every catalog mutation.
- Allowing arbitrary assets keeps the system flexible, but it also pushes all asset hygiene and review onto maintainers.

=== Concrete decisions

The remaining policy points should be treated as decided for the current plan:

- invalid package names are rejected if they fail the `lower_snake_case` pattern, exceed `200` characters, or use a reserved name
- official-widget compatibility metadata is optional, but if present it must pass schema validation during ingest
- `sand.toml` may later grow `homepage`, `repository`, and compatibility fields without changing the current package identity model
- every canonical package change requires a semantic version bump, including rename and promotion
- if `db` later becomes automated, it should get its own metadata file and, if needed, its own search TOML

=== Working conclusion

The extension hub can be much simpler than the first draft.

The current best direction is:

- `sand` and `db` only
- `sand.toml` drives the automated ingest flow
- one `sand/catalog.toml` avoids full directory scans
- package name as stable id
- two-letter path sharding
- one `lounge/` drop zone
- `dna` owns checksum and canonical writes
- `official` and `community` stay explicit
- `sand` gets its channel from `sand.toml`
- promotion is a move, renames are recorded in migrations, and both update `sand/catalog.toml`

Runtime guidance still matters for understanding widget behavior, but it does not need a dedicated taxonomy field or storage role in this plan.

=== Next board step

After the ingest and catalog rules are in place, the next practical UI step should be board-side package pickup from published organ records whose artifacts live in the bucket under the `lince/dna/sand` tree.

The intended direction is:

1. Add one more choice to the existing `Add card` popover for remote catalog pickup.
2. Label it something direct such as `Hub` or `DNA`.
3. Keep the existing `Importar` and `Local` flows unchanged.
4. Open a new Maud-rendered modal instead of navigating away or inventing a second screen.

The modal should follow the same broad shape as the current local catalog modal:

- search field at the top
- short catalog summary text
- scrollable list of matching package names
- one clear action per result to install and add the card

Suggested HTML shape in the page template:

```text
button#add-card-dna-button
div#dna-packages-modal-backdrop
section#import-modal.import-modal--catalog
input#dna-packages-search
div#dna-packages-summary
div#dna-package-list
```

This should be rendered in Maud next to the existing local catalog modal, not assembled ad-hoc in client JavaScript.

The search path should use one catalog TOML rather than full repository scans.

Practical rule:

1. Normalize the user query to the package-name shape.
2. Take the first two letters.
3. Fetch `sand/catalog.toml` from `main`.
4. Filter the package records in that TOML file on the client or through the backend proxy.
5. Render the matching package rows in the modal.

The first version can stay simple:

- search by package name first
- treat empty search as "show nothing yet" or "type at least two letters"
- only target `sand`
- prefer `official` entries first when both channels are ever shown together

The fetch source should be the `main` branch of the public `dna` repository.
The safer runtime path is a backend proxy in `lince`, even if the upstream source is GitHub, because that avoids client-side CORS assumptions and keeps remote fetching inside the server boundary.

Suggested remote sources:

- `sand/catalog.toml` for search
- `sand/<channel>/<prefix>/<package-name>/sand.toml` for package metadata
- `sand/<channel>/<prefix>/<package-name>/<package-name>_metadata.html` for installation

The installation path should reuse the current imported-widget flow as much as possible.

Practical direction:

1. The user selects a package row in the modal.
2. `lince` fetches that package's `sand.toml` and `<package-name>_metadata.html` from `dna/main`.
3. `lince` turns that into the same preview payload shape already used for local/imported widgets.
4. The user sees the normal preview and confirms.
5. On confirm, `lince` stores the downloaded widget into the local package catalog and creates the card.

This means the remote hub pickup should land in the same local installed-catalog path as other widgets after download.
The hub is the source, but the board still works with a local installed package once the user adds it.

The shortest implementation sequence should be:

1. add `Hub` or `DNA` to the `Add card` popover
2. add the Maud modal shell
3. add backend endpoints that proxy GitHub `main` TOML and HTML fetches
4. add client search + result rendering using `sand/catalog.toml`
5. add preview + confirm using the existing imported-widget card creation path

==== Board UX plan

The board-side experience should be:

1. Enter edit mode.
2. Open `Add card`.
3. Choose `DNA`.
4. The modal opens with an empty result list and a message such as `Digite pelo menos duas letras para buscar no hub.`
5. After the user types two or more characters, the board searches the hub.
6. The modal shows matching package rows with enough metadata to decide quickly.
7. Selecting a row opens the same preview surface already used for imported widgets.
8. Confirming the preview downloads the package into the local installed catalog and creates the card.

The first version should keep the interaction narrow:

- one search field
- one result list
- one preview-confirm path
- no multi-select
- no background synchronization

==== DNA search files

For the board flow, `dna` should expose one TOML search file in `main`:

```text
sand/catalog.toml
```

The purpose of `sand/catalog.toml` is:

- provide the search rows for the modal
- avoid repository directory scans
- keep one compact lookup file for the whole `sand` family

Suggested shape:

```toml
[packages.hello_world]
title = "Hello World"
description = "Example package"
path = "official/he/hello_world"
```

The package name lives in the TOML key.
The row metadata for search is only:

- `title`
- `description`
- `path`

==== Search behavior

The search contract should be simple and deterministic:

1. trim the query
2. lowercase it
3. normalize spaces and dashes into `_`
4. reject characters that do not fit package-name search
5. if fewer than two characters remain, do not hit the network
6. fetch `sand/catalog.toml`
7. filter the package rows by package name first
8. if desired, also match `title` and `description`

Result ordering should be:

1. exact package-name prefix match
2. package-name substring match
3. title match
4. description match
5. alphabetical by title

The first version should fetch the whole `sand/catalog.toml` file for each search refresh or from cache.
That keeps the search contract simple.

==== Modal contents

Each result row in the Maud modal should show:

- icon
- title
- package name
- short description

Each row should have one explicit action:

- `Preview`

The modal should not install immediately from the search list.
Preview first, then confirm through the existing preview modal.

==== Backend route plan

The backend should own all remote GitHub fetches.
The browser should never fetch raw GitHub URLs directly.

Suggested new API surface:

```text
GET  /api/packages/dna/catalog
GET  /api/packages/dna/search?q=<query>
GET  /api/packages/dna/{channel}/{package_name}/preview
POST /api/packages/dna/{channel}/{package_name}/install
```

Responsibilities:

- `catalog`: fetch and cache `sand/catalog.toml`
- `search`: normalize the query, fetch `sand/catalog.toml`, filter rows, and return lightweight summaries
- `preview`: fetch `sand.toml` plus `<package-name>_metadata.html`, parse them into the same preview payload shape already used by `/api/packages/local/{package_id}` and `/api/packages/preview`
- `install`: fetch the same remote package, persist it into the local widget catalog, and return the installed preview payload

This keeps the remote-hub logic inside the backend and lets the front-end reuse the existing package UI model.

==== Remote source rules

The first version should fetch only from:

- accessible organs exposed in the local host catalog
- published `record_extension(namespace = "lince.dna")` entries
- canonical `record_resource_ref(provider = "bucket", resource_kind = "sand")` artifacts under `lince/dna/sand`

The backend should translate those publication records to authenticated bucket fetches.

The expected remote files for a package are:

- `sand.toml`
- `<package-name>_metadata.html`

The board should not download the plain `<package-name>.html` file for installation in the first version.
The metadata-expanded HTML is the correct transport because it already satisfies the current embedded-manifest package contract used by local install and preview flows.

==== Local install rule

When `lince` installs a package downloaded from `dna`:

1. fetch `<package-name>_metadata.html`
2. parse it as a normal Lince package
3. persist it locally under the filename `<package-name>.html`

This rule is important.
The remote transport file is `<package-name>_metadata.html`, but the local installed filename should still be `<package-name>.html`.
Otherwise the local package id would drift to `<package-name>_metadata`, which is wrong.

The installed package should land in the same local catalog used today for uploaded widgets:

```text
~/.config/lince/web/widgets/<package-name>.html
```

After that, the package behaves like any other locally installed widget.

==== Collision and overwrite rule

For the first version, the simplest local rule should be:

- if the package is already installed locally and the downloaded version is newer, overwrite the local installed copy
- if the package is already installed locally and the downloaded version is the same, replace it anyway if the user explicitly installs it from the hub
- if the package exists only as a built-in official sand widget, allow the downloaded local copy to shadow it

This is explicit user intent.
Choosing a package from the hub is enough reason to let the local installed copy win over the built-in rendered official copy.

==== Caching rule

The backend should cache remote TOML and HTML responses in memory.

The first version can stay simple:

- cache `catalog.toml` for 5 minutes
- cache preview payloads for 1 minute

Cache invalidation does not need to be perfect.
The branch is `main`, not a versioned immutable archive, so short-lived stale reads are acceptable for the first version.

==== Error handling

The modal and preview flow should surface short concrete errors:

- `Falha ao buscar o catalogo do hub.`
- `Nenhum pacote corresponde a essa busca.`
- `Falha ao baixar o pacote selecionado.`
- `O pacote remoto nao possui metadados validos para instalacao.`
- `Falha ao instalar o pacote do hub no catalogo local.`

The modal should stay open on search and preview errors.
Only a successful install should close the remote modal and continue into the normal add-card success path.

==== Data shape reuse

The implementation should reuse the existing local package data shapes as much as possible.

Practical rule:

- search rows should look like a minimal summary built from package name plus `title`, `description`, and `path`
- preview responses should look like the existing `PackagePreview`
- install responses should also return `PackagePreview`

This keeps the new `DNA` path close to the existing local/imported widget path and avoids inventing a second package model in the front-end.

==== Implementation order

The practical implementation order should be:

1. extend `dna` ingest/rebuild so it writes `sand/catalog.toml`
2. add `DNA` to the add-card popover in Maud
3. add the Maud modal shell for remote search
4. add backend routes for `catalog`, `search`, `preview`, and `install`
5. add backend GitHub fetch + in-memory cache
6. add client-side search state and result rendering from `sand/catalog.toml`
7. route row click into the existing preview-confirm flow
8. persist the downloaded package locally as `<package-name>.html`
9. create the card using the existing imported-widget card creation path

# Kanban

Para fazer o Kanban funcional, precisamos de features do back, e front. A parte do back ja esta bem encaminhada. O front atual possui diversas features implementadas com IA, chamando os endpoints certos, demonstrando a ideia. Mas o componente esta muito ruim. A primeira passada foi so pra conectar os pontos. Precisamos de design e de um frontend feito mais na mao talvez, porque o gpt mesmo quando pedimos usa muito javascript pra coisas que provavelmente datastar faz mais facil. As tasks marcadas como feitas abaixo sao dessa primeira passada, precisam ser refeitas todas com cuidado principalmente no que diz respeito ao front. Mas as funcionalidades basicas estao la. CRUD, sse, assign, comentarios... As seguintes funcionalidades precisam ser feitas com maestria:

- [ ] Salvar estado do componente no linceconfigdir(.config/lince)/web/board-state.json
- [x] Puxar dados de uma View SSE de acordo com o ID da View.
- [x] Ler os dados que chegam apenas como `Record`; rejeitar outras tables e mostrar erro curto de incompatibilidade.
- [x] Fazer CRUD de `Record` dentro do Kanban: criar, editar `head`, editar `body`, alterar `quantity` clicando e arrastando pelas colunas e deletar cards.
- [x] Mapear colunas por `quantity`: `0` Backlog, `-1` Next, `-2` WIP, `-3` Review, `1` Done (o que n faz sentido nenhum mas por agr ta de boa).
- [ ] Deixar o usuário configurar qual quantidade envia os cards pra qual coluna.
- [x] Scroll vertical.
- [x] Permitir scroll horizontal do board inteiro quando as colunas excederem a largura.
- [x] Permitir alterar a largura de cada coluna clicando e arrastando ela.
- [x] Permitir minimizar/recolher colunas.
- [x] Skipar (Fica sempre escondido o body mostrando só a Head do Record): Permitir alternar a visualizacao do `body` de cada card especifico entre: oculto, pequeno trecho, e completo.
- [x] Skipar: Botao para alternar visualizaçao de todos os cards.
- [x] Skipar: Mostrar botao para pausar o recebimento de dados via SSE.
- [ ] Informar claramente se o widget esta com uma conexao SSE viva ou a conexão com o servidor caiu, os dois precisam estar ok para mostrar Live/Conectado ou Offline/Desconectado.
- [ ] Bucket: Conseguimos usar o widget the bucket pra mostrar imagens. Dentro do modo foco temos como colocar anexos (pede um path do bucket). Quando salvamos mostra o titulo que escolhemos colocar pra esse anexo no card. Soh nao mostrah a imagem.
- [ ] Modo pretty de markdown permite editar o conteudo sem ir para o modo raw de markdown. Precisamos de radio buttons pra alternar entre os tres
- [x] Inicio e fim da task datas
- [x] Estimativa de tempo
- [x] Consumo real: varios worklogs que tem um start e um end dentro do modo de foco do card.
- [x] atribuir a uma pessoa
- [x] Hierarquia de tarefas
- [x] status de kanban (quantidade)
- [x] comentários em tasks
- [x] Sorting by many different properties

Here’s the practical summary.

This spec is for a **Record-based Kanban widget** in the web app. The board is supposed to be **server-shaped**, not client-owned: the backend owns the structure and streams HTML fragments plus signal patches, while the frontend keeps only interaction-heavy local state like drag/drop, resize, scroll restoration, and some UI ergonomics. The stack split is: **Maud** for server-rendered structure, **Datastar** for reactive shell state and form state, and **JavaScript** only for imperative stuff that should stay imperative.

## Endpoints it calls

Main runtime contract:

- `GET /host/widgets/{instance_id}/contract`
- `GET /host/widgets/{instance_id}/stream`
- `POST /host/widgets/{instance_id}/actions/{action}`

It explicitly says the final Kanban should **not** depend directly on:

- `/host/integrations/servers/{server_id}/views/{view_id}/stream`

### Concrete actions under `/actions/{action}`

The semantic actions listed are:

- `load-record-detail`
- `create-record`
- `update-record`
- `move-record`
- `delete-record`
- `start-worklog`
- `stop-worklog`
- `apply-filters`

There is also one explicit route called out for focus/detail loading:

- `POST /host/widgets/{instance_id}/actions/load-record-detail`

## How the backend is supposed to look

At a high level, the backend shape is this:

1. A widget instance points to a real `server_id` and `view_id`.
2. The backend reads the widget config and `widgetState`.
3. It validates that the source view exposes at least `id`, `quantity`, `head`, `body`.
4. It builds a **derived SQL wrapper** from structured filters stored in `widgetState`.
5. It creates or reuses a derived view like `__lince_web_kanban_{instance_id}`.
6. The widget streams from that derived view, not directly from the source view.

So the backend is basically **instance-aware orchestration around a saved base view**, not a dumb pass-through stream.

## Backend service structure

The preferred host-side service split is:

- `KanbanContractService`
- `KanbanStreamService`
- `KanbanFilterService`
- `KanbanDetailService`
- `KanbanActionService`
- `KanbanWorklogService`
- `KanbanLivenessService`

What each one should do:

### `KanbanContractService`

Resolves widget metadata, validates the configured base view against the normalized Kanban contract, and serves `/host/widgets/{instance_id}/contract`.

### `KanbanStreamService`

Opens the instance-aware stream, resolves server/base view, asks the filter service for the SQL wrapper, subscribes to the filtered view stream, and emits Datastar fragments/signals.

### `KanbanFilterService`

Owns filter persistence and translation. It reads structured filters from `widgetState`, validates them, derives parameterized SQL, and tracks `filters_version`. Importantly, filtering must happen in **SQLite**, not in Rust after streaming.

### `KanbanDetailService`

Loads the detailed focused card by `record_id`, including comments, child summaries, assignees, resources, and worklog intervals.

### `KanbanActionService`

Handles record-level semantic actions like create, update, move, delete, and apply-filters. It also owns task-type/category CRUD and should reject invalid hierarchy writes like self-parent or obvious cycles.

### `KanbanWorklogService`

Handles `start-worklog` and `stop-worklog`, enforces one open interval per `(record_id, author_user_id)`, updates heartbeats, and refreshes worklog summaries.

### `KanbanLivenessService`

Owns heartbeat/ping behavior and stale/disconnected detection.

## Data model / tables the backend expects

The board is built around `record` as the core truth, with optional metadata stored in side tables. Required fields in the projection are:

- `id`
- `quantity`
- `head`
- `body`

Optional projected fields include categories, dates, estimates, actual work, assignees, parent/child data, task type, comments, and open worklog count.

Recommended tables:

- `record_extension`
- `record_link`
- `record_comment`
- `record_worklog`
- `record_resource_ref`

Mapping:

- task categories → `record_extension` namespace `task.categories`
- start/end dates → `record_extension` namespace `task.schedule`
- estimate → `record_extension` namespace `task.effort`
- task type → `record_extension` namespace `task.type`
- assignees → `record_link` with `assigned_to`
- hierarchy → `record_link` with `parent`
- comments → `record_comment`
- actual work → `record_worklog`

## Query / filtering model

The intended model is:

- one saved base view defines the broad dataset
- widget-local filters live in `widgetState`
- the backend derives SQL from those structured filters
- the frontend reconnects the stream after `apply-filters`
- the base saved SQL is **not** mutated by the widget UI

Important rule: do **not** stream a superset and filter in Rust or JS. Filtering must be SQL-side and parameterized.

## Runtime flow

When the widget opens:

- `/contract` reads card state and filter rows from `widgetState`
- `/stream` reads the same state, loads the base view, validates it, builds derived SQL, and streams from `__lince_web_kanban_{instance_id}`

When filters change:

- widget sends `apply-filters`
- backend persists `widgetState.filters`
- increments `filters_version`
- widget reconnects
- backend rebuilds the derived query and starts a fresh filtered stream

## Frontend/backend ownership boundary

This matters because the doc is strict about it:

- **Backend / Maud**: shell, columns, cards, sheets, focus surfaces, repeated structural fragments
- **Datastar**: signals, drafts, loading flags, connection state, persisted UI ergonomics
- **JS**: drag/drop, resize, scroll restoration, offline stop queue, heartbeat loop, thin transport interop

So the backend should render most DOM structure; the browser should not become the main renderer.

## Short “how it should be structured” diagram

```txt
Widget instance
  -> KanbanContractService
       reads widget config + widgetState
       validates source view contract

  -> KanbanFilterService
       reads structured filters
       builds parameterized SQL wrapper

  -> KanbanStreamService
       resolves base view
       creates/reuses __lince_web_kanban_{instance_id}
       streams summary payload/fragments via SSE

  -> KanbanActionService
       create/update/move/delete/apply-filters
       writes record + sidecar/link data

  -> KanbanDetailService
       load-record-detail for focused card

  -> KanbanWorklogService
       start/stop worklogs + heartbeat summary updates

  -> KanbanLivenessService
       heartbeat, stale, reconnecting, disconnected states
```

## Main takeaway

This backend is supposed to be a **widget-instance-aware Kanban runtime**, not just a raw DB view stream. The key idea is:

- saved base view for dataset
- per-widget structured filters in `widgetState`
- SQL-derived filtered view per widget instance
- semantic action endpoints
- server-rendered fragments + signal patches
- sidecar tables for metadata and worklog/comments/hierarchy

If you want, I can turn this into a cleaner backend module layout in Rust, like folders/files and structs/traits for each service.
