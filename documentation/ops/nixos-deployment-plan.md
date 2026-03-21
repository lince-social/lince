# NixOS Deployment Plan

## Goal

Move Lince deployment away from mutable Ubuntu + ad hoc service setup toward a reproducible Nix-based deployment with:

- one build target for the HTTP API service
- one systemd service managed declaratively
- Caddy managed declaratively
- environment secrets kept outside Git
- deploy on `push` to `dev`
- as few Nix files and GitHub Actions as possible

## Recommended End State

Use NixOS on the VPS and keep the deployment shape small:

1. `flake.nix`
   - builds `lince-http`
   - exposes a NixOS configuration for the VPS

2. `nixos/manas-organ.nix`
   - imports standard NixOS settings
   - enables OpenSSH
   - enables Caddy
   - defines the `lince` systemd service
   - stores runtime env in an `EnvironmentFile`

3. `secrets/lince.env`
   - not committed
   - contains `SECRET=...`
   - optionally `HTTP_LISTEN_ADDR=127.0.0.1:6174`

4. one GitHub Action on `push` to `dev`
   - SSH into the VPS
   - `git pull`
   - `sudo nixos-rebuild switch --flake .#manas-organ`

## Why This Is Simpler Than Ubuntu Bootstrap

On Ubuntu, deployment usually needs:

- package installation
- custom service install logic
- Caddy install logic
- repo update logic
- build logic
- restart logic

On NixOS, package installation and systemd/Caddy setup move into the NixOS config. The deploy command becomes:

```bash
sudo nixos-rebuild switch --flake .#manas-organ
```

That one command both installs dependencies and updates services.

## Service Shape

Recommended runtime model:

- Lince binds to `127.0.0.1:6174`
- Caddy binds publicly on `80/443`
- Caddy reverse proxies to `127.0.0.1:6174`

This keeps TLS out of the Rust binary and keeps the HTTP service private behind the reverse proxy.

## Minimal File Layout

Recommended minimal Nix layout:

```text
flake.nix
nixos/manas-organ.nix
```

Optional:

```text
.github/workflows/deploy-dev.yml
```

That is enough for:

- package build
- machine config
- deployment automation

## GitHub Actions

Minimal deployment action:

- trigger on `push` to `dev`
- SSH into VPS
- run:

```bash
cd /root/git/lince-social/lince
git fetch origin dev
git checkout dev
git reset --hard origin/dev
sudo nixos-rebuild switch --flake .#manas-organ
```

## Secrets

Do not commit secrets in the flake or NixOS module.

Use a file like:

```bash
/var/lib/lince/lince.env
```

Example contents:

```env
SECRET=replace-with-a-long-random-secret
HTTP_LISTEN_ADDR=127.0.0.1:6174
```

Then point the `lince` systemd service at that file with `EnvironmentFile=`.

## Migration Path

### Option A: Stay on Ubuntu for now

Keep:

- current deploy script
- current `systemd/lince.service`
- current Caddy setup

Add:

- bootstrap script
- deploy action

This is lower-risk short term.

### Option B: Reinstall VPS as NixOS

Do this if:

- you are okay replacing the current OS
- the VPS is mostly dedicated to Lince
- you want one declarative machine config instead of mutable setup scripts

This is the cleaner long-term path.

## Recommendation

Preferred long-term deployment:

1. reinstall the Manas Organ VPS as NixOS
2. keep deployment to two Nix files plus one GitHub Action
3. let NixOS manage:
   - Caddy
   - systemd
   - service restart
   - package dependencies

This minimizes drift and removes most custom bootstrap logic.
