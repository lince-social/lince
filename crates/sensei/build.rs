// non-lean-yet
// Scope: Run the shared rule registry when Cargo builds Sensei.
// BeautifulRust (baseline lines):
// - [manual: §3 Ownership, §5 Panics] Sites 10–20. Read CARGO_MANIFEST_DIR as an OS path and
//   give the Cargo-provided-variable invariant an explicit diagnostic. Honor the disable
//   switch before preparing the path. Keep Cargo's output directives and message routing.
// - [manual: §2 Tools] Sites 1–7. Rustfmt wraps both scoped allow attributes. The shared
//   modules intentionally expose methods the build script does not call.

#[path = "src/rules/mod.rs"]
#[allow(
    dead_code,
    reason = "The build script uses the checker subset of the shared rules module"
)]
mod rules;

#[path = "src/lesson.rs"]
#[allow(
    dead_code,
    reason = "The build script uses teach from the shared lesson module"
)]
mod lesson;

fn main() {
    println!("cargo:rerun-if-env-changed=SENSEI");
    let manifest =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    if std::env::var("SENSEI").as_deref() == Ok("off") {
        println!(
            "cargo:warning=SENSEI=off: the lessons did not run. \
             Run `cargo run -p sensei --bin sensei -- fix` to mend, then build without it."
        );
        return;
    }
    lesson::teach(&manifest);
}
