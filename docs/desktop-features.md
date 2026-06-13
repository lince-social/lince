# Desktop Features

Lince desktop is the HTML app running inside a Tauri system webview.

## What It Does

- Starts the local Lince web server inside the desktop app.
- Opens the HTML UI in a desktop window.
- Uses an ephemeral localhost port in desktop mode.
- Keeps the normal browser/server binary separate.
- Prevents a second desktop instance; launching again focuses the existing app.
- Closing the window hides it to tray instead of quitting.
- Tray menu has `Open Lince` and `Quit`.
- On Windows and macOS, startup-at-login is synced from the active `configuration` row.
- If startup is enabled and silent startup is enabled, `--desktop-autostart` starts hidden to tray.

## Configuration Fields

These live in the active `configuration` row:

- `desktop_start_on_login`
- `desktop_start_silent`

Use `1` for enabled, `0` for disabled, `NULL` for unset.

The desktop app polls these fields about every 30 seconds on Windows and macOS and syncs OS autostart.

## Auth Setup

Auth still lives in `lince.toml`.

Installer/setup can stage:

- auth enabled or disabled
- initial admin password
- startup preferences
- default language

On first app launch, staged setup is imported and then removed.

If auth is enabled during setup, the first admin username is:

```text
user
```

## Locale

Setup detects Portuguese/Brazil-related locale and stages:

```text
pt-BR
```

It only writes this into the active configuration if `language` is unset.

## Windows

The NSIS installer asks:

- start Lince when signing in
- open silently when started automatically
- enable auth
- initial admin password if auth is enabled

The installer stages those choices for first launch.

## macOS

The release includes a helper:

```sh
lince-macos-desktop-setup.sh
```

Install `Lince.app` into `/Applications`, run the helper, then open Lince once.

This is temporary. A future custom macOS installer app can use the same staging command.

## Linux

Linux desktop builds are supported.

Startup-at-login setup is not implemented for Linux in Lince yet.

Tray support depends on the desktop shell. The shell must expose StatusNotifier/AppIndicator tray items.

## NixOS

Enter the desktop shell:

```sh
nix develop .#desktop
```

Run checks or Tauri:

```sh
cargo check --locked -p lince-desktop
cd crates/desktop
cargo tauri dev
```

Build or run the package:

```sh
nix build .#lince-desktop
nix run .#lince-desktop
```

For login startup on NixOS, use a user service that runs:

```sh
lince-desktop --desktop-autostart
```

If your shell does not support StatusNotifier/AppIndicator, the app can run without showing a tray icon.

## Useful Setup Command

Installers and setup helpers call:

```sh
lince-desktop --stage-desktop-install-setup
```

Common flags:

```sh
--start-on-login
--start-silent
--auth-enabled
--initial-admin-password <password>
--language pt-BR
```

## Quirks

- Full local desktop checks on Linux require WebKitGTK and related native libraries.
- On NixOS, use `nix develop .#desktop` instead of installing those globally.
- Tauri uses the system webview. It does not bundle a whole browser.
- AppImage and installer artifacts may bundle runtime files, but those are release artifacts, not vendored source files.
- The macOS setup helper is not the final custom installer experience.
