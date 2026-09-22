import argparse
import base64
import hashlib
import json
from pathlib import Path
import tarfile
import tempfile
import shutil
import tomllib


def package(root, artifacts, repository, tag, revision):
    asset = artifacts / "lince-x86_64-unknown-linux-gnu.tar.gz"
    checksum = hashlib.sha256(asset.read_bytes()).digest()
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    release = {
        "version": version,
        "revision": revision,
        "url": f"https://github.com/{repository}/releases/download/{tag}/{asset.name}",
        "hash": "sha256-" + base64.b64encode(checksum).decode(),
    }
    with tempfile.TemporaryDirectory() as temporary:
        flake = Path(temporary) / "lince"
        flake.mkdir()
        shutil.copyfile(root / "scripts/ci/binary-flake.nix", flake / "flake.nix")
        shutil.copyfile(root / "flake.lock", flake / "flake.lock")
        shutil.copyfile(root / "scripts/deploy/nixos/lince-module.nix", flake / "lince-module.nix")
        (flake / "binary.json").write_text(json.dumps(release, indent=2) + "\n")
        destination = artifacts / "lince-flake.tar.gz"
        with tarfile.open(destination, "w:gz") as archive:
            archive.add(flake, arcname="lince")
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_suffix(".gz.sha256").write_text(f"{digest}  {destination.name}\n")
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--artifacts", type=Path, default=Path("release-artifacts"))
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    print(package(args.root, args.artifacts, args.repository, args.tag, args.revision))
