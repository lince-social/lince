use crate::Result;
use std::{ffi::OsString, path::Path};

mod character;
mod fixtures;
mod geometry;
mod validate;

#[cfg(test)]
mod tests;

pub(super) fn dispatch(root: &Path, caller: &Path, args: &[OsString]) -> Result<()> {
    match args.first().and_then(|value| value.to_str()) {
        Some("prepare") if args.len() == 2 => {
            let output = caller.join(&args[1]);
            fixtures::create(root, &output)?;
            let report = validate::check(&output)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Some("check") if args.len() == 2 => {
            let report = validate::check(&caller.join(&args[1]))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        Some("baseline") if args.len() == 2 => {
            let output = caller.join(&args[1]);
            if output.exists() {
                return Err(
                    "Baseline requires a new output directory; existing evidence is retained"
                        .into(),
                );
            }
            crate::checked(
                crate::cargo_command()
                    .current_dir(root)
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-sandbox",
                        "--example",
                        "workspace_baseline",
                        "--",
                    ])
                    .arg(output),
            )
        }
        None | Some("help" | "--help" | "-h") if args.len() <= 1 => {
            println!("cargo xtask workspace baseline <new-output-directory>");
            println!("cargo xtask workspace prepare <new-fixture-directory>");
            println!("cargo xtask workspace check <fixture-directory>");
            println!(
                "Runs native input, Area/gravity/settling and idle measurements in an isolated workspace."
            );
            Ok(())
        }
        _ => Err("Use cargo xtask workspace help".into()),
    }
}
