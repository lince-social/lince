import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def snapshot(root):
    paths = subprocess.check_output(
        [
            "git", "ls-files", "-z", "--", "Cargo.toml", "Cargo.lock",
            "rust-toolchain.toml", ".cargo", "crates", "assets", "vendor",
        ],
        cwd=root,
    ).split(b"\0")
    entries = {}
    directories = {}
    for raw in paths:
        if not raw:
            continue
        name = os.fsdecode(raw)
        path = root / name
        if path.is_symlink() or not path.is_file():
            continue
        if not path.resolve().is_relative_to(root.resolve()):
            continue
        if path.suffix == ".lingua":
            continue
        stat = path.stat()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        identity = [digest, stat.st_mode]
        entries[name] = {"identity": identity, "mtime": stat.st_mtime_ns}
        for parent in path.relative_to(root).parents:
            if parent == Path("."):
                continue
            directories.setdefault(str(parent), []).append([name, identity])
    for name, contents in directories.items():
        digest = hashlib.sha256(json.dumps(sorted(contents)).encode()).hexdigest()
        entries[name] = {
            "identity": [digest, (root / name).stat().st_mode],
            "mtime": (root / name).stat().st_mtime_ns,
        }
    return entries


def restore(root, state):
    current = snapshot(root)
    previous = json.loads(state.read_text()) if state.exists() else {}
    restored = 0
    for name, entry in current.items():
        path = root / name
        old = previous.get(name)
        if old and old["identity"] == entry["identity"]:
            os.utime(path, ns=(path.stat().st_atime_ns, old["mtime"]))
            restored += 1
        else:
            os.utime(path, None)
    state.parent.mkdir(parents=True, exist_ok=True)
    state.write_text(json.dumps(snapshot(root), sort_keys=True))
    print(f"Restored timestamps for {restored}/{len(current)} unchanged build inputs")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--state", type=Path, default=Path("target/source-cache.json"))
    args = parser.parse_args()
    restore(args.root, args.root / args.state)
