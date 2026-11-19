#!/usr/bin/env python3
"""Run local Clippy and reject unwrap diagnostics outside the migration inventory."""

from collections import Counter
from hashlib import sha256
import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
BASELINE = Path(__file__).with_name("rust-unwrap-inventory.json")


def anchor(lines, line):
    """Identify a source line without tying its exception to a line number."""
    text = lines[line]
    before = lines[line - 1] if line else ""
    return sha256(f"{before.strip()}\n{text.strip()}".encode()).hexdigest()[:24]


def location(message, manifest):
    span = next((span for span in message["spans"] if span["is_primary"]), None)
    if span is None:
        return None
    file = Path(span["file_name"])
    for directory in (Path.cwd(), ROOT, manifest.parent):
        path = (directory / file).resolve()
        if not path.is_relative_to(ROOT) or not path.is_file():
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        line = span["line_end"] - 1
        if not 0 <= line < len(lines):
            continue
        if not span["text"] or span["text"][-1]["text"] != lines[line]:
            continue
        key = path.relative_to(ROOT).as_posix(), anchor(lines, line)
        position = key[0], span["byte_start"], span["byte_end"]
        return key, position
    return None


def check(args):
    if "--" in args or any(arg.startswith("--message-format") for arg in args):
        raise SystemExit("Pass Cargo package, target, or feature flags only.")
    inventory = json.loads(BASELINE.read_text(encoding="utf-8"))
    existing = {path: Counter(keys.split()) for path, keys in inventory.items()}
    if not args:
        args = ["--locked", "--workspace", "--all-targets"]
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "unwrap-check"))
    cmd = [
        "cargo", "clippy", *args, "--keep-going", "--message-format=json",
        "--", "-A", "clippy::all", "--force-warn", "clippy::unwrap_used",
    ]
    seen = set()
    grandfathered = 0
    new = 0
    with subprocess.Popen(cmd, stdout=subprocess.PIPE, env=env, text=True) as process:
        for line in process.stdout:
            try:
                event = json.loads(line)
            except ValueError:
                print(line, end="")
                continue
            if event.get("reason") != "compiler-message":
                continue
            message = event["message"]
            rendered = message.get("rendered") or message["message"] + "\n"
            if (message.get("code") or {}).get("code") != "clippy::unwrap_used":
                sys.stderr.write(rendered)
                continue
            found = location(message, Path(event["manifest_path"]))
            if found:
                (path, key), position = found
                if position in seen:
                    continue
                seen.add(position)
                if path in existing and existing[path][key] > 0:
                    existing[path][key] -= 1
                    grandfathered += 1
                    continue
            new += 1
            sys.stderr.write(rendered)
        status = process.wait()
    print(f"unwrap audit: {grandfathered} existing diagnostics, {new} outside the inventory")
    return status or bool(new)


if __name__ == "__main__":
    sys.exit(check(sys.argv[1:]))
