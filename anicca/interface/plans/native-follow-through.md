# Native workflows after Dogfeeding

These migrations remain v1 work but do not block [Part A — Dogfeeding](part-a.md). The company milestone uses native task/project controls, threads/messages and administration; it does not require every native catalog root. This file is the task home for the migrations removed from the earlier Part A checklist. No task here has been implemented or cancelled by moving it.

The preserved A03 identifiers allow older graph entries and references to find their requirement. They are not active Dogfeeding nodes. Keep native controls on the same shared editor, Protein and Action contracts; a new backend operation ships with its UI. Browser-dependent portions stay unavailable until each has a way to run without embedding a browser in Lince; see [the build rule](../build.md#no-embedded-browser).

All new native work uses Bevy directly. Reuse Bevy UI/text/picking and the
Lince components already built for Dogfeeding, not an adapter around the old
retained renderer. Specialized document, terminal, video and game engines
remain low-priority end-of-v1 work; their absence does not block the native
controls and domain integrations in this checklist.

- [ ] **N01** Complete the full native Ontology exploration surface beyond Dogfeeding's project/status/assertion controls, preserving Concept identity, authorized traversal and edits through the same Record model. Reuse the minimal controls already delivered instead of implementing them again.

- [ ] **A03.2** Migrate Relations with directed edges, predicate labels, selection, assertion/retraction, Trail ordering, focus, local undo and explicit layout controls. Use one bounded graph leaf under ordinary Sands; graph camera or layout changes never mutate the underlying assertions. Start
with Bevy curves, retained gizmos and meshes for edges and marks; use a custom
Bevy layout system. Add a dependency only for a demonstrated missing capability.
- [ ] **A03.4** Migrate the current Transfer inspection and lifecycle controls with exact participants, quantities, direction, permission and delivery state. Run success/refusal proofs with isolated local parties, never real external transactions; general Transfer simulation is later work.
- [ ] **A03.5** Migrate the current Karma Rule/Frequency builder and causal inspection. Derive variant shapes and validation from Rust's domain contract instead of re-declaring the kernel in another frontend mapping. Show cycle/proof refusals beside the offending rule or edge using the domain's explanation. Label old Recurrence versus Program-backed schedules accurately and distinguish evaluation/candidates from execution capability; graph navigation never runs a Rule, and a configured Condition never claims an effect was delivered without a real acknowledged domain operation. The additional Program authoring surface has its own explicit later-v1 task.
- [ ] **A03.6** Migrate Communication's existing contact/thread surfaces and their real delivery/refusal states. Reuse Conversation and distinguish messages, private drafts and ephemeral activity. The future media/call stack and agent session-control backlog do not become prerequisites for the existing root.
- [ ] **A03.7** Migrate Instinct's selection, reading and deliberate materialization of tutorial Records. Preserve the editable `First Steps.linguai` draft without ingesting it or changing owner `.lingua` files, and expose an honest unavailable tutorial state where the native tutorial is not yet true.
- [ ] **A03.13** Migrate Archive's native selection, inspection and export behavior through validated domain/package operations. Embedded HTML previews remain disabled; reviving them needs a way to render a preview without embedding a browser in Lince. Exports use explicit destinations and scope; unknown inputs fail visibly and no legacy copied-HTML state becomes the new Box document format.
- [ ] **A03.14** Migrate Sand Publisher's native inspect/validate/publish workflow with exact hashes, declared capabilities and package notices; validate package metadata without running its HTML or JavaScript. Proof publishes only into an isolated local fixture; federation and the broader post-Box installation administration remain separate work.
- [ ] **A03.15** Migrate the existing AI builder shell as an inspectable composition-proposal surface over the existing authorized seam. Make unavailable providers and unapplied proposals honest. Do not add a Fiote harness, silently execute generated code or use this root to start the later Box operation API.

- [ ] **N02** Complete additional Conversation preset/agent-timing controls when their real domain capabilities are available. Reuse Dogfeeding's task/thread editor, author-private drafts, ordering and acknowledged sends; do not introduce another message or draft model. Fiote's harness and terminal-bearing surface keep their separate features; the terminal pane waits for the end-of-v1 terminal emulation/PTY and Bevy presentation work, since no embedded browser will supply one.

Program-backed Karma authoring, Frequency/candidate/grant controls and domain execution retain their separate [native follow-through](interface.md#native-follow-through-and-cross-feature-surfaces) and Karma tasks. This checklist does not quietly select the old Recurrence path as the final v1 authoring design. Calls and media retain Communication's own plan; none is needed to discuss a company task.
