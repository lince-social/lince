# Legacy Web UI inventory

Purpose: Record the existing Web component library and Gallery behavior that
the native Sand system must preserve or deliberately replace.

Owner source: no dedicated Customization Record currently exists;
[Interface in Lince](../Lince.lingua) governs shared interface decisions.

Status: Landed Web baseline and migration input. `LynxUI` is a historical
implementation/API name and does not survive as a second compositional
vocabulary beside Sand.

Read when: inventorying components, building the Gallery, or migrating official Sands.

[Corpus map](README.md) · [Current context](current.md) · [Customization plan](plans/customization.md)

---

### Historical LynxUI implementation

This is behavior and source inventory, not a second supported native UI. [Part A](plans/part-a.md) replaces enabled native-path callers; browser-backed leaves and necessary legacy support remain disabled until [the final CEF lane](plans/cef.md). Preserve that source without using it as the implementation of a newly migrated native control.

- [x] Select Lynx and keep one evolving light/dark demo in the canonical [`LynxUI Gallery Sand`](../../crates/web/src/sand/lynx_ui/index.html). Update this design-system description whenever the demo guidelines change.
- [x] Build the LynxUI base as a framework-free component library for official Sands. Use native semantic HTML, explicit `lynx-*` classes, and a small JavaScript layer only for behavior that HTML does not provide consistently.
- [x] Serve shared `lynx-ui.css` and `lynx-ui.js` assets. LynxUI uses the design-system tokens and defines no separate colorscheme, spacing scale, motion, or elevation. Component selectors have low specificity so global and per-Sand styles can override them.
- [x] Provide Catppuccin Macchiato as the second style. Its CSS changes only colorscheme variables, uses the official Base, Mantle, Crust, Text, Subtext, Overlay, Mauve, Lavender, Red, Yellow, Green, and Blue values, and carries the Catppuccin MIT notice.
- [x] The first component set has buttons and button groups; inputs, textareas, selects, checks, radios, labels, help and errors; boxes, panels, stacks, rows, grids, toolbars and dividers; badges, callouts and empty states; tables, lists, dropdowns, tooltips, dialogs, tabs and disclosures; and a small first-party SVG icon set.
"- [x]" (2026-08-07) Native date/time/datetime-local fields need no new component — `.lynx-input`/`.lynx-textarea`/`select.lynx-select` already style by class, not input type. `LynxUI.formatDateTime(iso, { relative })` adds the compact absolute (same-day time-only, else date+time) and relative (`Intl.RelativeTimeFormat`) display for Record work metadata and Kanban card metadata.
"- [x]" (2026-08-07) `LynxUI.combobox(container, { getOptions, getLabel, getValue, onSelect })` — a live-filtered `.lynx-combobox`/`.lynx-menu` autocomplete, arrow/Enter/Escape keyboard nav, single-select. Instantiated per element (unlike the delegated-click components) since its option list is usually Protein-backed and built at query time. Record's predicate/object inputs and Kanban's column/Concept pickers are this with a different `getOptions`.
"- [x]" (2026-08-07) `LynxUI.tokens(container, { getSelected, getOptions, onAdd, onRemove })` — composes `combobox` + `.lynx-status` badges (`.lynx-tokens`) into a removable token picker, no separate data model; the caller stays the source of truth via `getSelected()`/`refresh()`. For Record assignees, selected assertions, thread predicates.
"- [x]" (2026-08-07) `LynxUI.attachmentRow(name, { busy, onRemove })` builds the `.lynx-attach__row` shape (name, busy spinner or remove button); Record owns what an attachment means and its preview, this only owns the row.
"- [x]" (2026-08-07) `.lynx-meta` (`dl`/`dt`/`dd`) — a flat key–value grid, not a panel, for Record head/slug/quantity/work facts and Kanban card metadata.
"- [x]" (2026-08-07) `.lynx-duration` + `LynxUI.formatDuration`/`parseDuration` — minutes underneath (Record's estimate/worklog unit), "1h 30m" is how it displays and how free text like "90" or "1h30m" parses back.
"- [x]" (2026-08-07) `LynxUI.confirmDialog({ title, body, confirmLabel, danger })` — a destructive-confirmation composition of the existing `.lynx-dialog`, not a new component; returns `Promise<boolean>`, Escape and Cancel both resolve `false`. For Kanban bulk deletion and Record hard deletion.
"- [x]" (2026-08-07) `LynxUI.openMenu(x, y, items)` / `LynxUI.contextMenu(target, buildItems)` — the same `.lynx-menu` a dropdown already shows, anchored to a point (right-click, or Shift+F10/ContextMenu for keyboard parity) instead of a fixed sibling; closes on outside pointerdown or Escape. For Record links/attachments and Kanban cards.
"- [x]" (2026-08-07) `.lynx-spinner` (static ring — no animation, per the standing motion rule) and `.lynx-progress` (native `<progress>`, restyled) for Record saves, uploads, and Kanban moves; transient notifications, avatars, and range sliders stay out of scope until a Sand needs them.
- [x] Static components use native markup and classes. Interactive components use `data-lynx-*` attributes and one delegated event and keyboard handler per iframe. `window.LynxUI` provides icon and icon-button helpers for dynamic elements.
- [x] Add a development-only LynxUI Gallery with one scrollable page: a compact showcase of all LynxUI components sits beside a stacked set of seeded Kanban, message, inventory, and request-review Sand previews at their normal board sizes. It uses canonical assets and fixture data without Protein or Action requests. `mise run lynxui` serves it at `http://127.0.0.1:6175` and recompiles the gallery package for Lince on source changes.
- [x] Add a folded base control that highlights LynxUI components and shows their component names on hover. Sand-specific structure stays unmarked so the boundary is clear.
A Chart Sand is not a prerequisite library project. Add the smallest chart
renderer when a concrete Protein projection or workflow demonstrates a need,
and package any vendored dependency with its license and credits.
