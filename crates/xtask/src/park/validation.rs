use super::super::{Result, cargo_command};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{IsTerminal, Read, Write},
    net::TcpListener,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod browser;
mod diagnostics;
mod metrics;
mod observe;

static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn check_interruption() -> Result<()> {
    if INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
        Err("Validation interrupted; the session was not accepted as an orderly logout".into())
    } else {
        Ok(())
    }
}

fn interrupt_handler() -> Result<tokio::runtime::Runtime> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(error)?;
    runtime
        .block_on(async {
            let mut interrupt =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::spawn(async move {
                tokio::select! { _ = interrupt.recv() => {}, _ = terminate.recv() => {} }
                INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            Ok::<_, std::io::Error>(())
        })
        .map_err(error)?;
    Ok(runtime)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Program {
    program: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Helper {
    target: Program,
    capabilities: Vec<String>,
}

impl Helper {
    fn valid(&self) -> bool {
        !self.capabilities.is_empty()
            && self.capabilities.len() <= 6
            && self
                .capabilities
                .iter()
                .enumerate()
                .all(|(index, capability)| {
                    matches!(
                        capability.as_str(),
                        "clipboard"
                            | "window-list"
                            | "window-control"
                            | "output-control"
                            | "capture"
                            | "input-method"
                    ) && !self.capabilities[..index].contains(capability)
                })
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    terminal: Program,
    #[serde(default)]
    launcher: Option<Program>,
    #[serde(default)]
    lock: Option<Program>,
    #[serde(default)]
    showoff: bool,
    #[serde(default)]
    clients: Vec<Program>,
    #[serde(default)]
    helper: Option<Helper>,
    #[serde(default = "one")]
    lifecycle_rounds: u32,
    #[serde(default = "observation_seconds")]
    observation_seconds: u64,
    #[serde(default)]
    soak_seconds: u64,
}

fn one() -> u32 {
    1
}
fn observation_seconds() -> u64 {
    90
}

impl Configuration {
    fn valid_duration(&self) -> bool {
        (1..=20).contains(&self.lifecycle_rounds)
            && (15..=300).contains(&self.observation_seconds)
            && self.soak_seconds <= 3600
    }

    fn session_seconds(&self) -> u64 {
        90 + u64::from(self.lifecycle_rounds) * 55 + self.observation_seconds + self.soak_seconds
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Run {
    repository: PathBuf,
    directory: PathBuf,
    executable_sha256: String,
    process_executable: PathBuf,
    process_sha256: String,
    build_info: serde_json::Value,
    installed: bool,
    probe_sha256: String,
    capture_sha256: String,
    revision: String,
    port: u16,
    peer_port: u16,
    configuration: Configuration,
    recovery: String,
}

#[derive(Deserialize, Serialize)]
struct Phase {
    name: String,
    outcome: String,
    elapsed_ms: u128,
    detail: String,
}

#[derive(Serialize)]
struct Results {
    phases: Vec<Phase>,
    observations: BTreeMap<String, String>,
    resources: Vec<ResourceSample>,
}

#[derive(Serialize)]
struct ResourceSample {
    elapsed_ms: u128,
    resident_kib: Option<u64>,
    peak_resident_kib: Option<u64>,
    threads: Option<u64>,
    descriptors: Option<usize>,
    cpu_ticks: Option<u64>,
    drm_clients: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

fn resource_sample(pid: rustix::process::Pid, elapsed_ms: u128) -> ResourceSample {
    let base = PathBuf::from(format!("/proc/{}", pid.as_raw_nonzero()));
    let status = fs::read_to_string(base.join("status")).unwrap_or_default();
    let field = |key: &str| {
        status.lines().find_map(|line| {
            line.strip_prefix(key)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
    };
    let stat = fs::read_to_string(base.join("stat")).unwrap_or_default();
    let cpu_ticks = stat.rsplit_once(") ").and_then(|(_, fields)| {
        let values: Vec<_> = fields.split_whitespace().collect();
        values
            .get(11)?
            .parse::<u64>()
            .ok()?
            .checked_add(values.get(12)?.parse::<u64>().ok()?)
    });
    let mut drm_clients = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(base.join("fdinfo")) {
        for entry in entries.flatten() {
            let Ok(contents) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let fields: BTreeMap<String, String> = contents
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.starts_with("drm-")
                        .then(|| (name.to_owned(), value.trim().to_owned()))
                })
                .collect();
            let Some(client) = fields.get("drm-client-id") else {
                continue;
            };
            let device = fields
                .get("drm-pdev")
                .cloned()
                .or_else(|| {
                    fs::read_link(base.join("fd").join(entry.file_name()))
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
            drm_clients.insert(format!("{device}:{client}"), fields);
        }
    }
    ResourceSample {
        elapsed_ms,
        resident_kib: field("VmRSS:"),
        peak_resident_kib: field("VmHWM:"),
        threads: field("Threads:"),
        descriptors: fs::read_dir(base.join("fd"))
            .ok()
            .map(|entries| entries.filter_map(std::result::Result::ok).count()),
        cpu_ticks,
        drm_clients: (!drm_clients.is_empty()).then_some(drm_clients),
    }
}

fn resource_summary(samples: &[ResourceSample]) -> String {
    let Some(first) = samples.first() else {
        return "Resource samples: unavailable\n".into();
    };
    let Some(last) = samples.last() else {
        return "Resource samples: unavailable\n".into();
    };
    let difference = |first: Option<u64>, last: Option<u64>| {
        first
            .zip(last)
            .map(|(first, last)| (i128::from(last) - i128::from(first)).to_string())
            .unwrap_or_else(|| "unavailable".into())
    };
    let descriptors = first
        .descriptors
        .zip(last.descriptors)
        .map(|(first, last)| (last as i128 - first as i128).to_string())
        .unwrap_or_else(|| "unavailable".into());
    format!(
        "Resource samples: {} across {} ms; RSS delta {} KiB; descriptor delta {}; CPU tick delta {}; DRM client counters available in {} samples. Measurements require interpretation, not a stability pass.\n",
        samples.len(),
        last.elapsed_ms.saturating_sub(first.elapsed_ms),
        difference(first.resident_kib, last.resident_kib),
        descriptors,
        difference(first.cpu_ticks, last.cpu_ticks),
        samples
            .iter()
            .filter(|sample| sample.drm_clients.is_some())
            .count(),
    )
}

pub(super) fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|argument| argument.to_str()) {
        Some("analyze") => metrics::analyze(&arguments[1..]),
        Some("browser") => browser::run(&arguments[1..]),
        Some("observe") => observe::run(&arguments[1..]),
        Some("prepare") if arguments.len() == 2 => prepare(Path::new(&arguments[1]), None),
        Some("prepare") if arguments.len() == 4 && arguments[1] == "--executable" => {
            prepare(Path::new(&arguments[3]), Some(Path::new(&arguments[2])))
        }
        Some("seat") if arguments.len() == 2 => seat(Path::new(&arguments[1])),
        Some("ready") if arguments.len() == 2 => ready(Path::new(&arguments[1])),
        Some("report") if arguments.len() == 2 => {
            let path = Path::new(&arguments[1]).join("summary.txt");
            print!("{}", fs::read_to_string(path).map_err(error)?);
            Ok(())
        }
        None | Some("help" | "--help") => {
            println!(
                "cargo xtask park validate prepare [--executable /installed/bin/lince] /absolute/path/to/fixtures.json"
            );
            println!("cargo xtask park validate report /absolute/path/to/run");
            println!(
                "cargo xtask park validate observe /installed/bin/lince PID [--seconds 5400] [--unit USER_UNIT]"
            );
            println!(
                "cargo xtask park validate browser /installed/bin/lince --geckodriver PATH [--plain] [--provider FIXTURE.json] [--host-close PROGRAM] [--host-prepare PROGRAM] [--ownership] [--save-failure] [--reopen-sandbox] [--lifecycle-rounds 0] [--fullscreen] [--seconds 75] [--outputs 1]"
            );
            println!(
                "cargo xtask park validate analyze LOG [--warmup 10] [--seconds 60] [--minimum-fps 57] [--surface ID]"
            );
            println!(
                "Fixtures: {{\"terminal\":{{\"program\":\"xdg-terminal-exec\",\"arguments\":[]}},\"showoff\":false,\"clients\":[]}}"
            );
            println!(
                "Preparation checks and compiles before emitting one physical-session command."
            );
            Ok(())
        }
        _ => Err("Use cargo xtask park validate help".into()),
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(error)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(error)?;
    file.write_all(&bytes).map_err(error)?;
    file.sync_all().map_err(error)?;
    fs::rename(temporary, path).map_err(error)
}

fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(format!(
            "Invalid or oversized test configuration: {}",
            path.display()
        ));
    }
    serde_json::from_reader(File::open(path).map_err(error)?).map_err(error)
}

fn executable(program: &Path) -> Result<PathBuf> {
    let candidate = if program.components().count() > 1 {
        program.to_path_buf()
    } else {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(program))
            .find(|path| path.is_file())
            .ok_or_else(|| format!("Missing executable: {}", program.display()))?
    };
    let name = candidate
        .file_name()
        .ok_or("Executable path has no file name")?;
    let parent = candidate
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let candidate = fs::canonicalize(parent).map_err(error)?.join(name);
    let metadata = fs::metadata(&candidate).map_err(error)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!("Not executable: {}", candidate.display()));
    }
    Ok(candidate)
}

fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(error)?;
        if count == 0 {
            return Ok(hash
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect());
        }
        hash.update(&buffer[..count]);
    }
}

fn bounded(command: &mut Command, deadline: Duration) -> Result<(ExitStatus, u128)> {
    let start = Instant::now();
    let mut child = command.spawn().map_err(error)?;
    loop {
        if let Err(error) = check_interruption() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if let Some(status) = child.try_wait().map_err(error)? {
            return Ok((status, start.elapsed().as_millis()));
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Phase exceeded {} seconds", deadline.as_secs()));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn logged(command: &mut Command, path: &Path) -> Result<()> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(error)?;
    command
        .stdout(file.try_clone().map_err(error)?)
        .stderr(file);
    Ok(())
}

fn preparation_step(directory: &Path, name: &str, arguments: &[&str]) -> Result<()> {
    let mut command = cargo_command();
    command.args(arguments).env("RUST_MIN_STACK", "33554432");
    deny_warnings(&mut command);
    logged(&mut command, &directory.join(format!("{name}.log")))?;
    let (status, _) = bounded(&mut command, Duration::from_secs(3600))?;
    if !status.success() {
        return Err(format!(
            "{name} failed: {status}; see {}",
            directory.display()
        ));
    }
    Ok(())
}

fn prepare_capture_probe(directory: &Path) -> Result<String> {
    let mut command = cargo_command();
    command.args([
        "run",
        "--release",
        "--locked",
        "--message-format=json",
        "-p",
        "lince-park",
        "--features",
        "server-tests",
        "--example",
        "capture_probe",
        "--",
        "--help",
    ]);
    deny_warnings(&mut command);
    let output = command.output().map_err(error)?;
    fs::write(directory.join("prepare-capture-probe.log"), &output.stderr).map_err(error)?;
    if !output.status.success() {
        return Err("Preparing the capture probe failed".into());
    }
    let binary = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|value| {
            value["reason"] == "compiler-artifact" && value["target"]["name"] == "capture_probe"
        })
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
        .last()
        .ok_or("Cargo did not report the capture probe executable")?;
    let target = directory.join("bin/capture-probe");
    fs::copy(binary, &target).map_err(error)?;
    digest(&target)
}

fn deny_warnings(command: &mut Command) {
    command.env("RUSTFLAGS", super::warning_flags());
}

pub(super) fn inspect_build(executable: &Path) -> Result<serde_json::Value> {
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "10s"])
        .arg(executable)
        .arg("--build-info")
        .output()
        .map_err(error)?;
    if !output.status.success() {
        return Err("The selected executable cannot report its build information".into());
    }
    let information: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(error)?;
    validate_build(&information)?;
    Ok(information)
}

fn validate_build(information: &serde_json::Value) -> Result<()> {
    if information["features"]["park"] != true
        || information["features"]["ui"] != true
        || information["optimized"] != true
        || !information["target"]
            .as_str()
            .is_some_and(|target| target.ends_with("-linux-gnu"))
        || !["version", "revision"].iter().all(|key| {
            information[key]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        })
    {
        return Err("The selected executable must report an optimized Linux build with UI and Park support, a version and revision".into());
    }
    Ok(())
}

fn process_executable(executable: &Path) -> Result<PathBuf> {
    let executable = fs::canonicalize(executable).map_err(error)?;
    let payload = executable
        .parent()
        .ok_or("Executable has no parent")?
        .join(".lince-wrapped");
    let executable = if payload.is_file() {
        fs::canonicalize(payload).map_err(error)?
    } else {
        executable
    };
    let mut magic = [0; 4];
    File::open(&executable)
        .map_err(error)?
        .read_exact(&mut magic)
        .map_err(error)?;
    if magic == *b"\x7fELF" {
        return Ok(executable);
    }
    Err("Cannot identify the selected wrapper's compositor payload".into())
}

fn prepare(configuration: &Path, installed: Option<&Path>) -> Result<()> {
    let selected = installed
        .map(|path| -> Result<PathBuf> {
            if !path.is_absolute() {
                return Err("--executable requires an absolute installed path".into());
            }
            let path = executable(path)?;
            inspect_build(&path)?;
            Ok(path)
        })
        .transpose()?;
    if !configuration.is_absolute() {
        return Err("Use an absolute fixtures path".into());
    }
    let mut configuration: Configuration = load(configuration)?;
    if !configuration.valid_duration() {
        return Err(
            "Use 1–20 lifecycle rounds, 15–300 observation seconds and at most 3600 soak seconds"
                .into(),
        );
    }
    if configuration
        .helper
        .as_ref()
        .is_some_and(|helper| !helper.valid())
    {
        return Err("Choose explicit, distinct supported helper capabilities".into());
    }
    if configuration.clients.len() > 16 {
        return Err("At most sixteen test clients are supported".into());
    }
    for program in std::iter::once(&mut configuration.terminal)
        .chain(&mut configuration.clients)
        .chain(
            configuration
                .helper
                .iter_mut()
                .map(|helper| &mut helper.target),
        )
        .chain(configuration.launcher.iter_mut())
        .chain(configuration.lock.iter_mut())
    {
        program.program = executable(&program.program)?;
        if program.arguments.len() > 128
            || program
                .arguments
                .iter()
                .any(|argument| argument.len() > 8192 || argument.contains('\0'))
        {
            return Err("Invalid fixture arguments".into());
        }
    }
    let nix = executable(Path::new("nix"))?;
    let timeout = executable(Path::new("timeout"))?;
    executable(Path::new("loginctl"))?;
    executable(Path::new("dbus-run-session"))?;
    executable(Path::new("dbus-update-activation-environment"))?;
    let repository = env::current_dir().map_err(error)?;
    let base = repository.join("target/park-validation");
    fs::create_dir_all(&base).map_err(error)?;
    let metadata = fs::symlink_metadata(&base).map_err(error)?;
    if !metadata.is_dir() || metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err("Validation state must be an owned directory, not a symlink".into());
    }
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).map_err(error)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_nanos();
    let directory = base.join(format!("run-{stamp}-{}", std::process::id()));
    fs::create_dir(&directory).map_err(error)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(error)?;
    for name in ["data", "bin"] {
        fs::create_dir(directory.join(name)).map_err(error)?;
    }
    let profiles = directory.join("profiles");
    fs::create_dir(&profiles).map_err(error)?;
    for (index, program) in std::iter::once(&mut configuration.terminal)
        .chain(&mut configuration.clients)
        .chain(
            configuration
                .helper
                .iter_mut()
                .map(|helper| &mut helper.target),
        )
        .chain(configuration.launcher.iter_mut())
        .chain(configuration.lock.iter_mut())
        .enumerate()
    {
        let profile = profiles.join(format!("client-{index}"));
        fs::create_dir(&profile).map_err(error)?;
        let run_path = directory.to_str().ok_or("Test directory must be UTF-8")?;
        let profile_path = profile.to_str().ok_or("Test profile must be UTF-8")?;
        for argument in &mut program.arguments {
            *argument = argument
                .replace("{run}", run_path)
                .replace("{profile}", profile_path);
        }
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(error)?;
    if !revision.status.success() {
        return Err("Cannot record source revision".into());
    }
    fs::write(directory.join("source-revision.txt"), revision.stdout).map_err(error)?;
    let mut session = Command::new("loginctl");
    session.args(["list-sessions", "--no-pager"]);
    logged(&mut session, &directory.join("sessions-before.log"))?;
    let _ = bounded(&mut session, Duration::from_secs(10))?;
    if selected.is_none() {
        for (name, arguments) in [
            (
                "validation-tooling",
                vec!["test", "--locked", "-p", "xtask", "park::validation"],
            ),
            (
                "check-session",
                vec!["check", "--locked", "-p", "lince", "--features", "park"],
            ),
            ("check-sandbox", vec!["check", "--locked", "-p", "lince"]),
            (
                "protocol",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "server-tests",
                    "--lib",
                ],
            ),
            (
                "native-input-protocol",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "server::tests",
                ],
            ),
            (
                "native-input-editor",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "native_input::",
                ],
            ),
            (
                "capture-buffer-layout",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "desktop::renderer::capture::tests",
                ],
            ),
            (
                "capture-gpu-export",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "capture_gpu_window_export",
                    "--",
                    "--ignored",
                ],
            ),
            (
                "connected-cell-startup",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "connected_cell_is_attached_before_render_plugins_finish",
                    "--",
                    "--ignored",
                ],
            ),
            (
                "physical-output-planning",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "session,server-tests",
                    "--lib",
                    "session::display::tests",
                ],
            ),
            (
                "window-layout-policy",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "layout::desktop::tests",
                ],
            ),
            (
                "capture-helper-lifecycle",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-smithay-capture",
                    "--features",
                    "backend_drm",
                    "--lib",
                    "wayland::image_copy_capture::tests",
                ],
            ),
            (
                "policy",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::NATIVE_FEATURES,
                    "--example",
                    "native_policy_probe",
                ],
            ),
            (
                "clipboard",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "clipboard-gate",
                    "--example",
                    "clipboard_probe",
                ],
            ),
            (
                "gpu-synchronization",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "graphics-probe",
                    "--example",
                    "synchronization_probe",
                ],
            ),
            (
                "nested",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::NATIVE_FEATURES,
                    "--example",
                    "nested",
                    "--",
                    "--directory",
                ],
            ),
        ] {
            if name == "nested" {
                let data = directory.join("nested-data");
                let data = data.to_str().ok_or("Test path must be UTF-8")?;
                let mut arguments = arguments;
                arguments.extend([data, "--outputs", "2", "--smoke"]);
                preparation_step(&directory, name, &arguments)?;
            } else {
                preparation_step(&directory, name, &arguments)?;
            }
        }
    }
    let target = directory.join("bin/lince");
    if let Some(selected) = &selected {
        std::os::unix::fs::symlink(selected, &target).map_err(error)?;
        let resolved = fs::canonicalize(selected).map_err(error)?;
        if let Ok(relative) = resolved.strip_prefix("/nix/store") {
            let package = Path::new("/nix/store").join(
                relative
                    .components()
                    .next()
                    .ok_or("Invalid package closure")?,
            );
            let status = Command::new("nix-store")
                .arg("--add-root")
                .arg(directory.join("package-closure"))
                .arg("--realise")
                .arg(package)
                .status()
                .map_err(error)?;
            if !status.success() {
                return Err("Could not pin the selected package closure".into());
            }
        }
    } else {
        let mut command = cargo_command();
        command.args([
            "run",
            "--release",
            "--locked",
            "--message-format=json",
            "-p",
            "lince",
            "--features",
            "instinct,park",
            "--",
            "--build-info",
        ]);
        deny_warnings(&mut command);
        let output = command.output().map_err(error)?;
        fs::write(directory.join("prepare-executable.log"), &output.stderr).map_err(error)?;
        if !output.status.success() {
            return Err("Preparing the executable failed; inspect prepare-executable.log".into());
        }
        let binary = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
            .filter(|value| {
                value["reason"] == "compiler-artifact" && value["target"]["name"] == "lince"
            })
            .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
            .last()
            .ok_or("Cargo did not report the Lince executable")?;
        fs::copy(binary, &target).map_err(error)?;
    }
    let build_info = inspect_build(&target)?;
    let process_executable = process_executable(&target)?;
    let mut probe = cargo_command();
    probe.args([
        "run",
        "--release",
        "--locked",
        "--message-format=json",
        "-p",
        "lince-park",
        "--features",
        "server-tests",
        "--example",
        "session_probe",
        "--",
        "--help",
    ]);
    deny_warnings(&mut probe);
    let output = probe.output().map_err(error)?;
    fs::write(directory.join("prepare-probe.log"), &output.stderr).map_err(error)?;
    if !output.status.success() {
        return Err("Preparing the native session probe failed".into());
    }
    let probe = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|value| {
            value["reason"] == "compiler-artifact" && value["target"]["name"] == "session_probe"
        })
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
        .last()
        .ok_or("Cargo did not report the session probe executable")?;
    fs::copy(probe, directory.join("bin/session-probe")).map_err(error)?;
    let capture_sha256 = prepare_capture_probe(&directory)?;
    let capture_data = directory.join("nested-capture-data");
    fs::create_dir(&capture_data).map_err(error)?;
    let capture_report = directory.join("nested-capture.json");
    write_json(
        &capture_data.join("park.json"),
        &serde_json::json!({"ready": {"program": directory.join("bin/capture-probe"),
        "arguments": [capture_report], "capabilities": ["capture"]}}),
    )?;
    if selected.is_some() {
        let mut command = Command::new(&target);
        command
            .args([
                "--park-nested",
                "--park-duration",
                "20",
                "--park-outputs",
                "2",
                "--directory",
            ])
            .arg(&capture_data)
            .args(["--port", "0", "--peer-port", "0"]);
        if configuration.showoff {
            command.arg("--showoff");
        }
        logged(
            &mut command,
            &directory.join("nested-installed-capture.log"),
        )?;
        let (status, _) = bounded(&mut command, Duration::from_secs(45))?;
        if !status.success() {
            return Err(format!("Installed nested capture failed: {status}"));
        }
    } else {
        preparation_step(
            &directory,
            "nested-capture-export",
            &[
                "run",
                "--locked",
                "-p",
                "lince-park",
                "--features",
                super::NATIVE_FEATURES,
                "--example",
                "nested",
                "--",
                "--directory",
                capture_data
                    .to_str()
                    .ok_or("Capture test path must be UTF-8")?,
                "--outputs",
                "2",
                "--ready-hook",
                "--smoke",
            ],
        )?;
    }
    let captured: serde_json::Value = load(&capture_report)?;
    if captured["status"] != "passed" {
        return Err("Nested output capture did not pass".into());
    }
    fs::copy(
        env::current_exe().map_err(error)?,
        directory.join("bin/xtask"),
    )
    .map_err(error)?;
    let http = TcpListener::bind("127.0.0.1:0").map_err(error)?;
    let peer = TcpListener::bind("127.0.0.1:0").map_err(error)?;
    let run = Run {
        repository: repository.clone(), directory: directory.clone(),
        executable_sha256: digest(&target)?, revision: build_info["revision"].as_str().unwrap().to_owned(),
        process_sha256: digest(&process_executable)?, process_executable, build_info,
        installed: selected.is_some(),
        probe_sha256: digest(&directory.join("bin/session-probe"))?,
        capture_sha256,
        port: http.local_addr().map_err(error)?.port(),
        peer_port: peer.local_addr().map_err(error)?.port(), configuration,
        recovery: "Return to the Niri VT recorded in sessions-before.log; use the independent timeout if the test hangs.".into(),
    };
    write_json(&directory.join("run.json"), &run)?;
    write_json(
        &directory.join("data/park.json"),
        &serde_json::json!({
            "terminal": run.configuration.terminal,
            "launcher": run.configuration.launcher,
            "lock": run.configuration.lock,
            "ready": {"program": directory.join("bin/xtask"), "arguments": ["park", "validate", "ready", directory],
                "capabilities": run.configuration.helper.as_ref().map(|helper| helper.capabilities.clone()).unwrap_or_else(|| vec!["capture".into()])}
        }),
    )?;
    let script = format!(
        "#!/bin/sh\n{} --signal=TERM --kill-after=10s {}s {} develop {} -c {} park validate seat {}\npark_status=$?\nprintf '%s\\n' \"$park_status\" > {}\nexit \"$park_status\"\n",
        quote(&timeout),
        run.configuration.session_seconds() + 30,
        quote(&nix),
        quote(&PathBuf::from(format!(
            "{}#interface",
            repository.display()
        ))),
        quote(&directory.join("bin/xtask")),
        quote(&directory),
        quote(&directory.join("watchdog-exit.txt")),
    );
    let wrapper = directory.join("run-seat");
    fs::write(&wrapper, script).map_err(error)?;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).map_err(error)?;
    println!(
        "Prepared {}\nOn a spare foreground TTY, run:\n{}\n{}",
        directory.display(),
        quote(&wrapper),
        run.recovery
    );
    Ok(())
}

fn quote(path: &Path) -> String {
    format!(
        "'{}'",
        path.as_os_str().to_string_lossy().replace('\'', "'\\''")
    )
}

fn native_environment(command: &mut Command) -> Result<()> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    for name in [
        "LD_LIBRARY_PATH",
        "GBM_BACKENDS_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
    ] {
        let saved = format!("LINCE_HOST_{name}");
        if let Some(original) = env::var_os(&saved) {
            match original.as_bytes().split_first() {
                Some((b'0', [])) => {
                    command.env_remove(name);
                }
                Some((b'1', value)) => {
                    command.env(name, OsString::from_vec(value.to_vec()));
                }
                _ => return Err(format!("Invalid saved environment for {name}")),
            }
        }
        command.env_remove(saved);
    }
    for name in [
        "NIRI_SOCKET",
        "SWAYSOCK",
        "I3SOCK",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "WAYFIRE_SOCKET",
    ] {
        command.env_remove(name);
    }
    Ok(())
}

fn ready(directory: &Path) -> Result<()> {
    let run: Run = load(&directory.join("run.json"))?;
    if run.directory != directory || !directory.is_absolute() {
        return Err("Invalid prepared run directory".into());
    }
    let socket =
        PathBuf::from(env::var_os("WAYLAND_DISPLAY").ok_or("Park did not supply its display")?);
    let socket = if socket.is_absolute() {
        socket
    } else {
        PathBuf::from(
            env::var_os("XDG_RUNTIME_DIR").ok_or("Park did not supply its runtime directory")?,
        )
        .join(socket)
    };
    if !socket.is_absolute() || !socket.exists() {
        return Err("Park readiness did not supply a live absolute display path".into());
    }
    let pid = env::var("LINCE_PARK_BOOTSTRAP_PID").map_err(error)?;
    let pid = pid.parse::<i32>().map_err(error)?;
    if pid <= 1
        || socket.file_name().and_then(|name| name.to_str())
            != Some(format!("wayland-park-{pid}").as_str())
    {
        return Err("Readiness process identity does not match the Park socket".into());
    }
    let bus = env::var("DBUS_SESSION_BUS_ADDRESS").map_err(error)?;
    let status = Command::new("dbus-update-activation-environment")
        .args([
            "WAYLAND_DISPLAY",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_TYPE",
            "XDG_SESSION_DESKTOP",
        ])
        .env_remove("WAYLAND_SOCKET")
        .status()
        .map_err(error)?;
    if !status.success() {
        return Err(
            "Could not configure the validation session's private D-Bus activation environment"
                .into(),
        );
    }
    if run
        .configuration
        .helper
        .as_ref()
        .is_some_and(|helper| !helper.valid())
    {
        return Err("Invalid helper authorization in the prepared fixture".into());
    }
    write_json(
        &directory.join("ready.json"),
        &serde_json::json!({"socket": socket, "pid": pid, "bus": bus}),
    )?;
    if let Some(helper) = &run.configuration.helper {
        let descriptor = env::var("WAYLAND_SOCKET")
            .map_err(error)?
            .parse::<i32>()
            .map_err(error)?;
        if descriptor < 0 {
            return Err(
                "The configured helper did not receive its authorized Wayland connection".into(),
            );
        }
        let failure = Command::new(&helper.target.program)
            .args(&helper.target.arguments)
            .exec();
        return Err(format!("The authorized helper could not start: {failure}"));
    }
    let failure = Command::new(directory.join("bin/capture-probe"))
        .arg(directory.join("physical-capture.json"))
        .exec();
    Err(format!(
        "The output capture probe could not start: {failure}"
    ))
}

struct ManagedChild {
    child: Child,
    group: rustix::process::Pid,
}

impl ManagedChild {
    fn spawn(command: &mut Command) -> Result<Self> {
        let child = command.process_group(0).spawn().map_err(error)?;
        let group = rustix::process::Pid::from_raw(child.id() as i32)
            .ok_or("Invalid child process group")?;
        Ok(Self { child, group })
    }
}

struct Children(Vec<ManagedChild>);

impl Drop for Children {
    fn drop(&mut self) {
        for process in &mut self.0 {
            let _ =
                rustix::process::kill_process_group(process.group, rustix::process::Signal::KILL);
            let _ = process.child.wait();
        }
    }
}

fn physical_terminal(metadata: &str, input: &Path) -> bool {
    let terminal = metadata.lines().find_map(|line| line.strip_prefix("TTY="));
    metadata.lines().any(|line| line == "Active=yes")
        && metadata
            .lines()
            .any(|line| line.starts_with("Seat=") && line.len() > 5)
        && terminal.is_some_and(|terminal| {
            terminal
                .strip_prefix("tty")
                .is_some_and(|number| number.parse::<u32>().is_ok_and(|number| number > 0))
                && input == Path::new("/dev").join(terminal)
        })
}

fn seat(directory: &Path) -> Result<()> {
    let invalid_terminal =
        "Run from a logged-in, active physical TTY; the agent pseudo-terminal is insufficient";
    if !std::io::stdin().is_terminal() {
        return Err(invalid_terminal.into());
    }
    let input = fs::read_link("/proc/self/fd/0").map_err(error)?;
    let run: Run = load(&directory.join("run.json"))?;
    if !run.configuration.valid_duration()
        || run
            .configuration
            .helper
            .as_ref()
            .is_some_and(|helper| !helper.valid())
        || !directory.is_absolute()
        || run.directory != directory
        || digest(&directory.join("bin/lince"))? != run.executable_sha256
        || digest(&directory.join("bin/session-probe"))? != run.probe_sha256
        || digest(&directory.join("bin/capture-probe"))? != run.capture_sha256
        || digest(&run.process_executable)? != run.process_sha256
        || inspect_build(&directory.join("bin/lince"))? != run.build_info
    {
        return Err("Prepared executable or directory identity changed".into());
    }
    write_json(
        &directory.join("hardware-before.json"),
        &diagnostics::hardware(),
    )?;
    let session = Command::new("loginctl")
        .args([
            "show-session",
            "self",
            "--property=Active",
            "--property=TTY",
            "--property=Seat",
            "--property=VTNr",
        ])
        .output()
        .map_err(error)?;
    let metadata = String::from_utf8(session.stdout).map_err(error)?;
    fs::write(directory.join("physical-session.log"), &metadata).map_err(error)?;
    if !session.status.success() || !physical_terminal(&metadata, &input) {
        return Err(invalid_terminal.into());
    }
    let http = TcpListener::bind(("127.0.0.1", run.port)).map_err(error)?;
    let peer = TcpListener::bind(("127.0.0.1", run.peer_port)).map_err(error)?;
    drop((http, peer));
    let claim = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("seat-started"))
        .map_err(error)?;
    drop(claim);
    let _signals = interrupt_handler()?;
    let mut results = Results {
        phases: Vec::new(),
        resources: Vec::new(),
        observations: [
            "physical-input",
            "visible-scanout",
            "vt-return",
            "hotplug",
            "suspend-resume",
            "lock-protection",
            "clean-login",
            "long-session",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), "awaiting human observation".into()))
        .collect(),
    };
    let mut children = Children(Vec::new());
    let outcome = seat_inner(&run, &mut children, &mut results);
    if let Err(detail) = &outcome {
        results.phases.push(Phase {
            name: "physical-smoke".into(),
            outcome: "failed".into(),
            elapsed_ms: 0,
            detail: detail.clone(),
        });
    }
    drop(children);
    write_json(
        &directory.join("hardware-after.json"),
        &diagnostics::hardware(),
    )?;
    write_json(&directory.join("results.json"), &results)?;
    let mut summary = format!(
        "Park validation {}\nExecutable SHA-256: {}\n",
        run.revision, run.executable_sha256
    );
    for phase in &results.phases {
        summary.push_str(&format!(
            "{}: {} ({} ms) {}\n",
            phase.name, phase.outcome, phase.elapsed_ms, phase.detail
        ));
    }
    for (name, outcome) in &results.observations {
        summary.push_str(&format!("{name}: {outcome}\n"));
    }
    summary.push_str(&resource_summary(&results.resources));
    summary.push_str(&format!("{}\n", run.recovery));
    fs::write(directory.join("summary.txt"), &summary).map_err(error)?;
    print!("{summary}");
    outcome
}

fn seat_inner(run: &Run, children: &mut Children, results: &mut Results) -> Result<()> {
    let directory = &run.directory;
    let mut command = Command::new(executable(Path::new("timeout"))?);
    command
        .args(["--signal=TERM", "--kill-after=10s"])
        .arg(format!("{}s", run.configuration.session_seconds()))
        .args(["dbus-run-session", "--"])
        .arg(directory.join("bin/lince"))
        .args(["--park", "--no-tray", "--directory"])
        .arg(directory.join("data"))
        .arg("--port")
        .arg(run.port.to_string())
        .arg("--peer-port")
        .arg(run.peer_port.to_string())
        .env_remove("LINCE_DATA_DIR")
        .env_remove("LINCE_PORT")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("LINCE_PARK_BOOTSTRAP_PID")
        .env("LINCE_PARK_TRACE_FRAMES", "1")
        .env("RUST_MIN_STACK", "33554432");
    if run.configuration.showoff {
        command.arg("--showoff");
    }
    logged(&mut command, &directory.join("session.log"))?;
    children.0.push(ManagedChild::spawn(&mut command)?);
    let start = Instant::now();
    let (socket, pid, bus) = loop {
        check_interruption()?;
        if let Some(status) = children.0[0].child.try_wait().map_err(error)? {
            return Err(format!("Park exited before readiness: {status}"));
        }
        if directory.join("ready.json").exists() {
            let ready: serde_json::Value = load(&directory.join("ready.json"))?;
            let pid = ready["pid"]
                .as_i64()
                .and_then(|pid| i32::try_from(pid).ok())
                .and_then(rustix::process::Pid::from_raw)
                .ok_or("Invalid readiness process")?;
            let bus = ready["bus"]
                .as_str()
                .ok_or("Missing isolated session bus")?
                .to_owned();
            break (
                PathBuf::from(ready["socket"].as_str().ok_or("Invalid readiness result")?),
                pid,
                bus,
            );
        }
        if start.elapsed() > Duration::from_secs(40) {
            return Err("No first-frame readiness within 40 seconds".into());
        }
        thread::sleep(Duration::from_millis(50));
    };
    results.phases.push(Phase { name: "first-frame-readiness".into(), outcome: "passed".into(), elapsed_ms: start.elapsed().as_millis(), detail: "Readiness hook ran after the initial scene submission; physical scanout still needs observation".into() });
    for round in 0..run.configuration.lifecycle_rounds {
        results
            .resources
            .push(resource_sample(pid, start.elapsed().as_millis()));
        let mut probe = Command::new(directory.join("bin/session-probe"));
        probe
            .env("WAYLAND_DISPLAY", &socket)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env_remove("WAYLAND_SOCKET");
        let log = format!("native-session-probe-{round}.log");
        logged(&mut probe, &directory.join(&log))?;
        let (status, elapsed_ms) = bounded(&mut probe, Duration::from_secs(55))?;
        results.phases.push(Phase {
            name: format!("native-window-lifecycle-{round}"),
            outcome: if status.success() { "passed" } else { "failed" }.into(),
            elapsed_ms,
            detail: format!("{status}; {log}"),
        });
        if !status.success() {
            return Err("Native window lifecycle acceptance failed".into());
        }
        results
            .resources
            .push(resource_sample(pid, start.elapsed().as_millis()));
        write_json(&directory.join("progress.json"), results)?;
    }
    if run.configuration.helper.is_none() {
        let capture_started = Instant::now();
        let report = directory.join("physical-capture.json");
        while !report.exists() {
            check_interruption()?;
            if capture_started.elapsed() >= Duration::from_secs(12) {
                return Err("The authorized physical output capture probe did not complete".into());
            }
            if children.0[0].child.try_wait().map_err(error)?.is_some() {
                return Err("Park exited before capture acceptance".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        let captured: serde_json::Value = load(&report)?;
        if captured["status"] != "passed" {
            return Err("Physical output capture did not pass".into());
        }
        results.phases.push(Phase { name: "authorized-output-capture".into(), outcome: "passed".into(), elapsed_ms: capture_started.elapsed().as_millis(),
            detail: "Source/protocol/export/pixel checks in physical-capture.json; visual fidelity, cursor and portal acceptance remain separate".into() });
    }
    if children.0[0].child.try_wait().map_err(error)?.is_some() {
        return Err("Park exited after the probe closed its last windows".into());
    }
    let programs = std::iter::once(&run.configuration.terminal).chain(&run.configuration.clients);
    for (index, program) in programs.enumerate() {
        let mut command = Command::new(executable(Path::new("timeout"))?);
        command
            .args(["--signal=TERM", "--kill-after=5s"])
            .arg(format!(
                "{}s",
                run.configuration.observation_seconds + run.configuration.soak_seconds + 20
            ))
            .arg(&program.program)
            .args(&program.arguments)
            .env("WAYLAND_DISPLAY", &socket)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env("XDG_CURRENT_DESKTOP", "Lince")
            .env("XDG_SESSION_TYPE", "wayland")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("SDL_VIDEODRIVER", "wayland")
            .env("SDL_VIDEO_DRIVER", "wayland")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .env_remove("XDG_ACTIVATION_TOKEN");
        native_environment(&mut command)?;
        logged(&mut command, &directory.join(format!("client-{index}.log")))?;
        children.0.push(ManagedChild::spawn(&mut command)?);
    }
    results.phases.push(Phase {
        name: "external-clients".into(),
        outcome: "awaiting human observation".into(),
        elapsed_ms: 0,
        detail: "Processes launched; this alone does not establish mapped windows or rendering"
            .into(),
    });
    write_json(&directory.join("progress.json"), results)?;
    let observation = Instant::now();
    let hold =
        Duration::from_secs(run.configuration.observation_seconds + run.configuration.soak_seconds);
    println!(
        "Observe client windows and exercise the shortcut checklist. You may log out with Super+Shift+Escape; otherwise the runner requests logout after {} seconds. {}",
        hold.as_secs(),
        run.recovery
    );
    let mut requested_logout = false;
    let mut sample_at = Instant::now();
    loop {
        check_interruption()?;
        if let Some(status) = children.0[0].child.try_wait().map_err(error)? {
            let orderly = status.success()
                && fs::read_to_string(directory.join("session.log"))
                    .map_err(error)?
                    .lines()
                    .any(|line| line == "Park orderly logout completed");
            results.phases.push(Phase {
                name: "session-exit".into(),
                outcome: if orderly { "passed" } else { "failed" }.into(),
                elapsed_ms: start.elapsed().as_millis(),
                detail: format!("{status}; orderly logout marker: {orderly}"),
            });
            return if orderly {
                Ok(())
            } else {
                Err(format!(
                    "Session ended without confirmed orderly logout: {status}"
                ))
            };
        }
        if Instant::now() >= sample_at {
            results
                .resources
                .push(resource_sample(pid, start.elapsed().as_millis()));
            write_json(&directory.join("progress.json"), results)?;
            sample_at = Instant::now() + Duration::from_secs(1);
        }
        if !requested_logout && observation.elapsed() > hold {
            let live =
                fs::read_link(format!("/proc/{}/exe", pid.as_raw_nonzero())).map_err(error)?;
            if live != run.process_executable || digest(&live)? != run.process_sha256 {
                return Err("Compositor process identity changed before logout".into());
            }
            rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(error)?;
            requested_logout = true;
        }
        if observation.elapsed() > hold + Duration::from_secs(30) {
            return Err("Interactive smoke ended without confirmed logout; remaining automation and observations are not passed".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_probe_is_reaped_without_interrupting_other_tests() {
        const MARKER: &str = "LINCE_PARK_INTERRUPT_TEST";
        if let Some(path) = env::var_os(MARKER) {
            let _signals = interrupt_handler().unwrap();
            fs::write(path, "ready").unwrap();
            let started = Instant::now();
            let result = bounded(Command::new("sleep").arg("20"), Duration::from_secs(25));
            assert!(result.unwrap_err().contains("interrupted"));
            assert!(started.elapsed() < Duration::from_secs(5));
            return;
        }
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation");
        fs::create_dir_all(&directory).unwrap();
        let marker = directory.join(format!("interrupt-test-{}", std::process::id()));
        let mut child = Command::new(env::current_exe().unwrap())
            .args(["--exact", "park::validation::tests::interrupted_probe_is_reaped_without_interrupting_other_tests"])
            .env(MARKER, &marker).spawn().unwrap();
        let started = Instant::now();
        while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
            assert!(child.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
        let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if started.elapsed() > Duration::from_secs(6) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Interrupted validation did not exit promptly");
            }
            thread::sleep(Duration::from_millis(10));
        }
        fs::remove_file(marker).unwrap();
    }

    #[test]
    fn installed_identity_tracks_the_payload_of_shell_and_elf_wrappers() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/park-validation/wrapper-{stamp}"));
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("lince");
        let payload = directory.join(".lince-wrapped");
        fs::write(&payload, b"\x7fELFpayload").unwrap();
        for header in [b"#!/bin/sh".as_slice(), b"\x7fELFwrapper".as_slice()] {
            fs::write(&executable, header).unwrap();
            assert_eq!(
                process_executable(&executable).unwrap(),
                payload.canonicalize().unwrap()
            );
        }
        fs::remove_file(&payload).unwrap();
        assert_eq!(
            process_executable(&executable).unwrap(),
            executable.canonicalize().unwrap()
        );
        fs::write(&executable, b"#!/bin/sh").unwrap();
        assert!(process_executable(&executable).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn build_capabilities_reject_help_output_and_non_park_executables() {
        let information = serde_json::json!({"version":"1.0", "revision":"test", "optimized":true,
            "target":"x86_64-unknown-linux-gnu", "features":{"ui":true,"park":true}});
        assert!(validate_build(&information).is_ok());
        for field in ["ui", "park"] {
            let mut missing = information.clone();
            missing["features"][field] = false.into();
            assert!(validate_build(&missing).is_err());
        }
        let mut missing = information.clone();
        missing["optimized"] = false.into();
        assert!(validate_build(&missing).is_err());
        let mut missing = information.clone();
        missing["target"] = "aarch64-apple-darwin".into();
        assert!(validate_build(&missing).is_err());
        assert!(validate_build(&serde_json::json!({"help":"--park"})).is_err());
    }

    #[test]
    fn executable_resolution_preserves_the_invoked_symlink_name() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("park-executable-{stamp}-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let target = directory.join("dispatch");
        fs::write(&target, b"#!/bin/sh\n[ \"${0##*/}\" = selected-command ]\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        let alias = directory.join("selected-command");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let resolved = executable(&alias).unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved.file_name(), alias.file_name());
        assert!(Command::new(resolved).status().unwrap().success());
        assert!(executable(&directory).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(executable(&alias).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shell_paths_preserve_literal_metacharacters() {
        assert_eq!(
            quote(Path::new("/tmp/a'b $(touch x)")),
            "'/tmp/a'\\''b $(touch x)'"
        );
    }

    #[test]
    fn physical_validation_requires_the_actual_active_session_terminal() {
        let active = "Active=yes\nTTY=tty3\nSeat=seat0\nVTNr=3\n";
        assert!(physical_terminal(active, Path::new("/dev/tty3")));
        assert!(!physical_terminal(active, Path::new("/dev/pts/3")));
        assert!(!physical_terminal(active, Path::new("/dev/tty2")));
        assert!(!physical_terminal(
            &active.replace("Active=yes", "Active=no"),
            Path::new("/dev/tty3")
        ));
        assert!(!physical_terminal(
            &active.replace("Seat=seat0", "Seat="),
            Path::new("/dev/tty3")
        ));
        assert!(!physical_terminal(
            "Active=yes\nTTY=pts/3\nSeat=seat0\n",
            Path::new("/dev/pts/3")
        ));
        assert!(!physical_terminal(
            "Active=yes\nTTY=tty0\nSeat=seat0\n",
            Path::new("/dev/tty0")
        ));
    }

    #[test]
    fn fixture_schema_rejects_shell_and_unknown_fields() {
        assert!(
            serde_json::from_str::<Configuration>(
                r#"{"terminal":{"program":"terminal","shell":true}}"#
            )
            .is_err()
        );
        let config: Configuration = serde_json::from_str(
            r#"{"terminal":{"program":"terminal","arguments":["$(touch x)"]}}"#,
        )
        .unwrap();
        assert_eq!(config.terminal.arguments, ["$(touch x)"]);
        assert!(!config.showoff);
        assert!(config.launcher.is_none());
        assert!(config.lock.is_none());
        let config: Configuration = serde_json::from_str(
            r#"{"terminal":{"program":"terminal"},"launcher":{"program":"chosen-launcher","arguments":["two words"]},"lock":{"program":"chosen-locker","arguments":["$(touch x)"]}}"#,
        ).unwrap();
        assert_eq!(config.launcher.unwrap().arguments, ["two words"]);
        assert_eq!(config.lock.unwrap().arguments, ["$(touch x)"]);
        let config: Configuration =
            serde_json::from_str(r#"{"terminal":{"program":"terminal"},"showoff":true}"#).unwrap();
        assert!(config.showoff);
    }

    #[test]
    fn validation_durations_are_bounded_and_watchdogs_include_every_round() {
        let mut config: Configuration =
            serde_json::from_str(r#"{"terminal":{"program":"terminal"}}"#).unwrap();
        assert!(config.valid_duration());
        assert_eq!(config.lifecycle_rounds, 1);
        assert_eq!(config.observation_seconds, 90);
        assert_eq!(config.session_seconds(), 235);
        config.lifecycle_rounds = 20;
        config.observation_seconds = 300;
        config.soak_seconds = 3600;
        assert!(config.valid_duration());
        assert_eq!(config.session_seconds(), 5090);
        config.lifecycle_rounds = 21;
        assert!(!config.valid_duration());
        config.lifecycle_rounds = 1;
        config.soak_seconds = 3601;
        assert!(!config.valid_duration());
        config.soak_seconds = 0;
        config.observation_seconds = 0;
        assert!(!config.valid_duration());
    }

    #[test]
    fn resource_reports_preserve_negative_deltas_and_missing_measurements() {
        let sample = |elapsed_ms, resident_kib, descriptors, cpu_ticks| ResourceSample {
            elapsed_ms,
            resident_kib,
            descriptors,
            cpu_ticks,
            peak_resident_kib: None,
            threads: None,
            drm_clients: None,
        };
        let summary = resource_summary(&[
            sample(100, Some(2048), Some(20), Some(10)),
            sample(1100, Some(1024), Some(18), Some(14)),
        ]);
        assert!(summary.contains("across 1000 ms"));
        assert!(summary.contains("RSS delta -1024 KiB"));
        assert!(summary.contains("descriptor delta -2"));
        assert!(summary.contains("CPU tick delta 4"));
        let summary = resource_summary(&[sample(0, None, None, None)]);
        assert!(summary.contains("RSS delta unavailable"));
        assert!(!summary.contains("passed"));
    }

    #[test]
    fn helper_authorization_is_explicit_bounded_and_independent_of_client_arguments() {
        let parse = |capabilities: &str| {
            serde_json::from_str::<Helper>(&format!(r#"{{"target":{{"program":"chosen-helper","arguments":["two words"]}},"capabilities":{capabilities}}}"#)).unwrap()
        };
        let helper = parse(r#"["capture","window-list"]"#);
        assert!(helper.valid());
        assert_eq!(helper.target.arguments, ["two words"]);
        for capabilities in [
            "[]",
            r#"["all"]"#,
            r#"["capture","capture"]"#,
            r#"["clipboard","window-list","window-control","output-control","capture","input-method","capture"]"#,
        ] {
            assert!(!parse(capabilities).valid());
        }
        let config: Configuration = serde_json::from_str(r#"{"terminal":{"program":"terminal"},"helper":{"target":{"program":"provider"},"capabilities":["input-method"]}}"#).unwrap();
        assert!(config.helper.unwrap().valid());
    }
}
