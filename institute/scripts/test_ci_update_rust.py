import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class AutomaticRustUpdateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.origin = self.base / 'origin.git'
        self.work = self.base / 'work'
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        subprocess.run(['git', 'init', '--bare', '-q', str(self.origin)], check=True)
        subprocess.run(['git', 'init', '-q', '-b', 'main', str(self.work)], check=True)
        self.git('config', 'user.name', 'Build validation')
        self.git('config', 'user.email', 'build@localhost')
        (self.work / 'rust-toolchain.toml').write_text('[toolchain]\nchannel = "1.98.0"\n')
        (self.work / 'flake.lock').write_text('old metadata\n')
        (self.work / 'product.rs').write_text('existing feature\n')
        self.git('add', '.')
        self.git('commit', '--no-gpg-sign', '-qm', 'Baseline')
        self.baseline = self.git('rev-parse', 'HEAD')
        self.git('remote', 'add', 'origin', str(self.origin))
        self.git('push', '-q', 'origin', 'main')
        self.environment = dict(os.environ, PATH=f'{self.bin}:{os.environ["PATH"]}', GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='1', FIXTURE_ORIGIN=str(self.origin))
        self.executable('python3', '''#!/usr/bin/env bash
set -eu
if [[ ${FIXTURE_NO_UPDATE:-0} == 1 ]]; then exit 0; fi
printf '[toolchain]\\nchannel = "1.99.1"\\n' > rust-toolchain.toml
printf 'new metadata\\n' > flake.lock
if [[ ${FIXTURE_EXTRA_EDIT:-0} == 1 ]]; then printf changed > product.rs; fi
''')
        self.executable('gh', '''#!/usr/bin/env bash
set -eu
if [[ $1 == run && $2 == list ]]; then
  if [[ $* == *build.yml* ]]; then echo 101; else echo 102; fi
fi
if [[ $1 == run && $2 == watch ]]; then
  if [[ ${FIXTURE_CI_FAIL:-0} == "$3" ]]; then exit 1; fi
  if [[ -n ${FIXTURE_MAIN_MOVED:-} ]]; then git --git-dir="$FIXTURE_ORIGIN" update-ref refs/heads/main "$FIXTURE_MAIN_MOVED"; fi
fi
if [[ $1 == run && $2 == view ]]; then
  if [[ ${FIXTURE_SKIP_CHECKS:-0} == 1 ]]; then echo false; else echo true; fi
fi
''')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.work, text=True, stderr=subprocess.DEVNULL).strip()

    def executable(self, name, contents):
        path = self.bin / name
        path.write_text(contents)
        path.chmod(0o755)

    def run_update(self, **environment):
        return subprocess.run(['bash', str(ROOT / 'institute/scripts/ci-update-rust.sh')], cwd=self.work, env=dict(self.environment, **environment), capture_output=True, text=True)

    def main_revision(self):
        return subprocess.check_output(['git', '--git-dir', str(self.origin), 'rev-parse', 'main'], text=True).strip()

    def assert_branch_removed(self):
        refs = subprocess.check_output(['git', '--git-dir', str(self.origin), 'for-each-ref', '--format=%(refname)'], text=True)
        self.assertNotIn('automation/rust-', refs)

    def test_updates_main_after_both_compilation_workflows_pass(self):
        result = self.run_update()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(self.main_revision(), self.baseline)
        self.assertEqual(self.git('diff', '--name-only', self.baseline, self.main_revision()), 'flake.lock\nrust-toolchain.toml')
        self.assert_branch_removed()

    def test_failed_checks_preserve_main_and_remove_temporary_branch(self):
        result = self.run_update(FIXTURE_CI_FAIL='101')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.main_revision(), self.baseline)
        self.assert_branch_removed()

    def test_failed_android_checks_preserve_main(self):
        result = self.run_update(FIXTURE_CI_FAIL='102')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.main_revision(), self.baseline)
        self.assert_branch_removed()

    def test_concurrent_main_update_is_preserved(self):
        (self.work / 'product.rs').write_text('new feature\n')
        self.git('add', '.')
        self.git('commit', '--no-gpg-sign', '-qm', 'Concurrent feature')
        other = self.git('rev-parse', 'HEAD')
        self.git('push', '-q', 'origin', 'HEAD:concurrent-fixture')
        self.git('reset', '--hard', self.baseline)
        result = self.run_update(FIXTURE_MAIN_MOVED=other)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.main_revision(), other)
        self.assert_branch_removed()

    def test_unexpected_source_edit_never_reaches_main(self):
        result = self.run_update(FIXTURE_EXTRA_EDIT='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.main_revision(), self.baseline)

    def test_already_current_update_creates_no_commit_or_branch(self):
        result = self.run_update(FIXTURE_NO_UPDATE='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.baseline)
        self.assertEqual(self.main_revision(), self.baseline)
        self.assert_branch_removed()

    def test_skipped_compilation_jobs_never_advance_main(self):
        result = self.run_update(FIXTURE_SKIP_CHECKS='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.main_revision(), self.baseline)
        self.assert_branch_removed()


if __name__ == '__main__':
    unittest.main()
