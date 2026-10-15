use std::ffi::OsString;

use super::{Result, cargo_command, checked};

const NATIVE_FEATURES: &str = "nested,worker-gate,native-client-gate";

pub fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|argument| argument.to_str()) {
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
            | "worker-check"),
        ) if arguments.len() == 1 => {
            let (feature, example) = match task {
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
                "cargo xtask park <nested|protocol-check|policy-check|graphics-check|graphics-gate|native-check|native-client-check|worker-check>"
            );
            println!("Runs Park protocol regressions and Vulkan/wgpu sharing probes.");
            println!(
                "Use park nested --outputs 2 --smoke to exercise monitor focus and application movement."
            );
            Ok(())
        }
        _ => Err("unknown Park task; use cargo xtask park help".into()),
    }
}
