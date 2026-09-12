# Workflows to carry into native Sands

This is a description of wanted behavior from the earlier interface. It does not mark native implementations complete. [Lince.lingua](../Lince.lingua) holds progress; [the interface draft](plans/interface.md#more-workflows) gathers proposed wording.

## Records and notes

Record is the shared place to create, read and edit a Record. Show title, slug, quantity and body, then collapsible work dates, estimates, time logs, assignees, Assertions and threads. Save changed fields, preserve dirty edits during refresh, and keep Zero separate from permission-checked Delete. History begins closed.

The body editor stores Markdown. Raw shows editable source; Pretty is read-only rendering; Pragmatic exposes the current source line or complete fenced block while rendering the rest. A wrapped display row is not a source line. Keep the active block stable until the caret leaves or the person changes mode. Slash commands, images, checkboxes and Record references work wherever this editor is embedded. Rendering never writes.

The body shares the Record's outer border. A divided horizontal rule provides reveal-all and hide-all controls for properties; normally only filled sections are shown. Checkboxes keep their own block row. Work logs support start, pause and correction, and Assertion inputs autocomplete Concepts and Records.

Use the same collaborative binding in Record, Kanban, Table and Conversation. Preserve caret, selection, acknowledgements, reconnect and peer presence. Locked descriptions remain opaque until an authorized local unlock; deleted content cannot keep receiving edits through an old view. Quantity keeps its domain rules, and slug changes need their uniqueness rule.

Text editing must preserve concurrent contributions instead of saving a whole body over another edit. Repeated updates do not create a sync loop, and a late acknowledgement must not erase newer typing. Extend scalar field bindings through their ordinary Actions; quantities remain Fact-backed, and a slug is either excluded from free binding or renamed through a concurrency-safe uniqueness check. A new text document starts from the Record's existing content.

A Note starts as a local draft until its title creates a Record, or opens an existing Record through a picker. An embedded editor cannot create or switch its source. Recent displaced edits should be visible with the previous value and their retention limit. Smaller collaborative snapshots remain backend work.

Only a change flagged as displacing local work should be presented as a lost edit; ordinary incoming sync is not one. The earlier backend note said text compaction still exported full Loro snapshots. Switching to shallow snapshots must preserve readable text and imports from peers with different editing histories; it is an optimization to verify against current code, not an editor prerequisite.

## Collections and graphs

Table shows configured Protein results with inline editing and selection. Todo shows a chosen work view. Kanban uses configurable quantity or Concept lanes, presets, ordering, filters and optional swimlanes. Cards can show metadata and the shared body editor. Moving a card requests the mapped data change; a refused move stays understandable. Bulk operations report affected rows separately.

Relations shows directed Assertions and their predicates. People can select, assert, retract and follow them, with Trail ordering and local undo. Graph layout and camera changes preserve the underlying relationships. Ontology adds exploration of Concepts through the same Records.

## Other workflows

Conversation supplies threads, attribution, live message states, private drafts and acknowledged sends. Organ and Access Control provide identity, Roles, permissions and sharing controls. Transfer shows its participants, quantities, lifecycle and delivery. Karma exposes its actual Program, Frequency, candidate and grant operations, including cycle explanations.

Threads have separate tabs and search filters the thread list without hiding messages inside an open thread. Consecutive messages from the same author can share a name label, while each keeps its timestamp and permitted edit/delete controls. Show the originating Organ when needed to distinguish authors. Writing messages remain read-only until finished; interrupted messages are explicitly marked.

Karma controls use the domain's types and validation rather than restating every rule shape in another frontend mapping. The old Web autocomplete, caret and selection tests were removed alongside that mapping's tests; this historical gap does not waive focused tests for their native replacements.

Instinct lets a person read a tutorial and deliberately create its Records. Archive and Sand Publisher inspect, validate and export or publish a chosen scope. The AI composition surface shows a proposal before applying it. Configuration, navigation, Information and update controls use ordinary Sands too.

Calendar and Clock are described in [Time](time.md). Document reading, terminal, Website, Freedoom and logo animation need their respective native or external-opening designs. An unavailable definition stays visibly unavailable when restored.

Drawing inside a Record body remains an idea: create a PNG or WebP asset and put a content-addressed reference in Markdown. Keep image bytes outside the Record text. Editable vector drawings would need separate validation.
