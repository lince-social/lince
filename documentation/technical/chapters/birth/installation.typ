== Installation

You can run Lince as an executable, here are the #link("https://github.com/lince-social/lince/tags")[releases].
Pick the latest one for your machine and operating system. After the download, unzip it and run the binary:

```bash
./lince
```

Or you can install from the crates.io repository with:
```bash
cargo install lince
```

If you prefer to compile the source code, the easiest way is to download the repo and run:

```bash
cargo run
```

If you want local authentication enabled during setup, create or edit `~/.config/lince/lince.toml` and set:

```toml
[auth]
enabled = true
```

When auth is enabled, Lince will require an admin user to exist. If none exists yet, run `lince` once in an interactive terminal so it can prompt you to create the initial admin account.

If you don't have Cargo installed, you can install it with #link("https://www.rust-lang.org/tools/install")[Rustup].
It comes with Rust and its toolchain. If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`)
so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

This uses #link("https://typst.app/")[Typst] to generate the documentation.
One way of running the project is with Mise, the command runner that installs the programs you need to run. To get Mise, run:
```bash
curl https://mise.run | sh
```
Then run `mise docs` for documentation.
