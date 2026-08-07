#![forbid(unsafe_code)]

mod bootstrap_config;

use std::{env, io::Error, net::SocketAddr, path::PathBuf};

use utils::desktop_setup::{DesktopInstallSetup, read_staged_setup, remove_staged_setup};
use utils::logging::set_quiet;
use web::{HttpServeMode, serve_cell_api_only};

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

    // Server mode: hold the data, answer authenticated clients, hand nobody a
    // board. Forcing auth on is not a convenience — `authenticate_headers` is
    // a no-op when auth is off, so dropping the board while leaving
    // `/host/transport/ws` reachable would still give any network peer an
    // unauthenticated way to act on this store. Hiding the UI without this
    // would look like hardening and be none.
    //
    // Deliberately NOT persisted to `lince.toml`. `--server` is a
    // per-invocation posture, and a unit file that passes the flag every time
    // does not need it on disk. Writing it would also strand anyone trying the
    // flag out: the run can still abort afterwards (bad --listen-addr,
    // unreadable password file, the no-admin refusal below), and a persisted
    // toggle would survive that — leaving a login wall with no account on a
    // desktop board, from a flag they had already given up on.
    let server_mode = has_arg(&args, "--server");

    // The installer stages a one-shot setup file (auth toggle, initial admin
    // password, language, desktop startup flags). The auth toggle lands in the
    // bootstrap config here; the rest is imported by the Cell bootstrap inside
    // `serve_cell_api_only`, after which the staged file is removed.
    let mut staged_setup = read_staged_setup()?;
    if let Some(auth_enabled) = staged_setup.as_ref().and_then(|setup| setup.auth_enabled) {
        bootstrap_config::set_auth_enabled(auth_enabled)?;
    }

    // Non-interactive provisioning of the first admin, for a unit file or a
    // container that has no terminal to prompt on. This reuses the installer's
    // one-shot setup channel rather than inventing a second path into
    // `cell_bootstrap`.
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
        // `--server` overrides a configured `enabled = false`: an operator who
        // asks for a server gets login, and a stale config never silently
        // unlocks one.
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

/// The initial admin password, from a file (preferred) or straight from argv.
///
/// The file form exists because argv is world-readable in `ps` and normally
/// ends up committed in a unit file; a mode-0600 secret next to it is not.
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
    println!(
        "      --server         Serve only the API: no board UI, no sands, no static assets."
    );
    println!(
        "                       Forces login on — without it, hiding the board would"
    );
    println!(
        "                       still leave the socket open to anyone on the network."
    );
    println!("      --initial-admin-password-file <path>  Create the first admin from a file");
    println!("      --initial-admin-password <password>   Same, but visible in `ps`");
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
