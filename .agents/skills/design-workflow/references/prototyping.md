# Prototype workflow

## Commands

- `cargo xtask design new <slug> --focus structure` creates a study under `design/studies/<slug>`.
- `cargo xtask design serve <slug> [--port 6180]` serves a local comparison page with live refresh, theme selection, and desktop/mobile/Sand viewports.
- `cargo xtask design check <slug>` validates the manifest, local alternative files, and declared capability placements.
- `cargo xtask design tokens [--theme path.json] [--scheme Dark] [--kind Square]` prints a full JSON theme document and CSS variables resolved for the selected kind. Use `--format json` or `--format css` for a single format. A supplied theme file determines its own scheme.
- `cargo xtask design native <component> [--theme path.json] [--watch] [--mobile]` runs the real desktop component in an isolated preview. `--mobile` checks a narrow desktop window; it does not emulate Android behavior. Components are `square`, `text`, `editable-text`, `organ`, `time-castle`, `todo`, `configuration`, `access-control`, `sync`, `operation`, and `ontology`.

Start with the HTML loop. Native preview is for validating the chosen direction against actual Rust rendering and controls. It has no connected Cell and does not demonstrate real network, storage, camera, or backend actions. Restart clears temporary fixture state. Full app behavior and Android remain separate acceptance checks.

## Study files

`study.json` holds the title, one focus, retained features (`id` and `label`), alternatives (`id`, `title`, `file`, `hypothesis`, and `placements` mapping each feature ID to a clear destination), and scenario descriptions. Paths are relative to the study. The starter is illustrative scaffolding, not a generated redesign of the requested component. Replace its example tasks, fixtures, and markup with the actual workflow.

`brief.md` records intent and fixed dimensions. `decision.md` starts pending; record only an actual human choice. `fixtures.json` supplies synthetic data shared across alternatives. Optional `theme.json` uses the existing `ThemeDocument` format, including all tokens; the viewer exposes it as the Study theme. This supports preview overrides without changing global theme presets.

When Rust UI exists, record its host, nested panels, contextual controls and conditional states in a source-grounded baseline alongside the study. Map each distinct capability to a reachable page or control in every alternative. Audit loop-generated controls and alternate branches as well as literal action names; a source search or the manifest checker alone cannot establish parity. Keep a minimal opening view by relocating occasional capabilities, with explicit destinations for administration and recovery. Mark deep interaction sketches honestly rather than claiming a complete native recreation.

Keep reviewed alternatives available. For another focus, make another study referencing the preceding decision in its brief. Do not overwrite a chosen structure while exploring style.

## HTML contract

Each alternative links `/assets/prototype.css` and `/assets/prototype.js`. Styles use generated semantic variables such as `--Surface`, `--Ink`, `--Accent`, `--Spacing`, `--Padding`, `--FontSize`, and `--ControlRoundness`. Numeric variables use pixels except percentage-valued `CanvasPattern` and `ControlsCornerTransparency`; dimensionless companion values use `--Token-raw`. Colors preserve alpha. The viewer supplies `/tokens.css?scheme=Dark&kind=Square`, resolved by the same Rust theme code as native UI. Existing kind/global inheritance is retained; individual Sand and parent overrides require an explicitly supplied prototype theme rather than reading live workspace data.

Use the shared runtime's `window.designReady` promise to read `fixtures.json` and initialize controls. A `design:theme` event signals updated theme styles. Native and browser font rendering may differ; compare hierarchy, geometry, interaction, and effective tokens rather than requiring identical pixels.

The runtime provides keyboard tabs through `[role=tablist]` / `[role=tab]` / `[role=tabpanel]`, remembers the selected tab and named form field values across live refresh, and provides `window.designFixtures`. Give forms stable unique IDs to isolate their drafts. Native HTML details elements support optional sections. Domain interactions belong in each alternative's own small script and stay synthetic. Never put real secrets in prototype fixtures or session storage.

The viewer chooses fixtures via the prototype's Scenario control; every alternative should use the same scenario names and data. Add domain-specific states to `fixtures.json` and implement them in each prototype. Scenarios in the manifest describe observable tasks for manual/browser review, not automatically proven behavior.

## Validation and handoff

Walk the main task, each occasional capability, the empty state, pending action, failure/recovery, keyboard-only operation, narrow layout, long labels, and large lists as applicable. Compare alternatives with the same theme and viewport. Keep control labels readable; minimalism does not require hiding every label or putting every task behind another click.

A handoff identifies the chosen alternative and focus, feature destinations, relevant states, token changes, shared controls to reuse, native constraints, and acceptance scenarios. Do not treat a successful manifest check as browser or native validation.
