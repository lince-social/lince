use super::*;
use std::{
    process::{Child, Command},
    time::{Duration, Instant},
};

pub(super) const COMPONENTS: &[&str] = &[
    "square",
    "text",
    "editable-text",
    "organ",
    "time-castle",
    "todo",
    "configuration",
    "access-control",
    "sync",
    "operation",
    "ontology",
];

struct Preview(Option<Child>);

impl Preview {
    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            #[cfg(unix)]
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let deadline = Instant::now() + Duration::from_secs(2);
            while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            #[cfg(unix)]
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = child.wait();
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.stop();
    }
}

fn command(component: &str, theme: Option<&Path>, mobile: bool) -> Command {
    let mut command = crate::cargo_command();
    command.args([
        "run",
        "-p",
        "lince-desktop",
        "--example",
        "design_preview",
        "--",
        component,
    ]);
    if let Some(theme) = theme {
        command.arg("--theme").arg(theme);
    }
    if mobile {
        command.arg("--mobile");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
}

fn source_revision(root: &Path) -> Result<u64> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    super::server::fingerprint(&root.join("crates"), true)?.hash(&mut hasher);
    for file in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml"] {
        fs::read(root.join(file))
            .map_err(|error| error.to_string())?
            .hash(&mut hasher);
    }
    Ok(hasher.finish())
}

pub(super) fn run(
    root: &Path,
    component: &str,
    theme_path: Option<&Path>,
    watch: bool,
    mobile: bool,
) -> Result<()> {
    if !COMPONENTS.contains(&component) {
        return Err(format!(
            "Unknown component; choose {}",
            COMPONENTS.join(", ")
        ));
    }
    let theme_path = theme_path
        .map(|path| {
            let canonical = path.canonicalize().map_err(|error| error.to_string())?;
            theme(Some(&canonical), "Dark")?;
            Ok::<_, String>(canonical)
        })
        .transpose()?;
    let mut preview = Preview(None);
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let mut revision = if watch { source_revision(root)? } else { 0 };
        preview.0 = Some(command(component, theme_path.as_deref(), mobile).spawn().map_err(|error| error.to_string())?);
        println!("Native isolated preview: {component}. Theme JSON reloads live; Rust source edits {}.", if watch { "compile and restart" } else { "need a restart" });
        let mut interval = tokio::time::interval(Duration::from_millis(600));
        let mut changed = None;
        loop {
            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| error.to_string())?;
                    return Ok(());
                }
                _ = interval.tick() => {
                    if let Some(child) = preview.0.as_mut()
                        && let Some(status) = child.try_wait().map_err(|error| error.to_string())?
                    {
                        preview.stop();
                        if !watch {
                            return if status.success() { Ok(()) } else { Err(format!("Native preview exited with {status}")) };
                        }
                        println!("Preview exited with {status}. Waiting for a Rust source change; Ctrl+C stops watching.");
                    }
                    if watch {
                        match source_revision(root) {
                            Ok(next) if next != revision => {
                                revision = next;
                                changed = Some(Instant::now());
                            }
                            Ok(_) => {
                                if changed.is_some_and(|at| at.elapsed() >= Duration::from_millis(500)) {
                                    changed = None;
                                    preview.stop();
                                    println!("Rust sources changed. Compiling and restarting the isolated preview.");
                                    preview.0 = Some(command(component, theme_path.as_deref(), mobile).spawn().map_err(|error| error.to_string())?);
                                }
                            }
                            Err(error) => eprintln!("Could not inspect source changes: {error}"),
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_preview_rejects_unknown_components_before_starting_cargo() {
        assert!(
            run(Path::new("/missing"), "unknown", None, true, false)
                .unwrap_err()
                .contains("Unknown component")
        );
    }

    #[cfg(unix)]
    #[test]
    fn stopping_preview_terminates_its_process_group() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
        let mut preview = Preview(Some(child));
        preview.stop();
        assert!(preview.0.is_none());
        assert!(rustix::process::test_kill_process(pid).is_err());
    }
}
