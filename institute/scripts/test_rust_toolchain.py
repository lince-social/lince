import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('update_rust', ROOT / 'institute/scripts/update-rust-toolchain.py')
UPDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPDATE)


class RustToolchainTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.path = self.root / 'rust-toolchain.toml'
        self.original = '[toolchain]\nchannel = "1.96.0"\nprofile = "minimal"\ncomponents = ["rustfmt"]\ntargets = ["aarch64-linux-android"]\n'
        self.path.write_text(self.original)
        self.output = io.StringIO()
        redirect = contextlib.redirect_stdout(self.output)
        redirect.__enter__()
        self.addCleanup(redirect.__exit__, None, None, None)
        run = mock.patch.object(UPDATE.subprocess, 'run')
        self.run = run.start()
        self.addCleanup(run.stop)
        check_output = mock.patch.object(UPDATE.subprocess, 'check_output', side_effect=['1.99.1', 'rustc 1.99.1 (hash date)\n'])
        self.check_output = check_output.start()
        self.addCleanup(check_output.stop)

    def test_same_release_does_not_install_or_invalidate_caches(self):
        self.assertEqual(UPDATE.update(self.root, '1.96.0'), 0)
        self.assertEqual(self.path.read_text(), self.original)
        self.run.assert_not_called()
        self.check_output.assert_not_called()

    def test_check_reports_new_release_without_mutating(self):
        self.assertEqual(UPDATE.update(self.root, '1.99.1', check=True), 1)
        self.assertEqual(self.path.read_text(), self.original)
        self.run.assert_not_called()

    def test_floating_prerelease_and_downgrade_are_rejected(self):
        for release in ('stable', 'nightly', '1.99.1-beta.1', '1.95.0', '1.99.1\n'):
            with self.subTest(release=release), self.assertRaises(ValueError):
                UPDATE.update(self.root, release)
        self.assertEqual(self.path.read_text(), self.original)
        self.run.assert_not_called()

    def test_updater_keeps_settings_and_updates_only_toolchain_input(self):
        self.assertEqual(UPDATE.update(self.root, '1.99.1'), 0)
        self.assertEqual(self.path.read_text(), self.original.replace('1.96.0', '1.99.1'))
        commands = [call.args[0] for call in self.run.call_args_list]
        self.assertEqual(commands, [
            ['nix', 'flake', 'update', 'rust-overlay'],
            ['rustup', 'toolchain', 'install', '1.99.1', '--profile', 'minimal', '--no-self-update', '--component', 'rustfmt', '--target', 'aarch64-linux-android'],
        ])
        for call in self.run.call_args_list:
            self.assertEqual(call.kwargs['cwd'], self.root)
            self.assertTrue(call.kwargs['check'])
        environment = self.check_output.call_args_list[0].kwargs['env']
        self.assertEqual(environment['LINCE_RUST_UPDATE_FLAKE'], 'git+' + self.root.as_uri())

    def test_nix_update_failure_keeps_existing_pin(self):
        self.run.side_effect = subprocess.CalledProcessError(1, 'nix')
        with self.assertRaises(subprocess.CalledProcessError):
            UPDATE.update(self.root, '1.99.1')
        self.assertEqual(self.path.read_text(), self.original)

    def test_single_quoted_toml_settings_are_preserved(self):
        self.path.write_text(self.original.replace('"', "'"))
        self.assertEqual(UPDATE.update(self.root, '1.99.1'), 0)
        self.assertEqual(self.path.read_text(), self.original.replace('"', "'").replace('1.96.0', '1.99.1'))

    def test_unavailable_nix_release_keeps_existing_pin(self):
        self.check_output.side_effect = subprocess.CalledProcessError(1, 'nix eval')
        with self.assertRaises(subprocess.CalledProcessError):
            UPDATE.update(self.root, '1.99.1')
        self.assertEqual(self.path.read_text(), self.original)
        self.assertEqual(self.run.call_count, 1)

    def test_rustup_failure_keeps_existing_pin(self):
        self.run.side_effect = [None, subprocess.CalledProcessError(1, 'rustup')]
        with self.assertRaises(subprocess.CalledProcessError):
            UPDATE.update(self.root, '1.99.1')
        self.assertEqual(self.path.read_text(), self.original)

    def test_wrong_resolved_compiler_keeps_existing_pin(self):
        for outputs in (['1.98.0'], ['1.99.1', 'rustc 1.98.0 (hash date)']):
            with self.subTest(outputs=outputs):
                self.check_output.side_effect = outputs
                with self.assertRaises(ValueError):
                    UPDATE.update(self.root, '1.99.1')
                self.assertEqual(self.path.read_text(), self.original)

    def test_concurrent_pin_edit_is_preserved(self):
        def run(command, **kwargs):
            if command[0] == 'rustup':
                self.path.write_text(self.original + '\n')

        self.run.side_effect = run
        with self.assertRaises(ValueError):
            UPDATE.update(self.root, '1.99.1')
        self.assertEqual(self.path.read_text(), self.original + '\n')


class RustChannelTests(unittest.TestCase):
    def test_reader_uses_shared_toml_from_any_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / 'institute/scripts/rust-channel.sh'
            script.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / 'institute/scripts/rust-channel.sh', script)
            pin = root / 'rust-toolchain.toml'
            for channel in ('1.99.1', 'stable', 'nightly', '1.99.1-beta.1'):
                pin.write_text(f"[toolchain]\nchannel = '{channel}'\n")
                result = subprocess.run(['bash', str(script)], cwd='/', capture_output=True, text=True)
                if channel == '1.99.1':
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, 'version=1.99.1\n')
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, '')


if __name__ == '__main__':
    unittest.main()
