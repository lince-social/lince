<p align=center>
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/branco_no_preto.png" alt="Lince Logo">
<img width="24%" src="https://raw.githubusercontent.com/lince-social/lince/main/assets/preto_no_branco.png" alt="Lince Logo">
</p>

<img src="https://raw.githubusercontent.com/lince-social/lince/main/assets/display.gif" alt="Lince Logo">

# Lince

Tool for registry, interconnection and automation of Needs and Contributions with open scope.

Detailed explanations of what Lince is, how to run it and use it's ecosystem can be found in the [documentation](https://raw.githubusercontent.com/lince-social/lince/main/documents/content/documentation/main.pdf).

To install, you can download the crate and run it with Karma and a GUI:

```bash
# Download
cargo install lince

# Run
lince karma gui
```

Or get the binary [here](https://github.com/lince-social/lince/releases). Pick the latest one for your machine and operating system, then unzip and execute the binary:

```bash
./lince karma gui
```

If you want to compile it, and have [cargo](https://www.rust-lang.org/tools/install) installed, run:

```bash
cargo run -- karma gui
```

> Tip: One can run `lince karma` as a service to have Karma always running in the background. And run `lince gui` to use it through the GUI.

Have fun!

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

Check the maskfile.md for more information.
