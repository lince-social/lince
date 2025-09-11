# Radaaa

## install
```bash
cargo install mprocs mdbook cargo-edit cargo-udeps --locked
```

## update
```bash
mask install
rustup self update
rustup update stable
cargo upgrade
```

## clean
```bash
mask update
cargo fix --broken-code --allow-dirty && cargo clippy --fix --allow-dirty --quiet >/dev/null 2>&1
```

## run
```bash
mask update
mprocs \
"bacon . --job fix" \
"systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
"systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat"
```

## book
```bash
mask update
mdbook serve --port 9999
```
