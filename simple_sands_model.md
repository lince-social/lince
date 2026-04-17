# Sands Outside the Simple JSON SSE + JS Model

The simple model is: backend returns JSON snapshots or JSON SSE payloads, and the widget uses plain JavaScript to render and mutate state. The sands below do not fully follow that model today because they either push Datastar signal patches, parse SSE as a custom transport, or carry too much logic in the frontend.

## Outliers

- Trail: Datastar-first, with backend signal patches and client signal synchronization.
- Kanban Record View: mixed Datastar + JS state, with SSE-driven sync plus local persistence and UI orchestration.
- Table: parses SSE frames manually, then re-emits synthetic Datastar events after parsing payloads.
- Relations: SSE-driven widget with a large handwritten JS state machine and a lot of backend-shape parsing.
- Chess: SSE-driven widget with custom client state and stream handling.
- Todo: standard view stream plus local JS state and rendering logic.

## What I would change

Make the backend expose generic JSON resources and narrow SSE endpoints that only stream JSON snapshots or JSON patches, never HTML fragments and never domain-specific signal syntax. Then make each sand a thin JavaScript adapter that fetches the resource, normalizes the payload once, renders locally, and applies actions through explicit mutation endpoints. Datastar should stay optional and local for small UI flags like open panels, not as the core transport or state model. That removes the duplicated state machine, keeps the server protocol boring, and makes the sands easier to reason about and extend.
