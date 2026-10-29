use std::ffi::OsString;

use super::{Result, cargo_command, checked};

const NATIVE_FEATURES: &str = "nested,worker-gate,native-client-gate";
const INPUT_FEATURES: &str = "nested,server-tests";

#[cfg(target_os = "linux")]
mod showoff;
#[cfg(target_os = "linux")]
mod validation;

pub fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|argument| argument.to_str()) {
        Some("release") => release(&arguments[1..]),
        #[cfg(target_os = "linux")]
        Some("control") => checked(cargo_command().args(["run", "--locked", "-p", "lince-park", "--features", "server-tests", "--example", "park_control", "--"]).args(&arguments[1..])),
        #[cfg(target_os = "linux")]
        Some("validate") => validation::dispatch(&arguments[1..]),
        #[cfg(target_os = "linux")]
        Some("showoff") => showoff::run(&arguments[1..]).map_err(|error| error.to_string()),
        #[cfg(target_os = "linux")]
        Some("showoff-ready") => showoff::ready(&arguments[1..]).map_err(|error| error.to_string()),
        Some("ci") if arguments.len() == 1 => {
            for arguments in [
                vec!["check", "--locked", "-p", "lince", "--features", "park"],
                vec!["check", "--locked", "-p", "lince"],
                vec![
                    "check",
                    "--locked",
                    "-p",
                    "lince",
                    "--no-default-features",
                    "--features",
                    "instinct",
                ],
                vec![
                    "check",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--all-targets",
                    "--features",
                    "session,server-tests,clipboard-gate",
                ],
                vec!["test", "--locked", "-p", "xtask", "park::validation"],
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--no-default-features",
                    "--features",
                    "activation",
                    "--lib",
                    "activation::tests",
                ],
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "server-tests",
                    "--lib",
                ],
            ] {
                checked(
                    cargo_command()
                        .args(arguments)
                        .env("RUST_MIN_STACK", "33554432"),
                )?;
            }
            input_check()
        }
        Some("input-check") if arguments.len() == 1 => input_check(),
        Some("session") => {
            let mut command = cargo_command();
            command.args([
                "run",
                "--locked",
                "-p",
                "lince",
                "--features",
                "park",
                "--",
                "--park",
            ]);
            command.args(&arguments[1..]);
            checked(&mut command)
        }
        Some("clipboard-check") if arguments.len() == 1 => checked(cargo_command().args([
            "run",
            "--locked",
            "-p",
            "lince-park",
            "--features",
            "clipboard-gate",
            "--example",
            "clipboard_probe",
        ])),
        Some("nested") => {
            let mut command = cargo_command();
            command.args([
                "run",
                "--locked",
                "-p",
                "lince-park",
                "--features",
                NATIVE_FEATURES,
                "--example",
                "nested",
                "--",
            ]);
            command.args(&arguments[1..]);
            checked(&mut command)
        }
        Some("protocol-check") if arguments.len() == 1 => checked(cargo_command().args([
            "test",
            "--locked",
            "-p",
            "lince-park",
            "--features",
            "server-tests",
            "--lib",
            "server::tests",
        ])),
        Some(
            task @ ("graphics-check"
            | "graphics-gate"
            | "native-check"
            | "policy-check"
            | "native-client-check"
            | "worker-check"
            | "sync-check"),
        ) if arguments.len() == 1 => {
            let (feature, example) = match task {
                "sync-check" => ("graphics-probe", "synchronization_probe"),
                "graphics-gate" => ("graphics-gate", "graphics_gate"),
                "native-check" => (NATIVE_FEATURES, "native_canvas_probe"),
                "policy-check" => (NATIVE_FEATURES, "native_policy_probe"),
                "native-client-check" => (NATIVE_FEATURES, "native_client_probe"),
                "worker-check" => (NATIVE_FEATURES, "worker_probe"),
                _ => ("graphics-probe", "graphics_probe"),
            };
            checked(cargo_command().args([
                "run",
                "--locked",
                "-p",
                "lince-park",
                "--features",
                feature,
                "--example",
                example,
            ]))
        }
        None | Some("help" | "--help" | "-h") => {
            println!(
                "cargo xtask park <session|nested|control|clipboard-check|protocol-check|input-check|policy-check|graphics-check|graphics-gate|native-check|native-client-check|worker-check|sync-check>"
            );
            println!("Runs Park protocol regressions and Vulkan/wgpu sharing probes.");
            println!("Use park validate help for isolated, bounded session acceptance runs.");
            println!(
                "Use park showoff --nested for an interactive release session with Firefox and the saved Laboratory showoff workspace."
            );
            println!(
                "Use park ci for compiler and deterministic protocol checks without a desktop session."
            );
            println!(
                "Use park nested --outputs 2 --smoke to exercise monitor focus and application movement."
            );
            Ok(())
        }
        _ => Err("unknown Park task; use cargo xtask park help".into()),
    }
}

fn release(arguments: &[OsString]) -> Result<()> {
    let mut target = None;
    let mut headless = false;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--target") if target.is_none() => {
                target = Some(
                    arguments
                        .next()
                        .ok_or("--target requires a target triple")?,
                );
            }
            Some("--headless") if !headless => headless = true,
            _ => return Err("Use cargo xtask park release --target TARGET [--headless]".into()),
        }
    }
    let target = target.ok_or("--target requires a target triple")?;
    let park = !headless && target.to_string_lossy().ends_with("-linux-gnu");
    let revision = match std::env::var("LINCE_REVISION") {
        Ok(revision) if !revision.trim().is_empty() => revision,
        _ => {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err("Set LINCE_REVISION when releasing outside a Git checkout".into());
            }
            String::from_utf8(output.stdout)
                .map_err(|error| error.to_string())?
                .trim()
                .to_owned()
        }
    };
    let mut command = cargo_command();
    command
        .args([
            "run",
            "--release",
            "--locked",
            "--package",
            "lince",
            "--features",
            if park { "instinct,park" } else { "instinct" },
            "--target",
        ])
        .arg(target)
        .arg("--timings");
    if headless {
        command.arg("--no-default-features");
    }
    checked(
        command
            .args(["--", "--build-info"])
            .env("LINCE_REVISION", revision)
            .env("RUST_MIN_STACK", "33554432"),
    )
}

fn input_check() -> Result<()> {
    for (package, features, filter) in [
        ("lince-park", INPUT_FEATURES, "server::tests"),
        ("lince-park", INPUT_FEATURES, "native_input::"),
        (
            "lince-park",
            INPUT_FEATURES,
            "desktop::renderer::capture::tests",
        ),
        (
            "lince-park",
            "session,server-tests",
            "session::display::tests",
        ),
        (
            "lince-interface",
            "native-presentation",
            "layout::desktop::tests",
        ),
        (
            "lince-smithay-capture",
            "backend_drm",
            "wayland::image_copy_capture::tests",
        ),
    ] {
        checked(
            cargo_command()
                .args([
                    "test",
                    "--locked",
                    "-p",
                    package,
                    "--features",
                    features,
                    "--lib",
                    filter,
                ])
                .env("RUST_MIN_STACK", "33554432"),
        )?;
    }
    Ok(())
}
