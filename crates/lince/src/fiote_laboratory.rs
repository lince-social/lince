use cell::fiote::laboratory::{Options, Report};
use std::{
    io::{self, IsTerminal, Write},
    path::Path,
};

pub fn options(args: &[String]) -> io::Result<Options> {
    let options = Options {
        record: super::arg_value(args, "--laboratory-record"),
        separate: super::has_arg(args, "--laboratory-separate"),
        profile: super::arg_value(args, "--laboratory-profile"),
        model: super::arg_value(args, "--laboratory-model"),
        reasoning: super::arg_value(args, "--laboratory-reasoning"),
    };
    for flag in [
        "--laboratory-record",
        "--laboratory-profile",
        "--laboratory-model",
        "--laboratory-reasoning",
        "--laboratory-vault-password-file",
        "--laboratory-output",
    ] {
        if super::has_arg(args, flag)
            && super::arg_value(args, flag).is_none_or(|value| value.starts_with("--"))
        {
            return Err(io::Error::other(format!("Provide a value after {flag}")));
        }
    }
    Ok(options)
}

pub fn headless(args: &[String]) -> io::Result<()> {
    let options = options(args)?;
    let output = super::arg_value(args, "--laboratory-output")
        .map(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
        })
        .transpose()?;
    if let Some(directory) = super::arg_value(args, "--directory") {
        utils::config::set_lince_data_dir_override(directory.into())?;
    }
    let data = utils::config::lince_data_dir()
        .ok_or_else(|| io::Error::other("Cannot locate Lince data"))?;
    let started = std::time::Instant::now();
    let report = (|| -> io::Result<Report> {
        let _lock = lince_desktop::instance::claim_headless(&data)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()?;
        let cell = runtime.block_on(cell::Cell::open_mobile(cell::CellOptions::default()))?;
        let result = (|| -> io::Result<Report> {
            let engine = cell.runtime().engine.clone();
            let host = std::sync::Arc::new(
                runtime
                    .block_on(cell::fiote::Host::open(engine, data.join("fiote")))
                    .map_err(io::Error::other)?,
            );
            let password =
                if let Some(path) = super::arg_value(args, "--laboratory-vault-password-file") {
                    Some(password_file(Path::new(&path))?)
                } else if io::stdin().is_terminal() && io::stderr().is_terminal() {
                    cell::fiote::laboratory::terminal_password().map_err(io::Error::other)?
                } else {
                    None
                };
            if let Some(password) = password {
                runtime
                    .block_on(host.laboratory_unlock(password))
                    .map_err(io::Error::other)?;
            }
            let mut connected = cell.runtime().clone();
            connected.fiote = Some(host.clone());
            let _entered = runtime.enter();
            let report = lince_desktop::laboratory::fiote::run_headless(connected, options);
            runtime.block_on(host.stop_all());
            Ok(report)
        })();
        runtime.block_on(cell.shutdown());
        result
    })()
    .unwrap_or_else(|error| Report {
        outcome: "blocked".into(),
        stage: "startup".into(),
        detail: format!("Cannot start the live test: {error}"),
        elapsed_ms: started.elapsed().as_millis() as u64,
        ..Default::default()
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(mut file) = output {
        file.write_all(&bytes)?;
    } else {
        println!("{}", String::from_utf8_lossy(&bytes));
    }
    if report.outcome == "passed" {
        Ok(())
    } else {
        Err(io::Error::other(
            "Fiote live test did not pass; see its report",
        ))
    }
}

fn password_file(path: &Path) -> io::Result<cell::FioteSecret> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(io::Error::other(
            "Use a password file of at most 4096 bytes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "The password file must be readable only by its owner (chmod 600)",
            ));
        }
    }
    use io::Read;
    let mut password = cell::FioteSecret::default();
    file.take(4097).read_to_string(&mut password.0)?;
    if password.0.len() > 4096 {
        return Err(io::Error::other("Password file is too large"));
    }
    let length = password.0.trim_end_matches(['\r', '\n']).len();
    password.0.truncate(length);
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_require_values_and_leave_account_credentials_out() {
        assert!(options(&["lince".into(), "--laboratory-record".into()]).is_err());
        let parsed = options(&[
            "lince".into(),
            "--laboratory-record".into(),
            "r_test".into(),
            "--laboratory-separate".into(),
        ])
        .unwrap();
        assert_eq!(parsed.record.as_deref(), Some("r_test"));
        assert!(parsed.separate);
    }

    #[cfg(unix)]
    #[test]
    fn password_files_require_private_permissions_and_preserve_spaces() {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("lince-password-test-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, " secret with spaces \n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(password_file(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(password_file(&path).unwrap().0, " secret with spaces ");
        std::fs::remove_file(path).unwrap();
    }
}
