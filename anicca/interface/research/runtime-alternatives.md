# Runtime and HTML authoring alternatives

Purpose: Preserve the Maud, raw HTML, Datastar, renderer, physics, and composition evaluation that led to the accepted architecture.

Owner source: [Interface in Lince](../../Lince.lingua); no separate Customization
or Sands Record currently exists.

Status: Reference evidence. Accepted choices live in architecture, behavior, and current context; do not reopen alternatives without new evidence.

Read when: a measured failure challenges an accepted stack decision.

[Corpus map](../README.md) · [Current context](../current.md)

---

### Runtime plans and Plan B HTML alternatives

Plan A is the GPU-first native prototype described in
[Interface](../architecture.md#plan-a-gpu-first-native-prototype): a Lince-owned
`wgpu` compositor, retained Lince native UI, a replaceable 2D/3D world-engine
adapter, and accelerated CEF surfaces for real external HTML. Plan B is the
preserved Maud/HTML-first browser hybrid. The plans share the Sand graph and
token contract; the following HTML authoring/reactivity comparison chooses the
Plan B implementation and the HTML-backed adapters that remain present under Plan A.
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
`Lince.lingua`: small and ready-made Sands compose into Castles, receive
Protein, issue typed Actions, exchange events, expose editable properties, and
remain customizable in Box. A stack is rejected if it can make a reactive page
but cannot make those relationships visible and reusable.

#### Invariants independent of stack

The stack does not get to redefine these contracts:

- The semantic `SandDefinition` graph and the versioned Box document are the
  persistent truth; neither rendered DOM nor a reactive signal store is
  persisted as the composition. Renderer projection manifests remain separate
  from semantic identity and Behavior, and selected adapter handles remain
  disposable runtime state.
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
[Sands](../sand-model.md#one-definition-graph-several-authoring-paths). The sections
below follow one Sand from source to the final running Box and record where each
approach helps or charges complexity.

#### Plan A native renderer and customization contract

Plan A resolves the same token cascade once and projects it into each renderer:

| Implementation | Runtime binding | Presentation boundary |
| --- | --- | --- |
| Native application or control Sand | Rust systems and typed retained-UI state | Display work contributed to the host frame assembly |
| Box/world/game/map/GPU Sand | ECS entity and retained renderer handle | Shared native `wgpu` world scene |
| Installed external HTML Sand | Validated CEF process bridge with complete declared ports | Accelerated CEF texture imported into the compositor |
| Website Sand | Host-owned wrapper ports only | Isolated CEF request context/profile and browser surface |
| Browser or Facade adapter | Native ES modules and/or shared Wasm world renderer | Ordinary browser HTML plus canvas/iframe surfaces |

The native host has one accepted rendering device, queue/submission policy,
frame coordinator and final compositor. A CEF GPU process may produce an
external surface from its isolated graphics context; the adapter imports or
copies it with explicit synchronization and no framebuffer CPU readback. This
does not make a Chromium device or context part of the Sand definition and
does not weaken the one-host-owner rule.

Primitive and semantic tokens remain canonical serializable values. The native
adapter converts resolved roles into retained node styles, packed instance
data, text styles, shader uniforms, and renderer resources. The installed-HTML adapter
exposes allowed values as instance-scoped CSS custom properties. A Website can
customize only its Lince-owned wrapper unless the remote origin voluntarily
supports an ordinary Web theming API; Lince never injects styling into an
arbitrary cross-origin page.

Native and HTML implementations of the same built-in definition share identity,
ports, state planes, configuration, Behavior meaning, and test fixtures. Pixel
identity is not required across native UI, world GPU, and browser text rasterizers, but the
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
interface toolkit. Ordinary Sands, legacy LynxUI controls, Record text, forms,
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
  common legacy LynxUI primitives, while specialized modules retain direct listeners
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
