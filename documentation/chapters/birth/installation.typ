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

If you don't have Cargo installed, you can install it with #link("https://www.rust-lang.org/tools/install")[Rustup].
It comes with Rust and its toolchain. If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`)
so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

This uses #link("https://typst.app/")[Typst] to generate the documentation.
I recommend installing #link("https://github.com/jacobdeichert/mask")[mask] with `cargo install mask`, then running `mask docs`.
If you are in Arch or MacOS systems it will automatically install typst and #link("https://github.com/Myriad-Dreamin/tinymist")[tinymist] (web server),
else it will probably not work automatically.
