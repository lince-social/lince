<p align=center>
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">
</p>

# Lince

Tool for registry, interconnection and automation of Needs and Contributions with open scope.

Detailed explanations of what Lince is, how to run it and use it's ecosystem can be found in the [Dark Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/main/documents/content/documentation/main-dark.pdf)/[Light Mode Documentation](https://raw.githubusercontent.com/lince-social/lince/main/documents/content/documentation/main-light.pdf).

Check the website [lince.social](https://lince.social) for more.

# Ways to install

After installing Lince, you can run it with Karma and GUI functionalities.
One can run `lince karma` as a service to have Karma always running in the background. And run `lince gui` to use it through the GUI.

### 1. Releases

Check the [releases](https://github.com/lince-social/lince/releases), pick the latest one for your operating system. Unzip it, then run:

```bash
./lince karma gui
```

### 2. Cargo install

```bash
cargo install lince
lince karma gui
```

### 3. Compiling yourself

> Beware! Here be dragons (bugs).

For those that want to compile the latest and have [cargo](https://www.rust-lang.org/tools/install) installed, run:

```bash
cargo run -- karma gui
```

Or with [mise](https://mise.jdx.dev/):

```bash
mise dev
```

### Extra

You can also use `mise` to run Lince:

```bash
mise exec cargo:lince -- lince karma gui
```

# Contributions

To contribute, check the [CONTRIBUTING.md](CONTRIBUTING.md).

# License

Check the [License](LICENSE)
