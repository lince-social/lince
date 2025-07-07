# Introduction

This is the documentation for the Lince project. If you find any errors feel free to submit a [PR](http://github.com/xaviduds/programming_manual.git).

The tool used to construct this is the `mdbook` crate, which is used by the official Rust book and many other technologies for their documentation. We can customize the color palette, search, print, etc. `mdbook` uses Markdown (.md) files, supports hot-reloading during development, and outputs static files into a specified directory, which is ignored by Git for deployment purposes.

If you don't have Cargo installed yet, you can install it with [Rustup](https://www.rust-lang.org/tools/install). It comes with Rust and its toolchain—how convenient!

To install, build, and serve the manual easily with [just](https://github.com/casey/just), try running:

```bash
just book
```

If that doesn't work, try building and running the manual manually.

To install the tool that builds the manual, run:

```bash
cargo install mdbook
```

If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`) so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
#bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

With `mdbook` installed, enter the project folder and type:

```bash
mdbook serve
```

If you want to choose the port:

```bash
mdbook serve --port 9999
```

## [MdBook Documentation](https://rust-lang.github.io/mdBook/index.html)
