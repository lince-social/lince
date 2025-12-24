<p align=center>
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">

</p>

# Lince
Tool for registry, interconnection and automation of Needs and Contributions with open scope.

Detailed explanations of what Lince is, how to run it and use it's ecosystem can be found in the [documentation](https://lince-social.github.io/book/).

To install, you can download the crate and run it:
```bash
# Download
cargo install lince

# Run
lince
```

Or get the binary [here](https://github.com/lince-social/lince/tags). Pick the latest one for your machine and operating system, then unzip and execute the binary:

```bash
./lince
```

If you want to compile it, and have [cargo](https://www.rust-lang.org/tools/install) installed, run:

```bash
cargo run
```

Both methods should allow you to start using it at [http://localhost:6174](http://localhost:6174). I recommend using Lince as a linux service. Have fun.

---

# Disclamer

This project is licensed under the GNU GPLv3 license. Crowdfunding is the source of development compensation:

[GitHub Sponsors](https://github.com/sponsors/lince-social) | [Patreon](https://www.patreon.com/lince_social) | [Apoia.se](https://www.apoia.se/lince)

Lince tries to facilitate and automate the connection between people and resources, by transforming needs and contributions into data. The gains and losses related to the interaction, such as transportation, production and services themselves, remain the responsibility of the parties involved.

# Dev Commands

Using mask:
```bash
cargo install mask
```

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
# mask install-docs

# trap '
typst compile \
--root documentation \
--input json=0001_lince_overview.json \
documentation/chapters/posts/main.typ
# ' EXIT

# trap 'touying compile \
# --root documentation \
# --format html \
# documentation/chapters/posts/main.typ' EXIT

# tinymist preview \
# --root documentation \
# --control-plane-host 127.0.0.1:3002 \
# --data-plane-host 127.0.0.1:3001 \
# --static-file-host 127.0.0.1:3003 \
# --font-path documentation/font/IBM_Plex_Sans/static \
# --invert-colors='{"rest":"always", "image": "never"}' \
# documentation/chapters/posts/main.typ
```
> Starts typst documentation for social media posts with tinymist on http://localhost:3003
