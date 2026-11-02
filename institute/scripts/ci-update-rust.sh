#!/usr/bin/env bash
set -euo pipefail
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
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git add rust-toolchain.toml flake.lock
git commit -m "build: update shared stable Rust toolchain"
candidate=$(git rev-parse HEAD)
branch="automation/rust-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"
linux_run=
android_run=
cleanup() {
  for run in "$linux_run" "$android_run"; do
    if [[ -n "$run" ]]; then
      gh run cancel "$run" >/dev/null 2>&1 || true
    fi
  done
  git push origin --delete "$branch" >/dev/null 2>&1 || true
}
git push origin "HEAD:refs/heads/$branch"
trap cleanup EXIT
gh workflow run build.yml --ref "$branch" -f build_release=false -f publish_release=false
gh workflow run android.yml --ref "$branch" -f build_packages=false -f abi=all -f signed_release=false -f play_bundle=false
find_run() {
  local workflow=$1 found attempt
  for ((attempt = 0; attempt < 30; attempt++)); do
    found=$(gh run list --workflow "$workflow" --branch "$branch" --event workflow_dispatch --json databaseId,headSha --jq ".[] | select(.headSha == \"$candidate\") | .databaseId" --limit 1)
    if [[ -n "$found" ]]; then
      printf '%s\n' "$found"
      return 0
    fi
    sleep 5
  done
  printf 'No validation run appeared for %s\n' "$workflow" >&2
  return 1
}
linux_run=$(find_run build.yml)
android_run=$(find_run android.yml)
gh run watch "$linux_run" --interval 30 --exit-status
gh run watch "$android_run" --interval 30 --exit-status
linux_verified=$(gh run view "$linux_run" --json headSha,jobs --jq ".headSha == \"$candidate\" and ([.jobs[] | select(.name == \"Linux compile checks\" and .conclusion == \"success\") | .steps[] | select(.name == \"Check Lince, Park and development tools\" and .conclusion == \"success\")] | length) == 1")
android_verified=$(gh run view "$android_run" --json headSha,jobs --jq ".headSha == \"$candidate\" and ([.jobs[] | select(.conclusion == \"success\") | select(any(.steps[]; .name == \"Check Android library\" and .conclusion == \"success\")) | .name] | sort) == [\"Android (arm64-v8a)\", \"Android (x86_64)\"]")
if [[ "$linux_verified" != true || "$android_verified" != true ]]; then
  printf '%s\n' 'Required compilation jobs did not run successfully on the candidate commit.' >&2
  exit 1
fi
git fetch origin main
if [[ $(git rev-parse FETCH_HEAD) != "$baseline" ]]; then
  printf '%s\n' 'Main changed during validation; leave it intact and retry on the next scheduled run.' >&2
  exit 1
fi
git push origin 'HEAD:refs/heads/main'
