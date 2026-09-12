#![forbid(unsafe_code)]
#![recursion_limit = "256"]

mod admin;

use std::{env, io::Error, net::SocketAddr, path::PathBuf};

use utils::desktop_setup::{read_staged_setup, remove_staged_setup};
use utils::logging::{init, set_quiet};

fn main() -> Result<(), Error> {
    let args = env::args().collect::<Vec<String>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    #[cfg(not(feature = "facade"))]
    if has_arg(&args, "--facade") {
        return Err(Error::other(
            "Facade is not included in this executable. Run `mise facade` or compile with `--features facade`.",
        ));
    }
    #[cfg(not(feature = "ui"))]
    if has_arg(&args, "--laboratory-headless") {
        return Err(Error::other(
            "Laboratory requires a Lince executable with the interface included.",
        ));
    }
    #[cfg(feature = "ui")]
    if has_arg(&args, "--laboratory-headless") {
        let mut config = lince_interface::laboratory::StressConfig::default();
        if let Some(value) = arg_value(&args, "--laboratory-max-sands") {
            config.max_sands = value.parse().map_err(Error::other)?;
        }
        let report = lince_interface::laboratory::run_headless(config)?;
        let bytes = serde_json::to_vec_pretty(&report).map_err(Error::other)?;
        if let Some(path) = arg_value(&args, "--laboratory-output") {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(&bytes)?;
        } else {
            println!("{}", String::from_utf8_lossy(&bytes));
        }
        return if report.behavior.iter().any(|result| result.error.is_some()) {
            Err(Error::other(
                "Laboratory behavior checks failed; see the report",
            ))
        } else {
            Ok(())
        };
    }
    set_quiet(has_arg(&args, "--quiet"));
    init()?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()?;

    if let Some(data_dir) = arg_value(&args, "--directory") {
        utils::config::set_lince_data_dir_override(PathBuf::from(data_dir))?;
    }

    if let Some(result) = runtime.block_on(admin::dispatch(&args)) {
        return match result {
            Ok(()) => Ok(()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        };
    }

    let server_mode = has_arg(&args, "--server") || cfg!(not(feature = "ui"));
    #[cfg(feature = "ui")]
    let instance = if !server_mode {
        let data_dir = utils::config::lince_data_dir()
            .ok_or_else(|| Error::other("Cannot find the Lince data directory"))?;
        match runtime.block_on(lince_interface::instance::claim(&data_dir))? {
            Some(instance) => Some(instance),
            None => return Ok(()),
        }
    } else {
        None
    };
    let listen_addr = resolve_listen_addr(&args)?;

    let diagnostics = utils::diagnostics::Diagnostics::global();
    let _journal = match utils::config::lince_data_dir() {
        Some(directory) => match utils::diagnostics::Journal::open(
            directory.join("notifications.json"),
            diagnostics.clone(),
        ) {
            Ok(journal) => Some(journal),
            Err(error) => {
                let message = format!(
                    "Could not open notification history. Existing history has been kept: {error}"
                );
                diagnostics.report("lince::startup", &message);
                eprintln!("{message}");
                None
            }
        },
        None => return Err(Error::other("Cannot find the Lince data directory")),
    };

    let staged_setup = read_staged_setup()?;
    let language = staged_setup
        .as_ref()
        .and_then(|setup| setup.language.clone());
    let staged_password = initial_admin_password(&args)?.or_else(|| {
        staged_setup
            .as_ref()
            .and_then(|setup| setup.initial_admin_password.clone())
    });
    if staged_setup.is_some() {
        remove_staged_setup()?;
    }

    let mut cell = runtime
        .block_on(cell::Cell::open(cell::CellOptions {
            data_dir: None,
            local_base_url: listen_addr.as_ref().map(|addr| format!("http://{addr}")),
            language,
        }))
        .map_err(|error| {
            diagnostics.report(
                "cell::startup",
                &format!("Lince could not start this Cell: {error}"),
            );
            error
        })?;

    if server_mode || (has_arg(&args, "--facade") && staged_password.is_some()) {
        if let Err(error) = runtime.block_on(cell::ensure_admin(
            &cell.runtime().store,
            cell::AdminBootstrap {
                auth_required: true,
                mandatory: true,
                staged_password,
            },
        )) {
            runtime.block_on(cell.shutdown());
            diagnostics.report("cell::startup", &error.to_string());
            return Err(error);
        }
    }

    #[cfg(feature = "facade")]
    let _facade = if has_arg(&args, "--facade") {
        let address = listen_addr.as_deref().unwrap_or("127.0.0.1:6174");
        match runtime.block_on(lince_facade::Facade::start(cell.runtime().clone(), address)) {
            Ok(facade) => {
                println!("Facade: http://{}", facade.address);
                Some(facade)
            }
            Err(error) => {
                runtime.block_on(cell.shutdown());
                return Err(error);
            }
        }
    } else {
        None
    };

    #[cfg(feature = "facade")]
    let facade_address = _facade.as_ref().map(|facade| facade.address.to_string());
    #[cfg(not(feature = "facade"))]
    let facade_address = None;
    runtime.block_on(cell.start_information(server_mode, facade_address))?;
    let information = cell
        .runtime()
        .information
        .clone()
        .expect("information service started");

    if server_mode {
        let mut updates = information.clone();
        let signal = runtime.block_on(async {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => signal,
                _ = updates.wait_for_restart() => Ok(()),
            }
        });
        runtime.block_on(cell.shutdown());
        signal?;
        #[cfg(feature = "facade")]
        drop(_facade);
        if information.ready_to_restart() {
            utils::self_update::net::restart_program(&information.state.borrow().executable)?;
        }
        return Ok(());
    }

    #[cfg(feature = "ui")]
    {
        let cell_runtime = cell.runtime().clone();
        let _guard = runtime.enter();
        let result = lince_interface::run_native_interface(
            cell_runtime,
            instance
                .as_ref()
                .expect("UI instance was claimed before opening the Cell"),
            &utils::config::lince_data_dir()
                .expect("UI data directory was resolved before opening the Cell"),
        );
        runtime.block_on(cell.shutdown());
        result?;
        #[cfg(feature = "facade")]
        drop(_facade);
        drop(instance);
        if information.ready_to_restart() {
            utils::self_update::net::restart_program(&information.state.borrow().executable)?;
        }
    }
    Ok(())
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
    println!("Runs this Lince Cell — Karma, Transfers, sync, file sync and the peer");
    println!("endpoint. Without --server it also opens the native interface window;");
    println!("closing the window keeps the Cell running (recover it from the tray).");
    println!();
    println!("Options:");
    println!("  -h, --help            Show this help message");
    println!("      --directory <path> Override the Lince data directory");
    println!("      --port <port>     Override only the peer/local listen port");
    println!("      --listen-addr <addr>  Override the local listen address");
    println!("      --quiet          Suppress normal status output");
    #[cfg(feature = "facade")]
    {
        println!("      --facade         Serve the browser Kanban and record view");
        println!("                       at http://127.0.0.1:6174 (or --listen-addr / --port)");
    }
    println!();
    println!("Server mode:");
    #[cfg(feature = "ui")]
    {
        println!(
            "      --laboratory-headless    Run Interface behavior and CPU stress checks without a window"
        );
        println!("      --laboratory-max-sands N Limit each stress workload (default 8192)");
        println!("      --laboratory-output PATH Save the JSON report to a new file");
    }
    println!("      --server         Headless: run the Cell with no window. Requires an");
    println!("                       admin account — pass one on first run:");
    println!("      --initial-admin-password-file <path>  Create the first admin from a file");
    println!("      --initial-admin-password <password>   Same, but visible in `ps`");
    println!();
    println!("Administering a Cell with no window (run as the Cell's own user):");
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
