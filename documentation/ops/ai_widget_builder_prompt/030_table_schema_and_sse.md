Table and SSE guidance:

- Table endpoints are generic and table names are path parameters.
- Rows are JSON objects.
- Mutations use JSON object request bodies.
- Read and write flows should expect an integer `id` field for row identity when dealing with individual rows.
- CRUD widgets should be explicit about which table they operate on.
- If the current user prompt names a specific table, constrain all calls to that table.
- For record-mutation widgets, use the host-mediated record path `/host/integrations/servers/{server_id}/table/record/{id}` when a remote server is involved.

Current core schema reference derived from migrations:

- `record`: `id`, `quantity`, `head`, `body`
- `view`: `id`, `name`, `query`
- `collection`: `id`, `quantity`, `name`
- `configuration`: `id`, `quantity`, `name`, `language`, `timezone`, `style`, plus later config columns added by migrations
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

SSE view guidance:

- `GET /api/backend/sse/view/{view_id}` is the underlying backend endpoint.
- Widgets that run in the board should usually connect through the host-mediated stream path `/host/integrations/servers/{server_id}/views/{view_id}/stream`.
- `view_id` is an integer identifier for an existing saved view.
- The stream emits named SSE events.
- Event `snapshot` carries the latest JSON payload for the view snapshot.
- Event `error` carries a JSON payload describing a backend or subscription error.
- Widgets that consume this stream should reconnect cautiously and keep the UI usable while waiting for the first snapshot.
- Widgets should not assume that a workspace switch remounts the iframe. Persist internal UI state explicitly.
- Prefer host-persisted per-widget UI state for lane sizes, collapsed sections, filters, or display modes. Use `localStorage` only as a preview or offline fallback.
- If the widget expects a particular schema, validate the payload and show a compact mismatch state when the shape does not match.
- When reporting a mismatch, log a short form of the expected shape and the received shape to the console.

Authoring rules for backend-aware widgets:

- Prefer one obvious data source per widget: table CRUD or SSE view streaming.
- Do not invent columns or endpoints that are not described here or explicitly requested by the user.
- If the table schema needed by the widget is ambiguous, keep the widget generic and state the assumption in the widget copy or details field.
- For server-backed widgets, prefer host-managed configuration and locked-state behavior over custom login or endpoint forms inside the widget.
