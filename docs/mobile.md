Tauri mobile is not “desktop Tauri, but smaller.” It is a Rust-backed WebView app wrapped in native
Android/iOS projects. You must treat Android and iOS as real native targets: SDKs, signing,
permissions, store rules, generated platform projects, and platform-specific Rust cfg gates all
matter.

Sources: Tauri prerequisites, Google Play, App Store, plugin support docs: Prerequisites
(https://v2.tauri.app/start/prerequisites/), Google Play
(https://v2.tauri.app/distribute/google-play/), App Store
(https://v2.tauri.app/distribute/app-store/), Features & Plugins (https://v2.tauri.app/plugin/).

What You Need
For both:

- Rust installed.
- Tauri CLI installed:

  cargo install tauri-cli --locked --version '^2'

- Your Tauri app must expose a mobile entry point:

  #[cfg_attr(any(target_os = "android", target_os = "ios"), tauri::mobile_entry_point)]
  pub fn run() {
  tauri::Builder::default()
  .run(tauri::generate_context!())
  .expect("error while running app");
  }

- Desktop-only APIs must be behind cfg gates: tray, single-instance, autostart, global shortcuts,
  window-close-to-tray, desktop menus, etc.

- Your tauri.conf.json > identifier must be a stable reverse-DNS bundle id, for example:

  "identifier": "social.lince.app"

For Android:

- Android Studio.
- Java/JDK, normally Android Studio’s bundled JBR.
- Android SDK Platform, Platform Tools, NDK side-by-side, Build Tools, Command-line Tools.
- Env vars:

  export JAVA_HOME="/path/to/android-studio/jbr"
  export ANDROID_HOME="$HOME/Android/Sdk"
    export NDK_HOME="$ANDROID_HOME/ndk/$(ls -1 "$ANDROID_HOME/ndk" | tail -n1)"

- Rust targets:

  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-
  android
  Tauri docs list these Android targets directly.

For iOS:

- macOS only.
- Full Xcode, not just Command Line Tools.
- CocoaPods:

  brew install cocoapods

- Rust targets:

  rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
  Tauri docs state iOS development requires Xcode and macOS.

Initialize Mobile Projects
From the Tauri crate directory, in this repo:

cd crates/desktop
cargo tauri android init
cargo tauri ios init

This creates generated native projects under roughly:

- crates/desktop/gen/android
- crates/desktop/gen/apple

These are not disposable if you customize permissions, signing, Gradle, Xcode settings, manifests,
entitlements, icons, etc. Decide whether to commit them once stable.

Run On Android
With an emulator running or a device attached:

cd crates/desktop
cargo tauri android dev

Useful device commands:

adb devices
adb logcat

Build an APK for direct testing:

cargo tauri android build --apk

Tauri docs say APKs are useful for testing or non-store distribution. Install it:

adb install -r path/to/app.apk

If Android blocks install:

- Enable Developer Options.
- Enable USB debugging.
- Allow install from unknown sources if sideloading manually.
- Confirm device trust prompt.

Build an AAB for Google Play:

cargo tauri android build --aab

Tauri docs say AAB is the recommended Google Play upload format. The generated AAB path is documented
as:

gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab

Upload that in Google Play Console. First upload must be manual so Google can verify signature and
bundle id. Tauri explicitly notes it does not currently automate creating Android releases in Google
Play.

Run On iOS Simulator
From macOS:

cd crates/desktop
cargo tauri ios dev

Or open/build through Xcode:

cargo tauri ios build --open

For simulator builds, use a simulator target if needed:

cargo tauri ios build --target aarch64-apple-ios-sim

Run On iPhone
You need:

- Apple Developer account for distribution.
- Xcode signing configured.
- Bundle ID in App Store Connect / Apple Developer portal matching tauri.conf.json > identifier.
- Provisioning profile.
- Signing certificate/team selected in Xcode.

Typical path:

cd crates/desktop
cargo tauri ios build --open

Then in Xcode:

- Select your Team.
- Select a physical device.
- Fix signing/provisioning.
- Press Run.

For App Store / TestFlight:

cargo tauri ios build --export-method app-store-connect

Tauri docs say the generated IPA is under:

src-tauri/gen/apple/build/arm64/$APPNAME.ipa

For this repo the path will be under crates/desktop/gen/apple/..., because the Tauri crate is crates/
desktop.

Upload with Xcode Organizer, Transporter, or Apple CLI tooling. Tauri docs show xcrun altool, though
Apple’s tooling evolves.

Quirks
The biggest quirks:

- Android and iOS builds are native builds. You are not just compiling Rust; you are driving Gradle/
  Xcode.

- iOS requires macOS. You cannot build or sign real iOS apps on Linux.
- Signing is not optional for real distribution.
- Android APK is easy to sideload; AAB is for Play Store and not normally installed directly.
- iOS IPA installation is controlled by Apple signing/provisioning. You usually install through
  Xcode, TestFlight, MDM, or App Store.

- Mobile WebViews are not identical to desktop WebViews. Test layout, scrolling, keyboard, viewport
  units, file input, camera permissions, and storage behavior on real devices.

- Tauri permissions/capabilities matter. A plugin existing does not mean the OS permission is
  granted.

- Desktop plugins often do not apply on mobile. In our repo I already had to gate single-instance,
  autostart, tray behavior, and close-to-tray behavior.

- Background behavior is heavily restricted, especially on iOS. Do not assume a local server, sync
  loop, or long-running task can keep running after the app backgrounds.

- Store rules override framework capabilities. Apple/Google may reject behavior that technically
  works.

- Rust crates may not compile for mobile if they assume Linux/macOS/Windows APIs, native system libs,
  process spawning, shell access, or filesystem paths.

- Native permissions require native config changes: Android manifest, iOS
  Info.plist/entitlements/capabilities.

- Generated mobile projects can drift. If you rerun init or upgrade Tauri, review Gradle/Xcode diffs
  carefully.

- App icons/splash screens are platform-specific. Tauri can generate icons after android init / ios
  init, but you still need to inspect the native result.

- Versioning differs. Android has versionCode; Tauri derives it from semver unless configured. Google
  Play requires monotonically increasing version codes.

- CI for iOS needs certificates/profiles/secrets. A simulator build is much easier than a device/App
  Store build.

What Tauri Can’t Do
Tauri cannot:

- Bypass Apple or Google signing/store rules.
- Build iOS on Linux or Windows.
- Make every desktop API available on mobile.
- Provide native-quality mobile UX automatically; your web UI still needs mobile design work.
- Run arbitrary background services freely on iOS/Android.
- Guarantee every Rust crate works on mobile targets.
- Use desktop concepts like tray icons, global shortcuts, multi-window workflows, CLI args, or shell
  access the same way on mobile.

- Avoid native code entirely when you need platform-specific APIs. You may need Swift/Kotlin plugin
  work.

- Magically make a local desktop-server architecture behave correctly in mobile app lifecycle
  conditions.

Specific To Lince
Lince currently behaves like a desktop shell that starts a local HTTP server and opens a WebView to
it. That is workable on desktop, but mobile needs extra scrutiny:

- The server lifecycle must match mobile lifecycle.
- iOS may suspend it when backgrounded.
- Android may kill it under memory/battery pressure.
- You likely want the Tauri localhost plugin or a more mobile-native serving model.
- Tray/start-on-login/single-instance concepts should remain desktop-only.
- The UI needs phone/tablet viewport testing, not just compilation.

For first practical testing, I would do this order:

1. Android APK first:

   cd crates/desktop
   cargo tauri android init
   cargo tauri android dev
   cargo tauri android build --apk
   adb install -r path/to/app.apk

2. iOS simulator second:

   cargo tauri ios init
   cargo tauri ios dev

3. Only after those run, deal with signing, TestFlight, Play Console, and production release builds.
