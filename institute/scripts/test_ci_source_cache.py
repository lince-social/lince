import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "ci_source_cache", Path(__file__).with_name("ci-source-cache.py")
)
source_cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source_cache)


class SourceCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.state = self.root / "target/debug/ci-source-cache.json"
        subprocess.run(["git", "init", "-q", self.root], check=True)
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/*"]\nresolver = "3"\n')
        self.write(
            "crates/dependency/Cargo.toml",
            '[package]\nname = "dependency"\nversion = "0.1.0"\nedition = "2024"\n',
        )
        self.source = self.write(
            "crates/dependency/src/lib.rs", "pub fn value() -> u32 { 1 }\n"
        )
        self.write(
            "crates/dependency/build.rs",
            'fn main() { println!("cargo:rerun-if-changed=data"); }\n',
        )
        self.data = self.write("crates/dependency/data/input.txt", "first\n")
        self.write(
            "crates/application/Cargo.toml",
            '[package]\nname = "application"\nversion = "0.1.0"\nedition = "2024"\n'
            '[dependencies]\ndependency = { path = "../dependency" }\n',
        )
        self.write(
            "crates/application/src/lib.rs",
            "pub fn value() -> u32 { dependency::value() }\n",
        )
        self.track()

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def track(self):
        subprocess.run(["git", "add", "Cargo.toml", "crates"], cwd=self.root, check=True)

    def restore(self):
        source_cache.restore(self.root, self.state)

    def checkout(self):
        for name in source_cache.snapshot(self.root):
            os.utime(self.root / name, None)

    def check(self, revision=None):
        environment = os.environ.copy()
        for name in (
            "CARGO_TARGET_DIR", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_ENCODED_RUSTFLAGS",
            "LINCE_REVISION",
        ):
            environment.pop(name, None)
        environment["RUSTFLAGS"] = environment.get("RUSTFLAGS", "") + " -D warnings"
        if revision is not None:
            environment["LINCE_REVISION"] = revision
        result = subprocess.run(
            ["cargo", "check", "--offline", "--message-format=json"],
            cwd=self.root, env=environment, text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        artifacts = [json.loads(line) for line in result.stdout.splitlines()]
        return {
            artifact["target"]["name"]: artifact["fresh"]
            for artifact in artifacts
            if artifact["reason"] == "compiler-artifact"
        }

    def prime(self):
        self.restore()
        artifacts = self.check()
        self.assertFalse(artifacts["dependency"])
        self.assertFalse(artifacts["application"])

    def test_fresh_checkout_reuses_compiled_workspace_and_dependencies(self):
        self.prime()
        self.checkout()
        self.restore()
        artifacts = self.check()
        self.assertTrue(artifacts["dependency"])
        self.assertTrue(artifacts["application"])

    def test_changed_source_recompiles_even_with_unchanged_size_and_timestamp(self):
        self.prime()
        original = self.source.stat()
        self.source.write_text("pub fn value() -> u32 { 2 }\n")
        os.utime(self.source, ns=(original.st_atime_ns, original.st_mtime_ns))
        self.restore()
        artifacts = self.check()
        self.assertFalse(artifacts["dependency"])
        self.assertFalse(artifacts["application"])

    def test_changed_application_keeps_dependency_compiled(self):
        self.prime()
        self.checkout()
        self.write(
            "crates/application/src/lib.rs",
            "pub fn value() -> u32 { dependency::value() + 1 }\n",
        )
        self.restore()
        artifacts = self.check()
        self.assertTrue(artifacts["dependency"])
        self.assertFalse(artifacts["application"])

    def test_new_directory_input_reruns_build_script(self):
        self.prime()
        self.write("crates/dependency/data/new.txt", "second\n")
        self.track()
        self.restore()
        self.assertFalse(self.check()["dependency"])

    def test_changed_revision_recompiles_with_restored_timestamps(self):
        self.source.write_text(
            self.source.read_text()
            + 'pub const REVISION: Option<&str> = option_env!("LINCE_REVISION");\n'
        )
        self.restore()
        self.check(revision="first")
        self.checkout()
        self.restore()
        artifacts = self.check(revision="second")
        self.assertFalse(artifacts["dependency"])
        self.assertFalse(artifacts["application"])

    def test_removed_directory_input_reruns_build_script(self):
        self.prime()
        self.data.unlink()
        self.track()
        self.restore()
        self.assertFalse(self.check()["dependency"])

    def test_permission_change_keeps_new_timestamp(self):
        self.restore()
        original = self.source.stat().st_mtime_ns
        self.source.chmod(0o755)
        self.restore()
        self.assertNotEqual(self.source.stat().st_mtime_ns, original)
        self.assertEqual(self.source.stat().st_mode & 0o777, 0o755)

    def test_cached_paths_cannot_touch_files_outside_checkout(self):
        self.restore()
        with tempfile.TemporaryDirectory() as outside:
            external = Path(outside) / "external.rs"
            external.write_text("external\n")
            original = external.stat().st_mtime_ns
            link = self.root / "crates/dependency/src/link.rs"
            link.symlink_to(external)
            self.track()
            state = json.loads(self.state.read_text())
            state[str(external)] = {"identity": [], "mtime": 1}
            state["../external.rs"] = {"identity": [], "mtime": 1}
            self.state.write_text(json.dumps(state))
            self.restore()
            self.assertEqual(external.stat().st_mtime_ns, original)
            self.assertTrue(link.is_symlink())


if __name__ == "__main__":
    unittest.main()
