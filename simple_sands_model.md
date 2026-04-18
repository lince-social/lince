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

For views, specially in kanban's case, make the user set or create a specific Kanban View with a name, based on another view. That way we dont have cases of \_\_lince_web_kanban_card-id, to make a new Kanban view when we base ourselves in one view we have to name it, that way we have more friction to not let the backend create one everytime we reuse the existing ones that have a human-rememberable name. Look at how i do with current Trail creation (or at least how i wanted to do), i set a name and create it then reuse it. I have a 'My Try' Trail View that i created, i want Kanban to be the same. I can inspire myself in an initial view and then created a human named kanban view to not bloat with generic kanban id views.
