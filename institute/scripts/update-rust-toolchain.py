import argparse
import os
from pathlib import Path
import re
import subprocess
import tomllib
import urllib.request


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = 'https://static.rust-lang.org/dist/channel-rust-stable.toml'
NIX_VERSION = '''
let
  flake = builtins.getFlake (builtins.getEnv "LINCE_RUST_UPDATE_FLAKE");
  pkgs = import flake.inputs.nixpkgs {
    system = builtins.currentSystem;
    overlays = [ flake.inputs.rust-overlay.overlays.default ];
  };
in pkgs.rust-bin.stable.${builtins.getEnv "LINCE_RUST_UPDATE_VERSION"}.minimal.version
'''


def stable_version(value):
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', value):
        raise ValueError(f'Expected an exact stable Rust version, got {value!r}')
    return tuple(map(int, value.split('.')))


def update(root, latest, check=False):
    path = root / 'rust-toolchain.toml'
    original = path.read_text()
    config = tomllib.loads(original)['toolchain']
    current = config['channel']
    current_version = stable_version(current)
    latest_version = stable_version(latest)
    if current_version > latest_version:
        raise ValueError(f'Refusing to downgrade Rust {current} to {latest}')
    if current_version == latest_version:
        print(f'Rust {current} is already the latest stable release; no cache inputs changed.')
        return 0
    if check:
        print(f'Rust update available: {current} -> {latest}')
        return 1
    environment = dict(os.environ, LINCE_RUST_UPDATE_FLAKE='git+' + root.as_uri(), LINCE_RUST_UPDATE_VERSION=latest)
    subprocess.run(['nix', 'flake', 'update', 'rust-overlay'], cwd=root, check=True)
    available = subprocess.check_output(
        ['nix', 'eval', '--raw', '--impure', '--expr', NIX_VERSION],
        cwd=root, env=environment, text=True,
    ).strip()
    if available != latest:
        raise ValueError(f'Nix resolved Rust {available!r} instead of {latest}')
    command = ['rustup', 'toolchain', 'install', latest, '--profile', config.get('profile', 'minimal'), '--no-self-update']
    for component in config.get('components', []):
        command.extend(['--component', component])
    for target in config.get('targets', []):
        command.extend(['--target', target])
    subprocess.run(command, cwd=root, check=True)
    installed = subprocess.check_output(['rustup', 'run', latest, 'rustc', '--version'], cwd=root, text=True).split()[1]
    if installed != latest:
        raise ValueError(f'Rustup installed Rust {installed!r} instead of {latest}')
    if path.read_text() != original:
        raise ValueError('rust-toolchain.toml changed during the update; leaving its current contents intact')
    updated, count = re.subn(
        r'''(?m)^(\s*channel\s*=\s*)(["'])''' + re.escape(current) + r'\2',
        lambda match: match[1] + match[2] + latest + match[2], original, count=1,
    )
    if count != 1 or tomllib.loads(updated)['toolchain']['channel'] != latest:
        raise ValueError('Could not update the Rust channel without changing other settings')
    path.write_text(updated)
    print(f'Updated shared Rust pin: {current} -> {latest}. Re-enter the Nix shell before compiling.')
    return 0


def main():
    parser = argparse.ArgumentParser(description='Update the shared Rust pin and Nix toolchain metadata to latest stable.')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true', help='Report an available update without changing files or installing tools.')
    mode.add_argument('--latest', action='store_true', help='Print the official latest stable version without changing files or installing tools.')
    args = parser.parse_args()
    with urllib.request.urlopen(MANIFEST, timeout=30) as response:
        manifest = tomllib.loads(response.read().decode())
    latest = manifest['pkg']['rust']['version'].split()[0]
    stable_version(latest)
    if args.latest:
        print(latest)
        return 0
    return update(ROOT, latest, args.check)


if __name__ == '__main__':
    raise SystemExit(main())
