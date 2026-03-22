Backend and auth contract for server-backed widgets:

- The backend auth flow is host-owned. Widgets should not render their own login UI.
- Treat remote backend credentials as host-managed and in-memory only.
- Do not store backend tokens in `localStorage`, widget archives, cookies, or board state.
- When a widget needs server-backed data, assume the host card configuration provides the required identifiers like `server_id` and `view_id`.
- For host-configured widgets, read those values from `window.frameElement?.dataset?.linceServerId` and `window.frameElement?.dataset?.linceViewId`.
- The bridge meta also carries the same host-owned values, plus runtime mode and stream status.
- Treat `meta.serverId`, `meta.viewId`, and `meta.streams.enabled` as the authoritative runtime contract once the bridge is available.
- Do not build a second widget-local configuration store for `server_id` or `view_id`.
- If the widget is meant to use Lince's mediated backend integration, the manifest permissions must use the names the host already recognizes.

Host-recognized permissions for backend-aware widgets:

- `read_view_stream` means the widget consumes a configured SSE view stream and requires both `server_id` and `view_id`
- `write_records` means the widget mutates rows through the host-mediated record table route
- These permissions are what cause the board to show its standard configure and locked states
- Do not invent near-miss names like `read_backend_view` or `update_record`

Authentication:

- Login endpoint: `POST /api/backend/auth/login`
- Request body: JSON object with `username` and `password`
- Success response: JSON object with `token` and `token_type`
- Authenticated calls use the `Authorization` header with a bearer token
- Example: `Authorization: Bearer <token>`

Available backend endpoints:

- `GET /api/backend/table/{table}` lists rows from a table
- `POST /api/backend/table/{table}` creates a row in a table from a JSON object body
- `GET /api/backend/table/{table}/{id}` fetches a single row by integer id
- `PATCH /api/backend/table/{table}/{id}` updates a row from a JSON object body
- `DELETE /api/backend/table/{table}/{id}` deletes a row by integer id
- `GET /api/backend/sse/view/{view_id}` opens a server-sent events stream for a saved view

Host-mediated routes for widgets:

- Widgets should prefer the host-mediated routes under `/host/integrations/servers/{server_id}/...`
- SSE view route: `/host/integrations/servers/{server_id}/views/{view_id}/stream`
- Table collection route: `/host/integrations/servers/{server_id}/table/{table}`
- Table item route: `/host/integrations/servers/{server_id}/table/{table}/{id}`
- Do not call `/api/backend/...` directly from a widget when the widget is supposed to use a configured remote server
- Do not pass `server_id` as a query parameter to `/api/backend/...`

Widget behavior:

- If the widget performs backend calls through the mediated host routes, make the auth requirement explicit in the UI state.
- Prefer compact empty, loading, locked, and error states.
- If auth is missing, show a locked or waiting state instead of pretending there is no data.
- If host config is missing, show a misconfigured state that tells the user to use the board configure action.
- If `meta.streams.enabled` is `false`, stop or pause SSE consumption instead of reconnecting.
- A widget may request a per-card stream pause or resume by calling `window.LinceWidgetHost.setStreamsEnabled(enabled)`.
