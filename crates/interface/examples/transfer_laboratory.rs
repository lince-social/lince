use lince_interface::native::laboratory::{CaseResult, catalogue};
use serde::Serialize;
use std::{fs::OpenOptions, io::Error};

#[derive(Serialize)]
struct Report {
    schema: u32,
    scope: &'static str,
    behavior: Vec<CaseResult>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let output = arguments
        .next()
        .ok_or_else(|| Error::other("Choose a new report filename"))?;
    let filters = arguments
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let cases: Vec<_> = catalogue()
        .into_iter()
        .filter(|case| {
            [
                "::transfer_castle::",
                "::trust_castle::",
                "::map_castle::",
                "::organ_castle::social::profile_claims::",
            ]
            .iter()
            .any(|module| case.name.contains(module))
                && (filters.is_empty() || filters.iter().any(|filter| case.name.contains(filter)))
        })
        .collect();
    if !cases.iter().any(|case| {
        case.name.ends_with(
            "four_lince_transport_control_draft_finishes_sharing_before_later_settlement",
        )
    }) {
        return Err(
            Error::other("The Transport journey is missing from the Laboratory catalogue").into(),
        );
    }
    let result = std::thread::Builder::new()
        .name("transfer-laboratory".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || -> Result<Vec<CaseResult>, Error> {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(32 * 1024 * 1024)
                .enable_all()
                .build()?;
            Ok(cases
                .into_iter()
                .map(|case| {
                    let result = case.execute(&runtime);
                    eprintln!(
                        "{} {} ({:.1} ms)",
                        if result.error.is_none() {
                            "PASS"
                        } else {
                            "FAIL"
                        },
                        result.name,
                        result.milliseconds
                    );
                    result
                })
                .collect())
        })?
        .join()
        .map_err(|_| Error::other("Transfer Laboratory thread panicked"))??;
    let failed = result.iter().any(|result| result.error.is_some());
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    serde_json::to_writer_pretty(
        file,
        &Report {
            schema: 1,
            scope: "Selected Transfer Laboratory behavior; no GPU or OS/device claim",
            behavior: result,
        },
    )?;
    if failed {
        return Err(Error::other("Inspect the failed cases in the Laboratory report").into());
    }
    Ok(())
}
