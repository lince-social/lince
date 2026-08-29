# Existing official Sands

Purpose: Preserve user-visible behavior of current first-party Sands while their implementation is rebuilt on the new composition system.

Owner source: no dedicated Sands Record currently exists;
[Interface](../Interface.lingua) governs shared interface decisions.

Status: Migration input; checked historical capabilities are not proof of native C4 migration.

Read when: migrating or regression-testing Record, Kanban, Relation, Table, or another official Sand.

[Corpus map](README.md) · [Current context](current.md) · [Sand plan](plans/sands.md)

---

### Record

The reusable Record body editor keeps canonical Markdown while making it easy
to insert and render blocks. Typing the Markdown remains equivalent to choosing
from the searchable slash palette: `#` through `#######` create headings,
`![](url)` embeds an external image, the image picker stores an allowed local
image under an opaque media name, and `- [ ]` creates an interactive checkbox.
`@slug` references another Record in bodies and thread messages.

The Record sand presents that one canonical body in three modes: **Raw** is a
plain editable Markdown textarea, **Pretty** is a read-only rendering, and
**Pragmatic** is the default rendered editor. In Pragmatic mode the line under
the caret becomes source; fenced structures such as Mermaid become source as
a complete block. A line means source text ending at a real newline, never one
visual row produced by wrapping inside a narrow Sand. After five seconds
without caret activity it renders again and uses the browser's native caret in
that one rendered surface; moving or typing turns the line or block reached by
the caret back into borderless raw source. There is no synthetic caret or
hidden input to disturb the Sand's layout or scroll position. The body itself
has no border; the border belongs to the complete Record surface. Its
property accordion is a divided horizontal rule, not a box: the down-triangle
half reveals every section and the up-triangle half hides them all. It normally
shows only filled sections and always opens a new Record with History closed.
Head, slug, quantity, the accordion control, and body remain visible in that
order. Rendered checkboxes use their own block row so their control cannot
collapse surrounding text into one inline run.

- [x] **Record** (formerly "record_info" — the sole markdown editor, viewer, and creator for a record, and the home for every other per-record concern) — the get view IS the edit view (head/slug/quantity/ body writable, Save writes only what changed, a dirty form is never clobbered by live updates); Zero (`deactivate`) and Delete (`delete-record`, permission-gated) are separate buttons; creation mode shows the same fields empty, Create + focuses the new record; carries the shared slash-block editor (headings/images/checkboxes/`@slug`, the same palette everywhere in a body); collapsible sections for **Work** (start/due dates, estimate, worklogs with play/pause, on the `work` record extension, offline-queued writes), **Assignees** (`assigned-to` assertions), **Relations** (every hop-1 binary assertion in either direction, predicate+object inputs, both autocompleted — a document/URL is an asserted relationship or inline media in the body, with no separate resource/attachment model), and **Threads** (a real multi-thread system — a tab per thread, search filters which tabs list without hiding messages; chat-style runs (2026-08-07) show the sender name — `user@organ` when the message's origin-organ name differs from the sender's, just `user` when they match (the common single-user-organ case) — only on the first message of an unbroken run from the same `created_by`, every message keeps its own bottom-right timestamp and edit/delete controls, and editing an existing message now uses the same shared slash-block editor as composing one; `@slug` in a post becomes a Record reference, delete controls per permission). Reusable — any sand drives it via a scoped `recordClicked`/`recordCreate`; no sand keeps a private record sidepanel. Full real-time collaborative editing is blocked on the CRDT text relay in [Synchronization](../Ontology.lingua#sync-the-op-log).

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
- [x] Filter and search cards through Protein by assignee, work date, assertion predicate, directional binary assertion such as `@parent`/`@child`, quantity, or text. Filters can be nested in AND/OR groups up to 10 levels. See [Ontology](../Ontology.lingua).
- [x] Optionally group cards in swimlanes by assignee, parent, concept, or a Protein grouping key.
- [x] Order based on several important fields, from head of record, to quantity, @concept and links.

### Relation

Relation is a graph projection of **binary Record assertions**. It does not own
a separate relation or link model; the shared data semantics, CRUD operations,
hierarchy widening, and Protein behavior live in [Ontology](../Ontology.lingua).

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
### Table

Table is a simple tabular projection of Protein results. It is a useful
baseline renderer and composition primitive, but it does not imply that Ledger
storage is one generic table or make the Table Sand the base class of other
Sands.
