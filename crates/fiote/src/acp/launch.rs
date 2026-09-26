use super::*;
use std::path::Path;

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(super) fn resolve(config: &Config) -> Result<PathBuf, String> {
    if config.command.is_absolute() || config.command.components().count() > 1 {
        let path = if config.command.is_absolute() {
            config.command.clone()
        } else {
            config.directory.join(&config.command)
        };
        return executable(&path).then_some(path).ok_or_else(|| {
            format!(
                "The agent executable '{}' is missing or cannot be run. Choose an installed executable in Agent options.",
                config.command.display()
            )
        });
    }
    let path = config
        .environment
        .get("PATH")
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os("PATH"));
    if let Some(path) = path {
        if let Some(found) = std::env::split_paths(&path)
            .map(|directory| directory.join(&config.command))
            .find(|path| executable(path))
        {
            return found.canonicalize().map_err(|error| error.to_string());
        }
    }
    Err(format!(
        "The agent executable '{}' was not found. Install it or choose its full path in Agent options, then check the connection again. Restart Lince after installing an agent.",
        config.command.display()
    ))
}

pub(super) fn account() -> String {
    #[cfg(unix)]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            if let Some(uid) = status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|line| line.split_whitespace().nth(1))
            {
                return format!("runtime uid {uid}");
            }
        }
        if let Ok(output) = std::process::Command::new("id").arg("-u").output() {
            if output.status.success() {
                return format!(
                    "runtime uid {}",
                    String::from_utf8_lossy(&output.stdout).trim()
                );
            }
        }
    }
    "runtime account unavailable".into()
}
