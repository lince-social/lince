# Behavior source and execution boundary

Purpose: Define native Bevy Behavior and the separate validation rules for existing external-browser boundaries.

Owner source: [Interface in Lince](../Lince.lingua); no separate Customization or
Sands Record currently exists.

Status: Bevy-native implementation selected on 2026-09-07; browser material below applies only to external publication or historical packages.

Read when: changing Behavior modules, browser assets, Wasm renderer packaging, or CSP expectations.

[Corpus map](README.md) · [Current context](current.md)

---

### Native Bevy Behavior

Use Bevy systems, observers, messages, components and resources directly.
Simple `.on(...)` effects expose named typed operations that Box can inspect
and persist. Richer trusted native behavior can be Rust; it need not pass
through a portable module ABI or a JSON message for each widget operation.

The owner chose this simpler extension model for current work on 2026-09-07:
people compose registered components and named effects; reviewed/trusted Rust
plugins add native capabilities. Arbitrary untrusted executable installation,
a scripting runtime and its sandbox are deferred to an explicit later design.
Do not require them to finish editable Sands or treat a manifest as isolation.

Sand ownership, movement attachment and exported event scope are distinct.
Bevy pointer bubbling is useful for UI interaction, but crossing a Castle's
logical boundary still requires an exported Sand route. Durable writes still
use backend Actions and permissions; native Rust plugins are trusted code,
not capability sandboxes. The existing backend, saved-data and network
boundaries retain validation and may have narrow translators.

Lince plugins decide when work is due. Use Bevy's reactive loop, real timer
deadlines, targeted Protein updates and animation completion to stop needless
presentation work. Camera culling may reduce visuals, not suspend Behavior,
media, subscriptions or admitted physics.

No JavaScript engine, WIT layer, paired Maud fragment or renderer-neutral
Behavior runtime is required for new native code. Custom plugins,
internal/external crates or WGPU passes are scoped exceptions under
[Architecture](architecture.md#extensions-and-exceptions).

### Existing external-browser JavaScript Behavior

These browser execution and Wasm packaging rules apply to externally viewed projections, not to the native desktop. [Part A](plans/part-a.md) uses native Rust and pure package/export validation; it does not acquire a JavaScript or Wasm runtime requirement because a retained external package contains those assets. Embedded execution and its live parity tests do not return: [the build rule](build.md#no-embedded-browser) removed the browser they ran in, so installed HTML needs an execution story that does not run HTML inside Lince. A general browser client remains outside the product plan.

The earlier Plan B is not a second native implementation. JavaScript rules
below apply only where an existing or explicitly selected external-browser
export actually executes it. Installed HTML has no runtime inside Lince.
Native Sands use Bevy Rust implementations and do not carry a parallel
JavaScript version to prove portability.

A future Bevy web target or external HTML exporter has its own packaging
checks. Do not prebuild a Worker physics layer or standalone WGPU Wasm
renderer for the native interface. A browser client for using a Cell remains
unplanned. External projection never gains native World or device access.

This keeps the asset path direct:

- ordinary HTML, CSS, and Behavior edits need no frontend transpilation or
  JavaScript bundle step;
- browser stack traces and development tools point at the editable source;
- external-browser modules and their author examples use the same runtime
  language; native Bevy Behavior does not;
- module loading, source order, and content-security policy are visible rather
  than being transformed by a hidden build stage;
- Node remains a testing and development tool rather than a prerequisite for
  compiling the Lince binary.

If an external delivery actually selects Wasm, its release packaging must
compile and optimize that target and loader, with a focused `cargo check` for
that target. Native interface work has no such build requirement. Checked-in or packaged artifacts must never conceal a stale
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
- external-browser behavior stays out of inline scripts and HTML event attributes
  into small ES modules with explicit imports, exports, ownership, and teardown.
  This also permits a strict Facade content-security policy;
- vendored libraries remain in their distributed JavaScript with licenses and
  credits, behind small validating adapters;
- external Sands ship JavaScript and are bound by the versioned manifest,
  runtime schemas, ports, and capabilities. Their internal authoring tools are
  irrelevant to Lince and do not become part of its build.

The historical C2 prototype exercised this external boundary; it does not
prescribe a native Bevy lifecycle or certify a currently available HTML runtime. The recursive
video-call definition declares a content-addressed native ES module with
separate `mountComposition` and `teardownComposition` exports. The Rust host
tracks its Behavior handle beside native renderer handles; the Installed
adapter removes its delegated listener and clears its scoped roots before
remount. Declarative nested output, routed Sand event and granted Action write
remain typed and counted independently. The release report observes both
active and retired Behavior handles.

Native JavaScript Behavior does not forbid a future bundling step if measured
request or packaging costs justify one. Bundling would remain an asset
optimization, not a source-language change or a place to hide contract
behavior. Any future external renderer build remains separate from ordinary
native Bevy Behavior.
