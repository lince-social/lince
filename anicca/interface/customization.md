# Customization and visual system

Purpose: Define tokens, themes, visual character, scopes, and the design-system build order.

Owner source: no dedicated Customization Record currently exists;
[Interface in Lince](../Lince.lingua) governs shared interface decisions.

Preserved source metadata: `@customization`, order 0, `#chapter`,
`#instinct`, `#part-of @interface`, `#done`, uid
`r_4Z5VS9MJNRDHZGXW4KMGRNXMHK`.

Status: typed customization, semantic primitive-Sand, recursive-composition
and C3 Configuration/external-authoring kernels landed; C4 Rust catalog and
first retained-runtime slice landed while domain Behavior migration remains
active.

Read when: implementing themes, token resolution, configuration, native visual character, or the Gallery.

[Corpus map](README.md) · [Current context](current.md) · [Customization plan](plans/customization.md)

---

## Customization

The owner selected Bevy as the interface base on 2026-09-07. New native
controls use Bevy UI/widgets/text and scenes directly. The landed sections
below preserve prototype behavior and evidence, not its separate retained UI,
portable rendering API or paired HTML authoring requirement. Rewriting that
interface code is allowed; preserve the intended scopes and human control.

Resolve Lince's style roles and overrides into Bevy components and materials.
Use ordinary Configuration controls for simple edits and Flair for CSS
stylesheets and advanced authoring. Direct Rust/Bevy construction remains
available. This replaces the earlier first-party-only styling assumption;
do not build our own general CSS parser/cascade merely to avoid a dependency.

### Flair as the styling integration

Use [bevy_flair](https://github.com/eckz/bevy_flair) as the preferred CSS
integration with Bevy UI, subject to integration acceptance. Its 0.8 release
supports Bevy 0.19. Stylesheets, selectors, variables, inheritance, transitions
and hot reload can reduce Lince's styling code without adding another UI
renderer or changing how Sands are constructed. Keep embedded licenses and
credits with the relevant distribution.

Flair is not a browser or a promise of full CSS. Document the supported
property/selector profile and give visible diagnostics for rejected input;
do not rely on unsupported syntax being silently ignored. Check the pinned
release rather than assuming every browser stylesheet works. Package imports,
font/image URLs, selector complexity, stylesheet size and animation costs
remain bounded by Lince's authoring and asset policy. CSS is not permission
to read arbitrary files, fetch the network or change backend authority.

Keep one owner for each styled component property. Configuration/Box controls
edit the corresponding stored style input or override, not a second system
that fights Flair by writing the same component every frame. Map the existing
Lynx theme/workspace/group/instance precedence into explicit stylesheet roots,
layers, variables and overrides, with an inspectable winning source. Released
children retain their logical style scope even if movement reparenting changes
Bevy's transform hierarchy. Do not assume the CSS parent is always the
movement parent. Test inheritance, reparenting, copy/release, override/reset,
keyboard focus and core-widget interaction states together.

Flair's [style systems](https://raw.githubusercontent.com/eckz/bevy_flair/main/crates/bevy_flair_style/src/systems.rs)
request redraws for active animations, but some systems still visit styled
entities when a frame runs. This is not evidence of zero idle overhead.
Measure fully idle, one animated control among many static Sands, theme reload,
targeted overrides and repeated creation/removal on the minimum machine.
A narrow upstream fix, local patch or optimized style system is allowed for a
measured failure; do not prebuild an alternative styling framework. Be ready
to maintain a scoped patch if this community dependency cannot follow a needed
Bevy release promptly.

### Landed customization kernel

Contract version 1 is executable in the joined native runtime. It defines 92
canonical `--lynx-*` roles with typed color, non-negative pixel length, signed
pixel offset, scalar, integer, font, weight, line, duration and numeric-figure
values. This distinction matters: a shadow offset may be negative, while
padding, borders, radii, blur and type size may not. Unknown roles, type
mismatches, unsafe values and unknown contract versions fail closed.

The resolver has seven inspectable scopes: default contract, primitive
projection, Sand-definition defaults, active theme and mode, workspace,
group/Castle and Sand instance. Omission means inherit; every resolved value
retains its winning scope and label. The Lynx manifest completely defines Dark
and Light modes. A partial theme inherits, while remote or parent-relative
assets, undeclared extensions and invalid asset hashes are refused.

The native joined Gallery uses that resolved set for compositor background,
world nodes, borders and retained text. Its F1, F2, F3, F4 and F7 controls
exercise workspace palette, group density, instance radius, partial-theme and
mode changes. The Installed HTML fixture received the same resolved set as CSS
without a page reload; Website received neither style authority nor the
Lince bridge. The first release evidence resolved 91 tokens, performed two Installed
HTML updates while its document load count remained one, and passed on the
Wayland/Vulkan host. C3 added the Sand-margin role; the generated reference and
current joined evidence resolve all 92.

The complete source boundary and migration classifications are in
[Visual inventory](visual-inventory.md). That inventory does not claim the
legacy Web base or official Sands have migrated; those sources remain explicit
input to the later rebuild.

### Configuration

The Configuration Sand is the ordinary surface for Lince data and Box
settings. Common changes are approachable controls; arbitrary styling and
Behavior editing live behind an explicit advanced developer mode so the normal
surface stays small.

For v1 editing, select a target and scope before opening controls. Appearance,
Connections and Data can be inspected independently or combined. An appearance
edit defaults to the selected Sand, including a single Protein row's child;
the person deliberately chooses a shared template or definition when wanted.
Changing the visible facets never changes the underlying configuration.

Workspace controls are shown by default. A preference folds them into a corner
triangle with configurable color and transparency. A documented keyboard
action restores the controls regardless of that preference. Basic Sands and
packaged Castles occupy separate picker sections over the same catalog.

#### Customization and architecture

Customization is a contract, not a collection of CSS conveniences. The
contract has three levels: primitive values such as the Lynx palette and 4px
scale, semantic roles such as surface/ink/focus, and the smallest necessary
component aliases. A component consumes semantic or component roles; it never
reaches through them to a palette value. One canonical namespace replaces the
current mixture of `canvas`, `surface`, `raised-background`, `bg`, `panel`, and
similar legacy aliases.

Customization has explicit scopes. Defaults and component rules are followed
by a reusable Sand definition's structural rules and local semantic defaults,
then the active mode/style, workspace overrides, an optional group override,
and finally the Sand instance override. User choice therefore outranks an
author's defaults. A group override is ordinary inherited configuration, not a
new visual subsystem.
Every value shown in the ordinary configuration surface names its origin and
offers **inherit**; inherit removes the local value instead of copying the
currently resolved one.

#### Landed Configuration and external-authoring kernel

`crates/interface/src/configuration.rs` is the executable authority for
Configuration artifact, external-author manifest and domain-launch recipe
schema version 1. The Configuration artifact contains the exact composition
artifact rather than translating it: theme/mode, workspace and group layers,
developer CSS and launch receipts live above it, while instance style and
definition edits continue to revise the same placements and catalog.

The Configuration Sand is a 22-definition package made from the landed
primitives and compounds. F11 opens its native retained surface. Tab and
Shift-Tab traverse 16 operations; Enter, pointer activation and AccessKit reach
the same state transitions. The operations cover Dark/Light and content-hash
theme selection, spacing, margin, gap, thickness, typography, density, group
and instance overrides, typed extensions, definition configuration defaults,
ports, declarative Behavior, projection isolation/capabilities, developer CSS,
safe reset, inherit, undo, domain launch/focus and save/reopen. The panel shows
representative resolved values with the winning scope and label, and a refused
edit leaves the last good preview active.

Human runs atomically persist `configuration.json` beneath the Lince
configuration directory. Automated runs use their report directory. Save
writes a pending candidate and renames it only after the combined
Configuration/composition artifact validates. A missing file creates a
complete default; unreadable bytes, a stale schema, a missing theme identity
or a mismatched manifest hash opens the complete default without overwriting
the refused source. An edit is not installed into the live artifact or undo
history until that atomic rename succeeds, so a disk failure also leaves the
last good preview intact.

A theme is selected by manifest uid plus SHA-256, never by an arbitrary path or
remote URL. Manifest assets retain relative path, kind and hash validation.
Declared `--lynx-local-*` extensions carry a type and participate in the same
resolver. Native data, shared HTML, isolated Installed HTML and browser roots
receive identical resolved declarations; the joined probe changes the
Configuration preview in Installed HTML and returns to the Gallery style
without reloading its document.

Developer CSS is a bounded declaration list scoped to a declared root. The
validator refuses selectors and braces, `@import`, URLs, executable and
extension schemes, dynamic bindings, `!important`, host-security stacking and
positioning properties, and malformed or unapproved presentation properties.
The preview therefore cannot restyle the native Configuration Sand, which is
the always-reachable reset outside the customized root.

The parity diagnostic publishes the generated token reference, Configuration
and launch schemas, accepted/refused fixtures and a seven-file external-author
kit under `target/interface-laboratory/sand-contract/`. The kit uses ordinary
HTML, CSS and native ES modules; Maud remains optional. Unknown versions fail
closed. A domain recipe contains exact Sand revisions, placements, typed
Record inputs and exported Action bindings. One operation creates its ordinary
composition placements and persisted receipt; applying or reopening the same
domain object focuses the existing placement and increments provenance rather
than cloning it. Neither recipe nor receipt contains renderer handles.

#### Required visual character

The quality reference is the feeling of Zed: clean, minimal, sharp, dense
without being cramped, immediate under the pointer and keyboard, and quiet
enough that a person's work remains the focus. Lince does not need to imitate
Zed's exact colors or become an editor skin. It must reach the same standard of
clarity, frame pacing, typography, focus behavior and restraint. A generic
game-engine example, playful default widget theme or visibly raster-scaled
panel does not satisfy the requirement merely because it is fast.

That feeling is a system property rather than a GPUI brand property. It comes
from a small visual grammar, semantic tokens, correct native text shaping,
high-DPI geometry, consistent one-pixel decisions, low input-to-frame latency,
stable layout, restrained motion and complete keyboard/focus behavior.
The earlier P0b custom-host decision is superseded: Bevy UI, text and widgets
now provide application controls and rich Sands. Apply Lince's visual grammar
rather than copying Bevy's example theme. Bevy scene materials consume the
same resolved tokens; external publication translates the supported subset at
its own boundary. GPUI remains a quality reference, not a production path.

[Zed's account of GPUI](https://zed.dev/blog/videogame) is useful engineering
direction: a small set of data-driven GPU primitives and platform text shaping
can outperform a general arbitrary-vector layer while preserving native text
quality. Lince may reuse bounded, licensed GPUI techniques whose ownership is
separable and build specialised world-Sand passes where spatial transforms,
batching or depth require them. It does not import GPUI's window, renderer or
application lifecycle. The target is one coherent Lynx grammar, not two
visually unrelated toolkits.

The first native Gallery must test 1× and the fractional scale factors
available on the owner's hardware; light and dark themes; moving, scaling and
rotating a Sand; text while the world moves beneath it; keyboard-only
navigation; pointer capture; focus transfer between retained native UI, the
world and any external surface; and p95/p99 input-to-present latency. A 4K display is not a
current test prerequisite. Layout, clipping, text rasterization, surface
allocation, and quality selection must remain resolution- and
device-scale-aware so the architecture introduces no known 4K ceiling.
Physical 4K review begins when matching hardware is available.
Text and borders must be rerasterized or redrawn for their effective device
scale instead of magnifying a low-resolution Sand texture until it blurs.

Desk mode intentionally permits the visual cost of a modest low-graphics game:
an orthographic camera, simple or unlit materials, no physics unless enabled,
minimal post-processing and aggressive presentation batching. World mode may
add terrain, lighting, shadows, atmospheric effects, splats and dense geometry.
Quality profiles may change visual LOD, shadow resolution, antialiasing,
post-processing, splat density and cache budgets. They may not change Records,
permissions, Actions, Protein, declared game behavior or camera-invariant
simulation semantics.

### Space, thickness, roundness, rigidity

The scale covers layout space, component interior space, sizes, borders,
radii, and density. Geometry must be tokenized as deliberately as color so a
style can become spacious, sharp, soft, or dense without replacing component
CSS. Runtime coordinates, measured content, user-authored drawing geometry,
and physical simulation values are data rather than design tokens.

### Content and Sand chrome

Sand is the sole compositional vocabulary. The `LynxUI` name describes only
the legacy Web component library being inventoried and removed during
migration; it does not name a layer underneath or beside Sands in the new
interface. Every reusable control is a Sand definition with typed inputs,
outputs, state, configuration, accessibility semantics and optional Bevy
Behavior. Private Bevy helper entities do not require separate Sand identity. A button used by itself and the same button inside a
video-call composition are the same definition, not two implementations.
New native controls use Bevy directly. Existing HTML/Maud or public export
may translate a supported subset without requiring paired output from every
native constructor.

A locked group is not a special application type. It is a recursive Sand
composition with stable child identities, local layout, explicit connections,
and exported ports. **Castle** is a useful nickname for a saved, prepackaged
group of Sands, never a schema kind or a constraint. Any group can stay local,
be saved as a reusable compound Sand, or be forked. Movement attachment can be
released for one child without losing group ownership; editor locking remains
separate. See [Box](box.md#individual-appearances-and-released-children).
A Protein result template is
another use of the same composition model.

### Transparency and elevation

Elevation communicates temporary ownership or overlap, not importance or
truth. Opaque data surfaces and restrained component-local shadows remain the
default; transient editor handles and presence may be translucent because
they are explicitly not durable content.

### Line, shape, and texture grammar

Shape and line style remain usable without color. Solid means settled and
dashed means declared. User-owned colors, wallpapers, drawings, plots, and
other expression are content and are not forced into the interface palette;
their surrounding controls still use the design system.

### Typography

Typography tokens cover family, size, weight, line height, measure, numeric
features, and technical text. Language, content length, and values remain
data. Component layouts must survive English and Portuguese, zoom, user font
overrides, and the supported browser text-size settings without clipping.

### Motion

Motion tokens may describe direct manipulation and spatial simulation, but
ordinary component state changes are immediate. Reduced-motion is a behavior
contract, not merely a shorter duration token.

The first Box uses stable placement with existing movement code retained but
inactive. Decorative [shader effects](shaders.md) are later v1 work and begin
with bounded presets and a supported WGSL profile. Sizing and temporary focus
follow [Box](box.md#sizing-and-focus); focus is not a saved size override.

### The test

The Gallery is the visual and accessibility test surface. The landed F10
composition workbench additionally proves that the same primitive Sand can
stand alone, live inside a reusable compound Sand, be overridden and reset,
cross an isolated-root boundary, expose typed ports, and update all referenced
instances after its definition changes. This proves composition without
making the unfinished spatial board the test harness.
The Gallery is also the acceptance surface for the required visual character;
subjective review and captured frame/latency evidence are both required because
neither token coverage nor a benchmark alone proves that Sands feel good.

### Design system work

The active order follows Part A and the master plan:

1. Replace the custom native host/UI with one Bevy application. Use first-party
   Bevy UI, widgets, text, scenes, picking, curves and materials; no generic
   renderer adapter or separate native text/layout integration.
2. Carry the canonical tokens, Dark/Light defaults, partial themes and seven
   inspectable scopes into Bevy components/materials. Preserve inherit/reset,
   source provenance and bounded advanced styling.
3. Make native Sand/Effect composition editable through Bevy data directly.
   Keep stable ids, explicit ownership, exported ports and durable overrides;
   do not freeze the prototype's projection manifest or native ABI.
4. Rebuild or reuse primitives and Configuration through those Bevy pieces.
   Existing backend/data formats may have translators. Required editor
   extensions and custom accessibility semantics stay in Lince plugins.
5. Complete [Dogfeeding](plans/part-a.md) and its fresh visual, keyboard, IME,
   accessibility, lifecycle and resource gates. Include sleeping presentation,
   one-active/many-static workloads and bounded memory; old joined evidence
   does not certify the new application.
6. Continue the existing native follow-through and Box/time sequence. At the
   end of v1 choose specialized browserless content engines where needed.
   Public Facade/export has its own read-only boundary, not a second native
   style or widget framework.

A custom Bevy plugin, focused internal/external crate or pure WGPU pass is
allowed when a named Lince need requires it. Matching AccessKit types and
native platform services are accepted. Ordinary styling, text and curves
start with Bevy; a community crate is not first-party simply because it has
`bevy_` in its name.

Box canvas, Protein-area, grouping, wiring, and spatial-area implementation
does not begin before Part A's Bevy-native foundation gate is complete. The later Box editor consumes
the already-proven composition contract; it is not where that contract is
invented or where basic components are finally repaired.
