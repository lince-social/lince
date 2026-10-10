use super::*;
use std::io::Read;
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    fn config(command: PathBuf, directory: &Path) -> Config {
        serde_json::from_value(serde_json::json!({
            "command":command,"args":[],"directory":directory
        }))
        .unwrap()
    }

    #[test]
    fn node_launch_commands_are_rejected_before_execution() {
        let root = tempfile::tempdir().unwrap();
        for command in [
            "node",
            "nodejs",
            "npm",
            "npx",
            "agent.js",
            "agent.mjs",
            "agent.cjs",
        ] {
            let mut configured = config(command.into(), root.path());
            let error = configured.validate().unwrap_err();
            assert!(error.contains("Node.js"), "{command}: {error}");
        }
        let mut configured = config("sh".into(), root.path());
        configured.args = vec!["-c".into(), "exec node /missing/agent.js".into()];
        assert!(configured.validate().unwrap_err().contains("Node.js"));
    }

    #[cfg(unix)]
    #[test]
    fn node_wrappers_and_path_aliases_are_inspected_without_running_them() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let wrapper = root.path().join("codex-acp");
        let marker = root.path().join("must-not-run");
        for body in [
            "#!/usr/bin/env node\nrequire('missing');\n".into(),
            format!(
                "#!/bin/sh\ntouch {}\nexec node /missing/agent.js\n",
                marker.display()
            ),
        ] {
            std::fs::write(&wrapper, body).unwrap();
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut configured = config("codex-acp".into(), root.path());
            configured
                .environment
                .insert("PATH".into(), root.path().to_string_lossy().into());
            assert!(configured.validate().unwrap_err().contains("Node.js"));
            assert!(!marker.exists());
        }
        std::fs::write(&wrapper, "#!/bin/sh\nexit 0\n").unwrap();
        config(wrapper, root.path()).validate().unwrap();
    }
}

pub(super) fn requires_node(config: &Config) -> bool {
    fn node_program(value: &str) -> bool {
        let value = value
            .trim_matches(['\'', '"', ';'])
            .trim_start_matches("#!");
        let path = Path::new(value);
        matches!(
            path.file_stem().and_then(|name| name.to_str()),
            Some("node" | "nodejs" | "npm" | "npx")
        ) || matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("js" | "cjs" | "mjs")
        )
    }
    if node_program(&config.command.to_string_lossy()) {
        return true;
    }
    let command = resolve(config).unwrap_or_else(|_| config.command.clone());
    if node_program(&command.to_string_lossy()) {
        return true;
    }
    if matches!(
        config.command.file_name().and_then(|name| name.to_str()),
        Some("env" | "sh" | "bash")
    ) && config
        .args
        .iter()
        .flat_map(|arg| arg.split_whitespace())
        .any(node_program)
    {
        return true;
    }
    let Ok(file) = std::fs::File::open(command) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(8192).read_to_end(&mut bytes).is_err() || !bytes.starts_with(b"#!") {
        return false;
    }
    String::from_utf8_lossy(&bytes)
        .lines()
        .enumerate()
        .filter(|(index, line)| *index == 0 || !line.trim_start().starts_with('#'))
        .flat_map(|(_, line)| line.split_whitespace())
        .any(node_program)
}

pub(super) fn environment(config: &Config) -> BTreeMap<String, String> {
    let mut environment = config.environment.clone();
    if cfg!(target_os = "linux") {
        environment.entry("LD_LIBRARY_PATH".into()).or_default();
    }
    environment
}

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

pub fn resolve(config: &Config) -> Result<PathBuf, String> {
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
