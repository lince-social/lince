Slash commands, to be able to put several types of blocks in the body of Records as cards of kanban, or even as any Markdown body (reusable). If you type the underlying character/s you will end up seeing the same visual block, but you can enter slash mode to select from a list by name or start typing characters to filter them.

The list, with the characters and their blocks goes as following:

- [ ] '#': Headers 1-7
- [ ] '![](url)': Images either url or bucket if no prefix, maybe we can choose if external source of bucket, maybe we can grab from pc and then put in bucket.
- [ ] '- [ ]': Checkbox

- [ ] Being able to reference other Tasks inside comments.

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
