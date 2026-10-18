# Ailuros camera: verified results, iroh transport and remaining work

## Execution hold

The owner requested: do not run Lince, do not start or change its systemd service, leave it down, wrap up the coding and report the remaining coding/tests. No desktop Lince was launched or installed. The hardware-check helpers finished before this hold. The unfinished native test compilation was cancelled; no new runtime or test run should be started without the owner's next instruction.

The proposed NixOS service overrides were reverted. The unactivated Lince drop-in was removed from systemd's search path and saved only as an inert proposal at /home/user/.config/lince/ailuros/pending/lince-service.conf. No daemon reload, Lince service start or restart was performed. The earlier temporary Caddy ingress was active at its last inspection; that is separate from the stopped Lince Cell, and this hold does not imply that all auxiliary services were stopped. Do not alter services as part of further planning.

The earlier USB-group/udev and Wi-Fi-interface TCP 8443 NixOS changes were already applied by the owner. No additional rebuild is requested now.

Planning update, 2026-10-18: the owner requested research and a Markdown revision only. The requested direction is camera delivery over iroh across Wi-Fi/address changes, removal of the Caddy dependency, and reuse of existing Rust libraries. Implementation, compilation, tests, flashing, secret handling and service changes are not authorized by this planning request. The steps below are a proposal for refinement before implementation.

## What was implemented and verified

The subsequent repository cleanup separates public profiles under crates/ailuros/boards from shared firmware under crates/ailuros/firmware/esp-idf. The default esp32s3 profile assumes no PSRAM or camera; the named goouuu-esp32s3-cam-v1.5 profile carries the measured pin map, octal PSRAM settings and 8 MHz camera clock. Export/selection work through xtask and native setup. CMake derives SDK options from board.json and selects generated SDK configuration by the profile/default/dependency inputs, preventing old hardware settings from carrying across selections. Workflow/test compilation explicitly resets its CMake flags and records its mode in the artifact. Old personal exports and the wall-powered ESP were not modified.

crates/ailuros/TESTING.txt is the current ordered validation guide, including host tests, the CMake configuration matrix, actual SDK compilation for both profiles and modes, native/backend tests, and separately authorized hardware/viewer checks. These new executable tests, SDK builds and device checks have not run. Historical hardware evidence below applies to the baseline before this refactor.

Static validation of the cleanup passed: cargo check for lince-ailuros and xtask with test targets, cargo check for lince-interface with native-media and test targets, rustfmt checks, and JSON syntax validation. Compiler warnings remain denied. No Lince process, service operation, firmware installation or executable test was started for this cleanup.

Embedded was renamed to Ailuros across the crate, workspace dependencies, Engine integration, native Karma Castle controls, Facade routes, protocol domain labels, xtask commands and Nix shell. The crate is crates/ailuros; its editable ESP-IDF starter is crates/ailuros/firmware/esp-idf. Ailuros covers the wider hardware family; camera, scalar and custom firmware are its current starters. Historical migrations, third-party license wording and generic embedded-system terminology were not rewritten.

The shared firmware provides on-device P-256 identity, physical USB provisioning, Wi-Fi reconnection, trusted time and authenticated HTTPS/WSS communication. Camera-specific code follows viewer demand, bounds frames/rates, publishes JPEG metadata and bytes, and releases capture when the final viewer closes. Scalar/custom starters reuse the shared components. Source exports include SDK references, licenses, credits and resources.json. The normal IDE/Fiote source-editing flow can customize an exported project before the shared compile/install pipeline; a dedicated firmware generator is not implemented.

The firmware can now execute its Unity cases on boot through LINCE_RUN_TESTS, with retained test registration and required 64-bit assertions. The test path bypasses normal identity/configuration/network startup. The local owner CLI can inspect/enroll/enable/disable endpoints through existing typed actions while taking the Cell instance lock. The ailuros_camera_check example validates real JPEG delivery and subscription lifecycle. A Nix toolchain fix prevents host LD_LIBRARY_PATH/LD_PRELOAD from breaking the pinned SDK's Python ABI.

### Physical board findings

- GOOUUU ESP32-S3-CAM V1.5; ESP32-S3 revision v0.2; MAC 28:84:85:44:b3:20; physical 16 MiB flash and 8 MiB octal PSRAM.
- Camera sensor: OV2640, PID 0x26. Verified data pins [11,9,8,10,12,18,17,16], XCLK 15, PCLK 13, VSYNC 6, HREF 7, SDA 4, SCL 5, PWDN/reset -1. The starter board profile now contains this measured map. Other revisions/sensors still need their own validation.
- PSRAM is enabled at 80 MHz. The application retains a 4 MiB flash partition layout inside the physical 16 MiB chip; larger layouts need deliberate firmware configuration.
- Initial capture stalled network transmission at a 20 MHz camera clock, despite strong Wi-Fi reception. A lower clock fixed the observed failure. The installed baseline uses 8 MHz. In the generalized source, board.json supplies this setting: the GOOUUU profile chooses 8 MHz and the base profile disables the camera. This is a measured workaround on this board, not a universal diagnosis of every camera/Wi-Fi failure. The firsthand upstream report is https://github.com/espressif/arduino-esp32/issues/5834.
- No extra antenna was needed for the successful tests. RSSI during USB-connected checks ranged approximately from -39 to -46 dBm.
- The final configurable-clock firmware was compiled and installed through the shared host workflow, with target MAC, source/image hashes and flash ranges checked. Its artifact fingerprint is 3a7fcef8a7dacd20a1f0508efacbfbf428688ee5dcf10bf318af28422be20a140c6bab6c2bbfbcc06264dd0ea944bc3f.

### Test evidence

- Baseline Ailuros host library: 15 passed before the generalized profile/export refactor. The new profile, artifact and export tests still need execution.
- ESP firmware Unity: 15 passed on the actual board. Raw NVS bytes were identical before and after installing/executing the test image. Later ordinary firmware updates retained the enrolled identity and Wi-Fi configuration.
- Engine Signal endpoint integration: 17 passed. Facade Signal tests: ten passed after fixing the close helper to tolerate queued pings during shutdown.
- Video Castle tests: three passed. Native Ailuros tests initially passed 11 of 12; the obsolete Laboratory fixture was corrected, but the final combined native rerun was cancelled at the owner's stop instruction. It is pending, not a recorded pass or test failure.
- cargo check for the UI+Facade application, Ailuros/xtask tools and hardware-check example passed earlier with warnings denied. Subsequent shared-branch changes and final native execution still require the targeted checks below; do not present an earlier check as validation of arbitrary later edits.
- Before the stop instruction, final firmware passed 170 decoded 320x240 JPEG frames after reset: 150 with two viewers, ten after one viewer closed, frame release/epoch change after the last viewer closed, then ten after reopening. Initial phase took 42.31 seconds; maximum measured frame age was 1687 ms.
- The owner then moved the ESP to its wall charger and confirmed power. With no USB device present on this computer, the same 170-frame/lifecycle check passed. Its initial 150 frames took 53.07 seconds including startup/reconnection, maximum age 1302 ms; one-viewer and resumed phases had maximum ages 951 and 950 ms. This proves a short wall-powered Wi-Fi run, not a long soak or actual native display rendering.
- The broader Facade run had five remaining failures involving existing browser/permission/settings behavior. Do not change access semantics to hide them: configuration_templates_and_management_enforce_permissions; browser_login_signed_edit_live_updates_and_logout; tagged_creation_is_visible_immediately_and_invalid_drafts_leave_no_record; visibility_and_read_filters_gate_details_comments_and_writes; role_allow_and_block_rules_gate_reads_moves_and_role_changes.

## Saved setup and operational limits

The destination is this computer's existing Cell at /home/user/.config/lince. Signal: esp-camera (r_01M55MR9QE90Z8Y99K2C7S97V1). Enrolled endpoint: r_01M55MR9RA838A447WMAJ8WTNS, enabled with a grant expiring in October 2027. The private device key remains on the ESP. Belkin54h's password was provisioned opaquely from .trash/secret; never print it or include it in model context, source, command arguments or generated project files.

NixOS persists /home/user/.config/lince, while the initially used ~/.local/share/lince, ~/.local/state/lince and ~/.config/systemd/user locations are ephemeral. The relevant files were copied into the existing persistent Cell directory:

- Editable deployed firmware source: /home/user/.config/lince/ailuros/projects/goouuu-camera. Generated SDK build-directory files were excluded so a future compile can generate its cache at the new path.
- Original flash backup, public enrollment/identity/setup records and installed firmware images: /home/user/.config/lince/ailuros/state/28848544b320. installed-firmware contains the verified artifact manifest, images and ELF. The original flash backup preceded creation of the current private identity; no later raw NVS backup exported that identity.
- Cell database/keys and prior workspace snapshot backup: /home/user/.config/lince/ailuros/state/before-camera-setup.
- Pinned host Nix environment: /home/user/.config/lince/ailuros/host-runtime. This keeps the selected runtime available without requiring a fresh repository evaluation on every later launch. The SDK environment itself is not permanently pinned by this profile.
- Private TLS material: /home/user/.config/lince/ailuros/tls; Caddyfile in its parent. The tested origin is https://10.0.0.155:8443, forwarding to the Cell Facade on 127.0.0.1:6174. Strict certificate verification passed; never replace it with an insecure bypass.
- Browser administrator user was created because the local Cell had no administrator and the public Facade requires one. Its generated password is stored privately at /home/user/.config/lince/ailuros/facade-admin-password. Do not print it. Native owner access does not require browser login.

The saved Ailuros camera workspace already contains a resumed Video Castle targeting esp-camera; other workspaces were preserved. It has not been opened in the newly compiled native app. The intended persistent binary path /home/user/.config/lince/ailuros/bin/lince is not installed. The pending launcher proposal must not be activated yet.

The installed baseline ESP can run from wall USB power, but the destination Cell and ingress must be running for publication. This computer is that destination, so it remains necessary. With Lince down there is currently no camera feed to view. Frames are memory-only, not archived. The installed baseline still requires the receiver's DHCP address to remain stable, or its certificate and the ESP destination to be updated. This is a limitation to remove in the proposed iroh workflow below, not the intended final setup. The current TLS certificate and endpoint grant also eventually require renewal.

## Requested outcome and architecture to refine

The intended daily path is ESP camera -> iroh -> destination Cell -> existing Signal -> native Video Castle. The ESP dials the enrolled Cell's persistent iroh EndpointId. IPs and relay URLs are changeable connection information, not destination identity. Reuse Lince's existing iroh endpoint, Cell keys, discovery settings, Signal authorization and in-memory frame channel. Do not introduce a second desktop identity or a full Cell/Organ implementation on the ESP.

Direct iroh on this GOOUUU ESP32-S3 with PSRAM is the first candidate. Qualification must include concurrent camera capture, verified infrastructure TLS and bounded memory, rather than an echo-only demonstration. A separate gateway is a contingency to discuss if that gate fails, not an equivalent completed outcome. Changing Caddy to native HTTPS alone does not satisfy the changing-network requirement.

The proposed transport choices are explicit: iroh for the primary camera workflow, and native HTTPS/WSS for hardware/integrations that need HTTP. Both reuse application authentication and frame lifecycle. Native HTTPS removes the Caddy process but still needs a reachable URL and certificate. The iroh camera path needs neither a user-managed PC HTTPS certificate nor Caddy; its relay/address-lookup HTTPS connections must still validate those services normally.

Changing the PC's Wi-Fi must not require modifying or reinstalling the ESP. Changing the ESP's Wi-Fi still requires credentials for that network. Add a bounded list of saved Wi-Fi profiles and native USB update controls, preserving device identity and Cell selection. Iroh does not join unknown Wi-Fi, perform captive-portal login or guarantee operation through every network policy. No proof of Wi-Fi membership or network-restricted token is requested.

The normal user workflow should be: select a Cell and Signal in native Ailuros, provision over USB, then power the ESP from a charger and open its Video Castle. The destination Cell must remain running somewhere. Lookup and relays are configurable network infrastructure dependencies, not extra per-camera applications the owner must configure on the PC.

## Research findings, checked 2026-10-18

### Direct iroh is plausible on this board, but camera operation is unproven

Upstream lists ESP32-S3 with PSRAM as supporting relay and address lookup; Xtensa requires the ESP Rust toolchain. This supports trying the owner's 16 MiB flash / 8 MiB octal PSRAM board. It does not prove concurrent camera performance. Source: [iroh hardware compatibility](https://docs.iroh.computer/compatibility).

The upstream S3 PSRAM example is an ESP-IDF Rust echo endpoint using the matching octal-memory configuration and xtensa-esp32s3-espidf target. Its README describes a 4 MiB layout, but the inspected SDK defaults describe a larger image and an 8 MiB flash minimum. Measure the selected revision rather than copying a size claim. Sources: [S3 example](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/README.md), [SDK defaults](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/sdkconfig.defaults).

The example main embeds Wi-Fi credentials at compile time and optionally embeds an identity. It uses a current-thread Tokio runtime with relays and Pkarr lookup. Reuse its integration knowledge, not its credential/identity lifecycle. Source: [main source](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/src/main.rs).

The example's relay/Pkarr verifier accepts certificates and handshake signatures without validation. This is distinct from iroh's EndpointId peer authentication and must not be copied. Source: [example infrastructure verifier](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/src/insecure_verifier.rs).

It also adds a QUIC crypto-provider adapter and uses a Git branch of rustls-rustcrypto, whose own documentation warns against production use. Secure Xtensa provider support remains a feasibility gate: prefer supported functionality or upstream integration, not writing our own TLS, QUIC or cryptography. Sources: [example QUIC provider](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/src/quic_crypto_provider.rs), [selected provider documentation](https://github.com/n0-computer/rustls-rustcrypto/blob/feature-flag-algorithms-v0.0.2/README.md).

The inspected example manifest selects iroh/iroh-relay 1.3.0 and esp-idf-svc 0.51; its toolchain selects ESP-IDF 5.3.3. This repository pins iroh 1.2.0 and the current C firmware pins ESP-IDF 5.5.2. Reconcile one deliberate version/toolchain set and pin exact revisions for unavoidable Git dependencies. There is no requirement to preserve older Lince versions. Sources: [example manifest](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/Cargo.toml), [toolchain settings](https://github.com/n0-computer/iroh-esp32-examples/blob/main/server-esp32-s3-psram/.cargo/config.toml).

The SDK defaults demonstrate substantial stack, PSRAM, Wi-Fi and lwIP tuning and disable Mbed TLS. Our P-256 implementation uses Mbed TLS, and camera DMA needs internal memory. Retain required signing support and measure the combined system. Eight MB PSRAM does not make every allocation safe.

### Address lookup and relays are required parts of the network design

Iroh lookup maps EndpointId to current relay/address information; saved address hints alone do not satisfy network changes. Reuse configured lookup/publishing and refresh information on reconnect. Source: [address lookup](https://docs.iroh.computer/concepts/address-lookup).

Relays assist connection establishment and carry encrypted traffic when a direct path is unavailable. Public relays have rate limits and no uptime guarantee. Measure camera throughput and expose alternate/self-hosted relay configuration; do not promise unlimited relayed video. Sources: [relay behavior](https://docs.iroh.computer/concepts/relays), [public relay policy](https://docs.iroh.computer/iroh-services/relays/public).

Optional mDNS lookup can discover local peers. The two crate families use different discovery formats; prefer Lince's existing iroh-mdns-address-lookup family if qualified on the ESP. Offline-LAN support needs explicit matching discovery and tests. Source: [local address lookup](https://docs.iroh.computer/connecting/local-address-lookup).

### Existing Rust libraries can replace Caddy and retain the camera driver

Prefer axum-server 0.8.0 with rustls for native HTTPS. It provides an Axum-compatible TLS listener, shutdown and certificate reload; RustlsConfig accepts both in-memory PEM and files. Confirm integration with this repository's Axum 0.8.9 and selected crypto provider. Sources: [axum-server](https://docs.rs/axum-server/0.8.0/axum_server/), [RustlsConfig](https://docs.rs/axum-server/0.8.0/axum_server/tls_rustls/struct.RustlsConfig.html).

Use rcgen if Lince owns local certificate generation/renewal. It supplies certificate signing, not renewal policy, public trust or discovery. Tokio-rustls is a lower-level alternative only if axum-server proves unsuitable. Sources: [rcgen](https://docs.rs/rcgen/0.14.10/rcgen/), [tokio-rustls](https://docs.rs/tokio-rustls/0.26.5/tokio_rustls/).

For a Rust ESP frontend, esp-idf-sys supports compiling C components and generating bindings, including esp32-camera integration. Keep Espressif's driver and useful existing C core behind a small binding boundary; do not rewrite camera/DMA support. Source: [esp-idf-sys component integration](https://docs.rs/crate/esp-idf-sys/0.38.1/source/BUILD-OPTIONS.md).

Higher-level iroh streaming projects include iroh-live/MoQ and iroh-roq. Evaluate them only if they reduce code and fit this JPEG sensor workflow. The initial recommendation is bounded iroh streams carrying existing control messages/JPEGs, reusing codecs where appropriate rather than adding transcoding or a full media stack. Source: [iroh streaming libraries](https://docs.iroh.computer/protocols/streaming).

These are evolving upstream sources. Before implementation, record exact dependency versions, Git revisions where unavoidable, licenses and measured build configuration. Moving branch links are provenance, not reproducible build pins.

## Remaining coding and tests, in order

### 1. Refine the design before implementation

1. Review direct iroh first, native HTTPS as a separate reusable adapter, retained P-256 Signal authorization, USB-managed Wi-Fi profiles and Cell-owned persistent settings/keys with the owner. Summarize tradeoffs and incorporate refinements before coding.
2. Keep the Command Record authoritative for existing behavior. It describes reverse-proxy HTTPS and says Lince does not flash an ESP. The requested future iroh/native-TLS design differs from those passages. Track that proposed difference here; do not edit the owner-authored Record.
3. Set the direct-device acceptance gate: identity-pinned EndpointId dialing with refreshed lookup, direct and forced-relay operation with valid infrastructure certificate verification, concurrent 320x240 JPEG capture at the established 5 FPS target, and bounded memory/reconnect behavior. Preserve the measured 8 MHz camera clock initially.
4. If secure provider support, image size or camera/memory feasibility fails, document the blocker and refine the design with the owner. A gateway near the ESP adds an always-on host and reachable local endpoint. Do not silently substitute it or native HTTPS for the standalone ESP goal.

### 2. Finish existing correctness validation and pin the ESP Rust integration

First follow crates/ailuros/TESTING.txt for the generalized source: run the host suite and explicit CMake tests, then compile base/camera profiles in workflow/tests/workflow order and verify their actual SDK settings and mode flags. The earlier 170-frame run does not validate this refactored source. Do not reuse the old installed artifact fingerprint for a new export/build.

1. Review the corrected Laboratory fixture against the current native isolation behavior. Finish the combined Ailuros setup and Video Castle tests on the current branch, using warnings as errors. The existing separate build cache is /home/user/git/.lince-ailuros-target; its cancelled compilation can be resumed. The intended command is nix develop /home/user/.config/lince/ailuros/host-runtime --command cargo test -p lince-interface --features native-media --lib --target-dir /home/user/git/.lince-ailuros-target -- ailuros_ui video_castle::tests. This is documentation, not authorization to run during the hold.
2. Run cargo check -p lince --features facade against the final shared tree. If another task introduced compilation failures, isolate the actual cause and preserve its work. Do not use cargo build, cargo clean, worktrees or parallel coding agents.
3. Keep the 15 host tests and the already successful camera/firmware checks as the baseline. Repeat only the checks affected by new changes. Investigate the five broader Facade failures separately; distinguish unrelated failures from regressions introduced by this feature.
4. Review local-owner CLI error paths: duplicate enrollment must not silently replace an endpoint; a failed JPEG configuration must leave publication disabled; an active Cell must retain its instance lock. Existing typed actions provide the mutation checks. Add focused tests only for newly discovered gaps.
5. Pin the ESP Rust toolchain, ESP-IDF, esp-idf-sys/svc, iroh/relay, crypto provider and camera component. Reuse SDK 5.5.2 and camera 2.1.0 where qualification allows. Plan a Rust ESP-IDF frontend under crates/ailuros/firmware, outside the desktop target graph, integrating the existing C components through bindings. Each board profile, signing/provisioning routine and camera/lifecycle component should retain one authoritative source instead of a copied second core. Include required upstream licenses/credits in exported resources and update the validation guide when implementation begins; leave README.md untouched.
6. Extend the xtask/Nix firmware artifact pipeline for Rust firmware checking/linking without invoking cargo build. Extend exports and artifact hashes to include transport, toolchain, lockfile and partition inputs. Compute actual image/partition ranges and enforce physical flash capacity while preserving NVS. Qualify a larger layout if necessary; never automatically erase/repartition the enrolled device to fit an upstream example.

### 3. Prove direct iroh on an explicitly authorized device

1. Integrate a minimal ESP-IDF Rust iroh frontend using on-device provisioning/NVS, not compile-time Wi-Fi credentials. Use an isolated diagnostic endpoint/protocol for this feasibility check; production Wire integration follows in step 4. Retain an on-device iroh key across reboot. Never copy an example key, print a secret or silently regenerate an enrolled identity.
2. Dial the provisioned Cell EndpointId with fresh address lookup and configured relay information. Enroll/pin the target through trusted local setup. Keep Cell audience, device UID, Signal UID and iroh EndpointId distinct; a relay/address hint never replaces identity verification.
3. Qualify a provider that supports peer authentication and verifies actual relay/address-lookup certificates, names, signatures and dates. Remove the upstream no-op verifier entirely. Preserve trusted-time bootstrap and the retained floor; do not use unauthenticated server time to weaken TLS checks. Prefer supported dependencies/upstream fixes over cryptographic code maintained in Lince.
4. Test initial/repeated connections on direct and forced-relay paths, then add actual camera capture. Measure internal/DMA-capable RAM, PSRAM, largest allocations, task-stack high-water, image size and sustained throughput. Require bounded buffers and recoverable failures. An echo success alone does not pass this gate.
5. The enrolled wall-powered ESP is not a disposable test target. Do not flash it during the hold. Qualify on a recoverable device or wait for explicit hardware authorization. Bring any required fork, extra gateway or infrastructure deployment back as a concrete design change.

### 4. Share application authentication and frame lifecycle across transports

1. Extract only the session behavior needed by WSS and iroh. Reuse engine::ailuros challenge verification, endpoint authorization and engine::signal_streams::Publisher; do not duplicate policy between Facade and Wire. Keep camera/scalar/custom recipes customizable through exported source.
2. Add a dedicated device protocol to the existing Wire ALPN dispatch, proposed name lince/ailuros/1. Use bounded hardware admission rather than general Organ enrollment, browser login or live-session permissions. Preserve private Wire restrictions. A device connection must not gain access to unrelated Wire protocols.
3. Retain the existing P-256 application identity/grant. The additional iroh Ed25519 transport identity does not itself authorize Signal publication. Use a new challenge per connection and the existing audience, key epoch, Signal, expiry and enabled-state checks. Keep P-256 keys on the ESP and avoid replacing identity based on MAC or network location.
4. Specify framing before implementation: bounded control JSON and length-delimited JPEG records on a reliable bidirectional QUIC stream; validate type/length before allocation; use deadlines and integer-safe epoch/sequence/acknowledgment handling. Reuse existing framing helpers. Bound in-flight frames and discard stale work instead of building an unbounded reliable video backlog.
5. Preserve no-capture-without-viewers, a single producer per Signal, frame size/rate/age limits, active-session authorization checks, memory-only latest frames and clearing/epoch change when the final viewer leaves. Reconnect starts a fresh authenticated session; old frames or acknowledgments cannot cross sessions. Share scalar receipt/retry/outbox semantics where useful without making every workflow camera-specific or requiring iroh on every board.

### 5. Persist destination identity and Wi-Fi configuration

1. Add typed destination choices: iroh by Cell EndpointId/audience, or HTTPS by URL/trust configuration. Reject inconsistent inputs before provisioning. Make protocol/configuration changes deliberate; older-version compatibility is not required.
2. Reuse ESP-IDF Wi-Fi APIs for a bounded saved-network list, deterministic priority, reconnect backoff and status. Preserve working configuration on failure and update credentials independently of camera/workflow settings. Iroh handles reaching peers after network access exists.
3. Add native USB controls for Wi-Fi updates without recompilation or Signal reenrollment. Keep passwords out of logs, source exports, public database extensions and Fiote/model context. Retain them on the ESP; a second host password copy is not necessary to receive video.
4. Store the new iroh key in its own NVS entry, preserving existing P-256 identity/configuration and flash boundaries. Corruption is a recovery error, not permission to generate a new enrolled identity automatically.
5. Refresh lookup/reconnect on network changes. A PC Wi-Fi/IP change requires no USB operation. A previously unknown ESP Wi-Fi requires credentials through native USB setup; saved profiles reconnect without setup. Captive-portal handling, BLE and OTA remain separate work.

### 6. Replace Caddy with optional native HTTPS

1. Add an explicit rustls listener through axum-server, reusing Axum/application handlers and deliberate crypto-provider selection. Avoid writing a TLS accept loop unless a demonstrated integration gap requires tokio-rustls.
2. Refactor Facade's loopback/reverse-proxy assumptions so direct TLS is known from trusted listener context. Preserve origin/Host/session/CSRF checks, secure-transport enforcement, connection limits and admission bounds. Do not spoof X-Forwarded-Proto internally or let client headers prove TLS.
3. Refine the exposure boundary with the owner. Recommendation: a device listener exposes only Ailuros clock/readings/stream routes by default and does not require a browser administrator merely to receive a camera. Reuse browser Facade separately when requested; do not expose its complete surface to enable hardware ingress.
4. Load/import protected certificate/key files within the Cell directory. Reuse the saved certificate/CA for a staged local handover if still valid. Use rcgen when Lince owns issuance, with explicit renewal, atomic replacement, reload, private-file permissions and device trust-anchor provisioning. No insecure certificate bypass.
5. Native HTTPS still needs a reachable address and valid certificate. It does not solve NAT or changing IPs. An iroh-only camera must not require a domain, PC HTTPS certificate, Caddy or TCP 8443 listener.
6. Test disabled/configured listeners, bind errors, invalid keys/certificates, reload and shutdown. Any later Caddy stop/disable, port handover or NixOS cleanup requires the owner to lift the deployment hold; do not alter its service/configuration while planning.

### 7. Make startup ordinary Cell behavior and expose settings in native Lince

1. Persist receiver enablement and transport settings through typed Cell actions/settings. Reuse lince.discovery reach/relay policy where applicable rather than introducing conflicting global controls. Device-to-Signal grants, public keys and auth state remain database-owned.
2. Reuse the existing persistent Cell node key for destination identity. Keep private host keys and optional TLS material in protected files under the selected Cell directory; firmware sources/artifacts/recovery copies are optional under ailuros. Build caches and SDK tools are not runtime data. Startup must not depend on the repository checkout, .trash/secret, an editable export or a browser setup password.
3. Implement frontend/backend together: select Cell/Signal, enable hardware ingress, choose transport and routing policy, provision/update Wi-Fi, enroll/disable, inspect status and Open Video. Default this qualified board to iroh. Preserve local-owner restrictions; remote Facade access never grants USB or shell execution.
4. Show receiver running/disabled/error, device disconnected/connected, auth rejected/expired, waiting for viewers, direct/relay path and actionable lookup/relay/Wi-Fi errors. Avoid making the owner manage network plumbing merely to select a Cell.
5. Reconstruct ingress from settings/keys when the ordinary Cell runtime starts. Iroh mode works without browser Facade/Caddy. Ingress stays available without a viewer; capture follows demand. Cell/device restart reconnects without reenrollment. Login/boot autostart is normal packaging, not additional per-camera state.
6. Prepare durable NixOS/module or launcher changes only after review. Replace the temporary pending launcher proposal rather than activating it unchanged. Use the normal UI+receiver package; target/debug/lince is headless and must not be installed as the desktop app. Persist service declarations in the NixOS repository instead of ephemeral ~/.config/systemd/user. Preparation does not authorize rebuild/install/reload/activation.

### 8. Run focused automated tests for the completed design

1. Cover both adapters' shared auth: valid proof, replay, wrong Cell/audience/Signal, unknown key, disabled/expired grant, changed key epoch and revocation during publication. A known iroh peer alone cannot publish or access another protocol. Preserve duplicate-enrollment and active-Cell locking checks.
2. Cover framing and bounds: oversized/truncated lengths, malformed control/JPEG metadata, integer overflow, auth deadlines, slow readers, cancellation, reconnect, stale epoch/acknowledgment and duplicate producers. Assert bounded queues/allocations before authentication and during streaming.
3. Repeat two viewers -> one -> zero -> reopened over iroh/WSS with identical demand, clearing and epoch expectations. Cover native setup/Video Castle actions and scalar receipts/retries affected by factoring.
4. Use isolated test Cells, controlled lookup and local test relays for deterministic direct/forced-relay, stale-route refresh, receiver restart and relay failure/recovery. Public-network qualification is separate. Never use the personal Cell for tests while the hold remains.
5. Native TLS tests use verified clients: accept trusted material; reject wrong name, unknown issuer, invalid signature and expiry; verify reload/restart/shutdown. Plain HTTP or forged forwarding headers cannot satisfy TLS-only routes.
6. Test saved settings/key reload and ingress reconstruction using temporary Cell directories. Verify Wi-Fi-only updates preserve identities/workflow, source exports exclude private state, and transport/toolchain/partition changes invalidate artifacts. If schema changes are necessary, add append-only migrations/checksums and run cargo check -p store plus migration_guard tests before proceeding.
7. Run affected cargo check/test commands with warnings denied through xtask/Nix and the supported firmware matrix through the artifact pipeline. Report static checks, executed tests, actual linked firmware and hardware evidence separately. This Markdown-only task executes none of them.

### 9. Validate personal use and retire temporary dependencies when authorized

1. After approval, implementation, relevant checks and explicit runtime/hardware authorization, install the normal executable and qualified firmware. Preserve the personal Cell, P-256 identity, esp-camera Signal and workspaces. Verify changing sensor images in the actual saved Video Castle, not only a backend JPEG decoder.
2. With Caddy absent from the tested path, stream over iroh on the LAN, then move the PC to another internet connection. The same ESP configuration, Cell identity and Signal must reconnect without a fixed PC IP, new HTTPS certificate, port-forwarding setup or USB reprovisioning. Record direct/relay routing and frame age/throughput.
3. Update the ESP to a second 2.4 GHz Wi-Fi using native USB controls, return it to wall power and verify delivery across networks. Both device keys and the Signal grant survive. Repeat the short 170-frame/lifecycle baseline and native viewing because transport/firmware changed.
4. Test PC reboot, Cell restart, charger-only ESP cold boot, router/Wi-Fi interruption and relay recovery. Reopen the workspace. Demonstrate reconstruction from Cell directory/keys and device NVS without a manual shell, Caddy or retained test harness.
5. Only after successful switchover, retire temporary Caddy setup, obsolete launcher flags and camera-only firewall exceptions when unused. Check for other consumers before changing shared configuration. Retain optional native HTTPS certificates/settings if wanted. All service/NixOS operations remain separately reviewed deployment actions.
6. Report the actual minimum personal setup: powered configured ESP, destination Cell running with persistent state/identity, and reachable configured lookup/relay infrastructure for cross-network operation. Report unresolved provider, network-policy or performance limits. The Cell computer being off and unknown Wi-Fi credentials are not solved by iroh.

### 10. Additional reliability qualification

1. Soak direct and forced-relay camera paths while measuring internal RAM/PSRAM, stack headroom, frame age, bandwidth/drops, temperature and charger stability. Establish tolerances against 320x240/5 FPS; discuss measured relay limitations before accepting a lower target.
2. Exercise invalid/expired infrastructure certificates, incorrect clocks, long outages and path changes; restore normal operation without insecure fallbacks or identity reset. Preserve the retained time floor.
3. Design interrupted-provisioning/storage tests on a recoverable device before power cuts or NVS corruption. Ordinary updates retain identity and corrupt keys do not silently regenerate. Do not perform destructive tests on this enrolled board without recovery and authorization.
4. Qualify additional board/sensor combinations explicitly. Do not claim all ESPs or the base no-PSRAM profile support this measured camera/iroh combination. Keep shared auth/provisioning/lifecycle and suitable HTTPS/custom recipes reusable.

## Deferred extensions

1. First-class USB key rotation/recovery, with physical confirmation, disable-first ordering, interruption handling and reprovisioning. Backend key replacement already exists.
2. A separately deployed iroh gateway for devices unable to run the qualified transport. Promote it only after a documented feasibility failure and design refinement; it is no longer the default planned approach for this PSRAM S3 camera. No Wi-Fi-membership credential mode is requested.
3. OTA/wireless provisioning, BLE onboarding, protected key storage, secure boot, flash encryption and debug restrictions, with recovery designed before irreversible chip changes. USB Wi-Fi updates and persistent iroh identity are immediate planned work above. Current keys use software storage.
4. More verified board/sensor profiles and hardware-specific recipes, retaining shared authentication, provisioning, networking and lifecycle code.
5. A dedicated guided Fiote firmware generator if IDE/Fiote source editing followed by the shared compile/install flow proves insufficient. Keep generated changes reviewable and credentials outside model context.
6. General media distribution/transcoding and MoQ/RTP integration unless measurements and concrete workflow needs justify them. The first outcome remains JPEG camera frames in the existing Signal and Video Castle.

Preserve the owner-authored Command Record. Future wording should cover native iroh, optional native HTTPS and current flashing capability, but only the owner or a separately authorized exact edit may change it. Keep this Markdown until remaining work is implemented and verified; refine it with the owner before coding.
