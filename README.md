<p align=center>
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/white_in_black.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/black_in_white.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/white_in_black.svg" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/dev/assets/black_in_white.svg" alt="Lince Logo">
</p>

# Lince

Tool for registry, interconnection and automation of Needs and Contributions with open scope.

Detailed explanations of what Lince is, how to run it and use it's ecosystem can be found in the [Dark Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/dev/documents/content/documentation/main-dark.pdf)/[Light Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/dev/documents/content/documentation/main-light.pdf).

Check the website [lince.social](https://lince.social) for more.

# Ways to install

After installing Lince, you can run it with Karma and GUI functionalities.
One can run `lince karma` as a service to have Karma always running in the background. And run `lince gui` to use it through the GUI.

### 1. Releases

Check the [releases](https://github.com/lince-social/lince/releases), pick the latest one for your operating system. Unzip it, then run:

```bash
./lince
```

You can run with no GUI, passing `lince --guiless`, or with no Karma, passing `lince --karmaless`. It's the same running in other ways below.

### 2. Cargo install

```bash
cargo install lince
lince
```

### 3. Compiling yourself

> Beware! Here be dragons (bugs).

For those that want to compile the latest and have [cargo](https://www.rust-lang.org/tools/install) installed, run:

```bash
cargo run
```

Or with [mise](https://mise.jdx.dev/):

```bash
mise dev
```

### Extra

You can also use `mise` to run Lince:

```bash
mise exec cargo:lince -- lince
```

# Contributions

To contribute, check the [CONTRIBUTING.md](CONTRIBUTING.md).

# License

Check the [License](LICENSE)

---
