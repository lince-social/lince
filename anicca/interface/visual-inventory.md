# Visual inventory and token migration boundary

Purpose: Record every first-party visual source that must converge on the
renderer-neutral Lynx style contract and distinguish governed style from
content and runtime geometry.

Owner source: no dedicated Customization Record currently exists;
[Interface in Lince](../Lince.lingua) governs the shared interface direction.

Status: source inventory and canonical replacement contract landed; legacy
sources remain migration input until the official-Sand rebuild.

Read when: adding a style token, migrating a first-party surface, or reviewing
a hardcoded visual value.

[Customization](customization.md) · [Customization plan](plans/customization.md)
· [Legacy Web UI](legacy-web-ui.md)

---

## Governing distinction

The style contract governs values chosen to make Lince chrome and reusable
Sands visually coherent: palette roles, surfaces, ink, intent, state, space,
size, borders, radii, typography, icons, elevation, opacity, density, stacking,
truth lines and direct-manipulation motion. The canonical spelling is always
`--lynx-*`; a native projection receives the same typed values without
pretending that CSS is its source of truth.

The contract does not absorb runtime coordinates, transforms, measured
content dimensions, camera data, simulation constants, user drawings,
wallpapers, charts, document colors, shaders, games, terminal palettes or
specialized visualizations. Those are runtime or user content. Their ordinary
shell, focus, controls and accessibility presentation still consume Lynx
roles.

Every literal encountered during migration is assigned one of these classes:

| Class | Meaning | Destination |
| --- | --- | --- |
| Palette primitive | A Lynx theme source color or neutral step | Theme manifest only |
| Semantic role | A reusable meaning such as surface, ink, focus, need or danger | Canonical typed token |
| Component alias | A value genuinely local to one reusable primitive | Declared `--lynx-*` extension or eliminated |
| User-owned content | Deliberate expression or domain data | Preserved as data |
| Runtime layout | Coordinate, measurement, transform, viewport or simulation value | Preserved in runtime state |
| Specialized rendering | Graph, game, document, terminal or shader-owned visual | Preserved behind its Sand boundary |
| Accidental hardcoding | First-party chrome that should have used a role | Replaced during migration |

Compatibility aliases are not a destination. No one uses Lince yet, so a
legacy spelling is deleted when its callers move rather than being retained
beside the canonical role.

## First-party source inventory

Vendored libraries and their themes are excluded from design-system migration
but remain subject to their Sand's LICENSE and credit obligations. Generated
laboratory reports and build output are also excluded.

### Base Web and board host

- `crates/web/static/styles.css` is the largest legacy base surface. It mixes
  palette primitives, semantic aliases, component styling, board geometry and
  interaction state. Its aliases such as `canvas`, `surface`, `ink`,
  `primary-background`, `raised-background`, `primary-ink`, `secondary-ink`,
  `border`, `accent` and their Portuguese palette names are migration input,
  not additions to the contract.
- `crates/web/static/ai-builder.css` contains a smaller parallel surface and
  component vocabulary. Its semantic intent must be mapped to the same roles;
  its dimensions remain local only when the composition actually owns them.
- `crates/web/static/presentation/board/lynx-ui.css` and the style writes in
  `crates/web/static/presentation/board/main.js` are the legacy board adapter.
  Board coordinates, world size, transforms, resize measurements and grid
  parameters are runtime layout. Colors, borders, type, radii, shadows and
  ordinary state presentation are governed style.

### Legacy LynxUI Gallery

- `crates/web/src/sand/lynx_ui/demo.css`, `demo.js`, `index.html` and
  `styles/catppuccin-macchiato.css` are visual and behavior references only.
  Their useful semantic decisions migrate into tokens or primitive Sands.
  `LynxUI`, its global API and its parallel aliases do not survive as a second
  component system.
- The Catppuccin file is a first-party theme adapter around an external
  palette. It can inform a future manifest but does not expand the required
  Lynx contract or become a runtime dependency.

### Official Sands with embedded presentation

The following first-party Sand sources contain CSS, markup presentation, or
JavaScript style writes and must be classified while each Sand is rebuilt:

- `archive/archive.html`, `configuration/configuration.html`,
  `conversation/conversation.html`, `instinct/instinct.html`,
  `kanban/kanban.html`, `ontology/ontology.html`, `organ/organ.html`,
  `permissions/permissions.html`, `record/record.html`,
  `record_editor/record_editor.html`, `relations/relations.html`,
  `table/table.html` and `todo/todo.html`.
- `communication/communication.html` and the Transfer surface in
  `transfer/styles.css` plus its first-party modules.
- `document_viewer/styles.css` and its first-party modules,
  `karma/styles.css` and its first-party modules, `terminal/styles.css` and
  its first-party modules, and `lince_website/style.css`.

For each of these, ordinary buttons, fields, menus, cards, typography, focus,
empty/error/loading states and Sand chrome are governed. A document's own
page styling, graph marks, Karma canvas geometry, terminal cell palette,
Freedoom pixels, media frames and Website-authored page presentation are not.
This boundary prevents the token system from becoming an untyped bag of every
number or color the product can display.

### Native laboratory literals

The joined laboratory previously used local node, border, panel, text and
clear colors. Its active native nodes, border, retained text and compositor
background now resolve through the typed contract. Diagnostic-only CEF page
content remains a fixture, while its surrounding Installed projection consumes
the same CSS declaration set. Workload geometry, semantic fixture colors that
represent test cohorts, timestamps and instrumentation graphs remain test
data.

## Canonical replacement contract

`crates/interface/src/style.rs` is the current executable authority.
Contract version 1 defines 92 standard tokens across palette, surface, ink,
intent, state, spacing, size, border, radius, typography, icon, elevation,
opacity, density, stacking, motion and truth families. Values are typed as
color, non-negative pixel length, signed pixel offset, scalar, integer, font
family, font weight, line style, duration or numeric-figure policy.

C3 added `--lynx-margin-sand` beside the existing gap and padding roles so
margin can inherit and report provenance without becoming arbitrary layout
CSS. The generated token reference is derived from this executable table and
includes both Dark and Light default values for every role.

Signed offsets are deliberately separate from lengths: a shadow may travel
left or up, while padding, radius, border width, blur and text size cannot be
negative. Unknown tokens, mismatched types, non-finite values and unsafe
ranges fail closed.

The only cascade is:

1. default contract;
2. primitive projection rules;
3. Sand-definition defaults;
4. active theme and mode;
5. workspace override;
6. group or Castle override;
7. Sand-instance override.

Omission means inherit. Each resolved value retains its scope and source
label, so Configuration and Why-is-it-here can explain the effective result.
An isolated Installed HTML root receives the final typed set as safe CSS
declarations. A Website receives none because styling it would cross the same
authority boundary as giving it a Lince bridge.

The bundled Lynx manifest provides complete Dark and Light modes. Partial
themes inherit missing roles. Manifests may declare typed local extension
slots and content-addressed local raster, inert SVG or font assets. Unknown
contract versions, remote or parent-relative assets, undeclared extension
values and invalid hashes fail closed.

## Migration rule

[Part A](plans/part-a.md) applies this rule to the enabled native path. Browser-backed leaves and the support code they still need are retained behind the CEF boundary for [the final v1 lane](plans/cef.md). Their presence in this inventory is not a demand to migrate or delete them before native C5. Exclusions must name the retained source and disabled dependency; they cannot excuse copied controls in an active native workflow.

The inventory is complete as a source boundary, not as a claim that the old
Web UI has already migrated. During the official-Sand rebuild, each listed
source must either map a literal to the canonical contract, classify it as
content/runtime/specialized rendering, or delete it. The completion check then
rejects newly introduced governed literals and legacy aliases outside the
canonical definitions. Until that rebuild lands, legacy files may still
contain the values recorded here and must not be presented as examples for new
work.
