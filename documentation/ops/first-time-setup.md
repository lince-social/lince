# First-Time Setup

## Ubuntu VPS

Assumptions:

- the repo lives at `/root/git/lince-social/lince`
- you log in as `root`
- `mise` is already installed
- system packages needed for building/running Lince are already available

### 1. Clone the repo

```bash
mkdir -p /root/git/lince-social
cd /root/git/lince-social
git clone https://github.com/lince-social/lince.git
cd lince
git checkout dev
```

### 2. Create `.env` and seed the root user

```bash
mise init
```

That command will:

- create `.env` from `.env.example` if it does not exist
- generate `SECRET` if it is missing
- prompt for the root API user password
- seed the `bomboclaat` root API user with the password you entered

### 3. Build and install the service

```bash
mise http-install-service
```

### 4. Verify

```bash
sudo systemctl status lince --no-pager -l
curl -v http://127.0.0.1:6174/
```

## NixOS VPS

Assumptions:

- the repo lives at `/root/git/lince-social/lince`
- you log in as `root`
- Nix with flakes is available

### 1. Clone the repo

```bash
mkdir -p /root/git/lince-social
cd /root/git/lince-social
git clone https://github.com/lince-social/lince.git
cd lince
git checkout dev
```

### 2. Create the runtime env file

```bash
mkdir -p /var/lib/lince
cat >/var/lib/lince/lince.env <<'EOF'
SECRET=replace-with-a-long-random-secret
EOF
chmod 600 /var/lib/lince/lince.env
```

### 3. Switch to the NixOS configuration

```bash
sudo nixos-rebuild switch --flake /root/git/lince-social/lince#manas-organ
```

### 4. Seed the root API user

```bash
ROOT_USER_PASSWORD='replace-me' \
  nix develop -c cargo run --no-default-features --features "karma,http" -- \
    --seed-root-user \
    --seed-root-user-password "$ROOT_USER_PASSWORD"
```

### 5. Verify

```bash
sudo systemctl status lince --no-pager -l
curl -v http://127.0.0.1:6174/
```

## Automated Deploys

### Ubuntu

Active on `push` to `dev`:

- GitHub Action SSHes into the VPS
- resets the checkout to `origin/dev`
- runs `mise http-build`
- installs/restarts the service

### NixOS

Present in the repo but manual for now:

- GitHub Action `deploy-dev-manas-organ`
- SSHes into the VPS
- resets the checkout to `origin/dev`
- runs `sudo nixos-rebuild switch --flake .#manas-organ`
