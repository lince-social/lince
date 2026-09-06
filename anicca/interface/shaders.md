# Sand visual effects

Owner source: [Interface in Lince](../Lince.lingua), updated 2026-09-06.
Status: later v1 work, after the stable everyday interface. The custom-WGSL
idea needs the bounded authoring surface below before it is a usable feature.
The [Interface plan](plans/interface.md) owns execution order and checks.

A Sand can have an effect with named settings, a preview and a reset.
Ordinary users choose a preset and adjust its controls. Advanced authors edit
supported WGSL source and see errors beside the last working preview. Effects
follow the same appearance/template/definition scope as other styling.

Start with a bounded fragment effect on a Sand background or its declared
rendered content. Inputs are local coordinates, size, explicitly supplied
values and host-provided presentation time. It reads only its declared inputs
and assets, without desktop capture, other Sands, Record queries, Actions or
arbitrary GPU resources.

A bounded visual margin allows glow beyond the Sand body. The compositor
includes it in drawing bounds, clipping, overlap and culling; hit testing stays
on the original body. Glow does not become lighting on other objects.
Arbitrary vertex deformation, compute passes and scene lighting remain later
extensions with their own geometry and cost contracts. Preserve the possibility
without requiring them for the first effect editor.

WGPU support does not itself make arbitrary source cheap or safe to run.
WGSL has loops and a separate shader/pipeline validation lifecycle; see the
[WGSL specification](https://www.w3.org/TR/WGSL/#loop-statement). Define and
validate a bounded source profile before executing user WGSL: fixed host
bindings, no user loops or recursion, capped expanded expressions and calls,
an operation allowlist, no storage writes and no compute dispatch. Use the
runtime's shader parser/validator, never text searches for forbidden words.
Document supported syntax and limits with examples.

Bound pixel area including glow, texture sizes, passes and update rate. Cache
compiled effects by source and compilation settings. A failed edit keeps the
previous effect, displays its diagnostic and leaves Disable and Reset outside
the effect. These limits are not a promise that a watchdog can safely interrupt
any GPU program. Device-loss recovery restores an undecorated usable surface.

Text, focus indicators and workspace controls stay readable and reachable.
Reduced-motion/static presentation can hold decorative animation without
stopping Behavior, data or media. A workspace switch disables custom effects.
Imports follow executable-package review, hashes and capabilities. Include
dependency LICENSE, NOTICE and credits in the owning Sand when required.

Prove a recolorable pattern and an outward glow: edit settings and supported
source, reject an invalid edit, override one appearance, save/reopen, disable
and recover. Compare cost with effects disabled at the representative Sand
workload. Do not allocate a device or animation loop per decorated Sand.
