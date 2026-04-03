<p align=center>
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/white_in_black.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/black_in_white.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/white_in_black.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/black_in_white.svg" alt="Lince Logo">
</p>

# Lince

Tool for registry, interconnection and automation of Needs and Contributions with open scope.

Detailed explanations of what Lince is, how to run it and use it's ecosystem can be found in the [Dark Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-dark.pdf)/[Light Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/dev/documentation/lince-documentation-light.pdf).

Check the website [lince.social](https://lince.social) for more.

# Ways to install

Note that the `dev` branch is always further ahead, sometimes by a lot. But it is more unstable.

The default published binary and container target the HTTP frontend with Karma enabled.

### 1. One-line installer

The hosted installer is meant for macOS and glibc-based Linux systems. It installs to `~/.local/bin` by default and does not configure `systemd` automatically.

```bash
curl -fsSL https://lince.social/install.sh | bash
```

Inside this repo, the canonical script lives at `run/install.sh`:

```bash
./run/install.sh
```

To pin a specific release or install somewhere else:

```bash
curl -fsSL https://lince.social/install.sh | bash -s -- --version v0.6.1
curl -fsSL https://lince.social/install.sh | bash -s -- --bin-dir /usr/local/bin --force
```

### 2. Releases

Check the [releases](https://github.com/lince-social/lince/releases) and download the asset for your operating system. The rolling main-branch binaries live under the [`rolling`](https://github.com/lince-social/lince/releases/tag/rolling) release.

```bash
./lince --help
./lince --listen-addr 0.0.0.0:6174
```

### 3. Docker

The container keeps its state under `/var/lib/lince` and is meant to be run with a persistent volume.

```bash
docker run -d \
  --name lince \
  --restart unless-stopped \
  -p 6174:6174 \
  -v lince-data:/var/lib/lince \
  ghcr.io/lince-social/lince:rolling
```

### 4. Cargo install

```bash
cargo install --locked lince
lince
```

This publishes `lince` plus internal `lince-*` helper crates. Those helper crates exist so the binary can be installed from crates.io; they are implementation details, not a reusable public library surface.

Supported feature shapes for `cargo install`:

- Default HTTP + Karma: `cargo install --locked lince`
- HTTP only: `cargo install --locked lince --no-default-features --features "http"`
- Karma only: `cargo install --locked lince --no-default-features --features "karma"`
- TUI only: `cargo install --locked lince --no-default-features --features "tui"`
- TUI + Karma: `cargo install --locked lince --no-default-features --features "karma,tui"`
- GUI only: `cargo install --locked lince --no-default-features --features "gui"`
- GUI + Karma: `cargo install --locked lince --no-default-features --features "karma,gui"`

Rules:

- `http`, `tui`, and `gui` are mutually exclusive frontend features
- `karma` can be combined with one frontend, or used on its own

### 5. Compiling yourself

> Beware! Here be dragons (bugs).

For those that want to compile the latest and have [cargo](https://www.rust-lang.org/tools/install) installed, run:

```bash
cargo run
```

### Keeping it running

The installer only installs the binary. If you want Lince to stay up after boot, use your own process supervisor.

- On Linux with `systemd`, start from [run/systemd/lince.service](run/systemd/lince.service).
- With Docker, use `--restart unless-stopped`.

### Extra

You can also use `mise` to run Lince:

```bash
mise exec cargo:lince -- lince
```

For maintainers, the release flow is:

```bash
mise release
mise publish-crates
```

# Contributions

To contribute, check the [CONTRIBUTING.md](CONTRIBUTING.md).

# License

Check the [License](LICENSE)

---
