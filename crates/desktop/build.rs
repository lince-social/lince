fn main() {
    sensei::teach(env!("CARGO_MANIFEST_DIR"));

    #[cfg(not(target_os = "linux"))]
    tauri_build::build();
}
