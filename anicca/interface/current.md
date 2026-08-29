# Current interface context

Purpose: give a future session enough context to continue without loading the
full interface corpus.

Owner source: [Interface](../Interface.lingua). Ontology and Karma remain
authoritative where linked; no separate Customization, Sands, or
Interoperability Record currently exists.

Status: native laboratory, customization, semantic primitive-Sand and
recursive-composition runtimes accepted; C3 Configuration and external
authoring is next.

[Corpus map](README.md)

## Where work stands

Plan A has been accepted. Lince has a joined Wayland-only Linux runtime in
which Winit owns the event loop, WGPU/Vulkan owns composition, Lince owns the
semantic and frame coordination layers, selected Bevy resources remain
subordinate, Avian supplies collision work behind an adapter, Glyphon/
cosmic-text supplies native text, AccessKit supplies accessibility, and CEF
supplies accelerated external HTML surfaces.

CEF content reaches Lince-owned GPU memory through the accelerated DMA-BUF
path. Installed HTML Sands and Website Sands have separate authority. Website
content cannot call the Lince bridge. Off-camera semantic, physics, media,
timer and browser activity remains live; only extraction, copying and
presentation are suppressed.

The accepted representative load is 200 visible interactive Sands, 1,000
continuously eligible bodies at a 120 Hz fixed step, and 10,000 resident light
nodes. The joined benchmark passed the native-resolution frame, simulation,
input-call, lifetime, recovery, accessibility and authority gates. Physics is
currently CPU-side; GPU rendering is proven, while GPU-compute physics remains
a measured future decision.

The production desktop release builds and packages. A 2026-08-28 cold release
build also completed after pointing the offline desktop environment at the
already validated local CEF distribution. The live production report still
stops before opening the window because the owner-authored
`Interface.lingua` fails parsing at bytes 2489–2497 on line 31's
`Interfaceless` opening. The project checker also reports opening/closing UID
mismatches in the owner-authored `Karma.lingua`, `Lince.lingua` and
`Transfer.lingua`. Markdown must not repair or bypass those Records. Rerun
`mise run interface-desktop-report` after the owner repairs them before
claiming the production launch human-usable.

The exact landed evidence and remaining report are in
[plans/interface.md](plans/interface.md#landed-native-interface-foundation).
The architecture and test contract are in
[architecture.md](architecture.md) and [laboratory.md](laboratory.md).

The first product kernel after the laboratory has landed. Contract version 1
defines 91 typed canonical style roles, Dark and Light Lynx, partial themes,
safe local theme assets and a seven-scope cascade with per-value provenance.
The joined Gallery applies it to native WGPU nodes, borders, background and
retained text and projects the same result into Installed CEF without reload.
Website CEF receives no projection or bridge. The legacy source boundary is in
[visual-inventory.md](visual-inventory.md); those Web surfaces have not yet
been migrated.

The second product kernel has landed. Sand schema and ABI version 1 define the
renderer-neutral definition graph, persisted projection selection, disposable
runtime binding, artifact validation and bounded host messages. The native
Gallery exposes 19 reusable primitive Sands through keyboard, pointer and
AccessKit, while Installed CEF renders the same package as accessible HTML,
external CSS and native JavaScript modules. Its package closes the relative
static-import graph, its JavaScript ABI rejects unknown fields, and its valid
Protein-shaped mount and granted `record-clicked` route passed alongside the
zero-authority Website.
Generated schemas and valid/stale fixtures are under
`target/interface-laboratory/sand-contract/`; the joined release report there
passed.

The third kernel has landed. Composition artifact and catalog schema version 1
give exact revision propagation, saved and forked lineage, recursive mounting,
definition/child/instance style precedence, configuration, exported typed
ports and deterministic teardown. The F10 workbench exposes ten keyboard
operations over a standalone Button and a twice-nested video-call compound,
including lock, override/reset, shared edit, invalid-edit refusal,
save/reopen, save as definition, fork and remount. It draws separate Protein
`READ`, Sand `EVENT` and Action `WRITE` arrows. The same 21-definition package
is consumed by Rust, recursive Maud output and Installed HTML; the live report
passed with five final placements, 17 nodes and collision-free scoped DOM ids.

## Active sequence

1. Owner repairs the malformed Interface opening and the checker-reported
   Record UID mismatches; rerun the production desktop report. Isolated
   foundation work can continue while this independent gate is open.
2. Build C3: Configuration and external authoring on the landed composition
   primitives.
3. Build C4: rebuild the first-party interface and official Sands.
4. Pass C5: visual, accessibility, theme, authoring, lifecycle, runtime-health
   and scale gate.
5. Only then open Box navigation, layers, anchors, base pattern and input.
6. Build current-Protein result templates and visible field-to-port wiring.
7. Build force, sorting, mutation and immunity Areas through typed Actions.
8. Build topology brushes, filtered potential effects, admission policies and
    consistent 2D/3D field views; make the Avian/SoA decision from this measured
    workload.
9. Build Box durability and the operation/snapshot model.
10. Finish Installed HTML/Website administration and runtime health.
11. Build the public read-only Live Facade.

The active detailed checklist is
[plans/customization.md](plans/customization.md). The Sand contract that it
must converge with is [sand-model.md](sand-model.md), with its detailed work
in [plans/sands.md](plans/sands.md).

## Current boundaries

- Do not use the laboratory's fixture graph or replay format as the persisted
  Box schema.
- Do not invent a second composition model in Box. Its edit mode consumes the
  landed artifact, catalog and normalized workbench operations.
- Do not create a new `LynxUI` abstraction. The old name is migration
  inventory; every reusable control, layout, editor, and compound interface is
  a Sand with renderer projections.
- Do not turn Bevy into the window/application owner or retain GPUI as a
  production dependency or fallback. Study its visual, interaction, text, and
  retained-UI techniques and reimplement or reuse only bounded licensed parts
  that fit the Lince-owned host.
- Do not add an X11 path. Linux support is the tested Wayland/Ozone path.
- Do not turn CEF into the renderer for thousands of nodes. It is a
  heavyweight admitted surface; ordinary Sands stay native.
- Do not give Website Sands Lince authority.
- Do not make TypeScript or Datastar part of the first-party runtime.
- Do not pull globe, terrain, general scene construction or capture-to-work
  product scope into v1.
- Do not sleep or suspend off-camera Behavior merely to improve rendering
  numbers.
- Do not make physical 4K testing a current completion gate without matching
  hardware. Keep size, device scale, clipping, text rasterization, render
  targets, and quality selection dynamic so no known 4K ceiling is designed
  in.

## Minimal reading set for the next implementation

Read:

- [customization.md](customization.md)
- [visual-inventory.md](visual-inventory.md)
- [architecture.md](architecture.md)
- [sand-model.md](sand-model.md)
- [plans/customization.md](plans/customization.md)

Consult [legacy-web-ui.md](legacy-web-ui.md), [behavior.md](behavior.md), or
[html-and-websites.md](html-and-websites.md) only for the part being changed.
Do not load runtime alternatives, deferred work, Communication, or v2 by
default.
