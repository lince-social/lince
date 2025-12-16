== Installation

The easiest way is

You can run Lince as an executable, here are the [releases](https://github.com/lince-social/lince/tags).
Pick the latest one for your machine and operating system. After the download, unzip it and run the binary:

```bash
./lince
```

If you don't have Cargo installed yet, you can install it with [Rustup](https://www.rust-lang.org/tools/install). It comes with Rust and its toolchain.

If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`)
so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

If you preffer to compile the source code, the easiest way is to download the repo and run:

```bash
cargo run
```

If you don't have cargo installed, you can install it with with [Rustup](https://www.rust-lang.org/tools/install).

This uses [Typst](https://typst.app/) to generate the documentation.
I reccomend installing [mask](https://github.com/jacobdeichert/mask) with `cargo install mask`, then running `mask docs`.
If you are in Arch or MacOS systems it will automatically install typst and [tinymist](https://github.com/Myriad-Dreamin/tinymist) (web server),
else it will probably fail, learn how to install typst and tinymist manually.
