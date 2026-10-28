use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod ailuros;
mod android;
mod android_clean;
mod android_smoke;
mod content;
mod design;
mod hand_models;
mod maintenance;
mod park;
mod prune;
mod workspace;

type Result<T> = std::result::Result<T, String>;

fn main() {
    if let Err(error) = dispatch() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn dispatch() -> Result<()> {
    let caller_directory = env::current_dir().map_err(|error| error.to_string())?;
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
        Some("map-region") => {
            if extra.len() != 3 {
                return Err("usage: cargo xtask map-region <source-settings.json> <region.json> <new-output-directory>".into());
            }
            checked(
                cargo_command()
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-map",
                        "--example",
                        "region",
                        "--",
                    ])
                    .args(extra.iter().map(|path| caller_directory.join(path))),
            )
        }
        Some("map-package") => {
            if extra.len() != 1 {
                return Err("usage: cargo xtask map-package <package-directory>".into());
            }
            checked(
                cargo_command()
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-map",
                        "--example",
                        "package",
                        "--",
                    ])
                    .arg(caller_directory.join(&extra[0])),
            )
        }
        Some("content") => content::package(&root, &caller_directory, &extra),
        Some("maintenance") => maintenance::dispatch(&root, &extra),
        Some("workspace") => workspace::dispatch(&root, &caller_directory, &extra),
        Some("ailuros") => ailuros::dispatch(&extra),
        Some("lingua") => {
            let expression_mode = extra.first().is_some_and(|argument| argument == "eval");
            let mut arguments = extra;
            let mut directory_value = false;
            for (index, argument) in arguments.iter_mut().enumerate() {
                if directory_value {
                    *argument = caller_directory.join(&*argument).into_os_string();
                    directory_value = false;
                } else if argument == "--directory" {
                    directory_value = true;
                } else if !expression_mode
                    && !argument.to_string_lossy().starts_with('-')
                    && !(index == 0 && (argument == "check" || argument == "fmt"))
                {
                    *argument = caller_directory.join(&*argument).into_os_string();
                }
            }
            let mut command = cargo_command();
            command
                .args([
                    "run",
                    "--locked",
                    "-p",
                    "lince",
                    "--no-default-features",
                    "--",
                    "lingua",
                ])
                .args(arguments);
            if let Some(directory) = env::var_os("LINCE_DATA_DIR") {
                command.env("LINCE_DATA_DIR", caller_directory.join(directory));
            }
            checked(&mut command)
        }
        Some("hand-models") => {
            no_extra(&extra)?;
            hand_models::download(&root)
        }
        Some("design") => design::dispatch(&root, &extra),
        Some("park") => park::dispatch(&extra),
        Some("android") => android::dispatch(&root, &extra),
        Some("android-clean") => android_clean::run(&root, &extra),
        Some("android-smoke") => android_smoke::run(&root, &extra),
        Some("plugin-example") => {
            if extra.len() != 1 {
                return Err("usage: cargo xtask plugin-example <Lince-data-directory>".into());
            }
            checked(
                cargo_command()
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-plugin-nostr",
                        "--",
                        "--install",
                    ])
                    .args(&extra),
            )
        }
        Some("plugin-files") => {
            if extra.len() != 1 {
                return Err("usage: cargo xtask plugin-files <Lince-data-directory>".into());
            }
            checked(
                cargo_command()
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-plugin-files",
                        "--",
                        "--install",
                    ])
                    .args(&extra),
            )
        }
        Some("plugin-activitypub" | "plugin-irc") => {
            if extra.len() != 1 {
                return Err(
                    "usage: cargo xtask <plugin-activitypub|plugin-irc> <Lince-data-directory>"
                        .into(),
                );
            }
            let package = if task == "plugin-irc" {
                "lince-plugin-irc"
            } else {
                "lince-plugin-activitypub"
            };
            checked(
                cargo_command()
                    .args(["run", "--locked", "-p", package, "--", "--install"])
                    .args(&extra),
            )
        }
        Some("mobile-preview") => {
            if extra.len() > 1 {
                return Err("usage: cargo xtask mobile-preview [data-directory]".into());
            }
            let directory = extra
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("target/mobile-preview-data"));
            println!("Mobile preview data: {}", directory.display());
            checked(
                cargo_command()
                    .args([
                        "run",
                        "--locked",
                        "-p",
                        "lince-mobile",
                        "--features",
                        "preview",
                        "--bin",
                        "lince-mobile-preview",
                        "--",
                    ])
                    .arg(directory),
            )
        }
        Some("version") => {
            no_extra(&extra)?;
            version(&root)
        }
        Some("dev") => prune::with_cleanup(&root, || dev(&extra)),
        Some("check") => {
            prune::with_cleanup(&root, || checked(cargo_command().arg("check").args(&extra)))
        }
        Some("test") => {
            if extra.is_empty() {
                return Err("usage: cargo xtask test -p <package> <test-name> -- --exact".into());
            }
            prune::with_cleanup(&root, || checked(cargo_command().arg("test").args(&extra)))
        }
        Some("release") => {
            no_extra(&extra)?;
            checked(cargo_command().args(["run", "--release", "-p", "lince"]))
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
        Some("check-instinct") => {
            no_extra(&extra)?;
            prune::with_cleanup(&root, check_instinct)
        }
        Some("test-all") => {
            no_extra(&extra)?;
            prune::with_cleanup(&root, || {
                checked(cargo_command().args(["test", "--workspace", "--all-targets"]))
            })
        }
        Some("help") | None | Some("--help") | Some("-h") => {
            no_extra(&extra)?;
            println!(
                "cargo xtask <ailuros|lingua|design|workspace|park|dev|check|check-instinct|test|test-all|release|server|facade|version|android|android-clean|android-smoke|mobile-preview|plugin-example|plugin-files|plugin-activitypub|plugin-irc|hand-models>"
            );
            println!(
                "android opens an Android emulator; android help lists setup and APK options."
            );
            println!("mobile-preview [data-directory] runs the mobile UI in a desktop window.");
            println!(
                "android-clean previews retired Android cache files; --apply removes them; --builds also clears compiled artifacts."
            );
            println!("dev passes additional arguments to Lince.");
            println!("hand-models downloads the ONNX models for Hand Camera Castle.");
            println!(
                "LINCE_DATA_DIR and LINCE_PORT select dev data and port; LINCE_INSTINCT=1 enables the handbook."
            );
            println!(
                "check-instinct checks both handbook feature paths and shared practice contracts."
            );
            println!("design help lists focused HTML studies and native preview commands.");
            println!(
                "check and test forward arguments to Cargo; test-all runs every workspace target."
            );
            println!("One test: cargo xtask test -p <package> <test-name> -- --exact");
            println!(
                "dev, check, check-instinct, test and test-all prune superseded incremental snapshots before and after running."
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

fn check_instinct() -> Result<()> {
    let commands: &[&[&str]] = &[
        &["check", "--locked", "--package", "lince"],
        &[
            "check",
            "--locked",
            "--package",
            "lince",
            "--features",
            "instinct",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "anicca",
            "--test",
            "instinct",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--no-default-features",
            "--test",
            "practice",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "engine",
            "--features",
            "instinct",
            "--test",
            "instinct_import",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "instinct",
            "--lib",
            "ontology::tests::",
            "--",
            "--test-threads=1",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "instinct",
            "--lib",
            "sound::tests::",
            "--",
            "--test-threads=1",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "native-presentation",
            "--lib",
            "description::tests::",
            "--",
            "--test-threads=1",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "instinct",
            "--lib",
            "description::tests::",
            "--",
            "--test-threads=1",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "instinct",
            "--lib",
            "instinct",
            "--",
            "--test-threads=1",
        ],
        &[
            "test",
            "--locked",
            "--package",
            "lince-interface",
            "--features",
            "instinct",
            "--lib",
            "kept_cell_reopens",
            "--",
            "--test-threads=1",
        ],
    ];
    for args in commands {
        checked(cargo_command().args(*args))?;
    }
    Ok(())
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
    let mut command = cargo_command();
    let mut features = vec!["ui"];
    if env::var("LINCE_INSTINCT").is_ok_and(|value| value == "1") {
        features.push("instinct");
    }
    if extra.iter().any(|argument| argument == "--facade") {
        features.push("facade");
    }
    let features = features.join(",");
    command.args([
        "run",
        "--release",
        "-p",
        "lince",
        "--no-default-features",
        "--features",
        &features,
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

fn cargo_command() -> Command {
    if Command::new("nix")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        let mut command = Command::new("nix");
        command.args(["develop", ".#interface", "-c", "cargo"]);
        command
    } else {
        Command::new("cargo")
    }
}

fn version(root: &PathBuf) -> Result<()> {
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
