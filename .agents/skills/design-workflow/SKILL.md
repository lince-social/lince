---
name: design-workflow
description: Explore or simplify a Lince UI workflow through focused design questions, shared-token clickable HTML alternatives, and a recorded choice before Rust implementation. Use for crowded interfaces, navigation, visual treatments, interaction, density, copy, or accessibility studies. Ordinary feature implementation does not require a design study.
---

# Design workflow

Make one improvement dimension reviewable at a time. Lince's design decision is the most minimal useful default view: show the primary outcome and the few actions needed now, group related capabilities into clearly named destinations, and reveal occasional controls in context. Do not bombard the user with the feature inventory. Preserve capabilities through better composition and navigation, never through silent removal.

## Choose one focus

Infer the focus from the request. If ambiguous, offer a recommendation and ask which single question this run should answer:

- `structure`: grouping, navigation, entry points, sections, and disclosure.
- `style`: visual treatment, typography, spacing, borders, and token choices.
- `interaction`: steps, transitions, feedback, keyboard/touch behavior, and recovery.
- `density`: what is immediately visible and how the design fits its available space.
- `copy`: labels, instructions, and information hierarchy through language.
- `accessibility`: focus, reading order, contrast, target sizes, and equivalent access.

These are starting points. A human can request another narrowly named focus, such as `motion`; keep the same single-question and fixed-dimensions contract.

Lock the other dimensions to the existing UI or a previously chosen study. If a requested change crosses dimensions, explain the dependency and propose the next focused run. Do not silently turn a structure study into a visual redesign or backend rewrite. A diagnosis-only request can end with findings without manufacturing prototypes.

## Ground the conversation

First locate an existing Rust interface for the workflow, including its host composition, nested panels, contextual actions, and state-dependent controls. When it exists, use it as the mandatory capability and behavior baseline for HTML prototypes; an illustrative scaffold or remembered screenshot is insufficient. Inspect frontend states, shared models, actions, and backend behavior. Record source locations and distinguish duplicate entry points from distinct capabilities. When no interface exists, state that explicitly and inventory models and requested behavior before proposing a minimal view. Explain discoverable facts rather than asking the human to inventory their own code. Check existing shared controls and related workflows for reusable behavior without invoking an unrelated workflow automatically.

Summarize the current capabilities and distinguish working behavior, incomplete behavior, and assumptions. Ask a small round of consequential questions with suggested answers: the main user outcome, the few frequent tasks, occasional tasks, and which tasks belong together. Ask only what the code cannot tell you. Three or four main tasks can be a useful starting hypothesis, never a fixed quota.

For simplification, diagnose the source: too many competing goals, weak grouping, poor ordering, repeated controls, excessive explanation, or missing disclosure. Tabs separate destinations; sections group related work; disclosure hides optional detail; sequential steps support tasks with dependencies. Do not use pagination or an undifferentiated Advanced section as the default cure. Keep frequent tasks visible, preserve discoverability of occasional tasks, and keep consequential choices understandable where they occur.

## Make a focused study

Read [the prototype workflow](references/prototyping.md) when generating or reviewing artifacts. Use `cargo xtask design new <slug> --focus <focus>` to start a study, or continue an existing study without overwriting its prior alternatives.

Write the brief: focus question, audience, primary tasks, source-grounded capability inventory, fixed dimensions, relevant desktop/mobile/Sand contexts, and acceptance scenarios. For each retained capability, record its reachable placement in every alternative, including uncommon administration, recovery, and permission states. Consolidating duplicate controls is allowed; dropping their underlying behavior is not. Preserve capabilities by default; propose removal separately with a reason and obtain the human's decision before omitting it. An HTML mock must not imply that missing backend behavior already works.

Offer three meaningfully different alternatives by default, fewer for a narrow question or more when requested. Hold fixture data, viewport, theme, and dimensions outside the focus constant. For structure, compare organization rather than just colors. For style, retain navigation and task placement. Explain each alternative's hypothesis and tradeoff in plain language.

Implement clickable local mock interactions, including relevant empty, populated, pending, error, and recovery states. Use Lince tokens and bundled fonts, semantic HTML, visible focus, keyboard operation, and narrow touch layouts. Do not connect the study to real accounts, keys, devices, or backend mutations. Do not modify the unplugged web crate. Avoid embedded dependencies; if one is needed, include its license and available credits.

## Review and stop at the decision

Run `cargo xtask design check <slug>` and walk the acceptance scenarios at desktop and mobile widths, including constrained Sand sizes when relevant. The checker validates artifacts and declared capability coverage; it cannot prove usability or actual feature reachability. Use browser inspection when available and report missing visual validation honestly.

Show the alternatives, explain their differences within this run's focus, recommend one with reasons, and ask the human to choose or combine specific parts. Give the exact launch command, local viewer URL, and how to compare alternatives and viewports; say whether a preview server is already running. Do not invent a choice if no answer arrives. Record the actual choice in `decision.md`, including rejected tradeoffs, remaining questions, and the next focused run if needed. For unresolved studies, leave the choice explicitly pending.

A default run ends with prototypes and a recorded decision. Porting is a separate explicitly requested run: inspect the chosen study, reuse Rust controls/models, implement the selected behavior in the interface and relevant hosts, preserve backend semantics, and verify native desktop/mobile behavior. HTML is a design artifact, not production UI or automatically translatable Rust. Use `cargo xtask design native <component> --theme <theme-json> --watch` for native validation; theme files reload live, Rust edits compile and restart.
