# Widget Runtime, Bridge, and Sandbox

This note explains the current Web widget runtime in Lince, the browser warning about iframe sandboxing, and how the main pieces connect.

## The Problem

Lince renders widgets inside `iframe`s using `srcdoc`.

For live board widgets, the iframe sandbox currently includes:

```html
sandbox="allow-scripts allow-same-origin"
```

Browsers warn that an iframe with both `allow-scripts` and `allow-same-origin` can effectively escape many of the protections normally expected from sandboxing. In practice, that warning is correct: this is a weaker sandbox than a script-only sandbox.

In Lince's current architecture, this was not added by accident. The live widget runtime depends on same-origin behavior so widgets can call host-relative routes like `/host/...` and use authenticated browser requests against the local web host.

So the problem is not just "a bad iframe attribute". The problem is that the current runtime model couples:

- widget code running inside `srcdoc`
- host-mediated backend fetches
- authenticated same-origin requests
- the widget bridge
- SSE streams for live cards

That means a stricter sandbox is desirable, but not free.

## The Pieces

### Host

The host is the Lince Web application served by the local Rust backend.

It is responsible for:

- serving the board UI
- serving `/host/...` routes used by widgets
- persisting board state and card state
- storing and validating remote server sessions
- injecting widget runtime helpers into widget HTML

### Board

The board is the host-side page that lays out cards in the workspace grid.

It is responsible for:

- rendering cards
- deciding whether a widget is configured, locked, or ready
- creating the widget iframe
- passing card metadata such as `server_id`, `view_id`, and instance id
- maintaining widget bridge state and per-card state

### Widget

A widget is a standalone HTML document, usually distributed as a single `.html` file and rendered through `srcdoc`.

Widgets are responsible for:

- rendering their own UI
- reading host-provided metadata
- subscribing to bridge updates
- calling allowed host routes like `/host/integrations/...` or `/host/servers`
- optionally consuming SSE streams

### Iframe

The iframe is the isolation boundary between the board and a widget.

In the live board today, the iframe:

- uses `srcdoc`
- gets data attributes like `data-lince-server-id`
- runs injected bridge bootstrap code
- currently uses `sandbox="allow-scripts allow-same-origin"`

In preview contexts, the requirements are lower. The preview iframe does not need the full live runtime contract, so it can use a stricter sandbox such as:

```html
sandbox="allow-scripts"
```

### Bridge

The bridge is the host/widget communication layer.

It exists so the host can coordinate widget runtime state without each widget inventing its own protocol.

The host injects `window.LinceWidgetHost` into widget HTML. The bridge exposes operations such as:

- `getMeta()`
- `getCardState()`
- `setCardState(nextState)`
- `patchCardState(patch)`
- `setStreamsEnabled(enabled)`
- `subscribe(handler)`
- `requestState()`

Bridge traffic is based on `postMessage`, not direct DOM access from the parent to the widget internals.

The bridge carries runtime metadata such as:

- widget instance id
- board mode
- configured `serverId`
- configured `viewId`
- per-card persisted state
- stream enablement flags

### Streams

Streams are used by widgets that consume live backend data, especially view-based widgets such as Kanban.

The important current route shape is:

- `/host/integrations/servers/{server_id}/views/{view_id}/stream`

This is an SSE endpoint mediated by the host. A widget opens the stream itself from inside the iframe.

### Sandbox

The sandbox is the browser-enforced restriction layer on the iframe.

The main tradeoff is:

- `allow-scripts` is needed because widgets run JavaScript
- `allow-same-origin` is currently needed by the live runtime model

Without same-origin, the iframe gets an opaque origin. For Lince's current live widgets, that breaks the assumption that widget code can do authenticated same-origin fetches to host routes and open host-mediated streams directly.

## How They Connect

The runtime path for a live widget looks like this:

1. The host board renders a card.
2. If the card is a package widget and is configured, the board creates an `iframe`.
3. The iframe receives:
   - `srcdoc` with the widget HTML
   - the injected bridge bootstrap
   - data attributes like server id, view id, and package instance id
4. The widget starts inside the iframe.
5. The widget reads host metadata from the bridge and/or `window.frameElement.dataset`.
6. The widget may call host routes such as:
   - `/host/servers`
   - `/host/integrations/servers/{server_id}/table/...`
   - `/host/integrations/servers/{server_id}/views/{view_id}/stream`
7. The bridge keeps card state, runtime metadata, and stream toggles synchronized between board and widget.

In short:

- the board owns layout, permissions, and card configuration
- the iframe hosts the widget runtime
- the bridge synchronizes host state into the widget
- the widget performs direct host fetches and streams
- same-origin currently ties those pieces together

## Why Kanban Hits This

The Kanban widget is a good example because it depends on nearly all of the runtime pieces:

- it runs as a board widget inside an iframe
- it reads `serverId` and `viewId`
- it opens an SSE stream for a saved view
- it updates records through host-mediated routes
- it uses bridge state for per-card UI persistence

That means Kanban is not just a visual widget. It is an active microfrontend with runtime coupling to host networking, host auth state, and host-managed widget state.

## Workspace Switching Problem

There is another runtime issue that shows up clearly with Kanban and other active widgets.

When the user switches workspaces, Lince does not fully destroy inactive package widgets. Instead, the board keeps package-card DOM nodes alive in a hidden cache so they can be moved back quickly later.

The relevant behavior is:

- active workspace package cards are mounted in the visible card layer
- inactive workspace package cards are moved into a hidden cache container
- those hidden iframes still exist as live browsing contexts
- the widget bridge syncs all widget iframes in the document, not only visible ones

That means a workspace switch is not equivalent to unmounting a widget.

For a widget like Kanban, that matters because it has active behavior:

- it opens an SSE stream
- it reconnects when streams end
- it writes per-card state through the bridge
- it reacts to bridge metadata changes

So if a Kanban iframe remains alive while hidden, it can continue doing work.

### Why Requests Repeat

The repeated requests seen during workspace changes are consistent with this lifecycle:

1. a package iframe is created and boots
2. the widget sends `lince:widget-ready`
3. the board syncs frames through the bridge
4. the widget performs its startup work such as fetching data or opening a stream
5. the user switches workspaces
6. the iframe is moved to the hidden cache instead of being destroyed
7. the board later syncs frames again, including hidden ones
8. the still-alive widget reacts again, reconnects, or repeats startup requests

This explains an important observation:

- switching workspaces can repeat requests
- a full page refresh stops the repetition

The refresh stops it because the browser destroys every iframe and every open stream, resetting the runtime graph completely.

### Why This Is Not Only About Kanban

Kanban makes the issue obvious because SSE is visible in the network tab, but the underlying problem is broader:

- any widget with boot-time fetches can repeat them
- any widget with timers can keep running in the hidden cache
- any widget with bridge-driven side effects can respond while hidden
- any widget that persists card state can trigger additional board persistence writes

So this is a widget lifecycle and board architecture issue, not just an SSE issue.

## Possible Solutions

There are several possible directions. They have different tradeoffs.

### 1. Destroy inactive workspace package iframes

This is the simplest behavioral fix.

Instead of moving inactive package cards into a hidden cache, the board would remove those iframe nodes completely. When the workspace becomes active again, the iframe would be created again from the stored card definition.

Advantages:

- hidden widgets cannot keep streaming
- hidden widgets cannot keep running timers
- workspace switch semantics become easier to understand
- the browser naturally closes network activity when the iframe disappears

Tradeoffs:

- widget startup becomes more expensive when returning to a workspace
- in-memory widget UI state is lost unless persisted explicitly through card state or local storage

For Lince, this is likely the cleanest fix if predictable behavior matters more than preserving ephemeral hidden-widget runtime state.

### 2. Keep hidden iframes, but suspend them explicitly

The board could keep the hidden cache but send an explicit inactive lifecycle state through the bridge, and widgets would be required to stop transport when inactive.

That would mean:

- hidden widgets receive a bridge flag such as `meta.active = false`
- widgets stop SSE, timers, and nonessential polling when inactive
- widgets resume when active again

Advantages:

- preserves warm widget instances
- avoids full reboots on workspace return

Tradeoffs:

- every active widget must implement the lifecycle correctly
- bugs become distributed across widgets instead of being solved centrally
- hidden widgets still exist, so mistakes remain possible

This is viable, but it is weaker than true unmounting because it depends on widget discipline.

### 3. Restrict bridge sync to visible frames

The board currently syncs every widget iframe in the document. Another mitigation is to sync only visible workspace frames.

Advantages:

- reduces unnecessary bridge churn
- avoids waking hidden widgets on every sync

Tradeoffs:

- hidden iframes can still keep running whatever they already started
- SSE connections or timers may continue even without fresh bridge events

This helps, but it does not solve the full problem by itself.

### 4. Move transport behind the host bridge

This is the larger architectural direction.

Widgets would stop opening `/host/...` connections directly. Instead, the host would own networking and streams, and the bridge would deliver data to widgets.

Advantages:

- widget lifecycle becomes host-controlled
- hidden widgets can be centrally paused
- future sandbox tightening becomes more realistic

Tradeoffs:

- much more implementation work
- bridge protocol becomes more complex
- transport concerns move out of widgets and into the board runtime

This is the strongest long-term direction, but not the smallest fix.

## Practical Recommendation

For the current architecture, the most pragmatic fix is:

1. stop keeping inactive package iframes alive in the hidden cache, or at least do not keep live iframes there
2. if hidden caching is still desired for layout or animation, cache placeholders instead of live widget browsing contexts
3. treat widget UI state as something that must be persisted explicitly through card state, not preserved by keeping hidden iframes alive

That gives Lince a clearer rule:

- active workspace widgets may run
- inactive workspace widgets do not exist as live runtimes

This matches user expectations much better and avoids the repeated-request behavior seen when switching into or out of workspaces that contain active widgets.

## What Was Changed Already

The warning was reduced in non-live contexts by removing `allow-same-origin` from preview iframes such as:

- import preview
- AI builder preview

Those previews do not need the full live runtime contract.

The live board iframe was intentionally left unchanged because removing `allow-same-origin` there would break current widget behavior.

## Architectural Conclusion

The current live widget architecture is:

- safer than fully unsandboxed script injection
- but not a strong sandbox in the browser-security sense

If Lince wants a stricter live sandbox, the architecture needs to change. The main direction would be:

- widgets stop fetching `/host/...` directly
- widgets ask the host to perform transport on their behalf
- the bridge becomes the only runtime channel for backend reads, writes, and stream delivery

That would separate:

- widget rendering
- host networking
- host auth/session ownership
- browser origin concerns

Until that change happens, the live board runtime should be understood as a same-origin microfrontend architecture with a partial iframe sandbox, not as a fully isolated untrusted-widget sandbox.
