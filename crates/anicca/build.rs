use std::path::PathBuf;

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/grammar.rs");
    rust_sitter_tool::build_parsers(&PathBuf::from("src/grammar.rs"));
}
