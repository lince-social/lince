import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess


def digest(value):
    return hashlib.sha256(value).hexdigest()


def source_identity():
    # Use the same content/mode identities as timestamp restoration. Checkout
    # times must not affect keys, but every compiled source/asset change must.
    spec = importlib.util.spec_from_file_location('source_cache', Path(__file__).with_name('ci-source-cache.py'))
    source_cache = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(source_cache)
    entries = source_cache.snapshot(Path.cwd().resolve())
    return digest(json.dumps({name: entry['identity'] for name, entry in entries.items()}, sort_keys=True).encode())


def main():
    scope = os.environ['CACHE_SCOPE']
    target = os.environ['CACHE_TARGET']
    if not all(re.fullmatch(r'[A-Za-z0-9_.-]+', value) for value in (scope, target)):
        raise ValueError('Invalid cache scope or target')
    native = Path(os.environ['RUNNER_TEMP']) / 'lince-native-inputs'
    configuration = {
        'compiler': subprocess.check_output(['rustc', '-vV']).decode(),
        'flags': {key: os.environ.get(key) for key in set((
            'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_INCREMENTAL',
            'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
            'CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LINCE_CI_LINKER',
        )) | {key for key in os.environ if key.startswith('CARGO_PROFILE_')}},
        'native': digest(native.read_bytes()),
        'files': {name: digest(Path(name).read_bytes()) for name in (
            '.cargo/config.toml', 'rust-toolchain.toml',
            'institute/scripts/ci-cache-identity.py',
            'institute/scripts/ci-source-cache.py',
        )},
    }
    if scope.startswith('android-'):
        configuration['files']['crates/mobile/android/app/build.gradle'] = digest(Path('crates/mobile/android/app/build.gradle').read_bytes())
    else:
        configuration['files']['institute/scripts/ci-linux-setup.sh'] = digest(Path('institute/scripts/ci-linux-setup.sh').read_bytes())
    compatible = digest(json.dumps(configuration, sort_keys=True).encode())
    lock = digest(Path('Cargo.lock').read_bytes())
    revision = source_identity()
    prefix = f'rust-inputs-v2-{os.environ["RUNNER_OS"]}-{os.environ["RUNNER_ARCH"]}-{scope}-{target}-{compatible}-'
    print(f'prefix={prefix}')
    print(f'key={prefix}{lock}-{revision}')
    # Prefer progress from this source (including interrupted build snapshots),
    # then the same dependency graph, then unchanged packages across lockfile
    # updates. Compiler, native ABI and flags stay isolated.
    print('restore-keys<<RUST_CACHE_RESTORE_KEYS')
    print(f'{prefix}{lock}-{revision}')
    print(f'{prefix}{lock}-')
    print(prefix)
    print('RUST_CACHE_RESTORE_KEYS')
    print('paths<<RUST_CACHE_PATHS')
    print('~/.cargo/registry/cache')
    print('~/.cargo/registry/index')
    print('~/.cargo/git/db')
    for profile in dict.fromkeys(('target/debug', 'target/release', f'target/{target}/debug', f'target/{target}/release')):
        print(f'{profile}/.fingerprint')
        print(f'{profile}/build')
        for extension in ('rlib', 'rmeta', 'so', 'a', 'o', 'd'):
            print(f'{profile}/deps/*.{extension}')
        # These are compiled inputs, not test results. Cargo checks freshness
        # and CI still executes every required gate after restoring them.
        if scope == 'linux-validation':
            for name in ('lince_interface', 'lince_park'):
                print(f'{profile}/deps/{name}-*')
    print('target/debug/ci-source-cache.json')
    print('RUST_CACHE_PATHS')


if __name__ == '__main__':
    main()
