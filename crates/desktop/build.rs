fn main() {
    #[cfg(not(target_os = "linux"))]
    tauri_build::build();
}
