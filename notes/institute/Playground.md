# Archive Sand (Static Workspace Export)

Refined 2026-07-19. Earlier drafts described a second public *host* (a
sandbox/archive server). That is shelved (see the box at the end). The thing
that matters now is much smaller:

> Arrange sands however you like in one workspace, with data present in
> them. Click a button on an **Archive sand**. Out comes a single static
> HTML file of that workspace — the sands as they looked, the data as it was
> at that moment, the Archive sand itself excluded. The file makes no
> requests, ever. The page area is the bounding rectangle of the sands, so
> the infinite canvas shrinks to fit.

It is not a mode, not a binary, not a route policy. It is an export. The
output is a file you can email, drop on any static host (GitHub Pages, a
`python -m http.server`, an attachment), or open from disk. Lince does not
serve it and does not need to be running for it to work. That is the
portfolio story: the "personal website" is just this file.

## The flow

1. Add the Archive sand to a workspace (it is a normal sand under
   `crates/web/src/sand/`, one card among the others).
2. Arrange the other sands; let them load their data as usual.
3. Click **Archive**. The sand posts one message to the host chrome over the
   existing bridge.
4. The host chrome performs the capture (below) and hands the finished HTML
   back as a download (and/or writes it next to the media dir).

The capture runs host-side (in `main.js`-land), not inside the Archive
sand's iframe. This is forced by the platform: sand iframes cannot reach
sibling iframes' documents, but the board chrome owns every card iframe and
they are same-origin (`/sand/*`), so only the chrome can read their rendered
DOM. The Archive sand is just the button and the options UI; the bridge
message is the trigger.

## The capture

For each card in the current workspace **except the Archive sand's own
card**:

- Serialize the iframe's live document (`documentElement.outerHTML`). This
  is "the data that was present when archived" by construction — whatever
  rows the sand had rendered are simply in its DOM. No re-querying, no
  snapshot API, no Protein involvement at capture time.
- **Strip every `<script>`** and every `on*=` attribute from the serialized
  document, including the widget-frame bootstrap. Also strip
  `<link rel=preload/modulepreload>` and any `<meta http-equiv=refresh>`.
- **Inline external references.** Stylesheets referenced by `<link>` get
  inlined as `<style>`; images (including `/host/media/*` and `/static/*`
  paths) get fetched once at capture time and embedded as `data:` URIs.
  After this pass the document references nothing but itself. Anything that
  cannot be inlined is dropped, not left as a live URL.
- `<canvas>` elements are rasterized (`toDataURL()`) and replaced with
  `<img>`, since a serialized canvas is blank.

Each captured document is embedded in the output page as
`<iframe srcdoc="..." sandbox>` — the empty `sandbox` attribute is the
belt-and-braces layer: even if the stripping pass ever misses an executable
path, the browser refuses to run scripts, submit forms, navigate, or reach
any origin. Stripping is for cleanliness; `sandbox` is the guarantee.

The output file itself contains one small inline script of our own for
panning (optional — plain browser scrolling already works) and nothing else.
A `Content-Security-Policy` meta tag (`default-src 'none'; img-src data:;
style-src 'unsafe-inline'`) is stamped into the output head so the file
declares its own no-network policy wherever it is hosted.

## Layout: shrink to fit

- Compute the bounding rectangle of all captured cards (union of their
  board rects, Archive sand excluded), plus a small padding.
- The page body is exactly that rectangle's size; each card becomes an
  absolutely positioned, fixed-size element at
  `(card.x − rect.left, card.y − rect.top)`, same z-order as the board.
- "Moving around" is native scrolling of that finite page (optionally
  drag-to-pan via the one inline script). No infinite canvas, no viewport
  math, no board runtime shipped — the grid/viewport/interactions JS of the
  live board does not go into the file.

## Why this is safe, in one paragraph

There is no server surface: no routes exist because nothing is served. The
file cannot make requests: scripts are stripped, every subdocument is under
`sandbox` (script-less, origin-less), all assets are `data:` URIs, and the
CSP meta forbids network fetches besides. There is no data beyond what the
person archiving could already see rendered on their own screen at that
moment — the capture reads the DOM, not the store, so it can never expose
more than the archiving user's own view. Board-write permissions, rate
limiting, visibility subjects: all irrelevant here, because the artifact has
no backend.

The one real care point: **the archiver is the disclosure decision.** The
file contains whatever was on screen, verbatim. The Archive sand's UI should
say so plainly ("this exports exactly what you see, as a public file").

## What survives, what becomes inert

Survives: layout, text, tables, styles, images, current sand-rendered data,
z-order — the look of the workspace at the moment of the click.

Becomes inert: everything interactive. Buttons don't act, inputs don't
submit, live subscriptions don't update, terminals are frozen text, kanban
doesn't drag. This is the correct product outcome for an archive; nothing
should be "gracefully degraded" into making requests.

## Implementation (landed 2026-07-19)

1. **Archive sand** — `crates/web/src/sand/archive/` (button + optional
   filename), registered in the official catalog. Posts one flat bridge
   message via the new `LinceWidgetHost.archiveWorkspace(options)` in
   `frame.js`.
2. **Bridge routing** — `widget-bridge.js` handles
   `lince:archive-workspace`, guarded by `isCurrentFrameSource` (same guard
   as terminals: only the card's real iframe may trigger it) and forwards to
   the chrome's `archiveWorkspace` callback.
3. **Capture + compose** — `crates/web/static/presentation/board/archive.js`
   (`buildWorkspaceArchive`): serializes each card iframe's live DOM,
   reflects form state (password-ish values skipped) and canvases
   (`toDataURL` → `<img>`), collects the CSSOM into one inlined `<style>`,
   inlines `<img>` sources as `data:` URIs, strips scripts/on*/links/bases/
   javascript: URLs/srcset/media srcs, neutralizes external CSS `url()`s,
   and composes the bounding-rect page of sandboxed `srcdoc` iframes with
   the CSP meta. Text cards render as escaped static sections; the
   requesting card and system/pinned shell cards are excluded.
4. **Chrome wiring** — `main.js` `runWorkspaceArchive`: active-workspace
   snapshot + `getPackageFrameNode`, then `downloadTextFile(...)` and a
   status flash (skipped cards listed).
5. **Selftest** — `scripts/other/archive-sand-selftest.sh` (chromium,
   node-free, file://): real archive sand button through the real bridge,
   asserts data present, secrets absent, scripts/on* gone, canvas
   rasterized, offsets/z-order/stage size, shell + self exclusion, impostor
   postMessage rejected, nothing sent over the transport, and the produced
   file re-opened in chromium with zero external references.

---

<details>
<summary>📦 Shelved ideas (kept for later, not current scope)</summary>

Short summaries of everything the earlier drafts of this file explored.
None of it is needed for the Archive sand; revisit only if a *served*
public surface ever becomes a goal again.

- **Live archive host (Tier 1 / pinned queries).** A separate sealed
  server that serves a published board with live read-only data. Each
  card's Protein frozen at publish into a server-side manifest; the public
  endpoint accepts a single verb `subscribe_pinned{id}` — no arbitrary
  Protein, no Actions, no terminal, no lanes (those exist in the normal WS
  protocol and must never be publicly mounted; `TerminalOpen` is a PTY).
- **`public` visibility subject.** If queries ever run server-side for
  visitors, they run as a dedicated Protein subject with explicit
  visibility grants — never `subject: None`, which the visibility gate
  (`protein::execute_for`) treats as the all-seeing local Cell.
- **`web:state` permission.** `PUT /host/board/state` today accepts any
  valid JWT (and no auth when `local_auth_required=false`). A real
  permission check gating board writes on the normal Cell is worth doing
  regardless of any public mode, and would ship as its own small slice.
- **Sealed feature-gated binary.** Any public host would be a separate
  router built from scratch (forbidden routes absent, not disabled) behind
  a cargo feature — runtime config alone doesn't control binary
  composition.
- **Rate limiting.** Per-IP connection caps, subscription cap equal to the
  pinned-manifest size, and update coalescing on the fact-bus-driven
  re-execution — the resource wall for the live variant only.
- **Published-preset + local overlay (oldest draft).** Server-published
  preset, visitor-local overlay in `localStorage`, explicit reset,
  allowlisted SSE bindings per card. Superseded: the overlay/edit idea is
  gone entirely, and SSE-binding allowlists are the weaker ancestor of the
  pinned-query manifest.
- **Multi-workspace published artifact.** The served variants needed a
  whole-board archive shape (all workspaces, ordered, with cards and
  packages). The Archive sand deliberately scopes to one workspace; a
  multi-workspace static export (workspace switcher baked into the output
  file) is a natural later extension that stays request-free.

Durable conclusions that apply to any future served variant: widget
discipline is not a security model — the router and the session *type* are
the wall; hiding UI is not removing capability; the normal board client
assumes broad `/host/*` access and cannot be reused unchanged on a public
surface.

</details>
