use std::io;

pub fn environment_ready() -> bool {
    std::env::var("LINCE_PARK_BOOTSTRAP_PID").is_ok_and(|pid| pid == std::process::id().to_string())
        && std::env::var("WAYLAND_DISPLAY")
            .is_ok_and(|name| name == format!("wayland-park-{}", std::process::id()))
}

pub fn reexec_environment() -> io::Error {
    use std::os::unix::process::CommandExt;
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return error,
    };
    std::process::Command::new(executable)
        .args(std::env::args_os().skip(1))
        .env("LINCE_PARK_BOOTSTRAP_PID", std::process::id().to_string())
        .env(
            "WAYLAND_DISPLAY",
            format!("wayland-park-{}", std::process::id()),
        )
        .env("XDG_SESSION_TYPE", "wayland")
        .env("XDG_CURRENT_DESKTOP", "Lince")
        .env("GDK_BACKEND", "wayland")
        .env("QT_QPA_PLATFORM", "wayland")
        .env_remove("DISPLAY")
        .env_remove("XAUTHORITY")
        .env_remove("WAYLAND_SOCKET")
        .exec()
}
