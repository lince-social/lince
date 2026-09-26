**Android, iOS, and Ailuros with Bevy**

This is an implementation plan, updated on 2026-09-25. The crate rename, shared-crate extraction, mobile jobs, and AR support described here have not been implemented. The current desktop application still lives in `crates/interface`. This revision changes the plan only.

**Recommendation: keep Bevy, share the presentation code, and add at most two new internal crates.** Use Rust for the UI, application logic, and reusable platform integration. Use small Kotlin, Java, Swift, or Objective-C bridges where they make phone behavior reliable. Keep the existing Rust media implementation. Other languages being acceptable does not create a reason to replace it with libwebrtc again.

Bevy 0.19.1, currently pinned by Lince, has Android and iOS examples. Start with that version and upgrade only for a demonstrated need, with desktop verification. [Bevy mobile examples and packaging](https://github.com/bevyengine/bevy/blob/v0.19.1/examples/README.md#platform-specific-examples)

**Rename the current `interface` crate to `desktop`, and use `interface` for shared utilities.** Keep the current application working in `desktop`, then extract a little code at a time when a second host needs it. The limit remains two additional workspace crates after the rename; existing backend crates still receive the capability and portability changes they need.

| Location | Responsibility |
| --- | --- |
| `crates/desktop`, package `lince-desktop` | The existing `interface` crate, renamed. Initially retains all current UI and desktop behavior. Continues owning desktop startup, window/tray handling, single-instance behavior, and desktop-specific presentation and features. |
| `crates/interface`, package `lince-interface` | A small shared library introduced after the rename. Generic utilities for Lince's hosts: reusable Bevy components, theme primitives, layout, focus, input intents, accessibility, and later spatial presentation. Move individual widgets or Sands here only when useful to more than one host. |
| `crates/devices`, package `lince-devices` | Android, iOS, and the future Ailuros host: startup, lifecycle, permissions, native services, device adapters, and packaging projects. This replaces the previously proposed name `mobile`, because Ailuros is a wearable AR device. |
| Existing Cell, Engine, Store, Transport, Media, and other domain crates | Continue owning their existing responsibilities. Make desktop-only dependencies optional or target-specific where necessary. |

Both `desktop` and `devices` depend on the shared `interface`; neither `interface` nor the backend depends on a host. `devices` must not depend on `desktop`. Begin with generic utilities, keeping application controllers in their hosts until there is a concrete reason to share them. Reuse the existing typed backend operations and extract the reusable parts of the Cell bridge as needed. Moving Rust code across a crate boundary does not require IPC, another runtime, or a new serialization layer.

Put Android Rust adapters in `crates/devices/src/android`, iOS adapters in `crates/devices/src/ios`, and Ailuros adapters in `crates/devices/src/ailuros`. Put the Gradle project and Kotlin/Java sources in `crates/devices/android`, and the Xcode project and any Apple-language sources in `crates/devices/ios`. Ailuros can have its own binary target and packaging folder inside the same crate. These are folders and targets, not additional Cargo crates. Keep platform features and dependencies separate. Existing media capture interfaces can be implemented by the device adapters. Reusable media changes remain in `crates/media`.

Each host owns its Bevy application, event loop, runtime initialization, and platform resources. The shared crate exposes plugins and components; it does not open a window or call `App::run`. Keep one Bevy application and one UI world per active host. Android Activity recreation must not accidentally create duplicate Cell runtimes or subscribers. Follow the platform event-loop threading rules instead of putting the Bevy runner inside an arbitrary async worker.

**The shared `interface` should grow from real reuse.** Keep clear modules for widgets, theme, input/focus, layout, accessibility, and optional spatial utilities. It can eventually contain a reused Sand or presentation system, but it does not have to own the entire application UI. The goal is a useful common library for Lince's hosts, without building a general-purpose UI framework or filling it with unrelated helpers.

Extract the smallest utilities needed by the first working phone screen. Move implementation rather than copying it, keep extraction separate from behavior changes, and expand sharing as each screen is needed. Desktop features remain in `desktop` and compose shared primitives where appropriate. There is no requirement to move every desktop module or Sand before producing a useful phone app or AR prototype.

**Desktop capabilities and performance are acceptance requirements.** A crate boundary cannot guarantee the absence of bugs or overhead. Enforce these rules during implementation:

- Keep the full desktop feature set, rendering options, keyboard shortcuts, pointer behavior, and media paths enabled in desktop packages. Mobile limits must not lower desktop capabilities.
- Use one compatible Bevy dependency graph. Give the shared crate minimal defaults and additive capabilities; let hosts select features and platform backends. Avoid a global `mobile` feature that changes common desktop behavior when Cargo unifies features.
- Keep native dependencies target-specific. Do not pull Android/iOS libraries into desktop binaries or terminal, tray, and desktop capture libraries into mobile binaries.
- Preserve direct Bevy systems and current data flow. Cross the native bridge for OS operations, not for every widget or frame. Avoid extra polling, frame copies, and unnecessary dynamic dispatch in rendering paths.
- Establish desktop baselines for startup, idle CPU, resident memory, frame time during scrolling/editing, and representative 3D/media workloads. Compare the same hardware, assets, and release configuration after each extraction. Investigate repeatable regressions rather than assuming code movement is free.
- Run existing desktop checks and relevant smoke tests throughout the work. Add tests for lifecycle recovery, input translation, permissions, and the new shared boundaries. Move embedded dependency licenses and credits with their Sands and include them in both app packages.

The current low-power event-driven behavior should be retained. Phone-specific quality settings belong to the mobile host and must be measured on devices.

**Reuse the current backend and add phone services around it.** Preserve organ/person access checks, local records, synchronization, and typed operations. A phone can own a local Cell and work offline while open. It must recover after process termination and network changes, without depending on a final shutdown callback to save data.

The present `interface/native-runtime`, which will move to `desktop`, bundles UI with desktop services; `cell` enables terminal and updater paths; `media/native` bundles reusable media with desktop capture. Split these dependencies before attempting broad mobile compilation. Keep existing desktop defaults. Device support must not fork permissions or silently turn unsupported features into successful operations.

For Android, prefer the Rust NativeActivity path and a small native text-input bridge, retaining the earlier preference to avoid C++ dependencies. NativeActivity alone is not a complete phone editor. Use Rust JNI bindings plus Kotlin/Java for Activity callbacks and editing integration where necessary. For iOS, reuse Bevy/winit's integration and Rust Apple bindings, adding small native bridges when useful. Avoid introducing another full UI framework for a few system dialogs. [Android activity and text-input limitations](https://github.com/rust-mobile/android-activity)

Unify the existing desktop UI's CPAL 0.15.3 dependency with a tested newer version: the media crate already uses 0.16, whose Android implementation uses `ndk::audio` instead of Oboe. Coordinate AccessKit integration so TalkBack and VoiceOver actually work with the selected Bevy version; current upstream includes both mobile adapters. [CPAL change](https://github.com/RustAudio/cpal/blob/v0.16.0/CHANGELOG.md), [AccessKit adapters](https://github.com/AccessKit/accesskit/blob/main/adapters/winit/Cargo.toml)

Evaluate Robius for sharing, file selection, authentication, and application directories before writing adapters. Evaluate individual Waterkit modules for remaining services; its Rust APIs use native bridges, and its local notifications module does not supply a complete remote push/calling system. Adopt only modules exercised successfully on both target devices. [Robius](https://github.com/project-robius/robius), [Waterkit](https://github.com/water-rs/waterkit)

The first mobile scope is organ access, record browsing/editing, search, threads, local persistence, synchronization, and basic file sharing. Adapt navigation, safe areas, touch targets, scrolling, text selection, keyboard avoidance, and tablet layouts. Test accented text, emoji, composition, clipboard, and accessibility before expanding the screen inventory.

Calls follow the core app. Reuse media protocols and processing, but implement phone microphone/camera access, audio routing, interruptions, background calls, and screen sharing separately. Capture remains user initiated. Benchmark software video encoding and multi-device calls for heat and battery use; platform hardware codecs may require additional negotiation and desktop support.

A suspended phone is not a dependable always-running coordinator. Lince's current 30-second coordinator timeout makes this an explicit call-design issue. Reliable incoming iPhone calls need APNs/PushKit and CallKit; Android needs the corresponding calling/background integration. Foreground offline LAN calls remain a target, but offline wake-up of a suspended phone must not be promised. Push delivery is separate from peer-to-peer media. [Apple incoming calls](https://developer.apple.com/documentation/pushkit/responding-to-voip-notifications-from-pushkit), [Android calling integration](https://developer.android.com/develop/connectivity/telecom/voip-app/telecom)

**Local Android development can use the current Linux machine.** Windows or macOS can also host the Android tools.

| Requirement | Purpose |
| --- | --- |
| Repository-pinned Rust, currently 1.96.0 | Compile/check the shared code and device crate with the same compiler as CI. |
| Android Studio, or equivalent command-line SDK tools | SDK manager, platform/build tools, emulator, and `adb`. |
| A pinned Android NDK and `cargo-ndk` | Target linker, headers, and Rust/native dependency cross-compilation. |
| A JDK compatible with the selected Android Gradle Plugin | Run the committed Gradle wrapper and Kotlin/Java tasks. Use the chosen plugin's documented requirement. |
| `aarch64-linux-android` Rust target | ARM64 phone package. |
| `x86_64-linux-android` where needed | Emulator package for an x86_64 development host. ARM emulator images use the ARM64 target. |
| A physical Android phone with developer mode and USB debugging | Validate the actual GPU, keyboard, camera, audio, lifecycle, and power behavior. |

Pin the SDK, NDK, Gradle wrapper, Android Gradle Plugin, and cargo-ndk versions once the device test passes. Set the SDK location and NDK version explicitly. The NDK toolchain is still needed even when our application code is Rust. [Android tool configuration](https://developer.android.com/studio/projects/configure-agp-ndk), [JDK requirements](https://developer.android.com/build/jdks), [cargo-ndk](https://github.com/bbqsrc/cargo-ndk)

Start with ARM64 devices. Decide and record the minimum supported Android version after checking the chosen graphics and audio paths; do not confuse this with the compile/target SDK used for packaging. Select tools supporting 16 KB page sizes, and verify both ELF segment alignment and APK/AAB packaging for every included native library. Recent NDK/Gradle support helps but does not replace inspecting the Rust-linked output. [Android native-library alignment](https://developer.android.com/guide/practices/page-sizes)

A debug APK needs no Play Console account and uses a debug signing key. Distribution APKs need a stable release signing key. An AAB is a store-upload artifact, not a directly installable phone package. Play distribution additionally needs the appropriate developer account and store setup. [Android packaging and signing](https://developer.android.com/build/building-cmdline)

**Local iOS development needs a Mac with full Xcode and its iOS SDK.** Apple Silicon is the preferred new development setup, although supported Intel configurations can use their matching simulator target.

| Requirement | Purpose |
| --- | --- |
| macOS supported by the selected Xcode | Apple SDKs, linker, simulator, signing, and device deployment. Linux cannot provide the normal supported local iOS toolchain. |
| Repository-pinned Rust and `aarch64-apple-ios` | Physical iPhone/iPad code. |
| `aarch64-apple-ios-sim`, or `x86_64-apple-ios` on Intel | Simulator code; simulator and device libraries are distinct even when both use ARM64. |
| Installed simulator runtime | UI and lifecycle automation without a phone. |
| A physical iPhone, and an iPad if tablet support is claimed | Camera/audio, background behavior, accessibility, GPU, battery, and tablet validation. |
| Apple Account and Xcode signing setup | Personal-device testing. |
| Apple Developer Program membership, app identifier, and signing assets | TestFlight/App Store distribution and required production capabilities. |

Basic personal-device testing is possible with a free Apple Account, with provisioning/capability limits. Use the paid program for distribution and production push capabilities. A simulator app does not require distribution signing. A downloadable IPA is not generally installable on arbitrary iPhones; its provisioning and distribution method determine where it runs. [Apple account requirements](https://developer.apple.com/help/account/membership/program-enrollment), [distribution methods](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases)

For comfortable local work, I would budget about 32 GB RAM and 100–150 GB free SSD space for Lince outputs, both SDKs, and simulators. This is a planning allowance, not a measured minimum. Smaller machines can use fewer concurrent jobs and fewer installed targets. A Mac can cover both phone platforms; Linux plus GitHub's macOS runners can produce iOS artifacts, but remote builds are a slower substitute for interactive iOS development.

**Keep checks and package creation distinct.** Routine verification uses `cargo check`, locked dependencies, and warnings denied. Checks do not emit installable apps; Gradle/Xcode packaging must also perform compilation and linking.

The future local entry points should be the committed Android Gradle wrapper and iOS Xcode project. Gradle should invoke the pinned Rust/NDK integration automatically before `assembleDebug`, `installDebug`, and signed release tasks. Xcode should invoke the pinned Rust integration for the selected device/simulator target before linking and packaging. The developer should not have to copy Rust libraries manually.

Override the current repository-wide `target-cpu=native` setting in mobile checks and package tasks using target-appropriate flags. Preserve `-D warnings` and the required platform linker options. Keep the desktop configuration unchanged during the mobile bring-up. Use separate target/output directories per platform and build configuration. Bundle Bevy assets, fonts, shaders, icons, and license data in the application; keep writable records in the OS application-data directory.

**Android and iOS can be added to GitHub Actions.** Add sibling mobile jobs to the existing release workflow, with PR validation for the same package paths. Keep desktop compilation and publication independent while mobile support is being established.

| Job | Runner | Output and checks |
| --- | --- | --- |
| Android validation | Pinned Ubuntu image | ARM64 target check, emulator-target package, install/start/edit smoke test, alignment and asset checks. |
| Android package | Pinned Ubuntu image | Debug APK for testing; signed release APK and AAB on trusted release jobs. |
| iOS validation | Pinned macOS image and explicit Xcode version | Device target check, simulator app compilation/linking, simulator launch/edit smoke test, asset checks. |
| iOS package | Pinned macOS image and explicit Xcode version | Device archive and signed/exported IPA when signing is configured; simulator `.app` artifact separately. |
| Desktop regression | Existing supported desktop runners | Existing capabilities, checks, relevant UI/media smoke tests, and repeatable performance comparisons. |

Choose the simulator Rust target from the runner architecture. Pin an available stable runner/Xcode pair rather than assuming `macos-latest` preserves SDKs or architecture. Hosted Linux runners support Android emulator acceleration. Hosted runners do not replace physical-device validation. [GitHub runner capabilities](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)

The inspected `build.yml` currently enables Linux desktop releases; its Windows and macOS desktop entries are commented out. Its publishers classify downloaded binary artifacts as `kind: desktop`. Therefore mobile packages cannot simply be appended to the current artifact glob. Give mobile artifacts distinct names and publish them through explicit platform handling, without offering APK/IPA files to the desktop updater. Keep a failed mobile job from suppressing an otherwise valid desktop release.

Use the same package scripts locally and in CI. Cache by host, target, Rust version, lockfile, feature set, and SDK/NDK/Xcode version. Limit retained targets, incremental data, and debug symbols; measure disk and memory before enabling every architecture. If standard runners cannot hold the build, use a larger or self-hosted runner for packaging rather than weakening checks. Preserve symbols as separate artifacts for crash diagnosis.

Android release signing needs a keystore/upload key, alias, and passwords. iOS signing needs the selected certificate/private key, provisioning profiles, team/app identifiers, and matching entitlements. App Store Connect credentials are additionally needed for automated upload; an API key alone does not replace signing assets. Import signing material only into trusted jobs, use an ephemeral keychain, and clean up temporary credentials. PR checks must work without release secrets. [GitHub's Xcode signing procedure](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications)

Missing signing setup must be reported as an unavailable signed artifact, not as a successful installable release. Automatic store submission is a separate publishing decision. Compilation, release artifacts, and store publication should remain independently testable.

**Add Ailuros as an AR target, starting with a simple wearable display.** A see-through screen is a display capability. Content that stays attached to a real location also requires tracking, calibration, and appropriate optics. Treat these as separate milestones so useful software can be developed before the headset is finished.

| Milestone | Visible behavior | Required capability |
| --- | --- | --- |
| View-following display, or HUD | Selected records, notifications, and controls remain in a fixed part of the wearer's view. | Working display and optics; no room tracking required. |
| Orientation-aware display | Content responds to head rotation. | Calibrated motion sensors, timestamped samples, and orientation estimation. This does not establish a stable position in the room. |
| Room-anchored AR | A panel stays beside a real desk while the wearer turns and walks. | Reliable position and orientation tracking, coordinate calibration, tracking-loss recovery, and anchors. |

An IMU measures motion but does not by itself provide dependable long-term room position. Existing AR systems combine camera observations and inertial measurements for world tracking. Head direction is also not eye tracking; begin with head-pointing plus an explicit button or phone confirmation. [ARCore tracking](https://developers.google.com/ar/develop), [ARKit tracking](https://developer.apple.com/documentation/ARKit/managing-session-life-cycle-and-tracking-quality)

**Prefer a Pi-class computer for the first Bevy-powered Ailuros.** A Raspberry Pi 5 or Compute Module 5 provides Linux and a GPU with Vulkan support, making it a credible Bevy prototype target. That is a hardware-based inference, not a verified Lince result. Test the exact board, OS/Mesa version, display controller, and renderer configuration before choosing it for the wearable. Heavy desktop effects, software video encoding, and tracking may exceed its heat, power, or frame-time budget. The compute board can initially live off the headset while the display and sensors sit on it. [Raspberry Pi 5 specifications](https://www.raspberrypi.com/products/raspberry-pi-5/)

An ESP is useful for motion sensors, buttons, haptics, power management, and possibly a simple local display. Even the ESP32-P4's documented display support and pixel-processing accelerator are not a supported Vulkan/wgpu graphics backend. Do not make running Lince's full Bevy renderer on an ESP a v1 dependency. For an ESP-only HUD, evaluate a small Rust renderer such as `embedded-graphics`, sharing compact data and input messages rather than the complete Bevy UI. Exact display throughput and available drivers depend on the selected chip and panel. [ESP32-P4 display processing](https://docs.espressif.com/projects/esp-idf/en/release-v5.4/esp32p4/api-reference/peripherals/ppa.html), [embedded-graphics](https://docs.rs/embedded-graphics/latest/embedded_graphics/)

Recommended first hardware arrangement: Pi-class compute renders the display; an optional ESP supplies sensors and controls over a local link. A phone can provide pairing, settings, keyboard input, and selected Lince data. Use USB/serial for the initial sensor integration to simplify timing and debugging. Wireless or remote rendering can follow measured bandwidth and latency tests. A phone carried in a pocket cannot supply head pose directly; tracking sensors must describe the headset's motion or have a known rigid relationship to it.

**Reuse XR software before building tracking or a runtime.** The inspected `bevy_oxr` main branch targets Bevy 0.19 and wgpu 29, matching Lince's current version family. Evaluate and pin a tested revision for OpenXR-capable hardware. It is a community integration; its Android activity dependency and graphics setup still need compatibility checks. [Bevy XR dependency declarations](https://github.com/awtterpip/bevy_oxr/blob/main/Cargo.toml), [OpenXR adapter dependencies](https://github.com/awtterpip/bevy_oxr/blob/main/crates/bevy_openxr/Cargo.toml)

OpenXR connects an application to a runtime; it does not provide a driver, optical calibration, or tracking for an arbitrary custom display. For Linux, evaluate Monado and its existing driver/tracking integrations once the Ailuros sensors and optics are known. Monado is not an all-Rust stack. Under the Rust-first policy, its native implementation can remain behind the device adapter if it saves substantial work. Begin the simple Pi HUD with ordinary display output if no suitable XR runtime exists, without claiming it already supplies full AR. [OpenXR application/runtime boundary](https://www.khronos.org/openxr/), [Monado capabilities and hardware](https://monado.freedesktop.org/)

For optional phone AR, evaluate ARCore on supported Android devices and ARKit on iOS through native adapters. These are separate from a custom Linux headset runtime. A phone's AR support does not automatically transfer to Ailuros. Keep the normal phone application available on devices without AR support. [ARCore native API](https://developers.google.com/ar/reference/c), [optional Android AR](https://developers.google.com/ar/develop/java/enable-arcore)

**The first AR software work can happen on a normal desktop.** Add an optional simulator target under `devices`, with a Bevy window, simulated head motion, recorded sensor input, and one record card. Reuse only the needed presentation primitives from `interface`. Exercise selection, activation, recentering, disconnects, stale samples, and tracking loss before attaching hardware. This simulator is for application behavior; it cannot validate optics, real sensor accuracy, or motion-to-display latency.

Keep spatial utilities in an optional module of `interface`: explicit coordinate spaces and units, timestamped pose samples, tracking validity, input intents, and display capabilities such as mono/stereo and view-following/room-anchored modes. Do not make every host support every capability. Use separate layout rules for angular readability and spatial placement instead of shrinking the desktop canvas into the glasses. Host-specific tracking, camera import, projection/distortion, and frame submission stay in `devices`.

The XR host must use the display/runtime's frame timing and predicted pose where available. It may render continuously while tracking is active. That must not change the desktop's event-driven idle behavior. Keep sensor/media queues bounded, discard stale samples, and record timing for diagnosis. Agree on measured latency, drift, refresh rate, thermal, and power targets after the first display is selected; a desktop FPS number does not establish wearable quality.

The optical path needs its own calibration: field of view, eye position, display orientation, and per-eye projection if stereo. A transparent panel placed near the eyes is not a complete near-eye optical system. On additive see-through displays, black does not create an opaque background, so the current desktop theme cannot be assumed to remain readable. Keep a direct hide/recenter control and make tracking loss visible instead of presenting stale content as correctly anchored. [See-through rendering behavior](https://learn.microsoft.com/en-us/windows/mixed-reality/develop/advanced-concepts/rendering-overview)

Pair Ailuros through Lince's existing identity and access model. Showing a record on the device must retain that person's permissions; a sensor connection is not authorization to browse an organ. Raw pose streams, camera frames, and room maps should remain temporary/local by default, with explicit sharing for any persisted or synchronized spatial content. Request camera and microphone access only when a feature needs it.

Keep the current two-additional-crate limit: spatial utilities belong to `interface`, and the simulator/Pi/OpenXR adapters belong to `devices`. Do not add an internal `ar` or `xr` crate just for organization. ESP firmware is a later hardware-specific deliverable, not a promised target of the standard Bevy application. If that work requires an additional firmware crate, revise the scope and crate limit explicitly rather than concealing another Cargo package in a subfolder.

For local Pi validation, add a 64-bit Linux board, its supported graphics stack, display/controller, and eventually timestamped sensors. A screen on the desk is sufficient for the first output test. Add an `aarch64-unknown-linux-gnu` check/package job when the Pi host exists, using a matching sysroot or ARM64 Linux runner. Keep its Ailuros artifact separate from desktop releases. CI can replay poses and test UI behavior, but the actual Pi/display must validate rendering and latency. Phone AR also needs supported real devices.

**Implement in this order, with a usable result at every stage.**

1. Capture the desktop functional/performance baseline, then rename the existing crate/path/package to `desktop`/`lince-desktop`. Update workspace dependencies, Rust imports, examples, tests, scripts, build-time paths, and workflow path filters together. Introduce the small shared `interface` library without moving all UI into it. Completion: the current desktop application retains its behavior and its existing checks remain green.
2. Audit device dependency graphs and native build scripts, split desktop-only Cell/media dependencies, and pin the initial toolchains. Create `devices`, extract the minimum shared utilities, and package a Bevy screen on Android and iOS. Connect one real record and thread to the backend. Completion: both physical devices open the app, edit locally, restart without losing data, and synchronize.
3. Prove phone input, accessibility, graphics, and lifecycle. Test composition, selection, clipboard, safe areas, back navigation, rotation, TalkBack/VoiceOver, Activity recreation, suspension, and process death. Use small native bridges where required. Completion: the screen is useful on a phone and desktop baselines still pass.
4. Build the small Ailuros desktop simulator and the optional spatial utility boundary. Use simulated/replayed input and one record card; do not make phone delivery wait for custom tracking or optics. Completion: selection, recentering, and tracking-loss behavior can be exercised without a headset.
5. Extract shared presentation for organ access, records, search, and threads in small steps. Add permissions, secure storage, file sharing, and network recovery. Completion: a core mobile beta, with desktop-only features retained in `desktop` and unsupported phone capabilities stated clearly.
6. Complete reproducible local packaging and Actions jobs. Start unsigned/simulator CI early in stage 2, then finish release signing, artifact separation, symbols, and package installation tests here. Completion: a fresh checkout can reproduce the documented packages, and trusted CI produces the same artifacts.
7. When the display hardware is available, bring the Ailuros HUD to the selected Pi, add real controls and sensor input, and measure the result. Completion: readable output, working controls, and documented timing/power measurements. Hardware availability must not block the following phone work.
8. Add mobile audio calls, then camera and screen sharing. Implement background incoming calls, platform audio sessions, recovery, and coordinator behavior. Completion: real-device calls survive the supported interruptions and meet measured quality/power targets.
9. Evaluate existing OpenXR/runtime support on the selected Ailuros hardware, then add orientation-aware content and finally room anchors. Completion: each claimed tracking mode is demonstrated with calibration, tracking-loss recovery, and measured stability. Full room tracking remains a separate delivery milestone.

At each stage, keep no more than two additional Cargo crates: shared `interface` and `devices`, alongside the renamed existing `desktop`. Change existing backend crates where their boundaries require it. Use modules and binary/example targets for Android, iOS, Ailuros, and simulation.

**Estimated effort and criticism.** For one experienced full-time developer with the required machines and phones, budget 2–3 weeks for the first two-phone feasibility result, 12–20 weeks total for the core phone beta, another 4–8 weeks for reliable mobile audio/ringing, and another 6–12 weeks for camera/screen sharing and performance work. These are estimates, with substantial uncertainty until the initial device tests pass; the incremental crate extraction is included.

Budget an additional 2–4 weeks for the Ailuros simulator and first shared spatial utilities, and roughly 3–6 more weeks for a Pi HUD once a working display/driver is available. These AR estimates exclude electronics, optics, firmware development, and full room tracking. Doing these steps before the phone beta/calls adds their time to those delivery dates. Reliable custom six-degree tracking could become a months-long project; estimate it separately after selecting sensors and testing an existing runtime. Do not fold that uncertainty into a promise of a quick headset port.

- The rename clarifies ownership, but only disciplined extraction makes the shared crate useful. Keep it small until another host actually needs a component.
- Sharing Bevy components does not mean sharing every layout or control. Phone navigation and keyboards need deliberate design.
- Rust-first should reduce duplicated logic, not force us to maintain fragile replacements for standard phone services. Keep native bridges narrow and test their boundaries.
- No architecture can promise zero new bugs or zero performance regressions. The enforceable commitment is to preserve desktop capabilities and reject demonstrated regressions before accepting each change.
- Successful checks and simulator launches are necessary but insufficient. Real keyboards, audio routes, GPUs, background restrictions, and battery use require physical devices.
- Mobile hosting changes the always-running Cell assumption. That must be resolved explicitly for synchronization and calls, regardless of how much UI code is shared.
- AR adds timing, tracking, optics, and power constraints beyond mobile UI work. An ESP-only product and a Pi-powered headset have different rendering capabilities and must not be promised the same software stack.

The recommended outcome is `desktop` retaining the current full application, a gradually extracted shared `interface`, and `devices` hosting Android, iOS, and Ailuros. Start Ailuros software with a simulator and simple HUD, while keeping full spatial tracking a distinct, hardware-dependent milestone.
