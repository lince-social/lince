Customization (@customization: 0, is #chapter, #instinct, #part-of @interface, #done) { r_4Z5VS9MJNRDHZGXW4KMGRNXMHK

## Customization

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
stable layout, restrained motion and complete keyboard/focus behavior. GPUI is
the preferred implementation for application chrome, inspectors, editors and
rich native Sand surfaces because it already targets this class of interface.
Bevy UI is not the default visual vocabulary. The world renderer consumes the
same resolved tokens through dedicated Sand primitives, while CEF-backed Sands
receive their allowed projection as CSS variables.

[Zed's account of GPUI](https://zed.dev/blog/videogame) is useful engineering
direction: a small set of data-driven GPU primitives and platform text shaping
can outperform a general arbitrary-vector layer while preserving native text
quality. Lince may reuse GPUI or its public primitives where they fit and build
specialised world-Sand passes where spatial transforms, batching or depth
require them. The target is one coherent Lynx grammar, not two visually
unrelated toolkits.

The first native Gallery must test 1×, fractional scaling and 2×/4K-class
density; light and dark themes; moving, scaling and rotating a Sand; text while
the world moves beneath it; keyboard-only navigation; pointer capture; focus
transfer between GPUI, the world and CEF; and p95/p99 input-to-present latency.
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

LynxUI and Sand become one compositional vocabulary. LynxUI continues to own
the accessible behavior and visual anatomy of ordinary controls, but every
reusable control can also be referenced as a Sand definition with typed
inputs, outputs, state, and optional Behavior. A button used by itself and the
same button inside a video-call composition are the same definition, not two
implementations.

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

The Gallery is the visual and accessibility test surface. Before Box work, a
small composition workbench must additionally prove that the same primitive
Sand can stand alone, live inside a reusable compound Sand, be overridden at
each scope, cross an isolated-root boundary, expose typed ports, and update
all referenced instances after its definition changes. This proves
composition without making the unfinished spatial board the test harness.
The Gallery is also the acceptance surface for the required visual character;
subjective review and captured frame/latency evidence are both required because
neither token coverage nor a benchmark alone proves that Sands feel good.

### Design system work

The implementation order is binding and closes behind us:

1. Freeze v1's productivity scope, v2's permanent architectural invariants and
   the required visual-character acceptance checks. Run the GPU-first Plan A
   prototype and decide the native compositor, GPUI, selected Bevy features,
   CEF, Web/Facade, and ownership gates. Implementation size and fork work are
   accepted costs, not reasons to lower the capability or feel target. If no
   path can meet correctness, security, performance and visual-quality gates,
   select the preserved Maud/HTML-first Plan B and remove incomplete native
   paths without weakening the shared Sand or v2 contracts.
2. Inventory every current visual value and freeze one renderer-neutral token
   taxonomy, scope cascade, theme format, component-state matrix, and versioned
   contract. The same resolved semantic token must reach GPUI/world-renderer
   style data, CEF-installed Sand CSS variables, and Plan B browser CSS.
3. Freeze the runtime schemas used by the Sand definition, composition, port,
   host-message, renderer-adapter, and future Box-op models. Native Rust,
   installed HTML/JavaScript, Websites, GPU leaves, and browser Plan B are
   projections of this one semantic contract.
4. Freeze the first-party authoring paths selected by the prototype. Plan A
   uses native Rust/GPUI and world-renderer implementations paired with Sand
   definitions. Plan B uses Rust/Maud paired accessible fragments and native ES
   modules. Maud remains an authoring DSL, never a browser runtime or a
   requirement imposed on external Sands.
5. Freeze the chosen compositor/simulation ownership and logical ABI adapters,
   including CEF texture/input/lifecycle handling and the absolute rule that
   camera culling affects presentation only.
6. Finish the default Lynx tokens and rebuild LynxUI behavior and anatomy on
   them, with Dark, Light, partial-theme, density, and isolated-root coverage.
7. Make LynxUI primitives referenceable Sands and prove recursive composition,
   group locking, saved compound Sands/Castles, overrides, and exported ports
   in the composition workbench.
8. Finish the Configuration Sand and advanced editor so a human can exercise
   every supported scope without editing files.
9. Rebuild the native base and every official Sand on the shared pieces under
   Plan A, or the base Web surface under Plan B, deleting legacy CSS, copied
   HTML, and old APIs rather than adapting around them. Browser Facades keep
   the renderer adapter selected for their capabilities.
10. Land author documentation, accessibility and behavior tests, CEF/iframe
    and theme tests, runtime validation, and automated design-system checks.

Box canvas, Protein-area, grouping, wiring, and spatial-area implementation
does not begin before steps 1–10 are complete. The later Box editor consumes
the already-proven composition contract; it is not where that contract is
invented or where basic components are finally repaired.

### Plan B and installed-HTML JavaScript Behavior

Under Plan B, ordinary first-party interface Behavior is authored and shipped
as native JavaScript ES modules. Under Plan A, the same logical Behavior ports
may be implemented by native Rust systems, while installed external HTML still
uses JavaScript behind the CEF bridge. There is no TypeScript requirement,
generated DOM implementation, UI framework, or mandatory JavaScript bundler.
The JavaScript inspected in the repository is the JavaScript embedded or
packaged by Rust and executed by the browser runtime.

Plan B's shared Box world renderer and compute-heavy Worker kernels are one
deliberate exception: Rust compiles to a content-addressed WebAssembly artifact,
with narrowly scoped generated loader glue, so `wgpu` can use WebGPU or its
supported WebGL2 path in the browser. Plan A uses native Rust and `wgpu` for the
equivalent world work. Neither plan turns external HTML into Wasm, requires a
third-party author to use Rust, or silently moves DOM Behavior out of
JavaScript. Sources, generated boundaries, hashes, licenses, build commands,
and debugging artifacts remain separately visible.

This keeps the asset path direct:

- ordinary HTML, CSS, and Behavior edits need no frontend transpilation or
  JavaScript bundle step;
- browser stack traces and development tools point at the editable source;
- first-party modules, installed Sand modules, and external author examples use
  the same runtime language;
- module loading, source order, and content-security policy are visible rather
  than being transformed by a hidden build stage;
- Node remains a testing and development tool rather than a prerequisite for
  compiling the Lince binary.

The renderer changes the Rust packaging path honestly: release packaging must
compile and optimize its Wasm target and generated loader, while `cargo check`
remains the required Rust correctness command and gains a dedicated check for
the Wasm target. Checked-in or packaged artifacts must never conceal a stale
source/artifact hash mismatch.

The costs are equally explicit:

- property names, null handling, port compatibility, and operation coverage
  are not checked before execution;
- large refactors have less editor assistance and require smaller modules,
  contract fixtures, and focused tests to remain safe;
- the no-comments rule also excludes JSDoc as a parallel annotation system;
- runtime schemas can drift from Rust if they are hand-copied into browser
  code;
- JavaScript alone provides no validation, isolation, permissions,
  accessibility, frame-rate, or memory guarantee.

The safety contract therefore lives in executable boundaries rather than
annotations:

- the persistent and wire model stays authoritative in Rust. One explicit
  Rust-owned task emits versioned, machine-readable schemas and valid/invalid
  fixtures; browser modules consume those artifacts instead of restating the
  shapes;
- every value received from files, Protein, WebSockets, `postMessage`,
  installed Sands, Websites, or the network is validated and normalized before
  domain code can use it. Validation remains enabled in release builds;
- Sand definitions, instances, stable child identities, ports, Protein field
  mappings, override patches, host messages, and Box operations all carry an
  explicit version and discriminating `kind`. Dispatch handles the complete
  known registry and rejects an unknown version, kind, field, or malformed
  value rather than guessing;
- constructors and boundary decoders produce the canonical in-memory shapes.
  Renderers and interaction modules do not assemble look-alike contract
  objects ad hoc;
- shared contract fixtures run through Rust and JavaScript. Negative fixtures
  cover absent fields, wrong scalar/container kinds, unknown operations,
  duplicate identities, invalid port connections, excessive nesting, and
  untrusted extra fields;
- first-party behavior moves out of inline scripts and HTML event attributes
  into small ES modules with explicit imports, exports, ownership, and teardown.
  This also permits a strict Facade content-security policy;
- vendored libraries remain in their distributed JavaScript with licenses and
  credits, behind small validating adapters;
- external Sands ship JavaScript and are bound by the versioned manifest,
  runtime schemas, ports, and capabilities. Their internal authoring tools are
  irrelevant to Lince and do not become part of its build.

Native JavaScript Behavior does not forbid a future bundling step if measured
request or packaging costs justify one. Bundling would remain an asset
optimization, not a source-language change or a place to hide contract
behavior. Plan B's `wgpu` Wasm pipeline is a renderer build, not precedent
for generated implementations of ordinary Sand Behavior.

### Runtime plans and Plan B HTML alternatives

Plan A is the GPU-first native prototype described in
[Interface](Interface.md#plan-a-gpu-first-native-prototype): a Lince-owned
`wgpu` compositor, GPUI native UI, a replaceable 2D/3D world-engine adapter,
and accelerated CEF surfaces for real external HTML. Plan B is the preserved
Maud/HTML-first browser hybrid. The plans share the Sand graph and token
contract; the following HTML authoring/reactivity comparison chooses the Plan B
implementation and the HTML-backed adapters that remain present under Plan A.
Both are v1 runtime paths. The v2 world horizon is not “Plan B”; the preferred
Plan A exists partly to exercise its permanent compositor, Sand, coordinate and
external-HTML seams while shipping only v1's productivity feature scope.

This choice has three independent axes which must not be collapsed into one:

1. **How first-party HTML is authored:** Rust/Maud functions or raw `.html`
   files.
2. **How the running browser becomes reactive:** Lince's native ES-module
   composition runtime or Datastar attributes/signals/morphing, possibly with
   native modules beside it.
3. **How Box's spatial world is rendered and calculated under Plan B:** the
   shared Rust/`wgpu` WebAssembly renderer plus Worker simulation, independent
   of the DOM authoring and reactivity choices above. Plan A instead supplies
   the native compositor and world-engine adapter.

Maud output is still pure HTML. Choosing Maud does not put Rust in the browser,
and choosing raw HTML does not remove the need for the Rust-owned Sand and Box
schemas. Datastar is different: it is a browser runtime and, if selected for
first-party composition, it replaces the “no frontend framework” part of Plan
B's native-JavaScript Behavior decision. The four honest combinations are:

| First-party structure | Browser reactivity | Character |
| --- | --- | --- |
| Maud | Native ES modules | Plan B default: typed Rust authoring, ordinary DOM artifacts, Lince-owned runtime |
| Raw HTML | Native ES modules | Smallest toolchain and fastest direct prototype, but the weakest first-party structural reuse |
| Raw HTML | Datastar | Attribute-first reactive HTML with no Rust view composition; attractive for small forms and conventional pages |
| Maud | Datastar | Strong server-rendered hypermedia option: Rust functions render initial and streamed fragments while Datastar supplies signals and morphing |

All four must preserve the owner-authored Interface contract in
`Interface.lingua`: small and ready-made Sands compose into Castles, receive
Protein, issue typed Actions, exchange events, expose editable properties, and
remain customizable in Box. A stack is rejected if it can make a reactive page
but cannot make those relationships visible and reusable.

#### Invariants independent of stack

The stack does not get to redefine these contracts:

- `SandDefinition` and the versioned Box document are the persistent truth;
  neither rendered DOM nor a reactive signal store is persisted as the
  composition.
- A child is composable only when it has stable identity, typed ports,
  configuration, state-plane ownership, Behavior, capability requirements,
  isolation, lineage, and content-addressed assets. DOM nesting alone is not a
  composition tree.
- Protein is a read source. A displayed or reactive value is never writable
  authority. Durable changes occur only through typed Actions attributed to the
  Sand instance and actor.
- Ledger truth, persistent Box host state, and ephemeral browser/session state
  remain separate. No library-specific signal silently becomes a fourth state
  plane.
- Connections route through stable Sand ports, not CSS selectors, HTML ids,
  DOM bubbling, global functions, or framework signal names.
- The runtime rejects unknown schema versions, kinds, ports, operations, and
  capabilities. A library may help render accepted values but does not become
  a validation boundary.
- Trusted first-party nodes may share a renderer root. Installed external HTML
  and Websites retain their required CEF process/profile boundary under Plan A
  or iframe/WebView boundary under Plan B. A composition library cannot merge
  those security boundaries for convenience.
- CSS tokens and the complete customization cascade work in shared and isolated
  roots. A library's style system cannot become a competing token vocabulary.
- A public Facade uses the same definitions but has no Action authority. Its
  initial contract requires a strict CSP without inline script or
  `unsafe-eval`.
- Every mounted renderer and Behavior has deterministic teardown. Removing,
  replacing, forking, explicitly pausing, or morphing a Sand cannot leave
  listeners, subscriptions, timers, media, observers, or capability handles
  alive. Moving outside the camera is not a pause or teardown event.
- External HTML remains possible in every scenario. The first-party choice is
  not imposed on third-party authors.

The canonical Sand schema and paired-authoring examples live in
[Sands](Sands.md#one-definition-graph-several-authoring-paths). The sections
below follow one Sand from source to the final running Box and record where each
approach helps or charges complexity.

#### Plan A native renderer and customization contract

Plan A resolves the same token cascade once and projects it into each renderer:

| Implementation | Runtime binding | Presentation boundary |
| --- | --- | --- |
| Native application or control Sand | Rust systems and typed GPUI view state | GPUI surface or GPUI texture in the final compositor |
| Box/world/game/map/GPU Sand | ECS entity and retained renderer handle | Shared native `wgpu` world scene |
| Installed external HTML Sand | Validated CEF process bridge with complete declared ports | Accelerated CEF texture imported into the compositor |
| Website Sand | Host-owned wrapper ports only | Isolated CEF request context/profile and browser surface |
| Browser or Facade adapter | Native ES modules and/or shared Wasm world renderer | Ordinary browser HTML plus canvas/iframe surfaces |

Primitive and semantic tokens remain canonical serializable values. The native
adapter converts resolved roles into GPUI styles, packed instance data, text
styles, shader uniforms, and renderer resources. The installed-HTML adapter
exposes allowed values as instance-scoped CSS custom properties. A Website can
customize only its Lince-owned wrapper unless the remote origin voluntarily
supports an ordinary Web theming API; Lince never injects styling into an
arbitrary cross-origin page.

Native and HTML implementations of the same built-in definition share identity,
ports, state planes, configuration, Behavior meaning, and test fixtures. Pixel
identity is not required across GPUI, GPU, and browser text rasterizers, but the
information, order, affordances, focus behavior, non-color meaning, and resolved
token roles are. A renderer-specific value is permitted only below the semantic
token boundary and cannot become a second user-facing theme vocabulary.

Camera visibility is a renderer hint with one allowed effect: omit draw,
composite, tessellation, and accessibility nodes that are not currently
reachable on screen while preserving the authoritative semantic state needed
to restore them. It cannot suspend a renderer process, Sand Behavior, game,
physics body, Protein subscription, event route, media session, or Area
interaction. Focused and off-camera entities receive the same update policy;
only actual presentation work differs.

#### Plan B spatial renderer and compute stack

Preserved Plan B decision from 2026-08-23: Box uses one shared Rust/`wgpu`
renderer compiled to WebAssembly for its spatial world. It runs over WebGPU
when a usable adapter is available and over wgpu's supported WebGL2 backend for
the rendering subset on which the fallback is required. It is not a second
interface toolkit. Ordinary Sands, LynxUI controls, Record text, forms,
editors, accessibility, installed HTML, and Website iframes remain real DOM.

The browser composition runtime remains the authority that resolves Sand
definitions, delivers Protein values, routes typed ports, checks Action
requests, applies customization, and owns isolation. It mounts one of several
renderer adapters without changing the persistent Sand graph:

| Implementation | Runtime binding | Presentation boundary |
| --- | --- | --- |
| Maud/raw HTML Sand | Scoped native ES-module context | Trusted DOM composition root |
| Shared spatial or specialized GPU leaf | Renderer scene handle plus typed ports | Shared `wgpu` world surface |
| Installed external HTML Sand | Validated, size-bounded `MessagePort` bridge | Isolated iframe/WebView |
| Website Sand | Host-owned wrapper ports only | Sandboxed iframe or zero-capability WebView |
| Optional Wasm Behavior | Generated binding for the logical Behavior ABI | Worker or bounded component host; no ambient DOM |

The common ABI is semantic rather than one unsafe binary calling convention.
It covers definition and instance identity, versions, typed inputs and outputs,
configuration, selected state-plane handles, capabilities, bounds and device
scale, mount, resize, camera-visibility hints, explicit user/system pause,
dispose, error reporting, and host-validated Action requests. A
camera-visibility hint may cull presentation only; it never pauses Behavior,
physics, media, Protein, events, or CEF execution. The authoritative Rust model
generates runtime schemas, JavaScript validators and fixtures, iframe or CEF
messages, and any future WIT projection. DOM nodes, JavaScript functions, GPU
objects, raw pointers, credentials, and the global Box store never cross the
portable boundary. WIT may become a binding for optional Wasm Behaviors; it is
not the persisted Sand format and is not a browser requirement.

One renderer device and retained scene serve the workspace. Stable visual-node
uids index instance buffers so a transform change produces a bounded partial
buffer update. No ordinary Sand creates its own Wasm runtime, GPU device, frame
loop, or canvas. A specialized GPU Sand joins the shared renderer when its
requirements fit the safe renderer vocabulary; a dedicated canvas is an
exception justified by media, isolation, or incompatible surface ownership.
User or third-party shaders require an installed, content-pinned package,
declared GPU capability, validation, budgets, teardown, and bundled license and
credits rather than becoming arbitrary Box expressions.

Rendering and physics are measured separately. Plan B's first physics path is
a Rust/Wasm Worker with a spatial broad phase, dirty/awake sets, state-based
equilibrium sleep, group-level coarse bodies, and batched transform diffs. It
performs no all-pairs collision scan and sends no full-world snapshot merely
because one Sand moved. Camera position never changes simulation eligibility.
The main thread owns input and DOM composition; the renderer owns GPU
resources; the Worker owns simulation state. WebGPU compute is added only for
a measured large regular kernel whose total upload, dispatch, readback, and
synchronization cost beats the Worker path.

The baseline does not require Wasm threads or `SharedArrayBuffer`. Requiring
cross-origin isolation can change which external resources and embedded pages
work, so transferable or copied typed batches are proven first. If profiling
later justifies shared memory, its COOP/COEP consequences, Website behavior,
Facade headers, deployment, and fallback are a separate explicit decision.

This hybrid improves maintenance because presentation technology does not
change Sand meaning. A DOM button, a GPU graph node, an installed HTML panel,
and an optional Wasm Behavior exchange values only through the same visible
ports. External HTML does not link to `wgpu` or Wasm and cannot inspect GPU
state; it asks the host to emit an event or request an Action. Conversely, a
GPU renderer receives only normalized state and cannot acquire Protein or
Action authority from pixels.

The limits remain visible:

- `wgpu` cannot render, inspect, restyle, or capture a cross-origin iframe as
  an interactive texture; the browser compositor keeps Website pixels as HTML;
- one shared world surface cannot arbitrarily weave individual GPU objects
  between unrelated DOM stacking contexts, so the initial order is GPU world,
  DOM Sands/Websites, then host-owned DOM editing chrome;
- canvas pixels do not create an accessibility tree, so an interactive GPU
  leaf supplies a synchronized semantic DOM counterpart and keyboard route;
- 1,000 placed bodies that remain logically active regardless of camera is a
  design target; 1,000 simultaneously visible rich DOM editors or live
  Websites is a different and much more expensive workload that GPU rendering
  cannot make cheap;
- WebGL2 fallback can render the agreed subset but does not provide WebGPU
  compute, so compute remains an optional enhancement rather than correctness.

The renderer proof therefore measures the complete pipeline rather than an
isolated GPU demo: Wasm download and compilation, shader warm-up, device loss,
resize and device scale, frame time, Worker time, main-thread blocking, GPU
upload volume, memory, DOM reconciliation, Website focus, accessibility, and
teardown on the owner's reference machine.

#### Alternative A — Maud structure with native CSS and ES modules

This is the selected Plan B DOM authoring and reactivity choice. Rust/Maud authors
first-party structure; CSS remains ordinary token-based CSS; small native ES
modules own browser-only behavior; and Lince's JavaScript composition runtime
mounts the same normalized definitions created by Maud or Box. The shared
`wgpu`/Wasm world renderer is the independent spatial layer described above.

Lince's Cargo workspace already pins Maud 0.27.0, so this choice does not add a
new template crate; it expands the responsibility of one already used by the
Web surface. The current [Maud guide](https://maud.lambda.xyz/) describes it as
a compile-time Rust macro with ordinary functions returning `Markup` as its
composition mechanism. It is dual MIT/Apache-2.0 licensed in the
[upstream repository](https://github.com/lambda-fairy/maud). The exact version
still remains pinned and reviewed like any other dependency.

The final running path is:

1. Rust constructors build paired `AuthoredNode` values: accessible Maud
   `Markup` plus the matching Sand node, ports, configuration, Behavior
   references, and assets.
2. The artifact compiler validates the complete graph, renders fragments,
   namespaces packaged paths, verifies capability and license declarations,
   and hashes the definition, fragments, modules, CSS, and other assets.
3. Box loads its document and resolves every exact definition revision. It does
   not load copied HTML from individual placements.
4. The browser creates the trusted composition roots and required isolated
   roots, clones the referenced fragments, derives instance-safe DOM ids, and
   applies the token/configuration cascade.
5. Renderer adapters adopt their scoped elements, connect native DOM events to
   typed ports, and mount each declared Behavior module with a validated
   instance context.
6. One board transport distributes Protein updates, lane events, and host
   state. Modules update only the instance DOM they own. A typed port route may
   request an Action through the host; the engine remains authoritative.
7. Unmount disposes the complete subtree. Save persists definition references,
   Box operations, bindings, and overrides—not DOM or arbitrary JavaScript
   memory.

The source can be pleasant and function-shaped without pretending markup is
the complete component:

```rust
fn record_summary() -> AuthoredNode {
    panel("summary")
        .child(text("title").input("value", "record.title"))
        .child(text("description").input("value", "record.description"))
        .child(button("open", "Open").output("pressed"))
        .export_input("record", "record-ref")
        .export_output("selected", "record-ref")
        .build()
}
```

This is conceptual authoring syntax. C0 owns the final Rust API. The important
part is that each constructor returns markup paired with the semantic node and
that Box sees the same `title`, `description`, `open`, `record`, and `selected`
boundaries.

Advantages:

- Rust functions, structs, enums, pattern matching, iteration, and module
  boundaries make large first-party Sands much easier to assemble and refactor
  than monolithic HTML strings.
- Interpolated text and attribute values are escaped by default. Deliberately
  unescaped CSS, JavaScript, SVG, or trusted HTML stays visibly exceptional.
- The manifest, view, Behavior modules, assets, licenses, and tests can live in
  one Rust-owned artifact without a second frontend build.
- Maud can render reusable semantic fragments once per definition revision;
  placing 200 instances clones cached browser fragments rather than asking Rust
  to render each interaction.
- The browser receives normal semantic HTML and CSS. Accessibility tools,
  text selection, forms, contenteditable, browser inspection, external pages,
  and the hybrid GPU world layer remain native Web capabilities.
- Native modules can integrate difficult browser APIs directly: canvas,
  WebGL/WebGPU, Loro, WebRTC, media devices, drag/drop, observers, workers, and
  vendored libraries.
- Strict CSP remains possible because first-party behavior lives in external
  modules without `eval`, inline handlers, or server-pushed script.
- Protein can continue sending structured data over the existing board
  transport. High-frequency spatial or presence updates do not require the
  backend to render and transmit HTML.
- Facades reuse the same static assets and read-only runtime without requiring
  a server-side UI session for each visitor.
- The composition schema—not Maud—is portable. A Box-created Castle and a
  Maud-created Castle render through the same browser path.

Correctness costs and quirks:

- Paired constructors are real infrastructure. Returning only `Markup` creates
  nice Rust partials but does not create inspectable Sands. The artifact
  compiler must reject a declared child whose fragment, ports, or metadata do
  not agree.
- Maud's ordinary partials are only functions returning `Markup`; it has no
  built-in component, port, configuration, state, or Castle model. Rust type
  checking catches template-language and value-shape mistakes, but does not by
  itself prove valid ARIA relations, unique instance ids, correct Box wiring,
  safe capabilities, or agreement with the Sand definition.
- Maud runs before live Protein data arrives. It renders the starting fragment,
  empty/loading/error surfaces, and optional `<template>` structures; native
  modules reconcile subsequent records. Trying to rerun Maud for every browser
  interaction would turn this into a server-rendered architecture instead.
- DOM ownership must be explicit. Maud owns initial structure, renderer
  adapters own their declared nodes, the Box host owns placement chrome, and a
  specialized library owns only its island. Two modules cannot both replace the
  same subtree.
- Repeated fragments cannot contain fixed global ids. The runtime derives ids
  from instance uid and local node uid and must update `for`, `list`,
  `aria-controls`, `aria-describedby`, `aria-labelledby`, fragment links, and
  any library configuration that references them.
- Rust component parameters are not Box configuration automatically. Every
  property intended for live configuration needs an explicit schema entry,
  default, renderer application rule, validation, inheritance behavior, and
  override representation.
- Definition revision changes must preserve or visibly reject instance
  overrides and Box connections. Renaming a local child or port is a schema
  change, not an innocent Rust refactor.
- Maud templates are less directly served by HTML-specific formatters, Emmet,
  browser live editing, and standalone validators. The Gallery and rendered
  snapshot tests must compensate without treating snapshots as semantic proof.
- Compile errors couple view edits to Rust. Incremental compilation and a
  watch/preview command improve the loop, but recompilation is still the chosen
  price for typed first-party composition.
- `PreEscaped` is a trust boundary. It is permitted only for reviewed bundled
  source/assets whose provenance is known; Record values, configuration,
  Protein fields, imported HTML, translations, filenames, URLs, and external
  metadata never enter it.
- Fragment-relative asset URLs must be resolved against the Sand package, not
  the Box document or application origin. Archive and plain-HTML packages must
  behave identically.
- Native JavaScript has no compile-time type checking. Runtime-generated schema
  artifacts, boundary decoders, fixtures, focused modules, and browser tests
  carry that responsibility.
- The developer must decide whether a visual part is a reusable Sand boundary
  or private markup. Too few boundaries make Castles opaque; turning every
  wrapper into a Sand makes Box unusably noisy.

Performance considerations to measure rather than assume:

- Cache parsed/rendered fragments and imported modules by exact definition
  revision. Do not repeatedly parse identical HTML or import identical module
  URLs per instance.
- Prefer one delegated native-event adapter per trusted composition root for
  common LynxUI primitives, while specialized modules retain direct listeners
  when needed.
- Updating one input should not rerender an entire Castle. Renderer contracts
  declare the smallest owned node or attribute affected by each input.
- Protein result templates need keyed reconciliation and viewport
  virtualization. Recreating every row on every stream message would squander
  the benefit of stable identities.
- Offscreen suspension must distinguish cheap static DOM from live canvas,
  media, Website, observer, timer, and subscription work.
- Measure fragment parsing, mount/unmount, input propagation, port routing,
  focus preservation, memory, and pan/zoom at 200 visible interactive Sands.
  Maud itself is absent from these browser costs.

#### Alternative B — raw HTML, CSS, and native ES modules

This keeps the same Lince schema and browser runtime but authors a Sand's
fragments directly as `.html` files. A package carries raw HTML, CSS, ES
modules, assets, and a separate definition/manifest that declares the nodes,
ports, Behavior, and capabilities the HTML implements.

```html
<section data-lince-node="summary" class="record-summary lynx-panel">
  <h2 data-lince-node="title"></h2>
  <p data-lince-node="description"></p>
  <button data-lince-node="open" class="lynx-button" type="button">Open</button>
</section>
```

```javascript
export function mount(context) {
  const button = context.nodes.get("open");
  const releaseRecord = context.inputs.record.subscribe((record) => {
    context.nodes.get("title").textContent = record.title;
    context.nodes.get("description").textContent = record.description;
  });
  const select = () => context.outputs.selected.emit(context.inputs.record.value);
  button.addEventListener("click", select);
  return () => {
    button.removeEventListener("click", select);
    releaseRecord();
  };
}
```

Advantages:

- It has the shortest direct browser feedback loop and the least source
  notation. A developer can open the file, edit it, refresh, and inspect nearly
  the exact artifact that runs.
- HTML/CSS tooling, validators, design tools, browser snippets, and outside
  contributors work without learning Rust template syntax.
- It is the natural baseline for external and imported Sands and a good escape
  hatch for large vendored/static documents that gain nothing from conversion.
- There is no Rust compile cost for a structural prototype and no temptation to
  place browser state in server-side Rust functions.
- Runtime performance can be identical to Alternative A because both resolve
  to the same fragments and ES modules.

Limitations and prices:

- Reuse is textual or manual unless common parts are separate referenced Sand
  definitions. Includes, copy/paste, or JavaScript string templates easily
  recreate the duplication the Sand graph is intended to remove.
- The HTML and separate definition manifest can drift. Tests must prove every
  declared `data-lince-node` exists exactly once in the fragment, every
  required semantic element/ARIA relation exists, and no undeclared executable
  behavior appears.
- Raw files cannot use Rust enums and exhaustive matches to construct variant
  anatomy. Conditional initial markup either grows duplicated files, a custom
  preprocessor, server rendering, or more JavaScript.
- Values interpolated by string replacement are an XSS trap. Raw HTML should be
  static structure; live values enter through safe DOM properties such as
  `textContent`, validated URL setters, and explicit renderer adapters.
- Manifest, HTML, CSS, modules, assets, and licenses are physically separate,
  which makes it easier to forget one during refactors or package assembly.
- Building an official Kanban as raw HTML does not make its buttons and panels
  Castle children. The author must still reference real primitive definitions
  and use raw HTML only for private anatomy.
- Native modules carry the same runtime-schema, lifecycle, focus, testing, and
  performance obligations as Alternative A without Maud's authoring help.

This remains fully supported even if Maud becomes the first-party standard.
The disagreement is about the fastest maintainable way to author Lince-owned
structure, not about what a Sand package is allowed to contain.

#### What Datastar 1.0 actually adds

Datastar is not currently present in Lince; its previously vendored runtime and
bootstrap were removed. Reintroducing it is a new stack decision, not use of a
dormant facility.

As researched on 2026-08-22, the official repository's quick start uses
[Datastar v1.0.2](https://github.com/starfederation/datastar) and describes the
default browser bundle as roughly 11.75 KiB. The free core provides:

- reactive signals declared in HTML and referenced with `$name`;
- two-way input binding and computed signals;
- reactive text, attributes, classes, styles, visibility, and indicators;
- event, initialization, effect, intersection, interval, and signal-patch
  attributes, including debounce/throttle modifiers where supported;
- `@get`, `@post`, `@put`, `@patch`, and `@delete` backend actions with retry,
  cancellation, headers, form, payload, and signal-filter options;
- HTML, JSON signal patch, JavaScript, and SSE response handling;
- SSE events that patch signals or patch/morph/replace/prepend/append/remove
  DOM elements;
- preservation and ignore controls for elements/attributes that must survive a
  morph; and
- custom attribute/action/watcher plugins, although the official custom-plugin
  documentation is still described as in progress.

The relevant primary references are the official
[attributes](https://data-star.dev/reference/attributes),
[actions](https://data-star.dev/reference/actions),
[signals](https://data-star.dev/guide/reactive_signals),
[SSE events](https://data-star.dev/reference/sse_events), and
[Tao](https://data-star.dev/guide/the_tao_of_datastar) pages. The official
Rust SDK is a separate [MIT crate](https://crates.io/crates/datastar), currently
0.4.0, with Axum integration. Frontend and SDK version numbers are independent
and would both be pinned deliberately.

The following are not small footnotes for Lince:

- Datastar expressions are executable JavaScript-like strings compiled with
  `Function()`. The official [security reference](https://data-star.dev/reference/security)
  requires `script-src 'unsafe-eval'`. Calling the expression context
  “sandboxed” does not make it an iframe, a capability boundary, or compatible
  with Lince's planned strict Facade CSP.
- Signal values are visible and client-modifiable. They can represent view
  state or a projection but never identity, permission, authorization, or
  trusted Action arguments without server validation.
- Backend actions send all non-underscore signals by default. Lince would need
  explicit payloads or allow-list filters at every request boundary; otherwise
  one Sand can accidentally transmit unrelated state from the shared page-wide
  signal graph.
- Prefixing a signal with `_` prevents default transmission; it does not give a
  repeated component an independently named signal. Core compositions need
  collision-proof instance namespaces or isolation.
- Datastar's recommended backend-driven style sends HTML fragments and uses ids
  or selectors for morph targets. Lince's semantic connections deliberately
  use instance and port identities instead. An adapter must map between them;
  selectors cannot become the stored Box graph.
- DOM morphing can preserve ordinary input state well when elements are keyed,
  but editors, selections, Loro bindings, canvases, WebGL contexts, media
  elements, dialogs, focus traps, third-party widgets, and pointer capture need
  explicit preservation or ignored islands. Ignoring a subtree also means the
  server can no longer update it through that morph.
- Datastar accepts `text/javascript` responses and can execute scripts sent in
  SSE patches. Lince must disable and reject that response path; executable
  Behavior remains pinned package code, never code supplied by a live data
  response.
- GET streams close when the page is hidden by default and may reopen later;
  keeping them open consumes more resources. Neither mode supplies Lince's
  required revision/resume, authorization, idempotency, backpressure, and stale
  state semantics automatically.
- Datastar request cancellation and retries are useful UX mechanisms but cannot
  define durable Action semantics. Every Lince mutation still needs a unique
  operation identity, engine authorization, deterministic response, and honest
  partial-failure state.
- The browser runtime walks and applies data attributes and reapplies them after
  patches. Attribute evaluation order can matter. Large repeated Castle trees
  need measured initialization, dependency, patch, and cleanup costs.

[Datastar Pro](https://data-star.dev/pro) is a separate commercial product.
Pro attributes include persistence, resize/RAF helpers, query-string and URL
integration, animations, view transitions, and other conveniences. Rocket,
the typed-prop/custom-element API with component-local signal scopes, is Pro
and currently beta. The Pro license prohibits publishing it in an open-source
project and restricts redistribution for reuse. Lince therefore cannot base its
open Sand authoring/runtime contract on Rocket or other Pro code under the
current license. A future license change or explicit non-open distribution
decision would be a separate business decision; core architecture cannot
silently assume it.

#### Alternative C — raw HTML with Datastar

In this scenario an official or external Sand writes reactive attributes
directly in HTML. Rust exposes endpoints that validate requests and return raw
HTML, JSON signal patches, or Datastar SSE events. Maud is not used.

```html
<section data-signals="{_open: false}">
  <button type="button" data-on:click="$_open = !$_open">Details</button>
  <div data-show="$_open">Local details</div>
</section>
```

This is genuinely attractive for conventional forms, disclosures, filters,
loading indicators, validation messages, and server-driven lists. A small
Sand can express useful local behavior without a bespoke `mount()` module, and
a backend can replace a result list with one rendered HTML response rather than
designing a client reconciliation function.

Advantages:

- The markup shows state, event, and rendering relationships close together.
  Small reactive interactions can be built very quickly.
- Forms and backend requests gain standardized loading, retry, cancellation,
  signal transport, validation, and SSE response behavior.
- Server-driven patches can keep business formatting in Rust and avoid
  duplicating presentation branches in client JavaScript.
- Datastar morphing avoids a virtual DOM, frontend compiler, TypeScript, and
  mandatory bundler.
- Raw HTML remains approachable to external Web authors.

Limitations for the final Box:

- `_open` is excluded from default requests but is not safe for 200 repeated
  instances sharing a root. Every signal needs an instance-derived namespace,
  yet the static HTML file does not know the eventual Box instance uid. The
  host must rewrite expressions, generate the fragment at mount, or isolate the
  Sand. Each option adds machinery or boundaries.
- Behavior becomes split between Datastar expression strings and Lince's typed
  Behavior graph. If Box merely displays `data-on:click`, it cannot validate or
  draw the Action/event route. If the graph separately describes it, the two
  sources can drift.
- Direct `@post` or `@delete` calls bypass the Sand port/Action context unless
  every endpoint reconstructs and validates definition uid, instance uid,
  actor, target, capability, and operation uid. Hiding those requirements in a
  URL would undo the Action contract.
- Server DOM patches target browser structure instead of semantic Sand inputs.
  A Box connection cannot naturally say “send this event to that port” by
  asking a remote endpoint to morph a selector.
- CSP requires `unsafe-eval` in every root that runs Datastar core. This rules
  out the current Facade contract and weakens defense in depth for trusted
  shared roots.
- Raw reactive attributes are executable behavior embedded in HTML, reversing
  the current decision to keep first-party Behavior in explicit ES modules and
  definition nodes.
- Morph-based server rendering creates a server dependency for UI that the
  current package could run locally or in a static Facade. Offline and stale
  behavior must be designed for every interaction.
- Without Maud or another server template system, large Rust-generated patches
  return to included HTML files, string assembly, or duplicated variants.

This scenario is strongest for a conventional server-owned administration
page. It is weakest where Box needs many independent, repeated, locally
composed components with visible port wiring.

#### Alternative D — Maud with Datastar

This is the strongest Datastar scenario. Maud renders the initial document and
the same Rust functions render later fragments; Datastar supplies browser
signals, declarative event attributes, backend actions, and SSE morphing.

A conceptual local interaction is compact:

```rust
fn disclosure() -> Markup {
    html! {
        section data-signals="{_open: false}" {
            button type="button" data-on:click="$_open = !$_open" { "Details" }
            div data-show="$_open" { "Local details" }
        }
    }
}
```

A server-owned Protein projection could render through Maud and stream a
Datastar element patch through the Rust SDK:

```rust
fn records(rows: &[RecordView]) -> Markup {
    html! {
        section id="records" {
            @for row in rows {
                (record_summary(row))
            }
        }
    }
}
```

The endpoint can send `records(rows).into_string()` as a patch element. The
exact Axum/SSE code is deliberately left to the C0 spike because SDK and
stream ownership must be tested against Lince's existing transport rather than
selected from a documentation snippet.

Advantages:

- Maud supplies reusable, escaped Rust view functions for initial and updated
  fragments; Datastar supplies reactivity without hand-written DOM
  reconciliation for ordinary server-owned screens.
- One Rust function can be the rendering authority for loading, empty, error,
  and populated states, reducing markup drift between initial load and live
  updates.
- Protein changes can become streamed semantic HTML rather than a client-side
  rendering model. This is particularly pleasant for moderate-rate lists,
  forms, configuration panels, permission review, and status surfaces.
- Server validation can respond with the exact corrected controls and messages
  rather than a JSON error vocabulary duplicated in JavaScript.
- Maud can generate instance-specific ids and Datastar signal namespaces when
  the server knows the concrete instance, solving part of the collision issue.
- The browser still uses HTML/CSS and a small framework rather than a virtual
  DOM or frontend compiler.

Limitations and architectural prices:

- A static Maud artifact compiled once per definition does not know the final
  instance uid, while safe Datastar signal names and morph ids need it. Either
  Rust renders per instance/request, the JavaScript host rewrites attributes,
  or each instance receives isolation. Per-instance rendering introduces a
  server/session lifecycle the current definition-fragment model avoids.
- If Rust renders a Castle after each Protein change, the server must know its
  definition revision, Box overrides, token context, row bindings, visitor
  state, and current render target. That couples Box presentation to backend
  sessions and makes static/offline Facades less natural.
- Streaming HTML duplicates the existing Protein transport if both stay. Lince
  should have one board-level stream, not an SSE connection per Sand and a
  WebSocket carrying the same facts. Replacing the WebSocket would require
  proving lanes, bidirectional ephemeral events, collaboration, Action
  outcomes, resume, and backpressure over the new split fetch/SSE design.
- Server-rendered HTML costs more bandwidth than structured field changes when
  many records move or physics updates frequently. Compression helps ordinary
  fragments but does not make HTML appropriate for frame-rate data.
- The Sand definition graph and Datastar expressions can become two Behavior
  languages. If this alternative is chosen, handwritten Datastar Action/event
  expressions cannot be the semantic source. The artifact compiler must
  generate them from declarative Sand Behavior or wrap them behind a declared
  module adapter so Box remains truthful.
- Datastar morph identity uses DOM ids/selectors; Box identity uses definition,
  instance, child, and port uids. Every patch target must be derived from and
  checked against the latter. A server must never accept an arbitrary selector
  from a Sand.
- Interactive islands—Record editor, Loro text, canvas, graph, terminal,
  WebRTC, video, Website, drag session—must be excluded from morphing or given a
  proven preservation adapter. A fat morph of a Castle is unsafe merely because
  it is convenient.
- The strict CSP conflict remains. Maud does not remove Datastar's use of
  `Function()` or its need for `unsafe-eval`.
- The backend's ability to return executable JavaScript must be removed. Maud
  plus trusted Rust still does not justify turning live responses into a code
  delivery channel.
- The official Rust SDK is another protocol dependency to pin and vendor
  correctly; the browser runtime and SDK compatibility need shared wire
  fixtures.

If chosen, Datastar should be an implementation of part of the Sand renderer,
not the definition model. The best version of this alternative is:

- Maud paired constructors still emit `SandDefinition` nodes and ports;
- declarative Behavior compiles to a restricted reviewed subset of Datastar
  attributes;
- typed Lince Actions remain host calls, not arbitrary `@post` URLs;
- one board-owned adapter maps Protein patches into allowed signal/element
  patches;
- module Behaviors remain available for editors, spatial work, media, and
  specialized libraries;
- execute-script responses are rejected;
- morph targets are instance-derived and bounded to the owning root; and
- Facades either omit Datastar or deliberately revise the no-`unsafe-eval` CSP
  requirement with an explicit security decision.

That is possible, but it is substantially more integration work than “add one
small script”.

#### Datastar as only a Behavior or reactivity helper

There are three bounded interpretations:

1. **Local attributes only.** Datastar manages disclosure, selected tab,
   filtering, derived labels, form bindings, and indicators inside a Sand; it
   makes no backend requests and owns no durable state. Lince ports and modules
   translate between Sand inputs/outputs and signals.
2. **Server-rendered conventional surfaces only.** Configuration, package
   review, or another server-owned panel uses Maud/Datastar while Box and Sands
   retain the native runtime.
3. **Isolated Datastar Sand.** A package vendors Datastar core, declares its
   license and CSP need, and runs in an iframe boundary. The Box sees only its
   wrapper ports.

The first reduces tiny JavaScript modules but still introduces a second
reactivity language, global signal naming, `unsafe-eval`, and attribute/runtime
cost across the trusted root. It is difficult to justify when the Lince
Behavior graph already needs to represent the same toggle or event.

The second contains scope but gives the application two first-party UI
lifecycles and debugging models. It is reasonable only if a concrete
server-owned surface becomes dramatically simpler and never needs Castle
composition.

The third is technically cleanest for an optional third-party Sand because CSP
and signal scope are isolated. It is a poor way to implement every first-party
button: iframe cost and lost shared composition would defeat the Sand model.

An external Sand is always free to use Datastar internally under its own
package license and capability declaration. Lince integration still occurs
only through the bridge and typed ports. That does not require Datastar in the
base application.

#### Comparative decision record

| Concern | Maud + native modules | Raw HTML + native modules | Raw HTML + Datastar | Maud + Datastar |
| --- | --- | --- | --- | --- |
| First-party structural reuse | Strong Rust functions plus Sand references | Sand references only; private markup easily duplicates | Attribute reuse remains mostly textual | Strong Rust functions plus streamed fragments |
| Box/Castle equivalence | Direct through paired schema constructors | Requires separately maintained metadata | Requires adapter from attributes to typed Behavior | Possible only if schema generates/restricts attributes |
| Live Protein rendering | Structured data reconciled in JS | Structured data reconciled in JS | Server patches or signal patches | Rust renders streamed fragments or patches signals |
| Browser-only APIs | Direct native modules | Direct native modules | External scripts/components still required | Native modules still required for specialized islands |
| Strict Facade CSP | Compatible | Compatible | Incompatible with core `unsafe-eval` requirement | Incompatible with core `unsafe-eval` requirement |
| Offline/static Facade | Natural | Natural | Local-only attributes possible; backend behavior unavailable | Server-driven behavior unavailable without a server |
| State scope across repeated Sands | Lince instance context | Lince instance context | Manual signal namespace or isolation | Per-instance render/rewriting or isolation |
| Durable Action path | Typed host API | Typed host API | Must forbid raw URLs from becoming authority | Must adapt generated attributes to typed host API |
| DOM preservation | Explicit renderer ownership | Explicit renderer ownership | Morph keys/ignore rules required | Morph keys/ignore rules required |
| Debugging source | Rust markup plus browser modules | Artifact is direct source | HTML expressions, signals, SSE, backend | Rust template, HTML expressions, signals, SSE, modules |
| DOM/reactivity build | None | None | None for default bundle | None for default bundle |
| DOM/reactivity dependency | None beyond Lince runtime | None beyond Lince runtime | Datastar browser core | Browser core plus likely Rust SDK |
| Open-source licensing | Maud permissive | Native platform | Core MIT; Pro unavailable | Core/Rust SDK MIT; Pro unavailable |

The selected HTML-first Plan B is **Maud plus native CSS and ES modules for DOM
composition, together with one shared Rust/`wgpu` WebAssembly world renderer
and Rust/Wasm Worker simulation**. It buys first-party authoring reuse and
cross-platform spatial rendering without surrendering Lince's typed
composition graph, strict CSP, static Facades, one shared Protein transport,
direct access to browser APIs, or real external HTML. Raw HTML remains the
supported external and specialised escape hatch. Under Plan A this remains the
browser/Facade and fallback architecture; it is no longer the first desktop
runtime attempted. The Wasm build belongs only to the renderer and measured
compute kernels; the comparison table's build rows describe the separate
DOM/reactivity path.

Datastar's most compelling case is Maud-rendered conventional server UI with
moderate-rate SSE fragments. Its weakest fit is the final Box: many repeated
instance-scoped components, explicit cross-Sand ports, client-side spatial
interaction, CRDT editors, media, offline packages, and strict public Facades.
This recommendation can change after a spike, but no Datastar code should be
reintroduced merely to keep the option open.

#### Proof required before changing the recommendation

One vertical fixture should be implemented in isolated prototype code, not in
the production board, if Datastar remains under consideration. It contains:

- one Maud-authored Button Sand also constructible in Box;
- one Record summary compound with `record.title` and `record.description`;
- one Protein list producing at least 200 keyed repeated instances;
- one local disclosure, one cross-Sand `record-selected` event, and one typed
  mutation Action;
- one editable input whose focus, selection, validation, and unsaved value must
  survive an unrelated update;
- one Loro/contenteditable island, one canvas island, and one isolated iframe
  Sand that must survive a parent update;
- definition update, instance override, fork, save/reload, unmount, and
  off-camera presentation culling without Behavior suspension;
- a read-only Facade build with its real CSP; and
- instrumentation for bytes, requests/connections, mount time, patch time,
  memory, leaked listeners, and dropped frames.

Compare the current Maud/native implementation against Maud/Datastar and, only
where it adds useful evidence, raw HTML/Datastar. A Datastar prototype passes
correctness only if:

- the normalized Sand and Box schemas remain the sole persistent truth;
- every signal is demonstrably instance-scoped and mapped to one state plane;
- no request sends undeclared signals or authority-bearing client values;
- typed ports and Actions remain inspectable in Box and enforced by the host;
- no response can execute script;
- DOM patches cannot escape the owning Sand/root or target arbitrary selectors;
- editor/media/canvas/iframe islands preserve identity, focus, state, and
  teardown;
- one board stream supplies bounded resume/backpressure behavior without a
  connection per Sand; and
- the Facade security decision is explicit. If `unsafe-eval` remains forbidden,
  Datastar cannot be part of that runtime.

Only after correctness passes do performance results matter. A smaller source
file or framework bundle does not compensate for duplicate transports, larger
HTML patches, leaked observers, lost editor state, or a CSP regression. If the
Datastar alternative loses, remove the prototype and dependency completely;
there is no installed base requiring a compatibility layer.

### LynxUI

- [x] Select Lynx and keep one evolving light/dark demo in the canonical [`LynxUI Gallery Sand`](../crates/web/src/sand/lynx_ui/index.html). Update this design-system description whenever the demo guidelines change.
- [x] Build the LynxUI base as a framework-free component library for official Sands. Use native semantic HTML, explicit `lynx-*` classes, and a small JavaScript layer only for behavior that HTML does not provide consistently.
- [x] Serve shared `lynx-ui.css` and `lynx-ui.js` assets. LynxUI uses the design-system tokens and defines no separate colorscheme, spacing scale, motion, or elevation. Component selectors have low specificity so global and per-Sand styles can override them.
- [x] Provide Catppuccin Macchiato as the second style. Its CSS changes only colorscheme variables, uses the official Base, Mantle, Crust, Text, Subtext, Overlay, Mauve, Lavender, Red, Yellow, Green, and Blue values, and carries the Catppuccin MIT notice.
- [x] The first component set has buttons and button groups; inputs, textareas, selects, checks, radios, labels, help and errors; boxes, panels, stacks, rows, grids, toolbars and dividers; badges, callouts and empty states; tables, lists, dropdowns, tooltips, dialogs, tabs and disclosures; and a small first-party SVG icon set.
"- [x]" (2026-08-07) Native date/time/datetime-local fields need no new component — `.lynx-input`/`.lynx-textarea`/`select.lynx-select` already style by class, not input type. `LynxUI.formatDateTime(iso, { relative })` adds the compact absolute (same-day time-only, else date+time) and relative (`Intl.RelativeTimeFormat`) display for Record work metadata and Kanban card metadata.
"- [x]" (2026-08-07) `LynxUI.combobox(container, { getOptions, getLabel, getValue, onSelect })` — a live-filtered `.lynx-combobox`/`.lynx-menu` autocomplete, arrow/Enter/Escape keyboard nav, single-select. Instantiated per element (unlike the delegated-click components) since its option list is usually Protein-backed and built at query time. Record's predicate/object inputs and Kanban's column/Concept pickers are this with a different `getOptions`.
"- [x]" (2026-08-07) `LynxUI.tokens(container, { getSelected, getOptions, onAdd, onRemove })` — composes `combobox` + `.lynx-status` badges (`.lynx-tokens`) into a removable token picker, no separate data model; the caller stays the source of truth via `getSelected()`/`refresh()`. For Record assignees, selected assertions, thread predicates.
"- [x]" (2026-08-07) `LynxUI.attachmentRow(name, { busy, onRemove })` builds the `.lynx-attach__row` shape (name, busy spinner or remove button); Record owns what an attachment means and its preview, this only owns the row.
"- [x]" (2026-08-07) `.lynx-meta` (`dl`/`dt`/`dd`) — a flat key–value grid, not a panel, for Record head/slug/quantity/work facts and Kanban card metadata.
"- [x]" (2026-08-07) `.lynx-duration` + `LynxUI.formatDuration`/`parseDuration` — minutes underneath (Record's estimate/worklog unit), "1h 30m" is how it displays and how free text like "90" or "1h30m" parses back.
"- [x]" (2026-08-07) `LynxUI.confirmDialog({ title, body, confirmLabel, danger })` — a destructive-confirmation composition of the existing `.lynx-dialog`, not a new component; returns `Promise<boolean>`, Escape and Cancel both resolve `false`. For Kanban bulk deletion and Record hard deletion.
"- [x]" (2026-08-07) `LynxUI.openMenu(x, y, items)` / `LynxUI.contextMenu(target, buildItems)` — the same `.lynx-menu` a dropdown already shows, anchored to a point (right-click, or Shift+F10/ContextMenu for keyboard parity) instead of a fixed sibling; closes on outside pointerdown or Escape. For Record links/attachments and Kanban cards.
"- [x]" (2026-08-07) `.lynx-spinner` (static ring — no animation, per the standing motion rule) and `.lynx-progress` (native `<progress>`, restyled) for Record saves, uploads, and Kanban moves; transient notifications, avatars, and range sliders stay out of scope until a Sand needs them.
- [x] Static components use native markup and classes. Interactive components use `data-lynx-*` attributes and one delegated event and keyboard handler per iframe. `window.LynxUI` provides icon and icon-button helpers for dynamic elements.
- [x] Add a development-only LynxUI Gallery with one scrollable page: a compact showcase of all LynxUI components sits beside a stacked set of seeded Kanban, message, inventory, and request-review Sand previews at their normal board sizes. It uses canonical assets and fixture data without Protein or Action requests. `mise run lynxui` serves it at `http://127.0.0.1:6175` and recompiles the gallery package for Lince on source changes.
- [x] Add a folded base control that highlights LynxUI components and shows their component names on hover. Sand-specific structure stays unmarked so the boundary is clear.
A Chart Sand is not a prerequisite library project. Add the smallest chart
renderer when a concrete Protein projection or workflow demonstrates a need,
and package any vendored dependency with its license and credits.

## What is left

### Customization

#### C0 — Freeze the contract and toolchain

- [ ] Freeze the product horizons before implementation: v1 contains the
      productivity Box, current Protein, composable Sands/Castles, Areas,
      Customization and external HTML; v2 contains the world-model direction.
      Name the permanent v2 seams exercised by v1 and reject feature work that
      quietly pulls globe, CAD or capture scope into the v1 Box.
- [ ] Inventory every non-vendored visual value in the base Web surface,
      LynxUI, and official Sands. Classify each as a palette primitive,
      semantic token, component alias, user-owned content, runtime layout, or
      accidental hardcoding. Publish the inventory and remove synonyms before
      migration starts.
- [ ] Define one canonical `--lynx-*` namespace and the complete first token
      families: palette, surface and ink, intent and truth, focus and
      selection, spacing and density, size, border, radius, typography, icon,
      elevation, opacity, stacking, and permitted direct-manipulation motion.
      Define default, hover, focus-visible, active, selected, disabled,
      read-only, invalid, loading, and empty states where they apply.
- [ ] Define a versioned theme manifest with required and optional tokens,
      modes, local extension slots, supported asset references, and safe
      failure. A partial theme inherits; an unknown contract version, invalid
      value, remote asset, or undeclared executable content fails closed.
- [ ] Freeze and test the scope cascade: default contract, LynxUI rules, Sand
      structural rules and definition defaults, active global style/mode,
      workspace overrides, group/Castle overrides, and Sand-instance
      overrides. An isolated root receives a resolved, inspectable token set;
      it does not grow a second cascade with different semantics.
- [ ] Freeze the final-board stack after running the Plan A proof in
      "Runtime plans and Plan B HTML alternatives." Decide compositor
      ownership, the Lince `winit`/`wgpu` shell, GPUI fork/pin, the selected
      Bevy features and custom render passes, CEF texture and process model,
      browser/Facade renderer, packaging, and fallback from measured evidence.
      Prove one event loop, device/queue topology, final compositor, frame
      coordinator and input router; no adapter may start a competing owner. If
      Plan A cannot pass correctness, security, performance, visual-quality or
      defined-ownership gates even with the accepted implementation and fork
      work, select the preserved Maud/HTML-first Plan B and remove incomplete
      native production paths. Cost alone is not failure. Do not maintain two
      desktop runtimes indefinitely.
- [ ] Freeze JavaScript ES-module Behavior for Plan B and installed external
      HTML: source files are browser assets, imports and exports are explicit,
      inline scripts and HTML event handlers are removed as their surfaces are
      rebuilt, and ordinary HTML Sand Behavior needs no frontend compilation.
      Plan A first-party native Behavior uses Rust behind the same ports. Add
      syntax, import-graph, module-boundary, CEF, and browser smoke checks.
      Treat only Plan B's pinned `wgpu` renderer and measured, selected compute
      kernels as the explicit Wasm/generated-loader exception; give them a
      reproducible build, Wasm-target `cargo check`, source/artifact hash check,
      source maps,
      and browser smoke test.
- [ ] Freeze the logical Sand runtime ABI and its adapter projections: native
      Rust systems and GPUI view state, retained world-scene handles, a
      validated size- and rate-bounded CEF bridge for installed HTML,
      wrapper-only Website ports, Plan B scoped DOM/`MessagePort` calls, and an
      optional generated WIT projection for Wasm Behavior. The Rust model and
      shared fixtures are authoritative; no adapter passes raw DOM, GPU
      objects, pointers, credentials, or global Box state through the portable
      boundary. Prove an installed CEF Sand can consume Protein and emit a
      `record-clicked` Box event without giving a Website the same authority.
- [ ] Freeze the renderer/physics ownership contract: the chosen bounded GPU
      device/compositor topology, manual Bevy render-resource integration,
      normalized GPUI input/offscreen output, stable visual-node uids, partial
      instance-buffer writes, cameras and viewports, spatial broad phase,
      state-based dirty/awake sets, group-level coarse bodies, bounded diffs,
      deterministic teardown, device-loss recovery, and accessibility trees.
      Camera culling may omit presentation only and cannot suspend or throttle
      physics, Behaviors, games, media, CEF, Protein, or events. GPU compute and
      shared memory remain absent until measurements justify exact kernels.
- [ ] Under Plan B, freeze Maud as the standard structural authoring path for
      first-party HTML Sands. Define paired constructors that emit accessible
      markup and the same definition nodes, stable child uids, ports,
      configuration, and Behavior bindings produced by Box. Under Plan A,
      native GPUI/world constructors pair renderer implementations with those
      same nodes. Maud output remains ordinary HTML; raw and external HTML
      remains supported; no Maud runtime or third-party Rust requirement is
      introduced.
- [ ] If Datastar wins the spike, replace rather than weaken the incompatible
      decisions: define its exact core/browser and Rust SDK pins, vendored MIT
      license/credits, allowed attribute/action subset, per-instance signal
      namespace, one board stream, typed-Action adapter, morph ownership,
      execute-script rejection, teardown, and explicit Facade CSP. Pro/Rocket
      is excluded under its current commercial redistribution terms. If it
      loses, keep no runtime, SDK, bootstrap, schema field, or compatibility
      branch for it.
- [ ] Freeze the Sand artifact boundary: normalized definition graph, rendered
      fragments, native ES modules, CSS, Wasm modules, loader glue, shaders and
      other assets, manifest, capabilities, provenance, content hashes,
      LICENSE/NOTICE, and credits. A first-party artifact cannot be registered
      when these disagree, when generated Wasm artifacts are stale, or when a
      declared child exists only in markup.
- [ ] Generate versioned machine-readable schemas and shared valid/invalid
      fixtures from the authoritative Rust model. Make JavaScript boundary
      decoders consume them and prove unknown versions, operations, fields, and
      malformed values fail closed in release builds.

#### C1 — Themes and configuration

- [ ] Let people live-edit padding, margin, gap, border radius, thickness,
      typography, colorscheme, information density, Sand Behavior, and Box
      presentation. Official Web UI contains no hardcoded design color; it
      uses documented semantic variables from the default style.
- [ ] Architecture (from Sand: Colorschemes): The system is defined as named semantic tokens, not hex values — surface-raised, ink-primary, need, contribution, accent, focus — resolved per active colorscheme at runtime (the old a2 Operation to switch schemes is the spiritual ancestor). Scaling tokens (padding-s, radius-m) ride the same mechanism. A Sand author never picks a color; they name a slot, and the user's scheme decides what it looks like. That's how "the base app is minimalist so users can express themselves" survives contact with real widgets.
- [ ] The default style is always loaded first and defines every variable.
      User style files are optional overrides: when a variable is absent, the
      value from the default style remains. Every variable is explained in the
      generated token reference beside the source; source CSS contains no
      comments.
- [ ] Style files live in the Web styles directory and are selected by safe `.css` filename only. Invalid, missing, or unreadable files fall back to the default without preventing the app or a Sand from loading.
- [ ] Styles load in this order: default tokens, LynxUI component rules, Sand
      structural CSS and definition-local semantic defaults, active mode and
      global style, workspace overrides, group/Castle overrides, then Sand-
      instance overrides. Choosing "inherit" removes an override rather than
      copying the inherited values.
- [ ] The configuration table selects the global style and applies it immediately. The Sand gear configuration, together with login, Protein, and behavior, shows the inherited global style and selects an optional style for that Sand. Both choices persist.
- [ ] Every isolated HTML Sand root receives resolved style layers itself
      because CSS variables do not cross a CEF document, iframe, or process
      boundary. Trusted Sands sharing a composition root inherit directly.
      Native adapters receive the same resolved values as typed style data. A
      global change updates every inheriting Sand without replacing local
      overrides.
- [ ] Colorschemes use an open set of named semantic slots rather than a
      fixed 16-slot/48-variable ceiling. The default begins with
      `primary-background`, `secondary-background`, `raised-background`,
      `primary-ink`, `secondary-ink`, `border`, `accent`, `focus`, `need`,
      `contribution`, `peace`, `info`, `success`, `warning`, `danger`, and
      `selection`. A slot may define light/default/dark or other named modes;
      omitted values inherit, and intentional repetition is valid. A Sand may
      introduce documented local slots at its own boundary.
- [ ] The default colorscheme is Lynx with Dark Lynx as its initial mode and Light Lynx as its inverse. Dark Lynx uses Deep Lead for the background and Ice White for primary characters; Light Lynx reverses them. A single icon button switches mode immediately; both modes use the same scale, geometry, and component rules.
- [ ] Lynx derives close neutral steps from Deep Lead and Ice White. Default
      component backgrounds remain Deep Lead or Ice White; the 10% lighter and
      darker variants are used only for subtle inputs and restrained shadows.
      Default content does not assign red, green, amber, or another semantic
      color to data. Purple Cobalt is Light Lynx's primary accent and
      Nocturnal Purple is Dark Lynx's; the other purple supports it.
- [ ] Advanced developer mode exposes raw Sand and workspace CSS, token
      creation, Behavior and port editing, and an inspectable live preview.
      Invalid edits are isolated and reversible and never prevent Lince from
      reopening the workspace.
- [ ] Raw customization CSS remains presentation-only and scoped below the
      workspace or Sand root. Reject external URLs, `@import`, executable or
      browser-extension schemes, selectors that escape the declared root, and
      rules that cover or imitate the host security/configuration corners.
      Preview in a disposable boundary before adoption and retain a one-step
      safe-mode reset outside the customized root.

#### C2 — Lynx visual and behavior grammar

- [ ] Grid: 4px base unit; spacing scale 4, 8, 12, 16, 24, 32, 48. Compact component internals may use the 2px half-unit.
- [ ] Density: slim by default. Main content regions touch with no decorative gaps or outer padding. Component padding is half the previous demo spacing. Records use 5px internal padding; form controls use 3px vertically and 5px horizontally so empty space does not exceed the text height.
- [ ] Keep a compact 4px gap between text and adjacent metadata in the same compartment, such as a column name and its card count.
- [ ] Borders: use 0.5px hairlines only when needed to show where one region ends and another starts. `.lynx-shadow` is a small tokenized CSS shadow on an individual component’s own root; it follows that element’s shape and radius exactly, has no JavaScript or wrapper cost, and may be locally directed with `--lynx-shadow-x`, `--lynx-shadow-y`, and `--lynx-shadow-blur`. Its default is dark and falls to the right and bottom. `.lynx-shadow--light` adds an optional lighter top-left companion. Buttons, Kanban cards, Kanban column headers, and the message channel-list boundary opt into the dark shadow. Inputs, textareas, and dropdown controls are flat with a foreground border. Outer Sands and large workflow regions do not use shadows. Adjacent regions share one boundary; no double borders and no boxes inside boxes. Sand outer edges have no border by default on the free canvas. Stronger focus and active state remain distinct from ordinary borders.
- [ ] Roundness: Lynx is square by default. Badges and buttons use a restrained 2px radius. Custom styles may change the radius tokens.
- [ ] Badge is the component name and keeps the `.lynx-status` class/API. It has a transparent background and primary foreground hairline border by default; its optional icon uses the same color. The component does not interpret data as good, bad, warning, or any other semantic hue. A Sand may locally set `--status-color` when its own domain calls for it.
- [ ] Invalid inputs keep the error-color border and show an in-field error icon. Hovering or focusing that icon reveals the validation message in a transparent, error-color outlined tooltip; the message also remains associated with the input for assistive technology.
- [ ] Buttons use a subtle 2px radius. Use familiar, distinct action icons such as plus, check, close, save, download, and trash; keep text when the icon alone is ambiguous.
- [ ] Radio controls are minimal circles filled completely with the accent when selected. Dropdowns, selects, and disclosures use one small chevron treatment. LynxUI selects use the library menu instead of a browser-styled popup.
- [ ] Rigidity: "firm paper." Cards hold their shape with crisp hairline borders; nothing bounces, nothing elastic.

- [ ] Prefer content directly on the Sand surface. A card is a card, not a floating card inside another box; a column is a column, not a box containing another padded box.
- [ ] Remove redundant headings and labels. Do not show both "Sand / Communication" and "Messages", or text beside a self-explanatory icon.
- [ ] Omit a Sand header when the content already explains itself. Keep visible controls focused on creating, editing, moving, completing, or replying to Records.
- [ ] Only the focused Sand shows its gray bottom-right corner. Hovering or focusing the corner reveals configuration, Protein, layout, and other Sand controls like the corner of a page turning.
- [ ] The base web uses an always-visible gray folded top-right corner for system controls. It holds the mode switch and development tools without adding a persistent header; Sand controls remain in their bottom-right corners.
- [ ] Do not add dividers when content already makes the boundary clear. Message identity and avatar separate consecutive messages without a line.
- [ ] Use the darker generated black-and-white step, never gray or the lighter step, for background separation. Kanban, Messages, and workflow regions share the canvas background without outer shadows; only individual components may opt into the shared shadow.
- [ ] Do not divide Kanban columns, column headers from their first card, cards, table rows, list rows, or ordinary adjacent items with lines. Use grouping, spacing, and close surfaces only when a boundary needs to be understood.
- [ ] Inputs, textareas, and closed dropdown controls use the base surface with a foreground border and no shadow. Invalid controls use the `hot-border` token, red in default Lynx. Unchecked checkboxes use the base surface; checked checkboxes use the accent.
- [ ] Hovering an element on the base surface uses the subtle 10%-lighter surface. Hover never darkens the current base surface.
- [ ] Forms, tables, lists, and adjacent workflow regions align to shared edges. Do not use spacing that makes neighboring compartment boundaries stop at different positions, and omit row or field lines when grouping is already clear. A separator belongs between items, never after the final item.
- [ ] Prefer clear icons for actions and give every icon button an accessible name and a tooltip on hover. Keep text when an icon would be ambiguous.
- [ ] Keep tooltips within the boundary of the Sand or component that owns them, and give SVG strokes enough internal view-box space that icons are never clipped.
- [ ] When the Sand is idle, the visible UI prioritizes Records, their state, and direct interaction with them rather than configuration of the Sand.

- [ ] Data surfaces are always opaque. Honesty rule: you must always know exactly which surface a number sits on. No glassmorphism, no frosted panels over content.
- [ ] Translucency is allowed only for ephemeral chrome: edit-mode handles, drag previews, presence cursors, auto-hiding call UI — things that are explicitly not Ledger truth.
- [ ] Overlays/scrims at 50–60% ink. Menus are opaque. Tooltips use Deep Lead with Ice White text and a hairline border in that same foreground color; dialogs use a Gray border.
- [ ] Elevation is flat by default. `.lynx-shadow` is the sole shared elevation utility and is opt-in except for ordinary buttons, which use its default dark lower-right shadow. Form controls remain flat with a foreground border. Paper doesn't hover.

- [ ] A deliberate second channel so color is never the only carrier: Solid = settled (Ledger facts, committed quantities). Dashed = declared (promises, projections, staged rules). This is as load-bearing as any hue.

"- [ ] Numbers first: tabular figures everywhere quantities appear, negative quantities use a true minus (−3), and zero and positive quantities have no sign" (0, 5). The zero state is styled quietly — peace is the one value that should never demand attention.
- [ ] Lato is the default body and interface typeface. Aleo is used mostly for titles and semantic headings. Quantities and technical metadata keep the monospace token. EN/PT is supported from day one, with generous line lengths and no cramped all-caps labels.
- [ ] Ordinary interface text is 14px by default. Compact secondary metadata stays readable at 11–12px; do not shrink routine labels or content to create density.
- [ ] Inline icons, counts, and quantity-state marks are optically centered with the adjacent text. Correct a glyph inside its SVG when its drawing is off-center; do not move the entire control.

- [ ] Remove ornamental interface transitions, pulsing, hover movement,
      startup drawing, workspace sliding, animated modal entrances, and
      JavaScript that waits for transition completion. State changes are
      immediate and never rely on green alone to mean synced or healthy.
- [ ] Direct manipulation may use motion when motion conveys the physics or
      result of the person's action: pan inertia, drawing, and current spatial
      forces qualify; future topology may qualify if revived. It must remain
      interruptible, respect reduced-motion preferences, and settle instead of
      bouncing forever. Games, terminals, and artwork own their content motion.

- [ ] Every design decision gets one question: does this get Ana to her four minutes of human choice faster, or is it the tool asking to be looked at? The Death of Lince applies to its UI first. If the user wants, they can use lince as an app that lets them create and interact with beautiful and cool things and produce awesome graphs and automation visualizations, but that is a choice, the default of lince is meeting your need to use apps like it with minimal effort.

#### C3 — Merge LynxUI and Sand composition

- [ ] Define the recursive Sand contract before touching Box: definition uid
      and revision, instance uid, stable child uid, referenced definition,
      typed inputs/outputs/events, exported ports, Behavior, local layout,
      configuration schema, token defaults, instance/group override patches,
      renderer reference and binding kind, isolation, capability requirements,
      and content-addressed assets.
- [ ] Make each reusable LynxUI primitive available through that Sand contract
      without wrapping every trusted child in an iframe. Its native semantic
      element and accessibility behavior remain LynxUI's implementation; its
      identity, ports, state planes, configuration, and composition are Sand.
- [ ] Under Plan A, implement native Rust/GPUI and world-renderer constructors
      for those primitives and compound layouts, each paired with its Sand
      node. Under Plan B and for HTML-backed variants, implement the equivalent
      Rust/Maud constructors returning markup paired with that same semantic
      node. Nesting a Button in a panel or workflow records the same stable
      child and port graph that Box edit mode would create in either renderer.
- [ ] Make paired constructors stamp stable local node identities while the
      runtime creates instance-scoped DOM ids and repairs label/ARIA
      references. Runtime wiring uses the scoped node map; definitions and
      connections never persist global ids or CSS selectors.
- [ ] Implement one Rust-owned composition host as the sole semantic consumer
      of normalized definitions regardless of authoring origin. Under Plan A it
      selects GPUI, world-renderer, installed CEF, or Website CEF adapters;
      under Plan B its browser projection selects trusted DOM, shared `wgpu`,
      or isolated HTML adapters. It resolves exact revisions, applies
      configuration and overrides, wires ports, starts scoped Behavior, and
      tears down every listener, renderer handle, task, process bridge, and
      capability handle with the instance. JavaScript is an adapter runtime,
      not a second composition authority.
- [ ] Prefer inspectable declarative Behavior for typed Actions, event
      forwarding, local-state changes, and simple field mappings. Add native ES
      module Behavior only behind content-addressed assets, declared typed
      ports/configuration/capabilities, validated scoped context, and explicit
      teardown; never persist inline handlers, selectors, or JavaScript source
      strings in a definition.
- [ ] Implement one recursive composition operation for ordinary groups,
      locked groups, Protein result templates, and saved compound Sands. A
      saved compound group may be called a Castle in the interface, but no
      `castle` kind, storage table, renderer, or special event semantics may
      appear.
- [ ] Give every group stable local coordinates and child ordering; connections
      target stable child ports rather than selectors or copied HTML. A group
      may export selected child ports, and unexported internal events remain
      sheltered. Lock/unlock changes editing affordances only.
- [ ] Support save-as-definition, instantiate, edit shared definition, apply
      instance override, reset to inherited, fork/detach, and inspect lineage.
      A definition update changes every reference while preserving valid local
      patches; a bad revision leaves instances on the last known-good one.
- [ ] Build the composition workbench outside the spatial board. Use it to
      place the same Button Sand alone and inside a video-call compound Sand,
      nest a compound Sand again, wire typed events, alter each customization
      scope, save/reopen it, and visibly inspect the resulting tree and ports.
- [ ] Author the same Button/Video Call fixture once in the selected Plan A
      native constructor path, once in Plan B Rust/Maud, and once in the
      workbench. Their normalized graphs, ports, and Behavior meaning must be
      equivalent even when their renderer projections differ. Opening a
      code-owned fixture in Box permits overrides and a visible fork, not
      mutation or regeneration of Rust source.
- [ ] In edit mode, Protein-to-Sand read bindings are always visible as arrows
      from the Protein source/field to the exact child input. Write ports and
      Actions have a distinct direction and visual treatment so a data arrow
      can never imply mutation authority.
- [ ] Finish the Configuration Sand on the same composition primitives. It
      must edit styles, modes, tokens, density, definition defaults, group and
      instance overrides, Behavior, ports, isolation, and capabilities with
      live preview, undo, reset-to-inherited, and honest invalid states.
- [ ] Prove native view-state ownership in the composition workbench:
      selection, preview, inherited/overridden indicators, compatible ports,
      and validation state have explicit owners and teardown. Nest and destroy
      inspected Sand instances without leaked listeners, stale references, or
      state that can be mistaken for durable Protein or Box data.

#### C4 — Rebuild the existing interface before Box

- [ ] Migrate the board, shared components, and every official Sand from hardcoded visual values to the semantic color, spacing, border, radius, typography, elevation, and motion variables. User-owned expression colors remain data, not system chrome.
- [ ] Migrate every official Sand to LynxUI for ordinary interface elements and remove duplicated component CSS. Specialized graphs, terminals, games, document rendering, canvases and artwork keep their own implementation; their ordinary surrounding controls use LynxUI when practical.
- [ ] Under Plan A, migrate first-party-owned interface structure to native
      GPUI/world implementations paired with Sand definitions. Under Plan B
      and for HTML-backed variants, migrate monolithic embedded HTML to small
      Maud functions and paired Sand constructors wherever the structure is
      meant to compose or be maintained by Lince. Keep HTML CSS and JavaScript
      as separate package assets. Do not rewrite third-party, vendored,
      imported, or deliberately opaque documents merely to claim Maud coverage.
- [ ] Replace bespoke workflow markup with referenced primitive and compound
      Sands where the composition is genuine. Preserve specialized internals,
      but make shells, controls, ports, configuration, and state planes use the
      same contract. The ready-made Kanban and video call remain convenient
      compound definitions, not sealed exceptions.
- [ ] Refactor first-party JavaScript into small native ES modules as each
      owning surface is rebuilt. Split the current board host by contract,
      state ownership, and responsibility instead of moving the same large
      implementation behind a new entry file.
- [ ] Delete copied instance HTML, duplicate component CSS, stale token
      aliases, and replaced APIs as their callers move. No compatibility layer
      or dual component vocabulary remains.
- [ ] Publish a versioned LynxUI/Sand-author contract from the first external
      release. Official and user-authored Sands load the same declared version;
      an incompatible version fails closed. A version bump replaces the old
      contract rather than serving side-by-side compatibility or silently
      interpreting an obsolete shape.
- [ ] Remove `LynxDS-components.js` and rebuild remaining official callers on
      the schema-validated LynxUI/Sand modules. `window.LynxUI` may exist as a
      temporary gallery/bootstrap bridge during migration, but it is not the
      external author contract and is gone at the completion gate. Do not
      retain a compatibility layer for persisted legacy shell HTML.

#### C5 — Enforcement, authoring, and completion gate

- [ ] Add a design-system check that rejects hardcoded governed colors,
      spacing, radii, borders, typography, shadows, stacking values,
      transitions, and animations in the Web interface and official Sands.
      Exclude canonical default definitions, vendored assets, user-owned
      expression values, measured runtime layout, and declared specialized
      internals.
- [ ] Test default fallback, optional partial styles, safe filename handling,
      every override scope, inherit/reset, global persistence, per-Sand and
      group persistence and isolation, live changes, and resolved style loading
      inside iframes.
- [ ] Test quantity formatting, tabular figures, solid and dashed truth lines,
      and that status meaning is available through an icon, label, shape, or
      line style instead of color alone.
- [ ] Components have keyboard navigation, focus management, ARIA state, associated help and errors, non-color status meaning, and no transitions or animations.
- [ ] Treat specialized exceptions as a code-review convention, without manifest declarations or exemption attributes.
- [ ] Add concise theme- and Sand-author documentation, the generated token
      reference, component and composition behavior tests, iframe tests, theme
      override tests, schema fixtures, and the minimal external-author test
      kit.
- [ ] Test module teardown, stable instance identity, focus preservation,
      and bounded DOM work with at least 200 Sand instances. Confirm no view
      store can submit unrelated state, outlive its Sand root, or bypass
      validated Actions and Box operations.
- [ ] Verify release and development serve the same first-party JavaScript,
      every embedded import resolves, ordinary Sand Behavior needs no frontend
      compile, and browser diagnostics point to editable modules. Verify the
      separate `wgpu` Wasm artifact is reproducible, current, checked for its
      Wasm target, debuggable, licensed, and absent from a DOM-only Facade that
      does not use the world renderer. Facade output needs no inline-script or
      `unsafe-eval` CSP permission.
- [ ] Run the native visual-character gate at 1×, fractional scale and
      2×/4K-class density. Review still captures and interaction recordings for
      sharp text, stable hairlines, consistent effective pixel density, quiet
      hierarchy and absence of game-default styling. Record p95/p99
      input-to-present latency while Sands move and while a world pass animates.
      Passing either the subjective review or latency measurements alone is
      insufficient.
- [ ] Do not open the Box implementation stages until the Gallery and
      composition workbench pass in Dark Lynx, Light Lynx, a partial custom
      theme, reduced motion, keyboard-only use, an isolated root, after a
      save/reload, and through the native visual-character gate. At that point
      the board is a consumer of completed foundations rather than the place
      where they are debugged.
} r_4Z5VS9MJNRDHZGXW4KMGRNXMHK
