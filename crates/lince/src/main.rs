#![forbid(unsafe_code)]

mod admin;
mod bootstrap_config;

use std::{env, io::Error, net::SocketAddr, path::PathBuf};

use utils::desktop_setup::{DesktopInstallSetup, read_staged_setup, remove_staged_setup};
use utils::logging::set_quiet;
use web::{HttpServeMode, serve_cell_api_only};

fn main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .expect("failed to build the tokio runtime")
        .block_on(async_main())
}

async fn async_main() -> Result<(), Error> {
    let args = env::args().collect::<Vec<String>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    set_quiet(has_arg(&args, "--quiet"));

    if let Some(data_dir) = arg_value(&args, "--data-dir") {
        utils::config::set_lince_data_dir_override(PathBuf::from(data_dir))?;
    }

    if let Some(result) = admin::dispatch(&args).await {
        return match result {
            Ok(()) => Ok(()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        };
    }

    let server_mode = has_arg(&args, "--server");

    let mut staged_setup = read_staged_setup()?;
    if let Some(auth_enabled) = staged_setup.as_ref().and_then(|setup| setup.auth_enabled) {
        bootstrap_config::set_auth_enabled(auth_enabled)?;
    }

    if let Some(password) = initial_admin_password(&args)? {
        let setup = staged_setup.get_or_insert_with(DesktopInstallSetup::default);
        setup.initial_admin_password = Some(password);
    }

    let bootstrap = bootstrap_config::load_or_init_bootstrap_config()?;
    let listen_addr = resolve_listen_addr(&args)?;

    if staged_setup.is_some() {
        remove_staged_setup()?;
    }

    serve_cell_api_only(
        listen_addr,
        bootstrap.secret,
        bootstrap.auth_enabled || server_mode,
        staged_setup,
        None,
        if server_mode {
            HttpServeMode::ApiOnly
        } else {
            HttpServeMode::FullUi
        },
    )
    .await
}

fn initial_admin_password(args: &[String]) -> Result<Option<String>, Error> {
    if let Some(path) = arg_value(args, "--initial-admin-password-file") {
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| Error::other(format!("Cannot read `{path}`: {error}")))?;
        let password = raw.trim().to_string();
        if password.is_empty() {
            return Err(Error::other(format!("`{path}` is empty")));
        }
        return Ok(Some(password));
    }
    Ok(arg_value(args, "--initial-admin-password")
        .map(|password| password.trim().to_string())
        .filter(|password| !password.is_empty()))
}

fn print_help() {
    println!("Usage: lince [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help            Show this help message");
    println!("      --data-dir <path> Override the Lince data directory");
    println!("      --port <port>     Override only the HTTP listen port");
    println!("      --listen-addr <addr>  Override the HTTP listen address");
    println!("      --quiet          Suppress normal status output");
    println!();
    println!("Server mode:");
    println!("      --server         Serve only the API: no board UI, no sands, no static assets.");
    println!("                       Forces login on — without it, hiding the board would");
    println!("                       still leave the socket open to anyone on the network.");
    println!("      --initial-admin-password-file <path>  Create the first admin from a file");
    println!("      --initial-admin-password <password>   Same, but visible in `ps`");
    println!();
    println!("Administering a Cell with no board (run as the Cell's own user):");
    println!("  lince organ list                       Contacts, their trust, their login");
    println!("  lince organ users                      Who can log into this Cell");
    println!("  lince organ trust <who> known          Let them sync");
    println!("  lince organ login <who> <username>     Let them ENTER as that user (live mode)");
    println!("  lince organ logout <who>               Take it back");
    println!("  lince discovery                        Show the discovery doors");
    println!("  lince discovery accept-unknown on      Allow pairing; turn off once paired");
    println!();
    println!("To learn more visit https://lince.social")
}

fn has_arg(args: &[String], expected: &str) -> bool {
    args.iter().any(|arg| arg == expected)
}

fn arg_value(args: &[String], expected: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == expected).then(|| window[1].clone()))
}

fn resolve_listen_addr(args: &[String]) -> Result<Option<String>, Error> {
    if let Some(listen_addr) = arg_value(args, "--listen-addr") {
        validate_listen_addr(&listen_addr)?;
        return Ok(Some(listen_addr));
    }

    let Some(port) = arg_value(args, "--port") else {
        return Ok(None);
    };
    let port = port
        .parse::<u16>()
        .map_err(|error| Error::other(format!("Invalid port `{port}`: {error}")))?;
    let listen_addr = format!("127.0.0.1:{port}");
    validate_listen_addr(&listen_addr)?;
    Ok(Some(listen_addr))
}

fn validate_listen_addr(listen_addr: &str) -> Result<(), Error> {
    listen_addr
        .parse::<SocketAddr>()
        .map(|_| ())
        .map_err(|error| Error::other(format!("Invalid listen address `{listen_addr}`: {error}")))
}
