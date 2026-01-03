## install
```bash
cargo install mprocs mdbook bacon cargo-edit cargo-udeps --locked
```

## update
```bash
mask install
rustup self update
rustup update stable
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
documentation/main.typ
```
> Starts typst documentation with tinymist on http://localhost:3003

## tmil 
```bash
mask install-docs

trap 'typst compile \
--root documentation \
documentation/chapters/TMIL/main.typ' EXIT

trap 'touying compile \
--root documentation \
--format html \
documentation/chapters/TMIL/main.typ' EXIT

tinymist preview \
--root documentation \
--control-plane-host 127.0.0.1:3002 \
--data-plane-host 127.0.0.1:3001 \
--static-file-host 127.0.0.1:3003 \
--font-path documentation/font/IBM_Plex_Sans/static \
--invert-colors='{"rest":"always", "image": "never"}' \
documentation/chapters/TMIL/main.typ
```
> Starts typst documentation for This Month in Lince with tinymist on http://localhost:3003

## posts
```bash
mask install-docs
find documentation/chapters/posts -name '*.json' -type f | while read -r json; do
  rel="${json#documentation/chapters/posts/}"
  dir="$(dirname "$json")"
  base="$(basename "$json" .json)"
  echo $rel
  echo $dir
  echo $base

  typst compile \
    --root ./ \
    --format png \
    --input json="$rel" \
    documentation/chapters/posts/main.typ \
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
documentation/chapters/posts/main.typ
```
> Starts typst documentation for social media posts with tinymist on http://localhost:3003
