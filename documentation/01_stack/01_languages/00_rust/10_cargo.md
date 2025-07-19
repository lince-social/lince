There are some crates to help your CI:

#### Cargo Audit
> Audit your dependencies for crates with security vulnerabilities reported to the [RustSec Advisory Database](https://github.com/RustSec/advisory-db/).

```bash
# Install
cargo install cargo-audit --locked

# Run
cargo audit
```

#### Cargo Udeps
See your unused cargo dependencies:

```bash
# Install
cargo install cargo-udeps --locked

# Run
cargo +nightly udeps
```

#### Cargo Vet
> [...] tool to help projects ensure that third-party Rust dependencies have been audited by a trusted entity.
```bash
# Install
cargo install cargo-vet --locked

# Initialize a standard Vet criteria, this can be changed
cargo vet init

# Run
cargo vet
```
