use cell::fiote::laboratory::{Options, Report};
use std::io::{self, Write};

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
        let _lock = lince_sandbox::instance::claim_headless(&data)?;
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
            let mut connected = cell.runtime().clone();
            connected.fiote = Some(host.clone());
            let _entered = runtime.enter();
            let report = lince_sandbox::laboratory::fiote::run_headless(connected, options);
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
}
