mod bootstrap_config;
mod runtime;
#[cfg(not(target_os = "linux"))]
mod tauri_shell;

use std::io::Error;
use utils::desktop_setup::{DesktopInstallSetup, detected_language_default, write_staged_setup};

#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    tauri::mobile_entry_point
)]
pub fn run() {
    let args = std::env::args().collect::<Vec<_>>();
    #[cfg(target_os = "linux")]
    if args.iter().any(|argument| argument.starts_with("--type=")) {
        lince_interface::run_native_interface(None);
        return;
    }
    if args
        .iter()
        .any(|arg| arg == "--stage-desktop-install-setup")
    {
        if let Err(error) = stage_desktop_install_setup(&args) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .expect("failed to build the tokio runtime");

    #[cfg(target_os = "linux")]
    {
        match tokio_runtime.block_on(runtime::start_desktop_server()) {
            Ok(runtime) => lince_interface::run_native_interface(Some(runtime.url)),
            Err(error) => {
                eprintln!("Failed to start Lince desktop server: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    #[cfg(not(target_os = "linux"))]
    tauri_shell::run(tokio_runtime, args);
}

fn stage_desktop_install_setup(args: &[String]) -> Result<(), Error> {
    let auth_enabled = has_arg(args, "--auth-enabled");
    let initial_admin_password = arg_value(args, "--initial-admin-password");
    if auth_enabled
        && initial_admin_password
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(Error::other(
            "--initial-admin-password is required when --auth-enabled is set",
        ));
    }

    let setup = DesktopInstallSetup {
        start_on_login: Some(has_arg(args, "--start-on-login")),
        start_silent: Some(has_arg(args, "--start-silent")),
        language: arg_value(args, "--language").or_else(detected_language_default),
        auth_enabled: Some(auth_enabled),
        initial_admin_password,
    };

    write_staged_setup(&setup)
}

fn has_arg(args: &[String], expected: &str) -> bool {
    args.iter().any(|arg| arg == expected)
}

fn arg_value(args: &[String], expected: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == expected).then(|| window[1].clone()))
}
