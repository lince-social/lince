# Sand implementation plan

Purpose: Define production Sand schema, authoring, composition, package, HTML, Website, and migration work.

Owner source: no dedicated Sands Record currently exists;
[Interface](../../Interface.lingua) governs shared interface decisions.

Status: Coordinated with Customization C3-C5; semantic kernel, primitive
Gallery and recursive composition landed, and Box follows the gate.

Read when: implementing the Sand contract or external runtime surface.

[Corpus map](../README.md) · [Current context](../current.md)

---

## Landed C1 boundary

Sand schema and ABI version 1, persisted/runtime separation, projection
manifests, package/hash/license validation, generated JSON Schemas and
valid/stale fixtures have landed. The 19-definition retained Gallery and its
Installed CEF HTML/CSS/ES-module projection use that contract; the Website
companion retains no Lince authority. Keyboard, pointer and AccessKit actions
reach the retained state, a Protein-shaped mount reaches Installed HTML and
`record-clicked` returns through its exact event grant. Release parity and the
joined Wayland report pass. Package admission resolves only declared relative
static module imports, and the live Installed decoder proves malformed
unknown-field refusal before accepting a valid mount.

## Landed C2 boundary

Composition schema version 1 adds the exact-revision catalog, strict artifact,
placement, typed binding and lineage records above the Sand graph. One
Rust-owned recursive host mounts definition, child and instance patches,
chooses every declared adapter kind and owns all renderer and Behavior
handles. Shared edits propagate transactionally through exact ancestors;
invalid publications preserve the last-good catalog. Save as definition,
save/reopen, lock, override/reset and fork/detach are normalized operations,
not renderer-specific shortcuts.

The F10 workbench and its AccessKit nested Button are the human surface. A
standalone Button and twice-nested video-call compound expose the same typed
identity through native retained and Installed HTML projections. Protein
reads, Sand events and Action writes have visibly different arrows. Installed
HTML creates collision-free instance DOM ids, repairs label and ARIA
references and explicitly tears down its listener. Rust construction,
recursive Maud output and workbench artifacts agree in the release parity
report, and the joined Wayland report passes. Castle remains only a human name
for a saved compound.

## What is left

### Sand

- [ ] On the accepted [Native Interface Laboratory](../laboratory.md#native-interface-laboratory)
  runtime, make native Rust retained-UI or world-renderer constructors the
  first-party Plan A path, paired with the exact
  node/port/configuration metadata Box needs. Preserve Maud as the standard
  Plan B and HTML-backed authoring path without changing HTML packages into a
  Rust-only format. Its paired constructors produce accessible `Markup` and the
  same metadata; reject naked markup as a declared child boundary.
- [ ] Define the Sand artifact compiler that normalizes the selected authored
  graph (Rust/Maud or declared raw HTML metadata),
  validates it through the authoritative Rust schema, renders and hashes
  fragments, gathers native JavaScript modules, Wasm modules, generated loader
  glue, shaders and other assets, and enforces manifest, capability, source to
  artifact hash, LICENSE, NOTICE, and credit completeness.
- [ ] Define the logical Sand runtime ABI once and generate its adapter
  projections: native Rust retained-UI calls, retained world-scene handles, validated
  size- and rate-bounded CEF messages for installed external HTML,
  wrapper-only Website ports, Plan B scoped DOM/`MessagePort` calls, and an
  optional WIT projection for Wasm Behavior. Prove the adapters agree on
  lifecycle, typed ports, attribution, capabilities, state planes, Action
  requests, errors, camera-only presentation culling, and teardown while
  passing no raw DOM, GPU object, pointer, credential, or global Box store
  through the portable boundary.
- [ ] Define the GPU renderer vocabulary and package rules for built-in
  patterns, sprites/glyphs, zones, connections, drawings, selection, and
  specialized leaves. Use one host rendering device, queue/submission policy
  and retained scene with stable visual-node uids and partial buffer updates;
  isolated CEF GPU producer contexts cross only the synchronized external-
  surface boundary. An arbitrary shader is installed
  executable content with an exact hash, declared GPU capability, resource
  budget, validation, license, credits, and deterministic disposal.
- [ ] Rebuild official workflow Sands into referenced primitive/compound Sand
  definitions plus Behavior after the contract is proven. Do not preserve the
  legacy component API or old board state merely to avoid rebuilding.
- [ ] After the Customization completion gate and official-Sand migration,
  prove the Box model vertically with one current Protein item, visual result
  fields, a mixed bound/unbound result-template group, repeated row instances,
  and force/sort/mutation areas. Do not use this spatial proof to finish the
  component or composition foundations underneath it.
- [ ] In that vertical proof, remove and restore one stable Protein row and
  change one result field incompatibly. Show live, retired, restored and broken
  binding states, retain bounded recoverable instance-local state, and provide
  visible reconnect/clear/replace operations through **Why is it here?**.
- [ ] Expose the landed instantiate/override/reset/save/fork operations in Box
  edit mode. A code-owned native or Maud definition is never rewritten; Box
  edits a visibly forked user definition and preserves lineage and revision
  inspection.
- [ ] Write concise author documentation that starts with composing existing
  pieces and progresses to HTML, Protein, Actions, ports, permissions, and
  packaged assets.

- [ ] Define and version the Sand manifest, bridge handshake, typed ports,
  capability vocabulary, provenance record, resource limits, CSP, and package
  signature/integrity rules together.
- [ ] Prove the installed external path with one CEF HTML Sand that receives a
  mapped Protein Record summary, emits `record-clicked`, consumes a Box event,
  keeps local browser state, and requests one granted typed Action. Run the same
  semantic fixture through Plan B `MessagePort`. Reject undeclared ports,
  malformed values, excessive size/rate, spoofed instance identity, and the
  same messages from an arbitrary Website.
- [ ] Replace the current broad iframe grant with the trust tiers above and
  prove that a denied Sand cannot reach Actions through another Sand or leak
  data through lanes, navigation, popups, downloads, or network requests.
- [ ] Add import, inspect-before-run, permission review, update review,
  revoke, disable, and delete surfaces with honest failure and empty states.
- [ ] Add a runtime-health and admission surface for heavyweight Sands. Show
  process and texture cost, configured budget, denied starts, renderer crashes,
  recovery attempts and the exact unavailable reason; keep retry, disable and
  clear-storage controls reachable without opening developer tools.
- [ ] Build the Website Sand with an isolated CEF browser surface and request
  context under Plan A and a sandboxed iframe in browsers. Keep origin/security chrome above
  remote pixels and prove that Website content cannot invoke Lince native
  APIs, overlap system chrome, or receive a privileged parent message. Moving
  it off-camera culls composition only and does not unload, suspend, or throttle
  its browser execution.
- [ ] Enforce HTTPS navigation; deny custom protocols, filesystem access, and
  all Lince native capabilities; and harden every local HTTP/WebSocket
  endpoint against foreign origins, unauthenticated requests, and CSRF. In CEF
  or dedicated WebViews, additionally intercept requests to block loopback,
  link-local, private-network destinations, unsafe redirects, and DNS
  rebinding. In a browser iframe, disclose that broader private-network egress
  cannot be guaranteed rather than presenting it as enforced.
- [ ] Add the isolated per-origin Website profile, explicit persistent/private
  modes, storage quotas, usage inspection, clear-data controls, and tests for
  cookies, local storage, IndexedDB, Cache Storage, service workers, restart,
  and cross-instance sharing.
- [ ] Add per-origin permission and activity surfaces for downloads, popups,
  external protocols, clipboard, camera, microphone, location, notifications,
  screen capture, and storage-access requests. Denial and platform limitations
  have honest in-Sand explanations.
- [ ] Test malicious Websites for local-network requests, CSRF against Lince,
  navigation spoofing, popup escape, downloads, resource exhaustion, tracking
  identifiers crossing profiles, and frame/IPC confusion. Keep CEF, WebView,
  and browser runtimes patched; sandboxing does not eliminate engine exploits.
- [ ] In browser/Plan B iframe mode, detect sites that prohibit framing and
  offer an explicit open-in-browser fallback. Never strip or proxy around
  `frame-ancestors` or `X-Frame-Options`. In Plan A CEF mode, detect sites,
  authentication, protected media, or browser policies that still reject the
  embedded runtime and offer the same fallback.
- [ ] Package the authoring documentation and a minimal bridge test kit so
  external HTML can integrate without copying an official Sand as folklore.
- [ ] Remove the legacy nested-payload frame and old Lynx component API during
  the rebuild. Route `.lince` imports by inspected content rather than a
  legacy filename suffix; unknown shapes fail closed.
- [ ] Federate published Sand packages between connected Organs while
  preserving content hash, lineage, author, capabilities, and licenses. The
  contract must not depend on whether bytes live on disk or in a future object
  store, and it must not require a central registry.

- [ ] Later on, some form of creation of data, similar to ontology's trail should exist and be able to see it in this sand, to input in some dsl or lingua the creation of data to make this demo of paradigms of intelligence: https://paradigms-of-intelligence.github.io/morpho/.
