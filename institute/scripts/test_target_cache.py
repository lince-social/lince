import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("target_cache_installer", ROOT / "institute/scripts/install-target-cache.py")
INSTALLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INSTALLER)


class TargetCacheTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="lince-target-policy-")
        cls.root = Path(cls.directory.name)
        cls.executable = cls.root / "pruner"
        subprocess.run(["rustc", "--edition", "2024", "-D", "warnings", str(ROOT / "institute/scripts/target-cache.rs"),
                        "-o", str(cls.executable)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def test_policy_unit_tests(self):
        executable = self.root / "policy-tests"
        subprocess.run(["rustc", "--edition", "2024", "-D", "warnings", "--test",
                        str(ROOT / "crates/xtask/src/prune/budget.rs"), "-o", str(executable)], check=True)
        value = subprocess.run([str(executable)], check=True, capture_output=True, text=True)
        self.assertIn("11 passed; 0 failed", value.stdout)

    def test_existing_incremental_cleanup_and_budget_share_one_workflow(self):
        source = self.root / "cleanup-workflow.rs"
        (self.root / "prune").mkdir(exist_ok=True)
        shutil.copy2(ROOT / "crates/xtask/src/prune.rs", self.root / "prune.rs")
        shutil.copy2(ROOT / "crates/xtask/src/prune/budget.rs", self.root / "prune/budget.rs")
        fixture = json.dumps(str(self.root / "workflow-root"), ensure_ascii=False)
        source.write_text(f"mod prune;\n#[test] fn workflow_entrypoint_preserves_the_operation() {{ assert_eq!(prune::with_cleanup(std::path::Path::new({fixture}), || Ok(42)), Ok(42)); }}\n")
        executable = self.root / "cleanup-workflow-tests"
        subprocess.run(["rustc", "--edition", "2024", "-D", "warnings", "--test", str(source), "-o", str(executable)], check=True)
        result = subprocess.run([str(executable), "--skip", "prune::budget::tests"], capture_output=True, text=True, check=True)
        self.assertIn("0 failed", result.stdout)
        self.assertIn("workflow_entrypoint_preserves_the_operation", result.stdout)

    def test_timer_is_low_priority_and_escapes_systemd_arguments(self):
        values = INSTALLER.units(Path('/private/$name%/tool with spaces'), Path('/private/cache%/target'))
        service = values["lince-target-cache.service"]
        self.assertIn('ExecStart="/private/$$name%%/tool with spaces" "/private/cache%%/target"', service)
        self.assertIn("ConditionPathIsDirectory=/private/cache%%/target", service)
        self.assertIn("Nice=19", service)
        self.assertIn("IOSchedulingClass=idle", service)
        self.assertIn("OnUnitInactiveSec=5min", values["lince-target-cache.timer"])

    @unittest.skipUnless(shutil.which("systemd-analyze"), "systemd unit verifier unavailable")
    def test_real_systemd_parser_accepts_the_service_and_timer(self):
        with tempfile.TemporaryDirectory(prefix="lince-unit-policy-") as temporary:
            directory = Path(temporary)
            target = directory / "cache with spaces%" / "target"
            target.mkdir(parents=True)
            paths = []
            for name, value in INSTALLER.units(self.executable, target).items():
                path = directory / name
                path.write_text(value)
                paths.append(str(path))
            result = subprocess.run(["systemd-analyze", "verify", *paths], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn("ConditionPath", result.stderr)
            self.assertNotIn("ignoring", result.stderr)

    def test_actual_cargo_lock_running_binary_and_rebuild_after_eviction(self):
        with tempfile.TemporaryDirectory(prefix="lince-cargo-policy-") as temporary:
            root = Path(temporary)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text('[package]\nname="cache_fixture"\nversion="0.0.0"\nedition="2024"\n')
            (root / "src/main.rs").write_text('fn main() { if std::env::var_os("HOLD_EXECUTABLE").is_some() { println!("ready"); let mut line = String::new(); std::io::stdin().read_line(&mut line).unwrap(); } }\n')
            (root / "build.rs").write_text('fn main() { if std::env::var_os("HOLD_BUILD").is_some() { std::fs::write("ready", "ready").unwrap(); let start = std::time::Instant::now(); while !std::path::Path::new("release").exists() { assert!(start.elapsed().as_secs() < 30); std::thread::sleep(std::time::Duration::from_millis(10)); } } }\n')
            environment = os.environ.copy()
            environment.update(RUSTC_WRAPPER="", HOLD_BUILD="1")
            environment.setdefault("RUSTUP_TOOLCHAIN", tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"])
            environment.pop("CARGO_TARGET_DIR", None)
            environment.pop("CARGO_BUILD_TARGET", None)
            environment.pop("CARGO_BUILD_BUILD_DIR", None)
            target = root / "target"
            cargo = ["cargo", "build", "--offline", "--target-dir", str(target)]
            with (root / "build.log").open("w") as log:
                process = subprocess.Popen(cargo, cwd=root, env=environment, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 30
                    while not (root / "ready").exists():
                        self.assertIsNone(process.poll(), (root / "build.log").read_text())
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.02)
                    held = subprocess.run([str(self.executable), str(target), "--max-bytes", "1"], check=True, capture_output=True, text=True)
                    self.assertIn("0 artifact groups removed", held.stdout)
                    self.assertIn("1 busy profiles", held.stdout)
                    (root / "release").touch()
                    self.assertEqual(process.wait(timeout=30), 0, (root / "build.log").read_text())
                finally:
                    if process.poll() is None:
                        process.terminate()
                        process.wait(timeout=5)
            executable = target / "debug/cache_fixture"
            live_environment = {**environment, "HOLD_EXECUTABLE": "1"}
            live = subprocess.Popen([str(executable)], env=live_environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            try:
                self.assertEqual(live.stdout.readline().strip(), "ready")
                subprocess.run([str(self.executable), str(target), "--max-bytes", "1"], check=True, capture_output=True)
                self.assertTrue(executable.exists(), "Live executable was evicted")
                live.communicate("finish\n", timeout=5)
            finally:
                if live.poll() is None:
                    live.terminate()
                    live.wait(timeout=5)
            subprocess.run([str(self.executable), str(target), "--max-bytes", "1"], check=True, capture_output=True)
            self.assertFalse(executable.exists(), "Inactive compiled executable was not eligible for eviction")
            environment.pop("HOLD_BUILD")
            rebuilt = subprocess.run(cargo, cwd=root, env=environment, capture_output=True, text=True, timeout=30)
            self.assertEqual(rebuilt.returncode, 0, rebuilt.stderr)
            subprocess.run([str(executable)], check=True, timeout=5)


if __name__ == "__main__":
    unittest.main()
