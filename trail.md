# NeoTrail Sand

NeoTrail is the backend-rendered Trail page for a single widget instance. It uses Maud for the first HTML response and Datastar for live signal updates over SSE.

## What `instance_id` means

`instance_id` is the unique id of the widget card on the board.

- It identifies one specific Trail widget instance.
- It is the same id used in the board state and widget contract.
- It is not the package filename.
- It is not a database primary key for trail data itself.

In practice, the route `GET /host/trail/{instance_id}` means: "open the Trail UI for this exact card".

## Goal

- Render the Trail page from the backend.
- Seed Datastar signals from the initial GET.
- Keep an SSE stream open for live updates.
- Avoid frontend data-fetching logic.

## Core Endpoints

### Initial page

`GET /host/trail/{instance_id}`

- Loads the widget contract for that card instance.
- Returns the initial Maud page.
- Seeds the `trail` Datastar signal tree with contract data.
- Renders the Trail shell and graph container.

### Live stream

`GET /host/trail/{instance_id}/stream`

- Keeps the Trail stream open.
- Emits `datastar-patch-signals`.
- Updates `trail.binding`, `trail.binding.snapshot`, and `trail.stream`.
- Becomes the live source of truth after the initial page load.

## Backend widget endpoints

These endpoints are used by the Trail service and the page bootstrap.

### Contract

`GET /host/widgets/{instance_id}/contract`

- Returns the Trail contract for the widget instance.
- Includes widget metadata, source server info, permissions, binding, snapshot, and diagnostics.

### Stream

`GET /host/widgets/{instance_id}/stream`

- Existing widget stream endpoint.
- Still useful for compatibility and internal widget flows.
- The Trail page should prefer its own `/host/trail/{instance_id}/stream` SSE route.

### Actions

`POST /host/widgets/{instance_id}/actions/{action}`

Supported actions:

- `search-trails`
- `search-assignees`
- `bind-trail`
- `create-trail`
- `run-trail-sync`

## Workflow

1. The user clicks **Open trail** on a widget card.
2. The app navigates to `GET /host/trail/{instance_id}`.
3. The backend resolves that card instance and loads its contract.
4. The page renders Maud HTML with the initial `trail` signals.
5. If the Trail is already bound, the page opens `GET /host/trail/{instance_id}/stream`.
6. The SSE stream patches Datastar signals as the trail changes.
7. The graph reads from signals and redraws from the signal-backed state.
8. Mutations happen through widget actions, then SSE refreshes the signals.

## Signal shape

The page signal tree should look roughly like this:

```json
{
  "trail": {
    "widget": {},
    "source": {},
    "permissions": {},
    "binding": {
      "trailRootRecordId": 94,
      "viewId": 123,
      "sync": {},
      "snapshot": {
        "rows": []
      }
    },
    "dataContract": {},
    "search": {},
    "actions": [],
    "diagnostics": {},
    "stream": {
      "status": "live",
      "error": null,
      "url": "/host/trail/INSTANCE/stream"
    }
  }
}
```

## Rules

- Do not move trail data fetching into frontend JavaScript.
- Keep the Trail page Datastar-first.
- Keep SSE as the live update channel.
- Keep D3 only for graph rendering.
- Preserve trail root state in backend widget state so reloads keep the selected trail.
