use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod prune;

type Result<T> = std::result::Result<T, String>;

fn main() {
    if let Err(error) = dispatch() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn dispatch() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let task = args.next().unwrap_or_default();
    let extra: Vec<_> = args.collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|parent| parent.parent())
        .ok_or("cannot find workspace root")?
        .to_path_buf();
    env::set_current_dir(&root).map_err(|error| error.to_string())?;

    match task.to_str() {
        Some("prune") => prune::run(&root, &extra),
        Some("release") => {
            no_extra(&extra)?;
            release(&root)
        }
        Some("dev" | "test") => dev(&extra),
        Some("lince") => {
            no_extra(&extra)?;
            checked(Command::new("nix").args([
                "develop",
                ".#interface",
                "-c",
                "cargo",
                "run",
                "--release",
                "-p",
                "lince",
            ]))
        }
        Some("server") => {
            no_extra(&extra)?;
            checked(Command::new("cargo").args([
                "run",
                "--release",
                "-p",
                "lince",
                "--no-default-features",
                "--",
                "--server",
            ]))
        }
        Some("facade") => {
            no_extra(&extra)?;
            checked(Command::new("cargo").args([
                "run",
                "--release",
                "-p",
                "lince",
                "--no-default-features",
                "--features",
                "facade",
                "--",
                "--server",
                "--facade",
            ]))
        }
        Some("test-all") => {
            no_extra(&extra)?;
            checked(Command::new("cargo").args(["test", "--workspace", "--all-targets"]))
        }
        Some("stop") => {
            no_extra(&extra)?;
            let _ = Command::new("systemctl")
                .args(["--user", "stop", "lince"])
                .status();
            checked(Command::new("nix").arg("develop"))
        }
        Some("nix") => {
            no_extra(&extra)?;
            nix_session()
        }
        Some("help") | None | Some("--help") | Some("-h") => {
            no_extra(&extra)?;
            println!("cargo xtask <release|dev|test|lince|server|facade|test-all|stop|nix|prune>");
            println!("dev and test pass additional arguments to Lince.");
            println!(
                "prune [--dry-run] [--target-dir PATH] removes superseded incremental snapshots."
            );
            Ok(())
        }
        _ => Err(format!("unknown task: {}", task.to_string_lossy())),
    }
}

fn no_extra(extra: &[std::ffi::OsString]) -> Result<()> {
    if extra.is_empty() {
        Ok(())
    } else {
        Err("this task does not accept arguments".into())
    }
}

fn checked(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .map_err(|error| format!("could not start {:?}: {error}", command.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{:?} exited with {status}", command.get_program()))
    }
}

fn dev(extra: &[std::ffi::OsString]) -> Result<()> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("HOME").map_or_else(
                || PathBuf::from("/.config"),
                |home| PathBuf::from(home).join(".config"),
            )
        });
    let directory = env::var_os("LINCE_DATA_DIR")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| config.join("test_lince").into_os_string());
    let port = env::var_os("LINCE_PORT")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "6176".into());
    let mut command = if Command::new("nix")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        let mut command = Command::new("nix");
        command.args(["develop", ".#interface", "-c", "cargo"]);
        command
    } else {
        Command::new("cargo")
    };
    command.args([
        "run",
        "--release",
        "-p",
        "lince",
        "--no-default-features",
        "--features",
        "ui",
        "--",
        "--no-tray",
    ]);
    command.args(extra);
    command
        .arg("--directory")
        .arg(directory)
        .arg("--port")
        .arg(port);
    checked(&mut command)
}

fn nix_session() -> Result<()> {
    let shell = env::var_os("SHELL").unwrap_or_else(|| "sh".into());
    checked(Command::new("mprocs").args([
        "--names",
        "hx,claude,codex,git,shell,run,restart",
        "hx .",
        "claude",
        "codex",
        "lazygit",
        shell.to_str().ok_or("SHELL is not valid UTF-8")?,
        "systemctl --user stop lince; nix develop",
        "systemctl --user restart lince;",
    ]))
}

fn release(root: &PathBuf) -> Result<()> {
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|error| error.to_string())?;
    if !branch.status.success() || branch.stdout != b"main\n" {
        return Err("main only".into());
    }
    checked(Command::new("git-cliff").arg("--version"))?;
    checked(
        Command::new("cargo")
            .args(["set-version", "--help"])
            .stdout(Stdio::null()),
    )?;
    if !Command::new("gh")
        .args(["auth", "status"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        checked(Command::new("gh").args(["auth", "login"]))?;
        checked(Command::new("gh").args(["auth", "status"]))?;
    }

    let suggested = Command::new("git")
        .args(["cliff", "--bumped-version"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_owned())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "0.1.0".into());
    print!("Version (auto: {suggested}): ");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| error.to_string())?;
    let selected = if input.trim().is_empty() {
        suggested.as_str()
    } else {
        input.trim()
    };
    let number = selected.strip_prefix('v').unwrap_or(selected);
    if !number.starts_with(|character: char| character.is_ascii_digit())
        || !number
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".+-".contains(&byte))
    {
        return Err("invalid version".into());
    }
    let tag = format!("v{number}");

    checked(Command::new("git").args([
        "cliff",
        "--unreleased",
        "--bump",
        "--tag",
        &tag,
        "-o",
        "CHANGELOG.md",
    ]))?;
    checked(Command::new("cargo").args(["set-version", "--workspace", number]))?;
    let manifest = root.join("Cargo.toml");
    let source = fs::read_to_string(&manifest).map_err(|error| error.to_string())?;
    let updated = update_versions(&source, number)?;
    fs::write(manifest, updated).map_err(|error| error.to_string())?;
    checked(
        Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .stdout(Stdio::null()),
    )?;
    checked(Command::new("git").args(["add", "."]))?;
    checked(Command::new("git").args(["commit", "-m", &format!("chore(release): {tag}")]))?;
    checked(Command::new("git").args(["tag", "-a", &tag, "-m", &tag]))?;
    checked(Command::new("git").args(["push", "origin", "main", &tag]))?;
    checked(Command::new("gh").args([
        "run",
        "list",
        "--workflow=build.yml",
        "--limit",
        "1",
        "--json",
        "url",
        "-q",
        ".[0].url",
    ]))
}

fn update_versions(source: &str, version: &str) -> Result<String> {
    let mut section = "";
    let mut workspace_version = false;
    let mut dependencies = 0;
    let mut result = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        if line.starts_with('[') {
            section = line.trim();
        }
        if section == "[workspace.package]" && line.starts_with("version = \"") {
            result.push_str(&format!("version = \"{version}\"\n"));
            workspace_version = true;
        } else if section == "[workspace.dependencies]" {
            if let Some(start) = line.find("version = \"=") {
                let value_start = start + "version = \"=".len();
                let end = line[value_start..]
                    .find('"')
                    .ok_or("malformed workspace dependency version")?
                    + value_start;
                result.push_str(&line[..value_start]);
                result.push_str(version);
                result.push_str(&line[end..]);
                dependencies += 1;
            } else {
                result.push_str(line);
            }
        } else {
            result.push_str(line);
        }
    }
    if !workspace_version || dependencies == 0 {
        return Err("workspace versions were not found".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::update_versions;

    #[test]
    fn updates_only_workspace_versions() {
        let input = "[workspace.package]\nversion = \"0.7.0\"\n[workspace.dependencies]\nlince = { version = \"=0.7.0\", path = \"crates/lince\" }\nother = \"1\"\n[profile.dev]\nversion = \"=keep\"\n";
        let output = update_versions(input, "0.8.0").unwrap();
        assert_eq!(
            output,
            "[workspace.package]\nversion = \"0.8.0\"\n[workspace.dependencies]\nlince = { version = \"=0.8.0\", path = \"crates/lince\" }\nother = \"1\"\n[profile.dev]\nversion = \"=keep\"\n"
        );
    }
}
