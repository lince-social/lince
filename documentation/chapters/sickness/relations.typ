== Relations

This is the agreement note for the directed record graph widget.

Read `web_components.typ` first for the host and bridge contract, then read `kanban.typ` for the kind of server-shaped, host-bound widget this has to stay compatible with.

=== Goal

The intended widget is a specialized graph view for records and their one-way parent relations.

The target behavior is:

- show records as nodes
- show directed parent -> child links as ropes or magnets
- allow connect and disconnect actions from the graph
- allow one parent per child
- allow many children per parent
- keep category and parent filters available in the editing UI
- persist those filters by changing the generated SSE view
- expose graph physics controls in local widget state
- rename the generic edit-mode label to `origin`

The `origin` label matters because the user is not editing the graph in the abstract.
They are choosing where the data comes from: the organ plus SSE view binding that feeds the widget.

=== Critique

The plan is good, but it is too broad if treated as "just a D3 graph".

Main risks:

- a canvas demo by itself does not define storage rules
- a rope-and-magnet metaphor is UI, not truth
- one-way parentage means cycles and self-links must be rejected
- the widget needs a clear host contract before it can support connect and disconnect safely
- search is not part of the first version and should not sneak in through the filter model
- persistent view filters and local physics controls are different scopes and should not share the same storage path
- if the widget can reparent a child, it must define what happens to the previous parent before any implementation starts

The storage rule should stay strict even if the interaction feels loose.
Visually it can feel like drag-and-drop magnets.
Semantically it is still a directed graph with a single parent edge per child.

=== Agreement

We should agree on these non-negotiables before coding:

- the widget is host-bound
- the widget receives a normalized record view plus relation data
- `origin` config binds the organ and SSE view
- category editing is persistent and rewrites the SQL shape of the generated SSE view
- parent filtering is persistent and rewrites the SQL shape of the generated SSE view
- the parent filter should be based on text matching against `head` and should not become a general search feature yet
- graph physics settings only change local presentation and live in widget state
- relation actions must be validated by the host
- connect means assign a new parent
- disconnect means remove the current parent edge
- records themselves are never deleted by relation disconnect
- relation updates must reject self-parenting and cycles

=== Suggested data shape

The first contract should carry enough data for a directed hierarchy.

Useful fields:

- `id`
- `head`
- `body`
- `quantity`
- `categories_json`
- `parent_id`
- `parent_head`
- `children_json`
- `children_count`
- `depth`

If the final host contract adds more fields, they should still map back to the same hierarchy model.

=== Suggested controls

The widget should expose a compact control area with:

- `origin` binding
- persistent category editing controls that affect the generated SSE view
- persistent parent filtering controls that affect the generated SSE view
- local physics controls such as charge, link distance, collision radius, and center force
- selection details for the active node
- explicit connect and disconnect affordances

The controls should stay honest about what they affect.
If it changes the generated view, it is persistent filter/configuration state.
If it is about local layout, it is a physics setting.
If it changes the backing relation, it is a host action.

=== Criticism

The remaining implementation risk is overloading one UI panel with too many meanings.

Keep these separate:

- generated SSE view filters
- local widget physics
- relation edit actions
- host removal of the widget from the board

Do not add a search box in the first pass.
Name matching can exist as the parent filter shape if needed, but it should stay a SQL filter over `head`, not a general-purpose search surface.

=== Steps

The implementation order should be:

1. Define the host contract for origin, node data, and relation actions.
2. Define the persistent SQL filter inputs for category and parent shaping.
3. Define the widget state for physics.
4. Build the graph shell and selection UI.
5. Implement connect and disconnect with host validation.
6. Add relation constraints for single-parent, many-children, and cycle rejection.
7. Package the vendored graph library with its required license files.

=== License note

This widget already vendors D3 in `d3.v7.min.js`.
The package must keep the required license text with the sand assets, not only in source control.
If the implementation keeps using that vendored D3 file, bundle a matching `LICENSE.txt` alongside it.
