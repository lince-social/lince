> Extracted from `docs/Central: Karma.md` on 2026-07-29. This is the machinery
> every sand sits on; the individual surfaces are in `docs/Sand: Index.md`.

## [x] Board and sand infrastructure — the shipped web surface

- [x] One WebSocket (`/host/transport/ws`) shared by the unified bridge and
  the Data panel; the bridge speaks both the legacy nested-payload chrome
  shape and the current flat `frame.js` shape, routing by subscription id
  and lane room (ids never collide across consumers).
- [x] Sands are Rust-canonical: each official sand is a self-contained
  `.html` via `include_str!`, registered in `OFFICIAL_WIDGETS`; groups ship
  as `.lince` workspace archives; the catalog peeks content so a group
  archive is never mis-parsed as a single sand, and a group entry replaces a
  same-named single sand.
- [x] Groups nest: `BoardCard.group_ids` (outer → inner) is authoritative;
  disbanding an outer group preserves inner ones; adding a catalog group
  re-homes to a fresh inner id each time, so repeated adds are independent.
- [x] Events are scoped to a grouped sand's innermost group; ungrouped
  sources broadcast board-wide; cross-session mirroring rides lane rooms,
  never persisted.
- [x] (2026-07-19) Kanban, Relations, and Communication no longer ship as a
  GROUP bundled with their own Record sand — every board already has exactly
  one pinned Record (`shell-record`, bottom-right corner, icon by default),
  so bundling a second one per sand was redundant and, worse, its group
  scoping meant a grouped kanban's `recordClicked` never reached the pinned
  one. These three now ship as plain single `.html` packages (ungrouped),
  so their board-wide `recordClicked`/`recordCreate` reaches the pinned
  Record directly. The generic group-archive machinery (`.lince` workspace
  archives, `is_group` catalog entries, drag-drop import) stays for
  user-authored/imported groups — only the three OFFICIAL auto-grouped
  catalog entries were removed. Kanban's default add-to-board size also grew
  (`initial_width`/`initial_height` 6×6, up from 7×5 pre-clamp) since it's no
  longer sharing space with a bundled Record card.
- [x] Per-card host state flows both ways (`H.getCardState()`/
  `H.onCardState`/`H.patchCardState`) — any sand persists UI prefs without
  touching the Ledger; board chrome itself (pan/zoom/workspaces/position/
  size/pin/z-index/grouping/edit mode) is ALWAYS host state, never a Ledger
  fact.
- [x] The Data panel is the one place Protein gets configured (source,
  filters, sort, limit, includes) per card — sands ship with NO default
  driving Protein; an unconfigured card shows an explicit "pick a Protein"
  prompt instead of silently dumping every record. The builder autocompletes
  link-kind inputs from a `concept` source subscription; "All records"
  drives an explicit `{source:"record"}`, distinct from "unconfigured." The
  links include is MULTI-KIND ("+ kind" rows, `"*"` = every kind, both AST
  spellings round-trip) — one Protein pulls several link types and the
  relations graph draws parallel kinds between the same two nodes as
  fanned-out bent lines.
- [x] The shared slash-block editor (`window.LinceBodyEditor`) is used by
  every sand that touches record bodies: `/` opens a Notion-like block
  palette (headings, image placeholder, checkbox), `@` opens the record
  picker; the body stays canonical markdown, checkboxes toggle by original
  line index, `@slug` chips navigate and become real `references` links on
  save. Optional — a sand without it degrades to a plain textarea.
- [x] Local images: the editor's "/image" block picks/uploads a file (native
  OS dialog first, browser `<input type=file>` fallback), sniffs bytes
  against a raster allowlist, and stores under an opaque generated name —
  there is still no route serving an arbitrary disk path.
- [x] Action `warnings` reach sands end-to-end (bridge → `frame.js` → amber
  sand status), never surfaced as errors.
- [x] Record deletion is permission-gated (`record:delete` vs
  `record:delete_own` + creator match) at the one `DeleteRecord` action —
  since threads/messages are themselves records, this single gate covers
  all three; viewer identity (`H.getViewer()`/`H.onViewer`) flows to every
  sand so delete controls can show/hide correctly, though the engine gate
  (not the UI hint) is what actually enforces it.
- [x] The permission/role/user system is Protein(`source:"auth"`) + five
  gated Actions (`create-role`, `create-user`, `assign-role`,
  `grant-permission`, `revoke-permission`) — a plain CRUD sand on top, no
  different in kind from any other sand; auth-table mutations emit no
  facts, so the sand re-subscribes after every mutation instead of relying
  on live invalidation.
- [ ] Per-sand capability/permission model before imported sands can write
  arbitrary Actions (today any sand can call any Action — fine for official
  sands, needed before running imported ones freely); sand provenance
  `cause=sand:<uid>`.
- [ ] Blanket read/write permission enforcement across every OTHER Protein
  source and Action (today only `delete-record` and the five auth actions
  are gated) — sequenced after more of the role-management UI exists.
- [ ] `.lince` GROUP drag/drop import: client routing still checks the
  `.group.sand` extension — route by content instead, like the catalog
  does.
- [ ] Host-state sync for board presentation state across devices.
- [ ] Package import/publish subsystem on the new record/package model.

