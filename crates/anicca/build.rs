use std::path::PathBuf;

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/grammar.rs");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    for grammar in rust_sitter_tool::generate_grammars(&PathBuf::from("src/grammar.rs")) {
        let mut diagnostics = Vec::new();
        let (name, source) = tree_sitter_generate::generate_parser_for_grammar(
            &grammar.to_string(),
            Some((0, 25, 2)),
            tree_sitter_generate::OptLevel::default(),
            &mut diagnostics,
        )
        .expect("generate Lingua parser");
        assert!(
            diagnostics.is_empty(),
            "parser diagnostics: {diagnostics:?}"
        );
        let directory = output.join(&name);
        std::fs::create_dir_all(directory.join("tree_sitter")).expect("create parser directory");
        std::fs::write(directory.join("parser.c"), source).expect("write parser");
        std::fs::write(
            directory.join("tree_sitter/parser.h"),
            tree_sitter_generate::PARSER_HEADER,
        )
        .expect("write parser header");
        cc::Build::new()
            .std("c11")
            .opt_level(2)
            .warnings_into_errors(true)
            .flag_if_supported("-Wno-unused-label")
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-unused-but-set-variable")
            .flag_if_supported("-Wno-trigraphs")
            .include(&directory)
            .file(directory.join("parser.c"))
            .compile(&name);
    }
}
