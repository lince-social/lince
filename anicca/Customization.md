Customization (@customization: 0, is #chapter, #instinct, #part-of @interface, #done) { r_4Z5VS9MJNRDHZGXW4KMGRNXMHK

## Customization

### Configuration

The Configuration Sand is the ordinary surface for Lince data and Box
settings. Common changes are approachable controls; arbitrary styling and
Behavior editing live behind an explicit advanced developer mode so the normal
surface stays small.

#### Customization and architecture

### Space, thickness, roundness, rigidity

### Content and Sand chrome

### Transparency and elevation

### Line, shape, and texture grammar

### Typography

### Motion

### The test

### Design system work

The implementation order is binding. First finish the semantic color,
spacing, border, radius, typography, elevation, and motion token contract and
its load order. Then rebuild the base UI and every official Sand on those
tokens and composable LynxUI/Sand pieces, deleting legacy component CSS and
APIs instead of adapting around them. Finally land the author documentation,
behavior/accessibility tests, iframe/isolation tests, theme tests, and automated
design-system checks before calling the migration complete. Checks may be
developed alongside migration, but migration does not begin against an
unfinished token vocabulary.

### LynxUI

- [x] Select Lynx and keep one evolving light/dark demo in the canonical [`LynxUI Gallery Sand`](../crates/web/src/sand/lynx_ui/index.html). Update this design-system description whenever the demo guidelines change.
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

## What is left

### Customization

- [ ] Let people live-edit padding, margin, gap, border radius, thickness,
      typography, colorscheme, information density, Sand Behavior, and Box
      presentation. Official Web UI contains no hardcoded design color; it
      uses documented semantic variables from the default style.
- [ ] Architecture (from Sand: Colorschemes): The system is defined as named semantic tokens, not hex values — surface-raised, ink-primary, need, contribution, accent, focus — resolved per active colorscheme at runtime (the old a2 Operation to switch schemes is the spiritual ancestor). Scaling tokens (padding-s, radius-m) ride the same mechanism. A Sand author never picks a color; they name a slot, and the user's scheme decides what it looks like. That's how "the base app is minimalist so users can express themselves" survives contact with real widgets.
- [ ] The default style is always loaded first and defines every variable. User style files are optional overrides: when a variable is absent, the value from the default style remains. Every variable in the default file has a comment explaining its use.
- [ ] Style files live in the Web styles directory and are selected by safe `.css` filename only. Invalid, missing, or unreadable files fall back to the default without preventing the app or a Sand from loading.
- [ ] Styles load in this order: default tokens, LynxUI component rules, Sand
      structural CSS, configured workspace/global overrides, then per-Sand
      overrides. Choosing "inherit" removes an override rather than copying
      the inherited values.
- [ ] The configuration table selects the global style and applies it immediately. The Sand gear configuration, together with login, Protein, and behavior, shows the inherited global style and selects an optional style for that Sand. Both choices persist.
- [ ] Every isolated Sand root loads the style layers itself because CSS
      variables do not cross an iframe boundary. Trusted Sands sharing a
      composition root inherit directly. A global change updates every
      inheriting Sand without replacing local overrides.
- [ ] Colorschemes use an open set of named semantic slots rather than a
      fixed 16-slot/48-variable ceiling. The default begins with
      `primary-background`, `secondary-background`, `raised-background`,
      `primary-ink`, `secondary-ink`, `border`, `accent`, `focus`, `need`,
      `contribution`, `peace`, `info`, `success`, `warning`, `danger`, and
      `selection`. A slot may define light/default/dark or other named modes;
      omitted values inherit, and intentional repetition is valid. A Sand may
      introduce documented local slots at its own boundary.
- [ ] The default colorscheme is Lynx with Dark Lynx as its initial mode and Light Lynx as its inverse. Dark Lynx uses Deep Lead for the background and Ice White for primary characters; Light Lynx reverses them. A single icon button switches mode immediately; both modes use the same scale, geometry, and component rules.
- [ ] Lynx derives close neutral steps from Deep Lead and Ice White. Default
      component backgrounds remain Deep Lead or Ice White; the 10% lighter and
      darker variants are used only for subtle inputs and restrained shadows.
      Default content does not assign red, green, amber, or another semantic
      color to data. Purple Cobalt is Light Lynx's primary accent and
      Nocturnal Purple is Dark Lynx's; the other purple supports it.
- [ ] Advanced developer mode exposes raw Sand and workspace CSS, token
      creation, Behavior and port editing, and an inspectable live preview.
      Invalid edits are isolated and reversible and never prevent Lince from
      reopening the workspace.

- [ ] Grid: 4px base unit; spacing scale 4, 8, 12, 16, 24, 32, 48. Compact component internals may use the 2px half-unit.
- [ ] Density: slim by default. Main content regions touch with no decorative gaps or outer padding. Component padding is half the previous demo spacing. Records use 5px internal padding; form controls use 3px vertically and 5px horizontally so empty space does not exceed the text height.
- [ ] Keep a compact 4px gap between text and adjacent metadata in the same compartment, such as a column name and its card count.
- [ ] Borders: use 0.5px hairlines only when needed to show where one region ends and another starts. `.lynx-shadow` is a small tokenized CSS shadow on an individual component’s own root; it follows that element’s shape and radius exactly, has no JavaScript or wrapper cost, and may be locally directed with `--lynx-shadow-x`, `--lynx-shadow-y`, and `--lynx-shadow-blur`. Its default is dark and falls to the right and bottom. `.lynx-shadow--light` adds an optional lighter top-left companion. Buttons, Kanban cards, Kanban column headers, and the message channel-list boundary opt into the dark shadow. Inputs, textareas, and dropdown controls are flat with a foreground border. Outer Sands and large workflow regions do not use shadows. Adjacent regions share one boundary; no double borders and no boxes inside boxes. Sand outer edges have no border by default on the free canvas. Stronger focus and active state remain distinct from ordinary borders.
- [ ] Roundness: Lynx is square by default. Badges and buttons use a restrained 2px radius. Custom styles may change the radius tokens.
- [ ] Badge is the component name and keeps the `.lynx-status` class/API. It has a transparent background and primary foreground hairline border by default; its optional icon uses the same color. The component does not interpret data as good, bad, warning, or any other semantic hue. A Sand may locally set `--status-color` when its own domain calls for it.
- [ ] Invalid inputs keep the error-color border and show an in-field error icon. Hovering or focusing that icon reveals the validation message in a transparent, error-color outlined tooltip; the message also remains associated with the input for assistive technology.
- [ ] Buttons use a subtle 2px radius. Use familiar, distinct action icons such as plus, check, close, save, download, and trash; keep text when the icon alone is ambiguous.
- [ ] Radio controls are minimal circles filled completely with the accent when selected. Dropdowns, selects, and disclosures use one small chevron treatment. LynxUI selects use the library menu instead of a browser-styled popup.
- [ ] Rigidity: "firm paper." Cards hold their shape with crisp hairline borders; nothing bounces, nothing elastic.

- [ ] Prefer content directly on the Sand surface. A card is a card, not a floating card inside another box; a column is a column, not a box containing another padded box.
- [ ] Remove redundant headings and labels. Do not show both "Sand / Communication" and "Messages", or text beside a self-explanatory icon.
- [ ] Omit a Sand header when the content already explains itself. Keep visible controls focused on creating, editing, moving, completing, or replying to Records.
- [ ] Only the focused Sand shows its gray bottom-right corner. Hovering or focusing the corner reveals configuration, Protein, layout, and other Sand controls like the corner of a page turning.
- [ ] The base web uses an always-visible gray folded top-right corner for system controls. It holds the mode switch and development tools without adding a persistent header; Sand controls remain in their bottom-right corners.
- [ ] Do not add dividers when content already makes the boundary clear. Message identity and avatar separate consecutive messages without a line.
- [ ] Use the darker generated black-and-white step, never gray or the lighter step, for background separation. Kanban, Messages, and workflow regions share the canvas background without outer shadows; only individual components may opt into the shared shadow.
- [ ] Do not divide Kanban columns, column headers from their first card, cards, table rows, list rows, or ordinary adjacent items with lines. Use grouping, spacing, and close surfaces only when a boundary needs to be understood.
- [ ] Inputs, textareas, and closed dropdown controls use the base surface with a foreground border and no shadow. Invalid controls use the `hot-border` token, red in default Lynx. Unchecked checkboxes use the base surface; checked checkboxes use the accent.
- [ ] Hovering an element on the base surface uses the subtle 10%-lighter surface. Hover never darkens the current base surface.
- [ ] Forms, tables, lists, and adjacent workflow regions align to shared edges. Do not use spacing that makes neighboring compartment boundaries stop at different positions, and omit row or field lines when grouping is already clear. A separator belongs between items, never after the final item.
- [ ] Prefer clear icons for actions and give every icon button an accessible name and a tooltip on hover. Keep text when an icon would be ambiguous.
- [ ] Keep tooltips within the boundary of the Sand or component that owns them, and give SVG strokes enough internal view-box space that icons are never clipped.
- [ ] When the Sand is idle, the visible UI prioritizes Records, their state, and direct interaction with them rather than configuration of the Sand.

- [ ] Data surfaces are always opaque. Honesty rule: you must always know exactly which surface a number sits on. No glassmorphism, no frosted panels over content.
- [ ] Translucency is allowed only for ephemeral chrome: edit-mode handles, drag previews, presence cursors, auto-hiding call UI — things that are explicitly not Ledger truth.
- [ ] Overlays/scrims at 50–60% ink. Menus are opaque. Tooltips use Deep Lead with Ice White text and a hairline border in that same foreground color; dialogs use a Gray border.
- [ ] Elevation is flat by default. `.lynx-shadow` is the sole shared elevation utility and is opt-in except for ordinary buttons, which use its default dark lower-right shadow. Form controls remain flat with a foreground border. Paper doesn't hover.

- [ ] A deliberate second channel so color is never the only carrier: Solid = settled (Ledger facts, committed quantities). Dashed = declared (promises, projections, staged rules). This is as load-bearing as any hue.

"- [ ] Numbers first: tabular figures everywhere quantities appear, negative quantities use a true minus (−3), and zero and positive quantities have no sign" (0, 5). The zero state is styled quietly — peace is the one value that should never demand attention.
- [ ] Lato is the default body and interface typeface. Aleo is used mostly for titles and semantic headings. Quantities and technical metadata keep the monospace token. EN/PT is supported from day one, with generous line lengths and no cramped all-caps labels.
- [ ] Ordinary interface text is 14px by default. Compact secondary metadata stays readable at 11–12px; do not shrink routine labels or content to create density.
- [ ] Inline icons, counts, and quantity-state marks are optically centered with the adjacent text. Correct a glyph inside its SVG when its drawing is off-center; do not move the entire control.

- [ ] Remove ornamental interface transitions, pulsing, hover movement,
      startup drawing, workspace sliding, animated modal entrances, and
      JavaScript that waits for transition completion. State changes are
      immediate and never rely on green alone to mean synced or healthy.
- [ ] Direct manipulation may use motion when motion conveys the physics or
      result of the person's action: pan inertia, drawing, and current spatial
      forces qualify; future topology may qualify if revived. It must remain
      interruptible, respect reduced-motion preferences, and settle instead of
      bouncing forever. Games, terminals, and artwork own their content motion.

- [ ] Every design decision gets one question: does this get Ana to her four minutes of human choice faster, or is it the tool asking to be looked at? The Death of Lince applies to its UI first. If the user wants, they can use lince as an app that lets them create and interact with beautiful and cool things and produce awesome graphs and automation visualizations, but that is a choice, the default of lince is meeting your need to use apps like it with minimal effort.

- [ ] Migrate the board, shared components, and every official Sand from hardcoded visual values to the semantic color, spacing, border, radius, typography, elevation, and motion variables. User-owned expression colors remain data, not system chrome.
- [ ] Add a design-system check that rejects hardcoded colors in the Web interface and official Sands, excluding the default style definitions, vendored assets, and explicit user-owned expression values.
- [ ] Test default fallback, optional partial styles, safe filename handling, global persistence, per-Sand persistence and isolation, live style changes, and style loading inside iframes.
- [ ] Test quantity formatting, tabular figures, solid and dashed truth lines, and that status meaning is available through an icon, label, shape, or line style instead of color alone.

- [ ] Load styles in this order: default tokens, LynxUI, Sand structural CSS, configured global style, per-Sand style.
- [ ] Migrate every official Sand to LynxUI for ordinary interface elements and remove duplicated component CSS. Specialized graphs, terminals, games, document rendering, canvases and artwork keep their own implementation; their ordinary surrounding controls use LynxUI when practical.
- [ ] Publish a versioned LynxUI/Sand-author contract from the first external
      release. Official and user-authored Sands load the same declared version;
      an incompatible version fails closed. A version bump replaces the old
      contract rather than serving side-by-side compatibility or silently
      interpreting an obsolete shape.
- [ ] Remove `LynxDS-components.js` and rebuild any remaining official callers
      on `window.LynxUI`; do not retain a compatibility layer for persisted
      legacy shell HTML.
- [ ] Components have keyboard navigation, focus management, ARIA state, associated help and errors, non-color status meaning, and no transitions or animations.
- [ ] Treat specialized exceptions as a code-review convention, without manifest declarations or exemption attributes.
- [ ] Add concise Sand-author documentation, component behavior tests, iframe tests, theme override tests, and checks that LynxUI has no hardcoded design colors, unauthorized shadows, transitions, or animations.
} r_4Z5VS9MJNRDHZGXW4KMGRNXMHK
