import contextlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('cache_identity', ROOT / 'institute/scripts/ci-cache-identity.py')
IDENTITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IDENTITY)


class CacheIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.previous = Path.cwd()
        self.addCleanup(os.chdir, self.previous)
        for name in (
            'institute/scripts/ci-cache-identity.py',
            'institute/scripts/ci-source-cache.py',
            'institute/scripts/ci-linux-setup.sh',
            '.github/actions/rust-artifacts/action.yml',
            '.github/workflows/build.yml',
            '.github/workflows/android.yml',
        ):
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)
        for name, value in {
            'Cargo.toml': '[workspace]\nmembers = []\n',
            'Cargo.lock': 'version = 4\n',
            '.cargo/config.toml': '[build]\n',
            'rust-toolchain.toml': '[toolchain]\nchannel = "1.96.0"\n',
            'crates/probe/src/lib.rs': 'pub fn value() -> u32 { 1 }\n',
            'crates/mobile/android/app/build.gradle': 'native flags\n',
            'native/lince-native-inputs': 'native compiler and libraries v1\n',
        }.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value)
        os.chdir(self.root)
        subprocess.run(['git', 'init', '-q'], check=True)
        subprocess.run(['git', 'add', '.'], check=True)
        self.environment = mock.patch.dict(os.environ, {
            'CACHE_SCOPE': 'linux-checks', 'CACHE_TARGET': 'x86_64-unknown-linux-gnu',
            'RUNNER_TEMP': str(self.root / 'native'), 'RUNNER_OS': 'Linux',
            'RUNNER_ARCH': 'X64', 'RUSTFLAGS': '-D warnings',
        })
        self.environment.start()
        self.addCleanup(self.environment.stop)
        original = subprocess.check_output
        self.compiler = b'rustc 1.96.0\n'

        def output(command, *args, **kwargs):
            return self.compiler if command == ['rustc', '-vV'] else original(command, *args, **kwargs)

        patch = mock.patch.object(IDENTITY.subprocess, 'check_output', side_effect=output)
        patch.start()
        self.addCleanup(patch.stop)

    def identity(self):
        stream = io.StringIO()
        with contextlib.redirect_stdout(stream):
            IDENTITY.main()
        outputs = {}
        lines = iter(stream.getvalue().splitlines())
        for line in lines:
            if '<<' in line:
                name, delimiter = line.split('<<', 1)
                value = []
                for entry in lines:
                    if entry == delimiter:
                        break
                    value.append(entry)
                outputs[name] = '\n'.join(value)
            else:
                name, value = line.split('=', 1)
                outputs[name] = value
        return outputs

    def test_unchanged_bytes_reuse_key_despite_checkout_timestamps(self):
        before = self.identity()
        source = self.root / 'crates/probe/src/lib.rs'
        stat = source.stat()
        os.utime(source, ns=(stat.st_atime_ns, stat.st_mtime_ns + 1000000000))
        self.assertEqual(before, self.identity())

    def test_application_edits_advance_snapshot_with_compatible_restore(self):
        before = self.identity()
        source = self.root / 'crates/probe/src/lib.rs'
        source.write_text('pub fn value() -> u32 { 2 }\n')
        changed = self.identity()
        self.assertNotEqual(before['key'], changed['key'])
        self.assertEqual(before['prefix'], changed['prefix'])
        self.assertEqual(before['restore-keys'].splitlines()[1:], changed['restore-keys'].splitlines()[1:])
        added = self.root / 'crates/probe/src/added.rs'
        added.write_text('pub fn added() {}\n')
        subprocess.run(['git', 'add', str(added)], check=True)
        self.assertNotEqual(changed['key'], self.identity()['key'])
        added.unlink()
        self.assertEqual(changed['key'], self.identity()['key'])

    def test_profile_manifest_change_refreshes_cache_with_compatible_restore(self):
        before = self.identity()
        (self.root / 'Cargo.toml').write_text('[workspace]\nmembers = []\n[profile.dev]\nopt-level = 1\n')
        after = self.identity()
        self.assertNotEqual(before['key'], after['key'])
        self.assertEqual(before['prefix'], after['prefix'])

    def test_native_flags_and_compiler_isolate_restores(self):
        initial = self.identity()
        for variable, value in (
            ('RUSTFLAGS', '-D warnings -C target-cpu=x86-64'), ('CACHE_SCOPE', 'linux-release'),
            ('CACHE_TARGET', 'aarch64-unknown-linux-gnu'), ('CARGO_PROFILE_RELEASE_LTO', 'thin'),
        ):
            with mock.patch.dict(os.environ, {variable: value}):
                self.assertNotEqual(initial['prefix'], self.identity()['prefix'])
        self.compiler = b'rustc 1.97.0\n'
        self.assertNotEqual(initial['prefix'], self.identity()['prefix'])
        self.compiler = b'rustc 1.96.0\n'
        (self.root / 'native/lince-native-inputs').write_text('new native library ABI\n')
        self.assertNotEqual(initial['prefix'], self.identity()['prefix'])

    def test_lock_update_prefers_same_graph_then_restores_compatible_packages(self):
        initial = self.identity()
        (self.root / 'Cargo.lock').write_text('version = 4\nnew dependency\n')
        changed = self.identity()
        self.assertNotEqual(initial['key'], changed['key'])
        self.assertEqual(initial['prefix'], changed['prefix'])
        source, preferred, fallback = changed['restore-keys'].splitlines()
        self.assertEqual(source, changed['key'])
        self.assertTrue(changed['key'].startswith(preferred))
        self.assertFalse(initial['key'].startswith(preferred))
        self.assertTrue(initial['key'].startswith(fallback))

    def test_workflow_and_action_orchestration_do_not_invalidate_compilation(self):
        initial = self.identity()
        for name in ('.github/workflows/build.yml', '.github/workflows/android.yml', '.github/actions/rust-artifacts/action.yml'):
            path = self.root / name
            path.write_text(path.read_text() + '\n# Extra gate or upload step\n')
        self.assertEqual(initial, self.identity())

    def test_compiled_assets_and_modes_advance_source_snapshot(self):
        path = self.root / 'institute/assets/fixture.txt'
        path.parent.mkdir(parents=True)
        path.write_text('first')
        subprocess.run(['git', 'add', str(path)], check=True)
        initial = self.identity()
        path.write_text('other')
        self.assertNotEqual(initial['key'], self.identity()['key'])
        changed = self.identity()
        path.chmod(0o755)
        self.assertNotEqual(changed['key'], self.identity()['key'])

    def test_only_validation_scope_caches_test_executables_and_never_results(self):
        self.assertNotIn('/deps/lince_interface-*', self.identity()['paths'])
        with mock.patch.dict(os.environ, {'CACHE_SCOPE': 'linux-validation'}):
            paths = self.identity()['paths']
        self.assertIn('/deps/lince_interface-*', paths)
        self.assertIn('/deps/lince_park-*', paths)
        self.assertNotIn('park-validation-tests', paths)

    def test_android_gradle_native_flags_invalidate_compatible_prefix(self):
        with mock.patch.dict(os.environ, {'CACHE_SCOPE': 'android-arm64-package', 'CACHE_TARGET': 'aarch64-linux-android'}):
            before = self.identity()
            (self.root / 'crates/mobile/android/app/build.gradle').write_text('different Rust flags\n')
            self.assertNotEqual(before['prefix'], self.identity()['prefix'])

    def test_linux_native_installer_changes_invalidate_compatible_prefix(self):
        before = self.identity()
        path = self.root / 'institute/scripts/ci-linux-setup.sh'
        path.write_text(path.read_text() + '\nprintf new-native-configuration\n')
        self.assertNotEqual(before['prefix'], self.identity()['prefix'])

    def test_unsafe_scope_is_rejected(self):
        with mock.patch.dict(os.environ, {'CACHE_SCOPE': '../another-platform'}):
            with self.assertRaises(ValueError):
                self.identity()


if __name__ == '__main__':
    unittest.main()
