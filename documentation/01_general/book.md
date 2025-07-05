# Programming Manual

This is a Programming Manual. It contains guides and explanations for programming in certain architectures, languages, and frameworks, as well as best practices and code examples.

We use the `mdbook` crate, which is used by the official Rust book and many other technologies for documentation. We can customize the color palette, search, print, etc. `mdbook` uses Markdown (.md) files, supports hot-reloading during development, and outputs static files into the `./book` directory, which is ignored by Git for deployment purposes.

If you don't have Cargo installed yet, you can install it with [Rustup](https://www.rust-lang.org/tools/install). It comes with Rust and its toolchain—how convenient!

To install, build, and serve the manual easily, try running:

```bash
make dev
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

# [MdBook Documentation](https://rust-lang.github.io/mdBook/index.html)

You can create a structure in `src/SUMMARY.md` for chapters with titles and fake folders. The level of tabs or "nests" is reproduced in the book.

```markdown
- [Languages](./03_languages/00_logic.md)
  - [TypeScript](./03_languages/01_typescript/00_intro.md)
    - [Basics](./03_languages/01_typescript/01_basics.md)
```

These levels of nesting generate the files automatically. You can also create files manually and reference their paths (but that’s more work). The end result looks like this:

![](./assets/nesting.png)
