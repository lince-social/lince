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
mode changes. The Installed CEF fixture receives the same resolved set as CSS
without a page reload; Website CEF receives neither style authority nor the
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
Configuration preview in Installed CEF and returns to the Gallery style
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
stable layout, restrained motion and complete keyboard/focus behavior. P0b
selected a Lince-owned retained UI for application chrome, inspectors, editors
and rich native Sand surfaces because the measured GPUI source could not render
into the host-owned frame. GPUI remains the strongest behavior and quality
reference. Bevy UI is not the default visual vocabulary. The world renderer
consumes the same resolved tokens through dedicated Sand primitives, while
CEF-backed Sands receive their allowed projection as CSS variables.

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
world and CEF; and p95/p99 input-to-present latency. A 4K display is not a
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
outputs, state, configuration, accessibility semantics, renderer projections,
and optional Behavior. A button used by itself and the same button inside a
video-call composition are the same definition, not two implementations.
Native Rust and HTML/Maud may implement different projections of that
definition without creating a second component identity.

A locked group is not a special application type. It is a recursive Sand
composition with stable child identities, local layout, explicit connections,
and exported ports. **Castle** is a useful nickname for a saved, prepackaged
group of Sands, never a schema kind or a constraint. Any group can stay local,
be saved as a reusable compound Sand, or be forked; locking controls editing
and movement but does not change its data model. A Protein result template is
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

The implementation order is binding and closes behind us:

1. V1's productivity scope, v2's permanent architectural invariants and the
   native ownership boundary are frozen by Interface. The joined laboratory
   accepted the Lince-owned Wayland/WGPU host, selected Bevy rendering, retained
   UI, CEF and Web/Facade projections, and rejected GPUI as a production
   dependency. Linux has no second desktop runtime or fallback. Implementation
   size remains an accepted cost, not a reason to lower the capability or feel
   target.
2. The current visual sources, renderer-neutral token taxonomy, scope cascade,
   manifest rules, Dark/Light defaults, partial-theme behavior and native/CEF
   projection have landed as contract version 1. Browser projection will
   consume generated declarations when its adapter is rebuilt; it does not own
   another cascade.
3. Sand definition, persisted instance, projection, package and host-message
   schemas have landed as version 1. Recursive composition uses that graph;
   the future Box operation model remains a later consumer and is not implied
   by the laboratory replay format. Native Rust, installed HTML/JavaScript,
   Websites, GPU leaves and browser Plan B are projections of one semantic
   contract.
4. The first-party authoring paths selected by the prototype are frozen. Plan A
   uses native Rust retained-UI and world-renderer implementations paired with Sand
   definitions. Plan B uses Rust/Maud paired accessible fragments and native ES
   modules. Maud remains an authoring DSL, never a browser runtime or a
   requirement imposed on external Sands.
5. The compositor/simulation ownership and logical ABI adapters are frozen,
   including CEF texture/input/lifecycle handling and the absolute rule that
   camera culling affects presentation only.
6. Nineteen primitive native Sands and their Installed HTML projection have
   landed on the Lynx contract with retained state, AccessKit, Dark, Light,
   partial-theme, density and isolated-root coverage. The old Web component
   library remains migration inventory only.
7. Recursive composition, group locking, saved compound Sands/Castles,
   overrides, exported ports and explicit teardown have landed in the F10
   composition workbench.
8. Rebuild the native base and every official Sand on the shared pieces under
   Plan A, or the base Web surface under Plan B, deleting legacy CSS, copied
   HTML, and old APIs rather than adapting around them. Browser Facades keep
   the renderer adapter selected for their capabilities.
9. Land the remaining official-Sand author documentation, accessibility and behavior tests, CEF/iframe
    and theme tests, runtime validation, and automated design-system checks.

Box canvas, Protein-area, grouping, wiring, and spatial-area implementation
does not begin before steps 1–10 are complete. The later Box editor consumes
the already-proven composition contract; it is not where that contract is
invented or where basic components are finally repaired.
