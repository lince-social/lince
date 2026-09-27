pub fn compile() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let executable = output.join("lince-freedoom");
    let directory = "src/freedoom/vendor/engine";
    println!("cargo:rerun-if-changed={directory}");
    println!("cargo:rerun-if-changed=src/freedoom/host.c");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap() != "unix" {
        std::fs::write(executable, []).expect("write unavailable game host");
        return;
    }
    let compiler = cc::Build::new().warnings(false).get_compiler();
    let mut command = compiler.to_command();
    command.args([
        "-O2",
        "-DNORMALUNIX",
        "-DLINUX",
        "-D_DEFAULT_SOURCE",
        "-DDOOMGENERIC_RESX=320",
        "-DDOOMGENERIC_RESY=200",
        "-Werror",
        "-I",
        directory,
    ]);
    let sources = std::fs::read_to_string(format!("{directory}/SOURCES.txt")).unwrap();
    for source in sources.lines() {
        command.arg(format!("{directory}/{source}"));
    }
    command
        .args(["src/freedoom/host.c", "-lm", "-o"])
        .arg(executable);
    let result = command.output().expect("compile Freedoom host");
    assert!(
        result.status.success(),
        "Freedoom host: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
