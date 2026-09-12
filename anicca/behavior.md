# What an Effect does

A Sand can emit an event, such as a button being pressed or a Record being selected. A connection decides what happens next: open another Sand, change local presentation, invoke a registered Effect or request an Action.

For example, a button can receive a Record reference. One connection opens that Record; another button requests its deletion. The deletion still goes through the backend's permissions. Receiving data or an event does not grant permission to write.

Connections are visible and have declared inputs and outputs. Events stay inside their owning group unless the group exports them. Releasing a child from moving with a Castle preserves that ownership and event scope. One gesture must not become duplicate writes because a property has several appearances.

Current native Effects use Bevy systems and observers. People compose registered Effects; trusted Rust plugins add capabilities when needed. Saving a composition stores references to those Effects and their settings, not executable callbacks. Work and event loops need limits and understandable failures.

JavaScript matters only at an actual external publication boundary. Such code needs validated messages, declared authority, clear ownership and cleanup. Browser assets keep their source and required licenses. These requirements do not introduce a JavaScript runtime into the native interface.

The proposed authoring tasks are in [Sands, Effects and Castles](plans/interface.md#sands-effects-and-castles).
