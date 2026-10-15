# Live composition workflow

## Existing capabilities and gaps

These are starting points to inspect, not a claim that a general live Castle editor already exists:

- `crates/interface/src/presentation.rs` describes Record field selections, order, fills, and presentation settings. It does not describe an arbitrary component tree or control placement.
- `crates/sandbox/src/protein_area/presentation.rs` supports presentation sessions, preview/apply/undo, and captured per-record state saved in Area records.
- `crates/sandbox/src/protein_area/presentation/fields.rs` preserves existing field entities while arranging them and supports field dimensions, wrapping, and scoped tokens.
- `crates/sandbox/src/customization.rs` applies native scoped token changes. Inspect which values are persisted and which systems own their effective geometry.
- `crates/sandbox/src/layout/` and the Castle's own layout systems determine constraints, sizing, and placement. Runtime edits must participate in those systems rather than being overwritten next frame.
- `crates/sandbox/src/laboratory/` provides native behavioral cases. Captures and measurements must come from the real Bevy interface.

`cargo xtask design native <component> [--theme path.json] [--watch] [--mobile]` is an existing isolated native preview for its supported components. Theme changes reload live; Rust edits under `--watch` compile and restart. That is useful for native inspection, but it is not a general composition editor, does not preserve temporary fixture state across restart, and is not connected to Cell. `--mobile` checks a narrow desktop window, not Android.

Inspect `crates/xtask/src/design/native.rs` for supported components before launching. A Castle absent from that list needs a native fixture or preview host. Cell-dependent interactions need an isolated Cell-backed Laboratory fixture or an explicitly requested live instance.

The existing `design new`, `design serve`, and `design check` study commands create, serve, or validate browser artifacts. They are legacy tooling for this workflow; do not use them to generate new alternatives. Keep existing studies as historical evidence. Build or extend native tooling when needed, and document its actual interface instead of inventing a command.

## Composition and live editing

Represent the composition as serializable data consumed by the same native factory used for normal Castles. The shape should follow existing Lince models; avoid creating a competing layout engine.

A reusable composition needs stable semantic part identifiers, component kinds, parent/child order, layout constraints, scoped tokens, disclosure rules, and references to supported actions and data bindings. Store logical identifiers, not transient Bevy Entity numbers, closures, or an indiscriminate ECS dump. Resolve references to live entities when constructing or editing an instance.

Treat a date input with its action buttons, or another behavior-coupled control, as a coherent part. Moving its parent must preserve its internal editor, observers, action targets, event boundaries, and popup ownership. Validate reparenting against these relationships; a visually correct move can change event routing.

Provide inspect/read, patch, measure/capture, undo/reset, and save operations through the native editor and an available local agent interface. Agent and human edits should use the same transaction path. Check the selected instance and composition revision before applying a patch; stale edits should be rejected without discarding drafts. Apply layout and token changes atomically, with a useful diff and an undo entry.

Prefer updating existing entities. Change constraints and hierarchy without replacing input state, subscriptions, or timers. Rebuild only parts whose component kind actually changes, transferring compatible transient state deliberately. Keep layout ownership explicit so reconciliation or automatic sizing does not undo an edit on the next frame.

Use one baseline with named variants for comparison. Variants change composition, not the underlying Record's data or subscriptions. Independent interactive fixtures may be appropriate for side-by-side comparisons; distinguish them from multiple editors writing to one live Record.

## Measurements and correctness

Capture both authored constraints and resolved native geometry: logical-pixel bounds, padding, gaps, borders, font configuration, viewport, scale, scroll offsets, and relevant conditional state. Let layout settle before measuring; ignore hidden nodes when evaluating visible space. Screenshots complement these measurements and do not replace them.

Exact preservation means reconstructing the reviewed recipe under the same inputs produces the same constraints, hierarchy, behavior bindings, and measured layout. Preserve numeric precision in save/load; do not round reviewed values or approximate them during promotion. Permit only the renderer's documented physical-pixel rounding where needed. Different fonts, viewports, scale factors, or content can change computed geometry, so record those inputs and test responsive rules separately.

Do not bake every computed rectangle into absolute positioning. Preserve intrinsic sizing and flex/wrap rules; keep explicit measurements where the human chose fixed dimensions. The goal is faithful reconstruction plus responsive behavior.

Exercise real workflows after structural changes: disclosure, editing while a save is pending, date endpoints, popups, scroll, focus/caret, keyboard navigation, and the Castle's specialized actions. Check for duplicate observers or popups, leaked live entities, lost bindings, clipped controls, and changes to canvas placement. Select tests proportional to the edit rather than rebuilding an exhaustive feature inventory for every spacing adjustment.

## Persistence and defaults

An instance override stores that Castle's reviewed composition. A custom component stores a named reusable recipe with bindings resolved for each new instance. A default chooses the recipe used by the relevant Castle factory and hosts. Make the destination explicit in the saved decision.

Save and default promotion must consume the same reviewed recipe. If the existing default is hardcoded, refactor the factory to consume the composition rather than transcribing a screenshot into another hardcoded arrangement. The same layout, token, and binding code should render the live version, saved custom component, and promoted default.

After saving, reload the instance and instantiate a fresh custom component or default as appropriate. Compare structural and geometry captures under identical fixtures, and verify action/binding reachability and input state behavior. Check all relevant hosts, including standalone Records and Kanban where they share the Castle. State how existing instance overrides interact with the chosen default; do not silently replace customizations.

A handoff contains the reviewed composition, actual decision and destination, native captures and measurements, validation results, and any missing runtime capabilities. A future agent should be able to reproduce the chosen Castle without interpreting an image or reconstructing its layout from prose.
