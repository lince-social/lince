import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


cache = load("source-cache")
package_nix = load("package-nix")


class SourceCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.state = self.root / "target/source-cache.json"
        subprocess.run(["git", "init", "-q", self.root], check=True)

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        subprocess.run(["git", "add", name], cwd=self.root, check=True)
        return path

    def test_only_identical_files_and_directories_keep_their_timestamps(self):
        unchanged = self.write("crates/one/src/lib.rs", "pub fn one() {}\n")
        changed = self.write("crates/two/src/lib.rs", "pub fn two() {}\n")
        removed = self.write("crates/two/src/old.rs", "pub fn old() {}\n")
        cache.restore(self.root, self.state)
        previous = json.loads(self.state.read_text())
        for path in [unchanged, changed, unchanged.parent, changed.parent]:
            os.utime(path, ns=(1_000_000_000, 1_000_000_000))
        changed.write_text("pub fn new() {}\n")
        removed.unlink()
        self.write("crates/two/src/new.rs", "pub fn added() {}\n")
        cache.restore(self.root, self.state)
        self.assertEqual(unchanged.stat().st_mtime_ns, previous["crates/one/src/lib.rs"]["mtime"])
        self.assertEqual(unchanged.parent.stat().st_mtime_ns, previous["crates/one/src"]["mtime"])
        self.assertNotEqual(changed.stat().st_mtime_ns, previous["crates/two/src/lib.rs"]["mtime"])
        self.assertNotEqual(changed.parent.stat().st_mtime_ns, previous["crates/two/src"]["mtime"])
        self.assertNotIn("crates/two/src/old.rs", json.loads(self.state.read_text()))

    @unittest.skipUnless(shutil.which("cargo"), "cargo is required")
    def test_fresh_checkout_reuses_crates_and_rebuilds_changed_inputs(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/base", "crates/app"]\nresolver = "3"\n')
        self.write("crates/base/Cargo.toml", '[package]\nname = "cache-base"\nversion = "0.1.0"\nedition = "2024"\n')
        base = self.write("crates/base/src/lib.rs", "pub fn value() -> u8 { 1 }\n")
        self.write("crates/app/Cargo.toml", '[package]\nname = "cache-app"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\ncache-base = { path = "../base" }\n')
        app = self.write("crates/app/src/main.rs", 'fn main() { println!("{} {:?}", cache_base::value(), option_env!("LINCE_REVISION")); }\n')
        self.write("crates/app/build.rs", 'fn main() { println!("cargo:rerun-if-changed=../../assets"); }\n')
        asset = self.write("assets/input.txt", "original\n")
        environment = dict(os.environ, CARGO_TARGET_DIR=str(self.root / "target"), CARGO_INCREMENTAL="0", RUSTFLAGS="-D warnings", LINCE_REVISION="first")

        def check():
            result = subprocess.run(
                ["cargo", "check", "--offline", "--message-format=json"],
                cwd=self.root, env=environment, check=True, capture_output=True, text=True,
            )
            return {
                event["target"]["name"]: event["fresh"]
                for line in result.stdout.splitlines()
                if (event := json.loads(line))["reason"] == "compiler-artifact"
                and event["target"]["kind"] != ["custom-build"]
            }

        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": False, "cache-app": False})
        archive_path = self.root / "cache.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(self.root / "target", arcname="target")
        shutil.rmtree(self.root / "target")
        with tarfile.open(archive_path) as archive:
            archive.extractall(self.root, filter="data")
        for path in [base, app, asset, asset.parent]:
            os.utime(path, None)
        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": True, "cache-app": True})
        environment["LINCE_REVISION"] = "second"
        self.assertEqual(check(), {"cache_base": True, "cache-app": False})
        app.write_text('fn main() { println!("changed: {}", cache_base::value()); }\n')
        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": True, "cache-app": False})
        asset.unlink()
        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": True, "cache-app": False})
        self.write("assets/new.txt", "added\n")
        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": True, "cache-app": False})
        base.write_text("pub fn value() -> u8 { 2 }\n")
        cache.restore(self.root, self.state)
        self.assertEqual(check(), {"cache_base": False, "cache-app": False})


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="lince release ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.artifacts = self.root / "artifacts"
        self.artifacts.mkdir()
        self.log = self.root / "gh-arguments"
        gh = self.root / "gh"
        gh.write_text('#!/usr/bin/env bash\nset -eu\nprintf "%s\\0" "$@" >> "$GH_LOG"\nif [ "$2" = view ]; then exit "${GH_VIEW_STATUS:-0}"; fi\n')
        gh.chmod(0o755)
        self.environment = dict(os.environ, PATH=f"{self.root}:{os.environ['PATH']}", GH_LOG=str(self.log), GH_REPO="owner/repo", GITHUB_SHA="abc123")

    def publish(self):
        return subprocess.run(
            ["bash", SCRIPTS / "publish-main.sh", self.artifacts],
            env=self.environment, capture_output=True, text=True,
        )

    def test_linux_only_upload_never_passes_an_unmatched_zip_pattern(self):
        archive = self.artifacts / "lince-x86_64-unknown-linux-gnu.tar.gz"
        archive.write_bytes(b"binary")
        archive.with_suffix(".gz.sha256").write_text("checksum")
        self.environment["GH_VIEW_STATUS"] = "1"
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        arguments = self.log.read_bytes().split(b"\0")
        self.assertIn(b"create", arguments)
        self.assertIn(os.fsencode(archive), arguments)
        self.assertFalse(any(b"*" in argument for argument in arguments))
        self.assertFalse(any(b".zip" in argument for argument in arguments))

    def test_windows_only_upload_and_existing_release(self):
        archive = self.artifacts / "lince-x86_64-pc-windows-msvc.zip"
        archive.write_bytes(b"binary")
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        arguments = self.log.read_bytes().split(b"\0")
        self.assertNotIn(b"create", arguments)
        self.assertIn(os.fsencode(archive), arguments)

    def test_empty_artifacts_fail_before_contacting_github(self):
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertFalse(self.log.exists())

    def test_nix_flake_pins_the_immutable_release_and_binary_checksum(self):
        (self.artifacts / "lince-x86_64-unknown-linux-gnu.tar.gz").write_bytes(b"binary")
        destination = package_nix.package(ROOT, self.artifacts, "owner/repo", "rolling-42", "a" * 40)
        with tarfile.open(destination) as archive:
            release = json.load(archive.extractfile("lince/binary.json"))
            self.assertEqual(release["revision"], "a" * 40)
            self.assertEqual(release["hash"], "sha256-mjpF0BUxog6JrGrhCwsL6wSSrNchajaKoGLRpf7K+c0=")
            self.assertEqual(release["url"], "https://github.com/owner/repo/releases/download/rolling-42/lince-x86_64-unknown-linux-gnu.tar.gz")
            self.assertEqual(set(archive.getnames()), {"lince", "lince/binary.json", "lince/flake.nix", "lince/flake.lock", "lince/lince-module.nix"})


if __name__ == "__main__":
    unittest.main()
