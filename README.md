<p align=center>
<img width=24% src="assets/preto_no_branco.png">
<img width=24% src="assets/branco_no_preto.png">
<img width=24% src="assets/preto_no_branco.png">
<img width=24% src="assets/branco_no_preto.png">
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
"systemctl --user restart lince.service &&  journalctl --user -u lince.service -f --output=cat" \
"systemctl --user stop lince.service &&  journalctl --user -u lince.service -f --output=cat" \
"bacon --job featureful"
```

## book
```bash
mask update
mdbook serve --port 9999
```
