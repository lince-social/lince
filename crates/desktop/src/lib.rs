mod bootstrap_config;
mod runtime;
#[cfg(not(target_os = "linux"))]
mod tauri_shell;

use std::{io::Error, net::SocketAddr, path::PathBuf};
use utils::desktop_setup::{DesktopInstallSetup, detected_language_default, write_staged_setup};

#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    tauri::mobile_entry_point
)]
pub fn run() {
    let args = std::env::args().collect::<Vec<_>>();
    #[cfg(target_os = "linux")]
    if args.iter().any(|argument| argument.starts_with("--type=")) {
        lince_interface::run_native_interface();
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

    let desktop_options = match DesktopOptions::from_args(&args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if let Some(directory) = desktop_options.directory
        && let Err(error) = utils::config::set_lince_data_dir_override(directory)
    {
        eprintln!("{error}");
        std::process::exit(1);
    }

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .expect("failed to build the tokio runtime");

    #[cfg(target_os = "linux")]
    {
        match tokio_runtime.block_on(runtime::start_desktop_server(desktop_options.listen_addr)) {
            Ok(runtime) => {
                eprintln!("Legacy Lince interface available at {}", runtime.url);
                let _runtime_guard = tokio_runtime.enter();
                lince_interface::run_native_interface();
            }
            Err(error) => {
                eprintln!("Failed to start Lince desktop server: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    #[cfg(not(target_os = "linux"))]
    tauri_shell::run(tokio_runtime, args, desktop_options.listen_addr);
}

struct DesktopOptions {
    directory: Option<PathBuf>,
    listen_addr: String,
}

impl DesktopOptions {
    fn from_args(args: &[String]) -> Result<Self, Error> {
        let directory = arg_value(args, "--directory").map(PathBuf::from);
        let listen_addr = if let Some(listen_addr) = arg_value(args, "--listen-addr") {
            validate_listen_addr(&listen_addr)?;
            listen_addr
        } else if let Some(port) = arg_value(args, "--port") {
            let port = port
                .parse::<u16>()
                .map_err(|error| Error::other(format!("Invalid port `{port}`: {error}")))?;
            format!("127.0.0.1:{port}")
        } else {
            "127.0.0.1:6174".to_string()
        };

        Ok(Self {
            directory,
            listen_addr,
        })
    }
}

fn validate_listen_addr(listen_addr: &str) -> Result<(), Error> {
    listen_addr
        .parse::<SocketAddr>()
        .map(|_| ())
        .map_err(|error| Error::other(format!("Invalid listen address `{listen_addr}`: {error}")))
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

#[cfg(test)]
mod tests {
    use super::DesktopOptions;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn desktop_options_keep_installed_defaults() {
        let options = DesktopOptions::from_args(&args(&["lince-desktop"])).unwrap();
        assert_eq!(options.directory, None);
        assert_eq!(options.listen_addr, "127.0.0.1:6174");
    }

    #[test]
    fn desktop_options_accept_an_independent_data_dir_and_port() {
        let options = DesktopOptions::from_args(&args(&[
            "lince-desktop",
            "--directory",
            "/tmp/lince-dev",
            "--port",
            "6176",
        ]))
        .unwrap();
        assert_eq!(options.directory, Some(PathBuf::from("/tmp/lince-dev")));
        assert_eq!(options.listen_addr, "127.0.0.1:6176");
    }

    #[test]
    fn desktop_options_reject_an_invalid_port() {
        let error = DesktopOptions::from_args(&args(&["lince-desktop", "--port", "not-a-port"]))
            .err()
            .unwrap();
        assert!(error.to_string().contains("Invalid port"));
    }
}
