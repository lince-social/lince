#![forbid(unsafe_code)]

mod bootstrap_config;

use std::{env, io::Error, net::SocketAddr, path::PathBuf};

use utils::desktop_setup::{read_staged_setup, remove_staged_setup};
use utils::logging::set_quiet;
use web::serve_cell_api_only;

fn main() -> Result<(), Error> {
    // `engine::actions::act_at_with_authorship` is one giant async fn covering
    // every `Action` variant (78 arms as of 2026-07-19, still growing with the
    // transfer/negotiation work) — its generated state machine outgrows
    // tokio's default 2 MiB worker-thread stack in debug builds, so ANY Action
    // (not just kanban's) can stack-overflow and abort the whole process a few
    // seconds after boot. Bypass the `#[tokio::main]` macro to size the
    // runtime's worker threads generously instead (verified: 2 MiB reliably
    // crashes on a single `act()` call, 32 MiB does not).
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

    // The installer stages a one-shot setup file (auth toggle, initial admin
    // password, language, desktop startup flags). The auth toggle lands in the
    // bootstrap config here; the rest is imported by the Cell bootstrap inside
    // `serve_cell_api_only`, after which the staged file is removed.
    let staged_setup = read_staged_setup()?;
    if let Some(auth_enabled) = staged_setup.as_ref().and_then(|setup| setup.auth_enabled) {
        bootstrap_config::set_auth_enabled(auth_enabled)?;
    }

    let bootstrap = bootstrap_config::load_or_init_bootstrap_config()?;
    let listen_addr = resolve_listen_addr(&args)?;

    if staged_setup.is_some() {
        remove_staged_setup()?;
    }

    serve_cell_api_only(
        listen_addr,
        bootstrap.secret,
        bootstrap.auth_enabled,
        staged_setup,
        None,
    )
    .await
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
