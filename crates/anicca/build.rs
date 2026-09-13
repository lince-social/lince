use std::path::PathBuf;

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=src/grammar.rs");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    for grammar in rust_sitter_tool::generate_grammars(&PathBuf::from("src/grammar.rs")) {
        let name = grammar["name"].as_str().expect("grammar name").to_string();
        let directory = output.join(&name);
        let grammar = grammar.to_string();
        let inputs = [
            include_str!("build.rs"),
            include_str!("../../Cargo.lock"),
            &grammar,
        ]
        .join("\0");
        let stamp = directory.join("parser.inputs");
        if std::fs::read_to_string(&stamp).ok().as_deref() != Some(&inputs)
            || !directory.join("parser.c").is_file()
            || !directory.join("tree_sitter/parser.h").is_file()
        {
            if stamp.exists() {
                std::fs::remove_file(&stamp).expect("invalidate parser inputs");
            }
            let mut diagnostics = Vec::new();
            let (generated_name, source) = tree_sitter_generate::generate_parser_for_grammar(
                &grammar,
                Some((0, 25, 2)),
                tree_sitter_generate::OptLevel::default(),
                &mut diagnostics,
            )
            .expect("generate Lingua parser");
            assert_eq!(generated_name, name);
            assert!(
                diagnostics.is_empty(),
                "parser diagnostics: {diagnostics:?}"
            );
            std::fs::create_dir_all(directory.join("tree_sitter"))
                .expect("create parser directory");
            std::fs::write(directory.join("parser.c"), source).expect("write parser");
            std::fs::write(
                directory.join("tree_sitter/parser.h"),
                tree_sitter_generate::PARSER_HEADER,
            )
            .expect("write parser header");
            std::fs::write(&stamp, &inputs).expect("write parser inputs");
        }
        let archive = output.join(format!("lib{name}.a"));
        let msvc_archive = output.join(format!("{name}.lib"));
        let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        for path in [
            directory.join("parser.c"),
            directory.join("tree_sitter/parser.h"),
            archive.clone(),
        ] {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        if msvc {
            println!("cargo:rerun-if-changed={}", msvc_archive.display());
        }
        let mut build = cc::Build::new();
        build
            .std("c11")
            .opt_level(2)
            .warnings_into_errors(true)
            .flag_if_supported("-Wno-unused-label")
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-unused-but-set-variable")
            .flag_if_supported("-Wno-trigraphs")
            .include(&directory)
            .file(directory.join("parser.c"));
        let compiler = build.get_compiler();
        let compilation = format!(
            "{inputs}\0{:?}\0{:?}",
            compiler.to_command(),
            build.get_archiver(),
        );
        let compiled_stamp = directory.join("compiled.inputs");
        if std::fs::read_to_string(&compiled_stamp).ok().as_deref() == Some(&compilation)
            && archive.is_file()
            && (!msvc || msvc_archive.is_file())
        {
            println!("cargo:rustc-link-lib=static={name}");
            println!("cargo:rustc-link-search=native={}", output.display());
        } else {
            if compiled_stamp.exists() {
                std::fs::remove_file(&compiled_stamp).expect("invalidate compiler inputs");
            }
            build.compile(&name);
            std::fs::write(compiled_stamp, compilation).expect("write compiler inputs");
        }
    }
}
