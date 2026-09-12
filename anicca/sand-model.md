# Sands and Castles

A Sand definition describes a reusable piece of interface. An instance is one use of it with its own placement and settings. A Castle is a saved group of Sands that provides a workflow, such as Kanban. Groups can contain other groups.

The same button can stand alone or belong to a Castle. Code and Box use the same pieces. A definition owns its children and internal connections; a workspace holds placements and connections between them. Inputs and outputs make those connections inspectable.

Protein reads data. Actions change it. Saved presentation holds layout and settings. Temporary events carry things such as cursor movement and session progress. These have different meanings even when one Sand uses all of them.

A Protein area repeats a group for each result. Results and child appearances need stable identities so an edit to Apple survives a refresh. A Record can appear in several places, each with its own appearance settings. Showing a value twice does not copy the Record.

Group ownership and movement are separate. A released child stays part of the group but can be moved independently. Copy and release adds another appearance. Both remain bound to the source result and disappear if that result disappears. Unlocking the editor and forking a definition are different operations.

Definitions are referenced. Updating a shared definition preserves instance overrides; an invalid update keeps the last working revision and shows an error. Fork creates an independent definition. Box can override or fork a code-owned definition without rewriting its Rust source.

Save stable ids, authored component values, group structure, bindings and asset references. Recreate runtime entities and remap their references on load. Keep required dependency licenses and credits with the owning Sand. Trusted Rust plugins and editable registered components are the current extension model; arbitrary installed code needs a separate isolation design.

A workflow can open a configured Castle bound to its Record. Reopening should find that placement, preserve the person's arrangement and show where it came from. Closing a view must not end a backend session that owns its own lifetime.

The proposed tasks are gathered in [the interface draft](plans/interface.md). [Box](box.md) explains result appearances and editing.
