use lince_ailuros::{
    configuration::Configuration,
    serial::{Cancellation, Session},
};
use std::{ffi::OsString, io::Read};

fn text(value: &OsString) -> Result<&str, String> {
    value
        .to_str()
        .ok_or_else(|| "Ailuros arguments must be UTF-8".into())
}

fn configuration() -> Result<Configuration, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read configuration from stdin")?;
    if bytes.len() > 8192 {
        return Err("Configuration input exceeds 8192 bytes".into());
    }
    let config: Configuration = serde_json::from_slice(&bytes).map_err(
        |_| "Invalid configuration JSON; input is not echoed because it may contain credentials",
    )?;
    config.validate()?;
    Ok(config)
}

pub(super) fn dispatch(arguments: &[OsString]) -> Result<(), String> {
    let cancellation = cancellation()?;
    match arguments {
        [verb] if verb == "profiles" => {
            let profiles = lince_ailuros::profile::catalog().into_iter().map(|(id, _)| {
                lince_ailuros::profile::builtin(id).map(|profile| serde_json::json!({"id": id, "profile": profile}))
            }).collect::<Result<Vec<_>, _>>()?;
            println!("{}", serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?);
        }
        [verb, project, board] if verb == "export" => {
            lince_ailuros::project::export_for_board(std::path::Path::new(project), text(board)?)?;
            println!("Editable firmware project created for {}", text(board)?);
        }
        [verb, project] if verb == "export" => {
            lince_ailuros::project::export(std::path::Path::new(project))?;
            println!("Editable firmware project created");
        }
        [verb, project, board] if verb == "select-profile" => {
            let profile = lince_ailuros::project::select_board(std::path::Path::new(project), text(board)?)?;
            println!("Selected {}. Compile again before installation.", profile.name);
        }
        [verb, project] if verb == "compile" || verb == "compile-tests" => {
            let root = std::path::Path::new(project);
            let toolchain = lince_ailuros::project::Toolchain::Nix;
            let artifact = if verb == "compile-tests" {
                lince_ailuros::project::compile_tests(root, toolchain, &cancellation)?
            } else {
                lince_ailuros::project::compile(root, toolchain, &cancellation)?
            };
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({"artifact":artifact,"fingerprint":artifact.fingerprint()?})).map_err(|e| e.to_string())?);
        }
        [verb, port] if verb == "probe" => {
            let chip = lince_ailuros::project::probe(text(port)?, lince_ailuros::project::Toolchain::Nix, &cancellation)?;
            println!("{}",serde_json::to_string_pretty(&chip).map_err(|e|e.to_string())?);
        }
        [verb, project, port, fingerprint, mac] if verb == "flash" => {
            let root = std::path::Path::new(project);
            let artifact = lince_ailuros::project::artifact(root)?;
            lince_ailuros::project::flash(root, &artifact, text(fingerprint)?, text(port)?, text(mac)?, lince_ailuros::project::Toolchain::Nix, &cancellation)?;
            println!("Firmware installation confirmed by esptool; inspect the running device before provisioning");
        }
        [verb] if verb == "list" => {
            println!("{}", serde_json::to_string_pretty(&lince_ailuros::serial::ports()?).map_err(|error| error.to_string())?);
        }
        [verb, port] if verb == "inspect" => {
            let identity = Session::open(text(port)?)?.identity(&cancellation)?;
            println!("{}", serde_json::to_string_pretty(&identity).map_err(|error| error.to_string())?);
        }
        [verb, port, public_key] if verb == "status" || verb == "monitor" => {
            let mut session = Session::open(text(port)?)?;
            loop {
                cancellation.check()?;
                let status = session.status(text(public_key)?, &cancellation)?;
                println!("{}",serde_json::to_string(&status).map_err(|e|e.to_string())?);
                if verb == "status" { break; }
                for _ in 0..20 { cancellation.check()?; std::thread::sleep(std::time::Duration::from_millis(100)); }
            }
        }
        [verb] if verb == "validate-config" => {
            configuration()?;
            println!("Configuration valid");
        }
        [verb, port, public_key] if verb == "provision" => {
            let config = configuration()?;
            Session::open(text(port)?)?.configure(&config, text(public_key)?, &cancellation)?;
            println!("Device confirmed configuration storage");
        }
        _ => return Err("usage: cargo xtask ailuros profiles | list | export <folder> [board-profile-id] | select-profile <folder> <board-profile-id> | compile <folder> | compile-tests <folder> | probe <port> | flash <folder> <port> <artifact-fingerprint> <expected-MAC> | inspect <port> | status <port> <public-key> | monitor <port> <public-key> | validate-config < configuration.json | provision <port> <enrolled-public-key> < configuration.json".into()),
    }
    Ok(())
}

fn cancellation() -> Result<Cancellation, String> {
    let cancellation = Cancellation::default();
    let token = cancellation.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        let mut signal = {
            let _entered = runtime.enter();
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|e| e.to_string())?
        };
        std::thread::spawn(move || {
            runtime.block_on(async move {
                signal.recv().await;
                token.cancel();
            })
        });
    }
    #[cfg(not(unix))]
    std::thread::spawn(move || {
        runtime.block_on(async move {
            let _ = tokio::signal::ctrl_c().await;
            token.cancel();
        })
    });
    Ok(cancellation)
}
