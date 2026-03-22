# Web Host Server Session Spec

## Goal

Refactor the web host so it opens immediately without a startup login screen, while keeping backend access protected per remote Lince server.

The host should manage server authentication centrally. Widgets should not own auth state or auth UI. Widgets should only declare which server and backend resource they want to use.

This spec replaces the current mixed model where:

- the board startup screen asks for login up front
- the host keeps a single local session cookie
- some widgets and host integrations still assume old route patterns
- web-local state is stored inside the repo tree instead of a user config directory

## Desired User Experience

1. Opening the web UI should never require login.
2. The board should render immediately.
3. Widgets that do not depend on a remote server should work immediately.
4. Widgets that depend on a remote Lince server should render in a locked state until that server is authenticated.
5. Clicking or interacting with a locked widget should trigger a standard host-managed login flow for that widget's server.
6. Once login succeeds, every widget bound to that server should unlock together.
7. Tokens should live only in process memory.
8. Closing the host process should clear those tokens.

## Core Model

### Server Profiles

The central object should be a `server profile`, not a widget token.

Each backend-aware widget instance should reference a `server_id`.

A server profile should contain:

- `id`
- `name`
- `base_url`
- optional metadata useful for the UI

It should not contain a persisted token.

### Runtime Server Session

The host should maintain an in-memory session map keyed by `server_id`.

Runtime state per server should contain:

- `server_id`
- `username_hint` if available
- `bearer_token`
- connection/auth status
- optional timestamps like `connected_at`

This state should be held only in memory.

### Widget Binding

Widgets should not store raw tokens.

Widgets may store endpoint configuration data, but the recommended model is:

- `server_id`
- widget-specific parameters

For a view widget, the preferred parameters are:

- `server_id`
- `view_id`

This is better than storing separate `ip` and `port`, because `base_url` is the real abstraction and works for:

- localhost
- VPS domains
- reverse proxies
- HTTPS endpoints

## Authentication Model

### Remove Startup Login

The startup login flow should be removed from the initial board boot sequence.

The host auth layer should no longer gate:

- page load
- board state load
- widget catalog load
- local host tools that do not require a remote backend token

### Host-Owned Login Flow

When a widget needs a server-backed endpoint and the associated `server_id` is not authenticated:

1. The widget should appear locked.
2. The user interacts with the widget.
3. The host presents a standard login UI for the referenced server.
4. The host authenticates against the remote backend.
5. The host stores the token in memory under that `server_id`.
6. All widgets referencing that `server_id` become eligible to retry.

Widgets should not implement login UI themselves.

The host may render the login UI:

- as an overlay above the widget
- as a shared modal keyed to the server
- or as a side panel

The important rule is that the UI is owned by the host, not by individual packages.

### Token Scope

Tokens must remain in memory only.

They should not be written to:

- `~/.config/lince`
- cookies
- localStorage
- widget archives
- board state

The current host session cookie may still exist for local host identity/session continuity, but it should not be treated as the remote backend credential store.

## Endpoint Ownership

The current split should remain conceptually valid:

- `/api/...`
  stable real backend API
- `/host/...`
  web-host-only routes and proxy behavior

The frontend and widgets should follow this rule:

- use `/host/...` when the host is supplying auth or mediating the request
- use `/api/...` only for direct calls to the stable backend API when that is explicitly intended

For backend-aware widgets, the default should be host-mediated access through `/host/...`.

## Widget Locking Model

Each widget instance should expose whether it depends on a server.

Host-side resolution should produce one of these states:

- `ready`
- `locked(server_id)`
- `misconfigured`
- `error`

### Locked

A locked widget means:

- the widget configuration is valid
- the referenced server profile exists
- the host does not currently have an in-memory token for that server

### Misconfigured

A misconfigured widget means:

- missing `server_id`
- missing `view_id` for a view widget
- referenced server profile not found
- malformed widget settings

This distinction matters because "locked" should invite login, while "misconfigured" should invite editing.

## Storage Layout

The web host's local data should move out of the crate directory and into:

- `~/.config/lince/web/`

Recommended layout:

- `~/.config/lince/web/board-state.json`
- `~/.config/lince/web/widgets/`
- `~/.config/lince/web/workspaces/` if exported examples or templates are kept
- `~/.config/lince/web/servers.json`

The current repo-local files that should be replaced by this model are rooted in:

- [`crates/web/src/infrastructure/paths.rs`](../../crates/web/src/infrastructure/paths.rs)

### Board State

Move:

- current `.lince-board-state.json`

to:

- `~/.config/lince/web/board-state.json`

### Widgets

Move installed/local widget storage to:

- `~/.config/lince/web/widgets/`

The current local package catalog is seeded from repo directories. That is useful for development, but the durable user-facing location should be the config path above.

### Example Packages

Development examples may remain in the repo as source material, but the runtime-loaded local package directory should be under:

- `~/.config/lince/web/widgets/`

If example packages are auto-seeded, they should be copied there on first run or when absent.

### Workspace Examples

If workspace templates are kept, they should also live under:

- `~/.config/lince/web/`

not inside the crate as the runtime source of truth.

## Server Configuration Model

Introduce explicit server profiles.

Recommended persisted shape:

```json
[
  {
    "id": "local-dev",
    "name": "Local Lince",
    "base_url": "http://127.0.0.1:6174"
  },
  {
    "id": "vps-main",
    "name": "Main VPS",
    "base_url": "https://example.com"
  }
]
```

This becomes the configuration surface used by widgets and the host login UI.

## Widget Configuration Direction

### General Rule

A widget should not hardcode a token, and preferably should not hardcode a raw host-specific route if the data is really server-backed.

For server-backed widgets, store:

- `server_id`
- widget-specific settings

### Extra Simple

The `extra-simple` widget should become a configurable view widget.

Recommended editable settings:

- `server_id`
- `view_id`

Optional later:

- display label
- reconnect behavior
- empty state copy

It should not directly ask for:

- raw token
- separate `ip`
- separate `port`

If a low-level override is ever needed for testing, it should be a server-profile concern, not a widget concern.

## Required Host Features

### Server Registry

The host needs a service/store for:

- listing server profiles
- loading one by `server_id`
- saving and editing server profiles

### Runtime Session Registry

The host needs a runtime-only session registry for:

- associating `server_id -> bearer_token`
- checking whether a server is authenticated
- clearing one server session
- clearing all sessions

### Widget Auth Resolution

The host needs a way to answer:

- does this widget depend on a server
- which server
- is that server currently authenticated
- should this widget render locked, misconfigured, or ready

### Standard Login Surface

The host needs one standard login component for any backend-aware widget or tool.

The login flow should be keyed by `server_id`.

## Migration Plan

### Phase 1: Config Path Move

1. Move board state path to `~/.config/lince/web/board-state.json`.
2. Move installed widget directory to `~/.config/lince/web/widgets/`.
3. Move workspace/example runtime storage under `~/.config/lince/web/`.
4. Keep repo examples only as seed material.

### Phase 2: Server Profiles

1. Add persisted server profiles under `~/.config/lince/web/servers.json`.
2. Add host CRUD or editor UI for those server profiles.
3. Keep current `LINCE_API_BASE_URL` only as a fallback or development default.

### Phase 3: Runtime Sessions

1. Replace the current startup-login mental model with server-specific in-memory sessions.
2. Remove startup login gating from the board boot flow.
3. Keep sessions in memory only.

### Phase 4: Widget Lock States

1. Add lock state evaluation per widget.
2. Render locked and misconfigured states distinctly.
3. Trigger host login UI on interaction with locked widgets.

### Phase 5: Widget Configuration

1. Update `extra-simple` to use `server_id + view_id`.
2. Define a general pattern for server-backed widget settings.
3. Stop relying on hardcoded example URLs in server-backed widgets.

### Phase 6: Host Proxy Consistency

1. Ensure all server-backed widgets use `/host/...` routes.
2. Keep `/api/...` as the stable backend API for direct external clients and deliberate direct usage.

## Risks and Tradeoffs

### In-Memory Tokens

Pros:

- no token leakage to disk
- process restart clears auth state
- simpler security posture

Cons:

- user must log in again after process restart
- multi-server usage requires runtime re-auth after restart

This is still the right default.

### Server-Level Sessions

Pros:

- one login unlocks all widgets for the same server
- clear ownership model
- simpler token refresh/logout behavior

Cons:

- less fine-grained than per-widget auth

This is also the right tradeoff.

### Host-Managed Login

Pros:

- widget packages stay simpler
- consistent UX
- centralized error handling

Cons:

- host must understand widget/server dependencies

Still preferred over embedding auth logic in widgets.

## Acceptance Criteria

The refactor is complete when:

1. Opening the board never requires startup login.
2. Board state and local widgets load without remote authentication.
3. Backend-aware widgets render locked if their server is unauthenticated.
4. The host can log in to a specific server profile on demand.
5. The token is stored only in memory.
6. All widgets bound to that server unlock after login.
7. Restarting the host clears server sessions.
8. Board state, widgets, and server profiles live under `~/.config/lince/web/`.
9. `extra-simple` is configurable through `server_id` and `view_id`.

## Recommended First Implementation Slice

Implement in this order:

1. Move paths to `~/.config/lince/web/`.
2. Add server profile persistence.
3. Add runtime server session registry.
4. Remove startup login gating.
5. Add locked widget state rendering.
6. Add host login UI keyed by `server_id`.
7. Update `extra-simple` to `server_id + view_id`.

This order keeps the refactor incremental while preserving the current host/backend split.
