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

# Disclaimer

Lince is a non-profit project and crowdfunding is the source of development compensation:

[GitHub Sponsors](https://github.com/sponsors/lince-social) | [Patreon](https://www.patreon.com/lince_social) | [Apoia.se](https://www.apoia.se/lince)

Lince tries to facilitate and automate the connection between people and resources, by transforming needs and contributions into data.
The gains and losses related to the interaction, such as transportation, production and services themselves, remain the responsibility
and risk of the parties involved.

# Ways to install

Note that the `dev` branch is always further ahead, sometimes by a lot. But it is more unstable.

The default published binary and container target the HTTP frontend with Karma enabled. The RustFS object storage has it's settings in the configuration table, seeded when Lince starts. If your way of installing sets the bucket you can use it with `rustfsadmin` as initial 'username and password'.

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

The published container image is public at `ghcr.io/lince-social/lince`. The container keeps its state under `/var/lib/lince` and is meant to be run with a persistent volume.

```bash
# For new Ubuntu VPS you can first run
sudo apt-get update
sudo apt-get install -y docker.io
sudo systemctl enable --now docker

# To keep Lince running as a background service and bring it back after reboot, run it with `-d` and `--restart unless-stopped`:
docker run -d \
  --name lince \
  --restart unless-stopped \
  -p 6174:6174 \
  -v lince-data:/var/lib/lince \
  ghcr.io/lince-social/lince:rolling
```

The location of Lince's configuration directory depends on your operating system:

| Platform | Value                                 | Example                                        |
| -------- | ------------------------------------- | ---------------------------------------------- |
| Linux    | `$XDG_CONFIG_HOME` or `$HOME`/.config | /home/alice/.config/lince                      |
| macOS    | `$HOME`/Library/Application Support   | /Users/Alice/Library/Application Support/lince |
| Windows  | `{FOLDERID_RoamingAppData}`           | C:\Users\Alice\AppData\Roaming\lince           |

The directory and lince.toml is created automatically on first start and gives you a random auth secret. To enable auth you will need to add an `enable = true`, example:

```bash
[auth]
secret = "75b4c1a538af41a79b19ac17cbe03829"
enabled = true
```

When auth is enabled, Lince will require an admin user to exist. If none exists yet, run Lince once in a terminal so it can prompt you to create the initial admin user.

Accessing it from the outside world:

```bash
# If your VPS uses `ufw`, open the HTTP port:
sudo ufw allow 6174/tcp

# If you have a domain, you can use Caddy to setup a reverse proxy and make requests with HTTPS
sudo apt install -y caddy
sudo mkdir -p /etc/caddy
cat <<EOF | sudo tee /etc/caddy/Caddyfile
foo.com {
    reverse_proxy 127.0.0.1:6174
}
EOF
sudo ufw allow 80
sudo ufw allow 443
sudo systemctl enable --now caddy
```

This is the recommended container install path for always-on usage on a VPS or personal machine. `podman run` can use the same image and flags.

### 4. Cargo install

If you don't have Cargo installed, you can install it with #link("https://www.rust-lang.org/tools/install")[Rustup].
It comes with Rust and its toolchain. If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`)
so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

```bash
cargo install --locked lince
lince
```

Supported feature shapes for `cargo install`:

- Default (and recommended) HTTP + Karma: `cargo install --locked lince`
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
- With Docker or Podman, use the container command above with `--restart unless-stopped`.

### Extra

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
