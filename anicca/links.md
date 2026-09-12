# Research worth keeping

The earlier Pulsar and Helio study grew into thirty long reviews. This is the shorter reading: what each subject suggests for Lince, followed by the original source links. These are retained research notes, not newly verified claims about those projects or an additional task list.

Bevy is the selected application base. The study did not select Pulsar, Helio, GPUI, WGPUI or SceneDB as another production runtime. Older ideas about a separate compositor, embedded browser or general Behavior compiler do not override that decision.

## Useful lessons

- **Rendering a scene.** Keep saved Sand identities separate from temporary graphics resources. Update the parts that changed.
- **Scene storage and files.** Keep Records, Box state and file projections distinct while giving sharing one understandable home.
- **A viewport inside another application.** Let Bevy own the native window and rendering. A second application runtime is not required.
- **Large scenes on the GPU.** Batch drawing where measurement shows a benefit, while retaining a way to identify the Sand being shown.
- **Extending an application.** Add a focused plugin for a concrete need and make its authority clear.
- **Inspecting editable properties.** Build property controls from the registered data so names, types and validation stay consistent.
- **Particles.** Dense decorative effects can use the GPU without turning each particle into saved Lince data.
- **Lighting nearby objects.** Spatial grouping can reduce rendering work. It does not define which Records an Area affects.
- **Ordering rendering work.** Rendering steps need a clear order and resource lifetime within Bevy.
- **Indirect lighting.** Extra lighting is a possible visual feature, not a source of truth about Lince data.
- **Measuring graphics work.** Show what a measurement actually includes and distinguish an estimate from a measured cost.
- **Dragging and resizing.** Treat a complete gesture as one change that can be checked, cancelled and undone.
- **Drawing only what can be seen.** Save drawing work outside the camera while preserving the intended behavior of what is there.
- **Different graphics devices.** Check the device actually in use and offer an honest failure or supported fallback.
- **Moving work to the GPU.** Avoid copying results back and forth unnecessarily. A small number of submissions does not mean the work is free.
- **Shadows.** Use shadows to clarify depth while keeping text and controls readable.
- **Keeping the window responsive.** Reuse unchanged presentation and update the parts affected by input or data.
- **Custom visual effects.** Start with supported inputs, presets and limits, with a way back to the last working effect.
- **Combining rendering passes.** Optimize drawing only while preserving its intended order and resource use.
- **Agent tools.** Give agents the same checked and attributed operations used by ordinary controls.
- **Visual behavior graphs.** Visible connections can explain an interaction. Current native Effects can use Bevy directly.
- **Compiling behavior.** Keep one authoritative definition of a behavior and reject invalid connections before running it.
- **Material customization.** Offer simple appearance settings before deeper effects, and measure their cost.
- **Choosing a property editor.** Reuse the same suitable editor wherever a property appears.
- **Explaining a slow interaction.** Trace a delay from input through the work it caused to the displayed result.
- **More advanced materials.** Keep the allowed shader inputs and work explicit. An interesting research design is not an implemented capability.
- **Large world detail.** Load derived world detail in bounded pieces while preserving the identity and behavior of source objects.
- **XR and multiple views.** Different cameras should view one world. XR remains an idea with its own input and device work.
- **Many sprites.** Use dense graphics representations where useful without hiding the Records or results they represent.
- **Nested coordinate frames.** A Castle or world region needs one shared meaning for its coordinates across display, picking, saving and sharing.

## What the earlier source audits warned about

These are condensed findings from the saved reviews, not fresh audits of today's projects. They explain why retaining a useful idea did not mean adopting its implementation. The old recommendations to build a separate compositor or general Behavior compiler are superseded by the Bevy direction.

| Subject | Reason to keep the implementation at a distance |
| --- | --- |
| Renderer and SceneDB | Temporary scene handles are not saved identities; runtime storage and a virtual filesystem do not supply Box durability or collaboration by themselves. |
| Bevy–GPUI viewport | The central sharing path was Windows-specific, and its synchronization and resource lifetimes were not a safe template for the Wayland application. |
| Helio scene rendering | Demonstration counts and platform claims did not establish Lince's performance or supported devices. |
| Subsystems | The audit found concrete lifecycle coupling, ambient mutable state, silent conflicts and a Rust dynamic-library boundary behind the advertised separation. |
| Reflection and property editors | Process-local type ids, enum positions and Rust memory layout cannot become saved schemas. Silent downcast failure, unchecked setters and unsafe thread transfers were also rejected. |
| Particles | A million particles does not mean a million interactive Sands. Pool ownership, ordering and bounds still need proof. |
| Tiled lights | Projection, stale caches and overflowing candidate lists needed correction. A camera's light list cannot replace complete world-space Area evaluation. |
| Render graph | Declared resource usage was not proof that validation ran or that advertised aliasing and subpasses existed. String routing and manual encoder choices left ordering risks. |
| Indirect lighting | A connected probe atlas was not complete material-aware light transport. Coverage, history, leakage and fallback remained important gaps. |
| GPU profiler | Timestamps on unrelated encoders, blocking readback, names without stable identity and unbounded telemetry can make a cost report misleading. |
| Manipulation tools | Direct mouse-driven mutation did not provide cancellation, permissions, undo, persistence or concurrent editing. |
| Culling | Representative-only bounds, approximate boxes, fixed screen thresholds and camera-only cache invalidation could hide visible objects; the claimed precomputed-visibility path was not established. |
| Cross-platform graphics | Guessed device limits, invalid API examples and unexplained performance constants were not evidence of a working fallback. |
| Compute and sprites | Fixed submission cost is not constant total work. GPU-only semantic positions, reusable slots without generation checks and unbounded draw counts can lose correctness. |
| Shadows | Fixed large allocations, duplicated constants and fixed slots per light imposed costs and limits the presentation did not justify. |
| Window compositor | One large cached texture was a poor general model for transformed Sands. Input must target what was actually displayed, and lifecycle acknowledgements cannot be dropped like old frames. |
| Effect injection and materials | Raw source insertion, anonymous buffers, caller-supplied hashes and mutable unversioned registrations were not safe user-extension boundaries. The publication's volume/exposure implementation also needed correction. |
| Combining passes | Moving all compute ahead of drawing can break order. Raw pointers and pointer-only compatibility did not prove that passes could safely share resources or discard results. |
| Agent tools | Short-name registries, global provider state, parallel mutation and checking a path's spelling alone did not establish permission or filesystem isolation. |
| Behavior execution | Raw byte arenas, size-only calling rules, wildcard host access and thread-local generated state were unsafe foundations. A content hash is not a sandbox, and the claimed native mode was not sufficient proof. |
| Behavior compilation | Unknown types, silent conversions, prefix-based expansion and unstable output ordering needed validation. Two code generators agreeing does not establish correct behavior. |
| Frame capture | Labels and selected buffers are not complete GPU state. Redrawing with the current shader is not faithful replay; timing uncertainty, private data and capture limits need explanation. |
| Advanced material draft | Independently compiled pipelines cannot simply execute together in one all-material draw. The design needed a coherent batching or shader strategy. |
| Foliage | Camera residency must not control semantic lifetime. GPU floating-point regeneration was not a cross-device convergence guarantee, and several feature/performance claims were unimplemented. |
| XR | The audited dual-pass/multiview combination and fallback were not a reusable working integration. |
| Nested frames | Renderer-only membership and temporary space ids cannot define saved placement. Distance-based unloading and the portal path did not establish general alternate views. |

## Original articles

These links are retained from the earlier reviews so the owner can choose what to keep. Publication claims, source audits and performance results have not been rerun for this rewrite.

- [Helio Renderer](https://tridentforu.com/blog/posts/Helio-Renderer)
- [06 26 pulsar engine fs](https://pulsarnative.com/blog/2026-06-26-pulsar-engine-fs)
- [07 17 scenedb20 cross device spatial database](https://pulsarnative.com/blog/2026-07-17-scenedb20-cross-device-spatial-database)
- [08 26 state of scenedb](https://pulsarnative.com/blog/2026-08-26-state-of-scenedb)
- [gpui viewport](https://tridentforu.com/blog/posts/gpui-viewport)
- [06 01 introducing helio](https://pulsarnative.com/blog/2024-06-01-introducing-helio)
- [06 26 pulsar subsystems](https://pulsarnative.com/blog/2026-06-26-pulsar-subsystems)
- [06 26 pulsar reflection system](https://pulsarnative.com/blog/2026-06-26-pulsar-reflection-system)
- [06 26 corona gpu particles](https://pulsarnative.com/blog/2026-06-26-corona-gpu-particles)
- [06 29 tiled light culling](https://pulsarnative.com/blog/2026-06-29-tiled-light-culling)
- [06 29 render graph design](https://pulsarnative.com/blog/2026-06-29-render-graph-design)
- [06 29 radiance cascades gi](https://pulsarnative.com/blog/2026-06-29-radiance-cascades-gi)
- [06 29 gpu profiler](https://pulsarnative.com/blog/2026-06-29-gpu-profiler)
- [06 29 editor gizmos](https://pulsarnative.com/blog/2026-06-29-editor-gizmos)
- [06 29 culling system](https://pulsarnative.com/blog/2026-06-29-culling-system)
- [06 29 cross platform gpu](https://pulsarnative.com/blog/2026-06-29-cross-platform-gpu)
- [06 29 compute over cpu](https://pulsarnative.com/blog/2026-06-29-compute-over-cpu)
- [06 29 cascaded shadow maps](https://pulsarnative.com/blog/2026-06-29-cascaded-shadow-maps)
- [06 30 gpui compositor](https://pulsarnative.com/blog/2026-06-30-gpui-compositor)
- [07 07 post processing shader injection](https://pulsarnative.com/blog/2026-07-07-post-processing-shader-injection)
- [07 07 helio fusor](https://pulsarnative.com/blog/2026-07-07-helio-fusor)
- [07 11 pulsar ai tools](https://pulsarnative.com/blog/2026-07-11-pulsar-ai-tools)
- [07 14 pulsar blueprint executor](https://pulsarnative.com/blog/2026-07-14-pulsar-blueprint-executor)
- [07 16 pulsar blueprint compiler](https://pulsarnative.com/blog/2026-07-16-pulsar-blueprint-compiler)
- [07 18 helio radiant](https://pulsarnative.com/blog/2026-07-18-helio-radiant)
- [07 23 type agnostic reflection](https://pulsarnative.com/blog/2026-07-23-type-agnostic-reflection)
- [07 26 ui flamegraph profiler](https://pulsarnative.com/blog/2026-07-26-ui-flamegraph-profiler)
- [08 02 helio foliage system](https://pulsarnative.com/blog/2026-08-02-helio-foliage-system)
- [08 03 helio vr openxr](https://pulsarnative.com/blog/2026-08-03-helio-vr-openxr)
- [08 03 helio 2d gpu sprites](https://pulsarnative.com/blog/2026-08-03-helio-2d-gpu-sprites)
- [08 07 sublevels](https://pulsarnative.com/blog/2026-08-07-sublevels)

## Source revisions and supporting references

- [pulsarnative.com/Research/doc/?section=drafts&slug=scenedb20](https://pulsarnative.com/Research/doc/?section=drafts&slug=scenedb20)
- [github.com/Far-Beyond-Pulsar/SceneDB](https://github.com/Far-Beyond-Pulsar/SceneDB)
- [Pulsar-Native source 117ad4e2](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/117ad4e2ef4670aa471c041894124bdb603a982c)
- [Pulsar-Native source 0f4ee796](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/0f4ee7961addc0084ebec427542c26773c7ea0ac)
- [learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_resource_flags](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_resource_flags)
- [learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createsharedhandle](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createsharedhandle)
- [Helio source 4f9c85bc](https://github.com/Far-Beyond-Pulsar/Helio/tree/4f9c85bcea68729c4c44d36190b6ac8506ca19be)
- [Pulsar-Native source 85ca40c7](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/85ca40c7c94150e862efabe3b3f96d0a986f9074)
- [Pulsar-Reflection source 745ee787](https://github.com/Far-Beyond-Pulsar/Pulsar-Reflection/tree/745ee787cc63288463c170aafb778672db8e85ac)
- [Helio source a1f7243c](https://github.com/Far-Beyond-Pulsar/Helio/tree/a1f7243c5b1db282d27f5a3869483f3dadd179ac)
- [gpuweb.github.io/gpuweb/#usage-scopes](https://gpuweb.github.io/gpuweb/#usage-scopes)
- [Helio source b88e366d](https://github.com/Far-Beyond-Pulsar/Helio/tree/b88e366d6a6792e34d5b9c7afd1a197a78c54747)
- [WGPUI source 31fbf358](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/31fbf35800b4cfc2ddf202a1c16a2971251ca84c)
- [WGPUI source f9c3abb4](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/f9c3abb4aa5317e85cebb4fe3222a52da38b746d)
- [zed source bfa9c6c1](https://github.com/MSIsunny/zed/tree/bfa9c6c148f286fb4f645571ca08080c51cf0820)
- [Helio source d04034be](https://github.com/Far-Beyond-Pulsar/Helio/tree/d04034be77eb25409e31827b62018bf3647fe340)
- [gpuweb.github.io/gpuweb/#resource-usages](https://gpuweb.github.io/gpuweb/#resource-usages)
- [docs.rs/wgpu/latest/wgpu/enum.StoreOp.html](https://docs.rs/wgpu/latest/wgpu/enum.StoreOp.html)
- [docs.vulkan.org/guide/latest/tile_based_rendering_best_practices.html](https://docs.vulkan.org/guide/latest/tile_based_rendering_best_practices.html)
- [www.apple.com/newsroom/2021/10/introducing-m1-pro-and-m1-max-the-most-powerful-chips-apple-has-ever-built](https://www.apple.com/newsroom/2021/10/introducing-m1-pro-and-m1-max-the-most-powerful-chips-apple-has-ever-built/)
- [Pulsar-Native source d2bc125a](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/d2bc125a0080615aec77f75021594c8fe19e6ace)
- [ToolbeltRS source 116b9028](https://github.com/Far-Beyond-Pulsar/ToolbeltRS/tree/116b9028bad6b9467073de2534be60ef4df9b279)
- [modelcontextprotocol.io/specification/2025-11-25/basic/transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [Pulsar-Native source 4198dae5](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/4198dae5cb1686677b5ec9e1491d88f89dda6266)
- [PBGC source 89f42d51](https://github.com/Far-Beyond-Pulsar/PBGC/tree/89f42d511ea480f71d5102532ba2abe8e965da0d)
- [Plugin_Blueprints source f9edb82a](https://github.com/Far-Beyond-Pulsar/Plugin_Blueprints/tree/f9edb82ab641dcd38f28f527eed3cb88b4187124)
- [PBGC source 8739ed4b](https://github.com/Far-Beyond-Pulsar/PBGC/tree/8739ed4b9d4acc362887a3d6414d3ff96c880f4f)
- [Plugin_Blueprints source fbb0bdb9](https://github.com/Far-Beyond-Pulsar/Plugin_Blueprints/tree/fbb0bdb97a283acd89933ab1747ac5ec38a7bc0f)
- [docs.rs/linkme/latest/linkme](https://docs.rs/linkme/latest/linkme/)
- [Pulsar-Native source 91fea168](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/91fea1682471579ebdcde144961d60d4009bc11b)
- [Graphy source 13f874bd](https://github.com/Far-Beyond-Pulsar/Graphy/tree/13f874bd1cdb4c0f3dea7de74f39f39d756f8d50)
- [Pulsar-Native source db396ee0](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/db396ee0c804c48e6cf7cdc43fe5c4326049627f)
- [Helio source 8f73203a](https://github.com/Far-Beyond-Pulsar/Helio/tree/8f73203a9976bfb7bb3af5a8e9f43e3e5f23c7f7)
- [Helio source 6fb248c9](https://github.com/Far-Beyond-Pulsar/Helio/tree/6fb248c921fdce8adbc01d0eef8c18a9a98f2308)
- [Pulsar-Native source 40cd8f1b](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/40cd8f1be262c80a4544219ac3703bbc62989bb8)
- [Pulsar-Reflection source 9b887f1e](https://github.com/Far-Beyond-Pulsar/Pulsar-Reflection/tree/9b887f1ed327b5e3e2b6ba9066679469520cb446)
- [Pulsar-Native source 77249166](https://github.com/Far-Beyond-Pulsar/Pulsar-Native/tree/772491663469f7af963a2fe6590378d2627a1c5f)
- [WGPUI source 76ec65d7](https://github.com/Far-Beyond-Pulsar/WGPUI/tree/76ec65d7f774ba28221b928b579dea37c6d7f09c)
- [WGPUI-Component source 14e99ada](https://github.com/Far-Beyond-Pulsar/WGPUI-Component/tree/14e99adacbb004f6ebc880a86ba3014c3bdf163c)
- [pulsarnative.com/Research/doc/?section=drafts&slug=radiant-20](https://pulsarnative.com/Research/doc/?section=drafts&slug=radiant-20)
- [github.com/Far-Beyond-Pulsar/Helio](https://github.com/Far-Beyond-Pulsar/Helio)
- [github.com/Far-Beyond-Pulsar/Pulsar-Native](https://github.com/Far-Beyond-Pulsar/Pulsar-Native)
- [Helio source 7b5f4b82](https://github.com/Far-Beyond-Pulsar/Helio/tree/7b5f4b82fca0a8074a426f40674321c86dcdc961)
- [Helio source b8cdb87f](https://github.com/Far-Beyond-Pulsar/Helio/tree/b8cdb87f74ba142caa3618d847ff3b79730db8f7)
- [Helio source 50f0b1d4](https://github.com/Far-Beyond-Pulsar/Helio/tree/50f0b1d461823cb5b7acfb9d58d3c71b350e86bd)

The proposed interface work is gathered in [the interface draft](plans/interface.md). World, lighting and XR possibilities remain separate from ordinary editing.
