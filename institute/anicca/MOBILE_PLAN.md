**Lince task handheld, Android, iOS, and Ailuros with Bevy**

This is an implementation plan, updated on 2026-09-26. The crate rename, shared-crate extraction, device UI, and mobile jobs described here have not been implemented. The current desktop application still lives in `crates/interface`. This revision changes the plan only.

The first device is a handheld for managing Karma, Frequencies, and Records throughout the day in Porto Alegre, Brazil. It needs a rechargeable battery, touch input, text entry, and internet without tethering to the user's existing phone. Prefer e-ink, with an ordinary touchscreen as a fallback if editing is too slow. No hardware spending limit has been specified. Use eight hours of intermittent daily use as an initial battery test target, not a promised runtime. AR glasses, 3D, and calls are later features; they must not delay this task device.

**Start with an Android e-ink device with its own cellular modem and SIM, if the exact model passes the checks below.** This reuses an assembled battery, charging system, touchscreen, antennas, and power management. Use the touchscreen keyboard for quick changes and an optional small Bluetooth keyboard for longer writing. Keep a Linux/Pi assembly path for learning and custom hardware, but test the UI before committing to a custom enclosure or raw e-paper panel. Keep iOS in the shared software plan; the first e-ink hardware experiment is Android or Linux.

**Recommendation: keep Bevy, share the presentation code, and add at most two new internal crates.** Use Rust for the UI, application logic, and reusable platform integration. Use small Kotlin, Java, Swift, or Objective-C bridges where they make phone behavior reliable. Keep the existing Rust media implementation. Other languages being acceptable does not create a reason to replace it with libwebrtc again.

Bevy 0.19.1, currently pinned by Lince, has Android and iOS examples. Start with that version and upgrade only for a demonstrated need, with desktop verification. [Bevy mobile examples and packaging](https://github.com/bevyengine/bevy/blob/v0.19.1/examples/README.md#platform-specific-examples)

**Rename the current `interface` crate to `desktop`, and use `interface` for shared utilities.** Keep the current application working in `desktop`, then extract a little code at a time when a second host needs it. The limit remains two additional workspace crates after the rename; existing backend crates still receive the capability and portability changes they need.

| Location | Responsibility |
| --- | --- |
| `crates/desktop`, package `lince-desktop` | The existing `interface` crate, renamed. Initially retains all current UI and desktop behavior. Continues owning desktop startup, window/tray handling, single-instance behavior, and desktop-specific presentation and features. |
| `crates/interface`, package `lince-interface` | A small shared library introduced after the rename. Reusable Bevy components, theme primitives, layout, focus, touch/keyboard input, forms, and accessibility. Move individual widgets or Sands here only when useful to more than one host. |
| `crates/devices`, package `lince-devices` | Android, iOS, and Ailuros handheld hosts: startup, lifecycle, permissions, native services, display adapters, and packaging projects. A later wearable can use another target within this crate. |
| Existing Cell, Engine, Store, Transport, Media, and other domain crates | Continue owning their existing responsibilities. Make desktop-only dependencies optional or target-specific where necessary. |

Both `desktop` and `devices` depend on the shared `interface`; neither `interface` nor the backend depends on a host. `devices` must not depend on `desktop`. Begin with generic utilities, keeping application controllers in their hosts until there is a concrete reason to share them. Reuse the existing typed backend operations and extract the reusable parts of the Cell bridge as needed. Moving Rust code across a crate boundary does not require IPC, another runtime, or a new serialization layer.

Put Android Rust adapters in `crates/devices/src/android`, iOS adapters in `crates/devices/src/ios`, and Ailuros adapters in `crates/devices/src/ailuros`. Put the Gradle project and Kotlin/Java sources in `crates/devices/android`, and the Xcode project and any Apple-language sources in `crates/devices/ios`. Ailuros can have its own binary target and packaging folder inside the same crate. These are folders and targets, not additional Cargo crates. Keep platform features and dependencies separate. Existing media capture interfaces can be implemented by the device adapters. Reusable media changes remain in `crates/media`.

Each host owns its Bevy application, event loop, runtime initialization, and platform resources. The shared crate exposes plugins and components; it does not open a window or call `App::run`. Keep one Bevy application and one UI world per active host. Android Activity recreation must not accidentally create duplicate Cell runtimes or subscribers. Follow the platform event-loop threading rules instead of putting the Bevy runner inside an arbitrary async worker.

**The shared `interface` should grow from real reuse.** Keep clear modules for widgets, theme, input/focus, layout, and accessibility. It can eventually contain a reused Sand or presentation system, but it does not have to own the entire application UI. The goal is a useful common library for Lince's hosts, without building a general-purpose UI framework or filling it with unrelated helpers.

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

**The first useful UI must create, read, update, and delete all three kinds of data.** Start with a Today view, a board/list switch, search, and quick creation. Make Records, Karma, and Frequencies directly reachable. Use the same domain meanings and permissions as desktop. A task view is a view of existing Lince data, not a second task database.

| Screen | Required controls and behavior |
| --- | --- |
| Records | Search, open, create, edit text and relevant properties, change task status, and delete with a clear confirmation. Keep unsaved drafts when navigating away. |
| Karma | List and inspect rules; create and edit Condition, Threshold, and Consequence; choose referenced Records/Frequencies; show validation and the effect of changing shared fields. Expose the relevant existing execution controls. Include real removal semantics; pausing execution is not deletion. |
| Frequencies | List, create, inspect, edit, and delete; provide touch controls for common expressions and retain access to the complete supported expression. Where applicable, preview upcoming times through the existing backend, with an explicit time zone such as `America/Sao_Paulo`. Explain when a Frequency cannot be deleted because something uses it. |
| Kanban | Show existing task columns; move a card through a drag handle or a “Move to…” action. On a narrow screen show one readable column with column navigation; allow several columns on wider screens. Persist the same domain change desktop uses, not just the card's visual coordinates. |
| Save and sync state | Distinguish an unsaved draft, saved locally, pending synchronization, conflict, and rejected change. Provide retry/review actions without duplicating a creation or rule execution. |

The current implementation already provides useful pieces in `crates/interface/src/karma_castle.rs`, `frequency_castle.rs`, `kanban.rs`, and `crates/engine/src/actions.rs`. Karma saving uses `SaveKarmaRule` and `ReviseKarmaField`; Frequency editing uses `SaveKarmaFrequency`, and `DeleteFrequency` reports in-use failures. Records have `DeleteRecord`. The inspected action list does not expose a dedicated `DeleteKarma` operation: verify rule/field lifecycle and references, then implement any missing backend removal and authorization before exposing a mobile delete button. Do not assume generic record deletion also stops every related execution or safely removes shared fields.

The existing Kanban is built from Areas and seven task statuses. Reuse its membership/status behavior instead of introducing a disconnected three-column model. Moving into an Area may have effects beyond layout; expose those effects where relevant and retain the backend's checks. Record ordering, status changes, and editing a shared Karma field need separate, explicit operations and conflict handling.

**Share controls and editing behavior; let each host choose its layout.** Extract the next needed button, text field, search/select menu, validation message, focus helper, card, or scroll container into `interface`. Share draft handling and typed UI intents when both hosts use them. Keep business validation, permissions, recurrence interpretation, and mutation execution in the backend. Keep handset navigation, keyboard avoidance, native services, and display refresh policy in `devices`. The desktop canvas, full menus, shortcuts, media, and 3D remain available in `desktop`.

Touch needs deliberate gesture handling: a normal swipe scrolls; a drag begins from a handle or deliberate hold, has a movement threshold, and can be cancelled. Support edge scrolling and drop feedback without saving intermediate positions. Menus must fit inside the visible area above the keyboard. Every drag operation also needs a tap/keyboard alternative. Test Portuguese accents, `ç`, composition, emoji, selection, clipboard, keyboard layouts, and physical-keyboard focus. Native text-entry integration is necessary even when Bevy draws the form.

**E-ink needs its own presentation settings.** Prefer dark text on a light background, solid borders, generous text, and labels/icons that work without color. Remove decorative transitions, blinking activity indicators, and continuously moving backgrounds from this mode. Offer page-sized movement for long lists alongside touch scrolling. Use a simple outline during card dragging and update the final layout after the drop. Keep drag and scroll available, but accept them only after testing on the selected display.

On a normal lock or suspend, replace private task content with a neutral screen where the platform allows it. E-ink can retain the last image without power, so a stopped renderer does not itself hide the displayed Records. Validate the device's lock-screen behavior as well as the application setting. [E Ink image retention](https://www.eink.com/tech/detail/Benefits)

Redraw Bevy when input or visible data changes, and pause rendering when the app is hidden. Separate UI wake-ups from network/background work so a saved task does not require a permanently running frame loop. The device adapter may request fast or clean screen updates if a supported vendor API exists; otherwise use the device's per-app refresh settings. Bevy rendering a frame does not itself choose the e-paper panel's electrical refresh sequence. Test ghosting, keyboard latency, and full-refresh behavior on the real device. [BOOX per-app display settings](https://booxsupport.zendesk.com/hc/en-us/articles/10701299044372-App-Optimization)

Use a 2D Bevy feature set on devices: UI, text, images, input, and the necessary renderer. Exclude desktop PBR, splats, physics, capture, and calls from the first task package. Bevy's normal 2D rendering still requires a supported graphics path; selecting 2D does not make an ESP or a raw SPI e-paper panel a supported Bevy target. Keep these restrictions out of desktop defaults.

**Offline editing and sleep are part of the first release.** Persist drafts and accepted local edits during use, not only when closing the app. Audit synchronization and request deduplication rather than assuming a new retry queue is sufficient. Recheck access at the authoritative boundary; a queued edit must not bypass a revocation. If a permission cannot be established offline, preserve a draft and clearly defer the mutation.

The handheld may sleep or lose its process. It must not become a second uncontrolled Karma executor. Reuse executor designation and execution identity, and decide explicitly how missed occurrences are handled on resume. Rules that must run while the handheld sleeps need an authorized, reachable Cell that stays running. Local reminders are a separate OS notification feature; Android/iOS background restrictions must not be presented as continuous execution. Preserve battery by batching sync, backing off failed connections, and deferring large attachments on mobile data. Threads and file sharing can follow the task beta.

Calls follow the core app. Reuse media protocols and processing, but implement phone microphone/camera access, audio routing, interruptions, background calls, and screen sharing separately. Capture remains user initiated. Benchmark software video encoding and multi-device calls for heat and battery use; platform hardware codecs may require additional negotiation and desktop support.

A suspended phone is not a dependable always-running coordinator. Lince's current 30-second coordinator timeout makes this an explicit call-design issue. Reliable incoming iPhone calls need APNs/PushKit and CallKit; Android needs the corresponding calling/background integration. Foreground offline LAN calls remain a target, but offline wake-up of a suspended phone must not be promised. Push delivery is separate from peer-to-peer media. [Apple incoming calls](https://developer.apple.com/documentation/pushkit/responding-to-voip-notifications-from-pushkit), [Android calling integration](https://developer.android.com/develop/connectivity/telecom/voip-app/telecom)

**Local Android development can use the current Linux machine.** Windows or macOS can also host the Android tools.

| Requirement | Purpose |
| --- | --- |
| A fixed Rust release selected for the device work | `rust-toolchain.toml` currently selects `stable`, not a fixed version. Pin and validate one version for reproducible local and CI results. |
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
| The same selected Rust release and `aarch64-apple-ios` | Physical iPhone/iPad code. |
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

The inspected `build.yml` currently has an active Linux desktop matrix. Its publishers classify downloaded binary artifacts as `kind: desktop`. Therefore mobile packages cannot simply be appended to the current artifact glob. Give mobile artifacts distinct names and publish them through explicit platform handling, without offering APK/IPA files to the desktop updater. Keep a failed mobile job from suppressing an otherwise valid desktop release.

Use the same package scripts locally and in CI. Cache by host, target, Rust version, lockfile, feature set, and SDK/NDK/Xcode version. Limit retained targets, incremental data, and debug symbols; measure disk and memory before enabling every architecture. If standard runners cannot hold the build, use a larger or self-hosted runner for packaging rather than weakening checks. Preserve symbols as separate artifacts for crash diagnosis.

Android release signing needs a keystore/upload key, alias, and passwords. iOS signing needs the selected certificate/private key, provisioning profiles, team/app identifiers, and matching entitlements. App Store Connect credentials are additionally needed for automated upload; an API key alone does not replace signing assets. Import signing material only into trusted jobs, use an ephemeral keychain, and clean up temporary credentials. PR checks must work without release secrets. [GitHub's Xcode signing procedure](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications)

Missing signing setup must be reported as an unavailable signed artifact, not as a successful installable release. Automatic store submission is a separate publishing decision. Compilation, release artifacts, and store publication should remain independently testable.

**Choose the display and computer together.** E-ink holds an image without continually powering the pixels, but its controller, lighting, computer, and cellular radio still consume power. Frequent refreshing also costs energy. It suits text and occasional changes; it does not by itself make a Pi handheld last as long as a sleeping e-reader. [E Ink's explanation of image retention](https://www.eink.com/tech/detail/Benefits)

An ordinary Android e-ink device provides a normal application surface, so it is a plausible host for Bevy. That is an architectural inference, not a successful Lince test. Verify the exact GPU/driver, package installation, native text input, surface recreation, and vendor refresh behavior. A screenshot or a successful compile does not prove that typing and dragging feel good.

| Hardware route | Fit for this task | Main limitation |
| --- | --- | --- |
| Android e-ink handheld with cellular data | First choice to evaluate: touch, battery, charging, modem, and antennas are already integrated. | Exact Brazilian approval, bands, firmware support, and Bevy behavior remain purchase gates. |
| Ordinary Android phone or cellular tablet | Practical fallback if e-ink editing disappoints; fastest route to responsive drag/scroll. Can be a dedicated Lince device with its own SIM. | Display power and sunlight readability depend on the model. |
| Linux e-ink tablet plus pocket cellular router | Reuses assembled display, battery, and Linux hardware while avoiding dependence on the existing phone. | Two devices to charge, a larger tablet, and Linux graphics/suspend validation. |
| Raspberry Pi handheld assembled from modules | Best learning/customization route. Begin with a supported LCD touchscreen. | More weight, wiring, heat, charging work, and uncertain standby battery life. |
| Raw e-paper panel plus Pi or ESP | Useful later for a custom low-power design or small task display. | Controller, touch layer, refresh support, and renderer integration are separate work. Not the first daily-use device. |

Two current Android candidates are the **Bigme HiBreak Pro**, with a 6.13-inch monochrome screen, Android 14, cellular connectivity, and a 4,500 mAh battery; and the **BOOX Palma 2 Pro**, with a 6.13-inch color screen, Android 15, a data-only SIM facility, and a 3,950 mAh battery. The monochrome candidate better matches a text-first UI; the BOOX is also worth a device trial. These are a shortlist, not verified purchase recommendations. Neither battery figure establishes Lince runtime. [Bigme specifications](https://store.bigme.vip/pages/hibreak-pro-landing-page), [BOOX specifications](https://shop.boox.com/products/palma2pro)

No Anatel homologation has been verified in this research for either exact model. The inspected BOOX page lists several overseas certifications, which do not establish Brazilian approval. Obtain the exact model/SKU, homologation number, full LTE band list, installation policy, warranty, and current availability before ordering. Do not assume another Palma or HiBreak variant has the same radio. Imported price, freight, taxes, and return costs also need a Brazilian quote.

For Linux, the **PineNote** is an existing 10.3-inch touch e-ink tablet with a battery, GPU, Wi-Fi, and Bluetooth, but no listed cellular modem. It is an alternative to building a display assembly from scratch, particularly if a larger device is acceptable. Its documentation includes Linux/Wayland development; this does not establish compatibility with Lince's Bevy version. Test current firmware, graphics, touch, suspend, and external connectivity first. [PineNote hardware](https://pine64.org/devices/pinenote/), [Linux application notes](https://pine64.org/documentation/PineNote/Development/Apps/)

A raw e-paper panel is not an HDMI monitor. Many use SPI, a small serial connection, and require a panel-specific controller and refresh sequence; touch may be absent. For example, Waveshare's small touch e-paper documentation includes partial/full refresh restrictions. Its 10.3-inch HDMI e-paper monitor is a different product class and does not establish a pocket-sized touch solution. No compact Pi e-ink/touch kit has been qualified here. [Touch e-paper requirements](https://www.waveshare.com/wiki/2.13inch_Touch_e-Paper_HAT_Manual), [HDMI e-paper monitor](https://www.waveshare.com/wiki/EINK-DISP-103)

If a raw panel is pursued later, first seek a maintained Linux display driver and matching controller. Only then consider an adapter that transfers rendered images to the panel; GPU readback, grayscale conversion, changed-region detection, and refresh scheduling add work and power costs. Prefer an existing Android/Linux display stack before writing this. An ESP-only version would need a smaller renderer such as `embedded-graphics` and a separate evaluation of storage, security, and networking; full Bevy UI reuse must not be promised. [Rust embedded graphics](https://docs.rs/embedded-graphics/latest/embedded_graphics/)

**Keyboard recommendation: touch first, physical optional.** Quick status changes and short titles fit a touchscreen keyboard. For longer Record text or Karma expressions, add a small rechargeable Bluetooth keyboard with a stand; test Portuguese characters and symbols. It needs its own charging but leaves USB free. A USB keyboard avoids pairing and keyboard batteries but needs a supported USB host/OTG connection, cable, and possibly a charging hub. A built-in keyboard complicates the enclosure and narrows the device choices. Do not make one a v1 requirement before trying the workflow.

**Independent internet in Porto Alegre.** In Brazil, the “chip” is the SIM: it identifies a subscription. It does not contain the complete radio needed to reach a cell tower. The device needs a compatible modem, antennas, active SIM/data plan, coverage, and power. Android cellular hardware includes these parts; a normal Raspberry Pi does not include a cellular modem.

| Connection | Independent of the existing phone? | What to buy/use |
| --- | --- | --- |
| Built-in cellular modem | Yes; recommended for one handheld. | Compatible cellular Android device plus its own SIM and data plan. |
| USB LTE modem | Yes. | Linux-supported modem, SIM, antennas where required, and adequate USB power. |
| Pocket 4G router, often called MiFi | Yes, though it is a second device. | Router with its own rechargeable battery and SIM; handheld connects through Wi-Fi. |
| Home/work/public Wi-Fi | Yes within range; no independent street coverage. | The handheld's Wi-Fi and an available access point. |
| Phone hotspot | No. | Existing phone shares its connection; optional fallback only. |

A pocket router is not the same as tethering to the phone: it has its own cellular radio and subscription. A USB modem may instead expose a network adapter or its own small router. On Linux, check its exact USB mode and support in NetworkManager/ModemManager; “works with Windows” is insufficient. Prefer an assembled modem to a bare M.2/mini-PCIe radio module, which also needs a suitable adapter, SIM holder, antennas, and power design. [ModemManager device types](https://mobile-broadband.pages.freedesktop.org/docs/modemmanager/wwan-device-types/), [Claro mobile internet equipment](https://www.claro.com.br/faq/suporte/como-funciona-a-instalacao-do-claro-internet-movel)

Use these steps before committing to a carrier or modem:

1. Compare Vivo, Claro, and TIM coverage at the actual home/work addresses and daily routes in Porto Alegre. Use maps as a starting point and test indoors and while travelling with a prepaid SIM where possible. City-level coverage does not establish reliability at a particular desk. No operator is ranked as best by this research. [Anatel coverage maps and limitations](https://sistemas.anatel.gov.br/se/public/cmap.php)
2. Check the exact device's Brazilian approval and supported bands. LTE B3, B7, and B28 are important Brazilian bands to include in the check, but not a complete nationwide compatibility guarantee. Check the selected operator's local deployment and exact regional hardware variant. Buy locally homologated equipment where possible; a module's certification does not automatically prove a whole custom product is approved. [Anatel acquisition guidance](https://www.gov.br/anatel/pt-br/regulado/certificacao-de-produtos/orientacoes-para-aquisicao-de-aparelhos-celulares), [Anatel LTE band reference](https://informacoes.anatel.gov.br/legislacao/instrucoes-de-fiscalizacao/1824-port-2521)
3. Get a separate line with ordinary internet data. Prepaid, control, or postpaid may work; confirm the plan permits the actual reader/modem/router. Some data-only offers are for businesses. A messaging-app-only allowance is insufficient for Lince. Start modestly and measure synchronization traffic; attachments can dominate usage.
4. Register/activate the SIM using the carrier's process, normally including personal identification. Some activation flows require SMS or a call, so arrange store-assisted activation for a device without those functions; temporary use of another phone for setup does not create ongoing tethering dependence. Confirm the carrier's APN, the setting that selects its data service. [Example Claro activation procedure](https://www.claro.com.br/faq/pre-pago/como-ativar-e-cadastrar-o-chip-pre-pago-da-claro)
5. Verify an actual Lince save/sync with Wi-Fi off and the existing phone switched off. Then test loss of signal, exhausted data, reconnection, and switching to Wi-Fi. Keep local task work usable during outages.

4G is sufficient for task text and synchronization; 5G is optional. Avoid making a cheap 2G-only modem the basis of a new product. Physical SIM is the simplest starting point; eSIM still requires compatible radio hardware and carrier provisioning. LTE-M/NB-IoT and LoRa are specialized connectivity paths, not the default route to a general Lince app.

“Roaming” means using another carrier's network under your plan; sharing a phone's internet is tethering or “rotear.” This device avoids tethering but still depends on carrier coverage and an active subscription. It is not independent of cellular infrastructure.

Do not require a public IP or port forwarding from the mobile carrier. Validate Lince's existing Iroh direct/relay paths on mobile networks, including restrictive NAT, which may prevent direct incoming connections. Internet access alone does not prove peer synchronization works, and a relay must be reachable when required. This is separate from any future call-media connectivity. [Iroh direct and relay connections](https://www.iroh.computer/blog/iroh-0-91-0-the-last-relay-break)

**What to buy and assemble.** For the recommended integrated route, the initial kit is one verified cellular Android e-ink device, one SIM/data plan, a compatible USB-C charger/cable, and a protective case. Add a keyboard/stand only if longer writing justifies it. No extra Pi, ESP, modem, soldering, or custom battery circuit is needed. An ordinary Android device can prove the app before the e-ink purchase.

For a Pi learning prototype, buy in stages:

| Part | Initial choice and reason |
| --- | --- |
| Computer | One Raspberry Pi 5, initially 4–8 GB RAM, supported 64-bit Linux, and cooling. This is a reference test board, not a proven low-power final design. |
| Storage | One reliable microSD card, initially 64 GB, plus a reader for installation and recovery. Size the final choice from actual local data. |
| Screen | A 5-inch Raspberry Pi Touch Display 2 for the first assembled touch prototype, with the correct Pi 5 ribbon and power connections. It is LCD, not e-ink. A 7-inch version offers more room at greater size. |
| Input | Touch keyboard plus an optional compact USB/Bluetooth keyboard. |
| Cellular | Start with a homologated pocket LTE router, or an exact Linux-tested USB LTE modem. Buy the matching SIM and any manufacturer-required antennas. |
| Power | Begin with the official wall supply. After measuring the complete load, select an enclosed rechargeable battery solution explicitly compatible with that load and the Pi's power input. |
| Connections | Correct display/USB cables and, only if needed, a powered USB hub with documented power behavior. |
| Enclosure | Case or printed shell, spacers, fasteners, cable strain relief, ventilation, and a reachable power button. Fit around measured component sizes. |
| Tools | Small screwdrivers and a USB power meter; add a multimeter when learning wiring. A soldering iron is not required for an initial module-based assembly. |

The official Touch Display 2 provides a supported screen/touch starting point. A Compute Module is a compact computer module requiring a carrier board; consider it after the larger prototype proves useful. Neither a camera nor motion sensors are needed for this task UI. [Touch Display 2 hardware](https://www.raspberrypi.com/products/touch-display-2/)

Assembly order: prove Linux and Lince on wall power; attach and test touch; test the keyboard; add the independent network; measure consumption; select and test the battery/charging path; then fit the enclosure. Test sleep/wake and safe shutdown before relying on it for daily data. Do not design a custom board or order an enclosure before the screen/controller/power combination works.

**Battery selection needs energy and power measurements.** Watts describe the instantaneous load; watt-hours describe stored energy. For illustration, a bank labelled 20,000 mAh at 3.7 V stores about 74 Wh. Assuming 80% reaches the device, about 59 Wh remains: roughly 7.4 hours at an average 8 W, or 4.9 hours at 12 W. These are arithmetic examples, not measurements of Lince or a specific bank. A separate pocket router has its own energy budget.

For Pi 5, verify the actual 5 V output profile and cable, not just an advertised “65 W” bank rating that may apply at a higher voltage. Raspberry Pi recommends 5 A capability for the full peripheral budget; with a 3 A supply the default USB allowance is lower. Modems can create short load peaks. A high advertised capacity does not guarantee stable voltage or that plugging in a charger avoids rebooting. [Raspberry Pi power requirements](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#power-supply)

Prefer an enclosed protected pack or a documented Pi battery/UPS product. UPS here means a supply that can keep the device running when external power disappears. Verify charging while operating, low-battery reporting, cutoff, thermal behavior, and orderly shutdown. A charger board alone is not a complete protected battery system. Do not start by assembling loose lithium cells.

For both Android and Linux, measure active editing, a static screen, screen-off standby, weak cellular signal, and reconnection. Test an eight-hour day with a recorded number of edits and syncs, front-light setting, and remaining charge. Screen savings matter only if CPU/GPU activity, background work, and radio use also fall. Do not promise multi-day life from the word “e-ink.”

**Electronics knowledge to learn, in order.**

| Area | Plain meaning and why it matters |
| --- | --- |
| Basic electricity | Voltage is electrical pressure, current is the flow, and power is their product. Learn polarity, ground, and how a short circuit differs from a normal load. |
| Batteries and charging | Learn Wh versus mAh, battery protection, charging versus powering a running device, and low-voltage shutdown. Begin with complete protected products. |
| Connectors | USB-C is a connector; charging, negotiated power, and data support differ. HDMI/DSI carry display output; touch may need another connection. Matching connector shape is not enough. |
| Modules and boards | A module packages one function; a carrier/adapter connects it to the rest. A modem module is not a complete battery-powered router. |
| Linux hardware support | A driver lets software control hardware. Learn device detection, logs, display configuration, network settings, and recovery after unplugging. |
| Cellular basics | SIM, modem, antenna, band, APN, coverage, and subscription all have separate roles. Learn to identify exact model variants and homologation numbers. |
| Measurement | Use a USB meter to observe voltage/current/power; learn safe multimeter use before probing circuits. Measure the assembled system, not only each part's advertised rating. |
| Mechanical work | Protect connectors and the screen; provide ventilation and room for the battery. Weight, grip, and cable routing affect daily usability. |
| Later custom electronics | GPIO voltage levels, I²C/SPI/UART buses, soldering, circuit diagrams, PCB design, and radio integration. These are not prerequisites for the integrated Android route. |

**AR stays in the later plan.** The handheld does not need a transparent screen, optics, head tracking, or 3D. Later Ailuros glasses can reuse the task data, permissions, and selected shared widgets. Start with a view-following HUD, meaning a panel that stays in the same part of the wearer's view. Room-anchored AR needs additional tracking and calibration. Evaluate existing OpenXR runtimes before custom tracking. Keep optional spatial utilities in `interface` and host/runtime adapters in `devices`; do not add a third new crate or make the task UI depend on XR. [OpenXR's role](https://www.khronos.org/openxr/)

**Implement sequentially, with a useful result at each stage.**

1. Record desktop functional/performance baselines. Rename the current crate to `desktop`, updating imports, dependencies, paths, scripts, and CI together. Introduce the small shared `interface`. Completion: desktop behavior and checks remain intact.
2. Split desktop-only dependencies, pin toolchains, and create `devices`. Package one real Record editor on Android and iOS. Start CI checks and simulator packaging here. Completion: input, save, restart, and synchronization work on physical devices.
3. Prove the same screen on a candidate e-ink device before further hardware commitment. Test native keyboard, touch, refresh, graphics recovery, and standby. Completion: an explicit keep-e-ink or use-LCD decision based on actual interaction and power.
4. Deliver complete Record, Karma, and Frequency CRUD, including missing backend operations, references, authorization, and conflict behavior. Extract only the shared utilities these screens need. Completion: the user can manage the day's data without returning to desktop.
5. Add the existing Kanban semantics, touch drag/scroll, “Move to…” alternative, search, and e-ink presentation. Completion: board operations survive restart and match desktop after sync.
6. Finish offline/lifecycle handling, executor ownership, accessibility, network recovery, metered-data behavior, and power tuning. Complete reproducible local packages, signing, and distinct Actions artifacts. Completion: a tested daily-use Android/iOS task beta.
7. If custom hardware remains desired, assemble and measure the Linux prototype in the order above, using the same shared UI. Add an `aarch64-unknown-linux-gnu` check/package with a matching sysroot or ARM64 runner and a separate Ailuros artifact. Complete battery and enclosure trials before claiming portability.
8. Expand threads/file sharing, then optional calls and AR as separate milestones. None is required to finish the task handheld.

Keep at most two additional internal crates, shared `interface` and `devices`, beside the renamed `desktop`. Existing backend crates receive the domain and portability changes they need.

**Acceptance checks.**

- Create, inspect, edit, and remove Records, Karma, and Frequencies; test referenced-data failures, permission denial/revocation, shared-field effects, concurrent revisions, and repeated submissions.
- Move cards by touch and by menu/keyboard; ensure a scroll or cancelled drag never changes a task. Verify the same resulting state on desktop.
- Kill/restart the app and interrupt network/power around saves. Recover saved data and drafts without duplicate mutations or duplicate Karma execution.
- Test Portuguese input, external-keyboard symbols, safe areas, rotation, accessibility, menu clipping, and OS keyboard appearance/dismissal.
- Test actual e-ink typing, drag feedback, ghosting, clean refresh, reading in sunlight/indoors, and front-light use. Measure rather than infer latency from Android or Bevy frame rates.
- In Porto Alegre, test with Wi-Fi and the existing phone off, then weak signal, no coverage, Wi-Fi/cellular changes, and relay-dependent synchronization.
- Record eight-hour intermittent-use battery trials, heat, wake time, memory growth, idle rendering, and data usage. Compare desktop baselines after each extraction.
- Run relevant Rust tests and locked `cargo check` with warnings denied; verify installed packages on physical Android/iOS devices and the chosen Linux hardware. CI alone cannot validate radio coverage, e-ink behavior, or battery life.

**Estimated effort and criticism.** For one experienced full-time developer with the test devices available, allow 2–3 weeks for the first Android/iOS editor feasibility result; roughly 6–10 weeks total for an initial Android task alpha; and 12–20 weeks total for a tested Android/iOS task beta with incremental extraction and CI. These are overlapping milestones, not durations to add together. Backend deletion/execution gaps or native text-input trouble could extend them.

Allow 1–3 additional weeks for e-ink tuning after obtaining a compatible Android device. A module-based Pi/LCD prototype may take a few days to assemble and another 2–4 weeks to validate the host, power, and enclosure after the shared app works. A compact custom e-ink design needs its own estimate after proving the panel/controller; budget months if drivers, a circuit board, or battery electronics must be developed. Buying an integrated device removes much of that hardware work. Shipping, homologation, store review, and learning time are outside these software estimates.

The strongest practical recommendation is an integrated cellular Android device with an e-ink trial, plus an optional keyboard. A Pi is a useful development and learning board, but not automatically a comfortable pocket computer. E-ink trades display behavior for lower static-screen power; it is not a guarantee of either smooth interaction or long total runtime. Rust/Bevy reuse should serve the editing experience, and desktop must retain its full features. AR remains a separate future device goal.
