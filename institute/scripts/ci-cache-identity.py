import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def digest(value):
    return hashlib.sha256(value).hexdigest()


def main():
    scope = os.environ['CACHE_SCOPE']
    target = os.environ['CACHE_TARGET']
    if not all(re.fullmatch(r'[A-Za-z0-9_.-]+', value) for value in (scope, target)):
        raise ValueError('Invalid cache scope or target')
    native = Path(os.environ['RUNNER_TEMP']) / 'lince-native-inputs'
    configuration = {
        'compiler': subprocess.check_output(['rustc', '-vV']).decode(),
        'flags': {key: os.environ.get(key) for key in (
            'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_INCREMENTAL',
            'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_RELEASE_DEBUG', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
            'CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LINCE_CI_LINKER',
        )},
        'native': digest(native.read_bytes()),
        'files': {name: digest(Path(name).read_bytes()) for name in (
            '.cargo/config.toml', 'rust-toolchain.toml',
            '.github/actions/rust-artifacts/action.yml',
            'institute/scripts/ci-cache-identity.py',
            'institute/scripts/ci-source-cache.py',
        )},
    }
    if scope.startswith('android-'):
        configuration['files']['crates/mobile/android/app/build.gradle'] = digest(Path('crates/mobile/android/app/build.gradle').read_bytes())
    else:
        configuration['files']['institute/scripts/ci-linux-setup.sh'] = digest(Path('institute/scripts/ci-linux-setup.sh').read_bytes())
    recipe = '.github/workflows/android.yml' if scope.startswith('android-') else '.github/workflows/build.yml'
    configuration['files'][recipe] = digest(Path(recipe).read_bytes())
    names = subprocess.check_output(['git', 'ls-files', '-z', '--', 'Cargo.toml', '**/Cargo.toml', 'crates/vendor']).decode().split('\0')
    content = {name: digest(Path(name).read_bytes()) for name in names if name and Path(name).is_file()}
    compatible = digest(json.dumps(configuration, sort_keys=True).encode())
    lock = digest(Path('Cargo.lock').read_bytes())
    revision = digest(json.dumps(content, sort_keys=True).encode())
    prefix = f'rust-inputs-v1-{os.environ["RUNNER_OS"]}-{os.environ["RUNNER_ARCH"]}-{scope}-{target}-{compatible}-{lock}-'
    print(f'prefix={prefix}')
    print(f'key={prefix}{revision}')
    print('paths<<RUST_CACHE_PATHS')
    print('~/.cargo/registry/cache')
    print('~/.cargo/registry/index')
    print('~/.cargo/git/db')
    for profile in dict.fromkeys(('target/debug', 'target/release', f'target/{target}/debug', f'target/{target}/release')):
        print(f'{profile}/.fingerprint')
        print(f'{profile}/build')
        for extension in ('rlib', 'rmeta', 'so', 'a', 'o', 'd'):
            print(f'{profile}/deps/*.{extension}')
    print('target/debug/ci-source-cache.json')
    print('RUST_CACHE_PATHS')


if __name__ == '__main__':
    main()
