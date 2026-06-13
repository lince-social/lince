# Desktop Stream Fanout

## Problem

Each sand widget can currently open its own live stream. For example, todo and relations both subscribe to view updates from inside their iframe.

That works in normal browsers, but it is fragile in the desktop app because Linux Tauri uses WebKitGTK. Multiple iframe-owned streams can expose WebKit-specific buffering, scheduling, and rendering delays. The visible symptom is that one sand mutates data immediately, but another sand receives the matching update seconds later or only after a later user action.

## Goal

Use one host-owned live data path per source, then fan updates out to sand iframes with `postMessage`.

The top-level Lince page should own the network stream. Sand iframes should receive normalized state updates from the host bridge instead of each iframe opening its own stream.

## Current Shape

The host already has a bridge layer:

- Host to widget: `lince:bridge-state`
- Widget to host: `lince:widget-ready`
- Widget to host actions: `lince:widget-action`
- Widget iframe API: `window.LinceWidgetHost`

This is a good base for fanout. The missing piece is a host-managed stream registry.

## Proposed Design

Add a host-side stream manager in the board frontend.

The manager keeps a map keyed by source:

```text
server_id:view_id
```

Each entry owns one `EventSource` connection to the existing stream endpoint:

```text
/host/integrations/servers/{server_id}/views/{view_id}/stream
```

When an SSE `snapshot` event arrives, the host stores the latest payload and posts it to every iframe subscribed to that source.

The iframe receives the update through the existing bridge:

```js
window.LinceWidgetHost.subscribe((detail) => {
  // detail.meta.viewSnapshot or detail.meta.streams.latestSnapshot
});
```

The exact payload shape should be explicit and versioned. For example:

```json
{
  "meta": {
    "serverId": "1",
    "viewId": 4,
    "viewStream": {
      "status": "live",
      "version": 128,
      "snapshot": {}
    }
  }
}
```

## Responsibilities

Host:

- Open one `EventSource` per unique `server_id:view_id`.
- Track which card instances use each source.
- Cache the latest snapshot per source.
- Send the latest snapshot immediately when a widget becomes ready.
- Reconnect streams with backoff.
- Mark stream status as `connecting`, `live`, or `offline`.
- Close streams with no subscribers.

Sand iframe:

- Stop opening direct view streams for common host-provided data.
- Subscribe to `LinceWidgetHost`.
- Render from host-provided snapshots.
- Send mutations through existing host action endpoints.

Backend:

- Keep existing SSE endpoints.
- Keep anti-buffering headers on SSE responses.
- Optionally add compact delta events later, but start with snapshots.

## Why This Helps

One connection per data source is cheaper than one connection per widget.

The top-level Tauri webview handles network streaming once, which avoids iframe-specific stream scheduling problems in WebKitGTK.

All widgets receive the same update in the same event-loop turn through `postMessage`, so todo and relations stay visually consistent.

The host can inspect, log, debounce, or prioritize updates centrally.

## Migration Plan

1. Add a board-level stream manager module.
2. Teach `createWidgetBridge` to include host stream snapshots in bridge state.
3. Register each package widget's `serverId` and `viewId` as a stream subscription.
4. Convert todo to consume host fanout snapshots instead of opening `EventSource` directly.
5. Convert relations to consume host fanout snapshots.
6. Keep direct widget streams behind a compatibility path until all sands migrate.
7. Add instrumentation for stream latency:
   - server event time
   - host receive time
   - iframe receive time
   - render completion time

## Open Questions

- Should fanout send full snapshots only, or snapshots plus deltas?
- Should stream subscriptions be inferred from card config or explicitly requested by each widget?
- Should widgets be allowed to opt out and open direct streams for specialized cases?
- How should host fanout handle authenticated remote organs when a widget lacks auth?

## Default Recommendation

Use host fanout as the default for normal view-backed widgets.

Keep direct `EventSource` inside a sand only when the stream is widget-specific, very high volume, or intentionally isolated from the board host.
