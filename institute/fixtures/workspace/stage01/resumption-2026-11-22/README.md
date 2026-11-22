# Workspace desktop baseline, 22 November 2026

Stage 001 passed on revision `d46f719a7d8daab088ca5d58ba250a67fac3190c` with the local physics and replay changes identified in [build hashes](build-hashes). The desktop remains the ordinary Lince interface. The experiment uses a fresh workspace containing synthetic text Sands.

## What to watch during a replay

1. The Edit controls open. “Move this work” is picked and dragged. In a timing run the harness moves it 64 times between two positions while holding the button. Those alternating moves are inputs.
2. After release and the queued physics toggle, the matching Sand moves toward “Bring matching work here.” Overshoot can occur. The visible swings should shrink until the Sand stays within one canvas unit of the target. “Gravity and settling” falls from elevation 80 toward 10. In this top-down view its horizontal footprint can appear stationary.
3. When both bodies sleep, sample idle and reopen a copy in a fresh process. Check the attracted Sand, the resting elevation and the original position of the Sand in workspace 2. [Native actions](trace.jsonl) · [Fresh-process observations](reopen.json) · [Saved synthetic workspace](saved/)

## Correctness

**01.W01.** The recorded hit identified the expected Sand. Native input changed the presented position and the queued toggle enabled physics. The final position was [-400.1885535087872, 0.20235071889118345] against target [-400, 0], an error of 0.277 canvas units. The falling Sand reached elevation 9.9998. Both bodies slept. The separate workspace kept its position. The first real-clock preflight had exceeded the 120 s watchdog while oscillating. [Pre-repair native trace](before-fix-trace.jsonl) · [Watchdog result](before-fix.txt)

The repaired spatial path publishes each intermediate pose and evaluates Area forces again before a catch-up step. A focused test compares the position after 96 fixed steps grouped into display updates of 1, 2, 4 and 8 steps, with tolerance 0.000001 canvas units. [Correctness replay](baseline.json) · [Before drag](before-drag.png) · [After settling](settled.png)

**01.W02.** Reopened 10 cases across 5 roots with 1,636,549 packed bytes. Verified size, hashes and rights for 19 assets. Independent GLB buffer reads with node scaling measured 0.040 m and 0.007 m on all three axes for the selected fixtures. [Input review](input-review.json) · [Checked fixture](fixture.json)

**01.W03.** Reviewed the 49 first-consumer requirements and the four bounded B33–B36 hosting contracts, including participant roles, budget outcomes, rights and deferred B37. Runtime hosting and later scientific proofs retain their named owners. [Input review](input-review.json) · [Contract decisions](../contracts.json) · [Checks](checks.txt)

## Separate wall-clock measurements

All three runs used the same local development binary on an Intel i7-1165G7 with Iris Xe, Vulkan and Mesa 26.1.2. The native Niri/Wayland window was 1920 × 1052 at scale 1. Each run opened a fresh process and fixture after the correctness run. File and driver caches were warm. Compilation and tests finished first. A once-per-second process and DRM sampler recorded remaining desktop load. [Window observations and graphics counters](windows.json) · [Compiler](compiler) · [Per-run samples](timing-1.json) · [Run 2](timing-2.json) · [Run 3](timing-3.json)

| Measurement | Samples | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| Held-drag update interval | 765 | 40.04 ms | 44.67 ms | 49.80 ms |
| Injected pointer move to changed canvas presentation | 192 | 51.53 ms | 56.50 ms | 82.86 ms |

Nearest-rank percentiles pool the raw samples from three repetitions. Input ends at the first changed presented canvas position observed in PostUpdate. CPU consumed 65.53 s during 30.809 s of active sampling (212.7% of one logical CPU). Settled idle consumed 0.78 CPU-seconds over 4.574 wall-seconds (17.1%). All three idle samples had zero awake bodies, zero native wake signals and stable positions. Each sample used one separate wake to finish measurement. Peak process RSS ranged from 715.3 to 717.4 MiB. The largest sampled i915 client resident allocation per timing run ranged from 400.0 to 400.0 MiB in system memory. Background samples included Niri, the terminal and the measurement process. Kernel client counters and managed texture estimates have different scopes. Exact per-frame GPU time, total device residency, physical input-to-scanout and per-Sand CPU remain unmeasured. [Pooled measurements](measurements.json)

The October run had a 125.54 ms p95 over its mixed 251-frame correctness replay, one drag observation of 140.21 ms, and 0.43 CPU-seconds over 1.531 s idle. It used a manual clock, screenshots and concurrent development. November's percentiles describe repeated held-drag updates with the normal clock and screenshots disabled. The optimized 1280 × 720 navigation targets retain their own workload. [Historical measurements](../baseline/measurement-interpretation.json) · [Target profile](../profile.json)

## Five historical findings

| Finding | November disposition and evidence | Next owner |
| --- | --- | --- |
| Renderer slab allocation during workspace switch | Not reproduced under recorded conditions. See the switch and render in the [correctness log](accepted.log) and the separate [reopen log](accepted-reopened.log). | Native renderer and workspace-view lifetime owner at streamed-view acceptance |
| Wayland cursor warp | Reproduced. Wayland rejects a warp on an unlocked cursor. The injected press and drag passed the identity and position checks. [Log](accepted.log) · [Trace](trace.jsonl) | Native input harness and interaction measurement |
| Frame/input/idle CPU | Reproduced for this development workload. Input and frame tails and nonzero idle CPU are quantified above. [Measurements](measurements.json) | Desktop performance and native scheduling |
| Area and Sand labels near the target | Reproduced. At rest, “Move this work” overlaps “Attracts · Quantity = -1”. The left Edit panel shows physics enabled and the separate falling Sand stays identifiable. [Settled image](settled.png) | Native selection and Area presentation |
| Accessibility D-Bus status proxy | Environment-dependent, reproduced. The session could not activate the status service during startup. Assistive technology behavior remains unmeasured. [Startup](accepted.log) | Accessibility test-environment owner |

## Reproduction

Use the repository's interface Nix shell, the pinned Rust compiler and its workspace compiler wrapper from [the Linux CI setup](../../../../scripts/ci-linux-setup.sh). Complete compilation before the isolated window. Run the following from the repository root using new output names.

```sh
cargo test --locked -p xtask --features authoring workspace::tests
cargo xtask workspace check institute/fixtures/workspace/stage01/dataset
cargo test --locked -p transport --test workspace_live -p engine --test workspace_sync -p lince-simulation --test effects
cargo test --locked -p lince-interface --features native-presentation --lib native::physics::
cargo test --locked -p lince-interface --features native-presentation --lib native::topology::physics::
cargo test --locked -p lince-sandbox --test workspace_area_steps
cargo build --locked -p lince-sandbox --example workspace_baseline
sample=$(mktemp -d target/workspace-baseline.XXXXXXXX)
target/debug/examples/workspace_baseline "$sample/replay"
cp -a "$sample/replay" "$sample/reopened"
target/debug/examples/workspace_baseline "$sample/reopened" --reopen
for run in 1 2 3; do
    target/debug/examples/workspace_baseline "$sample/timing-$run" --timing
done
```
