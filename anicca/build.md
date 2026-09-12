# Building and checking the native interface

Use the current checkout and a reused Cargo target. Use `cargo check` with the affected target and features; warnings are errors. Run focused tests for changed behavior. New interface work belongs in the Bevy application. The old Web crate remains unplugged and untouched.

The native client connects to a Cell server through its transport. Check the real client and headless server entry points and their packaging, including connection and authorized Actions. A library check or laboratory window alone does not prove that journey. The native package must not acquire an embedded browser dependency.

The retained Linux target is Wayland, with no automatic X11 or XWayland fallback. If no usable Wayland compositor exists, explain the launch failure. Other operating systems and graphics backends need their own launch, input, recovery and accessibility evidence before being called supported.

## What to verify

Ordinary workflows need real reads, writes, permissions, useful failures and recovery. Check keyboard navigation, native text input and IME, focus, AccessKit/Orca, resize, suspend, resume, graphics recovery and shutdown through the same application people use.

Use isolated data directories, configuration and endpoints for tests, especially deletion, Transfers, terminal commands and publication. Missing tests, zero selected tests, stale reports and failed launches do not count as success. Keep the current branch and unrelated owner work intact, and never stop the owner's processes to manufacture a quiet benchmark.

Review light, dark and partial themes at the available display scales. Measure stationary editing separately from motion experiments. Keep the useful stress workload of 200 visible, 1,000 active and 10,000 resident Sands where it applies; it does not replace ordinary workflow checks.

Check quantity formatting, aligned numeric figures, associated labels and errors, and non-color status. Add a check for interface style values that bypass the shared definitions, with explicit exceptions for user content, measurements, vendored assets and specialized rendering.

Measure idle focused, unfocused and minimized windows, one active Sand among static ones, targeted Protein updates and repeated open/close. Record CPU and GPU work, wakeups, drawing submissions, input latency and memory. Main benchmark runs retain three 30-second warm-ups and 120-second samples on a quiet machine, with the source, features, workload and raw results identified.

The ordinary health surface should name the actual graphics device, software rendering if used, resource limits, refused starts and recovery progress. Explain measured versus estimated costs and unavailable measurements. Keep retry, disable and appropriate storage-reset controls reachable even when the affected Sand cannot open. Diagnostic captures need limits and must protect private content.

Check that repeated creation and removal release subscriptions, focus, sessions and graphics resources, including after a refused write. Performance measurements run without simultaneous compilation or another heavy workload; sharing a machine requires coordination rather than accepting contaminated samples.

Earlier custom-host and browser reports apply only to what they measured. New Bevy controls require fresh evidence. Input-to-present-call timing is a lower bound on display latency. An untested operating system, device or display scale stays untested.

## Reference machine

The earlier local inventory recorded an Intel i7-1165G7, 4 cores and 8 logical CPUs, about 15.37 GiB of usable RAM, Intel integrated graphics and NixOS with a Wayland/Vulkan target. This is the owner's selected minimum machine, not a new performance result.

For each measurement, record the actual adapter, driver, compositor, native resolution, scale and power mode. Leave memory headroom and bound caches. Integrated graphics share system memory; do not assume a separate fixed VRAM pool. Physical 4K review waits for matching hardware, while layout and rendering should already support changing resolution.

[The interface draft](plans/interface.md#checking-the-result) contains the proposed user-facing completion tasks. This documentation rewrite ran no application benchmarks.
