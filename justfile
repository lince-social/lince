install:
    cargo install mprocs mdbook cargo-edit cargo-udeps --locked
update: install
    rustup self update
    rustup update
    cargo upgrade

run: update
    mprocs \
    "bacon . --job fix" \
    "systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
    "systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat"

book: update
    mdbook serve --port 9999
