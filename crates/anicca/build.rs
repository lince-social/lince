use std::path::PathBuf;

fn main() {
    sensei::teach(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/grammar.rs");
    rust_sitter_tool::build_parsers(&PathBuf::from("src/grammar.rs"));
}
