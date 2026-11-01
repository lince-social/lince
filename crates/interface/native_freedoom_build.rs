pub fn compile() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let executable = output.join("lince-freedoom");
    println!("cargo:rerun-if-changed={}", executable.display());
    let directory = "src/native/freedoom/vendor/engine";
    println!("cargo:rerun-if-changed={directory}");
    println!("cargo:rerun-if-changed=src/native/freedoom/host.c");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap() != "unix" {
        if std::fs::read(&executable).ok().as_deref() != Some(&[]) {
            std::fs::write(executable, []).expect("write unavailable game host");
        }
        return;
    }
    let mut build = cc::Build::new();
    build.warnings(false);
    for flag in [
        "-O2",
        "-DNORMALUNIX",
        "-DLINUX",
        "-D_DEFAULT_SOURCE",
        "-DDOOMGENERIC_RESX=320",
        "-DDOOMGENERIC_RESY=200",
        "-Werror",
        "-I",
        directory,
    ] {
        build.flag(flag);
    }
    let sources = std::fs::read_to_string(format!("{directory}/SOURCES.txt")).unwrap();
    for source in sources.lines() {
        build.file(format!("{directory}/{source}"));
    }
    build.file("src/native/freedoom/host.c");
    let objects = build.compile_intermediates();
    let compiler = build.get_compiler();
    let mut command = compiler.to_command();
    let staged = output.join("lince-freedoom.next");
    command.args(objects).args(["-lm", "-o"]).arg(&staged);
    let result = command.output().expect("compile Freedoom host");
    assert!(
        result.status.success(),
        "Freedoom host: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&staged).expect("read compiled Freedoom host");
    if std::fs::read(&executable).ok().as_deref() == Some(&bytes) {
        std::fs::remove_file(staged).expect("remove unchanged staged Freedoom host");
    } else {
        std::fs::rename(staged, executable).expect("publish compiled Freedoom host");
    }
}
