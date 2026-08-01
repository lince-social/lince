# Web Interface

The Web interface is an HTML based one. It can be run in browsers or as a desktop app with Tauri.

The base app should be minimalist to give as much space as possible for user's to express themselves. That expression should feel familiar, reflecting what they want.

In addition to what we say is the base app we have possibly many components, widgets, called 'Sand'. They are HTML iframes inside a canvas, so like blocks of lego in a whiteboard. We have an edit mode for being able to move components around, add or remove them, including many other actions.

Vibe: As minimalist as possible, without loosing friendliness.
"No dashboard." The north star is Ana spending under four minutes, all of it on decisions only a human can make. The UI's job is to disappear. Attention is the scarcest resource in the system, so color, motion and elevation are spent, never decorated with.
Honesty over decoration. "A number on a chart that nobody can explain is worse than no number." Surfaces are opaque, line styles carry truth (settled vs. declared), color never carries meaning alone, as much as possible we replace colors as the main meaning (like a green ball for connection up with an icon for good connection that can be configured to have a color).
A whiteboard, not a cockpit. Sand are lego blocks on a blank canvas — dots, strokes, hand-drawn arrows, blocks the user arranges. Familiar, paper-like, user-owned. When the user makes their lince, it feels like they are creating an art piece, the built-in ui should be minimalist to not carry the composition away from the user's intention.
The brand is black and white by default, with purple as the supporting color.
Components are flat by default. A component may opt into the one restrained shadow utility when it needs to appear above an adjacent region; the area that is not content should feel tight and shunk, boiled to the essence, minimal, functional.

## Lince palette reference

Roxo Cobalt — CMYK: 66, 71, 0, 36; HEX: `#3730A3`; RGB: 55, 48, 163.
Roxo Noturno — CMYK: 59, 58, 0, 5; HEX: `#6366F1`; RGB: 99, 102, 241.
Chumbo Profundo — CMYK: 10, 10, 0, 92; HEX: `#121214`; RGB: 18, 18, 20.
Cinza — CMYK: 10, 5, 0, 12; HEX: `#A7B4C2`; RGB: 203, 213, 225.
Branco Gelo — CMYK: 2, 1, 0, 1; HEX: `#F8FAFC`; RGB: 248, 250, 252.

## Customization and architecture

- [ ] In Web Interface, user can control all the basic aspects of the ui, the padding, margin gap of elements, border radius, thickness and colorscheme. In web version there should not be even one color hardcoded, only use tags like primary-background, or light-accent. The default style should come from the main style .css file, that has comments on every variable to explain where it is used, so when people make their .css files and add to dir of styles and choose in configuration table which style they want (name of file) they get the variables values from file and the app changes (either on boot if makes app faster or during setting). When we speak of specific details of style here like default colorscheme and scale units we are talking about default file, if people want they can customize it.
- [ ] Architecture (from Sand: Colorschemes): The system is defined as named semantic tokens, not hex values — surface-raised, ink-primary, need, contribution, accent, focus — resolved per active colorscheme at runtime (the old a2 Operation to switch schemes is the spiritual ancestor). Scaling tokens (padding-s, radius-m) ride the same mechanism. A Sand author never picks a color; they name a slot, and the user's scheme decides what it looks like. That's how "the base app is minimalist so users can express themselves" survives contact with real widgets.
- [ ] The default style is always loaded first and defines every variable. User style files are optional overrides: when a variable is absent, the value from the default style remains. Every variable in the default file has a comment explaining its use.
- [ ] Style files live in the Web styles directory and are selected by safe `.css` filename only. Invalid, missing, or unreadable files fall back to the default without preventing the app or a Sand from loading.
- [ ] Styles load in this order: default style, configured global style, per-Sand style. The global style is stored in the configuration table. A per-Sand style is stored in that card's host state and overrides only that Sand; choosing "inherit global style" removes the override.
- [ ] The configuration table selects the global style and applies it immediately. The Sand gear configuration, together with login, Protein, and behavior, shows the inherited global style and selects an optional style for that Sand. Both choices persist.
- [ ] Every Sand iframe loads the style layers itself because CSS variables from the board do not cross the iframe boundary. Changing the global style updates Sands that inherit it without replacing a Sand's own override.
- [ ] The colorscheme has 16 semantic color slots, each with light, default, and dark values, for 48 color variables: primary-background, secondary-background, raised-background, primary-ink, secondary-ink, border, accent, focus, need, contribution, peace, info, success, warning, danger, and selection. The default Lince style may repeat colors between slots and tones; custom styles may define all 48 for finer control. LynxUI does not map backend data or a badge type to those slots; a Sand may opt into a local mapping at its own boundary.
- [ ] The default colorscheme is Lynx with Dark Lynx as its initial mode and Light Lynx as its inverse. Dark Lynx uses Chumbo Profundo for the background and Branco Gelo for primary characters; Light Lynx reverses them. A single icon button switches mode immediately; both modes use the same scale, geometry, and component rules.
- [ ] Lynx derives close neutral steps from Chumbo Profundo and Branco Gelo. Default component backgrounds remain Chumbo or Branco; the 10% lighter and darker variants are used only for subtle inputs and diffuse shadows. Default content is void-background with white or gray foreground; it does not assign red, green, amber, or any semantic color to data. Roxo Cobalt is the primary accent in Light Lynx and Roxo Noturno is the primary accent in Dark Lynx for stronger contrast; the other purple is the supporting accent.

## Space, thickness, roundness, rigidity

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

## Content and Sand chrome

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

## Transparency and elevation

- [ ] Data surfaces are always opaque. Honesty rule: you must always know exactly which surface a number sits on. No glassmorphism, no frosted panels over content.
- [ ] Translucency is allowed only for ephemeral chrome: edit-mode handles, drag previews, presence cursors, auto-hiding call UI — things that are explicitly not Ledger truth.
- [ ] Overlays/scrims at 50–60% ink. Menus are opaque. Tooltips use Chumbo Profundo with Branco Gelo text and a hairline border in that same foreground color; dialogs use a Cinza border.
- [ ] Elevation is flat by default. `.lynx-shadow` is the sole shared elevation utility and is opt-in except for ordinary buttons and form controls, which use its default dark lower-right shadow. Paper doesn't hover.

## Line, shape, and texture grammar

- [ ] A deliberate second channel so color is never the only carrier: Solid = settled (Ledger facts, committed quantities). Dashed = declared (promises, projections, staged rules). This is as load-bearing as any hue.

## Typography

- [ ] Numbers first: tabular figures everywhere quantities appear, negative quantities use a true minus (−3), and zero and positive quantities have no sign (0, 5). The zero state is styled quietly — peace is the one value that should never demand attention.
- [ ] Lato is the default body and interface typeface. Aleo is used mostly for titles and semantic headings. Quantities and technical metadata keep the monospace token. EN/PT is supported from day one, with generous line lengths and no cramped all-caps labels.
- [ ] Ordinary interface text is 14px by default. Compact secondary metadata stays readable at 11–12px; do not shrink routine labels or content to create density.
- [ ] Inline icons, counts, and quantity-state marks are optically centered with the adjacent text. Correct a glyph inside its SVG when its drawing is off-center; do not move the entire control.

## Motion

- [ ] No animations, things change instantly, and they dont pulse, if something is green it is synced and ok.
- [ ] Remove interface transitions, keyframe animations, hover movement, startup drawing, workspace sliding, animated modal entrances, and JavaScript that waits for transition completion. Content owned by a Sand, such as a game or terminal, is not interface motion.

## The test

- [ ] Every design decision gets one question: does this get Ana to her four minutes of human choice faster, or is it the tool asking to be looked at? The Death of Lince applies to its UI first. If the user wants, they can use lince as an app that lets them create and interact with beautiful and cool things and produce awesome graphs and automation visualizations, but that is a choice, the default of lince is meeting your need to use apps like it with minimal effort.

## Design system work

- [ ] Migrate the board, shared components, and every official Sand from hardcoded visual values to the semantic color, spacing, border, radius, typography, elevation, and motion variables. User-owned expression colors remain data, not system chrome.
- [ ] Add a design-system check that rejects hardcoded colors in the Web interface and official Sands, excluding the default style definitions, vendored assets, and explicit user-owned expression values.
- [ ] Test default fallback, optional partial styles, safe filename handling, global persistence, per-Sand persistence and isolation, live style changes, and style loading inside iframes.
- [ ] Test quantity formatting, tabular figures, solid and dashed truth lines, and that status meaning is available through an icon, label, shape, or line style instead of color alone.

## LynxUI

- [x] Select Lynx and keep one evolving light/dark demo at [`lynx-ui-concepts/lynx.html`](lynx-ui-concepts/lynx.html). Update this design-system description whenever the demo guidelines change.
- [x] Build the LynxUI base as a framework-free component library for official Sands. Use native semantic HTML, explicit `lynx-*` classes, and a small JavaScript layer only for behavior that HTML does not provide consistently.
- [x] Serve shared `lynx-ui.css` and `lynx-ui.js` assets. LynxUI uses the design-system tokens and defines no separate colorscheme, spacing scale, motion, or elevation. Component selectors have low specificity so global and per-Sand styles can override them.
- [x] Provide Catppuccin Macchiato as the second style. Its CSS changes only colorscheme variables, uses the official Base, Mantle, Crust, Text, Subtext, Overlay, Mauve, Lavender, Red, Yellow, Green, and Blue values, and carries the Catppuccin MIT notice.
- [ ] Load styles in this order: default tokens, LynxUI, Sand structural CSS, configured global style, per-Sand style.
- [x] The first component set has buttons and button groups; inputs, textareas, selects, checks, radios, labels, help and errors; boxes, panels, stacks, rows, grids, toolbars and dividers; badges, callouts and empty states; tables, lists, dropdowns, tooltips, dialogs, tabs and disclosures; and a small first-party SVG icon set.
- [ ] Add consistent native date, time, and datetime-local fields, plus compact absolute and relative date display, for Record work metadata and Kanban card metadata.
- [ ] Add an accessible combobox/autocomplete with a suggestion list, keyboard navigation, and single-select support. Record uses it for link kinds and targets; Kanban can use it for column and concept choices.
- [ ] Add a removable token picker for Record assignees, selected links, and thread tags. It composes the combobox and badge instead of creating a separate data model.
- [ ] Add a file attachment primitive: picker trigger, upload/busy state, attachment row, and remove action. Record owns message attachment semantics and previews.
- [ ] Add a compact semantic metadata list (`dl`) for Record head/slug/quantity/work facts and Kanban card metadata; it is a flat key–value display, not a panel.
- [ ] Add a compact duration field for Record estimates and worklog values. Record owns its timer and worklog behavior.
- [ ] Document a destructive confirmation-dialog composition using the existing dialog, for Kanban bulk deletion and Record hard deletion.
- [ ] Add an anchored action/context menu based on the existing menu behavior, for Record links and attachments and Kanban cards.
- [ ] Add small inline loading and progress states for Record saves, uploads, and Kanban moves; transient notifications, avatars, and range sliders remain out of scope until a Sand needs them.
- [x] Static components use native markup and classes. Interactive components use `data-lynx-*` attributes and one delegated event and keyboard handler per iframe. `window.LynxUI` provides icon and icon-button helpers for dynamic elements.
- [ ] Migrate every official Sand to LynxUI for ordinary interface elements and remove duplicated component CSS. Specialized graphs, terminals, games, document rendering, canvases and artwork keep their own implementation; their ordinary surrounding controls use LynxUI when practical.
- [ ] LynxUI is official-first and initially unversioned. User-authored Sands may load the same assets, but compatibility is not promised until the API is deliberately stabilized.
- [ ] Keep `LynxDS-components.js` compatible with persisted older shell HTML while new code uses `window.LynxUI`.
- [ ] Components have keyboard navigation, focus management, ARIA state, associated help and errors, non-color status meaning, and no transitions or animations.
- [ ] Treat specialized exceptions as a code-review convention, without manifest declarations or exemption attributes.
- [x] Add a development-only LynxUI Gallery with one scrollable page: a compact showcase of all LynxUI components sits beside a stacked set of seeded Kanban, message, inventory, and request-review Sand previews at their normal board sizes. It uses canonical assets and fixture data without Protein or Action requests. `mise run lynxui` serves it at `http://127.0.0.1:6175` and recompiles the gallery package for Lince on source changes.
- [x] Add a folded base control that highlights LynxUI components and shows their component names on hover. Sand-specific structure stays unmarked so the boundary is clear.
- [ ] Add concise Sand-author documentation, component behavior tests, iframe tests, theme override tests, and checks that LynxUI has no hardcoded design colors, unauthorized shadows, transitions, or animations.

<!-- - [ ] Chart Library -->
<!-- - [ ] Be able to draw (arrows, boxes, text and erasing at first is ok). If we fill the space with too much stuff in lets say, svg it will at some point if the person is making a complext diagram or making a painting frame around their component it will get heavy and make the app slow. We must solve that problem, excalidraw does that really well, putting a lot of drawings on the screen, lots of elements, doesnt make it slow. How do they do it? what do they implement? -->

# Web Platform

## Board and sand infrastructure — the shipped web surface

- [x] One WebSocket (`/host/transport/ws`) shared by the unified bridge and the Data panel; the bridge speaks both the legacy nested-payload chrome shape and the current flat `frame.js` shape, routing by subscription id and lane room (ids never collide across consumers).
- [x] Sands are Rust-canonical: each official sand is a self-contained `.html` via `include_str!`, registered in `OFFICIAL_WIDGETS`; groups ship as `.lince` workspace archives; the catalog peeks content so a group archive is never mis-parsed as a single sand, and a group entry replaces a same-named single sand.
- [x] Groups nest: `BoardCard.group_ids` (outer → inner) is authoritative; disbanding an outer group preserves inner ones; adding a catalog group re-homes to a fresh inner id each time, so repeated adds are independent.
- [x] Events are scoped to a grouped sand's innermost group; ungrouped sources broadcast board-wide; cross-session mirroring rides lane rooms, never persisted.
- [x] (2026-07-19) Kanban, Relations, and Communication no longer ship as a GROUP bundled with their own Record sand — every board already has exactly one pinned Record (`shell-record`, bottom-right corner, icon by default), so bundling a second one per sand was redundant and, worse, its group scoping meant a grouped kanban's `recordClicked` never reached the pinned one. These three now ship as plain single `.html` packages (ungrouped), so their board-wide `recordClicked`/`recordCreate` reaches the pinned Record directly. The generic group-archive machinery (`.lince` workspace archives, `is_group` catalog entries, drag-drop import) stays for user-authored/imported groups — only the three OFFICIAL auto-grouped catalog entries were removed. Kanban's default add-to-board size also grew (`initial_width`/`initial_height` 6×6, up from 7×5 pre-clamp) since it's no longer sharing space with a bundled Record card.
- [x] Per-card host state flows both ways (`H.getCardState()`/`H.onCardState`/`H.patchCardState`) — any sand persists UI prefs without touching the Ledger; board chrome itself (pan/zoom/workspaces/position/size/pin/z-index/grouping/edit mode) is ALWAYS host state, never a Ledger fact.
- [x] The Data panel is the one place Protein gets configured (source, filters, sort, limit, includes) per card — sands ship with NO default driving Protein; an unconfigured card shows an explicit "pick a Protein" prompt instead of silently dumping every record. The builder autocompletes link-kind inputs from a `concept` source subscription; "All records" drives an explicit `{source:"record"}`, distinct from "unconfigured." The links include is MULTI-KIND ("+ kind" rows, `"*"` = every kind, both AST spellings round-trip) — one Protein pulls several link types and the relations graph draws parallel kinds between the same two nodes as fanned-out bent lines.
- [x] The shared slash-block editor (`window.LinceBodyEditor`) is used by every sand that touches record bodies: `/` opens a Notion-like block palette (headings, image placeholder, checkbox), `@` opens the record picker; the body stays canonical markdown, checkboxes toggle by original line index, `@slug` chips navigate and become real `references` links on save. Optional — a sand without it degrades to a plain textarea.
- [x] Local images: the editor's "/image" block picks/uploads a file (native OS dialog first, browser `<input type=file>` fallback), sniffs bytes against a raster allowlist, and stores under an opaque generated name — there is still no route serving an arbitrary disk path.
- [x] Action `warnings` reach sands end-to-end (bridge → `frame.js` → amber sand status), never surfaced as errors.
- [x] Record deletion is permission-gated (`record:delete` vs `record:delete_own` + creator match) at the one `DeleteRecord` action — since threads/messages are themselves records, this single gate covers all three; viewer identity (`H.getViewer()`/`H.onViewer`) flows to every sand so delete controls can show/hide correctly, though the engine gate (not the UI hint) is what actually enforces it.
- [x] The permission/role/user system is Protein(`source:"auth"`) + five gated Actions (`create-role`, `create-user`, `assign-role`, `grant-permission`, `revoke-permission`) — a plain CRUD sand on top, no different in kind from any other sand; auth-table mutations emit no facts, so the sand re-subscribes after every mutation instead of relying on live invalidation.
<!-- - [ ] Per-sand capability/permission model before imported sands can write arbitrary Actions (today any sand can call any Action — fine for official sands, needed before running imported ones freely); sand provenance `cause=sand:<uid>`. -->
<!-- - [ ] Blanket read/write permission enforcement across every OTHER Protein source and Action (today only `delete-record` and the five auth actions are gated) — sequenced after more of the role-management UI exists. -->
<!-- - [ ] `.lince` GROUP drag/drop import: client routing still checks the `.group.sand` extension — route by content instead, like the catalog does. -->
<!-- - [ ] Host-state sync for board presentation state across devices. -->
<!-- - [ ] Package import/publish subsystem on the new record/package model. -->

## Wire protocol — how a sand talks to the Cell

- [x] One WebSocket (`/host/transport/ws`), multiplexed: Protein (reads) + Actions (writes) + ephemeral lanes (presence/cursors/events) + explicit host capabilities (e.g. a terminal PTY session) whose bytes don't belong in the Ledger.
- [x] Actions are JSON with a kebab-case `"action"` tag, snake_case everywhere else; Protein predicates/includes are snake_case too.
- [x] A subscription answers with a snapshot then re-executes and pushes on every relevant commit; invalidation is coarse-by-source — render idempotently, a sand may get refreshes it doesn't strictly need.
- [x] Action responses carry `created`, `facts` (what the Ledger committed, including any Karma cascade), and `warnings` (non-fatal advisories) — show warnings, never treat them as errors.
- [x] Ephemeral-lane and host-capability traffic (cursors, clicks, presence, PTY bytes) is never persisted; terminal PTYs are scoped to one connection and die with it.
