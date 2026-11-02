#!/usr/bin/env bash
set -euo pipefail
if [[ ${GITHUB_ACTIONS:-} != true || ${GITHUB_EVENT_NAME:-} != schedule || ${GITHUB_REF:-} != refs/heads/main ]]; then
  printf '%s\n' 'Automatic Rust updates require the scheduled main workflow.' >&2
  exit 1
fi
: "${GITHUB_REPOSITORY:?Missing repository}"
baseline=$(git rev-parse HEAD)
python3 -B institute/scripts/update-rust-toolchain.py
if git diff --quiet -- rust-toolchain.toml flake.lock; then
  exit 0
fi
changed=$(git diff --name-only | sort)
if [[ "$changed" != rust-toolchain.toml && "$changed" != $'flake.lock\nrust-toolchain.toml' ]]; then
  printf '%s\n' 'Refusing a Rust update that changes files outside the shared toolchain pins.' >&2
  exit 1
fi
version=$(bash institute/scripts/rust-channel.sh)
version=${version#version=}
linux_run=$(gh api --method POST -H 'X-GitHub-Api-Version: 2022-11-28' "repos/$GITHUB_REPOSITORY/actions/workflows/build.yml/dispatches" -f ref=main -F return_run_details=true -f 'inputs[build_release]=false' -f 'inputs[publish_release]=false' -f "inputs[rust_version]=$version" --jq '.workflow_run_id')
android_run=$(gh api --method POST -H 'X-GitHub-Api-Version: 2022-11-28' "repos/$GITHUB_REPOSITORY/actions/workflows/android.yml/dispatches" -f ref=main -F return_run_details=true -f 'inputs[build_packages]=false' -f 'inputs[abi]=all' -f 'inputs[signed_release]=false' -f 'inputs[play_bundle]=false' -f "inputs[rust_version]=$version" --jq '.workflow_run_id')
if [[ ! "$linux_run" =~ ^[0-9]+$ || ! "$android_run" =~ ^[0-9]+$ ]]; then
  printf '%s\n' 'GitHub did not return both validation run IDs.' >&2
  exit 1
fi
gh run watch "$linux_run" --interval 30 --exit-status
gh run watch "$android_run" --interval 30 --exit-status
linux_verified=$(gh run view "$linux_run" --json headSha,jobs --jq ".headSha == \"$baseline\" and ([.jobs[] | select(.name == \"Linux compile checks\" and .conclusion == \"success\") | .steps[] | select(.name == \"Check Lince, Park and development tools\" and .conclusion == \"success\")] | length) == 1")
android_verified=$(gh run view "$android_run" --json headSha,jobs --jq ".headSha == \"$baseline\" and ([.jobs[] | select(.conclusion == \"success\") | select(any(.steps[]; .name == \"Check Android library\" and .conclusion == \"success\")) | .name] | sort) == [\"Android (arm64-v8a)\", \"Android (x86_64)\"]")
if [[ "$linux_verified" != true || "$android_verified" != true ]]; then
  printf '%s\n' 'Required compilation jobs did not run successfully on the candidate commit.' >&2
  exit 1
fi
git fetch origin main
if [[ $(git rev-parse FETCH_HEAD) != "$baseline" ]]; then
  printf '%s\n' 'Main changed during validation; leave it intact and retry on the next scheduled run.' >&2
  exit 1
fi
changed=$(git diff --name-only | sort)
if [[ "$changed" != rust-toolchain.toml && "$changed" != $'flake.lock\nrust-toolchain.toml' ]]; then
  printf '%s\n' 'Unexpected tracked changes appeared during compiler validation.' >&2
  exit 1
fi
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git add rust-toolchain.toml flake.lock
git commit -m "build: update shared stable Rust toolchain"
git push origin 'HEAD:refs/heads/main'
