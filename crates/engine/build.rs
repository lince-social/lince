use std::path::Path;

fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("../../anicca");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("anicca/ must exist ({}): {error}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("lingua"))
        .collect();
    files.sort();

    let mut output = String::from("pub static BUNDLE: &[(&str, &str)] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("utf-8 filename");
        let text = std::fs::read_to_string(path).expect("readable");
        output.push_str(&format!("    ({name:?}, {text:?}),\n"));
    }
    output.push_str("];\n");

    let destination = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("bundle.rs");
    std::fs::write(destination, output).expect("write bundle.rs");
}
