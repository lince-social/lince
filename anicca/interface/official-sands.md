# Existing official Sands

Purpose: Preserve user-visible behavior of current first-party Sands while their implementation is rebuilt on the new composition system.

Owner source: no dedicated Sands Record currently exists;
[Interface in Lince](../Lince.lingua) governs shared interface decisions.

Status: all 25 official roots have an inspectable Rust decomposition;
Configuration is landed; edit controls, zoom controls, Record, Conversation,
Table, Todo and Kanban have native retained Behavior. Record, Conversation and
private message drafts are bound to live production Protein subscriptions,
the collection roots reuse the general Record result, and 17 roots remain
structure-only at that source checkpoint. Delivery now splits them between Dogfeeding, later native follow-through and the surfaces that still need a browserless design.

Read when: migrating or regression-testing Record, Kanban, Relation, Table, or another official Sand.

[Corpus map](README.md) · [Current context](current.md) · [Sand plan](plans/sands.md)

---

## Bevy-native migration rule

The catalog and reports below describe the earlier source checkpoint, not a
completed Bevy migration. Rebuild ordinary Sands directly with Bevy UI, text,
input, picking and rendering, retaining their user-visible behavior and real
Protein/Action boundaries. Existing frontend implementations may be replaced;
there is no requirement to preserve their renderer-neutral tree, Glyphon path,
projection package or paired HTML implementation. Only real existing domain
boundaries need translation. Private Bevy helper entities need not be exposed
as editable Sands.

Specialized content engines are low-priority end-of-v1 work. A leaf uses Bevy
first; an external/internal crate, custom Bevy plugin or pure WGPU work must
serve a named need and integrate with Bevy's lifecycle and presentation. Include
embedded dependency licenses and credits. This is not permission to embed a
browser or run untrusted native code in-process.

## Landed C4 structural catalog boundary

[The build rule](build.md) accounts for all 25 source roots, but [Dogfeeding](plans/part-a.md) qualifies only the reusable knowledge/work and access controls needed by the company acceptance case. Vocabulary, Role/Protein/property policies and views can be authored manually in the new native Sands; no company starter or legacy desktop surface substitutes for them. Other native roots remain in [native follow-through](plans/native-follow-through.md); Website, Document Viewer, Terminal, Freedoom and Logo LED each need a way to run without embedding a browser in Lince before they return, as [the build rule](build.md#no-embedded-browser) sets out. A catalog label is not a running workflow; unavailable references preserve data without executing.

`crates/interface/src/official_sands.rs` is the Rust-owned migration catalog.
It names the 25 current official roots and builds one validated package of 72
definitions. Ordinary structure is split into independently reusable Actions,
Record identities, properties, labeled fields and textareas, status lines,
toolbars, empty states, Record summaries, inspectors, table rows, Kanban
columns, Messages, Conversation threads and private draft/preset queues. The
roots are recursive compounds over those pieces and the 19 primitive Sands.

Documents, games, LEDs, Websites, terminals, graphs, Ontology and Karma use
explicit specialized leaf definitions. In the Bevy target, a specialized leaf
uses first-party Bevy facilities unless a specific need justifies an exception;
its surrounding controls, state and inspection remain ordinary Bevy Sands. No official workflow root is a specialized
monolith. All catalog records have Rust code-owned lineage.

F12 opens the native catalog. Tab/Shift-Tab, pointer and AccessKit select an
official root and show its legacy source, migration state, purpose, public
typed boundary and recursive definition tree. Enter/Space opens an available
runtime. The surface distinguishes landed Configuration, the seven native
Behavior roots, their live Protein and Action seams, and the 17 structure-only
roots. The generated exact package is
`target/interface-laboratory/sand-contract/official-sands.json`.

The catalog does not replace or disable the legacy Web Sands yet. Those remain
the behavioral reference below until each Protein read, Action, local state,
specialized renderer and honest empty/failure state has moved to its selected replacement. Native-path cleanup happens in Part A; browser-dependent implementations and needed support sources remain gated until their late-v1 replacements pass.

### Historical first native retained runtime

`crates/interface/src/retained_ui.rs` recursively projects exact definitions
and input exports to one renderer-neutral scene with stable semantic paths.
The paths, bounds, roles and labels are shared by WGPU geometry, Glyphon text,
pointer hit testing, keyboard focus and AccessKit. Definition composition,
rendering and accessibility therefore cannot silently acquire separate widget
trees.

The retained runtime covers edit controls, zoom controls, Record,
Conversation, Table, Todo and Kanban. It can toggle
edit mode, lock/unlock a group, emit a reusable-Castle save request, zoom,
recenter, emit the mounted Record identity, cycle property presentation, edit
a local Record draft and emit its send request. Generic actions wrap a generic Boolean trigger;
only the Record-specific action wraps the primitive that carries a Record and
emits `record-clicked`. Without a configured domain endpoint it shows a visible
representative Protein-shaped mount. The production desktop instead subscribes
to the running server with an ordinary Protein and replaces that mount with
live Record data through a bounded, reconnecting background adapter. Local
draft text survives incoming snapshots.

Conversation uses the same retained tree for live Conversation → Thread →
Message rows. Every Message materializes author, operator and explicit
`writing`, `finished` or `interrupted` lifecycle state. A writing Message may
grow and then finish or interrupt; terminal states cannot reopen. Message
drafts are ordinary private Records outside the shared Conversation replica
root. Their routing, pin state, declared timing and order survive restart.
Native edits coalesce for 250 ms and then cross distinct create/revise/delete
Actions; send first persists an unsaved draft, retains every draft until an
acknowledgement, consumes an unpinned draft after successful send and preserves
a pinned preset. New edits made while an older revision is in flight remain
dirty. With no running Fiote turn, deferred timing is visibly inert and the
control says that it sends now; the runtime exposes a turn-state seam for the
later Fiote attachment.

The transport now maintains three bounded subscriptions: general Records,
Conversation trees and author-visible message drafts. Incoming snapshots and
updates remain renderer-neutral domain values. Outgoing writes are ordinary
Actions over the existing WebSocket boundary; production diagnostics drain
their representative intents rather than mutating a person's data. Authenticated
multi-person desktop session establishment is still required before C5 can
claim the native client itself proves author isolation over a configured
authenticated transport.

The 2026-09-02 release parity and schema-10 joined Wayland/Vulkan report both
pass. The exact package hash is
`sha256:15d668d8add7ba5bbca6bc81f91c17d9c18c2d464c453a89273a24d17f69b3c5`.
The joined diagnostic exercises all three roots and verifies their state
changes before returning to the catalog; the interactive runtime remains
available for a person to operate rather than existing only in a test.

The 2026-09-05 production desktop Conversation checkpoint passed joined report
schema 12 on Wayland/Vulkan at 1920×1052. It retained the accepted 200 visible
Sands, 1,000 continuously eligible bodies, 10,000 resident nodes and two
embedded browser surfaces while measuring 13.047 ms frame p95, 16.384 ms p99, 7.791 ms CPU-frame
p95, 0.511 ms fixed-step p95 with zero backlog and 7.916 ms input-to-present-call
p95. The exact 72-definition package hash is
`sha256:c06be5a4c17ab85d590e96a87bea45b1b0a48544b37f2dde0e69a39522630347`;
the source fingerprint is
`013bc0510eeb888e6fa6b08de793b992487e5698acb42a03d3708d60ec1bd951`.
The final 2026-09-05 Conversation checkpoint passed schema 13 on the same
Wayland/Vulkan host. All three live Protein snapshots—Record, Conversation and
private drafts—arrived with zero refusal and no error. Durable draft creation,
revision, ordering, pinning and send intent were exercised without mutating
production data. At the same 200/1,000/10,000 workload and two accelerated
embedded browser surfaces it measured 9.897 ms frame p95, 11.460 ms p99, 4.721 ms CPU-frame p95,
0.535 ms fixed-step p95 with zero backlog and 3.936 ms input-to-present-call
p95. The report is
`target/interface-laboratory/c4-conversation-drafts/report.json`; the package
hash remains
`sha256:c06be5a4c17ab85d590e96a87bea45b1b0a48544b37f2dde0e69a39522630347`
and its source fingerprint is
`9301dd1860aca967c3af225dbcf1c959c72f6019bccf940bed0e8e77a22e572b`.

### First native Record collections

`RetainedPlacement` is the shared bridge from one stable Protein row to one
instance of a Rust-authored definition. It uses an encoded Record uid in the
semantic path, repeats definitions rather than cloning renderer markup, and
reapplies focus after reconciliation. Table repeats `official-table-row`,
while Todo and Kanban repeat the same `official-record-summary` cards used by
Record. Their toolbars, creator field, completion control, lane titles and
status lines are ordinary reusable Sands too.

Table paginates the current Record Protein and can create a blank Record. Todo
matches the established plain negative-quantity query, accepts a local title,
creates quantity `-1` Records and completes one by requesting quantity `0`.
Kanban currently uses the established Backlog `0`, Next `-1`, WIP `-2`, Review
`-3` and Done positive lanes; selecting a card emits `record-clicked`, while
moving left or right requests the destination quantity. No write mutates the
local Protein result optimistically or disappears before its Action receipt.

The 2026-09-05 schema-14 production report passed on Wayland/Vulkan at
1920×1052. Duplicate retained placement and expanded-node identities fail
closed, and Todo's Open selected control emits the same typed Record event as
its cards. It exercised three stable Record selections and two collection
write intents, then accepted all three live Protein snapshots without error.
At 200 visible Sands, 1,000 continuously eligible bodies, 10,000 resident nodes
and two accelerated embedded browser surfaces it measured 10.463 ms frame p95, 12.006 ms
p99, 5.369 ms CPU-frame p95, 0.559 ms fixed-step p95 with zero backlog and
4.535 ms input-to-present-call p95. The report is
`target/interface-laboratory/c4-record-collections/report.json`; its package
hash is
`sha256:80caab5ec2fe1a3637b7eeff0ce628b264162363e29d64994cabdd1c575142d7`
and source fingerprint is
`bac13fd4ab01078bc7f93221374d4a38c9a265d898bb1164190806af4c5321f2`.

This checkpoint does not claim all legacy collection behavior. Table inline
editing/deletion, user-authored Protein drivers, configured and saved Kanban
lane/concept rules, multi-selection, bulk operations and swimlanes remain C4
work. The visible per-lane capacity is bounded and reports overflow; later Box
viewport virtualization must replace that temporary capacity without changing
stable row identities.

### Record

The reusable Record body editor keeps canonical Markdown while making it easy
to insert and render blocks. Typing the Markdown remains equivalent to choosing
from the searchable slash palette: `#` through `#######` create headings,
`![](url)` embeds an external image, the image picker stores an allowed local
image under an opaque media name, and `- [ ]` creates an interactive checkbox.
`@slug` references another Record in bodies and thread messages.

The existing Web Record Editor uses `collab-editor.js` and the Loro text/transport boundary. Preserve its supported convergence, acknowledgements, reconnect, caret and presence behavior through a native binding in Part A; the newer retained Record draft surface alone is not collaborative-editing parity. This does not require running Loro's browser Wasm in the native host. The additional title-to-create Note and new scalar binding paths remain explicitly scheduled in [native follow-through](plans/interface.md#native-follow-through-and-cross-feature-surfaces). Locked descriptions accepted into the launch baseline stay opaque and outside text reconciliation, following [Secrets](../Secrets.md).

The Record sand presents that one canonical body in three modes: **Raw** is a
plain editable Markdown textarea, **Pretty** is a read-only rendering, and
**Pragmatic** is the default rendered editor. In Pragmatic mode the line under
the caret becomes source; fenced structures such as Mermaid become source as
a complete block. A line means source text ending at a real newline, never one
visual row produced by wrapping inside a narrow Sand. The active line or block
stays in source while the person thinks, selects text or uses input composition.
It renders after the caret leaves that block or the person explicitly chooses
another presentation. There is no five-second idle conversion. Preserve the
native caret, draft and scroll position across these transitions. The body itself
has no border; the border belongs to the complete Record surface. Its
property accordion is a divided horizontal rule, not a box: the down-triangle
half reveals every section and the up-triangle half hides them all. It normally
shows only filled sections and always opens a new Record with History closed.
Head, slug, quantity, the accordion control, and body remain visible in that
order. Rendered checkboxes use their own block row so their control cannot
collapse surrounding text into one inline run.

- [x] **Record** (formerly "record_info" — the sole markdown editor, viewer, and creator for a record, and the home for every other per-record concern) — the get view IS the edit view (head/slug/quantity/ body writable, Save writes only what changed, a dirty form is never clobbered by live updates); Zero (`deactivate`) and Delete (`delete-record`, permission-gated) are separate buttons; creation mode shows the same fields empty, Create + focuses the new record; carries the shared slash-block editor (headings/images/checkboxes/`@slug`, the same palette everywhere in a body); collapsible sections for **Work** (start/due dates, estimate, worklogs with play/pause, on the `work` record extension, offline-queued writes), **Assignees** (`assigned-to` assertions), **Relations** (every hop-1 binary assertion in either direction, predicate+object inputs, both autocompleted — a document/URL is an asserted relationship or inline media in the body, with no separate resource/attachment model), and **Threads** (a real multi-thread system — a tab per thread, search filters which tabs list without hiding messages; chat-style runs (2026-08-07) show the sender name — `user@organ` when the message's origin-organ name differs from the sender's, just `user` when they match (the common single-user-organ case) — only on the first message of an unbroken run from the same `created_by`, every message keeps its own bottom-right timestamp and edit/delete controls, and editing an existing message now uses the same shared slash-block editor as composing one; `@slug` in a post becomes a Record reference, delete controls per permission). Reusable — any sand drives it via a scoped `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel. Full real-time collaborative editing is blocked on the CRDT text relay in [Synchronization](../Lince.lingua).

#### Drawing in a body (future feature, moved here from Ontology 2026-08-16)

Wanted, not scheduled: draw on a canvas, scale the result down, and place it in
part of a Record body the way an image is placed — including replacing the body
art of a Record that arrived from a shipped bundle. Two things about it are
already settled and must survive the wait, because both were decided against a
constraint rather than a preference:

- **The body holds a REFERENCE to a content-addressed asset, never inline
  bytes.** Body text rides the op log to every peer, and inlining an image
  there undoes the O(live state) property the log work bought. Store the
  drawing in the media store addressed by its hash and reference it — which
  also deduplicates the same drawing across Records for free.
- **Raster, not SVG.** `media_assets` sniffs magic bytes and refuses SVG
  outright (`evil.svg` is a test case), because an SVG is a document that can
  carry script. PNG or WebP off the canvas. Vector strokes would need their own
  sanitised path — the same threat model as rendering a stranger's Facade — and
  that is separate work nobody has asked for yet.

### Kanban

The Kanban sand when ready will be able to provide teams the organization necessary to tackle projects together in a classic way. The data they CRUD in Kanban is accessible in other sands to fit greater workflows though.

- [x] Have a way to create a new Record.
- [x] Moving one card from one column to the other issues an update on the quantity of the Record.
- [x] Cards can be shown minimally or with a lot of information about them displayed.
- [x] The columns of the Kanban dictate what quantity the Records have underneath. The user doesnt have to know that column Done is for quantities 1 by default. But they have to know if they want to change it. I have a place to configure my columns, in that case i need to select one quantity for the the column, so Records with that quantity are shown in the respective column (would be cool to select a range, like from 1 to 2, 3 to 10). I must be able to sort them with drag and drop to say that column X is to be -1 and move it to -2. I must be able to click buttons to create new columns, give a name and type a quantity. Maybe have a tooltip to signal the reason behind using quantity.
  - [x] Have a way to CRUD column presets, like instead of Todo, WIP, Done its Backlog, Next, WIP, Finished. And i can apply one to this Kanban.
- [x] Having a small indicative that the connection with the backend is ok, can be used to signal that an update is taking place and when it is finished (maybe a cute little ball with different colors for the states - duds).
- [x] Be able to select one or more Records, to execute possible actions: move to another column, delete.
- [x] Currently, metadata of Records is only visible and editable in the Record sand, a reusable sand for editing in-depth info about Records. We must be able to see such metadata, even if we can only interact with it through Record sand. Either way, here are the tasks for metadata control someway:
  - [x] Date for the supposed start and end of the task.
  - [x] Time estimate, how much time do i think this is going to take, in hours and minutes (the data saved is in minutes).
  - [x] Play/Pause button to log time spent in the task. Play starts a work log, Pause ends one, time is added on Pause. Also we need to be able to full CRUD this so that if i spent some time before I can add it, if I inputted something wrong i can update the existing or delete it.
  - [x] Assign the task to someone, by their name or username.
  - [x] Be able to set the parent/children of this task.
  - [x] Being able to CRUD threads and messages as links of records (that belong to a record) that can have the same complexity of body content: text, images...
  - [x] The interaction with the body of Record must be able to have slash '/' commands to put add content in an easy way: typing /h3 will give you ### which is the end result that remains in the body (###). If we can make the body of a kanban have text, why not make it have the full editing and visualization that the 'Record' sand has for the body of the Record? We implemented the same checkbox clicking in the body of the record in kanban, why not put the body of the Record of the 'Record' sand?
    - [x] Changing the body of Records in Kanban cards by clicking on it to write in it or to check a box (slash commands only in Record sand? too hard to implement such feature twice? gotta be a way)
- [x] Filter and search cards through Protein by assignee, work date, assertion predicate, directional binary assertion such as `@parent`/`@child`, quantity, or text. Filters can be nested in AND/OR groups up to 10 levels. See [Ontology](../Lince.lingua).
- [x] Optionally group cards in swimlanes by assignee, parent, concept, or a Protein grouping key.
- [x] Order based on several important fields, from head of record, to quantity, @concept and links.

### Relation

Relation is a graph projection of **binary Record assertions**. It does not own
a separate relation or link model; the shared data semantics, CRUD operations,
hierarchy widening, and Protein behavior live in [Ontology](../Lince.lingua).

- [x] **Relations** — d3 force graph with physics sliders, golden-angle layout,
  zoom/pan/fit, directed arrows, and assertion-predicate labels. Shift+drag
  asserts a directed binary assertion optimistically; edge-click selects it;
  the header chip retracts it. Predicate inputs autocomplete from Concepts.
- [x] **Trail mode** — projects a selected predicate's forward assertion graph
  topologically, with a Done/Undo promotion cascade over shared status
  predicates (quantity or Concept buckets such as
  `@todo/@next/@wip/@done`). Delete retracts the selected assertion; Ctrl+Z
  undoes the local session action.
- [x] **Protein trails and Focus** — consumes a directed assertion-order item
  from Record Protein. The returned ordering supplies traversal and the
  earliest root; Focus advances through matching Record states.

### Calendar, Clock and simulation views — planned additions

These extend the catalog after the existing C4 migration; the historical
25-root count above does not claim they already exist. The native temporal
leaf, its surrounding Castle, source mapping, recurrence, spiral and simulation
contracts are in [Time](time.md). Their human surfaces and domain dependencies
are scheduled in [the Interface plan](plans/interface.md#calendar-clock-and-simulation).

### Table

Table is a simple tabular projection of Protein results. It is a useful
baseline renderer and composition primitive, but it does not imply that Ledger
storage is one generic table or make the Table Sand the base class of other
Sands.
