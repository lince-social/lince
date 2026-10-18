use crate::launch::Description;
use lince_interface::host::DesktopAction;
use serde::Deserialize;
use std::{io, path::Path};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Commands {
    pub outputs: std::collections::BTreeMap<String, OutputPreference>,
    pub keyboard: Keyboard,
    pub terminal: Option<Description>,
    pub launcher: Option<Description>,
    pub lock: Option<Description>,
    pub ready: Option<Description>,
}

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct OutputPreference {
    pub enabled: bool,
    pub scale: f64,
    pub position: Option<[i32; 2]>,
    pub mode: Option<[u32; 2]>,
    pub refresh_mhz: Option<i32>,
}

impl Default for OutputPreference {
    fn default() -> Self {
        Self { enabled: true, scale: 1.0, position: None, mode: None, refresh_mhz: None }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Keyboard {
    pub repeat_rate: u32,
    pub repeat_delay_ms: u32,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            repeat_rate: 25,
            repeat_delay_ms: 500,
        }
    }
}

impl Commands {
    pub fn load(path: &Path) -> io::Result<Self> {
        use std::io::Read;
        let mut bytes = Vec::new();
        match std::fs::File::open(path) {
            Ok(file) => {
                file.take(65537).read_to_end(&mut bytes)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        }
        if bytes.len() > 65536 {
            return Err(io::Error::other(
                "Park command configuration exceeds 64 KiB",
            ));
        }
        let commands: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if commands.outputs.len() > 16 || commands.outputs.iter().any(|(name, output)| {
            name.is_empty() || name.len() > 512 || !output.scale.is_finite()
                || !(0.5..=4.0).contains(&output.scale)
                || (output.scale * 120.0 - (output.scale * 120.0).round()).abs() > 0.000001
                || output.position.is_some_and(|position| position.iter().any(|value| !(0..=32768).contains(value)))
                || output.mode.is_some_and(|mode| mode.iter().any(|value| !(1..=16384).contains(value)))
                || output.refresh_mhz.is_some_and(|refresh| !(1000..=1_000_000).contains(&refresh))
        }) {
            return Err(io::Error::other("Invalid local output preferences"));
        }
        if commands.keyboard.repeat_rate > 100 || commands.keyboard.repeat_delay_ms > 5000 {
            return Err(io::Error::other(
                "Keyboard repeat needs a rate of 0–100 and a delay of 0–5000 ms",
            ));
        }
        for command in [
            &commands.terminal,
            &commands.launcher,
            &commands.lock,
            &commands.ready,
        ]
        .into_iter()
        .flatten()
        {
            if !command.valid() {
                return Err(io::Error::other("Invalid Park command configuration"));
            }
        }
        Ok(commands)
    }

    pub fn command(&self, action: DesktopAction) -> io::Result<Description> {
        let command = match action {
            DesktopAction::OpenTerminal => self.terminal.clone().or_else(|| {
                Some(Description {
                    program: "xdg-terminal-exec".into(),
                    arguments: Vec::new(),
                    directory: None,
                    capabilities: Vec::new(),
                })
            }),
            DesktopAction::OpenLauncher => self.launcher.clone(),
            DesktopAction::Lock => self.lock.clone(),
            _ => return Err(io::Error::other("This action is not an external command")),
        };
        command.ok_or_else(|| {
            io::Error::other("Choose this command in the local park.json configuration")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_preferences_are_bounded_and_do_not_accept_unknown_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        std::fs::write(&path, r#"{"outputs":{"DP-1":{"scale":1.25,"position":[0,0],"mode":[2560,1440],"refresh_mhz":60000}}}"#).unwrap();
        assert_eq!(Commands::load(&path).unwrap().outputs["DP-1"].scale, 1.25);
        for configuration in [
            r#"{"outputs":{"DP-1":{"scale":0}}}"#,
            r#"{"outputs":{"DP-1":{"scale":1.234}}}"#,
            r#"{"outputs":{"DP-1":{"position":[-10,0]}}}"#,
            r#"{"outputs":{"DP-1":{"mode":[0,1440]}}}"#,
            r#"{"outputs":{"DP-1":{"refesh_mhz":60000}}}"#,
        ] {
            std::fs::write(&path, configuration).unwrap();
            assert!(Commands::load(&path).is_err());
        }
    }

    #[test]
    fn commands_are_explicit_local_argv_and_reject_invalid_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        assert!(
            Commands::load(&path)
                .unwrap()
                .command(DesktopAction::OpenLauncher)
                .is_err()
        );
        std::fs::write(&path, r#"{"launcher":{"program":"chosen-launcher","arguments":["$(touch /tmp/unexpected)","two words"],"directory":null}}"#).unwrap();
        let command = Commands::load(&path)
            .unwrap()
            .command(DesktopAction::OpenLauncher)
            .unwrap();
        assert_eq!(command.arguments, ["$(touch /tmp/unexpected)", "two words"]);
        std::fs::write(
            &path,
            r#"{"launcher":{"program":"","arguments":[],"directory":null}}"#,
        )
        .unwrap();
        assert!(Commands::load(&path).is_err());
        std::fs::write(&path, r#"{"autostart":[]}"#).unwrap();
        assert!(Commands::load(&path).is_err());
        std::fs::write(
            &path,
            r#"{"terminal":{"program":"chosen-terminal","argument":["ignored"]}}"#,
        )
        .unwrap();
        assert!(Commands::load(&path).is_err());
    }
}
