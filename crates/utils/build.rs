// non-lean-yet
// Scope: fn main.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   2.

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
}
