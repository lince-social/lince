---
name: design-workflow
description: Refine Lince Castles through native Bevy alternatives, live composition edits, measured review, and saving a chosen component or default. Use for UI structure, interaction, density, visual treatment, copy, or accessibility work. Ordinary feature implementation does not require a design study.
---

# Design workflow

Design in Lince using the actual Rust controls and behavior. Start from a default Castle, change its composition in the running native interface, and let the human compare and refine alternatives. The reviewed composition is the implementation to save or promote; there is no HTML/CSS/JavaScript prototype or web-to-Rust porting stage.

Keep the most minimal useful opening view: show the primary outcome, group related capabilities into named destinations, and reveal occasional controls in context. Preserve capabilities and discoverability when changing their placement.

## Ground the change

Locate the Castle's factory, hosts, component tree, layout systems, action targets, bindings, conditional states, and persisted presentation settings. Inspect existing native editing capabilities before proposing another editor. Distinguish live runtime changes from Rust source changes that require compilation and restart.

Infer the main design question from the request: structure, style, interaction, density, copy, accessibility, or another named focus. Keep comparisons focused while honoring requests that span dimensions. Ask only consequential questions the code and conversation cannot answer; preserve choices already supplied.

Start with the existing default and record a baseline of its composition and measured geometry. Use the same fixture data, theme, font, viewport, scale, and disclosure state when comparing alternatives. Use isolated native fixtures for experiments involving saves or destructive actions unless the human requested edits to a particular live instance.

Read [the live composition workflow](references/prototyping.md) for runtime editing, measurements, persistence, and default promotion.

## Iterate inside Lince

Offer a few meaningfully different native compositions when alternatives help the decision. Keep each version named and recoverable; compare them side by side or switch the selected Castle between saved versions. A direct requested adjustment can proceed on one version without manufacturing alternatives.

Use supported native controls or an available local editing interface to apply changes to the running Castle. Move and resize semantic parts, reorder sections, adjust flex/wrap/alignment, and change scoped tokens without replacing working editors or reconstructing their behavior. Preserve typed drafts, focus, caret, selection, scroll position, and pending saves.

Apply reversible edits as transactions with undo and reset to the baseline. Keep a structural diff and native screenshots for changes that need review. Exercise the real controls and Laboratory scenarios; changing a Node's appearance alone does not establish usable interactions.

When the requested edit exceeds current capabilities, identify the missing native capability and build the smallest extension needed within the authorized task. For an assessment or skill-only request, explain the gap and the next implementation step. Do not substitute a browser mock or describe compile-and-restart as live composition editing.

## Review, save, and promote

Show the actual native alternatives, explain their tradeoffs, and recommend one when useful. Let the human adjust a version directly, choose a version, or combine specific parts. Record their actual choice; leave it pending when no answer arrives. Keep notes concise and scale artifacts to the task. A substantial study can keep its brief, composition versions, fixtures, measurements, captures, and decision under `design/studies/<slug>` using the native mechanisms actually available.

Separate three destinations: a live instance override, a reusable custom component, and the Castle's default. Editing an instance does not implicitly replace its default. When the human asks to save or set a default, use that authorization and promote the exact reviewed composition rather than manually recreating its appearance in a second implementation.

Validate save/reload and factory reconstruction against the reviewed composition, bindings, and measurements. Check relevant host widths, minimal/expanded states, nested panels, keyboard operation, and failure/recovery. Run Laboratory tests and cargo check appropriate to the change. Report what was verified, the launch or editing mechanism actually used, and remaining limitations; do not claim native touch or accessibility validation from desktop screenshots alone.
