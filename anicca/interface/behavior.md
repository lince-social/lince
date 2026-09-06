# Behavior source and execution boundary

Purpose: Define how native Rust and shipped JavaScript implement the same logical Sand Behavior without TypeScript, bundler, or framework requirements.

Owner source: [Interface in Lince](../Lince.lingua); no separate Customization or
Sands Record currently exists.

Status: Accepted boundary; Plan B remains browser/Facade projection rather than Linux desktop fallback.

Read when: changing Behavior modules, browser assets, Wasm renderer packaging, or CSP expectations.

[Corpus map](README.md) · [Current context](current.md)

---

### Plan B and installed-HTML JavaScript Behavior

These browser execution and Wasm packaging rules apply to enabled browser projections, not to the default native desktop. [Part A](plans/part-a.md) uses native Rust and pure package/export validation; it does not acquire a JavaScript or Wasm runtime requirement because a retained external package contains those assets. Embedded execution and its live parity tests return in [the final CEF lane](plans/cef.md). A general browser client remains outside the product plan.

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

C2 exercises this boundary rather than leaving it aspirational. The recursive
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
behavior. Plan B's `wgpu` Wasm pipeline is a renderer build, not precedent
for generated implementations of ordinary Sand Behavior.
