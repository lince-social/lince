# Karma Orchestra Karma Modal Plan

## Goal

Build the Karma modal as the create/update surface for Karma rows. The modal is opened from a Karma id box on an arrow line for update mode, and from a new top-right create button for create mode.

The modal must support creating more than one Karma while staying open. After a create succeeds, the newly created condition/consequence selection returns to disabled selected-state inputs, and the user can continue creating another Karma until they close the modal with the close button or `Esc`. In update mode, the user can update the current Karma without leaving the modal.

Do not implement this yet. This document is the implementation plan.

## Current Context

- The modal shell currently exists in `crates/web/src/sand/karma_orchestra/body.rs` as `#karma-karma-modal`.
- The client script currently opens the modal from link tags in `crates/web/src/sand/karma_orchestra/script.rs`.
- The backend service is `crates/web/src/application/karma_orchestra_widget.rs`.
- `load-graph` returns graph nodes, links, loops, and `karmaRows`.
- Expression display logic already exists in `crates/application/src/karma_analysis.rs` through `expression_display`.
- The widget service already loads `karma`, `karma_condition`, `karma_consequence`, `record`, `command`, `query`, and `frequency` rows to build the graph.

## New Layout

The modal remains centered at `80vw` by `80vh`.

The modal has one large content surface. It is not the old three-column layout. The new layout is:

- Top header row.
- Optional original/current Karma row.
- New creation/update row.
- Lower two banks: condition bank on the left, consequence bank on the right.
- Operator selector sits between condition and consequence.

Graph-level create entry:

- Add a new top-right create button in the Sand chrome.
- The button is a compact `+ Karma` control in the top-right HUD area.
- Clicking it opens this same modal in create mode.
- Create mode has no original/current Karma section.
- Create mode uses the same condition bank, operator dropdown, consequence bank, and authoring flows as update mode.

Top header row:

- Left title: `Karma Creation/Update`.
- Center text: `Karma id: N` in update mode.
- Quantity control beside the Karma id.
- Create mode also has an active switch for the new Karma quantity.
- Create-mode active switch defaults to active/on.
- Right button: `Delete Karma` in update mode.
- Primary action button: `Create Karma` in create mode and `Update Karma` in update mode.
- Create and update are separate modes entered from separate buttons.
- Update mode does not show a `Create Karma` primary action.
- Create mode does not show an `Update Karma` primary action.
- In create mode, original Karma id, quantity control, and delete button are hidden or replaced with create-mode-safe empty space.

Original/current row in update mode:

- Shows original condition id.
- Shows original condition code in a read-only div.
- Shows original operator in a read-only div.
- Shows original consequence id.
- Shows original consequence code in a read-only div.
- Shows evaluated/executable value where available.
- Shows human-readable replacement where useful, but this row is informational only.
- None of the original condition/operator/consequence values are interactable.
- Delete Karma is the only destructive interaction in the original/current area.

Original/current row in create mode:

- Does not exist.
- The modal starts directly with the New area.

New row:

- Left section title: `Condition`.
- `New` button to the right of the Condition title.
- Condition input below the title.
- Middle section title/control: `Operator`.
- Operator is a dropdown/select.
- Right section title: `Consequence`.
- `Create` button to the right of the Consequence title.
- Consequence input below the title.
- The top-level create action should be a `Create Karma` button at the top or in the main header/action area, not hidden in the lists.
- In update mode this top-level action becomes `Update Karma`.

Condition input:

- Starts disabled and empty in create mode.
- Starts disabled and copied from original in update mode.
- Clicking an existing condition in the lower condition bank fills this disabled input with that condition and selects its id.
- Clicking `New` enables the condition input and switches it to new-condition-authoring mode.
- Authoring a new condition exposes a condition name input.
- While authoring a new condition, a create/confirm control creates the `karma_condition` row.
- In new-condition-authoring mode, the user can write a condition code.
- After creating a new condition successfully, the input becomes disabled again and holds the created condition selection.
- Editing an existing selected condition is allowed from update mode, but must show a warning that other Karma rows may also be affected.

Consequence input:

- Starts disabled and empty in create mode.
- Starts disabled and copied from original in update mode.
- Clicking an existing consequence in the lower consequence bank fills this disabled input with that consequence and selects its id.
- Clicking the authoring control enables consequence authoring for a new consequence.
- Authoring a new consequence exposes a consequence name input.
- While authoring a new consequence, a create/confirm control creates the `karma_consequence` row.
- In new-consequence-authoring mode, the user can write a consequence code.
- Consequence authoring must be limited to one action target, such as one record receiving quantity or one command being run.
- After creating a new consequence successfully, the input becomes disabled again and holds the created consequence selection.
- Editing an existing selected consequence is allowed from update mode, but must show a warning that other Karma rows may also be affected.

Lower condition bank:

- Occupies the lower left side of the modal.
- Shows all existing conditions, not only conditions used by Karma.
- Has its own search input.
- Search input filters existing condition boxes.
- Condition boxes can be clicked to select that condition.
- Condition boxes can be dragged into the condition input to insert/select code where dropped.
- The bank also serves as the token search surface while authoring a new condition.

Lower consequence bank:

- Occupies the lower right side of the modal.
- Shows all existing consequences, not only consequences used by Karma.
- Has its own search input.
- Search input filters existing consequence boxes.
- Consequence boxes can be clicked to select that consequence.
- Consequence boxes can be dragged into the consequence input to insert/select code where dropped.
- The bank also serves as the token search surface while authoring a new consequence.

## Quantity And Active State

The original/current Karma quantity should be represented as an active true/false switch button.

- Active true means Karma quantity is non-zero.
- Active false means Karma quantity is zero.
- The control displays the current quantity as active state, not as a free numeric text field.
- Updating this switch should eventually write to the Karma quantity field.
- Changing this switch writes immediately.
- The write is a simple Karma update that sets quantity to `1` when active and `0` when inactive.
- The write updates only the current Karma active/quantity state and refreshes the graph.

## Delete Karma

The `Delete Karma` button is shown only in update mode.

- It sits on the right side of the top header row.
- It asks for confirmation before deleting.
- It deletes the current Karma row.
- When delete succeeds, the modal closes.
- After delete succeeds, the graph refreshes.

## Create Karma Flow

The user can create a Karma from selected or newly created condition/consequence values.

Required selected/new fields:

- Condition id.
- Operator.
- Consequence id.

Flow:

- User opens modal in create or update mode.
- User selects an existing condition, or clicks `New`, writes a condition, and creates it.
- User selects an existing consequence, or clicks `Create`, writes a consequence, and creates it.
- User chooses operator from dropdown.
- User clicks `Create Karma`.
- The new Karma is created.
- The modal stays open.
- The condition and consequence inputs stay disabled with the created/selected values.
- The modal receives and displays fresh data after create.
- The graph data refreshes so new arrows can appear behind the modal.
- User can change selections and create more Karma.
- User exits with close button or `Esc`.

## Update Karma Flow

The user can update the currently opened Karma row without leaving the modal.

Required fields:

- Existing or newly created condition id.
- Operator.
- Existing or newly created consequence id.

Flow:

- User opens the modal by clicking a Karma id box on an arrow line.
- Original/current Karma information is shown at the top as read-only reference.
- New row starts as a copy of the original condition, operator, and consequence.
- User can select existing condition/consequence rows, author new ones, and change operator.
- User clicks `Update Karma`.
- The existing Karma row is updated.
- The modal stays open and reloads the original/current section from the updated row.
- The modal receives and displays fresh data after update.
- The graph refreshes behind the modal.

## Condition Authoring

Condition authoring starts when the user clicks `New` beside the Condition title.

Behavior:

- The condition input becomes enabled.
- The lower condition bank switches from existing-condition selection mode into search/insert support for the input.
- User can type raw code like `-1 * f1`.
- Raw code is shown exactly as typed.
- User can press `@` to start human-readable token insertion.
- User can click/focus the condition bank search input to search tokens manually.
- While typing after `@`, the search bank filters records, frequencies, commands, queries, sync tokens if available, and existing conditions when useful.
- Clicking a search result inserts the corresponding code at the current cursor position.
- Dragging a search result into the input inserts the corresponding code at the drop cursor position.
- While dragging over the input, the insertion caret/position should update so the user can control where the token lands.
- If insertion started with `@`, the input should show a human-readable inline representation where possible.
- The actual stored code remains the machine code, such as `rq30`, `f1`, `c2`, or `sql5`.

Human-readable `@` behavior:

- Typing `@rq30` should attempt to display the record head for record id `30`.
- If `@rq30` resolves, show the human-readable record head in the visible input representation.
- If `@rq30` does not resolve to a known record, show a fallback such as `Record with id 30`.
- If `@` is not followed by a valid code, focus the condition bank search input.
- If the user writes `rq30` without `@`, show `rq30` as raw code.
- The difference is intentional: `@` requests human-readable display while retaining machine code underneath.
- The visible input must behave like a rich input: `@` tokens are shown human-readable, but cursor movement and editing map through to the underlying raw code.
- Human-readable `@` tokens should be visibly distinct with a very thin border, around `1px` or less.
- Passing the cursor through a human-readable `@` token passes through the underlying raw code positions.
- Deleting one character from an `@` token deletes the whole token.
- If the user typed an active `@...` query and selects a search result, the selected result replaces the active `@...` span rather than appending beside it.

## Consequence Authoring

Consequence authoring starts when the user clicks `Create` beside the Consequence title.

Behavior:

- The consequence input becomes enabled.
- The lower consequence bank switches from existing-consequence selection mode into search/insert support for the input.
- User can type one consequence target/action.
- User can press `@` to start human-readable token insertion.
- User can click/focus the consequence bank search input to search tokens manually.
- Clicking a search result inserts the corresponding code at the current cursor position.
- Dragging a search result into the input inserts the corresponding code at the drop cursor position.
- While dragging over the input, insertion position should update.

Consequence limits:

- Consequence should be one thing only.
- Valid consequence targets are the normal consequence forms that the current Karma execution engine can actually execute.
- Current engine-supported consequence forms should be treated as valid: one `rqN` record quantity target, one `cN` command, one `sqlN` query, or one supported sync token.
- Frequency tokens are valid for condition/search display, but should not be valid as a consequence unless the execution engine adds frequency consequences.
- Avoid allowing compound consequence expressions unless the existing Karma consequence engine already supports that exact pattern safely.
- The consequence input should prevent adding more code after one valid consequence target exists.
- If `@` is active in consequence authoring, deleting one character from the human-readable token deletes the whole underlying token.
- The UI should show the user why additional input is blocked or why a consequence is invalid.
- If user still reaches an invalid consequence state, the UI must reject it before allowing create/update.
- Backend validation must repeat the same consequence validation so invalid inputs cannot bypass the frontend.
- If the user typed an active `@...` query and selects a consequence search result, the selected result replaces the active `@...` span rather than appending beside it.

## Existing Item Selection

Existing condition item click:

- Sets selected condition id.
- Sets condition input display to the existing condition's code/human display.
- Keeps condition input disabled.
- Updates the draft condition value used by `Create Karma`.

Existing consequence item click:

- Sets selected consequence id.
- Sets consequence input display to the existing consequence's code/human display.
- Keeps consequence input disabled.
- Updates the draft consequence value used by `Create Karma`.

Selecting an existing item is distinct from creating a new item:

- Existing item click selects an id.
- New/Create button enables authoring.
- Authoring creates a new condition/consequence row first, then selects that new id.
- The user may create condition/consequence rows multiple times before finally pressing `Create Karma` or `Update Karma`.
- Pressing `Update Karma` after creating a new condition/consequence points the Karma row to the newly selected ids.
- Editing an existing condition/consequence mutates that existing row only after an explicit warning/confirmation because other Karma rows can reference it.

## Search And Filtering

Each lower bank has a search input.

Condition bank search filters:

- Existing condition id.
- Existing condition name.
- Existing condition raw code.
- Existing condition human-readable code.
- Record id and record head.
- Frequency id and frequency name.
- Command id and command name.
- Query id and query name.
- Raw token prefixes such as `rq`, `f`, `c`, `sql`.

Consequence bank search filters:

- Existing consequence id.
- Existing consequence name.
- Existing consequence raw code.
- Existing consequence human-readable code.
- Record id and record head.
- Command id and command name.
- Query id and query name if supported as consequence.
- Frequency id and frequency name only if supported as consequence.
- Raw token prefixes such as `rq`, `c`, `sql`, `f`.

Search behavior:

- Typing in a bank search filters that bank only.
- Typing in condition input after `@` should focus or drive the condition bank search.
- Typing in consequence input after `@` should focus or drive the consequence bank search.
- Clicking a result while an authoring input is active inserts code into that input.
- Clicking an existing condition/consequence while no authoring input is active selects that item.

## Drag And Drop Insertion

Search result boxes should support drag into the active input.

Drag behavior:

- Drag starts from a result box.
- The dragged result carries machine code and human-readable display.
- While hovering over the input, the input tracks the nearest insertion position.
- Dropping inserts machine code at the current insertion position.
- If the insertion was triggered from an `@` flow, visible display should prefer human-readable representation.
- If the insertion was not triggered from `@`, visible display can remain raw code.
- Dragging a full existing condition/consequence into the disabled selected input should select that existing row instead of editing raw text.
- Dragging a token result into an enabled authoring input inserts the token.
- Drag/drop insertion is part of the first implementation, not a later follow-up.

## Operator Dropdown

Operator control:

- The operator is a dropdown/select.
- Minimal options: `=` and `=*`.
- Original operator is display-only in update mode.
- New operator defaults to original operator in update mode.
- New operator defaults to `=` in create mode unless a better existing default is found.
- The operator dropdown is always interactable in the New area.

## Display And Evaluation Rules

Original condition:

- Show id.
- Show raw code.
- Show human-readable form if different.
- Show evaluated value if evaluable/executable.

Original consequence:

- Show id.
- Show raw code.
- Show human-readable form if different.
- Show evaluated value if evaluable/executable.

New condition:

- While existing condition is selected, display that condition's code/human/value.
- While authoring, display typed code and live human-readable token display where `@` was used.
- Authoring uses the rich input behavior: visible human-readable `@` tokens with underlying raw code.
- Live evaluated value should be displayed when enough information exists to compute it.
- New condition rows require a user-editable name field.

New consequence:

- While existing consequence is selected, display that consequence's code/human/value.
- While authoring, display typed code and live human-readable token display where `@` was used.
- Authoring uses the rich input behavior: visible human-readable `@` tokens with underlying raw code.
- Live evaluated value should be displayed when enough information exists to compute it.
- New consequence rows require a user-editable name field.

Token display:

- `rqN` can evaluate record quantity where numeric evaluation is valid.
- Commands, queries, frequencies, and sync tokens are display-only unless existing engine rules say otherwise.
- If a value is not executable/evaluable, show code and human-readable replacement but do not duplicate code as value.
- If no name exists, use existing fallbacks: `Nameless Frequency`, `Nameless Command`, `Nameless Query`.

## Data Model Needed By The Modal

Backend payload should include:

- `mode`: `create` or `update`.
- `original`: selected Karma data or null.
- `draft`: initial new state.
- `conditions`: all existing conditions.
- `consequences`: all existing consequences.
- `tokens`: search/autocomplete token catalog.
- `operators`: supported operators.

Original Karma payload in update mode:

- `karmaId`.
- `karmaName`.
- `karmaQuantity`.
- `karmaActive`.
- `operator`.
- `conditionId`.
- `conditionCode`.
- `conditionDisplay`.
- `consequenceId`.
- `consequenceCode`.
- `consequenceDisplay`.

Condition option:

- `id`.
- `name`.
- `quantity`.
- `code`.
- `display.code`.
- `display.human`.
- `display.value.text`.
- `display.value.numeric`.
- `display.value.complete`.
- `referencedByKarmaIds` for warning before editing an existing condition.

Consequence option:

- `id`.
- `name`.
- `quantity`.
- `code`.
- `display.code`.
- `display.human`.
- `display.value.text`.
- `display.value.numeric`.
- `display.value.complete`.
- `referencedByKarmaIds` for warning before editing an existing consequence.

Token option:

- `kind`: `record_quantity`, `frequency`, `command`, `query`, `sync`, or other supported token kind.
- `id`.
- `code`.
- `human`.
- `searchText`.
- `numeric` where available.
- `validForCondition`.
- `validForConsequence`.
- `deleteAsUnit` for `@` inserted tokens.
- `rawStart` and `rawEnd` or equivalent mapping data for rich-input cursor behavior.
- `borderedDisplay` or equivalent UI hint for human-readable `@` tokens.

## Backend Plan

Add read action:

- `load-karma-editor`.
- Payload: `{ "karmaId": number | null }`.
- If `karmaId` is present, return update-mode data.
- If `karmaId` is null or absent, return create-mode data.
- Reuse existing graph row loading so condition/consequence catalogs stay consistent.
- Reuse `expression_display` and existing catalog construction.
- Return all condition rows, all consequence rows, and token options.

Future write actions:

- `set-karma-active` to change quantity active true/false.
- `delete-karma`.
- `create-condition`.
- `create-consequence`.
- `update-condition`.
- `update-consequence`.
- `create-karma`.
- `update-karma`.

Write actions should be implemented after the read payload and modal layout are in place. The active switch writes immediately once implemented.
Editor data should reload after condition/consequence creation, condition/consequence update, Karma create/update, active switch writes, and delete success.

## Frontend State Plan

Add modal state in `script.rs`:

- `mode`: `create` or `update`.
- `original`: selected Karma details or null.
- `draft.conditionId`.
- `draft.conditionName`.
- `draft.conditionCode`.
- `draft.conditionMode`: `selected` or `authoring`.
- `draft.operator`.
- `draft.consequenceId`.
- `draft.consequenceName`.
- `draft.consequenceCode`.
- `draft.consequenceMode`: `selected` or `authoring`.
- `draft.karmaActive`.
- `conditions`.
- `consequences`.
- `tokens`.
- `conditionSearch`.
- `consequenceSearch`.
- `activeAuthoringSide`: `condition`, `consequence`, or null.
- `lastConditionCursor`.
- `lastConsequenceCursor`.
- `dragToken`.

Open update mode:

- Clicking a Karma id link tag calls `load-karma-editor` with that id.
- Store payload in `state.karmaEditor`.
- Initialize draft from original.
- Render modal.
- Show modal.

Open create mode:

- A new top-right create button calls `load-karma-editor` with `karmaId: null`.
- Store payload in `state.karmaEditor`.
- Initialize blank draft.
- Render modal.
- Show modal.

Save/update:

- `Create Karma` calls `create-karma` and keeps the modal open after success.
- `Update Karma` calls `update-karma` and keeps the modal open after success.
- Active switch calls `set-karma-active` immediately and writes quantity `1` or `0`.
- Delete button asks for confirmation, then calls `delete-karma`, then closes the modal after success.
- Create/update actions reload editor data and refresh graph data after success.
- Condition/consequence create/update actions reload editor data after success.

Close:

- Close button hides modal.
- `Esc` hides modal.

## Styling Plan

- Keep modal centered at `80vw` and `80vh`.
- Header spans full modal width.
- The graph-level `+ Karma` control sits in the top-right HUD area.
- Original/current row spans full modal width in update mode.
- New row spans full modal width above the lower banks.
- Lower half is two panels: condition bank left and consequence bank right.
- Operator dropdown sits between the condition and consequence input areas.
- Existing item boxes should be dense enough to scan many rows.
- Bank search inputs remain visible above each list.
- Add/New/Create buttons are small and attached to their section titles.
- Human-readable `@` tokens use an inline visual treatment with a very thin border.
- Preserve the current Sand style without refactoring unrelated visualization code.

## Implementation Steps

- [ ] 1. Keep this as a plan-only update; do not change runtime modal code in this pass.
- [ ] 2. Add `load-karma-editor` to the Karma Orchestra widget contract action list.
- [ ] 3. Add `LoadKarmaEditorRequest` with optional `karma_id`.
- [ ] 4. Add an action arm for `load-karma-editor` in `KarmaOrchestraWidgetService::action`.
- [ ] 5. Resolve the widget instance with read permission.
- [ ] 6. Reuse selected Karma Orchestra View validation before loading editor data.
- [ ] 7. Reuse or refactor `load_graph_rows` to expose raw condition rows, consequence rows, rules, and catalog.
- [ ] 8. Build all condition options from `karma_condition`, not from current graph nodes.
- [ ] 9. Build all consequence options from `karma_consequence`, not from current graph nodes.
- [ ] 10. Build token options from record, frequency, command, query, and sync-capable data.
- [ ] 11. Mark which token options are valid for condition authoring.
- [ ] 12. Mark which token options are valid for consequence authoring.
- [ ] 13. Build condition displays with `expression_display(condition.condition, catalog, true)`.
- [ ] 14. Build consequence displays with the same display rules used by the graph.
- [ ] 15. Include referenced-by-Karma metadata for each condition and consequence option.
- [ ] 16. In update mode, find the selected Karma row by id.
- [ ] 17. In update mode, return original Karma id, quantity, active state, condition, operator, and consequence.
- [ ] 18. In create mode, return null original, empty draft, and active default `true`.
- [ ] 19. Replace the temporary modal body with the new structural shell.
- [ ] 20. Add a graph-level top-right `+ Karma` button that opens create mode.
- [ ] 21. Add header DOM nodes for title, Karma id, active switch, primary create/update action button, Delete Karma button, close button.
- [ ] 22. Add create-mode active switch defaulted on.
- [ ] 23. Add original row DOM nodes for original condition id/code/value, operator, consequence id/code/value.
- [ ] 24. Add New condition title, New button, condition name input, and condition code input.
- [ ] 25. Add New operator dropdown.
- [ ] 26. Add New consequence title, Create button, consequence name input, and consequence code input.
- [ ] 27. Add condition bank title, search input, and scrollable result area.
- [ ] 28. Add consequence bank title, search input, and scrollable result area.
- [ ] 29. Add modal CSS for the new header, original row, new row, and lower two-bank layout.
- [ ] 30. Add CSS for thin-bordered human-readable `@` tokens.
- [ ] 31. Add script references for every new modal DOM node.
- [ ] 32. Change `openKarmaModal(ruleId)` to load editor data instead of only showing the id.
- [ ] 33. Add `openCreateKarmaModal()` for the top-right `+ Karma` button.
- [ ] 34. Add `renderKarmaEditorModal()`.
- [ ] 35. Render original/current area only in update mode.
- [ ] 36. Render active switch from `karmaQuantity > 0`, defaulting to on in create mode.
- [ ] 37. Wire active switch to immediate `set-karma-active` in update mode.
- [ ] 38. Render Delete Karma button only in update mode.
- [ ] 39. Wire Delete Karma with confirmation, close modal after success, and refresh graph.
- [ ] 40. Initialize update draft from original values.
- [ ] 41. Initialize create draft empty with operator default `=` and active default on.
- [ ] 42. Render primary action as `Create Karma` in create mode and `Update Karma` in update mode.
- [ ] 43. Render condition input disabled while draft condition mode is `selected`.
- [ ] 44. Render condition input enabled while draft condition mode is `authoring`.
- [ ] 45. Render consequence input disabled while draft consequence mode is `selected`.
- [ ] 46. Render consequence input enabled while draft consequence mode is `authoring`.
- [ ] 47. Clicking Condition `New` switches condition mode to `authoring`.
- [ ] 48. Add an explicit create/confirm control for authoring a new condition.
- [ ] 49. Clicking Consequence authoring control switches consequence mode to `authoring`.
- [ ] 50. Add an explicit create/confirm control for authoring a new consequence.
- [ ] 51. Clicking an existing condition selects its id and code, disables condition input, and updates draft.
- [ ] 52. Clicking an existing consequence selects its id and code, disables consequence input, and updates draft.
- [ ] 53. Allow editing selected existing condition with warning that other Karma rows can be affected.
- [ ] 54. Allow editing selected existing consequence with warning that other Karma rows can be affected.
- [ ] 55. Typing in condition bank search filters existing conditions and valid condition tokens.
- [ ] 56. Typing in consequence bank search filters existing consequences and valid consequence tokens.
- [ ] 57. Typing `@` in enabled condition input focuses or drives condition bank search.
- [ ] 58. Typing `@` in enabled consequence input focuses or drives consequence bank search.
- [ ] 59. Implement `@rq30` parsing so known records show their head in the visible representation.
- [ ] 60. Implement fallback display such as `Record with id 30` when `@rq30` cannot resolve.
- [ ] 61. Replace the active `@...` query span when selecting a search result.
- [ ] 62. Keep raw `rq30` displayed as raw code when no `@` was used.
- [ ] 63. Implement rich-input mapping from visible human-readable `@` tokens to underlying raw code.
- [ ] 64. Make deleting one character inside an `@` token delete the whole token.
- [ ] 65. Insert clicked token result code at the current cursor position for the active authoring input.
- [ ] 66. Track cursor position for condition input through the rich-input mapping.
- [ ] 67. Track cursor position for consequence input through the rich-input mapping.
- [ ] 68. Add draggable result boxes with machine code and human display payload.
- [ ] 69. While dragging over an enabled input, update intended insertion position.
- [ ] 70. On drop into an enabled condition input, insert token code at drop cursor.
- [ ] 71. On drop into an enabled consequence input, insert token code at drop cursor.
- [ ] 72. On drop into a disabled selected input, select the existing condition/consequence if the dragged item represents one.
- [ ] 73. Block adding more consequence code after one valid consequence target exists.
- [ ] 74. Validate consequence authoring as one target/action only in the frontend.
- [ ] 75. Validate consequence authoring as one engine-supported target/action in the backend.
- [ ] 76. Add `create-condition` action after authoring UI is visible and stable.
- [ ] 77. Add `create-consequence` action after authoring UI is visible and stable.
- [ ] 78. Add `update-condition` action with warning/confirmation.
- [ ] 79. Add `update-consequence` action with warning/confirmation.
- [ ] 80. After creating a condition, reload editor data, disable condition input, and select the created condition id.
- [ ] 81. After creating a consequence, reload editor data, disable consequence input, and select the created consequence id.
- [ ] 82. Add `create-karma` action after selected condition/operator/consequence are working.
- [ ] 83. Add `update-karma` action after update-mode draft editing is working.
- [ ] 84. After creating Karma, keep modal open, reload editor data, and refresh graph.
- [ ] 85. After updating Karma, keep modal open, reload original data, and refresh graph.
- [ ] 86. Add `set-karma-active` action for the active switch and write quantity as `0` or `1`.
- [ ] 87. Add `delete-karma` action for Delete Karma.
- [ ] 88. Support closing modal with close button.
- [ ] 89. Support closing modal with `Esc`.
- [ ] 90. Run `cargo check`.
- [ ] 91. Manually verify update mode shows original data, active state, and delete button.
- [ ] 92. Manually verify create mode opens from the top-right `+ Karma` button and hides original data.
- [ ] 93. Manually verify create mode has active default on.
- [ ] 94. Manually verify existing condition selection fills the disabled condition input.
- [ ] 95. Manually verify existing consequence selection fills the disabled consequence input.
- [ ] 96. Manually verify condition/consequence name fields are required when creating new rows.
- [ ] 97. Manually verify `@` token insertion shows human-readable display with thin border.
- [ ] 98. Manually verify selecting search result replaces active `@...` query.
- [ ] 99. Manually verify deleting inside an `@` token deletes the whole token.
- [ ] 100. Manually verify drag/drop inserts token code at the chosen cursor location.
- [ ] 101. Manually verify Create Karma keeps the modal open for another create.
- [ ] 102. Manually verify Update Karma keeps the modal open and updates the original/current section.

## Final Notes

- [ ] 1. Condition/consequence names are optional; blank names fall back to `Condition` / `Consequence`.
- [ ] 2. Existing condition/consequence editing is available for any selected row.
- [ ] 3. Existing condition/consequence editing uses a persistent warning and save confirmation.
- [ ] 4. `@` tokens are frontend-only display behavior; submitted code must not include `@`.
- [ ] 5. Token results appear only after `@`.
- [ ] 6. Create/update actions must not proceed unless the Karma row and referenced ids are valid.
