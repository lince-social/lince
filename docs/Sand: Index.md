> Extracted from `docs/Central: Karma.md` on 2026-07-29. A roll-up of every
> shipped and planned surface. Each sand with its own spec has a `Sand: *.md`
> beside this one; this file is the status board, not the specification.

## [ ] Sands — the individual surfaces

- [x] **Table** — the rebuild template: cell edits map by column to typed
  Actions.
- [x] **Todo** — focus queue + `set-quantity` undo/redo.
- [x] **Kanban** — quantity-lane board (fully overridable lanes), per-lane
  collapse/resize; full column system (create/rename/delete/reorder/hide,
  bucket by value/range/concept, shareable presets, optional per-column
  color wash); three body view modes (head/compact/full) with per-card/
  column/board override; writes records four ways (checkbox toggle, column
  move, in-place body edit, bulk delete with confirm); card click opens the
  grouped Record, "+" opens Record's creation mode; badges assignee/parent
  via a side concept-name subscription; three-state live dot.
- [x] **Relations** (ships as the group with Record) — d3 force graph with
  physics sliders, golden-angle layout, zoom/pan/fit, directed arrows,
  edge-kind labels; Shift+drag adds a link (optimistic), edge-click selects,
  the header chip's ✕ removes; **Trail mode** lays a root's forward
  link-tree out topologically with a Done/Undo promotion cascade over
  shared status presets (quantity or concept buckets, e.g.
  `@todo/@next/@wip/@done` — the SAME vocabulary a kanban column can use);
  resizable controls panel, link keybinds (Delete unlinks, Ctrl+Z undoes a
  session-local stack); link-kind inputs autocomplete from concepts.
- [x] **Relations node gravity (tree weight refactor)** — one extra physics
  variable on top of the existing forces: each node gets a *weight* from its
  depth in the selected link tree — the root is heaviest (or lightest, when
  inverted) and each hop toward the leaves gets lighter; nodes not connected
  to the root weigh the same as leaf nodes. A vertical gravity bias then
  pulls heavy nodes down and lets light ones float up, so the tree settles
  into root-down/leaves-up (or inverted, root-up/leaves-down) while charge,
  link, collision, and center forces keep working unchanged. Configurable
  per sand: gravity direction toggle (root sinks / root floats), strength
  slider, and which link kind/root defines the tree (reuse the Trail mode
  root picker). Applies in both graph mode and Trail mode — in Trail mode it
  replaces/augments the fixed topological ranks with the same simulated
  gravity so the done/next/ahead coloring stays readable on a physically
  settled tree. (Shipped: Graph controls → Node gravity section. Weight maps
  to BUOYANCY — a constant per-node vertical acceleration, not a target
  line, so nodes keep falling until their link tethers them and the tree
  hangs like a mobile; the center forces stay on for cohesion while
  charge/link soften. Graph mode simulates all nodes this way; Trail mode
  unpins the rows and simulates only tree nodes, x anchored to the topo
  layer.)
- [x] **Record** (formerly "record_info" — the sole markdown editor,
  viewer, and creator for a record, and the home for every other
  per-record concern) — the get view IS the edit view (head/slug/quantity/
  body writable, Save writes only what changed, a dirty form is never
  clobbered by live updates); Zero (`deactivate`) and Delete
  (`delete-record`, permission-gated) are separate buttons; creation mode
  shows the same fields empty, Create + focuses the new record; carries the
  shared slash-block editor (headings/images/checkboxes/`@slug`, the same
  palette everywhere in a body); collapsible sections for **Work**
  (start/due dates, estimate, worklogs with play/pause, on the `work`
  record extension, offline-queued writes), **Assignees** (`assigned-to`
  links), **Links** (every hop-1 link either direction, kind+target inputs,
  both autocompleted — a document/URL just lives as a link or inline media
  in the body, no separate resource/attachment concept), and **Threads**
  (a real multi-thread system — a tab per thread, search
  filters which tabs list without hiding messages, each message shows
  timestamp + sender, `@slug` in a post becomes a real link, delete
  controls per permission). Reusable — any sand drives it via a scoped
  `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel.
  Full real-time collaborative editing is blocked on the CRDT text relay in
  `docs/Central: Sync and Organs.md`.
- [x] **Organ** — Protein list of `kind=organ` records (this Cell + its
  contacts); selecting one shows/edits its `lince.file_sync` extension
  (enabled, disk path) via `set-extension` — File Sync to disk as a
  first-class per-organ feature. Deliberately thin: no trust/proximity/
  introduce/block/quarantine UI yet (that's the separate **Organ contacts
  manager** item below); the dormant, unregistered pre-Protein
  `organ_management` sand was left in place rather than adapted.
- [x] **Roles & Permissions** — role cards with one checkbox per catalog
  permission, a users table with a role select, "+ New role"/"+ New user"
  forms; a Forbidden response reverts the optimistic toggle inline, no
  second enforcement layer.
- [x] **Document Viewer** (embed-honest) — PDF/EPUB/image rendering, opaque
  authenticated media paths, no Protein/Action/lane/proxy of its own.
- [x] **Freedoom** (embed-honest) — local wasm/WAD, true solo mode, no Cell
  data plane.
- [x] **Lince Logo LED** (embed-honest) — selected visual mode as host
  state.
- [x] **Ghostty Terminal** (embed-honest) — explicit `terminal_session`
  frame API over the one transport socket, connection-scoped local PTYs, no
  separate socket.
- [ ] **Home manager / dashboard** — aggregate Proteins + Action writes, no
  new backend needed.
- [x] **Karma sand** — shipped 2026-07-26, renamed to `sand.karma` on
  2026-07-28 (it was briefly `sand.economy`, and before that an unwired Finance
  placeholder). Economy is a preset of Records and concepts loaded into it, not
  a sand of its own. One-line capture, correction/void/re-tag, recurring rules
  with a compound cadence and an apply/skip inbox, and a concept timeline
  spanning settled past, current position and declared future. Specified in
  **`docs/Sand: Karma.md`**; there is no `source:"economy"` and there never
  will be — it reads `Source::Entry`, `Source::Recurrence` and
  `Source::Timeline`, which know nothing about money. Still open: monthly
  dashboards, source profiles, Fiote capture review, and the driven-browser
  selftest.
- [ ] **Pantry / inventory dashboard** — `availability` + `projection`
  includes render "8 now, 5 available, 2 by Friday" with no math in the
  sand; crossing decisions already surface "you run out Thursday."
- [ ] **Organ contacts manager** — list contacts with trust/proximity/sync
  policy, introduce via URL, block button, quarantine viewer.
- [ ] **Todo polish**: create-task UI/Action, richer history backed by
  compensation/facts, live-update proof beyond the stubbed bridge, plus the
  old table sand's deferred keyboard-grid navigation, helix mode, and
  concept/unit inline editors.
- [ ] Every ported data-plane sand needs a driven chromium selftest
  (snapshot + Action round-trip + live update); two stale scripts
  (`board-selftest.sh`, `table-sand-selftest.sh`) still reference deleted
  crates/files and need rewriting on the current architecture.

