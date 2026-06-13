# Desktop Builds

Lince desktop uses Tauri to run the existing HTML app in the operating system webview. The repo does not vendor WebKitGTK, GTK, appindicator, compilers, or other native platform libraries. Those come from the OS package manager or the release artifact format.

## Generic Linux

Install Rust and the Tauri native prerequisites with your distro package manager. On Debian/Ubuntu-compatible systems:

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  curl \
  file \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  wget
```

Then build from source:

```sh
cargo check -p lince-desktop
cd crates/desktop
cargo tauri build
```

For end users, prefer the release artifacts. `.deb` relies on distro runtime packages. `.AppImage` bundles more runtime files, but the bundled output is a release artifact, not source tracked in this repo.

## Windows and macOS

GitHub Actions builds desktop artifacts for both platforms. Local source builds need the normal Tauri platform prerequisites plus Rust:

```sh
cargo check -p lince-desktop
cd crates/desktop
cargo tauri build
```

Windows releases produce an NSIS installer. During install, it asks whether Lince should start on login, whether automatic starts should open silently to the tray, and whether authentication should be enabled. If authentication is enabled, the installer requires an initial admin password. The first normal app launch imports those choices into Lince's active configuration and `lince.toml`.

macOS releases produce a desktop app bundle/DMG and ship `lince-macos-desktop-setup.sh` beside the DMG. Install the app into `/Applications`, run the setup helper, then open Lince once. The helper asks the same startup/auth questions and stages the same first-launch import data.

The macOS helper is an interim packaging step. A future custom macOS installer app can call the same `lince-desktop --stage-desktop-install-setup` command and does not need a different backend format.
