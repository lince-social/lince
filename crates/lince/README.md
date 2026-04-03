# Lince

Tool for registry, interconnection and automation of Needs and Contributions with open scope.

The default published binary enables the HTTP frontend together with Karma.

## Install

```bash
cargo install --locked lince
```

Supported feature shapes:

- Default HTTP + Karma: `cargo install --locked lince`
- HTTP only: `cargo install --locked lince --no-default-features --features "http"`
- Karma only: `cargo install --locked lince --no-default-features --features "karma"`
- TUI only: `cargo install --locked lince --no-default-features --features "tui"`
- TUI + Karma: `cargo install --locked lince --no-default-features --features "karma,tui"`
- GUI only: `cargo install --locked lince --no-default-features --features "gui"`
- GUI + Karma: `cargo install --locked lince --no-default-features --features "karma,gui"`

Frontend features are mutually exclusive: `http`, `tui`, and `gui`.

## Docker

For an always-on service, use the public container image:

```bash
docker run -d \
  --name lince \
  --restart unless-stopped \
  -p 6174:6174 \
  -v lince-data:/var/lib/lince \
  ghcr.io/lince-social/lince:rolling
```

`podman run` can use the same image and flags.

## Other install paths

- Installer: `curl -fsSL https://lince.social/install.sh | bash`
- Releases: https://github.com/lince-social/lince/releases
- Website: https://lince.social
