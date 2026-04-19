# Sand

Sand is the browser-side widget system. In the current codebase a sand can be either a plain HTML document or a packaged widget archive, and the host always runs it inside an iframe. Installed widgets are served from `/host/packages/local/by-filename/{filename}/content/index.html`, preview uploads from `/host/packages/previews/{preview_id}/content/index.html`, and raw board cards can still be embedded with `srcdoc` when the card does not point at an installed package.

The broader web app is the board shell around that widget runtime. `BoardState` currently carries `density`, `globalStreamsEnabled`, `activeWorkspaceId`, and `workspaces`, and each `BoardCard` carries layout plus widget binding data such as `kind`, `title`, `description`, `html`, `packageName`, `serverId`, `viewId`, `streamsEnabled`, and `widgetState`. Outside the board, the web surface also includes package preview/install, the DNA catalog and publisher, server management, terminal sessions, trail, and the AI builder.

The embedded manifest lives near the top of the HTML in a `script` tag with `id="lince-manifest"`. The manifest now has `icon`, `title`, `author`, `version`, `description`, `details`, `initial_width`, `initial_height`, `requires_server`, and `permissions`. The host normalizes missing values, clamps `initial_width` and `initial_height` to `1..6`, and treats `permissions` as a deduplicated capability list.

```html
<script type="application/json" id="lince-manifest">
{
  "icon": "▥",
  "title": "Kanban",
  "author": "Lince Labs",
  "version": "1.0.0",
  "description": "Kanban para acompanhar uma view SSE da tabela record.",
  "details": "Resolve o contrato do widget pela instancia do host, consome o stream filtrado e persiste ergonomia local no card.",
  "initial_width": 6,
  "initial_height": 5,
  "requires_server": true,
  "permissions": ["bridge_state", "read_view_stream", "write_records", "write_table"]
}
</script>
```

The accepted upload extensions are still `.html`, legacy `.sand`, and legacy zip-based `.lince`. The host parses all of them through the same package layer, injects the vendored Datastar runtime when needed, and bootstraps the frame with `/host/static/presentation/board/widget-frame-bootstrap.js`. A widget that wants host state talks to `window.LinceWidgetHost`, which currently exposes `getState()`, `getMeta()`, `getCardState()`, `setCardState(nextState)`, `patchCardState(patch)`, `setStreamsEnabled(enabled)`, `invalidateServerAuth(serverId)`, `subscribe(handler)`, `requestState()`, and `print(label)`.

```txt
GET  /host/widget-bridge/state
POST /host/widget-bridge/actions/print
```

The current package layout is still record-first, but it is no longer a vague bucket-only idea. Published sand packages live under the DNA catalog flow and are stored as bucket objects plus database rows. The current publication path is `/host/packages/dna/publish`, with catalog and lifecycle routes at `/host/packages/dna/catalog`, `/host/packages/dna/search`, `/host/packages/dna/publications/{organ_id}/{record_id}/preview`, `/host/packages/dna/publications/{organ_id}/{record_id}/install`, and `/host/packages/dna/publications/{organ_id}/{record_id}` for delete. A current publication writes `record`, `record_extension` with `namespace = "lince.dna"`, and `record_resource_ref` with `provider = "bucket"` and `resource_kind = "sand"`.

The bucket path is now deterministic and versioned. For a Kanban release, the current shape looks like this:

```text
lince/dna/sand/ka/kanban_record_view/1.0.0/
  kanban_record_view_metadata.html
  sand.toml
```

Maintained sands in the repo live in `crates/web/src/sand/`, and the larger ones are split into focused files such as `body.rs`, `styles.rs`, and `script.rs` instead of keeping everything in one file. If a widget vendors code or assets with required notices, the license and credit files stay alongside the widget package and ship with it.

```
crates/web/src/sand/
  kanban_record_view/
    mod.rs
    body.rs
    styles.rs
    script.rs
  relations/
    mod.rs
    body.rs
    script.rs
    LICENSE.txt
```

The practical rule is simple: keep the widget self-contained, keep the host as the authority for persisted state, and use the bridge for coordination instead of pushing large data blobs through it.

# Kanban

Kanban is the record board widget. The current package filename is `kanban-record-view.html`, and the host resolves it by widget instance through `/host/widgets/{instance_id}/contract`, `/host/widgets/{instance_id}/stream`, and `/host/widgets/{instance_id}/actions/{action}`. The widget contract is instance-aware, so it depends on the board card’s `serverId`, `viewId`, `streamsEnabled`, and `widgetState` instead of assuming the source view is already fixed in the frontend.

```txt
GET  /host/widgets/{instance_id}/contract
GET  /host/widgets/{instance_id}/stream
POST /host/widgets/{instance_id}/actions/{action}
```

The contract currently bundles widget metadata, source metadata, permissions, settings, data-contract expectations, filter definitions, relations, semantic actions, and liveness thresholds. In practice that means the widget knows whether the source organ requires auth, whether the session is connected, which columns must exist, and which actions the host is willing to execute. The liveness window is still explicit: heartbeat every 15 seconds, stale after 45 seconds, disconnected after 90 seconds.

The stream still expects the core `record` shape to be present and validated before the UI renders useful content. The required columns are `id`, `quantity`, `head`, and `body`. Optional columns currently include `primary_category`, `categories_json`, `assignee_names_json`, `parent_id`, `parent_head`, `task_type`, `comments_count`, and `active_worklog_count`, among others. If the shape is wrong, the backend returns a mismatch payload instead of pretending the data is fine.

```json
{
  "view_id": 42,
  "name": "Kanban de Produto",
  "query": "select ...",
  "columns": ["id", "quantity", "head", "body", "task_type", "comments_count"],
  "rows": [
    {
      "id": "1",
      "quantity": "0",
      "head": "Revisar cadastro",
      "body": "Ajustar validações do fluxo",
      "task_type": "task",
      "comments_count": "3"
    }
  ]
}
```

The lane key is still `quantity`. The current mapping is `0` for Backlog, `-1` for Next, `-2` for WIP, `-3` for Review, and `1` for Done. The widget does not own that truth; it only interprets it.

`widgetState` is for widget ergonomics, not durable business data. Kanban currently uses it for filter rows, settings, and runtime UI state, and `apply-filters` persists those filters back into the card state before the stream reconnects. The host-side state tables around the widget are still the normal Lince storage model: `record` for the core row, `record_extension` for structured metadata such as `task.categories` and `lince.dna`, `record_link` for relations like parent/assignee, `record_comment` for comments, `record_worklog` for time tracking, and `record_resource_ref` for external resources.

```json
{
  "widget": {
    "instanceId": "card-123",
    "title": "Kanban"
  },
  "source": {
    "serverId": "manas",
    "viewId": 42,
    "effectiveStreamsEnabled": true
  },
  "permissions": {
    "declared": ["bridge_state", "read_view_stream", "write_records", "write_table"],
    "readViewStream": true,
    "writeRecords": true,
    "writeTable": true
  }
}
```

The current semantic actions cover the actual board workflow: load record details, create/update/move/delete records, start/stop/heartbeat worklogs, create/update/delete comments, create/delete resource refs, and apply filters or settings. That is why the widget is host-shaped rather than a raw DB view renderer. The widget only works well when the backend owns the data contract and the frontend stays focused on presentation and interaction.
