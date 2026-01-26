## install

```bash
cargo install mprocs bacon git-cliff cargo-edit cargo-udeps --locked
```

## update

```bash
rustup self update
rustup update nightly
cargo upgrade
```

## off

```bash
mask update
cargo fix --broken-code --allow-dirty && cargo clippy --fix --allow-dirty --quiet >/dev/null 2>&1
```

## run

```bash
mprocs \
"bacon . --job fix" \
"bacon . --job run"
```

## dev

```bash
mprocs \
"systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
"systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat" \
"bacon . --job fix" \
"cargo run -- html gpui"
```

## install-docs

```bash
cmd() { command -v "$1" >/dev/null; }

if ! cmd tinymist || ! cmd typst; then
  read -p "Docs will run in http://localhost:23625. Write the package manager that will install typst and tinymist [brew/pacman]: " c
  case $c in
    brew) brew install tinymist typst ;;
    pacman) sudo pacman -Syu --needed --noconfirm typst tinymist ;;
    *) exit 1 ;;
  esac
fi
```

## docs

```bash
mask install-docs
tinymist preview \
--control-plane-host 127.0.0.1:3002 \
--data-plane-host 127.0.0.1:3001 \
--static-file-host 127.0.0.1:3003 \
--font-path documentation/font/IBM_Plex_Sans/static \
--invert-colors='{"rest":"always", "image": "never"}' \
documents/content/documentation/main.typ
```

> Starts typst documentation with tinymist on http://localhost:3003

## tmil

```bash
SEARCH_PATH="documents/content/this_month_in_lince"
LATEST_DIR=$(ls -d $SEARCH_PATH/20*/ | sort -V | tail -n 1 | xargs basename)

echo "🚀 Using latest data from: $LATEST_DIR"

# 2. Pass the directory to the EXIT traps
trap "typst compile \
  --root ./ \
  --input dir=$LATEST_DIR \
  $SEARCH_PATH/main.typ" EXIT

trap "touying compile \
  --root ./ \
  --format html \
  --input dir=$LATEST_DIR \
  $SEARCH_PATH/main.typ" EXIT

# 3. Start the preview with the dynamic input
tinymist preview \
  --root ./ \
  --control-plane-host 127.0.0.1:3002 \
  --data-plane-host 127.0.0.1:3001 \
  --static-file-host 127.0.0.1:3003 \
  --input dir=$LATEST_DIR \
  $SEARCH_PATH/main.typ
```

> Starts typst documentation for This Month in Lince with tinymist on http://localhost:3003

## posts

```bash
mask install-docs
find documents/content/social_media_posts -name '*.json' -type f | while read -r json; do
  rel="${json#documents/content/social_media_posts/}"
  dir="$(dirname "$json")"
  base="$(basename "$json" .json)"
  echo $rel
  echo $dir
  echo $base

  typst compile \
    --root ./ \
    --format png \
    --input json="$rel" \
    documents/content/social_media_posts/main.typ \
    "$dir/${base}-{0p}.png"
done
```

> Creates the PNGs for all the posts. It's in .gitignore, dont worry.

## post

```bash
tinymist preview \
--root ./ \
--control-plane-host 127.0.0.1:3002 \
--data-plane-host 127.0.0.1:3001 \
--static-file-host 127.0.0.1:3003 \
--font-path documentation/font/IBM_Plex_Sans/static \
--invert-colors='{"rest":"always", "image": "never"}' \
documents/content/social_media_posts/main.typ
```

> Starts typst documentation for social media posts with tinymist on http://localhost:3003

## release

```bash
#!/usr/bin/env bash
set -euo pipefail

branch=$(git rev-parse --abbrev-ref HEAD)

if [[ ! -f "cliff.toml" ]]; then
    echo "⚙️  Initializing git-cliff config..."
    git cliff --init
fi

if git describe --tags --abbrev=0 &>/dev/null; then
    last_tag=$(git describe --tags --abbrev=0)
else
    last_tag=""
fi
echo "Last tag: ${last_tag:-<none>}"

if [[ ! -f CHANGELOG.md ]]; then
    echo "📝 Creating initial CHANGELOG.md..."
    touch CHANGELOG.md
fi

if [[ -n "$last_tag" ]]; then
    NEXT_VERSION=$(git cliff --bumped-version)
else
    NEXT_VERSION="0.1.0"
fi

read -rp "Next version (auto: $NEXT_VERSION): " input_version
VERSION=${input_version:-$NEXT_VERSION}

if [[ "$VERSION" != v* ]]; then
  VERSION="v$VERSION"
fi

read -rp "Release title (optional, press enter to use '$VERSION'): " input_title
TITLE=${input_title:-"Release $VERSION"}

git cliff --unreleased --bump --tag "$VERSION" -o CHANGELOG.md

git add CHANGELOG.md
git commit -m "chore(release): $VERSION"
git tag -a "$VERSION" -m "$TITLE"
git push origin "$branch"
git push origin "$VERSION"

if command -v gh &>/dev/null; then
    echo "📦 Creating GitHub release for $VERSION..."
    gh release create "$VERSION" -F CHANGELOG.md --title "$TITLE"
    echo "✅ GitHub release created."
else
    echo "⚠️  'gh' CLI not found. Install it with:"
    echo "    sudo pacman -S github-cli && gh auth login"
    echo "Then rerun 'mask release' to auto-publish the release."
fi

echo "✅ Released $VERSION from $branch"
```
