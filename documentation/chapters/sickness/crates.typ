== Crates

This document explains the packaging problem around publishing `lince` to crates.io and what shape the workspace should take if `cargo install lince` is meant to work.

Current code is more authoritative than this document when there is conflict.
This note is about distribution shape and release policy, not about changing the internal architecture of the app.

=== Problem

The repo is a Rust workspace, not a single-package repository.
The `lince` binary depends on several internal library crates:

- `application`
- `domain`
- `injection`
- `persistence`
- `utils`
- optionally `web`
- optionally `gui`
- optionally `tui`

That is a normal internal workspace structure, but it matters for crates.io.

If `cargo install lince` is supposed to work from crates.io, Cargo must be able to resolve every dependency of `lince` from crates.io as well.
Path-only workspace dependencies are fine for local development, but they are not enough for publishing.

That means there is no middle state where:

- `lince` is published to crates.io
- helper crates remain local-only
- `cargo install lince` still works

If the binary remains split into internal crates, those internal crates must also be publishable.

=== Why the original generic names are a problem

Originally, the internal packages used very generic package names such as:

- `application`
- `domain`
- `web`
- `utils`

That shape is bad for crates.io for two reasons:

1. These names are global package identities on crates.io, so they are likely to collide with existing or future crates.
2. Even if the names were available, publishing those packages would make them look like general-purpose public libraries, which is not the intent.

The important distinction is:

- package name: the global crates.io identity
- library crate name: the local Rust name used in `use` statements

Those do not need to be the same.

=== Recommended shape

The clean solution is:

- keep the workspace split as it is
- publish internal packages under `lince-*` names
- keep the local Rust crate names short and unchanged

Examples:

- package `lince-application`, library crate `application`
- package `lince-domain`, library crate `domain`
- package `lince-persistence`, library crate `persistence`
- package `lince-web`, library crate `web`

This preserves the current code shape while making the published identities honest and low-risk.

The intended meaning is:

- `lince` is the user-facing install target
- `lince-*` helper crates exist only because the binary is internally modular
- `lince-*` crates are implementation details, not a reusable public platform

=== What I would do

I would keep the current workspace architecture and make it explicitly publishable.

That means:

- all internal packages share the same version as `lince`
- workspace dependencies specify both `path` and an exact `=version`
- internal packages use `lince-*` package names
- internal packages keep short `[lib]` names for code imports
- release automation bumps the whole workspace together
- crates publishing happens in dependency order

The publish order should be:

- `lince-utils`
- `lince-domain`
- `lince-persistence`
- `lince-injection`
- `lince-application`
- `lince-web`
- `lince-gui`
- `lince-tui`
- `lince`

Not every crate has to be enabled by default at runtime, but if `lince` depends on it for some publishable feature set, it has to be resolvable by Cargo.

The intended feature matrix for `lince` is:

- default: `karma` + `http`
- alternative frontend: `tui`
- alternative frontend: `gui`
- optional background worker: `karma`

The frontend features are mutually exclusive:

- `http`
- `tui`
- `gui`

So valid user-facing combinations include:

- `http`
- `karma,http`
- `tui`
- `karma,tui`
- `gui`
- `karma,gui`
- `karma`

=== Why not flatten everything into one crate

There is another possible direction:

- move all internal code into the `lince` package
- stop publishing helper crates entirely

That would also solve the crates.io problem.

I would not do that right now unless there is already a desire to simplify the source tree.
It would mix two unrelated changes:

- packaging policy
- codebase architecture

The workspace split already expresses meaningful boundaries inside the project.
If those boundaries are still useful for development, testing, and reading the code, then crates.io should adapt to the architecture, not force an unnecessary flattening.

=== Release policy

There should be two separate release ideas:

- GitHub release: publish binaries and container images
- crates.io release: publish `lince` and the internal `lince-*` helper crates

They are related, but they are not the same action.

The practical release flow should be:

1. bump the workspace version
2. create the Git tag and GitHub release flow
3. publish internal `lince-*` crates in dependency order
4. publish `lince`

This keeps `cargo install lince` aligned with the versioned binary release, while still acknowledging that crates.io propagation is a separate system.

Operationally, that should map to:

- `mise release`
- `mise publish-crates`

=== What this means for users

Users should think in terms of:

- `cargo install lince`
- downloaded release binaries
- Docker image pulls

Users should not need to know or care that `lince-domain` or `lince-application` exists.
Those packages are an implementation artifact of the workspace layout.

=== What this means for maintainers

Maintainers should treat the internal crates as:

- publishable
- versioned together
- documented as internal
- not promised as stable standalone APIs

That does not require pretending they are a reusable framework.
It only requires making the dependency graph publishable.

=== Default-members note

The workspace has `default-members = ["crates/lince"]`.
That means commands run at the workspace root without `--package` may still target `lince` today.

That is convenient, but CI and release automation should still prefer explicit package selection such as:

- `cargo build --package lince`
- `cargo check --package lince`

This avoids accidental behavior changes if the workspace default ever changes later.
