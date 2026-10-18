use std::ffi::OsString;

use super::{Result, cargo_command, checked};

const NATIVE_FEATURES: &str = "nested,worker-gate,native-client-gate";
const INPUT_FEATURES: &str = "nested,server-tests";

#[cfg(target_os = "linux")]
mod validation;

pub fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|argument| argument.to_str()) {
        #[cfg(target_os = "linux")]
        Some("validate") => validation::dispatch(&arguments[1..]),
        Some("ci") if arguments.len() == 1 => {
            for arguments in [
                vec!["check", "--locked", "-p", "lince", "--features", "park"],
                vec!["check", "--locked", "-p", "lince"],
                vec!["check", "--locked", "-p", "lince-park", "--all-targets", "--features", "session,server-tests,clipboard-gate"],
                vec!["test", "--locked", "-p", "xtask", "park::validation::tests"],
                vec!["test", "--locked", "-p", "lince-park", "--features", "server-tests", "--lib"],
            ] {
                let flags = std::env::var("RUSTFLAGS").unwrap_or_default();
                checked(cargo_command().args(arguments).env("RUSTFLAGS", format!("{flags} -D warnings")).env("RUST_MIN_STACK", "33554432"))?;
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
                "cargo xtask park <session|nested|clipboard-check|protocol-check|input-check|policy-check|graphics-check|graphics-gate|native-check|native-client-check|worker-check|sync-check>"
            );
            println!("Runs Park protocol regressions and Vulkan/wgpu sharing probes.");
            println!("Use park validate help for isolated, bounded session acceptance runs.");
            println!("Use park ci for compiler and deterministic protocol checks without a desktop session.");
            println!(
                "Use park nested --outputs 2 --smoke to exercise monitor focus and application movement."
            );
            Ok(())
        }
        _ => Err("unknown Park task; use cargo xtask park help".into()),
    }
}

fn input_check() -> Result<()> {
    for (package, features, filter) in [("lince-park", INPUT_FEATURES, "server::tests"), ("lince-park", INPUT_FEATURES, "native_input::"),
        ("lince-park", INPUT_FEATURES, "desktop::renderer::capture::tests"),
        ("lince-park", "session,server-tests", "session::display::tests"), ("lince-interface", "native-presentation", "layout::desktop::tests"),
        ("lince-smithay-capture", "backend_drm", "wayland::image_copy_capture::tests")] {
        let flags = std::env::var("RUSTFLAGS").unwrap_or_default();
        checked(cargo_command().args([
            "test", "--locked", "-p", package, "--features", features, "--lib", filter,
        ]).env("RUSTFLAGS", format!("{flags} -D warnings")).env("RUST_MIN_STACK", "33554432"))?;
    }
    Ok(())
}
