procs:
    # cargo install mprocs --locked
    mprocs \
    "bacon . --job clippy-all" \
    "dx serve --platform desktop" \
    "systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
    "systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat"

install-mdbook:
    # cargo install mdbook --locked

book: install-mdbook
    mdbook serve --port 9999
