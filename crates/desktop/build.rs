fn main() {
    sensei::teach(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    #[cfg(not(target_os = "linux"))]
    tauri_build::build();
}
