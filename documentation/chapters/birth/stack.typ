== Tech Stack

Backend:

- Programming Language: [Rust](https://www.rust-lang.org)
- Server: [Axum](https://github.com/tokio-rs/axum)
- Database: [Sqlite](https://www.sqlite.org/) | Easiest to replicate the concept of DNA.
- Database Driver: [SQLx](https://github.com/launchbadge/sqlx) | Fun async, previously was Rusqlite but didn't prototype that well.

Frontend:
- Web:
  - Base: HTML, CSS
  - Templating (Backend): [Maud](https://github.com/lambda-fairy/maud)
  - Framework: [HTMX](https://github.com/bigskysoftware/htmx) + [Datastar](https://github.com/starfederation/datastar) (transitioning into)
- Bevy:
  - Don't know yet, still learning, probably will use WASM.

Dirty Architecture:

- Application
  - Providers: simple CRUD operations.
  - Use_cases: anything complex, business ruly, that calls providers or manipulates data.
  - Schema: structure of data that comes in or goes out through the endpoints, or an alias for a data structure as a Type so it doesnt clutter the screen.
- Domain:
  - Entities: the most accurate representation of the main drivers of the application. The source of truth of Lince's concepts.
  - Repositories: traits for defining the methods of the real Infrastructure repositories (database operations).
- Infrastructure:
  - Http:
    - Routers: the paths and arguments of endpoints.
    - Handlers: the functions that are called by endpoints, that receive the query params, path variables, header, body if wanted. Responsible for calling providers, use_cases and/or returning presentation functions (frontend).
  - Database:
    - Management: database connection, migrations and schema.
    - Repositories: implementation of Domain traits for database operations like get, update...
  - Utils: functions for quality of life, like logging.
  - Cross_cutting: dependency injection for a collection of many methods in different layers. Mostly for single database connection initiation.
- Presentation
  - Html: the templates made with maud, or sometimes just big strings. that are returned from functions. These HTML may contain HTMX and/or Datastar. The directories are divided rudimentary by concepts/entities like 'operation', 'table', 'pages'.
  - Bevy: not implemented yet, but mainly for a more featureful and immersive GUI.

== Rust
Rust is a Procedural, Compiled, Borrow Checked (Memory Safe(r)), Statically Typed, Lower Level Than Go And Higher Level Than C programming language. It offers great concurrency, async (hard) and robust unwanted behavior elimination. Has a powerful standard library but shines greatly with crates (dependencies), using cargo, it's package management system.

There are many ways to write procedures like any language. Some more idiomatic than others, with one liners, syntax sugar and macros. This manual serves as a way of helping to write good Rust procedures, outside of executing Clippy's suggestions (more info below).

You install Rust preferably through [Rustup](https://www.rust-lang.org/tools/install), which comes with the whole toolchain (you are going to need it), including:
- rustc: the Rust compiler.
- [cargo](https://crates.io/): the package manager and build tool.
- rustfmt: formats Rust code according to style guidelines.
- [clippy](https://doc.rust-lang.org/stable/clippy/index.html): a linter that provides suggestions to improve your code (micro best practices).
- rust-analyzer: a language server for IDE support and code navigation.

There are [awesome](https://github.com/rust-unofficial/awesome-rust?tab=readme-ov-file#games) things built with Rust, including:

* [Bevy](https://bevy.org/): a game engine that you use by writing 'pretty normal' Rust.
* [Helix](https://helix-editor.com/) | [Zed](https://zed.dev/): text editors/IDEs.
* [uutils](https://uutils.github.io/): rewrite of GNU's core utils.
* [Deno](https://deno.com/): a JavaScript/TypeScript runtime.
* [Rust for Linux](https://github.com/Rust-for-Linux/linux): an effort to support Linux kernel modules in Rust.
* [Dioxus](https://dioxuslabs.com/) | [Tauri](https://tauri.app/): cross-platform application builders.
* [Axum](https://docs.rs/axum) | [Actix Web](https://actix.rs/): backend frameworks.
* [Redox](https://www.redox-os.org/): an operating system.
* [Limbo](https://github.com/tursodatabase/limbo): an SQLite 'evolution'.
* [Fish](https://fishshell.com/) | [Nushell](https://www.nushell.sh/): shells.
* [Servo](https://servo.org/): a browser engine.
* [Alacritty](https://github.com/alacritty/alacritty): a GPU-accelerated terminal emulator.
* [Cosmic Desktop Environment](https://system76.com/cosmic/): a desktop environment native to Pop_OS.
* [Niri](https://github.com/YaLTeR/niri): a scrollable window manager for Wayland.
* [Maud (awesome)](https://maud.lambda.xyz/) | [Yew](https://yew.rs/) | [Leptos](https://leptos.dev/): frontend.
* [Iced](https://iced.rs/) | [egui](https://www.egui.rs/): GUI toolkits.
* [Polars](https://github.com/pola-rs/polars): DataFrames.

== Learning Resources:
1. [Rust Book](https://doc.rust-lang.org/stable/book): chapters 1-6 will give you a simple base if you just want to hack something together. Passing through the entire book, doing the examples and understanding them deeply, will set you up for great vibe coding.

2. [Rustlings](https://rustlings.rust-lang.org/): for when you are done with the book. You will complete exercises on a broad spectrum of the language's features.

3. [Rust for Rustaceans](https://rust-for-rustaceans.com/): for a deeper dive in the language, good practices and crate recomendations.

4. [This Week in Rust](https://this-week-in-rust.org/): a weekly newsletter about Rust projects, crates and language changes. Amazing for keeping up to date with the ecosystem.

5. Youtube Channels: [Jon Gjengset](https://www.youtube.com/@jonhoo/videos) | [No Boilerplate](https://www.youtube.com/@NoBoilerplate) | [The Rustagen](https://www.youtube.com/@TheVimeagen) | [Tsoding](https://www.youtube.com/@TsodingDaily) |[Code to the Moon](https://www.youtube.com/@codetothemoon) | [fasterthanlime](https://www.youtube.com/@fasterthanlime) | [chris biscardi](https://www.youtube.com/@chrisbiscardi/featured) | [Mike Code](https://www.youtube.com/@mikecode-ns7tq/videos)

6. Podcast: [Rustacean Station](https://rustacean-station.org/)

7. Discord, Matrix and many more online communities are very welcoming to newbies.

There are some crates to help your CI:

== Cargo Audit
> Audit your dependencies for crates with security vulnerabilities reported to the [RustSec Advisory Database](https://github.com/RustSec/advisory-db/).

```bash
== Install
cargo install cargo-audit --locked

== Run
cargo audit
```

== Cargo Udeps
See your unused cargo dependencies:

```bash
== Install
cargo install cargo-udeps --locked

== Run
cargo +nightly udeps
```

== Cargo Vet
> [...] tool to help projects ensure that third-party Rust dependencies have been audited by a trusted entity.
```bash
== Install
cargo install cargo-vet --locked

== Initialize a standard Vet criteria, this can be changed
cargo vet init

== Run
cargo vet
```
