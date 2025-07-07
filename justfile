install:
    cargo install mprocs mdbook

run: install
    mprocs \
    "bacon . --job clippy-all" \
    "dx serve --platform desktop" \
    "systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
    "systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat"

book: install
    mdbook serve --port 9999
